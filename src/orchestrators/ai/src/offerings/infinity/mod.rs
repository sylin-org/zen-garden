//! Infinity offering adapter — embedding and reranking.
//!
//! Implements the [`Offering`](crate::catalog::Offering) trait for Infinity
//! instances (michaelfeil/infinity). Infinity provides OpenAI-compatible
//! `/embeddings` and `/rerank` endpoints.
//!
//! API reference: `sw/ai/infinity.research.md`

use std::time::Duration;

use anyhow::{Context, Result};

use crate::catalog::{
    BenchmarkSample, BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest,
    ProxyResponse, ServiceModel, SyncProgress,
};
use crate::domain::types::{Capability, OfferingKind, Sample, ServiceInstance};

const TIMEOUT: Duration = Duration::from_secs(10);

/// Infinity offering adapter.
pub struct InfinityOffering {
    http: reqwest::Client,
}

impl InfinityOffering {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .build()
                .expect("HTTP client"),
        }
    }
}

impl Default for InfinityOffering {
    fn default() -> Self {
        Self::new()
    }
}

impl Offering for InfinityOffering {

    fn offering_type(&self) -> OfferingKind {
        OfferingKind::Infinity
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::Embed, Capability::Rerank]
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "infinity".to_string(),
        }
    }

    fn probe(&self, endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>> {
        let url = format!("{endpoint}/health");
        Box::pin(async move {
            let resp = self
                .http
                .get(&url)
                .timeout(TIMEOUT)
                .send()
                .await
                .context("GET /health")?;
            let status = resp.status();
            if !status.is_success() {
                anyhow::bail!("health check returned {status}");
            }
            // Infinity /health returns {"unix": <timestamp>}
            let _body: serde_json::Value = resp.json().await.context("parse health")?;

            Ok(ProbeResult {
                version: None,
                capabilities: self.capabilities().to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::Value::Null,
            })
        })
    }

    fn enumerate(&self, endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let url = format!("{endpoint}/models");
        Box::pin(async move {
            let resp = self
                .http
                .get(&url)
                .timeout(TIMEOUT)
                .send()
                .await
                .context("GET /models")?;
            let body: serde_json::Value = resp.json().await.context("parse models")?;

            let models = body
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            let name = m.get("id")?.as_str()?.to_string();
                            Some(ServiceModel {
                                name,
                                capabilities: self.capabilities().to_vec(),
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
        // Infinity doesn't expose VRAM per model. Typical small models ~500MB.
        None
    }

    fn proxy(
        &self,
        endpoint: &str,
        _capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>> {
        let url = format!("{endpoint}{}", request.path);
        Box::pin(async move {
            let resp = self
                .http
                .request(request.method, &url)
                .headers(request.headers)
                .body(request.body)
                .send()
                .await
                .context("proxy to infinity")?;

            let status = resp.status().as_u16();
            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
                .collect();
            let body = resp.bytes().await.context("read response body")?;

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
        let endpoint = endpoint.to_string();
        let model = model.to_string();
        Box::pin(async move {
            let inputs = [
                "The quick brown fox jumps over the lazy dog",
                "Machine learning enables computers to learn from data",
                "Zen gardens are composed arrangements of rocks and sand",
            ];

            let mut samples = Vec::with_capacity(inputs.len());
            for (i, input) in inputs.iter().enumerate() {
                let start = std::time::Instant::now();
                let result = self
                    .http
                    .post(format!("{endpoint}/embeddings"))
                    .json(&serde_json::json!({
                        "input": input,
                        "model": &model,
                    }))
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await;

                match result {
                    Ok(resp) if resp.status().is_success() => {
                        samples.push(Sample {
                            prompt_index: i,
                            cold_start_ms: start.elapsed().as_millis() as u64,
                            tokens_per_second: None,
                            total_duration_ms: start.elapsed().as_millis() as u64,
                            valid_ratio: None,
                            error: None,
                        });
                    }
                    Ok(resp) => {
                        samples.push(Sample {
                            prompt_index: i,
                            cold_start_ms: 0,
                            tokens_per_second: None,
                            total_duration_ms: start.elapsed().as_millis() as u64,
                            valid_ratio: None,
                            error: Some(format!("HTTP {}", resp.status())),
                        });
                    }
                    Err(e) => {
                        samples.push(Sample {
                            prompt_index: i,
                            cold_start_ms: 0,
                            tokens_per_second: None,
                            total_duration_ms: start.elapsed().as_millis() as u64,
                            valid_ratio: None,
                            error: Some(e.to_string()),
                        });
                    }
                }
            }

            Ok(BenchmarkSample {
                model,
                capability,
                samples,
            })
        })
    }

    fn sync_resource(
        &self,
        _resource: &str,
        _from: &ServiceInstance,
        _to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<SyncProgress>> {
        // Infinity models are specified at startup. No runtime sync mechanism.
        Box::pin(async { Ok(SyncProgress::Completed { bytes_transferred: 0 }) })
    }
}
