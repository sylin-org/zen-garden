//! Media HTTP handlers.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::app_state::AppState;
use crate::domain::ids::MediaId;
use crate::domain::media::{MediaEntryView, MediaFilter, MediaSource};

pub async fn post_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    if body.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {"code": "validation_failed", "message": "empty upload body"},
            })),
        )
            .into_response();
    }

    match state
        .media_store
        .put(body, content_type, MediaSource::uploaded())
        .await
    {
        Ok(entry) => {
            let view = MediaEntryView::from(&entry);
            (StatusCode::CREATED, Json(view)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": {"code": "internal_error", "message": e.to_string()},
            })),
        )
            .into_response(),
    }
}

pub async fn get_download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let media_id = MediaId::from_string(id);
    let meta = match state.media_store.get_metadata(&media_id).await {
        Ok(m) => m,
        Err(_) => return not_found(),
    };
    let bytes = match state.media_store.get_bytes(&media_id).await {
        Ok(b) => b,
        Err(_) => return not_found(),
    };
    let _ = state.media_store.touch(&media_id).await;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, &meta.content_type)
        .header(header::CONTENT_LENGTH, meta.size_bytes)
        .header(header::ETAG, format!("\"{}\"", meta.content_hash))
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(bytes))
        .expect("static headers")
}

pub async fn head_media(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let media_id = MediaId::from_string(id);
    match state.media_store.get_metadata(&media_id).await {
        Ok(meta) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, &meta.content_type)
            .header(header::CONTENT_LENGTH, meta.size_bytes)
            .header(header::ETAG, format!("\"{}\"", meta.content_hash))
            .body(Body::empty())
            .expect("static headers"),
        Err(_) => not_found(),
    }
}

pub async fn get_metadata(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let media_id = MediaId::from_string(id);
    match state.media_store.get_metadata(&media_id).await {
        Ok(meta) => {
            let view = MediaEntryView::from(&meta);
            Json(view).into_response()
        }
        Err(_) => not_found(),
    }
}

pub async fn delete_media(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let media_id = MediaId::from_string(id);
    match state.media_store.delete(&media_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => not_found(),
    }
}

pub async fn list_media(State(state): State<AppState>) -> Response {
    match state.media_store.list(MediaFilter::default()).await {
        Ok(entries) => {
            let views: Vec<MediaEntryView> = entries.iter().map(MediaEntryView::from).collect();
            Json(json!({ "media": views })).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": {"code": "internal_error", "message": e.to_string()},
            })),
        )
            .into_response(),
    }
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": {"code": "not_found", "message": "media not found"},
        })),
    )
        .into_response()
}

// Silence an unused-import lint in handlers that never touch the raw
// Value type directly.
#[allow(dead_code)]
fn _unused(_: Value) {}
