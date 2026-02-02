//! P2P Transport Layer - UDP Discovery Infrastructure
//!
//! **SHARED INFRASTRUCTURE** - Used by moss, rake, and lantern
//!
//! This module owns ALL UDP communication for stone discovery. Applications should:
//! - ✅ Use `subscribe_to_announcement(type)` for filtered events
//! - ✅ Use `send_announcement(type, payload)` for outbound messages
//! - ❌ NEVER create bespoke UDP sockets
//! - ❌ NEVER call `UdpSocket::bind()` directly
//!
//! ## Discovery Transport Strategy
//!
//! **Multicast-first** (primary):
//! - Group: `239.255.42.99` (configurable via `DISCOVERY_MCAST_GROUP`)
//! - Port: `7184` (configurable via `DISCOVERY_PORT`)
//! - TTL: `1` (LAN-only, doesn't route)
//! - Receiver joins multicast on ALL eligible interfaces
//!
//! **Directed broadcast fallback** (secondary):
//! - Computes subnet broadcast per interface (e.g., `192.168.32.10/20` → `192.168.47.255`)
//! - Sends from socket bound to each interface IP
//! - Enabled by default (`DISCOVERY_ENABLE_BCAST_FALLBACK=true`)
//!
//! **Limited broadcast legacy** (tertiary, disabled by default):
//! - `255.255.255.255` as last resort
//! - Controlled by `DISCOVERY_ENABLE_LIMITED_BCAST=false`
//!
//! ## Why Multicast?
//!
//! Solves multi-homed Windows 11 discovery failures (WSL/Hyper-V vEthernet Companions).
//! Limited broadcast (`255.255.255.255`) egresses via default route interface, which may be
//! a virtual Companion instead of the physical NIC. Multicast join operations explicitly
//! specify which interface to listen on, and per-interface sender binding ensures packets
//! egress the correct NIC.
//!
//! See: `docs/discovery-transport.md` for full rationale.
//!
//! ## Configuration
//!
//! - `DISCOVERY_PORT`: UDP port (default: 7184)
//! - `DISCOVERY_MCAST_GROUP`: Multicast group IP (default: 239.255.42.99)
//! - `DISCOVERY_ENABLE_BCAST_FALLBACK`: Enable directed broadcast (default: true)
//! - `DISCOVERY_ENABLE_LIMITED_BCAST`: Enable 255.255.255.255 fallback (default: false)
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
//! │   • Joins multicast on each interface   │
//! │ - Sender: per-interface sockets         │
//! │   • Multicast to 239.255.42.99          │
//! │   • Directed broadcast per subnet       │
//! │ - Validates UdpAnnouncement envelopes   │
//! │ - Routes to filtered subscribers        │
//! └──────────────────────────────────────────┘
//! ```
//!
//! ## References
//! - [COMM-0001](../../../../docs/decisions/COMM-0001-p2p-transport-singleton.md)
//! - [COMM-0002](../../../../docs/decisions/COMM-0002-p2p-pipeline-spec.md)
//! - [discovery-transport.md](../../../../docs/discovery-transport.md)

use anyhow::{Context, Result};
use if_addrs::get_if_addrs;
use serde::Serialize;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, mpsc, Mutex, OnceCell, RwLock};
use tokio::time::Instant;

use crate::utils::ids::generate_guidv7;
use crate::{ports, UdpAnnouncement};

// ===== Configuration =====

/// Discovery transport configuration from environment
#[derive(Debug, Clone)]
struct DiscoveryConfig {
    /// UDP port for discovery (default: 7184)
    port: u16,
    /// Multicast group address (default: 239.255.42.99)
    mcast_group: Ipv4Addr,
    /// Enable directed broadcast fallback (default: true)
    enable_bcast_fallback: bool,
    /// Enable limited broadcast (255.255.255.255) fallback (default: false)
    enable_limited_bcast: bool,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            port: ports::DISCOVERY_UDP,
            mcast_group: Ipv4Addr::new(239, 255, 42, 99),
            enable_bcast_fallback: true,
            enable_limited_bcast: false,
        }
    }
}

impl DiscoveryConfig {
    /// Load configuration from environment variables
    fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(port_str) = std::env::var("DISCOVERY_PORT") {
            if let Ok(port) = port_str.parse() {
                config.port = port;
            }
        }

        if let Ok(group_str) = std::env::var("DISCOVERY_MCAST_GROUP") {
            if let Ok(group) = group_str.parse() {
                config.mcast_group = group;
            }
        }

        if let Ok(val) = std::env::var("DISCOVERY_ENABLE_BCAST_FALLBACK") {
            config.enable_bcast_fallback = val.eq_ignore_ascii_case("true") || val == "1";
        }

        if let Ok(val) = std::env::var("DISCOVERY_ENABLE_LIMITED_BCAST") {
            config.enable_limited_bcast = val.eq_ignore_ascii_case("true") || val == "1";
        }

        config
    }
}

// ===== Interface Management =====

/// Network interface information
#[derive(Debug, Clone)]
struct NetworkInterface {
    name: String,
    ip: Ipv4Addr,
    netmask: Option<Ipv4Addr>,
    broadcast: Option<Ipv4Addr>,
}

impl NetworkInterface {
    /// Compute directed broadcast address from IP and netmask
    fn compute_broadcast(&self) -> Option<Ipv4Addr> {
        if let Some(netmask) = self.netmask {
            let ip_octets = self.ip.octets();
            let mask_octets = netmask.octets();
            let broadcast = [
                ip_octets[0] | !mask_octets[0],
                ip_octets[1] | !mask_octets[1],
                ip_octets[2] | !mask_octets[2],
                ip_octets[3] | !mask_octets[3],
            ];
            Some(Ipv4Addr::from(broadcast))
        } else {
            None
        }
    }
}

/// Check if interface name or address suggests a virtual Companion
fn is_virtual_interface(name: &str, ip: &Ipv4Addr) -> bool {
    let name_lower = name.to_lowercase();

    // Virtual Companion name patterns
    let virtual_patterns = [
        "veth",      // Linux virtual Ethernet
        "virbr",     // libvirt bridge
        "docker",    // Docker bridge
        "br-",       // Linux bridge
        "vmnet",     // VMware
        "vboxnet",   // VirtualBox
        "hyperv",    // Hyper-V
        "wsl",       // WSL Companion
    ];

    for pattern in &virtual_patterns {
        if name_lower.contains(pattern) {
            return true;
        }
    }

    // Docker default bridge network
    if ip.octets()[..2] == [172, 17] {
        return true;
    }

    false
}

/// Enumerate eligible network interfaces for discovery
fn enumerate_eligible_interfaces() -> Vec<NetworkInterface> {
    let Ok(interfaces) = get_if_addrs() else {
        tracing::warn!("Failed to enumerate network interfaces");
        return Vec::new();
    };

    let mut eligible = Vec::new();

    for iface in interfaces {
        // Only IPv4
        let if_addrs::IfAddr::V4(ref v4_addr) = iface.addr else {
            continue;
        };

        let ipv4 = v4_addr.ip;

        // Skip loopback
        if ipv4.is_loopback() {
            continue;
        }

        // Skip link-local (169.254.x.x)
        if ipv4.octets()[0] == 169 && ipv4.octets()[1] == 254 {
            continue;
        }

        // Skip virtual Companions
        if is_virtual_interface(&iface.name, &ipv4) {
            tracing::debug!(
                interface = %iface.name,
                ip = %ipv4,
                "Skipping virtual interface"
            );
            continue;
        }

        // Extract netmask from V4 address
        let netmask = Some(v4_addr.netmask);

        // Compute broadcast address
        let temp_iface = NetworkInterface {
            name: iface.name.clone(),
            ip: ipv4,
            netmask,
            broadcast: None,
        };
        let broadcast = temp_iface.compute_broadcast();

        eligible.push(NetworkInterface {
            name: iface.name,
            ip: ipv4,
            netmask,
            broadcast,
        });
    }

    if eligible.is_empty() {
        tracing::warn!("No eligible network interfaces found for discovery");
    } else {
        tracing::debug!(
            count = eligible.len(),
            interfaces = ?eligible.iter().map(|i| format!("{}({})", i.name, i.ip)).collect::<Vec<_>>(),
            "Enumerated eligible interfaces"
        );
    }

    eligible
}

// ===== Core Types =====

/// Internal event dispatched from UDP receiver
#[derive(Debug, Clone)]
struct InternalUdpEvent {
    announcement_type: String,
    payload: serde_json::Value,
    source: SocketAddr,
}

/// Per-interface sender socket
#[derive(Debug)]
struct InterfaceSender {
    interface: NetworkInterface,
    socket: Arc<UdpSocket>,
}

// ===== Deduplication =====

/// TTL for dedup cache entries (5 seconds)
/// This is sufficient to handle multicast/broadcast race conditions
/// where the same message may arrive via different paths within milliseconds.
const DEDUP_TTL: Duration = Duration::from_secs(5);

/// Deduplication cache to prevent processing the same message multiple times
/// when it arrives via both multicast and broadcast paths.
struct DedupCache {
    seen: HashMap<String, Instant>,
}

impl DedupCache {
    fn new() -> Self {
        Self {
            seen: HashMap::new(),
        }
    }

    /// Check if message ID is a duplicate. Returns true if seen before (skip processing).
    /// Performs lazy cleanup of expired entries on each call.
    fn is_duplicate(&mut self, msg_id: &str) -> bool {
        let now = Instant::now();

        // Lazy cleanup: remove entries older than TTL
        self.seen
            .retain(|_, ts| now.duration_since(*ts) < DEDUP_TTL);

        // Check if already seen
        if self.seen.contains_key(msg_id) {
            return true; // Duplicate
        }

        // Mark as seen
        self.seen.insert(msg_id.to_string(), now);
        false
    }
}

/// Singleton holder for UDP receiver and broadcast channel
static UDP_RECEIVER: OnceCell<broadcast::Sender<InternalUdpEvent>> = OnceCell::const_new();

/// Singleton holder for per-interface UDP sender sockets (RwLock for reinitialization)
static UDP_SENDERS: OnceLock<RwLock<Arc<Vec<InterfaceSender>>>> = OnceLock::new();

/// Discovery configuration singleton
static DISCOVERY_CONFIG: OnceCell<DiscoveryConfig> = OnceCell::const_new();

/// Default debounce durations per announcement type
static DEFAULT_DEBOUNCE: OnceCell<HashMap<String, Duration>> = OnceCell::const_new();

/// Runtime debounce overrides (persists until cleared)
static DEBOUNCE_OVERRIDES: OnceCell<Mutex<HashMap<String, Duration>>> = OnceCell::const_new();

/// Active debounce channels per announcement type
static DEBOUNCE_CHANNELS: OnceCell<Mutex<HashMap<String, mpsc::UnboundedSender<Vec<u8>>>>> =
    OnceCell::const_new();

// ===== Public API =====

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
/// ```rust,ignore
/// use garden_common::infra::communications::{p2p, announcement_types};
///
/// let mut rx = p2p::subscribe_to_announcement(announcement_types::DISCOVERY_REQUEST).await?;
///
/// loop {
///     match rx.recv().await {
///         Some((payload, source)) => {
///             let request: garden_common::DiscoveryRequest = serde_json::from_value(payload)?;
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
pub async fn subscribe_to_all(
) -> Result<mpsc::Receiver<(String, serde_json::Value, SocketAddr)>> {
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

/// Send announcement with type-default debounce
///
/// Uses configured default for announcement type:
/// - STONE_CHIRP: 100ms debounce (batches rapid status changes)
/// - Others: Immediate send (Duration::ZERO)
pub async fn send_announcement<T: Serialize>(
    announcement_type: &str,
    payload: &T,
) -> Result<()> {
    let duration = resolve_debounce_duration(announcement_type).await;
    send_announcement_internal(announcement_type, payload, duration).await
}

/// Send announcement immediately (bypass debounce)
///
/// Use for urgent updates that must be broadcast without delay.
pub async fn send_announcement_immediate<T: Serialize>(
    announcement_type: &str,
    payload: &T,
) -> Result<()> {
    send_announcement_internal(announcement_type, payload, Duration::ZERO).await
}

/// Send announcement with custom debounce duration
///
/// Overrides type default for this announcement type.
pub async fn send_announcement_with_debounce<T: Serialize>(
    announcement_type: &str,
    payload: &T,
    debounce: Duration,
) -> Result<()> {
    // Initialize statics on first use
    DEBOUNCE_OVERRIDES
        .get_or_init(|| async { Mutex::new(HashMap::new()) })
        .await;

    // Cache override for this type
    {
        let mut overrides = DEBOUNCE_OVERRIDES.get().unwrap().lock().await;
        overrides.insert(announcement_type.to_string(), debounce);
    }

    send_announcement_internal(announcement_type, payload, debounce).await
}

/// Clear debounce override for announcement type (revert to default)
pub async fn clear_debounce_override(announcement_type: &str) {
    DEBOUNCE_OVERRIDES
        .get_or_init(|| async { Mutex::new(HashMap::new()) })
        .await;

    let mut overrides = DEBOUNCE_OVERRIDES.get().unwrap().lock().await;
    overrides.remove(announcement_type);
}

/// Reinitialize sender sockets (call when network becomes available)
///
/// Called by network monitor when transitioning from disconnected to connected state.
/// Recreates per-interface sender sockets for all eligible network interfaces.
///
/// This is necessary because:
/// - On Linux, network interfaces may not be ready during early boot
/// - P2P transport initializes once at startup with no senders if no interfaces available
/// - When network becomes available later, senders need to be recreated
///
/// ## Example
/// ```rust,ignore
/// // In network monitor reconnection handler:
/// if was_disconnected && !now_disconnected {
///     p2p::reinit_senders().await;
/// }
/// ```
pub async fn reinit_senders() {
    let config = DISCOVERY_CONFIG
        .get_or_init(|| async { DiscoveryConfig::from_env() })
        .await;

    let senders_lock = UDP_SENDERS.get_or_init(|| {
        RwLock::new(Arc::new(Vec::new()))
    });

    let mut write_guard = senders_lock.write().await;

    // Re-enumerate interfaces (network is now available)
    let interfaces = enumerate_eligible_interfaces();
    
    if interfaces.is_empty() {
        tracing::warn!("reinit_senders: No eligible interfaces found after reconnection");
        return;
    }

    tracing::info!(
        interface_count = interfaces.len(),
        "Reinitializing P2P sender sockets after network reconnection"
    );

    let mut interface_senders = Vec::new();
    for iface in interfaces {
        match create_interface_sender(&iface, config).await {
            Ok(socket) => {
                tracing::info!(
                    interface = %iface.name,
                    ip = %iface.ip,
                    "Created sender socket (network reinitialization)"
                );
                interface_senders.push(InterfaceSender {
                    interface: iface,
                    socket: Arc::new(socket),
                });
            }
            Err(e) => {
                tracing::warn!(
                    error = ?e,
                    interface = %iface.name,
                    "Failed to create sender for interface"
                );
            }
        }
    }

    if interface_senders.is_empty() {
        tracing::error!("reinit_senders: Failed to create any sender sockets");
    } else {
        tracing::info!(
            sender_count = interface_senders.len(),
            "P2P senders reinitialized successfully"
        );
    }

    *write_guard = Arc::new(interface_senders);
}

// ===== Internal Implementation =====

/// Initialize debounce configuration
fn init_debounce_defaults() -> HashMap<String, Duration> {
    use crate::infra::communications::announcement_types;

    let mut config = HashMap::new();
    // STONE_CHIRP gets 100ms debounce to batch rapid status changes
    config.insert(
        announcement_types::STONE_CHIRP.to_string(),
        Duration::from_millis(100),
    );
    // All other types default to immediate send (Duration::ZERO)
    config
}

/// Internal: Subscribe to raw broadcast channel
async fn subscribe_to_all_internal() -> Result<broadcast::Receiver<InternalUdpEvent>> {
    let tx = UDP_RECEIVER
        .get_or_init(|| async {
            // Initialize config
            let config = DISCOVERY_CONFIG
                .get_or_init(|| async { DiscoveryConfig::from_env() })
                .await;

            // Create broadcast channel with capacity for 100 events
            let (tx, _rx) = broadcast::channel(100);
            let broadcast_tx = tx.clone();

            // Bind receiver socket
            let addr = format!("0.0.0.0:{}", config.port);
            let socket = match create_multicast_receiver(&addr, config).await {
                Ok(s) => {
                    tracing::info!(
                        port = config.port,
                        mcast_group = %config.mcast_group,
                        "P2P transport receiver bound with multicast"
                    );
                    s
                }
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        port = config.port,
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

/// Internal: Send with specified debounce duration
async fn send_announcement_internal<T: Serialize>(
    announcement_type: &str,
    payload: &T,
    debounce_duration: Duration,
) -> Result<()> {
    // Serialize payload
    let payload_bytes =
        serde_json::to_vec(payload).context("Failed to serialize announcement payload")?;

    if debounce_duration.is_zero() {
        // Send immediately
        send_udp_packet(announcement_type, &payload_bytes).await
    } else {
        // Send through debouncer
        let tx = get_or_create_debounce_channel(announcement_type, debounce_duration).await;
        tx.send(payload_bytes).map_err(|_| {
            anyhow::anyhow!("Debounce channel closed for {}", announcement_type)
        })?;
        Ok(())
    }
}

/// Resolve effective debounce duration
async fn resolve_debounce_duration(announcement_type: &str) -> Duration {
    // Initialize statics on first use
    DEBOUNCE_OVERRIDES
        .get_or_init(|| async { Mutex::new(HashMap::new()) })
        .await;
    DEFAULT_DEBOUNCE
        .get_or_init(|| async { init_debounce_defaults() })
        .await;

    // Check for caller override
    {
        let overrides = DEBOUNCE_OVERRIDES.get().unwrap().lock().await;
        if let Some(duration) = overrides.get(announcement_type) {
            return *duration;
        }
    }

    // Use type default
    DEFAULT_DEBOUNCE
        .get()
        .unwrap()
        .get(announcement_type)
        .copied()
        .unwrap_or(Duration::ZERO)
}

/// Get or create debounce channel for announcement type
async fn get_or_create_debounce_channel(
    announcement_type: &str,
    _debounce_duration: Duration,
) -> mpsc::UnboundedSender<Vec<u8>> {
    // Initialize statics on first use
    DEBOUNCE_CHANNELS
        .get_or_init(|| async { Mutex::new(HashMap::new()) })
        .await;

    let mut channels = DEBOUNCE_CHANNELS.get().unwrap().lock().await;

    if let Some(tx) = channels.get(announcement_type) {
        return tx.clone();
    }

    // Create new debouncer task for this type
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let type_clone = announcement_type.to_string();

    let task_tx = tx.clone();
    tokio::spawn(async move {
        const MAX_DELAY: Duration = Duration::from_millis(500);

        while let Some(payload) = rx.recv().await {
            let first_request = Instant::now();
            let mut latest_payload = payload;

            // Resolve current debounce duration (may have changed via override)
            let current_debounce = resolve_debounce_duration(&type_clone).await;

            loop {
                tokio::select! {
                    // Debounce period elapsed
                    _ = tokio::time::sleep(current_debounce) => {
                        break;
                    }
                    // New announcement received
                    Some(new_payload) = rx.recv() => {
                        latest_payload = new_payload; // Replace with newer

                        // Check max delay cap
                        if first_request.elapsed() >= MAX_DELAY {
                            tracing::debug!(
                                announcement_type = %type_clone,
                                "Max delay reached, sending debounced announcement"
                            );
                            break;
                        }
                        // Continue debounce loop
                    }
                }
            }

            // Send the final (most recent) payload
            if let Err(e) = send_udp_packet(&type_clone, &latest_payload).await {
                tracing::warn!(
                    announcement_type = %type_clone,
                    error = ?e,
                    "Failed to send debounced announcement"
                );
            }
        }
    });

    channels.insert(announcement_type.to_string(), task_tx.clone());
    task_tx
}

/// Internal: Send UDP packet immediately via multicast + fallbacks
async fn send_udp_packet(announcement_type: &str, payload_bytes: &[u8]) -> Result<()> {
    // Initialize config on first use
    let config = DISCOVERY_CONFIG
        .get_or_init(|| async { DiscoveryConfig::from_env() })
        .await;

    // Initialize sender sockets (lazy, with RwLock for reinitialization)
    let senders_lock = UDP_SENDERS.get_or_init(|| {
        RwLock::new(Arc::new(Vec::new()))
    });
    
    // Try to get existing senders
    let senders = {
        let read_guard = senders_lock.read().await;
        if !read_guard.is_empty() {
            read_guard.clone()
        } else {
            drop(read_guard);
            
            // Need to initialize - acquire write lock
            let mut write_guard = senders_lock.write().await;
            
            // Double-check in case another task initialized while we waited
            if !write_guard.is_empty() {
                write_guard.clone()
            } else {
                // Actually initialize
                let interfaces = enumerate_eligible_interfaces();
                if interfaces.is_empty() {
                    tracing::warn!("No eligible network interfaces found for discovery");
                }

                let mut interface_senders = Vec::new();
                for iface in interfaces {
                    match create_interface_sender(&iface, config).await {
                        Ok(socket) => {
                            tracing::debug!(
                                interface = %iface.name,
                                ip = %iface.ip,
                                "Created sender socket"
                            );
                            interface_senders.push(InterfaceSender {
                                interface: iface,
                                socket: Arc::new(socket),
                            });
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = ?e,
                                interface = %iface.name,
                                "Failed to create sender for interface"
                            );
                        }
                    }
                }

                if interface_senders.is_empty() {
                    tracing::error!("No sender sockets created, discovery will fail");
                }

                let new_senders = Arc::new(interface_senders);
                *write_guard = new_senders.clone();
                new_senders
            }
        }
    };

    let config = DISCOVERY_CONFIG.get().unwrap();

    // Build announcement envelope with dedup ID
    let msg_id = generate_guidv7();
    let announcement = UdpAnnouncement {
        msg_id: Some(msg_id.clone()),
        announcement_type: announcement_type.to_string(),
        data: serde_json::from_slice(payload_bytes)
            .context("Failed to deserialize payload for announcement")?,
    };

    let data = serde_json::to_vec(&announcement).context("Failed to serialize envelope")?;

    let mut sent_count = 0;

    // Send via each interface
    for sender in senders.iter() {
        // 1. Send to multicast group
        let mcast_addr = SocketAddr::new(IpAddr::V4(config.mcast_group), config.port);
        match sender.socket.send_to(&data, &mcast_addr).await {
            Ok(_) => {
                sent_count += 1;
                tracing::trace!(
                    interface = %sender.interface.name,
                    target = %mcast_addr,
                    "Multicast sent"
                );
            }
            Err(e) => {
                tracing::debug!(
                    error = ?e,
                    interface = %sender.interface.name,
                    "Multicast send failed"
                );
            }
        }

        // 2. Directed broadcast fallback (if enabled)
        if config.enable_bcast_fallback {
            if let Some(bcast_ip) = sender.interface.broadcast {
                let bcast_addr = SocketAddr::new(IpAddr::V4(bcast_ip), config.port);
                match sender.socket.send_to(&data, &bcast_addr).await {
                    Ok(_) => {
                        sent_count += 1;
                        tracing::trace!(
                            interface = %sender.interface.name,
                            target = %bcast_addr,
                            "Directed broadcast sent"
                        );
                    }
                    Err(e) => {
                        tracing::debug!(
                            error = ?e,
                            interface = %sender.interface.name,
                            "Directed broadcast failed"
                        );
                    }
                }
            }
        }
    }

    // 3. Limited broadcast fallback (if enabled and no other sends succeeded)
    if config.enable_limited_bcast && sent_count == 0 {
        tracing::warn!("Falling back to limited broadcast (255.255.255.255)");
        if let Some(sender) = senders.first() {
            let limited_bcast = SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)),
                config.port,
            );
            sender.socket.send_to(&data, &limited_bcast).await.ok();
        }
    }

    if sent_count > 0 {
        tracing::trace!(
            announcement_type = %announcement_type,
            size = data.len(),
            sent_count,
            "UDP announcement sent"
        );
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Failed to send announcement on any interface"
        ))
    }
}

/// Internal UDP receiver loop
async fn udp_receiver_loop(
    broadcast_tx: broadcast::Sender<InternalUdpEvent>,
    socket: UdpSocket,
) -> Result<()> {
    let mut buf = [0u8; 4096];
    let mut dedup_cache = DedupCache::new();

    tracing::info!("P2P transport receiver started");

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, addr)) => {
                if let Ok(announcement) = serde_json::from_slice::<UdpAnnouncement>(&buf[..len]) {
                    // Deduplicate if msg_id is present
                    if let Some(ref msg_id) = announcement.msg_id {
                        if dedup_cache.is_duplicate(msg_id) {
                            tracing::trace!(
                                msg_id = %msg_id,
                                source = ?addr,
                                "Duplicate message ignored"
                            );
                            continue;
                        }
                    }

                    let event = InternalUdpEvent {
                        announcement_type: announcement.announcement_type.clone(),
                        payload: announcement.data,
                        source: addr,
                    };

                    tracing::trace!(
                        announcement_type = %event.announcement_type,
                        source = ?addr,
                        msg_id = ?announcement.msg_id,
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

// ===== Socket Creation =====

/// Create UDP receiver with multicast joins on all eligible interfaces
async fn create_multicast_receiver(addr: &str, config: &DiscoveryConfig) -> Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};

    let socket_addr: std::net::SocketAddr = addr.parse()?;
    let domain = if socket_addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };

    // Windows: Bind with retry/backoff to handle port release delays after restart
    // On Windows, when a process exits (especially via std::process::exit()), the port
    // may not be immediately released. This retry loop handles that overlap window.
    #[cfg(windows)]
    const MAX_BIND_ATTEMPTS: u32 = 10;
    #[cfg(windows)]
    const BIND_RETRY_DELAYS_MS: [u64; 6] = [100, 200, 400, 800, 1600, 2000];

    #[cfg(not(windows))]
    const MAX_BIND_ATTEMPTS: u32 = 1;

    let mut last_error = None;

    for attempt in 0..MAX_BIND_ATTEMPTS {
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

        match socket.bind(&socket_addr.into()) {
            Ok(()) => {
                if attempt > 0 {
                    tracing::info!(
                        attempt = attempt + 1,
                        "UDP bind succeeded after retry"
                    );
                }
                // Continue with multicast setup below
                socket.set_nonblocking(true)?;
                let udp_socket = UdpSocket::from_std(socket.into())?;
                return setup_multicast_joins(udp_socket, config).await;
            }
            Err(e) => {
                last_error = Some(e);
                #[cfg(windows)]
                {
                    let delay_idx = (attempt as usize).min(BIND_RETRY_DELAYS_MS.len() - 1);
                    let delay_ms = BIND_RETRY_DELAYS_MS[delay_idx];
                    tracing::debug!(
                        attempt = attempt + 1,
                        delay_ms,
                        "UDP bind failed, retrying (port may still be held by old process)"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    Err(last_error.map(|e| anyhow::anyhow!("Bind failed after {} attempts: {}", MAX_BIND_ATTEMPTS, e))
        .unwrap_or_else(|| anyhow::anyhow!("Bind failed")))
}

/// Setup multicast joins on all eligible interfaces
async fn setup_multicast_joins(udp_socket: UdpSocket, config: &DiscoveryConfig) -> Result<UdpSocket> {
    // Join multicast group on all eligible interfaces
    let interfaces = enumerate_eligible_interfaces();
    let mut join_count = 0;

    for iface in interfaces {
        match udp_socket
            .join_multicast_v4(config.mcast_group, iface.ip)
        {
            Ok(_) => {
                join_count += 1;
                tracing::debug!(
                    interface = %iface.name,
                    ip = %iface.ip,
                    mcast_group = %config.mcast_group,
                    "Joined multicast group"
                );
            }
            Err(e) => {
                tracing::warn!(
                    error = ?e,
                    interface = %iface.name,
                    "Failed to join multicast"
                );
            }
        }
    }

    if join_count == 0 {
        tracing::warn!("Failed to join multicast on any interface");
    } else {
        tracing::info!(
            join_count,
            mcast_group = %config.mcast_group,
            "Multicast joins complete"
        );
    }

    Ok(udp_socket)
}

/// Create sender socket bound to specific interface
async fn create_interface_sender(
    iface: &NetworkInterface,
    _config: &DiscoveryConfig,
) -> Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

    // Enable broadcast
    socket.set_broadcast(true)?;

    // Set multicast TTL
    socket.set_multicast_ttl_v4(1)?;

    // Bind to specific interface IP (not 0.0.0.0!)
    let bind_addr = SocketAddr::new(IpAddr::V4(iface.ip), 0); // Ephemeral port
    socket.bind(&bind_addr.into())?;

    // Set multicast interface
    socket.set_multicast_if_v4(&iface.ip)?;

    socket.set_nonblocking(true)?;

    Ok(UdpSocket::from_std(socket.into())?)
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_broadcast_slash_24() {
        let iface = NetworkInterface {
            name: "eth0".to_string(),
            ip: Ipv4Addr::new(192, 168, 1, 10),
            netmask: Some(Ipv4Addr::new(255, 255, 255, 0)),
            broadcast: None,
        };

        let bcast = iface.compute_broadcast().unwrap();
        assert_eq!(bcast, Ipv4Addr::new(192, 168, 1, 255));
    }

    #[test]
    fn test_compute_broadcast_slash_20() {
        let iface = NetworkInterface {
            name: "eth0".to_string(),
            ip: Ipv4Addr::new(192, 168, 32, 10),
            netmask: Some(Ipv4Addr::new(255, 255, 240, 0)),
            broadcast: None,
        };

        let bcast = iface.compute_broadcast().unwrap();
        assert_eq!(bcast, Ipv4Addr::new(192, 168, 47, 255));
    }

    #[test]
    fn test_compute_broadcast_slash_16() {
        let iface = NetworkInterface {
            name: "eth0".to_string(),
            ip: Ipv4Addr::new(10, 0, 5, 100),
            netmask: Some(Ipv4Addr::new(255, 255, 0, 0)),
            broadcast: None,
        };

        let bcast = iface.compute_broadcast().unwrap();
        assert_eq!(bcast, Ipv4Addr::new(10, 0, 255, 255));
    }

    #[test]
    fn test_is_virtual_interface() {
        assert!(is_virtual_interface("veth0", &Ipv4Addr::new(192, 168, 1, 1)));
        assert!(is_virtual_interface("docker0", &Ipv4Addr::new(192, 168, 1, 1)));
        assert!(is_virtual_interface("vmnet1", &Ipv4Addr::new(192, 168, 1, 1)));
        assert!(is_virtual_interface(
            "eth0",
            &Ipv4Addr::new(172, 17, 0, 1)
        )); // Docker bridge
        assert!(!is_virtual_interface(
            "eth0",
            &Ipv4Addr::new(192, 168, 1, 1)
        ));
    }

    #[test]
    fn test_discovery_config_defaults() {
        let config = DiscoveryConfig::default();
        assert_eq!(config.port, 7184);
        assert_eq!(config.mcast_group, Ipv4Addr::new(239, 255, 42, 99));
        assert!(config.enable_bcast_fallback);
        assert!(!config.enable_limited_bcast);
    }
}
