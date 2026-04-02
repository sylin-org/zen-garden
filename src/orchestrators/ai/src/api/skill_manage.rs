//! Skill management API — CRUD + analyze for provider-scoped skills (ORCH-0023).
//!
//! Routes under `/v1/services/{provider}/skills/...`

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::app_state::AppState;
use crate::skills::import::analyze;
use crate::skills::import::model_resolve::ManagerRegistry;

// ── GET /v1/services/{provider}/skills/analyze?t={input} ──────

#[derive(Debug, serde::Deserialize)]
pub struct AnalyzeQuery {
    /// The input to analyze: CivitAI URL, PNG URL, or raw JSON.
    pub t: String,
}

/// Smart import: detect input type, extract workflow, resolve models, create draft.
pub async fn analyze_skill(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(query): Query<AnalyzeQuery>,
) -> Response {
    if provider != "comfyui" {
        return error_response(
            StatusCode::NOT_FOUND,
            "unsupported_provider",
            &format!("Skill management not supported for provider '{provider}'"),
        );
    }

    let http = reqwest::Client::new();

    // Load the ComfyUI Manager registry (TODO: cache this in AppState)
    let manager_registry = ManagerRegistry::fetch(&http).await;

    let result = analyze::analyze_input(
        &http,
        &query.t,
        None,
        std::path::Path::new(&state.data_dir),
        &manager_registry,
    )
    .await;

    match result {
        Ok(analysis) => {
            // Create draft skill on disk
            let skills_dir = std::path::Path::new(&state.data_dir)
                .join("skills")
                .join(&provider)
                .join(&analysis.moniker);

            if let Err(e) = create_draft_skill(&skills_dir, &analysis).await {
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "draft_creation_failed",
                    &format!("Failed to create draft skill: {e}"),
                );
            }

            (StatusCode::OK, Json(serde_json::json!({
                "moniker": analysis.moniker,
                "display_name": analysis.display_name,
                "models": analysis.models,
                "inputs": analysis.inputs,
                "diagram": analysis.diagram,
                "source": analysis.source,
                "preview_url": analysis.preview_url,
                "warnings": analysis.warnings,
            }))).into_response()
        }
        Err(e) => error_response(
            StatusCode::BAD_REQUEST,
            "analyze_failed",
            &e.to_string(),
        ),
    }
}

/// Create a draft skill directory on disk from the analysis result.
async fn create_draft_skill(
    skill_dir: &std::path::Path,
    analysis: &analyze::AnalyzeResult,
) -> anyhow::Result<()> {
    use tokio::fs;

    fs::create_dir_all(skill_dir).await?;

    // Build skill.json
    let skill_json = serde_json::json!({
        "version": 1,
        "draft": true,
        "name": format!("image.{}", analysis.moniker),
        "display_name": analysis.display_name,
        "capability": "image",
        "description": format!("Imported skill: {}", analysis.display_name),
        "provider_kind": "comfy_ui",
        "vram_mb": 4096,
        "default_workflow": "workflow",
        "content_slots": analysis.inputs.iter().map(|i| serde_json::json!({
            "role": i.role,
            "content_type": i.content_type,
            "required": true,
        })).collect::<Vec<_>>(),
        "mappings": build_mappings(&analysis.inputs),
        "required_models": analysis.models.iter().filter_map(|m| {
            match m {
                crate::skills::import::model_resolve::ModelResolution::Resolved {
                    filename, url, sha256, size_bytes, ..
                } => Some(serde_json::json!({
                    "filename": filename,
                    "model_type": "checkpoints",
                    "url": url,
                    "size_bytes": size_bytes,
                    "sha256": sha256,
                })),
                crate::skills::import::model_resolve::ModelResolution::Cached { filename } => {
                    Some(serde_json::json!({
                        "filename": filename,
                        "model_type": "checkpoints",
                    }))
                }
                crate::skills::import::model_resolve::ModelResolution::Unresolved { filename, .. } => {
                    Some(serde_json::json!({
                        "filename": filename,
                        "model_type": "checkpoints",
                    }))
                }
            }
        }).collect::<Vec<_>>(),
        "source": analysis.source,
    });

    let skill_json_str = serde_json::to_string_pretty(&skill_json)?;
    fs::write(skill_dir.join("skill.json"), skill_json_str).await?;

    // Write the workflow template
    let workflow_str = serde_json::to_string_pretty(&analysis.workflow)?;
    fs::write(skill_dir.join("workflow.json"), workflow_str).await?;

    tracing::info!(
        moniker = %analysis.moniker,
        models = analysis.models.len(),
        inputs = analysis.inputs.len(),
        "created draft skill"
    );

    Ok(())
}

/// Build content + param mappings from detected inputs.
fn build_mappings(inputs: &[analyze::DetectedInput]) -> Vec<serde_json::Value> {
    inputs.iter().map(|i| {
        serde_json::json!({
            "type": "content",
            "role": i.role,
            "content_type": i.content_type,
            "placeholder": i.placeholder,
        })
    }).collect()
}

// ── GET /v1/services/{provider}/skills ─────────────────────────

/// List all skills for a provider (including drafts).
pub async fn list_skills(
    State(state): State<AppState>,
    Path(provider): Path<String>,
) -> Response {
    let skills_dir = std::path::Path::new(&state.data_dir)
        .join("skills")
        .join(&provider);

    let mut skills = Vec::new();

    if let Ok(mut entries) = tokio::fs::read_dir(&skills_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if !entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let skill_path = entry.path().join("skill.json");
            if let Ok(json_str) = tokio::fs::read_to_string(&skill_path).await {
                if let Ok(skill) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    skills.push(skill);
                }
            }
        }
    }

    (StatusCode::OK, Json(skills)).into_response()
}

// ── GET /v1/services/{provider}/skills/new ─────────────────────

/// Return an empty skill scaffold.
pub async fn new_skill(
    Path(provider): Path<String>,
) -> Response {
    let scaffold = serde_json::json!({
        "version": 1,
        "draft": true,
        "name": "",
        "display_name": "",
        "capability": "image",
        "description": "",
        "provider_kind": provider,
        "vram_mb": 4096,
        "default_workflow": "workflow",
        "content_slots": [],
        "mappings": [],
        "required_models": [],
    });

    (StatusCode::OK, Json(scaffold)).into_response()
}

// ── GET /v1/services/{provider}/skills/{moniker} ───────────────

/// Get a skill's data (skill.json content).
pub async fn get_skill(
    State(state): State<AppState>,
    Path((provider, moniker)): Path<(String, String)>,
) -> Response {
    let skill_path = std::path::Path::new(&state.data_dir)
        .join("skills")
        .join(&provider)
        .join(&moniker)
        .join("skill.json");

    match tokio::fs::read_to_string(&skill_path).await {
        Ok(json_str) => match serde_json::from_str::<serde_json::Value>(&json_str) {
            Ok(skill) => (StatusCode::OK, Json(skill)).into_response(),
            Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "parse_error", &e.to_string()),
        },
        Err(_) => error_response(StatusCode::NOT_FOUND, "not_found", &format!("Skill '{moniker}' not found")),
    }
}

// ── POST /v1/services/{provider}/skills/{moniker} ──────────────

/// Upsert a skill. Clears draft flag if present.
pub async fn upsert_skill(
    State(state): State<AppState>,
    Path((provider, moniker)): Path<(String, String)>,
    Json(mut skill): Json<serde_json::Value>,
) -> Response {
    let skill_dir = std::path::Path::new(&state.data_dir)
        .join("skills")
        .join(&provider)
        .join(&moniker);

    // Clear draft flag on save
    if let Some(obj) = skill.as_object_mut() {
        obj.remove("draft");
    }

    // TODO: validation (1h)

    if let Err(e) = tokio::fs::create_dir_all(&skill_dir).await {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "io_error", &e.to_string());
    }

    let json_str = match serde_json::to_string_pretty(&skill) {
        Ok(s) => s,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, "serialize_error", &e.to_string()),
    };

    match tokio::fs::write(skill_dir.join("skill.json"), json_str).await {
        Ok(()) => {
            tracing::info!(provider = %provider, moniker = %moniker, "skill saved (published)");
            (StatusCode::OK, Json(serde_json::json!({ "status": "saved", "moniker": moniker }))).into_response()
        }
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "write_error", &e.to_string()),
    }
}

// ── DELETE /v1/services/{provider}/skills/{moniker} ────────────

/// Delete a skill directory.
pub async fn delete_skill(
    State(state): State<AppState>,
    Path((provider, moniker)): Path<(String, String)>,
) -> Response {
    let skill_dir = std::path::Path::new(&state.data_dir)
        .join("skills")
        .join(&provider)
        .join(&moniker);

    if !skill_dir.exists() {
        return error_response(StatusCode::NOT_FOUND, "not_found", &format!("Skill '{moniker}' not found"));
    }

    match tokio::fs::remove_dir_all(&skill_dir).await {
        Ok(()) => {
            tracing::info!(provider = %provider, moniker = %moniker, "skill deleted");
            (StatusCode::OK, Json(serde_json::json!({ "status": "deleted", "moniker": moniker }))).into_response()
        }
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "delete_error", &e.to_string()),
    }
}

// ── Helpers ───────────────────────────────────────────────────

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": { "code": code, "message": message, "status": status.as_u16() }
    });
    (status, Json(body)).into_response()
}
