//! Cloud provider sync task.
//!
//! Registers cloud providers as ServiceInstance entries and periodically
//! re-enumerates their models. Cloud providers differ from local offerings:
//! - They use `DiscoveryConfig::Configured` (no topology discovery)
//! - Their "endpoint" is the base URL (e.g., "https://api.openai.com")
//! - Their models come from the provider's /models API or cached_models
//! - They run at priority -10 (cloud fallback)

use crate::app_state::AppState;
use crate::domain::types::*;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// How often to re-enumerate cloud provider models.
const CLOUD_REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60); // 30 minutes

/// Initial delay before first cloud sync (let local discovery go first).
const STARTUP_DELAY: Duration = Duration::from_secs(10);

/// Run the cloud provider sync loop.
///
/// On startup: registers all configured providers as ServiceInstance entries
/// using cached_models from disk. Then periodically re-enumerates to refresh.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    // Initial delay
    tokio::select! {
        _ = tokio::time::sleep(STARTUP_DELAY) => {}
        _ = shutdown.cancelled() => return,
    }

    // Register from cache on startup
    register_cloud_providers(&state).await;

    // Periodic re-enumeration
    loop {
        tokio::select! {
            _ = tokio::time::sleep(CLOUD_REFRESH_INTERVAL) => {}
            _ = shutdown.cancelled() => return,
        }

        refresh_cloud_models(&state).await;
    }
}

/// Register all enabled cloud providers as ServiceInstance + ModelInfo entries
/// from their cached model lists (no API calls — uses disk cache).
async fn register_cloud_providers(state: &AppState) {
    let store = state.cloud_store.read().await;

    for provider in store.all() {
        if !provider.enabled || provider.api_key.is_empty() {
            continue;
        }

        let kind = provider.kind;
        let endpoint = provider.base_url.clone();
        let name = provider.name.clone();

        // Create ServiceInstance for this cloud provider
        let instance = ServiceInstance {
            stone: Stone {
                id: format!("cloud-{name}"),
                name: format!("cloud:{name}"),
            },
            endpoint: endpoint.clone(),
            kind,
            gpu: Gpu {
                name: None,
                compute: ComputeType::Cpu, // cloud — not applicable
            },
            vram: Vram {
                total_bytes: 0,
                budget_bytes: 0,
                free_bytes: None,
            },
            health: InstanceHealth::Profiling,
            models_available: provider.cached_models.clone(),
            models_loaded: vec![],
            capabilities: provider.capabilities.clone(),
            queue_depth: 0,
            last_seen: Instant::now(),
            metadata: serde_json::json!({
                "cloud": true,
                "provider": name,
            }),
            priority: provider.priority,
        };

        state.upsert_instance(instance).await;

        // Register cached models in global model registry
        for model_name in &provider.cached_models {
            let info = ModelInfo {
                name: model_name.clone(),
                parameter_count: None,
                parameter_size: None,
                quantization_level: None,
                family: None,
                families: vec![],
                capabilities: provider
                    .capabilities
                    .iter()
                    .map(|c| c.as_str().to_string())
                    .collect(),
                format: None,
                size_disk: 0,
                vram_bytes: None,
                context_length: None,
            };
            state.upsert_model(info).await;
        }

        // Probe to check key validity
        if let Some(adapter) = state.registry.get(kind) {
            match adapter.probe(&endpoint).await {
                Ok(_) => {
                    state
                        .set_instance_health(&endpoint, InstanceHealth::Healthy)
                        .await;
                    tracing::info!(
                        provider = %name,
                        models = provider.cached_models.len(),
                        "cloud provider registered (healthy)"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        provider = %name,
                        error = %e,
                        "cloud provider registered but probe failed (unhealthy)"
                    );
                }
            }
        } else {
            let model_count = provider.cached_models.len();
            tracing::info!(
                provider = %name,
                models = model_count,
                "cloud provider registered from cache (no adapter)"
            );
        }
    }
}

/// Re-enumerate models from all healthy cloud providers and update the cache.
async fn refresh_cloud_models(state: &AppState) {
    let providers: Vec<(String, OfferingKind, String)> = {
        let store = state.cloud_store.read().await;
        store
            .all()
            .iter()
            .filter(|p| p.enabled && !p.api_key.is_empty())
            .map(|p| (p.name.clone(), p.kind, p.base_url.clone()))
            .collect()
    };

    for (name, kind, endpoint) in providers {
        let adapter = match state.registry.get(kind) {
            Some(a) => a,
            None => continue,
        };

        match adapter.enumerate(&endpoint).await {
            Ok(models) => {
                let model_names: Vec<String> = models.iter().map(|m| m.name.clone()).collect();
                let count = model_names.len();

                // Update instance model list
                state
                    .update_instance_models(&endpoint, model_names.clone(), vec![])
                    .await;

                // Register each model
                for sm in &models {
                    let info = ModelInfo {
                        name: sm.name.clone(),
                        parameter_count: None,
                        parameter_size: None,
                        quantization_level: None,
                        family: None,
                        families: vec![],
                        capabilities: sm
                            .capabilities
                            .iter()
                            .map(|c| c.as_str().to_string())
                            .collect(),
                        format: None,
                        size_disk: 0,
                        vram_bytes: None,
                        context_length: None,
                    };
                    state.upsert_model(info).await;
                }

                // Update cache on disk
                {
                    let mut store = state.cloud_store.write().await;
                    store.update_cached_models(&name, model_names);
                    if let Err(e) = store.save().await {
                        tracing::warn!(error = %e, "failed to persist cloud model cache");
                    }
                }

                tracing::info!(
                    provider = %name,
                    models = count,
                    "refreshed cloud provider models"
                );
            }
            Err(e) => {
                tracing::warn!(
                    provider = %name,
                    error = %e,
                    "failed to enumerate cloud provider models"
                );
            }
        }
    }
}
