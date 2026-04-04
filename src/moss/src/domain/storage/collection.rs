//! Volumes collection — the unified map of all local storage (STORAGE-0017).
//!
//! Single source of truth for all local storage state. Operations:
//! reconcile (tick), initial scan, health observation, and query helpers.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use garden_common::storage::{StorageChanged, StorageRole};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::domain::traits::{ManagementStoreOps, StoragePlatform};

use super::platform_types::VolumeSnapshot;
use super::volume::{DiskMetrics, Volume, VolumeState};

/// The unified volume collection — keyed by device path.
pub type Volumes = Arc<RwLock<HashMap<String, Volume>>>;

/// Create an empty `Volumes` map.
pub fn new_volumes() -> Volumes {
    Arc::new(RwLock::new(HashMap::new()))
}

// ============================================================================
// Domain operations
// ============================================================================

/// Reconcile the Volumes map against a fresh set of OS snapshots.
///
/// 1. New snapshots → classify and insert
/// 2. Existing snapshots → inform via `connect()` + `update_os_metadata()`
/// 3. Missing snapshots → inform via `disconnect()`
pub async fn reconcile<S: ManagementStoreOps + 'static>(
    volumes: &Volumes,
    snapshots: &[VolumeSnapshot],
    make_store: &(dyn Fn(PathBuf) -> Arc<S> + Send + Sync),
) -> Vec<StorageChanged> {
    let current_paths: std::collections::HashSet<&str> =
        snapshots.iter().map(|s| s.path.as_str()).collect();

    let mut map = volumes.write().await;
    let mut events = Vec::new();

    // Update existing and add new
    for snap in snapshots {
        let metrics = DiskMetrics {
            capacity_bytes: snap.capacity_bytes,
            used_bytes: 0, // reconcile doesn't measure usage — health tick does
        };

        if let Some(vol) = map.get_mut(&snap.path) {
            vol.update_os_metadata(PathBuf::from(&snap.mount_path), snap.label.clone());
            events.extend(vol.connect(metrics));

            // Re-classify unmanaged volumes — they may have gained a manifest.
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
                let manifest_id = vol.manifest_id().unwrap_or_default().to_string();

                // Manifest-based dedup: same device at different path.
                let stale_key = map
                    .iter()
                    .find(|(k, v)| {
                        *k != &snap.path
                            && v.manifest_id().is_some_and(|id| id == manifest_id)
                    })
                    .map(|(k, _)| k.clone());

                if let Some(old_key) = stale_key {
                    info!(old_path = %old_key, new_path = %snap.path, name = %name,
                          "Volume re-keyed (device path changed)");
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
        if let Some(vol) = map.get_mut(&path) {
            let vol_events = vol.disconnect();
            if !vol_events.is_empty() {
                info!(path = %path, name = %vol.display_name(), "Volume went offline");
            }
            events.extend(vol_events);
        }
    }

    events
}

/// Observe health on all volumes. Returns events for any state changes.
///
/// Reads disk metrics via the platform adapter, then informs each Volume.
/// Offline volumes are skipped by `observe_metrics()`.
pub async fn observe_all(
    volumes: &Volumes,
    platform: &(impl StoragePlatform + ?Sized),
) -> Vec<StorageChanged> {
    let mut map = volumes.write().await;
    let mut events = Vec::new();

    for vol in map.values_mut() {
        if !vol.is_online() {
            continue;
        }

        let mount_str = vol.mount_path().to_string_lossy().to_string();
        let metrics = platform.disk_usage(&mount_str).map(|usage| DiskMetrics {
            capacity_bytes: usage.total(),
            used_bytes: usage.used_bytes,
        });
        events.extend(vol.observe_metrics(metrics));

        // Reconcile pin state from disk (detect external pin changes).
        let pin_path = vol.mount_path().join(".zen-garden").join("pin.json");
        let disk_pin = std::fs::read_to_string(&pin_path)
            .ok()
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
            .and_then(|v| v.get("pin_id")?.as_str().map(|s| s.to_string()));
        vol.reconcile_pin(disk_pin);
    }

    events
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
    observe_all(volumes, platform.as_ref()).await;

    let map = volumes.read().await;
    let managed = map.values().filter(|v| v.is_managed()).count();
    let unmanaged = map.values().filter(|v| !v.is_managed()).count();
    let removable = map.values().filter(|v| v.removable()).count();
    info!(managed, unmanaged, removable, "Volume scan complete");
    for vol in map.values() {
        debug!(
            path = %vol.path(),
            name = %vol.display_name(),
            removable = vol.removable(),
            managed = vol.is_managed(),
            state = %vol.state(),
            "  volume"
        );
    }
}

// ============================================================================
// Query helpers
// ============================================================================

pub async fn list_managed(volumes: &Volumes) -> Vec<Volume> {
    let map = volumes.read().await;
    map.values().filter(|v| v.is_managed()).cloned().collect()
}

pub async fn list_candidates(volumes: &Volumes) -> Vec<Volume> {
    let map = volumes.read().await;
    map.values()
        .filter(|v| !v.is_managed() && v.removable() && v.is_online())
        .cloned()
        .collect()
}

pub async fn find_by_name(volumes: &Volumes, name: &str) -> Option<Volume> {
    let map = volumes.read().await;
    map.values()
        .find(|v| {
            v.management()
                .is_some_and(|m| m.name == name)
        })
        .cloned()
}

pub async fn find_by_id(volumes: &Volumes, id: &str) -> Option<Volume> {
    let map = volumes.read().await;
    map.values()
        .find(|v| v.management().is_some_and(|m| m.id == id))
        .cloned()
}

pub async fn roles_snapshot(volumes: &Volumes) -> HashMap<String, StorageRole> {
    let map = volumes.read().await;
    map.values()
        .filter_map(|v| {
            v.management()
                .map(|m| (m.display_name().to_string(), m.role))
        })
        .collect()
}

pub async fn pins_snapshot(volumes: &Volumes) -> HashMap<String, String> {
    let map = volumes.read().await;
    map.values()
        .filter_map(|v| {
            v.management().and_then(|m| {
                m.pin
                    .as_ref()
                    .map(|p| (m.display_name().to_string(), p.pin_id.clone()))
            })
        })
        .collect()
}

pub async fn name_id_pairs(volumes: &Volumes) -> Vec<(String, String)> {
    let map = volumes.read().await;
    map.values()
        .filter_map(|v| {
            v.management()
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

        assert_eq!(vol.path(), "/dev/sdb1");
        assert!(vol.removable());
        assert_eq!(*vol.state(), VolumeState::Online);
        assert!(!vol.is_managed());
        assert_eq!(vol.display_name(), "TEST");
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

        let snaps = vec![make_snapshot("/dev/sdb1", "/mnt/usb", true)];
        reconcile(&volumes, &snaps, &test_store_factory()).await;

        reconcile(&volumes, &[], &test_store_factory()).await;

        let map = volumes.read().await;
        let vol = map.get("/dev/sdb1").unwrap();
        assert_eq!(*vol.state(), VolumeState::Offline);
    }

    #[tokio::test]
    async fn test_list_candidates() {
        let volumes = new_volumes();
        let snaps = vec![
            make_snapshot("/dev/sdb1", "/mnt/usb", true),
            make_snapshot("/dev/sda2", "/mnt/data", false),
        ];
        reconcile(&volumes, &snaps, &test_store_factory()).await;

        let candidates = list_candidates(&volumes).await;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path(), "/dev/sdb1");
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

    // ── State machine tests (STORAGE-0017) ──────────────────────────

    #[test]
    fn connect_from_offline_emits_connected() {
        let snap = make_snapshot("/dev/sdb1", "/mnt/usb", true);
        let mut vol = Volume::from_snapshot(&snap);
        // Force offline first
        let _ = vol.disconnect();

        let events = vol.connect(DiskMetrics {
            capacity_bytes: 64_000_000_000,
            used_bytes: 1_000_000,
        });
        // Unmanaged volume — no event even on Offline→Online
        assert!(events.is_empty());
    }

    #[test]
    fn connect_when_already_online_no_event() {
        let snap = make_snapshot("/dev/sdb1", "/mnt/usb", true);
        let mut vol = Volume::from_snapshot(&snap);

        let events = vol.connect(DiskMetrics {
            capacity_bytes: 100,
            used_bytes: 50,
        });
        assert!(events.is_empty());
        assert_eq!(vol.capacity_bytes(), 100);
        assert_eq!(vol.used_bytes(), 50);
    }

    #[test]
    fn disconnect_from_online_for_unmanaged() {
        let snap = make_snapshot("/dev/sdb1", "/mnt/usb", true);
        let mut vol = Volume::from_snapshot(&snap);

        let events = vol.disconnect();
        // Unmanaged — no Released event
        assert!(events.is_empty());
        assert_eq!(*vol.state(), VolumeState::Offline);
    }

    #[test]
    fn disconnect_from_offline_is_noop() {
        let snap = make_snapshot("/dev/sdb1", "/mnt/usb", true);
        let mut vol = Volume::from_snapshot(&snap);
        vol.disconnect();

        let events = vol.disconnect();
        assert!(events.is_empty());
    }

    #[test]
    fn observe_metrics_offline_is_noop() {
        let snap = make_snapshot("/dev/sdb1", "/mnt/usb", true);
        let mut vol = Volume::from_snapshot(&snap);
        vol.disconnect();

        let events = vol.observe_metrics(Some(DiskMetrics {
            capacity_bytes: 100,
            used_bytes: 50,
        }));
        assert!(events.is_empty());
        assert_eq!(*vol.state(), VolumeState::Offline);
    }

    #[test]
    fn observe_metrics_none_degrades() {
        let snap = make_snapshot("/dev/sdb1", "/mnt/usb", true);
        let mut vol = Volume::from_snapshot(&snap);

        let _events = vol.observe_metrics(None);
        assert!(matches!(vol.state(), VolumeState::Degraded(_)));
    }

    #[test]
    fn observe_metrics_zero_capacity_degrades() {
        let snap = make_snapshot("/dev/sdb1", "/mnt/usb", true);
        let mut vol = Volume::from_snapshot(&snap);

        let _events = vol.observe_metrics(Some(DiskMetrics {
            capacity_bytes: 0,
            used_bytes: 0,
        }));
        assert!(matches!(vol.state(), VolumeState::Degraded(_)));
    }

    #[test]
    fn rename_same_name_no_event() {
        let snap = make_snapshot("/dev/sdb1", "/mnt/usb", true);
        let mut vol = Volume::from_snapshot(&snap);
        // Unmanaged — rename is a no-op
        let events = vol.rename("anything".to_string());
        assert!(events.is_empty());
    }

    #[test]
    fn release_unmanaged_no_event() {
        let snap = make_snapshot("/dev/sdb1", "/mnt/usb", true);
        let mut vol = Volume::from_snapshot(&snap);
        let events = vol.release();
        assert!(events.is_empty());
    }
}
