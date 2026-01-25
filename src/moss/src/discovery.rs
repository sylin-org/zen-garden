//! Stone discovery via UDP broadcasts
//!
//! REFACTORED (COMM-0001 Phase 3): UDP transport is now centralized in infra/communications/p2p.rs
//! This module provides a compatibility shim for existing callers.

use anyhow::Result;
use tokio::sync::broadcast;
use garden_common::{DiscoveryRequest, DiscoveryResponse};

// Re-export p2p types for backwards compatibility
pub use crate::infra::communications::p2p::UdpEvent;

/// Start or get reference to singleton UDP listener
/// Returns a receiver for subscribing to UDP events
///
/// **REFACTORED**: This is now a thin wrapper around p2p::subscribe_to_events()
/// All UDP handling is centralized in infra/communications/p2p.rs
pub async fn ensure_udp_listener(
    _stone_id: String,
    _stone_name: String,
    _api_endpoint: String,
) -> Result<broadcast::Receiver<UdpEvent>> {
    crate::infra::communications::p2p::subscribe_to_events().await
}

/// Actively discover peer stones on the network
/// 
/// Sends a discovery broadcast and collects chirp responses via p2p transport.
/// This is used during bootstrap to find existing stones in the garden.
///
/// **REFACTORED (COMM-0001)**: Now uses p2p transport for receiving chirps.
/// Sends discovery request, subscribes to p2p events, collects chirps as responses.
pub async fn discover_peers(
    stone_id: &str,
    timeout_secs: u64,
) -> Vec<DiscoveryResponse> {
    let mut discovered = Vec::new();

    // Subscribe to p2p events BEFORE sending discovery request
    let mut udp_rx = match crate::infra::communications::p2p::subscribe_to_events().await {
        Ok(rx) => rx,
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to subscribe to p2p events");
            return discovered;
        }
    };

    // Send discovery request via p2p transport
    let request = DiscoveryRequest {
        discover: "moss".to_string(),
        request_id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        requester: stone_id.to_string(),
    };

    if let Err(e) = crate::infra::communications::p2p::send_announcement(
        garden_common::announcement_types::DISCOVERY_REQUEST,
        &request,
    )
    .await
    {
        tracing::warn!(error = ?e, "Failed to send discovery broadcast");
        return discovered;
    }

    tracing::info!(request_id = %request.request_id, "Sent peer discovery request");

    // Collect chirp responses for timeout duration
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(timeout_secs);

    while tokio::time::Instant::now() < deadline {
        let remaining = deadline - tokio::time::Instant::now();
        match tokio::time::timeout(remaining, udp_rx.recv()).await {
            Ok(Ok(UdpEvent::Chirp { chirp, .. })) => {
                // Convert TopologyEntry chirp to DiscoveryResponse
                let response = DiscoveryResponse {
                    stone_id: Some(chirp.stone_id.clone()),
                    stone_name: chirp.stone_name.clone(),
                    stone_endpoint: chirp.endpoint.clone(),
                    moss_version: chirp.moss_version.clone(),
                    lantern_endpoint: None, // Not available in chirp
                };
                tracing::info!(
                    stone = %response.stone_name,
                    endpoint = %response.stone_endpoint,
                    "Discovered peer stone"
                );
                discovered.push(response);
            }
            Ok(Ok(_)) => {
                // Ignore other event types
            }
            Ok(Err(e)) => {
                tracing::trace!(error = ?e, "Broadcast recv lag");
            }
            Err(_) => break, // Timeout
        }
    }

    tracing::info!(count = discovered.len(), "Peer discovery complete");
    discovered
}

