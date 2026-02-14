//! Storage and seed bank types
//!
//! Shared contracts between Moss and Rake for USB seed bank management.
//! See docs/specs/STORAGE-0001-seed-bank-onboarding.md for full specification.

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
// Seed Bank Types
// ============================================================================

/// Visibility of a seed bank
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SeedBankVisibility {
    /// Visible to all stones in the garden
    #[default]
    Open,
    /// Only accessible to stones with the same seed bank name
    Closed,
    /// Visible but read-only (degraded state)
    ReadOnly,
}

/// Default seed bank name (unnamed pool)
pub const DEFAULT_SEED_BANK_NAME: &str = "seed-bank-zen-garden";

impl std::fmt::Display for SeedBankVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeedBankVisibility::Open => write!(f, "open"),
            SeedBankVisibility::Closed => write!(f, "closed"),
            SeedBankVisibility::ReadOnly => write!(f, "read-only"),
        }
    }
}

/// Information about a prepared seed bank
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedBankInfo {
    /// Unique identifier for the seed bank (GUIDv7)
    pub id: String,

    /// Human-readable name (e.g., "backup-vault", "seed-bank-zengarden")
    pub name: String,

    /// Pool identifier for sync groups (first 4 hex digits of origin GUIDv7)
    pub pool_id: String,

    /// Logical group for replicated seed banks (e.g., "primary", "offsite")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,

    /// Replica number within a group (1, 2, ...)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replica_id: Option<u32>,

    /// Device path (e.g., "/dev/sdb1")
    pub device: String,

    /// Mount path under data_dir (e.g., "/var/lib/zen-garden/mounts/backup-vault")
    pub mount_path: String,

    /// Total capacity in bytes
    pub capacity_bytes: u64,

    /// Used space in bytes
    pub used_bytes: u64,

    /// Visibility setting
    pub visibility: SeedBankVisibility,

    /// Whether the filesystem is btrfs (vs ext4)
    pub btrfs: bool,

    /// Stone that created this seed bank
    pub origin_stone: String,

    /// When the seed bank was created
    pub created_at: DateTime<Utc>,

    /// Last sync timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<DateTime<Utc>>,

    /// Whether this is a roaming seed bank (detected at boot, not originally created here)
    #[serde(default)]
    pub roaming: bool,

    /// Whether the device is currently mounted and accessible
    #[serde(default = "default_true")]
    pub online: bool,
}

fn default_true() -> bool {
    true
}

impl SeedBankInfo {
    /// Generate pool_id from a GUIDv7 id
    pub fn pool_id_from_guid(guid: &str) -> String {
        // GUIDv7 format: xxxxxxxx-xxxx-7xxx-yxxx-xxxxxxxxxxxx
        // Pool ID is first 4 hex chars (excluding dashes)
        guid.chars()
            .filter(|c| c.is_ascii_hexdigit())
            .take(4)
            .collect()
    }
}

// ============================================================================
// API Request/Response Types
// ============================================================================

/// Request to prepare a seed bank
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareSeedBankRequest {
    /// Device path (e.g., "/dev/sdb1")
    pub device: String,

    /// Name for the seed bank (default: "seed-bank-zengarden")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Generate a random name (e.g., "seed-kind-meadow")
    #[serde(default)]
    pub random_name: bool,

    /// Filesystem to use: "btrfs" (default) or "ext4"
    #[serde(default = "default_btrfs")]
    pub filesystem: String,

    /// Logical group for replicated seed banks (e.g., "primary", "offsite")
    /// When set, this device becomes part of a replicated seed bank group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,

    /// Replica number within a group (1, 2, ...)
    /// Only meaningful when `group` is set. Auto-assigned if not specified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replica_id: Option<u32>,
}

fn default_btrfs() -> String {
    "btrfs".to_string()
}

/// Response from prepare operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareSeedBankResponse {
    /// Job ID for tracking progress
    pub job_id: String,

    /// Expected seed bank name (may differ from requested if collision)
    pub name: String,

    /// Whether operation started successfully
    pub started: bool,
}

/// Request to change seed bank visibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetVisibilityRequest {
    pub visibility: SeedBankVisibility,
}

/// Request to rename a seed bank
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameSeedBankRequest {
    pub new_name: String,
}

/// Pool conflict event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConflictData {
    pub seed_bank: String,
    pub this_pool_id: String,
    pub target_pool_id: String,
    pub action_required: String,
}

/// Merge policy for pool synchronization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MergePolicy {
    /// Add files from source to target (default)
    #[default]
    Incremental,
    /// Delete target files not in source
    WipeTarget,
    /// Delete source files not in target
    WipeSource,
}

/// Request to merge seed bank pools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeSeedBankRequest {
    /// Source pool ID (4 hex digits)
    pub source_pool_id: String,
    /// Target pool ID (4 hex digits)
    pub target_pool_id: String,
    /// Merge policy
    #[serde(default)]
    pub policy: MergePolicy,
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
            offering_name: offering.name.clone(),
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
// Journal Types
// ============================================================================

/// Journal entry operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JournalOp {
    Put,
    Delete,
    Snapshot,
    Merge,
}

/// A single journal entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    /// GUIDv7 - serves as unique ID and timestamp
    pub id: String,

    /// Operation type
    pub op: JournalOp,

    /// Object key (path relative to seed bank root)
    pub key: String,

    /// Size in bytes (for Put operations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,

    /// BLAKE3 hash (for Put operations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,

    /// Stone that performed the operation
    pub stone: String,
}

impl JournalEntry {
    /// Create a new Put entry
    pub fn put(key: &str, size: u64, hash: &str, stone: &str) -> Self {
        Self {
            id: crate::utils::ids::generate_guidv7(),
            op: JournalOp::Put,
            key: key.to_string(),
            size: Some(size),
            hash: Some(hash.to_string()),
            stone: stone.to_string(),
        }
    }

    /// Create a new Delete entry
    pub fn delete(key: &str, stone: &str) -> Self {
        Self {
            id: crate::utils::ids::generate_guidv7(),
            op: JournalOp::Delete,
            key: key.to_string(),
            size: None,
            hash: None,
            stone: stone.to_string(),
        }
    }
}

// ============================================================================
// Seed Bank Manifest (stored on device)
// ============================================================================

/// Manifest stored at `.zen-garden/manifest.json` on the seed bank
///
/// The manifest is the single source of truth for seed bank identity and configuration.
/// Mount paths are derived from this manifest, not from filesystem labels.
///
/// ## Version History
/// - v1: Original format (name, pool_id, visibility, origin_stone, filesystem)
/// - v2: Added group/replica_id for multi-device seed banks (STORAGE-0005)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedBankManifest {
    /// Version of the manifest format (current: 2)
    #[serde(default = "default_manifest_version")]
    pub version: u32,

    /// Unique seed bank identifier (GUIDv7)
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Pool identifier (first 4 hex of origin GUIDv7)
    pub pool_id: String,

    /// Logical group for replicated seed banks (e.g., "primary", "offsite")
    ///
    /// When set, multiple devices can form one logical seed bank.
    /// Mount path: `/mounts/{group}/replica-{replica_id}`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,

    /// Replica number within a group (1, 2, ...)
    ///
    /// Only meaningful when `group` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replica_id: Option<u32>,

    /// Visibility setting
    pub visibility: SeedBankVisibility,

    /// Stone that created this seed bank
    pub origin_stone: String,

    /// Filesystem type ("btrfs" or "ext4")
    pub filesystem: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

fn default_manifest_version() -> u32 {
    1 // Default for v1 manifests that don't have version field
}

impl SeedBankManifest {
    /// Current manifest version
    pub const CURRENT_VERSION: u32 = 2;

    /// Create a new manifest (simple, non-replicated seed bank)
    pub fn new(
        name: &str,
        origin_stone: &str,
        filesystem: &str,
        visibility: SeedBankVisibility,
    ) -> Self {
        let id = crate::utils::ids::generate_guidv7();
        let pool_id = SeedBankInfo::pool_id_from_guid(&id);

        Self {
            version: Self::CURRENT_VERSION,
            id,
            name: name.to_string(),
            pool_id,
            group: None,
            replica_id: None,
            visibility,
            origin_stone: origin_stone.to_string(),
            filesystem: filesystem.to_string(),
            created_at: Utc::now(),
        }
    }

    /// Create a new manifest for a replicated seed bank
    pub fn new_replica(
        name: &str,
        group: &str,
        replica_id: u32,
        origin_stone: &str,
        filesystem: &str,
        visibility: SeedBankVisibility,
    ) -> Self {
        let id = crate::utils::ids::generate_guidv7();
        let pool_id = SeedBankInfo::pool_id_from_guid(&id);

        Self {
            version: Self::CURRENT_VERSION,
            id,
            name: name.to_string(),
            pool_id,
            group: Some(group.to_string()),
            replica_id: Some(replica_id),
            visibility,
            origin_stone: origin_stone.to_string(),
            filesystem: filesystem.to_string(),
            created_at: Utc::now(),
        }
    }

    /// Derive the mount path for this seed bank
    ///
    /// - Replicated: `{base}/mounts/{group}/replica-{id}`
    /// - Grouped without replica: `{base}/mounts/{group}`
    /// - Simple: `{base}/mounts/{name}`
    pub fn derive_mount_path(&self, base_dir: &str) -> String {
        let mounts_dir = format!("{}/mounts", base_dir);

        match (&self.group, self.replica_id) {
            // Replicated seed bank: /mounts/{group}/replica-{id}
            (Some(group), Some(id)) => format!("{}/{}/replica-{}", mounts_dir, group, id),

            // Named group without replica: /mounts/{group}
            (Some(group), None) => format!("{}/{}", mounts_dir, group),

            // Simple seed bank: /mounts/{name}
            (None, _) => format!("{}/{}", mounts_dir, self.name),
        }
    }

    /// Get the logical seed bank identifier (group name or seed bank name)
    pub fn logical_name(&self) -> &str {
        self.group.as_deref().unwrap_or(&self.name)
    }

    /// Check if this is a replicated seed bank
    pub fn is_replica(&self) -> bool {
        self.group.is_some() && self.replica_id.is_some()
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

    /// List of available seed banks (empty = no storage)
    pub seed_banks: Vec<SeedBankAnnouncement>,

    /// Beacon timestamp
    pub timestamp: DateTime<Utc>,
}

impl StorageBeacon {
    /// Create an empty beacon (no seed banks)
    pub fn empty(stone_id: &str, stone_name: &str, endpoint: &str) -> Self {
        Self {
            stone_id: stone_id.to_string(),
            stone_name: stone_name.to_string(),
            endpoint: endpoint.to_string(),
            seed_banks: Vec::new(),
            timestamp: Utc::now(),
        }
    }

    /// Check if this stone has any storage capability
    pub fn has_storage(&self) -> bool {
        !self.seed_banks.is_empty()
    }

    /// Check if this stone supports S3 protocol
    pub fn supports_s3(&self) -> bool {
        self.seed_banks
            .iter()
            .any(|sb| sb.protocols.contains(&"s3".to_string()))
    }
}

/// Seed bank announcement entry for beacons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedBankAnnouncement {
    /// Unique seed bank ID (GUIDv7)
    pub id: String,

    /// Human-readable name
    pub name: String,

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
}

impl SeedBankAnnouncement {
    /// Create from SeedBankInfo
    pub fn from_info(info: &SeedBankInfo) -> Self {
        Self {
            id: info.id.clone(),
            name: info.name.clone(),
            protocols: vec!["s3".to_string(), "storage".to_string()],
            access: StorageAccess::Direct,
            visibility: info.visibility.to_string(),
            health: if info.online {
                if matches!(info.visibility, SeedBankVisibility::ReadOnly) {
                    "read-only".to_string()
                } else {
                    "healthy".to_string()
                }
            } else {
                "degraded".to_string()
            },
            capacity_bytes: info.capacity_bytes,
            used_bytes: info.used_bytes,
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
    fn test_pool_id_from_guid() {
        let guid = "01956a3e-7c00-7000-8000-000000000001";
        let pool_id = SeedBankInfo::pool_id_from_guid(guid);
        assert_eq!(pool_id, "0195");
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
        let manifest = SeedBankManifest::new(
            "test-bank",
            "stone-alpha",
            "btrfs",
            SeedBankVisibility::Open,
        );

        assert_eq!(manifest.version, 2);
        assert_eq!(manifest.name, "test-bank");
        assert_eq!(manifest.pool_id.len(), 4);
        assert!(manifest.pool_id.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(manifest.group.is_none());
        assert!(manifest.replica_id.is_none());
    }

    #[test]
    fn test_replica_manifest_creation() {
        let manifest = SeedBankManifest::new_replica(
            "primary-backup",
            "primary",
            1,
            "stone-alpha",
            "ext4",
            SeedBankVisibility::Open,
        );

        assert_eq!(manifest.version, 2);
        assert_eq!(manifest.name, "primary-backup");
        assert_eq!(manifest.group, Some("primary".to_string()));
        assert_eq!(manifest.replica_id, Some(1));
        assert!(manifest.is_replica());
    }

    #[test]
    fn test_mount_path_derivation() {
        let base = "/var/lib/zen-garden";

        // Simple seed bank
        let simple = SeedBankManifest::new("my-backup", "stone", "ext4", SeedBankVisibility::Open);
        assert_eq!(
            simple.derive_mount_path(base),
            "/var/lib/zen-garden/mounts/my-backup"
        );

        // Replicated seed bank
        let replica = SeedBankManifest::new_replica(
            "primary-backup",
            "primary",
            2,
            "stone",
            "ext4",
            SeedBankVisibility::Open,
        );
        assert_eq!(
            replica.derive_mount_path(base),
            "/var/lib/zen-garden/mounts/primary/replica-2"
        );
    }
}
