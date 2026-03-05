//! Tools bounded context.
//!
//! Owns the garden-wide automation projection for tool state:
//! offerings and seed-banks exposed through one normalized contract.
//!
//! TOOLS-0003: The authoritative cache is now `GardenRegistry` in
//! `domain/garden_registry.rs`. This module retains the projector
//! (local tool construction) and event/capability infrastructure.

pub mod capability_orchestrator;
pub mod events;
pub mod projector;

pub use crate::domain::garden_registry::ToolQuery;
pub use events::{stream_event_type_for_delta, ToolsSnapshotPayload};
