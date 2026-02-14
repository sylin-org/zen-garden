//! Network infrastructure utilities
//!
//! Provides network operations:
//! - Wake-on-LAN (WoL) magic packet transmission
//! - Local IP address detection with priority ordering
//! - MAC address discovery for WoL support
//!
//! ## SoC/DDD Separation
//! - This module contains infrastructure concerns (raw UDP for WoL, IP detection)
//! - P2P discovery/communication lives in `communications/p2p.rs`
//! - WoL is hardware management, not service discovery

use anyhow::{Context, Result};
use tokio::net::UdpSocket;

/// Send a Wake-on-LAN magic packet
///
/// The magic packet consists of:
/// - 6 bytes of 0xFF (synchronization stream)
/// - MAC address repeated 16 times
///
/// Sent as UDP broadcast to port 9 (or 7).
///
/// ## Arguments
/// - `mac`: MAC address in format "AA:BB:CC:DD:EE:FF" or "AA-BB-CC-DD-EE-FF"
///
/// ## Examples
/// ```rust,no_run
/// # use garden_common::infra::network::send_wol_packet;
/// # async fn example() -> anyhow::Result<()> {
/// send_wol_packet("00:11:22:33:44:55").await?;
/// # Ok(())
/// # }
/// ```
pub async fn send_wol_packet(mac: &str) -> Result<()> {
    // Parse MAC address (accepts xx:xx:xx:xx:xx:xx or xx-xx-xx-xx-xx-xx)
    let mac_bytes = parse_mac_address(mac)?;

    // Build magic packet: 6 bytes of 0xFF + MAC repeated 16 times
    let mut packet = vec![0xFFu8; 6];
    for _ in 0..16 {
        packet.extend_from_slice(&mac_bytes);
    }

    // Create ephemeral UDP socket for broadcast
    // NOTE: WoL is a special case - it's NOT p2p discovery, it's raw ethernet protocol
    // This is one of the few legitimate uses of direct UdpSocket outside p2p.rs
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .context("Failed to bind UDP socket for WoL")?;

    socket
        .set_broadcast(true)
        .context("Failed to enable broadcast mode")?;

    // Try multiple broadcast addresses for better coverage
    let broadcast_addrs = ["255.255.255.255:9", "255.255.255.255:7"];

    for addr in broadcast_addrs {
        if let Err(e) = socket.send_to(&packet, addr).await {
            tracing::warn!(addr = %addr, error = ?e, "WoL broadcast failed to one address");
        }
    }

    tracing::info!(mac = %mac, "Wake-on-LAN magic packet sent");

    Ok(())
}

/// Parse MAC address string to bytes
///
/// Accepts formats:
/// - AA:BB:CC:DD:EE:FF (colon-separated)
/// - AA-BB-CC-DD-EE-FF (dash-separated)
///
/// ## Arguments
/// - `mac`: MAC address string
///
/// ## Returns
/// 6-byte array representing the MAC address
fn parse_mac_address(mac: &str) -> Result<[u8; 6]> {
    let mac = mac.replace('-', ":");
    let parts: Vec<&str> = mac.split(':').collect();

    if parts.len() != 6 {
        anyhow::bail!(
            "Invalid MAC address format: expected 6 hex pairs, got {}",
            parts.len()
        );
    }

    let mut bytes = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        bytes[i] = u8::from_str_radix(part, 16)
            .with_context(|| format!("Invalid hex in MAC address part {}: {}", i + 1, part))?;
    }

    Ok(bytes)
}

/// Get the local IP address for network communication
///
/// Returns the best non-loopback IPv4 address, preferring LAN addresses over
/// container/virtual network addresses.
///
/// Priority order:
/// 1. Private LAN (192.168.x.x, 10.x.x.x)
/// 2. Carrier-grade NAT (100.64-127.x.x) - for Tailscale, etc.
/// 3. Other private (172.16-31.x.x) but NOT Docker bridge (172.17.x.x)
/// 4. Any other non-loopback, non-link-local
///
/// Used for:
/// - Displaying management URLs
/// - Registering with service discovery
/// - Building service endpoints
pub fn get_local_ip() -> String {
    use std::net::IpAddr;

    // Try priority-based selection first
    if let Some(ip) = get_local_ip_with_priority() {
        tracing::debug!(ip = %ip, "Local IP detected via priority selection");
        return ip;
    }

    tracing::debug!("Priority-based IP selection failed, trying fallback");

    // Fallback: use local_ip_address crate's simpler method
    match local_ip_address::local_ip() {
        Ok(ip) => {
            if let IpAddr::V4(ipv4) = ip {
                // Still skip Docker bridge if we can detect it
                let octets = ipv4.octets();
                if !(octets[0] == 172 && octets[1] == 17) {
                    tracing::debug!(ip = %ipv4, "Local IP detected via fallback");
                    return ipv4.to_string();
                }
                tracing::debug!(ip = %ipv4, "Fallback returned Docker bridge IP, skipping");
            } else {
                tracing::debug!(ip = %ip, "Fallback returned IPv6, skipping");
            }
        }
        Err(e) => {
            tracing::warn!(error = ?e, "local_ip_address::local_ip() failed");
        }
    }

    // Last resort fallback
    tracing::warn!("All IP detection methods failed, using 127.0.0.1");
    "127.0.0.1".to_string()
}

/// Try to get local IP with priority-based selection
fn get_local_ip_with_priority() -> Option<String> {
    use std::net::IpAddr;

    let addrs = local_ip_address::list_afinet_netifas().ok()?;

    tracing::trace!(count = addrs.len(), "Enumerating network interfaces");

    let mut candidates: Vec<(u8, std::net::Ipv4Addr)> = Vec::new();

    for (iface_name, ip) in addrs {
        if let IpAddr::V4(ipv4) = ip {
            // Skip loopback and link-local
            if ipv4.is_loopback() {
                tracing::trace!(iface = %iface_name, ip = %ipv4, "Skipping loopback");
                continue;
            }
            if ipv4.is_link_local() {
                tracing::trace!(iface = %iface_name, ip = %ipv4, "Skipping link-local");
                continue;
            }

            let octets = ipv4.octets();
            let priority = get_ip_priority(octets, &iface_name);

            // Skip addresses with priority 0 (Docker bridge, etc.)
            if priority == 0 {
                tracing::trace!(iface = %iface_name, ip = %ipv4, "Skipping priority-0 interface");
                continue;
            }

            tracing::trace!(iface = %iface_name, ip = %ipv4, priority = priority, "Candidate IP");
            candidates.push((priority, ipv4));
        }
    }

    if candidates.is_empty() {
        tracing::debug!("No valid IP candidates found after filtering");
        return None;
    }

    // Sort by priority (higher is better)
    candidates.sort_by(|a, b| b.0.cmp(&a.0));

    candidates.first().map(|(_, ip)| ip.to_string())
}

/// Get priority score for an IP address (higher = better)
/// Returns 0 for addresses that should be skipped
fn get_ip_priority(octets: [u8; 4], iface_name: &str) -> u8 {
    // Skip Docker/container interfaces by name
    let iface_lower = iface_name.to_lowercase();
    if iface_lower.starts_with("docker")
        || iface_lower.starts_with("br-")
        || iface_lower.starts_with("veth")
        || iface_lower == "docker0"
    {
        return 0;
    }

    match octets {
        // 192.168.x.x - typical home/office LAN (highest priority)
        [192, 168, _, _] => 100,

        // 10.x.x.x - corporate/large private networks
        [10, _, _, _] => 90,

        // 100.64-127.x.x - Carrier-grade NAT (Tailscale, etc.)
        [100, second, _, _] if (64..=127).contains(&second) => 80,

        // 172.16-31.x.x - private range, but check for Docker bridge
        [172, second, _, _] if (16..=31).contains(&second) => {
            // Docker bridge is typically 172.17.x.x - deprioritize heavily
            if second == 17 {
                0 // Skip Docker bridge entirely
            } else {
                70
            }
        }

        // Any other routable address
        _ => 50,
    }
}

/// Get the local IP and MAC address for network communication
///
/// Returns (ip_address, mac_address) tuple using the same interface selection
/// logic as `get_local_ip()`. MAC address may be None if detection fails.
///
/// Used for:
/// - Announcing presence with MAC for Wake-on-LAN support
/// - Network identity in topology cache
pub fn get_local_ip_and_mac() -> (String, Option<String>) {
    // Try priority-based selection first
    if let Some((ip, mac)) = get_local_ip_and_mac_with_priority() {
        return (ip, mac);
    }

    // Fallback: use simple IP detection, no MAC available
    (get_local_ip(), None)
}

/// Try to get local IP and MAC with priority-based selection
fn get_local_ip_and_mac_with_priority() -> Option<(String, Option<String>)> {
    use std::net::IpAddr;

    let addrs = local_ip_address::list_afinet_netifas().ok()?;

    let mut candidates: Vec<(u8, std::net::Ipv4Addr, String)> = Vec::new();

    for (iface_name, ip) in addrs {
        if let IpAddr::V4(ipv4) = ip {
            // Skip loopback and link-local
            if ipv4.is_loopback() || ipv4.is_link_local() {
                continue;
            }

            let octets = ipv4.octets();
            let priority = get_ip_priority(octets, &iface_name);

            // Skip addresses with priority 0 (Docker bridge, etc.)
            if priority == 0 {
                continue;
            }

            candidates.push((priority, ipv4, iface_name));
        }
    }

    if candidates.is_empty() {
        return None;
    }

    // Sort by priority (higher is better)
    candidates.sort_by(|a, b| b.0.cmp(&a.0));

    let (_, ip, iface_name) = candidates.first()?;
    let mac = get_mac_for_interface(iface_name);

    Some((ip.to_string(), mac))
}

/// Get MAC address for a given interface name
///
/// Platform-specific implementation:
/// - Linux: Reads from /sys/class/net/{interface}/address
/// - Windows: Returns None (WoL primarily for Linux stones)
/// - Other: Returns None
fn get_mac_for_interface(interface_name: &str) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let path = format!("/sys/class/net/{}/address", interface_name);
        if let Ok(mac) = std::fs::read_to_string(&path) {
            let mac = mac.trim().to_uppercase();
            // Validate MAC format (xx:xx:xx:xx:xx:xx)
            if mac.len() == 17 && mac.chars().filter(|c| *c == ':').count() == 5 {
                return Some(mac);
            }
        }
        None
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = interface_name;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mac_address_colon() {
        let result = parse_mac_address("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(result, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn test_parse_mac_address_dash() {
        let result = parse_mac_address("AA-BB-CC-DD-EE-FF").unwrap();
        assert_eq!(result, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn test_parse_mac_address_lowercase() {
        let result = parse_mac_address("aa:bb:cc:dd:ee:ff").unwrap();
        assert_eq!(result, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    #[test]
    fn test_parse_mac_address_invalid_format() {
        let result = parse_mac_address("AA:BB:CC:DD:EE");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_mac_address_invalid_hex() {
        let result = parse_mac_address("AA:BB:CC:DD:EE:GG");
        assert!(result.is_err());
    }
}
