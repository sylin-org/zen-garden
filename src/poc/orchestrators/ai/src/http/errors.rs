//! HTTP adapter helpers for [`OrchestratorError`] → axum response.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::Value;

use crate::domain::errors::{ErrorCode, OrchestratorError};

use super::envelopes::{quick_error, ErrorBody, ErrorEnvelope, Meta};

/// Convert an [`OrchestratorError`] into an HTTP response using the
/// canonical envelope shape with a `_meta` block.
pub fn error_response(err: OrchestratorError, meta: Meta) -> Response {
    let status = StatusCode::from_u16(err.code.http_status()).unwrap_or(StatusCode::BAD_REQUEST);
    let envelope = ErrorEnvelope::from_error(err, meta);
    (status, Json(envelope)).into_response()
}

/// Lightweight variant for cases where we have no request context yet
/// (e.g., URL parse failures).
pub fn quick_error_response(code: ErrorCode, message: impl Into<String>) -> Response {
    let status = StatusCode::from_u16(code.http_status()).unwrap_or(StatusCode::BAD_REQUEST);
    let body: Value = quick_error(code, message, None);
    (status, Json(body)).into_response()
}

/// Assemble an error response body from its parts.
pub fn error_body(err: &OrchestratorError) -> ErrorBody {
    ErrorBody {
        code: err.code.as_str().to_string(),
        message: err.message.clone(),
        details: err.details.clone(),
    }
}
