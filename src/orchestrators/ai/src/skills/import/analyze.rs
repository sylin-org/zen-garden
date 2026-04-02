//! Analyze endpoint — detect input, extract workflow, resolve models, create draft.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use reqwest::Client;

use super::civitai;
use super::model_resolve::{self, ManagerRegistry, ModelResolution};
use super::png_extract;
use crate::skills::cache::{CachePaths, DependencyManifest};

/// Result of analyzing an input — ready to become a draft skill.
#[derive(Debug, serde::Serialize)]
pub struct AnalyzeResult {
    /// Auto-generated skill moniker.
    pub moniker: String,
    /// Auto-generated display name.
    pub display_name: String,
    /// The extracted ComfyUI API-format workflow.
    pub workflow: serde_json::Value,
    /// Parsed workflow info (from parser.rs).
    pub diagram: Option<String>,
    /// Detected model filenames with resolution status.
    pub models: Vec<ModelResolution>,
    /// Detected input nodes (images, text).
    pub inputs: Vec<DetectedInput>,
    /// Source tracking (if imported from URL).
    pub source: Option<AnalyzeSource>,
    /// Preview image URL (if available).
    pub preview_url: Option<String>,
    /// Warnings (e.g., missing custom nodes).
    pub warnings: Vec<AnalyzeWarning>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectedInput {
    pub role: String,
    pub content_type: String,
    pub node_id: String,
    pub placeholder: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AnalyzeSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_id: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AnalyzeWarning {
    #[serde(rename = "type")]
    pub warning_type: String,
    pub message: String,
}

/// Detect input type and run the full analysis pipeline.
pub async fn analyze_input(
    http: &Client,
    input: &str,
    input_bytes: Option<&[u8]>,
    data_dir: &Path,
    manager_registry: &ManagerRegistry,
) -> Result<AnalyzeResult> {
    // Detect input type and extract workflow
    let (workflow, source, preview_url) = if let Some(bytes) = input_bytes {
        // Binary upload — check if PNG
        if png_extract::is_png(bytes) {
            extract_from_png_bytes(bytes)?
        } else {
            // Try as JSON
            let json: serde_json::Value = serde_json::from_slice(bytes)
                .context("uploaded file is not a PNG or valid JSON")?;
            if png_extract::is_comfyui_workflow(&json) {
                (json, None, None)
            } else {
                anyhow::bail!("uploaded JSON does not look like a ComfyUI workflow");
            }
        }
    } else if !input.is_empty() {
        // Text input — could be URL or JSON
        if let Some(civitai_ref) = civitai::parse_civitai_url(input) {
            extract_from_civitai(http, civitai_ref).await?
        } else if input.starts_with("http://") || input.starts_with("https://") {
            extract_from_url(http, input).await?
        } else if input.trim_start().starts_with('{') {
            // Try as raw JSON
            let json: serde_json::Value = serde_json::from_str(input)
                .context("input looks like JSON but failed to parse")?;
            if png_extract::is_comfyui_workflow(&json) {
                (json, None, None)
            } else {
                anyhow::bail!("JSON does not look like a ComfyUI workflow");
            }
        } else {
            anyhow::bail!("unrecognized input — provide a CivitAI URL, PNG URL, or workflow JSON");
        }
    } else {
        anyhow::bail!("no input provided");
    };

    // Parse the workflow
    let parsed = crate::skills::parser::parse_workflow(&workflow)
        .map_err(|e| anyhow::anyhow!("failed to parse workflow: {e}"))?;

    // Extract model filenames and their types
    let model_filenames: Vec<String> = parsed.models.iter().map(|m| m.model_name.clone()).collect();
    let model_types: HashMap<String, String> = parsed
        .models
        .iter()
        .map(|m| (m.model_name.clone(), m.model_type.clone()))
        .collect();

    // Resolve models
    let cache_paths = CachePaths::new(data_dir, "comfyui");
    let manifest = DependencyManifest::load(&cache_paths.manifest_path).await;
    let civitai_resources = source
        .as_ref()
        .and_then(|s| s.image_id)
        .map(|_| Vec::new()) // TODO: pass resources from CivitAI metadata
        .unwrap_or_default();

    let models = model_resolve::resolve_models(
        http,
        &model_filenames,
        &model_types,
        &manifest,
        manager_registry,
        &civitai_resources,
    )
    .await;

    // Detect inputs (image placeholders, text prompts)
    let inputs = detect_inputs(&workflow, &parsed);

    // Generate moniker from source or workflow content
    let moniker = generate_moniker(&source, &parsed);
    let display_name = moniker
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
        .join(" ");

    Ok(AnalyzeResult {
        moniker,
        display_name,
        workflow,
        diagram: Some(parsed.diagram),
        models,
        inputs,
        source,
        preview_url,
        warnings: Vec::new(),
    })
}

/// Extract workflow from raw PNG bytes.
fn extract_from_png_bytes(
    bytes: &[u8],
) -> Result<(serde_json::Value, Option<AnalyzeSource>, Option<String>)> {
    let data = png_extract::extract_from_png(bytes)?;

    let workflow = data
        .prompt
        .with_context(|| "PNG has no embedded ComfyUI workflow (no 'prompt' tEXt chunk)")?;

    Ok((workflow, None, None))
}

/// Extract workflow from a CivitAI image URL.
async fn extract_from_civitai(
    http: &Client,
    civitai_ref: civitai::CivitaiImageRef,
) -> Result<(serde_json::Value, Option<AnalyzeSource>, Option<String>)> {
    let image = civitai::fetch_image_metadata(http, civitai_ref.image_id).await?;
    let preview_url = Some(image.url.clone());
    let png_bytes = civitai::download_image(http, &image.url).await?;

    let data = png_extract::extract_from_png(&png_bytes)?;

    let workflow = data
        .prompt
        .with_context(|| "CivitAI image has no embedded ComfyUI workflow")?;

    let source = AnalyzeSource {
        source_type: "civitai".into(),
        url: format!("https://civitai.com/images/{}", civitai_ref.image_id),
        image_id: Some(civitai_ref.image_id),
    };

    Ok((workflow, Some(source), preview_url))
}

/// Extract workflow from a direct PNG URL.
async fn extract_from_url(
    http: &Client,
    url: &str,
) -> Result<(serde_json::Value, Option<AnalyzeSource>, Option<String>)> {
    let bytes = civitai::download_image(http, url).await?;

    if png_extract::is_png(&bytes) {
        let data = png_extract::extract_from_png(&bytes)?;
        let workflow = data
            .prompt
            .with_context(|| "PNG has no embedded ComfyUI workflow")?;

        let source = AnalyzeSource {
            source_type: "url".into(),
            url: url.to_string(),
            image_id: None,
        };

        return Ok((workflow, Some(source), Some(url.to_string())));
    }

    // Try as JSON
    let json: serde_json::Value = serde_json::from_slice(&bytes)
        .context("URL did not return a PNG or valid JSON")?;

    if png_extract::is_comfyui_workflow(&json) {
        let source = AnalyzeSource {
            source_type: "url".into(),
            url: url.to_string(),
            image_id: None,
        };
        Ok((json, Some(source), None))
    } else {
        anyhow::bail!("URL returned JSON that is not a ComfyUI workflow");
    }
}

/// Detect input nodes from the workflow.
fn detect_inputs(
    _workflow: &serde_json::Value,
    parsed: &crate::skills::parser::ParsedWorkflow,
) -> Vec<DetectedInput> {
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
                    "mask".to_string()
                } else {
                    "source".to_string()
                }
            }
            InputKind::Text => "prompt".to_string(),
        };

        inputs.push(DetectedInput {
            role,
            content_type: content_type.to_string(),
            node_id: input.node_id.clone(),
            placeholder: input.placeholder.clone(),
        });
    }

    inputs
}

/// Generate a moniker from the source or workflow content.
fn generate_moniker(
    source: &Option<AnalyzeSource>,
    parsed: &crate::skills::parser::ParsedWorkflow,
) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if let Some(source) = source {
        if let Some(id) = source.image_id {
            return format!("imported-{id}");
        }
    }

    // Derive from node types
    let has_upscale = parsed.nodes.values().any(|n| n.class_type.contains("Upscale"));
    let has_ksampler = parsed.nodes.values().any(|n| n.class_type.contains("KSampler"));
    let has_inpaint = parsed.nodes.values().any(|n| n.class_type.contains("Inpaint"));

    let prefix = if has_inpaint {
        "imported-inpaint"
    } else if has_upscale && !has_ksampler {
        "imported-upscale"
    } else if has_ksampler {
        "imported-generate"
    } else {
        "imported-workflow"
    };

    format!("{prefix}-{timestamp}")
}
