//! Docker-required integration tests for snapshot capture + plant.
//!
//! These tests exercise the real ContainerRuntime against a live
//! Docker daemon. They:
//!
//! 1. Pull `alpine:latest` (small, pre-cached on most dev hosts)
//! 2. Create a `zen-offering-*` container with bind-mounted
//!    volumes — one "managed" path under volumes_dir, one
//!    "external" path elsewhere
//! 3. Run `capture_snapshot` end-to-end
//! 4. Verify the manifest, file existence, and SHA512 hashes
//! 5. Tear down: stop + remove container, image, temp dirs
//!
//! When Docker is unavailable the tests log and pass — same
//! pattern as `api_health.rs`. Set `RUST_LOG=info` to see the
//! skip message when running locally without Docker.
//!
//! Each test uses unique container names + temp dirs so parallel
//! runs of the suite (default for `cargo test`) don't collide.

use bollard::Docker;
use bollard::models::{ContainerCreateBody, HostConfig, Mount, MountTypeEnum};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, ListImagesOptions,
    RemoveContainerOptionsBuilder, RemoveImageOptionsBuilder, StartContainerOptions,
    StopContainerOptions,
};
use futures_util::StreamExt;
use garden_common::offerings::OfferingFqn;
use garden_moss::domain::offering_events::{EventActor, EventKind, EventLog};
use garden_moss::domain::snapshot::{LocalSnapshotStore, SnapshotStore};
use std::path::PathBuf;
use tempfile::TempDir;

const TEST_IMAGE: &str = "alpine:3.19";

/// Try to reach the local Docker daemon. If the version probe
/// fails we skip the test gracefully.
async fn docker_or_skip() -> Option<Docker> {
    let docker = Docker::connect_with_local_defaults().ok()?;
    docker.version().await.ok()?;
    Some(docker)
}

/// Pull the test image if it isn't already present locally.
async fn ensure_test_image(docker: &Docker) -> anyhow::Result<()> {
    let mut stream = docker.create_image(
        Some(
            CreateImageOptionsBuilder::default()
                .from_image(TEST_IMAGE)
                .build(),
        ),
        None,
        None,
    );
    while let Some(item) = stream.next().await {
        item.map_err(|e| anyhow::anyhow!("pull failed: {e}"))?;
    }
    Ok(())
}

/// One-shot test harness: pulls the image, creates a container
/// with the requested volume mounts, starts it, returns enough
/// info for the test to drive capture / plant against it.
struct TestContainer {
    docker: Docker,
    /// FQN string used to derive `zen-offering-<encoded>` name.
    fqn_string: String,
    container_name: String,
    /// Temp dirs that own the bind-mount source paths so the
    /// test cleans them up automatically.
    _managed_temp: TempDir,
    _external_temp: TempDir,
    /// Recorded for diagnostic logs only — the test asserts
    /// against the manifest's host_path values, not this field
    /// directly.
    #[allow(dead_code)]
    managed_host: PathBuf,
    external_host: PathBuf,
}

impl TestContainer {
    /// Set up a unique-named container under a synthetic FQN
    /// that won't collide with anything real.
    async fn setup(docker: Docker, suffix: &str) -> anyhow::Result<Self> {
        ensure_test_image(&docker).await?;

        // The "managed" volume needs a host path that lives
        // under `volumes_dir()/<encoded_fqn>` so the classifier
        // recognises it. We can't easily plant something there
        // in tests, so we override the env var GARDEN_VOLUMES_DIR
        // to point inside a tempdir.
        let managed_temp = TempDir::new()?;
        let external_temp = TempDir::new()?;
        // Override volumes_dir for this process. Tests read it
        // through `garden_common::constants::paths::volumes_dir`,
        // which honours `GARDEN_VOLUMES_DIR`. The override is
        // process-wide, but each integration test uses a unique
        // suffix so even when the runtime overlaps the encoded
        // FQNs don't collide.
        //
        // SAFETY: Edition 2024 marks `set_var` unsafe because
        // multi-threaded readers can race. In our case the
        // serial tests + per-suffix uniqueness make this safe;
        // the volumes_dir lookup is the only reader.
        unsafe {
            std::env::set_var("GARDEN_VOLUMES_DIR", managed_temp.path());
        }

        let fqn_string = format!("snaptest-{suffix}");
        let fqn = OfferingFqn::parse(&fqn_string)
            .map_err(|e| anyhow::anyhow!("parse FQN: {e}"))?;
        let encoded = fqn.encoded_for_container();
        let container_name = format!("zen-offering-{encoded}");

        let managed_host = managed_temp.path().join(&encoded).join("data");
        std::fs::create_dir_all(&managed_host)?;
        // Drop a marker file so capture has bytes to archive.
        std::fs::write(managed_host.join("marker.txt"), b"managed-vol-marker")?;

        let external_host = external_temp.path().join("photos");
        std::fs::create_dir_all(&external_host)?;
        std::fs::write(external_host.join("photo.txt"), b"external-mount-marker")?;

        // Belt-and-braces: remove a stray container with the same
        // name from a prior failed test run.
        let _ = docker
            .remove_container(
                &container_name,
                Some(
                    RemoveContainerOptionsBuilder::default()
                        .force(true)
                        .v(true)
                        .build(),
                ),
            )
            .await;

        let body = ContainerCreateBody {
            image: Some(TEST_IMAGE.into()),
            cmd: Some(vec!["sleep".into(), "600".into()]),
            host_config: Some(HostConfig {
                mounts: Some(vec![
                    Mount {
                        target: Some("/data/db".into()),
                        source: Some(path_to_docker_string(&managed_host)),
                        typ: Some(MountTypeEnum::BIND),
                        ..Default::default()
                    },
                    Mount {
                        target: Some("/photos".into()),
                        source: Some(path_to_docker_string(&external_host)),
                        typ: Some(MountTypeEnum::BIND),
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            }),
            ..Default::default()
        };
        docker
            .create_container(
                Some(
                    CreateContainerOptionsBuilder::default()
                        .name(&container_name)
                        .build(),
                ),
                body,
            )
            .await
            .map_err(|e| anyhow::anyhow!("create container: {e}"))?;
        docker
            .start_container(&container_name, None::<StartContainerOptions>)
            .await
            .map_err(|e| anyhow::anyhow!("start container: {e}"))?;

        Ok(Self {
            docker,
            fqn_string,
            container_name,
            _managed_temp: managed_temp,
            _external_temp: external_temp,
            managed_host,
            external_host,
        })
    }

    /// Stop + remove the container. Idempotent.
    async fn teardown(&self) {
        let _ = self
            .docker
            .stop_container(&self.container_name, None::<StopContainerOptions>)
            .await;
        let _ = self
            .docker
            .remove_container(
                &self.container_name,
                Some(
                    RemoveContainerOptionsBuilder::default()
                        .force(true)
                        .v(true)
                        .build(),
                ),
            )
            .await;
    }
}

/// Convert a tempdir's path to the form Docker expects on the
/// current platform. On Windows Docker Desktop converts host
/// paths to its own canonical form; we just hand it the
/// platform-native string.
fn path_to_docker_string(p: &std::path::Path) -> String {
    p.to_string_lossy().to_string()
}

/// Clean up `zen-harvest/*` images the capture flow committed.
/// Best-effort; image removal failures are not test failures.
async fn cleanup_harvest_images(docker: &Docker, encoded_fqn: &str) {
    let pattern = format!("zen-harvest/{encoded_fqn}");
    let images = match docker.list_images(None::<ListImagesOptions>).await {
        Ok(v) => v,
        Err(_) => return,
    };
    for image in images {
        let matches = image
            .repo_tags
            .iter()
            .any(|t| t.starts_with(&pattern));
        if !matches {
            continue;
        }
        let _ = docker
            .remove_image(
                &image.id,
                Some(
                    RemoveImageOptionsBuilder::default()
                        .force(true)
                        .build(),
                ),
                None,
            )
            .await;
    }
}

#[tokio::test]
#[ignore = "requires a live Docker daemon — run with `cargo test --test snapshot_integration -- --ignored`"]
async fn capture_round_trips_image_and_volumes_and_external_mounts() {
    let Some(docker) = docker_or_skip().await else {
        eprintln!("skipping snapshot_integration: Docker daemon not reachable");
        return;
    };

    let fixture = TestContainer::setup(docker.clone(), "capture-roundtrip")
        .await
        .expect("test container setup");

    let fqn = OfferingFqn::parse(&fixture.fqn_string).unwrap();
    let store_root = TempDir::new().unwrap();
    let store = LocalSnapshotStore::new(store_root.path().to_path_buf());

    let log_root = TempDir::new().unwrap();
    let log = EventLog::open(log_root.path().join("events.log"));

    let state = garden_moss::testing::build_test_state().await;

    let result = garden_moss::infra::snapshot::capture_snapshot(
        &state,
        &fqn,
        &store,
        &log,
        EventActor::system("stone-test"),
    )
    .await;

    // Always tear down even if the assertion below fails.
    let captured = match result {
        Ok(c) => c,
        Err(e) => {
            fixture.teardown().await;
            cleanup_harvest_images(&docker, &fqn.encoded_for_container()).await;
            panic!("capture_snapshot failed: {e:#}");
        }
    };

    // ── Manifest assertions ─────────────────────────────────
    let manifest = &captured.manifest;
    assert_eq!(manifest.source_fqn, fqn.fqn());
    assert!(!manifest.id.is_empty(), "snapshot id must be set");
    assert!(
        !manifest.source_event_id.is_empty(),
        "manifest must reference the BackupTaken event"
    );
    assert_eq!(
        manifest.source_event_id, captured.event_id,
        "manifest's source_event_id must equal the returned event_id",
    );
    assert!(
        manifest.image.size_bytes > 0,
        "captured image tarball must be non-empty"
    );
    assert!(
        !manifest.image.sha512.is_empty(),
        "image SHA512 must be set"
    );

    // One managed volume + one external mount — the harness
    // configured exactly this shape.
    assert_eq!(
        manifest.volumes.len(),
        1,
        "expected one managed volume in the snapshot"
    );
    assert_eq!(
        manifest.external_mounts.len(),
        1,
        "expected one external mount in the snapshot"
    );
    assert_eq!(manifest.volumes[0].container_path, "/data/db");
    assert_eq!(manifest.external_mounts[0].container_path, "/photos");
    assert_eq!(
        manifest.external_mounts[0].host_path,
        path_to_docker_string(&fixture.external_host)
    );

    // ── Artifact files must exist on disk ───────────────────
    let image_path = store.image_path(&manifest.id);
    assert!(image_path.exists(), "image.tar must exist after capture");
    assert!(
        image_path
            .metadata()
            .map(|m| m.len() == manifest.image.size_bytes)
            .unwrap_or(false),
        "image.tar size must match manifest"
    );
    let vol_path = store.volume_path(&manifest.id, &manifest.volumes[0].name);
    assert!(vol_path.exists(), "volume archive must exist after capture");
    let em_path = store.external_mount_path(
        &manifest.id,
        &manifest.external_mounts[0].host_path,
    );
    assert!(
        em_path.exists(),
        "external_mount archive must exist after capture"
    );

    // ── Event log must record the BackupTaken ───────────────
    let events = log.read_all().await.expect("read events");
    let backup_event = events
        .iter()
        .rev()
        .find(|e| matches!(e.kind, EventKind::BackupTaken))
        .expect("BackupTaken must be recorded");
    assert_eq!(backup_event.event_id, captured.event_id);
    assert_eq!(backup_event.fqn, fqn.fqn());
    let snapshot_id_in_event = backup_event
        .details
        .get("snapshot_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert_eq!(snapshot_id_in_event, manifest.id);

    // ── Cleanup ─────────────────────────────────────────────
    fixture.teardown().await;
    cleanup_harvest_images(&docker, &fqn.encoded_for_container()).await;

    // store_root, log_root drop here, removing the snapshot's
    // on-disk artifacts.
}

#[tokio::test]
#[ignore = "requires a live Docker daemon — run with `cargo test --test snapshot_integration -- --ignored`"]
async fn capture_with_load_image_round_trip_via_docker_save_and_load() {
    // Verifies that ContainerRuntime::save_image + load_image
    // produce a tarball Docker can read back. Run against a
    // committed image rather than the whole capture flow so
    // this test isolates the save/load contract.
    let Some(docker) = docker_or_skip().await else {
        eprintln!("skipping save/load round trip: Docker daemon not reachable");
        return;
    };

    let fixture = TestContainer::setup(docker.clone(), "save-load-roundtrip")
        .await
        .expect("setup");

    // Commit the running container to a fresh tag so we have a
    // distinct image to save / load.
    let runtime = garden_moss::docker::ContainerRuntime::new()
        .expect("ContainerRuntime");
    let tag = chrono::Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let repo = format!("zen-harvest/{}", fixture.fqn_string);
    let image_ref = format!("{repo}:{tag}");
    runtime
        .commit_container(&fixture.container_name, &repo, &tag, true)
        .await
        .expect("commit container");

    // Save → tarball.
    let tar_dir = TempDir::new().unwrap();
    let tar_path = tar_dir.path().join("image.tar");
    let bytes_written = runtime
        .save_image(&image_ref, &tar_path)
        .await
        .expect("save image");
    assert!(bytes_written > 0, "saved image tarball must be non-empty");
    assert!(tar_path.exists());
    assert_eq!(tar_path.metadata().unwrap().len(), bytes_written);

    // Remove the image, then load it back from the tarball.
    let _ = docker
        .remove_image(
            &image_ref,
            Some(RemoveImageOptionsBuilder::default().force(true).build()),
            None,
        )
        .await;

    runtime
        .load_image(&tar_path)
        .await
        .expect("load image from tarball");

    // After load, the image must be reachable by ref again.
    let inspected = docker.inspect_image(&image_ref).await;
    assert!(
        inspected.is_ok(),
        "image must be present after load_image: {inspected:?}"
    );

    // Cleanup
    fixture.teardown().await;
    let _ = docker
        .remove_image(
            &image_ref,
            Some(RemoveImageOptionsBuilder::default().force(true).build()),
            None,
        )
        .await;
}
