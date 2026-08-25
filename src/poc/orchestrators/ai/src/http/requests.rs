//! HTTP endpoints for the persistent request log (ORCH-0033).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::DateTime;
use serde::Deserialize;
use serde_json::json;

use crate::app_state::AppState;
use crate::domain::persisted_request::RequestFilter;

/// `GET /v1/requests` — list requests with optional filters.
pub async fn list_requests(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> impl IntoResponse {
    let filter = RequestFilter {
        action: params.action,
        status: params.status.and_then(|s| parse_status(&s)),
        pinned: params.pinned,
        parent_id: params.parent_id,
        before: params.before.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.to_utc())),
        after: params.after.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|d| d.to_utc())),
        limit: params.limit.or(Some(50)),
    };

    let requests = state.request_store.list(&filter).await;
    Json(json!({
        "count": requests.len(),
        "requests": requests,
    }))
}

/// `GET /v1/requests/{id}` — get a single request.
pub async fn get_request(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.request_store.get(&id).await {
        Ok(req) => Json(json!(req)).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "not_found",
                "message": format!("Request '{id}' not found"),
            })),
        )
            .into_response(),
    }
}

/// `PATCH /v1/requests/{id}/pin` — toggle pinned status.
pub async fn toggle_pin(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.request_store.toggle_pin(&id).await {
        Ok(pinned) => Json(json!({ "id": id, "pinned": pinned })).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "not_found",
                "message": format!("Request '{id}' not found"),
            })),
        )
            .into_response(),
    }
}

/// `DELETE /v1/requests` — flush unpinned requests older than threshold.
pub async fn flush_requests(
    State(state): State<AppState>,
    Query(params): Query<FlushParams>,
) -> impl IntoResponse {
    let before = match &params.before {
        Some(s) => match DateTime::parse_from_rfc3339(s) {
            Ok(dt) => dt.to_utc(),
            Err(_) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "validation_failed",
                        "message": "Invalid 'before' timestamp — expected RFC 3339",
                    })),
                )
                    .into_response();
            }
        },
        None => {
            // Default: flush requests older than 7 days.
            chrono::Utc::now() - chrono::Duration::days(7)
        }
    };

    match state.request_store.flush(before).await {
        Ok(count) => Json(json!({ "flushed": count, "before": before.to_rfc3339() })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "internal_error",
                "message": format!("Flush failed: {e}"),
            })),
        )
            .into_response(),
    }
}

/// `GET /v1/requests/{id}/lineage` — walk ancestor chain.
pub async fn get_lineage(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.request_store.lineage(&id).await {
        Ok(chain) => Json(json!({
            "request_id": id,
            "ancestors": chain,
        }))
        .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "not_found",
                "message": format!("Request '{id}' not found"),
            })),
        )
            .into_response(),
    }
}

// ── Query parameter types ────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub action: Option<String>,
    pub status: Option<String>,
    pub pinned: Option<bool>,
    pub parent_id: Option<String>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct FlushParams {
    pub before: Option<String>,
}

fn parse_status(s: &str) -> Option<crate::domain::persisted_request::RequestStatus> {
    match s {
        "running" => Some(crate::domain::persisted_request::RequestStatus::Running),
        "success" => Some(crate::domain::persisted_request::RequestStatus::Success),
        "failure" => Some(crate::domain::persisted_request::RequestStatus::Failure),
        _ => None,
    }
}
