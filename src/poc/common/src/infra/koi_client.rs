//! Shared mDNS discovery types
//!
//! Canonical types and helpers used by Moss, Lantern, and other Zen Garden
//! binaries for mDNS-based stone discovery. The actual mDNS transport is
//! provided by `koi-embedded` - this module holds only data types and
//! network classification helpers that live above the transport layer.

/// Check if an IP address is LAN-routable.
///
/// Accepts private ranges (RFC 1918), rejects loopback, link-local, and Docker bridge.
pub fn is_lan_routable(ip: &str) -> bool {
    let addr: std::net::Ipv4Addr = match ip.parse() {
        Ok(a) => a,
        Err(_) => return false, // IPv6 or invalid - skip
    };

    let octets = addr.octets();

    // 10.0.0.0/8
    if octets[0] == 10 {
        return true;
    }

    // 172.16.0.0/12 (excluding 172.17.0.0/16 - Docker default bridge)
    if octets[0] == 172 && (16..=31).contains(&octets[1]) && octets[1] != 17 {
        return true;
    }

    // 192.168.0.0/16
    if octets[0] == 192 && octets[1] == 168 {
        return true;
    }

    false
}

use crate::PeerAddress;

/// A stone discovered via mDNS.
///
/// This is the canonical discovery result type used by all consumers.
/// Self-filtering (skip own stone_name) is the caller's responsibility.
#[derive(Debug, Clone)]
pub struct DiscoveredStone {
    pub stone_id: Option<String>,
    pub stone_name: String,
    /// Network address of the discovered stone.
    pub address: PeerAddress,
    pub mac: Option<String>,
    pub version: Option<String>,
    pub health: Option<String>,
    pub discovered_at: chrono::DateTime<chrono::Utc>,
}
