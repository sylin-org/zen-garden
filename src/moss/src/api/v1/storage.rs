//! Storage API endpoints for seed bank management
//!
//! Design: USB device manifests ARE the source of truth. No persistence file.
//! The registry is built in-memory by scanning mounted devices.
//!
//! ## API Structure (STORAGE-0002)
//!
//! ```text
//! /api/v1/stone/storage                     GET  → Overview (bank types, counts)
//! /api/v1/stone/storage/bank                GET  → List all seed banks (ApiResponse)
//! /api/v1/stone/storage/bank/:id            GET  → Bank details + root objects (ApiResponse)
//! /api/v1/stone/storage/bank/:id/*path      GET  → Get object (raw bytes)
//! /api/v1/stone/storage/bank/:id/*path      PUT  → Create/update object (ApiResponse)
//! /api/v1/stone/storage/bank/:id/*path      DELETE → Delete object (ApiResponse)
//! /api/v1/stone/storage/bank/:id/*path      HEAD → Object metadata (headers)
//! ```
//!
//! See docs/decisions/STORAGE-0002-api-structure.md

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
use garden_common::storage::{
    DeviceState, PrepareSeedBankRequest,
    RenameSeedBankRequest, SeedBankInfo, SetVisibilityRequest,
    StorageDetectedInfo,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info, warn};


use crate::{error_response, AppState};
use crate::infra::storage::{analyze_device, ObjectStore, SeedBankRegistry};

// ============================================================================
// Response Types
// ============================================================================

/// Storage overview for GET /api/v1/stone/storage
#[derive(Debug, Serialize)]
pub struct StorageOverview {
    /// Number of local mounted seed banks
    pub bank_count: usize,
    /// Total capacity across all local banks (bytes)
    pub total_capacity_bytes: u64,
    /// Total used space across all local banks (bytes)
    pub total_used_bytes: u64,
    /// Storage types available
    pub types: Vec<StorageTypeInfo>,
    /// All seed banks across the garden (from storage_cache)
    pub garden_banks: Vec<GardenBankInfo>,
}

/// Info about a remote seed bank in the garden
#[derive(Debug, Serialize)]
pub struct GardenBankInfo {
    /// Unique seed bank ID
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Stone ID hosting this bank
    pub stone_id: String,
    /// Stone name hosting this bank
    pub stone_name: String,
    /// API endpoint for the stone
    pub endpoint: String,
    /// Whether this bank is on the local stone
    pub is_local: bool,
    /// Visibility ("open", "closed", "read-only")
    pub visibility: String,
    /// Health status
    pub health: String,
    /// Capacity in bytes
    pub capacity_bytes: u64,
    /// Used space in bytes
    pub used_bytes: u64,
}

/// Info about a storage type
#[derive(Debug, Serialize)]
pub struct StorageTypeInfo {
    pub name: String,
    pub count: usize,
    pub endpoint: String,
}

/// Response for prepare endpoint (async job)
#[derive(Debug, Serialize)]
pub struct PrepareAcceptedResponse {
    pub accepted: bool,
    pub job_id: String,
    pub message: String,
}

/// Response for release endpoint
#[derive(Debug, Serialize)]
pub struct ReleaseResponse {
    pub released: bool,
    pub name: String,
    pub message: String,
}

/// Object metadata response
#[derive(Debug, Serialize)]
pub struct ObjectMeta {
    pub key: String,
    pub size: u64,
    pub content_type: String,
    pub etag: String,
    pub last_modified: String,
}

/// Object list response for bank root
#[derive(Debug, Serialize)]
pub struct ObjectListResponse {
    pub bank_id: String,
    pub prefix: String,
    pub objects: Vec<ObjectMeta>,
    pub common_prefixes: Vec<String>,
}

/// Directory listing response with depth support
#[derive(Debug, Serialize)]
pub struct DirectoryListResponse {
    pub path: String,
    pub entries: Vec<DirectoryEntry>,
    pub truncated: bool,
}

/// Single entry in a directory listing
#[derive(Debug, Serialize)]
pub struct DirectoryEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,  // "file" or "dir"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

/// Query parameters for object listing
#[derive(Debug, Deserialize, Default)]
pub struct ListQueryParams {
    /// Depth of listing: 1 (default), 2, 3, ..., or "all"/-1 for recursive
    #[serde(default)]
    pub depth: Option<String>,
}

impl ListQueryParams {
    /// Parse depth parameter to a numeric value
    /// Returns None for unlimited (recursive), Some(n) for n levels
    pub fn parse_depth(&self) -> Option<usize> {
        match self.depth.as_deref() {
            None | Some("1") => Some(1),
            Some("all") | Some("-1") => None,
            Some(s) => s.parse().ok(),
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn err(status: StatusCode, code: &str, msg: &str) -> (StatusCode, Json<ApiErrorResponse>) {
    error_response(status, code, msg, None)
}

// ============================================================================
// GET /api/v1/stone/storage - Storage Overview
// ============================================================================

/// Get storage overview (types, counts)
/// 
/// Returns local bank stats plus garden-wide view from storage_cache.
pub async fn storage_overview_v1(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<StorageOverview>>, (StatusCode, Json<ApiErrorResponse>)> {
    // Get local banks from filesystem (for local stats)
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    let local_banks = registry.list();
    let total_capacity: u64 = local_banks.iter().map(|b| b.capacity_bytes).sum();
    let total_used: u64 = local_banks.iter().map(|b| b.used_bytes).sum();
    
    // Get garden-wide view from storage_cache
    let storage_cache = state.storage_cache.read().await;
    let mut garden_banks = Vec::new();
    
    for beacon in storage_cache.all_beacons() {
        let is_local = beacon.stone_id == state.stone_id;
        for sb in &beacon.seed_banks {
            garden_banks.push(GardenBankInfo {
                id: sb.id.clone(),
                name: sb.name.clone(),
                stone_id: beacon.stone_id.clone(),
                stone_name: beacon.stone_name.clone(),
                endpoint: beacon.endpoint.clone(),
                is_local,
                visibility: sb.visibility.clone(),
                health: sb.health.clone(),
                capacity_bytes: sb.capacity_bytes,
                used_bytes: sb.used_bytes,
            });
        }
    }
    
    let overview = StorageOverview {
        bank_count: local_banks.len(),
        total_capacity_bytes: total_capacity,
        total_used_bytes: total_used,
        types: vec![
            StorageTypeInfo {
                name: "bank".to_string(),
                count: local_banks.len(),
                endpoint: "/api/v1/stone/storage/bank".to_string(),
            },
        ],
        garden_banks,
    };
    
    Ok(Json(ApiResponse::new(overview)))
}

// ============================================================================
// GET /api/v1/stone/storage/bank - List Banks
// ============================================================================

/// List all seed banks
pub async fn list_banks_v1(
    State(_state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<SeedBankInfo>>>, (StatusCode, Json<ApiErrorResponse>)> {
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    let banks: Vec<SeedBankInfo> = registry.list().into_iter().cloned().collect();
    Ok(Json(ApiResponse::new(banks)))
}

// ============================================================================
// GET /api/v1/stone/storage/bank/:id - Get Bank Details
// ============================================================================

/// Get seed bank details and list root objects
pub async fn get_bank_v1(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<SeedBankInfo>>, (StatusCode, Json<ApiErrorResponse>)> {
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    let bank = registry.get(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &format!("Bank '{}' not found", id)))?;
    
    Ok(Json(ApiResponse::new(bank.clone())))
}

// ============================================================================
// DELETE /api/v1/stone/storage/bank/:id - Delete Bank
// ============================================================================

/// Remove seed bank mount directory (device must be unmounted first)
pub async fn delete_bank_v1(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiErrorResponse>)> {
    // Check if still mounted
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    if registry.exists(&id) {
        return Err(err(StatusCode::CONFLICT, "BANK_MOUNTED", "Bank must be released before deletion"));
    }
    
    // Remove mount directory if it exists
    let data_dir = garden_common::constants::paths::data_dir();
    let mount_dir = PathBuf::from(&data_dir).join("mounts").join(&id);
    
    if mount_dir.exists() {
        #[cfg(target_os = "linux")]
        {
            let output = tokio::process::Command::new("sudo")
                .args(["rm", "-rf", &mount_dir.to_string_lossy()])
                .output().await
                .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "DELETE_FAILED", &e.to_string()))?;
            if !output.status.success() {
                return Err(err(StatusCode::INTERNAL_SERVER_ERROR, "DELETE_FAILED", 
                    &String::from_utf8_lossy(&output.stderr)));
            }
        }
        #[cfg(not(target_os = "linux"))]
        tokio::fs::remove_dir_all(&mount_dir).await
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "DELETE_FAILED", &e.to_string()))?;
    }
    
    let event = crate::app_state::MossEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: "info".to_string(),
        message: format!("[STORAGE] Removed bank: {}", id),
        job_id: None,
    };
    let _ = state.event_tx.send(event);
    
    info!(id = %id, "Bank mount directory removed");
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// POST /api/v1/stone/storage/bank/:id/release - Release Bank
// ============================================================================

/// Safely unmount a seed bank
pub async fn release_bank_v1(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ReleaseResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    let _bank = registry.get(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &format!("Bank '{}' not found", id)))?;
    
    #[cfg(target_os = "linux")]
    unmount_device(&_bank.mount_path).await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "UNMOUNT_FAILED", &e.to_string()))?;
    
    let event = crate::app_state::MossEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: "info".to_string(),
        message: format!("[STORAGE] Released bank: {}", id),
        job_id: None,
    };
    let _ = state.event_tx.send(event);
    
    if let Err(e) = garden_common::console::print_storage_released_ribbon(&id) {
        warn!("Failed to print released ribbon: {}", e);
    }
    
    // STORAGE-0003: Update local storage cache AND broadcast beacon
    let storage_cache = state.storage_cache.clone();
    let stone_id = state.stone_id.clone();
    let stone_name = state.stone_name.clone();
    let endpoint = state.self_entry.read().await.endpoint.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::infra::storage::update_and_broadcast(&storage_cache, &stone_id, &stone_name, &endpoint).await {
            warn!(error = %e, "Failed to update storage cache and broadcast beacon after release");
        }
    });
    
    info!(id = %id, "Bank released");
    Ok(Json(ApiResponse::new(ReleaseResponse {
        released: true,
        name: id,
        message: "Bank safely released. You may now remove the device.".to_string(),
    })))
}

// ============================================================================
// PATCH /api/v1/stone/storage/bank/:id/rename - Rename Bank
// ============================================================================

/// Rename a seed bank
pub async fn rename_bank_v1(
    State(_state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<RenameSeedBankRequest>,
) -> Result<Json<ApiResponse<SeedBankInfo>>, (StatusCode, Json<ApiErrorResponse>)> {
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    let bank = registry.get(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &format!("Bank '{}' not found", id)))?;
    
    // Validate new name
    if request.new_name.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "INVALID_NAME", "New name cannot be empty"));
    }
    
    // Check if new name already exists
    if registry.exists(&request.new_name) {
        return Err(err(StatusCode::CONFLICT, "NAME_EXISTS", 
            &format!("Bank '{}' already exists", request.new_name)));
    }
    
    // Update manifest on device
    update_manifest_name(&bank.mount_path, &request.new_name).await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "RENAME_FAILED", &e.to_string()))?;
    
    // Re-scan to get updated info
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    let updated = registry.get(&request.new_name)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", "Bank not found after rename"))?;
    
    info!(old_id = %id, new_id = %request.new_name, "Bank renamed");
    Ok(Json(ApiResponse::new(updated.clone())))
}

// ============================================================================
// GET /api/v1/stone/storage/bank/:id/*path - Get Object or List Directory
// ============================================================================

/// Get an object from a bank (raw bytes) or list directory contents
/// 
/// If path ends with `/`, returns a directory listing with optional depth:
/// - `?depth=1` (default): immediate children only
/// - `?depth=3`: 3 levels deep
/// - `?depth=all` or `?depth=-1`: full recursive listing
pub async fn get_object_v1(
    State(_state): State<AppState>,
    Path((id, path)): Path<(String, String)>,
    Query(params): Query<ListQueryParams>,
) -> Response {
    let registry = match SeedBankRegistry::scan().await {
        Ok(r) => r,
        Err(e) => return error_response_raw(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    
    let bank = match registry.get(&id) {
        Some(b) => b,
        None => return error_response_raw(StatusCode::NOT_FOUND, &format!("Bank '{}' not found", id)),
    };
    
    let store = ObjectStore::new(&bank.mount_path);
    
    // Path format: app/bucket/key or just key (defaults to zen-garden/default)
    let (app, bucket, key) = parse_object_path(&path);
    
    // If path ends with /, treat as directory listing
    if path.ends_with('/') || key.is_empty() {
        return handle_directory_listing(&store, &id, &app, &bucket, &key, &params).await;
    }
    
    // Otherwise, get object
    match store.get_object(&app, &bucket, &key).await {
        Ok(Some((data, meta))) => {
            debug!(bank = %id, key = %key, size = data.len(), "GET object success");
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, &meta.content_type)
                .header(header::CONTENT_LENGTH, data.len())
                .header(header::ETAG, &meta.etag)
                .body(data.into())
                .unwrap()
        }
        Ok(None) => error_response_raw(StatusCode::NOT_FOUND, "Object not found"),
        Err(e) => error_response_raw(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// Handle directory listing with depth parameter
async fn handle_directory_listing(
    store: &ObjectStore,
    bank_id: &str,
    app: &str,
    bucket: &str,
    prefix: &str,
    params: &ListQueryParams,
) -> Response {
    let max_depth = params.parse_depth();
    let delimiter = if max_depth == Some(1) { Some("/") } else { None };
    
    match store.list_objects(app, bucket, Some(prefix), delimiter, None, 1000).await {
        Ok(result) => {
            let mut entries: Vec<DirectoryEntry> = Vec::new();
            
            // Add files from contents
            for obj in &result.contents {
                // Calculate relative path from prefix
                let name = obj.key.strip_prefix(prefix).unwrap_or(&obj.key);
                
                // Apply depth filter if needed
                if let Some(depth) = max_depth {
                    let path_depth = name.matches('/').count() + 1;
                    if path_depth > depth {
                        continue;
                    }
                }
                
                entries.push(DirectoryEntry {
                    name: name.to_string(),
                    entry_type: "file".to_string(),
                    size: Some(obj.size),
                    modified: Some(obj.last_modified.clone()),
                });
            }
            
            // Add directories from common_prefixes (only when depth=1)
            for prefix_path in &result.common_prefixes {
                let name = prefix_path.strip_prefix(prefix).unwrap_or(prefix_path);
                entries.push(DirectoryEntry {
                    name: name.to_string(),
                    entry_type: "dir".to_string(),
                    size: None,
                    modified: None,
                });
            }
            
            let response = DirectoryListResponse {
                path: format!("{}/{}/{}", app, bucket, prefix),
                entries,
                truncated: result.is_truncated,
            };
            
            debug!(bank = %bank_id, prefix = %prefix, depth = ?max_depth, count = response.entries.len(), "Directory listing");
            
            match serde_json::to_string(&ApiResponse::new(response)) {
                Ok(json) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(json.into())
                    .unwrap(),
                Err(e) => error_response_raw(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
            }
        }
        Err(e) => error_response_raw(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}


// ============================================================================
// PUT /api/v1/stone/storage/bank/:id/*path - Put Object
// ============================================================================

/// Create or update an object in a bank
pub async fn put_object_v1(
    State(_state): State<AppState>,
    Path((id, path)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<ObjectMeta>>, (StatusCode, Json<ApiErrorResponse>)> {
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    let bank = registry.get(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &format!("Bank '{}' not found", id)))?;
    
    let store = ObjectStore::new(&bank.mount_path);
    
    // Path format: app/bucket/key or just key (defaults to zen-garden/default)
    let (app, bucket, key) = parse_object_path(&path);
    
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");
    
    let result = store.put_object(&app, &bucket, &key, content_type, &body).await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "PUT_FAILED", &e.to_string()))?;
    
    debug!(bank = %id, key = %key, size = body.len(), "PUT object success");
    
    Ok(Json(ApiResponse::new(ObjectMeta {
        key,
        size: body.len() as u64,
        content_type: content_type.to_string(),
        etag: result.etag,
        last_modified: chrono::Utc::now().to_rfc3339(),
    })))
}

// ============================================================================
// DELETE /api/v1/stone/storage/bank/:id/*path - Delete Object
// ============================================================================

/// Delete an object from a bank
pub async fn delete_object_v1(
    State(_state): State<AppState>,
    Path((id, path)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ApiErrorResponse>)> {
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    let bank = registry.get(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &format!("Bank '{}' not found", id)))?;
    
    let store = ObjectStore::new(&bank.mount_path);
    
    let (app, bucket, key) = parse_object_path(&path);
    
    store.delete_object(&app, &bucket, &key).await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "DELETE_FAILED", &e.to_string()))?;
    
    debug!(bank = %id, key = %key, "DELETE object success");
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// HEAD /api/v1/stone/storage/bank/:id/*path - Head Object
// ============================================================================

/// Get object metadata (headers only)
pub async fn head_object_v1(
    State(_state): State<AppState>,
    Path((id, path)): Path<(String, String)>,
) -> Response {
    let registry = match SeedBankRegistry::scan().await {
        Ok(r) => r,
        Err(e) => return error_response_raw(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    
    let bank = match registry.get(&id) {
        Some(b) => b,
        None => return error_response_raw(StatusCode::NOT_FOUND, &format!("Bank '{}' not found", id)),
    };
    
    let store = ObjectStore::new(&bank.mount_path);
    
    let (app, bucket, key) = parse_object_path(&path);
    
    match store.head_object(&app, &bucket, &key).await {
        Ok(Some(meta)) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, &meta.content_type)
                .header(header::CONTENT_LENGTH, meta.size)
                .header(header::ETAG, &meta.etag)
                .header("Last-Modified", &meta.last_modified)
                .body("".into())
                .unwrap()
        }
        Ok(None) => error_response_raw(StatusCode::NOT_FOUND, "Object not found"),
        Err(e) => error_response_raw(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// ============================================================================
// Helper: Parse object path
// ============================================================================

/// Parse object path into (app, bucket, key)
/// Supports formats:
/// - `key` → ("zen-garden", "default", "key")
/// - `bucket/key` → ("zen-garden", "bucket", "key")
/// - `app/bucket/key` → ("app", "bucket", "key")
fn parse_object_path(path: &str) -> (String, String, String) {
    let path = path.trim_start_matches('/');
    let parts: Vec<&str> = path.splitn(3, '/').collect();
    
    match parts.len() {
        1 => ("zen-garden".to_string(), "default".to_string(), parts[0].to_string()),
        2 => ("zen-garden".to_string(), parts[0].to_string(), parts[1].to_string()),
        _ => (parts[0].to_string(), parts[1].to_string(), parts[2..].join("/")),
    }
}

/// Build error response for raw handlers
fn error_response_raw(status: StatusCode, message: &str) -> Response {
    let body = serde_json::json!({
        "error": {
            "code": status.as_u16(),
            "message": message
        }
    });
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.to_string().into())
        .unwrap()
}

// ============================================================================
// GET /api/v1/stone/storage/candidates
// ============================================================================

/// List eligible devices awaiting preparation
pub async fn list_candidates_v1(
    State(_state): State<AppState>,
) -> Result<(StatusCode, Json<Vec<StorageDetectedInfo>>), (StatusCode, Json<ApiErrorResponse>)> {
    #[cfg(target_os = "linux")]
    let candidates = scan_candidates().await.unwrap_or_default();
    #[cfg(not(target_os = "linux"))]
    let candidates = Vec::new();
    
    Ok((StatusCode::OK, Json(candidates)))
}

#[cfg(target_os = "linux")]
async fn scan_candidates() -> anyhow::Result<Vec<StorageDetectedInfo>> {
    use crate::infra::storage::list_usb_partitions;
    tokio::task::spawn_blocking(|| list_usb_partitions()).await?
}

// ============================================================================
// POST /api/v1/stone/storage/prepare
// ============================================================================

/// Prepare a device as a seed bank (async job)
pub async fn prepare_seed_bank_v1(
    State(state): State<AppState>,
    Json(request): Json<PrepareSeedBankRequest>,
) -> Result<(StatusCode, Json<PrepareAcceptedResponse>), (StatusCode, Json<ApiErrorResponse>)> {
    // Validate device exists and is eligible
    let device_info = analyze_device(&request.device)
        .map_err(|e| err(StatusCode::BAD_REQUEST, "DEVICE_ANALYSIS_FAILED", &e.to_string()))?;
    
    if !device_info.eligible {
        let reason = device_info.ineligible_reason.unwrap_or_else(|| "Unknown reason".to_string());
        let error_code = match device_info.state {
            DeviceState::HasData => "DEVICE_HAS_DATA",
            DeviceState::Prepared => "ALREADY_PREPARED",
            DeviceState::Unpartitioned => "DEVICE_UNPARTITIONED",
            _ => "DEVICE_NOT_ELIGIBLE",
        };
        return Err(err(StatusCode::BAD_REQUEST, error_code, &reason));
    }
    
    // Determine seed bank name
    // - random_name: true → generate seed-{adj}-{noun}
    // - name provided → use it
    // - neither → default to "seed-bank-zen-garden" (unnamed pool)
    let name = if request.random_name {
        generate_seed_bank_name()
    } else if let Some(ref n) = request.name {
        n.clone()
    } else {
        "seed-bank-zen-garden".to_string()
    };
    
    // Check for name collision (live scan)
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    if registry.exists(&name) {
        return Err(err(StatusCode::CONFLICT, "NAME_COLLISION", &format!("Seed bank '{}' already exists", name)));
    }
    
    let job_id = garden_common::utils::ids::generate_guidv7();
    info!(device = %request.device, name = %name, job_id = %job_id, "Accepted seed bank preparation request");
    
    // Spawn async job for preparation
    let job_id_clone = job_id.clone();
    let name_clone = name.clone();
    let device = request.device.clone();
    let filesystem = request.filesystem.clone();
    let stone_name = state.stone_name.clone();
    let stone_id = state.stone_id.clone();
    let api_port = state.api_port;
    let event_tx = state.event_tx.clone();
    
    tokio::spawn(async move {
        match run_prepare_job(&job_id_clone, &device, &name_clone, &filesystem, &stone_name, event_tx.clone()).await {
            Ok(()) => {
                // STORAGE-0003: Broadcast storage beacon on successful preparation
                let endpoint = format!("http://{}:{}", stone_name, api_port);
                if let Err(e) = crate::infra::storage::broadcast_beacon(&stone_id, &stone_name, &endpoint).await {
                    warn!(error = %e, "Failed to broadcast storage beacon after preparation");
                }
            }
            Err(e) => {
                tracing::error!(
                    job_id = %job_id_clone,
                    device = %device,
                    name = %name_clone,
                    error = %e,
                    error_chain = ?e,
                    "Seed bank preparation FAILED"
                );
                let failure_event = crate::app_state::MossEvent {
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    level: "error".to_string(),
                    message: format!("[STORAGE] FAILED: {} - {}", name_clone, e),
                    job_id: Some(job_id_clone.clone()),
                };
                let _ = event_tx.send(failure_event);
            }
        }
    });
    
    Ok((StatusCode::ACCEPTED, Json(PrepareAcceptedResponse {
        accepted: true,
        job_id,
        message: format!("Preparing seed bank '{}' in background", name),
    })))
}

/// Run the actual preparation job
async fn run_prepare_job(
    job_id: &str,
    device: &str,
    name: &str,
    filesystem: &str,
    stone_name: &str,
    event_tx: tokio::sync::broadcast::Sender<crate::app_state::MossEvent>,
) -> anyhow::Result<()> {
    use anyhow::Context;
    use chrono::Utc;
    use garden_common::storage::{SeedBankManifest, SeedBankVisibility};
    
    info!(job_id, device, name, "Starting seed bank preparation");
    emit_progress(&event_tx, job_id, "analyzing", "Analyzing device...");
    
    // Determine actual filesystem
    let actual_fs = if filesystem == "btrfs" && check_btrfs_support().await {
        "btrfs"
    } else {
        if filesystem == "btrfs" {
            warn!("btrfs not supported, falling back to ext4");
        }
        "ext4"
    };
    
    // Create mount point
    let data_dir = garden_common::constants::paths::data_dir();
    let mount_dir = PathBuf::from(&data_dir).join("mounts").join(name);
    
    #[cfg(target_os = "linux")]
    {
        let output = tokio::process::Command::new("sudo")
            .args(["mkdir", "-p", &mount_dir.to_string_lossy()])
            .output().await.context("Failed to run sudo mkdir")?;
        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to create mount directory: {}", String::from_utf8_lossy(&output.stderr)));
        }
    }
    #[cfg(not(target_os = "linux"))]
    tokio::fs::create_dir_all(&mount_dir).await.context("Failed to create mount directory")?;
    
    emit_progress(&event_tx, job_id, "formatting", &format!("Formatting as {}...", actual_fs));
    
    #[cfg(target_os = "linux")]
    format_device(device, actual_fs).await.context("Failed to format device")?;
    
    emit_progress(&event_tx, job_id, "mounting", "Mounting filesystem...");
    
    #[cfg(target_os = "linux")]
    mount_device(device, &mount_dir).await.context("Failed to mount device")?;
    
    #[cfg(target_os = "linux")]
    {
        let output = tokio::process::Command::new("sudo")
            .args(["chown", "-R", "stone:stone", &mount_dir.to_string_lossy()])
            .output().await.context("Failed to run chown")?;
        if !output.status.success() {
            warn!("Failed to chown mount directory: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
    
    emit_progress(&event_tx, job_id, "creating", "Creating seed bank structure...");
    
    // Create .zen-garden structure on the device
    let zen_dir = mount_dir.join(".zen-garden");
    tokio::fs::create_dir_all(&zen_dir).await.context("Failed to create .zen-garden directory")?;
    
    let manifest = SeedBankManifest::new(name, stone_name, actual_fs, SeedBankVisibility::Open);
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    
    // Atomic manifest write: temp file → fsync → rename
    let tmp_manifest = zen_dir.join(".manifest.json.tmp");
    let final_manifest = zen_dir.join("manifest.json");
    tokio::fs::write(&tmp_manifest, &manifest_json).await.context("Failed to write temp manifest")?;
    
    {
        let file = std::fs::File::open(&tmp_manifest).context("Failed to open temp manifest for sync")?;
        file.sync_all().context("Failed to sync temp manifest")?;
    }
    
    tokio::fs::rename(&tmp_manifest, &final_manifest).await.context("Failed to rename manifest")?;
    
    tokio::fs::create_dir_all(zen_dir.join("journal")).await.context("Failed to create journal directory")?;
    tokio::fs::create_dir_all(zen_dir.join("blobs")).await.context("Failed to create blobs directory")?;
    
    // Sync filesystem to ensure all data is on device
    #[cfg(target_os = "linux")]
    let _ = tokio::process::Command::new("sync").output().await;
    
    // Emit completion
    let event = crate::app_state::MossEvent {
        timestamp: Utc::now().to_rfc3339(),
        level: "info".to_string(),
        message: format!("[STORAGE] Prepared: {} at {}", name, mount_dir.display()),
        job_id: Some(job_id.to_string()),
    };
    let _ = event_tx.send(event);
    
    // Emit safe-to-remove event
    let safe_event = crate::app_state::MossEvent {
        timestamp: Utc::now().to_rfc3339(),
        level: "info".to_string(),
        message: format!("[STORAGE] Safe to remove: {} - all data synced to device", name),
        job_id: Some(job_id.to_string()),
    };
    let _ = event_tx.send(safe_event);
    
    if let Err(e) = garden_common::console::print_storage_prepared_ribbon(name, &mount_dir.to_string_lossy()) {
        warn!("Failed to print prepared ribbon: {}", e);
    }
    
    info!(name, "Seed bank preparation completed");
    Ok(())
}

fn emit_progress(tx: &tokio::sync::broadcast::Sender<crate::app_state::MossEvent>, job_id: &str, phase: &str, message: &str) {
    let event = crate::app_state::MossEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: "info".to_string(),
        message: format!("[STORAGE] {}: {}", phase, message),
        job_id: Some(job_id.to_string()),
    };
    let _ = tx.send(event);
}

async fn check_btrfs_support() -> bool {
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = tokio::process::Command::new("which").arg("mkfs.btrfs").output().await {
            return output.status.success();
        }
    }
    false
}

#[cfg(target_os = "linux")]
async fn format_device(device: &str, filesystem: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    let (cmd, args): (&str, Vec<&str>) = match filesystem {
        "btrfs" => ("mkfs.btrfs", vec!["-f", "-L", "zen-seed", device]),
        "ext4" => ("mkfs.ext4", vec!["-F", "-L", "zen-seed", device]),
        _ => return Err(anyhow::anyhow!("Unsupported filesystem: {}", filesystem)),
    };
    
    let output = tokio::process::Command::new("sudo")
        .args([cmd]).args(&args)
        .output().await.context(format!("Failed to run sudo {}", cmd))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!("Format failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    
    // Sync to flush filesystem to device
    let _ = tokio::process::Command::new("sync").output().await;
    Ok(())
}

#[cfg(target_os = "linux")]
async fn mount_device(device: &str, mount_point: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context;
    let output = tokio::process::Command::new("sudo")
        .args(["mount", device, &mount_point.to_string_lossy()])
        .output().await.context("Failed to run sudo mount")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!("Mount failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(())
}

fn generate_seed_bank_name() -> String {
    use rand::seq::SliceRandom;
    const ADJECTIVES: &[&str] = &["kind", "wise", "calm", "bold", "swift", "quiet", "bright", "deep", "warm", "cool", "fresh", "clear", "soft", "strong", "gentle"];
    const NOUNS: &[&str] = &["meadow", "valley", "river", "forest", "garden", "grove", "brook", "stone", "path", "spring", "hill", "field", "shore", "cliff", "peak"];
    let mut rng = rand::thread_rng();
    format!("seed-{}-{}", ADJECTIVES.choose(&mut rng).unwrap(), NOUNS.choose(&mut rng).unwrap())
}

// ============================================================================
// PATCH /api/v1/stone/storage/:name/visibility
// ============================================================================

/// Change seed bank visibility (updates manifest on device)
pub async fn set_visibility_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<SetVisibilityRequest>,
) -> Result<(StatusCode, Json<SeedBankInfo>), (StatusCode, Json<ApiErrorResponse>)> {
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    let bank = registry.get(&name)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "SEED_BANK_NOT_FOUND", &format!("Seed bank '{}' not found", name)))?;
    
    update_manifest_visibility(&bank.mount_path, request.visibility).await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "MANIFEST_UPDATE_FAILED", &e.to_string()))?;
    
    // Re-scan to get updated info
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    let updated = registry.get(&name)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "SEED_BANK_NOT_FOUND", "Seed bank disappeared after update"))?;
    
    info!(name = %name, visibility = ?request.visibility, "Seed bank visibility updated");
    
    // STORAGE-0003: Update local storage cache AND broadcast beacon
    let storage_cache = state.storage_cache.clone();
    let stone_id = state.stone_id.clone();
    let stone_name = state.stone_name.clone();
    let endpoint = state.self_entry.read().await.endpoint.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::infra::storage::update_and_broadcast(&storage_cache, &stone_id, &stone_name, &endpoint).await {
            warn!(error = %e, "Failed to update storage cache and broadcast beacon after visibility change");
        }
    });
    
    Ok((StatusCode::OK, Json(updated.clone())))
}

async fn update_manifest_visibility(mount_path: &str, visibility: garden_common::storage::SeedBankVisibility) -> anyhow::Result<()> {
    use anyhow::Context;
    let manifest_path = std::path::Path::new(mount_path).join(".zen-garden/manifest.json");
    let content = tokio::fs::read_to_string(&manifest_path).await.context("Failed to read manifest")?;
    let mut manifest: garden_common::storage::SeedBankManifest = serde_json::from_str(&content).context("Failed to parse manifest")?;
    manifest.visibility = visibility;
    tokio::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?).await.context("Failed to write manifest")?;
    Ok(())
}

async fn update_manifest_name(mount_path: &str, new_name: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    let manifest_path = std::path::Path::new(mount_path).join(".zen-garden/manifest.json");
    let content = tokio::fs::read_to_string(&manifest_path).await.context("Failed to read manifest")?;
    let mut manifest: garden_common::storage::SeedBankManifest = serde_json::from_str(&content).context("Failed to parse manifest")?;
    manifest.name = new_name.to_string();
    tokio::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?).await.context("Failed to write manifest")?;
    Ok(())
}

#[cfg(target_os = "linux")]
async fn unmount_device(mount_path: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    let _ = tokio::process::Command::new("sync").output().await;
    let output = tokio::process::Command::new("sudo")
        .args(["umount", mount_path])
        .output().await.context("Failed to run umount")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!("Unmount failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(())
}

// ============================================================================
// POST /api/v1/stone/storage/release-all
// ============================================================================

/// Safely unmount all seed banks
pub async fn release_all_seed_banks_v1(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<Vec<ReleaseResponse>>), (StatusCode, Json<ApiErrorResponse>)> {
    let registry = SeedBankRegistry::scan().await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "SCAN_FAILED", &e.to_string()))?;
    
    let mut results = Vec::new();
    
    for bank in registry.list() {
        #[cfg(target_os = "linux")]
        {
            match unmount_device(&bank.mount_path).await {
                Ok(_) => {
                    results.push(ReleaseResponse {
                        released: true,
                        name: bank.name.clone(),
                        message: "Seed bank safely released.".to_string(),
                    });
                }
                Err(e) => {
                    warn!(name = %bank.name, error = %e, "Failed to unmount seed bank");
                    results.push(ReleaseResponse {
                        released: false,
                        name: bank.name.clone(),
                        message: format!("Failed to unmount: {}", e),
                    });
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        results.push(ReleaseResponse {
            released: true,
            name: bank.name.clone(),
            message: "Seed bank released (non-Linux).".to_string(),
        });
    }
    
    let event = crate::app_state::MossEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: "info".to_string(),
        message: format!("[STORAGE] Released {} seed banks", results.len()),
        job_id: None,
    };
    let _ = state.event_tx.send(event);
    
    info!(count = results.len(), "All seed banks released");
    Ok((StatusCode::OK, Json(results)))
}

