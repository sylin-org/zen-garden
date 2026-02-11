//! Lantern API response helpers
//!
//! Reuses ApiErrorResponse from garden_common. Adds Lantern-specific helpers.

use axum::http::StatusCode;
use axum::Json;
use garden_common::api_utils::ApiErrorResponse;

/// Convenience constructor for error responses
pub fn error_response(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> (StatusCode, Json<ApiErrorResponse>) {
    (
        status,
        Json(ApiErrorResponse::new(code, message)),
    )
}
