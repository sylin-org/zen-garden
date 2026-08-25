//! Discovery request handler - responds to UDP discovery broadcasts
//!
//! Subscribes to p2p transport and responds to discovery requests with
//! DiscoveryResponse containing this stone's information.
//!
//! **Architecture**: Pure consumer of p2p events. No direct UDP socket creation.

use anyhow::Result;
use std::sync::Arc;

use crate::Moss;
use garden_common::infra::communications::p2p;

/// Start discovery request handler
///
/// Subscribes to p2p events and responds to discovery requests.
/// Runs as background task, never exits unless p2p transport fails.
pub async fn start_discovery_handler(state: Arc<Moss>) -> Result<()> {
    tracing::info!("Discovery handler starting, subscribing to p2p events");

    let mut udp_rx = p2p::subscribe_to_announcement(
        garden_common::infra::communications::announcement_types::DISCOVERY_REQUEST,
    )
    .await?;

    loop {
        match udp_rx.recv().await {
            Some((payload, from_addr)) => {
                let request: garden_common::DiscoveryRequest = match serde_json::from_value(payload)
                {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(error = ?e, "Failed to parse discovery request");
                        continue;
                    }
                };

                tracing::debug!(
                    request_id = %request.request_id,
                    requester = %request.requester,
                    from = %from_addr,
                    "Discovery request received, sending DiscoveryResponse"
                );

                // Build response from source fields
                let address = state.current.address.read().await.clone();
                let response = garden_common::DiscoveryResponse {
                    stone_id: Some(state.current.stone.id.clone()),
                    stone_name: state.current.stone.name.clone(),
                    address,
                    moss_version: crate::version_string(),
                    lantern_endpoint: None,
                };

                // Send response via p2p transport
                if let Err(e) = p2p::send_announcement(
                    garden_common::infra::communications::announcement_types::DISCOVERY_RESPONSE,
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
            None => {
                tracing::error!("P2P channel closed");
                break;
            }
        }
    }

    Ok(())
}
