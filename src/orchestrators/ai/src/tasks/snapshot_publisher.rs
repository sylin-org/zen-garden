//! Snapshot publisher — builds dashboard-ready JSON from live state.
//!
//! Periodically produces a comprehensive JSON snapshot of the orchestrator's
//! state and publishes it to a `watch` channel. The dashboard `/api/status`
//! endpoint reads from this channel (zero computation at request time).

use std::time::Duration;

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;

/// How often to rebuild the snapshot.
const PUBLISH_INTERVAL: Duration = Duration::from_secs(3);

/// Background task: periodic snapshot aggregation.
pub async fn run(
    state: AppState,
    snapshot_tx: watch::Sender<serde_json::Value>,
    shutdown: CancellationToken,
) {
    // Publish immediately so /api/status doesn't return Null for the
    // first PUBLISH_INTERVAL seconds after startup.
    let initial = build_snapshot(&state).await;
    let _ = snapshot_tx.send(initial);

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(PUBLISH_INTERVAL) => {}
        }

        let snapshot = build_snapshot(&state).await;
        let _ = snapshot_tx.send(snapshot);
    }

    tracing::info!("snapshot publisher shutting down");
}

/// Build a comprehensive JSON snapshot of the orchestrator state.
async fn build_snapshot(state: &AppState) -> serde_json::Value {
    let instances = state.instances.read().await;
    let models = state.models.read().await;
    let tiers = state.tiers.read().await;
    let config = state.config.read().await;
    let placement = state.placement.read().await;
    let benchmark = state.benchmark_run.read().await;
    let recommended = state.recommended_models.read().await;
    let jobs = state.jobs.read().await;

    let metrics_snapshot = {
        let metrics = state.metrics.read().await;
        metrics.snapshot()
    };

    let instance_list: Vec<serde_json::Value> = instances
        .values()
        .map(|inst| {
            serde_json::json!({
                "stone": inst.stone,
                "endpoint": inst.endpoint,
                "kind": inst.kind,
                "gpu": inst.gpu,
                "vram": inst.vram,
                "health": inst.health,
                "models_available": inst.models_available.len(),
                "models_loaded": inst.models_loaded.len(),
                "capabilities": inst.capabilities,
                "queue_depth": inst.queue_depth,
                "priority": inst.priority,
            })
        })
        .collect();

    let offering_counts: std::collections::HashMap<String, usize> = instances
        .values()
        .fold(std::collections::HashMap::new(), |mut acc, inst| {
            *acc.entry(inst.kind.to_string()).or_default() += 1;
            acc
        });

    serde_json::json!({
        "orchestrator": {
            "version": env!("CARGO_PKG_VERSION"),
            "uptime_secs": state.start_time.elapsed().as_secs(),
            "offerings_registered": state.catalog.len(),
            "instances_discovered": instances.len(),
            "models_known": models.len(),
        },
        "instances": instance_list,
        "offering_counts": offering_counts,
        "tiers": *tiers,
        "metrics": metrics_snapshot,
        "placement": *placement,
        "benchmark": {
            "status": benchmark.status,
            "id": benchmark.id,
        },
        "recommended_models": *recommended,
        "config": {
            "auto_pull_mode": config.features.auto_pull_mode,
            "delete_on_idle": config.features.delete_on_idle,
            "metrics_enabled": config.features.metrics_enabled,
            "pins": config.features.pins,
        },
        "jobs": jobs.iter().collect::<Vec<_>>(),
    })
}
