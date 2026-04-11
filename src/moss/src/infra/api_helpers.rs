//! API helper utilities
//!
//! Common utilities for HTTP API handlers including error response creation.

use crate::AppState;
use axum::{Json, http::StatusCode};
use garden_common::api_utils::ApiErrorResponse;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

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

// ============================================================================
// Typed error constructors — eliminate 213 inline error_response() calls
// ============================================================================

type ErrTuple = (StatusCode, Json<ApiErrorResponse>);

/// 404 Not Found — entity does not exist.
pub fn not_found(code: impl Into<String>, message: impl Into<String>) -> ErrTuple {
    error_response(StatusCode::NOT_FOUND, code, message, None)
}

/// 400 Bad Request — client sent invalid input.
pub fn bad_request(code: impl Into<String>, message: impl Into<String>) -> ErrTuple {
    error_response(StatusCode::BAD_REQUEST, code, message, None)
}

/// 500 Internal Server Error — unexpected server-side failure.
pub fn internal(code: impl Into<String>, message: impl Into<String>) -> ErrTuple {
    error_response(StatusCode::INTERNAL_SERVER_ERROR, code, message, None)
}

/// 503 Service Unavailable — a required backend is down.
pub fn unavailable(code: impl Into<String>, message: impl Into<String>) -> ErrTuple {
    error_response(StatusCode::SERVICE_UNAVAILABLE, code, message, None)
}

/// 409 Conflict — resource state conflict.
pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> ErrTuple {
    error_response(StatusCode::CONFLICT, code, message, None)
}

/// 502 Bad Gateway — upstream stone/service returned an error.
pub fn bad_gateway(code: impl Into<String>, message: impl Into<String>) -> ErrTuple {
    error_response(StatusCode::BAD_GATEWAY, code, message, None)
}

/// 403 Forbidden — operation not permitted.
pub fn forbidden(code: impl Into<String>, message: impl Into<String>) -> ErrTuple {
    error_response(StatusCode::FORBIDDEN, code, message, None)
}

/// 501 Not Implemented — feature not yet available.
pub fn not_implemented(code: impl Into<String>, message: impl Into<String>) -> ErrTuple {
    error_response(StatusCode::NOT_IMPLEMENTED, code, message, None)
}

// ============================================================================
// Precondition checks
// ============================================================================

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
