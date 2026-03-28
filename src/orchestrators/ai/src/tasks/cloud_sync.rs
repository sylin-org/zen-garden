//! Cloud provider instance registration and periodic model refresh.
//!
//! Cloud providers are not discovered via topology/SSE — they are
//! configured via API keys in environment variables. This task:
//!
//! 1. At startup, creates a `ServiceInstance` for each registered cloud
//!    provider offering (with `priority: -10`).
//! 2. Periodically refreshes the model list from each provider (every 30 min).
//! 3. Marks instances unhealthy if the provider's API becomes unreachable.

use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;
use crate::domain::types::*;

/// How often to refresh cloud provider model lists.
const REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60); // 30 minutes

/// Default priority for cloud providers (below local/garden).
const CLOUD_PRIORITY: i32 = -10;

/// Background task: register and maintain cloud provider instances.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    // ── Initial registration ─────────────────────────────────────
    //
    // For each cloud provider in the catalog, probe it and create a
    // ServiceInstance so the routing engine can find it.
    register_cloud_instances(&state).await;

    // ── Periodic refresh ─────────────────────────────────────────
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(REFRESH_INTERVAL) => {}
        }

        refresh_cloud_models(&state).await;
    }

    tracing::info!("cloud sync task shutting down");
}

/// Create ServiceInstance entries for all cloud provider offerings in the catalog.
async fn register_cloud_instances(state: &AppState) {
    let cloud_kinds: Vec<OfferingKind> = state
        .catalog
        .iter()
        .filter(|o| o.offering_type().is_cloud())
        .map(|o| o.offering_type())
        .collect();

    if cloud_kinds.is_empty() {
        tracing::debug!("cloud_sync: no cloud providers registered");
        return;
    }

    tracing::info!(
        count = cloud_kinds.len(),
        "cloud_sync: registering cloud provider instances"
    );

    for kind in cloud_kinds {
        let offering = match state.catalog.get(kind) {
            Some(o) => o.clone(),
            None => continue,
        };

        // The "endpoint" for cloud providers is their base URL. We use
        // the offering kind as a synthetic endpoint key since cloud
        // providers don't have stone-local endpoints.
        let endpoint = format!("cloud://{kind}");

        // Probe to verify API key is valid.
        let probe = match offering.probe(&endpoint).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    provider = ?kind,
                    error = %e,
                    "cloud_sync: probe failed, skipping"
                );
                continue;
            }
        };

        // Enumerate available models.
        let models = match offering.enumerate(&endpoint).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    provider = ?kind,
                    error = %e,
                    "cloud_sync: enumerate failed"
                );
                vec![]
            }
        };

        let models_available: Vec<String> = models.iter().map(|m| m.name.clone()).collect();

        // Register model metadata.
        for m in &models {
            let info = ModelInfo {
                name: m.name.clone(),
                parameter_count: None,
                parameter_size: None,
                quantization_level: None,
                family: None,
                families: vec![],
                capabilities: m.capabilities.iter().map(|c| c.to_string()).collect(),
                format: None,
                size_disk: 0,
                vram_bytes: None,
                context_length: None,
            };
            state.upsert_model(info).await;
        }

        let instance = ServiceInstance {
            stone: Stone {
                id: format!("cloud-{kind}"),
                name: format!("cloud-{kind}"),
            },
            endpoint: endpoint.clone(),
            kind,
            gpu: Gpu {
                name: None,
                compute: ComputeType::Cpu, // Cloud manages its own hardware.
            },
            vram: Vram {
                total_bytes: 0,
                budget_bytes: 0,
                free_bytes: None,
            },
            health: InstanceHealth::Healthy,
            models_available,
            models_loaded: vec![], // Cloud doesn't report loaded state.
            capabilities: probe.capabilities,
            queue_depth: 0,
            last_seen: Instant::now(),
            metadata: probe.metadata,
            priority: CLOUD_PRIORITY,
        };

        state.upsert_instance(instance).await;

        tracing::info!(
            provider = ?kind,
            models = models.len(),
            "cloud_sync: registered cloud provider instance"
        );
    }
}

/// Refresh model lists for all cloud provider instances.
async fn refresh_cloud_models(state: &AppState) {
    let cloud_instances: Vec<(String, OfferingKind)> = {
        let instances = state.instances.read().await;
        instances
            .values()
            .filter(|i| i.kind.is_cloud())
            .map(|i| (i.endpoint.clone(), i.kind))
            .collect()
    };

    for (endpoint, kind) in cloud_instances {
        let offering = match state.catalog.get(kind) {
            Some(o) => o.clone(),
            None => continue,
        };

        // Probe for health.
        match offering.probe(&endpoint).await {
            Ok(_) => {
                state
                    .set_instance_health(&endpoint, InstanceHealth::Healthy)
                    .await;
            }
            Err(e) => {
                tracing::warn!(
                    provider = ?kind,
                    error = %e,
                    "cloud_sync: provider unreachable"
                );
                state
                    .set_instance_health(
                        &endpoint,
                        InstanceHealth::Unhealthy {
                            since: Instant::now(),
                            reason: e.to_string(),
                        },
                    )
                    .await;
                continue;
            }
        }

        // Refresh model list.
        match offering.enumerate(&endpoint).await {
            Ok(models) => {
                let names: Vec<String> = models.iter().map(|m| m.name.clone()).collect();
                state
                    .update_instance_models(&endpoint, names, vec![])
                    .await;
            }
            Err(e) => {
                tracing::warn!(
                    provider = ?kind,
                    error = %e,
                    "cloud_sync: model refresh failed"
                );
            }
        }
    }
}
