//! Storage orchestration — volume lifecycle coordination signals.

use std::sync::Arc;

/// Coordination signals for the storage domain.
///
/// Field path: `state.orchestration.storage.*`
#[derive(Clone)]
pub struct StorageOrchestration {
    /// Write-event tick channels at two frequencies.
    pub tick:   Tick,

    /// Wakes the orchestration loop immediately (skip the 3s tick wait).
    /// Fired on beacon arrival, rename, pin/unpin.
    pub nudge:  Arc<tokio::sync::Notify>,

    /// Requests a full volume reconcile from the watcher loop.
    /// Sent by API handlers after on-disk manifest mutations.
    pub rescan: tokio::sync::mpsc::Sender<()>,
}

/// Write-event tick channels at two frequencies.
///
/// Field path: `state.orchestration.storage.tick.{raw|debounced}`
#[derive(Clone)]
pub struct Tick {
    /// Raw per-write tick (high frequency, internal only).
    /// Consumed by the debounce task; not for downstream subscribers.
    pub raw:       tokio::sync::broadcast::Sender<garden_common::storage::StorageTick>,

    /// Debounced tick (2s quiet / 10s deadline cap).
    /// Subscribers: SSE stream, replication task.
    pub debounced: tokio::sync::broadcast::Sender<garden_common::storage::StorageTick>,
}
