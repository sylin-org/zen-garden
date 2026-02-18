//! Storage and seed bank infrastructure
//!
//! Handles USB storage device detection, preparation, and management.
//! Linux-only: Uses udev for device monitoring.
//!
//! Design: No persistence file - USB device manifests ARE the source of truth.
//! The registry is built in-memory by scanning mounted devices.
//!
//! ## Object Storage
//!
//! The `objects` module provides S3-compatible object storage on seed banks:
//! - Objects stored at: `{mount}/garden/storage/{bucket}/{key}`
//! - Metadata in sidecar files: `{key}.meta.json`
//! - Atomic writes with temp-file + rename
//!
//! ## Storage Beacon (STORAGE-0003)
//!
//! The `beacon` module broadcasts storage capability announcements:
//! - Triggered on mount/unmount, visibility change, new stone online
//! - All stones lurk-listen and update their StorageCache

mod beacon;
mod device;
mod lifecycle;
mod objects;
mod registry;
mod store;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod subprocess;

#[cfg(target_os = "linux")]
mod monitor;

pub use beacon::{
    broadcast_beacon, broadcast_if_has_storage, build_beacon, update_and_broadcast,
    update_local_storage_cache,
};
pub use device::{
    analyze_device, list_unmounted_removable_devices, list_usb_partitions, DeviceAnalyzer,
    UnmountedDevice,
};
pub use lifecycle::{StorageDevice, StorageHealth};
pub use objects::{ListResult, ObjectMetadata, ObjectStore, PutResult};
pub use registry::SeedBankRegistry;
pub use store::SeedBankStore;

#[cfg(target_os = "linux")]
pub use registry::{create_mount_tracker, MountTracker, TrackedMount};

#[cfg(target_os = "linux")]
pub use monitor::StorageMonitor;
