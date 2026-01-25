//! P2P Transport Layer - UDP Singleton for Stone-to-Stone Communication
//!
//! **CRITICAL ARCHITECTURAL COMPONENT**
//!
//! This module owns ALL UDP communication on port 7184. No other module should:
//! - Import `tokio::net::UdpSocket`
//! - Call `UdpSocket::bind()`
//! - Create its own UDP sockets
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │ P2P Transport (infra/communications)    │
//! │ - Receiver socket: 0.0.0.0:7184         │
//! │ - Sender socket: ephemeral port         │
//! │ - Broadcast channel: UdpEvent stream    │
//! └─────────────────────────────────────────┘
//!          ↓ subscribe_to_events()
//!          ↓ send_announcement()
//! ┌─────────────────────────────────────────┐
//! │ Domain Handlers (tasks/)                │
//! │ - discovery_handler.rs                  │
//! │ - election_handler.rs                   │
//! │ - ceremony_handler.rs (future)          │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ### Receiving Events
//! ```rust,no_run
//! let mut udp_rx = p2p::subscribe_to_events().await?;
//! loop {
//!     match udp_rx.recv().await {
//!         Ok(UdpEvent::ElectionRequest { request, .. }) => handle(request),
//!         Ok(UdpEvent::StoneChirp { chirp, .. }) => handle(chirp),
//!         _ => {} // Ignore other types
//!     }
//! }
//! ```
//!
//! ### Sending Announcements
//! ```rust,no_run
//! p2p::send_announcement(
//!     announcement_types::ELECTION_REQUEST,
//!     &election_request
//! ).await?;
//! ```
//!
//! ## References
//! - [COMM-0001](../../../docs/decisions/COMM-0001-p2p-transport-singleton.md)

use anyhow::{Context, Result};
use garden_common::{announcement_types, ports, UdpAnnouncement};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, OnceCell};

use crate::domain::TopologyEntry;
use crate::network_singletons;

/// UDP event propagated to consumers
///
/// Consumers subscribe to these events and filter by variant.
#[derive(Debug, Clone)]
pub enum UdpEvent {
    /// Discovery request from a stone looking for peers
    Request {
        request: garden_common::DiscoveryRequest,
        from_addr: SocketAddr,
    },
    /// Stone chirp - a TopologyEntry being broadcast
    Chirp {
        chirp: TopologyEntry,
        from_addr: SocketAddr,
    },
    /// Stone goodbye - graceful shutdown notification
    Goodbye {
        goodbye: garden_common::StoneGoodbyePayload,
        from_addr: SocketAddr,
    },
    /// Election request - initiates distributed election
    ElectionRequest {
        request: garden_common::election::ElectionRequest,
        from_addr: SocketAddr,
    },
    /// Election candidate - stone announcing candidacy
    ElectionCandidate {
        candidate: garden_common::election::ElectionCandidate,
        from_addr: SocketAddr,
    },
    /// Election result - winner announcement
    ElectionResult {
        result: garden_common::election::ElectionResult,
        from_addr: SocketAddr,
    },
}

/// Singleton holder for UDP receiver and broadcast channel
static UDP_RECEIVER: OnceCell<broadcast::Sender<UdpEvent>> = OnceCell::const_new();

/// Singleton holder for UDP sender socket
static UDP_SENDER: OnceCell<Arc<UdpSocket>> = OnceCell::const_new();

/// Subscribe to UDP events from the singleton listener
///
/// Returns a broadcast receiver that receives all UDP events on port 7184.
/// Consumers filter by `UdpEvent` variant to handle relevant message types.
///
/// ## Example
/// ```rust,no_run
/// let mut udp_rx = p2p::subscribe_to_events().await?;
/// loop {
///     match udp_rx.recv().await {
///         Ok(UdpEvent::ElectionRequest { request, .. }) => {
///             // Handle election
///         }
///         _ => {} // Ignore others
///     }
/// }
/// ```
pub async fn subscribe_to_events() -> Result<broadcast::Receiver<UdpEvent>> {
    let tx = UDP_RECEIVER
        .get_or_init(|| async {
            // Create broadcast channel with capacity for 100 events
            let (tx, _rx) = broadcast::channel(100);
            let broadcast_tx = tx.clone();

            // Bind socket BEFORE spawning to ensure immediate availability
            let addr = format!("0.0.0.0:{}", ports::DISCOVERY_UDP);
            let socket = match network_singletons::create_reusable_udp_socket(&addr).await {
                Ok(s) => {
                    tracing::info!(
                        port = ports::DISCOVERY_UDP,
                        "P2P transport receiver socket bound"
                    );
                    s
                }
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        port = ports::DISCOVERY_UDP,
                        "Failed to bind P2P receiver socket"
                    );
                    // Return sender anyway - subscribers won't get events but won't block startup
                    return tx;
                }
            };

            // Spawn receiver loop
            tokio::spawn(async move {
                if let Err(e) = udp_receiver_loop(broadcast_tx, socket).await {
                    tracing::error!(error = ?e, "P2P receiver loop failed");
                }
            });

            tx
        })
        .await;

    Ok(tx.subscribe())
}

/// Send UDP announcement via singleton sender socket
///
/// Wraps payload in `UdpAnnouncement` envelope, serializes to JSON,
/// and broadcasts to 255.255.255.255:7184.
///
/// ## Example
/// ```rust,no_run
/// p2p::send_announcement(
///     announcement_types::STONE_CHIRP,
///     &topology_entry
/// ).await?;
/// ```
pub async fn send_announcement<T: Serialize>(
    announcement_type: &str,
    payload: &T,
) -> Result<()> {
    let socket = UDP_SENDER
        .get_or_init(|| async {
            match UdpSocket::bind("0.0.0.0:0").await {
                Ok(s) => {
                    if let Err(e) = s.set_broadcast(true) {
                        tracing::warn!(error = ?e, "Failed to set broadcast flag on sender socket");
                    }
                    tracing::debug!("P2P transport sender socket created");
                    Arc::new(s)
                }
                Err(e) => {
                    tracing::error!(error = ?e, "Failed to create P2P sender socket");
                    panic!("Cannot initialize P2P sender socket: {}", e);
                }
            }
        })
        .await;

    let announcement = UdpAnnouncement {
        announcement_type: announcement_type.to_string(),
        data: serde_json::to_value(payload)
            .context("Failed to serialize announcement payload")?,
    };

    let data = serde_json::to_vec(&announcement)
        .context("Failed to serialize announcement envelope")?;

    let broadcast_addr = format!("255.255.255.255:{}", ports::DISCOVERY_UDP);

    socket
        .send_to(&data, &broadcast_addr)
        .await
        .with_context(|| format!("Failed to send UDP announcement to {}", broadcast_addr))?;

    tracing::trace!(
        announcement_type,
        size = data.len(),
        "UDP announcement sent"
    );

    Ok(())
}

/// Internal UDP receiver loop - parses and broadcasts events
async fn udp_receiver_loop(
    broadcast_tx: broadcast::Sender<UdpEvent>,
    socket: UdpSocket,
) -> Result<()> {
    let mut buf = [0u8; 4096]; // Large buffer for topology entries with services

    tracing::info!("P2P transport receiver loop started");

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, addr)) => {
                // Try parsing as UdpAnnouncement envelope
                if let Ok(announcement) =
                    serde_json::from_slice::<UdpAnnouncement>(&buf[..len])
                {
                    dispatch_announcement(&announcement, addr, &broadcast_tx).await;
                }
                // Legacy: Try parsing as raw DiscoveryRequest (backwards compat)
                else if let Ok(request) =
                    serde_json::from_slice::<garden_common::DiscoveryRequest>(&buf[..len])
                {
                    tracing::trace!(?addr, request_id = %request.request_id, "Legacy discovery request");
                    let _ = broadcast_tx.send(UdpEvent::Request {
                        request,
                        from_addr: addr,
                    });
                } else {
                    tracing::trace!(?addr, len, "Unrecognized UDP packet, ignoring");
                }
            }
            Err(e) => {
                // Log but continue - UDP receiver must not die on transient errors
                tracing::warn!(error = ?e, "UDP recv error, continuing");
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Dispatch announcement by type to broadcast channel
async fn dispatch_announcement(
    announcement: &UdpAnnouncement,
    addr: SocketAddr,
    broadcast_tx: &broadcast::Sender<UdpEvent>,
) {
    match announcement.announcement_type.as_str() {
        announcement_types::DISCOVERY_REQUEST => {
            if let Ok(request) = serde_json::from_value::<garden_common::DiscoveryRequest>(
                announcement.data.clone(),
            ) {
                tracing::trace!(?addr, request_id = %request.request_id, "Discovery request");
                let _ = broadcast_tx.send(UdpEvent::Request {
                    request,
                    from_addr: addr,
                });
            }
        }
        announcement_types::STONE_CHIRP => {
            if let Ok(chirp) =
                serde_json::from_value::<TopologyEntry>(announcement.data.clone())
            {
                tracing::trace!(
                    stone = %chirp.stone_name,
                    services = chirp.services.len(),
                    health = %chirp.health,
                    from = ?addr,
                    "Stone chirp"
                );
                let _ = broadcast_tx.send(UdpEvent::Chirp {
                    chirp,
                    from_addr: addr,
                });
            }
        }
        announcement_types::STONE_GOODBYE => {
            if let Ok(goodbye) = serde_json::from_value::<garden_common::StoneGoodbyePayload>(
                announcement.data.clone(),
            ) {
                tracing::info!(
                    stone = %goodbye.stone_name,
                    from = ?addr,
                    "Stone goodbye"
                );
                let _ = broadcast_tx.send(UdpEvent::Goodbye {
                    goodbye,
                    from_addr: addr,
                });
            }
        }
        announcement_types::ELECTION_REQUEST => {
            if let Ok(request) = serde_json::from_value::<garden_common::election::ElectionRequest>(
                announcement.data.clone(),
            ) {
                tracing::debug!(
                    election_id = %request.election_id,
                    election_type = ?request.election_type,
                    from = ?addr,
                    "Election request"
                );
                let _ = broadcast_tx.send(UdpEvent::ElectionRequest {
                    request,
                    from_addr: addr,
                });
            }
        }
        announcement_types::ELECTION_CANDIDATE => {
            if let Ok(candidate) =
                serde_json::from_value::<garden_common::election::ElectionCandidate>(
                    announcement.data.clone(),
                )
            {
                tracing::debug!(
                    election_id = %candidate.election_id,
                    stone_id = %candidate.stone_id,
                    from = ?addr,
                    "Election candidate"
                );
                let _ = broadcast_tx.send(UdpEvent::ElectionCandidate {
                    candidate,
                    from_addr: addr,
                });
            }
        }
        announcement_types::ELECTION_RESULT => {
            if let Ok(result) =
                serde_json::from_value::<garden_common::election::ElectionResult>(
                    announcement.data.clone(),
                )
            {
                tracing::debug!(
                    election_id = %result.election_id,
                    winner_id = %result.winner_id,
                    from = ?addr,
                    "Election result"
                );
                let _ = broadcast_tx.send(UdpEvent::ElectionResult {
                    result,
                    from_addr: addr,
                });
            }
        }
        _ => {
            tracing::trace!(
                announcement_type = %announcement.announcement_type,
                "Unknown announcement type"
            );
        }
    }
}
