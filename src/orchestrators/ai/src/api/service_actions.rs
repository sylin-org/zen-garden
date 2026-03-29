//! Service management action endpoints for the dashboard.
//!
//! These endpoints support the Service Detail page's action buttons:
//! pull, refresh, load, unload, delete, benchmark, sync.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::domain::types::{JobKind, OfferingKind};
use crate::offerings::ollama::OllamaOffering;
use crate::AppState;

// ── Request Types ──────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PullRequest {
    pub model: String,
    pub targets: Vec<String>,
}

#[derive(Deserialize)]
pub struct LoadUnloadRequest {
    pub model: String,
    pub endpoint: String,
}

// ── Helpers ────────────────────────────────────────────────────

/// Resolve an offering kind from a URL path segment string.
fn resolve_offering_kind(name: &str) -> Option<OfferingKind> {
    match name {
        "ollama" => Some(OfferingKind::Ollama),
        "comfyui" => Some(OfferingKind::ComfyUi),
        "speaches" => Some(OfferingKind::Speaches),
        "openedai-speech" => Some(OfferingKind::OpenedaiSpeech),
        "infinity" => Some(OfferingKind::Infinity),
        "libretranslate" => Some(OfferingKind::LibreTranslate),
        "huggingface" => Some(OfferingKind::HuggingFace),
        "openai" => Some(OfferingKind::OpenAi),
        "anthropic" => Some(OfferingKind::Anthropic),
        "stability-ai" => Some(OfferingKind::StabilityAi),
        "elevenlabs" => Some(OfferingKind::ElevenLabs),
        "cohere" => Some(OfferingKind::Cohere),
        "deepgram" => Some(OfferingKind::Deepgram),
        "google" => Some(OfferingKind::Google),
        _ => None,
    }
}

/// Get the OllamaClient from the registry via downcast.
fn get_ollama_client(state: &AppState) -> Option<crate::offerings::ollama::client::OllamaClient> {
    state
        .registry
        .get(OfferingKind::Ollama)?
        .as_any()
        .downcast_ref::<OllamaOffering>()
        .map(|o| o.client().clone())
}

/// JSON error response.
fn error_response(
    status: StatusCode,
    message: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({"status": "error", "message": message})))
}

// ── POST /api/services/:offering/pull ──────────────────────────

/// Pull a model to target instances. Creates a background job.
pub async fn pull_model(
    State(state): State<AppState>,
    Path(offering): Path<String>,
    Json(req): Json<PullRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let kind = resolve_offering_kind(&offering)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "unknown offering"))?;

    if kind != OfferingKind::Ollama {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "pull is only supported for Ollama offerings",
        ));
    }

    let client = get_ollama_client(&state)
        .ok_or_else(|| error_response(StatusCode::SERVICE_UNAVAILABLE, "Ollama adapter not available"))?;

    if req.targets.is_empty() {
        return Err(error_response(StatusCode::BAD_REQUEST, "targets must not be empty"));
    }

    let job_id = state
        .create_job(JobKind::ModelPull {
            model: req.model.clone(),
            targets: req.targets.clone(),
        })
        .await;

    // Spawn background pull tasks
    let state_bg = state.clone();
    let model = req.model.clone();
    let targets = req.targets.clone();
    let job_id_bg = job_id.clone();

    tokio::spawn(async move {
        use futures_util::StreamExt;

        state_bg
            .update_job(&job_id_bg, crate::domain::types::JobStatus::Running, None)
            .await;

        let mut failed = false;
        for target in &targets {
            tracing::info!(model = %model, target = %target, "pulling model");
            match client.pull_model(target, &model).await {
                Ok(mut stream) => {
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(bytes) => {
                                // Try to parse progress for job updates
                                if let Ok(progress) =
                                    serde_json::from_slice::<serde_json::Value>(&bytes)
                                {
                                    if let Some(status) = progress.get("status").and_then(|s| s.as_str()) {
                                        state_bg
                                            .update_job(
                                                &job_id_bg,
                                                crate::domain::types::JobStatus::Running,
                                                Some(format!("{target}: {status}")),
                                            )
                                            .await;
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!(model = %model, target = %target, error = %e, "pull stream error");
                                failed = true;
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(model = %model, target = %target, error = %e, "failed to initiate pull");
                    failed = true;
                }
            }
        }

        if failed {
            state_bg.fail_job(&job_id_bg, "pull failed on one or more targets").await;
        } else {
            state_bg.complete_job(&job_id_bg).await;
        }

        state_bg
            .emit_event(
                "models.changed",
                &serde_json::json!({"action": "pull", "model": model}).to_string(),
            )
            .await;
    });

    Ok(Json(serde_json::json!({
        "job_id": job_id,
        "status": "queued",
    })))
}

// ── POST /api/services/:offering/refresh ───────────────────────

/// Re-enumerate models on all instances of this offering kind.
pub async fn refresh_models(
    State(state): State<AppState>,
    Path(offering): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let kind = resolve_offering_kind(&offering)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "unknown offering"))?;

    let adapter = state
        .registry
        .get(kind)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "offering not registered"))?
        .clone();

    // Find all instance endpoints for this offering kind
    let endpoints: Vec<String> = {
        let instances = state.instances.read().await;
        instances
            .values()
            .filter(|i| i.kind == kind)
            .map(|i| i.endpoint.clone())
            .collect()
    };

    let mut total_models = 0usize;
    for endpoint in &endpoints {
        match adapter.enumerate(endpoint).await {
            Ok(service_models) => {
                let model_names: Vec<String> = service_models.iter().map(|m| m.name.clone()).collect();
                let model_count = model_names.len();

                // Update model infos in state
                for sm in &service_models {
                    let info = crate::domain::types::ModelInfo {
                        name: sm.name.clone(),
                        parameter_count: sm.metadata.get("parameter_count").and_then(|v| v.as_u64()),
                        parameter_size: sm.metadata.get("parameter_size").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        quantization_level: sm.metadata.get("quantization_level").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        family: sm.metadata.get("family").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        families: sm.metadata.get("families")
                            .and_then(|v| v.as_array())
                            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                            .unwrap_or_default(),
                        capabilities: sm.capabilities.iter().map(|c| c.as_str().to_string()).collect(),
                        specializations: sm.specializations.clone(),
                        format: sm.metadata.get("format").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        size_disk: sm.metadata.get("size_disk").and_then(|v| v.as_u64()).unwrap_or(0),
                        vram_bytes: sm.vram_bytes,
                        context_length: sm.metadata.get("context_length").and_then(|v| v.as_u64()),
                    };
                    state.upsert_model(info).await;
                }

                // Update instance model lists
                // For full accuracy we'd need loaded models too, but enumerate gives available.
                // We update models_available; loaded state stays as-is from health checks.
                {
                    let mut instances = state.instances.write().await;
                    if let Some(inst) = instances.get_mut(endpoint) {
                        inst.models_available = model_names;
                    }
                }

                total_models += model_count;
                tracing::info!(endpoint = %endpoint, models = model_count, "refreshed models");
            }
            Err(e) => {
                tracing::warn!(endpoint = %endpoint, error = %e, "failed to enumerate models");
            }
        }
    }

    state
        .emit_event(
            "models.changed",
            &serde_json::json!({"action": "refresh", "offering": offering}).to_string(),
        )
        .await;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "models": total_models,
    })))
}

// ── POST /api/services/:offering/benchmark ─────────────────────

/// Trigger a benchmark (placeholder).
pub async fn trigger_benchmark(
    Path(_offering): Path<String>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "not_implemented",
        "message": "Benchmark coming soon",
    }))
}

// ── POST /api/services/:offering/sync ──────────────────────────

/// Sync models across instances (placeholder).
pub async fn sync_models(
    Path(_offering): Path<String>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "not_implemented",
        "message": "Model sync coming soon",
    }))
}

// ── POST /api/services/:offering/load ──────────────────────────

/// Load a model into VRAM on a specific instance.
pub async fn load_model(
    State(state): State<AppState>,
    Path(offering): Path<String>,
    Json(req): Json<LoadUnloadRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let kind = resolve_offering_kind(&offering)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "unknown offering"))?;

    if kind != OfferingKind::Ollama {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "load is only supported for Ollama offerings",
        ));
    }

    let client = get_ollama_client(&state)
        .ok_or_else(|| error_response(StatusCode::SERVICE_UNAVAILABLE, "Ollama adapter not available"))?;

    client
        .load_model(&req.endpoint, &req.model)
        .await
        .map_err(|e| error_response(StatusCode::BAD_GATEWAY, &format!("load failed: {e}")))?;

    state
        .emit_event(
            "models.changed",
            &serde_json::json!({"action": "load", "model": req.model, "endpoint": req.endpoint}).to_string(),
        )
        .await;

    Ok(Json(serde_json::json!({"status": "ok"})))
}

// ── POST /api/services/:offering/unload ────────────────────────

/// Unload a model from VRAM on a specific instance.
pub async fn unload_model(
    State(state): State<AppState>,
    Path(offering): Path<String>,
    Json(req): Json<LoadUnloadRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let kind = resolve_offering_kind(&offering)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "unknown offering"))?;

    if kind != OfferingKind::Ollama {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "unload is only supported for Ollama offerings",
        ));
    }

    let client = get_ollama_client(&state)
        .ok_or_else(|| error_response(StatusCode::SERVICE_UNAVAILABLE, "Ollama adapter not available"))?;

    client
        .unload_model(&req.endpoint, &req.model)
        .await
        .map_err(|e| error_response(StatusCode::BAD_GATEWAY, &format!("unload failed: {e}")))?;

    state
        .emit_event(
            "models.changed",
            &serde_json::json!({"action": "unload", "model": req.model, "endpoint": req.endpoint}).to_string(),
        )
        .await;

    Ok(Json(serde_json::json!({"status": "ok"})))
}

// ── DELETE /api/services/:offering/models/:model ───────────────

/// Delete a model from all instances of this offering kind.
pub async fn delete_model(
    State(state): State<AppState>,
    Path((offering, model)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let kind = resolve_offering_kind(&offering)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "unknown offering"))?;

    if kind != OfferingKind::Ollama {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "delete is only supported for Ollama offerings",
        ));
    }

    let client = get_ollama_client(&state)
        .ok_or_else(|| error_response(StatusCode::SERVICE_UNAVAILABLE, "Ollama adapter not available"))?;

    // Find all instances of this offering kind to delete from
    let endpoints: Vec<String> = {
        let instances = state.instances.read().await;
        instances
            .values()
            .filter(|i| i.kind == kind && i.models_available.contains(&model))
            .map(|i| i.endpoint.clone())
            .collect()
    };

    if endpoints.is_empty() {
        return Err(error_response(StatusCode::NOT_FOUND, "model not found on any instance"));
    }

    let job_id = state
        .create_job(JobKind::ModelDelete {
            model: model.clone(),
            targets: endpoints.clone(),
        })
        .await;

    let state_bg = state.clone();
    let model_bg = model.clone();
    let job_id_bg = job_id.clone();

    tokio::spawn(async move {
        state_bg
            .update_job(&job_id_bg, crate::domain::types::JobStatus::Running, None)
            .await;

        let mut failed = false;
        for endpoint in &endpoints {
            if let Err(e) = client.delete_model(endpoint, &model_bg).await {
                tracing::error!(model = %model_bg, endpoint = %endpoint, error = %e, "failed to delete model");
                failed = true;
            }
        }

        if failed {
            state_bg.fail_job(&job_id_bg, "delete failed on one or more instances").await;
        } else {
            state_bg.complete_job(&job_id_bg).await;
            state_bg.remove_model(&model_bg).await;
        }

        state_bg
            .emit_event(
                "models.changed",
                &serde_json::json!({"action": "delete", "model": model_bg}).to_string(),
            )
            .await;
    });

    Ok(Json(serde_json::json!({
        "job_id": job_id,
        "status": "queued",
    })))
}
