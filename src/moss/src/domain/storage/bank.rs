//! Storage bank (STORAGE-0014, STORAGE-0017)
//!
//! Domain bridge for physical storage events. Routes OS facts into Volume
//! domain objects and forwards the returned events to the broadcast channel.

use std::path::PathBuf;
use std::sync::Arc;

use tracing::{debug, info};

use crate::domain::traits::ManagementStoreOps;

use super::volume::DiskMetrics;
use super::{Volume, VolumeSnapshot, Volumes};

/// Domain bridge for physical storage events.
///
/// The monitor detects and measures; the bank routes facts to Volume objects
/// and forwards whatever events Volume returns.
pub struct StorageBank<S: ManagementStoreOps + 'static = crate::infra::storage::ContentStore> {
    volumes: Volumes,
    changed: tokio::sync::broadcast::Sender<garden_common::storage::StorageChanged>,
    make_store: Box<dyn Fn(PathBuf) -> Arc<S> + Send + Sync>,
}

impl<S: ManagementStoreOps + 'static> StorageBank<S> {
    pub fn new(
        volumes: Volumes,
        changed: tokio::sync::broadcast::Sender<garden_common::storage::StorageChanged>,
        make_store: impl Fn(PathBuf) -> Arc<S> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            volumes,
            changed,
            make_store: Box::new(make_store),
        })
    }

    /// Forward events returned by a Volume method to the broadcast channel.
    fn emit(&self, events: Vec<garden_common::storage::StorageChanged>) {
        for event in events {
            let _ = self.changed.send(event);
        }
    }

    /// Called by the platform monitor after it has detected AND measured a volume.
    pub async fn on_appeared(
        &self,
        device_path: String,
        mount_path: PathBuf,
        label: Option<String>,
        capacity_bytes: u64,
        used_bytes: u64,
        removable: bool,
    ) {
        let snap = VolumeSnapshot {
            path: device_path,
            mount_path: mount_path.to_string_lossy().to_string(),
            label,
            capacity_bytes,
            removable,
        };
        let metrics = DiskMetrics {
            capacity_bytes,
            used_bytes,
        };

        let mut map = self.volumes.write().await;
        if !map.contains_key(&snap.path) {
            // New volume — classify first (requires async I/O, drop lock).
            let mut vol = Volume::from_snapshot(&snap);
            drop(map);

            vol.classify(&self.make_store).await;

            // Re-acquire lock, dedup by manifest ID, insert.
            let mut map = self.volumes.write().await;

            if vol.is_managed() {
                let name = vol.display_name().to_string();
                let manifest_id = vol.manifest_id().unwrap_or_default().to_string();

                // Manifest-based dedup: same manifest_id at different device path.
                let stale_key = map
                    .iter()
                    .find(|(k, v)| {
                        *k != &snap.path
                            && v.manifest_id()
                                .is_some_and(|id| id == manifest_id)
                    })
                    .map(|(k, _)| k.clone());

                if let Some(old_key) = stale_key {
                    info!(old_path = %old_key, new_path = %snap.path, name = %name,
                          "Volume re-keyed on appear (device path changed)");
                    map.remove(&old_key);
                } else {
                    info!(path = %snap.path, name = %name, "Managed volume appeared");
                }

                let events = vol.connect(metrics);
                map.insert(snap.path, vol);
                self.emit(events);
            } else {
                debug!(path = %snap.path, "Unmanaged volume appeared");
                map.insert(snap.path, vol);
            }
        } else {
            // Re-appeared — inform the Volume, let it decide.
            if let Some(vol) = map.get_mut(&snap.path) {
                vol.update_os_metadata(mount_path, snap.label.clone());
                let events = vol.connect(metrics);
                if !events.is_empty() {
                    info!(path = %snap.path, name = %vol.display_name(), "Volume came back online");
                }
                self.emit(events);
            }
        }
    }

    /// Called by the platform monitor on device removal.
    pub async fn on_vanished(&self, path: String) {
        let mut map = self.volumes.write().await;
        if let Some(vol) = map.get_mut(&path) {
            let events = vol.disconnect();
            if !events.is_empty() {
                info!(path = %path, name = %vol.display_name(), "Volume disappeared");
            }
            self.emit(events);
        }
    }
}
