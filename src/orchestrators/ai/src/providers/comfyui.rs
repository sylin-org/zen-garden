//! ComfyUI provider — workflow-based image generation and processing.
//!
//! Implements the `Provider` trait for ComfyUI instances:
//! - Probe via `GET /system_stats` (version, device info, VRAM)
//! - Enumerate via `GET /models/{type}` (installed model inventory)
//! - Skills via `skills::builtin` (dynamic from installed models)
//! - Workflow execution via `POST /prompt` + polling
//!
//! ComfyUI API reference: `docs/reference/comfyui-api.md`

use anyhow::{Context, Result};
use reqwest::Client;
use std::time::Duration;

use crate::catalog::traits::{
    BoxFuture, DiscoveryConfig, FormSchema, ProbeResult, Provider, ProviderContext, ServiceModel,
};
use crate::domain::skill::{
    ContentType, SkillDefinition, WorkflowJob, WorkflowJobStatus, WorkflowRequest,
};
use crate::domain::types::{Capability, OfferingKind};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const ENUMERATE_TIMEOUT: Duration = Duration::from_secs(10);
const WORKFLOW_TIMEOUT: Duration = Duration::from_secs(300);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

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

            let mut specializations = Vec::new();
            if !upscale_models.is_empty() {
                specializations.push("upscale".to_string());
            }
            if !checkpoints.is_empty() {
                specializations.push("generate".to_string());
            }

            let model_count =
                checkpoints.len() + upscale_models.len() + loras.len() + vae.len();

            Ok(vec![ServiceModel {
                name: "comfyui".to_string(),
                capabilities: vec![Capability::Image],
                specializations,
                vram_bytes: None,
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

    // ── Skills (ORCH-0018) ────────────────────────────────────────

    fn skills(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<SkillDefinition>>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let mut skills = Vec::new();

            let upscale_models = list_models(&self.http, &endpoint, "upscale_models")
                .await
                .unwrap_or_default();

            if !upscale_models.is_empty() {
                skills.push(crate::skills::builtin::image_upscale(&upscale_models));
            }

            Ok(skills)
        })
    }

    fn workflow(
        &self,
        ctx: &ProviderContext,
        req: WorkflowRequest,
    ) -> BoxFuture<'_, Result<WorkflowJob>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let workflow_template = match req.skill.as_str() {
                "image.upscale" => crate::skills::builtin::image_upscale(&[]).implementation,
                other => anyhow::bail!("unknown skill: {}", other),
            };

            execute_workflow(&self.http, &endpoint, &req, workflow_template).await
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
pub(crate) async fn list_models(
    http: &Client,
    endpoint: &str,
    model_type: &str,
) -> Result<Vec<String>> {
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

// ── Workflow Execution ─────────────────────────────────────────

/// Execute a workflow on a ComfyUI instance.
///
/// Pipeline: resolve input → upload image → fill template → submit → poll → extract result.
async fn execute_workflow(
    http: &Client,
    endpoint: &str,
    req: &WorkflowRequest,
    mut workflow: serde_json::Value,
) -> Result<WorkflowJob> {
    // 1. Extract and upload input image
    let image_content = req
        .content
        .iter()
        .find(|c| c.content_type == ContentType::Image)
        .context("workflow requires an image content block")?;

    let image_bytes = resolve_content_bytes(http, image_content).await?;
    let uploaded_name = upload_image(http, endpoint, &image_bytes).await?;

    // 2. Resolve model from parameters or query available
    let model_name = req
        .parameters
        .get("upscale_model")
        .and_then(|v| v.as_str())
        .map(String::from);

    let model_name = match model_name {
        Some(m) => m,
        None => {
            let models = list_models(http, endpoint, "upscale_models").await?;
            models
                .into_iter()
                .next()
                .context("no upscale models installed")?
        }
    };

    // 3. Fill placeholders in workflow template
    fill_placeholder(&mut workflow, "PLACEHOLDER_IMAGE", &uploaded_name);
    fill_placeholder(&mut workflow, "PLACEHOLDER_MODEL", &model_name);

    // 3b. Handle scale factor — the upscale model always produces 4x.
    // For 2x, insert a downscale node between upscale and save.
    let scale = req
        .parameters
        .get("scale")
        .and_then(|v| v.as_u64())
        .unwrap_or(4);

    if scale == 2 {
        // Rewire: upscale → ImageScale (0.5x) → save
        // The save node currently reads from the upscale node.
        // Insert a scale node that halves the 4x result to get 2x.
        let scale_node_id = "5";
        workflow[scale_node_id] = serde_json::json!({
            "class_type": "ImageScale",
            "inputs": {
                "image": ["3", 0],
                "upscale_method": "lanczos",
                "width": 0,
                "height": 0,
                "crop": "disabled",
                "scale_by": 0.5
            }
        });
        // Rewire save to read from the scale node instead of upscale
        workflow["4"]["inputs"]["images"] = serde_json::json!([scale_node_id, 0]);
    }

    // 4. Submit workflow
    let client_id = format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );

    let resp = http
        .post(format!("{endpoint}/prompt"))
        .json(&serde_json::json!({
            "prompt": workflow,
            "client_id": client_id,
        }))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .context("POST comfyui /prompt")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("comfyui /prompt failed HTTP {status}: {text}");
    }

    let prompt_result: serde_json::Value = resp.json().await.context("parse /prompt response")?;
    let prompt_id = prompt_result["prompt_id"]
        .as_str()
        .context("missing prompt_id")?
        .to_string();

    // 5. Poll for completion
    let start = std::time::Instant::now();
    let output_images = loop {
        if start.elapsed() > WORKFLOW_TIMEOUT {
            anyhow::bail!("workflow timed out after {}s", WORKFLOW_TIMEOUT.as_secs());
        }

        tokio::time::sleep(POLL_INTERVAL).await;

        let history = http
            .get(format!("{endpoint}/history/{prompt_id}"))
            .timeout(Duration::from_secs(5))
            .send()
            .await;

        let Ok(resp) = history else { continue };
        if !resp.status().is_success() {
            continue;
        }

        let history_json: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(entry) = history_json.get(&prompt_id) {
            if let Some(status) = entry.get("status") {
                if status.get("status_str").and_then(|s| s.as_str()) == Some("error") {
                    let messages = status
                        .get("messages")
                        .and_then(|m| m.as_array())
                        .map(|arr| format!("{:?}", arr))
                        .unwrap_or_else(|| "unknown error".into());
                    anyhow::bail!("workflow execution failed: {messages}");
                }
            }

            if let Some(outputs) = entry.get("outputs") {
                let images = extract_output_images(outputs);
                if !images.is_empty() {
                    break images;
                }
            }
        }
    };

    // 6. Build result
    let duration_ms = start.elapsed().as_millis() as u64;

    let content: Vec<crate::domain::skill::ContentBlock> = output_images
        .into_iter()
        .map(|img| crate::domain::skill::ContentBlock {
            content_type: ContentType::Image,
            role: None,
            data: None,
            url: Some(format!(
                "{endpoint}/view?filename={}&type=output&subfolder={}",
                img.filename, img.subfolder
            )),
            format: Some("png".into()),
        })
        .collect();

    Ok(WorkflowJob {
        id: prompt_id,
        skill: req.skill.clone(),
        status: WorkflowJobStatus::Completed,
        progress: Some(1.0),
        content: Some(content),
        error: None,
        usage: Some(crate::domain::skill::WorkflowUsage { duration_ms }),
    })
}

/// Resolve a ContentBlock to raw bytes (base64 inline or URL fetch).
async fn resolve_content_bytes(
    http: &Client,
    content: &crate::domain::skill::ContentBlock,
) -> Result<Vec<u8>> {
    if let Some(data) = &content.data {
        let base64_str = if let Some((_prefix, payload)) = data.split_once(";base64,") {
            payload
        } else {
            data.as_str()
        };

        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(base64_str)
            .context("invalid base64 in content data")
    } else if let Some(url) = &content.url {
        let resp = http
            .get(url)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .with_context(|| format!("fetch content URL: {url}"))?;

        if !resp.status().is_success() {
            anyhow::bail!("content URL returned HTTP {}", resp.status());
        }

        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .with_context(|| format!("read body: {url}"))
    } else {
        anyhow::bail!("content block has neither 'data' nor 'url'")
    }
}

/// Upload an image to ComfyUI via `POST /upload/image`.
async fn upload_image(http: &Client, endpoint: &str, image_bytes: &[u8]) -> Result<String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let filename = format!("zen-input-{nanos:x}.png");

    let part = reqwest::multipart::Part::bytes(image_bytes.to_vec())
        .file_name(filename)
        .mime_str("image/png")?;

    let form = reqwest::multipart::Form::new()
        .part("image", part)
        .text("overwrite", "true");

    let resp = http
        .post(format!("{endpoint}/upload/image"))
        .multipart(form)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("POST comfyui /upload/image")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("/upload/image failed HTTP {status}: {text}");
    }

    let result: serde_json::Value = resp.json().await.context("parse upload response")?;

    result["name"]
        .as_str()
        .map(String::from)
        .context("missing 'name' in upload response")
}

/// Replace a placeholder string throughout a workflow JSON tree.
fn fill_placeholder(workflow: &mut serde_json::Value, placeholder: &str, value: &str) {
    match workflow {
        serde_json::Value::String(s) if s == placeholder => {
            *s = value.to_string();
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                fill_placeholder(v, placeholder, value);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                fill_placeholder(v, placeholder, value);
            }
        }
        _ => {}
    }
}

/// Extract output image references from ComfyUI history outputs.
fn extract_output_images(outputs: &serde_json::Value) -> Vec<OutputImage> {
    let mut images = Vec::new();

    if let Some(obj) = outputs.as_object() {
        for (_node_id, node_output) in obj {
            if let Some(img_array) = node_output.get("images").and_then(|v| v.as_array()) {
                for img in img_array {
                    if let (Some(filename), Some(subfolder), Some(img_type)) = (
                        img.get("filename").and_then(|v| v.as_str()),
                        img.get("subfolder").and_then(|v| v.as_str()),
                        img.get("type").and_then(|v| v.as_str()),
                    ) {
                        if img_type == "output" {
                            images.push(OutputImage {
                                filename: filename.to_string(),
                                subfolder: subfolder.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    images
}

struct OutputImage {
    filename: String,
    subfolder: String,
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

    // ── Placeholder filling ────────────────────────────────────

    #[test]
    fn fill_placeholder_replaces_strings() {
        let mut wf = serde_json::json!({
            "node": { "inputs": { "image": "PLACEHOLDER_IMAGE" } }
        });
        fill_placeholder(&mut wf, "PLACEHOLDER_IMAGE", "photo.png");
        assert_eq!(wf["node"]["inputs"]["image"], "photo.png");
    }

    #[test]
    fn fill_placeholder_preserves_non_matching() {
        let mut wf = serde_json::json!({
            "node": { "inputs": { "image": "PLACEHOLDER_IMAGE", "prefix": "zen" } }
        });
        fill_placeholder(&mut wf, "PLACEHOLDER_IMAGE", "photo.png");
        assert_eq!(wf["node"]["inputs"]["prefix"], "zen");
    }

    #[test]
    fn fill_placeholder_handles_arrays() {
        let mut wf = serde_json::json!({
            "node": { "inputs": { "model": ["2", 0], "name": "PLACEHOLDER_MODEL" } }
        });
        fill_placeholder(&mut wf, "PLACEHOLDER_MODEL", "4x-UltraSharp.pth");
        assert_eq!(wf["node"]["inputs"]["name"], "4x-UltraSharp.pth");
        // Array edge reference should be untouched
        assert_eq!(wf["node"]["inputs"]["model"][0], "2");
    }

    // ── Output extraction ──────────────────────────────────────

    #[test]
    fn extract_output_images_from_history() {
        let outputs = serde_json::json!({
            "4": {
                "images": [
                    { "filename": "zen-upscale_00001_.png", "subfolder": "", "type": "output" }
                ]
            }
        });
        let images = extract_output_images(&outputs);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].filename, "zen-upscale_00001_.png");
    }

    #[test]
    fn extract_output_images_skips_temp() {
        let outputs = serde_json::json!({
            "4": {
                "images": [
                    { "filename": "temp.png", "subfolder": "", "type": "temp" },
                    { "filename": "result.png", "subfolder": "", "type": "output" }
                ]
            }
        });
        let images = extract_output_images(&outputs);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].filename, "result.png");
    }

    #[test]
    fn extract_output_images_empty() {
        assert!(extract_output_images(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn extract_output_images_multiple_nodes() {
        let outputs = serde_json::json!({
            "4": { "images": [{ "filename": "a.png", "subfolder": "", "type": "output" }] },
            "7": { "images": [{ "filename": "b.png", "subfolder": "sub", "type": "output" }] }
        });
        assert_eq!(extract_output_images(&outputs).len(), 2);
    }
}
