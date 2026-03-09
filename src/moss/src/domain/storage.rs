//! Unified storage domain (STORAGE-0011)
//!
//! Single source of truth for all storage lifecycle on this stone.
//! Receives platform-agnostic [`VolumeEvent`]s from the OS adapter,
//! classifies volumes as managed or unmanaged, and owns the full pipeline:
//!
//! 1. **Classify** — check for `.zen-garden/manifest.json`
//! 2. **Register** — insert into the unified `Volumes` map
//! 3. **Health** — probe capacity and mount liveness
//! 4. **Orchestrate** — resolve Primary/Dormant roles
//! 5. **Publish** — build beacon data for UDP broadcast
//! 6. **Deregister** — remove on volume disappearance
//!
//! ## Data model
//!
//! [`Volume`] is the universal entity. Whether Zen Garden manages it is
//! an attribute (`management: Option<Management>`), not a separate type.
//!
//! [`Volumes`] is the single collection, keyed by device path.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use garden_common::storage::{
    StorageInfo, StorageManifest, StorageRole, StorageSummary, StorageVisibility,
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::infra::storage::platform::{self, VolumeSnapshot};
use crate::infra::storage::ContentStore;

// ============================================================================
// Volume health
// ============================================================================

/// Physical health of a volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeHealth {
    /// Mounted and responsive (capacity > 0).
    Healthy,
    /// Mounted but showing issues.
    Degraded(String),
    /// Mount point exists but device detached.
    Unmounted,
    /// Device is no longer visible.
    Lost,
}

impl VolumeHealth {
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded(_))
    }
}

impl std::fmt::Display for VolumeHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded(r) => write!(f, "degraded: {}", r),
            Self::Unmounted => write!(f, "unmounted"),
            Self::Lost => write!(f, "lost"),
        }
    }
}

// ============================================================================
// Pin state
// ============================================================================

/// Persisted pin state (Primary claim with GUIDv7 ordering).
#[derive(Debug, Clone)]
pub struct PinState {
    pub pin_id: String,
}

// ============================================================================
// Management — domain enrichment for managed volumes
// ============================================================================

/// Domain state for a volume that Zen Garden manages.
///
/// Present when the volume has a valid `.zen-garden/manifest.json`.
#[derive(Debug, Clone)]
pub struct Management {
    /// Unique GUIDv7 per physical device.
    pub id: String,
    /// First 8 hex chars of `id`.
    pub short_id: String,
    /// Logical storage name (FQN). Shared across replicas.
    pub name: String,
    /// Whether content is encrypted.
    pub encrypted: bool,
    /// Whether this device was not originally created on this stone.
    pub roaming: bool,
    /// Stone that created this storage.
    pub origin_stone: String,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Visibility setting.
    pub visibility: StorageVisibility,
    /// Composable roles (e.g., `["seed-bank"]`).
    pub roles: Vec<String>,
    /// Runtime role (Primary / Dormant).
    pub role: StorageRole,
    /// Pin state — `Some` means Primary role is locked.
    pub pin: Option<PinState>,
    /// I/O chokepoint (filesystem reads/writes, encryption).
    pub store: ContentStore,
}

// ============================================================================
// Volume — the universal entity
// ============================================================================

/// A storage volume known to this stone.
///
/// Represents any accessible storage: USB drive, NAS mount, local directory.
/// Whether Zen Garden manages it is determined by `management`.
#[derive(Debug, Clone)]
pub struct Volume {
    // --- From OS adapter ---
    /// Device identifier: `/dev/sdb1` on Linux, `E:\` on Windows.
    pub path: String,
    /// Where the volume's content is accessible.
    pub mount_path: PathBuf,
    /// Filesystem label.
    pub label: Option<String>,
    /// Total capacity in bytes.
    pub capacity_bytes: u64,
    /// Used space in bytes.
    pub used_bytes: u64,
    /// Whether the OS considers this removable.
    pub removable: bool,
    /// Whether the volume is currently accessible.
    pub online: bool,
    /// Physical health.
    pub health: VolumeHealth,

    // --- Domain enrichment ---
    /// Management state. `Some` = Zen Garden manages this volume.
    pub management: Option<Management>,
}

impl Volume {
    /// Whether Zen Garden manages this volume.
    pub fn is_managed(&self) -> bool {
        self.management.is_some()
    }

    /// Human-readable name: management name, or label, or path.
    pub fn display_name(&self) -> &str {
        if let Some(ref m) = self.management {
            &m.name
        } else if let Some(ref l) = self.label {
            l
        } else {
            &self.path
        }
    }

    /// Construct from an OS snapshot. Management is set later by `classify()`.
    pub fn from_snapshot(snap: &VolumeSnapshot) -> Self {
        Self {
            path: snap.path.clone(),
            mount_path: PathBuf::from(&snap.mount_path),
            label: snap.label.clone(),
            capacity_bytes: snap.capacity_bytes,
            used_bytes: 0,
            removable: snap.removable,
            online: true,
            health: VolumeHealth::Healthy,
            management: None,
        }
    }

    /// Classify this volume by checking for `.zen-garden/manifest.json`.
    ///
    /// If found and valid, sets `management`. Otherwise leaves it `None`.
    pub async fn classify(&mut self) {
        let manifest_path = self.mount_path.join(".zen-garden").join("manifest.json");

        let content = match tokio::fs::read_to_string(&manifest_path).await {
            Ok(c) => c,
            Err(_) => return, // no manifest → unmanaged
        };

        let manifest: StorageManifest = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    path = %self.path,
                    error = %e,
                    "Found .zen-garden/manifest.json but failed to parse"
                );
                return;
            }
        };

        let stone_name = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let roaming = manifest.origin_stone != stone_name;

        let store = ContentStore::new(self.mount_path.clone(), None);

        // Load persisted pin from disk
        let pin = match store.read_pin().await {
            Some(pin_id) => {
                info!(name = %manifest.name, pin_id = %pin_id, "Loaded persisted pin");
                Some(PinState { pin_id })
            }
            None => None,
        };

        let short_id = StorageInfo::short_id(&manifest.id);

        self.management = Some(Management {
            id: manifest.id.clone(),
            short_id,
            name: manifest.name.clone(),
            encrypted: manifest.encrypted,
            roaming,
            origin_stone: manifest.origin_stone.clone(),
            created_at: manifest.created_at,
            visibility: manifest.visibility,
            roles: manifest.roles.clone(),
            role: StorageRole::default(),
            pin,
            store,
        });
    }

    /// Run a health tick — probe capacity and mount liveness.
    pub fn health_tick(&mut self) {
        let mount_str = self.mount_path.to_string_lossy().to_string();

        match platform::disk_usage(&mount_str) {
            Some(usage) => {
                if usage.total() == 0 {
                    self.health = VolumeHealth::Degraded("zero capacity".into());
                } else {
                    self.capacity_bytes = usage.total();
                    self.used_bytes = usage.used_bytes;
                    self.online = true;
                    self.health = VolumeHealth::Healthy;
                }
            }
            None => {
                // Check if mount path still exists
                if self.mount_path.exists() {
                    self.health = VolumeHealth::Degraded("capacity probe failed".into());
                } else {
                    self.health = VolumeHealth::Lost;
                    self.online = false;
                }
            }
        }

        // Reconcile pin state from disk (detect external changes)
        if self.health.is_usable() {
            if let Some(ref mut mgmt) = self.management {
                // We can't do async here, so we do a blocking pin read.
                // Pin files are tiny (<100 bytes), blocking is acceptable.
                let pin_path = self.mount_path.join(".zen-garden").join("pin.json");
                let disk_pin = std::fs::read_to_string(&pin_path)
                    .ok()
                    .and_then(|content| {
                        serde_json::from_str::<serde_json::Value>(&content).ok()
                    })
                    .and_then(|v| v.get("pin_id")?.as_str().map(|s| s.to_string()));

                match (&mgmt.pin, &disk_pin) {
                    (Some(current), Some(on_disk)) if current.pin_id != *on_disk => {
                        debug!(name = %mgmt.name, "Pin reconciliation: adopting disk version");
                        mgmt.pin = Some(PinState {
                            pin_id: on_disk.clone(),
                        });
                    }
                    (None, Some(on_disk)) => {
                        debug!(name = %mgmt.name, "Pin reconciliation: adopting disk pin");
                        mgmt.pin = Some(PinState {
                            pin_id: on_disk.clone(),
                        });
                    }
                    (Some(_), None) => {
                        debug!(name = %mgmt.name, "Pin reconciliation: clearing memory pin");
                        mgmt.pin = None;
                    }
                    _ => {}
                }
            }
        }
    }

    // ========================================================================
    // Pin operations
    // ========================================================================

    /// Pin this volume as Primary. Writes pin.json, updates role.
    pub async fn pin(&mut self) -> anyhow::Result<String> {
        let mgmt = self
            .management
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Cannot pin an unmanaged volume"))?;

        let pin_id = uuid::Uuid::now_v7().to_string();
        mgmt.store.write_pin(&pin_id).await?;
        mgmt.pin = Some(PinState {
            pin_id: pin_id.clone(),
        });
        mgmt.role = StorageRole::Primary;

        info!(name = %mgmt.name, pin_id = %pin_id, "Volume pinned — claiming Primary");
        Ok(pin_id)
    }

    /// Unpin, returning to normal orchestration.
    pub async fn unpin(&mut self) -> anyhow::Result<Option<String>> {
        let mgmt = self
            .management
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Cannot unpin an unmanaged volume"))?;

        let old = mgmt.pin.take();
        if let Some(ref ps) = old {
            if let Err(e) = mgmt.store.delete_pin().await {
                warn!(name = %mgmt.name, error = %e, "Failed to delete pin file");
            }
            info!(name = %mgmt.name, pin_id = %ps.pin_id, "Volume unpinned");
        }
        Ok(old.map(|ps| ps.pin_id))
    }

    /// Whether the Primary role is pinned.
    pub fn is_pinned(&self) -> bool {
        self.management
            .as_ref()
            .map(|m| m.pin.is_some())
            .unwrap_or(false)
    }

    /// The pin_id if pinned.
    pub fn pin_id(&self) -> Option<&str> {
        self.management
            .as_ref()
            .and_then(|m| m.pin.as_ref())
            .map(|ps| ps.pin_id.as_str())
    }

    // ========================================================================
    // Snapshot last-known-good
    // ========================================================================

    /// Snapshot critical files to `last-known-good/`.
    pub async fn snapshot_lkg(&self) {
        if let Some(ref mgmt) = self.management {
            if let Err(e) = mgmt.store.snapshot_lkg().await {
                warn!(name = %mgmt.name, error = %e, "Failed to snapshot LKG");
            }
        }
    }

    // ========================================================================
    // Projections for API / beacon compatibility
    // ========================================================================

    /// Project to `StorageInfo` (for API responses and beacon construction).
    pub fn to_storage_info(&self) -> Option<StorageInfo> {
        let mgmt = self.management.as_ref()?;
        Some(StorageInfo::new(
            mgmt.id.clone(),
            mgmt.name.clone(),
            self.path.clone(),
            self.mount_path.to_string_lossy().to_string(),
            self.capacity_bytes,
            self.used_bytes,
            mgmt.visibility,
            false, // btrfs detection not needed here
            mgmt.origin_stone.clone(),
            mgmt.created_at,
            mgmt.roaming,
            self.online && self.health.is_usable(),
            mgmt.encrypted,
            mgmt.roles.clone(),
        ))
    }

    /// Build a `StorageSummary` for CLI display.
    pub fn to_summary(&self, stone_name: Option<&str>) -> Option<StorageSummary> {
        let mgmt = self.management.as_ref()?;
        let info = self.to_storage_info()?;
        Some(StorageSummary::from_info(
            &info,
            mgmt.role,
            mgmt.pin.is_some(),
            stone_name,
        ))
    }
}

// ============================================================================
// Medium — physical disk layer (host-only)
// ============================================================================

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
    pub bus_type: platform::BusType,
    /// Total size in bytes.
    pub size_bytes: u64,
    /// Whether the medium is external/removable.
    pub removable: bool,
    /// Physical condition.
    pub condition: platform::MediumCondition,
    /// Partitions on this medium.
    pub partitions: Vec<platform::PartitionSnapshot>,
}

impl Medium {
    /// Construct from an OS snapshot.
    pub fn from_snapshot(snap: &platform::MediumSnapshot) -> Self {
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
pub async fn reconcile_media(media: &Media, snapshots: &[platform::MediumSnapshot]) {
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

// ============================================================================
// Volumes collection
// ============================================================================

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
pub async fn reconcile(volumes: &Volumes, snapshots: &[VolumeSnapshot]) {
    let current_paths: std::collections::HashSet<&str> =
        snapshots.iter().map(|s| s.path.as_str()).collect();

    let mut map = volumes.write().await;

    // Update existing and add new
    for snap in snapshots {
        if let Some(vol) = map.get_mut(&snap.path) {
            // Update from latest snapshot
            vol.capacity_bytes = snap.capacity_bytes;
            vol.label = snap.label.clone();
            vol.online = true;
            if vol.health == VolumeHealth::Lost || vol.health == VolumeHealth::Unmounted {
                vol.health = VolumeHealth::Healthy;
            }
            // Re-classify unmanaged volumes — they may have gained a manifest
            // since last scan (e.g. `storage add` wrote one).
            if !vol.is_managed() {
                vol.classify().await;
                if vol.is_managed() {
                    info!(path = %snap.path, name = %vol.display_name(), "Volume became managed");
                }
            }
        } else {
            // New volume — classify
            let mut vol = Volume::from_snapshot(snap);
            vol.classify().await;

            if vol.is_managed() {
                let name = vol.display_name().to_string();
                info!(path = %snap.path, name = %name, "Managed volume registered");
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
            if vol.online {
                info!(path = %path, name = %vol.display_name(), "Volume went offline");
                vol.online = false;
                vol.health = VolumeHealth::Lost;
            }
        }
    }
}

/// Ingest a single volume event (from the watcher).
pub async fn ingest_event(volumes: &Volumes, event: platform::VolumeEvent) {
    match event {
        platform::VolumeEvent::Appeared(snap) => {
            let mut map = volumes.write().await;
            if !map.contains_key(&snap.path) {
                let mut vol = Volume::from_snapshot(&snap);
                // Drop the lock before async classify
                drop(map);

                vol.classify().await;

                let mut map = volumes.write().await;
                if vol.is_managed() {
                    info!(path = %snap.path, name = %vol.display_name(), "Managed volume appeared");
                } else {
                    debug!(path = %snap.path, "Unmanaged volume appeared");
                }
                map.insert(snap.path, vol);
            } else {
                // Re-appeared — mark online
                if let Some(vol) = map.get_mut(&snap.path) {
                    vol.online = true;
                    vol.health = VolumeHealth::Healthy;
                    vol.capacity_bytes = snap.capacity_bytes;
                    info!(path = %snap.path, name = %vol.display_name(), "Volume came back online");
                }
            }
        }
        platform::VolumeEvent::Disappeared { path } => {
            let mut map = volumes.write().await;
            if let Some(vol) = map.get_mut(&path) {
                info!(path = %path, name = %vol.display_name(), "Volume disappeared");
                vol.online = false;
                vol.health = VolumeHealth::Lost;
            }
        }
    }
}

/// Run health ticks on all volumes.
pub async fn health_tick_all(volumes: &Volumes) {
    let mut map = volumes.write().await;
    for vol in map.values_mut() {
        if vol.online {
            vol.health_tick();
        }
    }
}

/// Initial scan: enumerate OS volumes, classify, populate the map.
pub async fn initial_scan(volumes: &Volumes) {
    let snapshots = tokio::task::spawn_blocking(platform::scan_volumes)
        .await
        .unwrap_or_default();

    if snapshots.is_empty() {
        debug!("Initial volume scan found no volumes");
        return;
    }

    info!(count = snapshots.len(), "Initial volume scan");
    reconcile(volumes, &snapshots).await;

    // Probe disk usage for all volumes
    health_tick_all(volumes).await;

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
            online = vol.online,
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
        .filter(|v| !v.is_managed() && v.removable && v.online)
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
        .find(|v| {
            v.management
                .as_ref()
                .map(|m| m.id == id)
                .unwrap_or(false)
        })
        .cloned()
}

/// Snapshot of roles keyed by storage name — for beacon/broadcast callers.
pub async fn roles_snapshot(volumes: &Volumes) -> HashMap<String, StorageRole> {
    let map = volumes.read().await;
    map.values()
        .filter_map(|v| {
            v.management
                .as_ref()
                .map(|m| (m.name.clone(), m.role))
        })
        .collect()
}

/// Snapshot of pins keyed by storage name — for beacon/broadcast callers.
pub async fn pins_snapshot(volumes: &Volumes) -> HashMap<String, String> {
    let map = volumes.read().await;
    map.values()
        .filter_map(|v| {
            v.management.as_ref().and_then(|m| {
                m.pin.as_ref().map(|p| (m.name.clone(), p.pin_id.clone()))
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
        assert!(vol.online);
        assert!(!vol.is_managed());
        assert_eq!(vol.display_name(), "TEST"); // label takes priority over path
    }

    #[test]
    fn test_volume_health() {
        assert!(VolumeHealth::Healthy.is_usable());
        assert!(VolumeHealth::Degraded("test".into()).is_usable());
        assert!(!VolumeHealth::Unmounted.is_usable());
        assert!(!VolumeHealth::Lost.is_usable());
    }

    #[tokio::test]
    async fn test_reconcile_adds_new_volumes() {
        let volumes = new_volumes();
        let snaps = vec![make_snapshot("/dev/sdb1", "/mnt/usb", true)];

        reconcile(&volumes, &snaps).await;

        let map = volumes.read().await;
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("/dev/sdb1"));
    }

    #[tokio::test]
    async fn test_reconcile_marks_departed_offline() {
        let volumes = new_volumes();

        // Add a volume
        let snaps = vec![make_snapshot("/dev/sdb1", "/mnt/usb", true)];
        reconcile(&volumes, &snaps).await;

        // Reconcile with empty → should mark offline
        reconcile(&volumes, &[]).await;

        let map = volumes.read().await;
        let vol = map.get("/dev/sdb1").unwrap();
        assert!(!vol.online);
        assert_eq!(vol.health, VolumeHealth::Lost);
    }

    #[tokio::test]
    async fn test_list_candidates() {
        let volumes = new_volumes();
        let snaps = vec![
            make_snapshot("/dev/sdb1", "/mnt/usb", true),  // removable
            make_snapshot("/dev/sda2", "/mnt/data", false), // fixed
        ];
        reconcile(&volumes, &snaps).await;

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
}
