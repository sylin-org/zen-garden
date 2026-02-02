//! Network addressing domain types
//!
//! Domain model for static IP management with offering-bound lifecycle.
//! Static IP is tied to offerings that request it - when the last requester
//! is removed, the system automatically reverts to DHCP.
//!
//! ## Lifecycle Rules
//! 1. First offering requesting static IP → apply from pool
//! 2. Additional offerings → share existing static IP (reference count)
//! 3. Last offering removed → revert to DHCP
//!
//! ## Safety
//! - DHCP fallback on any failure
//! - Never modify existing network configs
//! - Atomic operations with rollback

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

// ============================================================================
// Network Mode
// ============================================================================

/// Network addressing mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NetworkMode {
    /// OS-managed DHCP (default)
    Dhcp,

    /// Moss-managed static IP from configured pool
    Static {
        address: Ipv4Addr,
        applied_at: DateTime<Utc>,
    },

    /// Static IP desired but fell back to DHCP
    FallbackDhcp {
        desired: Ipv4Addr,
        reason: String,
        fallback_at: DateTime<Utc>,
    },
}

impl Default for NetworkMode {
    fn default() -> Self {
        Self::Dhcp
    }
}

impl NetworkMode {
    /// Create a new static mode
    pub fn static_ip(address: Ipv4Addr) -> Self {
        Self::Static {
            address,
            applied_at: Utc::now(),
        }
    }

    /// Create a fallback DHCP mode
    pub fn fallback(desired: Ipv4Addr, reason: impl Into<String>) -> Self {
        Self::FallbackDhcp {
            desired,
            reason: reason.into(),
            fallback_at: Utc::now(),
        }
    }

    /// Check if currently using static IP
    pub fn is_static(&self) -> bool {
        matches!(self, Self::Static { .. })
    }

    /// Check if in DHCP mode (including fallback)
    pub fn is_dhcp(&self) -> bool {
        matches!(self, Self::Dhcp | Self::FallbackDhcp { .. })
    }

    /// Get the static IP address if in static mode
    pub fn static_address(&self) -> Option<Ipv4Addr> {
        match self {
            Self::Static { address, .. } => Some(*address),
            _ => None,
        }
    }

    /// Get mode name for display
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Dhcp => "DHCP",
            Self::Static { .. } => "Static",
            Self::FallbackDhcp { .. } => "Fallback DHCP",
        }
    }
}

// ============================================================================
// Static IP State (Persistent)
// ============================================================================

/// Persistent static IP state with offering-bound lifecycle
///
/// This is persisted to `/etc/zen-garden/network-state.json` and tracks
/// which offerings are using the static IP (reference counting).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StaticIpState {
    /// Schema version for forward compatibility
    #[serde(default = "default_version")]
    pub version: u8,

    /// Current network mode
    #[serde(default)]
    pub mode: NetworkMode,

    /// Offerings currently using the static IP (reference counting)
    /// When empty, system reverts to DHCP
    #[serde(default)]
    pub requested_by: Vec<String>,

    /// Desired static IP configuration (what we want to apply)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desired: Option<StaticIpDesired>,

    /// Currently active configuration (what's actually applied)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<StaticIpActive>,
}

fn default_version() -> u8 {
    1
}

impl StaticIpState {
    /// Create new empty state (DHCP mode)
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an offering as a requester
    ///
    /// Returns true if this is the first requester (need to apply static IP)
    pub fn add_requester(&mut self, offering: &str) -> bool {
        let is_first = self.requested_by.is_empty();
        if !self.requested_by.contains(&offering.to_string()) {
            self.requested_by.push(offering.to_string());
        }
        is_first
    }

    /// Remove an offering as a requester
    ///
    /// Returns true if no requesters remain (need to revert to DHCP)
    pub fn remove_requester(&mut self, offering: &str) -> bool {
        self.requested_by.retain(|o| o != offering);
        self.requested_by.is_empty()
    }

    /// Check if any offerings are requesting static IP
    pub fn has_requesters(&self) -> bool {
        !self.requested_by.is_empty()
    }

    /// Get count of requesters
    pub fn requester_count(&self) -> usize {
        self.requested_by.len()
    }
}

/// Desired static IP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticIpDesired {
    pub address: Ipv4Addr,
    pub prefix_length: u8,
    pub gateway: Ipv4Addr,
    pub dns: Vec<Ipv4Addr>,
    pub interface: String,
}

/// Active static IP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticIpActive {
    pub address: Ipv4Addr,
    #[serde(default = "default_obtained_via")]
    pub obtained_via: String,
    pub applied_at: DateTime<Utc>,
}

fn default_obtained_via() -> String {
    "static".to_string()
}

// ============================================================================
// Static IP Request/Release Events
// ============================================================================

/// Request for static IP assignment (domain event)
#[derive(Debug, Clone)]
pub struct StaticIpRequest {
    pub offering: String,
    pub reason: String,
    pub severity: StaticIpSeverity,
}

/// Request to release static IP (domain event, on offering removal)
#[derive(Debug, Clone)]
pub struct StaticIpRelease {
    pub offering: String,
}

/// Static IP request severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticIpSeverity {
    /// Informational - offering works without static IP
    Info,
    /// Warning - offering works better with static IP
    Warn,
    /// Required - offering refuses to install without static IP
    Required,
}

impl StaticIpSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Required => "required",
        }
    }
}

// ============================================================================
// Probe Results
// ============================================================================

/// Result of IP conflict probing
#[derive(Debug, Clone)]
pub enum ProbeResult {
    /// IP is available for use
    Available,

    /// IP is in use by another device
    Conflict {
        method: &'static str,
        responder_mac: Option<String>,
    },

    /// IP is bound locally (loopback, etc.)
    LocalConflict,

    /// Probing failed (couldn't determine status)
    Error(String),
}

impl ProbeResult {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    pub fn is_conflict(&self) -> bool {
        matches!(self, Self::Conflict { .. } | Self::LocalConflict)
    }
}

/// Error when all IPs in pool are exhausted
#[derive(Debug, Clone)]
pub struct PoolExhausted {
    pub pool_start: Ipv4Addr,
    pub pool_end: Ipv4Addr,
    pub conflicts: Vec<(Ipv4Addr, String)>, // (ip, reason)
}

impl std::fmt::Display for PoolExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "All IPs in pool {}-{} have conflicts ({} addresses checked)",
            self.pool_start,
            self.pool_end,
            self.conflicts.len()
        )
    }
}

impl std::error::Error for PoolExhausted {}

// ============================================================================
// Network Errors
// ============================================================================

/// Network configuration errors
#[derive(Debug)]
pub enum NetworkError {
    /// Static IP pool not configured
    PoolNotConfigured,

    /// All IPs in pool are in use
    PoolExhausted(PoolExhausted),

    /// Platform adapter not available
    PlatformNotSupported(String),

    /// Insufficient privileges
    PrivilegeRequired(String),

    /// Configuration apply failed
    ApplyFailed(String),

    /// State persistence failed
    PersistenceFailed(String),

    /// Probe failed
    ProbeFailed(String),

    /// Generic I/O error
    Io(std::io::Error),
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PoolNotConfigured => write!(f, "Static IP pool not configured in garden-moss.toml"),
            Self::PoolExhausted(e) => write!(f, "{}", e),
            Self::PlatformNotSupported(p) => write!(f, "Platform not supported: {}", p),
            Self::PrivilegeRequired(msg) => write!(f, "Privilege required: {}", msg),
            Self::ApplyFailed(msg) => write!(f, "Failed to apply network configuration: {}", msg),
            Self::PersistenceFailed(msg) => write!(f, "Failed to persist network state: {}", msg),
            Self::ProbeFailed(msg) => write!(f, "IP probe failed: {}", msg),
            Self::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for NetworkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PoolExhausted(e) => Some(e),
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for NetworkError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_mode_default() {
        let mode = NetworkMode::default();
        assert!(mode.is_dhcp());
        assert!(!mode.is_static());
    }

    #[test]
    fn test_network_mode_static() {
        let addr = "192.168.1.100".parse().unwrap();
        let mode = NetworkMode::static_ip(addr);
        assert!(mode.is_static());
        assert!(!mode.is_dhcp());
        assert_eq!(mode.static_address(), Some(addr));
    }

    #[test]
    fn test_static_ip_state_requesters() {
        let mut state = StaticIpState::new();

        // First requester
        let is_first = state.add_requester("pihole");
        assert!(is_first);
        assert_eq!(state.requester_count(), 1);

        // Second requester
        let is_first = state.add_requester("bind9");
        assert!(!is_first);
        assert_eq!(state.requester_count(), 2);

        // Duplicate add (should not increase count)
        state.add_requester("pihole");
        assert_eq!(state.requester_count(), 2);

        // Remove first
        let is_empty = state.remove_requester("pihole");
        assert!(!is_empty);
        assert_eq!(state.requester_count(), 1);

        // Remove last
        let is_empty = state.remove_requester("bind9");
        assert!(is_empty);
        assert!(!state.has_requesters());
    }

    #[test]
    fn test_probe_result() {
        let available = ProbeResult::Available;
        assert!(available.is_available());
        assert!(!available.is_conflict());

        let conflict = ProbeResult::Conflict {
            method: "arp",
            responder_mac: Some("aa:bb:cc:dd:ee:ff".to_string()),
        };
        assert!(!conflict.is_available());
        assert!(conflict.is_conflict());
    }
}
