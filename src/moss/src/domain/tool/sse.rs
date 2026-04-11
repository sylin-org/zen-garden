use garden_common::tools::event_types;
use garden_common::tools::{GardenTool, ToolDelta, ToolDeltaKind};
use serde::Serialize;

pub fn stream_event_type_for_delta(delta: &ToolDelta) -> &'static str {
    match delta.kind {
        ToolDeltaKind::Upsert => event_types::TOOL_UPSERT,
        ToolDeltaKind::Remove => event_types::TOOL_REMOVE,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolsSnapshotPayload {
    pub cursor: u64,
    pub tools: Vec<GardenTool>,
}
