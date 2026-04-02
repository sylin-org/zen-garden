//! Analyze orchestrator — detect input, fetch, extract, resolve, produce result.
//!
//! This is the pipeline coordinator. Each step delegates to a focused module.
//! Failures in optional steps (model resolution, preview) are warnings, not errors.

use std::path::Path;

use anyhow::{Context, Result};
use reqwest::Client;

use super::{civitai, gen_data_parse, input_detect, model_resolve, param_extract, png_extract, ui_to_api, workflow_synth};
use crate::skills::cache::{CachePaths, DependencyManifest};

// ── Result Types ──────────────────────────────────────────────

/// Complete analysis result — ready to become a draft skill.
#[derive(Debug, serde::Serialize)]
pub struct AnalyzeResult {
    pub moniker: String,
    pub display_name: String,
    pub capability: String,
    pub workflow: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram: Option<String>,
    pub models: Vec<model_resolve::ModelResolution>,
    pub mappings: Vec<crate::domain::skill::SkillMapping>,
    pub content_slots: Vec<param_extract::ContentSlotDetection>,
    pub inputs: Vec<DetectedInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
    pub warnings: Vec<Warning>,
    /// Generation params (for the UI to display/edit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<GenerationSummary>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectedInput {
    pub role: String,
    pub content_type: String,
    pub node_id: String,
    pub placeholder: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Source {
    #[serde(rename = "type")]
    pub source_type: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Warning {
    #[serde(rename = "type")]
    pub warning_type: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GenerationSummary {
    pub prompt: String,
    pub negative_prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cfg_scale: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampler: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

// ── Main Entry Point ──────────────────────────────────────────

/// Run the full analysis pipeline.
///
/// Accepts either text input (`input`) or binary bytes (`input_bytes`).
/// Produces an AnalyzeResult or a descriptive error.
pub async fn run(
    http: &Client,
    input: &str,
    input_bytes: Option<&[u8]>,
    data_dir: &Path,
    _manager_registry: &model_resolve::ManagerRegistry,
    civitai_token: Option<&str>,
) -> Result<AnalyzeResult> {
    let civitai = match civitai_token {
        Some(token) => civitai::CivitaiClient::with_token(http.clone(), token.to_string()),
        None => civitai::CivitaiClient::new(http.clone()),
    };
    let mut warnings = Vec::new();

    // ── Step 1: Detect input type and extract workflow ─────────
    let extraction_path;
    let (workflow, source, preview_url, civitai_meta) = if let Some(bytes) = input_bytes {
        if input_detect::is_png_bytes(bytes) {
            extraction_path = "png_bytes";
            extract_from_png(bytes, &mut warnings)?
        } else {
            extraction_path = "text_from_bytes";
            let text = std::str::from_utf8(bytes).context("binary input is not PNG or valid UTF-8")?;
            extract_from_text(&civitai, text, &mut warnings).await?
        }
    } else {
        extraction_path = "text_input";
        extract_from_text(&civitai, input, &mut warnings).await?
    };

    tracing::info!(
        extraction_path,
        source = ?source,
        has_civitai_meta = civitai_meta.is_some(),
        has_preview = preview_url.is_some(),
        "IMPORT_DEBUG step 1: extraction complete"
    );

    // ── Step 2: Parse the workflow (for diagram + model detection) ──
    // Log the raw workflow's checkpoint value BEFORE any processing
    if let Some(obj) = workflow.as_object() {
        for (nid, node) in obj {
            if let Some(ct) = node.get("class_type").and_then(|v| v.as_str()) {
                if ct == "CheckpointLoaderSimple" {
                    let raw_ckpt = node.get("inputs").and_then(|i| i.get("ckpt_name"));
                    tracing::info!(
                        node_id = nid,
                        raw_ckpt_value = ?raw_ckpt,
                        "analyze: raw workflow checkpoint BEFORE param_extract"
                    );
                }
            }
        }
    }

    let parsed = crate::skills::parser::parse_workflow(&workflow)
        .map_err(|e| anyhow::anyhow!("workflow parse failed: {e}"))?;

    tracing::info!(
        model_count = parsed.models.len(),
        models = ?parsed.models.iter().map(|m| (&m.model_name, &m.model_type, m.is_placeholder)).collect::<Vec<_>>(),
        "analyze: parsed workflow models"
    );

    // ── Step 2b: Extract parameters + inject placeholders ─────
    // This walks the workflow, identifies tunable values, replaces them
    // with PLACEHOLDER_ tokens, and generates mappings.
    let extraction = param_extract::extract(&workflow);
    let workflow = extraction.workflow;
    let mappings = extraction.mappings;
    let content_slots = extraction.content_slots;

    // ── Step 3: Collect model filenames (from ORIGINAL, before placeholders) ──
    let model_pairs: Vec<(String, String)> = parsed
        .models
        .iter()
        .filter(|m| !m.is_placeholder)
        .map(|m| (m.model_name.clone(), m.model_type.clone()))
        .collect();

    // ── Step 4: Resolve models ────────────────────────────────
    let cache_paths = CachePaths::new(data_dir, "comfyui");
    let cache_manifest = DependencyManifest::load(&cache_paths.manifest_path).await;

    // Resolve CivitAI model version IDs (best source)
    let mut civitai_models = Vec::new();
    if let Some(ref meta) = civitai_meta {
        for vid in &meta.model_version_ids {
            match civitai::resolve_model_version(&civitai, *vid).await {
                Some(resolved) => civitai_models.push(resolved),
                None => {
                    warnings.push(Warning {
                        warning_type: "model_resolution".into(),
                        message: format!("CivitAI model version {} could not be resolved", vid),
                    });
                }
            }
        }
    }

    let hashes = civitai_meta
        .as_ref()
        .and_then(|m| m.generation.as_ref())
        .map(|g| g.hashes.clone())
        .unwrap_or_default();

    let resolution_ctx = model_resolve::ResolutionContext {
        civitai_models,
        hashes,
        cache_manifest,
        manager: model_resolve::ManagerRegistry::default(), // TODO: pass the shared one
    };

    let models = model_resolve::resolve_all(&civitai, &model_pairs, &resolution_ctx).await;

    tracing::info!(
        resolved_count = models.len(),
        resolved = ?models.iter().map(|m| m.filename()).collect::<Vec<_>>(),
        "analyze step 4: model resolution complete"
    );

    // Log mapping defaults BEFORE reconciliation
    for m in &mappings {
        if let crate::domain::skill::SkillMapping::Param { field, default, param_type, .. } = m {
            if matches!(field.as_str(), "checkpoint" | "lora" | "upscale_model" | "vae") {
                tracing::info!(
                    field = field.as_str(),
                    default = ?default,
                    param_type = ?std::mem::discriminant(param_type),
                    "analyze step 4b: model mapping BEFORE reconcile"
                );
            }
        }
    }

    // ── Step 4b: Reconcile mapping defaults with resolved filenames ──
    let mappings = reconcile_model_mappings(mappings, &model_pairs, &models);

    // Log mapping defaults AFTER reconciliation
    for m in &mappings {
        if let crate::domain::skill::SkillMapping::Param { field, default, .. } = m {
            if matches!(field.as_str(), "checkpoint" | "lora" | "upscale_model" | "vae") {
                tracing::info!(
                    field = field.as_str(),
                    default = ?default,
                    "analyze step 4b: model mapping AFTER reconcile"
                );
            }
        }
    }

    // ── Step 4c: Fail if no checkpoint could be resolved ──────
    // A skill with PLACEHOLDER_CHECKPOINT as the default will fail at execution.
    let has_unresolved_checkpoint = mappings.iter().any(|m| {
        matches!(m,
            crate::domain::skill::SkillMapping::Param { field, default: Some(serde_json::Value::String(v)), .. }
            if field == "checkpoint" && v.starts_with("PLACEHOLDER")
        )
    });
    if has_unresolved_checkpoint && models.is_empty() {
        anyhow::bail!(
            "Could not determine which model (checkpoint) this image uses. \
             The CivitAI metadata does not include model information. \
             Try a different image from the same model, or paste the workflow JSON directly."
        );
    }

    // ── Step 5: Detect inputs ─────────────────────────────────
    let inputs = detect_inputs(&parsed);

    // ── Step 6: Build metadata ────────────────────────────────
    let generation = civitai_meta
        .as_ref()
        .and_then(|m| m.generation.as_ref())
        .map(|g| GenerationSummary {
            prompt: g.prompt.clone(),
            negative_prompt: g.negative_prompt.clone(),
            steps: g.steps,
            cfg_scale: g.cfg_scale,
            sampler: g.sampler.clone(),
            seed: g.seed,
            model: g.model_name.clone(),
        });

    let moniker = generate_moniker(&source, &parsed, &models, &generation);
    let display_name = humanize_moniker(&moniker);

    Ok(AnalyzeResult {
        moniker,
        display_name,
        capability: "image".into(),
        workflow,
        diagram: Some(parsed.diagram),
        models,
        mappings,
        content_slots,
        inputs,
        source,
        preview_url,
        warnings,
        generation,
    })
}

// ── Extraction Paths ──────────────────────────────────────────

type ExtractionResult = (
    serde_json::Value,           // workflow
    Option<Source>,              // source
    Option<String>,              // preview URL
    Option<CivitaiMetaBundle>,   // CivitAI metadata for resolution
);

/// Bundle of CivitAI-specific metadata for model resolution.
struct CivitaiMetaBundle {
    model_version_ids: Vec<u64>,
    generation: Option<civitai::GenerationMeta>,
}

fn extract_from_png(
    bytes: &[u8],
    warnings: &mut Vec<Warning>,
) -> Result<ExtractionResult> {
    let extraction = png_extract::extract(bytes)?;

    if let Some(workflow) = extraction.workflow {
        // Convert UI format to API format if needed
        let workflow = if ui_to_api::is_ui_format(&workflow) {
            ui_to_api::convert(&workflow)?
        } else {
            workflow
        };
        return Ok((workflow, None, None, None));
    }

    // No workflow in PNG — try parameters text
    if let Some(params_text) = extraction.parameters_text {
        let params = gen_data_parse::parse(&params_text);
        let workflow = workflow_synth::synthesize_txt2img(&params);
        warnings.push(Warning {
            warning_type: "synthesized".into(),
            message: "No ComfyUI workflow found in PNG. Synthesized a standard txt2img workflow from generation parameters.".into(),
        });
        return Ok((workflow, None, None, None));
    }

    anyhow::bail!("PNG has no embedded ComfyUI workflow or generation parameters")
}

async fn extract_from_text(
    civitai: &civitai::CivitaiClient,
    input: &str,
    warnings: &mut Vec<Warning>,
) -> Result<ExtractionResult> {
    let input_type = input_detect::classify(input)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    match input_type {
        input_detect::InputType::CivitaiImage { image_id } => {
            extract_from_civitai(civitai, image_id, warnings).await
        }
        input_detect::InputType::CivitaiModel { model_id, version_id } => {
            extract_from_civitai_model(civitai, model_id, version_id, warnings).await
        }
        input_detect::InputType::PngUrl { url } => {
            extract_from_url(civitai.http(), &url, warnings).await
        }
        input_detect::InputType::GenericUrl { url } => {
            extract_from_url(civitai.http(), &url, warnings).await
        }
        input_detect::InputType::WorkflowJson { json } => {
            Ok((json, None, None, None))
        }
        input_detect::InputType::GenerationText { text } => {
            let params = gen_data_parse::parse(&text);
            let workflow = workflow_synth::synthesize_txt2img(&params);
            warnings.push(Warning {
                warning_type: "synthesized".into(),
                message: "Synthesized a standard txt2img workflow from generation parameters.".into(),
            });
            Ok((workflow, None, None, None))
        }
    }
}

async fn extract_from_civitai(
    civitai: &civitai::CivitaiClient,
    image_id: u64,
    warnings: &mut Vec<Warning>,
) -> Result<ExtractionResult> {
    let meta = civitai::fetch_image(civitai, image_id).await?;

    // Check for unsupported generators
    if let Some(generator) = civitai::is_unsupported_generator(meta.base_model.as_deref()) {
        anyhow::bail!("This image was generated by {generator}, which cannot be imported as a ComfyUI skill.");
    }

    let source = Source {
        source_type: "civitai".into(),
        url: format!("https://civitai.com/images/{image_id}"),
        image_id: Some(image_id),
        username: Some(meta.username.clone()),
    };
    let preview_url = Some(meta.image_url.clone());

    let civitai_bundle = CivitaiMetaBundle {
        model_version_ids: meta.model_version_ids.clone(),
        generation: meta.generation,
    };

    // Try to download the original image and extract workflow from PNG
    match civitai::download_original_image(civitai, &meta.image_url).await {
        Ok(bytes) if input_detect::is_png_bytes(&bytes) => {
            let extraction = png_extract::extract(&bytes);
            if let Ok(ext) = extraction {
                if let Some(workflow) = ext.workflow {
                    tracing::info!(image_id, "civitai: using workflow from PNG tEXt chunk");
                    // Dump a snippet of the workflow for debugging
                    if let Some(obj) = workflow.as_object() {
                        for (nid, node) in obj {
                            if let Some(ct) = node.get("class_type").and_then(|v| v.as_str()) {
                                if ct == "CheckpointLoaderSimple" {
                                    let ckpt = node.get("inputs").and_then(|i| i.get("ckpt_name"));
                                    tracing::info!(
                                        image_id, node_id = nid, ckpt_value = ?ckpt,
                                        "civitai: PNG workflow checkpoint value"
                                    );
                                }
                            }
                        }
                    }
                    return Ok((workflow, Some(source), preview_url, Some(civitai_bundle)));
                }
            }
            // PNG but no workflow — fall through to synthesis
            tracing::debug!(image_id, "CivitAI PNG has no embedded workflow");
        }
        Ok(_) => {
            tracing::debug!(image_id, "CivitAI image is JPEG — no embedded workflow possible");
        }
        Err(e) => {
            warnings.push(Warning {
                warning_type: "download".into(),
                message: format!("Could not download original image: {e}"),
            });
        }
    }

    // No workflow from PNG — synthesize from CivitAI generation metadata
    if let Some(ref gen_meta) = civitai_bundle.generation {
        if !gen_meta.prompt.is_empty() || gen_meta.model_name.is_some() {
            let params = gen_data_parse::GenerationParams {
                prompt: gen_meta.prompt.clone(),
                negative_prompt: gen_meta.negative_prompt.clone(),
                steps: gen_meta.steps,
                cfg_scale: gen_meta.cfg_scale,
                sampler: gen_meta.sampler.clone(),
                seed: gen_meta.seed,
                model: gen_meta.model_name.clone(),
                width: gen_meta.width,
                height: gen_meta.height,
                clip_skip: gen_meta.clip_skip,
                extra: std::collections::HashMap::new(),
            };
            tracing::info!(
                image_id,
                model_name = ?params.model,
                "civitai: synthesizing workflow from generation metadata"
            );
            let workflow = workflow_synth::synthesize_txt2img(&params);
            warnings.push(Warning {
                warning_type: "synthesized".into(),
                message: "No embedded workflow found. Synthesized from CivitAI generation metadata.".into(),
            });
            return Ok((workflow, Some(source), preview_url, Some(civitai_bundle)));
        }
    }

    // No workflow, no generation data — but we may have model version IDs.
    // Resolve the version IDs and synthesize a workflow from the resources.
    if !civitai_bundle.model_version_ids.is_empty() {
        let mut resources = Vec::new();
        for vid in &civitai_bundle.model_version_ids {
            if let Some(resolved) = civitai::resolve_model_version(civitai, *vid).await {
                resources.push(workflow_synth::ResolvedResource {
                    filename: resolved.filename,
                    model_type: resolved.model_type, // CivitAI types: "Checkpoint", "LORA", etc.
                    weight: None, // no weight info without generation data
                });
            }
        }

        if resources.iter().any(|r| r.model_type == "Checkpoint") {
            tracing::info!(
                image_id,
                resource_count = resources.len(),
                "civitai: synthesizing workflow from resource list (no generation data)"
            );
            let workflow = workflow_synth::synthesize_from_resources(&resources);
            warnings.push(Warning {
                warning_type: "synthesized".into(),
                message: "No generation parameters found. Synthesized a template workflow from identified resources. Add your prompt and adjust parameters.".into(),
            });
            return Ok((workflow, Some(source), preview_url, Some(civitai_bundle)));
        }
    }

    anyhow::bail!("CivitAI image has no usable generation data or identifiable checkpoint model.")
}

async fn extract_from_civitai_model(
    civitai: &civitai::CivitaiClient,
    model_id: u64,
    version_id: Option<u64>,
    warnings: &mut Vec<Warning>,
) -> Result<ExtractionResult> {
    let meta = civitai::fetch_model_page(civitai, model_id, version_id).await?;

    tracing::info!(
        model_id,
        version_id = meta.version_id,
        model_name = %meta.model_name,
        model_type = %meta.model_type,
        file_name = %meta.file_name,
        resources = meta.resource_version_ids.len(),
        "civitai model: fetched page metadata"
    );

    let source = Source {
        source_type: "civitai".into(),
        url: format!("https://civitai.com/models/{model_id}?modelVersionId={}", meta.version_id),
        image_id: None,
        username: None,
    };
    let preview_url = meta.preview_url.clone();

    // Build the CivitAI meta bundle for model resolution.
    // For non-workflow types, include this model's own version ID for resolution.
    let mut version_ids = meta.resource_version_ids.clone();
    if !version_ids.contains(&meta.version_id) {
        version_ids.push(meta.version_id);
    }
    let civitai_bundle = CivitaiMetaBundle {
        model_version_ids: version_ids,
        generation: meta.generation,
    };

    let is_workflow_type = meta.model_type == "Workflows";

    // ── Path A: Workflow-type models — download and extract the JSON ──
    if is_workflow_type {
        let workflow = civitai::download_workflow(civitai, &meta.download_url).await;
        match workflow {
            Ok(wf) if input_detect::is_comfyui_workflow(&wf) => {
                tracing::info!(model_id, "civitai model: using downloaded workflow JSON");
                return Ok((wf, Some(source), preview_url, Some(civitai_bundle)));
            }
            Ok(_) => {
                warnings.push(Warning {
                    warning_type: "workflow_format".into(),
                    message: "Downloaded file is not a ComfyUI API-format workflow.".into(),
                });
            }
            Err(e) => {
                warnings.push(Warning {
                    warning_type: "download".into(),
                    message: format!("Could not download workflow file: {e}"),
                });
            }
        }
    }

    // ── Path B: Model-type (Checkpoint, LoRA, etc.) — synthesize a workflow ──
    // Resolve this model to get the filename, then build a workflow around it.
    if !is_workflow_type {
        let resolved = civitai::resolve_model_version(civitai, meta.version_id).await;

        if let Some(resolved) = resolved {
            tracing::info!(
                model_id,
                filename = %resolved.filename,
                resolved_type = %resolved.model_type,
                "civitai model: resolved model file, synthesizing workflow"
            );

            let is_lora = matches!(resolved.model_type.to_lowercase().as_str(), "lora" | "locon" | "lycoris");
            let is_checkpoint = matches!(resolved.model_type.to_lowercase().as_str(), "checkpoint");

            // Build generation params from example images if available
            let gen_params = civitai_bundle.generation.as_ref().map(|g| {
                gen_data_parse::GenerationParams {
                    prompt: g.prompt.clone(),
                    negative_prompt: g.negative_prompt.clone(),
                    steps: g.steps,
                    cfg_scale: g.cfg_scale,
                    sampler: g.sampler.clone(),
                    seed: g.seed,
                    model: if is_checkpoint { Some(resolved.filename.clone()) } else { g.model_name.clone() },
                    width: g.width,
                    height: g.height,
                    clip_skip: g.clip_skip,
                    extra: std::collections::HashMap::new(),
                }
            }).unwrap_or_else(|| gen_data_parse::GenerationParams {
                model: if is_checkpoint { Some(resolved.filename.clone()) } else { None },
                ..Default::default()
            });

            let workflow = if is_lora {
                // LoRA: synthesize txt2img with LoRA wired in
                let weight = 1.0; // default weight
                workflow_synth::synthesize_txt2img_with_lora(&gen_params, &resolved.filename, weight)
            } else {
                // Checkpoint or other: standard txt2img
                workflow_synth::synthesize_txt2img(&gen_params)
            };

            let what = if is_lora { "LoRA" } else if is_checkpoint { "Checkpoint" } else { &resolved.model_type };
            warnings.push(Warning {
                warning_type: "synthesized".into(),
                message: format!(
                    "This is a {what} model, not a workflow. Synthesized a txt2img template using '{}'.",
                    resolved.filename
                ),
            });

            return Ok((workflow, Some(source), preview_url, Some(civitai_bundle)));
        }
    }

    // ── Fallback: synthesize from generation metadata ──
    if let Some(ref gen_meta) = civitai_bundle.generation {
        if !gen_meta.prompt.is_empty() || gen_meta.model_name.is_some() {
            let params = gen_data_parse::GenerationParams {
                prompt: gen_meta.prompt.clone(),
                negative_prompt: gen_meta.negative_prompt.clone(),
                steps: gen_meta.steps,
                cfg_scale: gen_meta.cfg_scale,
                sampler: gen_meta.sampler.clone(),
                seed: gen_meta.seed,
                model: gen_meta.model_name.clone(),
                width: gen_meta.width,
                height: gen_meta.height,
                clip_skip: gen_meta.clip_skip,
                extra: std::collections::HashMap::new(),
            };
            let workflow = workflow_synth::synthesize_txt2img(&params);
            warnings.push(Warning {
                warning_type: "synthesized".into(),
                message: "Synthesized workflow from model page generation metadata.".into(),
            });
            return Ok((workflow, Some(source), preview_url, Some(civitai_bundle)));
        }
    }

    anyhow::bail!(
        "CivitAI model '{}' — could not extract or synthesize a workflow.",
        meta.model_name
    )
}

async fn extract_from_url(
    http: &Client,
    url: &str,
    warnings: &mut Vec<Warning>,
) -> Result<ExtractionResult> {
    let resp = http
        .get(url)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .with_context(|| format!("failed to download: {url}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("download returned HTTP {} for {url}", resp.status());
    }
    let bytes = resp.bytes().await.context("read response bytes")?.to_vec();

    if input_detect::is_png_bytes(&bytes) {
        let extraction = png_extract::extract(&bytes)?;
        if let Some(workflow) = extraction.workflow {
            let source = Source {
                source_type: "url".into(),
                url: url.to_string(),
                image_id: None,
                username: None,
            };
            return Ok((workflow, Some(source), Some(url.to_string()), None));
        }

        // PNG with parameters but no workflow
        if let Some(params_text) = extraction.parameters_text {
            let params = gen_data_parse::parse(&params_text);
            let workflow = workflow_synth::synthesize_txt2img(&params);
            warnings.push(Warning {
                warning_type: "synthesized".into(),
                message: "PNG has no ComfyUI workflow. Synthesized from embedded parameters.".into(),
            });
            return Ok((workflow, None, None, None));
        }

        anyhow::bail!("PNG at URL has no embedded workflow or generation parameters");
    }

    // Try as JSON
    let text = std::str::from_utf8(&bytes).context("URL returned non-PNG, non-UTF-8 content")?;
    let json: serde_json::Value = serde_json::from_str(text)
        .context("URL returned content that is not PNG or valid JSON")?;

    if input_detect::is_comfyui_workflow(&json) {
        return Ok((json, None, None, None));
    }

    anyhow::bail!("URL returned JSON that does not look like a ComfyUI workflow")
}

// ── Helpers ───────────────────────────────────────────────────

fn detect_inputs(parsed: &crate::skills::parser::ParsedWorkflow) -> Vec<DetectedInput> {
    use crate::skills::parser::InputKind;
    let mut inputs = Vec::new();

    for input in &parsed.inputs {
        let content_type = match input.kind {
            InputKind::Image => "image",
            InputKind::Text => "text",
        };
        let role = match input.kind {
            InputKind::Image => {
                if inputs.iter().any(|i: &DetectedInput| i.role == "source") {
                    "mask"
                } else {
                    "source"
                }
            }
            InputKind::Text => {
                if inputs.iter().any(|i: &DetectedInput| i.role == "prompt") {
                    "negative"
                } else {
                    "prompt"
                }
            }
        };

        inputs.push(DetectedInput {
            role: role.to_string(),
            content_type: content_type.to_string(),
            node_id: input.node_id.clone(),
            placeholder: input.placeholder.clone(),
        });
    }

    inputs
}

/// Update mapping defaults/options with resolved filenames (which include extensions).
///
/// Handles three cases:
/// 1. Bare name without extension (e.g., `aMixIllustrious_aMix` → `aMixIllustrious_aMix.safetensors`)
/// 2. Garbage value like a node ID or link reference (e.g., `"3"`) → replaced with first resolved model
/// 3. Correct filename already → left unchanged
fn reconcile_model_mappings(
    mut mappings: Vec<crate::domain::skill::SkillMapping>,
    original_pairs: &[(String, String)],
    resolved: &[model_resolve::ModelResolution],
) -> Vec<crate::domain::skill::SkillMapping> {
    // Build a map: bare name (without extension) → resolved full filename
    let mut bare_to_full: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for resolution in resolved {
        let full = resolution.filename().to_string();
        let bare = full.rsplit_once('.').map(|(b, _)| b).unwrap_or(&full);
        bare_to_full.insert(bare.to_string(), full.clone());
    }
    for (name, _model_type) in original_pairs {
        // Only add valid model filenames — skip garbage like "3"
        if !is_valid_model_filename(name) {
            continue;
        }
        let bare = name.rsplit_once('.').map(|(b, _)| b).unwrap_or(name);
        if !bare_to_full.contains_key(bare) {
            bare_to_full.insert(bare.to_string(), name.clone());
        }
    }

    // Build a map: param field name → model_type → first resolved filename
    // Used as fallback when the value is garbage (node ID, link ref, etc.)
    let field_to_model_type: std::collections::HashMap<&str, &str> = [
        ("checkpoint", "checkpoints"),
        ("lora", "loras"),
        ("upscale_model", "upscale_models"),
        ("vae", "vae"),
    ].into_iter().collect();

    for mapping in &mut mappings {
        let crate::domain::skill::SkillMapping::Param {
            field,
            default,
            param_type,
            ..
        } = mapping else { continue };

        // Only fix model-related fields
        let Some(expected_model_type) = field_to_model_type.get(field.as_str()) else {
            continue;
        };

        // Find the fallback: first resolved model matching this model_type
        let fallback = resolved.iter().find_map(|m| {
            match m {
                model_resolve::ModelResolution::Resolved { filename, model_type, .. }
                | model_resolve::ModelResolution::Cached { filename, model_type }
                | model_resolve::ModelResolution::AuthRequired { filename, model_type, .. }
                    if model_type == *expected_model_type => Some(filename.clone()),
                _ => None,
            }
        });

        // Fix default value
        if let Some(serde_json::Value::String(val)) = default {
            if !is_valid_model_filename(val) {
                // Try bare-name lookup first, then fallback to first resolved model
                if let Some(full) = bare_to_full.get(val.as_str()).or(fallback.as_ref()) {
                    *val = full.clone();
                }
            }
        } else if default.is_none() {
            // No default at all — set to resolved model if we have one
            if let Some(full) = &fallback {
                *default = Some(serde_json::Value::String(full.clone()));
            }
        }

        // Fix options values
        if let crate::domain::skill::ParamType::Options { options } = param_type {
            for opt in options.iter_mut() {
                if let serde_json::Value::String(val) = &mut opt.value {
                    if !is_valid_model_filename(val) {
                        if let Some(full) = bare_to_full.get(val.as_str()).or(fallback.as_ref()) {
                            *val = full.clone();
                        }
                    }
                }
            }
        }
    }

    mappings
}

/// A valid model filename contains a dot (extension) and is longer than a few characters.
/// Rejects garbage values like node IDs ("3"), link references, or empty strings.
fn is_valid_model_filename(val: &str) -> bool {
    val.contains('.') && val.len() > 4
}

fn generate_moniker(
    _source: &Option<Source>,
    parsed: &crate::skills::parser::ParsedWorkflow,
    models: &[model_resolve::ModelResolution],
    generation: &Option<GenerationSummary>,
) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Try to derive a name from the primary model
    let model_name = models.iter().find_map(|m| {
        if let model_resolve::ModelResolution::Resolved { display_name: Some(name), .. } = m {
            // Take the model name part (before " / version")
            Some(name.split('/').next().unwrap_or(name).trim().to_string())
        } else {
            None
        }
    });

    // Try generation model name
    let gen_model = generation.as_ref().and_then(|g| g.model.clone());

    // Pick the best name source
    let base = model_name
        .or(gen_model)
        .unwrap_or_else(|| {
            // Derive from workflow type
            let has_upscale = parsed.nodes.values().any(|n| n.class_type.contains("Upscale"));
            let has_ksampler = parsed.nodes.values().any(|n| n.class_type.contains("KSampler"));
            let has_inpaint = parsed.nodes.values().any(|n| n.class_type.contains("Inpaint"));
            let has_lora = parsed.nodes.values().any(|n| n.class_type.contains("Lora"));

            if has_inpaint { "inpaint" }
            else if has_upscale && !has_ksampler { "upscale" }
            else if has_lora { "generate-lora" }
            else if has_ksampler { "generate" }
            else { "workflow" }
            .to_string()
        });

    let slug = sanitize_moniker(&base);

    // Add a short numeric suffix for uniqueness
    let short_ts = ts % 100000;
    format!("{slug}-{short_ts}")
}

/// Sanitize a string for use as a filesystem-safe moniker.
fn sanitize_moniker(input: &str) -> String {
    let slug: String = input
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse multiple hyphens, trim, truncate
    let mut result = String::new();
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen && !result.is_empty() {
                result.push('-');
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }

    result.trim_matches('-').chars().take(40).collect()
}

fn humanize_moniker(moniker: &str) -> String {
    moniker
        .replace('-', " ")
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().to_string() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
