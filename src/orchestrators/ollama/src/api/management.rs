//! Model management API endpoints.
//!
//! These are called by the dashboard UI for multi-stone model operations.
//! Pull and delete operations now create background jobs and return immediately.

use crate::app_state::AppState;
use crate::domain::types::{JobKind, JobStatus};
use crate::infra::ollama_client::OllamaClient;
use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Clone)]
pub struct ManagementState {
    pub app: AppState,
    pub client: OllamaClient,
}

#[derive(Deserialize)]
pub struct PullRequest {
    pub model: String,
    /// Target instance endpoints. If empty, use VRAM-feasible instances.
    #[serde(default)]
    pub targets: Vec<String>,
}

#[derive(Deserialize)]
pub struct DeleteRequest {
    pub model: String,
    /// Target instance endpoints. If empty, delete from all.
    #[serde(default)]
    pub targets: Vec<String>,
}

/// `POST /api/management/pull` — pull a model to one or more instances.
///
/// Creates a background job and returns immediately with the job ID.
pub async fn pull_model(
    State(state): State<ManagementState>,
    Json(req): Json<PullRequest>,
) -> impl IntoResponse {
    let targets = if req.targets.is_empty() {
        let instances = state.app.instances.read().await;
        instances
            .values()
            .filter(|i| i.health.is_routable())
            .map(|i| i.endpoint.clone())
            .collect::<Vec<_>>()
    } else {
        req.targets.clone()
    };

    if targets.is_empty() {
        return Json(json!({"error": "no healthy instances available"})).into_response();
    }

    let job_id = state
        .app
        .create_job(JobKind::ModelPull {
            model: req.model.clone(),
            targets: targets.clone(),
        })
        .await;

    // Spawn background work
    let response_id = job_id.clone();
    let app = state.app.clone();
    let client = state.client.clone();
    let model = req.model.clone();
    tokio::spawn(async move {
        app.update_job(&job_id, JobStatus::Running, None).await;

        let mut results = Vec::new();
        for target in &targets {
            app.update_job(
                &job_id,
                JobStatus::Running,
                Some(format!("pulling to {target}")),
            )
            .await;

            match pull_and_wait(&client, target, &model).await {
                Ok(status) => {
                    results.push((target.clone(), status == "success"));
                    if let Ok((avail, loaded, infos, _)) = client.full_profile(target).await {
                        app.update_instance_models(target, avail, loaded).await;
                        for info in infos {
                            app.upsert_model(info).await;
                        }
                    }
                }
                Err(e) => {
                    results.push((target.clone(), false));
                    tracing::warn!(model = %model, target = %target, error = %e, "pull failed");
                }
            }
        }

        let successes = results.iter().filter(|(_, ok)| *ok).count();
        if successes == results.len() {
            app.complete_job(&job_id).await;
        } else if successes > 0 {
            app.complete_job(&job_id).await;
        } else {
            app.fail_job(&job_id, "pull failed on all instances").await;
        }
        app.emit_event("models.updated", "{}").await;
    });

    Json(json!({"job_id": response_id, "status": "queued"})).into_response()
}

/// `POST /api/management/delete` — delete a model from instances.
///
/// Creates a background job and returns immediately with the job ID.
pub async fn delete_model(
    State(state): State<ManagementState>,
    Json(req): Json<DeleteRequest>,
) -> impl IntoResponse {
    let targets = if req.targets.is_empty() {
        let instances = state.app.instances.read().await;
        instances
            .values()
            .filter(|i| {
                i.health.is_routable() && i.models_available.iter().any(|m| m == &req.model)
            })
            .map(|i| i.endpoint.clone())
            .collect::<Vec<_>>()
    } else {
        req.targets.clone()
    };

    if targets.is_empty() {
        return Json(json!({"error": "model not found on any instance"})).into_response();
    }

    let job_id = state
        .app
        .create_job(JobKind::ModelDelete {
            model: req.model.clone(),
            targets: targets.clone(),
        })
        .await;

    let response_id = job_id.clone();
    let app = state.app.clone();
    let client = state.client.clone();
    let model = req.model.clone();
    tokio::spawn(async move {
        app.update_job(&job_id, JobStatus::Running, None).await;

        let mut any_failure = false;
        for target in &targets {
            match client.delete_model(target, &model).await {
                Ok(()) => {
                    // Update instance registry
                    let instances = app.instances.read().await;
                    if let Some(inst) = instances.get(target.as_str()) {
                        let new_available: Vec<String> = inst
                            .models_available
                            .iter()
                            .filter(|m| m.as_str() != model)
                            .cloned()
                            .collect();
                        let new_loaded: Vec<crate::domain::types::LoadedModel> = inst
                            .models_loaded
                            .iter()
                            .filter(|m| m.name != model)
                            .cloned()
                            .collect();
                        drop(instances);
                        app.update_instance_models(target, new_available, new_loaded)
                            .await;
                    }
                }
                Err(e) => {
                    tracing::warn!(model = %model, target = %target, error = %e, "delete failed");
                    any_failure = true;
                }
            }
        }

        // If no instances have the model anymore, remove from global registry
        {
            let instances = app.instances.read().await;
            let still_exists = instances
                .values()
                .any(|i| i.models_available.iter().any(|m| m == &model));
            if !still_exists {
                app.remove_model(&model).await;
            }
        }

        if any_failure {
            app.fail_job(&job_id, "delete failed on some instances")
                .await;
        } else {
            app.complete_job(&job_id).await;
        }
        app.emit_event("models.updated", "{}").await;
    });

    Json(json!({"job_id": response_id, "status": "queued"})).into_response()
}

/// `GET /api/management/feasibility?model=<name>` — pre-flight VRAM feasibility check.
pub async fn check_feasibility(
    State(state): State<ManagementState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let model = match params.get("model") {
        Some(m) => m,
        None => return Json(json!({"error": "model parameter required"})),
    };

    let instances = state.app.instances.read().await;
    let models = state.app.models.read().await;

    let vram_needed = models
        .get(model.as_str())
        .map(|m| m.vram_estimate_bytes)
        .unwrap_or(0);

    let feasible: Vec<serde_json::Value> = instances
        .values()
        .filter(|i| i.health.is_routable())
        .map(|i| {
            let fits = vram_needed == 0 || i.vram_budget_bytes >= vram_needed;
            let has_it = i.models_available.iter().any(|m| m == model);
            json!({
                "stone_name": i.stone_name,
                "endpoint": i.endpoint,
                "vram_budget_mb": i.vram_budget_bytes / 1_048_576,
                "fits": fits,
                "already_has": has_it,
            })
        })
        .collect();

    Json(json!({
        "model": model,
        "vram_needed_mb": vram_needed / 1_048_576,
        "instances": feasible,
    }))
}

/// Pull a model and consume the stream, returning the last status string.
async fn pull_and_wait(
    client: &OllamaClient,
    endpoint: &str,
    model: &str,
) -> anyhow::Result<String> {
    use futures_util::StreamExt;
    let mut stream = client.pull_model(endpoint, model).await?;
    let mut last_status = String::new();
    while let Some(chunk) = stream.next().await {
        if let Ok(bytes) = chunk {
            if let Ok(progress) =
                serde_json::from_slice::<crate::domain::types::OllamaPullProgress>(&bytes)
            {
                last_status = progress.status;
            }
        }
    }
    Ok(last_status)
}
