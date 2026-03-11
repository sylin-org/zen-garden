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
    broadcast_tools_beacon_inner(stone_id, stone_name, endpoint, deltas, false).await
}

/// Broadcast a snapshot tools beacon (marks beacon as authoritative full set).
///
/// Receivers will reconcile: any previously-announced entries from this stone
/// that are absent from the snapshot will be removed.
pub async fn broadcast_tools_snapshot_beacon(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
    deltas: Vec<ToolDelta>,
) -> Result<()> {
    broadcast_tools_beacon_inner(stone_id, stone_name, endpoint, deltas, true).await
}

async fn broadcast_tools_beacon_inner(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
    deltas: Vec<ToolDelta>,
    snapshot: bool,
) -> Result<()> {
    // For snapshot beacons we always send (even if empty) so receivers can
    // reconcile stale entries. For incremental beacons, skip if empty.
    if deltas.is_empty() && !snapshot {
        return Ok(());
    }

    let beacon = ToolsBeacon {
        stone_id: stone_id.to_string(),
        stone_name: stone_name.to_string(),
        endpoint: endpoint.to_string(),
        deltas,
        timestamp: Utc::now(),
        snapshot,
    };

    debug!(
        stone = %stone_name,
        deltas = beacon.deltas.len(),
        snapshot = snapshot,
        "Broadcasting tools beacon"
    );

    p2p::send_announcement(announcement_types::TOOLS_BEACON, &beacon)
        .await
        .context("Failed to send TOOLS_BEACON")?;

    info!(
        stone = %stone_name,
        deltas = beacon.deltas.len(),
        snapshot = snapshot,
        "Tools beacon broadcast complete"
    );

    Ok(())
}
