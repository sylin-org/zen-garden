//! Dashboard API handlers — status, events, settings, offerings, jobs.
//!
//! Also serves the embedded React SPA (built from web/ via rust-embed).

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::stream::Stream;
use rust_embed::Embed;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::app_state::AppState;

/// Embedded dashboard SPA assets (built from web/dist/).
#[derive(Embed)]
#[folder = "web/dist/"]
struct DashboardAssets;

/// Serve embedded static files. Falls back to index.html for SPA routing.
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try exact file match first (JS, CSS, images).
    if let Some(content) = DashboardAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return Response::builder()
            .header(header::CONTENT_TYPE, mime.as_ref())
            .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
            .body(Body::from(content.data.to_vec()))
            .unwrap_or_default();
    }

    // SPA fallback: serve index.html for all unmatched routes.
    match DashboardAssets::get("index.html") {
        Some(content) => Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(content.data.to_vec()))
            .unwrap_or_default(),
        None => (StatusCode::NOT_FOUND, "Dashboard not built").into_response(),
    }
}

/// `GET /api/status` — full orchestrator snapshot (from watch channel).
pub async fn status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let snapshot = state.snapshot_rx.borrow().clone();
    Json(snapshot)
}

/// `GET /api/events` — SSE stream of dashboard events.
pub async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.dashboard_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(event) => Some(Ok(Event::default()
            .event(event.event_type)
            .data(event.data))),
        Err(_) => None, // Lagged — skip silently.
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// `GET /api/settings` — current router configuration.
pub async fn get_settings(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.config.read().await;
    Json(serde_json::json!({
        "features": {
            "auto_pull_mode": config.features.auto_pull_mode,
            "delete_on_idle": config.features.delete_on_idle,
            "metrics_enabled": config.features.metrics_enabled,
            "pins": config.features.pins,
        },
        "stones": config.stones,
    }))
}

/// `POST /api/settings` — update router configuration.
pub async fn post_settings(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> StatusCode {
    let mut config = state.config.write().await;

    if let Some(features) = body.get("features") {
        if let Some(mode) = features.get("auto_pull_mode").and_then(|v| {
            serde_json::from_value(v.clone()).ok()
        }) {
            config.features.auto_pull_mode = mode;
        }
        if let Some(v) = features.get("delete_on_idle").and_then(|v| v.as_bool()) {
            config.features.delete_on_idle = v;
        }
        if let Some(v) = features.get("metrics_enabled").and_then(|v| v.as_bool()) {
            config.features.metrics_enabled = v;
            let mut metrics = state.metrics.write().await;
            metrics.enabled = v;
        }
        if let Some(pins) = features.get("pins").and_then(|v| v.as_object()) {
            for (k, v) in pins {
                if let Some(val) = v.as_str() {
                    config.features.pins.insert(k.clone(), val.to_string());
                }
            }
        }
    }

    // Persist to disk.
    let toml_str = toml::to_string_pretty(&*config).unwrap_or_default();
    let path = std::path::Path::new(&state.data_dir).join("router-config.toml");
    let _ = tokio::fs::write(&path, toml_str).await;

    state.emit_event("settings.updated", "{}").await;
    StatusCode::OK
}

/// `GET /api/offerings` — registered offering types + instance counts.
pub async fn offerings(State(state): State<AppState>) -> Json<serde_json::Value> {
    let instances = state.instances.read().await;

    let mut offering_data: Vec<serde_json::Value> = Vec::new();
    for offering in state.catalog.iter() {
        let kind = offering.offering_type();
        let count = instances.values().filter(|i| i.kind == kind).count();
        let healthy = instances
            .values()
            .filter(|i| i.kind == kind && i.health.is_healthy())
            .count();

        offering_data.push(serde_json::json!({
            "kind": kind,
            "capabilities": offering.capabilities(),
            "instances": count,
            "healthy": healthy,
        }));
    }

    Json(serde_json::json!({ "offerings": offering_data }))
}

/// `GET /api/jobs` — background job ring buffer.
pub async fn jobs(State(state): State<AppState>) -> Json<serde_json::Value> {
    let jobs = state.jobs.read().await;
    let job_list: Vec<&crate::domain::types::OrchestratorJob> = jobs.iter().collect();
    Json(serde_json::json!({ "jobs": job_list }))
}

/// `POST /api/metrics/reset` — reset all metrics.
pub async fn reset_metrics(State(state): State<AppState>) -> StatusCode {
    let mut metrics = state.metrics.write().await;
    metrics.reset();
    StatusCode::OK
}

/// `POST /api/metrics/model-counters/reset` — reset per-model request counters.
pub async fn reset_model_counters(State(state): State<AppState>) -> StatusCode {
    let mut metrics = state.metrics.write().await;
    metrics.reset_model_counters();
    StatusCode::OK
}
