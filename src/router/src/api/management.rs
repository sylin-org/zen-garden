//! Model management API endpoints.
//!
//! These are called by the dashboard UI for multi-stone model operations.

use crate::app_state::AppState;
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
pub async fn pull_model(
    State(state): State<ManagementState>,
    Json(req): Json<PullRequest>,
) -> impl IntoResponse {
    let targets = if req.targets.is_empty() {
        // Select all healthy instances (VRAM feasibility check could be added)
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

    let mut results = Vec::new();
    for target in &targets {
        let stone_name = {
            let instances = state.app.instances.read().await;
            instances
                .get(target.as_str())
                .map(|i| i.stone_name.clone())
                .unwrap_or_else(|| target.clone())
        };

        match state.client.pull_model(target, &req.model).await {
            Ok(mut stream) => {
                use futures_util::StreamExt;
                let mut last_status = String::new();
                while let Some(chunk) = stream.next().await {
                    if let Ok(bytes) = chunk {
                        if let Ok(progress) =
                            serde_json::from_slice::<crate::domain::types::OllamaPullProgress>(
                                &bytes,
                            )
                        {
                            last_status = progress.status.clone();
                        }
                    }
                }
                results.push(json!({
                    "stone": stone_name,
                    "endpoint": target,
                    "status": last_status,
                    "success": last_status == "success",
                }));
            }
            Err(e) => {
                results.push(json!({
                    "stone": stone_name,
                    "endpoint": target,
                    "status": e.to_string(),
                    "success": false,
                }));
            }
        }
    }

    // Trigger a re-profile of affected instances
    for target in &targets {
        if let Ok((avail, loaded, infos, _)) = state.client.full_profile(target).await {
            state
                .app
                .update_instance_models(target, avail, loaded)
                .await;
            for info in infos {
                state.app.upsert_model(info).await;
            }
        }
    }

    state.app.emit_event("models.updated", "{}").await;
    Json(json!({"results": results})).into_response()
}

/// `POST /api/management/delete` — delete a model from instances.
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

    let mut results = Vec::new();
    for target in &targets {
        let stone_name = {
            let instances = state.app.instances.read().await;
            instances
                .get(target.as_str())
                .map(|i| i.stone_name.clone())
                .unwrap_or_else(|| target.clone())
        };

        match state.client.delete_model(target, &req.model).await {
            Ok(()) => {
                results.push(json!({
                    "stone": stone_name,
                    "endpoint": target,
                    "success": true,
                }));
            }
            Err(e) => {
                results.push(json!({
                    "stone": stone_name,
                    "endpoint": target,
                    "success": false,
                    "error": e.to_string(),
                }));
            }
        }
    }

    // Remove from registry
    for target in &targets {
        let instances = state.app.instances.read().await;
        if let Some(inst) = instances.get(target.as_str()) {
            let new_available: Vec<String> = inst
                .models_available
                .iter()
                .filter(|m| m.as_str() != req.model)
                .cloned()
                .collect();
            let new_loaded: Vec<crate::domain::types::LoadedModel> = inst
                .models_loaded
                .iter()
                .filter(|m| m.name != req.model)
                .cloned()
                .collect();
            drop(instances);
            state
                .app
                .update_instance_models(target, new_available, new_loaded)
                .await;
        }
    }

    // If no instances have the model anymore, remove from global registry
    {
        let instances = state.app.instances.read().await;
        let still_exists = instances
            .values()
            .any(|i| i.models_available.iter().any(|m| m == &req.model));
        if !still_exists {
            state.app.remove_model(&req.model).await;
        }
    }

    state.app.emit_event("models.updated", "{}").await;
    Json(json!({"results": results})).into_response()
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
