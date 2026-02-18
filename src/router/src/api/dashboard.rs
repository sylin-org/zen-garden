//! Dashboard: serves the single-page HTML dashboard and its data endpoints.

use crate::app_state::AppState;
use crate::infra::events::dashboard_sse_stream;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse},
    Json,
};
use serde_json::json;

/// The dashboard HTML — embedded at compile time (same pattern as Moss Portrait).
const DASHBOARD_HTML: &str = include_str!("../../assets/dashboard.html");

/// `GET /` — serve the dashboard SPA.
pub async fn get_dashboard() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

/// `GET /api/status` — full router status JSON (polled by dashboard).
pub async fn get_status(State(state): State<AppState>) -> impl IntoResponse {
    let instances = state.instances.read().await;
    let tiers = state.tiers.read().await;
    let models = state.models.read().await;
    let leases = state.leases.read().await;
    let metrics = state.metrics.read().await;
    let config = state.config.read().await;

    let stones: Vec<serde_json::Value> = instances
        .values()
        .map(|i| {
            let lease_info = leases.get_lease(&i.endpoint);
            json!({
                "stone_name": i.stone_name,
                "endpoint": i.endpoint,
                "gpu_name": i.gpu_name,
                "vram_total_mb": i.vram_total_bytes / 1_048_576,
                "vram_budget_mb": i.vram_budget_bytes / 1_048_576,
                "health": format!("{:?}", i.health),
                "healthy": i.health.is_routable(),
                "queue_depth": i.queue_depth,
                "models_available": i.models_available,
                "models_loaded": i.models_loaded,
                "lease": lease_info.map(|l| json!({
                    "model": l.model_name,
                    "remaining_secs": l.duration.as_secs().saturating_sub(l.granted_at.elapsed().as_secs()),
                })),
                "ollama_version": i.ollama_version,
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
            // Find which instances have this model
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
                "capabilities": m.capabilities,
                "vram_estimate_mb": m.vram_estimate_bytes / 1_048_576,
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

    Json(json!({
        "offering_name": state.offering_name,
        "uptime_secs": state.start_time.elapsed().as_secs(),
        "stones": stones,
        "tiers": tier_list,
        "models": model_list,
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
            "auto_pull": config.features.auto_pull,
            "delete_on_idle": config.features.delete_on_idle,
            "metrics_enabled": config.features.metrics_enabled,
        },
    }))
}

/// `GET /api/events` — SSE stream for real-time dashboard updates.
pub async fn get_events(State(state): State<AppState>) -> impl IntoResponse {
    dashboard_sse_stream(&state.dashboard_tx)
}

/// `GET /api/settings` — current configuration.
pub async fn get_settings(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().await;
    Json(json!(*config))
}

/// `POST /api/settings` — update configuration.
pub async fn post_settings(
    State(state): State<AppState>,
    Json(new_config): Json<crate::domain::types::RouterConfig>,
) -> impl IntoResponse {
    {
        let mut config = state.config.write().await;
        *config = new_config.clone();
    }

    // Update metrics engine enabled flag
    {
        let mut metrics = state.metrics.write().await;
        metrics.enabled = new_config.features.metrics_enabled;
    }

    // Persist to disk
    if let Err(e) = crate::infra::persistence::save_config(&state.data_dir, &new_config).await {
        tracing::warn!(error = %e, "failed to persist config");
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()})));
    }

    state.emit_event("config.updated", "{}").await;
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

/// `POST /api/metrics/reset` — reset all metrics counters.
pub async fn post_metrics_reset(State(state): State<AppState>) -> impl IntoResponse {
    let mut metrics = state.metrics.write().await;
    metrics.reset();
    state.emit_event("metrics.reset", "{}").await;
    Json(json!({"status": "ok"}))
}
