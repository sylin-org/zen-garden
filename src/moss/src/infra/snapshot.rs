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
pub async fn capture_snapshot<S: SnapshotStore + ?Sized>(
    state: &Moss,
    fqn: &OfferingFqn,
    store: &S,
    log: &EventLog,
    actor: EventActor,
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

    // 1 + 2 — commit container, save image to <store>/<id>/image.tar.
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
    let image_sha = sha512_file(&image_path)
        .await
        .context("hash snapshot image tarball")?;

    // 3 + 4 — pack volumes and external mounts; classify each by
    // path. The managed-volumes root for this offering is
    // `<volumes_dir>/<encoded_fqn>`; anything else is external.
    let volumes = state
        .platform
        .container
        .get_container_volumes(&fqn_string)
        .await
        .context("list container volume mounts")?;
    let managed_root = PathBuf::from(garden_common::constants::paths::volumes_dir())
        .join(&encoded_fqn);

    let mut snapshot_volumes: Vec<SnapshotVolume> = Vec::new();
    let mut snapshot_external_mounts: Vec<SnapshotExternalMount> = Vec::new();

    for (host_path, container_path) in volumes {
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

    // 4b — manifest digest. We hash the canonical JSON form of
    // the offering's compiled manifest at capture time so
    // restore can detect drift. Best-effort: if the catalog
    // doesn't have a compiled manifest cached (e.g. an Adopted
    // offering), record the digest of the empty payload — the
    // restore-side check just warns either way.
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

    // 5 — append BackupTaken event so the snapshot's
    // source_event_id is meaningful. Other instances comparing
    // their watermark to this id will know they're behind.
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

    // 6 — assemble + save manifest.
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

    // 7 — retention. Truncate every event before this snapshot.
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
}

