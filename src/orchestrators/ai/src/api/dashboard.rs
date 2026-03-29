//! Dashboard API endpoints.
//!
//! - `GET /api/status` — full snapshot for page load
//! - `GET /api/events` — SSE stream for incremental updates
//! - `GET /api/settings` — current config
//! - `POST /api/settings` — update config
//! - `GET /api/jobs` — recent jobs

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::stream::Stream;
use serde::Serialize;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::Ordering;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::app_state::DashboardEvent;
use crate::domain::types::*;
use crate::offerings::cloud::CloudProviderConfig;
use crate::AppState;

// ── Status Snapshot ─────────────────────────────────────────────

/// Full dashboard snapshot — returned by `GET /api/status`.
/// Contains everything the frontend needs to render all pages.
#[derive(Serialize)]
pub struct DashboardStatus {
    pub capabilities: Vec<CapabilityStatus>,
    pub stones: Vec<StoneStatus>,
    pub instances: Vec<InstanceStatus>,
    pub models: Vec<ModelStatus>,
    pub config: OrchestratorConfig,
    pub jobs: Vec<OrchestratorJob>,
    pub recommendations: HashMap<String, String>,
    pub uptime_secs: u64,
    pub version: String,
}

/// Per-capability status for the overview grid.
#[derive(Serialize)]
pub struct CapabilityStatus {
    pub capability: String,
    pub state: CapabilityState,
    pub recommended_model: Option<String>,
    pub model_count: usize,
    pub offering_count: usize,
    pub offerings: Vec<String>,
    pub instance_count: usize,
    pub healthy_instance_count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    /// Serving requests — has healthy instances with models
    Active,
    /// Service installed but no models for this capability
    NeedsSetup,
    /// No service installed that could serve this capability
    NotInstalled,
    /// Was working, now degraded/down
    Degraded,
}

/// Per-stone status for the VRAM view.
#[derive(Serialize)]
pub struct StoneStatus {
    pub id: String,
    pub name: String,
    pub gpu: Option<String>,
    pub vram_total_mb: u64,
    pub vram_used_mb: u64,
    pub offerings: Vec<StoneOfferingStatus>,
    pub health: String,
}

#[derive(Serialize)]
pub struct StoneOfferingStatus {
    pub kind: String,
    pub model_count: usize,
    pub loaded_count: usize,
    pub healthy: bool,
}

/// Per-instance status (flat view).
#[derive(Serialize)]
pub struct InstanceStatus {
    pub endpoint: String,
    pub stone_name: String,
    pub kind: String,
    pub health: String,
    pub models_available: Vec<String>,
    pub models_loaded: Vec<String>,
    pub vram_total_mb: u64,
    pub vram_budget_mb: u64,
    pub gpu: Option<String>,
    pub queue_depth: u32,
    pub capabilities: Vec<String>,
    pub priority: i32,
}

/// Per-model status (flat view, cross-offering).
#[derive(Serialize)]
pub struct ModelStatus {
    pub name: String,
    pub capabilities: Vec<String>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
    pub family: Option<String>,
    pub size_disk: u64,
    pub vram_bytes: Option<u64>,
    pub context_length: Option<u64>,
    pub available_on: Vec<ModelPlacement>,
}

#[derive(Serialize)]
pub struct ModelPlacement {
    pub stone: String,
    pub endpoint: String,
    pub offering: String,
    pub loaded: bool,
}

// ── Handlers ────────────────────────────────────────────────────

/// `GET /api/status` — full snapshot for page load.
pub async fn get_status(State(state): State<AppState>) -> Json<DashboardStatus> {
    let instances = state.instances.read().await;
    let models = state.models.read().await;
    let config = state.config.read().await;
    let jobs = state.jobs.read().await;
    let queue_depths = state.queue_depths.read().await;
    let recommended = state.recommended_models.read().await;

    // Build capability statuses
    let capabilities = build_capability_statuses(&instances, &models, &recommended, &state);

    // Build stone statuses (group instances by stone)
    let stones = build_stone_statuses(&instances);

    // Build flat instance list
    let instance_list: Vec<InstanceStatus> = instances
        .values()
        .map(|i| {
            let qd = queue_depths
                .get(&i.endpoint)
                .map(|c| c.load(Ordering::Relaxed))
                .unwrap_or(0);

            InstanceStatus {
                endpoint: i.endpoint.clone(),
                stone_name: i.stone.name.clone(),
                kind: i.kind.as_str().to_string(),
                health: format!("{:?}", i.health).to_lowercase(),
                models_available: i.models_available.clone(),
                models_loaded: i.models_loaded.iter().map(|m| m.name.clone()).collect(),
                vram_total_mb: i.vram.total_bytes / 1_048_576,
                vram_budget_mb: i.vram.budget_bytes / 1_048_576,
                gpu: i.gpu.name.clone(),
                queue_depth: qd,
                capabilities: i.capabilities.iter().map(|c| c.as_str().to_string()).collect(),
                priority: i.priority,
            }
        })
        .collect();

    // Build flat model list with placement info
    let model_list: Vec<ModelStatus> = models
        .values()
        .map(|m| {
            let available_on: Vec<ModelPlacement> = instances
                .values()
                .filter(|i| i.models_available.contains(&m.name))
                .map(|i| ModelPlacement {
                    stone: i.stone.name.clone(),
                    endpoint: i.endpoint.clone(),
                    offering: i.kind.as_str().to_string(),
                    loaded: i.models_loaded.iter().any(|lm| lm.name == m.name),
                })
                .collect();

            ModelStatus {
                name: m.name.clone(),
                capabilities: m.capabilities.clone(),
                parameter_size: m.parameter_size.clone(),
                quantization_level: m.quantization_level.clone(),
                family: m.family.clone(),
                size_disk: m.size_disk,
                vram_bytes: m.vram_bytes,
                context_length: m.context_length,
                available_on,
            }
        })
        .collect();

    Json(DashboardStatus {
        capabilities,
        stones,
        instances: instance_list,
        models: model_list,
        config: config.clone(),
        jobs: jobs.iter().rev().cloned().collect(),
        recommendations: recommended.clone(),
        uptime_secs: state.start_time.elapsed().as_secs(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// `GET /api/events` — SSE stream for incremental updates.
pub async fn get_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.dashboard_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => Some(Ok(Event::default()
            .event(&event.event_type)
            .data(&event.data))),
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            tracing::warn!(skipped = n, "SSE consumer lagged");
            None
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// `GET /api/settings` — current config.
pub async fn get_settings(State(state): State<AppState>) -> Json<OrchestratorConfig> {
    let config = state.config.read().await;
    Json(config.clone())
}

/// `POST /api/settings` — update config.
pub async fn post_settings(
    State(state): State<AppState>,
    Json(new_config): Json<OrchestratorConfig>,
) -> Json<serde_json::Value> {
    let metrics_enabled = new_config.features.metrics_enabled;
    {
        let mut config = state.config.write().await;
        *config = new_config.clone();
    }
    {
        let mut metrics = state.metrics.write().await;
        metrics.enabled = metrics_enabled;
    }

    // Persist
    if let Err(e) =
        crate::infra::persistence::save_config(&state.data_dir, &new_config).await
    {
        tracing::warn!(error = %e, "failed to persist config");
    }

    state.refresh_recommendations().await;
    state.emit_event("config.updated", "{}").await;

    Json(serde_json::json!({"status": "ok"}))
}

/// `GET /api/jobs` — recent jobs.
pub async fn get_jobs(State(state): State<AppState>) -> Json<serde_json::Value> {
    let jobs = state.jobs.read().await;
    let jobs_vec: Vec<_> = jobs.iter().rev().cloned().collect();
    Json(serde_json::json!({"jobs": jobs_vec}))
}

// ── Cloud Provider Management ───────────────────────────────────

/// Provider info with masked API key — safe for dashboard display.
#[derive(Serialize)]
pub struct ProviderInfo {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub masked_key: String,
    pub enabled: bool,
    pub priority: i32,
    pub capabilities: Vec<String>,
    pub model_count: usize,
}

/// `GET /api/providers` — list configured cloud providers (keys masked).
pub async fn get_providers(State(state): State<AppState>) -> Json<Vec<ProviderInfo>> {
    let store = state.cloud_store.read().await;
    let providers: Vec<ProviderInfo> = store
        .all()
        .iter()
        .map(|p| ProviderInfo {
            name: p.name.clone(),
            kind: p.kind.as_str().to_string(),
            base_url: p.base_url.clone(),
            masked_key: p.masked_key(),
            enabled: p.enabled,
            priority: p.priority,
            capabilities: p.capabilities.iter().map(|c| c.as_str().to_string()).collect(),
            model_count: p.models.len(),
        })
        .collect();
    Json(providers)
}

/// `POST /api/providers` — add or update a cloud provider.
pub async fn add_provider(
    State(state): State<AppState>,
    Json(config): Json<CloudProviderConfig>,
) -> Json<serde_json::Value> {
    let provider_name = config.name.clone();
    {
        let mut store = state.cloud_store.write().await;
        store.add(config);
        if let Err(e) = store.save().await {
            tracing::warn!(error = %e, "failed to persist cloud provider store");
        }
    }

    state
        .emit_event(
            "providers.updated",
            &serde_json::json!({"action": "add", "name": provider_name}).to_string(),
        )
        .await;

    Json(serde_json::json!({"status": "ok", "name": provider_name}))
}

/// `DELETE /api/providers/:name` — remove a cloud provider.
pub async fn delete_provider(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    let removed = {
        let mut store = state.cloud_store.write().await;
        let removed = store.remove(&name);
        if removed
            && let Err(e) = store.save().await
        {
            tracing::warn!(error = %e, "failed to persist cloud provider store");
        }
        removed
    };

    if removed {
        state
            .emit_event(
                "providers.updated",
                &serde_json::json!({"action": "remove", "name": name}).to_string(),
            )
            .await;

        Json(serde_json::json!({"status": "ok", "removed": name}))
    } else {
        Json(serde_json::json!({"status": "not_found", "name": name}))
    }
}

// ── Helpers ─────────────────────────────────────────────────────

fn build_capability_statuses(
    instances: &HashMap<String, ServiceInstance>,
    models: &HashMap<String, ModelInfo>,
    recommended: &HashMap<String, String>,
    state: &AppState,
) -> Vec<CapabilityStatus> {
    Capability::ALL
        .iter()
        .map(|cap| {
            let cap_str = cap.as_str();

            // Find all offerings that could serve this capability
            let serving_offerings: Vec<String> = state
                .registry
                .kinds()
                .filter(|kind| {
                    state
                        .registry
                        .get(*kind)
                        .map(|o| o.capabilities().contains(cap))
                        .unwrap_or(false)
                })
                .map(|k| k.as_str().to_string())
                .collect();

            // Find instances that declare this capability and are healthy
            let capable_instances: Vec<&ServiceInstance> = instances
                .values()
                .filter(|i| i.capabilities.contains(cap))
                .collect();

            let healthy_count = capable_instances.iter().filter(|i| i.is_routable()).count();

            // Count models that have this capability tag
            let model_count = models
                .values()
                .filter(|m| m.capabilities.iter().any(|c| c == cap_str))
                .count();

            // Determine state
            let cap_state = if healthy_count > 0 && model_count > 0 {
                CapabilityState::Active
            } else if !capable_instances.is_empty() || !serving_offerings.is_empty() {
                // Service exists but no models or unhealthy
                if capable_instances.iter().any(|i| !i.is_routable()) && healthy_count == 0 {
                    CapabilityState::Degraded
                } else {
                    CapabilityState::NeedsSetup
                }
            } else {
                CapabilityState::NotInstalled
            };

            CapabilityStatus {
                capability: cap_str.to_string(),
                state: cap_state,
                recommended_model: recommended.get(cap_str).cloned(),
                model_count,
                offering_count: serving_offerings.len(),
                offerings: serving_offerings,
                instance_count: capable_instances.len(),
                healthy_instance_count: healthy_count,
            }
        })
        .collect()
}

fn build_stone_statuses(instances: &HashMap<String, ServiceInstance>) -> Vec<StoneStatus> {
    let mut stones: HashMap<String, StoneStatus> = HashMap::new();

    for inst in instances.values() {
        let stone = stones
            .entry(inst.stone.name.clone())
            .or_insert_with(|| StoneStatus {
                id: inst.stone.id.clone(),
                name: inst.stone.name.clone(),
                gpu: inst.gpu.name.clone(),
                vram_total_mb: inst.vram.total_bytes / 1_048_576,
                vram_used_mb: 0,
                offerings: vec![],
                health: "healthy".to_string(),
            });

        // Sum VRAM used by loaded models
        let loaded_vram: u64 = inst.models_loaded.iter().map(|m| m.size_vram).sum();
        stone.vram_used_mb += loaded_vram / 1_048_576;

        stone.offerings.push(StoneOfferingStatus {
            kind: inst.kind.as_str().to_string(),
            model_count: inst.models_available.len(),
            loaded_count: inst.models_loaded.len(),
            healthy: inst.is_routable(),
        });

        if !inst.is_routable() {
            stone.health = "degraded".to_string();
        }
    }

    stones.into_values().collect()
}
