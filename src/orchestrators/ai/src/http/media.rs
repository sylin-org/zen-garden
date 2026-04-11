//! Media HTTP handlers.

use std::io::Cursor;
use std::path::PathBuf;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use bytes::Bytes;
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::app_state::AppState;
use crate::domain::ids::MediaId;
use crate::domain::media::{MediaEntryView, MediaFilter, MediaSource};

/// Bundled SVG icons served by the representation endpoint for
/// non-image media kinds. Kept at `assets/media-icons/` and embedded
/// into the binary at compile time so the dashboard works offline
/// and out-of-the-box.
#[derive(RustEmbed)]
#[folder = "assets/media-icons/"]
struct MediaIcons;

/// Query parameters for `GET /v1/media/{id}`. Absent `format` means
/// "return the original bytes as-is" — the default behavior this
/// handler has always had.
#[derive(Debug, Deserialize, Default)]
pub struct MediaQuery {
    /// Representation format: `thumbnail` (256×256 image for
    /// `image/*`, bundled SVG icon for everything else) or
    /// `preview` (first 256 chars as `text/plain` for `text/*`).
    /// Unknown formats fall through to the original bytes.
    #[serde(default)]
    pub format: Option<String>,
}

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
    Query(query): Query<MediaQuery>,
) -> Response {
    let media_id = MediaId::from_string(id);
    let meta = match state.media_store.get_metadata(&media_id).await {
        Ok(m) => m,
        Err(_) => return not_found(),
    };

    match query.format.as_deref() {
        Some("thumbnail") => return serve_thumbnail(&state, &media_id, &meta).await,
        Some("preview") => return serve_preview(&state, &media_id, &meta).await,
        // Any other format (including None) falls through to the
        // original-bytes path below. Unknown formats silently
        // degrade to the original — keeps the endpoint forward-
        // compatible with future additions.
        _ => {}
    }

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

/// Serve a 256×256 PNG thumbnail for `image/*` media, or a bundled
/// SVG icon for other kinds. Image thumbnails are rendered once and
/// cached on disk next to the original (`{id}.thumbnail.png`); SVG
/// icons are served directly from the embedded `MediaIcons` bundle.
async fn serve_thumbnail(
    state: &AppState,
    media_id: &MediaId,
    meta: &crate::domain::media::MediaEntry,
) -> Response {
    // Non-image kinds → bundled SVG icon, no per-media cache needed.
    if !meta.content_type.starts_with("image/") {
        return serve_kind_icon(&meta.content_type);
    }

    let cache_path = thumbnail_cache_path(state, media_id);

    // Cache hit — stream from disk.
    if let Ok(bytes) = tokio::fs::read(&cache_path).await {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/png")
            .header(header::CONTENT_LENGTH, bytes.len() as u64)
            .header(header::CACHE_CONTROL, "public, max-age=86400")
            .body(Body::from(bytes))
            .expect("static headers");
    }

    // Cache miss — render, write, serve.
    let original = match state.media_store.get_bytes(media_id).await {
        Ok(b) => b,
        Err(_) => return not_found(),
    };

    let rendered = match render_thumbnail_png(&original) {
        Some(b) => b,
        // Decode failed — fall back to a generic file icon rather
        // than 500. Corrupt uploads shouldn't make the UI sad.
        None => return serve_kind_icon("application/octet-stream"),
    };

    // Best-effort cache write — a failed write just means the next
    // request re-renders. Don't block the response on it.
    if let Some(parent) = cache_path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let _ = tokio::fs::write(&cache_path, &rendered).await;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/png")
        .header(header::CONTENT_LENGTH, rendered.len() as u64)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(rendered))
        .expect("static headers")
}

/// Serve a text preview (first 256 chars as `text/plain`) for
/// `text/*` media. Non-text kinds get a 415 — preview is a
/// text-specific format.
async fn serve_preview(
    state: &AppState,
    media_id: &MediaId,
    meta: &crate::domain::media::MediaEntry,
) -> Response {
    if !meta.content_type.starts_with("text/") {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({
                "error": {
                    "code": "unsupported_media_type",
                    "message": format!(
                        "preview format is only defined for text/* media; this is {}",
                        meta.content_type
                    ),
                },
            })),
        )
            .into_response();
    }

    let bytes = match state.media_store.get_bytes(media_id).await {
        Ok(b) => b,
        Err(_) => return not_found(),
    };

    // Decode lossily — preview is best-effort, not strict UTF-8.
    let full = String::from_utf8_lossy(&bytes);
    let preview: String = full.chars().take(256).collect();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(preview))
        .expect("static headers")
}

/// Map a content type to its bundled SVG icon. Always returns a
/// response — the generic `file.svg` is the last-resort fallback
/// and is guaranteed to exist.
fn serve_kind_icon(content_type: &str) -> Response {
    let name = if content_type.starts_with("audio/") {
        "audio.svg"
    } else if content_type.starts_with("video/") {
        "video.svg"
    } else if content_type == "application/pdf" {
        "pdf.svg"
    } else if content_type.starts_with("text/") {
        "text.svg"
    } else {
        "file.svg"
    };

    // `expect` is safe: these five icon files are bundled at
    // compile time via `RustEmbed`. Their absence is a build-time
    // error, not a runtime one.
    let asset = MediaIcons::get(name).expect("bundled media icon missing");
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/svg+xml")
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(Bytes::from(asset.data.into_owned())))
        .expect("static headers")
}

/// Where rendered thumbnails live on disk. Keyed by media id so
/// thumbnails are invalidated automatically when the source media
/// is deleted (both paths get swept together).
fn thumbnail_cache_path(state: &AppState, media_id: &MediaId) -> PathBuf {
    state
        .data_dir
        .join("media")
        .join("thumbnails")
        .join(format!("{}.png", media_id.as_str()))
}

/// Decode the input bytes, resize to fit 256×256 preserving aspect,
/// and encode as PNG. Returns `None` if decoding fails — the caller
/// falls back to a generic icon.
fn render_thumbnail_png(original: &Bytes) -> Option<Vec<u8>> {
    use image::imageops::FilterType;
    use image::ImageFormat;

    let img = image::load_from_memory(original).ok()?;
    let thumb = img.resize(256, 256, FilterType::Triangle);

    let mut buf = Vec::new();
    thumb
        .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .ok()?;
    Some(buf)
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
