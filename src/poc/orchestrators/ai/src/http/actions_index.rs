//! `GET /v1/do` — the action index with examples and setup hints.

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::app_state::AppState;

pub async fn get_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let docs = state.catalog.snapshot();
    let etag = format!("\"actions-{}\"", docs.directory_version);

    if let Some(inm) = headers.get(header::IF_NONE_MATCH) {
        if let Ok(raw) = inm.to_str() {
            if raw == etag {
                return StatusCode::NOT_MODIFIED.into_response();
            }
        }
    }

    let body = Json(docs.actions_index.as_ref().clone());
    let mut response = body.into_response();
    if let Ok(v) = HeaderValue::from_str(&etag) {
        response.headers_mut().insert(header::ETAG, v);
    }
    response
}
