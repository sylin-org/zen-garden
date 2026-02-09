//! Maintenance sweep API endpoints
//!
//! - GET  /api/v1/stone/maintenance/history — Last N sweep runs
//! - POST /api/v1/stone/maintenance/sweep   — Trigger immediate sweep

use axum::{extract::State, http::StatusCode, Json};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};

use crate::domain::maintenance::{run_sweep, SweepRun};
use crate::infra::{error_response, maintenance_store};
use crate::AppState;

/// GET /api/v1/stone/maintenance/history
///
/// Returns the last N sweep runs (newest first).
pub async fn get_sweep_history(
    State(_state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<SweepRun>>>, (StatusCode, Json<ApiErrorResponse>)> {
    match maintenance_store::load_sweep_history().await {
        Ok(history) => Ok(Json(ApiResponse::new(history))),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "maintenance_history_failed",
            format!("Failed to load sweep history: {}", e),
            None,
        )),
    }
}

/// POST /api/v1/stone/maintenance/sweep
///
/// Trigger an immediate sweep, persist the result, and return it.
pub async fn trigger_sweep(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<SweepRun>>, (StatusCode, Json<ApiErrorResponse>)> {
    let run = run_sweep(&state).await;

    if let Err(e) = maintenance_store::save_sweep_run(&run).await {
        tracing::warn!(error = ?e, "Failed to save on-demand sweep report");
    }

    Ok(Json(ApiResponse::new(run)))
}
