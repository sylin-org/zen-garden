//! Managed storage types (STORAGE-0009, STORAGE-0013)
//!
//! Shared contracts between Moss and Rake for managed storage.
//! Storage is the universal entity; seed bank is a composable role.
//!
//! ## Identity Model (STORAGE-0013)
//!
//! Two-level identity:
//! - **Device**: `id` (GUIDv7) + `name` (display sugar). One per physical device.
//! - **Replica set**: `replica_set_id` (GUIDv7) + `replica_set_name` (display sugar).
//!   Groups devices that replicate the same content.
//!
//! FQN convention: empty name = default set ("storage"), named = "storage::{name}".

use crate::OfferingMode;
use crate::manifests::Offering as OfferingManifest;
use crate::types::Offering;
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

/// Information about a detected storage device (partition/volume level)
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
// Medium Detection Types (physical disk layer)
// ============================================================================

/// Bus type of the physical connection (matches infra::storage::platform::BusType).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BusType {
    Usb,
    Sata,
    Nvme,
    Scsi,
    Mmc,
    Unknown,
}

impl std::fmt::Display for BusType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usb => write!(f, "USB"),
            Self::Sata => write!(f, "SATA"),
            Self::Nvme => write!(f, "NVMe"),
            Self::Scsi => write!(f, "SCSI"),
            Self::Mmc => write!(f, "MMC"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Candidate-stage condition of a physical medium (STORAGE-0019).
///
/// The five-state taxonomy from STORAGE-0019 §"Candidate state taxonomy".
/// Each variant maps cleanly to a single user-facing action in
/// `garden-rake storage add`:
///
/// | Variant | Signals | Action offered |
/// |---|---|---|
/// | `Adoptable` | Has partitions and existing user files | `adopt` (preserves) or `format` (wipes) |
/// | `Empty` | Filesystem present, zero user files | `adopt` (use as-is) or `format` (reformat) |
/// | `Raw` | No partition table or no filesystem | `format` (single full-size storage) |
/// | `NoMedia` | Bridge enumerated, 0 bytes, no I/O errors | "Insert a drive into the enclosure" |
/// | `Unreachable` | Bridge enumerated, 0 bytes, I/O errors present | Auto-heal; on failure, friendly replug hint |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediumCondition {
    /// Has partitions with existing user files. Adoption preserves
    /// the data; formatting wipes it.
    ///
    /// Subsumes the legacy `Partitioned` value when serializing —
    /// older Moss versions emitted `partitioned` for any device with
    /// a partition table; the deserializer accepts that token via
    /// `#[serde(alias)]` and maps it here.
    #[serde(alias = "partitioned")]
    Adoptable,
    /// Has a filesystem but no user content. The user can adopt
    /// it as-is (and Moss will catalog the empty volume) or reformat.
    Empty,
    /// No partition table or no filesystem. The whole device is
    /// available for a fresh-format adoption.
    Raw,
    /// USB bridge or controller enumerated the device but reports
    /// zero bytes with no I/O errors — almost always an empty
    /// enclosure (no drive inserted). No software recovery helps.
    NoMedia,
    /// Bridge enumerated with zero bytes AND I/O errors observed.
    /// A bridge-firmware glitch the connectivity stage may be able
    /// to recover via SCSI rescan or USB re-authorization. Renamed
    /// from the legacy `Unreadable` for vocabulary consistency
    /// (the device is reachable on the bus but unresponsive — not
    /// unreadable per se).
    #[serde(alias = "unreadable")]
    Unreachable,
}

impl std::fmt::Display for MediumCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Adoptable => write!(f, "adoptable"),
            Self::Empty => write!(f, "empty"),
            Self::Raw => write!(f, "raw"),
            Self::NoMedia => write!(f, "no_media"),
            Self::Unreachable => write!(f, "unreachable"),
        }
    }
}

/// A partition on a detected medium (for API responses).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediumPartitionInfo {
    /// Partition number (1-based).
    pub index: u32,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Filesystem type if known (e.g., "NTFS", "ext4").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<String>,
    /// Drive letter (Windows) or mount point (Linux).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_path: Option<String>,
    /// Volume label if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Information about a physical storage medium (disk-level).
///
/// Host-only — never broadcast to the garden. Used for `rake storage candidates`
/// to show physical disks that may need partitioning or formatting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediumInfo {
    /// OS device identifier (e.g., `\\.\PhysicalDrive2`, `/dev/sdb`).
    pub device_id: String,
    /// Vendor/model name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Physical bus type.
    pub bus_type: BusType,
    /// Total size in bytes.
    pub size_bytes: u64,
    /// Whether the medium is external/removable.
    pub removable: bool,
    /// Physical condition.
    pub condition: MediumCondition,
    /// Partitions on this medium.
    pub partitions: Vec<MediumPartitionInfo>,
    /// Whether any partition is already managed by Zen Garden.
    pub managed: bool,
    /// Suggested action for the user.
    pub suggested_action: MediumAction,
}

/// What action the user should take for this medium.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediumAction {
    /// Disk needs partitioning and formatting before use.
    NeedsPartition,
    /// Disk has partition(s) but no filesystem — needs formatting.
    NeedsFormat,
    /// Disk is ready — has a mounted volume that can be added with `storage add`.
    Ready,
    /// Disk is already managed by Zen Garden.
    AlreadyManaged,
    /// Disk is unreadable — may need physical inspection.
    Unreadable,
}

impl std::fmt::Display for MediumAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsPartition => write!(f, "needs partition"),
            Self::NeedsFormat => write!(f, "needs format"),
            Self::Ready => write!(f, "ready"),
            Self::AlreadyManaged => write!(f, "already managed"),
            Self::Unreadable => write!(f, "unreadable"),
        }
    }
}

/// Combined candidates response for GET /api/v1/stone/storage/candidates.
///
/// Returns both partition-level candidates (mounted volumes ready for `storage add`)
/// and physical media (disks that may need partitioning/formatting).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidatesResponse {
    /// Mounted volumes eligible for `storage add` (unmanaged, removable, online).
    pub spaces: Vec<StorageDetectedInfo>,
    /// Physical media visible to this stone (USB drives, external disks).
    /// Includes disks without partitions or drive letters.
    pub media: Vec<MediumInfo>,
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

/// Reserved display name for the default replica set (STORAGE-0013).
/// When `replica_set_name` is empty, this is the display moniker.
pub const DEFAULT_REPLICA_SET_DISPLAY: &str = "storage";

impl std::fmt::Display for StorageVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageVisibility::Open => write!(f, "{}", crate::constants::VISIBILITY_OPEN),
            StorageVisibility::Closed => write!(f, "{}", crate::constants::VISIBILITY_CLOSED),
            StorageVisibility::ReadOnly => write!(f, "{}", crate::constants::VISIBILITY_READ_ONLY),
        }
    }
}

/// Information about a managed storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageInfo {
    /// Unique device identifier (GUIDv7)
    pub id: String,

    /// Human-readable name — logical FQN shared across replicas
    pub name: String,

    /// Replica set ID (GUIDv7). Groups devices that replicate the same content (STORAGE-0013).
    #[serde(default)]
    pub replica_set_id: String,

    /// Replica set display name (STORAGE-0013).
    #[serde(default)]
    pub replica_set_name: String,

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
    #[expect(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        replica_set_id: String,
        replica_set_name: String,
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
            replica_set_id,
            replica_set_name,
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
        self.roles
            .iter()
            .any(|r| r == crate::constants::ROLE_SEED_BANK)
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
    /// First 8 hex chars of the device GUIDv7
    pub short_id: String,

    /// Device display name (sugar)
    pub name: String,

    /// Replica set ID (STORAGE-0013)
    #[serde(default)]
    pub replica_set_id: String,

    /// Replica set display name (STORAGE-0013). Empty = default "storage".
    #[serde(default)]
    pub replica_set_name: String,

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
    /// Build from a beacon announcement (remote stone).
    pub fn from_announcement(ann: &StorageAnnouncement, stone_name: &str) -> Self {
        Self {
            short_id: short_id_from_guid(&ann.id),
            name: ann.name.clone(),
            replica_set_id: ann.replica_set_id.clone(),
            replica_set_name: ann.replica_set_name.clone(),
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

    /// Display name for the replica set.
    pub fn replica_set_display_name(&self) -> &str {
        if self.replica_set_name.is_empty() {
            DEFAULT_REPLICA_SET_DISPLAY
        } else {
            &self.replica_set_name
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
// Garden Storage Wire Types
// ============================================================================
//
// Wire shapes returned by `/api/v1/garden/storage` and the user-content
// listing endpoints under `/api/v1/garden/storage/{name}/fs`. Shared
// between Moss (server) and clients (Pavilion's Cloud Filter provider,
// Rake) so the contract is enforced at the type level.

/// Summary of a storage visible across the garden.
///
/// Returned by `GET /api/v1/garden/storage`. Aggregates local managed
/// storages with remote registry beacons, grouped by storage name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GardenStorageSummary {
    pub name: String,
    pub replica_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_stone: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
}

/// Directory listing response from the garden user-content endpoint.
///
/// Returned by `GET /api/v1/garden/storage/{name}/fs[?path=&depth=N]`
/// and by `GET /api/v1/garden/storage/{name}/fs/{*path}` when `path`
/// resolves to a directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryListResponse {
    pub path: String,
    pub entries: Vec<DirectoryEntry>,
    pub truncated: bool,
}

/// Single entry in a garden directory listing.
///
/// `entry_type` is `"file"` or `"dir"`; `size` and `modified` are
/// present only for file entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

impl DirectoryEntry {
    /// Whether this entry is a directory (vs a file).
    pub fn is_dir(&self) -> bool {
        self.entry_type == "dir"
    }
}

// ============================================================================
// Filesystem Capabilities (STORAGE-0019)
// ============================================================================
//
// Tier-based capability model so the data plane and election logic know
// what each managed drive can guarantee. Three tiers map to "what kind of
// filesystem is this in operational terms":
//
// - `Native` — ext4 / btrfs / xfs. Full POSIX semantics; Moss's primary
//   target.
// - `Foreign` — NTFS / exFAT / ReFS. Read-write supported via Linux
//   drivers (ntfs3, exfat). Replication works; some POSIX-specific
//   attributes flatten on cross-tier round-trips.
// - `ForeignReadOnly` — APFS / HFS+ on Linux today. Adopt as a library;
//   never as a write target.

/// Operational tier of the managed filesystem.
///
/// Used by election (Native preferred for Primary), capability gating
/// (`ForeignReadOnly` excluded from write paths), and CLI rendering
/// (`<family> (<filesystem>)` labels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsTier {
    /// ext4 / btrfs / xfs — full Moss semantics.
    Native,
    /// NTFS / exFAT / ReFS — read-write with attribute caveats.
    Foreign,
    /// APFS / HFS+ — read-only on Linux.
    ForeignReadOnly,
}

impl FsTier {
    /// Whether the tier is read-only at the filesystem level.
    pub fn is_read_only(&self) -> bool {
        matches!(self, FsTier::ForeignReadOnly)
    }

    /// Whether this tier can take the Primary role in a replica set.
    /// `ForeignReadOnly` cannot accept writes, so it cannot be Primary.
    pub fn can_be_primary(&self) -> bool {
        !self.is_read_only()
    }
}

impl std::fmt::Display for FsTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsTier::Native => write!(f, "native"),
            FsTier::Foreign => write!(f, "foreign"),
            FsTier::ForeignReadOnly => write!(f, "foreign-readonly"),
        }
    }
}

/// What a managed filesystem can guarantee, declared at mount time.
///
/// Consumers (replication, election, classifier) read these bits to
/// decide what behaviors are safe on this drive. New fields can be
/// added without breaking older clients — every consumer matches on the
/// fields it cares about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsCapabilities {
    pub tier: FsTier,
    /// Filesystem distinguishes `Photo.JPG` from `photo.jpg`. NTFS
    /// case-folds by default; ext4/btrfs are case-sensitive.
    pub case_sensitive: bool,
    /// Filesystem honors POSIX permission bits (`0644`, `0700`, …).
    /// NTFS via ntfs3 simulates uid/gid but cannot enforce mode bits.
    pub posix_permissions: bool,
    /// Filesystem supports Linux extended attributes (xattrs).
    pub xattrs: bool,
    /// `rename(2)` is atomic with respect to readers and crash recovery.
    /// All major filesystems support this; carried explicitly so a
    /// future tier (e.g. raw FAT12) can opt out.
    pub atomic_rename: bool,
    /// Filesystem supports sparse files (holes that don't allocate
    /// blocks). Useful for replication of pre-allocated containers.
    pub sparse_files: bool,
    /// Maximum filename length in bytes. NTFS: 255. ext4: 255.
    /// FAT32: 255 in long-filename mode, 11 (8.3) in short.
    pub max_filename_bytes: u32,
}

impl FsCapabilities {
    /// Capabilities for ext4 / btrfs / xfs — full Moss semantics.
    pub const fn native() -> Self {
        Self {
            tier: FsTier::Native,
            case_sensitive: true,
            posix_permissions: true,
            xattrs: true,
            atomic_rename: true,
            sparse_files: true,
            max_filename_bytes: 255,
        }
    }

    /// Capabilities for NTFS via the Linux ntfs3 driver.
    pub const fn ntfs() -> Self {
        Self {
            tier: FsTier::Foreign,
            case_sensitive: false,
            posix_permissions: false,
            xattrs: false,
            atomic_rename: true,
            sparse_files: true,
            max_filename_bytes: 255,
        }
    }

    /// Capabilities for exFAT.
    pub const fn exfat() -> Self {
        Self {
            tier: FsTier::Foreign,
            case_sensitive: false,
            posix_permissions: false,
            xattrs: false,
            atomic_rename: true,
            sparse_files: false,
            max_filename_bytes: 255,
        }
    }

    /// Capabilities for FAT32 (long-filename mode).
    pub const fn fat32() -> Self {
        Self {
            tier: FsTier::Foreign,
            case_sensitive: false,
            posix_permissions: false,
            xattrs: false,
            atomic_rename: true,
            sparse_files: false,
            max_filename_bytes: 255,
        }
    }

    /// Capabilities for APFS / HFS+ via apfs-fuse — read-only.
    pub const fn apfs_readonly() -> Self {
        Self {
            tier: FsTier::ForeignReadOnly,
            case_sensitive: true,
            posix_permissions: true,
            xattrs: true,
            atomic_rename: true,
            sparse_files: true,
            max_filename_bytes: 255,
        }
    }

    /// Look up canonical capabilities by lowercase filesystem token.
    /// Returns `None` for unrecognized filesystems — callers may
    /// fall back to a conservative default or refuse adoption.
    pub fn for_filesystem(fs: &str) -> Option<Self> {
        match fs.to_ascii_lowercase().as_str() {
            "ext2" | "ext3" | "ext4" | "btrfs" | "xfs" | "f2fs" | "zfs" => {
                Some(Self::native())
            }
            "ntfs" | "ntfs3" | "refs" => Some(Self::ntfs()),
            "exfat" => Some(Self::exfat()),
            "fat" | "fat16" | "fat32" | "vfat" => Some(Self::fat32()),
            "apfs" | "hfs+" | "hfsplus" => Some(Self::apfs_readonly()),
            _ => None,
        }
    }
}

// ============================================================================
// Connectivity Status (STORAGE-0019)
// ============================================================================
//
// The connectivity-recovery stage that sits between the storage listener
// and the classifier produces a status companion for every event it
// processes. The status records what was attempted and what residual
// concerns remain, even on success — useful telemetry for "this drive
// is flaky, watch it" without blocking adoption.

/// Action the connectivity helper took to recover a degraded device.
///
/// Recorded for tracing, telemetry, and the SSE
/// `storage.connectivity.recovered` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    /// `echo 1 > /sys/block/sdX/device/rescan` — re-issue INQUIRY +
    /// READ CAPACITY without disturbing the USB endpoint.
    ScsiRescan,
    /// `echo 0 > /sys/bus/usb/devices/<port>/authorized; …; echo 1 > …`
    /// — soft replug; kernel re-enumerates the USB device.
    UsbReauth,
}

impl std::fmt::Display for RecoveryAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoveryAction::ScsiRescan => write!(f, "scsi_rescan"),
            RecoveryAction::UsbReauth => write!(f, "usb_reauth"),
        }
    }
}

/// Residual concern carried alongside a recovered or healthy device.
///
/// Even when a device enumerates cleanly, the helper may surface a
/// non-fatal concern (e.g. one I/O error in the lifetime counter) for
/// downstream consumers to render or act on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConnectivityWarning {
    /// One or more I/O errors observed on the device since boot,
    /// even though it currently enumerates correctly. May indicate
    /// a flaky bridge, cable, or marginal drive.
    PriorIoErrors { count: u64 },
    /// Device has been re-enumerated more than once in the recent
    /// past (cable wiggle, intermittent power, etc.).
    RecentReauth { count: u32, window_seconds: u64 },
    /// Device is on a USB port that has historically required
    /// recovery to enumerate cleanly.
    PortHistoricallyTroubled { port: String },
}

/// Outcome of the connectivity-recovery stage for a single device.
///
/// Rides alongside the `PhysicalStorageEvent` into the classifier.
/// Even on the happy path (`recoveries_attempted == 0`) the status is
/// emitted so consumers always have a uniform shape to render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectivityStatus {
    /// How many recovery actions were attempted on this device.
    /// Zero on the happy path.
    pub recoveries_attempted: u32,
    /// The action that ultimately succeeded, if any. `None` means
    /// either no recovery was needed (`recoveries_attempted == 0`)
    /// or every attempted action failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovered_via: Option<RecoveryAction>,
    /// Wall-time spent on recovery, in milliseconds. Zero on the
    /// happy path.
    #[serde(default)]
    pub duration_ms: u64,
    /// Residual concerns that didn't block adoption but consumers
    /// may surface or log.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_warnings: Vec<ConnectivityWarning>,
}

impl ConnectivityStatus {
    /// The "device enumerated cleanly, no recovery needed" status.
    /// Carried on every healthy event so consumers don't need to
    /// `Option::unwrap_or_default` everywhere.
    pub fn healthy() -> Self {
        Self {
            recoveries_attempted: 0,
            recovered_via: None,
            duration_ms: 0,
            residual_warnings: Vec::new(),
        }
    }

    /// True when the helper attempted at least one recovery action.
    pub fn required_recovery(&self) -> bool {
        self.recoveries_attempted > 0
    }

    /// True when the helper attempted recovery and one of the actions
    /// succeeded. Used to decide whether to fire the
    /// `storage.connectivity.recovered` SSE event.
    pub fn was_recovered(&self) -> bool {
        self.recovered_via.is_some()
    }
}

impl Default for ConnectivityStatus {
    fn default() -> Self {
        Self::healthy()
    }
}

// ============================================================================
// Filesystem Label Rendering (STORAGE-0019)
// ============================================================================

/// Render a lowercase filesystem token as a user-facing label of the
/// form `<family> (<filesystem>)`.
///
/// Designed so casual users read the family ("oh, that's the format
/// Windows uses") and technical users read the filesystem name in
/// parentheses. Both audiences served by one column.
///
/// Examples:
///
/// | Input    | Output             |
/// |----------|--------------------|
/// | `"ext4"` | `"Linux (ext4)"`   |
/// | `"btrfs"`| `"Linux (btrfs)"`  |
/// | `"ntfs"` | `"Windows (NTFS)"` |
/// | `"exfat"`| `"Windows (exFAT)"`|
/// | `"apfs"` | `"Mac (APFS)"`    |
/// | `"zfs"`  | `"(zfs)"`          |
///
/// Unknown filesystems render as `(<token>)` without a family — the
/// blank is the honest signal that Moss recognizes the filesystem but
/// doesn't have an opinion about which OS family it belongs to.
pub fn render_fs_label(fs: &str) -> String {
    let lower = fs.to_ascii_lowercase();
    let family = fs_family(&lower);
    let pretty = fs_pretty_name(&lower).unwrap_or(fs);
    match family {
        Some(f) => format!("{f} ({pretty})"),
        None => format!("({pretty})"),
    }
}

/// OS-family classification for a lowercase filesystem token.
fn fs_family(lower: &str) -> Option<&'static str> {
    match lower {
        "ext2" | "ext3" | "ext4" | "btrfs" | "xfs" | "f2fs" | "jfs" | "reiserfs" => {
            Some("Linux")
        }
        "ntfs" | "ntfs3" | "fat" | "fat12" | "fat16" | "fat32" | "vfat" | "exfat" | "refs" => {
            Some("Windows")
        }
        "apfs" | "hfs+" | "hfsplus" | "hfs" => Some("Mac"),
        "iso9660" | "udf" => Some("Optical"),
        _ => None,
    }
}

/// Canonical user-facing spelling for a lowercase filesystem token.
fn fs_pretty_name(lower: &str) -> Option<&'static str> {
    match lower {
        "ext2" => Some("ext2"),
        "ext3" => Some("ext3"),
        "ext4" => Some("ext4"),
        "btrfs" => Some("btrfs"),
        "xfs" => Some("XFS"),
        "f2fs" => Some("F2FS"),
        "jfs" => Some("JFS"),
        "reiserfs" => Some("ReiserFS"),
        "ntfs" | "ntfs3" => Some("NTFS"),
        "exfat" => Some("exFAT"),
        "fat12" => Some("FAT12"),
        "fat16" => Some("FAT16"),
        "fat32" | "vfat" | "fat" => Some("FAT32"),
        "refs" => Some("ReFS"),
        "apfs" => Some("APFS"),
        "hfs+" | "hfsplus" => Some("HFS+"),
        "hfs" => Some("HFS"),
        "iso9660" => Some("ISO 9660"),
        "udf" => Some("UDF"),
        _ => None,
    }
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
    /// Replica set display name (STORAGE-0013: was `storage`)
    pub storage: String,
    /// Replica set ID (STORAGE-0013)
    #[serde(default)]
    pub replica_set_id: String,
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
/// - v5: Two-level identity — device + replica set. (STORAGE-0013)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageManifest {
    /// Version of the manifest format (current: 5)
    #[serde(default = "default_manifest_version")]
    pub version: u32,

    /// Unique identifier (GUIDv7) — one per physical device, never changes.
    pub id: String,

    /// Device display name (sugar). Unique per device, user-renamable.
    pub name: String,

    /// Replica set identifier (GUIDv7). Groups devices that replicate the same content.
    /// All devices with the same `replica_set_id` form a replica set.
    #[serde(default = "generate_guidv7_string")]
    pub replica_set_id: String,

    /// Replica set display name (sugar). Empty = default set ("storage").
    /// Named set FQN: "storage::{replica_set_name}".
    #[serde(default)]
    pub replica_set_name: String,

    /// Timestamp of last replica set rename. Used for rename catch-up
    /// when offline members reconnect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replica_set_name_updated_at: Option<DateTime<Utc>>,

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

fn generate_guidv7_string() -> String {
    crate::utils::ids::generate_guidv7()
}

fn default_manifest_version() -> u32 {
    1
}

fn default_seed_bank_role() -> Vec<String> {
    vec![crate::constants::ROLE_SEED_BANK.to_string()]
}

impl StorageManifest {
    pub const CURRENT_VERSION: u32 = 5;

    /// Create a new manifest with default roles (seed-bank).
    ///
    /// Generates both a device ID and a replica set ID. The device `name`
    /// is the user-visible device sugar. The `replica_set_name` defaults
    /// to empty (= default set "storage").
    pub fn new(
        name: &str,
        origin_stone: &str,
        filesystem: &str,
        visibility: StorageVisibility,
    ) -> Self {
        let id = crate::utils::ids::generate_guidv7();
        let replica_set_id = crate::utils::ids::generate_guidv7();

        Self {
            version: Self::CURRENT_VERSION,
            id,
            name: name.to_string(),
            replica_set_id,
            replica_set_name: String::new(),
            replica_set_name_updated_at: None,
            visibility,
            origin_stone: origin_stone.to_string(),
            filesystem: filesystem.to_string(),
            created_at: Utc::now(),
            encrypted: false,
            pond_fingerprint: None,
            roles: vec![crate::constants::ROLE_SEED_BANK.to_string()],
        }
    }

    /// Create a new manifest joining a specific replica set.
    pub fn new_in_set(
        name: &str,
        origin_stone: &str,
        filesystem: &str,
        visibility: StorageVisibility,
        replica_set_id: &str,
        replica_set_name: &str,
    ) -> Self {
        let mut manifest = Self::new(name, origin_stone, filesystem, visibility);
        manifest.replica_set_id = replica_set_id.to_string();
        manifest.replica_set_name = replica_set_name.to_string();
        manifest
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
        let short_id = short_id_from_guid(&self.id);
        format!("{}/mounts/{}/{}", base_dir, self.name, short_id)
    }

    /// Whether this storage has the seed-bank role
    pub fn is_seed_bank(&self) -> bool {
        self.roles
            .iter()
            .any(|r| r == crate::constants::ROLE_SEED_BANK)
    }

    /// Display name for the replica set — returns the reserved moniker
    /// "storage" for the default (empty-name) set.
    pub fn replica_set_display_name(&self) -> &str {
        if self.replica_set_name.is_empty() {
            DEFAULT_REPLICA_SET_DISPLAY
        } else {
            &self.replica_set_name
        }
    }

    /// Full FQN for the replica set: "storage" or "storage::{name}".
    pub fn replica_set_fqn(&self) -> String {
        if self.replica_set_name.is_empty() {
            "storage".to_string()
        } else {
            format!("storage::{}", self.replica_set_name)
        }
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
            StorageRole::Primary => write!(f, "{}", crate::constants::ROLE_PRIMARY),
            StorageRole::Dormant => write!(f, "{}", crate::constants::ROLE_DORMANT),
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

/// Storage announcement entry for beacons (STORAGE-0013: canonical wire type).
///
/// Two-level identity: device (`id`/`name`) + replica set (`replica_set_id`/`replica_set_name`).
/// This is the single wire format for beacon broadcast and registry storage.
/// Replaces the former `StorageMetadata` on `GardenTool`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageAnnouncement {
    // --- Device identity ---
    /// Unique device ID (GUIDv7). One per physical storage device.
    pub id: String,

    /// Device display name (sugar). User-renamable.
    pub name: String,

    // --- Replica set identity ---
    /// Replica set ID (GUIDv7). Groups devices that replicate the same content.
    #[serde(default)]
    pub replica_set_id: String,

    /// Replica set display name (sugar). Empty = default set ("storage").
    #[serde(default)]
    pub replica_set_name: String,

    /// Timestamp of last replica set rename. For catch-up on reconnect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replica_set_name_updated_at: Option<DateTime<Utc>>,

    // --- Runtime state ---
    /// Runtime role (Primary / Dormant)
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

    /// Whether content is encrypted
    #[serde(default)]
    pub encrypted: bool,

    /// Pin ID — a GUIDv7 that claims Primary by pin.
    /// Higher GUIDv7 wins in a conflict (last-pin-wins).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_id: Option<String>,

    /// Composable roles (e.g., ["seed-bank"])
    #[serde(default = "default_seed_bank_role")]
    pub roles: Vec<String>,
}

impl StorageAnnouncement {
    /// Display name for the replica set — returns "storage" for the default set.
    pub fn replica_set_display_name(&self) -> &str {
        if self.replica_set_name.is_empty() {
            DEFAULT_REPLICA_SET_DISPLAY
        } else {
            &self.replica_set_name
        }
    }

    /// Full FQN for the replica set: "storage" or "storage::{name}".
    pub fn replica_set_fqn(&self) -> String {
        if self.replica_set_name.is_empty() {
            "storage".to_string()
        } else {
            format!("storage::{}", self.replica_set_name)
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

// ============================================================================
// Standalone helpers (STORAGE-0013: moved off StorageInfo)
// ============================================================================

/// Derive the short ID (first 8 hex chars of a GUIDv7, excluding dashes).
///
/// Used as the per-device directory name under mounts/{name}/{short_id}/.
pub fn short_id_from_guid(guid: &str) -> String {
    guid.chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(8)
        .collect()
}

// ============================================================================
// Storage Changed Event (STORAGE-0013)
// ============================================================================

/// Domain event emitted when storage state changes.
///
/// Broadcast on `storage_changed_tx`. Consumers subscribe and react by
/// pulling fresh data from the Moss boundary.
#[derive(Debug, Clone)]
pub enum StorageChanged {
    /// A new device was added (mounted, classified as managed).
    Added {
        device_id: String,
        replica_set_id: String,
    },
    /// A device was removed (unmounted, released).
    Removed {
        device_id: String,
        replica_set_id: String,
    },
    /// A device's role changed (Primary ↔ Dormant).
    RoleChanged {
        device_id: String,
        replica_set_id: String,
        new_role: StorageRole,
    },
    /// A replica set was renamed.
    Renamed {
        replica_set_id: String,
        new_name: String,
    },
    /// A pin state changed on a device.
    PinChanged {
        device_id: String,
        replica_set_id: String,
    },
    /// Volumes were reclassified (broad change, re-pull everything).
    Reclassified,
    /// Storage device sensed — recognised and being measured. Triggers a brief
    /// "checking..." line before size data is available.
    Sensed { name: String, roles: Vec<String> },
    /// Managed storage connected or reconnected — size confirmed, triggers ribbon.
    Connected {
        name: String,
        roles: Vec<String>,
        used_bytes: u64,
        capacity_bytes: u64,
    },
    /// Storage released — triggers released ribbon.
    Released { name: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_id_from_guid() {
        let guid = "01956a3e-7c00-7000-8000-000000000001";
        let short = short_id_from_guid(guid);
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
        let manifest =
            StorageManifest::new("test-bank", "stone-alpha", "btrfs", StorageVisibility::Open);

        assert_eq!(manifest.version, 5);
        assert_eq!(manifest.name, "test-bank");
        assert!(!manifest.replica_set_id.is_empty());
        assert!(manifest.replica_set_name.is_empty());
        assert_eq!(manifest.replica_set_display_name(), "storage");
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

        assert_eq!(manifest.version, 5);
        assert!(manifest.encrypted);
        assert_eq!(manifest.pond_fingerprint.as_deref(), Some("abc123def456"));
    }

    #[test]
    fn test_mount_path_derivation() {
        let base = "/var/lib/zen-garden";

        // All seed banks use {name}/{short_id} now
        let manifest = StorageManifest::new("my-backup", "stone", "ext4", StorageVisibility::Open);
        let path = manifest.derive_mount_path(base);

        // Path should be: /var/lib/zen-garden/mounts/my-backup/{first 8 hex of id}
        let short_id = short_id_from_guid(&manifest.id);
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
            "01956a3e-7c00-7000-8000-rs0000000001".to_string(),
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
            vec![crate::constants::ROLE_SEED_BANK.to_string()],
        )
    }

    fn make_test_announcement() -> StorageAnnouncement {
        StorageAnnouncement {
            id: "01956a3e-7c00-7000-8000-000000000002".to_string(),
            name: "private-seed-bank".to_string(),
            replica_set_id: "019aaaaa-0000-7000-8000-000000000001".to_string(),
            replica_set_name: "personal".to_string(),
            replica_set_name_updated_at: None,
            role: StorageRole::Dormant,
            protocols: vec!["s3".to_string()],
            access: StorageAccess::Direct,
            visibility: "open".to_string(),
            health: "healthy".to_string(),
            capacity_bytes: 128 * 1024 * 1024 * 1024,
            used_bytes: 0,
            encrypted: true,
            pin_id: Some("019c6d5a-0000-7000-8000-000000000001".to_string()),
            roles: vec![crate::constants::ROLE_SEED_BANK.to_string()],
        }
    }

    #[test]
    fn test_summary_from_announcement() {
        let ann = make_test_announcement();
        let summary = StorageSummary::from_announcement(&ann, "stone-beta");

        assert_eq!(summary.short_id, "01956a3e");
        assert_eq!(summary.name, "private-seed-bank");
        assert_eq!(
            summary.replica_set_id,
            "019aaaaa-0000-7000-8000-000000000001"
        );
        assert_eq!(summary.replica_set_name, "personal");
        assert_eq!(summary.replica_set_display_name(), "personal");
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
            replica_set_id: String::new(),
            replica_set_name: String::new(),
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
            replica_set_id: String::new(),
            replica_set_name: String::new(),
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
            replica_set_id: String::new(),
            replica_set_name: String::new(),
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
    fn test_announcement_replica_set_display_name() {
        let ann = make_test_announcement();
        assert_eq!(ann.replica_set_display_name(), "personal");

        let mut default_ann = make_test_announcement();
        default_ann.replica_set_name = String::new();
        assert_eq!(default_ann.replica_set_display_name(), "storage");
    }

    #[test]
    fn test_manifest_replica_set_fqn() {
        let mut manifest =
            StorageManifest::new("seed-01", "stone-alpha", "btrfs", StorageVisibility::Open);

        // Default set → "storage"
        assert_eq!(manifest.replica_set_fqn(), "storage");

        // Named set → "storage::images"
        manifest.replica_set_name = "images".to_string();
        assert_eq!(manifest.replica_set_fqn(), "storage::images");
    }

    #[test]
    fn test_manifest_new_in_set() {
        let manifest = StorageManifest::new_in_set(
            "seed-02",
            "stone-beta",
            "ext4",
            StorageVisibility::Open,
            "existing-set-id",
            "images",
        );

        assert_eq!(manifest.replica_set_id, "existing-set-id");
        assert_eq!(manifest.replica_set_name, "images");
        assert_eq!(manifest.replica_set_fqn(), "storage::images");
    }

    #[test]
    fn test_storage_changed_variants() {
        // Ensure all variants compile and debug-print
        let events = vec![
            StorageChanged::Added {
                device_id: "d1".into(),
                replica_set_id: "r1".into(),
            },
            StorageChanged::Removed {
                device_id: "d1".into(),
                replica_set_id: "r1".into(),
            },
            StorageChanged::RoleChanged {
                device_id: "d1".into(),
                replica_set_id: "r1".into(),
                new_role: StorageRole::Primary,
            },
            StorageChanged::Renamed {
                replica_set_id: "r1".into(),
                new_name: "photos".into(),
            },
            StorageChanged::PinChanged {
                device_id: "d1".into(),
                replica_set_id: "r1".into(),
            },
            StorageChanged::Reclassified,
        ];
        for e in &events {
            let _ = format!("{:?}", e);
        }
    }

    // ====================================================================
    // Garden Storage Wire Type round-trips
    // (DirectoryEntry, DirectoryListResponse, GardenStorageSummary)
    //
    // These tests pin the on-the-wire JSON shape that Moss serves and
    // Pavilion's Cloud Filter provider consumes. Each scenario exercises
    // a real serialize → deserialize round trip so any rename, default,
    // or rename_all attribute drift fails immediately.
    // ====================================================================

    #[test]
    fn directory_entry_file_roundtrips_through_json() {
        let original = DirectoryEntry {
            name: "vacation.jpg".to_string(),
            entry_type: "file".to_string(),
            size: Some(1_500_000),
            modified: Some("2026-01-01T00:00:00Z".to_string()),
        };
        let json = serde_json::to_string(&original).unwrap();
        // Wire field is `type`, not `entry_type`.
        assert!(json.contains("\"type\":\"file\""));
        assert!(json.contains("\"size\":1500000"));
        let parsed: DirectoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, original.name);
        assert_eq!(parsed.entry_type, original.entry_type);
        assert_eq!(parsed.size, original.size);
        assert_eq!(parsed.modified, original.modified);
        assert!(!parsed.is_dir());
    }

    #[test]
    fn directory_entry_dir_omits_optional_fields() {
        let original = DirectoryEntry {
            name: "photos".to_string(),
            entry_type: "dir".to_string(),
            size: None,
            modified: None,
        };
        let json = serde_json::to_string(&original).unwrap();
        // `size` and `modified` are skip_serializing_if Option::is_none.
        assert!(!json.contains("\"size\""));
        assert!(!json.contains("\"modified\""));
        let parsed: DirectoryEntry = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_dir());
        assert_eq!(parsed.size, None);
        assert_eq!(parsed.modified, None);
    }

    #[test]
    fn directory_entry_tolerates_unknown_future_fields() {
        // Forward compatibility: Moss may grow the wire shape in future
        // versions. Pavilion (older client) must keep parsing it.
        let json = r#"{
            "name": "newfile.txt",
            "type": "file",
            "size": 100,
            "owner": "alice",
            "permissions": "0644"
        }"#;
        let parsed: DirectoryEntry = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.name, "newfile.txt");
        assert_eq!(parsed.size, Some(100));
    }

    #[test]
    fn directory_list_response_roundtrip() {
        let original = DirectoryListResponse {
            path: "/photos".to_string(),
            entries: vec![
                DirectoryEntry {
                    name: "a.jpg".into(),
                    entry_type: "file".into(),
                    size: Some(42),
                    modified: None,
                },
                DirectoryEntry {
                    name: "subdir".into(),
                    entry_type: "dir".into(),
                    size: None,
                    modified: None,
                },
            ],
            truncated: false,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: DirectoryListResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.path, original.path);
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[0].name, "a.jpg");
        assert!(parsed.entries[1].is_dir());
        assert!(!parsed.truncated);
    }

    #[test]
    fn directory_list_response_empty_directory() {
        let json = r#"{
            "path": "/",
            "entries": [],
            "truncated": false
        }"#;
        let parsed: DirectoryListResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.entries.is_empty());
        assert_eq!(parsed.path, "/");
    }

    #[test]
    fn directory_list_response_truncated_flag_round_trips() {
        let original = DirectoryListResponse {
            path: "/big-dir".into(),
            entries: vec![],
            truncated: true,
        };
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("\"truncated\":true"));
        let parsed: DirectoryListResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.truncated);
    }

    #[test]
    fn garden_storage_summary_with_primary_stone() {
        let original = GardenStorageSummary {
            name: "storage".into(),
            replica_count: 2,
            primary_stone: Some("stone-alpha".into()),
            roles: vec!["seed-bank".into()],
        };
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("\"primary_stone\":\"stone-alpha\""));
        let parsed: GardenStorageSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "storage");
        assert_eq!(parsed.replica_count, 2);
        assert_eq!(parsed.primary_stone.as_deref(), Some("stone-alpha"));
        assert_eq!(parsed.roles, vec!["seed-bank".to_string()]);
    }

    #[test]
    fn garden_storage_summary_without_primary_stone() {
        let original = GardenStorageSummary {
            name: "personal".into(),
            replica_count: 0,
            primary_stone: None,
            roles: vec![],
        };
        let json = serde_json::to_string(&original).unwrap();
        // primary_stone is skip_serializing_if Option::is_none.
        assert!(!json.contains("\"primary_stone\""));
        let parsed: GardenStorageSummary = serde_json::from_str(&json).unwrap();
        assert!(parsed.primary_stone.is_none());
        assert!(parsed.roles.is_empty());
    }

    #[test]
    fn garden_storage_summary_accepts_explicit_null_primary_stone() {
        // Some servers may emit `primary_stone: null` rather than omitting
        // the field. Both shapes must parse — parity with how Moss's
        // current handler emits the value.
        let json = r#"{
            "name": "personal",
            "replica_count": 1,
            "primary_stone": null,
            "roles": []
        }"#;
        let parsed: GardenStorageSummary = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.name, "personal");
        assert!(parsed.primary_stone.is_none());
    }

    #[test]
    fn garden_storage_summary_defaults_roles_when_omitted() {
        // `roles` is `#[serde(default)]` so older Moss versions that
        // didn't emit the field still parse correctly.
        let json = r#"{
            "name": "legacy",
            "replica_count": 1
        }"#;
        let parsed: GardenStorageSummary = serde_json::from_str(json).unwrap();
        assert!(parsed.roles.is_empty());
        assert!(parsed.primary_stone.is_none());
    }

    // ====================================================================
    // STORAGE-0019: FsTier, FsCapabilities round-trips
    // ====================================================================

    #[test]
    fn fs_tier_serializes_as_snake_case() {
        assert_eq!(serde_json::to_string(&FsTier::Native).unwrap(), "\"native\"");
        assert_eq!(serde_json::to_string(&FsTier::Foreign).unwrap(), "\"foreign\"");
        assert_eq!(
            serde_json::to_string(&FsTier::ForeignReadOnly).unwrap(),
            "\"foreign_read_only\""
        );
    }

    #[test]
    fn fs_tier_round_trips_through_json() {
        for tier in [FsTier::Native, FsTier::Foreign, FsTier::ForeignReadOnly] {
            let json = serde_json::to_string(&tier).unwrap();
            let parsed: FsTier = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, tier);
        }
    }

    #[test]
    fn fs_tier_capability_predicates() {
        assert!(FsTier::Native.can_be_primary());
        assert!(FsTier::Foreign.can_be_primary());
        assert!(!FsTier::ForeignReadOnly.can_be_primary());
        assert!(FsTier::ForeignReadOnly.is_read_only());
        assert!(!FsTier::Native.is_read_only());
    }

    #[test]
    fn fs_capabilities_native_round_trips() {
        let caps = FsCapabilities::native();
        assert_eq!(caps.tier, FsTier::Native);
        assert!(caps.case_sensitive);
        assert!(caps.posix_permissions);
        assert!(caps.xattrs);
        let json = serde_json::to_string(&caps).unwrap();
        let parsed: FsCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, caps);
    }

    #[test]
    fn fs_capabilities_ntfs_correctly_models_foreign_tier() {
        let caps = FsCapabilities::ntfs();
        assert_eq!(caps.tier, FsTier::Foreign);
        assert!(!caps.case_sensitive, "NTFS is case-insensitive by default");
        assert!(!caps.posix_permissions, "NTFS via ntfs3 cannot enforce POSIX modes");
        assert!(!caps.xattrs);
        assert!(caps.atomic_rename);
        assert!(caps.sparse_files);
    }

    #[test]
    fn fs_capabilities_apfs_is_read_only() {
        let caps = FsCapabilities::apfs_readonly();
        assert!(caps.tier.is_read_only());
        assert!(!caps.tier.can_be_primary());
    }

    #[test]
    fn fs_capabilities_for_filesystem_token_lookup() {
        // Linux family
        assert_eq!(
            FsCapabilities::for_filesystem("ext4").map(|c| c.tier),
            Some(FsTier::Native)
        );
        assert_eq!(
            FsCapabilities::for_filesystem("BTRFS").map(|c| c.tier),
            Some(FsTier::Native),
            "lookup is case-insensitive"
        );
        // Windows family
        assert_eq!(
            FsCapabilities::for_filesystem("ntfs").map(|c| c.tier),
            Some(FsTier::Foreign)
        );
        assert_eq!(
            FsCapabilities::for_filesystem("exfat").map(|c| c.tier),
            Some(FsTier::Foreign)
        );
        // Mac family
        assert_eq!(
            FsCapabilities::for_filesystem("apfs").map(|c| c.tier),
            Some(FsTier::ForeignReadOnly)
        );
        // Unknown
        assert!(FsCapabilities::for_filesystem("zfs-encrypted").is_none());
    }

    // ====================================================================
    // STORAGE-0019: ConnectivityStatus round-trips
    // ====================================================================

    #[test]
    fn connectivity_status_healthy_omits_optional_fields() {
        let status = ConnectivityStatus::healthy();
        assert!(!status.required_recovery());
        assert!(!status.was_recovered());
        let json = serde_json::to_string(&status).unwrap();
        // recovered_via is None → skip_serializing_if applies.
        assert!(!json.contains("recovered_via"), "got: {json}");
        // residual_warnings is empty → skip_serializing_if applies.
        assert!(!json.contains("residual_warnings"), "got: {json}");
    }

    #[test]
    fn connectivity_status_recovered_round_trip() {
        let status = ConnectivityStatus {
            recoveries_attempted: 2,
            recovered_via: Some(RecoveryAction::UsbReauth),
            duration_ms: 4380,
            residual_warnings: vec![ConnectivityWarning::PriorIoErrors { count: 1 }],
        };
        assert!(status.required_recovery());
        assert!(status.was_recovered());
        let json = serde_json::to_string(&status).unwrap();
        let parsed: ConnectivityStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }

    #[test]
    fn connectivity_status_default_is_healthy() {
        let s: ConnectivityStatus = Default::default();
        assert_eq!(s, ConnectivityStatus::healthy());
    }

    #[test]
    fn connectivity_warning_tagged_serialization() {
        // `kind` is the discriminator, so `PriorIoErrors` → `{"kind":"prior_io_errors", …}`.
        let w = ConnectivityWarning::PriorIoErrors { count: 3 };
        let json = serde_json::to_string(&w).unwrap();
        assert!(
            json.contains("\"kind\":\"prior_io_errors\""),
            "got: {json}"
        );
        let parsed: ConnectivityWarning = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, w);
    }

    #[test]
    fn connectivity_warning_recent_reauth_round_trips() {
        let w = ConnectivityWarning::RecentReauth {
            count: 4,
            window_seconds: 60,
        };
        let json = serde_json::to_string(&w).unwrap();
        let parsed: ConnectivityWarning = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, w);
    }

    #[test]
    fn recovery_action_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&RecoveryAction::ScsiRescan).unwrap(),
            "\"scsi_rescan\""
        );
        assert_eq!(
            serde_json::to_string(&RecoveryAction::UsbReauth).unwrap(),
            "\"usb_reauth\""
        );
    }

    // ====================================================================
    // STORAGE-0019: render_fs_label
    // ====================================================================

    #[test]
    fn render_fs_label_linux_family() {
        assert_eq!(render_fs_label("ext4"), "Linux (ext4)");
        assert_eq!(render_fs_label("btrfs"), "Linux (btrfs)");
        assert_eq!(render_fs_label("xfs"), "Linux (XFS)");
    }

    #[test]
    fn render_fs_label_windows_family() {
        assert_eq!(render_fs_label("ntfs"), "Windows (NTFS)");
        assert_eq!(render_fs_label("exfat"), "Windows (exFAT)");
        assert_eq!(render_fs_label("fat32"), "Windows (FAT32)");
        assert_eq!(render_fs_label("vfat"), "Windows (FAT32)");
        assert_eq!(render_fs_label("refs"), "Windows (ReFS)");
    }

    #[test]
    fn render_fs_label_mac_family() {
        assert_eq!(render_fs_label("apfs"), "Mac (APFS)");
        assert_eq!(render_fs_label("hfsplus"), "Mac (HFS+)");
        assert_eq!(render_fs_label("hfs+"), "Mac (HFS+)");
    }

    #[test]
    fn render_fs_label_optical_family() {
        assert_eq!(render_fs_label("iso9660"), "Optical (ISO 9660)");
        assert_eq!(render_fs_label("udf"), "Optical (UDF)");
    }

    #[test]
    fn render_fs_label_unknown_drops_family_keeps_token() {
        // Honest signal: Moss recognizes the filesystem name but doesn't
        // claim a family. Renders as `(token)` with the original casing.
        assert_eq!(render_fs_label("zfs"), "(zfs)");
        assert_eq!(render_fs_label("future-fs"), "(future-fs)");
    }

    #[test]
    fn render_fs_label_is_case_insensitive_on_input() {
        assert_eq!(render_fs_label("NTFS"), "Windows (NTFS)");
        assert_eq!(render_fs_label("Ext4"), "Linux (ext4)");
        assert_eq!(render_fs_label("APFS"), "Mac (APFS)");
    }
}
