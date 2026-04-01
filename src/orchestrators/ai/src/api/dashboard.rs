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
    /// Unified model catalog from the ORCH-0015 model directory.
    pub models: Vec<DirectoryEntry>,
    pub config: OrchestratorConfig,
    pub jobs: Vec<OrchestratorJob>,
    pub recommendations: HashMap<String, String>,
    pub uptime_secs: u64,
    pub version: String,
}

/// A model in the directory — serializable view of `ModelEntry`.
/// Includes both metadata and placement (which instances serve it).
#[derive(Serialize)]
pub struct DirectoryEntry {
    pub model: String,
    pub parameters: Option<String>,
    pub model_identity: String,
    pub capabilities: Vec<String>,
    pub specializations: Vec<String>,
    pub metadata: ModelMetadata,
    /// All FQN strings of instances that can serve this model.
    pub instances: Vec<String>,
    /// Number of instances.
    pub instance_count: usize,
    /// Per-instance placement details (stone, loaded status).
    pub available_on: Vec<ModelPlacement>,
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

/// Where a model is available — one entry per instance serving it.
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
    let reg_snap = state.registry.snapshot().clone();
    let dir_snap = state.directory.snapshot().clone();
    let obs_snap = state.observability.snapshot().clone();
    let intel_snap = state.intelligence.snapshot().clone();
    let config = state.config.read().await;

    let instances = &reg_snap.instances;
    let directory = &dir_snap.directory;
    let recommended = &intel_snap.recommendations;

    // Build capability statuses
    let capabilities = build_capability_statuses(instances, directory, recommended, &state);

    // Build stone statuses (group instances by stone)
    let stones = build_stone_statuses(instances);

    // Build flat instance list
    let instance_list: Vec<InstanceStatus> = instances
        .values()
        .map(|i| {
            let qd = reg_snap
                .queue_counters
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

    // Build model catalog from directory with placement info
    let model_list = build_directory_entries(directory, instances);

    Json(DashboardStatus {
        capabilities,
        stones,
        instances: instance_list,
        models: model_list,
        config: config.clone(),
        jobs: obs_snap.jobs.iter().cloned().collect(),
        recommendations: (**recommended).clone(),
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
    state.observability.set_metrics_enabled(metrics_enabled).await;

    // Persist
    if let Err(e) =
        crate::infra::persistence::save_config(&state.data_dir, &new_config).await
    {
        tracing::warn!(error = %e, "failed to persist config");
    }

    state.emit_event("config.updated", "{}").await;

    Json(serde_json::json!({"status": "ok"}))
}

/// `GET /api/defaults` — current inference defaults per capability.
pub async fn get_defaults(
    State(state): State<AppState>,
) -> Json<HashMap<String, InferenceDefaults>> {
    let config = state.config.read().await;
    Json(config.defaults.clone())
}

/// `POST /api/defaults` — update inference defaults per capability.
pub async fn post_defaults(
    State(state): State<AppState>,
    Json(defaults): Json<HashMap<String, InferenceDefaults>>,
) -> Json<serde_json::Value> {
    {
        let mut config = state.config.write().await;
        config.defaults = defaults;
    }

    // Persist the full config.
    let config = state.config.read().await.clone();
    if let Err(e) = crate::infra::persistence::save_config(&state.data_dir, &config).await {
        tracing::warn!(error = %e, "failed to persist config after defaults update");
    }

    state.emit_event("config.updated", "{}").await;
    Json(serde_json::json!({"status": "ok"}))
}

/// `GET /api/jobs` — recent jobs.
pub async fn get_jobs(State(state): State<AppState>) -> Json<serde_json::Value> {
    let obs_snap = state.observability.snapshot().clone();
    let jobs_vec: Vec<_> = obs_snap.jobs.iter().cloned().collect();
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
///
/// If `cached_models` is populated (e.g., from a prior test-key call),
/// those are persisted to disk alongside the key. On subsequent startups,
/// the cached list avoids a cold-start enumeration.
pub async fn add_provider(
    State(state): State<AppState>,
    Json(config): Json<CloudProviderConfig>,
) -> Json<serde_json::Value> {
    let provider_name = config.name.clone();
    let model_count = config.cached_models.len();
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
            &serde_json::json!({
                "action": "add",
                "name": provider_name,
                "models": model_count,
            })
            .to_string(),
        )
        .await;

    Json(serde_json::json!({"status": "ok", "name": provider_name, "models": model_count}))
}

/// `PATCH /api/providers/:name/toggle` — enable or disable a cloud provider.
///
/// When disabled: removes the provider's models from the directory and
/// its instance from the registry. Models disappear from capability lists.
/// When enabled: triggers an immediate sync (probe + enumerate).
pub async fn toggle_provider(
    State(state): State<AppState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<serde_json::Value> {
    let (kind, new_enabled, endpoint) = {
        let mut store = state.cloud_store.write().await;
        let provider = match store.all().iter().find(|p| p.name == name) {
            Some(p) => p,
            None => {
                return Json(serde_json::json!({"status": "not_found", "name": name}));
            }
        };
        let kind = provider.kind;
        let new_enabled = !provider.enabled;
        let endpoint = provider.base_url.clone();

        // Toggle the enabled flag
        store.set_enabled(&name, new_enabled);
        if let Err(e) = store.save().await {
            tracing::warn!(error = %e, "failed to persist cloud provider store");
        }
        (kind, new_enabled, endpoint)
    };

    if new_enabled {
        // Re-enable: trigger a sync for this provider
        tracing::info!(provider = %name, "cloud provider enabled — triggering sync");

        // The cloud_sync task will pick it up on next cycle, but also do an
        // immediate probe+enumerate via the Provider trait.
        if let Some(provider_impl) = state.providers.get(kind).cloned() {
            let api_key = {
                let store = state.cloud_store.read().await;
                store
                    .all()
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| p.api_key.clone())
            };

            let ctx = crate::catalog::ProviderContext {
                endpoint: endpoint.clone(),
                model: None,
                api_key,
            };

            // Register instance
            let instance = ServiceInstance {
                stone: Stone {
                    id: format!("cloud-{name}"),
                    name: format!("cloud:{name}"),
                },
                endpoint: endpoint.clone(),
                kind,
                gpu: Gpu {
                    name: None,
                    compute: ComputeType::Cpu,
                },
                vram: Vram {
                    total_bytes: 0,
                    budget_bytes: 0,
                    free_bytes: None,
                },
                health: InstanceHealth::Profiling,
                models_available: vec![],
                models_loaded: vec![],
                capabilities: vec![],
                queue_depth: 0,
                last_seen: std::time::Instant::now(),
                metadata: serde_json::json!({"cloud": true, "provider": name}),
                priority: -10,
            };
            { let cfg = state.config.read().await; state.registry.upsert_instance(instance, &cfg).await; }

            // Probe + enumerate in background
            let state_bg = state.clone();
            let name_bg = name.clone();
            tokio::spawn(async move {
                if provider_impl.probe(&ctx).await.is_ok() {
                    state_bg
                        .registry
                        .set_instance_health(&endpoint, InstanceHealth::Healthy)
                        .await;
                    if let Ok(models) = provider_impl.enumerate(&ctx).await {
                        let model_names: Vec<String> =
                            models.iter().map(|m| m.name.clone()).collect();
                        state_bg
                            .registry
                            .update_instance_models(&endpoint, model_names, vec![])
                            .await;
                        for sm in &models {
                            let fqn = ModelFqn::new(kind.as_str(), &name_bg, &sm.name, None);
                            let metadata = ModelMetadata {
                                context_length: sm
                                    .metadata
                                    .get("input_token_limit")
                                    .and_then(|v| v.as_u64()),
                                ..Default::default()
                            };
                            state_bg
                                .directory
                                .upsert(
                                    fqn,
                                    sm.capabilities.clone(),
                                    sm.specializations.clone(),
                                    metadata,
                                )
                                .await;
                        }
                    }
                }
            });
        }
    } else {
        // Disable: remove models from directory and instance from registry
        tracing::info!(provider = %name, "cloud provider disabled — removing models");
        state
            .directory
            .remove_provider(kind.as_str(), &name)
            .await;
        state.registry.remove_instance(&endpoint).await;
    }

    state
        .emit_event(
            "providers.updated",
            &serde_json::json!({
                "action": "toggle",
                "name": name,
                "enabled": new_enabled,
            })
            .to_string(),
        )
        .await;

    Json(serde_json::json!({
        "status": "ok",
        "name": name,
        "enabled": new_enabled,
    }))
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
    directory: &ModelDirectory,
    recommended: &HashMap<String, String>,
    state: &AppState,
) -> Vec<CapabilityStatus> {
    Capability::ALL
        .iter()
        .map(|cap| {
            let cap_str = cap.as_str();

            // Find all offerings that could serve this capability
            let serving_offerings: Vec<String> = state
                .providers
                .kinds()
                .filter(|kind| {
                    state
                        .providers
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
            let model_count = directory.models_with_capability(*cap).len();

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

fn build_directory_entries(
    dir: &ModelDirectory,
    instances: &HashMap<String, ServiceInstance>,
) -> Vec<DirectoryEntry> {
    dir.entries()
        .values()
        .map(|entry| {
            let model_identity = match &entry.parameters {
                Some(p) => format!("{}|{}", entry.model, p),
                None => entry.model.clone(),
            };

            let available_on: Vec<ModelPlacement> = entry
                .instances
                .iter()
                .filter_map(|fqn| {
                    // Match by: same source (offering kind) AND locator matches
                    // stone name. Cloud instances use "cloud:{name}" as stone.name
                    // but the FQN locator is just "{name}", so also try the
                    // "cloud:{locator}" format.
                    instances
                        .values()
                        .find(|i| {
                            i.kind.as_str() == fqn.source
                                && (i.stone.name == fqn.locator
                                    || i.stone.name == format!("cloud:{}", fqn.locator))
                        })
                        .map(|i| ModelPlacement {
                            stone: i.stone.name.clone(),
                            endpoint: i.endpoint.clone(),
                            offering: i.kind.as_str().to_string(),
                            loaded: i.models_loaded.iter().any(|lm| lm.name == entry.model),
                        })
                })
                .collect();

            DirectoryEntry {
                model: entry.model.clone(),
                parameters: entry.parameters.clone(),
                model_identity,
                capabilities: entry
                    .capabilities
                    .iter()
                    .map(|c| c.as_str().to_string())
                    .collect(),
                specializations: entry.specializations.clone(),
                metadata: entry.metadata.clone(),
                instances: entry.instances.iter().map(|fqn| fqn.fqn()).collect(),
                instance_count: entry.instances.len(),
                available_on,
            }
        })
        .collect()
}

/// `GET /api/directory` — model directory snapshot.
pub async fn get_directory(State(state): State<AppState>) -> Json<Vec<DirectoryEntry>> {
    let dir_snap = state.directory.snapshot().clone();
    let reg_snap = state.registry.snapshot().clone();
    Json(build_directory_entries(&dir_snap.directory, &reg_snap.instances))
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
