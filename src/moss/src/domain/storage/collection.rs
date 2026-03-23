//! Volumes collection — the unified map of all local storage.
//!
//! Single source of truth for all local storage state. Operations:
//! reconcile (tick), initial scan, health probing, and query helpers.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use garden_common::storage::StorageRole;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::domain::traits::{ManagementStoreOps, StoragePlatform};

use super::platform_types::VolumeSnapshot;
use super::volume::{Volume, VolumeState};

/// The unified volume collection — keyed by device path.
///
/// Single source of truth for all local storage state.
pub type Volumes = Arc<RwLock<HashMap<String, Volume>>>;

/// Create an empty `Volumes` map.
pub fn new_volumes() -> Volumes {
    Arc::new(RwLock::new(HashMap::new()))
}

// ============================================================================
// Domain operations on the Volumes collection
// ============================================================================

/// Reconcile the Volumes map against a fresh set of OS snapshots.
///
/// This is the core domain tick:
/// 1. New snapshots → classify and insert
/// 2. Existing snapshots → update capacity/online
/// 3. Missing snapshots → mark offline or remove
///
/// ## Manifest-based identity (STORAGE-0011)
///
/// When a device reappears at a different path (e.g., `/dev/sdb1` → `/dev/sdc1`),
/// the old entry is removed and replaced by the new one, preserving the management
/// state by matching on the manifest ID (GUIDv7) instead of the device path.
pub async fn reconcile<S: ManagementStoreOps + 'static>(
    volumes: &Volumes,
    snapshots: &[VolumeSnapshot],
    make_store: &(dyn Fn(PathBuf) -> Arc<S> + Send + Sync),
) {
    let current_paths: std::collections::HashSet<&str> =
        snapshots.iter().map(|s| s.path.as_str()).collect();

    let mut map = volumes.write().await;

    // Update existing and add new
    for snap in snapshots {
        if let Some(vol) = map.get_mut(&snap.path) {
            // Update from latest snapshot
            vol.capacity_bytes = snap.capacity_bytes;
            vol.label = snap.label.clone();
            vol.mount_path = PathBuf::from(&snap.mount_path);
            vol.state = VolumeState::Online;
            // Re-classify unmanaged volumes — they may have gained a manifest
            // since last scan (e.g. `storage add` wrote one).
            if !vol.is_managed() {
                vol.classify(make_store).await;
                if vol.is_managed() {
                    info!(path = %snap.path, name = %vol.display_name(), "Volume became managed");
                }
            }
        } else {
            // New volume — classify
            let mut vol = Volume::from_snapshot(snap);
            vol.classify(make_store).await;

            if vol.is_managed() {
                let name = vol.display_name().to_string();
                let manifest_id = vol.management.as_ref().map(|m| m.id.as_str()).unwrap_or("");

                // Manifest-based dedup: check if a stale entry exists for this
                // same physical device under a different device path.
                let stale_key = map
                    .iter()
                    .find(|(k, v)| {
                        *k != &snap.path
                            && v.management
                                .as_ref()
                                .map(|m| m.id.as_str() == manifest_id)
                                .unwrap_or(false)
                    })
                    .map(|(k, _)| k.clone());

                if let Some(old_key) = stale_key {
                    info!(
                        old_path = %old_key,
                        new_path = %snap.path,
                        name = %name,
                        id = %manifest_id,
                        "Volume re-keyed (device path changed)"
                    );
                    map.remove(&old_key);
                } else {
                    info!(path = %snap.path, name = %name, "Managed volume registered");
                }
            } else {
                debug!(path = %snap.path, "Unmanaged volume registered");
            }

            map.insert(snap.path.clone(), vol);
        }
    }

    // Mark disappeared volumes
    let departed: Vec<String> = map
        .keys()
        .filter(|k| !current_paths.contains(k.as_str()))
        .cloned()
        .collect();

    for path in departed {
        if let Some(vol) = map.get_mut(&path)
            && vol.state != VolumeState::Offline {
                info!(path = %path, name = %vol.display_name(), "Volume went offline");
                vol.state = VolumeState::Offline;
            }
    }
}

/// Probe health on all volumes. Offline volumes are skipped by [`Volume::probe_health`].
pub async fn health_tick_all(volumes: &Volumes, platform: &(impl StoragePlatform + ?Sized)) {
    let mut map = volumes.write().await;
    for vol in map.values_mut() {
        vol.probe_health(platform);
    }
}

/// Initial scan: enumerate OS volumes, classify, populate the map.
pub async fn initial_scan<P: StoragePlatform + 'static, S: ManagementStoreOps + 'static>(
    volumes: &Volumes,
    platform: Arc<P>,
    make_store: &(dyn Fn(PathBuf) -> Arc<S> + Send + Sync),
) {
    let p = platform.clone();
    let snapshots = match tokio::task::spawn_blocking(move || p.scan_volumes()).await {
        Ok(snaps) => snaps,
        Err(e) => {
            tracing::error!(error = ?e, "Volume scan task panicked; skipping initial scan");
            return;
        }
    };

    if snapshots.is_empty() {
        debug!("Initial volume scan found no volumes");
        return;
    }

    info!(count = snapshots.len(), "Initial volume scan");
    reconcile(volumes, &snapshots, make_store).await;

    // Probe disk usage for all volumes
    health_tick_all(volumes, platform.as_ref()).await;

    let map = volumes.read().await;
    let managed = map.values().filter(|v| v.is_managed()).count();
    let unmanaged = map.values().filter(|v| !v.is_managed()).count();
    let removable = map.values().filter(|v| v.removable).count();
    info!(managed, unmanaged, removable, "Volume scan complete");
    for vol in map.values() {
        debug!(
            path = %vol.path,
            name = %vol.display_name(),
            removable = vol.removable,
            managed = vol.is_managed(),
            state = %vol.state,
            "  volume"
        );
    }
}

// ============================================================================
// Query helpers
// ============================================================================

/// List all managed volumes.
pub async fn list_managed(volumes: &Volumes) -> Vec<Volume> {
    let map = volumes.read().await;
    map.values().filter(|v| v.is_managed()).cloned().collect()
}

/// List unmanaged removable volumes (candidates for `storage add`).
pub async fn list_candidates(volumes: &Volumes) -> Vec<Volume> {
    let map = volumes.read().await;
    map.values()
        .filter(|v| !v.is_managed() && v.removable && v.state.is_online())
        .cloned()
        .collect()
}

/// Find a managed volume by logical name.
pub async fn find_by_name(volumes: &Volumes, name: &str) -> Option<Volume> {
    let map = volumes.read().await;
    map.values()
        .find(|v| {
            v.management
                .as_ref()
                .map(|m| m.name == name)
                .unwrap_or(false)
        })
        .cloned()
}

/// Find a managed volume by storage ID (GUIDv7).
pub async fn find_by_id(volumes: &Volumes, id: &str) -> Option<Volume> {
    let map = volumes.read().await;
    map.values()
        .find(|v| v.management.as_ref().map(|m| m.id == id).unwrap_or(false))
        .cloned()
}

/// Snapshot of roles keyed by replica set display name — for beacon/broadcast callers.
pub async fn roles_snapshot(volumes: &Volumes) -> HashMap<String, StorageRole> {
    let map = volumes.read().await;
    map.values()
        .filter_map(|v| {
            v.management
                .as_ref()
                .map(|m| (m.display_name().to_string(), m.role))
        })
        .collect()
}

/// Snapshot of pins keyed by replica set display name — for beacon/broadcast callers.
pub async fn pins_snapshot(volumes: &Volumes) -> HashMap<String, String> {
    let map = volumes.read().await;
    map.values()
        .filter_map(|v| {
            v.management.as_ref().and_then(|m| {
                m.pin
                    .as_ref()
                    .map(|p| (m.display_name().to_string(), p.pin_id.clone()))
            })
        })
        .collect()
}

/// Snapshot of (name, id) pairs for signpost generation.
pub async fn name_id_pairs(volumes: &Volumes) -> Vec<(String, String)> {
    let map = volumes.read().await;
    map.values()
        .filter_map(|v| {
            v.management
                .as_ref()
                .map(|m| (m.name.clone(), m.id.clone()))
        })
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::storage::ContentStore;

    fn test_store_factory() -> impl Fn(PathBuf) -> Arc<ContentStore> {
        |path| Arc::new(ContentStore::new(path, None))
    }

    fn make_snapshot(path: &str, mount: &str, removable: bool) -> VolumeSnapshot {
        VolumeSnapshot {
            path: path.to_string(),
            mount_path: mount.to_string(),
            label: Some("TEST".to_string()),
            capacity_bytes: 64_000_000_000,
            removable,
        }
    }

    #[test]
    fn test_volume_from_snapshot() {
        let snap = make_snapshot("/dev/sdb1", "/mnt/usb", true);
        let vol = Volume::from_snapshot(&snap);

        assert_eq!(vol.path, "/dev/sdb1");
        assert!(vol.removable);
        assert_eq!(vol.state, VolumeState::Online);
        assert!(!vol.is_managed());
        assert_eq!(vol.display_name(), "TEST"); // label takes priority over path
    }

    #[test]
    fn test_volume_state() {
        assert!(VolumeState::Online.is_online());
        assert!(VolumeState::Degraded("test".into()).is_online());
        assert!(!VolumeState::Offline.is_online());
    }

    #[tokio::test]
    async fn test_reconcile_adds_new_volumes() {
        let volumes = new_volumes();
        let snaps = vec![make_snapshot("/dev/sdb1", "/mnt/usb", true)];

        reconcile(&volumes, &snaps, &test_store_factory()).await;

        let map = volumes.read().await;
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("/dev/sdb1"));
    }

    #[tokio::test]
    async fn test_reconcile_marks_departed_offline() {
        let volumes = new_volumes();

        // Add a volume
        let snaps = vec![make_snapshot("/dev/sdb1", "/mnt/usb", true)];
        reconcile(&volumes, &snaps, &test_store_factory()).await;

        // Reconcile with empty → should mark offline
        reconcile(&volumes, &[], &test_store_factory()).await;

        let map = volumes.read().await;
        let vol = map.get("/dev/sdb1").unwrap();
        assert_eq!(vol.state, VolumeState::Offline);
    }

    #[tokio::test]
    async fn test_list_candidates() {
        let volumes = new_volumes();
        let snaps = vec![
            make_snapshot("/dev/sdb1", "/mnt/usb", true), // removable
            make_snapshot("/dev/sda2", "/mnt/data", false), // fixed
        ];
        reconcile(&volumes, &snaps, &test_store_factory()).await;

        let candidates = list_candidates(&volumes).await;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, "/dev/sdb1");
    }

    #[tokio::test]
    async fn test_new_volumes_is_empty() {
        let volumes = new_volumes();
        let map = volumes.read().await;
        assert!(map.is_empty());
    }

    #[test]
    fn test_allowed_mount_paths() {
        assert!(super::super::analysis::is_allowed_mount("/mnt/usb"));
        assert!(super::super::analysis::is_allowed_mount("/media/user/USB"));
        assert!(super::super::analysis::is_allowed_mount(
            "/run/media/user/USB"
        ));
        assert!(!super::super::analysis::is_allowed_mount("/home/user/usb"));
        assert!(!super::super::analysis::is_allowed_mount("/var/lib/data"));
    }
}
