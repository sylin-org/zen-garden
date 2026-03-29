//! OpenAI-compatible cloud provider adapter.
//!
//! Works for OpenAI, Groq, Together, and any provider that implements
//! the OpenAI `/v1/models` and `/v1/chat/completions` API format.
//! The adapter reads its API key from `CloudProviderConfig` at call time.

use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::TryStreamExt;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

use crate::catalog::{
    BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest, ProxyResponse,
    ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind, ServiceInstance};

use super::types::CloudProviderConfig;

/// Timeout for cloud API probe/enumerate calls.
const CLOUD_TIMEOUT: Duration = Duration::from_secs(15);

/// OpenAI-compatible provider adapter.
pub struct OpenAiProvider {
    config: CloudProviderConfig,
    client: Client,
}

impl OpenAiProvider {
    pub fn new(config: CloudProviderConfig) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client build");
        Self { config, client }
    }

    /// The base URL for this provider.
    fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// The API key for this provider.
    fn api_key(&self) -> &str {
        &self.config.api_key
    }
}

/// OpenAI `/v1/models` response shape.
#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
    #[serde(default)]
    owned_by: Option<String>,
}

const OPENAI_CAPABILITIES: &[Capability] = &[
    Capability::Chat,
    Capability::Embed,
    Capability::Vision,
    Capability::Tools,
    Capability::Think,
    Capability::Imagine,
    Capability::Speak,
    Capability::Transcribe,
];

impl Offering for OpenAiProvider {
    fn offering_type(&self) -> OfferingKind {
        self.config.kind
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn capabilities(&self) -> &[Capability] {
        if self.config.capabilities.is_empty() {
            OPENAI_CAPABILITIES
        } else {
            &self.config.capabilities
        }
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::Configured
    }

    fn probe(&self, _endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>> {
        // For cloud providers, the "endpoint" is the base URL from config.
        let url = format!("{}/v1/models", self.base_url());
        let api_key = self.api_key().to_string();

        Box::pin(async move {
            if api_key.is_empty() {
                anyhow::bail!("no API key configured for OpenAI provider");
            }

            let resp = self
                .client
                .get(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .timeout(CLOUD_TIMEOUT)
                .send()
                .await
                .context("probe OpenAI /v1/models")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let summary = if body.len() > 256 {
                    format!("{}...", &body[..256])
                } else {
                    body
                };
                anyhow::bail!("OpenAI probe failed: HTTP {status}: {summary}");
            }

            Ok(ProbeResult {
                version: None,
                capabilities: self.capabilities().to_vec(),
                vram_free_bytes: None, // cloud — not applicable
                metadata: serde_json::json!({
                    "provider": self.config.name,
                    "base_url": self.config.base_url,
                }),
            })
        })
    }

    fn enumerate(&self, _endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let url = format!("{}/v1/models", self.base_url());
        let api_key = self.api_key().to_string();
        let model_filter = self.config.models.clone();

        Box::pin(async move {
            if api_key.is_empty() {
                return Ok(Vec::new());
            }

            let resp = self
                .client
                .get(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .timeout(CLOUD_TIMEOUT)
                .send()
                .await
                .context("enumerate OpenAI /v1/models")?;

            if !resp.status().is_success() {
                anyhow::bail!("enumerate failed: HTTP {}", resp.status());
            }

            let models_resp: ModelsResponse =
                resp.json().await.context("parse /v1/models response")?;

            let models = models_resp
                .data
                .into_iter()
                .filter(|m| {
                    model_filter.is_empty() || model_filter.iter().any(|f| m.id.contains(f))
                })
                .map(|m| ServiceModel {
                    name: m.id.clone(),
                    capabilities: vec![Capability::Chat], // conservative default
                    specializations: vec![],
                    vram_bytes: None,
                    metadata: serde_json::json!({
                        "owned_by": m.owned_by,
                        "cloud": true,
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
                anyhow::bail!("no API key configured for OpenAI provider");
            }

            let body_bytes = match request.body {
                ProxyBody::Complete(bytes) => Bytes::from(bytes),
                ProxyBody::Stream(_) => {
                    anyhow::bail!("streaming request bodies not supported for cloud proxy");
                }
            };

            let url = format!("{base_url}{}", request.path);
            let method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
                .unwrap_or(reqwest::Method::POST);

            let mut builder = self.client.request(method, &url).body(body_bytes);

            // Set authorization header (overrides any client-supplied auth)
            builder = builder.header("Authorization", format!("Bearer {api_key}"));

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

            let resp = builder.send().await.context("proxy forward to OpenAI")?;
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
