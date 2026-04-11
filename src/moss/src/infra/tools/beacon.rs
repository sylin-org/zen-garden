//! UDP tools-beacon transport adapter.
//!
//! Adapts the garden's P2P announcement channel to the
//! [`ToolsBeaconTransport`](crate::domain::tool::transport::ToolsBeaconTransport)
//! port declared by the Tool aggregate. Ch4 of ARCH-0019.

use crate::domain::tool::transport::ToolsBeaconTransport;
use anyhow::{Context, Result};
use chrono::Utc;
use garden_common::infra::communications::{announcement_types, p2p};
use garden_common::tools::{ToolDelta, ToolsBeacon};
use std::future::Future;
use std::pin::Pin;
use tracing::info;

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Production adapter — publishes tool beacons via the garden P2P UDP
/// announcement transport.
#[derive(Debug, Default, Clone, Copy)]
pub struct P2pBeaconTransport;

impl ToolsBeaconTransport for P2pBeaconTransport {
    fn broadcast_incremental<'a>(
        &'a self,
        stone_id: &'a str,
        stone_name: &'a str,
        endpoint: &'a str,
        deltas: Vec<ToolDelta>,
    ) -> BoxFut<'a, Result<()>> {
        Box::pin(
            async move { broadcast_inner(stone_id, stone_name, endpoint, deltas, false).await },
        )
    }

    fn broadcast_snapshot<'a>(
        &'a self,
        stone_id: &'a str,
        stone_name: &'a str,
        endpoint: &'a str,
        deltas: Vec<ToolDelta>,
    ) -> BoxFut<'a, Result<()>> {
        Box::pin(async move { broadcast_inner(stone_id, stone_name, endpoint, deltas, true).await })
    }
}

/// Free-function wrapper preserved for callers that still hold a direct
/// handle (announcer, discovery join path). Ch5 migrates these to the
/// typed port.
pub async fn broadcast_tools_beacon(
    stone_id: &str,
    stone_name: &str,
    endpoint: &str,
    deltas: Vec<ToolDelta>,
) -> Result<()> {
    broadcast_inner(stone_id, stone_name, endpoint, deltas, false).await
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
    broadcast_inner(stone_id, stone_name, endpoint, deltas, true).await
}

async fn broadcast_inner(
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

    info!(
        stone = %stone_name,
        deltas = beacon.deltas.len(),
        snapshot = snapshot,
        "Broadcasting tools beacon ({} deltas)",
        beacon.deltas.len(),
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
