//! HTTP client for Infinity API operations.
//!
//! Encapsulates all direct communication with Infinity instances.

use super::types::{HealthResponse, ModelsResponse};
use anyhow::{Context, Result};
use bytes::Bytes;
use reqwest::Client;
use std::time::Duration;

/// Timeout for discovery/profiling queries.
const PROFILE_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum bytes of error body to include in diagnostics.
const ERROR_BODY_MAX: usize = 512;

/// Client for an Infinity instance.
#[derive(Clone)]
pub struct InfinityClient {
    http: Client,
}

impl Default for InfinityClient {
    fn default() -> Self {
        Self::new()
    }
}

impl InfinityClient {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(2)
            .build()
            .expect("HTTP client build");
        Self { http }
    }

    // -- Discovery / Profiling ------------------------------------------

    /// Health check: `GET /health` expecting 200 with `{"unix": ...}`.
    pub async fn health(&self, endpoint: &str) -> Result<HealthResponse> {
        let url = format!("{endpoint}/health");
        let resp = self
            .http
            .get(&url)
            .timeout(PROFILE_TIMEOUT)
            .send()
            .await
            .context("GET /health")?;
        let resp = check_status(resp, "GET /health").await?;
        resp.json().await.context("parse /health")
    }

    /// List loaded models: `GET /models` (OpenAI-compatible format).
    pub async fn models(&self, endpoint: &str) -> Result<ModelsResponse> {
        let url = format!("{endpoint}/models");
        let resp = self
            .http
            .get(&url)
            .timeout(PROFILE_TIMEOUT)
            .send()
            .await
            .context("GET /models")?;
        let resp = check_status(resp, "GET /models").await?;
        resp.json().await.context("parse /models")
    }

    // -- Proxy Forwarding -----------------------------------------------

    /// Forward an arbitrary request and return the raw response.
    pub async fn forward_request(
        &self,
        endpoint: &str,
        path: &str,
        method: reqwest::Method,
        body: Bytes,
        headers: reqwest::header::HeaderMap,
    ) -> Result<reqwest::Response> {
        let url = format!("{endpoint}{path}");
        let mut builder = self.http.request(method, &url).body(body);

        for (key, value) in headers.iter() {
            let name = key.as_str();
            if name == "content-type" || name == "accept" || name == "authorization" {
                builder = builder.header(key, value);
            }
        }

        builder.send().await.context("forward request to Infinity")
    }
}

/// Check response status, preserving the response body on error.
async fn check_status(resp: reqwest::Response, label: &str) -> Result<reqwest::Response> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let body_summary = if body.len() > ERROR_BODY_MAX {
        format!("{}...", &body[..ERROR_BODY_MAX])
    } else {
        body
    };
    tracing::warn!(
        label = %label,
        status = %status,
        body = %body_summary,
        "upstream HTTP error"
    );
    anyhow::bail!("{label} HTTP {status}: {body_summary}")
}
