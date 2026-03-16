//! Medium — physical disk layer (host-only).
//!
//! A `Medium` represents a physical storage device (disk), not a partition.
//! Host-only — never broadcast to the garden. Used for device candidate
//! discovery and condition reporting.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, info};

use super::platform_types::{BusType, MediumCondition, MediumSnapshot, PartitionSnapshot};
use super::volume::Volume;

/// A physical storage medium known to this stone.
///
/// Host-only — never broadcast to the garden. Used for:
/// - Detecting new physical devices (even without partitions)
/// - Categorizing device condition for the user
/// - Determining actions needed (partition, format, prepare)
/// - Showing in `rake storage candidates`
#[derive(Debug, Clone)]
pub struct Medium {
    /// OS device identifier (e.g., `\\.\PhysicalDrive2`, `/dev/sdb`).
    pub device_id: String,
    /// Vendor/model name.
    pub model: Option<String>,
    /// Serial number.
    pub serial: Option<String>,
    /// Physical bus type.
    pub bus_type: BusType,
    /// Total size in bytes.
    pub size_bytes: u64,
    /// Whether the medium is external/removable.
    pub removable: bool,
    /// Physical condition.
    pub condition: MediumCondition,
    /// Partitions on this medium.
    pub partitions: Vec<PartitionSnapshot>,
}

impl Medium {
    /// Construct from an OS snapshot.
    pub fn from_snapshot(snap: &MediumSnapshot) -> Self {
        Self {
            device_id: snap.device_id.clone(),
            model: snap.model.clone(),
            serial: snap.serial.clone(),
            bus_type: snap.bus_type,
            size_bytes: snap.size_bytes,
            removable: snap.removable,
            condition: snap.condition,
            partitions: snap.partitions.clone(),
        }
    }

    /// Human-readable summary: model or device_id.
    pub fn display_name(&self) -> &str {
        self.model.as_deref().unwrap_or(&self.device_id)
    }

    /// Whether any partition on this medium has a mounted filesystem.
    pub fn has_mounted_space(&self) -> bool {
        self.partitions.iter().any(|p| p.mount_path.is_some())
    }

    /// Whether any partition on this medium matches a managed Volume.
    pub fn has_managed_space(&self, volumes: &HashMap<String, Volume>) -> bool {
        self.partitions.iter().any(|p| {
            p.mount_path.as_ref().is_some_and(|mp| {
                volumes.values().any(|v| {
                    v.is_managed() && v.mount_path.to_string_lossy() == mp.as_str()
                })
            })
        })
    }
}

/// Media collection — keyed by device_id. Host-only.
pub type Media = Arc<RwLock<HashMap<String, Medium>>>;

/// Create an empty `Media` map.
pub fn new_media() -> Media {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Reconcile the Media map against a fresh set of OS snapshots.
pub async fn reconcile_media(media: &Media, snapshots: &[MediumSnapshot]) {
    let current_ids: std::collections::HashSet<&str> =
        snapshots.iter().map(|s| s.device_id.as_str()).collect();

    let mut map = media.write().await;

    // Update existing and add new
    for snap in snapshots {
        if let Some(m) = map.get_mut(&snap.device_id) {
            // Update in place
            m.condition = snap.condition;
            m.partitions = snap.partitions.clone();
            m.size_bytes = snap.size_bytes;
        } else {
            let m = Medium::from_snapshot(snap);
            if m.removable {
                info!(
                    device = %m.device_id,
                    model = %m.display_name(),
                    bus = %m.bus_type,
                    condition = %m.condition,
                    partitions = m.partitions.len(),
                    "Medium detected"
                );
            } else {
                debug!(
                    device = %m.device_id,
                    model = %m.display_name(),
                    "Internal medium detected"
                );
            }
            map.insert(snap.device_id.clone(), m);
        }
    }

    // Remove departed media
    let departed: Vec<String> = map
        .keys()
        .filter(|k| !current_ids.contains(k.as_str()))
        .cloned()
        .collect();

    for id in departed {
        if let Some(m) = map.remove(&id) {
            info!(device = %id, model = %m.display_name(), "Medium departed");
        }
    }
}
