//! Ollama offering adapter.
//!
//! Implements the [`Offering`](crate::catalog::Offering) trait for Ollama
//! instances. Encapsulates all Ollama-specific protocol knowledge: HTTP
//! client for `/api/tags`, `/api/ps`, `/api/show`, NDJSON streaming,
//! model pull protocol, and benchmark payloads.

pub mod client;
pub mod types;

use std::pin::Pin;

use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::{Stream, StreamExt};

use crate::catalog::{
    BoxFuture, Offering,
    BenchmarkSample, DiscoveryConfig, ProbeResult, ProxyBody, ProxyRequest, ProxyResponse,
    ServiceModel, SyncProgress,
};
use crate::domain::types::{Capability, LoadedModel, OfferingKind, Sample, ServiceInstance};

use client::OllamaClient;

/// Ollama offering adapter — routes LLM inference through Ollama instances.
pub struct OllamaOffering {
    client: OllamaClient,
}

impl OllamaOffering {
    pub fn new() -> Self {
        Self {
            client: OllamaClient::new(),
        }
    }

    /// Get a reference to the underlying HTTP client (for tasks that need
    /// direct Ollama API access beyond the trait surface).
    pub fn client(&self) -> &OllamaClient {
        &self.client
    }
}

impl Default for OllamaOffering {
    fn default() -> Self {
        Self::new()
    }
}

impl Offering for OllamaOffering {

    fn offering_type(&self) -> OfferingKind {
        OfferingKind::Ollama
    }

    fn capabilities(&self) -> &[Capability] {
        &[
            Capability::Chat,
            Capability::Generate,
            Capability::Embed,
            Capability::Vision,
            Capability::Tools,
            Capability::Think,
        ]
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "ollama".to_string(),
        }
    }

    fn probe(&self, endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>> {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            let tags = self.client.get_tags(&endpoint).await?;
            let version = self.client.get_version(&endpoint).await.ok();

            Ok(ProbeResult {
                version: version.map(|v| v.version),
                capabilities: self.capabilities().to_vec(),
                vram_free_bytes: None, // Ollama doesn't report free VRAM via probe.
                metadata: serde_json::json!({
                    "model_count": tags.models.len(),
                }),
            })
        })
    }

    fn enumerate(&self, endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            let (available, loaded, infos, _version) =
                self.client.full_profile(&endpoint).await?;

            let loaded_map: std::collections::HashMap<&str, &LoadedModel> = loaded
                .iter()
                .map(|l| (l.name.as_str(), l))
                .collect();

            let models = infos
                .into_iter()
                .map(|info| {
                    let caps = infer_capabilities(&info.capabilities);
                    let vram = loaded_map
                        .get(info.name.as_str())
                        .map(|l| l.vram_bytes)
                        .or(info.vram_bytes);
                    let is_loaded = loaded_map.contains_key(info.name.as_str());
                    ServiceModel {
                        name: info.name,
                        capabilities: caps,
                        vram_bytes: vram,
                        is_loaded,
                        metadata: serde_json::json!({
                            "parameter_count": info.parameter_count,
                            "parameter_size": info.parameter_size,
                            "quantization_level": info.quantization_level,
                            "family": info.family,
                            "context_length": info.context_length,
                            "format": info.format,
                        }),
                    }
                })
                .collect();

            let _ = available; // used indirectly via tags → infos
            Ok(models)
        })
    }

    fn vram_estimate(&self, model: &ServiceModel) -> Option<u64> {
        model.vram_bytes
    }

    fn proxy(
        &self,
        endpoint: &str,
        _capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>> {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            let resp = self
                .client
                .forward_request(
                    &endpoint,
                    &request.path,
                    request.method,
                    request.body,
                    request.headers,
                )
                .await?;

            let status = resp.status().as_u16();
            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| {
                    v.to_str()
                        .ok()
                        .map(|v| (k.as_str().to_string(), v.to_string()))
                })
                .collect();

            let stream = resp.bytes_stream();
            let mapped: Pin<Box<dyn Stream<Item = Result<Bytes, anyhow::Error>> + Send>> =
                Box::pin(stream.map(|r| r.map_err(|e| anyhow::anyhow!(e))));

            Ok(ProxyResponse {
                status,
                headers,
                body: ProxyBody::Stream(mapped),
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
            let samples = match capability.fitness_capability() {
                Capability::Generate => {
                    self.benchmark_generate(&endpoint, &model).await?
                }
                Capability::Embed => {
                    self.benchmark_embed(&endpoint, &model).await?
                }
                // Vision, Tools, Think would have their own benchmark methods.
                // For now, fall back to generate benchmarks.
                _ => {
                    self.benchmark_generate(&endpoint, &model).await?
                }
            };
            Ok(BenchmarkSample {
                model,
                capability,
                samples,
            })
        })
    }

    fn sync_resource(
        &self,
        resource: &str,
        _from: &ServiceInstance,
        to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<SyncProgress>> {
        let model = resource.to_string();
        let target = to.endpoint.clone();
        Box::pin(async move {
            // Ollama uses native POST /api/pull on the target instance.
            let mut stream = self
                .client
                .pull_model(&target, &model)
                .await
                .context("start model pull")?;

            let mut bytes_transferred = 0u64;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.context("pull stream chunk")?;
                bytes_transferred += chunk.len() as u64;
                // Progress could be parsed from NDJSON here and yielded.
                // For now, consume the stream to completion.
            }

            Ok(SyncProgress::Completed { bytes_transferred })
        })
    }
}

// ── Private helpers ─────────────────────────────────────────────

impl OllamaOffering {
    /// Run generate benchmarks (5 prompts).
    async fn benchmark_generate(&self, endpoint: &str, model: &str) -> Result<Vec<Sample>> {
        let prompts = [
            "Explain quantum computing in simple terms",
            "Write a haiku about programming",
            "What is the capital of France?",
            "Describe the water cycle",
            "List three benefits of exercise",
        ];

        let mut samples = Vec::with_capacity(prompts.len());
        for (i, prompt) in prompts.iter().enumerate() {
            let start = std::time::Instant::now();
            match self
                .client
                .benchmark_generate(endpoint, model, prompt, 80)
                .await
            {
                Ok(result) => {
                    let cold_start_ms = result.load_duration / 1_000_000;
                    let tps = if result.eval_duration > 0 {
                        result.eval_count as f64
                            / (result.eval_duration as f64 / 1_000_000_000.0)
                    } else {
                        0.0
                    };
                    samples.push(Sample {
                        prompt_index: i,
                        cold_start_ms,
                        tokens_per_second: Some(tps),
                        total_duration_ms: start.elapsed().as_millis() as u64,
                        valid_ratio: None,
                        error: None,
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
        Ok(samples)
    }

    /// Run embed benchmarks (3 inputs).
    async fn benchmark_embed(&self, endpoint: &str, model: &str) -> Result<Vec<Sample>> {
        let inputs = [
            "The quick brown fox jumps over the lazy dog",
            "Machine learning is a subset of artificial intelligence",
            "Zen gardens are carefully composed arrangements of rocks, water, and plants",
        ];

        let mut samples = Vec::with_capacity(inputs.len());
        for (i, input) in inputs.iter().enumerate() {
            let start = std::time::Instant::now();
            match self.client.benchmark_embed(endpoint, model, input).await {
                Ok(_result) => {
                    samples.push(Sample {
                        prompt_index: i,
                        cold_start_ms: start.elapsed().as_millis() as u64,
                        tokens_per_second: None,
                        total_duration_ms: start.elapsed().as_millis() as u64,
                        valid_ratio: None,
                        error: None,
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
        Ok(samples)
    }
}

/// Infer capabilities from Ollama's capability tags.
fn infer_capabilities(tags: &[String]) -> Vec<Capability> {
    let mut caps = vec![Capability::Chat, Capability::Generate];
    for tag in tags {
        match tag.as_str() {
            "vision" => caps.push(Capability::Vision),
            "tools" => caps.push(Capability::Tools),
            "thinking" => caps.push(Capability::Think),
            "embedding" => {
                caps.clear();
                caps.push(Capability::Embed);
                return caps; // Embedding models don't do chat.
            }
            _ => {}
        }
    }
    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_capabilities_basic() {
        let caps = infer_capabilities(&[]);
        assert!(caps.contains(&Capability::Chat));
        assert!(caps.contains(&Capability::Generate));
    }

    #[test]
    fn infer_capabilities_vision() {
        let caps = infer_capabilities(&["vision".to_string()]);
        assert!(caps.contains(&Capability::Vision));
        assert!(caps.contains(&Capability::Chat));
    }

    #[test]
    fn infer_capabilities_embedding() {
        let caps = infer_capabilities(&["embedding".to_string()]);
        assert_eq!(caps, vec![Capability::Embed]);
        assert!(!caps.contains(&Capability::Chat));
    }

    #[test]
    fn infer_capabilities_tools_thinking() {
        let caps = infer_capabilities(&["tools".to_string(), "thinking".to_string()]);
        assert!(caps.contains(&Capability::Tools));
        assert!(caps.contains(&Capability::Think));
    }
}
