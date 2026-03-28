//! ComfyUI offering adapter — image generation.
//!
//! Implements the [`Offering`](crate::catalog::Offering) trait for ComfyUI
//! instances. ComfyUI uses a custom workflow-based API:
//! - Submit workflow via `POST /prompt`
//! - Monitor execution via WebSocket
//! - Retrieve output via `GET /view`
//!
//! This adapter provides health probing, model enumeration, and a proxy
//! that forwards workflow requests. Full workflow template dispatch
//! (parameterized txt2img/img2img/inpaint workflows) is deferred to
//! Phase 3 — the adapter currently forwards raw workflow JSON as-is.
//!
//! API reference: `sw/ai/comfyui.research.md`

use std::time::Duration;

use anyhow::{Context, Result};

use crate::catalog::{
    BenchmarkSample, BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest,
    ProxyResponse, ServiceModel, SyncProgress,
};
use crate::domain::types::{Capability, OfferingKind, Sample, ServiceInstance};

const TIMEOUT: Duration = Duration::from_secs(10);

/// ComfyUI offering adapter.
pub struct ComfyUiOffering {
    http: reqwest::Client,
}

impl ComfyUiOffering {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .build()
                .expect("HTTP client"),
        }
    }
}

impl Default for ComfyUiOffering {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from `GET /system_stats`.
#[derive(Debug, serde::Deserialize)]
struct SystemStats {
    system: SystemInfo,
    #[serde(default)]
    devices: Vec<DeviceInfo>,
}

#[derive(Debug, serde::Deserialize)]
struct SystemInfo {
    #[serde(default)]
    comfyui_version: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct DeviceInfo {
    #[serde(default)]
    name: String,
    #[serde(default)]
    vram_total: u64,
    #[serde(default)]
    vram_free: u64,
}

impl Offering for ComfyUiOffering {
    fn offering_type(&self) -> OfferingKind {
        OfferingKind::ComfyUi
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::Imagine, Capability::Edit, Capability::Render]
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "comfyui".to_string(),
        }
    }

    fn probe(&self, endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>> {
        let url = format!("{endpoint}/system_stats");
        Box::pin(async move {
            let resp = self
                .http
                .get(&url)
                .timeout(TIMEOUT)
                .send()
                .await
                .context("GET /system_stats")?;
            if !resp.status().is_success() {
                anyhow::bail!("system_stats returned {}", resp.status());
            }

            let stats: SystemStats = resp.json().await.context("parse system_stats")?;

            // Extract VRAM from first GPU device.
            let vram_free = stats.devices.first().map(|d| d.vram_free);

            Ok(ProbeResult {
                version: stats.system.comfyui_version,
                capabilities: self.capabilities().to_vec(),
                vram_free_bytes: vram_free,
                metadata: serde_json::json!({
                    "device_count": stats.devices.len(),
                    "devices": stats.devices.iter().map(|d| {
                        serde_json::json!({
                            "name": d.name,
                            "vram_total": d.vram_total,
                            "vram_free": d.vram_free,
                        })
                    }).collect::<Vec<_>>(),
                }),
            })
        })
    }

    fn enumerate(&self, endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let url = format!("{endpoint}/models/checkpoints");
        Box::pin(async move {
            let resp = self
                .http
                .get(&url)
                .timeout(TIMEOUT)
                .send()
                .await
                .context("GET /models/checkpoints")?;
            let filenames: Vec<String> = resp.json().await.context("parse checkpoints")?;

            let models = filenames
                .into_iter()
                .map(|name| {
                    // Strip extension for display: "flux-dev.safetensors" → "flux-dev"
                    let display = name
                        .strip_suffix(".safetensors")
                        .or_else(|| name.strip_suffix(".ckpt"))
                        .unwrap_or(&name)
                        .to_string();
                    ServiceModel {
                        name,
                        capabilities: vec![Capability::Imagine, Capability::Edit],
                        vram_bytes: None, // VRAM depends on checkpoint size
                        metadata: serde_json::json!({"display_name": display}),
                    }
                })
                .collect();

            Ok(models)
        })
    }

    fn vram_estimate(&self, _model: &ServiceModel) -> Option<u64> {
        // ComfyUI VRAM varies wildly by model:
        // SD 1.5 ~4GB, SDXL ~8GB, Flux ~12GB
        // Cannot estimate without model metadata.
        None
    }

    fn proxy(
        &self,
        endpoint: &str,
        _capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>> {
        // Forward requests to ComfyUI as-is. The proxy handler routes
        // /api/imagine, /api/edit, /api/render to this adapter.
        // For now, clients must send raw ComfyUI workflow JSON.
        // Phase 3 will add parameterized workflow templates.
        let url = format!("{endpoint}{}", request.path);
        Box::pin(async move {
            let resp = self
                .http
                .request(request.method, &url)
                .headers(request.headers)
                .body(request.body)
                .send()
                .await
                .context("proxy to comfyui")?;

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
            // ComfyUI benchmark requires submitting a workflow and waiting for
            // execution via WebSocket. This is complex and deferred to Phase 3.
            // For now, just probe system_stats to verify responsiveness.
            let start = std::time::Instant::now();
            let result = self
                .http
                .get(format!("{endpoint}/system_stats"))
                .timeout(Duration::from_secs(5))
                .send()
                .await;

            let sample = match result {
                Ok(resp) if resp.status().is_success() => Sample {
                    prompt_index: 0,
                    cold_start_ms: start.elapsed().as_millis() as u64,
                    tokens_per_second: None,
                    total_duration_ms: start.elapsed().as_millis() as u64,
                    valid_ratio: None,
                    error: None,
                },
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
                model: "comfyui".to_string(),
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
        // ComfyUI checkpoints are large files (2-12GB). Sync via storage
        // bank transport (Phase 3). No native pull mechanism.
        Box::pin(async { Ok(SyncProgress::Completed { bytes_transferred: 0 }) })
    }
}
