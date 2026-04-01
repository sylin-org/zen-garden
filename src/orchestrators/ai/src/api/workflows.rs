//! Skill API handlers — `/v1/{capability}/skill/{moniker}` and `/v1/jobs/...` on port 7190.
//!
//! Capability-namespaced skill invocation (ORCH-0018):
//! - `POST /v1/{capability}/skill/{moniker}` → invoke a skill, get a job ID
//! - `GET /v1/jobs/{id}` → poll job status + result
//! - `GET /v1/jobs/{id}/assets/{filename}` → retrieve output asset
//! - `GET /v1/skills` → list registered skills
//! - `GET /v1/skills/{skill}/form` → mappings + diagram for TryIt UI

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::app_state::AppState;
use crate::catalog::traits::ProviderContext;
use crate::domain::skill::{SkillFormView, WorkflowJobStatus, WorkflowRequest};
use crate::domain::types::Capability;

// ── POST /v1/{capability}/skill/{moniker} ─────────────────────

/// Invoke a skill. Capability and moniker come from the URL path.
/// The request body carries content + parameters only — no routing fields.
pub async fn invoke_skill(
    State(state): State<AppState>,
    Path((capability, moniker)): Path<(String, String)>,
    Json(mut req): Json<WorkflowRequest>,
) -> Response {
    // 1. Validate capability
    let cap = Capability::ALL
        .iter()
        .find(|c| c.as_str() == capability);
    if cap.is_none() {
        return error_response(
            StatusCode::NOT_FOUND,
            "unknown_capability",
            &format!("Unknown capability '{capability}'. Valid: {}", capability_list()),
        );
    }

    // 2. Build dotted skill name and fill the request
    let skill_name = format!("{capability}.{moniker}");
    req.skill = skill_name.clone();

    // 3. Look up the skill
    let skill_def = match state.skills.get_skill(&skill_name).await {
        Some(def) => def,
        None => {
            return error_response(
                StatusCode::NOT_FOUND,
                "skill_not_found",
                &format!("Unknown skill '{moniker}' in capability '{capability}'."),
            );
        }
    };

    // 4. Check availability (ORCH-0021: readiness is per-instance)
    {
        let snap = state.skills.snapshot().clone();
        let available = snap
            .skills
            .iter()
            .find(|v| v.definition.name == skill_name)
            .map(|v| v.available)
            .unwrap_or(false);
        if !available {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "skill_not_ready",
                &format!(
                    "Skill '{}' has no ready instances. Try again shortly.",
                    skill_def.display_name
                ),
            );
        }
    }

    // 5. Find a healthy instance for this provider kind
    let reg_snap = state.registry.snapshot().clone();
    let endpoint = reg_snap
        .instances
        .values()
        .find(|inst| inst.kind == skill_def.provider_kind && inst.health.is_routable())
        .map(|inst| inst.endpoint.clone());

    let endpoint = match endpoint {
        Some(ep) => ep,
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "no_instances",
                &format!("No healthy {} instances available", skill_def.provider_kind),
            );
        }
    };

    // 6. Build context and dispatch
    let ctx = ProviderContext {
        endpoint,
        model: None,
        api_key: None,
    };

    let provider = match state.providers.get(skill_def.provider_kind) {
        Some(p) => p.clone(),
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "provider_not_found",
                &format!("{} provider not registered", skill_def.provider_kind),
            );
        }
    };

    match provider.workflow(&ctx, req).await {
        Ok(mut job) => {
            // 7. Assign GUIDv7 as public ID, rewrite asset URLs
            let public_id = garden_common::utils::ids::generate_guidv7();
            job.id = public_id.clone();

            // Rewrite content URLs from ComfyUI endpoint to orchestrator asset path
            if let Some(content) = &mut job.content {
                for block in content.iter_mut() {
                    if let Some(url) = &block.url {
                        if let Some(filename) = extract_filename_from_comfyui_url(url) {
                            block.url = Some(format!("/v1/jobs/{public_id}/assets/{filename}"));
                        }
                    }
                }
            }

            state.skills.submit_job(job.clone()).await;

            let status_code = match job.status {
                WorkflowJobStatus::Completed => StatusCode::OK,
                _ => StatusCode::ACCEPTED,
            };

            (status_code, Json(&job)).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, skill = %skill_name, "skill execution failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "execution_failed",
                &e.to_string(),
            )
        }
    }
}

// ── GET /v1/jobs/{id} ─────────────────────────────────────────

/// Poll job status and retrieve results.
pub async fn get_job(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Response {
    match state.skills.get_job(&job_id).await {
        Some(job) => (StatusCode::OK, Json(&job)).into_response(),
        None => error_response(
            StatusCode::NOT_FOUND,
            "job_not_found",
            &format!("No job with ID '{job_id}'"),
        ),
    }
}

// ── GET /v1/jobs/{id}/assets/{filename} ───────────────────────

/// Proxy an output asset from the provider instance.
/// Decouples clients from knowing the internal ComfyUI endpoint.
pub async fn get_job_asset(
    State(state): State<AppState>,
    Path((job_id, filename)): Path<(String, String)>,
) -> Response {
    let job = match state.skills.get_job(&job_id).await {
        Some(j) => j,
        None => {
            return error_response(
                StatusCode::NOT_FOUND,
                "job_not_found",
                &format!("No job with ID '{job_id}'"),
            );
        }
    };

    let endpoint = match &job.endpoint {
        Some(ep) => ep.clone(),
        None => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "no_endpoint",
                "Job has no associated endpoint for asset retrieval",
            );
        }
    };

    // Proxy from ComfyUI's /view endpoint
    let proxy_url = format!(
        "{endpoint}/view?filename={filename}&type=output&subfolder="
    );

    let http = reqwest::Client::new();
    match http
        .get(&proxy_url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();

            match resp.bytes().await {
                Ok(bytes) => axum::response::Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", content_type)
                    .header("content-length", bytes.len())
                    .body(axum::body::Body::from(bytes))
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
                Err(e) => error_response(
                    StatusCode::BAD_GATEWAY,
                    "asset_read_failed",
                    &format!("Failed to read asset from provider: {e}"),
                ),
            }
        }
        Ok(resp) => error_response(
            StatusCode::BAD_GATEWAY,
            "asset_fetch_failed",
            &format!("Provider returned HTTP {}", resp.status()),
        ),
        Err(e) => error_response(
            StatusCode::BAD_GATEWAY,
            "asset_unreachable",
            &format!("Cannot reach provider: {e}"),
        ),
    }
}

// ── GET /v1/skills ────────────────────────────────────────────

/// List all registered skills with computed availability.
pub async fn list_skills(State(state): State<AppState>) -> Response {
    let snap = state.skills.snapshot().clone();
    (StatusCode::OK, Json(&*snap.skills)).into_response()
}

// ── GET /v1/skills/{skill}/form ───────────────────────────────

/// Return mappings + diagram for a skill's TryIt UI.
pub async fn skill_form(
    State(state): State<AppState>,
    Path(skill): Path<String>,
) -> Response {
    match state.skills.get_skill(&skill).await {
        Some(def) => {
            let view = SkillFormView::from_definition(&def);
            (StatusCode::OK, Json(view)).into_response()
        }
        None => error_response(
            StatusCode::NOT_FOUND,
            "skill_not_found",
            &format!("Unknown skill '{skill}'"),
        ),
    }
}

// ── Helpers ───────────────────────────────────────────────────

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "code": code,
            "message": message,
            "status": status.as_u16(),
        }
    });
    (status, Json(body)).into_response()
}

/// Extract the `filename` query parameter from a ComfyUI `/view?filename=...` URL.
fn extract_filename_from_comfyui_url(url: &str) -> Option<String> {
    url.split("filename=")
        .nth(1)
        .map(|s| s.split('&').next().unwrap_or(s).to_string())
}

fn capability_list() -> String {
    Capability::ALL
        .iter()
        .map(|c| c.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
