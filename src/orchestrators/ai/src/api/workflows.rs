//! Workflow API handlers — `/v1/workflows/...` and `/v1/skills/...` on port 7190.
//!
//! Skill-based operations that execute asynchronous workflows:
//! - `POST /v1/workflows/run` → submit a workflow, get a job ID
//! - `GET /v1/workflows/jobs/{id}` → poll job status + result
//! - `GET /v1/skills` → list registered skills
//! - `GET /v1/skills/{skill}/form` → schema + diagram for TryIt UI

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::app_state::AppState;
use crate::catalog::traits::ProviderContext;
use crate::domain::skill::{SkillPresentation, WorkflowJobStatus, WorkflowRequest};

// ── POST /v1/workflows/run ─────────────────────────────────────

/// Submit a workflow for execution. Returns a job ID for polling.
pub async fn run_workflow(
    State(state): State<AppState>,
    Json(req): Json<WorkflowRequest>,
) -> Response {
    // 1. Look up the skill and check readiness
    {
        match state.skills.get_skill(&req.skill).await {
            None => {
                return error_response(
                    StatusCode::NOT_FOUND,
                    "skill_not_found",
                    &format!("Unknown skill '{}'. Use GET /v1/skills to list available skills.", req.skill),
                );
            }
            Some(skill) => {
                // Check availability via the snapshot (ORCH-0021: readiness is per-instance, not on definition)
                let snap = state.skills.snapshot().clone();
                let available = snap.skills.iter()
                    .find(|v| v.definition.name == req.skill)
                    .map(|v| v.available)
                    .unwrap_or(false);
                if !available {
                    return error_response(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "skill_not_ready",
                        &format!("Skill '{}' has no ready instances. Try again shortly.", skill.display_name),
                    );
                }
            }
        }
    }

    // 2. Find a ComfyUI instance that can serve this skill
    let reg_snap = state.registry.snapshot().clone();
    let endpoint = reg_snap
        .instances
        .values()
        .find(|inst| {
            inst.kind == crate::domain::types::OfferingKind::ComfyUi
                && inst.health.is_routable()
        })
        .map(|inst| inst.endpoint.clone());

    let endpoint = match endpoint {
        Some(ep) => ep,
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "no_instances",
                "No healthy ComfyUI instances available",
            );
        }
    };

    // 3. Build provider context
    let ctx = ProviderContext {
        endpoint,
        model: None,
        api_key: None,
    };

    // 4. Dispatch to provider
    let provider = match state
        .providers
        .get(crate::domain::types::OfferingKind::ComfyUi)
    {
        Some(p) => p.clone(),
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "provider_not_found",
                "ComfyUI provider not registered",
            );
        }
    };

    match provider.workflow(&ctx, req).await {
        Ok(job) => {
            // Store the job for polling
            state.skills.submit_job(job.clone()).await;

            let status_code = match job.status {
                WorkflowJobStatus::Completed => StatusCode::OK,
                _ => StatusCode::ACCEPTED,
            };

            (status_code, Json(&job)).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "workflow execution failed");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "workflow_failed",
                &e.to_string(),
            )
        }
    }
}

// ── GET /v1/workflows/jobs/{id} ────────────────────────────────

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
            &format!("No workflow job with ID '{}'", job_id),
        ),
    }
}

// ── GET /v1/skills ─────────────────────────────────────────────

/// List all registered skills with computed availability.
pub async fn list_skills(State(state): State<AppState>) -> Response {
    let snap = state.skills.snapshot().clone();
    (StatusCode::OK, Json(&*snap.skills)).into_response()
}

// ── GET /v1/skills/{skill}/form ────────────────────────────────

/// Return the schema + diagram for a skill's TryIt UI.
pub async fn skill_form(
    State(state): State<AppState>,
    Path(skill): Path<String>,
) -> Response {
    match state.skills.get_skill(&skill).await {
        Some(def) => {
            let presentation = SkillPresentation::from_definition(&def);
            (StatusCode::OK, Json(presentation)).into_response()
        }
        None => error_response(
            StatusCode::NOT_FOUND,
            "skill_not_found",
            &format!("Unknown skill '{}'", skill),
        ),
    }
}

// ── Error helper ───────────────────────────────────────────────

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
