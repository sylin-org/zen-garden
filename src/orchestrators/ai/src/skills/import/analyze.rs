//! Analyze orchestrator — detect input, fetch, extract, resolve, produce result.
//!
//! This is the pipeline coordinator. Each step delegates to a focused module.
//! Failures in optional steps (model resolution, preview) are warnings, not errors.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use reqwest::Client;

use super::{civitai, gen_data_parse, input_detect, model_resolve, param_extract, png_extract, workflow_synth};
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
    manager_registry: &model_resolve::ManagerRegistry,
) -> Result<AnalyzeResult> {
    let mut warnings = Vec::new();

    // ── Step 1: Detect input type and extract workflow ─────────
    let (workflow, source, preview_url, civitai_meta) = if let Some(bytes) = input_bytes {
        if input_detect::is_png_bytes(bytes) {
            extract_from_png(bytes, &mut warnings)?
        } else {
            // Try as text
            let text = std::str::from_utf8(bytes).context("binary input is not PNG or valid UTF-8")?;
            extract_from_text(http, text, &mut warnings).await?
        }
    } else {
        extract_from_text(http, input, &mut warnings).await?
    };

    // ── Step 2: Parse the workflow (for diagram + model detection) ──
    let parsed = crate::skills::parser::parse_workflow(&workflow)
        .map_err(|e| anyhow::anyhow!("workflow parse failed: {e}"))?;

    // ── Step 2b: Extract parameters + inject placeholders ─────
    // This walks the workflow, identifies tunable values, replaces them
    // with PLACEHOLDER_ tokens, and generates mappings.
    let extraction = param_extract::extract(&workflow);
    let workflow = extraction.workflow; // use the placeholder-injected version
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
            match civitai::resolve_model_version(http, *vid).await {
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

    let models = model_resolve::resolve_all(http, &model_pairs, &resolution_ctx).await;

    // ── Step 5: Detect inputs ─────────────────────────────────
    let inputs = detect_inputs(&parsed);

    // ── Step 6: Build metadata ────────────────────────────────
    let moniker = generate_moniker(&source, &parsed);
    let display_name = humanize_moniker(&moniker);

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
    http: &Client,
    input: &str,
    warnings: &mut Vec<Warning>,
) -> Result<ExtractionResult> {
    let input_type = input_detect::classify(input)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    match input_type {
        input_detect::InputType::CivitaiImage { image_id } => {
            extract_from_civitai(http, image_id, warnings).await
        }
        input_detect::InputType::PngUrl { url } => {
            extract_from_url(http, &url, warnings).await
        }
        input_detect::InputType::GenericUrl { url } => {
            extract_from_url(http, &url, warnings).await
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
    http: &Client,
    image_id: u64,
    warnings: &mut Vec<Warning>,
) -> Result<ExtractionResult> {
    let meta = civitai::fetch_image(http, image_id).await?;

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
    match civitai::download_original_image(http, &meta.image_url).await {
        Ok(bytes) if input_detect::is_png_bytes(&bytes) => {
            let extraction = png_extract::extract(&bytes);
            if let Ok(ext) = extraction {
                if let Some(workflow) = ext.workflow {
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
            let workflow = workflow_synth::synthesize_txt2img(&params);
            warnings.push(Warning {
                warning_type: "synthesized".into(),
                message: "No embedded workflow found. Synthesized from CivitAI generation metadata.".into(),
            });
            return Ok((workflow, Some(source), preview_url, Some(civitai_bundle)));
        }
    }

    // No workflow, no generation data — but we may still have model version IDs
    if !civitai_bundle.model_version_ids.is_empty() {
        anyhow::bail!(
            "CivitAI image has no generation data, but {} model(s) were identified. \
             Provide a workflow JSON manually to create a skill with these models.",
            civitai_bundle.model_version_ids.len()
        );
    }

    anyhow::bail!("CivitAI image has no generation data or model information.")
}

async fn extract_from_url(
    http: &Client,
    url: &str,
    warnings: &mut Vec<Warning>,
) -> Result<ExtractionResult> {
    let bytes = civitai::download_original_image(http, url).await?;

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

fn generate_moniker(source: &Option<Source>, parsed: &crate::skills::parser::ParsedWorkflow) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if let Some(source) = source {
        if let Some(id) = source.image_id {
            return format!("civitai-{id}");
        }
    }

    let has_upscale = parsed.nodes.values().any(|n| n.class_type.contains("Upscale"));
    let has_ksampler = parsed.nodes.values().any(|n| n.class_type.contains("KSampler"));
    let has_inpaint = parsed.nodes.values().any(|n| n.class_type.contains("Inpaint"));
    let has_lora = parsed.nodes.values().any(|n| n.class_type.contains("Lora"));

    let kind = if has_inpaint { "inpaint" }
        else if has_upscale && !has_ksampler { "upscale" }
        else if has_lora { "generate-lora" }
        else if has_ksampler { "generate" }
        else { "workflow" };

    format!("imported-{kind}-{ts}")
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
