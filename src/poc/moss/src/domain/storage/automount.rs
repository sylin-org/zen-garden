//! Auto-mount — discover and mount unmounted managed devices.
//!
//! Scans for unmounted removable devices that have a Zen Garden manifest,
//! then mounts them at their canonical path. VolumeMonitor detects the
//! mount and calls VolumeIngestor to classify and register.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

use super::ports::StoragePlatform;

/// How long to remember that a device has *no* Zen Garden manifest.
///
/// The probe at [`StoragePlatform::probe_device_manifest`] mounts the device
/// read-only, reads `.zen-garden/manifest.json`, and unmounts again. For
/// devices that aren't managed Zen Garden storage that's a wasted
/// mount/umount cycle on every tick of the storage-lifecycle task (every
/// 10 s) — visible as a constant `mount` / `ntfs-3g` / `umount` flood in
/// the journal on stones with an attached non-Zen drive.
///
/// We cache the negative verdict for this duration. Devices that *did*
/// have a manifest are mounted and tracked elsewhere, so they fall out of
/// `list_unmounted_removable` naturally. A 5-minute TTL means we still
/// re-probe occasionally to catch the case where a user repartitions or
/// writes a manifest to a previously-unmanaged device.
const PROBE_NEGATIVE_CACHE_TTL: Duration = Duration::from_secs(300);

static PROBE_NEGATIVE_CACHE: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Returns true if `device` was probed within the negative-cache TTL and
/// found to have no manifest.
fn recently_probed_negative(device: &str) -> bool {
    let cache = PROBE_NEGATIVE_CACHE.lock().expect("probe cache poisoned");
    cache
        .get(device)
        .is_some_and(|last| last.elapsed() < PROBE_NEGATIVE_CACHE_TTL)
}

/// Record that `device` has no Zen Garden manifest, so future probe ticks
/// skip the mount/umount cycle until the TTL expires.
fn record_negative_probe(device: &str) {
    let mut cache = PROBE_NEGATIVE_CACHE.lock().expect("probe cache poisoned");
    cache.insert(device.to_string(), Instant::now());
    // Opportunistically expire stale entries so the map doesn't grow
    // unboundedly across the process lifetime.
    cache.retain(|_, last| last.elapsed() < PROBE_NEGATIVE_CACHE_TTL);
}

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
        // Skip devices we've already probed and found unmanaged. Without
        // this, every 10s tick mounts and unmounts every non-Zen drive
        // attached to the stone — a noisy waste of subprocess time and
        // journal space.
        if recently_probed_negative(&device.device) {
            continue;
        }

        // Probe for manifest
        let manifest = match platform.probe_device_manifest(&device.device).await {
            Ok(Some(m)) => m,
            Ok(None) => {
                record_negative_probe(&device.device);
                continue; // not managed
            }
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
