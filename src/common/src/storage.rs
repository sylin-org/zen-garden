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

/// Default name for unencrypted (public) seed banks
pub const DEFAULT_PUBLIC_SEED_BANK_NAME: &str = "public-seed-bank";
/// Default name for encrypted (private / pond-scoped) seed banks
pub const DEFAULT_PRIVATE_SEED_BANK_NAME: &str = "private-seed-bank";

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

    /// Human-readable name — logical FQN shared across replicas (STORAGE-0006)
    pub name: String,

    /// Device path (e.g., "/dev/sdb1")
    pub device: String,

    /// Mount path under data_dir (e.g., "/var/lib/zen-garden/mounts/my-bank/01956a3e")
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

    /// Whether content on this seed bank is encrypted (STORAGE-0006)
    #[serde(default)]
    pub encrypted: bool,

    // === Backward-compat: ignored fields from v2 manifests ===
    /// Deprecated: pool_id. Ignored on deserialization.
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    pool_id: Option<String>,

    /// Deprecated: group. Ignored on deserialization.
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    group: Option<String>,

    /// Deprecated: replica_id. Ignored on deserialization.
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    replica_id: Option<u32>,
}

fn default_true() -> bool {
    true
}

impl SeedBankInfo {
    /// Create a new SeedBankInfo with all required fields.
    /// Deprecated backward-compat fields are set to None internally.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        device: String,
        mount_path: String,
        capacity_bytes: u64,
        used_bytes: u64,
        visibility: SeedBankVisibility,
        btrfs: bool,
        origin_stone: String,
        created_at: DateTime<Utc>,
        roaming: bool,
        online: bool,
        encrypted: bool,
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
            pool_id: None,
            group: None,
            replica_id: None,
        }
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
// Seed Bank Summary (shared formatting utility — STORAGE-0006 Phase 5)
// ============================================================================

/// Compact seed-bank summary for CLI display and portrait enrichment.
///
/// Reusable across `garden-rake seed-banks`, `release` disambiguation picker,
/// `pin` selection view, and the portrait endpoint. Avoids duplicating the
/// formatting logic in multiple places.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedBankSummary {
    /// First 8 hex chars of the GUIDv7
    pub short_id: String,

    /// Logical seed bank name (shared across replicas)
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
    pub role: SeedBankRole,

    /// Whether the Primary role is pinned (locked)
    #[serde(default)]
    pub pinned: bool,

    /// Whether content is encrypted
    #[serde(default)]
    pub encrypted: bool,

    /// Whether the device is online
    #[serde(default = "default_true")]
    pub online: bool,
}

impl SeedBankSummary {
    /// Build from a local `SeedBankInfo` plus runtime state.
    pub fn from_info(
        info: &SeedBankInfo,
        role: SeedBankRole,
        pinned: bool,
        stone_name: Option<&str>,
    ) -> Self {
        Self {
            short_id: SeedBankInfo::short_id(&info.id),
            name: info.name.clone(),
            capacity_gb: info.capacity_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
            device: Some(info.device.clone()),
            stone_name: stone_name.map(|s| s.to_string()),
            role,
            pinned,
            encrypted: info.encrypted,
            online: info.online,
        }
    }

    /// Build from a beacon announcement (remote stone).
    pub fn from_announcement(ann: &SeedBankAnnouncement, stone_name: &str) -> Self {
        Self {
            short_id: SeedBankInfo::short_id(&ann.id),
            name: ann.name.clone(),
            capacity_gb: ann.capacity_bytes as f32 / 1024.0 / 1024.0 / 1024.0,
            device: None,
            stone_name: Some(stone_name.to_string()),
            role: ann.role,
            pinned: ann.pin_id.is_some(),
            encrypted: ann.encrypted,
            online: true,
        }
    }

    /// Format a compact single-line summary for CLI display.
    ///
    /// Example: `● 01956a3e  64GB   stone-01  Primary  ★ pinned`
    pub fn format_line(&self) -> String {
        let marker = if self.role == SeedBankRole::Primary {
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

/// Request to prepare a seed bank
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareSeedBankRequest {
    /// Device path (e.g., "/dev/sdb1")
    pub device: String,

    /// Name for the seed bank (default: context-dependent — see STORAGE-0006 §12)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Generate a random name (e.g., "seed-kind-meadow")
    #[serde(default)]
    pub random_name: bool,

    /// Filesystem to use: "btrfs" (default) or "ext4"
    #[serde(default = "default_btrfs")]
    pub filesystem: String,

    /// Whether to encrypt content (pond-scoped, STORAGE-0006)
    #[serde(default)]
    pub encrypted: bool,

    // === Backward-compat: ignored fields from old API clients ===
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    group: Option<String>,
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    replica_id: Option<u32>,
}

impl PrepareSeedBankRequest {
    /// Create a new prepare request with required fields. Deprecated fields default to None.
    pub fn new(
        device: String,
        name: Option<String>,
        random_name: bool,
        filesystem: String,
        encrypted: bool,
    ) -> Self {
        Self {
            device,
            name,
            random_name,
            filesystem,
            encrypted,
            group: None,
            replica_id: None,
        }
    }
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

// PoolConflictData, MergePolicy, MergeSeedBankRequest — removed in STORAGE-0006 (dead code)

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
/// Appended by `SeedBankStore::write()` and `SeedBankStore::delete()`.
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
    /// Latest cursor on this seed bank
    pub cursor: String,
    /// Seed bank name
    pub seed_bank: String,
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
/// - v3: Removed group/replica_id/pool_id. Added encrypted/pond_fingerprint. (STORAGE-0006)
///   Identity simplified: `name` = FQN, `id` = physical. Same name = replicas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedBankManifest {
    /// Version of the manifest format (current: 3)
    #[serde(default = "default_manifest_version")]
    pub version: u32,

    /// Unique seed bank identifier (GUIDv7) — one per physical device, never changes
    pub id: String,

    /// Logical seed bank name (FQN). Shared across all replicas.
    /// Two devices with the same name and different IDs are replicas.
    pub name: String,

    /// Visibility setting
    pub visibility: SeedBankVisibility,

    /// Stone that created this seed bank
    pub origin_stone: String,

    /// Filesystem type ("btrfs" or "ext4")
    pub filesystem: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Whether content is encrypted (STORAGE-0006)
    #[serde(default)]
    pub encrypted: bool,

    /// Pond CA fingerprint (present when encrypted = true)
    /// Used to match the seed bank to the correct pond's data key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pond_fingerprint: Option<String>,

    // === Backward-compat: old fields silently ignored on deserialization ===
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    pool_id: Option<String>,
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    group: Option<String>,
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    replica_id: Option<u32>,
}

fn default_manifest_version() -> u32 {
    1 // Default for v1 manifests that don't have version field
}

impl SeedBankManifest {
    /// Current manifest version
    pub const CURRENT_VERSION: u32 = 3;

    /// Create a new manifest
    ///
    /// All seed banks — single or replicated — use this constructor.
    /// Two devices prepared with the same `name` are replicas (STORAGE-0006).
    pub fn new(
        name: &str,
        origin_stone: &str,
        filesystem: &str,
        visibility: SeedBankVisibility,
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
            pool_id: None,
            group: None,
            replica_id: None,
        }
    }

    /// Create a new manifest with encryption enabled (STORAGE-0006)
    pub fn new_encrypted(
        name: &str,
        origin_stone: &str,
        filesystem: &str,
        visibility: SeedBankVisibility,
        pond_fingerprint: &str,
    ) -> Self {
        let mut manifest = Self::new(name, origin_stone, filesystem, visibility);
        manifest.encrypted = true;
        manifest.pond_fingerprint = Some(pond_fingerprint.to_string());
        manifest
    }

    /// Derive the mount path for this seed bank (STORAGE-0006)
    ///
    /// All seed banks: `{base}/mounts/{name}/{short_id}/`
    /// Where short_id = first 8 hex chars of the GUIDv7.
    ///
    /// This 2-level scheme supports multiple replicas under the same name
    /// and gives the scanner a consistent walk pattern.
    pub fn derive_mount_path(&self, base_dir: &str) -> String {
        let short_id = SeedBankInfo::short_id(&self.id);
        format!("{}/mounts/{}/{}", base_dir, self.name, short_id)
    }

    /// Get the logical seed bank name (FQN)
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

/// Runtime role of a seed bank within its replica group (STORAGE-0006)
///
/// Assigned at runtime by the seed bank orchestration task.
/// Same pattern as OfferingRole (first-online-wins, reconciliation window).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SeedBankRole {
    /// Accepts writes. One primary per logical seed bank name.
    #[default]
    Primary,
    /// Read-only replica. Replicates from primary via SSE pull.
    Dormant,
}

impl std::fmt::Display for SeedBankRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeedBankRole::Primary => write!(f, "primary"),
            SeedBankRole::Dormant => write!(f, "dormant"),
        }
    }
}

/// Orchestration state for a logical seed bank (STORAGE-0006)
///
/// Persisted per seed bank name on the stone. Mirrors OfferingOrchestrationState.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedBankOrchestrationState {
    /// Current role of this seed bank on this stone
    pub role: SeedBankRole,

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

impl Default for SeedBankOrchestrationState {
    fn default() -> Self {
        Self {
            role: SeedBankRole::Primary,
            primary_stone_id: None,
            primary_seed_bank_id: None,
            pinned: false,
            pin_timestamp: None,
        }
    }
}

/// Seed bank announcement entry for beacons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedBankAnnouncement {
    /// Unique seed bank ID (GUIDv7)
    pub id: String,

    /// Human-readable name (logical FQN — shared across replicas)
    pub name: String,

    /// Runtime role (STORAGE-0006)
    #[serde(default)]
    pub role: SeedBankRole,

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
}

impl SeedBankAnnouncement {
    /// Create from SeedBankInfo
    pub fn from_info(info: &SeedBankInfo) -> Self {
        Self {
            id: info.id.clone(),
            name: info.name.clone(),
            role: SeedBankRole::default(),
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
            encrypted: info.encrypted,
            pin_id: None,
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
        let short = SeedBankInfo::short_id(guid);
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
        let manifest = SeedBankManifest::new(
            "test-bank",
            "stone-alpha",
            "btrfs",
            SeedBankVisibility::Open,
        );

        assert_eq!(manifest.version, 3);
        assert_eq!(manifest.name, "test-bank");
        assert!(!manifest.encrypted);
        assert!(manifest.pond_fingerprint.is_none());
    }

    #[test]
    fn test_encrypted_manifest_creation() {
        let manifest = SeedBankManifest::new_encrypted(
            "private-bank",
            "stone-alpha",
            "ext4",
            SeedBankVisibility::Open,
            "abc123def456",
        );

        assert_eq!(manifest.version, 3);
        assert!(manifest.encrypted);
        assert_eq!(manifest.pond_fingerprint.as_deref(), Some("abc123def456"));
    }

    #[test]
    fn test_mount_path_derivation() {
        let base = "/var/lib/zen-garden";

        // All seed banks use {name}/{short_id} now
        let manifest =
            SeedBankManifest::new("my-backup", "stone", "ext4", SeedBankVisibility::Open);
        let path = manifest.derive_mount_path(base);

        // Path should be: /var/lib/zen-garden/mounts/my-backup/{first 8 hex of id}
        let short_id = SeedBankInfo::short_id(&manifest.id);
        assert_eq!(
            path,
            format!("/var/lib/zen-garden/mounts/my-backup/{}", short_id)
        );
    }

    #[test]
    fn test_seed_bank_role_display() {
        assert_eq!(SeedBankRole::Primary.to_string(), "primary");
        assert_eq!(SeedBankRole::Dormant.to_string(), "dormant");
    }

    #[test]
    fn test_backward_compat_deserialize() {
        // Old v2 manifest with group/replica_id/pool_id should deserialize fine
        let json = r#"{
            "version": 2,
            "id": "01956a3e-7c00-7000-8000-000000000001",
            "name": "old-bank",
            "pool_id": "0195",
            "group": "primary",
            "replica_id": 1,
            "visibility": "open",
            "origin_stone": "stone-alpha",
            "filesystem": "btrfs",
            "created_at": "2026-01-01T00:00:00Z"
        }"#;
        let manifest: SeedBankManifest = serde_json::from_str(json).unwrap();
        assert_eq!(manifest.name, "old-bank");
        assert_eq!(manifest.version, 2);
        assert!(!manifest.encrypted);
    }

    // ====================================================================
    // SeedBankSummary tests
    // ====================================================================

    fn make_test_info() -> SeedBankInfo {
        SeedBankInfo::new(
            "01956a3e-7c00-7000-8000-000000000001".to_string(),
            "public-seed-bank".to_string(),
            "/dev/sdb1".to_string(),
            "/var/lib/zen-garden/mounts/public-seed-bank/01956a3e".to_string(),
            64 * 1024 * 1024 * 1024, // 64 GB
            10 * 1024 * 1024 * 1024, // 10 GB used
            SeedBankVisibility::Open,
            true,
            "stone-alpha".to_string(),
            Utc::now(),
            false,
            true,
            false,
        )
    }

    fn make_test_announcement() -> SeedBankAnnouncement {
        SeedBankAnnouncement {
            id: "01956a3e-7c00-7000-8000-000000000002".to_string(),
            name: "private-seed-bank".to_string(),
            role: SeedBankRole::Dormant,
            protocols: vec!["s3".to_string()],
            access: StorageAccess::Direct,
            visibility: "open".to_string(),
            health: "healthy".to_string(),
            capacity_bytes: 128 * 1024 * 1024 * 1024,
            used_bytes: 0,
            encrypted: true,
            pin_id: Some("019c6d5a-0000-7000-8000-000000000001".to_string()),
        }
    }

    #[test]
    fn test_summary_from_info() {
        let info = make_test_info();
        let summary =
            SeedBankSummary::from_info(&info, SeedBankRole::Primary, false, Some("stone-alpha"));

        assert_eq!(summary.short_id, "01956a3e");
        assert_eq!(summary.name, "public-seed-bank");
        assert_eq!(summary.capacity_gb as u32, 64);
        assert_eq!(summary.device.as_deref(), Some("/dev/sdb1"));
        assert_eq!(summary.stone_name.as_deref(), Some("stone-alpha"));
        assert_eq!(summary.role, SeedBankRole::Primary);
        assert!(!summary.pinned);
        assert!(!summary.encrypted);
        assert!(summary.online);
    }

    #[test]
    fn test_summary_from_info_pinned_encrypted() {
        let mut info = make_test_info();
        info.encrypted = true;
        let summary = SeedBankSummary::from_info(&info, SeedBankRole::Dormant, true, None);

        assert_eq!(summary.role, SeedBankRole::Dormant);
        assert!(summary.pinned);
        assert!(summary.encrypted);
        assert!(summary.stone_name.is_none());
    }

    #[test]
    fn test_summary_from_announcement() {
        let ann = make_test_announcement();
        let summary = SeedBankSummary::from_announcement(&ann, "stone-beta");

        assert_eq!(summary.short_id, "01956a3e");
        assert_eq!(summary.name, "private-seed-bank");
        assert_eq!(summary.capacity_gb as u32, 128);
        assert!(summary.device.is_none()); // remote — no device
        assert_eq!(summary.stone_name.as_deref(), Some("stone-beta"));
        assert_eq!(summary.role, SeedBankRole::Dormant);
        assert!(summary.pinned);
        assert!(summary.encrypted);
        assert!(summary.online); // remote assumed online
    }

    #[test]
    fn test_format_line_primary() {
        let summary = SeedBankSummary {
            short_id: "01956a3e".to_string(),
            name: "public-seed-bank".to_string(),
            capacity_gb: 64.0,
            device: Some("/dev/sdb1".to_string()),
            stone_name: Some("stone-01".to_string()),
            role: SeedBankRole::Primary,
            pinned: false,
            encrypted: false,
            online: true,
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
        let summary = SeedBankSummary {
            short_id: "0195b2c4".to_string(),
            name: "private-seed-bank".to_string(),
            capacity_gb: 128.0,
            device: None,
            stone_name: Some("stone-02".to_string()),
            role: SeedBankRole::Dormant,
            pinned: true,
            encrypted: true,
            online: true,
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
        let summary = SeedBankSummary {
            short_id: "abcdef01".to_string(),
            name: "test".to_string(),
            capacity_gb: 32.0,
            device: None,
            stone_name: None,
            role: SeedBankRole::Primary,
            pinned: false,
            encrypted: false,
            online: true,
        };
        let line = summary.format_line();
        assert!(line.contains("local"));
    }

    #[test]
    fn test_announcement_from_info_sets_defaults() {
        let info = make_test_info();
        let ann = SeedBankAnnouncement::from_info(&info);

        assert_eq!(ann.role, SeedBankRole::Primary); // default
        assert!(ann.pin_id.is_none()); // always None from_info
        assert!(!ann.encrypted);
        assert_eq!(ann.health, "healthy");
        assert_eq!(ann.capacity_bytes, 64 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_announcement_from_info_read_only() {
        let mut info = make_test_info();
        info.visibility = SeedBankVisibility::ReadOnly;
        let ann = SeedBankAnnouncement::from_info(&info);
        assert_eq!(ann.health, "read-only");
    }

    #[test]
    fn test_announcement_from_info_offline() {
        let mut info = make_test_info();
        info.online = false;
        let ann = SeedBankAnnouncement::from_info(&info);
        assert_eq!(ann.health, "degraded");
    }
}
