//! Reconciliation task: periodic polling of all Ollama instances
//! to detect model drift (manual pulls/deletes/evictions).

use crate::app_state::AppState;
use crate::domain::reconciliation;
use crate::domain::types::LoadedModel;
use crate::infra::ollama_client::OllamaClient;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Poll interval for reconciliation.
const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Run the reconciliation loop.
pub async fn run(state: AppState, client: OllamaClient, shutdown: CancellationToken) {
    // Wait for initial discovery to complete before starting reconciliation
    tokio::time::sleep(Duration::from_secs(10)).await;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
            _ = shutdown.cancelled() => return,
        }

        reconcile_all(&state, &client).await;

        // Also reap expired leases
        let reaped = state.leases.write().await.reap_expired();
        if reaped > 0 {
            tracing::debug!(count = reaped, "reaped expired leases");
        }
    }
}

/// Poll all instances and apply drifts.
async fn reconcile_all(state: &AppState, client: &OllamaClient) {
    let endpoints: Vec<(String, String)> = {
        let instances = state.instances.read().await;
        instances
            .values()
            .map(|i| (i.endpoint.clone(), i.stone_name.clone()))
            .collect()
    };

    for (endpoint, stone_name) in endpoints {
        reconcile_instance(state, client, &endpoint, &stone_name).await;
    }
}

/// Reconcile a single instance.
async fn reconcile_instance(
    state: &AppState,
    client: &OllamaClient,
    endpoint: &str,
    stone_name: &str,
) {
    // Fetch fresh state from Ollama
    let (tags_result, ps_result) =
        tokio::join!(client.get_tags(endpoint), client.get_ps(endpoint),);

    let tags = match tags_result {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(stone = %stone_name, error = %e, "reconciliation: failed to query tags");
            // Mark unhealthy
            state
                .set_instance_health(
                    endpoint,
                    crate::domain::types::InstanceHealth::Unhealthy {
                        since: std::time::Instant::now(),
                        reason: e.to_string(),
                    },
                )
                .await;
            return;
        }
    };

    let ps = match ps_result {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(stone = %stone_name, error = %e, "reconciliation: failed to query ps");
            return;
        }
    };

    // If we got here, the instance is healthy
    state
        .set_instance_health(endpoint, crate::domain::types::InstanceHealth::Healthy)
        .await;

    let fresh_available: Vec<String> = tags.models.iter().map(|t| t.name.clone()).collect();
    let fresh_loaded: Vec<LoadedModel> = ps
        .models
        .iter()
        .map(|m| LoadedModel {
            name: m.name.clone(),
            size_vram: m.size_vram,
            expires_at: m.expires_at.clone(),
        })
        .collect();

    // Diff against registry
    let drifts = {
        let instances = state.instances.read().await;
        if let Some(instance) = instances.get(endpoint) {
            reconciliation::diff_instance(instance, &fresh_available, &fresh_loaded)
        } else {
            return;
        }
    };

    if !drifts.is_empty() {
        tracing::info!(
            stone = %stone_name,
            count = drifts.len(),
            "reconciliation detected drifts"
        );

        for drift in &drifts {
            match drift {
                reconciliation::RegistryDrift::ModelAppeared { model_name, .. } => {
                    tracing::info!(stone = %stone_name, model = %model_name, "model appeared (pulled outside router)");
                    // Profile the new model
                    if let Ok(show) = client.show_model(endpoint, model_name).await {
                        let tag = tags.models.iter().find(|t| t.name == *model_name);
                        let details = tag.and_then(|t| t.details.as_ref());
                        let param_count = show.parameter_count();
                        let quant = details.and_then(|d| d.quantization_level.as_deref());
                        let format = details.and_then(|d| d.format.clone());

                        // vram_bytes = None for a newly appeared model that
                        // is not yet loaded.  It will be set to a real value
                        // when Ollama loads it (ModelLoaded drift).
                        state
                            .upsert_model(crate::domain::types::ModelInfo {
                                name: model_name.clone(),
                                parameter_count: param_count,
                                parameter_size: details.and_then(|d| d.parameter_size.clone()),
                                quantization_level: quant.map(|s| s.to_string()),
                                family: details.and_then(|d| d.family.clone()),
                                families: details.map(|d| d.families.clone()).unwrap_or_default(),
                                capabilities: show.capabilities,
                                format,
                                size_disk: tag.map(|t| t.size).unwrap_or(0),
                                vram_bytes: None,
                            })
                            .await;
                    }
                }
                reconciliation::RegistryDrift::ModelDisappeared { model_name, .. } => {
                    tracing::info!(stone = %stone_name, model = %model_name, "model disappeared (deleted outside router)");
                }
                reconciliation::RegistryDrift::ModelLoaded {
                    model_name,
                    size_vram,
                    ..
                } => {
                    tracing::debug!(stone = %stone_name, model = %model_name, vram = size_vram, "model loaded into VRAM");
                    // Update authoritative VRAM in model registry
                    let mut models = state.models.write().await;
                    if let Some(info) = models.get_mut(model_name.as_str()) {
                        info.vram_bytes = Some(*size_vram);
                    }
                }
                reconciliation::RegistryDrift::ModelUnloaded { model_name, .. } => {
                    tracing::debug!(stone = %stone_name, model = %model_name, "model unloaded from VRAM");
                }
                reconciliation::RegistryDrift::VramChanged {
                    model_name,
                    new_vram,
                    ..
                } => {
                    tracing::info!(stone = %stone_name, model = %model_name, new_vram = new_vram, "VRAM usage changed");
                    let mut models = state.models.write().await;
                    if let Some(info) = models.get_mut(model_name.as_str()) {
                        info.vram_bytes = Some(*new_vram);
                    }
                }
            }
        }

        // Apply the fresh state
        state
            .update_instance_models(endpoint, fresh_available, fresh_loaded)
            .await;

        state.emit_event("registry.reconciled", "{}").await;
    } else {
        // No drift — just update last_seen and load state
        state
            .update_instance_models(endpoint, fresh_available, fresh_loaded)
            .await;
    }
}
