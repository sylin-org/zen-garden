//! Storage infrastructure (STORAGE-0011)
//!
//! Platform-agnostic volume detection, object storage, and broadcast.
//!
//! ## Modules
//!
//! - `platform` — cross-platform volume adapter (scan, watch, usage) [NEW]
//! - `layout` — `.zen-garden/` dotfolder structure
//! - `store` — `ContentStore` filesystem I/O
//! - `objects` — S3-compatible object storage
//! - `beacon` — UDP broadcast of storage beacons
//! - `signpost` — SMB signpost generation
//! - `adapter` — `StorageAdapter` trait for `storage add`
//!
//! ## Legacy modules (STORAGE-0007, being migrated to STORAGE-0011)
//!
//! - `device` — Linux device analysis (being replaced by `platform`)
//! - `lifecycle` — StorageDevice health (being replaced by `domain::storage::Volume`)
//! - `registry` — StorageRegistry scan (being replaced by `domain::storage::Volumes`)

pub mod adapter;
mod beacon;
mod device;
pub mod layout;
mod lifecycle;
mod objects;
pub mod platform;
mod registry;
mod signpost;
mod store;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod subprocess;

#[cfg(target_os = "linux")]
mod monitor;
pub mod watcher;

// New (STORAGE-0011)
pub use beacon::{broadcast_beacon, broadcast_if_has_storage, build_beacon};
pub use objects::{ListResult, ObjectMetadata, ObjectStore, PutResult};
pub use signpost::refresh_signpost;
pub use store::ContentStore;
pub use watcher::StorageWatcherSet;

// Legacy (will be removed as consumers migrate to domain::storage::Volume)
pub use device::{
    analyze_device, list_unmounted_removable_devices, list_usb_partitions, DeviceAnalyzer,
    UnmountedDevice,
};
pub use lifecycle::{StorageDevice, StorageHealth};
pub use registry::StorageRegistry;

#[cfg(target_os = "linux")]
pub use registry::{create_mount_tracker, MountTracker, TrackedMount};

#[cfg(target_os = "linux")]
pub use monitor::StorageMonitor;
