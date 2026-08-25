//! Local Moss HTTP client (COMPANION-0014).
//!
//! Companions run on the same stone as moss; the loopback HTTP API is
//! the canonical, authoritative source of stone state. `MossLocalClient`
//! is the thin facade adapters use to query that state.
//!
//! The contract is request/response: callers ask, moss answers. There
//! is no client-side state aggregate, no projection, no race against
//! event timing. Live deltas continue to flow via the SSE → Pulse
//! pipeline; this client serves the read path.

use anyhow::{Context, Result};
use garden_common::presence::PresenceSnapshot;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

/// Default timeout for any single HTTP call. Generous because moss is
/// on loopback — anything slower than this is moss being unhealthy.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Shared `reqwest::Client` — connection pooling matters even for loopback because adapter-side
/// rehydration may fire repeatedly. Built via the per-profile HTTP factory (`garden_common::http`)
/// so TLS roots are platform-correct: Android/bionic has no system trust store, where reqwest's
/// default rustls platform-verifier would otherwise panic at client build.
fn shared_http() -> Arc<reqwest::Client> {
    static CLIENT: OnceLock<Arc<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Arc::new(
                garden_common::http::client_builder()
                    .timeout(DEFAULT_TIMEOUT)
                    .build()
                    .expect("companion moss-client"),
            )
        })
        .clone()
}

/// Local moss HTTP client.
#[derive(Clone)]
pub struct MossLocalClient {
    base: String,
    http: Arc<reqwest::Client>,
}

impl MossLocalClient {
    /// Construct a client targeting the given moss endpoint.
    /// Typically `http://127.0.0.1:7185`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            base: endpoint.into().trim_end_matches('/').to_string(),
            http: shared_http(),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.base
    }

    /// Fetch the current stone presence snapshot. The same shape that
    /// the SSE stream emits as its first event — but delivered as a
    /// deterministic HTTP response. Adapters call this at startup to
    /// hydrate their display before entering the live event loop.
    pub async fn presence_snapshot(&self) -> Result<PresenceSnapshot> {
        let url = format!("{}/api/v1/stone/presence", self.base);
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        if !response.status().is_success() {
            anyhow::bail!(
                "GET {url} returned HTTP {}",
                response.status()
            );
        }
        response
            .json::<PresenceSnapshot>()
            .await
            .context("decode presence snapshot")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_strips_trailing_slash() {
        let c = MossLocalClient::new("http://127.0.0.1:7185/");
        assert_eq!(c.endpoint(), "http://127.0.0.1:7185");
    }

    #[test]
    fn shared_http_is_pooled() {
        let a = shared_http();
        let b = shared_http();
        assert!(Arc::ptr_eq(&a, &b));
    }
}
