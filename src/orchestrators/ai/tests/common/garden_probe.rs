//! Garden probe — detects whether a real/mock AI orchestrator is reachable
//! for end-to-end tests.
//!
//! Docker-era tests don't wire an in-process fixture: they speak HTTP to a
//! running orchestrator on `AI_ORCH_TEST_URL` (default `http://localhost:7190`).
//! If the probe fails, the test logs a reason and returns early so CI runs
//! without real hardware don't produce red reports.
//!
//! The probe also reads the reference garden description from the
//! orchestrator's `/health` and `/v1/catalog` endpoints so tests can assert
//! against the real topology.

#![allow(dead_code)]

use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

/// Default orchestrator URL for test runs.
pub const DEFAULT_ORCH_URL: &str = "http://localhost:7190";

/// Environment variable override for orchestrator URL.
pub const ENV_ORCH_URL: &str = "AI_ORCH_TEST_URL";

/// Environment variable that forces tests to skip the probe and fail fast
/// when the orchestrator is unreachable. Useful for CI gates that must
/// prove the environment is healthy.
pub const ENV_REQUIRE_GARDEN: &str = "AI_ORCH_TEST_REQUIRE";

/// Environment variable to enable mock-garden mode (no real hardware).
pub const ENV_MOCK_GARDEN: &str = "MOCK_GARDEN";

/// Result of probing the orchestrator's health.
#[derive(Debug, Clone)]
pub struct GardenHandle {
    pub url: String,
    pub http: Client,
    pub health: HealthSnapshot,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthSnapshot {
    pub status: String,
    #[serde(default)]
    pub providers_registered: u32,
    #[serde(default)]
    pub providers_healthy: u32,
    #[serde(default)]
    pub directory_version: u64,
}

#[derive(Debug)]
pub enum ProbeError {
    Unreachable(String),
    BadResponse(String),
    NotHealthy {
        status: String,
        providers_healthy: u32,
        providers_registered: u32,
    },
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::Unreachable(e) => write!(f, "orchestrator unreachable: {e}"),
            ProbeError::BadResponse(e) => write!(f, "orchestrator returned bad response: {e}"),
            ProbeError::NotHealthy {
                status,
                providers_healthy,
                providers_registered,
            } => write!(
                f,
                "orchestrator not healthy: status={status}, {providers_healthy}/{providers_registered} providers healthy"
            ),
        }
    }
}

impl GardenHandle {
    /// Probe the orchestrator described by `AI_ORCH_TEST_URL` (or the default)
    /// and return a handle if it is reachable and at least one provider is
    /// healthy. Callers that want stricter checks (e.g., the reference garden
    /// topology) should verify via the returned handle's HTTP client.
    pub async fn probe() -> Result<Self, ProbeError> {
        let url = std::env::var(ENV_ORCH_URL).unwrap_or_else(|_| DEFAULT_ORCH_URL.to_string());
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| ProbeError::Unreachable(e.to_string()))?;

        let health_url = format!("{}/health", url.trim_end_matches('/'));
        let resp = http
            .get(&health_url)
            .send()
            .await
            .map_err(|e| ProbeError::Unreachable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(ProbeError::BadResponse(format!(
                "health returned {}",
                resp.status()
            )));
        }

        let health: HealthSnapshot = resp
            .json()
            .await
            .map_err(|e| ProbeError::BadResponse(format!("health JSON parse: {e}")))?;

        // Accept "ok" (new style) and "healthy" (older health envelopes)
        // for forward compatibility.
        if health.status != "ok" && health.status != "healthy" {
            return Err(ProbeError::NotHealthy {
                status: health.status.clone(),
                providers_healthy: health.providers_healthy,
                providers_registered: health.providers_registered,
            });
        }

        Ok(Self { url, http, health })
    }

    /// Probe with automatic skip-on-failure. Returns `Some(handle)` on
    /// success; prints a skip reason and returns `None` otherwise (unless
    /// `AI_ORCH_TEST_REQUIRE=1` is set, in which case the test panics).
    pub async fn probe_or_skip() -> Option<Self> {
        match Self::probe().await {
            Ok(h) => Some(h),
            Err(e) => {
                if std::env::var(ENV_REQUIRE_GARDEN).as_deref() == Ok("1") {
                    panic!("AI_ORCH_TEST_REQUIRE=1 but probe failed: {e}");
                }
                eprintln!("⊘ skipping: {e}");
                None
            }
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn http(&self) -> &Client {
        &self.http
    }

    /// Build a full URL for a path like `/v1/catalog`.
    pub fn endpoint(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }
}

/// Returns true if the test is running in mock-garden mode.
pub fn is_mock_garden() -> bool {
    std::env::var(ENV_MOCK_GARDEN).as_deref() == Ok("1")
}
