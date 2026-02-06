//! Tools domain contracts
//!
//! Shared contracts for the automation-grade tools projection:
//! - identity (`tool_fqid`)
//! - projection snapshots
//! - deltas and beacons
//! - capability wish parsing

pub mod event_types;
pub mod types;

pub use types::{
    build_tool_fqid, parse_capability_wish, parse_tool_fqid, CapabilityDelta, CapabilitySelector,
    CapabilitySnapshot, CapabilityWish, CapabilityWishParseError, ToolConnection, ToolDelta,
    ToolDeltaKind, ToolParseError, ToolProjection, ToolState, ToolType, ToolsBeacon,
};
