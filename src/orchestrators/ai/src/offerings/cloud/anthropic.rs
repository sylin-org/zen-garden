//! Anthropic cloud provider adapter.
//!
//! Implements the `Offering` trait for Claude models via the Anthropic
//! Messages API. Unlike OpenAI-compatible providers, Anthropic uses a
//! different auth scheme (`x-api-key` header) and does not have a
//! `/v1/models` listing endpoint, so models are hardcoded.
//!
//! Request/response translation (OpenAI <-> Anthropic format) is planned
//! but not yet implemented — the proxy currently forwards raw requests.

use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::TryStreamExt;
use reqwest::Client;
use std::time::Duration;

use crate::catalog::{
    BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest, ProxyResponse,
    ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind, ServiceInstance};

use super::types::CloudProviderConfig;

/// Timeout for Anthropic API calls.
const CLOUD_TIMEOUT: Duration = Duration::from_secs(15);

/// Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Hardcoded model list — Anthropic does not expose a model listing endpoint.
const ANTHROPIC_MODELS: &[&str] = &[
    "claude-sonnet-4-20250514",
    "claude-haiku-4-20250514",
    "claude-opus-4-20250514",
    "claude-3-5-sonnet-20241022",
    "claude-3-5-haiku-20241022",
    "claude-3-opus-20240229",
];

const ANTHROPIC_CAPABILITIES: &[Capability] = &[
    Capability::Chat,
    Capability::Vision,
    Capability::Tools,
    Capability::Think,
];

/// Anthropic cloud provider adapter.
pub struct AnthropicProvider {
    config: CloudProviderConfig,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(config: CloudProviderConfig) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client build");
        Self { config, client }
    }

    fn base_url(&self) -> &str {
        &self.config.base_url
    }

    fn api_key(&self) -> &str {
        &self.config.api_key
    }
}

impl Offering for AnthropicProvider {
    fn offering_type(&self) -> OfferingKind {
        OfferingKind::Anthropic
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn capabilities(&self) -> &[Capability] {
        if self.config.capabilities.is_empty() {
            ANTHROPIC_CAPABILITIES
        } else {
            &self.config.capabilities
        }
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::Configured
    }

    fn probe(&self, _endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>> {
        let url = format!("{}/v1/messages", self.base_url());
        let api_key = self.api_key().to_string();

        Box::pin(async move {
            if api_key.is_empty() {
                anyhow::bail!("no API key configured for Anthropic provider");
            }

            // Minimal probe: send a tiny request to verify the key works.
            // A short max_tokens=1 request is cheap and confirms auth.
            let probe_body = serde_json::json!({
                "model": "claude-sonnet-4-20250514",
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "hi"}]
            });

            let resp = self
                .client
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&probe_body)
                .timeout(CLOUD_TIMEOUT)
                .send()
                .await
                .context("probe Anthropic /v1/messages")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let summary = if body.len() > 256 {
                    format!("{}...", &body[..256])
                } else {
                    body
                };
                anyhow::bail!("Anthropic probe failed: HTTP {status}: {summary}");
            }

            Ok(ProbeResult {
                version: None,
                capabilities: self.capabilities().to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({
                    "provider": "anthropic",
                    "base_url": self.config.base_url,
                }),
            })
        })
    }

    fn enumerate(&self, _endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let model_filter = self.config.models.clone();

        Box::pin(async move {
            if self.api_key().is_empty() {
                return Ok(Vec::new());
            }

            // Anthropic has no /models endpoint — return hardcoded list.
            let models = ANTHROPIC_MODELS
                .iter()
                .filter(|m| {
                    model_filter.is_empty() || model_filter.iter().any(|f| m.contains(f.as_str()))
                })
                .map(|&name| ServiceModel {
                    name: name.to_string(),
                    capabilities: ANTHROPIC_CAPABILITIES.to_vec(),
                    vram_bytes: None,
                    metadata: serde_json::json!({
                        "cloud": true,
                        "provider": "anthropic",
                    }),
                })
                .collect();

            Ok(models)
        })
    }

    fn vram_estimate(&self, _model: &ServiceModel) -> Option<u64> {
        None // cloud — VRAM not applicable
    }

    fn proxy(
        &self,
        _endpoint: &str,
        _capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>> {
        let base_url = self.base_url().to_string();
        let api_key = self.api_key().to_string();

        Box::pin(async move {
            if api_key.is_empty() {
                anyhow::bail!("no API key configured for Anthropic provider");
            }

            let body_bytes = match request.body {
                ProxyBody::Complete(bytes) => Bytes::from(bytes),
                ProxyBody::Stream(_) => {
                    anyhow::bail!("streaming request bodies not supported for cloud proxy");
                }
            };

            // For now, forward the request as-is to the Anthropic API.
            // TODO: Add OpenAI -> Anthropic request format translation.
            let url = format!("{base_url}{}", request.path);
            let method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
                .unwrap_or(reqwest::Method::POST);

            let mut builder = self.client.request(method, &url).body(body_bytes);

            // Anthropic auth headers
            builder = builder
                .header("x-api-key", &api_key)
                .header("anthropic-version", ANTHROPIC_VERSION);

            // Forward safe headers
            for (key, value) in request.headers.iter() {
                let name = key.as_str();
                if matches!(name, "content-type" | "accept")
                    && let (Ok(n), Ok(v)) = (
                        reqwest::header::HeaderName::from_bytes(key.as_str().as_bytes()),
                        reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
                    )
                {
                    builder = builder.header(n, v);
                }
            }

            let resp = builder.send().await.context("proxy forward to Anthropic")?;
            let status = resp.status().as_u16();

            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| {
                    v.to_str()
                        .ok()
                        .map(|val| (k.as_str().to_string(), val.to_string()))
                })
                .collect();

            // Detect streaming responses (SSE)
            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            let is_streaming = content_type.contains("text/event-stream");

            let body = if is_streaming {
                let stream = resp
                    .bytes_stream()
                    .map_err(|e| anyhow::anyhow!("stream error: {e}"));
                ProxyBody::Stream(Box::pin(stream))
            } else {
                let bytes = resp.bytes().await.context("read response body")?;
                ProxyBody::Complete(bytes.to_vec())
            };

            Ok(ProxyResponse {
                status,
                headers,
                body,
            })
        })
    }

    fn sync_resource(
        &self,
        _resource: &str,
        _from: &ServiceInstance,
        _to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<crate::catalog::SyncProgress>> {
        Box::pin(async {
            Ok(crate::catalog::SyncProgress::Failed {
                reason: "cloud providers do not support resource sync".to_string(),
            })
        })
    }
}
