//! Offering Volume API — read/write files in offering volumes (ORCH-0019).
//!
//! Endpoints:
//! - `PUT  /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path` — write file
//! - `GET  /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path` — read file
//! - `HEAD /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path` — check existence
//! - `GET  /api/v1/stone/offerings/{fqn}/volumes/{volume}` — list every file
//!   in the volume (recursive walk, returns relative paths + sizes).
//!
//! Used by the AI orchestrator to provision model files to ComfyUI instances
//! and to inventory installed resources for capability publication.
//! Path traversal is validated — `..` segments are rejected.

use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::Moss;

/// Path parameters: (fqn, volume, file_path)
type VolumePath = (String, String, String);

/// Path parameters for the listing endpoint: (fqn, volume) — no
/// trailing `*path` segment.
type VolumeRoot = (String, String);

/// `PUT /api/v1/stone/offerings/{fqn}/volumes/{volume}/*path`
///
/// Write a file to the offering's volume. Creates intermediate directories.
/// Streams the request body to a temp file, then atomically renames.
/// Never buffers the full body in memory.
pub async fn put_volume_file(
    State(_state): State<Moss>,
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
    if let Some(parent) = dest.parent()
        && let Err(e) = tokio::fs::create_dir_all(parent).await
    {
        return internal_response(&format!("Failed to create directory: {e}"));
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
    State(_state): State<Moss>,
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
    State(_state): State<Moss>,
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

/// `GET /api/v1/stone/offerings/{fqn}/volumes/{volume}`
///
/// List every file in the offering's volume. Walks the volume root
/// recursively and returns one entry per file with its volume-relative
/// path and byte size. Used by the AI orchestrator to inventory
/// installed resources (e.g. ComfyUI checkpoint files) so it can
/// gate skill publication and dispatch on actual resource presence.
///
/// Same path resolution and "missing volume = 404" semantics as
/// [`head_volume_file`], just walking the directory instead of
/// stat-ing one file. Empty volumes return 200 with `count: 0`.
///
/// Response shape:
/// ```json
/// {
///   "fqn": "comfyui",
///   "volume": "comfyui-models",
///   "files": [
///     {"path": "checkpoints/sdxl.safetensors", "size": 6938040744}
///   ],
///   "count": 1,
///   "total_bytes": 6938040744
/// }
/// ```
pub async fn list_volume_files(
    State(_state): State<Moss>,
    Path((fqn, volume)): Path<VolumeRoot>,
) -> Response {
    // Resolve to the volume root by passing an empty path. The
    // helper builds `{volumes_dir}/{encoded_fqn}/{volume}/`, which
    // is exactly what we walk.
    let host_root = match resolve_volume_path(&fqn, &volume, "") {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    // Strip the trailing `/` so std::path::Path treats it as a
    // directory consistently.
    let root = host_root.trim_end_matches('/').to_string();

    // Confirm the volume root exists. If the offering has never
    // been provisioned (no PUT yet), the directory may be absent —
    // return 404 to mirror the file-level behavior.
    match tokio::fs::metadata(&root).await {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => {
            return not_found_response(&format!("Volume root is not a directory: {fqn}/{volume}"));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return not_found_response(&format!("Volume not provisioned: {fqn}/{volume}"));
        }
        Err(e) => {
            return internal_response(&format!("Failed to stat volume root: {e}"));
        }
    }

    // Walk recursively. Errors on individual entries are logged
    // but do not abort the listing — partial results are more
    // useful than no results when the orchestrator is trying to
    // decide what to provision.
    let mut files: Vec<serde_json::Value> = Vec::new();
    let mut total_bytes: u64 = 0;
    if let Err(e) = walk_volume_dir(
        std::path::Path::new(&root),
        std::path::Path::new(&root),
        &mut files,
        &mut total_bytes,
    )
    .await
    {
        return internal_response(&format!("Failed to walk volume directory: {e}"));
    }

    let count = files.len();
    let body = serde_json::json!({
        "fqn": fqn,
        "volume": volume,
        "files": files,
        "count": count,
        "total_bytes": total_bytes,
    });

    tracing::debug!(
        fqn = %fqn,
        volume = %volume,
        count,
        total_bytes,
        "listed offering volume files"
    );

    (StatusCode::OK, axum::Json(body)).into_response()
}

/// Recursive directory walker for [`list_volume_files`].
///
/// Implemented with manual recursion (rather than `walkdir`) to keep
/// the dependency footprint identical to `head_volume_file`'s
/// `tokio::fs::metadata` path. Async recursion needs `Box::pin` to
/// satisfy the borrow checker on the future-returning call site.
fn walk_volume_dir<'a>(
    root: &'a std::path::Path,
    dir: &'a std::path::Path,
    out: &'a mut Vec<serde_json::Value>,
    total_bytes: &'a mut u64,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::io::Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_type = match entry.file_type().await {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "skipping directory entry: failed to read file type"
                    );
                    continue;
                }
            };
            if file_type.is_dir() {
                walk_volume_dir(root, &path, out, total_bytes).await?;
            } else if file_type.is_file() {
                // Compute the volume-relative path with forward slashes
                // (matching the input format of `PUT/HEAD/GET *path`).
                let relative = match path.strip_prefix(root) {
                    Ok(r) => r
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/"),
                    Err(_) => continue,
                };
                let size = match entry.metadata().await {
                    Ok(m) => m.len(),
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "skipping directory entry: failed to read metadata"
                        );
                        continue;
                    }
                };
                *total_bytes += size;
                out.push(serde_json::json!({
                    "path": relative,
                    "size": size,
                }));
            }
            // Symlinks and other entry types are ignored — the
            // listing is for regular files only.
        }
        Ok(())
    })
}

// ── Helpers ────────────────────────────────────────────────────

/// Resolve the host filesystem path for a volume file.
///
/// Layout: `{volumes_dir}/{fqn_encoded}/{volume}/{path}`
///
/// Err variant carries an axum `Response` (≥128 bytes). This is standard
/// for axum handlers that want to early-return a full HTTP response;
/// boxing would obscure the shape with no runtime benefit. Accepted as
/// pre-existing pattern.
#[allow(clippy::result_large_err)]
fn resolve_volume_path(fqn: &str, volume: &str, file_path: &str) -> Result<String, Response> {
    let offering_fqn = garden_common::offerings::OfferingFqn::parse(fqn).map_err(|e| {
        bad_request_response(
            "INVALID_FQN",
            &format!("Invalid offering FQN '{}': {}", fqn, e),
        )
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
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
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
