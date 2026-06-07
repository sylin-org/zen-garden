//! Shared HTTP clients for Moss
//!
//! Provides process-wide reqwest::Client singletons to avoid per-request
//! construction overhead (~2ms each) and enable connection pooling.
//!
//! See code-standards.md §19: "Share expensive resources".

use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;

/// rustls client config backed by bundled webpki roots.
///
/// reqwest 0.13 + the `rustls` feature defaults to `rustls-platform-verifier`, which
/// queries the OS trust store. On hosts without one (Android/bionic, where `/etc/ssl`
/// is absent) that fails at client build with "No CA certificates were loaded from the
/// system". Supplying bundled roots via a preconfigured config makes every Moss HTTP
/// client build and verify TLS independent of the host trust store. Built with an explicit
/// aws-lc-rs provider so it does not depend on a process-level default being installed.
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

/// Base reqwest client builder whose TLS roots follow the host profile's `tls.root_source`.
///
/// Use this instead of `reqwest::Client::builder()` for any Moss HTTP client.
/// - `System` (conventional Linux default): the OS trust store — honors corporate
///   proxies / custom PKI (reqwest's default rustls-platform-verifier).
/// - `Bundled` (Android default): bundled webpki roots — works with no system CA store.
/// - `Merged` (minimal/air-gapped default): bundled roots (extra-CA merging is a follow-up).
pub fn client_builder() -> reqwest::ClientBuilder {
    use garden_common::host::TlsRootSource;
    let builder = reqwest::Client::builder();
    match garden_common::host::profile().tls.root_source {
        TlsRootSource::System => builder,
        TlsRootSource::Bundled | TlsRootSource::Merged => {
            builder.use_preconfigured_tls(webpki_tls_config())
        }
    }
}

/// General-purpose HTTP client with 30-second timeout.
///
/// Use for stone-to-stone API calls, lantern registration, resources
/// fetching, and any request that doesn't need special TLS config.
pub static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    client_builder()
        .timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(4)
        .build()
        .expect("shared HTTP client")
});

/// HTTP client for companion command forwarding.
///
/// Uses the companion-specific timeout from `garden_common::constants`.
pub static COMPANION: LazyLock<reqwest::Client> = LazyLock::new(|| {
    client_builder()
        .timeout(garden_common::constants::timeouts::companion_command_timeout())
        .build()
        .expect("companion HTTP client")
});

/// HTTP client that accepts invalid TLS certificates.
///
/// Used for proxying requests to peer stones whose certificates are
/// issued by the pond CA (not in the system trust store).
pub static INSECURE_PROXY: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(30))
        .build()
        .expect("insecure proxy HTTP client")
});
