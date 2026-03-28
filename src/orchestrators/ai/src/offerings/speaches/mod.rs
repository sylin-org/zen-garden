//! Speaches offering adapter — speech-to-text and text-to-speech.
//!
//! Implements the [`Offering`](crate::catalog::Offering) trait for Speaches
//! instances. Speaches provides OpenAI-compatible endpoints for both STT
//! (`POST /v1/audio/transcriptions`) and TTS (`POST /v1/audio/speech`).
//!
//! Note: Speaches is Docker-first. Native installation is non-trivial.
//! This adapter works with both managed containers and (rare) native installs.

use std::time::Duration;

use anyhow::{Context, Result};

use crate::catalog::{
    BenchmarkSample, BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest,
    ProxyResponse, ServiceModel, SyncProgress,
};
use crate::domain::types::{Capability, OfferingKind, Sample, ServiceInstance};

const TIMEOUT: Duration = Duration::from_secs(10);

/// Speaches offering adapter.
pub struct SpeachesOffering {
    http: reqwest::Client,
}

impl SpeachesOffering {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .build()
                .expect("HTTP client"),
        }
    }
}

impl Default for SpeachesOffering {
    fn default() -> Self {
        Self::new()
    }
}

impl Offering for SpeachesOffering {
    fn offering_type(&self) -> OfferingKind {
        OfferingKind::Speaches
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::Transcribe, Capability::Speak]
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "speaches".to_string(),
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

            Ok(ProbeResult {
                version: None,
                capabilities: vec![Capability::Transcribe, Capability::Speak],
                vram_free_bytes: None,
                metadata: serde_json::Value::Null,
            })
        })
    }

    fn enumerate(&self, _endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        // Speaches loads models dynamically per-request (specified in request body).
        // Models are auto-downloaded from HuggingFace on first use.
        // No single enumeration endpoint — return the configured default.
        Box::pin(async {
            Ok(vec![ServiceModel {
                name: "whisper".to_string(),
                capabilities: vec![Capability::Transcribe],
                vram_bytes: None,
                        is_loaded: false,
                metadata: serde_json::json!({
                    "note": "Speaches loads models per-request from HuggingFace"
                }),
            }])
        })
    }

    fn vram_estimate(&self, _model: &ServiceModel) -> Option<u64> {
        None
    }

    fn proxy(
        &self,
        endpoint: &str,
        _capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>> {
        // Speaches is OpenAI-compatible — forward as-is.
        let url = format!("{endpoint}{}", request.path);
        Box::pin(async move {
            let resp = self
                .http
                .request(request.method, &url)
                .headers(request.headers)
                .body(request.body)
                .send()
                .await
                .context("proxy to speaches")?;

            let status = resp.status().as_u16();
            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
                .collect();
            // Speaches TTS responses are streaming audio — forward as stream
            // to avoid buffering entire audio into memory (§20).
            let stream = resp.bytes_stream();
            let mapped: std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<bytes::Bytes, anyhow::Error>> + Send>> =
                Box::pin(futures_util::StreamExt::map(stream, |r| r.map_err(|e| anyhow::anyhow!(e))));

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
        _model: &str,
        capability: Capability,
    ) -> BoxFuture<'_, Result<BenchmarkSample>> {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            // Use a silent WAV file for transcription benchmark.
            let wav_data = crate::offerings::whispercpp::generate_silent_wav(16000, 1);

            let start = std::time::Instant::now();
            let form = reqwest::multipart::Form::new()
                .part(
                    "file",
                    reqwest::multipart::Part::bytes(wav_data)
                        .file_name("benchmark.wav")
                        .mime_str("audio/wav")
                        .unwrap(),
                )
                .text("model", "Systran/faster-whisper-tiny");

            let result = self
                .http
                .post(format!("{endpoint}/v1/audio/transcriptions"))
                .multipart(form)
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
                model: "whisper".to_string(),
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
        // Models auto-download from HuggingFace. No explicit sync.
        Box::pin(async { Ok(SyncProgress::Completed { bytes_transferred: 0 }) })
    }
}
