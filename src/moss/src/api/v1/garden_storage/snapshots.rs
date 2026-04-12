//! Memories (harvest backup) access on managed storage
//!
//! Read-only access to nurturing backups for external orchestrators.
//! Storage name comes from the URL path — no header-based selection.

use super::audit::{AuditAccessEntry, log_access};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::Response,
};
use garden_common::api_utils::ApiResponse;
use garden_common::constants::headers::{HEADER_REQUESTING_STONE_ID, HEADER_REQUESTING_STONE_NAME};
use garden_common::constants::paths;
use garden_common::storage::MemoriesOfferingManifest;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::domain::nurturing::{RemoteNurturingIndex, RemoteSnapshot};
use crate::domain::storage_service::ProxyTarget;
use crate::infra::storage::handle::StorageResolver;

use super::{err, error_response_raw, has_path_traversal};

// ============================================================================
// Constants
// ============================================================================

const AUDIT_CATEGORY: &str = "memories";
const ACTION_LIST: &str = "list";
const ACTION_LIST_OFFERING: &str = "list_offering";
const ACTION_MANIFEST: &str = "manifest";
const ACTION_DOWNLOAD: &str = "download";

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct OfferingSnapshotsResponse {
    pub offering_id: String,
    pub snapshots: Vec<RemoteSnapshot>,
}

// ============================================================================
// Helpers
// ============================================================================

fn validate_storage_layout(mount_path: &std::path::Path) -> Result<(), String> {
    let memories = mount_path.join(paths::STORAGE_MEMORIES_DIR);
    if !memories.is_dir() {
        return Err(
            "Storage layout invalid; missing .zen-garden/memories. Re-prepare the storage."
                .to_string(),
        );
    }
    Ok(())
}

fn log_memories_access(
    state: &AppState,
    headers: &HeaderMap,
    status: StatusCode,
    action: &str,
    storage_name: Option<&str>,
    offering_id: Option<&str>,
    harvest_id: Option<&str>,
) {
    let requesting_stone_id = headers
        .get(HEADER_REQUESTING_STONE_ID)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let requesting_stone_name = headers
        .get(HEADER_REQUESTING_STONE_NAME)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let forwarded_for = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let entry = AuditAccessEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        category: AUDIT_CATEGORY.to_string(),
        action: action.to_string(),
        status: status.as_u16(),
        stone_id: state.current.stone.id.clone(),
        stone_name: state.current.stone.name.clone(),
        storage: storage_name.map(|s| s.to_string()),
        offering_id: offering_id.map(|s| s.to_string()),
        harvest_id: harvest_id.map(|s| s.to_string()),
        requesting_stone_id,
        requesting_stone_name,
        user_agent,
        forwarded_for,
    };

    tokio::spawn(async move {
        log_access(&entry).await;
    });
}

async fn proxy_memories_request(
    method: reqwest::Method,
    target: &ProxyTarget,
    path: &str,
    headers: &HeaderMap,
    requesting_stone_id: &str,
    requesting_stone_name: &str,
) -> Response {
    let url = format!(
        "{}/{}",
        target.endpoint.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let mut request = crate::http::HTTP.request(method, url);

    if let Some(user_agent) = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
    {
        request = request.header(reqwest::header::USER_AGENT, user_agent);
    }
    if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        request = request.header("x-forwarded-for", forwarded);
    }

    if let Some(value) = headers
        .get(HEADER_REQUESTING_STONE_ID)
        .and_then(|v| v.to_str().ok())
    {
        request = request.header(HEADER_REQUESTING_STONE_ID, value);
    } else {
        request = request.header(HEADER_REQUESTING_STONE_ID, requesting_stone_id);
    }

    if let Some(value) = headers
        .get(HEADER_REQUESTING_STONE_NAME)
        .and_then(|v| v.to_str().ok())
    {
        request = request.header(HEADER_REQUESTING_STONE_NAME, value);
    } else {
        request = request.header(HEADER_REQUESTING_STONE_NAME, requesting_stone_name);
    }

    let response = match request.send().await {
        Ok(resp) => resp,
        Err(e) => {
            return error_response_raw(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string());
        }
    };

    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = response.headers().clone();
    let body = response.bytes().await.unwrap_or_default();

    let mut builder = Response::builder().status(status);
    if let Some(value) = resp_headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        builder = builder.header(header::CONTENT_TYPE, value);
    }
    if let Some(value) = resp_headers
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
    {
        builder = builder.header(header::CONTENT_LENGTH, value);
    }

    builder.body(body.into()).unwrap()
}

// ============================================================================
// GET /api/v1/garden/storage/{name}/memories
// ============================================================================

/// List all offering snapshots on a storage.
pub async fn list_memories_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> crate::api::ApiResult<RemoteNurturingIndex> {
    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };
    let handle = resolver.for_read(&name).await.map_err(|e| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "NO_STORAGE",
            &e.to_string(),
        )
    })?;

    if let Some(mount_path) = handle.mount_path() {
        if let Err(msg) = validate_storage_layout(mount_path) {
            return Err(err(StatusCode::CONFLICT, "INVALID_LAYOUT", &msg));
        }
        let store = handle.content_store_for_read().unwrap();
        let volume_id = handle.volume_id().unwrap();
        match state
            .nurturing
            .store
            .list_remote_snapshots(&store, volume_id)
            .await
        {
            Ok(index) => {
                log_memories_access(
                    &state,
                    &headers,
                    StatusCode::OK,
                    ACTION_LIST,
                    Some(handle.storage_name()),
                    None,
                    None,
                );
                crate::api::ok(index)
            }
            Err(e) => {
                log_memories_access(
                    &state,
                    &headers,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ACTION_LIST,
                    Some(handle.storage_name()),
                    None,
                    None,
                );
                Err(err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "LIST_FAILED",
                    &e.to_string(),
                ))
            }
        }
    } else {
        // Remote — proxy
        let target = handle.proxy_target().unwrap();
        let response = proxy_memories_request(
            reqwest::Method::GET,
            target,
            &format!("api/v1/garden/storage/{}/memories", name),
            &headers,
            &state.current.stone.id,
            &state.current.stone.name,
        )
        .await;

        if response.status() != StatusCode::OK {
            return Err(err(
                StatusCode::BAD_GATEWAY,
                "UPSTREAM_ERROR",
                "Failed to list memories",
            ));
        }

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|e| err(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string()))?;
        let data: ApiResponse<RemoteNurturingIndex> = serde_json::from_slice(&bytes)
            .map_err(|e| err(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string()))?;
        Ok(Json(data))
    }
}

// ============================================================================
// GET /api/v1/garden/storage/{name}/memories/{offering_id}
// ============================================================================

/// List snapshots for a specific offering.
pub async fn list_offering_snapshots_v1(
    State(state): State<AppState>,
    Path((name, offering_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> crate::api::ApiResult<OfferingSnapshotsResponse> {
    if offering_id.is_empty() || has_path_traversal(&offering_id) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_OFFERING",
            "Invalid offering id",
        ));
    }

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };
    let handle = resolver.for_read(&name).await.map_err(|e| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "NO_STORAGE",
            &e.to_string(),
        )
    })?;

    if let Some(mount_path) = handle.mount_path() {
        if let Err(msg) = validate_storage_layout(mount_path) {
            return Err(err(StatusCode::CONFLICT, "INVALID_LAYOUT", &msg));
        }
        let store = handle.content_store_for_read().unwrap();
        let volume_id = handle.volume_id().unwrap();
        let index = state
            .nurturing
            .store
            .list_remote_snapshots(&store, volume_id)
            .await
            .map_err(|e| {
                log_memories_access(
                    &state,
                    &headers,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ACTION_LIST_OFFERING,
                    Some(handle.storage_name()),
                    Some(&offering_id),
                    None,
                );
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "LIST_FAILED",
                    &e.to_string(),
                )
            })?;

        let snapshots: Vec<RemoteSnapshot> = index
            .snapshots
            .into_iter()
            .filter(|s| s.offering_id == offering_id)
            .collect();

        log_memories_access(
            &state,
            &headers,
            StatusCode::OK,
            ACTION_LIST_OFFERING,
            Some(handle.storage_name()),
            Some(&offering_id),
            None,
        );

        crate::api::ok(OfferingSnapshotsResponse {
            offering_id,
            snapshots,
        })
    } else {
        // Remote — proxy
        let target = handle.proxy_target().unwrap();
        let response = proxy_memories_request(
            reqwest::Method::GET,
            target,
            &format!("api/v1/garden/storage/{}/memories/{}", name, offering_id),
            &headers,
            &state.current.stone.id,
            &state.current.stone.name,
        )
        .await;

        if response.status() != StatusCode::OK {
            return Err(err(
                StatusCode::BAD_GATEWAY,
                "UPSTREAM_ERROR",
                "Failed to list snapshots",
            ));
        }

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|e| err(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string()))?;
        let data: ApiResponse<OfferingSnapshotsResponse> = serde_json::from_slice(&bytes)
            .map_err(|e| err(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string()))?;
        Ok(Json(data))
    }
}

// ============================================================================
// GET /api/v1/garden/storage/{name}/memories/{offering_id}/manifest
// ============================================================================

/// Read the offering manifest from memories.
pub async fn get_offering_manifest_v1(
    State(state): State<AppState>,
    Path((name, offering_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> crate::api::ApiResult<MemoriesOfferingManifest> {
    if offering_id.is_empty() || has_path_traversal(&offering_id) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_OFFERING",
            "Invalid offering id",
        ));
    }

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };
    let handle = resolver.for_read(&name).await.map_err(|e| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "NO_STORAGE",
            &e.to_string(),
        )
    })?;

    if let Some(mount_path) = handle.mount_path() {
        let mount = mount_path.to_string_lossy().into_owned();
        let path = paths::storage_memory_offering_manifest_path(&mount, &offering_id);
        let json = match tokio::fs::read_to_string(&path).await {
            Ok(json) => json,
            Err(_) => {
                log_memories_access(
                    &state,
                    &headers,
                    StatusCode::NOT_FOUND,
                    ACTION_MANIFEST,
                    Some(handle.storage_name()),
                    Some(&offering_id),
                    None,
                );
                return Err(err(
                    StatusCode::NOT_FOUND,
                    "NOT_FOUND",
                    "Offering manifest not found",
                ));
            }
        };

        let manifest: MemoriesOfferingManifest = match serde_json::from_str(&json) {
            Ok(m) => m,
            Err(e) => {
                log_memories_access(
                    &state,
                    &headers,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ACTION_MANIFEST,
                    Some(handle.storage_name()),
                    Some(&offering_id),
                    None,
                );
                return Err(err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "PARSE_FAILED",
                    &e.to_string(),
                ));
            }
        };

        log_memories_access(
            &state,
            &headers,
            StatusCode::OK,
            ACTION_MANIFEST,
            Some(handle.storage_name()),
            Some(&offering_id),
            None,
        );

        crate::api::ok(manifest)
    } else {
        // Remote — proxy
        let target = handle.proxy_target().unwrap();
        let response = proxy_memories_request(
            reqwest::Method::GET,
            target,
            &format!(
                "api/v1/garden/storage/{}/memories/{}/manifest",
                name, offering_id
            ),
            &headers,
            &state.current.stone.id,
            &state.current.stone.name,
        )
        .await;

        if response.status() != StatusCode::OK {
            return Err(err(
                StatusCode::BAD_GATEWAY,
                "UPSTREAM_ERROR",
                "Failed to read offering manifest",
            ));
        }

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|e| err(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string()))?;
        let data: ApiResponse<MemoriesOfferingManifest> = serde_json::from_slice(&bytes)
            .map_err(|e| err(StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", &e.to_string()))?;
        Ok(Json(data))
    }
}

// ============================================================================
// GET /api/v1/garden/storage/{name}/memories/{offering_id}/{harvest_id}
// ============================================================================

/// Download a snapshot tarball.
pub async fn download_snapshot_v1(
    State(state): State<AppState>,
    Path((name, offering_id, harvest_id)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    if offering_id.is_empty() || has_path_traversal(&offering_id) {
        return error_response_raw(
            StatusCode::BAD_REQUEST,
            "INVALID_OFFERING",
            "Invalid offering id",
        );
    }
    if harvest_id.is_empty() || has_path_traversal(&harvest_id) {
        return error_response_raw(
            StatusCode::BAD_REQUEST,
            "INVALID_HARVEST",
            "Invalid harvest id",
        );
    }

    let resolver = StorageResolver {
        volumes: &state.current.storage.volumes,
        registry: &state.tool.registry,
        stone_id: &state.current.stone.id,
        tick: None,
    };
    let handle = match resolver.for_read(&name).await {
        Ok(h) => h,
        Err(e) => {
            return error_response_raw(
                StatusCode::SERVICE_UNAVAILABLE,
                "NO_STORAGE",
                &e.to_string(),
            );
        }
    };

    if let Some(mount_path) = handle.mount_path() {
        let mount = mount_path.to_string_lossy().into_owned();
        let path = paths::storage_memory_harvest_path(&mount, &offering_id, &harvest_id);
        let data = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(_) => {
                log_memories_access(
                    &state,
                    &headers,
                    StatusCode::NOT_FOUND,
                    ACTION_DOWNLOAD,
                    Some(handle.storage_name()),
                    Some(&offering_id),
                    Some(&harvest_id),
                );
                return error_response_raw(
                    StatusCode::NOT_FOUND,
                    "NOT_FOUND",
                    "Snapshot not found",
                );
            }
        };

        log_memories_access(
            &state,
            &headers,
            StatusCode::OK,
            ACTION_DOWNLOAD,
            Some(handle.storage_name()),
            Some(&offering_id),
            Some(&harvest_id),
        );

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/gzip")
            .header(header::CONTENT_LENGTH, data.len())
            .body(Bytes::from(data).into())
            .unwrap()
    } else {
        // Remote — proxy
        let target = handle.proxy_target().unwrap();
        proxy_memories_request(
            reqwest::Method::GET,
            target,
            &format!(
                "api/v1/garden/storage/{}/memories/{}/{}",
                name, offering_id, harvest_id
            ),
            &headers,
            &state.current.stone.id,
            &state.current.stone.name,
        )
        .await
    }
}
