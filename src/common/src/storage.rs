//! Managed storage types (STORAGE-0009)
//!
//! Shared contracts between Moss and Rake for managed storage.
//! Storage is the universal entity; seed bank is a composable role.

use crate::manifests::Offering as OfferingManifest;
use crate::types::Offering;
use crate::OfferingMode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// Device State Types
// ============================================================================

/// State of a detected storage device
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    /// Raw device, no partition table
    Unpartitioned,
    /// Partition exists, no filesystem
    Unformatted,
    /// Filesystem exists, zero visible files
    Empty,
    /// Has `.zen-garden/` directory - already a seed bank
    Prepared,
    /// Contains visible files - cannot prepare
    HasData,
}

impl DeviceState {
    /// Returns true if device can be prepared as a seed bank
    pub fn is_eligible(&self) -> bool {
        matches!(
            self,
            DeviceState::Unpartitioned | DeviceState::Unformatted | DeviceState::Empty
        )
    }
}

impl std::fmt::Display for DeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceState::Unpartitioned => write!(f, "unpartitioned"),
            DeviceState::Unformatted => write!(f, "unformatted"),
            DeviceState::Empty => write!(f, "empty"),
            DeviceState::Prepared => write!(f, "prepared"),
            DeviceState::HasData => write!(f, "has_data"),
        }
    }
}

// ============================================================================
// Storage Detection Types
// ============================================================================

/// Information about a detected storage device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageDetectedInfo {
    /// Device path (e.g., "/dev/sdb1")
    pub device: String,

    /// Mount path if mounted (e.g., "/mnt/usb")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_path: Option<String>,

    /// Device label if available (e.g., "SANDISK_32GB")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// Capacity in bytes
    pub capacity_bytes: u64,

    /// Current state of the device
    pub state: DeviceState,

    /// Whether device is eligible for preparation
    pub eligible: bool,

    /// Whether device is removable (USB, SD card, etc.)
    pub removable: bool,

    /// Reason if not eligible
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ineligible_reason: Option<String>,
}

// ============================================================================
// Managed Storage Types (STORAGE-0009)
// ============================================================================

/// Visibility of a managed storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StorageVisibility {
    /// Visible to all stones in the garden
    #[default]
    Open,
    /// Only accessible locally
    Closed,
    /// Visible but read-only (degraded state)
    ReadOnly,
}

/// Default name for unencrypted (public) storage
pub const DEFAULT_PUBLIC_STORAGE_NAME: &str = "zen-garden";
/// Default name for encrypted (private / pond-scoped) storage
pub const DEFAULT_PRIVATE_STORAGE_NAME: &str = "private";

/// Well-known role: receives offering harvests from nurturing cycles
pub const ROLE_SEED_BANK: &str = "seed-bank";

impl std::fmt::Display for StorageVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageVisibility::Open => write!(f, "open"),
            StorageVisibility::Closed => write!(f, "closed"),
            StorageVisibility::ReadOnly => write!(f, "read-only"),
        }
    }
}

/// Information about a managed storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageInfo {
    /// Unique identifier (GUIDv7)
    pub id: String,

    /// Human-readable name — logical FQN shared across replicas
    pub name: String,

    /// Device path (e.g., "/dev/sdb1")
    pub device: String,

    /// Mount path under data_dir
    pub mount_path: String,

    /// Total capacity in bytes
    pub capacity_bytes: u64,

    /// Used space in bytes
    pub used_bytes: u64,

    /// Visibility setting
    pub visibility: StorageVisibility,

    /// Whether the filesystem is btrfs (vs ext4)
    pub btrfs: bool,

    /// Stone that created this storage
    pub origin_stone: String,

    /// When the storage was created
    pub created_at: DateTime<Utc>,

    /// Last sync timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<DateTime<Utc>>,

    /// Whether this is a roaming storage (detected at boot, not originally created here)
    #[serde(default)]
    pub roaming: bool,

    /// Whether the device is currently mounted and accessible
    #[serde(default = "default_true")]
    pub online: bool,

    /// Whether content is encrypted
    #[serde(default)]
    pub encrypted: bool,

    /// Composable roles (e.g., ["seed-bank"])
    #[serde(default = "default_seed_bank_role")]
    pub roles: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl StorageInfo {
    /// Create a new StorageInfo with all required fields.
    /// Deprecated backward-compat fields are set to None internally.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        device: String,
        mount_path: String,
        capacity_bytes: u64,
        used_bytes: u64,
        visibility: StorageVisibility,
        btrfs: bool,
        origin_stone: String,
        created_at: DateTime<Utc>,
        roaming: bool,
        online: bool,
        encrypted: bool,
        roles: Vec<String>,
    ) -> Self {
        Self {
            id,
            name,
            device,
            mount_path,
            capacity_bytes,
            used_bytes,
            visibility,
            btrfs,
            origin_stone,
            created_at,
            last_sync: None,
            roaming,
            online,
            encrypted,
            roles,
        }
    }

    /// Whether this storage has the seed-bank role (receives offering backups)
    pub fn is_seed_bank(&self) -> bool {
        self.roles.iter().any(|r| r == ROLE_SEED_BANK)
    }

    /// Derive the short ID (first 8 hex chars of the GUIDv7, excluding dashes).
    /// Used as the per-device directory name under mounts/{name}/{short_id}/.
    pub fn short_id(guid: &str) -> String {
        guid.chars()
            .filter(|c| c.is_ascii_hexdigit())
            .take(8)
            .collect()
    }
}

// ============================================================================
// Storage Summary (shared formatting utility)
// ============================================================================

/// Compact storage summary for CLI display and portrait enrichment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSummary {
    /// First 8 hex chars of the GUIDv7
    pub short_id: String,

    /// Logical storage name (shared across replicas)
    pub name: String,

    /// Capacity in GB (human-readable)
    pub capacity_gb: f32,

    /// Device path (e.g., "/dev/sdb1") — only meaningful on the local stone
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,

    /// Name of the hosting stone
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stone_name: Option<String>,

    /// Runtime role (Primary / Dormant)
    pub role: StorageRole,

    /// Whether the Primary role is pinned (locked)
    #[serde(default)]
    pub pinned: bool,

    /// Whether content is encrypted
    #[serde(default)]
    pub encrypted: bool,

    /// Whether the device is online
    #[serde(default = "default_true")]
    pub online: bool,

    /// Composable roles (e.g., ["seed-bank"])
    #[serde(default = "default_seed_bank_role")]
    pub roles: Vec<String>,
}

impl StorageSummary {
    /// Build from a local `StorageInfo` plus runtime state.
    pub fn from_info(
        info: &StorageInfo,
        role: StorageRole,
        pinned: bool,
        stone_name: Option<&str>,
    ) -> Self {
        Self {
            short_id: StorageInfo::short_id(&info.id),
            name: info.name.clone(),
            capacity_gb: info.capacity_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
            device: Some(info.device.clone()),
            stone_name: stone_name.map(|s| s.to_string()),
            role,
            pinned,
            encrypted: info.encrypted,
            online: info.online,
            roles: info.roles.clone(),
        }
    }

    /// Build from a beacon announcement (remote stone).
    pub fn from_announcement(ann: &StorageAnnouncement, stone_name: &str) -> Self {
        Self {
            short_id: StorageInfo::short_id(&ann.id),
            name: ann.name.clone(),
            capacity_gb: ann.capacity_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
            device: None,
            stone_name: Some(stone_name.to_string()),
            role: ann.role,
            pinned: ann.pin_id.is_some(),
            encrypted: ann.encrypted,
            online: true,
            roles: ann.roles.clone(),
        }
    }

    /// Format a compact single-line summary for CLI display.
    ///
    /// Example: `● 01956a3e  64GB   stone-01  Primary  ★ pinned`
    pub fn format_line(&self) -> String {
        let marker = if self.role == StorageRole::Primary {
            "●"
        } else {
            " "
        };
        let cap = format!("{}GB", self.capacity_gb as u32);
        let stone = self.stone_name.as_deref().unwrap_or("local");
        let pin_label = if self.pinned { "  ★ pinned" } else { "" };
        format!(
            "  {} {}  {:>5}  {:12}  {}{}",
            marker, self.short_id, cap, stone, self.role, pin_label
        )
    }
}

// ============================================================================
// API Request/Response Types
// ============================================================================

/// Unified request to add storage (STORAGE-0010).
///
/// The server inspects `target` to determine the operation:
/// - Block device with no filesystem → formats, mounts, creates `.zen-garden/`
/// - Block device with filesystem, no files → mounts, creates `.zen-garden/`
/// - Block device or directory with existing files → creates `.zen-garden/`, catalogs content
/// - Path with existing `.zen-garden/` → 409 Conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddStorageRequest {
    /// Target path: block device (e.g., "/dev/sdb1") or directory (e.g., "/mnt/nas-media").
    pub target: String,

    /// Logical name for this storage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Whether to format the device (only valid for block devices).
    #[serde(default)]
    pub format: bool,

    /// Filesystem to use when formatting: "btrfs" (default) or "ext4".
    #[serde(default = "default_btrfs")]
    pub filesystem: String,

    /// Whether to encrypt content (pond-scoped).
    #[serde(default)]
    pub encrypted: bool,

    /// Roles to assign (e.g., ["seed-bank"]).
    #[serde(default)]
    pub roles: Vec<String>,
}

fn default_btrfs() -> String {
    "btrfs".to_string()
}

/// Response from add storage operation (STORAGE-0010).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddStorageResponse {
    /// Storage ID (GUIDv7).
    pub id: String,
    /// Logical storage name.
    pub name: String,
    /// Mount path where storage is accessible.
    pub mount_path: String,
    /// Whether the device was formatted.
    pub formatted: bool,
    /// Number of existing files cataloged for replication baseline.
    #[serde(default)]
    pub cataloged: usize,
    /// Job ID if formatting runs asynchronously.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

/// Request to change storage visibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetVisibilityRequest {
    pub visibility: StorageVisibility,
}

/// Request to rename a managed storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameStorageRequest {
    pub new_name: String,
}

/// Request to set roles on a managed storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRolesRequest {
    pub roles: Vec<String>,
}

// ============================================================================
// Hydration Metadata
// ============================================================================

/// Offering manifest snapshot stored alongside memories for hydration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoriesOfferingManifest {
    /// Offering ID (GUIDv7)
    pub offering_id: String,
    /// Instance name
    pub offering_name: String,
    /// Offering template name
    pub offering: String,
    /// Offering mode at time of capture
    pub mode: OfferingMode,
    /// Version string (if available)
    pub version: String,
    /// Stone ID that captured the manifest
    pub source_stone_id: String,
    /// Stone name that captured the manifest
    pub source_stone_name: String,
    /// Capture timestamp
    pub captured_at: DateTime<Utc>,
    /// Offering definition (manifest) if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<OfferingManifest>,
}

impl MemoriesOfferingManifest {
    /// Build a hydration manifest from a runtime offering and optional definition.
    pub fn from_offering(
        offering: &Offering,
        manifest: Option<OfferingManifest>,
        stone_id: &str,
        stone_name: &str,
    ) -> Self {
        Self {
            offering_id: offering.offering_id.clone(),
            offering_name: offering.name.to_string(),
            offering: offering.offering.clone(),
            mode: offering.mode(),
            version: offering.version.clone(),
            source_stone_id: stone_id.to_string(),
            source_stone_name: stone_name.to_string(),
            captured_at: Utc::now(),
            manifest,
        }
    }
}

// ============================================================================
// Changelog Types (STORAGE-0006 — cursor-based replication)
// ============================================================================

/// Changelog operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangelogOp {
    /// Created (new file)
    C,
    /// Modified (overwritten)
    M,
    /// Deleted
    D,
}

impl std::fmt::Display for ChangelogOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::C => write!(f, "C"),
            Self::M => write!(f, "M"),
            Self::D => write!(f, "D"),
        }
    }
}

/// A single changelog entry — one line in `.zen-garden/changelog.jsonl`.
///
/// Appended by `ContentStore::write()` and `ContentStore::delete()`.
/// The cursor `c` is a GUIDv7 — time-sortable, unique, extractable timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    /// GUIDv7 cursor — time-sortable unique identifier for this change
    pub c: String,

    /// Operation: C (create), M (modify), D (delete)
    pub op: ChangelogOp,

    /// Relative path within the seed bank mount root
    pub path: String,

    /// Size in bytes (for C/M operations, omitted for D)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

impl ChangelogEntry {
    /// Create a "file created" entry
    pub fn created(path: &str, size: u64) -> Self {
        Self {
            c: crate::utils::ids::generate_guidv7(),
            op: ChangelogOp::C,
            path: path.to_string(),
            bytes: Some(size),
        }
    }

    /// Create a "file modified" entry
    pub fn modified(path: &str, size: u64) -> Self {
        Self {
            c: crate::utils::ids::generate_guidv7(),
            op: ChangelogOp::M,
            path: path.to_string(),
            bytes: Some(size),
        }
    }

    /// Create a "file deleted" entry
    pub fn deleted(path: &str) -> Self {
        Self {
            c: crate::utils::ids::generate_guidv7(),
            op: ChangelogOp::D,
            path: path.to_string(),
            bytes: None,
        }
    }
}

/// SSE notification tick — lightweight "something changed" signal.
///
/// Emitted on the storage notification stream when the changelog advances.
/// The Dormant subscriber uses this to know when to pull changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageTick {
    /// Latest cursor on this managed storage
    pub cursor: String,
    /// Storage name
    pub storage: String,
    /// Count of creates since last tick
    #[serde(rename = "C")]
    pub creates: u32,
    /// Count of modifies since last tick
    #[serde(rename = "M")]
    pub modifies: u32,
    /// Count of deletes since last tick
    #[serde(rename = "D")]
    pub deletes: u32,
}

/// Response for the changes pull endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangesResponse {
    /// Latest cursor (the `c` of the last entry returned)
    pub cursor: String,
    /// Changelog entries since the requested cursor (squashed to net-effect per path)
    pub changes: Vec<ChangelogEntry>,
    /// When `true`, the requested cursor has been compacted away.
    /// The Dormant must perform a full directory-walk reconciliation
    /// instead of incremental sync. `changes` will be empty.
    #[serde(default, skip_serializing_if = "is_false")]
    pub full_sync_required: bool,
}

fn is_false(v: &bool) -> bool {
    !v
}

// ============================================================================
// Storage Manifest (stored on device at .zen-garden/manifest.json)
// ============================================================================

/// Manifest stored at `.zen-garden/manifest.json` on managed storage.
///
/// Single source of truth for storage identity and configuration.
/// Mount paths are derived from this manifest, not from filesystem labels.
///
/// ## Version History
/// - v1-v3: Legacy formats (pre STORAGE-0009)
/// - v4: Added roles array. Storage is the entity; seed bank is a role. (STORAGE-0009)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageManifest {
    /// Version of the manifest format (current: 4)
    #[serde(default = "default_manifest_version")]
    pub version: u32,

    /// Unique identifier (GUIDv7) — one per physical device, never changes
    pub id: String,

    /// Logical storage name (FQN). Shared across all replicas.
    /// Two devices with the same name and different IDs are replicas.
    pub name: String,

    /// Visibility setting
    pub visibility: StorageVisibility,

    /// Stone that created this storage
    pub origin_stone: String,

    /// Filesystem type ("btrfs" or "ext4")
    pub filesystem: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Whether content is encrypted
    #[serde(default)]
    pub encrypted: bool,

    /// Pond CA fingerprint (present when encrypted = true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pond_fingerprint: Option<String>,

    /// Composable roles (STORAGE-0009). Default: ["seed-bank"] for backward compat.
    #[serde(default = "default_seed_bank_role")]
    pub roles: Vec<String>,
}

fn default_manifest_version() -> u32 {
    1
}

fn default_seed_bank_role() -> Vec<String> {
    vec![ROLE_SEED_BANK.to_string()]
}

impl StorageManifest {
    pub const CURRENT_VERSION: u32 = 4;

    /// Create a new manifest with default roles (seed-bank).
    pub fn new(
        name: &str,
        origin_stone: &str,
        filesystem: &str,
        visibility: StorageVisibility,
    ) -> Self {
        let id = crate::utils::ids::generate_guidv7();

        Self {
            version: Self::CURRENT_VERSION,
            id,
            name: name.to_string(),
            visibility,
            origin_stone: origin_stone.to_string(),
            filesystem: filesystem.to_string(),
            created_at: Utc::now(),
            encrypted: false,
            pond_fingerprint: None,
            roles: vec![ROLE_SEED_BANK.to_string()],
        }
    }

    /// Create a new manifest with specific roles.
    pub fn with_roles(
        name: &str,
        origin_stone: &str,
        filesystem: &str,
        visibility: StorageVisibility,
        roles: Vec<String>,
    ) -> Self {
        let mut manifest = Self::new(name, origin_stone, filesystem, visibility);
        manifest.roles = roles;
        manifest
    }

    /// Create a new manifest with encryption enabled.
    pub fn new_encrypted(
        name: &str,
        origin_stone: &str,
        filesystem: &str,
        visibility: StorageVisibility,
        pond_fingerprint: &str,
    ) -> Self {
        let mut manifest = Self::new(name, origin_stone, filesystem, visibility);
        manifest.encrypted = true;
        manifest.pond_fingerprint = Some(pond_fingerprint.to_string());
        manifest
    }

    /// Derive the mount path: `{base}/mounts/{name}/{short_id}/`
    pub fn derive_mount_path(&self, base_dir: &str) -> String {
        let short_id = StorageInfo::short_id(&self.id);
        format!("{}/mounts/{}/{}", base_dir, self.name, short_id)
    }

    /// Whether this storage has the seed-bank role
    pub fn is_seed_bank(&self) -> bool {
        self.roles.iter().any(|r| r == ROLE_SEED_BANK)
    }

    /// Get the logical storage name (FQN)
    pub fn logical_name(&self) -> &str {
        &self.name
    }
}

// ============================================================================
// Storage Beacon Types (STORAGE-0003)
// ============================================================================

/// Storage capability beacon - lightweight announcement for routing
///
/// Broadcast on seed bank mount/unmount/visibility change.
/// All stones lurk-listen and update their StorageCache.
///
/// See docs/decisions/STORAGE-0003-beacon-protocol.md
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageBeacon {
    /// Stone ID (links to TopologyEntry in TopologyCache)
    pub stone_id: String,

    /// Human-readable stone name
    pub stone_name: String,

    /// HTTP endpoint for storage API (e.g., "http://stone-alpha.local:7185")
    pub endpoint: String,

    /// List of available managed storages (empty = no storage)
    pub storages: Vec<StorageAnnouncement>,

    /// Beacon timestamp
    pub timestamp: DateTime<Utc>,
}

impl StorageBeacon {
    /// Create an empty beacon (no storages)
    pub fn empty(stone_id: &str, stone_name: &str, endpoint: &str) -> Self {
        Self {
            stone_id: stone_id.to_string(),
            stone_name: stone_name.to_string(),
            endpoint: endpoint.to_string(),
            storages: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    /// Check if this stone has any storage capability
    pub fn has_storage(&self) -> bool {
        !self.storages.is_empty()
    }

    /// Check if this stone supports S3 protocol
    pub fn supports_s3(&self) -> bool {
        self.storages
            .iter()
            .any(|s| s.protocols.contains(&"s3".to_string()))
    }
}

/// Runtime role of a managed storage within its replica group.
///
/// Assigned at runtime by the storage orchestration task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StorageRole {
    /// Accepts writes. One primary per logical storage name.
    #[default]
    Primary,
    /// Read-only replica. Replicates from primary via SSE pull.
    Dormant,
}

impl std::fmt::Display for StorageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageRole::Primary => write!(f, "primary"),
            StorageRole::Dormant => write!(f, "dormant"),
        }
    }
}

/// Orchestration state for a logical managed storage.
///
/// Persisted per storage name on the stone. Mirrors OfferingOrchestrationState.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageOrchestrationState {
    /// Current role of this seed bank on this stone
    pub role: StorageRole,

    /// Stone ID of the current primary (from beacons)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_stone_id: Option<String>,

    /// Seed bank ID of the current primary device
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_seed_bank_id: Option<String>,

    /// Whether this seed bank is pinned to this stone
    #[serde(default)]
    pub pinned: bool,

    /// When pinning was set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_timestamp: Option<DateTime<Utc>>,
}

impl Default for StorageOrchestrationState {
    fn default() -> Self {
        Self {
            role: StorageRole::Primary,
            primary_stone_id: None,
            primary_seed_bank_id: None,
            pinned: false,
            pin_timestamp: None,
        }
    }
}

/// Seed bank announcement entry for beacons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageAnnouncement {
    /// Unique seed bank ID (GUIDv7)
    pub id: String,

    /// Human-readable name (logical FQN — shared across replicas)
    pub name: String,

    /// Runtime role (STORAGE-0006)
    #[serde(default)]
    pub role: StorageRole,

    /// Supported protocols (e.g., ["s3", "storage"])
    pub protocols: Vec<String>,

    /// Access type
    pub access: StorageAccess,

    /// Visibility ("open" or "closed")
    pub visibility: String,

    /// Health status ("healthy", "degraded", "read-only")
    pub health: String,

    /// Total capacity in bytes
    pub capacity_bytes: u64,

    /// Used space in bytes
    pub used_bytes: u64,

    /// Whether content is encrypted (STORAGE-0006)
    #[serde(default)]
    pub encrypted: bool,

    /// Pin ID — a GUIDv7 that claims Primary by pin (STORAGE-0006 Phase 5).
    /// Higher GUIDv7 wins in a conflict (last-pin-wins).
    /// `None` means unpinned; orchestration uses normal tiebreaker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_id: Option<String>,

    /// Composable roles (e.g., ["seed-bank"])
    #[serde(default = "default_seed_bank_role")]
    pub roles: Vec<String>,
}

impl StorageAnnouncement {
    /// Create from StorageInfo
    pub fn from_info(info: &StorageInfo) -> Self {
        Self {
            id: info.id.clone(),
            name: info.name.clone(),
            role: StorageRole::default(),
            protocols: vec!["s3".to_string(), "storage".to_string()],
            access: StorageAccess::Direct,
            visibility: info.visibility.to_string(),
            health: if info.online {
                if matches!(info.visibility, StorageVisibility::ReadOnly) {
                    "read-only".to_string()
                } else {
                    "healthy".to_string()
                }
            } else {
                "degraded".to_string()
            },
            capacity_bytes: info.capacity_bytes,
            used_bytes: info.used_bytes,
            encrypted: info.encrypted,
            pin_id: None,
            roles: info.roles.clone(),
        }
    }
}

/// Storage access type for routing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
#[derive(Default)]
pub enum StorageAccess {
    /// Stone can access storage directly
    #[default]
    Direct,
    /// Stone proxies to another gateway
    Proxy {
        /// Stone ID of the direct gateway
        via: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_id_from_guid() {
        let guid = "01956a3e-7c00-7000-8000-000000000001";
        let short = StorageInfo::short_id(guid);
        assert_eq!(short, "01956a3e");
    }

    #[test]
    fn test_device_state_eligibility() {
        assert!(DeviceState::Empty.is_eligible());
        assert!(DeviceState::Unpartitioned.is_eligible());
        assert!(DeviceState::Unformatted.is_eligible());
        assert!(!DeviceState::Prepared.is_eligible());
        assert!(!DeviceState::HasData.is_eligible());
    }

    #[test]
    fn test_manifest_creation() {
        let manifest = StorageManifest::new(
            "test-bank",
            "stone-alpha",
            "btrfs",
            StorageVisibility::Open,
        );

        assert_eq!(manifest.version, 4);
        assert_eq!(manifest.name, "test-bank");
        assert!(!manifest.encrypted);
        assert!(manifest.pond_fingerprint.is_none());
        assert!(manifest.is_seed_bank());
    }

    #[test]
    fn test_encrypted_manifest_creation() {
        let manifest = StorageManifest::new_encrypted(
            "private-bank",
            "stone-alpha",
            "ext4",
            StorageVisibility::Open,
            "abc123def456",
        );

        assert_eq!(manifest.version, 4);
        assert!(manifest.encrypted);
        assert_eq!(manifest.pond_fingerprint.as_deref(), Some("abc123def456"));
    }

    #[test]
    fn test_mount_path_derivation() {
        let base = "/var/lib/zen-garden";

        // All seed banks use {name}/{short_id} now
        let manifest =
            StorageManifest::new("my-backup", "stone", "ext4", StorageVisibility::Open);
        let path = manifest.derive_mount_path(base);

        // Path should be: /var/lib/zen-garden/mounts/my-backup/{first 8 hex of id}
        let short_id = StorageInfo::short_id(&manifest.id);
        assert_eq!(
            path,
            format!("/var/lib/zen-garden/mounts/my-backup/{}", short_id)
        );
    }

    #[test]
    fn test_seed_bank_role_display() {
        assert_eq!(StorageRole::Primary.to_string(), "primary");
        assert_eq!(StorageRole::Dormant.to_string(), "dormant");
    }

    #[test]
    fn test_manifest_roles_default() {
        // Manifest without roles field gets default ["seed-bank"]
        let json = r#"{
            "version": 4,
            "id": "01956a3e-7c00-7000-8000-000000000001",
            "name": "zen-garden",
            "visibility": "open",
            "origin_stone": "stone-alpha",
            "filesystem": "btrfs",
            "created_at": "2026-01-01T00:00:00Z"
        }"#;
        let manifest: StorageManifest = serde_json::from_str(json).unwrap();
        assert!(manifest.is_seed_bank());
        assert_eq!(manifest.roles, vec!["seed-bank"]);
    }

    #[test]
    fn test_manifest_no_roles() {
        let manifest = StorageManifest::with_roles(
            "personal",
            "stone-alpha",
            "ext4",
            StorageVisibility::Open,
            vec![],
        );
        assert!(!manifest.is_seed_bank());
        assert!(manifest.roles.is_empty());
    }

    // ====================================================================
    // StorageSummary tests
    // ====================================================================

    fn make_test_info() -> StorageInfo {
        StorageInfo::new(
            "01956a3e-7c00-7000-8000-000000000001".to_string(),
            "public-seed-bank".to_string(),
            "/dev/sdb1".to_string(),
            "/var/lib/zen-garden/mounts/public-seed-bank/01956a3e".to_string(),
            64 * 1024 * 1024 * 1024, // 64 GB
            10 * 1024 * 1024 * 1024, // 10 GB used
            StorageVisibility::Open,
            true,
            "stone-alpha".to_string(),
            Utc::now(),
            false,
            true,
            false,
            vec![ROLE_SEED_BANK.to_string()],
        )
    }

    fn make_test_announcement() -> StorageAnnouncement {
        StorageAnnouncement {
            id: "01956a3e-7c00-7000-8000-000000000002".to_string(),
            name: "private-seed-bank".to_string(),
            role: StorageRole::Dormant,
            protocols: vec!["s3".to_string()],
            access: StorageAccess::Direct,
            visibility: "open".to_string(),
            health: "healthy".to_string(),
            capacity_bytes: 128 * 1024 * 1024 * 1024,
            used_bytes: 0,
            encrypted: true,
            pin_id: Some("019c6d5a-0000-7000-8000-000000000001".to_string()),
            roles: vec![ROLE_SEED_BANK.to_string()],
        }
    }

    #[test]
    fn test_summary_from_info() {
        let info = make_test_info();
        let summary =
            StorageSummary::from_info(&info, StorageRole::Primary, false, Some("stone-alpha"));

        assert_eq!(summary.short_id, "01956a3e");
        assert_eq!(summary.name, "public-seed-bank");
        assert_eq!(summary.capacity_gb as u32, 64);
        assert_eq!(summary.device.as_deref(), Some("/dev/sdb1"));
        assert_eq!(summary.stone_name.as_deref(), Some("stone-alpha"));
        assert_eq!(summary.role, StorageRole::Primary);
        assert!(!summary.pinned);
        assert!(!summary.encrypted);
        assert!(summary.online);
    }

    #[test]
    fn test_summary_from_info_pinned_encrypted() {
        let mut info = make_test_info();
        info.encrypted = true;
        let summary = StorageSummary::from_info(&info, StorageRole::Dormant, true, None);

        assert_eq!(summary.role, StorageRole::Dormant);
        assert!(summary.pinned);
        assert!(summary.encrypted);
        assert!(summary.stone_name.is_none());
    }

    #[test]
    fn test_summary_from_announcement() {
        let ann = make_test_announcement();
        let summary = StorageSummary::from_announcement(&ann, "stone-beta");

        assert_eq!(summary.short_id, "01956a3e");
        assert_eq!(summary.name, "private-seed-bank");
        assert_eq!(summary.capacity_gb as u32, 128);
        assert!(summary.device.is_none()); // remote — no device
        assert_eq!(summary.stone_name.as_deref(), Some("stone-beta"));
        assert_eq!(summary.role, StorageRole::Dormant);
        assert!(summary.pinned);
        assert!(summary.encrypted);
        assert!(summary.online); // remote assumed online
    }

    #[test]
    fn test_format_line_primary() {
        let summary = StorageSummary {
            short_id: "01956a3e".to_string(),
            name: "public-seed-bank".to_string(),
            capacity_gb: 64.0,
            device: Some("/dev/sdb1".to_string()),
            stone_name: Some("stone-01".to_string()),
            role: StorageRole::Primary,
            pinned: false,
            encrypted: false,
            online: true,
            roles: vec![],
        };
        let line = summary.format_line();
        assert!(line.contains("●"), "Primary should have ● marker");
        assert!(line.contains("01956a3e"));
        assert!(line.contains("64GB"));
        assert!(line.contains("stone-01"));
        assert!(line.contains("primary"));
        assert!(!line.contains("pinned"));
    }

    #[test]
    fn test_format_line_dormant_pinned() {
        let summary = StorageSummary {
            short_id: "0195b2c4".to_string(),
            name: "private-seed-bank".to_string(),
            capacity_gb: 128.0,
            device: None,
            stone_name: Some("stone-02".to_string()),
            role: StorageRole::Dormant,
            pinned: true,
            encrypted: true,
            online: true,
            roles: vec![],
        };
        let line = summary.format_line();
        assert!(!line.contains("●"), "Dormant should not have ● marker");
        assert!(line.contains("0195b2c4"));
        assert!(line.contains("128GB"));
        assert!(line.contains("dormant"));
        assert!(line.contains("★ pinned"));
    }

    #[test]
    fn test_format_line_no_stone_name_uses_local() {
        let summary = StorageSummary {
            short_id: "abcdef01".to_string(),
            name: "test".to_string(),
            capacity_gb: 32.0,
            device: None,
            stone_name: None,
            role: StorageRole::Primary,
            pinned: false,
            encrypted: false,
            online: true,
            roles: vec![],
        };
        let line = summary.format_line();
        assert!(line.contains("local"));
    }

    #[test]
    fn test_announcement_from_info_sets_defaults() {
        let info = make_test_info();
        let ann = StorageAnnouncement::from_info(&info);

        assert_eq!(ann.role, StorageRole::Primary); // default
        assert!(ann.pin_id.is_none()); // always None from_info
        assert!(!ann.encrypted);
        assert_eq!(ann.health, "healthy");
        assert_eq!(ann.capacity_bytes, 64 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_announcement_from_info_read_only() {
        let mut info = make_test_info();
        info.visibility = StorageVisibility::ReadOnly;
        let ann = StorageAnnouncement::from_info(&info);
        assert_eq!(ann.health, "read-only");
    }

    #[test]
    fn test_announcement_from_info_offline() {
        let mut info = make_test_info();
        info.online = false;
        let ann = StorageAnnouncement::from_info(&info);
        assert_eq!(ann.health, "degraded");
    }
}
