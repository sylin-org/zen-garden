//! Health check task: periodic liveness probes on all AI service instances.
//!
//! Uses the Offering trait's `probe()` method for service-specific health
//! checks — each offering type knows its own health endpoint.

use crate::app_state::AppState;
use crate::domain::types::InstanceHealth;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Health check interval.
const CHECK_INTERVAL: Duration = Duration::from_secs(15);

/// Run the health check loop.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(CHECK_INTERVAL) => {}
            _ = shutdown.cancelled() => return,
        }

        check_all(&state).await;
    }
}

async fn check_all(state: &AppState) {
    let snapshot: Vec<_> = {
        let snap = state.registry.snapshot().clone();
        snap.instances
            .values()
            .map(|i| (i.endpoint.clone(), i.stone.name.clone(), i.kind, i.health.is_routable()))
            .collect()
    };

    for (endpoint, stone_name, kind, was_healthy) in snapshot {
        let adapter = match state.providers.get(kind) {
            Some(a) => a,
            None => continue,
        };

        // Cloud providers need their API key for probe
        let api_key = if kind.is_cloud() {
            let store = state.cloud_store.read().await;
            store
                .all()
                .iter()
                .find(|p| p.base_url == endpoint && p.kind == kind)
                .map(|p| p.api_key.clone())
        } else {
            None
        };

        let ctx = crate::catalog::ProviderContext {
            endpoint: endpoint.clone(),
            model: None,
            api_key,
        };
        let healthy = adapter.probe(&ctx).await.is_ok();

        if healthy && !was_healthy {
            tracing::info!(stone = %stone_name, kind = %kind, "instance recovered");
            state
                .registry
                .set_instance_health(&endpoint, InstanceHealth::Healthy)
                .await;
        } else if !healthy && was_healthy {
            tracing::warn!(stone = %stone_name, kind = %kind, "instance became unhealthy");
            state
                .registry
                .set_instance_health(
                    &endpoint,
                    InstanceHealth::Unhealthy {
                        since: Instant::now(),
                        reason: "health check failed".to_string(),
                    },
                )
                .await;
        }
    }
}
