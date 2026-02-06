//! Tools bounded context.
//!
//! Owns the garden-wide automation projection for tool state:
//! offerings and seed-banks exposed through one normalized contract.

pub mod cache;
pub mod capability_orchestrator;
pub mod events;
pub mod projector;
pub mod readiness;

pub use cache::{new_tools_cache, ToolQuery, ToolsCache, ToolsCacheInner};
pub use events::{stream_event_type_for_delta, ToolsSnapshotPayload};
