//! Offering Volume API — read/write files in offering volumes (ORCH-0019).
//!
//! Endpoints:
//! - `PUT  /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path` — write file
//! - `GET  /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path` — read file
//! - `HEAD /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path` — check existence
//!
//! Used by the AI orchestrator to provision model files to ComfyUI instances.
//! Path traversal is validated — `..` segments are rejected.

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::AppState;

/// Path parameters: (fqn, volume, file_path)
type VolumePath = (String, String, String);

/// `PUT /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path`
///
/// Write a file to the offering's volume. Creates intermediate directories.
/// Streams the request body to a temp file, then atomically renames.
/// Never buffers the full body in memory.
pub async fn put_volume_file(
    State(_state): State<AppState>,
    Path((fqn, volume, file_path)): Path<VolumePath>,
    request: Request,
) -> Response {
    if has_path_traversal(&file_path) {
        return bad_request_response("PATH_TRAVERSAL", "Path contains '..' segments");
    }

    let host_path = match resolve_volume_path(&fqn, &volume, &file_path) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };

    let dest = std::path::Path::new(&host_path);

    // Create parent directories
    if let Some(parent) = dest.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return internal_response(&format!("Failed to create directory: {e}"));
        }
    }

    let existed = tokio::fs::try_exists(&dest).await.unwrap_or(false);

    // Stream body to a temp file — never buffer in memory
    let tmp_path = format!("{host_path}.tmp");
    let mut file = match tokio::fs::File::create(&tmp_path).await {
        Ok(f) => f,
        Err(e) => return internal_response(&format!("Failed to create temp file: {e}")),
    };

    let mut stream = request.into_body().into_data_stream();
    let mut written: u64 = 0;

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                if let Err(e) = file.write_all(&bytes).await {
                    let _ = tokio::fs::remove_file(&tmp_path).await;
                    return internal_response(&format!("Failed to write chunk: {e}"));
                }
                written += bytes.len() as u64;
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return internal_response(&format!("Failed to read request body: {e}"));
            }
        }
    }

    if let Err(e) = file.flush().await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return internal_response(&format!("Failed to flush file: {e}"));
    }
    drop(file);

    // Atomic rename
    if let Err(e) = tokio::fs::rename(&tmp_path, &dest).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return internal_response(&format!("Failed to rename temp file: {e}"));
    }

    tracing::info!(
        fqn = %fqn,
        volume = %volume,
        path = %file_path,
        bytes = written,
        "wrote file to offering volume"
    );

    if existed {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::CREATED.into_response()
    }
}

/// `GET /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path`
///
/// Read a file from the offering's volume. Streams from disk — never
/// buffers the full file in memory.
pub async fn get_volume_file(
    State(_state): State<AppState>,
    Path((fqn, volume, file_path)): Path<VolumePath>,
) -> Response {
    if has_path_traversal(&file_path) {
        return bad_request_response("PATH_TRAVERSAL", "Path contains '..' segments");
    }

    let host_path = match resolve_volume_path(&fqn, &volume, &file_path) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };

    let meta = match tokio::fs::metadata(&host_path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return not_found_response(&format!("File not found: {file_path}"));
        }
        Err(e) => {
            return internal_response(&format!("Failed to stat file: {e}"));
        }
    };

    let file = match tokio::fs::File::open(&host_path).await {
        Ok(f) => f,
        Err(e) => {
            return internal_response(&format!("Failed to open file: {e}"));
        }
    };

    let content_type = mime_from_extension(&file_path);
    let stream = tokio_util::io::ReaderStream::new(file);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, meta.len())
        .body(Body::from_stream(stream))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// `HEAD /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path`
///
/// Check if a file exists in the offering's volume. Returns 200 + Content-Length
/// or 404.
pub async fn head_volume_file(
    State(_state): State<AppState>,
    Path((fqn, volume, file_path)): Path<VolumePath>,
) -> Response {
    if has_path_traversal(&file_path) {
        return bad_request_response("PATH_TRAVERSAL", "Path contains '..' segments");
    }

    let host_path = match resolve_volume_path(&fqn, &volume, &file_path) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };

    match tokio::fs::metadata(&host_path).await {
        Ok(meta) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_LENGTH, meta.len())
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap_or_else(|_| StatusCode::NOT_FOUND.into_response()),
    }
}

// ── Helpers ────────────────────────────────────────────────────

/// Resolve the host filesystem path for a volume file.
///
/// Layout: `{volumes_dir}/{fqn_encoded}/{volume}/{path}`
fn resolve_volume_path(fqn: &str, volume: &str, file_path: &str) -> Result<String, Response> {
    let offering_fqn = garden_common::offerings::OfferingFqn::parse(fqn).map_err(|e| {
        bad_request_response("INVALID_FQN", &format!("Invalid offering FQN '{}': {}", fqn, e))
    })?;

    let encoded = offering_fqn.encoded_for_container();
    let base = garden_common::constants::paths::volumes_dir();

    Ok(format!("{base}/{encoded}/{volume}/{file_path}"))
}

/// Check for path traversal attempts.
fn has_path_traversal(path: &str) -> bool {
    path.contains("..") || path.contains('\0')
}

/// Infer MIME type from file extension.
fn mime_from_extension(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "pth" | "pt" | "bin" | "safetensors" => "application/octet-stream",
        "json" => "application/json",
        "yaml" | "yml" => "text/yaml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

fn bad_request_response(code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": { "code": code, "message": message }
    });
    (StatusCode::BAD_REQUEST, axum::Json(body)).into_response()
}

fn not_found_response(message: &str) -> Response {
    let body = serde_json::json!({
        "error": { "code": "NOT_FOUND", "message": message }
    });
    (StatusCode::NOT_FOUND, axum::Json(body)).into_response()
}

fn internal_response(message: &str) -> Response {
    let body = serde_json::json!({
        "error": { "code": "INTERNAL_ERROR", "message": message }
    });
    (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(body)).into_response()
}
