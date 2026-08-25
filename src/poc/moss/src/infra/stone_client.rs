//! Stone-to-stone HTTP client gateway
//!
//! `StoneClient` centralizes all inter-stone HTTP communication behind a
//! single type that:
//!
//! - Maintains connection-pooled HTTP and TLS clients
//! - Resolves the correct transport (HTTP/HTTPS) per peer
//! - Applies mTLS identity when pond security is active
//! - Reloads TLS configuration on enrollment events
//!
//! ## Usage
//!
//! ```ignore
//! // In an API handler with access to Moss:
//! let resp = state.stone_client
//!     .get(&entry.address, "/api/v1/services")
//!     .timeout(Duration::from_secs(5))
//!     .send()
//!     .await?;
//! ```
//!
//! ## Lifecycle
//!
//! Created once during bootstrap, stored in `Moss`. When pond enrollment
//! changes (`PondEvent::EnrollmentChanged`), call `reload_tls()` to rebuild
//! the TLS client with fresh certificates.

use crate::domain::traits::PondClient;
use anyhow::{Context, Result};
use garden_common::PeerAddress;
use reqwest::Method;
use std::path::PathBuf;
use std::time::Duration;

/// Default timeout for inter-stone requests when callers don't specify one.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Stone-to-stone HTTP client.
///
/// Holds a permanent HTTP client and an optional TLS client (present when
/// this stone is enrolled in a pond with certmesh certificates).
///
/// All methods return `reqwest::RequestBuilder` — callers chain `.timeout()`,
/// `.json()`, `.header()`, `.send()` as needed.
pub struct StoneClient {
    /// Plain HTTP client (always available).
    http: reqwest::Client,

    /// TLS client with mTLS identity (available when pond certs exist).
    /// Protected by `std::sync::RwLock` — the lock is held only long enough
    /// to clone the `reqwest::Client` (an Arc bump, nanoseconds).
    tls: std::sync::RwLock<Option<reqwest::Client>>,

    /// This stone's name (for cert path resolution).
    stone_name: String,
}

impl StoneClient {
    /// Create a new `StoneClient`.
    ///
    /// Probes for certmesh certificates on disk. If found, builds a TLS
    /// client with mTLS identity and pond CA trust.
    pub fn new(stone_name: &str) -> Self {
        let http = crate::http::client_builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .expect("Failed to build HTTP client");

        let tls = Self::try_build_tls_client(stone_name);
        if tls.is_some() {
            tracing::info!("StoneClient: TLS client initialized (pond certs found)");
        } else {
            tracing::debug!("StoneClient: HTTP-only mode (no pond certs)");
        }

        Self {
            http,
            tls: std::sync::RwLock::new(tls),
            stone_name: stone_name.to_string(),
        }
    }

    /// Reload TLS client after enrollment changes (join/drain).
    ///
    /// Call this when `PondEvent::EnrollmentChanged` fires. If certs exist
    /// on disk, rebuilds the mTLS client. If not, clears the TLS client
    /// (e.g., after pond drain).
    pub fn reload_tls(&self) {
        let new_tls = Self::try_build_tls_client(&self.stone_name);
        let mut guard = self.tls.write().expect("StoneClient TLS lock poisoned");

        if new_tls.is_some() {
            tracing::info!("StoneClient: TLS client reloaded");
        } else {
            tracing::info!("StoneClient: TLS client cleared (no certs)");
        }

        *guard = new_tls;
    }

    /// Whether a TLS client is currently available.
    pub fn has_tls(&self) -> bool {
        self.tls
            .read()
            .expect("StoneClient TLS lock poisoned")
            .is_some()
    }

    // ── Request builders ───────────────────────────────────────────────

    /// Build a GET request to a peer stone.
    ///
    /// Automatically upgrades to HTTPS if both sides support TLS.
    pub fn get(&self, peer: &PeerAddress, path: &str) -> reqwest::RequestBuilder {
        self.request(Method::GET, peer, path)
    }

    /// Build a POST request to a peer stone.
    pub fn post(&self, peer: &PeerAddress, path: &str) -> reqwest::RequestBuilder {
        self.request(Method::POST, peer, path)
    }

    /// Build a PUT request to a peer stone.
    pub fn put(&self, peer: &PeerAddress, path: &str) -> reqwest::RequestBuilder {
        self.request(Method::PUT, peer, path)
    }

    /// Build a DELETE request to a peer stone.
    pub fn delete(&self, peer: &PeerAddress, path: &str) -> reqwest::RequestBuilder {
        self.request(Method::DELETE, peer, path)
    }

    /// Build a request with an explicit HTTP method.
    ///
    /// Resolves the URL based on peer TLS capability and local cert
    /// availability. If both sides have TLS, uses HTTPS with mTLS.
    /// Otherwise, falls back to HTTP.
    pub fn request(
        &self,
        method: Method,
        peer: &PeerAddress,
        path: &str,
    ) -> reqwest::RequestBuilder {
        let tls_guard = self.tls.read().expect("StoneClient TLS lock poisoned");

        if let (Some(tls_client), true) = (&*tls_guard, peer.has_tls()) {
            // Both sides have TLS — use HTTPS with mTLS
            let url = peer
                .https_url(path)
                .expect("has_tls() was true but https_url returned None");
            tls_client.request(method, url)
        } else {
            // Fallback to plain HTTP
            let url = peer.http_url(path);
            self.http.request(method, url)
        }
    }

    // ── Internal ───────────────────────────────────────────────────────

    /// Attempt to build a TLS client from certmesh certificates on disk.
    ///
    /// Returns `None` if cert files don't exist (stone not enrolled).
    fn try_build_tls_client(stone_name: &str) -> Option<reqwest::Client> {
        let certs_dir = Self::certs_dir(stone_name);

        let cert_path = certs_dir.join("cert.pem");
        let key_path = certs_dir.join("key.pem");
        let ca_path = certs_dir.join("ca.pem");

        if !cert_path.exists() || !key_path.exists() {
            return None;
        }

        match Self::build_tls_client_inner(&cert_path, &key_path, &ca_path) {
            Ok(client) => Some(client),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to build TLS client from certs");
                None
            }
        }
    }

    fn build_tls_client_inner(
        cert_path: &std::path::Path,
        key_path: &std::path::Path,
        ca_path: &std::path::Path,
    ) -> Result<reqwest::Client> {
        let cert_pem = std::fs::read(cert_path).context("read cert.pem")?;
        let key_pem = std::fs::read(key_path).context("read key.pem")?;

        // Build PKCS#8 identity from cert + key PEM concatenation
        let mut identity_pem = cert_pem;
        identity_pem.extend_from_slice(&key_pem);

        let identity =
            reqwest::Identity::from_pem(&identity_pem).context("parse client identity")?;

        // Load CA certificate for peer verification (if available)
        let mut builder = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .identity(identity)
            .use_rustls_tls();

        if ca_path.exists() {
            let ca_pem = std::fs::read(ca_path).context("read ca.pem")?;
            let ca_cert = reqwest::Certificate::from_pem(&ca_pem).context("parse CA cert")?;
            builder = builder.add_root_certificate(ca_cert);
        }

        builder.build().context("build TLS client")
    }

    fn certs_dir(stone_name: &str) -> PathBuf {
        PathBuf::from(garden_common::constants::paths::data_dir())
            .join("koi")
            .join("certs")
            .join(stone_name)
    }
}

// reqwest::Client is Clone + Send + Sync, so StoneClient can be shared via Arc.
// The RwLock is std::sync (not tokio) since we only hold it for Clone operations.

impl PondClient for StoneClient {
    fn get(&self, address: &PeerAddress, path: &str) -> reqwest::RequestBuilder {
        StoneClient::get(self, address, path)
    }

    fn post(&self, address: &PeerAddress, path: &str) -> reqwest::RequestBuilder {
        StoneClient::post(self, address, path)
    }

    fn put(&self, address: &PeerAddress, path: &str) -> reqwest::RequestBuilder {
        StoneClient::put(self, address, path)
    }

    fn delete(&self, address: &PeerAddress, path: &str) -> reqwest::RequestBuilder {
        StoneClient::delete(self, address, path)
    }

    fn reload_tls(&self) {
        StoneClient::reload_tls(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_peer() -> PeerAddress {
        PeerAddress::new("192.168.1.10".parse().unwrap(), 7185)
    }

    fn test_peer_tls() -> PeerAddress {
        PeerAddress::new("192.168.1.10".parse().unwrap(), 7185).with_tls(7183)
    }

    #[test]
    fn new_client_http_only() {
        let client = StoneClient::new("test-stone");
        // No certs on disk in test environment
        assert!(!client.has_tls());
    }

    #[test]
    fn get_returns_request_builder() {
        let client = StoneClient::new("test-stone");
        // Just verify it doesn't panic — we can't easily inspect the URL
        // from a RequestBuilder, but we can verify the method works.
        let _builder = client.get(&test_peer(), "/api/v1/health");
    }

    #[test]
    fn post_returns_request_builder() {
        let client = StoneClient::new("test-stone");
        let _builder = client.post(&test_peer(), "/api/v1/services");
    }

    #[test]
    fn request_without_tls_uses_http() {
        let client = StoneClient::new("test-stone");
        // Even if peer has TLS, we don't have certs → HTTP fallback
        let _builder = client.get(&test_peer_tls(), "/api/v1/health");
    }

    #[test]
    fn reload_tls_without_certs_is_noop() {
        let client = StoneClient::new("test-stone");
        client.reload_tls();
        assert!(!client.has_tls());
    }
}
