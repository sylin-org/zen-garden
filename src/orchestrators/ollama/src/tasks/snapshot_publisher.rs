//! Snapshot publisher: builds a read-only dashboard snapshot every 2 seconds.
//!
//! The dashboard reads from the `watch` channel (lock-free on the HTTP path),
//! ensuring the request handler never competes for locks with the proxy or
//! background tasks.  This is the SoC boundary: background tasks own their
//! domain state; the dashboard reads a pre-built projection.

use crate::app_state::AppState;
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// How often to publish a new snapshot.
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(2);

/// Run the snapshot publisher loop.
pub async fn run(
    state: AppState,
    tx: watch::Sender<serde_json::Value>,
    shutdown: CancellationToken,
) {
    loop {
        let snapshot = build_snapshot(&state).await;
        let _ = tx.send(snapshot);

        tokio::select! {
            _ = tokio::time::sleep(SNAPSHOT_INTERVAL) => {}
            _ = shutdown.cancelled() => return,
        }
    }
}

/// Build the full status JSON from current domain state.
///
/// Acquires read locks sequentially — acceptable here because this runs
/// in a dedicated background task, NOT on the request hot-path.  The locks
/// are held briefly while data is read; the JSON is built from owned copies.
async fn build_snapshot(state: &AppState) -> serde_json::Value {
    let instances = state.instances.read().await;
    let tiers = state.tiers.read().await;
    let models = state.models.read().await;
    let leases = state.leases.read().await;
    let metrics = state.metrics.read().await;
    let config = state.config.read().await;
    let placement = state.placement.read().await;
    let depths = state.queue_depths.read().await;
    let bench_run = state.benchmark_run.read().await;

    // Pre-compute per-stone tok/s (generation + roundtrip)
    let tps_gen = metrics.tokens_per_sec_by_stone(300);
    let tps_rt = metrics.roundtrip_tokens_per_sec_by_stone(300);

    let stones: Vec<serde_json::Value> = instances
        .values()
        .map(|i| {
            let lease_info = leases.get_lease(&i.endpoint);
            // Read live queue depth from atomic (more accurate than cached field)
            let queue_depth = depths
                .get(&i.endpoint)
                .map(|c| c.load(Ordering::Relaxed))
                .unwrap_or(i.queue_depth);

            json!({
                "stone_name": i.stone_name,
                "endpoint": i.endpoint,
                "gpu_name": i.gpu_name,
                "vram_total_mb": i.vram_total_bytes / 1_048_576,
                "vram_budget_mb": i.vram_budget_bytes / 1_048_576,
                "health": format!("{:?}", i.health),
                "healthy": i.health.is_routable(),
                "queue_depth": queue_depth,
                "models_available": i.models_available,
                "models_loaded": i.models_loaded,
                "lease": lease_info.map(|l| json!({
                    "model": l.model_name,
                    "remaining_secs": l.duration.as_secs().saturating_sub(l.granted_at.elapsed().as_secs()),
                })),
                "ollama_version": i.ollama_version,
                "tokens_per_sec": tps_gen.get(&i.stone_name).copied().map(|v| (v * 10.0).round() / 10.0),
                "tokens_per_sec_roundtrip": tps_rt.get(&i.stone_name).copied().map(|v| (v * 10.0).round() / 10.0),
                "tokens_per_sec_cumulative": metrics.cumulative_tokens_per_sec(&i.stone_name).map(|v| (v * 10.0).round() / 10.0),
                "tokens_per_sec_cumulative_roundtrip": metrics.cumulative_roundtrip_tokens_per_sec(&i.stone_name).map(|v| (v * 10.0).round() / 10.0),
            })
        })
        .collect();

    let tier_list: Vec<serde_json::Value> = tiers
        .iter()
        .map(|t| {
            json!({
                "label": t.label,
                "vram_gb": t.vram_bytes / 1_073_741_824,
                "instances": t.instance_endpoints,
            })
        })
        .collect();

    let model_list: Vec<serde_json::Value> = models
        .values()
        .map(|m| {
            let on_stones: Vec<&str> = instances
                .values()
                .filter(|i| i.models_available.iter().any(|name| name == &m.name))
                .map(|i| i.stone_name.as_str())
                .collect();
            let loaded_on: Vec<&str> = instances
                .values()
                .filter(|i| i.models_loaded.iter().any(|l| l.name == m.name))
                .map(|i| i.stone_name.as_str())
                .collect();

            json!({
                "name": m.name,
                "parameter_size": m.parameter_size,
                "quantization_level": m.quantization_level,
                "family": m.family,
                "format": m.format,
                "capabilities": m.capabilities,
                "vram_mb": m.vram_bytes.map(|v| v / 1_048_576),
                "size_disk_mb": m.size_disk / 1_048_576,
                "on_stones": on_stones,
                "loaded_on": loaded_on,
            })
        })
        .collect();

    let window = 300; // 5 min
    let avg_response_ms = metrics
        .avg_response_ns(window)
        .map(|ns| ns / 1_000_000)
        .unwrap_or(0);
    let top_models = metrics.top_models(5);

    let demand = metrics.demand_shares(window);
    let demand_json: serde_json::Value = demand
        .iter()
        .map(|(m, s)| (m.clone(), json!((s * 100.0).round() / 100.0)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    // Flip placement from model→[endpoints] to stone_name→[models]
    let mut assignments_by_stone: HashMap<String, Vec<String>> = HashMap::new();
    for (model, endpoints) in &placement.assignments {
        for ep in endpoints {
            let name = instances
                .get(ep)
                .map(|i| i.stone_name.clone())
                .unwrap_or_else(|| ep.clone());
            assignments_by_stone
                .entry(name)
                .or_default()
                .push(model.clone());
        }
    }

    json!({
        "offering_name": state.offering_name,
        "uptime_secs": state.start_time.elapsed().as_secs(),
        "stones": stones,
        "tiers": tier_list,
        "models": model_list,
        "placement": {
            "assignments": assignments_by_stone,
            "computed_at": placement.computed_at,
            "stable": placement.stable,
        },
        "demand": demand_json,
        "metrics": {
            "requests_total": metrics.requests_total,
            "tokens_in": metrics.tokens_in_total,
            "tokens_out": metrics.tokens_out_total,
            "errors": metrics.errors_total,
            "requests_5min": metrics.requests_in_window(window),
            "avg_response_ms": avg_response_ms,
            "top_models": top_models,
            "enabled": metrics.enabled,
        },
        "config": {
            "auto_pull_mode": format!("{}", config.features.auto_pull_mode),
            "delete_on_idle": config.features.delete_on_idle,
            "metrics_enabled": config.features.metrics_enabled,
        },
        "benchmark": &*bench_run,
    })
}
