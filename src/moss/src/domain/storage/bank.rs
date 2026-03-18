//! Storage bank (STORAGE-0014)
//!
//! Single entry point for physical storage events entering the domain.
//! Called by the platform monitor after it has detected AND measured a volume.

use std::path::PathBuf;
use std::sync::Arc;

use garden_common::storage::StorageChanged;
use tracing::{debug, info};

use crate::domain::traits::ManagementStoreOps;

use super::{Volume, VolumeSnapshot, VolumeState, Volumes};

/// Domain bridge for physical storage events.
///
/// The monitor detects and measures; the bank classifies, registers,
/// and emits domain events.
pub struct StorageBank {
    volumes: Volumes,
    changed: tokio::sync::broadcast::Sender<StorageChanged>,
    make_store: Box<dyn Fn(PathBuf) -> Arc<dyn ManagementStoreOps> + Send + Sync>,
}

impl StorageBank {
    pub fn new(
        volumes: Volumes,
        changed: tokio::sync::broadcast::Sender<StorageChanged>,
        make_store: impl Fn(PathBuf) -> Arc<dyn ManagementStoreOps> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            volumes,
            changed,
            make_store: Box::new(make_store),
        })
    }

    /// Called by the platform monitor after it has detected AND measured a volume.
    ///
    /// Classifies via manifest, upserts into volumes map, emits BankConnected.
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

        let mut map = self.volumes.write().await;
        if !map.contains_key(&snap.path) {
            let mut vol = Volume::from_snapshot(&snap);
            vol.used_bytes = used_bytes;
            // Drop lock before async classify
            drop(map);

            vol.classify(&self.make_store).await;

            let mut map = self.volumes.write().await;
            if vol.is_managed() {
                let name = vol.display_name().to_string();
                let roles = vol
                    .management
                    .as_ref()
                    .map(|m| m.roles.clone())
                    .unwrap_or_default();
                let manifest_id = vol
                    .management
                    .as_ref()
                    .map(|m| m.id.clone())
                    .unwrap_or_default();

                // Manifest-based dedup: same manifest_id at different path
                let stale_key = map
                    .iter()
                    .find(|(k, v)| {
                        *k != &snap.path
                            && v.management
                                .as_ref()
                                .map(|m| m.id == manifest_id)
                                .unwrap_or(false)
                    })
                    .map(|(k, _)| k.clone());

                if let Some(old_key) = stale_key {
                    info!(
                        old_path = %old_key,
                        new_path = %snap.path,
                        name = %name,
                        "Volume re-keyed on appear (device path changed)"
                    );
                    map.remove(&old_key);
                } else {
                    info!(path = %snap.path, name = %name, "Managed volume appeared");
                }

                map.insert(snap.path, vol);
                let _ = self.changed.send(StorageChanged::Connected {
                    name,
                    roles,
                    used_bytes,
                    capacity_bytes,
                });
            } else {
                debug!(path = %snap.path, "Unmanaged volume appeared");
                map.insert(snap.path, vol);
            }
        } else {
            // Re-appeared — mark online, update metrics
            if let Some(vol) = map.get_mut(&snap.path) {
                vol.state = VolumeState::Online;
                vol.capacity_bytes = capacity_bytes;
                vol.used_bytes = used_bytes;
                vol.mount_path = mount_path;
                info!(path = %snap.path, name = %vol.display_name(), "Volume came back online");
                if vol.is_managed() {
                    let name = vol.display_name().to_string();
                    let roles = vol
                        .management
                        .as_ref()
                        .map(|m| m.roles.clone())
                        .unwrap_or_default();
                    let _ = self.changed.send(StorageChanged::Connected {
                        name,
                        roles,
                        used_bytes,
                        capacity_bytes,
                    });
                }
            }
        }
    }

    /// Called by the platform monitor on device removal.
    pub async fn on_vanished(&self, path: String) {
        let mut map = self.volumes.write().await;
        if let Some(vol) = map.get_mut(&path) {
            info!(path = %path, name = %vol.display_name(), "Volume disappeared");
            let was_managed = vol.is_managed();
            let name = vol.display_name().to_string();
            vol.state = VolumeState::Offline;
            if was_managed {
                let _ = self.changed.send(StorageChanged::Released { name });
                let _ = self.changed.send(StorageChanged::Reclassified);
            }
        }
    }
}
