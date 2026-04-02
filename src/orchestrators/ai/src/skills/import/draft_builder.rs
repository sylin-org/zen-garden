//! Draft builder — write a draft skill.json + workflow to disk.

use std::path::Path;
use anyhow::{Context, Result};
use tokio::fs;

use super::analyze::AnalyzeResult;
use super::model_resolve::ModelResolution;

/// Create a draft skill directory from an analysis result.
pub async fn create_draft(
    skills_dir: &Path,
    provider: &str,
    result: &AnalyzeResult,
) -> Result<std::path::PathBuf> {
    let skill_dir = skills_dir.join(provider).join(&result.moniker);
    fs::create_dir_all(&skill_dir)
        .await
        .with_context(|| format!("create skill dir: {}", skill_dir.display()))?;

    // Build skill.json — use extracted mappings and content slots
    let content_slots: Vec<serde_json::Value> = result
        .content_slots
        .iter()
        .map(|s| {
            let mut v = serde_json::json!({
                "role": s.role,
                "content_type": s.content_type,
                "required": s.required,
            });
            if let Some(ref overlay) = s.overlay {
                v.as_object_mut().unwrap().insert("overlay".into(), serde_json::json!(overlay));
            }
            if let Some(ref default) = s.default {
                v.as_object_mut().unwrap().insert("default".into(), serde_json::json!(default));
            }
            v
        })
        .collect();

    let mappings: Vec<serde_json::Value> = result
        .mappings
        .iter()
        .map(|m| serde_json::to_value(m).unwrap_or_default())
        .collect();

    let required_models: Vec<serde_json::Value> = result
        .models
        .iter()
        .map(|m| match m {
            ModelResolution::Resolved { filename, url, sha256, size_bytes, model_type, display_name, license, .. } => {
                serde_json::json!({
                    "filename": filename,
                    "model_type": model_type,
                    "url": url,
                    "size_bytes": size_bytes,
                    "sha256": sha256,
                    "license": license,
                    "description": display_name,
                })
            }
            ModelResolution::Cached { filename, model_type } => {
                serde_json::json!({
                    "filename": filename,
                    "model_type": model_type,
                })
            }
            ModelResolution::Unresolved { filename, model_type, .. } => {
                serde_json::json!({
                    "filename": filename,
                    "model_type": model_type,
                })
            }
        })
        .collect();

    // Build a meaningful description from the generation prompt
    let description = if let Some(ref gen_summary) = result.generation {
        if !gen_summary.prompt.is_empty() {
            gen_summary.prompt.clone()
        } else if let Some(ref model) = gen_summary.model {
            format!("Generated with {model}")
        } else {
            format!("Imported: {}", result.display_name)
        }
    } else {
        format!("Imported: {}", result.display_name)
    };

    let skill_json = serde_json::json!({
        "version": 1,
        "draft": true,
        "name": format!("{}.{}", result.capability, result.moniker),
        "display_name": result.display_name,
        "capability": result.capability,
        "description": description,
        "provider_kind": "comfy_ui",
        "vram_mb": 4096,
        "default_workflow": "workflow",
        "content_slots": content_slots,
        "mappings": mappings,
        "required_models": required_models,
        "source": result.source,
        "preview_url": result.preview_url,
        "diagram": result.diagram,
    });

    let json_str = serde_json::to_string_pretty(&skill_json)
        .context("serialize skill.json")?;
    fs::write(skill_dir.join("skill.json"), json_str)
        .await
        .context("write skill.json")?;

    // Write workflow template
    let workflow_str = serde_json::to_string_pretty(&result.workflow)
        .context("serialize workflow")?;
    fs::write(skill_dir.join("workflow.json"), workflow_str)
        .await
        .context("write workflow.json")?;

    tracing::info!(
        moniker = %result.moniker,
        models = result.models.len(),
        inputs = result.inputs.len(),
        warnings = result.warnings.len(),
        "draft skill created"
    );

    Ok(skill_dir)
}
