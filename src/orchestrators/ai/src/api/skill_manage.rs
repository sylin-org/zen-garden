//! Skill management API — CRUD + analyze for provider-scoped skills (ORCH-0023).
//!
//! Routes under `/v1/services/{provider}/skills/...`

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::app_state::AppState;
use crate::skills::import::{analyze, draft_builder, model_resolve};

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
    let manager_registry = model_resolve::ManagerRegistry::fetch(&http).await;

    // Get CivitAI API token from secrets store (enables restricted image access)
    let civitai_token = state.secrets.get("civitai").await;

    let result = analyze::run(
        &http,
        &query.t,
        None,
        std::path::Path::new(&state.data_dir),
        &manager_registry,
        civitai_token.as_deref(),
    )
    .await;

    match result {
        Ok(analysis) => {
            // Create draft skill on disk
            let skills_dir = std::path::Path::new(&state.data_dir).join("skills");
            if let Err(e) = draft_builder::create_draft(&skills_dir, &provider, &analysis).await {
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "draft_creation_failed",
                    &format!("Failed to create draft skill: {e}"),
                );
            }

            // Spawn AI naming in the background (ORCH-0026).
            // Updates skill.json + emits SSE event when done.
            spawn_background_naming(
                state.clone(),
                provider.clone(),
                analysis.moniker.clone(),
                &analysis,
            );

            (StatusCode::OK, Json(&analysis)).into_response()
        }
        Err(e) => error_response(
            StatusCode::BAD_REQUEST,
            "analyze_failed",
            &e.to_string(),
        ),
    }
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

// ── GET /v1/services/{provider}/skills/{moniker}/workflows ─────

/// List workflow files in the skill directory.
pub async fn list_workflows(
    State(state): State<AppState>,
    Path((provider, moniker)): Path<(String, String)>,
) -> Response {
    let skill_dir = std::path::Path::new(&state.data_dir)
        .join("skills")
        .join(&provider)
        .join(&moniker);

    let mut files = Vec::new();

    // Read the skill.json to find the default_workflow
    let default_wf = tokio::fs::read_to_string(skill_dir.join("skill.json"))
        .await
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("default_workflow").and_then(|d| d.as_str()).map(String::from))
        .unwrap_or_default();

    if let Ok(mut entries) = tokio::fs::read_dir(&skill_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".json") && name != "skill.json" {
                let stem = name.trim_end_matches(".json").to_string();
                files.push(serde_json::json!({
                    "name": stem,
                    "filename": name,
                    "is_default": stem == default_wf,
                }));
            }
        }
    }

    files.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    (StatusCode::OK, Json(files)).into_response()
}

// ── GET /v1/services/{provider}/skills/{moniker}/workflows/{name} ──

/// Read a workflow file's content.
pub async fn get_workflow(
    State(state): State<AppState>,
    Path((provider, moniker, wf_name)): Path<(String, String, String)>,
) -> Response {
    let path = std::path::Path::new(&state.data_dir)
        .join("skills")
        .join(&provider)
        .join(&moniker)
        .join(format!("{wf_name}.json"));

    match tokio::fs::read_to_string(&path).await {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(json) => (StatusCode::OK, Json(json)).into_response(),
            Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "parse_error", &e.to_string()),
        },
        Err(_) => error_response(StatusCode::NOT_FOUND, "not_found", &format!("Workflow '{wf_name}' not found")),
    }
}

// ── PUT /v1/services/{provider}/skills/{moniker}/workflows/{name} ──

/// Write a workflow file's content.
pub async fn put_workflow(
    State(state): State<AppState>,
    Path((provider, moniker, wf_name)): Path<(String, String, String)>,
    Json(content): Json<serde_json::Value>,
) -> Response {
    let path = std::path::Path::new(&state.data_dir)
        .join("skills")
        .join(&provider)
        .join(&moniker)
        .join(format!("{wf_name}.json"));

    let json_str = match serde_json::to_string_pretty(&content) {
        Ok(s) => s,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, "serialize_error", &e.to_string()),
    };

    match tokio::fs::write(&path, json_str).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "status": "saved", "name": wf_name }))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, "write_error", &e.to_string()),
    }
}

// ── Background Naming (ORCH-0026) ────────────────────────────

/// Spawn AI-assisted naming in the background.
///
/// Calls the garden's chat model to generate a proper name/description,
/// then updates skill.json on disk and emits an SSE event so the
/// dashboard can update the UI live.
fn spawn_background_naming(
    state: AppState,
    provider: String,
    moniker: String,
    analysis: &analyze::AnalyzeResult,
) {
    use crate::skills::import::namer;

    // Build naming context from the analysis
    let naming_ctx = namer::NamingContext {
        prompt: analysis.generation.as_ref().map(|g| g.prompt.clone()).unwrap_or_default(),
        negative_prompt: analysis.generation.as_ref().map(|g| g.negative_prompt.clone()).unwrap_or_default(),
        model_names: analysis.models.iter().map(|m| {
            let kind = match m {
                crate::skills::import::model_resolve::ModelResolution::Resolved { model_type, .. } => model_type.as_str(),
                _ => "model",
            };
            format!("{} ({})", m.filename(), kind)
        }).collect(),
        steps: analysis.generation.as_ref().and_then(|g| g.steps),
        cfg_scale: analysis.generation.as_ref().and_then(|g| g.cfg_scale),
        sampler: analysis.generation.as_ref().and_then(|g| g.sampler.clone()),
        width: None,
        height: None,
    };

    // Skip if no meaningful context to name from
    if naming_ctx.prompt.is_empty() && naming_ctx.model_names.is_empty() {
        return;
    }

    let skill_name = format!("{}.{}", analysis.capability, moniker);

    tokio::spawn(async move {
        let naming = match namer::generate_name(&state.http, &naming_ctx).await {
            Some(n) => n,
            None => return, // AI naming unavailable — heuristic stands
        };

        // Update skill.json on disk
        let skill_path = std::path::Path::new(&state.data_dir)
            .join("skills")
            .join(&provider)
            .join(&moniker)
            .join("skill.json");

        if let Ok(json_str) = tokio::fs::read_to_string(&skill_path).await {
            if let Ok(mut skill) = serde_json::from_str::<serde_json::Value>(&json_str) {
                skill["display_name"] = serde_json::Value::String(naming.name.clone());
                skill["description"] = serde_json::Value::String(naming.description.clone());

                if let Ok(updated) = serde_json::to_string_pretty(&skill) {
                    let _ = tokio::fs::write(&skill_path, updated).await;
                }
            }
        }

        // Emit SSE event so the dashboard updates live
        let _ = state.dashboard_tx.send(crate::app_state::DashboardEvent {
            event_type: "skill.named".to_string(),
            data: serde_json::json!({
                "skill": skill_name,
                "moniker": moniker,
                "provider": provider,
                "display_name": naming.name,
                "description": naming.description,
            }).to_string(),
        });

        tracing::info!(
            skill = %skill_name,
            name = %naming.name,
            "background naming complete"
        );
    });
}

// ── Helpers ───────────────────────────────────────────────────

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": { "code": code, "message": message, "status": status.as_u16() }
    });
    (status, Json(body)).into_response()
}
