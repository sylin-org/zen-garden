//! LibreTranslate offering adapter — machine translation.
//!
//! Implements the [`Offering`](crate::catalog::Offering) trait for LibreTranslate
//! instances. Custom API (NOT OpenAI-compatible) with `POST /translate`.
//!
//! API reference: `sw/ai/libretranslate.research.md`

use std::time::Duration;

use anyhow::{Context, Result};

use crate::catalog::{
    BenchmarkSample, BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest,
    ProxyResponse, ServiceModel, SyncProgress,
};
use crate::domain::types::{Capability, OfferingKind, Sample, ServiceInstance};

const TIMEOUT: Duration = Duration::from_secs(10);

/// LibreTranslate offering adapter.
pub struct LibreTranslateOffering {
    http: reqwest::Client,
}

impl LibreTranslateOffering {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .build()
                .expect("HTTP client"),
        }
    }
}

impl Default for LibreTranslateOffering {
    fn default() -> Self {
        Self::new()
    }
}

impl Offering for LibreTranslateOffering {

    fn offering_type(&self) -> OfferingKind {
        OfferingKind::LibreTranslate
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::Translate]
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "libretranslate".to_string(),
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
        let url = format!("{endpoint}/languages");
        Box::pin(async move {
            let resp = self
                .http
                .get(&url)
                .timeout(TIMEOUT)
                .send()
                .await
                .context("GET /languages")?;
            let languages: Vec<serde_json::Value> =
                resp.json().await.context("parse languages")?;

            // Each language pair is a "model" in our abstraction.
            let models = languages
                .iter()
                .filter_map(|lang| {
                    let code = lang.get("code")?.as_str()?.to_string();
                    let name = lang.get("name")?.as_str()?.to_string();
                    Some(ServiceModel {
                        name: code.clone(),
                        capabilities: vec![Capability::Translate],
                        vram_bytes: None,
                        is_loaded: false,
                        metadata: serde_json::json!({
                            "language_name": name,
                            "language_code": code,
                        }),
                    })
                })
                .collect();

            Ok(models)
        })
    }

    fn vram_estimate(&self, _model: &ServiceModel) -> Option<u64> {
        // LibreTranslate is CPU-only (Argos Translate / CTranslate2).
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
                .context("proxy to libretranslate")?;

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
            let test_cases = [
                ("en", "es", "The weather is beautiful today."),
                ("en", "fr", "Artificial intelligence is transforming technology."),
                ("en", "de", "Good morning, how are you?"),
            ];

            let mut samples = Vec::with_capacity(test_cases.len());
            for (i, (source, target, text)) in test_cases.iter().enumerate() {
                let start = std::time::Instant::now();
                let result = self
                    .http
                    .post(format!("{endpoint}/translate"))
                    .json(&serde_json::json!({
                        "q": text,
                        "source": source,
                        "target": target,
                        "format": "text",
                    }))
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await;

                match result {
                    Ok(resp) if resp.status().is_success() => {
                        let _body = resp.text().await;
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
                model: "translate".to_string(),
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
        // Language models auto-download from Argos Translate. No explicit sync.
        Box::pin(async { Ok(SyncProgress::Completed { bytes_transferred: 0 }) })
    }
}
