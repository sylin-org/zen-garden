//! Shared HTTP clients for Moss
//!
//! Provides process-wide reqwest::Client singletons to avoid per-request
//! construction overhead (~2ms each) and enable connection pooling.
//!
//! See code-standards.md §19: "Share expensive resources".

use std::sync::LazyLock;
use std::time::Duration;

/// Base reqwest client builder whose TLS roots follow the host profile's `tls.root_source`.
///
/// Delegates to the shared per-profile HTTP factory (`garden_common::http`) — the single place
/// that builds reqwest clients with platform-appropriate TLS roots. Kept as a thin wrapper so the
/// Moss statics below and existing call sites are unchanged.
/// Use this instead of `reqwest::Client::builder()` for any Moss HTTP client.
pub fn client_builder() -> reqwest::ClientBuilder {
    garden_common::http::client_builder()
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
