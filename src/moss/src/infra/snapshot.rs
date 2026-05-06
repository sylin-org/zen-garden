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
//! Per ORCH-0039 §"Seed metadata schema":
//! 1. Commit the running container to a new image tag
//! 2. `docker save` the image to `<store>/<id>/image.tar`
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
    ImageTransport, SnapshotExternalMount, SnapshotImage, SnapshotManifest, SnapshotStore,
    SnapshotVolume, VolumeClass, classify_volume, sha256_bytes, sha512_file,
};

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
/// - Commits the running container to a new image tag
///   `zen-harvest/<encoded_fqn>:<timestamp>` (the same naming
///   convention as the existing harvest)
/// - Writes `<store>/<id>/image.tar`, one volume per file under
///   `<store>/<id>/volumes/`, and one external-mount archive
///   per file under `<store>/<id>/external_mounts/`
/// - Appends `BackupTaken` to `log` and truncates the log to
///   that event's id (truncate-since-snapshot retention)
///
/// On failure between steps the partial state is left for a
/// gardener to clean up (or for the next capture to overwrite
/// via the snapshot id namespace). The function does not
/// implement transactional rollback today.
/// Number of fixed steps before + after the variable per-volume
/// archive loop in `capture_snapshot`. The total step count for a
/// capture is `FIXED_STEPS_BEFORE + volume_count + FIXED_STEPS_AFTER`
/// = `4 + N + 4` where N is the number of volume mounts.
///
/// - Before: commit container, save image, hash image, list volumes
/// - After: compute manifest digest, record BackupTaken event,
///   save manifest, truncate event log
const CAPTURE_STEPS_BEFORE_VOLUMES: u32 = 4;
const CAPTURE_STEPS_AFTER_VOLUMES: u32 = 4;

pub async fn capture_snapshot<S: SnapshotStore + ?Sized>(
    state: &Moss,
    fqn: &OfferingFqn,
    store: &S,
    log: &EventLog,
    actor: EventActor,
    job_id: Option<&str>,
) -> Result<CapturedSnapshot> {
    let snapshot_id = garden_common::utils::ids::generate_guidv7();
    let encoded_fqn = fqn.encoded_for_container();
    let fqn_string = fqn.fqn();
    let container_name = crate::docker::zen_offering_container_name(&fqn_string)
        .context("derive container name from FQN")?;

    tracing::info!(
        snapshot_id = %snapshot_id,
        offering = %fqn_string,
        "Starting snapshot capture"
    );

    // Step 1 — commit container.
    state
        .jobs
        .record_step_opt(job_id, &fqn_string, 1, 0, "committing container")
        .await;
    let original_image = state
        .platform
        .container
        .get_service_image(&fqn_string)
        .await
        .context("get current container image")?;
    let repo = format!("zen-harvest/{}", encoded_fqn);
    let tag = Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let _committed_id = state
        .platform
        .container
        .commit_container(&container_name, &repo, &tag, true)
        .await
        .context("commit container for snapshot")?;
    let image_ref = format!("{}:{}", repo, tag);

    // Step 2 — save image to tar.
    state
        .jobs
        .record_step_opt(job_id, &fqn_string, 2, 0, "saving image to tar")
        .await;
    let image_path = store.image_path(&snapshot_id);
    if let Some(parent) = image_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create snapshot image dir: {}", parent.display()))?;
    }
    let image_size = state
        .platform
        .container
        .save_image(&image_ref, &image_path)
        .await
        .context("save Docker image to snapshot tarball")?;

    // Step 3 — hash image tarball.
    state
        .jobs
        .record_step_opt(job_id, &fqn_string, 3, 0, "hashing image tarball")
        .await;
    let image_sha = sha512_file(&image_path)
        .await
        .context("hash snapshot image tarball")?;

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
    let total_steps =
        CAPTURE_STEPS_BEFORE_VOLUMES + volumes.len() as u32 + CAPTURE_STEPS_AFTER_VOLUMES;

    let mut snapshot_volumes: Vec<SnapshotVolume> = Vec::new();
    let mut snapshot_external_mounts: Vec<SnapshotExternalMount> = Vec::new();

    // Steps 5..=4+N — archive + hash each volume / external mount.
    let mut step = CAPTURE_STEPS_BEFORE_VOLUMES;
    for (host_path, container_path) in volumes {
        step += 1;
        state
            .jobs
            .record_step_opt(
                job_id,
                &fqn_string,
                step,
                total_steps,
                &format!("archiving {host_path}"),
            )
            .await;
        let host_path_buf = PathBuf::from(&host_path);
        let class = classify_volume(&host_path_buf, &managed_root);
        let archive_dest = match class {
            VolumeClass::Managed => {
                let name = derive_volume_name(&container_path);
                store.volume_path(&snapshot_id, &name)
            }
            VolumeClass::External => store.external_mount_path(&snapshot_id, &host_path),
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

    // Step 5+N — manifest digest.
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
        serde_json::Value::String(snapshot_id.clone()),
    );
    details.insert(
        "size_bytes".into(),
        serde_json::Value::Number(serde_json::Number::from(
            image_size
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
        id: snapshot_id.clone(),
        source_fqn: fqn_string.clone(),
        source_stone: state.current.stone.name.clone(),
        source_event_id: event.event_id.clone(),
        created_at: event.at,
        manifest_digest,
        image: SnapshotImage {
            ref_string: image_ref,
            transport: ImageTransport::DockerSave,
            size_bytes: image_size,
            sha512: image_sha,
        },
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

    let _ = original_image; // diagnostic only — captured above for future use
    Ok(CapturedSnapshot {
        manifest,
        event_id: event.event_id,
    })
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

#[cfg(test)]
mod tests {
    //! Unit-level coverage for the orchestrator's pure helpers.
    //! The full capture flow requires Docker (image commit, save,
    //! container volume listing) and is covered by integration
    //! tests in tests/snapshot_integration.rs once that file is
    //! added — the existing harvest precedent.

    use super::*;

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
}

