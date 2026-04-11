//! Tools domain contracts — GardenTool unified model (TOOLS-0002)
//!
//! Shared contracts for the automation-grade tools projection:
//! - `GardenTool` unified resource model
//! - deltas and beacons
//! - capability wish parsing

pub mod event_types;
pub mod types;

pub use types::{
    Capability, CapabilityDelta, CapabilitySelector, CapabilitySnapshot, CapabilityWish,
    CapabilityWishParseError, GardenTool, ServiceInfo, Stone, StorageMetadata, ToolDelta,
    ToolDeltaKind, ToolIdentity, ToolType, ToolsBeacon, build_tool_key, fqid_matches,
    parse_capability_wish,
};
