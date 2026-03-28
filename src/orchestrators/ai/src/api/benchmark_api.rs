//! Benchmark API — fitness profiling lifecycle management.
//!
//! The benchmark task drives the actual profiling work. These handlers
//! expose the control surface: start, cancel, results, export.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::app_state::AppState;
use crate::domain::fitness::RunStatus;

/// `POST /api/benchmark/start` — trigger a fitness benchmark run.
pub async fn start_benchmark(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let run = state.benchmark_run.read().await;
    if run.is_running() {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "benchmark already running", "id": run.id})),
        );
    }
    drop(run);

    // Create a new benchmark run.
    let id = state.create_job(crate::domain::types::JobKind::Benchmark, "fitness benchmark").await;

    let mut run = state.benchmark_run.write().await;
    run.id = id.clone();
    run.status = RunStatus::Running;
    run.started_at = Some(chrono::Utc::now().to_rfc3339());
    run.completed_at = None;
    run.error = None;

    // The actual benchmark execution would be triggered via the benchmark task.
    // For now, mark as running — the task will pick it up.
    state.emit_event("benchmark.started", &serde_json::json!({"id": &id}).to_string()).await;

    (StatusCode::ACCEPTED, Json(serde_json::json!({"id": id, "status": "running"})))
}

/// `POST /api/benchmark/cancel` — cancel a running benchmark.
pub async fn cancel_benchmark(State(state): State<AppState>) -> StatusCode {
    let cancel = state.benchmark_cancel.read().await;
    if let Some(ref token) = *cancel {
        token.cancel();
    }
    drop(cancel);

    let mut run = state.benchmark_run.write().await;
    if run.status == RunStatus::Running {
        run.status = RunStatus::Cancelled;
        run.completed_at = Some(chrono::Utc::now().to_rfc3339());
    }

    StatusCode::OK
}

/// `GET /api/benchmark/results` — current benchmark progress + results.
pub async fn benchmark_results(State(state): State<AppState>) -> Json<serde_json::Value> {
    let run = state.benchmark_run.read().await;
    Json(serde_json::json!({
        "id": run.id,
        "status": run.status,
        "started_at": run.started_at,
        "completed_at": run.completed_at,
        "stones": run.stones.len(),
        "gpu_matrix_entries": run.gpu_matrix.entries.len(),
        "error": run.error,
    }))
}

/// `GET /api/benchmark/export` — export raw fitness.json data.
pub async fn benchmark_export(State(state): State<AppState>) -> Json<serde_json::Value> {
    let run = state.benchmark_run.read().await;
    Json(serde_json::json!(run.gpu_matrix))
}

/// `GET /api/management/feasibility` — check if a model fits on available stones.
pub async fn management_feasibility(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let model_name = params.get("model").map(|s| s.as_str()).unwrap_or("");
    let instances = state.instances.read().await;

    let feasible: Vec<serde_json::Value> = instances
        .values()
        .filter(|i| i.health.is_healthy())
        .map(|i| {
            serde_json::json!({
                "stone": i.stone.name,
                "endpoint": i.endpoint,
                "kind": i.kind,
                "vram_budget_mb": i.vram.budget_bytes / 1_048_576,
            })
        })
        .collect();

    Json(serde_json::json!({
        "model": model_name,
        "viable_stones": feasible.len(),
        "stones": feasible,
    }))
}
