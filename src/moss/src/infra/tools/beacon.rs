use anyhow::{Context, Result};
use chrono::Utc;
use garden_common::infra::communications::{announcement_types, p2p};
use garden_common::tools::{ToolDelta, ToolsBeacon};
use tracing::{debug, info};

pub async fn broadcast_tools_beacon(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
    deltas: Vec<ToolDelta>,
) -> Result<()> {
    if deltas.is_empty() {
        return Ok(());
    }

    let beacon = ToolsBeacon {
        stone_id: stone_id.to_string(),
        stone_name: stone_name.to_string(),
        endpoint: endpoint.to_string(),
        deltas,
        timestamp: Utc::now(),
    };

    debug!(
        stone = %stone_name,
        deltas = beacon.deltas.len(),
        "Broadcasting tools beacon"
    );

    p2p::send_announcement(announcement_types::TOOLS_BEACON, &beacon)
        .await
        .context("Failed to send TOOLS_BEACON")?;

    info!(
        stone = %stone_name,
        deltas = beacon.deltas.len(),
        "Tools beacon broadcast complete"
    );

    Ok(())
}
