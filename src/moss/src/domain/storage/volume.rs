//! Volume — the universal storage entity (STORAGE-0017).
//!
//! A `Volume` is the domain object for any accessible storage device
//! (USB drive, NAS mount, local directory). It owns its state and
//! decides what changed when informed of OS facts.
//!
//! ## State Machine
//!
//! ```text
//!              connect(metrics)
//!    Offline ──────────────────→ Online
//!       ↑                         ↑ ↓
//!       │ disconnect()            │ │ observe_metrics()
//!       │                         │ ↓
//!       ←──────────────────── Degraded
//!              disconnect()
//! ```
//!
//! OS facts flow in via methods. Domain events flow out as return values.
//! Volume never touches channels, Arc, or async (except pin/unpin I/O).

use std::path::PathBuf;
use std::sync::Arc;

use garden_common::storage::{
    short_id_from_guid, StorageAccess, StorageAnnouncement, StorageChanged, StorageInfo,
    StorageManifest, StorageRole, StorageSummary, StorageVisibility, DEFAULT_REPLICA_SET_DISPLAY,
};
use tracing::{debug, info, warn};

use crate::domain::traits::ManagementStoreOps;

use super::platform_types::{DeviceHealth, VolumeSnapshot};

// ============================================================================
// Volume state
// ============================================================================

/// Lifecycle state of a volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeState {
    /// Mounted, accessible, disk_usage valid.
    Online,
    /// Accessible but with issues (zero capacity, probe error).
    Degraded(String),
    /// Device gone. Only `connect()` can resurrect.
    Offline,
}

impl VolumeState {
    pub fn is_online(&self) -> bool {
        matches!(self, Self::Online | Self::Degraded(_))
    }
}

impl std::fmt::Display for VolumeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Online => write!(f, "{}", garden_common::constants::HEALTH_HEALTHY),
            Self::Degraded(r) => write!(f, "{}: {}", garden_common::constants::HEALTH_DEGRADED, r),
            Self::Offline => write!(f, "offline"),
        }
    }
}

// ============================================================================
// Disk metrics — OS measurement passed into Volume methods
// ============================================================================

/// Measured disk usage from the OS. Passed into Volume state machine methods.
/// Volume decides what to do with the information.
#[derive(Debug, Clone, Copy)]
pub struct DiskMetrics {
    pub capacity_bytes: u64,
    pub used_bytes: u64,
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
///
/// ## Identity (STORAGE-0013)
///
/// Two-level: device (`id`/`name`) + replica set (`replica_set_id`/`replica_set_name`).
#[derive(Debug, Clone)]
pub struct Management {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub replica_set_id: String,
    pub replica_set_name: String,
    pub replica_set_name_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub encrypted: bool,
    pub roaming: bool,
    pub origin_stone: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub visibility: StorageVisibility,
    pub roles: Vec<String>,
    pub role: StorageRole,
    pub pin: Option<PinState>,
}

impl Management {
    /// Replica set display name, falling back to the default when empty.
    pub fn display_name(&self) -> &str {
        if self.replica_set_name.is_empty() {
            DEFAULT_REPLICA_SET_DISPLAY
        } else {
            &self.replica_set_name
        }
    }
}

// ============================================================================
// Volume — the domain object
// ============================================================================

/// A storage volume known to this stone.
///
/// All state changes go through domain methods that enforce valid transitions
/// and return the events to emit. No direct field mutation from outside.
#[derive(Debug, Clone)]
pub struct Volume {
    // --- OS identity ---
    path: String,
    mount_path: PathBuf,
    label: Option<String>,
    capacity_bytes: u64,
    used_bytes: u64,
    removable: bool,
    state: VolumeState,

    // --- Domain enrichment ---
    management: Option<Management>,
}

// ── Getters ─────────────────────────────────────────────────────────

impl Volume {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn mount_path(&self) -> &PathBuf {
        &self.mount_path
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn capacity_bytes(&self) -> u64 {
        self.capacity_bytes
    }

    pub fn used_bytes(&self) -> u64 {
        self.used_bytes
    }

    pub fn removable(&self) -> bool {
        self.removable
    }

    pub fn state(&self) -> &VolumeState {
        &self.state
    }

    pub fn management(&self) -> Option<&Management> {
        self.management.as_ref()
    }

    pub fn management_mut(&mut self) -> Option<&mut Management> {
        self.management.as_mut()
    }

    pub fn is_managed(&self) -> bool {
        self.management.is_some()
    }

    pub fn is_online(&self) -> bool {
        self.state.is_online()
    }

    /// Human-readable name: management name → label → path.
    pub fn display_name(&self) -> &str {
        if let Some(ref m) = self.management {
            &m.name
        } else if let Some(ref l) = self.label {
            l
        } else {
            &self.path
        }
    }

    pub fn is_pinned(&self) -> bool {
        self.management
            .as_ref()
            .is_some_and(|m| m.pin.is_some())
    }

    pub fn pin_id(&self) -> Option<&str> {
        self.management
            .as_ref()
            .and_then(|m| m.pin.as_ref())
            .map(|ps| ps.pin_id.as_str())
    }
}

// ── Construction ────────────────────────────────────────────────────

impl Volume {
    /// Construct from an OS snapshot. Starts as Offline — caller must call
    /// `connect()` to transition Online and produce the Connected event.
    /// Management is set later by `classify()`.
    pub fn from_snapshot(snap: &VolumeSnapshot) -> Self {
        Self {
            path: snap.path.clone(),
            mount_path: PathBuf::from(&snap.mount_path),
            label: snap.label.clone(),
            capacity_bytes: snap.capacity_bytes,
            used_bytes: 0,
            removable: snap.removable,
            state: VolumeState::Offline,
            management: None,
        }
    }

    /// Classify this volume by checking for `.zen-garden/manifest.json`.
    ///
    /// If found and valid, sets `management`. Otherwise leaves it `None`.
    pub async fn classify<S: ManagementStoreOps>(
        &mut self,
        make_store: &(dyn Fn(PathBuf) -> Arc<S> + Send + Sync),
    ) {
        let manifest_path = self.mount_path.join(".zen-garden").join("manifest.json");

        let content = match tokio::fs::read_to_string(&manifest_path).await {
            Ok(c) => c,
            Err(_) => return,
        };

        let manifest: StorageManifest = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                warn!(path = %self.path, error = %e, "Found manifest but failed to parse");
                return;
            }
        };

        let stone_name = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let roaming = manifest.origin_stone != stone_name;

        let store = make_store(self.mount_path.clone());
        let pin = match store.read_pin().await {
            Some(pin_id) => {
                info!(name = %manifest.name, pin_id = %pin_id, "Loaded persisted pin");
                Some(PinState { pin_id })
            }
            None => None,
        };

        let short_id = short_id_from_guid(&manifest.id);

        self.management = Some(Management {
            id: manifest.id.clone(),
            short_id,
            name: manifest.name.clone(),
            replica_set_id: manifest.replica_set_id.clone(),
            replica_set_name: manifest.replica_set_name.clone(),
            replica_set_name_updated_at: manifest.replica_set_name_updated_at,
            encrypted: manifest.encrypted,
            roaming,
            origin_stone: manifest.origin_stone.clone(),
            created_at: manifest.created_at,
            visibility: manifest.visibility,
            roles: manifest.roles.clone(),
            role: StorageRole::default(),
            pin,
        });
    }

    /// Manifest ID if this volume is managed (used for dedup in StorageBank).
    pub fn manifest_id(&self) -> Option<&str> {
        self.management.as_ref().map(|m| m.id.as_str())
    }
}

// ── State machine — OS facts in, domain events out ──────────────────

impl Volume {
    /// Device appeared or reappeared. Transitions Offline → Online.
    /// If already Online/Degraded, updates metrics silently (no event).
    pub fn connect(&mut self, metrics: DiskMetrics) -> Vec<StorageChanged> {
        let was_offline = self.state == VolumeState::Offline;

        self.capacity_bytes = metrics.capacity_bytes;
        self.used_bytes = metrics.used_bytes;
        self.state = VolumeState::Online;

        if was_offline && self.is_managed() {
            let mgmt = self.management.as_ref().unwrap();
            vec![StorageChanged::Connected {
                name: mgmt.display_name().to_string(),
                roles: mgmt.roles.clone(),
                used_bytes: self.used_bytes,
                capacity_bytes: self.capacity_bytes,
            }]
        } else {
            vec![]
        }
    }

    /// Device disappeared. Transitions Online|Degraded → Offline.
    /// No-op if already Offline.
    pub fn disconnect(&mut self) -> Vec<StorageChanged> {
        if self.state == VolumeState::Offline {
            return vec![];
        }

        let was_managed = self.is_managed();
        let name = self.display_name().to_string();
        self.state = VolumeState::Offline;

        if was_managed {
            vec![
                StorageChanged::Released { name },
                StorageChanged::Reclassified,
            ]
        } else {
            vec![]
        }
    }

    /// Periodic health observation. Accepts measured disk metrics (or None
    /// if the measurement failed) and device health signals. May transition
    /// Online ↔ Degraded, or force-disconnect stale/unresponsive devices.
    /// Never touches Offline volumes — only `connect()` can resurrect.
    pub fn observe_metrics(
        &mut self,
        metrics: Option<DiskMetrics>,
        health: DeviceHealth,
    ) -> Vec<StorageChanged> {
        if self.state == VolumeState::Offline {
            return vec![];
        }

        // Stale or unresponsive device → force disconnect (STORAGE-0018).
        // This catches ghost devices the VolumeMonitor missed.
        if health.stale_reference {
            warn!(path = %self.path, "Device has stale kernel reference — forcing offline");
            return self.disconnect();
        }
        if !health.responsive {
            warn!(path = %self.path, "Device unresponsive — forcing offline");
            return self.disconnect();
        }

        let old_online = self.state.is_online();

        // Read-only transition (ext4 error recovery, hardware write-protect).
        if health.read_only {
            self.state = VolumeState::Degraded("filesystem read-only".into());
        } else {
            match metrics {
                Some(m) if m.capacity_bytes == 0 => {
                    self.state = VolumeState::Degraded("zero capacity".into());
                }
                Some(m) => {
                    self.capacity_bytes = m.capacity_bytes;
                    self.used_bytes = m.used_bytes;
                    self.state = VolumeState::Online;
                }
                None => {
                    self.state = VolumeState::Degraded("capacity probe failed".into());
                }
            }
        }

        if old_online != self.state.is_online() {
            vec![StorageChanged::Reclassified]
        } else {
            vec![]
        }
    }

    /// Reconcile pin state from what's on disk. Called during health ticks
    /// to detect external pin changes (e.g., another stone wrote pin.json).
    pub fn reconcile_pin(&mut self, disk_pin: Option<String>) {
        let Some(ref mut mgmt) = self.management else {
            return;
        };
        if !self.state.is_online() {
            return;
        }

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

    /// Update OS metadata (mount path, label) without changing state.
    pub fn update_os_metadata(&mut self, mount_path: PathBuf, label: Option<String>) {
        self.mount_path = mount_path;
        self.label = label;
    }
}

// ── Domain operations — called by API handlers ──────────────────────

impl Volume {
    /// Rename the replica set. Returns Renamed event if the name actually changed.
    pub fn rename(&mut self, new_name: String) -> Vec<StorageChanged> {
        let Some(ref mut mgmt) = self.management else {
            return vec![];
        };
        if mgmt.replica_set_name == new_name {
            return vec![];
        }
        mgmt.replica_set_name = new_name.clone();
        mgmt.replica_set_name_updated_at = Some(chrono::Utc::now());
        vec![StorageChanged::Renamed {
            replica_set_id: mgmt.replica_set_id.clone(),
            new_name,
        }]
    }

    /// Set roles. Returns Reclassified event if roles actually changed.
    pub fn set_roles(&mut self, roles: Vec<String>) -> Vec<StorageChanged> {
        let Some(ref mut mgmt) = self.management else {
            return vec![];
        };
        if mgmt.roles == roles {
            return vec![];
        }
        mgmt.roles = roles;
        vec![StorageChanged::Reclassified]
    }

    /// Set visibility. Returns Reclassified if changed.
    pub fn set_visibility(&mut self, vis: StorageVisibility) -> Vec<StorageChanged> {
        let Some(ref mut mgmt) = self.management else {
            return vec![];
        };
        if mgmt.visibility == vis {
            return vec![];
        }
        mgmt.visibility = vis;
        vec![StorageChanged::Reclassified]
    }

    /// Release management (make this volume unmanaged).
    pub fn release(&mut self) -> Vec<StorageChanged> {
        let Some(mgmt) = self.management.take() else {
            return vec![];
        };
        vec![
            StorageChanged::Released {
                name: mgmt.display_name().to_string(),
            },
            StorageChanged::Removed {
                device_id: mgmt.id,
                replica_set_id: mgmt.replica_set_id,
            },
        ]
    }

    /// Pin this volume as Primary. Returns PinChanged event.
    pub async fn pin(
        &mut self,
        store: &(impl ManagementStoreOps + ?Sized),
    ) -> anyhow::Result<Vec<StorageChanged>> {
        let mgmt = self
            .management
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Cannot pin an unmanaged volume"))?;

        let pin_id = uuid::Uuid::now_v7().to_string();
        store.write_pin(&pin_id).await?;
        mgmt.pin = Some(PinState {
            pin_id: pin_id.clone(),
        });
        mgmt.role = StorageRole::Primary;

        info!(name = %mgmt.name, pin_id = %pin_id, "Volume pinned — claiming Primary");
        Ok(vec![StorageChanged::PinChanged {
            device_id: mgmt.id.clone(),
            replica_set_id: mgmt.replica_set_id.clone(),
        }])
    }

    /// Unpin, returning to normal orchestration. Returns PinChanged event.
    pub async fn unpin(
        &mut self,
        store: &(impl ManagementStoreOps + ?Sized),
    ) -> anyhow::Result<Vec<StorageChanged>> {
        let mgmt = self
            .management
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Cannot unpin an unmanaged volume"))?;

        let old = mgmt.pin.take();
        if let Some(ref ps) = old {
            if let Err(e) = store.delete_pin().await {
                warn!(name = %mgmt.name, error = %e, "Failed to delete pin file");
            }
            info!(name = %mgmt.name, pin_id = %ps.pin_id, "Volume unpinned");
        }
        if old.is_some() {
            Ok(vec![StorageChanged::PinChanged {
                device_id: mgmt.id.clone(),
                replica_set_id: mgmt.replica_set_id.clone(),
            }])
        } else {
            Ok(vec![])
        }
    }

    /// Snapshot critical files to `last-known-good/`.
    pub async fn snapshot_lkg(&self, store: &(impl ManagementStoreOps + ?Sized)) {
        if let Some(ref mgmt) = self.management
            && let Err(e) = store.snapshot_lkg().await
        {
            warn!(name = %mgmt.name, error = %e, "Failed to snapshot LKG");
        }
    }
}

// ── Projections — read-only views for API/wire formats ──────────────

impl Volume {
    pub fn to_announcement(&self) -> Option<StorageAnnouncement> {
        let mgmt = self.management.as_ref()?;
        if !self.state.is_online() {
            return None;
        }
        Some(StorageAnnouncement {
            id: mgmt.id.clone(),
            name: mgmt.name.clone(),
            replica_set_id: mgmt.replica_set_id.clone(),
            replica_set_name: mgmt.replica_set_name.clone(),
            replica_set_name_updated_at: mgmt.replica_set_name_updated_at,
            role: mgmt.role,
            protocols: vec![
                garden_common::constants::PROTOCOL_STORAGE.to_string(),
                garden_common::constants::PROTOCOL_S3.to_string(),
            ],
            access: StorageAccess::Direct,
            visibility: mgmt.visibility.to_string(),
            health: if self.state == VolumeState::Online {
                garden_common::constants::HEALTH_HEALTHY.to_string()
            } else {
                garden_common::constants::HEALTH_DEGRADED.to_string()
            },
            capacity_bytes: self.capacity_bytes,
            used_bytes: self.used_bytes,
            encrypted: mgmt.encrypted,
            pin_id: mgmt.pin.as_ref().map(|p| p.pin_id.clone()),
            roles: mgmt.roles.clone(),
        })
    }

    pub fn to_storage_info(&self) -> Option<StorageInfo> {
        let mgmt = self.management.as_ref()?;
        if !self.state.is_online() {
            return None;
        }
        Some(StorageInfo::new(
            mgmt.id.clone(),
            mgmt.name.clone(),
            mgmt.replica_set_id.clone(),
            mgmt.replica_set_name.clone(),
            self.path.clone(),
            self.mount_path.to_string_lossy().to_string(),
            self.capacity_bytes,
            self.used_bytes,
            mgmt.visibility,
            false,
            mgmt.origin_stone.clone(),
            mgmt.created_at,
            mgmt.roaming,
            self.state.is_online(),
            mgmt.encrypted,
            mgmt.roles.clone(),
        ))
    }

    pub fn to_summary(&self, stone_name: Option<&str>) -> Option<StorageSummary> {
        let ann = self.to_announcement()?;
        Some(StorageSummary::from_announcement(
            &ann,
            stone_name.unwrap_or("local"),
        ))
    }
}

// ── Test helpers ──────────────────────────────────────────────────────

#[cfg(test)]
impl Volume {
    /// Build a Volume for tests with all fields specified directly.
    pub fn for_test(
        path: &str,
        mount_path: PathBuf,
        label: Option<String>,
        capacity_bytes: u64,
        used_bytes: u64,
        removable: bool,
        state: VolumeState,
        management: Option<Management>,
    ) -> Self {
        Self {
            path: path.to_string(),
            mount_path,
            label,
            capacity_bytes,
            used_bytes,
            removable,
            state,
            management,
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn online_volume() -> Volume {
        let snap = VolumeSnapshot {
            path: "/dev/sdb1".to_string(),
            mount_path: "/mnt/usb".to_string(),
            label: Some("TEST".to_string()),
            capacity_bytes: 64_000_000_000,
            removable: true,
        };
        let mut vol = Volume::from_snapshot(&snap);
        vol.connect(DiskMetrics {
            capacity_bytes: 64_000_000_000,
            used_bytes: 1_000_000,
        });
        vol
    }

    // ── STORAGE-0018: Device health transitions ──────────────────────

    #[test]
    fn stale_device_forces_disconnect() {
        let mut vol = online_volume();
        assert!(vol.is_online());

        let health = DeviceHealth {
            stale_reference: true,
            ..DeviceHealth::healthy()
        };
        let events = vol.observe_metrics(
            Some(DiskMetrics { capacity_bytes: 64_000_000_000, used_bytes: 0 }),
            health,
        );

        assert_eq!(*vol.state(), VolumeState::Offline);
        // Unmanaged volume — no Released event, but state changed
        assert!(events.is_empty());
    }

    #[test]
    fn unresponsive_device_forces_disconnect() {
        let mut vol = online_volume();

        let health = DeviceHealth {
            responsive: false,
            ..DeviceHealth::healthy()
        };
        let events = vol.observe_metrics(None, health);

        assert_eq!(*vol.state(), VolumeState::Offline);
        assert!(events.is_empty()); // unmanaged
    }

    #[test]
    fn read_only_mount_degrades() {
        let mut vol = online_volume();

        let health = DeviceHealth {
            read_only: true,
            ..DeviceHealth::healthy()
        };
        let events = vol.observe_metrics(
            Some(DiskMetrics { capacity_bytes: 64_000_000_000, used_bytes: 0 }),
            health,
        );

        assert!(matches!(vol.state(), VolumeState::Degraded(reason) if reason == "filesystem read-only"));
        // Online → Degraded is still "online" per is_online(), so no Reclassified event
        assert!(events.is_empty());
    }

    #[test]
    fn healthy_device_stays_online() {
        let mut vol = online_volume();

        let events = vol.observe_metrics(
            Some(DiskMetrics { capacity_bytes: 64_000_000_000, used_bytes: 2_000_000 }),
            DeviceHealth::healthy(),
        );

        assert_eq!(*vol.state(), VolumeState::Online);
        assert!(events.is_empty());
        assert_eq!(vol.used_bytes(), 2_000_000);
    }

    #[test]
    fn stale_on_offline_is_noop() {
        let mut vol = online_volume();
        vol.disconnect();

        let health = DeviceHealth {
            stale_reference: true,
            ..DeviceHealth::healthy()
        };
        let events = vol.observe_metrics(None, health);

        assert_eq!(*vol.state(), VolumeState::Offline);
        assert!(events.is_empty());
    }

    #[test]
    fn read_only_recovery_returns_online() {
        let mut vol = online_volume();

        // Degrade with read-only
        vol.observe_metrics(
            Some(DiskMetrics { capacity_bytes: 64_000_000_000, used_bytes: 0 }),
            DeviceHealth { read_only: true, ..DeviceHealth::healthy() },
        );
        assert!(matches!(vol.state(), VolumeState::Degraded(_)));

        // Recover — filesystem is now read-write again
        let events = vol.observe_metrics(
            Some(DiskMetrics { capacity_bytes: 64_000_000_000, used_bytes: 0 }),
            DeviceHealth::healthy(),
        );
        assert_eq!(*vol.state(), VolumeState::Online);
        // Degraded → Online: is_online() was true both times, so no event
        assert!(events.is_empty());
    }
}
