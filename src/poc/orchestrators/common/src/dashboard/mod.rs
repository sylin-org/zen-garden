//! Shared embedded dashboard server for orchestrators (ORCH-0012).
//!
//! Provides the common HTTP infrastructure that every orchestrator dashboard
//! needs: HTML serving, status API, SSE event stream, and health check.
//! Adapter-specific status snapshots are injected via a callback.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use orchestrator_common::dashboard::{DashboardConfig, serve_dashboard};
//!
//! let config = DashboardConfig {
//!     port: 7193,
//!     service_name: "valkey-orchestrator",
//!     html: include_str!("../../assets/dashboard.html"),
//! };
//!
//! let status_fn: StatusFn = Arc::new({
//!     let state = state.clone();
//!     move || {
//!         let state = state.clone();
//!         Box::pin(async move { state.snapshot().await })
//!     }
//! });
//!
//! serve_dashboard(config, status_fn, event_tx, shutdown).await;
//! ```

use std::sync::Arc;

use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::{Json, Router};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::events::{dashboard_sse_stream, DashboardEvent};

/// Dashboard configuration.
pub struct DashboardConfig {
    /// Port to bind the dashboard HTTP server on.
    pub port: u16,
    /// Human-readable service name (for health endpoint).
    pub service_name: &'static str,
    /// Embedded HTML content (from `include_str!()`).
    pub html: &'static str,
}

/// Async function that returns the current status snapshot as JSON.
pub type StatusFn = Arc<
    dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = serde_json::Value> + Send>>
        + Send
        + Sync,
>;

/// Shared state for dashboard handlers.
#[derive(Clone)]
struct DashboardState {
    html: &'static str,
    service_name: &'static str,
    status_fn: StatusFn,
    event_tx: broadcast::Sender<DashboardEvent>,
}

/// Serve the orchestrator dashboard on the configured port.
///
/// Blocks until the shutdown token is cancelled. Call from a spawned task.
pub async fn serve_dashboard(
    config: DashboardConfig,
    status_fn: StatusFn,
    event_tx: broadcast::Sender<DashboardEvent>,
    shutdown: CancellationToken,
) {
    let state = DashboardState {
        html: config.html,
        service_name: config.service_name,
        status_fn,
        event_tx,
    };

    let router = Router::new()
        .route("/", get(handler_html))
        .route("/api/status", get(handler_status))
        .route("/api/events", get(handler_events))
        .route("/health", get(handler_health))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(port = config.port, error = %e, "Dashboard failed to bind");
            return;
        }
    };

    tracing::info!(
        port = config.port,
        service = config.service_name,
        "Dashboard server listening"
    );

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
        .ok();
}

// ── Handlers ──────────────────────────────────────────────────────────

async fn handler_html(State(state): State<DashboardState>) -> Html<&'static str> {
    Html(state.html)
}

async fn handler_status(State(state): State<DashboardState>) -> impl IntoResponse {
    let snapshot = (state.status_fn)().await;
    Json(snapshot)
}

async fn handler_events(State(state): State<DashboardState>) -> impl IntoResponse {
    dashboard_sse_stream(&state.event_tx)
}

async fn handler_health(State(state): State<DashboardState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": state.service_name,
    }))
}

/// Build an Axum router for the dashboard (alternative to `serve_dashboard`
/// for orchestrators that need to add custom routes).
pub fn dashboard_router(
    html: &'static str,
    service_name: &'static str,
    status_fn: StatusFn,
    event_tx: broadcast::Sender<DashboardEvent>,
) -> Router {
    let state = DashboardState {
        html,
        service_name,
        status_fn,
        event_tx,
    };

    Router::new()
        .route("/", get(handler_html))
        .route("/api/status", get(handler_status))
        .route("/api/events", get(handler_events))
        .route("/health", get(handler_health))
        .with_state(state)
}
