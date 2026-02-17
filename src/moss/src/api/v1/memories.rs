//! Memories API - Garden-wide access to seed bank backups (read-only)
//!
//! Provides endpoints for external orchestrators to discover and retrieve
//! nurturing backups without exposing garden/storage.
//!
//! - GET /api/v1/memories
//! - GET /api/v1/memories/:offering_id
//! - GET /api/v1/memories/:offering_id/manifest
//! - GET /api/v1/memories/:offering_id/:harvest_id

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
use garden_common::audit::{log_access, AuditAccessEntry};
use garden_common::constants::headers::{
    HEADER_REQUESTING_STONE_ID, HEADER_REQUESTING_STONE_NAME, HEADER_SEED_BANK,
};
use garden_common::constants::paths;
use garden_common::storage::{MemoriesOfferingManifest, DEFAULT_PUBLIC_SEED_BANK_NAME};
use serde::{Deserialize, Serialize};

use crate::domain::nurturing::{RemoteNurturingIndex, RemoteSnapshot};
use crate::infra::storage::SeedBankRegistry;
use crate::{error_response, AppState};

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

#[derive(Debug, Default, Deserialize)]
pub struct SeedBankSelector {
    #[serde(rename = "seed-bank")]
    seed_bank: Option<String>,
}

impl SeedBankSelector {
    fn name(&self) -> Option<String> {
        self.seed_bank.clone()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OfferingSnapshotsResponse {
    pub offering_id: String,
    pub snapshots: Vec<RemoteSnapshot>,
}

enum SeedBankRoute {
    Local {
        mount_path: String,
        seed_bank_id: String,
        seed_bank_name: String,
    },
    Remote {
        endpoint: String,
    },
}

// ============================================================================
// Helpers
// ============================================================================

fn err(status: StatusCode, code: &str, msg: &str) -> (StatusCode, Json<ApiErrorResponse>) {
    error_response(status, code, msg, None)
}

fn error_response_raw(status: StatusCode, code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "code": code,
            "message": message
        }
    });
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.to_string().into())
        .unwrap()
}

fn get_seed_bank_name(headers: &HeaderMap, selector: &SeedBankSelector) -> Option<String> {
    if let Some(name) = headers.get(HEADER_SEED_BANK).and_then(|v| v.to_str().ok()) {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    selector.name()
}

fn validate_seed_bank_layout(mount_path: &str) -> Result<(), String> {
    let memories = std::path::Path::new(mount_path).join(paths::SEED_BANK_MEMORIES_DIR);
    let storage = std::path::Path::new(mount_path).join(paths::SEED_BANK_STORAGE_DIR);

    if !memories.is_dir() || !storage.is_dir() {
        return Err("Seed bank is non-canonical; missing garden/memories and/or garden/storage. Re-prepare the seed bank.".to_string());
    }

    Ok(())
}

fn has_path_traversal(value: &str) -> bool {
    if value.contains('\\') {
        return true;
    }
    std::path::Path::new(value).components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    })
}

fn log_memories_access(
    state: &AppState,
    headers: &HeaderMap,
    status: StatusCode,
    action: &str,
    seed_bank: Option<&str>,
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
        stone_id: state.stone_id.clone(),
        stone_name: state.stone_name.clone(),
        seed_bank: seed_bank.map(|s| s.to_string()),
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

async fn resolve_seed_bank_route(
    state: &AppState,
    name: &str,
) -> Result<SeedBankRoute, (StatusCode, String)> {
    let registry = SeedBankRegistry::scan().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to scan seed banks: {}", e),
        )
    })?;

    if let Some(bank) = registry.get(name) {
        if let Err(msg) = validate_seed_bank_layout(&bank.mount_path) {
            return Err((StatusCode::CONFLICT, msg));
        }
        return Ok(SeedBankRoute::Local {
            mount_path: bank.mount_path.clone(),
            seed_bank_id: bank.id.clone(),
            seed_bank_name: bank.name.clone(),
        });
    }

    let cache = state.storage_cache.read().await;
    for beacon in cache.all_beacons() {
        if beacon.stone_id == state.stone_id {
            continue;
        }
        for sb in &beacon.seed_banks {
            if sb.name == name {
                return Ok(SeedBankRoute::Remote {
                    endpoint: beacon.endpoint.clone(),
                });
            }
        }
    }

    Err((
        StatusCode::SERVICE_UNAVAILABLE,
        format!("Seed bank '{}' not available", name),
    ))
}

async fn proxy_memories_request(
    method: reqwest::Method,
    endpoint: &str,
    path: &str,
    query: Vec<(String, String)>,
    headers: &HeaderMap,
    requesting_stone_id: &str,
    requesting_stone_name: &str,
) -> Response {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/{}",
        endpoint.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let mut request = client.request(method, url);

    if !query.is_empty() {
        request = request.query(&query);
    }

    if let Some(user_agent) = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
    {
        request = request.header(reqwest::header::USER_AGENT, user_agent);
    }
    if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        request = request.header("x-forwarded-for", forwarded);
    }

    if headers.get(HEADER_REQUESTING_STONE_ID).is_some() {
        if let Some(value) = headers
            .get(HEADER_REQUESTING_STONE_ID)
            .and_then(|v| v.to_str().ok())
        {
            request = request.header(HEADER_REQUESTING_STONE_ID, value);
        }
    } else {
        request = request.header(HEADER_REQUESTING_STONE_ID, requesting_stone_id);
    }

    if headers.get(HEADER_REQUESTING_STONE_NAME).is_some() {
        if let Some(value) = headers
            .get(HEADER_REQUESTING_STONE_NAME)
            .and_then(|v| v.to_str().ok())
        {
            request = request.header(HEADER_REQUESTING_STONE_NAME, value);
        }
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
// GET /api/v1/memories - List all snapshots
// ============================================================================

pub async fn list_memories(
    State(state): State<AppState>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<RemoteNurturingIndex>>, (StatusCode, Json<ApiErrorResponse>)> {
    let selected = get_seed_bank_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_PUBLIC_SEED_BANK_NAME.to_string());

    let route = resolve_seed_bank_route(&state, &selected)
        .await
        .map_err(|(status, msg)| err(status, "NO_SEED_BANK", &msg))?;

    match route {
        SeedBankRoute::Local {
            mount_path,
            seed_bank_id,
            seed_bank_name,
        } => {
            let store = crate::infra::storage::SeedBankStore::new_public(&mount_path);
            let index = match state
                .nurturing_store
                .list_remote_snapshots(&store, &seed_bank_id)
                .await
            {
                Ok(index) => {
                    log_memories_access(
                        &state,
                        &headers,
                        StatusCode::OK,
                        ACTION_LIST,
                        Some(&seed_bank_name),
                        None,
                        None,
                    );
                    index
                }
                Err(e) => {
                    log_memories_access(
                        &state,
                        &headers,
                        StatusCode::INTERNAL_SERVER_ERROR,
                        ACTION_LIST,
                        Some(&seed_bank_name),
                        None,
                        None,
                    );
                    return Err(err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "LIST_FAILED",
                        &e.to_string(),
                    ));
                }
            };

            Ok(Json(ApiResponse::new(index)))
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query_params = Vec::new();
            if selected != DEFAULT_PUBLIC_SEED_BANK_NAME {
                query_params.push(("seed-bank".to_string(), selected));
            }
            let response = proxy_memories_request(
                reqwest::Method::GET,
                &endpoint,
                "/api/v1/memories",
                query_params,
                &headers,
                &state.stone_id,
                &state.stone_name,
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
}

// ============================================================================
// GET /api/v1/memories/:offering_id - List snapshots for offering
// ============================================================================

pub async fn list_offering_snapshots(
    State(state): State<AppState>,
    Path(offering_id): Path<String>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<OfferingSnapshotsResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    if offering_id.is_empty() || has_path_traversal(&offering_id) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_OFFERING",
            "Invalid offering id",
        ));
    }

    let selected = get_seed_bank_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_PUBLIC_SEED_BANK_NAME.to_string());

    let route = resolve_seed_bank_route(&state, &selected)
        .await
        .map_err(|(status, msg)| err(status, "NO_SEED_BANK", &msg))?;

    match route {
        SeedBankRoute::Local {
            mount_path,
            seed_bank_id,
            seed_bank_name,
        } => {
            let store = crate::infra::storage::SeedBankStore::new_public(&mount_path);
            let index = match state
                .nurturing_store
                .list_remote_snapshots(&store, &seed_bank_id)
                .await
            {
                Ok(index) => index,
                Err(e) => {
                    log_memories_access(
                        &state,
                        &headers,
                        StatusCode::INTERNAL_SERVER_ERROR,
                        ACTION_LIST_OFFERING,
                        Some(&seed_bank_name),
                        Some(&offering_id),
                        None,
                    );
                    return Err(err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "LIST_FAILED",
                        &e.to_string(),
                    ));
                }
            };

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
                Some(&seed_bank_name),
                Some(&offering_id),
                None,
            );

            Ok(Json(ApiResponse::new(OfferingSnapshotsResponse {
                offering_id,
                snapshots,
            })))
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query_params = Vec::new();
            if selected != DEFAULT_PUBLIC_SEED_BANK_NAME {
                query_params.push(("seed-bank".to_string(), selected));
            }
            let response = proxy_memories_request(
                reqwest::Method::GET,
                &endpoint,
                &format!("/api/v1/memories/{}", offering_id),
                query_params,
                &headers,
                &state.stone_id,
                &state.stone_name,
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
}

// ============================================================================
// GET /api/v1/memories/:offering_id/manifest - Offering manifest
// ============================================================================

pub async fn get_offering_manifest(
    State(state): State<AppState>,
    Path(offering_id): Path<String>,
    Query(selector): Query<SeedBankSelector>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<MemoriesOfferingManifest>>, (StatusCode, Json<ApiErrorResponse>)> {
    if offering_id.is_empty() || has_path_traversal(&offering_id) {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_OFFERING",
            "Invalid offering id",
        ));
    }

    let selected = get_seed_bank_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_PUBLIC_SEED_BANK_NAME.to_string());

    let route = resolve_seed_bank_route(&state, &selected)
        .await
        .map_err(|(status, msg)| err(status, "NO_SEED_BANK", &msg))?;

    match route {
        SeedBankRoute::Local {
            mount_path,
            seed_bank_name,
            ..
        } => {
            let path = paths::seed_bank_memory_offering_manifest_path(&mount_path, &offering_id);
            let json = match tokio::fs::read_to_string(&path).await {
                Ok(json) => json,
                Err(_) => {
                    log_memories_access(
                        &state,
                        &headers,
                        StatusCode::NOT_FOUND,
                        ACTION_MANIFEST,
                        Some(&seed_bank_name),
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
                        Some(&seed_bank_name),
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
                Some(&seed_bank_name),
                Some(&offering_id),
                None,
            );

            Ok(Json(ApiResponse::new(manifest)))
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query_params = Vec::new();
            if selected != DEFAULT_PUBLIC_SEED_BANK_NAME {
                query_params.push(("seed-bank".to_string(), selected));
            }
            let response = proxy_memories_request(
                reqwest::Method::GET,
                &endpoint,
                &format!("/api/v1/memories/{}/manifest", offering_id),
                query_params,
                &headers,
                &state.stone_id,
                &state.stone_name,
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
}

// ============================================================================
// GET /api/v1/memories/:offering_id/:harvest_id - Download snapshot
// ============================================================================

pub async fn download_snapshot(
    State(state): State<AppState>,
    Path((offering_id, harvest_id)): Path<(String, String)>,
    Query(selector): Query<SeedBankSelector>,
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

    let selected = get_seed_bank_name(&headers, &selector)
        .unwrap_or_else(|| DEFAULT_PUBLIC_SEED_BANK_NAME.to_string());

    let route = match resolve_seed_bank_route(&state, &selected).await {
        Ok(route) => route,
        Err((status, msg)) => return error_response_raw(status, "NO_SEED_BANK", &msg),
    };

    match route {
        SeedBankRoute::Local {
            mount_path,
            seed_bank_name,
            ..
        } => {
            let path = paths::seed_bank_memory_harvest_path(&mount_path, &offering_id, &harvest_id);
            let data = match tokio::fs::read(&path).await {
                Ok(bytes) => bytes,
                Err(_) => {
                    log_memories_access(
                        &state,
                        &headers,
                        StatusCode::NOT_FOUND,
                        ACTION_DOWNLOAD,
                        Some(&seed_bank_name),
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
                Some(&seed_bank_name),
                Some(&offering_id),
                Some(&harvest_id),
            );

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/gzip")
                .header(header::CONTENT_LENGTH, data.len())
                .body(Bytes::from(data).into())
                .unwrap()
        }
        SeedBankRoute::Remote { endpoint } => {
            let mut query_params = Vec::new();
            if selected != DEFAULT_PUBLIC_SEED_BANK_NAME {
                query_params.push(("seed-bank".to_string(), selected));
            }
            proxy_memories_request(
                reqwest::Method::GET,
                &endpoint,
                &format!("/api/v1/memories/{}/{}", offering_id, harvest_id),
                query_params,
                &headers,
                &state.stone_id,
                &state.stone_name,
            )
            .await
        }
    }
}
