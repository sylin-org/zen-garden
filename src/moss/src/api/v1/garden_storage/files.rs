//! User file operations on managed storage
//!
//! Exposes the storage mount root as user-accessible content.
//! Path validation prevents access into `.zen-garden/`.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
use garden_common::constants::paths;
use garden_common::storage::StorageRole;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::domain::storage_service::StorageRoute;
use crate::AppState;

use super::{
    err, error_response_raw, has_path_traversal, is_proxied, proxy_request, DirectoryEntry,
    DirectoryListResponse,
};

// ============================================================================
// Path validation
// ============================================================================

/// Reject paths that access the `.zen-garden/` dotfolder or use traversal.
fn validate_file_path(path: &str) -> Result<(), &'static str> {
    if has_path_traversal(path) {
        return Err("Path contains invalid segments");
    }
    let normalized = path.trim_start_matches('/');
    if normalized.starts_with(paths::STORAGE_DOTFOLDER)
        || normalized.starts_with(&format!("{}\\", paths::STORAGE_DOTFOLDER))
    {
        return Err("Access to managed storage internals is not allowed");
    }
    // Also block the symlink name
    if normalized.starts_with("Zen Garden") {
        return Err("Access to managed storage internals is not allowed");
    }
    Ok(())
}

// ============================================================================
// Directory listing helper
// ============================================================================

/// List entries in a directory under the storage root.
async fn list_directory(mount_path: &std::path::Path, rel_path: &str) -> Response {
    let full = if rel_path.is_empty() {
        mount_path.to_path_buf()
    } else {
        mount_path.join(rel_path)
    };

    if !full.is_dir() {
        return error_response_raw(StatusCode::NOT_FOUND, "NOT_FOUND", "Directory not found");
    }

    let mut entries = Vec::new();
    let mut dir = match tokio::fs::read_dir(&full).await {
        Ok(d) => d,
        Err(e) => {
            return error_response_raw(
                StatusCode::INTERNAL_SERVER_ERROR,
                "READ_DIR_FAILED",
                &e.to_string(),
            )
        }
    };

    while let Ok(Some(entry)) = dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();

        // Hide .zen-garden and its symlink from listings
        if name == paths::STORAGE_DOTFOLDER || name == "Zen Garden" {
            continue;
        }

        let meta = entry.metadata().await.ok();
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);

        entries.push(DirectoryEntry {
            name,
            entry_type: if is_dir { "dir" } else { "file" }.to_string(),
            size: if is_dir {
                None
            } else {
                meta.as_ref().map(|m| m.len())
            },
            modified: meta
                .and_then(|m| m.modified().ok())
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()),
        });
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let response = DirectoryListResponse {
        path: if rel_path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", rel_path)
        },
        entries,
        truncated: false,
    };

    match serde_json::to_string(&ApiResponse::new(response)) {
        Ok(body) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.into())
            .unwrap(),
        Err(e) => error_response_raw(
            StatusCode::INTERNAL_SERVER_ERROR,
            "SERIALIZE_FAILED",
            &e.to_string(),
        ),
    }
}

// ============================================================================
// File write result
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct FileWriteResult {
    path: String,
    size: u64,
}

// ============================================================================
// GET /api/v1/garden/storage/{name}/files/{*path}
// ============================================================================

/// Read a user file or list a directory from the storage root.
pub async fn get_file_v1(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let path = path.trim_start_matches('/');
    if let Err(msg) = validate_file_path(path) {
        return error_response_raw(StatusCode::BAD_REQUEST, "INVALID_PATH", msg);
    }

    if is_proxied(&headers) {
        if let Some(local) = StorageRoute::find_local(&name, &state.current.storage.volumes).await {
            if local.role != StorageRole::Primary {
                return error_response_raw(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "PROXY_LOOP",
                    "Proxied request reached a non-primary stone",
                );
            }
        }
    }

    let route = match StorageRoute::for_read(&name, &state.current.storage.volumes, &state.tool.registry, &state.current.stone.id).await {
        Ok(r) => r,
        Err(e) => {
            return error_response_raw(
                StatusCode::SERVICE_UNAVAILABLE,
                "NO_STORAGE",
                &e.to_string(),
            )
        }
    };

    match route {
        StorageRoute::Local(local) => {
            let full = local.mount_path.join(path);

            // Directory listing
            if path.is_empty() || path.ends_with('/') || full.is_dir() {
                return list_directory(&local.mount_path, path).await;
            }

            // File read
            match tokio::fs::read(&full).await {
                Ok(data) => {
                    let content_type = mime_guess::from_path(&full)
                        .first_or_octet_stream()
                        .to_string();
                    debug!(storage = %name, path = %path, size = data.len(), "garden GET file");
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, content_type)
                        .header(header::CONTENT_LENGTH, data.len())
                        .body(data.into())
                        .unwrap()
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    error_response_raw(StatusCode::NOT_FOUND, "NOT_FOUND", "File not found")
                }
                Err(e) => error_response_raw(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "READ_FAILED",
                    &e.to_string(),
                ),
            }
        }
        StorageRoute::Proxy(target) => {
            proxy_request(
                reqwest::Method::GET,
                &target,
                &format!("api/v1/garden/storage/{}/files/{}", name, path),
                "",
                &headers,
                None,
            )
            .await
        }
    }
}

// ============================================================================
// PUT /api/v1/garden/storage/{name}/files/{*path}
// ============================================================================

/// Write a user file to the storage root.
pub async fn put_file_v1(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<FileWriteResult>>, (StatusCode, Json<ApiErrorResponse>)> {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_PATH",
            "File path is required",
        ));
    }
    if let Err(msg) = validate_file_path(path) {
        return Err(err(StatusCode::BAD_REQUEST, "INVALID_PATH", msg));
    }

    if is_proxied(&headers) {
        if let Some(local) = StorageRoute::find_local(&name, &state.current.storage.volumes).await {
            if local.role != StorageRole::Primary {
                return Err(err(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "PROXY_LOOP",
                    "Proxied request reached a non-primary stone",
                ));
            }
        }
    }

    let route = StorageRoute::for_write(&name, &state.current.storage.volumes, &state.tool.registry, &state.current.stone.id)
        .await
        .map_err(|e| err(StatusCode::SERVICE_UNAVAILABLE, "NO_STORAGE", &e.to_string()))?;

    match route {
        StorageRoute::Local(local) => {
            let full = local.mount_path.join(path);

            // Ensure parent directory exists
            if let Some(parent) = full.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "MKDIR_FAILED",
                        &e.to_string(),
                    )
                })?;
            }

            let size = body.len() as u64;
            tokio::fs::write(&full, &body).await.map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "WRITE_FAILED",
                    &e.to_string(),
                )
            })?;

            debug!(storage = %name, path = %path, size, "garden PUT file");
            Ok(Json(ApiResponse::new(FileWriteResult {
                path: path.to_string(),
                size,
            })))
        }
        StorageRoute::Proxy(target) => {
            let response = proxy_request(
                reqwest::Method::PUT,
                &target,
                &format!("api/v1/garden/storage/{}/files/{}", name, path),
                "",
                &headers,
                Some(body),
            )
            .await;

            if response.status() != StatusCode::OK && response.status() != StatusCode::CREATED {
                return Err(err(
                    StatusCode::BAD_GATEWAY,
                    "UPSTREAM_ERROR",
                    "Failed to write file on primary",
                ));
            }
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .map_err(|e| err(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string()))?;
            let data: ApiResponse<FileWriteResult> = serde_json::from_slice(&bytes)
                .map_err(|e| err(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string()))?;
            Ok(Json(data))
        }
    }
}

// ============================================================================
// DELETE /api/v1/garden/storage/{name}/files/{*path}
// ============================================================================

/// Delete a user file from the storage root.
pub async fn delete_file_v1(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<ApiErrorResponse>)> {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_PATH",
            "File path is required",
        ));
    }
    if let Err(msg) = validate_file_path(path) {
        return Err(err(StatusCode::BAD_REQUEST, "INVALID_PATH", msg));
    }

    if is_proxied(&headers) {
        if let Some(local) = StorageRoute::find_local(&name, &state.current.storage.volumes).await {
            if local.role != StorageRole::Primary {
                return Err(err(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "PROXY_LOOP",
                    "Proxied request reached a non-primary stone",
                ));
            }
        }
    }

    let route = StorageRoute::for_write(&name, &state.current.storage.volumes, &state.tool.registry, &state.current.stone.id)
        .await
        .map_err(|e| err(StatusCode::SERVICE_UNAVAILABLE, "NO_STORAGE", &e.to_string()))?;

    match route {
        StorageRoute::Local(local) => {
            let full = local.mount_path.join(path);
            if !full.exists() {
                return Err(err(StatusCode::NOT_FOUND, "NOT_FOUND", "File not found"));
            }

            if full.is_dir() {
                tokio::fs::remove_dir_all(&full).await.map_err(|e| {
                    err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "DELETE_FAILED",
                        &e.to_string(),
                    )
                })?;
            } else {
                tokio::fs::remove_file(&full).await.map_err(|e| {
                    err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "DELETE_FAILED",
                        &e.to_string(),
                    )
                })?;
            }

            debug!(storage = %name, path = %path, "garden DELETE file");
            Ok(StatusCode::NO_CONTENT)
        }
        StorageRoute::Proxy(target) => {
            let response = proxy_request(
                reqwest::Method::DELETE,
                &target,
                &format!("api/v1/garden/storage/{}/files/{}", name, path),
                "",
                &headers,
                None,
            )
            .await;

            if response.status() != StatusCode::NO_CONTENT && response.status() != StatusCode::OK {
                return Err(err(
                    StatusCode::BAD_GATEWAY,
                    "UPSTREAM_ERROR",
                    "Failed to delete file on primary",
                ));
            }
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

// ============================================================================
// HEAD /api/v1/garden/storage/{name}/files/{*path}
// ============================================================================

/// Get file metadata from the storage root.
pub async fn head_file_v1(
    State(state): State<AppState>,
    Path((name, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let path = path.trim_start_matches('/');
    if let Err(msg) = validate_file_path(path) {
        return error_response_raw(StatusCode::BAD_REQUEST, "INVALID_PATH", msg);
    }

    if is_proxied(&headers) {
        if let Some(local) = StorageRoute::find_local(&name, &state.current.storage.volumes).await {
            if local.role != StorageRole::Primary {
                return error_response_raw(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "PROXY_LOOP",
                    "Proxied request reached a non-primary stone",
                );
            }
        }
    }

    let route = match StorageRoute::for_read(&name, &state.current.storage.volumes, &state.tool.registry, &state.current.stone.id).await {
        Ok(r) => r,
        Err(e) => {
            return error_response_raw(
                StatusCode::SERVICE_UNAVAILABLE,
                "NO_STORAGE",
                &e.to_string(),
            )
        }
    };

    match route {
        StorageRoute::Local(local) => {
            let full = local.mount_path.join(path);
            match tokio::fs::metadata(&full).await {
                Ok(meta) => {
                    let content_type = mime_guess::from_path(&full)
                        .first_or_octet_stream()
                        .to_string();
                    let last_modified = meta
                        .modified()
                        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                        .unwrap_or_default();
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, content_type)
                        .header(header::CONTENT_LENGTH, meta.len())
                        .header(header::LAST_MODIFIED, last_modified)
                        .body("".into())
                        .unwrap()
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    error_response_raw(StatusCode::NOT_FOUND, "NOT_FOUND", "File not found")
                }
                Err(e) => error_response_raw(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "HEAD_FAILED",
                    &e.to_string(),
                ),
            }
        }
        StorageRoute::Proxy(target) => {
            proxy_request(
                reqwest::Method::HEAD,
                &target,
                &format!("api/v1/garden/storage/{}/files/{}", name, path),
                "",
                &headers,
                None,
            )
            .await
        }
    }
}
