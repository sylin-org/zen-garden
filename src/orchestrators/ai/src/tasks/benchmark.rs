//! Benchmark task — fitness profiling across all offering types.
//!
//! Drives the benchmark lifecycle: iterate stones → iterate models →
//! run capability tests → aggregate verdicts → synthesize GPU matrix.
//!
//! Dispatches through `Offering::benchmark()` — each offering adapter
//! defines its own test payloads and measurement logic. The task
//! orchestrates the loop and state management.
//!
//! Generalized from ollama-orchestrator tasks/benchmark.rs.

use tokio_util::sync::CancellationToken;

use crate::app_state::AppState;
use crate::domain::fitness::{RunStatus, StoneReport, StoneStatus, TestStatus, TestSuite};

/// Background task: monitor for benchmark requests and execute them.
///
/// The benchmark is triggered via `POST /api/benchmark/start`, which
/// sets `benchmark_run.status = Running`. This task polls for that
/// state and executes the profiling loop.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
        }

        // Check if a benchmark run was requested.
        let should_run = {
            let run = state.benchmark_run.read().await;
            run.status == RunStatus::Running && run.stones.is_empty()
        };

        if !should_run {
            continue;
        }

        tracing::info!("benchmark: starting fitness profiling");

        // Create a cancel token for this run.
        let cancel = CancellationToken::new();
        *state.benchmark_cancel.write().await = Some(cancel.clone());

        execute_benchmark(&state, &cancel).await;

        *state.benchmark_cancel.write().await = None;
    }

    tracing::info!("benchmark task shutting down");
}

/// Execute a full benchmark run across all healthy instances.
async fn execute_benchmark(state: &AppState, cancel: &CancellationToken) {
    // Snapshot current instances.
    let instances: Vec<(String, String, crate::domain::types::OfferingKind, Option<String>)> = {
        let reg = state.instances.read().await;
        reg.values()
            .filter(|i| i.health.is_healthy())
            .map(|i| {
                (
                    i.stone.name.clone(),
                    i.endpoint.clone(),
                    i.kind,
                    i.gpu.name.clone(),
                )
            })
            .collect()
    };

    if instances.is_empty() {
        let mut run = state.benchmark_run.write().await;
        run.status = RunStatus::Completed;
        run.completed_at = Some(chrono::Utc::now().to_rfc3339());
        run.error = Some("no healthy instances to benchmark".to_string());
        return;
    }

    // Initialize stone reports.
    let mut stone_reports: Vec<StoneReport> = instances
        .iter()
        .map(|(stone_name, endpoint, _kind, gpu)| StoneReport {
            stone_name: stone_name.clone(),
            endpoint: endpoint.clone(),
            gpu_model: gpu.clone(),
            vram_mb: None,
            status: StoneStatus::Pending,
            tests: vec![],
            error: None,
        })
        .collect();

    // Profile each instance.
    for (idx, (stone_name, endpoint, kind, _gpu)) in instances.iter().enumerate() {
        if cancel.is_cancelled() {
            tracing::info!("benchmark: cancelled");
            break;
        }

        stone_reports[idx].status = StoneStatus::Testing;

        // Update live state.
        {
            let mut run = state.benchmark_run.write().await;
            run.stones = stone_reports.clone();
        }
        state
            .emit_event(
                "benchmark.progress",
                &serde_json::json!({"stone": stone_name, "status": "testing"}).to_string(),
            )
            .await;

        let offering = match state.catalog.get(*kind) {
            Some(o) => o.clone(),
            None => {
                stone_reports[idx].status = StoneStatus::Skipped;
                stone_reports[idx].error =
                    Some(format!("no adapter registered for {kind:?}"));
                continue;
            }
        };

        // Get models on this instance.
        let models: Vec<String> = {
            let reg = state.instances.read().await;
            reg.get(endpoint)
                .map(|i| i.models_available.clone())
                .unwrap_or_default()
        };

        // Get capabilities this offering supports.
        let capabilities = offering.capabilities().to_vec();

        // Run tests: model × capability.
        let mut tests = Vec::new();
        for model_name in &models {
            for &cap in &capabilities {
                if cancel.is_cancelled() {
                    break;
                }

                let mut suite = TestSuite {
                    model: model_name.clone(),
                    capability: cap,
                    status: TestStatus::Running,
                    samples: vec![],
                    summary: None,
                    error: None,
                };

                tracing::debug!(
                    stone = %stone_name,
                    model = %model_name,
                    capability = %cap,
                    "benchmark: testing"
                );

                match offering.benchmark(endpoint, model_name, cap).await {
                    Ok(result) => {
                        suite.samples = result.samples;
                        suite.status = TestStatus::Done;
                        suite.summarise();
                    }
                    Err(e) => {
                        suite.status = TestStatus::Error;
                        suite.error = Some(e.to_string());
                    }
                }

                tests.push(suite);
            }
        }

        stone_reports[idx].tests = tests;
        stone_reports[idx].status = if cancel.is_cancelled() {
            StoneStatus::Skipped
        } else {
            StoneStatus::Done
        };
    }

    // Finalize: synthesize GPU matrix.
    let final_status = if cancel.is_cancelled() {
        RunStatus::Cancelled
    } else {
        RunStatus::Completed
    };

    {
        let mut run = state.benchmark_run.write().await;
        run.stones = stone_reports;
        run.status = final_status;
        run.completed_at = Some(chrono::Utc::now().to_rfc3339());
        run.synthesise_matrix();

        // Persist fitness data.
        let fitness_path = std::path::Path::new(&state.data_dir).join("fitness.json");
        if let Ok(json) = serde_json::to_string_pretty(&run.gpu_matrix) {
            let _ = tokio::fs::write(&fitness_path, json).await;
        }
    }

    // Refresh recommendations with new fitness data.
    state.refresh_recommendations().await;

    state
        .emit_event("benchmark.completed", "{}")
        .await;

    tracing::info!("benchmark: completed");
}
