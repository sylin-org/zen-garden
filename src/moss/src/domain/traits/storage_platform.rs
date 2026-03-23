//! Storage platform abstraction.
//!
//! Trait boundary between the storage domain and OS-specific I/O:
//! volume scanning, disk usage, mount/unmount, device probing.

use anyhow::Result;
use garden_common::storage::{DeviceState, StorageManifest};
use std::future::Future;

use crate::domain::storage::{DiskUsage, MediumSnapshot, UnmountedDevice, VolumeSnapshot};

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
