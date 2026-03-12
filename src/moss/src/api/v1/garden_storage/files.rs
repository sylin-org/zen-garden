//! User file operations on managed storage
//!
//! Exposes the storage mount root as user-accessible content.
//! All I/O dispatch goes through `StorageRouter` — no `match Local/Proxy`.
//! Path validation prevents access into `.zen-garden/`.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
use garden_common::constants::storage::share;
use garden_common::storage::StorageRole;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::domain::storage_service::StorageRoute;
use crate::infra::storage::router::{FileEntry, StorageRouter};
use crate::AppState;

use super::{err, error_response_raw, has_path_traversal, is_proxied, DirectoryEntry, DirectoryListResponse};

// ============================================================================
// Path validation
// ============================================================================

/// Reject paths that use traversal or access managed storage internals.
///
/// Returns `Err((status, message))` on rejection.
fn validate_file_path(path: &str) -> Result<(), (StatusCode, &'static str)> {
    if has_path_traversal(path) {
        return Err((StatusCode::BAD_REQUEST, "Path contains invalid segments"));
    }
    if share::is_blocked_path(path) {
        return Err((
            StatusCode::FORBIDDEN,
            "Access to managed storage internals is not allowed",
        ));
    }
    Ok(())
}

// ============================================================================
// Mapping helpers
// ============================================================================

/// Map `FileEntry` (router type) to `DirectoryEntry` (API wire type).
fn to_directory_entry(e: &FileEntry) -> DirectoryEntry {
    DirectoryEntry {
        name: e.name.clone(),
        entry_type: if e.is_dir { "dir" } else { "file" }.to_string(),
        size: if e.is_dir { None } else { Some(e.size) },
        modified: e.modified.map(|t| t.to_rfc3339()),
    }
}

/// Build a JSON directory listing response.
fn dir_list_response(rel_path: &str, entries: Vec<FileEntry>) -> Response {
    let filtered: Vec<DirectoryEntry> = entries
        .iter()
        .filter(|e| !share::is_blocked_name(&e.name))
        .map(to_directory_entry)
        .collect();

    let response = DirectoryListResponse {
        path: if rel_path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", rel_path)
        },
        entries: filtered,
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
// Proxy loop guard
// ============================================================================

/// Check whether a proxied request has reached a non-primary stone.
///
/// Returns `Err(Response)` if the request is proxied but we're not Primary.
async fn check_proxy_loop_guard(
    name: &str,
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), Response> {
    if is_proxied(headers) {
        if let Some(local) = StorageRoute::find_local(name, &state.current.storage.volumes).await {
            if local.role != StorageRole::Primary {
                return Err(error_response_raw(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "PROXY_LOOP",
                    "Proxied request reached a non-primary stone",
                ));
            }
        }
    }
    Ok(())
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
    if let Err((status, msg)) = validate_file_path(path) {
        return error_response_raw(status, "INVALID_PATH", msg);
    }

    if let Err(resp) = check_proxy_loop_guard(&name, &state, &headers).await {
        return resp;
    }

    let router = match StorageRouter::for_read(
        &name,
        &state.current.storage.volumes,
        &state.tool.registry,
        &state.current.stone.id,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return error_response_raw(
                StatusCode::SERVICE_UNAVAILABLE,
                "NO_STORAGE",
                &e.to_string(),
            )
        }
    };

    // Explicit directory paths → list
    if path.is_empty() || path.ends_with('/') {
        return match router.list(path.trim_end_matches('/')).await {
            Ok(entries) => dir_list_response(path, entries),
            Err(e) => {
                error_response_raw(StatusCode::NOT_FOUND, "NOT_FOUND", &e.to_string())
            }
        };
    }

    // Check if path points to a directory
    if let Ok(meta) = router.metadata(path).await {
        if meta.is_dir {
            return match router.list(path).await {
                Ok(entries) => dir_list_response(path, entries),
                Err(e) => {
                    error_response_raw(StatusCode::NOT_FOUND, "NOT_FOUND", &e.to_string())
                }
            };
        }
    }

    // File read
    match router.read(path).await {
        Ok(data) => {
            let content_type = mime_guess::from_path(path)
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
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("NotFound") || msg.contains("404") {
                error_response_raw(StatusCode::NOT_FOUND, "NOT_FOUND", "File not found")
            } else {
                error_response_raw(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "READ_FAILED",
                    &msg,
                )
            }
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
    if let Err((status, msg)) = validate_file_path(path) {
        return Err(err(status, "INVALID_PATH", msg));
    }

    if is_proxied(&headers) {
        if let Some(local) =
            StorageRoute::find_local(&name, &state.current.storage.volumes).await
        {
            if local.role != StorageRole::Primary {
                return Err(err(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "PROXY_LOOP",
                    "Proxied request reached a non-primary stone",
                ));
            }
        }
    }

    let router = StorageRouter::for_write(
        &name,
        &state.current.storage.volumes,
        &state.tool.registry,
        &state.current.stone.id,
    )
    .await
    .map_err(|e| err(StatusCode::SERVICE_UNAVAILABLE, "NO_STORAGE", &e.to_string()))?;

    let size = body.len() as u64;
    router
        .write(path, &body)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "WRITE_FAILED", &e.to_string()))?;

    debug!(storage = %name, path = %path, size, "garden PUT file");
    Ok(Json(ApiResponse::new(FileWriteResult {
        path: path.to_string(),
        size,
    })))
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
    if let Err((status, msg)) = validate_file_path(path) {
        return Err(err(status, "INVALID_PATH", msg));
    }

    if is_proxied(&headers) {
        if let Some(local) =
            StorageRoute::find_local(&name, &state.current.storage.volumes).await
        {
            if local.role != StorageRole::Primary {
                return Err(err(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "PROXY_LOOP",
                    "Proxied request reached a non-primary stone",
                ));
            }
        }
    }

    let router = StorageRouter::for_write(
        &name,
        &state.current.storage.volumes,
        &state.tool.registry,
        &state.current.stone.id,
    )
    .await
    .map_err(|e| err(StatusCode::SERVICE_UNAVAILABLE, "NO_STORAGE", &e.to_string()))?;

    // Determine if path is a directory or file
    let is_dir = router
        .metadata(path)
        .await
        .map(|m| m.is_dir)
        .unwrap_or(false);

    let result = if is_dir {
        router.delete_dir(path).await
    } else {
        router.delete_file(path).await
    };

    result.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("not found") || msg.contains("NotFound") || msg.contains("404") {
            err(StatusCode::NOT_FOUND, "NOT_FOUND", "File not found")
        } else {
            err(StatusCode::INTERNAL_SERVER_ERROR, "DELETE_FAILED", &msg)
        }
    })?;

    debug!(storage = %name, path = %path, "garden DELETE file");
    Ok(StatusCode::NO_CONTENT)
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
    if let Err((status, msg)) = validate_file_path(path) {
        return error_response_raw(status, "INVALID_PATH", msg);
    }

    if let Err(resp) = check_proxy_loop_guard(&name, &state, &headers).await {
        return resp;
    }

    let router = match StorageRouter::for_read(
        &name,
        &state.current.storage.volumes,
        &state.tool.registry,
        &state.current.stone.id,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return error_response_raw(
                StatusCode::SERVICE_UNAVAILABLE,
                "NO_STORAGE",
                &e.to_string(),
            )
        }
    };

    match router.metadata(path).await {
        Ok(meta) => {
            let content_type = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            let last_modified = meta
                .modified
                .map(|t| t.to_rfc3339())
                .unwrap_or_default();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CONTENT_LENGTH, meta.size)
                .header(header::LAST_MODIFIED, last_modified)
                .body("".into())
                .unwrap()
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("NotFound") || msg.contains("404") {
                error_response_raw(StatusCode::NOT_FOUND, "NOT_FOUND", "File not found")
            } else {
                error_response_raw(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "HEAD_FAILED",
                    &msg,
                )
            }
        }
    }
}
