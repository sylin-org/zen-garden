//! whisper.cpp offering adapter — speech-to-text.
//!
//! Implements the [`Offering`](crate::catalog::Offering) trait for whisper.cpp
//! server instances. whisper.cpp uses a custom `POST /inference` multipart
//! endpoint (NOT OpenAI-compatible). The adapter translates between the
//! orchestrator's unified proxy format and whisper.cpp's native API.
//!
//! API reference: `sw/ai/whispercpp.research.md`

use std::time::Duration;

use anyhow::{Context, Result};

use crate::catalog::{
    BenchmarkSample, BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest,
    ProxyResponse, ServiceModel, SyncProgress,
};
use crate::domain::types::{Capability, OfferingKind, Sample, ServiceInstance};

const TIMEOUT: Duration = Duration::from_secs(10);

/// whisper.cpp offering adapter.
pub struct WhisperCppOffering {
    http: reqwest::Client,
}

impl WhisperCppOffering {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .build()
                .expect("HTTP client"),
        }
    }
}

impl Default for WhisperCppOffering {
    fn default() -> Self {
        Self::new()
    }
}

impl Offering for WhisperCppOffering {

    fn offering_type(&self) -> OfferingKind {
        OfferingKind::WhisperCpp
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::Transcribe]
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "whispercpp".to_string(),
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
                anyhow::bail!("health check returned {} (model may be loading)", resp.status());
            }
            // {"status":"ok"} when ready, 503 + {"status":"loading model"} while loading.
            let body: serde_json::Value = resp.json().await.context("parse health")?;
            let status = body
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            if status != "ok" {
                anyhow::bail!("whisper.cpp status: {status}");
            }

            Ok(ProbeResult {
                version: None,
                capabilities: self.capabilities().to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::Value::Null,
            })
        })
    }

    fn enumerate(&self, _endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        // whisper.cpp runs a single model specified at startup. No enumeration API.
        // Return a placeholder model entry so the routing table has something to match.
        Box::pin(async {
            Ok(vec![ServiceModel {
                name: "whisper".to_string(),
                capabilities: vec![Capability::Transcribe],
                vram_bytes: None,
                        is_loaded: false,
                metadata: serde_json::json!({
                    "note": "whisper.cpp loads one model at startup; model identity not queryable via API"
                }),
            }])
        })
    }

    fn vram_estimate(&self, _model: &ServiceModel) -> Option<u64> {
        // Pre-built binaries are CPU-only. GPU builds vary.
        None
    }

    fn proxy(
        &self,
        endpoint: &str,
        _capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>> {
        // whisper.cpp uses POST /inference with multipart/form-data.
        // The proxy forwards the request as-is — the client must send
        // multipart data with a `file` field.
        let url = format!("{endpoint}/inference");
        Box::pin(async move {
            let resp = self
                .http
                .post(&url)
                .headers(request.headers)
                .body(request.body)
                .send()
                .await
                .context("proxy to whisper.cpp")?;

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
        _model: &str,
        capability: Capability,
    ) -> BoxFuture<'_, Result<BenchmarkSample>> {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            // Generate a short silent WAV for benchmarking (16kHz, mono, 1 second).
            let wav_data = generate_silent_wav(16000, 1);

            let start = std::time::Instant::now();
            let form = reqwest::multipart::Form::new()
                .part(
                    "file",
                    reqwest::multipart::Part::bytes(wav_data)
                        .file_name("benchmark.wav")
                        .mime_str("audio/wav")
                        .unwrap(),
                )
                .text("response_format", "json");

            let result = self
                .http
                .post(format!("{endpoint}/inference"))
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
        // whisper.cpp models are pre-placed. POST /load can switch models
        // but there's no pull mechanism.
        Box::pin(async { Ok(SyncProgress::Completed { bytes_transferred: 0 }) })
    }
}

/// Generate a minimal valid WAV file with silence (for benchmark probing).
///
/// 16-bit PCM, mono, at the specified sample rate and duration.
pub fn generate_silent_wav(sample_rate: u32, duration_secs: u32) -> Vec<u8> {
    let num_samples = sample_rate * duration_secs;
    let data_size = num_samples * 2; // 16-bit = 2 bytes per sample
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(file_size as usize + 8);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // sub-chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    // data sub-chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.resize(wav.len() + data_size as usize, 0); // silence

    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_wav_is_valid() {
        let wav = generate_silent_wav(16000, 1);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        // Total size: 44 header + 32000 data = 32044 bytes
        assert_eq!(wav.len(), 32044);
    }
}
