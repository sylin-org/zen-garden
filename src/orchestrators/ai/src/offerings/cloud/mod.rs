//! Cloud provider offering adapters.
//!
//! Each cloud provider implements the [`Offering`] trait at priority -10.
//! Cloud instances are configured via API keys (environment variables),
//! not auto-discovered. The AI orchestrator's priority gate (RT-4)
//! ensures cloud providers are only selected when no local instance
//! serves the capability.
//!
//! ## Provider/Model Naming Convention
//!
//! Cloud models use `provider/model-name` format (following LiteLLM and
//! OpenRouter conventions):
//! - `openai/gpt-4o`
//! - `anthropic/claude-sonnet-4-20250514`
//! - `google/gemini-2.5-pro`
//!
//! The proxy resolves the provider prefix and dispatches to the correct
//! adapter.
//!
//! ## API Key Management
//!
//! Keys are read from environment variables at startup — never stored in
//! config files. The naming convention is `{PROVIDER}_API_KEY`:
//! - `OPENAI_API_KEY`
//! - `ANTHROPIC_API_KEY`
//! - `GOOGLE_API_KEY`
//!
//! A provider with no API key is silently skipped (not registered).

pub mod openai_compat;

use std::time::Duration;

use anyhow::{Context, Result};

use crate::catalog::{
    BenchmarkSample, BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest,
    ProxyResponse, ServiceModel, SyncProgress,
};
use crate::domain::types::{Capability, OfferingKind, Sample, ServiceInstance};

/// A cloud provider offering — wraps an OpenAI-compatible API endpoint.
///
/// Most cloud providers (OpenAI, Anthropic via proxy, Google, Cohere, etc.)
/// are OpenAI-compatible or have thin translation layers. This adapter
/// handles the common case; provider-specific quirks are in submodules.
pub struct CloudProviderOffering {
    kind: OfferingKind,
    capabilities: Vec<Capability>,
    api_key: String,
    base_url: String,
    http: reqwest::Client,
}

impl CloudProviderOffering {
    /// Create a cloud provider offering from environment.
    ///
    /// Returns `None` if the API key env var is not set (provider skipped).
    pub fn from_env(
        kind: OfferingKind,
        capabilities: Vec<Capability>,
        base_url: &str,
        api_key_env: &str,
    ) -> Option<Self> {
        let api_key = std::env::var(api_key_env).ok()?;
        if api_key.is_empty() {
            return None;
        }
        Some(Self {
            kind,
            capabilities,
            api_key,
            base_url: base_url.to_string(),
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(120))
                .build()
                .expect("HTTP client"),
        })
    }
}

impl Offering for CloudProviderOffering {
    fn offering_type(&self) -> OfferingKind {
        self.kind
    }

    fn capabilities(&self) -> &[Capability] {
        &self.capabilities
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::Configured
    }

    fn probe(&self, _endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>> {
        let url = format!("{}/models", self.base_url);
        let api_key = self.api_key.clone();
        Box::pin(async move {
            let resp = self
                .http
                .get(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .timeout(Duration::from_secs(10))
                .send()
                .await
                .context("cloud provider health check")?;
            if !resp.status().is_success() {
                anyhow::bail!("cloud provider returned {}", resp.status());
            }
            Ok(ProbeResult {
                version: None,
                capabilities: self.capabilities.clone(),
                vram_free_bytes: None,
                metadata: serde_json::Value::Null,
            })
        })
    }

    fn enumerate(&self, _endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let url = format!("{}/models", self.base_url);
        let api_key = self.api_key.clone();
        let caps = self.capabilities.clone();
        Box::pin(async move {
            let resp = self
                .http
                .get(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .timeout(Duration::from_secs(10))
                .send()
                .await
                .context("list cloud models")?;
            let body: serde_json::Value = resp.json().await.context("parse models")?;

            let models = body
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            let id = m.get("id")?.as_str()?.to_string();
                            Some(ServiceModel {
                                name: id,
                                capabilities: caps.clone(),
                                vram_bytes: None,
                                is_loaded: false,
                                metadata: serde_json::Value::Null,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            Ok(models)
        })
    }

    fn vram_estimate(&self, _model: &ServiceModel) -> Option<u64> {
        None // Cloud manages its own resources.
    }

    fn proxy(
        &self,
        _endpoint: &str,
        _capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>> {
        let url = format!("{}{}", self.base_url, request.path);
        let api_key = self.api_key.clone();
        Box::pin(async move {
            let mut builder = self
                .http
                .request(request.method, &url)
                .header("Authorization", format!("Bearer {api_key}"))
                .body(request.body);

            // Forward content-type.
            if let Some(ct) = request.headers.get("content-type") {
                if let Ok(v) = ct.to_str() {
                    builder = builder.header("content-type", v);
                }
            }

            let resp = builder.send().await.context("cloud provider proxy")?;

            let status = resp.status().as_u16();
            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
                .collect();
            let body = resp.bytes().await.context("read cloud response")?;

            Ok(ProxyResponse {
                status,
                headers,
                body: ProxyBody::Complete(body),
            })
        })
    }

    fn benchmark(
        &self,
        endpoint: &str,
        model: &str,
        capability: Capability,
    ) -> BoxFuture<'_, Result<BenchmarkSample>> {
        let _endpoint = endpoint;
        let model = model.to_string();
        let api_key = self.api_key.clone();
        Box::pin(async move {
            // Latency-only benchmark — measure round-trip time.
            let start = std::time::Instant::now();
            let result = self
                .http
                .post(format!("{}/chat/completions", self.base_url))
                .header("Authorization", format!("Bearer {api_key}"))
                .json(&serde_json::json!({
                    "model": &model,
                    "messages": [{"role": "user", "content": "Hi"}],
                    "max_tokens": 10,
                }))
                .timeout(Duration::from_secs(30))
                .send()
                .await;

            let sample = match result {
                Ok(resp) if resp.status().is_success() => {
                    let _body = resp.text().await;
                    Sample {
                        prompt_index: 0,
                        cold_start_ms: start.elapsed().as_millis() as u64,
                        tokens_per_second: None,
                        total_duration_ms: start.elapsed().as_millis() as u64,
                        valid_ratio: None,
                        error: None,
                    }
                }
                Ok(resp) => Sample {
                    prompt_index: 0,
                    cold_start_ms: 0,
                    tokens_per_second: None,
                    total_duration_ms: start.elapsed().as_millis() as u64,
                    valid_ratio: None,
                    error: Some(format!("HTTP {}", resp.status())),
                },
                Err(e) => Sample {
                    prompt_index: 0,
                    cold_start_ms: 0,
                    tokens_per_second: None,
                    total_duration_ms: start.elapsed().as_millis() as u64,
                    valid_ratio: None,
                    error: Some(e.to_string()),
                },
            };

            Ok(BenchmarkSample {
                model,
                capability,
                samples: vec![sample],
            })
        })
    }

    fn sync_resource(
        &self,
        _resource: &str,
        _from: &ServiceInstance,
        _to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<SyncProgress>> {
        Box::pin(async { Ok(SyncProgress::Completed { bytes_transferred: 0 }) })
    }
}
