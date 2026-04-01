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
        let registry = state.skill_registry.read().await;
        match registry.get(&req.skill) {
            None => {
                return error_response(
                    StatusCode::NOT_FOUND,
                    "skill_not_found",
                    &format!("Unknown skill '{}'. Use GET /v1/skills to list available skills.", req.skill),
                );
            }
            Some(skill) => {
                use crate::domain::skill::SkillStatus;
                match skill.status {
                    SkillStatus::Ready | SkillStatus::Degraded => {} // OK to proceed
                    SkillStatus::Initializing => {
                        return error_response(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "skill_initializing",
                            &format!("Skill '{}' is downloading required models. Try again shortly.", skill.display_name),
                        );
                    }
                    SkillStatus::Provisioning => {
                        return error_response(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "skill_provisioning",
                            &format!("Skill '{}' is being deployed to instances. Try again shortly.", skill.display_name),
                        );
                    }
                    SkillStatus::Failed => {
                        return error_response(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "skill_failed",
                            &format!("Skill '{}' failed to provision. Check orchestrator logs.", skill.display_name),
                        );
                    }
                }
            }
        }
    }

    // 2. Find a ComfyUI instance that can serve this skill
    let instances = state.instances.read().await;
    let candidate = instances.values().find(|inst| {
        inst.kind == crate::domain::types::OfferingKind::ComfyUi
            && inst.health.is_routable()
    });

    let endpoint = match candidate {
        Some(inst) => inst.endpoint.clone(),
        None => {
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "no_instances",
                "No healthy ComfyUI instances available",
            );
        }
    };
    drop(instances);

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
            {
                let mut jobs = state.workflow_jobs.write().await;
                jobs.insert(job.id.clone(), job.clone());
            }

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
    let jobs = state.workflow_jobs.read().await;
    match jobs.get(&job_id) {
        Some(job) => (StatusCode::OK, Json(job)).into_response(),
        None => error_response(
            StatusCode::NOT_FOUND,
            "job_not_found",
            &format!("No workflow job with ID '{}'", job_id),
        ),
    }
}

// ── GET /v1/skills ─────────────────────────────────────────────

/// List all registered skills.
pub async fn list_skills(State(state): State<AppState>) -> Response {
    let registry = state.skill_registry.read().await;
    let skills: Vec<_> = registry.list().iter().map(|s| {
        serde_json::json!({
            "name": s.name,
            "display_name": s.display_name,
            "capability": s.capability,
            "description": s.description,
            "status": s.status,
            "vram_mb": s.vram_mb,
            "content_slots": s.content_slots,
            "has_diagram": s.diagram.is_some(),
            "required_models": s.required_models,
        })
    }).collect();

    (StatusCode::OK, Json(skills)).into_response()
}

// ── GET /v1/skills/{skill}/form ────────────────────────────────

/// Return the schema + diagram for a skill's TryIt UI.
pub async fn skill_form(
    State(state): State<AppState>,
    Path(skill): Path<String>,
) -> Response {
    let registry = state.skill_registry.read().await;
    match registry.get(&skill) {
        Some(def) => {
            let presentation = SkillPresentation::from_definition(def);
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
