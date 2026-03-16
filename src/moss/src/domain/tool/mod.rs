//! Tool domain — garden-wide tool registry and delta stream (ARCH-0004).
//!
//! Two stores:
//! - [`Tool`] (`state.tool`) — garden-wide aggregate: all stones' tools
//!   (Local + Announced `GardenTool` entries).
//! - `state.current.tool` — this stone's local tools (introduced in Phase 9).
//!
//! Write paths:
//! - Local offering/storage change → write to `state.current.tool.registry`,
//!   propagate into `state.tool.registry`.
//! - Remote beacon arrives → write directly into `state.tool.registry`.

use crate::domain::garden_registry::GardenRegistry;
use garden_common::tools::ToolDelta;
use tokio::sync::broadcast;

/// Garden-wide tool aggregate (`state.tool`).
///
/// Holds all `GardenTool` entries from all stones: Local and Announced origins.
/// FQN handler registrations (Registered origin) are extracted in Phase 3
/// (`state.fqn_handler`).
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
    /// ```rust
    /// let sse_feed = state.tool.delta_stream();
    /// ```
    pub fn delta_stream(&self) -> broadcast::Receiver<ToolDelta> {
        self.delta.subscribe()
    }
}
