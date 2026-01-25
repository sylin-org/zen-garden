//! Network infrastructure utilities
//!
//! Provides low-level network operations not covered by p2p transport:
//! - Wake-on-LAN (WoL) magic packet transmission
//! - Raw ethernet protocol support
//!
//! ## SoC/DDD Separation
//! - This module contains infrastructure concerns (raw UDP sockets for WoL)
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
        anyhow::bail!("Invalid MAC address format: expected 6 hex pairs, got {}", parts.len());
    }

    let mut bytes = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        bytes[i] = u8::from_str_radix(part, 16)
            .with_context(|| format!("Invalid hex in MAC address part {}: {}", i + 1, part))?;
    }

    Ok(bytes)
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
