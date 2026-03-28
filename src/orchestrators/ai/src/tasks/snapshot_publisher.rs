//! Snapshot publisher — builds dashboard-ready JSON from live state.
//!
//! Periodically produces a comprehensive JSON snapshot and:
//! 1. Publishes to the `watch` channel (for `/api/status` GET)
//! 2. Emits as a `status.snapshot` event on the dashboard broadcast
//!    channel (for SSE streaming — eliminates polling)

use std::time::Duration;

use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::app_state::{AppState, DashboardEvent};

const PUBLISH_INTERVAL: Duration = Duration::from_secs(3);

pub async fn run(
    state: AppState,
    snapshot_tx: watch::Sender<serde_json::Value>,
    shutdown: CancellationToken,
) {
    // Publish immediately on startup.
    let initial = build_snapshot(&state).await;
    let _ = snapshot_tx.send(initial.clone());
    emit_snapshot_event(&state, &initial);

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(PUBLISH_INTERVAL) => {}
        }

        let snapshot = build_snapshot(&state).await;
        let _ = snapshot_tx.send(snapshot.clone());
        emit_snapshot_event(&state, &snapshot);
    }

    tracing::info!("snapshot publisher shutting down");
}

/// Emit the snapshot as an SSE event so the React frontend receives
/// it on the single EventSource connection (no polling needed).
fn emit_snapshot_event(state: &AppState, snapshot: &serde_json::Value) {
    let _ = state.dashboard_tx.send(DashboardEvent {
        event_type: "status.snapshot".to_string(),
        data: snapshot.to_string(),
    });
}

/// Build a comprehensive JSON snapshot of the orchestrator state.
///
/// This is the single source of truth for the dashboard. Every field
/// the frontend needs must be included here.
async fn build_snapshot(state: &AppState) -> serde_json::Value {
    let instances = state.instances.read().await;
    let models = state.models.read().await;
    let tiers = state.tiers.read().await;
    let config = state.config.read().await;
    let placement = state.placement.read().await;
    let benchmark = state.benchmark_run.read().await;
    let recommended = state.recommended_models.read().await;
    let jobs = state.jobs.read().await;
    let vram_budgets = state.vram_budgets.read().await;

    let metrics_snapshot = {
        let metrics = state.metrics.read().await;
        metrics.snapshot()
    };

    let demand_shares = {
        let metrics = state.metrics.read().await;
        metrics.demand_shares(300) // 5 minute window
    };

    // Full instance data — include model lists, loaded models, VRAM details.
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
                "models_available": inst.models_available,
                "models_loaded": inst.models_loaded,
                "capabilities": inst.capabilities,
                "queue_depth": inst.queue_depth,
                "priority": inst.priority,
                "metadata": inst.metadata,
            })
        })
        .collect();

    // Full model registry.
    let model_list: Vec<serde_json::Value> = models
        .values()
        .map(|m| {
            serde_json::json!({
                "name": m.name,
                "parameter_count": m.parameter_count,
                "parameter_size": m.parameter_size,
                "quantization_level": m.quantization_level,
                "family": m.family,
                "capabilities": m.capabilities,
                "format": m.format,
                "size_disk": m.size_disk,
                "vram_bytes": m.vram_bytes,
                "context_length": m.context_length,
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
        "models": model_list,
        "offering_counts": offering_counts,
        "tiers": *tiers,
        "vram_budgets": *vram_budgets,
        "metrics": metrics_snapshot,
        "demand_shares": demand_shares,
        "placement": *placement,
        "benchmark": {
            "id": benchmark.id,
            "status": benchmark.status,
            "started_at": benchmark.started_at,
            "completed_at": benchmark.completed_at,
            "stones": benchmark.stones,
            "gpu_matrix": benchmark.gpu_matrix,
            "error": benchmark.error,
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
