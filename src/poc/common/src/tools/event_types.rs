//! Event type constants for the tools stream contract.

/// API path for garden tools stream.
pub const TOOLS_STREAM_PATH: &str = "/api/v1/garden/tools/stream";

/// API path for garden tools snapshot.
pub const TOOLS_SNAPSHOT_PATH: &str = "/api/v1/garden/tools";

/// Snapshot event emitted first on stream connect.
pub const TOOLS_SNAPSHOT: &str = "tools.snapshot";

/// Tool projection upsert event.
pub const TOOL_UPSERT: &str = "tool.upsert";

/// Tool projection remove event.
pub const TOOL_REMOVE: &str = "tool.remove";

/// Capability sync lifecycle events.
pub const TOOL_CAPABILITY_SYNC_STARTED: &str = "tool.capability.sync_started";
pub const TOOL_CAPABILITY_SYNC_COMPLETED: &str = "tool.capability.sync_completed";
pub const TOOL_CAPABILITY_SYNC_FAILED: &str = "tool.capability.sync_failed";

/// Keepalive/heartbeat event.
pub const TOOLS_HEARTBEAT: &str = "tools.heartbeat";
