//! P2P Transport Layer - UDP Communication Infrastructure
//!
//! **SHARED INFRASTRUCTURE** - Used by moss, rake, and lantern
//!
//! This module owns ALL UDP communication on port 7184. Applications should:
//! - ✅ Use `subscribe_to_announcement(type)` for filtered events
//! - ✅ Use `send_announcement(type, payload)` for outbound messages
//! - ❌ NEVER create bespoke UDP sockets
//! - ❌ NEVER call `UdpSocket::bind()` directly
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────┐
//! │ Applications (moss, rake, lantern)       │
//! │ - subscribe_to_announcement(type)        │
//! │ - send_announcement(type, payload)       │
//! └────────────────┬─────────────────────────┘
//!                  │
//! ┌────────────────▼─────────────────────────┐
//! │ P2P Transport (common)                   │
//! │ - Receiver: 0.0.0.0:7184                │
//! │ - Sender: ephemeral port                │
//! │ - Validates UdpAnnouncement envelopes   │
//! │ - Broadcasts to filtered subscribers    │
//! └──────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ### Filtered Subscription (Recommended)
//! ```rust,no_run
//! use garden_common::infra::communications::p2p;
//! use garden_common::announcement_types;
//!
//! let mut rx = p2p::subscribe_to_announcement(announcement_types::DISCOVERY_REQUEST).await?;
//! 
//! loop {
//!     match rx.recv().await {
//!         Some((payload, source)) => {
//!             let request: DiscoveryRequest = serde_json::from_value(payload)?;
//!             // Handle request...
//!         },
//!         None => break,
//!     }
//! }
//! ```
//!
//! ### Sending Announcements
//! ```rust,no_run
//! p2p::send_announcement(
//!     announcement_types::DISCOVERY_RESPONSE,
//!     &response
//! ).await?;
//! ```
//!
//! ## References
//! - [COMM-0001](../../../../docs/decisions/COMM-0001-p2p-transport-singleton.md)
//! - [COMM-0002](../../../../docs/decisions/COMM-0002-p2p-pipeline-spec.md)

use anyhow::{Context, Result};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, mpsc, OnceCell};

use crate::{ports, UdpAnnouncement};

/// Internal event dispatched from UDP receiver
#[derive(Debug, Clone)]
struct InternalUdpEvent {
    announcement_type: String,
    payload: serde_json::Value,
    source: SocketAddr,
}

/// Singleton holder for UDP receiver and broadcast channel
static UDP_RECEIVER: OnceCell<broadcast::Sender<InternalUdpEvent>> = OnceCell::const_new();

/// Singleton holder for UDP sender socket
static UDP_SENDER: OnceCell<Arc<UdpSocket>> = OnceCell::const_new();

/// Subscribe to a specific announcement type (filtered)
///
/// Returns a channel that receives only messages matching the specified type.
/// This is the recommended API for most consumers.
///
/// ## Arguments
/// - `announcement_type`: Constant from `announcement_types` module
///
/// ## Returns
/// - `mpsc::Receiver<(serde_json::Value, SocketAddr)>`: Filtered stream of (payload, source)
///
/// ## Example
/// ```rust,no_run
/// use garden_common::infra::communications::p2p;
/// use garden_common::announcement_types;
///
/// let mut rx = p2p::subscribe_to_announcement(announcement_types::DISCOVERY_REQUEST).await?;
/// 
/// loop {
///     match rx.recv().await {
///         Some((payload, source)) => {
///             let request: DiscoveryRequest = serde_json::from_value(payload)?;
///             // Handle...
///         },
///         None => break,
///     }
/// }
/// ```
pub async fn subscribe_to_announcement(
    announcement_type: &str,
) -> Result<mpsc::Receiver<(serde_json::Value, SocketAddr)>> {
    let mut broadcast_rx = subscribe_to_all_internal().await?;
    let filter_type = announcement_type.to_string();

    // Create filtered channel
    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok(event) if event.announcement_type == filter_type => {
                    if tx.send((event.payload, event.source)).await.is_err() {
                        // Receiver dropped, exit filter task
                        break;
                    }
                }
                Ok(_) => {
                    // Wrong type, ignore
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(
                        skipped,
                        announcement_type = %filter_type,
                        "Subscriber lagged, events dropped"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    Ok(rx)
}

/// Subscribe to all UDP events (unfiltered)
///
/// Returns a broadcast receiver for ALL announcement types.
/// Use this only when a handler needs multiple types (e.g., coordinator).
/// Most consumers should use `subscribe_to_announcement()` instead.
///
/// ## Example
/// ```rust,no_run
/// let mut rx = p2p::subscribe_to_all().await?;
/// 
/// loop {
///     match rx.recv().await {
///         Ok((announcement_type, payload, source)) => {
///             match announcement_type.as_str() {
///                 announcement_types::STONE_CHIRP => { /* handle */ },
///                 announcement_types::STONE_GOODBYE => { /* handle */ },
///                 _ => {},
///             }
///         },
///         Err(e) => break,
///     }
/// }
/// ```
pub async fn subscribe_to_all() -> Result<mpsc::Receiver<(String, serde_json::Value, SocketAddr)>> {
    let mut broadcast_rx = subscribe_to_all_internal().await?;

    let (tx, rx) = mpsc::channel(100);

    tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok(event) => {
                    if tx
                        .send((event.announcement_type, event.payload, event.source))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "Subscriber lagged");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    Ok(rx)
}

/// Internal: Subscribe to raw broadcast channel
async fn subscribe_to_all_internal() -> Result<broadcast::Receiver<InternalUdpEvent>> {
    let tx = UDP_RECEIVER
        .get_or_init(|| async {
            // Create broadcast channel with capacity for 100 events
            let (tx, _rx) = broadcast::channel(100);
            let broadcast_tx = tx.clone();

            // Bind socket
            let addr = format!("0.0.0.0:{}", ports::DISCOVERY_UDP);
            let socket = match create_reusable_udp_socket(&addr).await {
                Ok(s) => {
                    tracing::info!(
                        port = ports::DISCOVERY_UDP,
                        "P2P transport receiver bound"
                    );
                    s
                }
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        port = ports::DISCOVERY_UDP,
                        "Failed to bind P2P receiver"
                    );
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
/// ## Arguments
/// - `announcement_type`: Constant from `announcement_types` module
/// - `payload`: Serializable payload struct
///
/// ## Example
/// ```rust,no_run
/// use garden_common::infra::communications::p2p;
/// use garden_common::announcement_types;
///
/// let response = DiscoveryResponse { /* ... */ };
/// p2p::send_announcement(
///     announcement_types::DISCOVERY_RESPONSE,
///     &response
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
                        tracing::warn!(error = ?e, "Failed to set broadcast on sender");
                    }
                    tracing::debug!("P2P sender socket created");
                    Arc::new(s)
                }
                Err(e) => {
                    tracing::error!(error = ?e, "Failed to create P2P sender");
                    panic!("Cannot initialize P2P sender: {}", e);
                }
            }
        })
        .await;

    let announcement = UdpAnnouncement {
        announcement_type: announcement_type.to_string(),
        data: serde_json::to_value(payload).context("Failed to serialize payload")?,
    };

    let data = serde_json::to_vec(&announcement).context("Failed to serialize envelope")?;

    let broadcast_addr = format!("255.255.255.255:{}", ports::DISCOVERY_UDP);

    socket
        .send_to(&data, &broadcast_addr)
        .await
        .with_context(|| format!("Failed to send to {}", broadcast_addr))?;

    tracing::trace!(
        announcement_type,
        size = data.len(),
        "UDP announcement sent"
    );

    Ok(())
}

/// Internal UDP receiver loop
async fn udp_receiver_loop(
    broadcast_tx: broadcast::Sender<InternalUdpEvent>,
    socket: UdpSocket,
) -> Result<()> {
    let mut buf = [0u8; 4096];

    tracing::info!("P2P transport receiver started");

    // Open log file for UDP debugging (append mode)
    let log_path = std::path::Path::new("/tmp/moss-udp-recv.log");
    let mut log_file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .await
        .ok();

    if log_file.is_some() {
        tracing::info!("UDP debug logging enabled: {}", log_path.display());
    }

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, addr)) => {
                // Log raw payload to file
                if let Some(ref mut file) = log_file {
                    let timestamp = chrono::Utc::now().to_rfc3339();
                    let raw_str = String::from_utf8_lossy(&buf[..len]);
                    let log_line = format!(
                        "[{}] FROM {} ({}b): {}\n",
                        timestamp, addr, len, raw_str
                    );
                    let _ = tokio::io::AsyncWriteExt::write_all(file, log_line.as_bytes()).await;
                    let _ = tokio::io::AsyncWriteExt::flush(file).await;
                }

                if let Ok(announcement) = serde_json::from_slice::<UdpAnnouncement>(&buf[..len]) {
                    let event = InternalUdpEvent {
                        announcement_type: announcement.announcement_type.clone(),
                        payload: announcement.data,
                        source: addr,
                    };

                    tracing::trace!(
                        announcement_type = %event.announcement_type,
                        source = ?addr,
                        "UDP event received"
                    );

                    let _ = broadcast_tx.send(event);
                } else {
                    tracing::trace!(?addr, len, "Invalid UDP packet");
                }
            }
            Err(e) => {
                tracing::warn!(error = ?e, "UDP recv error");
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
    }
}

/// Create UDP socket with SO_REUSEADDR and platform-specific fixes
async fn create_reusable_udp_socket(addr: &str) -> Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    
    let socket_addr: std::net::SocketAddr = addr.parse()?;
    let domain = if socket_addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    
    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    
    // Enable SO_REUSEADDR for port reuse
    socket.set_reuse_address(true)?;
    
    // Enable broadcast (required to receive broadcast packets!)
    socket.set_broadcast(true)?;
    
    // Windows: Disable WSAECONNRESET from ICMP port unreachable
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawSocket;
        const SIO_UDP_CONNRESET: u32 = 0x9800000C;
        let mut bytes_returned: u32 = 0;
        let enable: u32 = 0;
        unsafe {
            let sock = socket.as_raw_socket() as usize;
            let result = windows_sys::Win32::Networking::WinSock::WSAIoctl(
                sock,
                SIO_UDP_CONNRESET,
                &enable as *const _ as *const _,
                std::mem::size_of::<u32>() as u32,
                std::ptr::null_mut(),
                0,
                &mut bytes_returned as *mut _,
                std::ptr::null_mut(),
                None,
            );
            if result != 0 {
                tracing::warn!("Failed to disable SIO_UDP_CONNRESET");
            }
        }
    }
    
    socket.bind(&socket_addr.into())?;
    socket.set_nonblocking(true)?;
    
    Ok(UdpSocket::from_std(socket.into())?)
}
