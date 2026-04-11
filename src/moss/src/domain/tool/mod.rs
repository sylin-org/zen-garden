//! Tool bounded context — garden-wide tool registry and delta stream.
//!
//! Owns the `GardenTool` registry (offerings + storage + gateways +
//! remote-announced tools) and the broadcast streams that fire on
//! every registry mutation.
//!
//! ```text
//! domain/tool/
//! ├── mod.rs         — re-exports (this file)
//! ├── aggregate.rs   — Tool DDD aggregate: commands, queries, events
//! ├── state.rs       — (reserved for future use; Tool state lives in registry.rs)
//! ├── registry.rs    — GardenRegistryInner, ToolQuery, EntryOrigin, RegistryEntry
//! ├── event.rs       — ToolChanged domain event + ChangeKind for metrics
//! ├── error.rs       — ToolError
//! ├── projection.rs  — project_local_tools (local offerings + storage → GardenTool)
//! ├── capability.rs  — record_capability_added/removed (offering sub-capabilities)
//! ├── sse.rs         — ToolsSnapshotPayload, stream_event_type_for_delta
//! └── tests.rs       — unit tests (aggregate behaviour)
//! ```
//!
//! **Strangler phase (Ch3–Ch5):** the aggregate's `registry` field is
//! `pub(crate)` so fifty legacy call sites continue to compile
//! unchanged against `state.tool.registry.read().await`. Ch6 migrates
//! every one of them to a typed command or query method and marks the
//! field private. The field-level strangler is the Ch3 refinement of
//! ARCH-0019's original `ActiveGuard` plan — same end state, fewer
//! moving parts.

pub mod aggregate;
pub mod capability;
pub mod error;
pub mod event;
pub mod projection;
pub mod registry;
pub mod sse;
pub mod transport;

#[cfg(test)]
mod tests;

pub use aggregate::Tool;
pub use error::ToolError;
pub use event::{ChangeKind, ToolChanged};
pub use registry::{
    EntryOrigin, GardenRegistry, GardenRegistryInner, RegistryEntry, ToolQuery, new_registry,
};
pub use sse::{ToolsSnapshotPayload, stream_event_type_for_delta};
pub use transport::{NoopBeaconTransport, ToolsBeaconTransport};
