//! Tool bounded context — garden-wide tool registry and delta stream.
//!
//! Owns the `GardenTool` registry (offerings + storage + gateways +
//! remote-announced tools) and the broadcast stream that fires on every
//! registry mutation. Book II of ARCH-0017 extracts this into a proper
//! DDD aggregate; Chapter 2 is the module consolidation step — the files
//! that make up this concept now live in one place.
//!
//! ```text
//! domain/tool/
//! ├── mod.rs         — re-exports (this file)
//! ├── registry.rs    — GardenRegistryInner, ToolQuery, EntryOrigin
//! ├── projection.rs  — project_local_tools(&AppState) → Vec<GardenTool>
//! ├── capability.rs  — record_capability_added/removed (offering mutations)
//! └── sse.rs         — ToolsSnapshotPayload, stream_event_type_for_delta
//! ```
//!
//! Later chapters collapse the aggregate shell (`Tool` struct below)
//! into `aggregate.rs` with private state, typed commands, and typed
//! queries. Ch2 keeps the existing `pub registry` field so call sites
//! continue to compile.

pub mod capability;
pub mod projection;
pub mod registry;
pub mod sse;

pub use registry::{
    EntryOrigin, GardenRegistry, GardenRegistryInner, RegistryEntry, ToolQuery, new_registry,
};
pub use sse::{ToolsSnapshotPayload, stream_event_type_for_delta};

use garden_common::tools::ToolDelta;
use tokio::sync::broadcast;

/// Garden-wide tool aggregate (`state.tool`).
///
/// Holds all `GardenTool` entries from all stones: Local, Gateway, and
/// Announced. Ch3 replaces the `pub registry` field with a private
/// `RwLock<ToolState>` and typed command/query methods.
#[derive(Clone)]
pub struct Tool {
    /// All tools from all stones in the garden.
    pub registry: GardenRegistry,

    /// Tool delta broadcast stream.
    ///
    /// Fired on every upsert/remove. Internal — use [`Tool::delta_stream()`]
    /// to subscribe.
    pub(crate) delta: broadcast::Sender<ToolDelta>,
}

impl Tool {
    /// Subscribe to the tool delta stream.
    ///
    /// Returns a broadcast receiver of [`ToolDelta`] events. Name the local
    /// receiver by its consumer's purpose:
    ///
    /// ```rust,ignore
    /// let sse_feed = state.tool.delta_stream();
    /// ```
    pub fn delta_stream(&self) -> broadcast::Receiver<ToolDelta> {
        self.delta.subscribe()
    }
}
