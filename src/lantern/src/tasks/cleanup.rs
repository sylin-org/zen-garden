//! Cleanup task — spawns TTL cleanup with proper error handling

use crate::AppState;

/// Spawn the TTL cleanup background task
pub fn spawn_ttl_cleanup(state: &AppState) -> tokio::task::JoinHandle<()> {
    let topology = state.topology.clone();
    let event_bus = state.event_bus.clone();

    tokio::spawn(async move {
        tracing::info!("TTL cleanup task started");
        crate::infra::ttl_cleanup::run_ttl_cleanup(topology, event_bus).await;
    })
}
