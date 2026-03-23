pub mod responses;
pub mod suggestions;
pub mod v1;

use axum::{http::StatusCode, Json};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};

/// Standard return type for API handlers that return data + optional suggestions.
///
/// Replaces the verbose `Result<Json<ApiResponse<T>>, (StatusCode, Json<ApiErrorResponse>)>`
/// that appears in 78+ handler signatures.
pub type ApiResult<T> = Result<Json<ApiResponse<T>>, (StatusCode, Json<ApiErrorResponse>)>;

/// Wrap data in a successful API response (no suggestions).
pub fn ok<T: serde::Serialize>(data: T) -> ApiResult<T> {
    Ok(Json(ApiResponse {
        data,
        suggestions: None,
    }))
}

/// Wrap data in a successful API response with suggestions.
pub fn ok_with<T: serde::Serialize>(data: T, suggestions: Vec<String>) -> ApiResult<T> {
    Ok(Json(ApiResponse {
        data,
        suggestions: Some(suggestions),
    }))
}

/// Wrap data in a successful API response with optional suggestions.
pub fn ok_maybe<T: serde::Serialize>(data: T, suggestions: Option<Vec<String>>) -> ApiResult<T> {
    Ok(Json(ApiResponse { data, suggestions }))
}
