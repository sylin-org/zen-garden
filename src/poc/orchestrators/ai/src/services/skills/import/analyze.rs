//! Analyze orchestrator — the import pipeline coordinator
//! (ORCH-0029 Phase 3).
//!
//! Takes raw caller input (text or bytes), classifies it, fetches
//! the workflow from the appropriate source, runs the parser, walks
//! `param_extract` to produce typed bindings, resolves any model
//! dependencies through the 5-level cascade, reconciles the
//! extracted model_selector against the resolved filenames, and
//! returns an [`AnalyzeResult`] ready for the draft builder.
//!
//! Each step delegates to a focused sub-module. Failures in optional
//! steps (preview download, one failed CivitAI hash lookup) are
//! surfaced as `Warning`s on the result, not errors. The one hard
//! failure is an unresolvable checkpoint — a skill with no valid
//! checkpoint can never execute, and writing such a draft is a
//! footgun.

#![allow(dead_code)]

use std::path::Path;

use anyhow::{Context, Result};
use reqwest::Client;

use super::{
    civitai, gen_data_parse, input_detect, model_resolve, param_extract, png_extract, ui_to_api,
    workflow_parser, workflow_synth,
};
use crate::domain::primitive::Primitive;
use crate::services::skills::cache::{CachePaths, DependencyManifest};
use crate::services::skills::types::{Binding, ModelSelector, ParamOption, Variant};

// ── Result types ──────────────────────────────────────────────

/// Complete analysis output — the import pipeline's deliverable,
/// ready for the draft builder to write to disk.
#[derive(Debug, serde::Serialize)]
pub struct AnalyzeResult {
    pub moniker: String,
    pub display_name: String,
    pub description: String,
    /// The canonical primitive this skill targets. For now the
    /// import pipeline always produces `image.generate` skills;
    /// classification of edit/upscale/analyze targets from imported
    /// workflows is a future enhancement.
    pub primitive: Primitive,
    /// The workflow with `PLACEHOLDER_*` tokens inlined at the
    /// right locations.
    pub workflow: serde_json::Value,
    /// Typed bindings ready for the skill loader.
    pub bindings: Vec<Binding>,
    /// Optional typed model selector, hoisted from the workflow's
    /// checkpoint loader by `param_extract` and reconciled against
    /// the resolved filenames.
    pub model_selector: Option<ModelSelector>,
    /// Variants — empty for single-workflow imports. Multi-workflow
    /// import lands in a later phase.
    pub variants: Option<Vec<Variant>>,
    /// Resolved models ready for the provisioner.
    pub models: Vec<model_resolve::ModelResolution>,
    /// Import provenance, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
    /// Non-fatal issues surfaced during analysis.
    pub warnings: Vec<Warning>,
    /// Generation parameters (for the dashboard to display).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<GenerationSummary>,
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

// ── Pipeline entry point ─────────────────────────────────────

/// Run the full analysis pipeline.
///
/// Accepts either text input (`input`) or binary bytes
/// (`input_bytes`). Returns an `AnalyzeResult` or a descriptive
/// error.
pub async fn run(
    http: &Client,
    input: &str,
    input_bytes: Option<&[u8]>,
    data_dir: &Path,
    manager: &model_resolve::ManagerRegistry,
    civitai_token: Option<&str>,
) -> Result<AnalyzeResult> {
    let civitai_client = match civitai_token {
        Some(token) => civitai::CivitaiClient::with_token(http.clone(), token.to_string()),
        None => civitai::CivitaiClient::new(http.clone()),
    };
    let mut warnings = Vec::new();

    // ── Step 1: Detect input type and extract the workflow ─────
    let (workflow, source, preview_url, civitai_meta) = if let Some(bytes) = input_bytes {
        if input_detect::is_png_bytes(bytes) {
            extract_from_png(bytes, &mut warnings)?
        } else {
            let text = std::str::from_utf8(bytes)
                .context("binary input is not PNG or valid UTF-8")?;
            extract_from_text(&civitai_client, text, &mut warnings).await?
        }
    } else {
        extract_from_text(&civitai_client, input, &mut warnings).await?
    };

    // ── Step 2: Parse the workflow ─────────────────────────────
    let parsed = workflow_parser::parse_workflow(&workflow)
        .map_err(|e| anyhow::anyhow!("workflow parse failed: {e}"))?;

    // ── Step 3: Param extraction ───────────────────────────────
    //
    // Walks the parsed workflow, plants `PLACEHOLDER_*` tokens,
    // emits typed bindings + an optional `ExtractedModelSelector`.
    let extraction = param_extract::extract(&workflow);
    let workflow = extraction.workflow;
    let mut bindings = extraction.bindings;
    let extracted_selector = extraction.model_selector;

    // ── Step 4: Collect model dependencies ─────────────────────
    let model_pairs: Vec<(String, String)> = parsed
        .models
        .iter()
        .filter(|m| !m.is_placeholder)
        .map(|m| (m.model_name.clone(), m.model_type.clone()))
        .collect();

    // ── Step 5: Resolve models through the cascade ────────────
    let cache_paths = CachePaths::new(data_dir, "comfyui");
    let cache_manifest = DependencyManifest::load(&cache_paths.manifest_path).await;

    let mut civitai_models = Vec::new();
    if let Some(ref meta) = civitai_meta {
        for vid in &meta.model_version_ids {
            match civitai::resolve_model_version(&civitai_client, *vid).await {
                Some(resolved) => civitai_models.push(resolved),
                None => warnings.push(Warning {
                    warning_type: "model_resolution".into(),
                    message: format!("CivitAI model version {vid} could not be resolved"),
                }),
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
        manager: manager.clone(),
    };
    let models = model_resolve::resolve_all(&civitai_client, &model_pairs, &resolution_ctx).await;

    // ── Step 5b: Reconcile the model selector ──────────────────
    //
    // If the workflow had a checkpoint loader, `param_extract`
    // hoisted its current value into `extracted_selector`. We now
    // replace that with the resolved full filename from the
    // cascade. Bare names without extensions, garbage values like
    // `"3"`, or empty defaults get fixed here.
    let model_selector = reconcile_model_selector(extracted_selector, &model_pairs, &models);

    // ── Step 5c: Hard-fail if no checkpoint resolved ───────────
    //
    // A skill whose model_selector default is still a
    // `PLACEHOLDER_*` sentinel will fail at dispatch. Better to
    // bail now with a clear message than to ship a broken draft.
    if let Some(ref sel) = model_selector {
        if sel.default.starts_with("PLACEHOLDER") && models.is_empty() {
            anyhow::bail!(
                "Could not determine which checkpoint this workflow uses. \
                 The CivitAI metadata does not include model information. \
                 Try a different image from the same model, or paste the workflow JSON directly."
            );
        }
    }

    // ── Step 6: Gather generation metadata ─────────────────────
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

    // ── Step 7: Generate moniker, display name, description ────
    let moniker = generate_moniker(&source, &parsed, &models, &generation);
    let display_name = humanize_moniker(&moniker);
    let description = generation
        .as_ref()
        .map(|g| {
            if !g.prompt.is_empty() {
                g.prompt.clone()
            } else {
                format!("Imported: {display_name}")
            }
        })
        .unwrap_or_else(|| format!("Imported: {display_name}"));

    // Backfill prompt/negative defaults from the generation params
    // when the param_extract walk didn't capture them (e.g. because
    // the workflow had the text wired through a linked PrimitiveNode).
    backfill_bindings_from_generation(&mut bindings, generation.as_ref());

    Ok(AnalyzeResult {
        moniker,
        display_name,
        description,
        primitive: Primitive::ImageGenerate,
        workflow,
        bindings,
        model_selector,
        variants: None,
        models,
        source,
        preview_url,
        warnings,
        generation,
    })
}

// ── Extraction paths ──────────────────────────────────────────

type ExtractionResult = (
    serde_json::Value,
    Option<Source>,
    Option<String>,
    Option<CivitaiMetaBundle>,
);

struct CivitaiMetaBundle {
    model_version_ids: Vec<u64>,
    generation: Option<civitai::GenerationMeta>,
}

fn extract_from_png(bytes: &[u8], warnings: &mut Vec<Warning>) -> Result<ExtractionResult> {
    let extraction = png_extract::extract(bytes)?;

    if let Some(workflow) = extraction.workflow {
        let workflow = if ui_to_api::is_ui_format(&workflow) {
            ui_to_api::convert(&workflow)?
        } else {
            workflow
        };
        return Ok((workflow, None, None, None));
    }

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
    let input_type = input_detect::classify(input).map_err(|e| anyhow::anyhow!("{e}"))?;

    match input_type {
        input_detect::InputType::CivitaiImage { image_id } => {
            extract_from_civitai(civitai, image_id, warnings).await
        }
        input_detect::InputType::CivitaiModel { model_id, version_id } => {
            extract_from_civitai_model(civitai, model_id, version_id, warnings).await
        }
        input_detect::InputType::PngUrl { url }
        | input_detect::InputType::GenericUrl { url } => {
            extract_from_url(civitai.http(), &url, warnings).await
        }
        input_detect::InputType::WorkflowJson { json } => Ok((json, None, None, None)),
        input_detect::InputType::GenerationText { text } => {
            let params = gen_data_parse::parse(&text);
            let workflow = workflow_synth::synthesize_txt2img(&params);
            warnings.push(Warning {
                warning_type: "synthesized".into(),
                message:
                    "Synthesized a standard txt2img workflow from generation parameters.".into(),
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

    if let Some(generator) = civitai::is_unsupported_generator(meta.base_model.as_deref()) {
        anyhow::bail!(
            "This image was generated by {generator}, which cannot be imported as a ComfyUI skill."
        );
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

    // Try PNG extraction from the original image first.
    match civitai::download_original_image(civitai, &meta.image_url).await {
        Ok(bytes) if input_detect::is_png_bytes(&bytes) => {
            if let Ok(extraction) = png_extract::extract(&bytes) {
                if let Some(workflow) = extraction.workflow {
                    return Ok((workflow, Some(source), preview_url, Some(civitai_bundle)));
                }
            }
        }
        Ok(_) => {}
        Err(e) => warnings.push(Warning {
            warning_type: "download".into(),
            message: format!("Could not download original image: {e}"),
        }),
    }

    // Fall back to synthesizing from generation metadata.
    if let Some(ref gen_meta) = civitai_bundle.generation {
        let mut resolved_resources = Vec::new();
        for res in &gen_meta.civitai_resources {
            if let Some(resolved) =
                civitai::resolve_model_version(civitai, res.model_version_id).await
            {
                resolved_resources.push(workflow_synth::ResolvedResource {
                    filename: resolved.filename,
                    model_type: resolved.model_type,
                    weight: res.weight,
                });
            }
        }

        let model_name = gen_meta.model_name.clone().or_else(|| {
            resolved_resources
                .iter()
                .find(|r| r.model_type == "Checkpoint")
                .map(|r| r.filename.clone())
        });

        if !gen_meta.prompt.is_empty() || model_name.is_some() || !resolved_resources.is_empty() {
            let params = gen_data_parse::GenerationParams {
                prompt: gen_meta.prompt.clone(),
                negative_prompt: gen_meta.negative_prompt.clone(),
                steps: gen_meta.steps,
                cfg_scale: gen_meta.cfg_scale,
                sampler: gen_meta.sampler.clone(),
                seed: gen_meta.seed,
                model: model_name.clone(),
                width: gen_meta.width,
                height: gen_meta.height,
                clip_skip: gen_meta.clip_skip,
                extra: std::collections::HashMap::new(),
            };
            let loras: Vec<_> = resolved_resources
                .iter()
                .filter(|r| r.model_type == "LORA")
                .collect();
            let workflow = if !loras.is_empty() && model_name.is_some() {
                workflow_synth::synthesize_from_resources_with_params(
                    &resolved_resources,
                    Some(&params),
                )
            } else if loras.len() == 1 {
                workflow_synth::synthesize_txt2img_with_lora(
                    &params,
                    &loras[0].filename,
                    loras[0].weight.unwrap_or(1.0),
                )
            } else {
                workflow_synth::synthesize_txt2img(&params)
            };
            warnings.push(Warning {
                warning_type: "synthesized".into(),
                message:
                    "No embedded workflow found. Synthesized from CivitAI generation metadata."
                        .into(),
            });
            return Ok((workflow, Some(source), preview_url, Some(civitai_bundle)));
        }
    }

    // No generation data at all — try resource list.
    if !civitai_bundle.model_version_ids.is_empty() {
        let mut resources = Vec::new();
        for vid in &civitai_bundle.model_version_ids {
            if let Some(resolved) = civitai::resolve_model_version(civitai, *vid).await {
                resources.push(workflow_synth::ResolvedResource {
                    filename: resolved.filename,
                    model_type: resolved.model_type,
                    weight: None,
                });
            }
        }
        if resources.iter().any(|r| r.model_type == "Checkpoint") {
            let workflow = workflow_synth::synthesize_from_resources(&resources);
            warnings.push(Warning {
                warning_type: "synthesized".into(),
                message:
                    "No generation parameters found. Synthesized a template workflow from identified resources."
                        .into(),
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

    let source = Source {
        source_type: "civitai".into(),
        url: format!(
            "https://civitai.com/models/{model_id}?modelVersionId={}",
            meta.version_id
        ),
        image_id: None,
        username: None,
    };
    let preview_url = meta.preview_url.clone();

    let mut version_ids = meta.resource_version_ids.clone();
    if !version_ids.contains(&meta.version_id) {
        version_ids.push(meta.version_id);
    }
    let civitai_bundle = CivitaiMetaBundle {
        model_version_ids: version_ids,
        generation: meta.generation,
    };

    let is_workflow_type = meta.model_type == "Workflows";

    // Path A: Workflow-type models — download the workflow file.
    if is_workflow_type {
        match civitai::download_workflow(civitai, &meta.download_url).await {
            Ok(wf) if input_detect::is_comfyui_workflow(&wf) => {
                return Ok((wf, Some(source), preview_url, Some(civitai_bundle)));
            }
            Ok(_) => warnings.push(Warning {
                warning_type: "workflow_format".into(),
                message: "Downloaded file is not a ComfyUI API-format workflow.".into(),
            }),
            Err(e) => warnings.push(Warning {
                warning_type: "download".into(),
                message: format!("Could not download workflow file: {e}"),
            }),
        }
    }

    // Path B: model-type (Checkpoint / LoRA / etc.) — synthesize.
    if !is_workflow_type {
        if let Some(resolved) = civitai::resolve_model_version(civitai, meta.version_id).await {
            let is_lora = matches!(
                resolved.model_type.to_lowercase().as_str(),
                "lora" | "locon" | "lycoris"
            );
            let is_checkpoint = resolved.model_type.to_lowercase() == "checkpoint";

            let gen_params = civitai_bundle
                .generation
                .as_ref()
                .map(|g| gen_data_parse::GenerationParams {
                    prompt: g.prompt.clone(),
                    negative_prompt: g.negative_prompt.clone(),
                    steps: g.steps,
                    cfg_scale: g.cfg_scale,
                    sampler: g.sampler.clone(),
                    seed: g.seed,
                    model: if is_checkpoint {
                        Some(resolved.filename.clone())
                    } else {
                        g.model_name.clone()
                    },
                    width: g.width,
                    height: g.height,
                    clip_skip: g.clip_skip,
                    extra: std::collections::HashMap::new(),
                })
                .unwrap_or_else(|| gen_data_parse::GenerationParams {
                    model: if is_checkpoint {
                        Some(resolved.filename.clone())
                    } else {
                        None
                    },
                    ..Default::default()
                });

            let workflow = if is_lora {
                workflow_synth::synthesize_txt2img_with_lora(&gen_params, &resolved.filename, 1.0)
            } else {
                workflow_synth::synthesize_txt2img(&gen_params)
            };

            let what = if is_lora {
                "LoRA"
            } else if is_checkpoint {
                "Checkpoint"
            } else {
                &resolved.model_type
            };
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
        .with_context(|| format!("download {url}"))?;
    if !resp.status().is_success() {
        anyhow::bail!("download returned HTTP {} for {url}", resp.status());
    }
    let bytes = resp.bytes().await.context("read response")?.to_vec();

    if input_detect::is_png_bytes(&bytes) {
        let extraction = png_extract::extract(&bytes)?;
        if let Some(workflow) = extraction.workflow {
            let source = Source {
                source_type: "url".into(),
                url: url.into(),
                image_id: None,
                username: None,
            };
            return Ok((workflow, Some(source), Some(url.into()), None));
        }
        if let Some(params_text) = extraction.parameters_text {
            let params = gen_data_parse::parse(&params_text);
            let workflow = workflow_synth::synthesize_txt2img(&params);
            warnings.push(Warning {
                warning_type: "synthesized".into(),
                message:
                    "PNG at URL has no embedded workflow. Synthesized from generation parameters."
                        .into(),
            });
            return Ok((workflow, None, None, None));
        }
        anyhow::bail!("PNG at URL has no embedded workflow or generation parameters");
    }

    // Try as JSON.
    let text = std::str::from_utf8(&bytes).context("URL returned non-PNG non-UTF-8 content")?;
    let json: serde_json::Value = serde_json::from_str(text)
        .context("URL returned content that is not PNG or valid JSON")?;
    if input_detect::is_comfyui_workflow(&json) {
        return Ok((json, None, None, None));
    }
    anyhow::bail!("URL returned JSON that does not look like a ComfyUI workflow")
}

// ── Reconciliation ───────────────────────────────────────────

fn reconcile_model_selector(
    extracted: Option<param_extract::ExtractedModelSelector>,
    original_pairs: &[(String, String)],
    resolved: &[model_resolve::ModelResolution],
) -> Option<ModelSelector> {
    let extracted = extracted?;

    // Bare-name → full lookup.
    let mut bare_to_full: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for resolution in resolved {
        let full = resolution.filename().to_string();
        let bare = full.rsplit_once('.').map(|(b, _)| b).unwrap_or(&full);
        bare_to_full.insert(bare.to_string(), full.clone());
    }
    for (name, _mt) in original_pairs {
        if !is_valid_model_filename(name) {
            continue;
        }
        let bare = name.rsplit_once('.').map(|(b, _)| b).unwrap_or(name);
        bare_to_full.entry(bare.to_string()).or_insert(name.clone());
    }

    // First checkpoint-type resolution, used as fallback when the
    // extracted default is garbage.
    let fallback = resolved.iter().find_map(|m| match m {
        model_resolve::ModelResolution::Resolved { filename, model_type, .. }
        | model_resolve::ModelResolution::Cached { filename, model_type }
        | model_resolve::ModelResolution::AuthRequired { filename, model_type, .. } => {
            if model_type == "checkpoints" || model_type == "Checkpoint" {
                Some(filename.clone())
            } else {
                None
            }
        }
        _ => None,
    });

    // Repair the default if invalid.
    let mut default = extracted.default.unwrap_or_default();
    if !is_valid_model_filename(&default) {
        if let Some(full) = bare_to_full.get(&default).or(fallback.as_ref()) {
            default = full.clone();
        } else if let Some(first) = resolved.first() {
            default = first.filename().to_string();
        }
    }

    // Rebuild options — repair any invalid values.
    let mut options: Vec<ParamOption> = extracted
        .options
        .into_iter()
        .map(|mut opt| {
            if let serde_json::Value::String(ref mut val) = opt.value {
                if !is_valid_model_filename(val) {
                    if let Some(full) = bare_to_full.get(val).or(fallback.as_ref()) {
                        *val = full.clone();
                    }
                }
            }
            opt
        })
        .collect();

    // Ensure the default is represented in options.
    if !options
        .iter()
        .any(|o| o.value.as_str() == Some(default.as_str()))
        && !default.is_empty()
    {
        options.push(ParamOption {
            value: serde_json::Value::String(default.clone()),
            label: None,
        });
    }

    if default.is_empty() {
        return None;
    }

    Some(ModelSelector {
        placeholder: extracted.placeholder,
        default,
        options,
    })
}

fn is_valid_model_filename(val: &str) -> bool {
    val.contains('.') && val.len() > 4
}

/// When the source image came with generation parameters, use those
/// as the skill's initial defaults for the prompt and negative
/// bindings so the operator sees something meaningful in the
/// dashboard preview.
fn backfill_bindings_from_generation(
    bindings: &mut [Binding],
    generation: Option<&GenerationSummary>,
) {
    use crate::domain::keys;
    let Some(generation) = generation else {
        return;
    };
    for b in bindings.iter_mut() {
        if b.default.is_some() {
            continue;
        }
        if b.field == keys::image::PROMPT_POSITIVE && !generation.prompt.is_empty() {
            b.default = Some(serde_json::Value::String(generation.prompt.clone()));
        }
        if b.field == keys::image::PROMPT_NEGATIVE && !generation.negative_prompt.is_empty() {
            b.default = Some(serde_json::Value::String(generation.negative_prompt.clone()));
        }
    }
}

// ── Moniker + display name ──────────────────────────────────

fn generate_moniker(
    _source: &Option<Source>,
    parsed: &workflow_parser::ParsedWorkflow,
    models: &[model_resolve::ModelResolution],
    generation: &Option<GenerationSummary>,
) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let model_name = models.iter().find_map(|m| {
        if let model_resolve::ModelResolution::Resolved {
            display_name: Some(name),
            ..
        } = m
        {
            Some(name.split('/').next().unwrap_or(name).trim().to_string())
        } else {
            None
        }
    });
    let gen_model = generation.as_ref().and_then(|g| g.model.clone());

    let base = model_name.or(gen_model).unwrap_or_else(|| {
        let has_upscale = parsed
            .nodes
            .values()
            .any(|n| n.class_type.contains("Upscale"));
        let has_ksampler = parsed
            .nodes
            .values()
            .any(|n| n.class_type.contains("KSampler"));
        let has_inpaint = parsed
            .nodes
            .values()
            .any(|n| n.class_type.contains("Inpaint"));
        let has_lora = parsed
            .nodes
            .values()
            .any(|n| n.class_type.contains("Lora"));

        if has_inpaint {
            "inpaint"
        } else if has_upscale && !has_ksampler {
            "upscale"
        } else if has_lora {
            "generate-lora"
        } else if has_ksampler {
            "generate"
        } else {
            "workflow"
        }
        .to_string()
    });

    let slug = sanitize_moniker(&base);
    let short_ts = ts % 100_000;
    format!("{slug}-{short_ts}")
}

fn sanitize_moniker(input: &str) -> String {
    let slug: String = input
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
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
