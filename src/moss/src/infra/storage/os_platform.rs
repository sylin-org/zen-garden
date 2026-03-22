//! OS platform adapter — implements the `StoragePlatform` trait
//! by delegating to the platform-specific free functions in `platform.rs`.

use anyhow::Result;
use garden_common::storage::{DeviceState, StorageManifest};

use crate::domain::storage::{DiskUsage, MediumSnapshot, UnmountedDevice, VolumeSnapshot};
use crate::domain::traits::StoragePlatform;

use super::platform;

/// Stateless adapter wrapping OS-level storage operations.
pub struct OsPlatform;

impl StoragePlatform for OsPlatform {
    fn disk_usage(&self, path: &str) -> Option<DiskUsage> {
        platform::disk_usage(path)
    }

    fn scan_volumes(&self) -> Vec<VolumeSnapshot> {
        platform::scan_volumes()
    }

    fn scan_media(&self) -> Vec<MediumSnapshot> {
        platform::scan_media()
    }

    fn list_unmounted_removable(&self) -> Vec<UnmountedDevice> {
        platform::list_unmounted_removable()
    }

    fn is_mount_point(&self, path: &str) -> bool {
        platform::is_mount_point(path)
    }

    fn device_at_mount_point(&self, mount_path: &str) -> Option<String> {
        platform::device_at_mount_point(mount_path)
    }

    fn is_removable(&self, path: &str) -> bool {
        platform::is_removable(path)
    }

    fn device_capacity(&self, device_path: &str) -> u64 {
        platform::device_capacity(device_path)
    }

    fn device_label(&self, device_path: &str) -> Option<String> {
        platform::device_label(device_path)
    }

    fn mount_point_for_device(&self, device: &str) -> Option<String> {
        platform::mount_point_for_device(device)
    }

    fn probe_device_state(
        &self,
        device_path: &str,
        mount_path: Option<&str>,
    ) -> Result<DeviceState> {
        platform::probe_device_state(device_path, mount_path)
    }

    async fn probe_device_manifest(&self, device: &str) -> Result<Option<StorageManifest>> {
        platform::probe_device_manifest(device).await
    }

    async fn unmount_lazy(&self, path: &str) -> Result<()> {
        platform::unmount_lazy(path).await
    }

    async fn mount_device(&self, device: &str, mount_path: &str) -> Result<()> {
        platform::mount_device(device, mount_path).await
    }
}
