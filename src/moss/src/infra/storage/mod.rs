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
//! - Objects stored at: `{mount}/apps/{app}/{bucket}/{key}`
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
mod objects;
mod registry;

#[cfg(target_os = "linux")]
mod monitor;

pub use beacon::{broadcast_beacon, broadcast_if_has_storage, build_beacon};
pub use device::{DeviceAnalyzer, analyze_device, list_usb_partitions};
pub use objects::{ObjectStore, ObjectMetadata, ListResult, PutResult};
pub use registry::SeedBankRegistry;

#[cfg(target_os = "linux")]
pub use monitor::StorageMonitor;
