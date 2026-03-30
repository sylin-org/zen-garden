//! Cloud provider sync task.
//!
//! Registers cloud providers as ServiceInstance entries and periodically
//! re-enumerates their models via the `Provider` trait.
//!
//! No per-kind auth or enumerate logic lives here — all protocol knowledge
//! is in the provider implementations (`providers/*.rs`). This task just
//! orchestrates: iterate configs, call provider.probe(), call provider.enumerate(),
//! register results.

use crate::app_state::AppState;
use crate::catalog::ProviderContext;
use crate::domain::types::*;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// How often to re-enumerate cloud provider models.
const CLOUD_REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// Initial delay before first cloud sync (let local discovery go first).
const STARTUP_DELAY: Duration = Duration::from_secs(10);

/// Run the cloud provider sync loop.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    tokio::select! {
        _ = tokio::time::sleep(STARTUP_DELAY) => {}
        _ = shutdown.cancelled() => return,
    }

    sync_all(&state).await;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(CLOUD_REFRESH_INTERVAL) => {}
            _ = shutdown.cancelled() => return,
        }
        sync_all(&state).await;
    }
}

/// Register/refresh all enabled cloud providers.
async fn sync_all(state: &AppState) {
    let configs: Vec<_> = {
        let store = state.cloud_store.read().await;
        store
            .all()
            .iter()
            .filter(|p| p.enabled && !p.api_key.is_empty())
            .cloned()
            .collect()
    };

    for config in &configs {
        let kind = config.kind;
        let endpoint = config.base_url.clone();
        let name = config.name.clone();

        // Look up the provider impl
        let provider = match state.providers.get(kind) {
            Some(p) => p,
            None => {
                tracing::debug!(
                    provider = %name,
                    kind = %kind,
                    "no provider registered for this kind, skipping"
                );
                continue;
            }
        };

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
                compute: ComputeType::Cpu,
            },
            vram: Vram {
                total_bytes: 0,
                budget_bytes: 0,
                free_bytes: None,
            },
            health: InstanceHealth::Profiling,
            models_available: config.cached_models.clone(),
            models_loaded: vec![],
            capabilities: config.capabilities.clone(),
            queue_depth: 0,
            last_seen: Instant::now(),
            metadata: serde_json::json!({
                "cloud": true,
                "provider": name,
            }),
            priority: config.priority,
        };

        state.upsert_instance(instance).await;

        // Register cached models for immediate availability (with provider-level caps)
        for model_name in &config.cached_models {
            let fqn = ModelFqn::new(kind.as_str(), &name, model_name, None);
            state
                .directory_upsert(
                    fqn,
                    config.capabilities.clone(),
                    vec!["cloud".to_string()],
                    ModelMetadata::default(),
                )
                .await;
        }

        // Build provider context
        let ctx = ProviderContext {
            endpoint: endpoint.clone(),
            model: None,
            api_key: Some(config.api_key.clone()),
        };

        // Probe
        match provider.probe(&ctx).await {
            Ok(_) => {
                state
                    .set_instance_health(&endpoint, InstanceHealth::Healthy)
                    .await;

                // Enumerate via Provider trait — returns ServiceModel with per-model capabilities
                match provider.enumerate(&ctx).await {
                    Ok(service_models) => {
                        let model_names: Vec<String> =
                            service_models.iter().map(|m| m.name.clone()).collect();
                        let count = model_names.len();

                        state
                            .update_instance_models(&endpoint, model_names.clone(), vec![])
                            .await;

                        // Clear stale directory entries for this provider before
                        // re-registering with per-model capabilities. This prevents
                        // the union-merge from retaining stale provider-level caps.
                        state
                            .directory_remove_provider(kind.as_str(), &name)
                            .await;

                        // Register each model with its own capabilities (not provider-level)
                        for sm in &service_models {
                            let fqn = ModelFqn::new(kind.as_str(), &name, &sm.name, None);
                            let metadata = ModelMetadata {
                                context_length: sm
                                    .metadata
                                    .get("input_token_limit")
                                    .and_then(|v| v.as_u64()),
                                ..Default::default()
                            };
                            state
                                .directory_upsert(
                                    fqn,
                                    sm.capabilities.clone(),
                                    sm.specializations.clone(),
                                    metadata,
                                )
                                .await;
                        }

                        // Update cache
                        {
                            let mut store = state.cloud_store.write().await;
                            store.update_cached_models(&name, model_names);
                            if let Err(e) = store.save().await {
                                tracing::warn!(error = %e, "failed to persist cloud model cache");
                            }
                        }

                        tracing::info!(
                            provider = %name,
                            kind = %kind,
                            models = count,
                            "cloud provider synced"
                        );
                    }
                    Err(e) => {
                        tracing::info!(
                            provider = %name,
                            kind = %kind,
                            cached = config.cached_models.len(),
                            error = %e,
                            "cloud provider healthy but enumerate failed (using cache)"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    provider = %name,
                    kind = %kind,
                    cached = config.cached_models.len(),
                    error = %e,
                    "cloud provider probe failed (using cache)"
                );
            }
        }
    }
}
