//! Health check task — periodic probe of all discovered instances.
//!
//! Dispatches through the `Offering` trait's `probe()` method.
//! Marks instances unhealthy after 3 consecutive failures.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;
use crate::domain::types::InstanceHealth;

/// Consecutive failures before marking unhealthy.
const MAX_FAILURES: u32 = 3;
/// Health check interval for local/garden instances.
const LOCAL_INTERVAL: Duration = Duration::from_secs(15);

/// Background task: periodic health checks on all instances.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    let mut failure_counts: HashMap<String, u32> = HashMap::new();

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(LOCAL_INTERVAL) => {}
        }

        let endpoints: Vec<(String, crate::domain::types::OfferingKind)> = {
            let instances = state.instances.read().await;
            instances
                .values()
                .map(|i| (i.endpoint.clone(), i.kind))
                .collect()
        };

        for (endpoint, kind) in endpoints {
            let offering = match state.catalog.get(kind) {
                Some(o) => o.clone(),
                None => continue,
            };

            match offering.probe(&endpoint).await {
                Ok(_probe_result) => {
                    failure_counts.remove(&endpoint);
                    state
                        .set_instance_health(&endpoint, InstanceHealth::Healthy)
                        .await;
                }
                Err(e) => {
                    let count = failure_counts.entry(endpoint.clone()).or_default();
                    *count += 1;

                    if *count >= MAX_FAILURES {
                        tracing::warn!(
                            endpoint = %endpoint,
                            failures = *count,
                            error = %e,
                            "marking instance unhealthy"
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
                    }
                }
            }
        }
    }

    tracing::info!("health check task shutting down");
}
