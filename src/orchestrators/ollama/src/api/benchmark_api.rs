//! Benchmark API endpoints.
//!
//! HTTP handlers for starting, cancelling, and querying fitness benchmarks.
//! Mounted on the dashboard server, not the proxy.

use crate::app_state::AppState;
use crate::domain::fitness::{BenchmarkScope, WipeScope};
use crate::infra::ollama_client::OllamaClient;
use crate::tasks::benchmark;
use axum::{extract::State, response::IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;

/// Shared state for benchmark handlers (needs both AppState and OllamaClient).
#[derive(Clone)]
pub struct BenchmarkState {
    pub app: AppState,
    pub client: OllamaClient,
}

// ── Request Types ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct StartRequest {
    /// "full" or "stone:<name>"
    #[serde(default = "default_scope")]
    pub scope: String,
    /// Pull missing models before benchmarking.
    #[serde(default)]
    pub sync: bool,
    /// Wipe existing results: null, "all", or "stone:<name>"
    #[serde(default)]
    pub wipe: Option<String>,
}

fn default_scope() -> String {
    "full".to_string()
}

// ── Handlers ─────────────────────────────────────────────────────

/// `POST /api/benchmark/start` — start a benchmark run.
pub async fn start_benchmark(
    State(state): State<BenchmarkState>,
    Json(req): Json<StartRequest>,
) -> impl IntoResponse {
    // Check if already running
    {
        let run = state.app.benchmark_run.read().await;
        if run.is_running() {
            return Json(json!({
                "ok": false,
                "error": "benchmark already running"
            }));
        }
    }

    // Parse scope
    let scope = if req.scope == "full" {
        BenchmarkScope::Full
    } else if let Some(name) = req.scope.strip_prefix("stone:") {
        BenchmarkScope::Stone(name.to_string())
    } else {
        return Json(json!({
            "ok": false,
            "error": format!("invalid scope: {}", req.scope)
        }));
    };

    // Parse wipe
    let wipe = match req.wipe.as_deref() {
        None => None,
        Some("all") => Some(WipeScope::All),
        Some(s) if s.starts_with("stone:") => {
            Some(WipeScope::Stone(s.strip_prefix("stone:").unwrap().to_string()))
        }
        Some(s) => {
            return Json(json!({
                "ok": false,
                "error": format!("invalid wipe scope: {s}")
            }));
        }
    };

    benchmark::start(state.app, state.client, scope, req.sync, wipe).await;

    Json(json!({
        "ok": true,
        "message": "benchmark started"
    }))
}

/// `POST /api/benchmark/cancel` — cancel a running benchmark.
pub async fn cancel_benchmark(State(state): State<BenchmarkState>) -> impl IntoResponse {
    benchmark::cancel(&state.app).await;
    Json(json!({
        "ok": true,
        "message": "benchmark cancelled"
    }))
}

/// `GET /api/benchmark/results` — full benchmark run (tree structure).
pub async fn get_results(State(state): State<BenchmarkState>) -> impl IntoResponse {
    let run = state.app.benchmark_run.read().await;
    Json(serde_json::to_value(&*run).unwrap_or_default())
}

/// `GET /api/benchmark/export` — download raw benchmark run as JSON.
pub async fn export_fitness(State(state): State<BenchmarkState>) -> impl IntoResponse {
    let run = state.app.benchmark_run.read().await;
    Json(serde_json::to_value(&*run).unwrap_or_default())
}
