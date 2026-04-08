//! `GET /v1/catalog` — full machine-readable catalog.
//!
//! The catalog JSON is pre-rendered by the `catalog_builder`
//! background task and published via a `watch::channel`. The handler
//! clones the `Arc` and serializes directly.
//!
//! The previous `/v1/catalog/events` SSE stream has been retired in
//! favor of the unified `/v1/events?focus=catalog.*` bus per
//! ORCH-0030 §1.6.

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::app_state::AppState;

pub async fn get_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let docs = state.catalog.snapshot();
    let etag = format!("\"{}\"", docs.directory_version);

    if let Some(inm) = headers.get(header::IF_NONE_MATCH) {
        if let Ok(raw) = inm.to_str() {
            if raw == etag {
                return (StatusCode::NOT_MODIFIED).into_response();
            }
        }
    }

    let body = Json(docs.catalog.as_ref().clone());
    let mut response = body.into_response();
    if let Ok(v) = HeaderValue::from_str(&etag) {
        response.headers_mut().insert(header::ETAG, v);
    }
    response
}

