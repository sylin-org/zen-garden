//! Platform-agnostic value types for storage volumes and physical media.
//!
//! These are pure data types — serde-derived, no I/O, no side effects.
//! They live in domain because domain code depends on them. The infra
//! platform adapter produces them; the domain consumes them.

use serde::{Deserialize, Serialize};

// ============================================================================
// Volume snapshot
// ============================================================================

/// Snapshot of a volume as seen by the OS.
///
/// No domain concepts — just what the platform reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSnapshot {
    /// Device identifier: `/dev/sdb1` on Linux, `E:\` on Windows.
    pub path: String,

    /// Where the volume's content is accessible.
    /// On Linux this is the mount point (e.g. `/mnt/usb`).
    /// On Windows this equals `path` (drive letter IS the mount).
    pub mount_path: String,

    /// Filesystem label if available (e.g. "SANDISK_32GB").
    pub label: Option<String>,

    /// Total capacity in bytes.
    pub capacity_bytes: u64,

    /// Whether the OS considers this removable (USB, SD card, etc.).
    pub removable: bool,
}

// ============================================================================
// Disk usage
// ============================================================================

/// Disk usage result.
#[derive(Debug, Clone, Copy)]
pub struct DiskUsage {
    pub used_bytes: u64,
    pub available_bytes: u64,
}

impl DiskUsage {
    pub fn total(&self) -> u64 {
        self.used_bytes + self.available_bytes
    }
}

// ============================================================================
// Physical media types
// ============================================================================

/// Bus type of the physical connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BusType {
    Usb,
    Sata,
    Nvme,
    Scsi,
    /// SD card, eMMC
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

/// Condition of a physical medium.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediumCondition {
    /// No partition table. Brand new or wiped disk.
    Raw,
    /// Has a partition table with 1+ partitions.
    Partitioned,
    /// Device is offline or reporting I/O errors.
    Unreadable,
}

impl std::fmt::Display for MediumCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Raw => write!(f, "raw"),
            Self::Partitioned => write!(f, "partitioned"),
            Self::Unreadable => write!(f, "unreadable"),
        }
    }
}

// Conversions to shared API types (garden_common::storage)
impl From<BusType> for garden_common::storage::BusType {
    fn from(b: BusType) -> Self {
        match b {
            BusType::Usb => Self::Usb,
            BusType::Sata => Self::Sata,
            BusType::Nvme => Self::Nvme,
            BusType::Scsi => Self::Scsi,
            BusType::Mmc => Self::Mmc,
            BusType::Unknown => Self::Unknown,
        }
    }
}

impl From<MediumCondition> for garden_common::storage::MediumCondition {
    fn from(c: MediumCondition) -> Self {
        match c {
            MediumCondition::Raw => Self::Raw,
            MediumCondition::Partitioned => Self::Partitioned,
            MediumCondition::Unreadable => Self::Unreadable,
        }
    }
}

/// A partition on a physical medium.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionSnapshot {
    /// Partition number (1-based).
    pub index: u32,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Filesystem type if known (e.g., "NTFS", "ext4").
    pub filesystem: Option<String>,
    /// Drive letter (Windows) or mount point (Linux), if assigned.
    pub mount_path: Option<String>,
    /// Volume label if available.
    pub label: Option<String>,
}

/// Snapshot of a physical storage medium (disk) as seen by the OS.
///
/// Represents the whole disk, not a partition. A medium can have 0 or more
/// partitions. Host-only — never broadcast to the garden.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediumSnapshot {
    /// OS-level device identifier.
    /// Windows: `\\.\PhysicalDrive2`, Linux: `/dev/sdb`
    pub device_id: String,

    /// Vendor/model name (e.g., "SanDisk Portable SSD").
    pub model: Option<String>,

    /// Serial number if available.
    pub serial: Option<String>,

    /// Physical bus type.
    pub bus_type: BusType,

    /// Total size in bytes (entire disk).
    pub size_bytes: u64,

    /// Whether the medium is external/removable.
    pub removable: bool,

    /// Physical condition.
    pub condition: MediumCondition,

    /// Partitions on this medium.
    pub partitions: Vec<PartitionSnapshot>,
}

// ============================================================================
// Device health
// ============================================================================

/// Platform-agnostic device health snapshot (STORAGE-0018).
///
/// Produced by `StoragePlatform::probe_device_health()`, consumed by
/// `Volume::observe()`. All fields are OS facts — the domain
/// decides what they mean.
#[derive(Debug, Clone, Copy)]
pub struct DeviceHealth {
    /// Basic I/O probe succeeded (statvfs or equivalent).
    pub responsive: bool,

    /// Filesystem mounted read-only (ext4 error recovery, hardware
    /// write-protect, etc.).
    pub read_only: bool,

    /// sysfs entry exists but physical device is gone (kernel ghost).
    /// Linux: `/sys/block/{dev}/device/state` is "offline" or
    /// "transport-offline".
    pub stale_reference: bool,

    /// Cumulative I/O error count from the device driver (if available).
    /// Linux: `/sys/block/{dev}/device/ioerr_cnt`. Zero when unavailable.
    pub io_errors: u64,
}

impl Default for DeviceHealth {
    fn default() -> Self {
        Self {
            responsive: true,
            read_only: false,
            stale_reference: false,
            io_errors: 0,
        }
    }
}

impl DeviceHealth {
    /// A healthy device: responsive, read-write, no errors.
    pub fn healthy() -> Self {
        Self::default()
    }
}

// ============================================================================
// Unmounted devices
// ============================================================================

/// An unmounted removable device that could potentially be a managed storage.
#[derive(Debug, Clone)]
pub struct UnmountedDevice {
    /// Device path (e.g., `/dev/sdb1`).
    pub device: String,
    /// Device name (e.g., `sdb1`).
    pub name: String,
    /// Capacity in bytes (from sysfs).
    pub capacity_bytes: u64,
    /// Filesystem label if available.
    pub label: Option<String>,
}
