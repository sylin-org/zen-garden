//! Stone discovery via UDP broadcasts
//!
//! REFACTORED (COMM-0001 Phase 3): UDP transport is now centralized in infra/communications/p2p.rs
//! This module provides a compatibility shim for existing callers.

use anyhow::Result;
use tokio::sync::broadcast;
use garden_common::{ports, DiscoveryRequest, DiscoveryResponse};

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
/// Sends a discovery broadcast and collects responses for the specified timeout.
/// This is used during bootstrap to find existing stones in the garden.
pub async fn discover_peers(
    stone_id: &str,
    timeout_secs: u64,
) -> Vec<DiscoveryResponse> {
    use tokio::net::UdpSocket;

    let mut discovered = Vec::new();

    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to bind socket for peer discovery");
            return discovered;
        }
    };

    if let Err(e) = socket.set_broadcast(true) {
        tracing::warn!(error = ?e, "Failed to set broadcast for peer discovery");
        return discovered;
    }

    // Send discovery request
    let request = DiscoveryRequest {
        discover: "moss".to_string(),
        request_id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        requester: stone_id.to_string(),
    };

    let data = match serde_json::to_vec(&request) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to serialize discovery request");
            return discovered;
        }
    };

    let broadcast_addr = format!("255.255.255.255:{}", ports::DISCOVERY_UDP);
    if let Err(e) = socket.send_to(&data, &broadcast_addr).await {
        tracing::warn!(error = ?e, "Failed to send discovery broadcast");
        return discovered;
    }

    tracing::info!(request_id = %request.request_id, "Sent peer discovery request");

    // Collect responses for timeout duration
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(timeout_secs);
    let mut buf = [0u8; 2048];

    while tokio::time::Instant::now() < deadline {
        let remaining = deadline - tokio::time::Instant::now();
        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, addr))) => {
                if let Ok(response) = serde_json::from_slice::<DiscoveryResponse>(&buf[..len]) {
                    tracing::info!(
                        stone = %response.stone_name,
                        endpoint = %response.stone_endpoint,
                        from = %addr,
                        "Discovered peer stone"
                    );
                    discovered.push(response);
                }
            }
            Ok(Err(_)) | Err(_) => break,
        }
    }

    tracing::info!(count = discovered.len(), "Peer discovery complete");
    discovered
}

