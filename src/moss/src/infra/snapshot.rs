//! Snapshot capture — orchestrates the capture-pack-record flow
//! that produces a [`SnapshotManifest`] in a [`SnapshotStore`]
//! and a `BackupTaken` entry in the offering's event log.
//!
//! The flow is the same for any storage target (local disk, bank
//! mount, future remote destinations) — the [`SnapshotStore`]
//! trait abstracts where artifacts land. The pure helpers used
//! here ([`classify_volume`], [`sha512_file`], the manifest
//! types) all live in [`crate::domain::snapshot`] and have unit
//! coverage; this module is the I/O-heavy glue.
//!
//! Per ORCH-0039 §"Seed metadata schema", refined by ORCH-0040:
//! 1. Capture the image. Reference-first: if the running image has a
//!    registry digest, record it (no bytes). Otherwise fall back to
//!    committing the container to a new image tag and
//! 2. `docker save`-ing it to `<store>/<id>/image.tar`.
//! 3. For each `(host_path, container_path)` mount Docker
//!    reports, classify it (managed volume / external mount)
//!    and archive it into the corresponding store path
//! 4. Compute SHA512 of every artifact and a SHA256 of the
//!    offering's compiled manifest at capture time
//! 5. Append `BackupTaken` to the offering event log
//! 6. Save the SnapshotManifest
//! 7. Truncate the event log to retain only events from this
//!    snapshot forward
//!
//! Atomicity note: failure between steps 5 and 6 (manifest save
//! after the event was appended) leaves the event log claiming
//! a snapshot that's not in the store. The truncate in step 7
//! is what bounds replay; the orphan event is benign because the
//! plant flow checks the store before honouring the event. We
//! tolerate this gap because the alternative (event last) means
//! a successful capture isn't visible to peers until the next
//! event, which is worse for the replica catch-up story
//! M3 wants. See [ORCH-0039] §M2 cut.
//!
//! [ORCH-0039]: ../../../../docs/decisions/ORCH-0039-seed-based-offering-replication.md

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use garden_common::infra::archive;
use garden_common::offerings::OfferingFqn;

use crate::Moss;
use crate::domain::offering_events::{EventActor, EventKind, EventLog, new_event};
use crate::domain::snapshot::{
    ImageTransport, LocalSnapshotStore, SnapshotExternalMount, SnapshotImage, SnapshotManifest,
    SnapshotStore, SnapshotVolume, VolumeClass, classify_volume, prune_snapshots, sha256_bytes,
    sha512_file,
};

/// Local periodic snapshots retain the most-recent N per offering,
/// pruned after each successful capture. Matches the seed-bank
/// retention default so both backup paths bound disk the same way —
/// see [`garden_common::nurturing::DEFAULT_RETENTION_SLOTS`].
pub(crate) const RETENTION_KEEP: usize = garden_common::nurturing::DEFAULT_RETENTION_SLOTS;

/// What [`capture_snapshot`] returns on success — the persisted
/// manifest and the event_id that records the capture in the
/// offering's event log. Callers (HTTP handlers, schedulers,
/// drag-canvas) typically surface both to the user.
#[derive(Debug, Clone)]
pub struct CapturedSnapshot {
    pub manifest: SnapshotManifest,
    pub event_id: String,
}

/// Capture a snapshot of the offering identified by `fqn` into
/// `store`, recording a `BackupTaken` event in `log`.
///
/// Side effects:
/// - Commits the running container to a transient image
///   `zen-harvest/<encoded_fqn>:<timestamp>`, `docker save`s it into
///   `<store>/<id>/image.tar`, then removes the Docker image — the
///   tarball is the durable copy; the image would otherwise leak.
/// - Writes one archive per volume under `<store>/<id>/volumes/` and
///   per external mount under `<store>/<id>/external_mounts/`, with the
///   container paused for the duration so a live process can't tear the
///   archive.
/// - Appends `BackupTaken` to `log` and truncates the log to that
///   event's id (truncate-since-snapshot retention).
///
/// Resilience contract (this wrapper):
/// - **On failure** the partial snapshot directory is removed, so an
///   aborted capture leaves no orphaned bytes on disk.
/// - **On success** the store is pruned to [`RETENTION_KEEP`] most-recent
///   snapshots.
///
/// Both post-actions are best-effort and logged; neither masks the
/// capture's own `Result`.
pub async fn capture_snapshot<S: SnapshotStore + ?Sized>(
    state: &Moss,
    fqn: &OfferingFqn,
    store: &S,
    log: &EventLog,
    actor: EventActor,
    job_id: Option<&str>,
) -> Result<CapturedSnapshot> {
    let snapshot_id = garden_common::utils::ids::generate_guidv7();
    match capture_into(state, fqn, store, log, actor, job_id, &snapshot_id).await {
        Ok(captured) => {
            // Retention: bound on-disk snapshots after a successful capture.
            match prune_snapshots(store, RETENTION_KEEP).await {
                Ok(pruned) if !pruned.is_empty() => tracing::info!(
                    offering = %fqn.fqn(),
                    pruned = pruned.len(),
                    keep = RETENTION_KEEP,
                    "Pruned old snapshots to retention limit"
                ),
                Ok(_) => {}
                Err(e) => tracing::warn!(
                    error = %e,
                    offering = %fqn.fqn(),
                    "Snapshot retention prune failed (non-fatal)"
                ),
            }
            Ok(captured)
        }
        Err(e) => {
            // Disposal: a failed capture must not leave a partial directory
            // (typically a multi-hundred-MB image.tar with no manifest).
            match store.delete(&snapshot_id).await {
                Ok(()) => tracing::info!(
                    snapshot_id = %snapshot_id,
                    offering = %fqn.fqn(),
                    "Removed partial snapshot directory after capture failure"
                ),
                Err(cleanup_err) => tracing::warn!(
                    error = %cleanup_err,
                    snapshot_id = %snapshot_id,
                    offering = %fqn.fqn(),
                    "Failed to remove partial snapshot after capture failure (non-fatal)"
                ),
            }
            Err(e)
        }
    }
}

/// Number of fixed steps before + after the variable per-volume archive
/// loop. Total = `CAPTURE_STEPS_BEFORE_VOLUMES + volume_count +
/// CAPTURE_STEPS_AFTER_VOLUMES` = `4 + N + 4` where N is the mount count.
///
/// - Before: commit container, save image, hash image, list volumes
/// - After: compute manifest digest, record BackupTaken event, save
///   manifest, truncate event log
const CAPTURE_STEPS_BEFORE_VOLUMES: u32 = 4;
const CAPTURE_STEPS_AFTER_VOLUMES: u32 = 4;

/// Inner capture flow. Produces `<store>/<snapshot_id>/` and its
/// artifacts, bailing on the first error (possibly leaving a partial
/// directory — [`capture_snapshot`] owns that cleanup). Disposes the
/// committed Docker image and unpauses the container on every exit path,
/// so a mid-capture failure never leaks those resources.
async fn capture_into<S: SnapshotStore + ?Sized>(
    state: &Moss,
    fqn: &OfferingFqn,
    store: &S,
    log: &EventLog,
    actor: EventActor,
    job_id: Option<&str>,
    snapshot_id: &str,
) -> Result<CapturedSnapshot> {
    let encoded_fqn = fqn.encoded_for_container();
    let fqn_string = fqn.fqn();
    let container_name = crate::docker::zen_offering_container_name(&fqn_string)
        .context("derive container name from FQN")?;

    tracing::info!(
        snapshot_id = %snapshot_id,
        offering = %fqn_string,
        "Starting snapshot capture"
    );

    // Steps 1-3 — capture the image. Reference-first (ORCH-0040): when the
    // running image has a registry digest, record the digest and store no
    // bytes; otherwise fall back to commit + `docker save` so the snapshot
    // is self-contained. Failing to resolve the digest is non-fatal — we
    // capture by value rather than abort the snapshot.
    state
        .jobs
        .record_step_opt(job_id, &fqn_string, 1, 0, "resolving image")
        .await;
    let repo_digest = state
        .platform
        .container
        .service_image_repo_digest(&fqn_string)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                error = %e,
                offering = %fqn_string,
                "Could not resolve image registry digest; capturing image by value"
            );
            None
        });

    let image_artifact = match repo_digest {
        Some(digest) => {
            tracing::info!(
                offering = %fqn_string,
                image = %digest,
                "Capturing image by reference (registry digest); no bytes stored"
            );
            state
                .jobs
                .record_step_opt(job_id, &fqn_string, 2, 0, "image captured by reference")
                .await;
            state
                .jobs
                .record_step_opt(job_id, &fqn_string, 3, 0, "image captured by reference")
                .await;
            SnapshotImage {
                ref_string: digest,
                transport: ImageTransport::Registry,
                size_bytes: 0,
                sha512: String::new(),
            }
        }
        None => {
            state
                .jobs
                .record_step_opt(job_id, &fqn_string, 2, 0, "committing and saving image")
                .await;
            let artifact =
                capture_image_by_value(state, store, snapshot_id, &encoded_fqn, &container_name)
                    .await?;
            state
                .jobs
                .record_step_opt(job_id, &fqn_string, 3, 0, "image saved")
                .await;
            artifact
        }
    };

    // Step 4 — list container volumes. Once the count is known, the
    // job's total_steps gets pinned to 4 + volumes.len() + 4.
    state
        .jobs
        .record_step_opt(job_id, &fqn_string, 4, 0, "listing volumes")
        .await;
    let volumes = state
        .platform
        .container
        .get_container_volumes(&fqn_string)
        .await
        .context("list container volume mounts")?;
    let managed_root = PathBuf::from(garden_common::constants::paths::volumes_dir())
        .join(&encoded_fqn);
    let volume_count = volumes.len() as u32;
    let total_steps = CAPTURE_STEPS_BEFORE_VOLUMES + volume_count + CAPTURE_STEPS_AFTER_VOLUMES;

    // Steps 5..=4+N — archive volumes with the container paused so a live
    // process (e.g. a database flushing data files) can't mutate bytes
    // mid-`tar`. Pause failure is non-fatal — we proceed, accepting a
    // possibly-less-consistent archive rather than blocking the backup.
    // Unpause runs on every path after a successful pause so the
    // container is never left stuck paused, even when archiving fails.
    let paused = match state.platform.container.pause_container(&container_name).await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(
                error = %e,
                container = %container_name,
                "Failed to pause container for volume archive; proceeding without pause"
            );
            false
        }
    };

    let archive_result = archive_volumes(
        state,
        store,
        snapshot_id,
        &fqn_string,
        volumes,
        &managed_root,
        job_id,
        total_steps,
    )
    .await;

    if paused {
        if let Err(e) = state.platform.container.unpause_container(&container_name).await {
            tracing::error!(
                error = %e,
                container = %container_name,
                "Failed to unpause container after volume archive — it may be stuck paused"
            );
        }
    }

    let (snapshot_volumes, snapshot_external_mounts) = archive_result?;

    // Step 5+N — manifest digest.
    let mut step = CAPTURE_STEPS_BEFORE_VOLUMES + volume_count;
    step += 1;
    state
        .jobs
        .record_step_opt(
            job_id,
            &fqn_string,
            step,
            total_steps,
            "computing manifest digest",
        )
        .await;
    let manifest_digest = match state.catalog.get_compiled(&fqn_string).await {
        Some(compiled) => {
            let bytes = serde_json::to_vec(&compiled)
                .context("serialize compiled offering for manifest digest")?;
            sha256_bytes(&bytes)
        }
        None => {
            tracing::warn!(
                offering = %fqn_string,
                "no compiled manifest in catalog; using empty digest"
            );
            sha256_bytes(b"")
        }
    };

    // Step 6+N — record BackupTaken event so the snapshot's
    // source_event_id is meaningful. Other instances comparing
    // their watermark to this id will know they're behind.
    step += 1;
    state
        .jobs
        .record_step_opt(
            job_id,
            &fqn_string,
            step,
            total_steps,
            "recording BackupTaken event",
        )
        .await;
    let mut details = serde_json::Map::new();
    details.insert(
        "snapshot_id".into(),
        serde_json::Value::String(snapshot_id.to_string()),
    );
    details.insert(
        "size_bytes".into(),
        serde_json::Value::Number(serde_json::Number::from(
            image_artifact.size_bytes
                + snapshot_volumes.iter().map(|v| v.size_bytes).sum::<u64>()
                + snapshot_external_mounts
                    .iter()
                    .map(|m| m.size_bytes)
                    .sum::<u64>(),
        )),
    );
    let event = new_event(log, fqn_string.clone(), EventKind::BackupTaken, actor, details)
        .await
        .context("build BackupTaken event")?;
    log.append(&event).await.context("append BackupTaken event")?;

    // Step 7+N — save manifest.
    step += 1;
    state
        .jobs
        .record_step_opt(job_id, &fqn_string, step, total_steps, "saving manifest")
        .await;
    let mut manifest = SnapshotManifest {
        id: snapshot_id.to_string(),
        source_fqn: fqn_string.clone(),
        source_stone: state.current.stone.name.clone(),
        source_event_id: event.event_id.clone(),
        created_at: event.at,
        manifest_digest,
        image: image_artifact,
        volumes: snapshot_volumes,
        external_mounts: snapshot_external_mounts,
        size_total_bytes: 0,
    };
    manifest.refresh_total_size();

    store
        .save_manifest(&manifest)
        .await
        .context("persist snapshot manifest")?;

    // Step 8+N — retention. Truncate every event before this snapshot.
    step += 1;
    state
        .jobs
        .record_step_opt(
            job_id,
            &fqn_string,
            step,
            total_steps,
            "truncating event log",
        )
        .await;
    if let Err(e) = log.truncate_before(&event.event_id).await {
        // Non-fatal: the snapshot is durable, the event is
        // recorded. A failed truncate just leaves disk debris
        // until the next successful one.
        tracing::warn!(
            error = %e,
            event_id = %event.event_id,
            "Failed to truncate event log after snapshot capture (non-fatal)"
        );
    }

    tracing::info!(
        snapshot_id = %snapshot_id,
        offering = %fqn_string,
        size = manifest.size_total_bytes,
        volumes = manifest.volumes.len(),
        external_mounts = manifest.external_mounts.len(),
        "Snapshot capture complete"
    );

    Ok(CapturedSnapshot {
        manifest,
        event_id: event.event_id,
    })
}

/// Capture the image *by value* (ORCH-0040 DockerSave fallback): commit
/// the container, `docker save` it into the store, dispose of the
/// committed Docker image, and hash the tarball. Used for images without
/// a registry digest (locally built / committed / image-direct), so the
/// snapshot is self-contained for a cross-stone plant.
///
/// Disposes the committed image on every path — including a late commit
/// failure and a save failure — so no path leaks a tagged image.
async fn capture_image_by_value<S: SnapshotStore + ?Sized>(
    state: &Moss,
    store: &S,
    snapshot_id: &str,
    encoded_fqn: &str,
    container_name: &str,
) -> Result<SnapshotImage> {
    // Derive the image ref before the commit: a late commit failure (the
    // daemon committed but the response was lost) can still leave the
    // tagged image behind, so we must be able to dispose of it on error.
    let repo = format!("zen-harvest/{encoded_fqn}");
    let tag = Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let image_ref = format!("{repo}:{tag}");

    if let Err(e) = state
        .platform
        .container
        .commit_container(container_name, &repo, &tag, true)
        .await
    {
        // remove_image treats a 404 as success, so this is safe whether or
        // not the commit produced an image.
        if let Err(rm) = state.platform.container.remove_image(&image_ref, true).await {
            tracing::warn!(
                error = %rm,
                image = %image_ref,
                "Failed to remove harvest image after commit failure (non-fatal)"
            );
        }
        return Err(e.context("commit container for snapshot"));
    }

    let image_path = store.image_path(snapshot_id);
    if let Some(parent) = image_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create snapshot image dir: {}", parent.display()))?;
    }
    let save_result = state
        .platform
        .container
        .save_image(&image_ref, &image_path)
        .await;

    // The committed image has served its purpose the moment `docker save`
    // runs: on success its bytes are in the tarball; on failure the
    // tarball is discarded by the caller's cleanup. Either way the Docker
    // image is redundant — remove it so no path leaks a tagged image.
    if let Err(e) = state.platform.container.remove_image(&image_ref, true).await {
        tracing::warn!(
            error = %e,
            image = %image_ref,
            "Failed to remove committed harvest image after save (non-fatal)"
        );
    }

    let size_bytes = save_result.context("save Docker image to snapshot tarball")?;
    let sha512 = sha512_file(&image_path)
        .await
        .context("hash snapshot image tarball")?;

    Ok(SnapshotImage {
        ref_string: image_ref,
        transport: ImageTransport::DockerSave,
        size_bytes,
        sha512,
    })
}

/// Archive every volume / external mount of a capture into the store,
/// returning the manifest entries. Split out of [`capture_into`] so the
/// caller can bracket it with container pause/unpause: any error here
/// propagates only *after* the caller unpauses, so a failed archive
/// never leaves the container stuck paused.
#[allow(clippy::too_many_arguments)]
async fn archive_volumes<S: SnapshotStore + ?Sized>(
    state: &Moss,
    store: &S,
    snapshot_id: &str,
    fqn_string: &str,
    volumes: Vec<(String, String)>,
    managed_root: &Path,
    job_id: Option<&str>,
    total_steps: u32,
) -> Result<(Vec<SnapshotVolume>, Vec<SnapshotExternalMount>)> {
    let mut snapshot_volumes: Vec<SnapshotVolume> = Vec::new();
    let mut snapshot_external_mounts: Vec<SnapshotExternalMount> = Vec::new();

    let mut step = CAPTURE_STEPS_BEFORE_VOLUMES;
    for (host_path, container_path) in volumes {
        step += 1;
        state
            .jobs
            .record_step_opt(
                job_id,
                fqn_string,
                step,
                total_steps,
                &format!("archiving {host_path}"),
            )
            .await;
        let host_path_buf = PathBuf::from(&host_path);
        let class = classify_volume(&host_path_buf, managed_root);
        let archive_dest = match class {
            VolumeClass::Managed => {
                let name = derive_volume_name(&container_path);
                store.volume_path(snapshot_id, &name)
            }
            VolumeClass::External => store.external_mount_path(snapshot_id, &host_path),
        };
        if let Some(parent) = archive_dest.parent() {
            tokio::fs::create_dir_all(parent).await.with_context(|| {
                format!("create snapshot artifact dir: {}", parent.display())
            })?;
        }
        let info = archive::create_archive(Path::new(&host_path), &archive_dest)
            .await
            .with_context(|| format!("archive volume {} for snapshot", host_path))?;
        let sha = sha512_file(&archive_dest)
            .await
            .with_context(|| format!("hash volume archive {}", archive_dest.display()))?;
        match class {
            VolumeClass::Managed => snapshot_volumes.push(SnapshotVolume {
                name: derive_volume_name(&container_path),
                container_path,
                size_bytes: info.size_bytes,
                sha512: sha,
            }),
            VolumeClass::External => snapshot_external_mounts.push(SnapshotExternalMount {
                host_path,
                container_path,
                size_bytes: info.size_bytes,
                sha512: sha,
            }),
        }
    }
    Ok((snapshot_volumes, snapshot_external_mounts))
}

/// Compute the total step count for a capture given the volume mount
/// count. Public so the API handler can pre-set total_steps when it
/// knows the count, and the integration tests can verify the math.
pub fn capture_total_steps(volume_count: u32) -> u32 {
    CAPTURE_STEPS_BEFORE_VOLUMES + volume_count + CAPTURE_STEPS_AFTER_VOLUMES
}

/// Derive a volume's display name from its container path. Mirrors
/// the existing harvest convention so a snapshot's volume names
/// match what `OsHarvestOps::create_harvest` produces for the
/// same offering.
fn derive_volume_name(container_path: &str) -> String {
    Path::new(container_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "data".to_string())
}

/// Outcome of a [`reconcile_all_snapshots`] pass, for logging.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReconcileReport {
    /// Per-offering subdirectories examined under the root.
    pub offerings_seen: usize,
    /// Orphaned (manifest-less) directories removed across all offerings.
    pub orphans_reaped: usize,
    /// Complete snapshots pruned to honour the retention limit.
    pub snapshots_pruned: usize,
}

/// Reconcile every offering's local snapshot store under `root`: reap
/// orphaned (aborted-capture) directories, then prune each offering to
/// its `keep` most-recent complete snapshots.
///
/// Filesystem-driven — it discovers offerings from the directory layout
/// (`<root>/<encoded_offering>/<snapshot_id>/`), so it corrects debris
/// even for offerings not currently in the registry. Invoked once at
/// startup, before the periodic loop begins capturing, to self-heal
/// accumulated state (e.g. the orphaned `image.tar`s a pre-retention
/// build left behind).
///
/// Resilient by design: a failure for one offering is logged and
/// skipped so it never blocks the rest of the sweep.
pub async fn reconcile_all_snapshots(root: &Path, keep: usize) -> Result<ReconcileReport> {
    let mut report = ReconcileReport::default();
    let mut entries = match tokio::fs::read_dir(root).await {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(e) => {
            return Err(anyhow::Error::from(e)
                .context(format!("read snapshots root: {}", root.display())));
        }
    };
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        report.offerings_seen += 1;
        let offering_dir = entry.file_name();
        let store = LocalSnapshotStore::new(entry.path());

        // Reap aborted captures first so retention counts only complete
        // snapshots.
        match store.reap_orphans().await {
            Ok(reaped) => report.orphans_reaped += reaped.len(),
            Err(e) => tracing::warn!(
                error = %e,
                offering = %offering_dir.to_string_lossy(),
                "snapshot reconcile: reaping orphans failed (skipping offering)"
            ),
        }
        match prune_snapshots(&store, keep).await {
            Ok(pruned) => report.snapshots_pruned += pruned.len(),
            Err(e) => tracing::warn!(
                error = %e,
                offering = %offering_dir.to_string_lossy(),
                "snapshot reconcile: retention prune failed (skipping offering)"
            ),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    //! Unit-level coverage for the orchestrator's pure helpers.
    //! The full capture flow requires Docker (image commit, save,
    //! container volume listing) and is covered by integration
    //! tests in tests/snapshot_integration.rs once that file is
    //! added — the existing harvest precedent.

    use super::*;
    use tempfile::TempDir;

    #[test]
    fn derive_volume_name_uses_basename_of_container_path() {
        assert_eq!(derive_volume_name("/data/db"), "db");
        assert_eq!(derive_volume_name("/var/log/postgres"), "postgres");
    }

    #[test]
    fn derive_volume_name_falls_back_to_data_for_pathological_inputs() {
        // Empty container path or one ending in a separator
        // can yield no basename; we fall back to "data" so the
        // snapshot manifest still has a usable name field.
        assert_eq!(derive_volume_name(""), "data");
        // A path with only a trailing separator has no basename.
        assert_eq!(derive_volume_name("/"), "data");
    }

    #[test]
    fn capture_total_steps_for_singleton_offering() {
        // mongodb with one data volume + no external mounts:
        // 4 (commit, save, hash, list) + 1 (archive volume) +
        // 4 (manifest digest, BackupTaken event, save manifest,
        // truncate log) = 9 steps.
        assert_eq!(capture_total_steps(1), 9);
    }

    #[test]
    fn capture_total_steps_for_zero_volumes_offering() {
        // Stateless offering with no mounts (uncommon but possible):
        // 4 + 0 + 4 = 8 steps. Still positive so the seed-chip
        // fills smoothly even in this degenerate case.
        assert_eq!(capture_total_steps(0), 8);
    }

    #[test]
    fn capture_total_steps_scales_linearly_with_volume_count() {
        // 3 volumes + 2 external mounts (a richly-mounted offering):
        // 4 + 5 + 4 = 13 steps.
        assert_eq!(capture_total_steps(5), 13);
        assert_eq!(capture_total_steps(10), 18);
    }

    /// Create a snapshot directory under `<root>/<offering>/<id>`.
    /// With a manifest it counts as a complete snapshot; without, it's
    /// an orphan (aborted capture) carrying a stand-in `image.tar`.
    async fn touch_snapshot(root: &Path, offering: &str, id: &str, complete: bool) {
        let dir = root.join(offering).join(id);
        tokio::fs::create_dir_all(&dir).await.unwrap();
        if complete {
            tokio::fs::write(dir.join("manifest.json"), b"{}").await.unwrap();
        } else {
            tokio::fs::write(dir.join("image.tar"), b"partial").await.unwrap();
        }
    }

    #[tokio::test]
    async fn reconcile_all_snapshots_reaps_orphans_and_prunes_per_offering() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // mongodb: 7 complete snapshots + 2 aborted captures (orphans).
        for id in ["01-a", "01-b", "01-c", "01-d", "01-e", "01-f", "01-g"] {
            touch_snapshot(root, "mongodb", id, true).await;
        }
        touch_snapshot(root, "mongodb", "01-orphan1", false).await;
        touch_snapshot(root, "mongodb", "01-orphan2", false).await;

        // searxng: 3 complete snapshots, within retention, no orphans.
        for id in ["01-a", "01-b", "01-c"] {
            touch_snapshot(root, "searxng", id, true).await;
        }

        let report = reconcile_all_snapshots(root, 5).await.unwrap();
        assert_eq!(report.offerings_seen, 2);
        assert_eq!(report.orphans_reaped, 2);
        assert_eq!(report.snapshots_pruned, 2, "mongodb 7 → keep 5 = prune 2");

        // mongodb: orphans gone; the 5 most-recent complete snapshots remain.
        let mongo = LocalSnapshotStore::new(root.join("mongodb"));
        assert_eq!(
            mongo.list_ids().await.unwrap(),
            vec!["01-c", "01-d", "01-e", "01-f", "01-g"]
        );
        assert!(!root.join("mongodb").join("01-orphan1").exists());
        assert!(!root.join("mongodb").join("01-orphan2").exists());

        // searxng: untouched (within retention, no orphans).
        let searx = LocalSnapshotStore::new(root.join("searxng"));
        assert_eq!(searx.list_ids().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn reconcile_all_snapshots_returns_default_when_root_missing() {
        let tmp = TempDir::new().unwrap();
        let report = reconcile_all_snapshots(&tmp.path().join("nope"), 5)
            .await
            .unwrap();
        assert_eq!(report, ReconcileReport::default());
    }
}

