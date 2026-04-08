//! `GET /v1/catalog` — full machine-readable catalog.
//! `GET /v1/catalog/events` — SSE stream of catalog version changes.
//!
//! The catalog JSON is pre-rendered by the `catalog_builder`
//! background task and published via a `watch::channel`. Handlers
//! clone the `Arc` and serialize directly.

use std::convert::Infallible;

use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::stream::BoxStream;
use serde_json::json;

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

/// SSE stream of catalog snapshots. Every time the catalog builder
/// publishes a new pre-rendered document, a `catalog` event fires
/// with the full JSON. Dashboards subscribe once and stay live.
pub async fn get_catalog_events(State(state): State<AppState>) -> Response {
    let mut rx = state.catalog.subscribe();

    let stream: BoxStream<'static, Result<Event, Infallible>> = Box::pin(async_stream::stream! {
        // Emit the current value immediately so new subscribers see
        // the live snapshot without waiting for the next change.
        {
            let initial = rx.borrow_and_update().clone();
            let payload = json!({
                "version": initial.directory_version,
                "catalog": initial.catalog.as_ref(),
            });
            yield Ok(
                Event::default()
                    .event("catalog")
                    .id(initial.directory_version.to_string())
                    .json_data(&payload)
                    .unwrap_or_else(|_| Event::default().data("{}"))
            );
        }

        while rx.changed().await.is_ok() {
            let docs = rx.borrow_and_update().clone();
            let payload = json!({
                "version": docs.directory_version,
                "catalog": docs.catalog.as_ref(),
            });
            yield Ok(
                Event::default()
                    .event("catalog")
                    .id(docs.directory_version.to_string())
                    .json_data(&payload)
                    .unwrap_or_else(|_| Event::default().data("{}"))
            );
        }
    });

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
