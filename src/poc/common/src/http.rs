//! Profile-driven HTTP client construction — the per-profile HTTP capability.
//!
//! This is the single place in the codebase that builds `reqwest` clients. The TLS roots follow
//! `HostProfile.tls.root_source`, so every component (moss, companions, …) gets a client that
//! builds and verifies TLS correctly on its platform.
//!
//! Why this exists: reqwest 0.13 + the `rustls` feature defaults to `rustls-platform-verifier`,
//! which queries the OS trust store. Hosts without one (Android/bionic — no `/etc/ssl`) fail at
//! client **build** with "No CA certificates were loaded from the system". Supplying bundled webpki
//! roots via a preconfigured config makes the client build independent of the host trust store.
//!
//! **Use [`client_builder`] instead of `reqwest::Client::builder()` everywhere.**

use crate::host::{TlsRootSource, profile};
use std::sync::Arc;

/// rustls client config backed by bundled webpki roots. Built with an explicit aws-lc-rs provider
/// so it does not depend on a process-level default being installed.
fn webpki_tls_config() -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("rustls default protocol versions")
    .with_root_certificates(roots)
    .with_no_client_auth()
}

/// A `reqwest::ClientBuilder` whose TLS roots follow the host profile's `tls.root_source`.
/// Callers add their own timeouts / pool settings, then `.build()`.
///
/// - `System` (conventional Linux default): the OS trust store (reqwest's default
///   rustls-platform-verifier) — honors corporate proxies / custom PKI.
/// - `Bundled` (Android default): bundled webpki roots — works with no system CA store.
/// - `Merged` (minimal/air-gapped default): bundled roots (extra-CA merging is a follow-up).
pub fn client_builder() -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();
    match profile().tls.root_source {
        TlsRootSource::System => builder,
        TlsRootSource::Bundled | TlsRootSource::Merged => {
            builder.use_preconfigured_tls(webpki_tls_config())
        }
    }
}
