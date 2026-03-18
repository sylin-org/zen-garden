//! Volume — the universal storage entity.
//!
//! A `Volume` represents any accessible storage (USB drive, NAS mount, local
//! directory). Whether Zen Garden manages it is determined by `management`.

use std::path::PathBuf;
use std::sync::Arc;

use garden_common::storage::{
    short_id_from_guid, StorageAccess, StorageAnnouncement, StorageInfo, StorageManifest,
    StorageRole, StorageSummary, StorageVisibility, DEFAULT_REPLICA_SET_DISPLAY,
};
use tracing::{debug, info, warn};

use crate::domain::traits::{ManagementStoreOps, StoragePlatform};

use super::platform_types::VolumeSnapshot;

// ============================================================================
// Volume state
// ============================================================================

/// Lifecycle state of a volume.
///
/// The monitor (via [`super::StorageBank`]) is the sole authority for
/// `Offline → Online` transitions. [`Volume::probe_health`] can only move
/// between `Online` and `Degraded` — it never resurrects an offline volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeState {
    /// Mounted, accessible, disk_usage valid.
    Online,
    /// Accessible but with issues (zero capacity, probe error).
    Degraded(String),
    /// Device gone. Only the monitor (via StorageBank) can revive.
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
    /// Unique GUIDv7 per physical device.
    pub id: String,
    /// First 8 hex chars of `id`.
    pub short_id: String,
    /// Device display name (sugar). User-renamable.
    pub name: String,

    // --- Replica set identity (STORAGE-0013) ---
    /// Replica set ID (GUIDv7). Groups devices that replicate the same content.
    pub replica_set_id: String,
    /// Replica set display name (sugar). Empty = default set ("storage").
    pub replica_set_name: String,
    /// Timestamp of last replica set rename. For catch-up on reconnect.
    pub replica_set_name_updated_at: Option<chrono::DateTime<chrono::Utc>>,

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
    /// Lifecycle state — Online, Degraded, or Offline.
    pub state: VolumeState,

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
            state: VolumeState::Online,
            management: None,
        }
    }

    /// Classify this volume by checking for `.zen-garden/manifest.json`.
    ///
    /// If found and valid, sets `management`. Otherwise leaves it `None`.
    ///
    /// `make_store` constructs the management store for the given mount path.
    /// This is provided by infra so domain code never depends on a concrete store type.
    pub async fn classify(
        &mut self,
        make_store: &(dyn Fn(PathBuf) -> Arc<dyn ManagementStoreOps> + Send + Sync),
    ) {
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

        // Read persisted pin from disk via an ephemeral store
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

    /// Probe capacity and update Online/Degraded state.
    ///
    /// No-op for Offline volumes — the monitor (via [`super::StorageBank`]) is the sole
    /// authority for `Offline → Online` transitions. This prevents a statvfs
    /// call on an unmounted directory from silently reviving a lost volume.
    pub fn probe_health(&mut self, platform: &dyn StoragePlatform) {
        if self.state == VolumeState::Offline {
            return;
        }

        let mount_str = self.mount_path.to_string_lossy().to_string();

        match platform.disk_usage(&mount_str) {
            Some(usage) => {
                if usage.total() == 0 {
                    self.state = VolumeState::Degraded("zero capacity".into());
                } else {
                    self.capacity_bytes = usage.total();
                    self.used_bytes = usage.used_bytes;
                    self.state = VolumeState::Online;
                }
            }
            None => {
                self.state = VolumeState::Degraded("capacity probe failed".into());
            }
        }

        // Reconcile pin state from disk (detect external changes)
        if self.state.is_online() {
            if let Some(ref mut mgmt) = self.management {
                // We can't do async here, so we do a blocking pin read.
                // Pin files are tiny (<100 bytes), blocking is acceptable.
                let pin_path = self.mount_path.join(".zen-garden").join("pin.json");
                let disk_pin = std::fs::read_to_string(&pin_path)
                    .ok()
                    .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
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
    pub async fn pin(&mut self, store: &dyn ManagementStoreOps) -> anyhow::Result<String> {
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
        Ok(pin_id)
    }

    /// Unpin, returning to normal orchestration.
    pub async fn unpin(
        &mut self,
        store: &dyn ManagementStoreOps,
    ) -> anyhow::Result<Option<String>> {
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
    pub async fn snapshot_lkg(&self, store: &dyn ManagementStoreOps) {
        if let Some(ref mgmt) = self.management {
            if let Err(e) = store.snapshot_lkg().await {
                warn!(name = %mgmt.name, error = %e, "Failed to snapshot LKG");
            }
        }
    }

    // ========================================================================
    // Projections (STORAGE-0013)
    // ========================================================================

    /// Project to `StorageAnnouncement` — the canonical wire type for beacons,
    /// registry storage, and API responses.
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

    /// Project to `StorageInfo` for API responses.
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

    /// Build a `StorageSummary` for CLI display.
    pub fn to_summary(&self, stone_name: Option<&str>) -> Option<StorageSummary> {
        let ann = self.to_announcement()?;
        let sn = stone_name.unwrap_or("local");
        Some(StorageSummary::from_announcement(&ann, sn))
    }
}
