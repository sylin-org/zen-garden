//! Discovery request handler - responds to UDP discovery broadcasts
//!
//! Subscribes to p2p transport and responds to discovery requests with
//! DiscoveryResponse containing this stone's information.
//!
//! **Architecture**: Pure consumer of p2p events. No direct UDP socket creation.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::infra::communications::p2p;
use crate::domain::TopologyEntry;

/// Start discovery request handler
///
/// Subscribes to p2p events and responds to discovery requests.
/// Runs as background task, never exits unless p2p transport fails.
pub async fn start_discovery_handler(
    self_entry: Arc<RwLock<TopologyEntry>>,
) -> Result<()> {
    tracing::info!("Discovery handler starting, subscribing to p2p events");

    let mut udp_rx = p2p::subscribe_to_events().await?;

    loop {
        match udp_rx.recv().await {
            Ok(p2p::UdpEvent::Request { request, from_addr }) => {
                tracing::debug!(
                    request_id = %request.request_id,
                    requester = %request.requester,
                    from = %from_addr,
                    "Discovery request received, sending DiscoveryResponse"
                );

                // Build response from current self_entry
                let entry = self_entry.read().await.clone();
                let response = garden_common::DiscoveryResponse {
                    stone_id: Some(entry.stone_id.clone()),
                    stone_name: entry.stone_name.clone(),
                    stone_endpoint: entry.endpoint.clone(),
                    moss_version: entry.moss_version.clone(),
                    lantern_endpoint: None,
                };

                // Send response via p2p transport
                if let Err(e) = p2p::send_announcement(
                    garden_common::announcement_types::DISCOVERY_RESPONSE,
                    &response,
                )
                .await
                {
                    tracing::warn!(
                        error = ?e,
                        request_id = %request.request_id,
                        "Failed to send discovery response"
                    );
                }
            }
            Ok(_) => {
                // Ignore other event types (chirps, goodbyes, elections)
            }
            Err(e) => {
                tracing::debug!(error = ?e, "UDP event recv lag");
            }
        }
    }
}
