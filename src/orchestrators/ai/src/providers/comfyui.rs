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
    AutoKind, ContentType, ParamType, SkillDefinition, SkillMapping,
    WorkflowJob, WorkflowJobStatus, WorkflowRequest,
};
use crate::domain::types::{Capability, OfferingKind};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const ENUMERATE_TIMEOUT: Duration = Duration::from_secs(10);
const WORKFLOW_TIMEOUT: Duration = Duration::from_secs(300);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// ComfyUI is a skill-only provider — it doesn't serve standalone inference.
/// Its capabilities are determined by the skills loaded from disk:
/// Image (generate, upscale, transform, inpaint), Vision (WD14 tagger),
/// Speech (TTS), etc. This list covers all capabilities that ComfyUI
/// skills may declare, so the dashboard shows ComfyUI under the right tabs.
const COMFYUI_CAPABILITIES: &[Capability] = &[
    Capability::Image,
    Capability::Vision,
    Capability::Speech,
];

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
                capabilities: COMFYUI_CAPABILITIES.to_vec(),
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

    fn builtin_skills(&self) -> Vec<SkillDefinition> {
        vec![
            crate::skills::builtin::image_upscale(&[]),
            crate::skills::builtin::image_generate(&[]),
            crate::skills::builtin::image_img2img(&[]),
            crate::skills::builtin::image_inpaint(&[]),
        ]
    }

    fn check_skill_readiness(
        &self,
        ctx: &ProviderContext,
        skill: &str,
    ) -> BoxFuture<'_, Result<crate::domain::skill::SkillReadiness>> {
        let endpoint = ctx.endpoint.clone();
        let skill = skill.to_string();

        Box::pin(async move {
            // Skills requiring specific model types installed on the instance
            let model_check = match skill.as_str() {
                "image.upscale" => Some("upscale_models"),
                "image.generate" | "image.img2img" | "image.inpaint" => Some("checkpoints"),
                // WD14 and TTS auto-download models — check node availability via /object_info
                "vision.tag" => {
                    return check_custom_node(&self.http, &endpoint, "WD14Tagger|pysssss", "ComfyUI-WD14-Tagger").await;
                }
                "speech.tts" => {
                    return check_custom_node(&self.http, &endpoint, "ChatterBoxEngineNode", "TTS-Audio-Suite").await;
                }
                _ => None,
            };

            match model_check {
                Some(model_type) => {
                    let models = list_models(&self.http, &endpoint, model_type)
                        .await
                        .unwrap_or_default();

                    if models.is_empty() {
                        Ok(crate::domain::skill::SkillReadiness {
                            ready: false,
                            reason: format!("no {} installed", model_type),
                        })
                    } else {
                        Ok(crate::domain::skill::SkillReadiness {
                            ready: true,
                            reason: "ready".into(),
                        })
                    }
                }
                None => {
                    // Unknown skills default to ready — disk-loaded skills
                    // shouldn't require Rust match arms to be available.
                    Ok(crate::domain::skill::SkillReadiness {
                        ready: true,
                        reason: "ready".into(),
                    })
                }
            }
        })
    }

    fn provision_skill(
        &self,
        ctx: &ProviderContext,
        skill_name: &str,
        cache_dir: &std::path::Path,
        moss_endpoint: &str,
        fqn: &str,
    ) -> BoxFuture<'_, Result<()>> {
        let endpoint = ctx.endpoint.clone();
        let skill_name = skill_name.to_string();
        let cache_dir = cache_dir.to_path_buf();
        let moss_endpoint = moss_endpoint.to_string();
        let fqn = fqn.to_string();

        Box::pin(async move {
            // Look up skill definition to get required_models
            let skill_def = match skill_name.as_str() {
                "image.upscale" => crate::skills::builtin::image_upscale(&[]),
                "image.generate" => crate::skills::builtin::image_generate(&[]),
                "image.img2img" => crate::skills::builtin::image_img2img(&[]),
                "image.inpaint" => crate::skills::builtin::image_inpaint(&[]),
                // WD14 tagger and TTS have no orchestrator-provisioned models;
                // custom nodes auto-download their own models.
                "vision.tag" | "speech.tts" => {
                    tracing::debug!(skill = %skill_name, "no provisioning needed — models auto-download");
                    return Ok(());
                }
                other => anyhow::bail!("unknown skill: {}", other),
            };

            // Aggregate recommended models for download URLs
            let mut recommended: Vec<crate::skills::builtin::RecommendedModel> = Vec::new();
            recommended.extend(crate::skills::builtin::recommended_upscale_models());
            recommended.extend(crate::skills::builtin::recommended_checkpoint_models());

            for model_ref in &skill_def.required_models {
                let rec = recommended.iter().find(|r| r.filename == model_ref.filename);
                let rec = match rec {
                    Some(r) => r,
                    None => {
                        tracing::debug!(
                            filename = %model_ref.filename,
                            "model not in recommended list — skipping"
                        );
                        continue;
                    }
                };

                // Check if model already exists on instance.
                // Volume is "comfyui-models" (the Docker volume name).
                // Path is "{model_type}/{filename}" (subdirectory within the volume).
                let volume = "comfyui-models";
                let model_path = format!("{}/{}", rec.model_type, rec.filename);
                if crate::skills::prep::model_exists_on_instance(
                    &self.http,
                    &moss_endpoint,
                    &fqn,
                    volume,
                    &model_path,
                )
                .await
                {
                    tracing::debug!(
                        filename = %rec.filename,
                        endpoint = %endpoint,
                        "model already on instance"
                    );
                    continue;
                }

                // Download to local cache
                let local_path = crate::skills::prep::ensure_cached(
                    &self.http,
                    &cache_dir,
                    &rec.model_type,
                    &rec.filename,
                    &rec.url,
                )
                .await
                .with_context(|| format!("cache model: {}", rec.filename))?;

                // Push to instance via Moss volume API
                crate::skills::prep::push_model_to_instance(
                    &self.http,
                    &moss_endpoint,
                    &fqn,
                    volume,
                    &model_path,
                    &local_path,
                )
                .await
                .with_context(|| format!("push model {} to {}", rec.filename, endpoint))?;

                tracing::info!(
                    filename = %rec.filename,
                    endpoint = %endpoint,
                    "model provisioned on instance"
                );
            }

            Ok(())
        })
    }

    fn workflow(
        &self,
        ctx: &ProviderContext,
        req: WorkflowRequest,
        skill: &SkillDefinition,
    ) -> BoxFuture<'_, Result<WorkflowJob>> {
        let endpoint = ctx.endpoint.clone();
        let skill = skill.clone();

        Box::pin(async move {
            execute_workflow(&self.http, &endpoint, &req, &skill).await
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

/// Check if a custom node is installed by querying /object_info/{class_type}.
///
/// ComfyUI's /object_info endpoint returns node definitions for installed nodes.
/// Returns ready=true if the node class exists, ready=false with install hint otherwise.
async fn check_custom_node(
    http: &Client,
    endpoint: &str,
    class_type: &str,
    pack_name: &str,
) -> Result<crate::domain::skill::SkillReadiness> {
    let resp = http
        .get(format!("{endpoint}/object_info/{class_type}"))
        .timeout(Duration::from_secs(5))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or_default();
            if body.get(class_type).is_some() {
                Ok(crate::domain::skill::SkillReadiness {
                    ready: true,
                    reason: "ready".into(),
                })
            } else {
                Ok(crate::domain::skill::SkillReadiness {
                    ready: false,
                    reason: format!("custom node not installed: {pack_name}"),
                })
            }
        }
        _ => Ok(crate::domain::skill::SkillReadiness {
            ready: false,
            reason: format!("custom node not installed: {pack_name}"),
        }),
    }
}

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

// ── Mapping-Driven Workflow Execution ──────────────────────────

/// Execute a workflow by iterating the skill's declarative mappings.
///
/// Pipeline: select template → apply mappings → submit → poll → extract.
/// Zero skill-specific branches.
async fn execute_workflow(
    http: &Client,
    endpoint: &str,
    req: &WorkflowRequest,
    skill: &SkillDefinition,
) -> Result<WorkflowJob> {
    let params = &req.parameters;

    // 1. Select workflow template: parameters.workflow overrides default_workflow
    let template_name = params
        .get("workflow")
        .and_then(|v| v.as_str())
        .unwrap_or(&skill.default_workflow);

    let mut workflow = skill
        .workflows
        .get(template_name)
        .cloned()
        .with_context(|| format!("unknown workflow template: {template_name}"))?;

    // 2. Apply all mappings
    for mapping in &skill.mappings {
        match mapping {
            SkillMapping::Content { role, content_type, placeholder } => {
                let content_block = req.content.iter().find(|c| {
                    c.role.as_deref() == Some(role)
                });

                match content_type {
                    ContentType::Image => {
                        if let Some(block) = content_block {
                            let image_bytes = resolve_content_bytes(http, block).await?;
                            let uploaded_name = upload_image(http, endpoint, &image_bytes).await?;
                            fill_placeholder(&mut workflow, placeholder, &uploaded_name);
                        }
                    }
                    ContentType::Text => {
                        let text = content_block
                            .and_then(|b| b.data.as_deref())
                            .unwrap_or("");
                        fill_placeholder(&mut workflow, placeholder, text);
                    }
                    ContentType::Audio => {
                        // Audio input content — resolve to bytes, upload via /upload/image
                        // (ComfyUI routes audio through the same upload endpoint)
                        if let Some(block) = content_block {
                            let audio_bytes = resolve_content_bytes(http, block).await?;
                            let uploaded_name = upload_audio(http, endpoint, &audio_bytes).await?;
                            fill_placeholder(&mut workflow, placeholder, &uploaded_name);
                        }
                    }
                }
            }
            SkillMapping::Param { field, node, input, placeholder, param_type, default, .. } => {
                // Skip the "workflow" field — already consumed above
                if field == "workflow" {
                    continue;
                }

                let value = resolve_param_value(params, field, param_type, default.as_ref());
                if value.is_null() {
                    continue;
                }

                // Placeholder substitution (string values throughout the tree)
                if let Some(ph) = placeholder {
                    if let Some(s) = value.as_str() {
                        fill_placeholder(&mut workflow, ph, s);
                        continue;
                    }
                }

                // Node-targeted (set specific node input by ID)
                if let (Some(n), Some(i)) = (node, input) {
                    set_node_input(&mut workflow, n, i, value);
                }
            }
        }
    }

    // 3. Submit + poll + extract
    submit_and_poll(http, endpoint, &req.skill, workflow).await
}

/// Resolve a parameter value from the request, falling back to default or auto-generation.
fn resolve_param_value(
    params: &serde_json::Value,
    field: &str,
    param_type: &ParamType,
    default: Option<&serde_json::Value>,
) -> serde_json::Value {
    // User-provided value takes priority
    if let Some(value) = params.get(field) {
        if !value.is_null() {
            return value.clone();
        }
    }

    // Auto-generated value
    if let ParamType::Auto { kind } = param_type {
        return match kind {
            AutoKind::RandomInt => {
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as u64;
                serde_json::json!(seed)
            }
        };
    }

    // Default value
    default.cloned().unwrap_or(serde_json::Value::Null)
}

/// Set a value on a specific node's input by node ID.
fn set_node_input(workflow: &mut serde_json::Value, node_id: &str, input_name: &str, value: serde_json::Value) {
    if value.is_null() {
        return;
    }
    if let Some(inputs) = workflow
        .get_mut(node_id)
        .and_then(|n| n.get_mut("inputs"))
        .and_then(|i| i.as_object_mut())
    {
        inputs.insert(input_name.to_string(), value);
    }
}

/// Submit workflow to ComfyUI, poll for completion, extract output images.
async fn submit_and_poll(
    http: &Client,
    endpoint: &str,
    skill_name: &str,
    workflow: serde_json::Value,
) -> Result<WorkflowJob> {
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

    // Poll for completion
    let start = std::time::Instant::now();
    let output_assets = loop {
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
                let assets = extract_outputs(outputs);
                if !assets.is_empty() {
                    break assets;
                }
            }
        }
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    let content: Vec<crate::domain::skill::ContentBlock> = output_assets
        .into_iter()
        .map(|asset| match asset {
            OutputAsset::File { filename, subfolder, content_type } => {
                let format = infer_format(&filename);
                crate::domain::skill::ContentBlock {
                    content_type,
                    role: None,
                    data: None,
                    url: Some(format!(
                        "{endpoint}/view?filename={filename}&type=output&subfolder={subfolder}",
                    )),
                    format: Some(format),
                }
            }
            OutputAsset::Text(text) => crate::domain::skill::ContentBlock {
                content_type: ContentType::Text,
                role: None,
                data: Some(text),
                url: None,
                format: None,
            },
        })
        .collect();

    Ok(WorkflowJob {
        id: prompt_id.clone(),
        skill: skill_name.to_string(),
        status: WorkflowJobStatus::Completed,
        prompt_id: Some(prompt_id),
        endpoint: Some(endpoint.to_string()),
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

/// Upload audio to ComfyUI via `POST /upload/image` with correct MIME type.
///
/// ComfyUI uses the same `/upload/image` endpoint for all file types but
/// routes based on the subfolder param. Audio files need correct extension
/// and MIME type so ComfyUI stores them in the right input directory.
async fn upload_audio(http: &Client, endpoint: &str, audio_bytes: &[u8]) -> Result<String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let filename = format!("zen-input-{nanos:x}.wav");

    let part = reqwest::multipart::Part::bytes(audio_bytes.to_vec())
        .file_name(filename)
        .mime_str("audio/wav")?;

    let form = reqwest::multipart::Form::new()
        .part("image", part)
        .text("subfolder", "audio")
        .text("overwrite", "true");

    let resp = http
        .post(format!("{endpoint}/upload/image"))
        .multipart(form)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("POST comfyui /upload/image (audio)")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("/upload/image (audio) failed HTTP {status}: {text}");
    }

    let result: serde_json::Value = resp.json().await.context("parse upload response")?;

    result["name"]
        .as_str()
        .map(String::from)
        .context("missing 'name' in upload response")
}

/// Infer output format from filename extension.
fn infer_format(filename: &str) -> String {
    match filename.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "png" => "png",
        "jpg" | "jpeg" => "jpeg",
        "webp" => "webp",
        "gif" => "gif",
        "wav" => "wav",
        "flac" => "flac",
        "mp3" => "mp3",
        "ogg" => "ogg",
        _ => "bin",
    }
    .to_string()
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

/// Extract output assets from ComfyUI history outputs.
///
/// ComfyUI stores outputs under type-keyed arrays per node:
/// - `"images"` → image files (PNG, JPG)
/// - `"audio"` → audio files (WAV, FLAC, MP3)
/// - `"text"` → text strings (tags, captions)
///
/// Each file entry has `filename`, `subfolder`, `type` (= "output").
/// Text entries are plain strings.
fn extract_outputs(outputs: &serde_json::Value) -> Vec<OutputAsset> {
    let mut assets = Vec::new();

    let Some(obj) = outputs.as_object() else {
        return assets;
    };

    for (_node_id, node_output) in obj {
        // Image outputs
        if let Some(arr) = node_output.get("images").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(asset) = parse_file_output(item, ContentType::Image) {
                    assets.push(asset);
                }
            }
        }

        // Audio outputs (SaveAudio, PreviewAudio, etc.)
        if let Some(arr) = node_output.get("audio").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(asset) = parse_file_output(item, ContentType::Audio) {
                    assets.push(asset);
                }
            }
        }

        // Text outputs (generic text output nodes)
        if let Some(arr) = node_output.get("text").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(s) = item.as_str() {
                    assets.push(OutputAsset::Text(s.to_string()));
                }
            }
        }

        // Tag outputs (WD14 tagger, captioners — use "tags" key)
        if let Some(arr) = node_output.get("tags").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(s) = item.as_str() {
                    assets.push(OutputAsset::Text(s.to_string()));
                }
            }
        }
    }

    assets
}

/// Parse a file-based output entry (images or audio).
fn parse_file_output(item: &serde_json::Value, content_type: ContentType) -> Option<OutputAsset> {
    let filename = item.get("filename")?.as_str()?;
    let subfolder = item.get("subfolder").and_then(|v| v.as_str()).unwrap_or("");
    let file_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("output");

    if file_type != "output" {
        return None;
    }

    Some(OutputAsset::File {
        filename: filename.to_string(),
        subfolder: subfolder.to_string(),
        content_type,
    })
}

/// An output asset from a ComfyUI workflow execution.
enum OutputAsset {
    /// A file (image or audio) available via /view endpoint.
    File {
        filename: String,
        subfolder: String,
        content_type: ContentType,
    },
    /// A text string (tags, captions).
    Text(String),
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_and_capabilities() {
        let p = ComfyUiProvider::new();
        assert_eq!(p.kind(), OfferingKind::ComfyUi);
        assert!(p.capabilities().contains(&Capability::Image));
        assert!(p.capabilities().contains(&Capability::Vision));
        assert!(p.capabilities().contains(&Capability::Speech));
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
    fn extract_outputs_images_from_history() {
        let outputs = serde_json::json!({
            "4": {
                "images": [
                    { "filename": "zen-upscale_00001_.png", "subfolder": "", "type": "output" }
                ]
            }
        });
        let assets = extract_outputs(&outputs);
        assert_eq!(assets.len(), 1);
        match &assets[0] {
            OutputAsset::File { filename, content_type, .. } => {
                assert_eq!(filename, "zen-upscale_00001_.png");
                assert_eq!(*content_type, ContentType::Image);
            }
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn extract_outputs_skips_temp() {
        let outputs = serde_json::json!({
            "4": {
                "images": [
                    { "filename": "temp.png", "subfolder": "", "type": "temp" },
                    { "filename": "result.png", "subfolder": "", "type": "output" }
                ]
            }
        });
        let assets = extract_outputs(&outputs);
        assert_eq!(assets.len(), 1);
        match &assets[0] {
            OutputAsset::File { filename, .. } => assert_eq!(filename, "result.png"),
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn extract_outputs_empty() {
        assert!(extract_outputs(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn extract_outputs_multiple_nodes() {
        let outputs = serde_json::json!({
            "4": { "images": [{ "filename": "a.png", "subfolder": "", "type": "output" }] },
            "7": { "images": [{ "filename": "b.png", "subfolder": "sub", "type": "output" }] }
        });
        assert_eq!(extract_outputs(&outputs).len(), 2);
    }

    #[test]
    fn extract_outputs_audio() {
        let outputs = serde_json::json!({
            "9": {
                "audio": [
                    { "filename": "speech_00001_.wav", "subfolder": "", "type": "output" }
                ]
            }
        });
        let assets = extract_outputs(&outputs);
        assert_eq!(assets.len(), 1);
        match &assets[0] {
            OutputAsset::File { filename, content_type, .. } => {
                assert_eq!(filename, "speech_00001_.wav");
                assert_eq!(*content_type, ContentType::Audio);
            }
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn extract_outputs_text() {
        let outputs = serde_json::json!({
            "5": {
                "text": ["1girl, solo, long_hair, looking_at_viewer"]
            }
        });
        let assets = extract_outputs(&outputs);
        assert_eq!(assets.len(), 1);
        match &assets[0] {
            OutputAsset::Text(text) => {
                assert!(text.contains("1girl"));
            }
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn extract_outputs_tags_wd14() {
        let outputs = serde_json::json!({
            "2": {
                "tags": ["1girl, solo, long_hair, looking_at_viewer, blush"]
            }
        });
        let assets = extract_outputs(&outputs);
        assert_eq!(assets.len(), 1);
        match &assets[0] {
            OutputAsset::Text(text) => {
                assert!(text.contains("1girl"));
                assert!(text.contains("looking_at_viewer"));
            }
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn extract_outputs_mixed() {
        let outputs = serde_json::json!({
            "3": { "images": [{ "filename": "out.png", "subfolder": "", "type": "output" }] },
            "7": { "audio": [{ "filename": "out.wav", "subfolder": "", "type": "output" }] },
            "9": { "text": ["hello world"] },
            "11": { "tags": ["1girl, solo"] }
        });
        let assets = extract_outputs(&outputs);
        assert_eq!(assets.len(), 4);
    }

    #[test]
    fn infer_format_from_extension() {
        assert_eq!(infer_format("photo.png"), "png");
        assert_eq!(infer_format("photo.jpg"), "jpeg");
        assert_eq!(infer_format("audio.wav"), "wav");
        assert_eq!(infer_format("audio.flac"), "flac");
        assert_eq!(infer_format("audio.mp3"), "mp3");
        assert_eq!(infer_format("unknown.xyz"), "bin");
    }
}
