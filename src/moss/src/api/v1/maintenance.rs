//! Maintenance sweep API endpoints
//!
//! - GET  /api/v1/stone/maintenance/history — Last N sweep runs
//! - POST /api/v1/stone/maintenance/sweep   — Trigger immediate sweep

use axum::extract::State;

use crate::domain::maintenance::{SweepRun, run_sweep};
use crate::infra::maintenance_store;
use crate::{Moss, internal};

/// GET /api/v1/stone/maintenance/history
///
/// Returns the last N sweep runs (newest first).
pub async fn get_sweep_history(
    State(_state): State<Moss>,
) -> crate::api::ApiResult<Vec<SweepRun>> {
    match maintenance_store::load_sweep_history().await {
        Ok(history) => crate::api::ok(history),
        Err(e) => Err(internal(
            "maintenance_history_failed",
            format!("Failed to load sweep history: {}", e),
        )),
    }
}

/// POST /api/v1/stone/maintenance/sweep
///
/// Trigger an immediate sweep, persist the result, and return it.
pub async fn trigger_sweep(State(state): State<Moss>) -> crate::api::ApiResult<SweepRun> {
    let task_store = crate::infra::TaskStore::new();
    let run = run_sweep(&state, &task_store).await;

    if let Err(e) = maintenance_store::save_sweep_run(&run).await {
        tracing::warn!(error = ?e, "Failed to save on-demand sweep report");
    }

    crate::api::ok(run)
}
