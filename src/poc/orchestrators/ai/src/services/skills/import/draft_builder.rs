//! Draft builder — write a v3 `skill.json` + workflow file to disk
//! from an `AnalyzeResult` (ORCH-0029 Phase 3).
//!
//! The draft is flagged `draft: true` so the loader skips it until
//! the operator reviews it in the dashboard and publishes via the
//! CRUD API (Phase 4). A companion `_debug.json` captures the
//! pipeline's intermediate state (resolved models, warnings,
//! source) for troubleshooting.

use std::path::Path;

use anyhow::{Context, Result};
use tokio::fs;

use super::analyze::AnalyzeResult;
use super::model_resolve::ModelResolution;

/// Create a draft skill directory from an analysis result. Writes
/// `skill.json` (v3), `workflow.json`, and `_debug.json` into
/// `{skills_dir}/{provider}/{moniker}/`.
pub async fn create_draft(
    skills_dir: &Path,
    provider: &str,
    result: &AnalyzeResult,
) -> Result<std::path::PathBuf> {
    let skill_dir = skills_dir.join(provider).join(&result.moniker);
    fs::create_dir_all(&skill_dir)
        .await
        .with_context(|| format!("create skill dir: {}", skill_dir.display()))?;

    // Convert the typed bindings back into JSON for the wire format.
    // This mirrors what the v3 loader deserializes at startup.
    let bindings: Vec<serde_json::Value> = result
        .bindings
        .iter()
        .map(binding_to_json)
        .collect();

    let model_selector_json = result.model_selector.as_ref().map(|sel| {
        serde_json::json!({
            "placeholder": sel.placeholder,
            "default": sel.default,
            "options": sel.options,
        })
    });

    let required_models: Vec<serde_json::Value> = result
        .models
        .iter()
        .map(|m| match m {
            ModelResolution::Resolved {
                filename,
                url,
                sha256,
                size_bytes,
                model_type,
                display_name,
                license,
                ..
            } => serde_json::json!({
                "filename": filename,
                "model_type": model_type,
                "url": url,
                "size_bytes": size_bytes,
                "sha256": sha256,
                "license": license,
                "description": display_name,
            }),
            ModelResolution::Cached { filename, model_type } => serde_json::json!({
                "filename": filename,
                "model_type": model_type,
            }),
            ModelResolution::AuthRequired {
                filename,
                url,
                model_type,
                secret_key,
                ..
            } => serde_json::json!({
                "filename": filename,
                "model_type": model_type,
                "url": url,
                "auth_required": true,
                "secret_key": secret_key,
            }),
            ModelResolution::Unresolved { filename, model_type, .. } => serde_json::json!({
                "filename": filename,
                "model_type": model_type,
            }),
        })
        .collect();

    let skill_json = serde_json::json!({
        "version": 3,
        "draft": true,
        "name": result.moniker,
        "display_name": result.display_name,
        "primitive": result.primitive.dotted(),
        "description": result.description,
        "vram_mb": 4096,
        "default_workflow": "workflow",
        "bindings": bindings,
        "model_selector": model_selector_json,
        "variants": result.variants,
        "required_models": required_models,
        "source": result.source,
        "preview_url": result.preview_url,
    });

    let json_str = serde_json::to_string_pretty(&skill_json).context("serialize skill.json")?;
    fs::write(skill_dir.join("skill.json"), json_str)
        .await
        .context("write skill.json")?;

    let workflow_str = serde_json::to_string_pretty(&result.workflow)
        .context("serialize workflow")?;
    fs::write(skill_dir.join("workflow.json"), workflow_str)
        .await
        .context("write workflow.json")?;

    // Debug dump for troubleshooting imports that land broken.
    let debug_dump = serde_json::json!({
        "moniker": result.moniker,
        "primitive": result.primitive.dotted(),
        "models": result.models.iter().map(|m| format!("{:?}", m)).collect::<Vec<_>>(),
        "warnings": result
            .warnings
            .iter()
            .map(|w| format!("{}: {}", w.warning_type, w.message))
            .collect::<Vec<_>>(),
        "source": result.source,
    });
    let debug_str = serde_json::to_string_pretty(&debug_dump).unwrap_or_default();
    let _ = fs::write(skill_dir.join("_debug.json"), debug_str).await;

    tracing::info!(
        moniker = %result.moniker,
        primitive = %result.primitive.dotted(),
        bindings = result.bindings.len(),
        models = result.models.len(),
        warnings = result.warnings.len(),
        "draft skill created"
    );

    Ok(skill_dir)
}

/// Convert an in-memory [`crate::services::skills::types::Binding`]
/// back into the v3 on-disk shape. This is the inverse of the
/// loader's binding parse.
fn binding_to_json(b: &crate::services::skills::types::Binding) -> serde_json::Value {
    use crate::services::skills::types::BindingTarget;

    let mut obj = serde_json::Map::new();
    obj.insert("field".into(), serde_json::Value::String(b.field.as_str().into()));

    match &b.target {
        BindingTarget::Placeholder(p) => {
            obj.insert("placeholder".into(), serde_json::Value::String(p.clone()));
        }
        BindingTarget::NodeInput { node, input } => {
            obj.insert("node".into(), serde_json::Value::String(node.clone()));
            obj.insert("input".into(), serde_json::Value::String(input.clone()));
        }
    }

    if let Some(ref default) = b.default {
        obj.insert("default".into(), default.clone());
    }
    if let Some(ref narrow) = b.narrow {
        if let Ok(v) = serde_json::to_value(narrow) {
            obj.insert("narrow".into(), v);
        }
    }
    if let Some(ref label) = b.label {
        obj.insert("label".into(), serde_json::Value::String(label.clone()));
    }
    if b.required {
        obj.insert("required".into(), serde_json::Value::Bool(true));
    }
    if let Some(delivery) = b.delivery {
        if let Ok(v) = serde_json::to_value(delivery) {
            obj.insert("delivery".into(), v);
        }
    }
    if !b.accepted_types.is_empty() {
        obj.insert(
            "accepted_types".into(),
            serde_json::Value::Array(
                b.accepted_types
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            ),
        );
    }
    if let Some(ref overlay) = b.overlay {
        obj.insert("overlay".into(), serde_json::Value::String(overlay.clone()));
    }

    serde_json::Value::Object(obj)
}
