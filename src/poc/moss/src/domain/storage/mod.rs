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
pub mod bank_aggregate;
pub mod collection;
pub mod health;
pub mod medium;
pub mod platform_types;
pub mod ports;
pub mod routing;
pub mod volume;

// ── Re-exports for backward compatibility ──────────────────────────────

// Volume types
pub use volume::{DiskMeasurement, Management, PinState, Volume, VolumeState};

// Collection types and operations
pub use collection::{
    Volumes, find_by_id, find_by_name, initial_scan, list_candidates, list_managed, name_id_pairs,
    new_volumes, observe_all, pins_snapshot, reconcile, roles_snapshot,
};

// Medium types
pub use medium::{Media, Medium, new_media, reconcile_media};

// Bank (ARCH-0025)
pub use bank::VolumeIngestor;
pub use bank_aggregate::{Bank, BankContentOps, BankError};

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
/// Holds the collections that describe physical storage (volumes, media,
/// domain event channel) and the coordination primitives that drive the
/// storage orchestration loop (tick, nudge, rescan, S3 listeners).
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

    /// Coordination primitives for the storage orchestration loop (ARCH-0029).
    /// Formerly `state.orchestration.storage.*`.
    pub coordination: Coordination,
}

impl Storage {
    /// Subscribe to the debounced storage tick stream.
    ///
    /// Returns a broadcast receiver of [`garden_common::storage::StorageTick`]
    /// events quantized at 2s quiet / 10s deadline. Use this for SSE streams
    /// and replication tasks instead of accessing `coordination.tick.debounced`
    /// directly.
    pub fn tick_stream(
        &self,
    ) -> tokio::sync::broadcast::Receiver<garden_common::storage::StorageTick> {
        self.coordination.tick.debounced.subscribe()
    }
}

/// Coordination signals for the storage domain (ARCH-0029).
///
/// Drives the background storage orchestration loop (Primary/Replica
/// role assignment, tick aggregation, S3 listener lifecycle).
///
/// Formerly `StorageOrchestration` in `domain::orchestration::storage`.
/// Field path: `state.current.storage.coordination.*`
#[derive(Clone)]
pub struct Coordination {
    /// Write-event tick channels at two frequencies.
    pub tick: Tick,

    /// Wakes the orchestration loop immediately (skip the 3s tick wait).
    /// Fired on beacon arrival, rename, pin/unpin.
    pub nudge: std::sync::Arc<tokio::sync::Notify>,

    /// Requests a full volume reconcile from the watcher loop.
    /// Sent by API handlers after on-disk manifest mutations.
    pub rescan: tokio::sync::mpsc::Sender<()>,

    /// Per-storage S3 listeners (STORAGE-0016).
    /// Arms a dedicated S3-compatible HTTP port per managed storage.
    pub s3_listeners: std::sync::Arc<crate::infra::storage::S3Listeners>,
}

/// Write-event tick channels at two frequencies.
///
/// Field path: `state.current.storage.coordination.tick.{raw|debounced}`
#[derive(Clone)]
pub struct Tick {
    /// Raw per-write tick (high frequency, internal only).
    /// Consumed by the debounce task; not for downstream subscribers.
    pub raw: tokio::sync::broadcast::Sender<garden_common::storage::StorageTick>,

    /// Debounced tick (2s quiet / 10s deadline cap).
    /// Internal — use [`Storage::tick_stream()`] to subscribe.
    pub(crate) debounced: tokio::sync::broadcast::Sender<garden_common::storage::StorageTick>,
}
