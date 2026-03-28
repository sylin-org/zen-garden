//! OpenedAI Speech offering adapter — text-to-speech.
//!
//! Implements the [`Offering`](crate::catalog::Offering) trait for OpenedAI
//! Speech instances. Provides OpenAI-compatible `POST /v1/audio/speech` API.
//!
//! API reference: `sw/ai/openedai-speech.research.md`

use std::pin::Pin;
use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::{Stream, StreamExt};

use crate::catalog::{
    BenchmarkSample, BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest,
    ProxyResponse, ServiceModel, SyncProgress,
};
use crate::domain::types::{Capability, OfferingKind, Sample, ServiceInstance};

const TIMEOUT: Duration = Duration::from_secs(10);

/// OpenedAI Speech offering adapter.
pub struct OpenedaiSpeechOffering {
    http: reqwest::Client,
}

impl OpenedaiSpeechOffering {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .build()
                .expect("HTTP client"),
        }
    }
}

impl Default for OpenedaiSpeechOffering {
    fn default() -> Self {
        Self::new()
    }
}

impl Offering for OpenedaiSpeechOffering {

    fn offering_type(&self) -> OfferingKind {
        OfferingKind::OpenedaiSpeech
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::Speak]
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "openedai-speech".to_string(),
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
            if !resp.status().is_success() {
                anyhow::bail!("health check returned {}", resp.status());
            }
            // {"status": "ok"} or {"status": "unk"}
            let body: serde_json::Value = resp.json().await.context("parse health")?;
            let status = body
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            if status != "ok" {
                anyhow::bail!("health status: {status}");
            }

            Ok(ProbeResult {
                version: None,
                capabilities: self.capabilities().to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::Value::Null,
            })
        })
    }

    fn enumerate(&self, endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let url = format!("{endpoint}/v1/models");
        Box::pin(async move {
            let resp = self
                .http
                .get(&url)
                .timeout(TIMEOUT)
                .send()
                .await
                .context("GET /v1/models")?;
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
                                capabilities: vec![Capability::Speak],
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

    fn vram_estimate(&self, model: &ServiceModel) -> Option<u64> {
        // XTTS v2 (tts-1-hd) ~4GB, Piper (tts-1) ~0 (CPU)
        match model.name.as_str() {
            "tts-1-hd" => Some(4 * 1024 * 1024 * 1024),
            _ => None, // Piper is CPU-only
        }
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
                .context("proxy to openedai-speech")?;

            let status = resp.status().as_u16();
            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
                .collect();

            // TTS responses are streaming audio — forward as stream.
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
            let phrases = [
                "Hello, welcome to Zen Garden.",
                "The weather today is sunny with a light breeze.",
                "Please check your email for the latest updates.",
            ];

            let mut samples = Vec::with_capacity(phrases.len());
            for (i, phrase) in phrases.iter().enumerate() {
                let start = std::time::Instant::now();
                let result = self
                    .http
                    .post(format!("{endpoint}/v1/audio/speech"))
                    .json(&serde_json::json!({
                        "model": &model,
                        "input": phrase,
                        "voice": "alloy",
                        "response_format": "wav",
                    }))
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await;

                match result {
                    Ok(resp) if resp.status().is_success() => {
                        // Consume the stream to measure total duration.
                        let _bytes = resp.bytes().await;
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
        // Voices are pre-installed. No runtime sync mechanism.
        Box::pin(async { Ok(SyncProgress::Completed { bytes_transferred: 0 }) })
    }
}
