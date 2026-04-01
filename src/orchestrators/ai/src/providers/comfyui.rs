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

use crate::catalog::traits::{
    BoxFuture, DiscoveryConfig, FormSchema, ProbeResult, Provider, ProviderContext, ServiceModel,
};
use crate::domain::skill::{
    ContentSlot, ContentType, ModelRef, SkillDefinition, WorkflowJob,
    WorkflowJobStatus, WorkflowRequest,
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
            let capabilities = vec![Capability::Image];
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

    // ── Skills (ORCH-0018) ────────────────────────────────────────

    fn skills(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<SkillDefinition>>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let mut skills = Vec::new();

            // Upscale skill: available when upscale models are installed
            let upscale_models = list_models(&self.http, &endpoint, "upscale_models")
                .await
                .unwrap_or_default();

            if !upscale_models.is_empty() {
                skills.push(build_upscale_skill(&upscale_models));
            }

            // Future: image.generate (when checkpoints present),
            // image.img2img, image.inpaint, image.remove_bg, etc.

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
            match req.skill.as_str() {
                "image.upscale" => execute_upscale(&self.http, &endpoint, &req).await,
                other => anyhow::bail!("unknown skill: {}", other),
            }
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

// ── Built-in Skills ────────────────────────────────────────────

/// Build the `image.upscale` skill definition from installed models.
fn build_upscale_skill(available_models: &[String]) -> SkillDefinition {
    // Build enum of available models for the parameter schema
    let model_enum: Vec<serde_json::Value> = available_models
        .iter()
        .map(|m| serde_json::Value::String(m.clone()))
        .collect();

    let default_model = available_models.first().cloned().unwrap_or_default();

    SkillDefinition {
        name: "image.upscale".into(),
        capability: Capability::Image,
        description: "Upscale an image using an AI upscaling model".into(),
        content_slots: vec![ContentSlot {
            role: "source".into(),
            content_type: ContentType::Image,
            required: true,
        }],
        parameter_schema: FormSchema {
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "upscale_model": {
                        "type": "string",
                        "title": "Upscale Model",
                        "enum": model_enum,
                        "default": default_model
                    }
                }
            }),
            ui_schema: serde_json::json!({
                "upscale_model": { "ui:widget": "select" }
            }),
        },
        diagram: Some(
            "graph LR\n    A[Load Image] --> C[Upscale]\n    B[Load Model] --> C\n    C --> D[Save Image]".into()
        ),
        required_models: available_models
            .iter()
            .map(|m| ModelRef {
                filename: m.clone(),
                model_type: "upscale_models".into(),
                description: None,
            })
            .collect(),
        implementation: upscale_workflow_template(),
    }
}

/// The ComfyUI workflow template for image upscaling.
///
/// Placeholders:
/// - `PLACEHOLDER_IMAGE` → uploaded filename (from POST /upload/image)
/// - `PLACEHOLDER_MODEL` → upscale model filename
fn upscale_workflow_template() -> serde_json::Value {
    serde_json::json!({
        "load_image": {
            "class_type": "LoadImage",
            "inputs": { "image": "PLACEHOLDER_IMAGE" }
        },
        "load_model": {
            "class_type": "UpscaleModelLoader",
            "inputs": { "model_name": "PLACEHOLDER_MODEL" }
        },
        "upscale": {
            "class_type": "ImageUpscaleWithModel",
            "inputs": {
                "upscale_model": ["load_model", 0],
                "image": ["load_image", 0]
            }
        },
        "save": {
            "class_type": "SaveImage",
            "inputs": {
                "images": ["upscale", 0],
                "filename_prefix": "zen-upscale"
            }
        }
    })
}

// ── Workflow Execution ─────────────────────────────────────────

const WORKFLOW_TIMEOUT: Duration = Duration::from_secs(300);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Execute the `image.upscale` workflow on a ComfyUI instance.
async fn execute_upscale(
    http: &Client,
    endpoint: &str,
    req: &WorkflowRequest,
) -> Result<WorkflowJob> {
    // 1. Extract input image from content blocks
    let image_content = req
        .content
        .iter()
        .find(|c| c.content_type == ContentType::Image)
        .context("image.upscale requires an image content block")?;

    let image_bytes = resolve_content_bytes(http, image_content).await?;

    // 2. Select upscale model from parameters (or use first available)
    let model_name = req
        .parameters
        .get("upscale_model")
        .and_then(|v| v.as_str())
        .map(String::from);

    // If no model specified, query available models and pick first
    let model_name = match model_name {
        Some(m) => m,
        None => {
            let models = list_models(http, endpoint, "upscale_models").await?;
            models
                .into_iter()
                .next()
                .context("no upscale models installed on this ComfyUI instance")?
        }
    };

    // 3. Upload image to ComfyUI
    let uploaded_name = upload_image(http, endpoint, &image_bytes).await?;

    // 4. Fill workflow template
    let mut workflow = upscale_workflow_template();
    workflow["load_image"]["inputs"]["image"] = serde_json::Value::String(uploaded_name);
    workflow["load_model"]["inputs"]["model_name"] = serde_json::Value::String(model_name);

    // 5. Submit workflow
    let client_id = format!("{:x}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos());
    let prompt_response = http
        .post(format!("{endpoint}/prompt"))
        .json(&serde_json::json!({
            "prompt": workflow,
            "client_id": client_id,
        }))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .context("POST comfyui /prompt")?;

    if !prompt_response.status().is_success() {
        let status = prompt_response.status();
        let text = prompt_response.text().await.unwrap_or_default();
        anyhow::bail!("comfyui /prompt failed HTTP {status}: {text}");
    }

    let prompt_result: serde_json::Value = prompt_response
        .json()
        .await
        .context("parse /prompt response")?;

    let prompt_id = prompt_result["prompt_id"]
        .as_str()
        .context("missing prompt_id in /prompt response")?
        .to_string();

    // 6. Poll for completion (WebSocket would be better, but polling is simpler for MVP)
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

        // Check if our prompt is in the history
        if let Some(entry) = history_json.get(&prompt_id) {
            // Check for execution errors
            if let Some(status) = entry.get("status") {
                if status.get("status_str").and_then(|s| s.as_str()) == Some("error") {
                    let messages = status
                        .get("messages")
                        .and_then(|m| m.as_array())
                        .map(|arr| format!("{:?}", arr))
                        .unwrap_or_else(|| "unknown error".into());
                    anyhow::bail!("comfyui workflow execution failed: {}", messages);
                }
            }

            // Extract output images from the "save" node
            if let Some(outputs) = entry.get("outputs") {
                let images = extract_output_images(outputs);
                if !images.is_empty() {
                    break images;
                }
            }
        }
    };

    // 7. Build result with asset URLs
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
        skill: "image.upscale".into(),
        status: WorkflowJobStatus::Completed,
        progress: Some(1.0),
        content: Some(content),
        error: None,
        usage: Some(crate::domain::skill::WorkflowUsage { duration_ms }),
    })
}

/// Resolve a ContentBlock to raw bytes (inline base64 or URL fetch).
async fn resolve_content_bytes(
    http: &Client,
    content: &crate::domain::skill::ContentBlock,
) -> Result<Vec<u8>> {
    if let Some(data) = &content.data {
        // Strip data URI prefix if present: "data:image/png;base64,..." → base64 payload
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
            .with_context(|| format!("read content URL body: {url}"))
    } else {
        anyhow::bail!("content block has neither 'data' nor 'url'")
    }
}

/// Upload an image to ComfyUI via POST /upload/image.
async fn upload_image(http: &Client, endpoint: &str, image_bytes: &[u8]) -> Result<String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let filename = format!("zen-input-{:x}.png", nanos);

    let part = reqwest::multipart::Part::bytes(image_bytes.to_vec())
        .file_name(filename.clone())
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
        anyhow::bail!("comfyui /upload/image failed HTTP {status}: {text}");
    }

    let result: serde_json::Value = resp.json().await.context("parse upload response")?;

    result["name"]
        .as_str()
        .map(String::from)
        .context("missing 'name' in upload response")
}

/// Extract output image references from a ComfyUI history entry's outputs.
fn extract_output_images(outputs: &serde_json::Value) -> Vec<OutputImage> {
    let mut images = Vec::new();

    // outputs is an object keyed by node ID → {"images": [...]}
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

    // ── Skill building ─────────────────────────────────────────

    #[test]
    fn build_upscale_skill_populates_model_enum() {
        let models = vec!["4x-UltraSharp.pth".into(), "RealESRGAN_x4plus.pth".into()];
        let skill = build_upscale_skill(&models);

        assert_eq!(skill.name, "image.upscale");
        assert_eq!(skill.capability, Capability::Image);
        assert_eq!(skill.content_slots.len(), 1);
        assert_eq!(skill.content_slots[0].role, "source");
        assert!(skill.content_slots[0].required);
        assert!(skill.diagram.is_some());
        assert_eq!(skill.required_models.len(), 2);

        // Parameter schema should have the model enum
        let props = &skill.parameter_schema.schema["properties"]["upscale_model"];
        let enum_vals = props["enum"].as_array().unwrap();
        assert_eq!(enum_vals.len(), 2);
        assert_eq!(enum_vals[0], "4x-UltraSharp.pth");
        assert_eq!(props["default"], "4x-UltraSharp.pth");
    }

    #[test]
    fn build_upscale_skill_single_model() {
        let models = vec!["4x-UltraSharp.pth".into()];
        let skill = build_upscale_skill(&models);

        assert_eq!(skill.required_models.len(), 1);
        assert_eq!(skill.required_models[0].model_type, "upscale_models");
    }

    // ── Workflow template ──────────────────────────────────────

    #[test]
    fn workflow_template_has_correct_structure() {
        let tmpl = upscale_workflow_template();

        assert!(tmpl.get("load_image").is_some());
        assert!(tmpl.get("load_model").is_some());
        assert!(tmpl.get("upscale").is_some());
        assert!(tmpl.get("save").is_some());

        assert_eq!(tmpl["load_image"]["class_type"], "LoadImage");
        assert_eq!(tmpl["load_model"]["class_type"], "UpscaleModelLoader");
        assert_eq!(tmpl["upscale"]["class_type"], "ImageUpscaleWithModel");
        assert_eq!(tmpl["save"]["class_type"], "SaveImage");
    }

    #[test]
    fn workflow_template_placeholder_filling() {
        let mut tmpl = upscale_workflow_template();
        tmpl["load_image"]["inputs"]["image"] = serde_json::json!("my-photo.png");
        tmpl["load_model"]["inputs"]["model_name"] = serde_json::json!("4x-UltraSharp.pth");

        assert_eq!(tmpl["load_image"]["inputs"]["image"], "my-photo.png");
        assert_eq!(tmpl["load_model"]["inputs"]["model_name"], "4x-UltraSharp.pth");

        // Edges should still reference node outputs correctly
        assert_eq!(tmpl["upscale"]["inputs"]["upscale_model"][0], "load_model");
        assert_eq!(tmpl["upscale"]["inputs"]["image"][0], "load_image");
        assert_eq!(tmpl["save"]["inputs"]["images"][0], "upscale");
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
        assert_eq!(images[0].subfolder, "");
    }

    #[test]
    fn extract_output_images_multiple_nodes() {
        let outputs = serde_json::json!({
            "4": {
                "images": [
                    { "filename": "img1.png", "subfolder": "", "type": "output" },
                    { "filename": "img2.png", "subfolder": "", "type": "output" }
                ]
            },
            "7": {
                "images": [
                    { "filename": "img3.png", "subfolder": "sub", "type": "output" }
                ]
            }
        });

        let images = extract_output_images(&outputs);
        assert_eq!(images.len(), 3);
    }

    #[test]
    fn extract_output_images_skips_temp_type() {
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
        let outputs = serde_json::json!({});
        let images = extract_output_images(&outputs);
        assert!(images.is_empty());
    }

    #[test]
    fn extract_output_images_no_images_key() {
        let outputs = serde_json::json!({
            "4": { "text": "some output" }
        });
        let images = extract_output_images(&outputs);
        assert!(images.is_empty());
    }
}
