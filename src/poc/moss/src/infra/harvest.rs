//! Harvest creation and restoration operations
//!
//! Orchestrates the backup and restore of offerings, combining:
//! - Client operations (image commit, volume inspection)
//! - Archive module (centralized compression/checksum)
//! - Manifest persistence (HarvestStore)

use crate::docker::ContainerRuntime;
use crate::domain::harvest::{HarvestManifest, VolumeArchive};
use crate::domain::traits::HarvestOps;
use crate::infra::HarvestStore;
use anyhow::{Context, Result};
use garden_common::infra::archive;
use garden_common::offerings::OfferingFqn;
use std::path::Path;
use std::sync::Arc;

/// Create a harvest for an offering
///
/// This captures the current state of an offering before nourishment:
/// 1. Commits the container image (if commit_image is true)
/// 2. Archives all mounted volumes (using centralized archive module)
/// 3. Saves the manifest for later restoration
///
/// # Arguments
/// * `docker` - Client manager for container operations
/// * `store` - Harvest store for persistence
/// * `offering` - Offering name (without zen-offering- prefix)
/// * `source_stone` - Stone ID where the harvest is created
/// * `commit_image` - Whether to commit the container image
///
/// # Returns
/// The created harvest manifest
pub async fn create_harvest(
    docker: &ContainerRuntime,
    store: &HarvestStore,
    offering: &str,
    source_stone: &str,
    commit_image: bool,
) -> Result<HarvestManifest> {
    let fqn = OfferingFqn::parse(offering)
        .map_err(|e| anyhow::anyhow!("Invalid offering name '{}': {}", offering, e))?;
    let encoded_offering = fqn.encoded_for_container();
    let container_name = crate::docker::zen_offering_container_name(offering)?;

    // Get current image
    let original_image = docker
        .get_service_image(offering)
        .await
        .context("Failed to get container image")?;

    let mut manifest = HarvestManifest::new(offering, source_stone, &original_image);

    tracing::info!(
        offering,
        harvest_id = %manifest.id,
        commit_image,
        "Creating harvest"
    );

    // Commit container image if requested
    if commit_image {
        let repo = format!("zen-harvest/{}", encoded_offering);
        let tag = chrono::Utc::now().format("%Y%m%dT%H%M%S").to_string();

        let image_id = docker
            .commit_container(&container_name, &repo, &tag, true)
            .await
            .context("Failed to commit container")?;

        manifest.committed_image = Some(format!("{}:{}", repo, tag));
        tracing::info!(
            offering,
            image_id = %image_id,
            committed_image = ?manifest.committed_image,
            "Committed container image"
        );
    }

    // Archive volumes using centralized archive module
    let volumes = docker.get_container_volumes(offering).await?;
    let volumes_dir = store.volumes_path(&manifest.id);

    for (host_path, container_path) in volumes {
        let volume_name = Path::new(&container_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "data".to_string());

        let archive_name = format!("{}.tar.gz", volume_name);
        let archive_path = volumes_dir.join(&archive_name);

        tracing::debug!(
            offering,
            volume = %volume_name,
            host_path = %host_path,
            "Archiving volume"
        );

        // Use centralized archive module - returns ArchiveInfo with size + checksum
        let archive_info = archive::create_archive(Path::new(&host_path), &archive_path)
            .await
            .context(format!("Failed to archive volume {}", volume_name))?;

        let size_display = garden_common::utils::format_bytes(archive_info.size_bytes);

        manifest.volumes.push(VolumeArchive {
            name: volume_name.clone(),
            container_path,
            archive_path: archive_path.to_string_lossy().to_string(),
            size_bytes: archive_info.size_bytes,
            checksum: archive_info.checksum,
        });

        tracing::info!(
            offering,
            volume = %volume_name,
            size = %size_display,
            "Archived volume"
        );
    }

    // Save manifest
    store.save_manifest(&manifest).await?;

    tracing::info!(
        offering,
        harvest_id = %manifest.id,
        total_size = %garden_common::utils::format_bytes(manifest.total_size_bytes()),
        volume_count = manifest.volumes.len(),
        "Harvest created successfully"
    );

    Ok(manifest)
}

/// Restore an offering from a harvest
///
/// Restores volume data from a previous harvest. The container must be stopped
/// before calling this function.
///
/// # Arguments
/// * `docker` - Client manager (used to verify volumes)
/// * `store` - Harvest store
/// * `harvest_id` - ID of the harvest to restore
///
/// # Note
/// This function does NOT restore the container image - that should be handled
/// by the ceremony orchestrator which may want to use a different image.
pub async fn restore_harvest(
    docker: &ContainerRuntime,
    store: &HarvestStore,
    harvest_id: &str,
) -> Result<()> {
    let manifest = store.load_manifest(&harvest_id.to_string()).await?;

    tracing::info!(
        harvest_id,
        offering = %manifest.offering,
        volume_count = manifest.volumes.len(),
        "Restoring harvest"
    );

    // Verify checksums before restoring
    for volume in &manifest.volumes {
        let valid = archive::verify_checksum(Path::new(&volume.archive_path), &volume.checksum)
            .await
            .context(format!(
                "Failed to verify checksum for volume {}",
                volume.name
            ))?;

        if !valid {
            anyhow::bail!(
                "Checksum mismatch for volume {} - archive may be corrupted",
                volume.name
            );
        }

        tracing::debug!(volume = %volume.name, "Checksum verified");
    }

    // Get current volume mappings
    let volumes = docker.get_container_volumes(&manifest.offering).await?;

    // Restore each volume using centralized archive module
    for volume_archive in &manifest.volumes {
        // Find matching host path
        if let Some((host_path, _)) = volumes
            .iter()
            .find(|(_, cp)| *cp == volume_archive.container_path)
        {
            tracing::debug!(
                volume = %volume_archive.name,
                host_path = %host_path,
                "Restoring volume"
            );

            archive::extract_archive(
                Path::new(&volume_archive.archive_path),
                Path::new(host_path),
            )
            .await
            .context(format!("Failed to restore volume {}", volume_archive.name))?;

            tracing::info!(volume = %volume_archive.name, "Volume restored");
        } else {
            tracing::warn!(
                volume = %volume_archive.name,
                container_path = %volume_archive.container_path,
                "Volume mount not found in current container - skipping"
            );
        }
    }

    tracing::info!(
        harvest_id,
        offering = %manifest.offering,
        "Harvest restored successfully"
    );

    Ok(())
}

/// One volume's restoration plan: where the archive lives, where
/// it should land. Pulled out so [`apply_volumes_with_staging`]
/// can be tested without Docker.
#[derive(Debug, Clone)]
pub struct VolumeRestorePlan {
    pub name: String,
    pub archive_path: std::path::PathBuf,
    pub live_path: std::path::PathBuf,
}

/// Restore an offering from a harvest using staging volumes with
/// atomic swap. Functionally equivalent to [`restore_harvest`] on
/// success; differs in failure semantics.
///
/// Failure modes:
/// - Manifest load / checksum verification fails before any
///   filesystem changes.
/// - Extraction of any archive into its staging directory fails:
///   all staging directories are cleaned up; live volumes are
///   untouched. Caller sees the original error.
/// - Mid-swap failure (rare; rename is a single syscall on the
///   same filesystem): already-swapped volumes are rolled back
///   from their `.previous-{harvest_id}` directories. Staging
///   and previous artifacts are cleaned up; caller sees the
///   swap error.
///
/// Used by the plant flow in [ORCH-0039] where a torn restore
/// would leave the destination in an unusable state. The Water
/// phase of nourish ceremonies still uses [`restore_harvest`]
/// because nourish runs the rollback against a known-good prior
/// harvest — the partial-state risk is bounded.
///
/// [ORCH-0039]: ../../../../docs/decisions/ORCH-0039-seed-based-offering-replication.md
pub async fn restore_harvest_with_staging(
    docker: &ContainerRuntime,
    store: &HarvestStore,
    harvest_id: &str,
) -> Result<()> {
    let manifest = store.load_manifest(&harvest_id.to_string()).await?;

    tracing::info!(
        harvest_id,
        offering = %manifest.offering,
        volume_count = manifest.volumes.len(),
        "Restoring harvest with staging"
    );

    // Pre-flight: verify all checksums before touching any
    // filesystem state. Same as `restore_harvest`.
    for volume in &manifest.volumes {
        let valid = archive::verify_checksum(Path::new(&volume.archive_path), &volume.checksum)
            .await
            .context(format!(
                "Failed to verify checksum for volume {}",
                volume.name
            ))?;

        if !valid {
            anyhow::bail!(
                "Checksum mismatch for volume {} - archive may be corrupted",
                volume.name
            );
        }
    }

    // Resolve live host paths via Docker.
    let volumes = docker.get_container_volumes(&manifest.offering).await?;
    let mut plans = Vec::with_capacity(manifest.volumes.len());
    for volume_archive in &manifest.volumes {
        let live = volumes
            .iter()
            .find(|(_, cp)| *cp == volume_archive.container_path)
            .map(|(host, _)| host.clone());

        let Some(live_path) = live else {
            tracing::warn!(
                volume = %volume_archive.name,
                container_path = %volume_archive.container_path,
                "Volume mount not found in current container - skipping"
            );
            continue;
        };

        plans.push(VolumeRestorePlan {
            name: volume_archive.name.clone(),
            archive_path: std::path::PathBuf::from(&volume_archive.archive_path),
            live_path: std::path::PathBuf::from(&live_path),
        });
    }

    apply_volumes_with_staging(&plans, harvest_id).await?;

    tracing::info!(
        harvest_id,
        offering = %manifest.offering,
        "Harvest restored with staging"
    );

    Ok(())
}

/// Pure filesystem orchestration for staged volume restoration.
///
/// Three phases:
///   1. **Stage**: extract each archive into a sibling
///      `<live>.staging-<harvest_id>` directory. If any extraction
///      fails, all staging directories are cleaned up and the
///      original error is returned. Live volumes are unchanged.
///   2. **Swap**: for each plan, rename `live` → `<live>.previous-<harvest_id>`
///      (preserving prior content for rollback) and `<staging>` → `live`.
///      If any swap fails, the already-swapped volumes are rolled
///      back from their `.previous` directories.
///   3. **Cleanup**: remove all `.previous-<harvest_id>` directories.
///
/// Atomic-rename guarantee assumes staging and live are on the
/// same filesystem. Callers responsible for ensuring this — in
/// practice, the staging path is always a sibling of live, so
/// they share the parent directory's filesystem.
pub async fn apply_volumes_with_staging(
    plans: &[VolumeRestorePlan],
    harvest_id: &str,
) -> Result<()> {
    if plans.is_empty() {
        return Ok(());
    }

    // ── Phase 1: Stage ───────────────────────────────────────────
    // Extract every archive into a sibling staging directory. On
    // any failure, clean up everything we've staged and abort.
    let mut staged: Vec<std::path::PathBuf> = Vec::with_capacity(plans.len());
    for plan in plans {
        let staging_path = staging_path_for(&plan.live_path, harvest_id);

        // Defensively remove a leftover staging dir from a prior
        // crashed restore for this same harvest_id.
        let _ = tokio::fs::remove_dir_all(&staging_path).await;

        match archive::extract_archive(&plan.archive_path, &staging_path).await {
            Ok(()) => {
                staged.push(staging_path);
                tracing::debug!(
                    volume = %plan.name,
                    "Volume staged for swap"
                );
            }
            Err(e) => {
                tracing::error!(
                    volume = %plan.name,
                    error = %e,
                    "Staging extraction failed; aborting restore (live volumes untouched)"
                );
                cleanup_paths(&staged).await;
                let _ = tokio::fs::remove_dir_all(&staging_path).await;
                return Err(e).context(format!(
                    "Failed to stage volume {} for restore",
                    plan.name
                ));
            }
        }
    }

    // ── Phase 2: Swap ────────────────────────────────────────────
    // Move live → previous, then staging → live, per volume.
    // Track which swaps committed so we can roll back on failure.
    let mut swapped: Vec<SwappedVolume> = Vec::with_capacity(plans.len());
    for (plan, staging) in plans.iter().zip(staged.iter()) {
        let previous = previous_path_for(&plan.live_path, harvest_id);

        // If a previous-* dir exists from a prior crashed restore,
        // remove it before we reuse the path.
        let _ = tokio::fs::remove_dir_all(&previous).await;

        let live_existed = plan.live_path.exists();
        if live_existed {
            if let Err(e) = tokio::fs::rename(&plan.live_path, &previous).await {
                tracing::error!(
                    volume = %plan.name,
                    error = %e,
                    "Live → previous rename failed; rolling back prior swaps"
                );
                rollback_swaps(&swapped).await;
                cleanup_staging(&staged, &swapped).await;
                return Err(anyhow::anyhow!(e).context(format!(
                    "Failed to move live volume {} aside before swap",
                    plan.name
                )));
            }
        }

        if let Err(e) = tokio::fs::rename(staging, &plan.live_path).await {
            tracing::error!(
                volume = %plan.name,
                error = %e,
                "Staging → live rename failed; rolling back this swap and prior swaps"
            );
            // Restore the live we just moved aside.
            if live_existed {
                let _ = tokio::fs::rename(&previous, &plan.live_path).await;
            }
            rollback_swaps(&swapped).await;
            cleanup_staging(&staged, &swapped).await;
            return Err(anyhow::anyhow!(e).context(format!(
                "Failed to swap staged volume {} into live",
                plan.name
            )));
        }

        swapped.push(SwappedVolume {
            live_path: plan.live_path.clone(),
            previous_path: live_existed.then_some(previous),
        });
        tracing::info!(volume = %plan.name, "Volume restored");
    }

    // ── Phase 3: Cleanup ─────────────────────────────────────────
    // Best-effort removal of `.previous-*` directories — failure
    // here doesn't affect correctness (live state is already
    // correct), it just leaves disk debris.
    for swap in &swapped {
        if let Some(prev) = &swap.previous_path {
            if let Err(e) = tokio::fs::remove_dir_all(prev).await {
                tracing::warn!(
                    path = %prev.display(),
                    error = %e,
                    "Failed to clean up previous-volume directory after restore"
                );
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
struct SwappedVolume {
    live_path: std::path::PathBuf,
    /// `None` when there was no live volume to move aside (first
    /// plant). On rollback, we just remove the now-live directory.
    previous_path: Option<std::path::PathBuf>,
}

fn staging_path_for(live_path: &Path, harvest_id: &str) -> std::path::PathBuf {
    let mut s = live_path.as_os_str().to_owned();
    s.push(format!(".staging-{}", harvest_id));
    std::path::PathBuf::from(s)
}

fn previous_path_for(live_path: &Path, harvest_id: &str) -> std::path::PathBuf {
    let mut s = live_path.as_os_str().to_owned();
    s.push(format!(".previous-{}", harvest_id));
    std::path::PathBuf::from(s)
}

/// Best-effort removal of a list of staging directories.
async fn cleanup_paths(paths: &[std::path::PathBuf]) {
    for p in paths {
        let _ = tokio::fs::remove_dir_all(p).await;
    }
}

/// Cleanup leftover staging dirs that were extracted but not yet
/// swapped at the moment of failure. `swapped` carries the count
/// of entries already promoted to live (whose staging dirs no
/// longer exist as such — the rename moved them).
async fn cleanup_staging(staged: &[std::path::PathBuf], swapped: &[SwappedVolume]) {
    // Skip the first `swapped.len()` staging entries — those have
    // already been renamed into live.
    for staging in staged.iter().skip(swapped.len()) {
        let _ = tokio::fs::remove_dir_all(staging).await;
    }
}

/// Roll back already-committed swaps in reverse order.
async fn rollback_swaps(swapped: &[SwappedVolume]) {
    for swap in swapped.iter().rev() {
        // Remove the now-incorrect live (it's the staging content
        // we just put there) and rename .previous → live.
        let _ = tokio::fs::remove_dir_all(&swap.live_path).await;
        if let Some(prev) = &swap.previous_path {
            if let Err(e) = tokio::fs::rename(prev, &swap.live_path).await {
                // If rollback rename fails the user is in a torn
                // state. Log loudly — we cannot recover here.
                tracing::error!(
                    live = %swap.live_path.display(),
                    previous = %prev.display(),
                    error = %e,
                    "ROLLBACK FAILED — manual intervention required"
                );
            }
        }
    }
}

/// Verify a harvest's integrity
///
/// Checks that all archives exist and have valid checksums.
pub async fn verify_harvest(store: &HarvestStore, harvest_id: &str) -> Result<bool> {
    let manifest = store.load_manifest(&harvest_id.to_string()).await?;

    for volume in &manifest.volumes {
        let archive_path = Path::new(&volume.archive_path);

        if !archive_path.exists() {
            tracing::warn!(
                harvest_id,
                volume = %volume.name,
                "Archive file missing"
            );
            return Ok(false);
        }

        let valid = archive::verify_checksum(archive_path, &volume.checksum).await?;
        if !valid {
            tracing::warn!(
                harvest_id,
                volume = %volume.name,
                "Checksum mismatch"
            );
            return Ok(false);
        }
    }

    Ok(true)
}

/// Concrete harvest operations backed by Docker + HarvestStore.
pub struct OsHarvestOps {
    docker: Arc<ContainerRuntime>,
    store: Arc<HarvestStore>,
}

impl OsHarvestOps {
    pub fn new(docker: Arc<ContainerRuntime>, store: Arc<HarvestStore>) -> Self {
        Self { docker, store }
    }
}

impl HarvestOps for OsHarvestOps {
    async fn create_harvest(
        &self,
        offering: &str,
        source_stone: &str,
        commit_image: bool,
    ) -> Result<HarvestManifest> {
        create_harvest(
            &self.docker,
            &self.store,
            offering,
            source_stone,
            commit_image,
        )
        .await
    }

    async fn restore_harvest(&self, harvest_id: &str) -> Result<()> {
        restore_harvest(&self.docker, &self.store, harvest_id).await
    }

    async fn restore_harvest_with_staging(&self, harvest_id: &str) -> Result<()> {
        restore_harvest_with_staging(&self.docker, &self.store, harvest_id).await
    }
}

#[cfg(test)]
mod tests {
    //! Integration tests for the create + restore round-trip
    //! require Docker — see tests/harvest_integration.rs.
    //!
    //! The unit tests below cover the pure-filesystem
    //! [`apply_volumes_with_staging`] orchestration so the
    //! atomic-swap and rollback paths are testable without a
    //! container runtime. Each test uses a fresh tempdir to
    //! simulate live + archive paths.

    use super::*;
    use tempfile::TempDir;

    /// Build a tar.gz archive of `source` at `archive_path`. Wraps
    /// `archive::create_archive` with a panic on failure — these
    /// are tests, not production paths.
    async fn make_archive(source: &Path, archive_path: &Path) {
        archive::create_archive(source, archive_path)
            .await
            .expect("test setup: create_archive must succeed");
    }

    /// Read every regular file under `dir` and return a sorted
    /// `(relative_path, contents)` list. Used to assert directory
    /// contents match across the staging boundary.
    fn snapshot_dir(dir: &Path) -> Vec<(String, String)> {
        if !dir.exists() {
            return Vec::new();
        }
        fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
            let entries = std::fs::read_dir(dir).expect("read_dir");
            for entry in entries {
                let entry = entry.expect("dir entry");
                let path = entry.path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    let rel = path
                        .strip_prefix(root)
                        .expect("strip_prefix")
                        .to_string_lossy()
                        .replace('\\', "/")
                        .to_string();
                    let content = std::fs::read_to_string(&path).unwrap_or_default();
                    out.push((rel, content));
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    /// Drop a single text file into a directory.
    fn write_file(dir: &Path, relative: &str, content: &str) {
        let p = dir.join(relative);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("create_dir_all");
        }
        std::fs::write(&p, content).expect("write file");
    }

    #[tokio::test]
    async fn empty_plan_is_a_noop() {
        // Defensive case — apply_volumes_with_staging must not
        // touch the filesystem when given nothing to do.
        apply_volumes_with_staging(&[], "harv-1").await.unwrap();
    }

    #[tokio::test]
    async fn successful_restore_replaces_live_with_archive_content() {
        let work = TempDir::new().unwrap();

        // `archive::create_archive` runs `tar -czf <dest> -C <parent>
        // <basename>` and so embeds the source directory's basename
        // as the archive's top-level entry. Extraction recreates
        // that subdirectory inside the extraction target. The
        // staging code matches production: live ends up containing
        // a single `<source_basename>/...` subtree.
        let source = work.path().join("source");
        write_file(&source, "hello.txt", "from-archive");
        write_file(&source, "nested/deep.txt", "deep-content");

        let archive = work.path().join("vol1.tar.gz");
        make_archive(&source, &archive).await;

        // Live state — what's there before restore. The restore
        // must replace this with the archive content.
        let live = work.path().join("live");
        write_file(&live, "hello.txt", "stale-live-content");
        write_file(&live, "old.txt", "this-was-here-before");

        let plans = vec![VolumeRestorePlan {
            name: "vol1".into(),
            archive_path: archive,
            live_path: live.clone(),
        }];
        apply_volumes_with_staging(&plans, "harv-success")
            .await
            .unwrap();

        // Live now holds the extracted `source/` subtree. The
        // pre-restore content (`old.txt`) is gone.
        assert!(
            !live.join("old.txt").exists(),
            "pre-restore content must be replaced, not merged"
        );
        assert_eq!(
            std::fs::read_to_string(live.join("source/hello.txt")).unwrap(),
            "from-archive",
        );
        assert_eq!(
            std::fs::read_to_string(live.join("source/nested/deep.txt")).unwrap(),
            "deep-content",
        );

        // Cleanup invariant: no .staging-* or .previous-* dirs
        // left behind on success.
        assert!(
            !work.path().join("live.staging-harv-success").exists(),
            "staging directory must be cleaned up on success"
        );
        assert!(
            !work.path().join("live.previous-harv-success").exists(),
            "previous directory must be cleaned up on success"
        );
    }

    #[tokio::test]
    async fn restore_into_nonexistent_live_creates_it() {
        // First-plant case — there's no prior live volume to
        // move aside. The staging directory becomes live in a
        // single rename.
        let work = TempDir::new().unwrap();
        let source = work.path().join("source");
        write_file(&source, "a.txt", "alpha");
        let archive = work.path().join("vol.tar.gz");
        make_archive(&source, &archive).await;

        let live = work.path().join("first-plant-live");
        // Note: live does NOT exist beforehand.
        assert!(!live.exists());

        let plans = vec![VolumeRestorePlan {
            name: "vol".into(),
            archive_path: archive,
            live_path: live.clone(),
        }];
        apply_volumes_with_staging(&plans, "harv-fresh")
            .await
            .unwrap();

        assert!(live.exists(), "live must exist after restore");
        // See note in `successful_restore_replaces_live_with_archive_content`
        // — extraction nests under the source directory's basename.
        let content = std::fs::read_to_string(live.join("source/a.txt")).unwrap();
        assert_eq!(content, "alpha");
    }

    #[tokio::test]
    async fn corrupt_archive_fails_extraction_and_leaves_live_unchanged() {
        // Critical safety property: a failure during staging
        // extraction must not mutate live volumes.
        let work = TempDir::new().unwrap();

        // Good archive for vol-good.
        let good_source = work.path().join("good_source");
        write_file(&good_source, "ok.txt", "good-content");
        let good_archive = work.path().join("good.tar.gz");
        make_archive(&good_source, &good_archive).await;

        // Corrupt archive for vol-bad — write a non-tar file.
        let bad_archive = work.path().join("bad.tar.gz");
        std::fs::write(&bad_archive, b"this is not a valid tar.gz").unwrap();

        // Live state for both volumes — must survive the failed
        // restore intact.
        let live_good = work.path().join("live_good");
        write_file(&live_good, "preserved.txt", "must-stay");
        let live_bad = work.path().join("live_bad");
        write_file(&live_bad, "also-preserved.txt", "must-also-stay");

        let live_good_before = snapshot_dir(&live_good);
        let live_bad_before = snapshot_dir(&live_bad);

        let plans = vec![
            VolumeRestorePlan {
                name: "vol-good".into(),
                archive_path: good_archive,
                live_path: live_good.clone(),
            },
            VolumeRestorePlan {
                name: "vol-bad".into(),
                archive_path: bad_archive,
                live_path: live_bad.clone(),
            },
        ];
        let result = apply_volumes_with_staging(&plans, "harv-bad").await;
        assert!(
            result.is_err(),
            "corrupt archive must surface as an error"
        );

        // Live state for BOTH volumes is unchanged — the staging
        // failure occurred before any swap.
        assert_eq!(
            snapshot_dir(&live_good),
            live_good_before,
            "live_good must be unchanged after failed restore"
        );
        assert_eq!(
            snapshot_dir(&live_bad),
            live_bad_before,
            "live_bad must be unchanged after failed restore"
        );

        // No staging artifacts should remain.
        for entry in std::fs::read_dir(work.path()).unwrap() {
            let p = entry.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            assert!(
                !name.contains(".staging-harv-bad"),
                "staging dir must be cleaned up after failure: {name}"
            );
            assert!(
                !name.contains(".previous-harv-bad"),
                "previous dir must not exist after pre-swap failure: {name}"
            );
        }
    }

    #[tokio::test]
    async fn leftover_staging_from_prior_crash_is_overwritten() {
        // Simulate a prior crashed restore that left a stale
        // `.staging-*` directory. The new restore must overwrite
        // it cleanly.
        let work = TempDir::new().unwrap();
        let source = work.path().join("source");
        write_file(&source, "fresh.txt", "fresh");
        let archive = work.path().join("v.tar.gz");
        make_archive(&source, &archive).await;

        let live = work.path().join("live");
        write_file(&live, "old.txt", "old");

        // Pre-create a stale staging directory with garbage.
        let stale_staging = work.path().join("live.staging-harv-recovered");
        std::fs::create_dir_all(&stale_staging).unwrap();
        std::fs::write(stale_staging.join("garbage.txt"), "leftover").unwrap();

        let plans = vec![VolumeRestorePlan {
            name: "v".into(),
            archive_path: archive,
            live_path: live.clone(),
        }];
        apply_volumes_with_staging(&plans, "harv-recovered")
            .await
            .unwrap();

        // Same nesting note as the success test — content lands
        // under the source basename.
        let content = std::fs::read_to_string(live.join("source/fresh.txt")).unwrap();
        assert_eq!(content, "fresh", "stale staging must not pollute restore");
        assert!(
            !work
                .path()
                .join("live.staging-harv-recovered")
                .exists(),
            "staging dir must be cleaned up after success"
        );
    }

    #[tokio::test]
    async fn three_volume_restore_swaps_all_or_none() {
        // Multi-volume happy path. Verifies the loop's per-volume
        // swap doesn't drift when more than one volume is in play.
        let work = TempDir::new().unwrap();
        let mut plans = Vec::new();
        for (i, content) in ["alpha", "beta", "gamma"].iter().enumerate() {
            let source = work.path().join(format!("src-{i}"));
            write_file(&source, "f.txt", content);
            let archive = work.path().join(format!("a-{i}.tar.gz"));
            make_archive(&source, &archive).await;
            let live = work.path().join(format!("live-{i}"));
            write_file(&live, "f.txt", "stale");
            plans.push(VolumeRestorePlan {
                name: format!("vol-{i}"),
                archive_path: archive,
                live_path: live,
            });
        }

        apply_volumes_with_staging(&plans, "harv-three")
            .await
            .unwrap();

        for (i, content) in ["alpha", "beta", "gamma"].iter().enumerate() {
            // Source basename `src-{i}` becomes the top-level
            // directory inside the extracted live volume.
            let live_file = work
                .path()
                .join(format!("live-{i}"))
                .join(format!("src-{i}"))
                .join("f.txt");
            assert_eq!(std::fs::read_to_string(&live_file).unwrap(), *content);
        }
    }

    #[test]
    fn staging_and_previous_path_helpers_use_harvest_id_suffix() {
        let live = std::path::PathBuf::from("/var/lib/zen-garden/volumes/data");
        assert_eq!(
            staging_path_for(&live, "abc123").to_string_lossy(),
            "/var/lib/zen-garden/volumes/data.staging-abc123"
        );
        assert_eq!(
            previous_path_for(&live, "abc123").to_string_lossy(),
            "/var/lib/zen-garden/volumes/data.previous-abc123"
        );
    }
}
