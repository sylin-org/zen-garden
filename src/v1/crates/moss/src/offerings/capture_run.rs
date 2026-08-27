//! The pipeline that executes a will (ADR-0005 §2-§3): Phase A is
//! synchronous and bounded by DISK speed — carve a workspace, quiesce,
//! imprint, resume (finally-style); Phase B is asynchronous and unbounded —
//! pack, ferry to sinks, commit, reclaim. Lock time belongs to disk speed,
//! never network speed.

use super::capture::{CaptureMode, CapturePolicy};
use super::storage::Storage;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Checkpoints kept per offering per location (§3 rotation default).
pub const CHECKPOINT_KEEP: usize = 5;
/// Directory under a sink bank that receives ferried checkpoints.
pub const SINK_CHECKPOINT_DIR: &str = "zen-garden/checkpoints";

/// Where capture work happens: `~/.zen-garden/workspace/{fqn}/{run}/`
/// (MOSS_WORKSPACE_DIR overrides the root — deployment concern, R3.7).
pub fn workspace_root() -> PathBuf {
    std::env::var("MOSS_WORKSPACE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_or_home().join(".zen-garden").join("workspace")
        })
}

/// Local checkpoint ledger: `~/.zen-garden/checkpoints/{fqn}/{run}/`.
pub fn checkpoints_root() -> PathBuf {
    dirs_or_home().join(".zen-garden").join("checkpoints")
}

fn dirs_or_home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// The seam hooks run through: argv inside the offering's container.
/// Docker implements it with `exec`; tests doubles script the server.
#[async_trait::async_trait]
pub trait HookRunner: Send + Sync {
    async fn exec(
        &self,
        container: &str,
        argv: &[String],
        timeout: Duration,
    ) -> Result<(), String>;
}

/// The no-world hook runner: refuses loudly. A companion modality has no
/// containers to tell anything to (R2.5: degrade observable, never silent).
pub struct NullHooks;

#[async_trait::async_trait]
impl HookRunner for NullHooks {
    async fn exec(&self, _: &str, _: &[String], _: Duration) -> Result<(), String> {
        Err("no container runtime on this stone: hooks cannot run".into())
    }
}

/// What the last (or running) capture of an offering looks like.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunInfo {
    pub fqn: String,
    pub run_id: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
    /// Where the checkpoint landed (local ledger always; sinks when mounted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ferried_to: Option<Vec<String>>,
}

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
    runs: parking_lot::Mutex<HashMap<String, RunInfo>>,
}

impl Runner {
    pub fn new(storage: Arc<Storage>, hooks: Arc<dyn HookRunner>) -> Self {
        Self {
            workspace_root: workspace_root(),
            checkpoints_root: checkpoints_root(),
            storage,
            hooks,
            runs: parking_lot::Mutex::new(HashMap::new()),
        }
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
    pub fn last_run(&self, fqn: &str) -> Option<RunInfo> {
        self.runs.lock().get(fqn).cloned()
    }

    fn record(&self, info: RunInfo) {
        self.runs.lock().insert(info.fqn.clone(), info);
    }

    /// Publish the caller-visible "accepted" record before the task starts.
    pub fn announce(&self, info: RunInfo) {
        self.record(info);
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
        let mut info = RunInfo {
            fqn: fqn.to_string(),
            run_id: run_id.to_string(),
            started_at: chrono::Utc::now(),
            phase: "imprint".into(),
            error: None,
            checkpoint: None,
            ferried_to: None,
        };
        self.record(info.clone());
        let workspace = self
            .workspace_root
            .join(super::directory::slug(fqn))
            .join(run_id);
        let result = self
            .execute_inner(fqn, policy, workload, &workspace, run_id, &mut info)
            .await;
        match &result {
            Ok(checkpoint) => {
                info.phase = "done".into();
                info.checkpoint = Some(checkpoint.display().to_string());
            }
            Err(e) => {
                info.phase = "failed".into();
                info.error = Some(e.clone());
            }
        }
        // Reclaim the workspace either way (§2 Phase B's last step; a
        // failed run must not leak disk). The emptied offering directory
        // goes too, best-effort.
        let _ = std::fs::remove_dir_all(&workspace);
        let _ = std::fs::remove_dir(workspace.parent().unwrap_or(&workspace));
        self.record(info);
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
        info: &mut RunInfo,
    ) -> Result<PathBuf, String> {
        // ---- Phase A: synchronous, bounded (disk speed) ----
        std::fs::create_dir_all(workspace)
            .map_err(|e| format!("workspace carve failed: {e}"))?;
        self.phase_a(policy, workload, workspace, run_id, info).await?;

        // ---- Phase B: pack, ferry, commit, reclaim ----
        info.phase = "pack".into();
        self.record(info.clone());
        let checkpoint = self.pack(fqn, workload, workspace, run_id)?;
        info.phase = "ferry".into();
        self.record(info.clone());
        let ferried = self.ferry(fqn, &checkpoint);
        info.ferried_to = Some(ferried);
        self.rotate(&self.checkpoints_root.join(super::directory::slug(fqn)));
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
        info: &mut RunInfo,
    ) -> Result<(), String> {
        match policy.mode {
            CaptureMode::Stateless => Ok(()), // signature only; Phase B carries it
            CaptureMode::LockAndCopy => {
                let (Some(quiesce), Some(resume)) = (&policy.quiesce, &policy.resume) else {
                    return Err("lock-and-copy requires quiesce and resume hooks".into());
                };
                let container = &workload.container;
                if workload.running {
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
                if workload.running {
                    self.hooks
                        .exec(container, &resume.exec, Duration::from_secs(resume.timeout_s))
                        .await
                        .map_err(|e| format!("resume failed — the lock may be stranded: {e}"))?;
                }
                match imprint {
                    Ok(Ok(())) => {
                        tracing::info!(offering = run_id, locked_ms = started.elapsed().as_millis() as u64, "imprint complete inside the lock budget");
                        Ok(())
                    }
                    Ok(Err(e)) => Err(e),
                    Err(_) => Err(format!(
                        "imprint exceeded max_locked_s ({}s): aborted loudly, resume executed, nothing committed",
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
        .inspect(|_| info.phase = "pack".into())
    }

    /// Pack the workspace into `checkpoint.tar.zst` + a SHA-256 manifest,
    /// then commit atomically: everything lands in `{run}.partial/` and
    /// ONE rename makes it `{run}` (§3 — dumb-storage-friendly).
    fn pack(
        &self,
        fqn: &str,
        workload: &Workload,
        workspace: &Path,
        run_id: &str,
    ) -> Result<PathBuf, String> {
        // Collect the file set: signature (record/plan/configs) + imprint.
        let mut files: Vec<(PathBuf, String)> = Vec::new(); // (abs, rel)
        for entry in ["record.json", "candidate.json", "plan.json"] {
            let p = workload.dir.join(entry);
            if p.is_file() {
                files.push((p, entry.to_string()));
            }
        }
        let configs = workload.dir.join("configs");
        collect_files(&configs, &configs, &mut files)?;
        collect_files(workspace, workspace, &mut files)?;

        // Deterministic order for stable manifests.
        files.sort_by(|a, b| a.1.cmp(&b.1));

        let staged = self
            .checkpoints_root
            .join(super::directory::slug(fqn))
            .join(format!("{run_id}.partial"));
        std::fs::create_dir_all(&staged)
            .map_err(|e| format!("checkpoint stage failed: {e}"))?;

        // tar the file set, zstd the stream, hash the bytes as they flow.
        let archive_path = staged.join("checkpoint.tar.zst");
        let tar_buffer = tar::Builder::new(Vec::new());
        let mut tar_buffer = tar_buffer;
        for (abs, rel) in &files {
            tar_buffer
                .append_path_with_name(abs, rel)
                .map_err(|e| format!("pack: {}: {e}", rel))?;
        }
        let tar_bytes = tar_buffer
            .into_inner()
            .map_err(|e| format!("pack: {e}"))?;
        let mut hasher = Sha256::new();
        hasher.update(&tar_bytes);
        let archive_sha = hex(&hasher.finalize());
        let compressed = zstd::stream::encode_all(&tar_bytes[..], 3)
            .map_err(|e| format!("pack compress: {e}"))?;
        std::fs::write(&archive_path, &compressed)
            .map_err(|e| format!("archive write: {e}"))?;

        // The embedded manifest: per-file hashes plus the archive's own.
        let mut manifest = serde_json::json!({
            "fqn": fqn,
            "run": run_id,
            "created_at": chrono::Utc::now().to_rfc3339(),
            "archive": {
                "file": "checkpoint.tar.zst",
                "sha256": archive_sha,
                "bytes": compressed.len(),
            },
            "files": [],
        });
        let file_records: Vec<serde_json::Value> = files
            .iter()
            .map(|(abs, rel)| {
                let mut h = Sha256::new();
                let bytes = std::fs::read(abs).unwrap_or_default();
                h.update(&bytes);
                serde_json::json!({
                    "path": rel,
                    "sha256": hex(&h.finalize()),
                    "bytes": bytes.len(),
                })
            })
            .collect();
        manifest["files"] = serde_json::Value::Array(file_records);
        let manifest_path = staged.join("manifest.json");
        // fs::write closes the handle before we rename the directory
        // (Windows denies renaming a tree with an open file in it).
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| format!("manifest encode: {e}"))?;
        std::fs::write(&manifest_path, manifest_bytes)
            .map_err(|e| format!("manifest write: {e}"))?;

        // The commit: one rename makes it a checkpoint (§3).
        let final_dir = staged.with_file_name(run_id);
        std::fs::rename(&staged, &final_dir)
            .map_err(|e| format!("checkpoint commit rename: {e}"))?;
        Ok(final_dir)
    }

    /// Copy the committed checkpoint to every mounted sink bank (§4):
    /// best-effort per sink — a sink that fails is loud, not fatal; the
    /// local ledger already holds the truth.
    fn ferry(&self, fqn: &str, checkpoint: &Path) -> Vec<String> {
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
                .join(super::directory::slug(fqn));
            if copy_tree(checkpoint, &target.join(checkpoint.file_name().unwrap_or_default())).is_ok() {
                reached.push(bank.fqn.clone());
            } else {
                tracing::warn!(sink = %bank.fqn, "ferry failed; sink stays loud in posture");
            }
        }
        reached
    }

    /// Keep the newest [`CHECKPOINT_KEEP`] checkpoints in one location.
    fn rotate(&self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut runs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && !p.ends_with(".partial"))
            .collect();
        runs.sort();
        while runs.len() > CHECKPOINT_KEEP {
            let oldest = runs.remove(0);
            let _ = std::fs::remove_dir_all(&oldest);
        }
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
/// What verifying a checkpoint found.
// Consumed by the restore/replant faces (slice 5); tests exercise it now.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct VerifyReport {
    pub files: usize,
    pub bytes: u64,
}

/// The newest committed checkpoint of an offering, if any (§3 select).
/// Replant's slice gives this its consumer (fetch latest -> verify ->
/// unpack -> place); until then it is exercised by tests only.
#[cfg(test)]
pub fn latest_checkpoint(checkpoints_root: &Path, fqn: &str) -> Option<PathBuf> {
    let dir = checkpoints_root.join(super::directory::slug(fqn));
    let mut runs: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !p.ends_with(".partial"))
        .collect();
    runs.sort();
    runs.pop()
}

/// Verify a checkpoint against its embedded manifest (§3): the archive's
/// own hash, then every file's hash. A checkpoint that cannot prove itself
/// is not a checkpoint — refuse loudly, restore nothing.
#[allow(dead_code)]
pub fn verify_checkpoint(checkpoint: &Path) -> Result<VerifyReport, String> {
    let manifest_path = checkpoint.join("manifest.json");
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .map_err(|e| format!("manifest unreadable: {e}"))?,
    )
    .map_err(|e| format!("manifest unparsable: {e}"))?;

    let archive_file = manifest["archive"]["file"]
        .as_str()
        .unwrap_or("checkpoint.tar.zst")
        .to_string();
    let want_archive = manifest["archive"]["sha256"]
        .as_str()
        .ok_or("manifest declares no archive hash")?
        .to_string();
    let archive_bytes = std::fs::read(checkpoint.join(&archive_file))
        .map_err(|e| format!("archive unreadable: {e}"))?;
    // The manifest hashes the TAR (pack hashed it before compressing):
    // decode first, then prove the decoded bytes.
    let tar_bytes = zstd::stream::decode_all(&archive_bytes[..])
        .map_err(|e| format!("archive decompress: {e}"))?;
    let mut h = Sha256::new();
    h.update(&tar_bytes);
    let got = hex(&h.finalize());
    if got != want_archive {
        return Err(format!(
            "archive hash mismatch: manifest says {want_archive}, archive proves {got}"
        ));
    }

    // The expected file set, by path -> hash.
    let mut expected: HashMap<String, String> = HashMap::new();
    for f in manifest["files"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
        if let (Some(p), Some(s)) = (f["path"].as_str(), f["sha256"].as_str()) {
            expected.insert(p.to_string(), s.to_string());
        }
    }

    let mut archive = tar::Archive::new(&tar_bytes[..]);
    let mut files = 0usize;
    let mut bytes = 0u64;
    for entry in archive.entries().map_err(|e| format!("archive walk: {e}"))? {
        let mut entry = entry.map_err(|e| format!("archive walk: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("archive path: {e}"))?
            .to_string_lossy()
            .into_owned();
        let mut h = Sha256::new();
        let n = std::io::copy(&mut entry, &mut h).map_err(|e| format!("{path}: {e}"))?;
        bytes += n;
        let got = hex(&h.finalize());
        match expected.get(&path) {
            Some(want) if want == &got => {}
            Some(want) => {
                return Err(format!("file hash mismatch for '{path}': manifest says {want}, archive proves {got}"))
            }
            None => return Err(format!("archive carries '{path}' which the manifest never declared")),
        }
        files += 1;
    }
    if files != expected.len() {
        return Err(format!(
            "manifest declares {} files but the archive held {files}",
            expected.len()
        ));
    }
    Ok(VerifyReport { files, bytes })
}

/// Unpack a verified checkpoint's volumes into a fresh volumes directory
/// (§3 restore: select -> verify -> unpack; replant composes on top).
/// Returns the file count. Entry paths are traversal-checked: a
/// checkpoint that tries to escape is refused, loudly.
#[allow(dead_code)]
pub fn unpack_volumes(checkpoint: &Path, volumes_dir: &Path) -> Result<usize, String> {
    verify_checkpoint(checkpoint)?;
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

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::offerings::capture::CapturePolicy;

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
        ) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(argv.first().cloned().unwrap_or_default());
            Ok(())
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
        let report = verify_checkpoint(&checkpoint).unwrap();
        assert!(report.files >= 2, "signature + imprint: {report:?}");

        // A tampered archive is refused: rewrite the manifest to expect a
        // hash no honest archive could carry.
        let manifest_path = checkpoint.join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest["archive"]["sha256"] = serde_json::json!(format!("{:064x}", 0u128));
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let err = verify_checkpoint(&checkpoint).unwrap_err();
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

    #[tokio::test]
    async fn ferry_reaches_mounted_sink_banks() {
        let tmp = std::env::temp_dir().join(format!("zg-cap-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let (hooks, _) = Scripted::recording();
        let storage = Arc::new(Storage::new());
        // A mounted bank granted the sink role (ADR-0005 §4).
        let vol = crate::offerings::storage::VolumeFact {
            mount_point: tmp.join("sink-bank"),
            device_id: Some("dev-sink".into()),
            fqn: Some("seed-vault::default".into()),
            capacity_bytes: 1_000_000,
            available_bytes: 900_000,
        };
        std::fs::create_dir_all(&vol.mount_point).unwrap();
        storage.reconcile(&[vol]);
        storage
            .set_roles(
                "seed-vault::default",
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
}
