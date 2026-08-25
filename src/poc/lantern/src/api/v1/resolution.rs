//! Service resolution endpoint

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use garden_common::api_utils::ApiErrorResponse;
use serde_json::Value;
use std::collections::HashMap;

use crate::api::responses::error_response;
use crate::domain::resolution::resolve_service;
use crate::AppState;

/// GET /api/v1/resolve?service=<type>
pub async fn get_resolve(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<ApiErrorResponse>)> {
    let service_type = params.get("service").ok_or_else(|| {
        error_response(
            StatusCode::BAD_REQUEST,
            "MISSING_PARAMETER",
            "Missing 'service' query parameter",
        )
    })?;

    let topology = state.topology.read().await;

    match resolve_service(&topology, service_type) {
        Some(response) => Ok(Json(serde_json::to_value(response).unwrap())),
        None => Err(error_response(
            StatusCode::NOT_FOUND,
            "SERVICE_NOT_AVAILABLE",
            format!("No stone provides service type '{}'", service_type),
        )),
    }
}
