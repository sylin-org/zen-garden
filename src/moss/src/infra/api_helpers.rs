//! API helper utilities
//!
//! Common utilities for HTTP API handlers including error response creation.

use axum::{http::StatusCode, Json};
use garden_common::api_utils::ApiErrorResponse;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use crate::AppState;

/// Create an error response for API handlers
///
/// This is a convenience helper that wraps ApiErrorResponse creation
/// with the common (StatusCode, Json<ApiErrorResponse>) return type.
pub fn error_response(
    status_code: StatusCode,
    error_code: impl Into<String>,
    message: impl Into<String>,
    details: Option<HashMap<String, serde_json::Value>>,
) -> (StatusCode, Json<ApiErrorResponse>) {
    let response = if let Some(details) = details {
        ApiErrorResponse::with_details(error_code, message, details)
    } else {
        ApiErrorResponse::new(error_code, message)
    };
    (status_code, Json(response))
}

/// Check if Docker daemon is available
///
/// Returns Ok(()) if Docker is ready, or a 503 Service Unavailable error if not.
/// Use this at the start of API handlers that require Docker operations.
///
/// # Example
/// ```rust,ignore
/// pub async fn create_service(state: State<AppState>) -> Result<..., (StatusCode, Json<ApiErrorResponse>)> {
///     require_docker(&state)?;
///     // ... Docker operations ...
/// }
/// ```
pub fn require_docker(state: &AppState) -> Result<(), (StatusCode, Json<ApiErrorResponse>)> {
    if !state.subsystems.docker.ready.load(Ordering::Relaxed) {
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            garden_common::constants::DOCKER_UNAVAILABLE,
            "Docker daemon is currently unavailable. The service will automatically reconnect when Docker becomes available.",
            None,
        ));
    }
    Ok(())
}

