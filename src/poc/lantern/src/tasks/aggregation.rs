//! Aggregation task — spawns the Moss portrait poller

use crate::AppState;

/// Spawn the aggregation background task
pub fn spawn_aggregation(state: &AppState) -> tokio::task::JoinHandle<()> {
    let topology = state.topology.clone();
    let event_bus = state.event_bus.clone();
    let client = state.http_client.clone();

    tokio::spawn(async move {
        tracing::info!("Aggregation task started (15s poll interval)");
        crate::infra::moss_aggregator::run_aggregation(topology, client, event_bus).await;
    })
}
