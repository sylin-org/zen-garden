//! Storage domain ports — trait boundaries for OS-level operations.
//!
//! These traits decouple the storage domain from platform-specific I/O.
//! Infra adapters implement them; domain code depends only on the trait.
//!
//! Relocated from `domain/traits/` in Book VIII (ARCH-0025) per
//! code-standards §14 (ports live inside their owning context).

use anyhow::Result;
use garden_common::storage::{DeviceState, StorageManifest};
use std::future::Future;

use super::platform_types::{DeviceHealth, DiskUsage, MediumSnapshot, UnmountedDevice, VolumeSnapshot};

// ============================================================================
// StoragePlatform — OS-level volume and device operations
// ============================================================================

/// OS-level storage operations.
///
/// Domain code depends on this trait; the infra layer implements it
/// by delegating to platform-specific syscalls and commands.
pub trait StoragePlatform: Send + Sync {
    // ---- Sync queries (cheap, no spawning required) ----

    /// Get disk usage for a mounted path.
    fn disk_usage(&self, path: &str) -> Option<DiskUsage>;

    /// Scan all currently accessible volumes (blocking).
    fn scan_volumes(&self) -> Vec<VolumeSnapshot>;

    /// Scan physical storage media / disks (blocking, heavier than scan_volumes).
    fn scan_media(&self) -> Vec<MediumSnapshot>;

    /// List unmounted removable devices that could be managed storage.
    fn list_unmounted_removable(&self) -> Vec<UnmountedDevice>;

    /// Check whether a path is a mount point.
    fn is_mount_point(&self, path: &str) -> bool;

    /// Return the device currently mounted at `mount_path`, if any.
    fn device_at_mount_point(&self, mount_path: &str) -> Option<String>;

    /// Check whether a path is on a removable device.
    fn is_removable(&self, path: &str) -> bool;

    /// Get the capacity of a block device in bytes.
    fn device_capacity(&self, device_path: &str) -> u64;

    /// Get the filesystem label of a block device.
    fn device_label(&self, device_path: &str) -> Option<String>;

    /// Get the mount point for a device, if mounted.
    fn mount_point_for_device(&self, device: &str) -> Option<String>;

    /// Probe device health from OS-level signals (STORAGE-0018).
    ///
    /// Returns a platform-agnostic health snapshot: responsive, read-only,
    /// stale reference, I/O error count. The domain decides what the signals
    /// mean. Called on every observe tick for online volumes.
    fn probe_device_health(&self, device_path: &str, mount_path: &str) -> DeviceHealth;

    /// Remove a stale block device reference from the kernel (STORAGE-0018).
    ///
    /// Linux: writes `1` to `/sys/block/{dev}/device/delete`.
    /// Only called for removable devices with `stale_reference = true`.
    /// No-op on platforms that clean up device references automatically.
    fn remove_stale_device(&self, device_path: &str) -> Result<()>;

    /// Probe a block device to determine its state.
    fn probe_device_state(
        &self,
        device_path: &str,
        mount_path: Option<&str>,
    ) -> Result<DeviceState>;

    // ---- Async operations (I/O, process spawning) ----

    /// Temp-mount a device, read manifest, unmount.
    fn probe_device_manifest(
        &self,
        device: &str,
    ) -> impl Future<Output = Result<Option<StorageManifest>>> + Send;

    /// Lazy unmount — detach filesystem immediately.
    fn unmount_lazy(&self, path: &str) -> impl Future<Output = Result<()>> + Send;

    /// Mount a block device at the given path.
    fn mount_device(
        &self,
        device: &str,
        mount_path: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}

// ============================================================================
// ManagementStoreOps — per-volume I/O for pin and LKG
// ============================================================================

/// Storage management I/O operations.
///
/// Used by `Management` (domain) for pin persistence and LKG snapshots
/// without depending on the concrete `ContentStore` in infra.
pub trait ManagementStoreOps: Send + Sync {
    /// Read persisted pin_id from the storage device, if any.
    fn read_pin(&self) -> impl Future<Output = Option<String>> + Send;

    /// Persist a pin_id to the storage device.
    fn write_pin(&self, pin_id: &str) -> impl Future<Output = Result<()>> + Send;

    /// Delete the persisted pin file.
    fn delete_pin(&self) -> impl Future<Output = Result<()>> + Send;

    /// Snapshot critical files to `last-known-good/` for resilience.
    fn snapshot_lkg(&self) -> impl Future<Output = Result<()>> + Send;
}
