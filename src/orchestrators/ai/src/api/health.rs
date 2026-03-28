//! Liveness probe endpoint.

use axum::http::StatusCode;

/// `GET /health` — returns 200 if the orchestrator is running.
pub async fn health() -> StatusCode {
    StatusCode::OK
}
