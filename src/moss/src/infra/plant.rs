//! Plant from snapshot — applies a captured [`SnapshotManifest`]
//! to the local stone, recreating the offering's container with
//! the captured image, volumes, and external mounts.
//!
//! ORCH-0039 §"Plant" specifies the flow:
//! 1. Resolve the snapshot (local or, in commit P2, cross-stone)
//! 2. Validate the manifest digest against the offering's
//!    current compiled manifest. Drift warns; doesn't refuse.
//! 3. Stop + remove existing container if any (preserve volumes
//!    so the staging restore can swap them atomically).
//! 4. Load the captured image into the local Docker daemon.
//! 5. Restore volumes + external mounts via
//!    [`apply_volumes_with_staging`] (atomic per-volume, with
//!    rollback on failure).
//! 6. Recreate the container from the offering's compiled
//!    manifest using the existing
//!    [`ContainerRuntime::install_service`] path.
//! 7. Wait for health via [`Health::wait_until_healthy`].
//! 8. Append `RestoreApplied` to the per-offering event log.
//!
//! Local-snapshot only in M2. Cross-stone fetch (commit P2)
//! adds a download step ahead of (3) that materialises a remote
//! snapshot into a local staging store; the rest of the flow
//! is identical.
//!
//! [ORCH-0039]: ../../../../docs/decisions/ORCH-0039-seed-based-offering-replication.md

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use garden_common::offerings::OfferingFqn;

use crate::Moss;
use crate::docker::ContainerSpec;
use crate::domain::offering_events::{EventActor, EventKind, EventLog, new_event};
use crate::domain::snapshot::{ImageTransport, SnapshotManifest, SnapshotStore, sha256_bytes};
use crate::infra::cross_stone::{self, CrossStoneError};
use crate::infra::harvest::{VolumeRestorePlan, apply_volumes_with_staging};

/// Outcome of [`plant_from_local_snapshot`] — surfaced to the
/// HTTP handler so it can echo the relevant ids back to the
/// client.
#[derive(Debug, Clone)]
pub struct PlantedSnapshot {
    pub snapshot_id: String,
    pub event_id: String,
    pub source_fqn: String,
    pub target_fqn: String,
    pub digest_drift: DigestDrift,
}

/// Compares the snapshot's `manifest_digest` to the target
/// offering's current compiled-manifest digest. M2 default
/// policy: warn on drift, proceed with restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestDrift {
    /// Both digests match — manifest hasn't changed since
    /// capture.
    Match,
    /// Digests differ. Caller logged a warning; the restore
    /// proceeds because the user asked for it.
    Drift,
    /// Target offering has no compiled manifest in the catalog
    /// (e.g. Adopted offerings). Drift cannot be evaluated; the
    /// restore proceeds by default.
    Unknown,
}

/// Total step count for `plant_from_local_snapshot`. Fixed (no
/// variable per-volume axis at this level — volume restoration
/// happens in a single bulk `apply_volumes_with_staging` call).
///
/// Steps:
/// 1. Loading snapshot manifest (+ digest drift + compiled lookup)
/// 2. Stopping existing container (no-op-but-counted when absent)
/// 3. Loading image into Docker
/// 4. Restoring volumes
/// 5. Recreating container
/// 6. Waiting for health
/// 7. Recording RestoreApplied event
pub const PLANT_TOTAL_STEPS: u32 = 7;

/// Plant a snapshot already present in `store` onto this stone
/// as `target_fqn`. The snapshot's `source_fqn` may differ from
/// `target_fqn` — that's the "fork" case where the user is
/// deriving a new instance from existing seeded data.
pub async fn plant_from_local_snapshot<S: SnapshotStore + ?Sized>(
    state: &Moss,
    target_fqn: &OfferingFqn,
    store: &S,
    snapshot_id: &str,
    log: &EventLog,
    actor: EventActor,
    job_id: Option<&str>,
) -> Result<PlantedSnapshot> {
    let target_name = target_fqn.fqn();

    // Step 1 — load manifest + drift check + compiled lookup.
    state
        .jobs
        .record_step_opt(
            job_id,
            &target_name,
            1,
            PLANT_TOTAL_STEPS,
            "loading snapshot manifest",
        )
        .await;
    let manifest = store
        .load_manifest(snapshot_id)
        .await
        .with_context(|| format!("load snapshot manifest {snapshot_id}"))?;

    tracing::info!(
        snapshot_id,
        source_fqn = %manifest.source_fqn,
        target_fqn = %target_fqn.fqn(),
        "Starting plant from local snapshot"
    );

    let digest_drift = check_digest_drift(state, target_fqn, &manifest).await;
    if digest_drift == DigestDrift::Drift {
        tracing::warn!(
            snapshot_id,
            target_fqn = %target_fqn.fqn(),
            "Manifest digest drift between snapshot capture and current target offering — proceeding with restore"
        );
    }

    let compiled = state
        .catalog
        .get_compiled(&target_fqn.fqn())
        .await
        .ok_or_else(|| {
            anyhow::anyhow!(
                "target offering '{}' has no compiled manifest in catalog — plant requires a known offering shape",
                target_fqn.fqn()
            )
        })?;

    // Step 2 — stop + remove existing container if present. Counted
    // even when no container exists, so the seed-chip's progress is
    // monotonic regardless of whether this is a first-time plant
    // (no existing container) or a restore-in-place.
    state
        .jobs
        .record_step_opt(
            job_id,
            &target_name,
            2,
            PLANT_TOTAL_STEPS,
            "stopping existing container",
        )
        .await;
    if state
        .platform
        .container
        .zen_container_exists(&target_name)
        .await
        .unwrap_or(false)
    {
        // stop is idempotent (no-op when already stopped); the
        // remove uses v: false so volumes survive — they'll be
        // swapped atomically below.
        let _ = state
            .platform
            .container
            .stop_service(&target_name, Some(&state.console))
            .await;
        state
            .platform
            .container
            .remove_service(&target_name, Some(&state.console))
            .await
            .context("remove existing container before plant")?;
    }

    // Step 3 — make the image available. Reference-first snapshots
    // (ORCH-0040) carry no bytes: the container is recreated from the
    // offering manifest's image at step 5, so there is nothing to load.
    // DockerSave snapshots carry a self-contained tarball we load here.
    state
        .jobs
        .record_step_opt(
            job_id,
            &target_name,
            3,
            PLANT_TOTAL_STEPS,
            "loading image into Docker",
        )
        .await;
    match manifest.image.transport {
        ImageTransport::DockerSave => {
            let image_path = store.image_path(snapshot_id);
            state
                .platform
                .container
                .load_image(&image_path)
                .await
                .with_context(|| format!("load snapshot image {}", image_path.display()))?;
        }
        ImageTransport::Registry => {
            tracing::debug!(
                target_fqn = %target_name,
                image = %manifest.image.ref_string,
                "Snapshot image captured by reference; recreating from offering manifest image"
            );
        }
    }

    // Step 4 — restore volumes via the staging path.
    state
        .jobs
        .record_step_opt(
            job_id,
            &target_name,
            4,
            PLANT_TOTAL_STEPS,
            "restoring volumes",
        )
        .await;
    let plans = build_volume_restore_plans(&manifest, &compiled.volumes, store, snapshot_id);

    if !plans.is_empty() {
        apply_volumes_with_staging(&plans, snapshot_id)
            .await
            .context("apply volumes with staging during plant")?;
    }

    // Step 5 — recreate container.
    state
        .jobs
        .record_step_opt(
            job_id,
            &target_name,
            5,
            PLANT_TOTAL_STEPS,
            "recreating container",
        )
        .await;
    let spec = compiled_to_container_spec(&compiled);
    state
        .platform
        .container
        .install_service(&target_name, &spec, Some(&state.console))
        .await
        .context("install container after plant")?;

    // Step 6 — wait for health.
    state
        .jobs
        .record_step_opt(
            job_id,
            &target_name,
            6,
            PLANT_TOTAL_STEPS,
            "waiting for container health",
        )
        .await;
    let healthy = state
        .health
        .wait_until_healthy(&target_name, Duration::from_secs(120))
        .await;
    if !healthy {
        // Soft error — the plant did what it could; the
        // operator may want to inspect. We still record the
        // event so the chain is honest about what was applied.
        tracing::warn!(
            target_fqn = %target_name,
            "Container did not reach healthy state within 120 s after plant"
        );
    }

    // Step 7 — append RestoreApplied event so peers (when M3 lands)
    // can detect this instance jumped forward.
    state
        .jobs
        .record_step_opt(
            job_id,
            &target_name,
            7,
            PLANT_TOTAL_STEPS,
            "recording RestoreApplied event",
        )
        .await;
    let mut details = serde_json::Map::new();
    details.insert(
        "from_snapshot_id".into(),
        serde_json::Value::String(snapshot_id.to_string()),
    );
    details.insert(
        "source_fqn".into(),
        serde_json::Value::String(manifest.source_fqn.clone()),
    );
    details.insert(
        "digest_drift".into(),
        serde_json::Value::String(format!("{digest_drift:?}").to_lowercase()),
    );
    details.insert(
        "healthy_after_plant".into(),
        serde_json::Value::Bool(healthy),
    );
    let event = new_event(
        log,
        target_fqn.fqn(),
        EventKind::RestoreApplied,
        actor,
        details,
    )
    .await
    .context("build RestoreApplied event")?;
    log.append(&event)
        .await
        .context("append RestoreApplied event")?;

    Ok(PlantedSnapshot {
        snapshot_id: snapshot_id.to_string(),
        event_id: event.event_id,
        source_fqn: manifest.source_fqn,
        target_fqn: target_fqn.fqn(),
        digest_drift,
    })
}

/// Fetch every artifact of a snapshot from a remote stone and
/// write them into the target [`SnapshotStore`] at the same
/// paths the local capture would produce. Used by P2 (cross-
/// stone plant): the caller resolves the source stone +
/// source FQN, constructs a local staging store, and calls
/// this to materialise the snapshot before invoking
/// [`plant_from_local_snapshot`].
///
/// `source_fqn` is required because the snapshot lives in a
/// per-FQN catalog on the source stone; the source-side URL
/// includes it as a path segment. In practice the caller
/// always knows the source FQN — it's the FQN the user clicked
/// in the catalog or dragged on the canvas.
///
/// Sequential fetch: manifest first, then image, then volumes,
/// then external mounts. M2 ships the simpler shape; future
/// commits may parallelise per-file fetches for multi-GB
/// snapshots over fast LANs.
pub async fn fetch_snapshot_from_stone<S: SnapshotStore + ?Sized>(
    state: &Moss,
    source_stone: &str,
    source_fqn: &OfferingFqn,
    snapshot_id: &str,
    store: &S,
) -> Result<SnapshotManifest> {
    let endpoint = cross_stone::resolve_stone_endpoint(state, source_stone)
        .await
        .ok_or_else(|| {
            anyhow::anyhow!(
                "source stone '{source_stone}' not in topology cache; can't fetch snapshot"
            )
        })?;
    let client = streaming_client()?;

    let manifest_path = format!(
        "/api/v1/stone/offerings/{}/snapshots/{}",
        urlencoding::encode(&source_fqn.fqn()),
        urlencoding::encode(snapshot_id)
    );
    let manifest: SnapshotManifest = cross_stone::fetch_from_stone(
        &client,
        &endpoint,
        source_stone,
        &manifest_path,
    )
    .await
    .map_err(|e: CrossStoneError| anyhow::anyhow!(e.to_string()))?;

    // Persist the manifest first so a partial download leaves a
    // visible (but incomplete) snapshot the user can clean up
    // explicitly. Subsequent steps fill in the artifacts.
    store
        .save_manifest(&manifest)
        .await
        .context("save fetched snapshot manifest")?;

    // Image — only DockerSave snapshots carry an image.tar. Registry
    // snapshots (ORCH-0040) store no bytes; the source stone never wrote
    // image.tar, so fetching it would 404. The image is reproduced from
    // the offering manifest at plant time.
    if manifest.image.transport == ImageTransport::DockerSave {
        fetch_artifact(
            &client,
            &endpoint,
            source_stone,
            source_fqn,
            snapshot_id,
            "image",
            "image.tar",
            &store.image_path(snapshot_id),
        )
        .await
        .context("fetch snapshot image")?;
    }

    // Volumes.
    for vol in &manifest.volumes {
        fetch_artifact(
            &client,
            &endpoint,
            source_stone,
            source_fqn,
            snapshot_id,
            "volume",
            &vol.name,
            &store.volume_path(snapshot_id, &vol.name),
        )
        .await
        .with_context(|| format!("fetch snapshot volume {}", vol.name))?;
    }

    // External mounts. The `kind=external_mount` endpoint
    // expects the *encoded* host path; the local store
    // computes the same encoding when it constructs
    // `external_mount_path`, so the round-trip is consistent.
    for em in &manifest.external_mounts {
        let encoded =
            crate::domain::snapshot::LocalSnapshotStore::encoded_external_mount_for(&em.host_path);
        fetch_artifact(
            &client,
            &endpoint,
            source_stone,
            source_fqn,
            snapshot_id,
            "external_mount",
            &encoded,
            &store.external_mount_path(snapshot_id, &em.host_path),
        )
        .await
        .with_context(|| format!("fetch snapshot external mount {}", em.host_path))?;
    }

    Ok(manifest)
}

/// Stream one artifact from the source stone to a local file.
async fn fetch_artifact(
    client: &reqwest::Client,
    endpoint: &str,
    source_stone: &str,
    source_fqn: &OfferingFqn,
    snapshot_id: &str,
    kind: &str,
    artifact: &str,
    dest: &std::path::Path,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create snapshot artifact dir: {}", parent.display()))?;
    }

    let path = format!(
        "/api/v1/stone/offerings/{}/snapshots/{}/files/{}/{}",
        urlencoding::encode(&source_fqn.fqn()),
        urlencoding::encode(snapshot_id),
        urlencoding::encode(kind),
        urlencoding::encode(artifact),
    );
    let response =
        cross_stone::stream_from_stone(client, endpoint, source_stone, &path)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let mut file = tokio::fs::File::create(dest)
        .await
        .with_context(|| format!("create artifact dest: {}", dest.display()))?;
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("stream chunk for {kind}/{artifact}"))?;
        file.write_all(&chunk)
            .await
            .with_context(|| format!("write artifact chunk: {}", dest.display()))?;
    }
    file.flush()
        .await
        .with_context(|| format!("flush artifact file: {}", dest.display()))?;
    Ok(())
}

/// HTTP client for cross-stone artifact streaming. No overall
/// timeout (snapshot files are large and may take many minutes
/// over slow LANs); per-read timeouts on the underlying TCP
/// stream are enforced by the OS. Pond mTLS upgrade is via
/// `StoneClient` in the future — M2 uses plain HTTP plus
/// trust-the-LAN, matching `mirror_capabilities`.
fn streaming_client() -> Result<reqwest::Client> {
    crate::http::client_builder()
        .build()
        .context("build snapshot streaming HTTP client")
}

/// Pure builder: map a snapshot's volume + external-mount lists
/// to filesystem-rooted [`VolumeRestorePlan`]s.
///
/// - Each snapshot volume needs the compiled manifest's
///   `(host_path, container_path)` table to resolve where its
///   data should land. A volume in the snapshot whose
///   container_path doesn't appear in the compiled volumes is
///   skipped with a warning — the offering shape changed
///   between capture and plant such that this mount is gone.
/// - Each snapshot external mount restores to its captured
///   `host_path` directly (the snapshot manifest is the
///   authoritative source for external-mount placement —
///   restoring elsewhere would defeat the entire purpose).
pub fn build_volume_restore_plans<S: SnapshotStore + ?Sized>(
    manifest: &SnapshotManifest,
    compiled_volumes: &[(String, String)],
    store: &S,
    snapshot_id: &str,
) -> Vec<VolumeRestorePlan> {
    let mut plans = Vec::with_capacity(manifest.volumes.len() + manifest.external_mounts.len());

    for vol in &manifest.volumes {
        let live = compiled_volumes
            .iter()
            .find(|(_, cp)| *cp == vol.container_path)
            .map(|(host, _)| host.clone());
        let Some(live_host) = live else {
            tracing::warn!(
                volume = %vol.name,
                container_path = %vol.container_path,
                "Snapshot volume not present in target offering's compiled volumes — skipping"
            );
            continue;
        };
        plans.push(VolumeRestorePlan {
            name: vol.name.clone(),
            archive_path: store.volume_path(snapshot_id, &vol.name),
            live_path: PathBuf::from(live_host),
        });
    }

    for em in &manifest.external_mounts {
        plans.push(VolumeRestorePlan {
            name: format!("ext:{}", em.host_path),
            archive_path: store.external_mount_path(snapshot_id, &em.host_path),
            live_path: PathBuf::from(&em.host_path),
        });
    }

    plans
}

/// Compare the snapshot's recorded manifest digest to the
/// target offering's current compiled-manifest digest.
async fn check_digest_drift(
    state: &Moss,
    target_fqn: &OfferingFqn,
    snapshot: &SnapshotManifest,
) -> DigestDrift {
    let Some(compiled) = state.catalog.get_compiled(&target_fqn.fqn()).await else {
        return DigestDrift::Unknown;
    };
    let current_digest = match serde_json::to_vec(&compiled) {
        Ok(bytes) => sha256_bytes(&bytes),
        Err(_) => return DigestDrift::Unknown,
    };
    if current_digest == snapshot.manifest_digest {
        DigestDrift::Match
    } else {
        DigestDrift::Drift
    }
}

/// Build a [`ContainerSpec`] from a [`CompiledOffering`]. Mirrors
/// the field-by-field copy that the existing nourish + install
/// flows do at their own boundaries; centralised here so plant
/// doesn't need to depend on the (image-direct) resolver path.
fn compiled_to_container_spec(
    compiled: &crate::domain::catalog::entry::CompiledOffering,
) -> ContainerSpec {
    // Ports: ContainerSpec expects `Vec<(host, container)>`;
    // CompiledOffering keys ports by name. We only carry the
    // tuples — `install_service` resolves names internally
    // when remediating port conflicts.
    let mut ports: Vec<(u16, u16)> = compiled.ports.values().copied().collect();
    // Stable order so the diff against running containers is
    // deterministic — install_service uses the order to drive
    // port-conflict resolution and we want reproducible
    // remappings.
    ports.sort_by_key(|p| p.1);

    ContainerSpec {
        image: compiled.image.clone(),
        command: compiled.command.clone(),
        ports,
        environment: compiled.environment.clone(),
        volumes: compiled.volumes.clone(),
        config_files: compiled.config_files.clone(),
        device_requests: compiled.device_requests.clone(),
        memory_bytes: compiled.resource_limits.memory_bytes,
        nano_cpus: compiled.resource_limits.nano_cpus,
        healthcheck: compiled.healthcheck.clone(),
    }
}

#[cfg(test)]
mod tests {
    //! Coverage for the pure helpers. Plant's full Docker-
    //! dependent orchestration is exercised in
    //! tests/plant_integration.rs (added when the harvest
    //! integration suite lands).
    use super::*;
    use crate::domain::snapshot::{
        ImageTransport, LocalSnapshotStore, SnapshotExternalMount, SnapshotImage, SnapshotVolume,
    };
    use chrono::Utc;
    use tempfile::TempDir;

    fn make_manifest() -> SnapshotManifest {
        SnapshotManifest {
            id: "snap-1".into(),
            source_fqn: "mongodb::prd".into(),
            source_stone: "stone-alpha".into(),
            source_event_id: "evt-1".into(),
            created_at: Utc::now(),
            manifest_digest: "deadbeef".into(),
            image: SnapshotImage {
                ref_string: "zen-harvest/mongodb--prd:t1".into(),
                transport: ImageTransport::DockerSave,
                size_bytes: 100,
                sha512: "img".into(),
            },
            volumes: vec![SnapshotVolume {
                name: "data".into(),
                container_path: "/data/db".into(),
                size_bytes: 50,
                sha512: "v".into(),
            }],
            external_mounts: vec![SnapshotExternalMount {
                host_path: "/var/data/photos".into(),
                container_path: "/photos".into(),
                size_bytes: 200,
                sha512: "em".into(),
            }],
            size_total_bytes: 350,
        }
    }

    #[test]
    fn build_plans_maps_snapshot_volumes_to_compiled_host_paths() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().to_path_buf());
        let manifest = make_manifest();
        // Compiled volumes table: container_path → host_path.
        let compiled = vec![
            ("/var/lib/zen-garden/volumes/mongodb--prd/db".into(), "/data/db".into()),
        ];
        let plans = build_volume_restore_plans(&manifest, &compiled, &store, "snap-1");

        // 1 volume + 1 external mount = 2 plans.
        assert_eq!(plans.len(), 2);
        // Volume goes to the compiled host_path.
        assert_eq!(plans[0].name, "data");
        assert_eq!(
            plans[0].live_path,
            PathBuf::from("/var/lib/zen-garden/volumes/mongodb--prd/db")
        );
        // External mount goes to the captured host_path.
        assert_eq!(plans[1].name, "ext:/var/data/photos");
        assert_eq!(plans[1].live_path, PathBuf::from("/var/data/photos"));
    }

    #[test]
    fn build_plans_skips_volume_when_compiled_no_longer_declares_it() {
        // Snapshot has a volume at container_path /data/db, but
        // the target offering's manifest has changed and no
        // longer mentions /data/db. The plan-builder must skip
        // (with a warning logged elsewhere) rather than try to
        // restore into a guessed path.
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().to_path_buf());
        let manifest = make_manifest();
        let compiled = vec![
            // Different container_path — doesn't match snapshot.
            ("/var/x/foo".into(), "/different/path".into()),
        ];
        let plans = build_volume_restore_plans(&manifest, &compiled, &store, "snap-1");
        // Volume skipped, external mount kept.
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].name, "ext:/var/data/photos");
    }

    #[test]
    fn build_plans_with_no_volumes_or_mounts_returns_empty() {
        let dir = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(dir.path().to_path_buf());
        let mut manifest = make_manifest();
        manifest.volumes.clear();
        manifest.external_mounts.clear();
        let plans = build_volume_restore_plans(&manifest, &[], &store, "snap-1");
        assert!(plans.is_empty());
    }

    // `compiled_to_container_spec` is a 5-line field copy. The
    // CompiledOffering type isn't Default and constructing one
    // by hand here would couple this test to the catalog
    // schema's evolution. Coverage lives at the integration
    // level instead — the plant flow round-trips through it.

    #[test]
    fn plant_total_steps_is_seven() {
        // Pin the constant so accidentally adding/removing a step
        // shows up as a test failure. If you change PLANT_TOTAL_STEPS,
        // update this assertion AND the per-step record_step_opt
        // calls in plant_from_local_snapshot AND any consumer that
        // pre-allocates step capacity (e.g. Pavilion's seed-chip
        // computing percent = step / total).
        assert_eq!(PLANT_TOTAL_STEPS, 7);
    }
}
