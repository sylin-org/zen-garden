//! ComfyUI provider — workflow-based image generation and processing.
//!
//! Implements the `Provider` trait for ComfyUI instances:
//! - Probe via `GET /system_stats` (version, device info, VRAM)
//! - Enumerate via `GET /models/{type}` (installed model inventory)
//! - Workflow execution via `POST /prompt` + WebSocket progress tracking
//!
//! ComfyUI API reference: `docs/reference/comfyui-api.md`

use anyhow::{Context, Result};
use reqwest::Client;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::traits::{
    BoxFuture, DiscoveryConfig, FormSchema, ProbeResult, Provider, ProviderContext, ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const ENUMERATE_TIMEOUT: Duration = Duration::from_secs(10);

const COMFYUI_CAPABILITIES: &[Capability] = &[Capability::Image];

// ── Provider ───────────────────────────────────────────────────

pub struct ComfyUiProvider {
    http: Client,
}

impl ComfyUiProvider {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client");
        Self { http }
    }
}

impl Default for ComfyUiProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for ComfyUiProvider {
    fn kind(&self) -> OfferingKind {
        OfferingKind::ComfyUi
    }

    fn capabilities(&self) -> &[Capability] {
        COMFYUI_CAPABILITIES
    }

    fn discovery(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "comfyui".into(),
        }
    }

    // ── Lifecycle ───────────────────────────────────────────────

    fn probe(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<ProbeResult>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let resp = self
                .http
                .get(format!("{endpoint}/system_stats"))
                .timeout(PROBE_TIMEOUT)
                .send()
                .await
                .context("probe comfyui /system_stats")?;

            if !resp.status().is_success() {
                anyhow::bail!("comfyui health check failed: HTTP {}", resp.status());
            }

            let stats: SystemStats = resp.json().await.context("parse system_stats")?;

            // Extract VRAM from first CUDA/GPU device
            let vram_free = stats
                .devices
                .iter()
                .find(|d| d.device_type == "cuda" || d.device_type == "mps")
                .map(|d| d.vram_free as u64);

            Ok(ProbeResult {
                version: Some(stats.system.comfyui_version),
                capabilities: COMFYUI_CAPABILITIES.to_vec(),
                vram_free_bytes: vram_free,
                metadata: serde_json::json!({
                    "provider": "comfyui",
                    "pytorch_version": stats.system.pytorch_version,
                    "os": stats.system.os,
                    "devices": stats.devices.iter().map(|d| serde_json::json!({
                        "name": d.name,
                        "type": d.device_type,
                        "vram_total": d.vram_total,
                        "vram_free": d.vram_free,
                    })).collect::<Vec<_>>(),
                }),
            })
        })
    }

    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            // Query all model categories in parallel
            let (checkpoints, upscale_models, loras, vae) = tokio::join!(
                list_models(&self.http, &endpoint, "checkpoints"),
                list_models(&self.http, &endpoint, "upscale_models"),
                list_models(&self.http, &endpoint, "loras"),
                list_models(&self.http, &endpoint, "vae"),
            );

            let checkpoints = checkpoints.unwrap_or_default();
            let upscale_models = upscale_models.unwrap_or_default();
            let loras = loras.unwrap_or_default();
            let vae = vae.unwrap_or_default();

            // Build a single ServiceModel representing this ComfyUI instance.
            // Individual models are tracked in metadata — the instance itself
            // is the routable unit, not individual checkpoint files.
            let mut capabilities = vec![Capability::Image];
            let mut specializations = Vec::new();

            if !upscale_models.is_empty() {
                specializations.push("upscale".to_string());
            }
            if !checkpoints.is_empty() {
                specializations.push("generate".to_string());
            }

            // Compute total model inventory for metadata
            let model_count =
                checkpoints.len() + upscale_models.len() + loras.len() + vae.len();

            Ok(vec![ServiceModel {
                name: "comfyui".to_string(),
                capabilities,
                specializations,
                vram_bytes: None, // VRAM is per-instance, reported in probe
                metadata: serde_json::json!({
                    "provider": "comfyui",
                    "model_count": model_count,
                    "checkpoints": checkpoints,
                    "upscale_models": upscale_models,
                    "loras": loras,
                    "vae": vae,
                }),
            }])
        })
    }

    // ── Form Schema (ORCH-0017) ──────────────────────────────────

    fn form_schema(&self, _model: &str, capability: Capability) -> FormSchema {
        match capability {
            Capability::Image => FormSchema {
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "message": {
                            "type": "string",
                            "title": "Prompt",
                            "description": "Describe the image to generate"
                        }
                    },
                    "required": ["message"]
                }),
                ui_schema: serde_json::json!({
                    "message": {
                        "ui:widget": "textarea",
                        "ui:options": { "rows": 3 }
                    }
                }),
            },
            _ => FormSchema::default(),
        }
    }
}

// ── ComfyUI API Types ──────────────────────────────────────────

/// Response from `GET /system_stats`.
#[derive(Debug, serde::Deserialize)]
struct SystemStats {
    system: SystemInfo,
    devices: Vec<DeviceInfo>,
}

#[derive(Debug, serde::Deserialize)]
struct SystemInfo {
    #[serde(default)]
    os: String,
    #[serde(default)]
    comfyui_version: String,
    #[serde(default)]
    pytorch_version: String,
}

#[derive(Debug, serde::Deserialize)]
struct DeviceInfo {
    name: String,
    #[serde(rename = "type")]
    device_type: String,
    #[serde(default)]
    vram_total: i64,
    #[serde(default)]
    vram_free: i64,
}

// ── Helpers ────────────────────────────────────────────────────

/// Query `GET /models/{model_type}` and return the list of filenames.
async fn list_models(http: &Client, endpoint: &str, model_type: &str) -> Result<Vec<String>> {
    let resp = http
        .get(format!("{endpoint}/models/{model_type}"))
        .timeout(ENUMERATE_TIMEOUT)
        .send()
        .await
        .with_context(|| format!("list comfyui models/{model_type}"))?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "comfyui models/{} failed: HTTP {}",
            model_type,
            resp.status()
        );
    }

    resp.json()
        .await
        .with_context(|| format!("parse comfyui models/{model_type}"))
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_and_capabilities() {
        let p = ComfyUiProvider::new();
        assert_eq!(p.kind(), OfferingKind::ComfyUi);
        assert_eq!(p.capabilities(), &[Capability::Image]);
    }

    #[test]
    fn discovery_returns_topology_filter() {
        let p = ComfyUiProvider::new();
        match p.discovery() {
            DiscoveryConfig::TopologyFilter { offering_name } => {
                assert_eq!(offering_name, "comfyui");
            }
            _ => panic!("expected TopologyFilter"),
        }
    }

    #[test]
    fn parse_system_stats() {
        let json = serde_json::json!({
            "system": {
                "os": "linux",
                "ram_total": 16292962304_i64,
                "ram_free": 14670872576_i64,
                "comfyui_version": "0.18.2",
                "pytorch_version": "2.11.0+cu130",
                "embedded_python": false,
                "argv": []
            },
            "devices": [{
                "name": "cuda:0 NVIDIA GeForce RTX 3060 Ti",
                "type": "cuda",
                "index": 0,
                "vram_total": 8589410304_i64,
                "vram_free": 7472152576_i64,
                "torch_vram_total": 0,
                "torch_vram_free": 0
            }]
        });

        let stats: SystemStats = serde_json::from_value(json).unwrap();
        assert_eq!(stats.system.comfyui_version, "0.18.2");
        assert_eq!(stats.devices.len(), 1);
        assert_eq!(stats.devices[0].device_type, "cuda");
        assert_eq!(stats.devices[0].vram_total, 8589410304);
    }

    #[test]
    fn parse_empty_model_list() {
        let json: Vec<String> = serde_json::from_str("[]").unwrap();
        assert!(json.is_empty());
    }

    #[test]
    fn parse_model_list() {
        let json: Vec<String> =
            serde_json::from_str(r#"["4x-UltraSharp.pth", "RealESRGAN_x4plus.pth"]"#).unwrap();
        assert_eq!(json.len(), 2);
        assert_eq!(json[0], "4x-UltraSharp.pth");
    }
}
