//! The pipeline that executes a will (ADR-0005 §2-§3): Phase A is
//! synchronous and bounded by DISK speed — carve a workspace, quiesce,
//! imprint, resume (finally-style); Phase B is asynchronous and unbounded —
//! pack, ferry to sinks, commit, reclaim. Lock time belongs to disk speed,
//! never network speed.

use super::checkpoint;
use super::checkpoint::SINK_CHECKPOINT_DIR;
use super::policy::{CaptureMode, CapturePolicy};
use super::run::{Phase, Run, RunInfo};
use crate::garden::storage::Storage;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Where capture work happens: `~/.zen-garden/workspace/{fqn}/{run}/`
/// (MOSS_WORKSPACE_DIR overrides the root — deployment concern, R3.7).
pub fn workspace_root() -> PathBuf {
    std::env::var("MOSS_WORKSPACE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".zen-garden")
                .join("workspace")
        })
}

/// The seam hooks run through: argv inside the offering's container.
/// Docker implements it with `exec`; tests doubles script the server.
#[async_trait::async_trait]
pub trait HookRunner: Send + Sync {
    /// Run argv inside the container; the collected output returns for
    /// readers (the capability sweep's list channel). Hooks that only
    /// care about success ignore it.
    async fn exec(
        &self,
        container: &str,
        argv: &[String],
        timeout: Duration,
    ) -> Result<String, String>;

    /// Run argv inside the container, streaming output lines as they
    /// come — long operations (model pulls) report progress live. The
    /// caller enforces its own deadline while consuming.
    async fn exec_lines(
        &self,
        container: &str,
        argv: &[String],
    ) -> Result<ExecLines, String>;
}

/// The no-world hook runner: refuses loudly. A companion modality has no
/// containers to tell anything to (R2.5: degrade observable, never silent).
pub struct NullHooks;

#[async_trait::async_trait]
impl HookRunner for NullHooks {
    async fn exec(&self, _: &str, _: &[String], _: Duration) -> Result<String, String> {
        Err("no container runtime on this stone: hooks cannot run".into())
    }

    async fn exec_lines(
        &self,
        _: &str,
        _: &[String],
    ) -> Result<ExecLines, String> {
        Err("no container runtime on this stone: hooks cannot run".into())
    }
}

/// A live line stream from an in-container command (the capability
/// growth's progress source). The caller owns the deadline.
pub type ExecLines = std::pin::Pin<Box<dyn futures::Stream<Item = String> + Send>>;

/// What Phase A needs to know about the workload being imprinted.
#[derive(Debug, Clone)]
pub struct Workload {
    /// The offering's directory (signature files live here).
    pub dir: PathBuf,
    /// Container volume host paths -> name (the imprint set for
    /// lock-and-copy; stateless imprints the signature only).
    pub volumes: Vec<(PathBuf, String)>,
    /// Container name for hooks (docker exec target).
    pub container: String,
    /// Is the workload serving? Rested offerings skip quiesce entirely —
    /// direct imprint is consistent by definition (§2).
    pub running: bool,
}

/// The pipeline. One per stone; runs are keyed by offering FQN.
pub struct Runner {
    workspace_root: PathBuf,
    checkpoints_root: PathBuf,
    storage: Arc<Storage>,
    hooks: Arc<dyn HookRunner>,
    /// The room's ears: lets the ferry reach sink banks this stone does
    /// not hold (ADR-0005 §4's tier-1 sink, wherever the operator plugged
    /// it). None where only the local lane is exercised.
    topology: Option<Arc<crate::room::topology::Topology>>,
    /// The stone's fact stream: each run's fate lands here too, so the
    /// journal tells the whole story without reading offering chains.
    journal: Option<Arc<crate::journal::Journal>>,
    runs: parking_lot::Mutex<HashMap<String, Run>>,
}

impl Runner {
    pub fn new(storage: Arc<Storage>, hooks: Arc<dyn HookRunner>) -> Self {
        Self {
            workspace_root: workspace_root(),
            checkpoints_root: checkpoint::checkpoints_root(),
            storage,
            hooks,
            topology: None,
            journal: None,
            runs: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    /// Give the ferry the room: remote sink banks, heard through the
    /// chirp, are reached through their holder's file face.
    pub fn with_topology(mut self, topology: Arc<crate::room::topology::Topology>) -> Self {
        self.topology = Some(topology);
        self
    }

    /// Give the run fates to the stone's fact stream.
    pub fn with_journal(mut self, journal: Arc<crate::journal::Journal>) -> Self {
        self.journal = Some(journal);
        self
    }

    /// Explicit roots — deployment overrides (tests, until the scheduler
    /// slice moves workspaces into config).
    #[cfg(test)]
    pub fn with_roots(
        mut self,
        workspace_root: PathBuf,
        checkpoints_root: PathBuf,
    ) -> Self {
        self.workspace_root = workspace_root;
        self.checkpoints_root = checkpoints_root;
        self
    }

    /// The last known run of an offering, if any (surfaced by GET faces).
    /// The capture workspace root — scratch lives beside it.
    pub fn workspace_root(&self) -> &std::path::Path {
        &self.workspace_root
    }

    pub fn last_run(&self, fqn: &str) -> Option<RunInfo> {
        self.runs.lock().get(fqn).map(|r| r.snapshot())
    }

    fn record(&self, run: Run) {
        self.runs.lock().insert(run.info().fqn.clone(), run);
    }

    /// Select a checkpoint: the named run, or the newest across the local
    /// ledger AND every mounted sink bank (ADR-0005 §5 - whichever stone
    /// the bank sits on, the will can reach it).
    pub fn select_checkpoint(
        &self,
        fqn: &str,
        run: Option<&str>,
    ) -> Result<PathBuf, String> {
        let slug = crate::garden::directory::slug(fqn);
        let mut roots = vec![self.checkpoints_root.join(&slug)];
        for bank in self.storage.banks() {
            if bank.roles.iter().any(|r| r == garden_glossary::bank::role::SINK)
                && bank.state == garden_glossary::bank::MOUNTED
            {
                roots.push(
                    Path::new(&bank.mount_point)
                        .join(SINK_CHECKPOINT_DIR)
                        .join(&slug),
                );
            }
        }
        if let Some(run) = run {
            for root in &roots {
                let p = root.join(run);
                if p.is_dir() {
                    return Ok(p);
                }
            }
            return Err(format!("no checkpoint run '{run}' for '{fqn}' in the ledger or any mounted sink"));
        }
        // Newest across every root (names sort chronologically - GUIDv7).
        let mut all: Vec<PathBuf> = Vec::new();
        for root in &roots {
            if let Ok(entries) = std::fs::read_dir(root) {
                all.extend(
                    entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.is_dir() && !p.ends_with(".partial")),
                );
            }
        }
        all.sort();
        all.pop().ok_or_else(|| {
            format!("no checkpoint exists for '{fqn}' - nothing to replant from")
        })
    }

    /// Restore a verified checkpoint into an offering directory (§6):
    /// signature files land at the root, volumes under `volumes/`. The
    /// target must be fresh - an existing record means the offering is
    /// already incarnate here, and replant refuses to overwrite identity.
    /// Returns (file count, the manifest's archive hash - the replant
    /// event's final_hash).
    pub fn restore_into(&self, checkpoint: &Path, dir: &Path) -> Result<(usize, String), String> {
        let record = dir.join("record.json");
        if record.exists() {
            return Err(format!(
                "{} already holds a record - replant refuses to overwrite an incarnation",
                dir.display()
            ));
        }
        let report = checkpoint::verify(checkpoint)?;
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(checkpoint.join("manifest.json"))
                .map_err(|e| format!("manifest unreadable: {e}"))?,
        )
        .map_err(|e| format!("manifest unparsable: {e}"))?;
        let archive_file = manifest["archive"]["file"]
            .as_str()
            .unwrap_or("checkpoint.tar.zst")
            .to_string();
        let final_hash = manifest["archive"]["sha256"]
            .as_str()
            .ok_or("manifest declares no archive hash")?
            .to_string();
        let archive_bytes = std::fs::read(checkpoint.join(&archive_file))
            .map_err(|e| format!("archive unreadable: {e}"))?;
        let tar_bytes = zstd::stream::decode_all(&archive_bytes[..])
            .map_err(|e| format!("archive decompress: {e}"))?;
        let mut archive = tar::Archive::new(&tar_bytes[..]);
        let mut count = 0usize;
        for entry in archive.entries().map_err(|e| format!("archive walk: {e}"))? {
            let mut entry = entry.map_err(|e| format!("archive walk: {e}"))?;
            let path = entry
                .path()
                .map_err(|e| format!("archive path: {e}"))?
                .to_string_lossy()
                .into_owned();
            if path.contains("..") || path.starts_with('/') {
                return Err(format!("checkpoint carries an unsafe path: '{path}' - refused"));
            }
            let target = dir.join(&path);
            if entry.header().entry_type().is_dir() {
                std::fs::create_dir_all(&target).map_err(|e| format!("{path}: {e}"))?;
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("{path}: {e}"))?;
            }
            entry
                .unpack(&target)
                .map_err(|e| format!("restore of '{path}' failed: {e}"))?;
            count += 1;
        }
        let _ = report;
        Ok((count, final_hash))
    }

    /// Run ledger statistics for posture (B3): how many offerings have
    /// runs tracked, and how many of those last failed.
    pub fn run_stats(&self) -> (usize, usize) {
        let runs = self.runs.lock();
        let failed = runs.values().filter(|r| r.info().phase == "failed").count();
        (runs.len(), failed)
    }

    /// Boot convergence for runs (ADR-0015 law 3): rebuild the last run
    /// of every offering from its own audit chain. A run left in flight
    /// by a restart is honestly dead — marked failed, never silently
    /// forgotten.
    pub fn replay_runs(&self, dirs_root: &crate::garden::directory::OfferingsRoot) {
        let Ok(stones_dirs) = std::fs::read_dir(&dirs_root.base) else {
            return;
        };
        for stem in stones_dirs.flatten() {
            let Some(stem_name) = stem.file_name().into_string().ok() else { continue };
            let Ok(instances) = std::fs::read_dir(stem.path()) else { continue };
            for instance in instances.flatten() {
                let Some(instance_name) = instance.file_name().into_string().ok() else { continue };
                let fqn = format!("{stem_name}::{instance_name}");
                let chain = instance.path().join("events.jsonl");
                if !chain.is_file() {
                    continue;
                }
                let Ok(log) = super::super::events::EventLog::for_root(&instance.path())
                    .validate()
                else {
                    eprintln!("replay: chain INVALID at {}", chain.display());
                    continue;
                };
                let _ = log;
                let Ok(content) = std::fs::read_to_string(&chain) else { continue };
                let mut last: Option<Run> = None;
                for line in content.lines() {
                    let Ok(ev) = serde_json::from_str::<serde_json::Value>(line) else { continue };
                    let kind = ev["kind"].as_str().unwrap_or_default().to_string();
                    let details = ev["details"].clone();
                    let Some(run_id) = details["run"].as_str().map(str::to_string) else { continue };
                    let started = ev["at"].as_str().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&chrono::Utc));
                    let mut fresh = |phase: &str| {
                        Run::from_snapshot(RunInfo {
                            fqn: fqn.clone(),
                            run_id: run_id.clone(),
                            started_at: started.unwrap_or_else(chrono::Utc::now),
                            phase: phase.into(),
                            error: None,
                            checkpoint: None,
                            ferried_to: None,
                        })
                    };
                    match kind.as_str() {
                        "RunStarted" => last = Some(fresh("imprint")),
                        "CheckpointCommitted" => {
                            if let Some(r) = last.as_mut() {
                                r.advance(super::run::Phase::Done);
                                r.info_mut().checkpoint = details["checkpoint"].as_str().map(str::to_string);
                                r.delivered_to(
                                    details["ferried"].as_array().map(|a| {
                                        a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()
                                    }).unwrap_or_default(),
                                );
                            }
                        }
                        "RunFailed" => {
                            if let Some(r) = last.as_mut() {
                                r.fail(details["error"].as_str().unwrap_or("failed"));
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(mut r) = last {
                    if r.in_flight() {
                        // The process died mid-run: honest, not forgotten.
                        r.fail("interrupted by restart");
                    }
                    self.record(r);
                }
            }
        }
    }

    /// Publish the caller-visible "accepted" record before the task
    /// starts (a Run::begin snapshot — imprint is the first phase).
    pub fn announce(&self, info: RunInfo) {
        self.record(Run::from_snapshot(info));
    }

    /// Execute with a caller-chosen run id (the HTTP face mints it so the
    /// response can name the run). Progress lands in [`Runner::last_run`].
    pub async fn execute_named(
        &self,
        fqn: &str,
        policy: &CapturePolicy,
        workload: &Workload,
        run_id: &str,
    ) -> Result<PathBuf, String> {
        let mut run = Run::begin(fqn, run_id);
        self.record(Run::from_snapshot(run.snapshot()));
        // The offering's own chain carries the run's fate — it rides the
        // checkpoint and replays at boot (a run is never amnesia).
        let audit = super::super::events::EventLog::for_root(&workload.dir);
        let _ = audit.append(
            "RunStarted",
            serde_json::json!({ "run": run_id, "mode": policy.mode.as_str() }),
        );
        if let Some(j) = &self.journal {
            j.append(crate::journal::Kind::RunStarted {
                fqn: fqn.to_string(),
                run: run_id.to_string(),
            });
        }
        let workspace = self
            .workspace_root
            .join(crate::garden::directory::slug(fqn))
            .join(run_id);
        let result = self
            .execute_inner(fqn, policy, workload, &workspace, run_id, &mut run)
            .await;
        match &result {
            Ok(checkpoint) => {
                run.finish(checkpoint);
                let ferried = run.info().ferried_to.clone().unwrap_or_default();
                let _ = audit.append(
                    "CheckpointCommitted",
                    serde_json::json!({
                        "run": run_id,
                        "checkpoint": checkpoint.display().to_string(),
                        "ferried": ferried,
                    }),
                );
                if let Some(j) = &self.journal {
                    j.append(crate::journal::Kind::CheckpointCommitted {
                        fqn: fqn.to_string(),
                        run: run_id.to_string(),
                    });
                    for sink in &ferried {
                        j.append(crate::journal::Kind::CheckpointDelivered {
                            fqn: fqn.to_string(),
                            run: run_id.to_string(),
                            sink: sink.clone(),
                        });
                    }
                }
            }
            Err(e) => {
                run.fail(e);
                let _ = audit.append(
                    "RunFailed",
                    serde_json::json!({ "run": run_id, "error": e.clone() }),
                );
                if let Some(j) = &self.journal {
                    j.append(crate::journal::Kind::RunAborted {
                        fqn: fqn.to_string(),
                        run: run_id.to_string(),
                        reason: e.clone(),
                    });
                }
            }
        }
        // Reclaim the workspace either way (§2 Phase B's last step; a
        // failed run must not leak disk). The emptied offering directory
        // goes too, best-effort.
        let _ = std::fs::remove_dir_all(&workspace);
        let _ = std::fs::remove_dir(workspace.parent().unwrap_or(&workspace));
        self.record(run);
        result
    }

    /// Execute a will end to end. Phase A inline; Phase B inline too when
    /// `async_phase_b` is false (tests); the surface spawns this task and
    /// reports via [`Runner::last_run`].
    #[cfg(test)]
    pub async fn execute(
        &self,
        fqn: &str,
        policy: &CapturePolicy,
        workload: &Workload,
    ) -> Result<PathBuf, String> {
        self.execute_named(fqn, policy, workload, &uuid::Uuid::now_v7().to_string())
            .await
    }

    async fn execute_inner(
        &self,
        fqn: &str,
        policy: &CapturePolicy,
        workload: &Workload,
        workspace: &Path,
        run_id: &str,
        run: &mut Run,
    ) -> Result<PathBuf, String> {
        // ---- Phase A: synchronous, bounded (disk speed) ----
        std::fs::create_dir_all(workspace)
            .map_err(|e| format!("workspace carve failed: {e}"))?;
        self.phase_a(policy, workload, workspace, run_id, run).await?;

        // ---- Phase B: pack, ferry, commit, reclaim ----
        run.advance(Phase::Pack);
        self.record(Run::from_snapshot(run.snapshot()));
        let checkpoint = self.pack(fqn, workload, workspace, run_id)?;
        run.advance(Phase::Ferry);
        self.record(Run::from_snapshot(run.snapshot()));
        let ferried = self.ferry(fqn, &checkpoint).await;
        run.delivered_to(ferried);
        checkpoint::rotate(&checkpoint::dir_for(&self.checkpoints_root, fqn));
        Ok(checkpoint)
    }

    /// Quiesce -> imprint -> resume, with resume on EVERY path once the
    /// lock is taken; the lock window is hard-bounded by `max_locked_s`.
    async fn phase_a(
        &self,
        policy: &CapturePolicy,
        workload: &Workload,
        workspace: &Path,
        run_id: &str,
        run: &mut Run,
    ) -> Result<(), String> {
        match policy.mode {
            CaptureMode::Stateless => Ok(()), // signature only; Phase B carries it
            CaptureMode::LockAndCopy => {
                let container = &workload.container;
                // The lock is optional by validated policy: flat-file
                // services have no application lock to take (D15) — a
                // hookless will copies freely, still inside the budget.
                let lock = match (&policy.quiesce, &policy.resume) {
                    (Some(q), Some(r)) => Some((q, r)),
                    (None, None) => None,
                    _ => {
                        return Err(
                            "lock-and-copy requires quiesce and resume hooks together".into()
                        )
                    }
                };
                if let (Some((quiesce, _)), true) = (lock.as_ref(), workload.running) {
                    self.hooks
                        .exec(container, &quiesce.exec, Duration::from_secs(quiesce.timeout_s))
                        .await
                        .map_err(|e| format!("quiesce failed (no lock held; aborting cleanly): {e}"))?;
                }
                // The lock is held (or never needed): imprint inside the budget.
                let started = std::time::Instant::now();
                let imprint = async {
                    for (host_path, name) in &workload.volumes {
                        let target = workspace.join("volumes").join(name);
                        copy_tree(host_path, &target).map_err(|e| {
                            format!("imprint of volume '{name}' failed: {e}")
                        })?;
                    }
                    Ok::<(), String>(())
                };
                let budget = Duration::from_secs(policy.max_locked_s);
                let imprint = tokio::time::timeout(budget, imprint).await;
                // Resume is finally-style: executed whenever the lock was taken.
                if let (Some((_, resume)), true) = (lock.as_ref(), workload.running) {
                    self.hooks
                        .exec(container, &resume.exec, Duration::from_secs(resume.timeout_s))
                        .await
                        .map_err(|e| format!("resume failed — the lock may be stranded: {e}"))?;
                }
                match imprint {
                    Ok(Ok(())) => {
                        tracing::info!(offering = run_id, imprint_ms = started.elapsed().as_millis() as u64, "imprint complete inside the budget");
                        Ok(())
                    }
                    Ok(Err(e)) => Err(e),
                    Err(_) => Err(format!(
                        "imprint exceeded max_locked_s ({}s): aborted loudly, nothing committed",
                        policy.max_locked_s
                    )),
                }
            }
            CaptureMode::Export => {
                // Byte-copy is wrong for this engine: the offering produces
                // its own dump, straight into the workspace. Needs the live
                // server by definition.
                let Some(export) = &policy.export else {
                    return Err("export requires an export hook".into());
                };
                let argv: Vec<String> = export
                    .exec
                    .iter()
                    .map(|a| a.replace("{workspace}", &workspace.display().to_string()))
                    .collect();
                let container = &workload.container;
                self.hooks
                    .exec(container, &argv, Duration::from_secs(export.timeout_s))
                    .await
                    .map_err(|e| format!("export failed: {e}"))?;
                Ok(())
            }
        }
        .inspect(|_| run.advance(Phase::Pack))
    }

    /// Pack the workspace into a committed checkpoint (§3): the
    /// offering's signature files plus the imprint, tar'd, zstd'd,
    /// hashed, manifest-last, ONE rename. The mechanics live on the
    /// entity; the saga only gathers what the checkpoint must carry.
    fn pack(
        &self,
        fqn: &str,
        workload: &Workload,
        workspace: &Path,
        run_id: &str,
    ) -> Result<PathBuf, String> {
        let mut files: Vec<(PathBuf, String)> = Vec::new();
        for entry in ["record.json", "candidate.json", "plan.json", "events.jsonl"] {
            let p = workload.dir.join(entry);
            if p.is_file() {
                files.push((p, entry.to_string()));
            }
        }
        let configs = workload.dir.join("configs");
        checkpoint::collect_files(&configs, &configs, &mut files)?;
        let workspace_files = {
            let mut wf = Vec::new();
            checkpoint::collect_files(workspace, workspace, &mut wf)?;
            wf
        };
        checkpoint::commit(fqn, files, &workspace_files, &self.checkpoints_root, run_id)
    }

    /// Copy the committed checkpoint to every mounted sink bank (§4):
    /// best-effort per sink — a sink that fails is loud, not fatal; the
    /// local ledger already holds the truth.
    async fn ferry(&self, fqn: &str, checkpoint: &Path) -> Vec<String> {
        let mut reached = Vec::new();
        for bank in self.storage.banks() {
            if !bank.roles.iter().any(|r| r == garden_glossary::bank::role::SINK) {
                continue;
            }
            if bank.state != garden_glossary::bank::MOUNTED {
                continue;
            }
            let target = Path::new(&bank.mount_point)
                .join(SINK_CHECKPOINT_DIR)
                .join(crate::garden::directory::slug(fqn));
            if copy_tree(checkpoint, &target.join(checkpoint.file_name().unwrap_or_default())).is_ok() {
                reached.push(bank.fqn.clone());
            } else {
                tracing::warn!(sink = %bank.fqn, "ferry failed; sink stays loud in posture");
            }
        }
        // The room's sinks: heard through the chirp (ADR-0005 §8), reached
        // through the holder's file face — a write that binds at the sink's
        // authority. The checkpoint survives this stone because it lives
        // on another one.
        if let Some(topology) = &self.topology {
            for (bank_fqn, base) in remote_sinks(&topology.snapshot(), &reached) {
                match ferry_via_http(fqn, checkpoint, &base, &bank_fqn).await {
                    Ok(()) => reached.push(bank_fqn),
                    Err(e) => {
                        tracing::warn!(sink = %bank_fqn, error = %e, "ferry failed; sink stays loud in posture")
                    }
                }
            }
        }
        reached
    }

}

/// Recursively copy one directory tree to another (imprint, ferry).
pub fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    let _ = collect_files(from, from, &mut Vec::new()); // existence probe
    fn walk(from: &Path, to: &Path, rel_root: &Path, out: &mut Vec<(PathBuf, PathBuf)>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let p = entry.path();
            let rel = p.strip_prefix(rel_root).unwrap_or(&p).to_path_buf();
            if p.is_dir() {
                walk(&p, &to.join(&rel), rel_root, out)?;
            } else {
                out.push((p, to.join(rel)));
            }
        }
        Ok(())
    }
    let mut pairs = Vec::new();
    walk(from, to, from, &mut pairs)?;
    for (src, dst) in pairs {
        std::fs::create_dir_all(dst.parent().unwrap_or(Path::new(".")))?;
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

/// A per-PUT budget, generous but real (§2's Phase B is unbounded as a
/// whole; the wire is not).
const FERRY_HTTP_TIMEOUT_SECS: u64 = 300;

/// Sinks the room hears about but `skip` (the locally reached) does not:
/// (bank fqn, holder's http base). Only mounted sink banks count — a
/// plugged-but-unclaimed drive is nobody's backup (ADR-0005 §4, §8).
fn remote_sinks(snapshot: &[crate::room::topology::StoneView], skip: &[String]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for peer in snapshot {
        let Some(banks) = &peer.body.inventory.banks else {
            continue; // the stone says nothing about banks
        };
        for bank in &banks.items {
            if !bank.roles.iter().any(|r| r == garden_glossary::bank::role::SINK) {
                continue;
            }
            if bank.state != garden_glossary::bank::MOUNTED {
                continue;
            }
            if skip.iter().any(|s| s == &bank.fqn) {
                continue;
            }
            let address = &peer.body.stone.network.address;
            out.push((bank.fqn.clone(), format!("http://{}:{}", address.ip, address.port)));
        }
    }
    out
}

/// Copy one committed checkpoint dir onto a remote sink through the
/// holder's storage-file face (PUT creates parent directories — the
/// face that makes a sink a real storage destination). The archive
/// lands first, `manifest.json` LAST: on dumb storage the manifest is
/// the commit marker — §3's atomic rename, hand-carried across the wire.
async fn ferry_via_http(fqn: &str, checkpoint: &Path, base: &str, bank_fqn: &str) -> Result<(), String> {
    let run = checkpoint
        .file_name()
        .ok_or_else(|| "checkpoint dir has no name".to_string())?
        .to_string_lossy()
        .into_owned();
    let mut files = Vec::new();
    checkpoint::collect_files(checkpoint, checkpoint, &mut files)?;
    if files.is_empty() {
        return Err("checkpoint dir is empty; nothing to ferry".into());
    }
    // manifest.json last — everything else first.
    files.sort_by_key(|(_, rel)| (rel == "manifest.json", rel.clone()));
    let prefix = format!(
        "{base}/api/v1/storage/{bank_fqn}/files/{SINK_CHECKPOINT_DIR}/{}/{run}",
        crate::garden::directory::slug(fqn)
    );
    for (abs, rel) in files {
        let body = std::fs::read(&abs).map_err(|e| format!("read {rel}: {e}"))?;
        let uri = format!("{prefix}/{rel}");
        http_put_bytes(&uri, body.into())
            .await
            .map_err(|e| format!("{rel}: {e}"))?;
    }
    Ok(())
}

/// PUT raw bytes at one URL. The same wire law as the room's own faces:
/// the sink answers or the ferry says so — silence is never success.
async fn http_put_bytes(uri: &str, body: bytes::Bytes) -> Result<(), String> {
    use hyper_util::client::legacy::connect::HttpConnector;
    use hyper_util::client::legacy::Client;
    use hyper_util::rt::TokioExecutor;
    let client: Client<HttpConnector, http_body_util::Full<bytes::Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();
    let uri: hyper::Uri = uri.parse().map_err(|e| format!("bad sink uri: {e}"))?;
    let request = hyper::Request::builder()
        .method(hyper::Method::PUT)
        .uri(uri)
        .header("content-type", "application/octet-stream")
        .body(http_body_util::Full::new(body))
        .map_err(|e| format!("build request: {e}"))?;
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(FERRY_HTTP_TIMEOUT_SECS),
        client.request(request),
    )
    .await
    .map_err(|_| format!("exceeded its {FERRY_HTTP_TIMEOUT_SECS}s budget"))?
    .map_err(|e| format!("{e}"))?;
    if !response.status().is_success() {
        return Err(format!("sink answered http {}", response.status().as_u16()));
    }
    http_body_util::BodyExt::collect(response.into_body())
        .await
        .map_err(|e| format!("read sink answer: {e}"))?;
    Ok(())
}

/// Every regular file under `root`, as (absolute, relative-to-root).
fn collect_files(root: &Path, rel_root: &Path, out: &mut Vec<(PathBuf, String)>) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_files(&p, rel_root, out)?;
        } else {
            // Forward slashes always: the manifest's paths must match the
            // tar's on every platform (B1 - one shape, many mouths).
            let rel = p
                .strip_prefix(rel_root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR.to_string().as_str(), "/");
            out.push((p.clone(), rel));
        }
    }
    Ok(())
}

#[cfg(test)]
pub fn latest_checkpoint(checkpoints_root: &Path, fqn: &str) -> Option<PathBuf> {
    let dir = checkpoints_root.join(crate::garden::directory::slug(fqn));
    let mut runs: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !p.ends_with(".partial"))
        .collect();
    runs.sort();
    runs.pop()
}

/// Unpack a verified checkpoint's volumes into a fresh volumes directory
/// (§3 restore: select -> verify -> unpack; replant composes on top).
/// Returns the file count. Entry paths are traversal-checked: a
/// checkpoint that tries to escape is refused, loudly.
#[allow(dead_code)]
pub fn unpack_volumes(checkpoint: &Path, volumes_dir: &Path) -> Result<usize, String> {
    checkpoint::verify(checkpoint)?;
    let manifest_path = checkpoint.join("manifest.json");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&manifest_path).map_err(|e| format!("manifest unreadable: {e}"))?,
    )
    .map_err(|e| format!("manifest unparsable: {e}"))?;
    let archive_file = manifest["archive"]["file"]
        .as_str()
        .unwrap_or("checkpoint.tar.zst")
        .to_string();
    let archive_bytes = std::fs::read(checkpoint.join(&archive_file))
        .map_err(|e| format!("archive unreadable: {e}"))?;
    let tar_bytes = zstd::stream::decode_all(&archive_bytes[..])
        .map_err(|e| format!("archive decompress: {e}"))?;
    let mut archive = tar::Archive::new(&tar_bytes[..]);
    let mut count = 0usize;
    for entry in archive.entries().map_err(|e| format!("archive walk: {e}"))? {
        let mut entry = entry.map_err(|e| format!("archive walk: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("archive path: {e}"))?
            .to_string_lossy()
            .into_owned();
        if path.contains("..") || path.starts_with('/') {
            return Err(format!("checkpoint carries an unsafe path: '{path}' — refused"));
        }
        let Some(rel) = path.strip_prefix("volumes/") else {
            continue; // only volumes restore here; the signature is already home
        };
        if rel.is_empty() {
            continue;
        }
        let target = volumes_dir.join(rel);
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| format!("{rel}: {e}"))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{rel}: {e}"))?;
        }
        entry
            .unpack(&target)
            .map_err(|e| format!("restore of '{rel}' failed: {e}"))?;
        count += 1;
    }
    Ok(count)
}
/// The capture cadence (§3: rotation keeps five DAILY checkpoints - the
/// cadence is the protocol's own period, R2.8).
pub const CAPTURE_CADENCE_SECS: u64 = 86_400;

/// Build the imprint workload for a placed offering (shared by the HTTP
/// face and the scheduler - one composer, many mouths, B1).
pub fn workload_for(
    offering: &crate::garden::model::Offering,
    dirs_root: &crate::garden::directory::OfferingsRoot,
) -> Workload {
    let dir = dirs_root.dir_for(&offering.name).root;
    let volumes = offering
        .managed()
        .map(|m| {
            m.spec
                .volumes
                .iter()
                .map(|v| {
                    let name = std::path::Path::new(&v.host_path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| v.host_path.clone());
                    (std::path::PathBuf::from(&v.host_path), name)
                })
                .collect()
        })
        .unwrap_or_default();
    Workload {
        dir,
        volumes,
        container: crate::garden::docker::DockerRuntime::container_name(&offering.name),
        running: offering.status == crate::garden::model::Status::Running,
    }
}

/// The capture scheduler (§3's "five daily"): every cadence, every placed
/// managed offering with a TRUSTED declared will runs it. Untrusted
/// offerings are never silently tarred; in-flight runs are never doubled.
pub async fn run_scheduler(
    service: Arc<crate::garden::service::OfferingService>,
    runner: Arc<Runner>,
    cadence: Duration,
    token: tokio_util::sync::CancellationToken,
) {
    let mut ticker = tokio::time::interval(cadence);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // A boot is not a calendar: consume interval's immediate first tick
    // so the will first runs one full cadence after boot, not at boot
    // (W15: three boot-time captures witnessed).
    ticker.tick().await;
    loop {
        tokio::select! {
            _ = token.cancelled() => return,
            _ = ticker.tick() => {
                for (offering, policy) in service.capture_targets() {
                    if let Some(last) = runner.last_run(&offering.name)
                        && last.in_flight()
                    {
                        continue; // a will already in flight is not doubled
                    }
                    let workload = workload_for(&offering, &service.dirs_root);
                    let runner = Arc::clone(&runner);
                    let fqn = offering.name.clone();
                    tokio::spawn(async move {
                        if let Err(e) = runner.execute_named(&fqn, &policy, &workload, &uuid::Uuid::now_v7().to_string()).await {
                            tracing::warn!(offering = %fqn, error = %e, "scheduled capture failed");
                        }
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use super::checkpoint::CHECKPOINT_KEEP;
    use crate::garden::will::policy::CapturePolicy;

    /// A hook runner that records every call (argv[0]) - the double for
    /// the docker exec seam (R2.3).
    struct Scripted {
        calls: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl Scripted {
        fn recording() -> (Arc<Self>, Arc<std::sync::Mutex<Vec<String>>>) {
            let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
            (
                Arc::new(Self { calls: Arc::clone(&calls) }),
                calls,
            )
        }
    }

    #[async_trait::async_trait]
    impl HookRunner for Scripted {
        async fn exec(
            &self,
            _container: &str,
            argv: &[String],
            _timeout: Duration,
        ) -> Result<String, String> {
            self.calls
                .lock()
                .unwrap()
                .push(argv.first().cloned().unwrap_or_default());
            Ok(String::new())
        }

        async fn exec_lines(
            &self,
            _container: &str,
            _argv: &[String],
        ) -> Result<ExecLines, String> {
            Ok(Box::pin(futures::stream::iter(Vec::new())))
        }
    }

    fn stateless() -> CapturePolicy {
        serde_json::from_value(serde_json::json!({ "mode": "stateless" })).unwrap()
    }

    fn lock_copy() -> CapturePolicy {
        serde_json::from_value(serde_json::json!({
            "mode": "lock-and-copy",
            "quiesce": { "exec": ["quiesce"] },
            "resume": { "exec": ["resume"] },
            "max_locked_s": 30
        }))
        .unwrap()
    }

    /// A workload fixture: signature files, and one real volume on demand.
    fn workload(tmp: &Path, volumes: bool, volume_exists: bool) -> Workload {
        let dir = tmp.join("offering-dir");
        std::fs::create_dir_all(dir.join("configs")).unwrap();
        std::fs::write(dir.join("record.json"), b"{\"v3\":true}").unwrap();
        let mut w = Workload {
            dir: dir.clone(),
            volumes: vec![],
            container: "zen-offering-test".into(),
            running: true,
        };
        if volumes {
            let vol_src = tmp.join("real-volume");
            if volume_exists {
                std::fs::create_dir_all(&vol_src).unwrap();
                std::fs::write(vol_src.join("data.bin"), b"precious").unwrap();
            }
            w.volumes.push((vol_src, "data".into()));
        }
        w
    }

    fn runner(tmp: &Path, hooks: Arc<dyn HookRunner>) -> Runner {
        Runner::new(Arc::new(Storage::new()), hooks)
            .with_roots(tmp.join("workspace"), tmp.join("checkpoints"))
    }

    #[tokio::test]
    async fn stateless_run_commits_a_signed_checkpoint() {
        let tmp = std::env::temp_dir().join(format!("zg-cap-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let (hooks, _calls) = Scripted::recording();
        let runner = runner(&tmp, hooks);
        let w = workload(&tmp, false, false);

        let checkpoint = runner
            .execute("db::default", &stateless(), &w)
            .await
            .unwrap();

        assert!(checkpoint.join("manifest.json").is_file());
        assert!(checkpoint.join("checkpoint.tar.zst").is_file());
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(checkpoint.join("manifest.json")).unwrap())
                .unwrap();
        assert!(manifest["archive"]["sha256"].as_str().is_some());
        let files: Vec<&str> = manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["path"].as_str())
            .collect();
        assert!(files.contains(&"record.json"), "the signature rides: {files:?}");
        // Workspace reclaimed (a run must not leak disk).
        assert!(!tmp.join("workspace").join("db__default").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn lock_copy_imprints_between_quiesce_and_resume() {
        let tmp = std::env::temp_dir().join(format!("zg-cap-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let (hooks, calls) = Scripted::recording();
        let runner = runner(&tmp, hooks);
        let w = workload(&tmp, true, true);

        runner
            .execute("db::default", &lock_copy(), &w)
            .await
            .unwrap();

        let order = calls.lock().unwrap().clone();
        assert_eq!(order, vec!["quiesce", "resume"], "the lock brackets the imprint");

        // The imprint carried the volume into the committed archive.
        let run_dir = runner
            .checkpoints_root
            .join("db__default")
            .read_dir()
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .next()
            .unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(run_dir.join("manifest.json")).unwrap())
                .unwrap();
        let files: Vec<&str> = manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["path"].as_str())
            .collect();
        assert!(
            files.iter().any(|f| f.contains("volumes") && f.contains("data.bin")),
            "the imprint rode along: {files:?}"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn imprint_failure_still_resumes_and_aborts() {
        let tmp = std::env::temp_dir().join(format!("zg-cap-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let (hooks, calls) = Scripted::recording();
        let runner = runner(&tmp, hooks);
        // A volume whose source does not exist: the imprint cannot proceed.
        let w = workload(&tmp, true, false);

        let err = runner
            .execute("db::default", &lock_copy(), &w)
            .await
            .unwrap_err();
        assert!(err.contains("imprint of volume 'data' failed"), "{err}");
        // Resume executed anyway (finally-style), after the failed imprint.
        let order = calls.lock().unwrap().clone();
        assert_eq!(order, vec!["quiesce", "resume"], "resume is finally-style");

        // Nothing committed (the ledger directory may not even exist).
        let ledger = runner.checkpoints_root.join("db__default");
        let empty_or_absent = ledger
            .read_dir()
            .map(|mut n| n.next().is_none())
            .unwrap_or(true);
        assert!(empty_or_absent, "a failed run must not commit");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn rested_offerings_skip_the_lock_entirely() {
        let tmp = std::env::temp_dir().join(format!("zg-cap-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let (hooks, calls) = Scripted::recording();
        let runner = runner(&tmp, hooks);
        let mut w = workload(&tmp, true, true);
        w.running = false; // stopped: direct imprint is consistent by definition

        runner
            .execute("db::default", &lock_copy(), &w)
            .await
            .unwrap();
        let order = calls.lock().unwrap().clone();
        assert_eq!(order, Vec::<String>::new(), "no hooks for a rested offering");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn rotation_keeps_five_checkpoints() {
        let tmp = std::env::temp_dir().join(format!("zg-cap-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let (hooks, _) = Scripted::recording();
        let runner = runner(&tmp, hooks);
        let w = workload(&tmp, false, false);

        for _ in 0..(CHECKPOINT_KEEP + 1) {
            runner.execute("db::default", &stateless(), &w).await.unwrap();
        }
        let kept = runner
            .checkpoints_root
            .join("db__default")
            .read_dir()
            .unwrap()
            .count();
        assert_eq!(kept, CHECKPOINT_KEEP, "rotation holds the line");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn verify_passes_an_honest_checkpoint_and_catches_tampering() {
        let tmp = std::env::temp_dir().join(format!("zg-cap-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let (hooks, _) = Scripted::recording();
        let runner = runner(&tmp, hooks);
        let w = workload(&tmp, true, true);

        let checkpoint = runner
            .execute("db::default", &lock_copy(), &w)
            .await
            .unwrap();

        // An honest checkpoint proves itself.
        let report = checkpoint::verify(&checkpoint).unwrap();
        assert!(report.files >= 2, "signature + imprint: {report:?}");

        // A tampered archive is refused: rewrite the manifest to expect a
        // hash no honest archive could carry.
        let manifest_path = checkpoint.join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest["archive"]["sha256"] = serde_json::json!(format!("{:064x}", 0u128));
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let err = checkpoint::verify(&checkpoint).unwrap_err();
        assert!(err.contains("archive hash mismatch"), "{err}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn unpack_restores_volumes_fresh() {
        let tmp = std::env::temp_dir().join(format!("zg-cap-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let (hooks, _) = Scripted::recording();
        let runner = runner(&tmp, hooks);
        let w = workload(&tmp, true, true);

        runner
            .execute("db::default", &lock_copy(), &w)
            .await
            .unwrap();

        // Select the newest, then restore into a FRESH volumes directory
        // (the replant contract's physical half; select -> verify -> unpack).
        let checkpoint = latest_checkpoint(&runner.checkpoints_root, "db::default")
            .expect("the run committed a checkpoint");
        let fresh = tmp.join("fresh-volumes");
        let restored = unpack_volumes(&checkpoint, &fresh).unwrap();
        assert!(restored >= 1, "the volume's files came home");
        assert_eq!(
            std::fs::read(fresh.join("data").join("data.bin")).unwrap(),
            b"precious"
        );
        // The signature does NOT land here — volumes only; replant owns it.
        assert!(!fresh.join("record.json").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The scheduler's promise: a placed offering with a TRUSTED declared
    /// will runs it on the cadence; nothing else is silently tarred.
    #[tokio::test]
    async fn the_scheduler_runs_declared_wills() {
        use crate::garden::directory::OfferingsRoot;
        use crate::garden::manifest::Catalog;
        use crate::garden::registry::{MemorySnapshotStore, Registry};
        use crate::garden::runtime::{NullRuntime, RuntimeRegistry};
        use crate::garden::service::OfferingService;
        use crate::garden::{model::Location, model::ManagedData, model::ModeData, model::Offering, model::Status};

        let tmp = std::env::temp_dir().join(format!("zg-sched-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();

        let yaml = "kind: software
name: db
category: data
description: x
managed:
  image: db:7
capture:
  mode: stateless
";
        let catalog = Catalog::embedded(vec![("db", yaml)]).unwrap();
        let registry = Arc::new(Registry::new(Arc::new(MemorySnapshotStore::default())));
        let worlds = Arc::new(RuntimeRegistry::build(vec![Arc::new(NullRuntime)]));
        let service = Arc::new(OfferingService::new(
            Arc::clone(&registry),
            worlds,
            "null".into(),
            Arc::new(catalog),
            Arc::new(crate::garden::facts::Factsheet::empty()),
            OfferingsRoot::new(tmp.join("offerings")),
            crate::garden::ports::Pool::default(),
            None,
        ));
        // A placed managed offering whose stem declares the will.
        registry.register(Offering {
            offering_id: uuid::Uuid::now_v7().to_string(),
            name: "db::default".into(),
            offering: "db".into(),
            category: "data".into(),
            status: Status::Running,
            location: Location { host: "localhost".into(), port: 0, protocol: "http".into() },
            sub_capabilities: Default::default(),
            mode_data: ModeData::Managed(ManagedData {
                runtime_kind: "null".into(),
                spec: Default::default(),
                port_map: Default::default(),
                plan: None,
            }),
            registered_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        });
        assert_eq!(service.capture_targets().len(), 1, "the declared will is a target");

        let (hooks, _) = Scripted::recording();
        let runner = Arc::new(runner(&tmp, hooks));
        let token = tokio_util::sync::CancellationToken::new();
        tokio::spawn(run_scheduler(
            Arc::clone(&service),
            Arc::clone(&runner),
            Duration::from_millis(100),
            token.clone(),
        ));

        // The will executes on cadence: a committed checkpoint appears.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let mut committed = false;
        while std::time::Instant::now() < deadline {
            if latest_checkpoint(&tmp.join("checkpoints"), "db::default").is_some() {
                committed = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        token.cancel();
        assert!(committed, "the scheduler ran the declared will");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn ferry_reaches_mounted_sink_banks() {
        let tmp = std::env::temp_dir().join(format!("zg-cap-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let (hooks, _) = Scripted::recording();
        let storage = Arc::new(Storage::new());
        // A mounted bank granted the sink role (ADR-0005 §4) - adopted
        // for real: the ceremony writes the record the declaration amends.
        let vol = crate::garden::storage::VolumeFact {
            mount_point: tmp.join("sink-bank"),
            device_id: None,
            fqn: None,
            roles: Vec::new(),
            capacity_bytes: 1_000_000,
            available_bytes: 900_000,
        };
        std::fs::create_dir_all(&vol.mount_point).unwrap();
        storage.adopt(&vol, "seed-vault", "stone-1").unwrap();
        storage
            .set_roles(
                "seed-vault",
                vec![garden_glossary::bank::role::SINK.into()],
            )
            .unwrap()
            .unwrap();

        let runner = Runner::new(Arc::clone(&storage), hooks).with_roots(
            tmp.join("workspace"),
            tmp.join("checkpoints"),
        );
        let w = workload(&tmp, false, false);
        let info = runner.last_run("db::default");
        let _ = info;
        let checkpoint = runner
            .execute("db::default", &stateless(), &w)
            .await
            .unwrap();

        // The bank mount holds the ferried checkpoint tree.
        let ferried = tmp
            .join("sink-bank")
            .join(SINK_CHECKPOINT_DIR)
            .join("db__default")
            .join(checkpoint.file_name().unwrap());
        assert!(ferried.join("manifest.json").is_file(), "the sink holds the will");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Runs converge at boot from the offering's own audit chain: the
    /// committed run returns with its checkpoint and sinks; a run left
    /// in flight by a restart is marked interrupted, never forgotten.
    #[test]
    fn replay_runs_rebuilds_the_last_run_from_the_chain() {
        use crate::garden::directory::{DirectoryStore, OfferingsRoot};
        use crate::garden::events::EventLog;

        let tmp = std::env::temp_dir().join(format!("zg-replay-{}", uuid::Uuid::now_v7()));
        let base = tmp.join("offerings");
        std::fs::create_dir_all(base.join("ntfy/default")).unwrap();

        // The chain, as the saga would have written it.
        let log = EventLog::for_dir(&base, "ntfy::default");
        log.append("RunStarted", serde_json::json!({ "run": "r-early", "mode": "lock-and-copy" })).unwrap();
        log.append("RunFailed", serde_json::json!({ "run": "r-early", "error": "imprint refused" })).unwrap();
        log.append("RunStarted", serde_json::json!({ "run": "r-last", "mode": "lock-and-copy" })).unwrap();
        log.append(
            "CheckpointCommitted",
            serde_json::json!({ "run": "r-last", "checkpoint": "/cp/r-last", "ferried": ["seed-vault::default"] }),
        )
        .unwrap();

        let runner = Runner::new(
            Arc::new(Storage::new()),
            Arc::new(NullHooks),
        );
        runner.replay_runs(&OfferingsRoot::new(base.clone()));

        let last = runner.last_run("ntfy::default").expect("the last run came home");
        assert_eq!(last.run_id, "r-last");
        assert_eq!(last.phase, "done");
        assert_eq!(last.checkpoint.as_deref(), Some("/cp/r-last"));
        assert_eq!(last.ferried_to.as_deref(), Some(&["seed-vault::default".to_string()][..]));

        // The interrupted run is honestly dead, not forgotten.
        let mut interrupted = Run::begin("ntfy::default", "r-early");
        interrupted.fail("imprint refused");
        assert!(!interrupted.in_flight());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// The room's heard banks: only a MOUNTED sink held elsewhere is a
    /// remote ferry target; local hits and silent stones never double up.
    #[test]
    fn remote_sinks_picks_mounted_sink_banks_the_room_hears() {
        use garden_contract::chirp::{
            BankEntry, ChirpFrame, Inventory, Moss, Network, PeerAddress, Presence, Reception,
            Stone,
        };
        use crate::room::topology::StoneView;

        fn peer(id: &str, ip: [u8; 4], banks: Option<Inventory<BankEntry>>) -> StoneView {
            let now = chrono::Utc::now();
            StoneView {
                body: ChirpFrame {
                    stone: Stone {
                        id: id.into(),
                        name: format!("stone-{id}"),
                        moss: Moss { version: "0.1.0".into() },
                        network: Network {
                            address: PeerAddress {
                                ip: std::net::IpAddr::V4(std::net::Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3])),
                                port: 7285,
                                tls_port: None,
                            },
                            mac: None,
                        },
                    },
                    presence: Presence {
                        health: garden_glossary::health::THRIVING.into(),
                        status: garden_glossary::presence::ONLINE.into(),
                    },
                    inventory: garden_contract::chirp::InventoryMap { banks, ..Default::default() },
                    meta: Default::default(),
                    received: Reception { discovered_at: now, last_seen: now },
                },
                last_seen: now,
                chirps: 1,
            }
        }

        fn bank(fqn: &str, state: &str, roles: &[&str]) -> BankEntry {
            BankEntry {
                fqn: fqn.into(),
                device_id: "dev-1".into(),
                state: state.into(),
                roles: roles.iter().map(|s| s.to_string()).collect(),
                capacity_bytes: Some(1),
                used_bytes: Some(0),
            }
        }

        let snapshot = vec![
            peer("195", [192, 168, 1, 195], Some(Inventory {
                rev: Some(1),
                total: None,
                items: vec![
                    bank("seed-vault::default", garden_glossary::bank::MOUNTED, &["sink"]),
                    bank("cold-storage::nas", garden_glossary::bank::MOUNTED, &[]),
                    bank("unplugged::usb", "ejected", &["sink"]),
                ],
            })),
            peer("82", [192, 168, 1, 82], None),
        ];

        let targets = remote_sinks(&snapshot, &[]);
        assert_eq!(targets.len(), 1, "only the mounted sink counts: {targets:?}");
        assert_eq!(targets[0].0, "seed-vault::default");
        assert_eq!(targets[0].1, "http://192.168.1.195:7285");

        let again = remote_sinks(&snapshot, &["seed-vault::default".into()]);
        assert!(again.is_empty(), "a bank reached locally is not ferried twice");
    }

    /// The remote lane writes through the holder's file face: the
    /// archive lands, and manifest.json lands LAST — on dumb storage
    /// the manifest is the commit marker.
    #[tokio::test]
    async fn ferry_rides_the_holders_file_face() {
        use std::sync::Mutex;

        #[derive(Clone)]
        struct Sink(Arc<Mutex<Vec<(String, Vec<u8>)>>>);
        async fn put_file(
            axum::extract::Path((fqn, path)): axum::extract::Path<(String, String)>,
            axum::extract::State(sink): axum::extract::State<Sink>,
            body: axum::body::Bytes,
        ) -> axum::http::StatusCode {
            sink.0.lock().unwrap().push((format!("/api/v1/storage/{fqn}/files/{path}"), body.to_vec()));
            axum::http::StatusCode::OK
        }

        let received: Arc<Mutex<Vec<(String, Vec<u8>)>>> = Arc::default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new()
            .route("/api/v1/storage/{fqn}/files/{*path}", axum::routing::put(put_file))
            .with_state(Sink(Arc::clone(&received)));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        // A committed checkpoint: archive + manifest.
        let tmp = std::env::temp_dir().join(format!("zg-ferry-{}", uuid::Uuid::now_v7()));
        let run_dir = tmp.join("ntfy__default").join("run-1");
        std::fs::create_dir_all(&run_dir).unwrap();
        std::fs::write(run_dir.join("checkpoint.tar.zst"), b"archive-bytes").unwrap();
        std::fs::write(run_dir.join("manifest.json"), b"{}").unwrap();

        ferry_via_http(
            "ntfy::default",
            &run_dir,
            &format!("http://{addr}"),
            "seed-vault::default",
        )
        .await
        .unwrap();

        let took = received.lock().unwrap();
        assert_eq!(took.len(), 2, "both files crossed: {took:?}");
        for (path, body) in took.iter() {
            assert!(
                path.starts_with("/api/v1/storage/seed-vault::default/files/zen-garden/checkpoints/ntfy__default/run-1/"),
                "the checkpoint rides the sink dir: {path}"
            );
            if path.ends_with("checkpoint.tar.zst") {
                assert_eq!(body, b"archive-bytes", "bytes intact");
            }
        }
        assert!(
            took.last().unwrap().0.ends_with("manifest.json"),
            "the manifest commits LAST: {:?}",
            took.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

/// The W7 rehearsal, minus the live docker: capture a will, kill the
/// offering (workload AND directory AND volumes), replant from the
/// checkpoint - identity, spec, and volumes come home, and the checkpoint
/// of a SINK BANK is a legal source (the dead stone's ledger is not the
/// only witness).
#[tokio::test]
async fn replant_restores_the_incarnation_from_a_checkpoint() {
    use crate::garden::directory::{DirectoryStore, OfferingsRoot};
    use crate::garden::registry::Registry;
    use crate::garden::storage::VolumeFact;

    let tmp = std::env::temp_dir().join(format!("zg-replant-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&tmp).unwrap();

    // The stone: registry + directories + a mounted SINK bank, adopted
    // for real (the ceremony writes the record declarations amend).
    let store = DirectoryStore::new(tmp.join("offerings"));
    let registry = Arc::new(Registry::new(Arc::new(store)));
    let storage = Arc::new(Storage::new());
    let sink_mount = tmp.join("sink-bank");
    let vol = VolumeFact {
        mount_point: sink_mount.clone(),
        device_id: None,
        fqn: None,
        roles: Vec::new(),
        capacity_bytes: 1_000_000,
        available_bytes: 900_000,
    };
    std::fs::create_dir_all(&sink_mount).unwrap();
    storage.adopt(&vol, "seed-vault", "stone-1").unwrap();
    storage
        .set_roles(
            "seed-vault",
            vec![garden_glossary::bank::role::SINK.into()],
        )
        .unwrap()
        .unwrap();

    let (hooks, _calls) = Scripted::recording();
    let runner = Runner::new(Arc::clone(&storage), hooks).with_roots(
        tmp.join("workspace"),
        tmp.join("checkpoints"),
    );

    // The offering's directory: a v3 record + a real volume with data.
    let root = OfferingsRoot::new(tmp.join("offerings"));
    let dir = root.dir_for("db::default");
    std::fs::create_dir_all(dir.volumes().join("data")).unwrap();
    std::fs::write(dir.volumes().join("data").join("data.bin"), b"precious").unwrap();
    let record = crate::garden::record::OfferingRecord {
        identity: crate::garden::record::Identity {
            offering_id: "01a0dead-0000-7000-8000-0000000000ef".into(),
            name: "db::default".into(),
            stem: "db".into(),
            category: "data".into(),
        },
        state: crate::garden::record::State {
            status: crate::garden::model::Status::Running,
        },
        sub_capabilities: Default::default(),
        location: crate::garden::model::Location {
            host: "localhost".into(),
            port: 7300,
            protocol: "http".into(),
        },
        mode_data: crate::garden::model::ModeData::Managed(
            crate::garden::model::ManagedData {
                runtime_kind: "oci".into(),
                spec: crate::garden::model::WorkloadSpec {
                    image: "db:7".into(),
                    ..Default::default()
                },
                port_map: Default::default(),
                plan: None,
            },
        ),
        registered_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    std::fs::write(
        dir.record_json(),
        serde_json::to_vec_pretty(&record).unwrap(),
    )
    .unwrap();

    // Capture the will (lock-and-copy: the volume imprint rides).
    let workload = Workload {
        dir: dir.root.clone(),
        volumes: vec![(dir.volumes().join("data"), "data".into())],
        container: "zen-offering-db__default".into(),
        running: true,
    };
    let policy: CapturePolicy =
        serde_json::from_value(serde_json::json!({
            "mode": "lock-and-copy",
            "quiesce": { "exec": ["quiesce"] },
            "resume": { "exec": ["resume"] },
            "max_locked_s": 30
        }))
        .unwrap();
    runner.execute("db::default", &policy, &workload).await.unwrap();

    // DEATH: the directory, volumes and registration vanish. The bank's
    // ferried checkpoint survives (it was ferried during the run).
    let _ = std::fs::remove_dir_all(&dir.root);
    registry.remove(&record.identity.offering_id);
    assert!(registry.get_by_name("db::default").is_none());

    // REPLANT: select (the bank holds it), restore, and hand the record
    // to the service's place-from-stored-spec path.
    let checkpoint = runner
        .select_checkpoint("db::default", None)
        .expect("the will outlives the stone");
    assert!(
        checkpoint.starts_with(&sink_mount),
        "the ferried sink copy is a legal source: {checkpoint:?}"
    );
    let (count, final_hash) = runner.restore_into(&checkpoint, &dir.root).unwrap();
    assert!(count >= 2, "record + volume ride home: {count}");
    assert_eq!(
        std::fs::read(dir.volumes().join("data").join("data.bin")).unwrap(),
        b"precious"
    );

    // The restored record parses and is handed to the service: placement
    // itself is the runtime adapter's craft (W7 watches it live with docker).
    let bytes = std::fs::read(dir.record_json()).unwrap();
    let restored: crate::garden::record::OfferingRecord =
        serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored.identity.offering_id, record.identity.offering_id, "same identity");
    assert!(
        matches!(restored.mode_data, crate::garden::model::ModeData::Managed(_)),
        "the restored will is managed"
    );
    if let crate::garden::model::ModeData::Managed(m) = &restored.mode_data {
        assert_eq!(m.spec.image, "db:7", "the stored spec is complete");
    }
    let _ = (final_hash, count);
    let _ = std::fs::remove_dir_all(&tmp);
}
}
