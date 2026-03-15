//! Harvest creation and restoration operations
//!
//! Orchestrates the backup and restore of offerings, combining:
//! - Client operations (image commit, volume inspection)
//! - Archive module (centralized compression/checksum)
//! - Manifest persistence (HarvestStore)

use crate::docker::Client;
use crate::domain::harvest::{HarvestManifest, VolumeArchive};
use crate::infra::HarvestStore;
use anyhow::{Context, Result};
use garden_common::infra::archive;
use garden_common::offerings::OfferingFqn;
use std::path::Path;

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
    docker: &Client,
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
    docker: &Client,
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

#[cfg(test)]
mod tests {
    // Integration tests require Client - see tests/harvest_integration.rs
}
