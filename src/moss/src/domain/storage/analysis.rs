//! Device analysis — eligibility checks for `storage add`.
//!
//! Composes platform queries (removable, capacity, label, mount) with domain
//! rules (allowed mount paths, device state) into a single result.

use std::path::Path;

use anyhow::{Context, Result};
use garden_common::storage::{DeviceState, StorageDetectedInfo, StorageManifest};

use crate::domain::traits::StoragePlatform;

/// Check if a mount path is in an allowed location for managed storage.
pub fn is_allowed_mount(mount_path: &str) -> bool {
    mount_path.starts_with("/mnt/")
        || mount_path.starts_with("/media/")
        || mount_path.starts_with("/run/media/")
        || mount_path.starts_with("/var/lib/zen-garden/mounts/")
        || mount_path.starts_with("/var/lib/garden-moss/mounts/")
}

/// Validate a `.zen-garden/` manifest directory and return the parsed manifest.
pub fn validate_manifest(zen_dir: &Path) -> Result<StorageManifest> {
    let manifest_path = zen_dir.join("manifest.json");

    if !manifest_path.exists() {
        anyhow::bail!("Manifest file does not exist");
    }

    let content =
        std::fs::read_to_string(&manifest_path).context("Failed to read manifest file")?;

    let manifest: StorageManifest =
        serde_json::from_str(&content).context("Manifest JSON is corrupt or incomplete")?;

    if manifest.id.is_empty() {
        anyhow::bail!("Manifest missing id field");
    }
    if manifest.name.is_empty() {
        anyhow::bail!("Manifest missing name field");
    }
    if manifest.origin_stone.is_empty() {
        anyhow::bail!("Manifest missing origin_stone field");
    }

    if !zen_dir.join("blobs").exists() {
        anyhow::bail!("Missing blobs directory");
    }
    if !zen_dir.join("journal").exists() {
        anyhow::bail!("Missing journal directory");
    }

    Ok(manifest)
}

/// Analyze a block device and return full eligibility information.
///
/// Composes platform queries (removable, capacity, label, mount) with domain
/// rules (allowed mount paths, device state) into a single result.
pub fn analyze_device(
    device_path: &str,
    platform: &(impl StoragePlatform + ?Sized),
) -> Result<StorageDetectedInfo> {
    let removable = platform.is_removable(device_path);
    let capacity_bytes = platform.device_capacity(device_path);
    let label = platform.device_label(device_path);
    let mount_path = platform.mount_point_for_device(device_path);

    let state = platform
        .probe_device_state(device_path, mount_path.as_deref())
        .unwrap_or(DeviceState::HasData);

    let mut eligible = state.is_eligible();
    let mut ineligible_reason = None;

    if !removable {
        eligible = false;
        ineligible_reason = Some("Device is not removable".to_string());
    } else if let Some(ref mount) = mount_path
        && !is_allowed_mount(mount) {
            eligible = false;
            ineligible_reason = Some(format!("Mount path {} is not in allowed location", mount));
        }

    if !state.is_eligible() && ineligible_reason.is_none() {
        ineligible_reason = Some(format!("Device state is {}", state));
    }

    Ok(StorageDetectedInfo {
        device: device_path.to_string(),
        mount_path,
        label,
        capacity_bytes,
        state,
        eligible,
        removable,
        ineligible_reason,
    })
}
