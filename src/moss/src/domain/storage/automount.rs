//! Auto-mount — discover and mount unmounted managed devices.
//!
//! Scans for unmounted removable devices that have a Zen Garden manifest,
//! then mounts them at their canonical path. VolumeMonitor detects the
//! mount and calls VolumeIngestor to classify and register.

use tracing::{debug, info, warn};

use super::ports::StoragePlatform;

/// Auto-mount unmounted removable devices that have a Zen Garden manifest.
///
/// Scans for unmounted removable devices, probes each for a manifest, and mounts
/// to the canonical path. Returns the count mounted.
/// Emits no StorageChanged events — VolumeMonitor detects the mount and calls VolumeIngestor.
pub async fn auto_mount_unmounted(platform: &(impl StoragePlatform + ?Sized)) -> usize {
    let unmounted = platform.list_unmounted_removable();
    if unmounted.is_empty() {
        return 0;
    }

    debug!(
        count = unmounted.len(),
        "Checking unmounted removable devices for manifests"
    );

    let data_dir = garden_common::constants::paths::data_dir();
    let mut mounted = 0usize;

    for device in &unmounted {
        // Probe for manifest
        let manifest = match platform.probe_device_manifest(&device.device).await {
            Ok(Some(m)) => m,
            Ok(None) => continue, // not managed
            Err(e) => {
                warn!(device = %device.device, error = %e, "Failed to probe device");
                continue;
            }
        };

        // Derive canonical mount path
        let mount_path = manifest.derive_mount_path(&data_dir);

        // Skip if already correctly mounted; replace stale FUSE mounts
        if platform.is_mount_point(&mount_path) {
            let mounted_device = platform.device_at_mount_point(&mount_path);
            if mounted_device.as_deref() == Some(&device.device) {
                debug!(mount = %mount_path, "Device already mounted at canonical path, skipping");
                continue;
            }
            // A different (stale) device occupies the mount point — evict it
            warn!(
                mount = %mount_path,
                stale_device = ?mounted_device,
                new_device = %device.device,
                "Stale mount at canonical path — replacing"
            );
            if let Err(e) = platform.unmount_lazy(&mount_path).await {
                warn!(
                    mount = %mount_path,
                    error = %e,
                    "Failed to remove stale mount, skipping"
                );
                continue;
            }
        }

        // Mount
        info!(
            device = %device.device,
            mount = %mount_path,
            name = %manifest.name,
            id = %manifest.id,
            "Auto-mounting managed storage"
        );

        if let Err(e) = platform.mount_device(&device.device, &mount_path).await {
            warn!(
                device = %device.device,
                mount = %mount_path,
                error = %e,
                "Failed to auto-mount"
            );
            continue;
        }

        // VolumeIngestor will detect the new mount via VolumeMonitor and emit Connected.
        mounted += 1;
    }

    if mounted > 0 {
        info!(count = mounted, "Auto-mounted managed storage devices");
    }
    mounted
}
