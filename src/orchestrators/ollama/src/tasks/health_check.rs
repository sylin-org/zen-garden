//! Health check task: periodic liveness checks on all Ollama instances.

use crate::app_state::AppState;
use crate::domain::types::InstanceHealth;
use crate::infra::ollama_client::OllamaClient;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Health check interval.
const CHECK_INTERVAL: Duration = Duration::from_secs(15);

/// Run the health check loop.
pub async fn run(state: AppState, client: OllamaClient, shutdown: CancellationToken) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(CHECK_INTERVAL) => {}
            _ = shutdown.cancelled() => return,
        }

        check_all(&state, &client).await;
    }
}

async fn check_all(state: &AppState, client: &OllamaClient) {
    let endpoints: Vec<(String, String, bool)> = {
        let instances = state.instances.read().await;
        instances
            .values()
            .map(|i| {
                (
                    i.endpoint.clone(),
                    i.stone_name.clone(),
                    i.health.is_routable(),
                )
            })
            .collect()
    };

    for (endpoint, stone_name, was_healthy) in endpoints {
        let healthy = client.health_check(&endpoint).await;

        if healthy && !was_healthy {
            tracing::info!(stone = %stone_name, "instance recovered");
            state
                .set_instance_health(&endpoint, InstanceHealth::Healthy)
                .await;
        } else if !healthy && was_healthy {
            tracing::warn!(stone = %stone_name, "instance became unhealthy");
            state
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
