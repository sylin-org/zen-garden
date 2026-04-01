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
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::AppState;

/// Path parameters: (fqn, volume, file_path)
type VolumePath = (String, String, String);

/// `PUT /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path`
///
/// Write a file to the offering's volume. Creates intermediate directories.
/// Accepts raw binary body.
pub async fn put_volume_file(
    State(_state): State<AppState>,
    Path((fqn, volume, file_path)): Path<VolumePath>,
    body: axum::body::Bytes,
) -> Response {
    if has_path_traversal(&file_path) {
        return bad_request_response("PATH_TRAVERSAL", "Path contains '..' segments");
    }

    let host_path = match resolve_volume_path(&fqn, &volume, &file_path) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };

    // Create parent directories
    if let Some(parent) = std::path::Path::new(&host_path).parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return internal_response(&format!("Failed to create directory: {e}"));
        }
    }

    let existed = tokio::fs::try_exists(&host_path).await.unwrap_or(false);

    if let Err(e) = tokio::fs::write(&host_path, &body).await {
        return internal_response(&format!("Failed to write file: {e}"));
    }

    tracing::info!(
        fqn = %fqn,
        volume = %volume,
        path = %file_path,
        bytes = body.len(),
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
/// Read a file from the offering's volume. Returns binary with Content-Type.
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

    match tokio::fs::read(&host_path).await {
        Ok(bytes) => {
            let content_type = mime_from_extension(&file_path);
            let len = bytes.len();

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CONTENT_LENGTH, len)
                .body(Body::from(bytes))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            not_found_response(&format!("File not found: {file_path}"))
        }
        Err(e) => {
            internal_response(&format!("Failed to read file: {e}"))
        }
    }
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
