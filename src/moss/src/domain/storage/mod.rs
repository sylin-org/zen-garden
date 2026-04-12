//! Unified storage domain (STORAGE-0011)
//!
//! Single source of truth for all storage lifecycle on this stone.
//!
//! ## Submodules
//!
//! - [`volume`] — `Volume`, `VolumeState`, `Management`, `PinState` (the universal entity)
//! - [`collection`] — `Volumes` map, reconcile, initial scan, query helpers
//! - [`medium`] — `Medium`, `Media` (physical disk layer, host-only)
//! - [`bank`] — `VolumeIngestor` (domain bridge for physical storage events)
//! - [`health`] — seed bank validation and health assessment
//! - [`automount`] — auto-mount unmounted managed devices
//! - [`analysis`] — device eligibility for `storage add`
//! - [`platform_types`] — OS-agnostic value types (`VolumeSnapshot`, `DiskUsage`, etc.)

pub mod analysis;
pub mod automount;
pub mod bank;
pub mod collection;
pub mod health;
pub mod medium;
pub mod platform_types;
pub mod ports;
pub mod routing;
pub mod volume;

// ── Re-exports for backward compatibility ──────────────────────────────

// Volume types
pub use volume::{DiskResources, Management, PinState, Volume, VolumeState};

// Collection types and operations
pub use collection::{
    Volumes, find_by_id, find_by_name, initial_scan, list_candidates, list_managed, name_id_pairs,
    new_volumes, observe_all, pins_snapshot, reconcile, roles_snapshot,
};

// Medium types
pub use medium::{Media, Medium, new_media, reconcile_media};

// Bank
pub use bank::VolumeIngestor;

// Analysis
pub use analysis::{analyze_device, is_allowed_mount, validate_manifest};

// Health
pub use health::{
    SeedBankHealth, StorageHealth, assess_storage_health, is_mount_readonly,
    validate_seed_bank_layout,
};

// Automount
pub use automount::auto_mount_unmounted;

// Ports (ARCH-0025)
pub use ports::{ManagementStoreOps, StoragePlatform};

// Routing (ARCH-0025 — absorbed from storage_service.rs)
pub use routing::{LocalStorage, ProxyTarget, StorageRoute};

// Platform value types
pub use platform_types::{
    BusType, DeviceHealth, DiskUsage, MediumCondition, MediumSnapshot, PartitionSnapshot,
    UnmountedDevice, VolumeSnapshot,
};

// ── Storage domain context (ARCH-0004) ─────────────────────────────────

/// Storage data plane — what physically exists on this stone (ARCH-0004).
///
/// Holds only the collections that describe physical storage: volumes, media,
/// and the domain event channel. Coordination primitives (tick, nudge, rescan,
/// nurturing, nourishment) live in `state.orchestration.*`.
///
/// Field path: `state.current.storage.*`
#[derive(Clone)]
pub struct Storage {
    /// Unified volume collection — keyed by device path.
    pub volumes: Volumes,

    /// Physical storage media — keyed by OS device ID.
    pub media: Media,

    /// Storage domain event channel (STORAGE-0013).
    /// Emitted on add, remove, rename, role change, health change, rescan.
    pub changed: tokio::sync::broadcast::Sender<garden_common::storage::StorageChanged>,
}
