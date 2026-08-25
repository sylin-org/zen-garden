//! Network address for a peer stone
//!
//! `PeerAddress` is a value object representing where a stone lives on the
//! network. It replaces stringly-typed `endpoint: String` fields with a
//! structured type that separates identity from transport.
//!
//! ## Fields
//!
//! - `ip` — The stone's LAN IP address
//! - `port` — HTTP port (typically 7185)
//! - `tls_port` — HTTPS port when pond security is active (typically 7183)
//!
//! ## Usage
//!
//! ```rust
//! use garden_common::PeerAddress;
//! use std::net::IpAddr;
//!
//! // Plain HTTP peer
//! let addr = PeerAddress::new("192.168.1.10".parse().unwrap(), 7185);
//! assert_eq!(addr.http_base(), "http://192.168.1.10:7185");
//! assert!(!addr.has_tls());
//!
//! // Pond-enrolled peer with HTTPS
//! let addr = addr.with_tls(7183);
//! assert_eq!(addr.https_base(), Some("https://192.168.1.10:7183".to_string()));
//! assert!(addr.has_tls());
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::IpAddr;

/// Network address for a peer stone.
///
/// Structured replacement for `endpoint: String`. Carries the stone's IP,
/// HTTP port, and optional HTTPS port (set when pond security is active).
///
/// The transport decision (HTTP vs HTTPS) is made by [`StoneClient`], not by
/// the address itself — `PeerAddress` is a pure value object.
///
/// [`StoneClient`]: crate (moss::infra::stone_client::StoneClient — separate crate)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PeerAddress {
    /// Stone's LAN-routable IP address.
    pub ip: IpAddr,
    /// HTTP port (e.g., 7185 for Moss, 7186 for Lantern).
    pub port: u16,
    /// HTTPS port when pond security is active (e.g., 7183).
    /// `None` means the peer has no TLS capability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_port: Option<u16>,
}

impl PeerAddress {
    /// Create a new HTTP-only peer address.
    pub fn new(ip: IpAddr, port: u16) -> Self {
        Self {
            ip,
            port,
            tls_port: None,
        }
    }

    /// Parse a `PeerAddress` from an HTTP endpoint string like `"http://192.168.1.10:7185"`.
    ///
    /// This is a migration helper for code that still passes endpoint strings.
    /// Falls back to `0.0.0.0:7185` for unparseable input.
    pub fn from_http_url(endpoint: &str) -> Self {
        let without_proto = endpoint
            .strip_prefix("http://")
            .or_else(|| endpoint.strip_prefix("https://"))
            .unwrap_or(endpoint);
        let without_path = without_proto.split('/').next().unwrap_or(without_proto);
        let (ip_str, port) = match without_path.rsplit_once(':') {
            Some((h, p)) => (h, p.parse().unwrap_or(7185)),
            None => (without_path, 7185u16),
        };
        let ip = ip_str
            .parse()
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        Self::new(ip, port)
    }

    /// Return a copy with TLS port set.
    #[must_use]
    pub fn with_tls(mut self, tls_port: u16) -> Self {
        self.tls_port = Some(tls_port);
        self
    }

    /// Whether this peer advertises TLS capability.
    pub fn has_tls(&self) -> bool {
        self.tls_port.is_some()
    }

    /// HTTP base URL: `http://IP:PORT`
    pub fn http_base(&self) -> String {
        format!("http://{}:{}", self.ip, self.port)
    }

    /// HTTPS base URL when TLS is available: `https://IP:TLS_PORT`
    ///
    /// Returns `None` if `tls_port` is not set.
    pub fn https_base(&self) -> Option<String> {
        self.tls_port.map(|p| format!("https://{}:{}", self.ip, p))
    }

    /// Build an HTTP URL for a given path.
    ///
    /// ```rust
    /// # use garden_common::PeerAddress;
    /// let addr = PeerAddress::new("10.0.0.1".parse().unwrap(), 7185);
    /// assert_eq!(addr.http_url("/api/v1/health"), "http://10.0.0.1:7185/api/v1/health");
    /// ```
    pub fn http_url(&self, path: &str) -> String {
        format!("http://{}:{}{}", self.ip, self.port, path)
    }

    /// Build an HTTPS URL for a given path if TLS is available.
    pub fn https_url(&self, path: &str) -> Option<String> {
        self.tls_port
            .map(|p| format!("https://{}:{}{}", self.ip, p, path))
    }

    /// The IP address as a string.
    pub fn ip_str(&self) -> String {
        self.ip.to_string()
    }
}

/// Display format: `192.168.1.10:7185` (or `192.168.1.10:7185[tls:7183]` with TLS).
impl fmt::Display for PeerAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)?;
        if let Some(tls) = self.tls_port {
            write!(f, "[tls:{}]", tls)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr() -> PeerAddress {
        PeerAddress::new("192.168.1.10".parse().unwrap(), 7185)
    }

    #[test]
    fn new_has_no_tls() {
        let a = addr();
        assert!(!a.has_tls());
        assert_eq!(a.https_base(), None);
    }

    #[test]
    fn with_tls_sets_port() {
        let a = addr().with_tls(7183);
        assert!(a.has_tls());
        assert_eq!(a.tls_port, Some(7183));
    }

    #[test]
    fn http_base_format() {
        assert_eq!(addr().http_base(), "http://192.168.1.10:7185");
    }

    #[test]
    fn https_base_format() {
        let a = addr().with_tls(7183);
        assert_eq!(
            a.https_base(),
            Some("https://192.168.1.10:7183".to_string())
        );
    }

    #[test]
    fn http_url_with_path() {
        assert_eq!(
            addr().http_url("/api/v1/health"),
            "http://192.168.1.10:7185/api/v1/health"
        );
    }

    #[test]
    fn https_url_with_path() {
        let a = addr().with_tls(7183);
        assert_eq!(
            a.https_url("/api/v1/health"),
            Some("https://192.168.1.10:7183/api/v1/health".to_string())
        );
    }

    #[test]
    fn display_plain() {
        assert_eq!(format!("{}", addr()), "192.168.1.10:7185");
    }

    #[test]
    fn display_with_tls() {
        assert_eq!(
            format!("{}", addr().with_tls(7183)),
            "192.168.1.10:7185[tls:7183]"
        );
    }

    #[test]
    fn serde_roundtrip_no_tls() {
        let a = addr();
        let json = serde_json::to_string(&a).unwrap();
        let b: PeerAddress = serde_json::from_str(&json).unwrap();
        assert_eq!(a, b);
        assert!(!json.contains("tls_port")); // skip_serializing_if
    }

    #[test]
    fn serde_roundtrip_with_tls() {
        let a = addr().with_tls(7183);
        let json = serde_json::to_string(&a).unwrap();
        let b: PeerAddress = serde_json::from_str(&json).unwrap();
        assert_eq!(a, b);
        assert!(json.contains("tls_port"));
    }

    #[test]
    fn deserialize_without_tls_port_field() {
        let json = r#"{"ip":"10.0.0.1","port":7185}"#;
        let a: PeerAddress = serde_json::from_str(json).unwrap();
        assert!(!a.has_tls());
    }
}
