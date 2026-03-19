//! Storage infrastructure (STORAGE-0011)
//!
//! Platform-agnostic volume detection, object storage, and broadcast.
//!
//! ## Modules
//!
//! - `platform` — cross-platform volume adapter (scan, watch, mount, probe)
//! - `layout` — `.zen-garden/` dotfolder structure
//! - `store` — `ContentStore` filesystem I/O
//! - `objects` — S3-compatible object storage
//! - `beacon` — UDP broadcast of storage beacons
//! - `signpost` — SMB signpost generation
//! - `adapter` — `StorageAdapter` trait for `storage add`

pub mod adapter;
mod beacon;
pub mod handle;
pub mod layout;
pub mod monitor;
pub mod multipart;
mod objects;
mod os_platform;
pub mod platform;
pub mod s3_listener;
mod signpost;
mod store;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) mod subprocess;
pub mod watcher;

pub use beacon::{broadcast_beacon, broadcast_if_has_storage, build_beacon};
pub use handle::{FileEntry, FileMeta, RouterError, StorageHandle, StorageResolver};
pub use objects::{ListResult, ObjectMetadata, ObjectStore, PutResult};
pub use os_platform::OsPlatform;
pub use s3_listener::S3ListenerManager;
pub use signpost::refresh_signpost;
pub use store::ContentStore;
pub use watcher::StorageWatcherSet;
