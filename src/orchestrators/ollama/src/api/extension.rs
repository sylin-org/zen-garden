//! Extension API (`/v1/`) — garden-aware orchestrator endpoints.
//!
//! Read-only endpoints that expose richer data than the Ollama-compatible
//! `/api/*` surface.  Safe for any client; no side effects.
//!
//! These live on the same proxy port (`:21434`) under the `/v1/` prefix,
//! which Ollama will never claim — zero collision risk.

use crate::api::proxy::ProxyState;
use crate::domain::recommendation;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;

// ── /v1/models ───────────────────────────────────────────────────

/// Model summary with placement, VRAM usage, fitness and loaded state.
#[derive(Serialize)]
struct V1Model {
    name: String,
    parameter_size: Option<String>,
    quantization_level: Option<String>,
    family: Option<String>,
    capabilities: Vec<String>,
    format: Option<String>,
    size_disk: u64,
    vram_bytes: Option<u64>,
    /// Context window in tokens from GGUF metadata (e.g. 256, 8192, 131072).
    /// `null` if `/api/show` didn't return it (model never profiled).
    context_length: Option<u64>,
    /// Stones that have this model installed.
    available_on: Vec<ModelPlacement>,
    /// Stones where this model is currently loaded in VRAM.
    loaded_on: Vec<ModelLoaded>,
}

#[derive(Serialize)]
struct ModelPlacement {
    stone: String,
    endpoint: String,
    tier: String,
    fitness_score: Option<u32>,
}

#[derive(Serialize)]
struct ModelLoaded {
    stone: String,
    endpoint: String,
    size_vram: u64,
    expires_at: Option<String>,
}

#[derive(Serialize)]
struct V1ModelsResponse {
    models: Vec<V1Model>,
}

/// `GET /v1/models` — all known models with per-stone placement and load state.
pub async fn get_models(State(state): State<ProxyState>) -> impl IntoResponse {
    let instances = state.app.instances.read().await;
    let models = state.app.models.read().await;
    let tiers = state.app.tiers.read().await;
    let gpu_matrix = {
        let run = state.app.benchmark_run.read().await;
        run.gpu_matrix.clone()
    };

    // Helper: which tier label does an endpoint belong to?
    let tier_of = |endpoint: &str| -> String {
        tiers
            .iter()
            .find(|t| t.instance_endpoints.contains(&endpoint.to_string()))
            .map(|t| t.label.clone())
            .unwrap_or_default()
    };

    let mut result: Vec<V1Model> = Vec::new();

    for (name, info) in models.iter() {
        let mut available_on = Vec::new();
        let mut loaded_on = Vec::new();

        for inst in instances.values() {
            if !inst.models_available.contains(name) {
                continue;
            }

            available_on.push(ModelPlacement {
                stone: inst.stone_name.clone(),
                endpoint: inst.endpoint.clone(),
                tier: tier_of(&inst.endpoint),
                fitness_score: gpu_matrix.fitness_score(name, &inst.endpoint),
            });

            for loaded in &inst.models_loaded {
                if loaded.name == *name {
                    loaded_on.push(ModelLoaded {
                        stone: inst.stone_name.clone(),
                        endpoint: inst.endpoint.clone(),
                        size_vram: loaded.size_vram,
                        expires_at: loaded.expires_at.clone(),
                    });
                }
            }
        }

        // Sort placements: fitness descending (None last)
        available_on.sort_by(|a, b| b.fitness_score.cmp(&a.fitness_score));

        result.push(V1Model {
            name: name.clone(),
            parameter_size: info.parameter_size.clone(),
            quantization_level: info.quantization_level.clone(),
            family: info.family.clone(),
            capabilities: info.capabilities.clone(),
            format: info.format.clone(),
            size_disk: info.size_disk,
            vram_bytes: info.vram_bytes,
            context_length: info.context_length,
            available_on,
            loaded_on,
        });
    }

    // Sort models alphabetically
    result.sort_by(|a, b| a.name.cmp(&b.name));

    Json(V1ModelsResponse { models: result })
}

// ── /v1/stones ───────────────────────────────────────────────────

#[derive(Serialize)]
struct V1Stone {
    name: String,
    endpoint: String,
    health: String,
    gpu: Option<StoneGpu>,
    models: StoneModels,
    queue_depth: u32,
    tier: String,
    ollama_version: Option<String>,
}

#[derive(Serialize)]
struct StoneGpu {
    name: String,
    vram_total_mb: u64,
    vram_budget_mb: u64,
    vram_used_mb: u64,
}

#[derive(Serialize)]
struct StoneModels {
    available: Vec<String>,
    loaded: Vec<String>,
}

#[derive(Serialize)]
struct V1StonesResponse {
    stones: Vec<V1Stone>,
}

/// `GET /v1/stones` — per-stone hardware, models, queue depth and health.
pub async fn get_stones(State(state): State<ProxyState>) -> impl IntoResponse {
    let instances = state.app.instances.read().await;
    let tiers = state.app.tiers.read().await;
    let depths = state.app.queue_depths.read().await;

    let tier_of = |endpoint: &str| -> String {
        tiers
            .iter()
            .find(|t| t.instance_endpoints.contains(&endpoint.to_string()))
            .map(|t| t.label.clone())
            .unwrap_or_default()
    };

    let mut stones: Vec<V1Stone> = instances
        .values()
        .map(|inst| {
            let vram_used_bytes: u64 = inst.models_loaded.iter().map(|m| m.size_vram).sum();

            let gpu = inst.gpu_name.as_ref().map(|name| StoneGpu {
                name: name.clone(),
                vram_total_mb: inst.vram_total_bytes / (1024 * 1024),
                vram_budget_mb: inst.vram_budget_bytes / (1024 * 1024),
                vram_used_mb: vram_used_bytes / (1024 * 1024),
            });

            let queue_depth = depths
                .get(&inst.endpoint)
                .map(|c| c.load(Ordering::Relaxed))
                .unwrap_or(0);

            let health = match &inst.health {
                crate::domain::types::InstanceHealth::Healthy => "healthy".to_string(),
                crate::domain::types::InstanceHealth::Profiling => "profiling".to_string(),
                crate::domain::types::InstanceHealth::Unhealthy { reason, .. } => {
                    format!("unhealthy: {reason}")
                }
            };

            V1Stone {
                name: inst.stone_name.clone(),
                endpoint: inst.endpoint.clone(),
                health,
                gpu,
                models: StoneModels {
                    available: inst.models_available.clone(),
                    loaded: inst.models_loaded.iter().map(|m| m.name.clone()).collect(),
                },
                queue_depth,
                tier: tier_of(&inst.endpoint),
                ollama_version: inst.ollama_version.clone(),
            }
        })
        .collect();

    // Sort by name for stable output
    stones.sort_by(|a, b| a.name.cmp(&b.name));

    Json(V1StonesResponse { stones })
}

// ── /v1/recommendations ─────────────────────────────────────────

#[derive(Deserialize)]
pub struct RecommendationQuery {
    /// Optional capability filter.  When omitted, returns recommendations
    /// for all applicable capabilities grouped in a single response.
    /// Valid values: quick, chat, completion, synthesis, vision, ocr, tools,
    /// thinking, embedding.
    pub capability: Option<String>,
}

/// All user-facing recommendation categories.
const ALL_CAPABILITIES: &[&str] = &[
    "quick", "chat", "synthesis", "vision", "ocr", "tools", "thinking", "embedding",
];

/// `GET /v1/recommendations` — ranked model recommendations.
///
/// With `?capability=chat` returns a single `RecommendationResponse`.
/// Without a capability parameter returns an array of all categories.
pub async fn get_recommendations(
    State(state): State<ProxyState>,
    Query(params): Query<RecommendationQuery>,
) -> impl IntoResponse {
    let models = state.app.models.read().await;
    let instances = state.app.instances.read().await;
    let gpu_matrix = {
        let run = state.app.benchmark_run.read().await;
        run.gpu_matrix.clone()
    };
    let pins = {
        let config = state.app.config.read().await;
        config.features.pins.clone()
    };

    match params.capability {
        Some(cap) => {
            let pin = pins.get(&cap).map(|s| s.as_str());
            let resp = recommendation::recommend(&cap, &models, &instances, &gpu_matrix, pin);
            Json(serde_json::to_value(resp).unwrap_or_default())
        }
        None => {
            let all: Vec<recommendation::RecommendationResponse> = ALL_CAPABILITIES
                .iter()
                .map(|cap| {
                    let pin = pins.get(*cap).map(|s| s.as_str());
                    recommendation::recommend(cap, &models, &instances, &gpu_matrix, pin)
                })
                .filter(|r| !r.recommendations.is_empty())
                .collect();
            Json(serde_json::to_value(all).unwrap_or_default())
        }
    }
}

// ── /v1/recommendations/:capability/pin ────────────────────────

#[derive(Deserialize)]
pub struct PinRequest {
    pub model: String,
}

/// `PUT /v1/recommendations/:capability/pin` — pin a model for a capability.
pub async fn put_pin(
    State(state): State<ProxyState>,
    Path(capability): Path<String>,
    Json(body): Json<PinRequest>,
) -> impl IntoResponse {
    if !ALL_CAPABILITIES.contains(&capability.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("unknown capability: {capability}")})),
        );
    }

    let new_config = {
        let mut config = state.app.config.write().await;
        config.features.pins.insert(capability.clone(), body.model.clone());
        config.clone()
    };

    if let Err(e) =
        crate::infra::persistence::save_config(&state.app.data_dir, &new_config).await
    {
        tracing::warn!(error = %e, "failed to persist pin");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        );
    }

    state.app.emit_event("config.updated", "{}").await;
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "ok", "capability": capability, "model": body.model})),
    )
}

/// `DELETE /v1/recommendations/:capability/pin` — unpin a capability.
pub async fn delete_pin(
    State(state): State<ProxyState>,
    Path(capability): Path<String>,
) -> impl IntoResponse {
    let new_config = {
        let mut config = state.app.config.write().await;
        config.features.pins.remove(&capability);
        config.clone()
    };

    if let Err(e) =
        crate::infra::persistence::save_config(&state.app.data_dir, &new_config).await
    {
        tracing::warn!(error = %e, "failed to persist unpin");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        );
    }

    state.app.emit_event("config.updated", "{}").await;
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "ok", "capability": capability})),
    )
}
