//! Tool domain — garden-wide tool registry and delta stream (ARCH-0004).
//!
//! `state.tool.registry` is the single source of truth for all tools.
//! Three origin types, each with its own lifecycle owner:
//!
//! - `Local` — projected from offerings + storage via `reconcile_local`.
//! - `Gateway` — written directly by orchestrator registration, TTL-managed.
//! - `Announced` — received from remote stones via beacon.

use crate::domain::garden_registry::GardenRegistry;
use garden_common::tools::ToolDelta;
use tokio::sync::broadcast;

/// Garden-wide tool aggregate (`state.tool`).
///
/// Holds all `GardenTool` entries from all stones: Local, Gateway, and Announced.
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
