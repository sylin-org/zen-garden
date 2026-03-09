//! Storage API endpoints for managed storage (stone-local)
//!
//! Design: The Volumes domain (`domain::storage::Volumes`) is the single source
//! of truth. Populated at boot by `initial_scan()`, updated in real-time by the
//! cross-platform volume watcher (STORAGE-0011).
//!
//! ## API Structure (STORAGE-0010)
//!
//! Stone-tier routes — always operate on this stone's local replicas.
//!
//! ```text
//! /api/v1/stone/storage                         GET   → Overview (bank types, counts)
//! /api/v1/stone/storage/health                  GET   → Readiness check
//! /api/v1/stone/storage/add                     POST  → Add storage (unified)
//! /api/v1/stone/storage/banks                   GET   → List all storages
//! /api/v1/stone/storage/banks/:name             GET   → Storage details
//! /api/v1/stone/storage/banks/:name             DELETE → Remove storage
//! /api/v1/stone/storage/banks/:name/release     POST  → Unmount storage
//! /api/v1/stone/storage/banks/:name/rename      PATCH → Rename storage
//! /api/v1/stone/storage/banks/:name/visibility  PATCH → Change visibility
//! /api/v1/stone/storage/banks/:name/pin         POST  → Pin Primary role
//! /api/v1/stone/storage/banks/:name/unpin       POST  → Unpin Primary role
//! /api/v1/stone/storage/banks/:name/roles       PATCH → Set composable roles
//! /api/v1/stone/storage/banks/:name/changes     GET   → Replication changelog
//! /api/v1/stone/storage/candidates              GET   → Eligible devices
//! /api/v1/stone/storage/release-all             POST  → Unmount all
//! /api/v1/stone/storage/stream                  GET   → SSE storage ticks
//! ```
//!
//! See docs/decisions/STORAGE-0010-unified-storage-add-command.md

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use garden_common::api_utils::{ApiErrorResponse, ApiResponse};
use garden_common::constants::paths;
use garden_common::storage::{
    AddStorageRequest, AddStorageResponse, CandidatesResponse, DeviceState, MediumAction,
    MediumInfo, MediumPartitionInfo, RenameStorageRequest, SetRolesRequest, SetVisibilityRequest,
    StorageDetectedInfo, StorageInfo, StorageManifest, StorageVisibility,
    DEFAULT_PRIVATE_STORAGE_NAME,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::infra::storage::{analyze_device, layout, ContentStore};
use crate::infra::{DomainPulse, PulseEvent};
use crate::{error_response, AppState};
use garden_common::presence::event_types;

// ============================================================================
// Prepare-job guard
// ============================================================================

/// Tracks devices currently undergoing preparation.
///
/// Prevents concurrent `prepare_seed_bank_v1` calls from racing on the same
/// physical device (e.g., double-click, retry while still formatting).
static PREPARING_DEVICES: std::sync::OnceLock<
    tokio::sync::Mutex<std::collections::HashSet<String>>,
> = std::sync::OnceLock::new();

fn preparing_devices() -> &'static tokio::sync::Mutex<std::collections::HashSet<String>> {
    PREPARING_DEVICES.get_or_init(|| tokio::sync::Mutex::new(std::collections::HashSet::new()))
}

/// RAII guard that removes a device from `PREPARING_DEVICES` on drop.
struct PrepareGuard {
    device: String,
}

impl Drop for PrepareGuard {
    fn drop(&mut self) {
        preparing_devices().blocking_lock().remove(&self.device);
    }
}

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
    /// All seed banks across the garden (from registry)
    pub garden_banks: Vec<GardenBankInfo>,
}

/// Info about a remote seed bank in the garden
#[derive(Debug, Serialize, Deserialize)]
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
    /// Runtime role (STORAGE-0006)
    #[serde(default)]
    pub role: garden_common::storage::StorageRole,
    /// Whether the Primary role is pinned (STORAGE-0006 Phase 5)
    #[serde(default)]
    pub pinned: bool,
    /// Whether content is encrypted (STORAGE-0006)
    #[serde(default)]
    pub encrypted: bool,
    /// Composable roles (e.g., ["seed-bank"])
    #[serde(default)]
    pub roles: Vec<String>,
}

/// Info about a storage type
#[derive(Debug, Serialize)]
pub struct StorageTypeInfo {
    pub name: String,
    pub count: usize,
    pub endpoint: String,
}

/// Storage readiness overview for this stone
#[derive(Debug, Serialize)]
pub struct StorageHealth {
    pub ready: bool,
    pub bank_count: usize,
    pub ready_count: usize,
    pub banks: Vec<SeedBankHealth>,
    pub issues: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SeedBankHealth {
    pub id: String,
    pub name: String,
    pub device: String,
    pub mount_path: String,
    pub canonical: bool,
    pub writable: bool,
    pub ready: bool,
    pub issues: Vec<String>,
}


/// Response for release endpoint
#[derive(Debug, Serialize)]
pub struct ReleaseResponse {
    pub released: bool,
    pub name: String,
    pub message: String,
}

// ============================================================================
// Helper Functions
// ============================================================================

fn err(status: StatusCode, code: &str, msg: &str) -> (StatusCode, Json<ApiErrorResponse>) {
    error_response(status, code, msg, None)
}

/// Check whether a mount is read-only by reading `/proc/mounts`.
async fn is_mount_readonly(mount_path: &str) -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        let mounts = tokio::fs::read_to_string("/proc/mounts").await.ok()?;
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 && parts[1] == mount_path {
                let opts = parts[3];
                let ro = opts.split(',').any(|o| o == "ro");
                return Some(ro);
            }
        }
        None
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = mount_path;
        Some(false)
    }
}

/// Validate that a seed bank uses the canonical layout.
fn validate_seed_bank_layout(mount_path: &str) -> Result<(), String> {
    let memories = std::path::Path::new(mount_path).join(paths::STORAGE_MEMORIES_DIR);
    let storage = std::path::Path::new(mount_path).join(paths::STORAGE_OBJECTS_DIR);

    let mut missing = Vec::new();
    if !memories.is_dir() {
        missing.push(paths::STORAGE_MEMORIES_DIR);
    }
    if !storage.is_dir() {
        missing.push(paths::STORAGE_OBJECTS_DIR);
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Seed bank is non-canonical; missing {}. Re-prepare the seed bank.",
            missing.join(" and ")
        ))
    }
}

// ============================================================================
// GET /api/v1/stone/storage - Storage Overview
// ============================================================================

/// Get storage overview (types, counts)
///
/// Returns local bank stats plus garden-wide view from registry.
pub async fn storage_overview_v1(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<StorageOverview>>, (StatusCode, Json<ApiErrorResponse>)> {
    // Get local banks from the unified Volumes domain (STORAGE-0011)
    let local_banks: Vec<StorageInfo> = {
        let map = state.volumes.read().await;
        map.values()
            .filter_map(|v| v.to_storage_info())
            .filter(|b| validate_seed_bank_layout(&b.mount_path).is_ok())
            .collect()
    };
    let total_capacity: u64 = local_banks.iter().map(|b| b.capacity_bytes).sum();
    let total_used: u64 = local_banks.iter().map(|b| b.used_bytes).sum();

    // Get garden-wide view from unified registry
    let reg = state.registry.read().await;
    let local_roles = crate::domain::storage::roles_snapshot(&state.volumes).await;
    let local_pins = crate::domain::storage::pins_snapshot(&state.volumes).await;
    let mut garden_banks = Vec::new();

    for entry in reg.storage_entries() {
        let is_local = entry.tool.stone.id == state.stone_id;
        let sm = entry.tool.storage.as_ref();

        let role = if is_local {
            // Overlay authoritative runtime role for local banks
            let name = &entry.tool.fqid;
            local_roles
                .get(name.as_str())
                .copied()
                .unwrap_or(garden_common::storage::StorageRole::Primary)
        } else {
            match sm.and_then(|s| s.role.as_deref()) {
                Some("dormant") => garden_common::storage::StorageRole::Dormant,
                _ => garden_common::storage::StorageRole::Primary,
            }
        };
        let pinned = if is_local {
            local_pins.contains_key(entry.tool.fqid.as_str())
        } else {
            sm.and_then(|s| s.pin_id.as_ref()).is_some()
        };

        garden_banks.push(GardenBankInfo {
            id: entry.tool.tool.id.clone(),
            name: entry.tool.fqid.clone(),
            stone_id: entry.tool.stone.id.clone(),
            stone_name: entry.tool.stone.name.clone(),
            endpoint: entry.tool.stone.endpoint.clone(),
            is_local,
            visibility: sm.map(|s| s.visibility.clone()).unwrap_or_else(|| "open".to_string()),
            health: entry.tool.service.status.clone(),
            capacity_bytes: sm.map(|s| s.capacity_bytes).unwrap_or(0),
            used_bytes: sm.map(|s| s.used_bytes).unwrap_or(0),
            role,
            pinned,
            encrypted: sm.map(|s| s.encrypted).unwrap_or(false),
            roles: sm.map(|s| s.roles.clone()).unwrap_or_default(),
        });
    }

    let overview = StorageOverview {
        bank_count: local_banks.len(),
        total_capacity_bytes: total_capacity,
        total_used_bytes: total_used,
        types: vec![StorageTypeInfo {
            name: "bank".to_string(),
            count: local_banks.len(),
            endpoint: "/api/v1/stone/storage/banks".to_string(),
        }],
        garden_banks,
    };

    Ok(Json(ApiResponse::new(overview)))
}

// ============================================================================
// GET /api/v1/stone/storage/health - Storage Readiness
// ============================================================================

/// Get storage readiness for this stone (mounted + canonical + writable).
pub async fn storage_health_v1(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<StorageHealth>>, (StatusCode, Json<ApiErrorResponse>)> {
    let managed: Vec<StorageInfo> = {
        let map = state.volumes.read().await;
        map.values().filter_map(|v| v.to_storage_info()).collect()
    };

    let mut banks = Vec::new();

    for bank in &managed {
        let mut issues = Vec::new();

        let canonical = validate_seed_bank_layout(&bank.mount_path).is_ok();
        if !canonical {
            issues.push("non-canonical layout".to_string());
        }

        let writable = match is_mount_readonly(&bank.mount_path).await {
            Some(true) => {
                issues.push("mount is read-only".to_string());
                false
            }
            Some(false) => true,
            None => {
                issues.push("mount options unavailable".to_string());
                false
            }
        };

        let ready = canonical && writable;

        banks.push(SeedBankHealth {
            id: bank.id.clone(),
            name: bank.name.clone(),
            device: bank.device.clone(),
            mount_path: bank.mount_path.clone(),
            canonical,
            writable,
            ready,
            issues,
        });
    }

    let bank_count = banks.len();
    let ready_count = banks.iter().filter(|b| b.ready).count();
    let ready = ready_count > 0;

    let mut issues = Vec::new();
    if bank_count == 0 {
        issues.push("no seed banks mounted".to_string());
    } else if ready_count == 0 {
        issues.push("no seed banks are ready".to_string());
    }

    Ok(Json(ApiResponse::new(StorageHealth {
        ready,
        bank_count,
        ready_count,
        banks,
        issues,
    })))
}

// ============================================================================
// GET /api/v1/stone/storage/banks - List Banks
// ============================================================================

/// List all seed banks
pub async fn list_banks_v1(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<StorageInfo>>>, (StatusCode, Json<ApiErrorResponse>)> {
    let banks: Vec<StorageInfo> = {
        let map = state.volumes.read().await;
        map.values()
            .filter_map(|v| v.to_storage_info())
            .filter(|b| validate_seed_bank_layout(&b.mount_path).is_ok())
            .collect()
    };
    Ok(Json(ApiResponse::new(banks)))
}

// ============================================================================
// GET /api/v1/stone/storage/banks/:name - Get Bank Details
// ============================================================================

/// Get seed bank details by name
pub async fn get_bank_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<StorageInfo>>, (StatusCode, Json<ApiErrorResponse>)> {
    let bank = {
        let map = state.volumes.read().await;
        map.values()
            .find(|v| v.management.as_ref().is_some_and(|m| m.name == name))
            .and_then(|v| v.to_storage_info())
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &format!("Bank '{}' not found", name)))?
    };

    if let Err(msg) = validate_seed_bank_layout(&bank.mount_path) {
        return Err(err(StatusCode::CONFLICT, "BANK_NONCANONICAL", &msg));
    }

    Ok(Json(ApiResponse::new(bank)))
}

// ============================================================================
// DELETE /api/v1/stone/storage/banks/:name - Delete Bank
// ============================================================================

/// Remove seed bank mount directory (device must be unmounted first)
pub async fn delete_bank_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiErrorResponse>)> {
    // Check if still mounted (managed volume present = still mounted)
    {
        let map = state.volumes.read().await;
        if map.values().any(|v| v.management.as_ref().is_some_and(|m| m.name == name) && v.online) {
            return Err(err(
                StatusCode::CONFLICT,
                "BANK_MOUNTED",
                "Bank must be released before deletion",
            ));
        }
    }

    // Remove mount directory if it exists
    let data_dir = garden_common::constants::paths::data_dir();
    let mount_dir = PathBuf::from(&data_dir).join("mounts").join(&name);

    if mount_dir.exists() {
        #[cfg(target_os = "linux")]
        {
            let output = tokio::process::Command::new("sudo")
                .args(["rm", "-rf", &mount_dir.to_string_lossy()])
                .output()
                .await
                .map_err(|e| {
                    err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "DELETE_FAILED",
                        &e.to_string(),
                    )
                })?;
            if !output.status.success() {
                return Err(err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DELETE_FAILED",
                    &String::from_utf8_lossy(&output.stderr),
                ));
            }
        }
        #[cfg(not(target_os = "linux"))]
        tokio::fs::remove_dir_all(&mount_dir).await.map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "DELETE_FAILED",
                &e.to_string(),
            )
        })?;
    }

    let pulse = DomainPulse::storage_event(
        event_types::STORAGE_REMOVED,
        format!("Seed bank '{}' removed", name),
        "info",
        None,
        Some(serde_json::json!({ "name": name })),
    );
    let _ = state.pulse_tx.send(PulseEvent::Domain(pulse));

    info!(name = %name, "Bank mount directory removed");
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// POST /api/v1/stone/storage/banks/:name/release - Release Bank
// ============================================================================

/// Safely unmount a seed bank
pub async fn release_bank_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<ReleaseResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let _mount_path = {
        let map = state.volumes.read().await;
        let vol = map.values()
            .find(|v| v.management.as_ref().is_some_and(|m| m.name == name))
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &format!("Bank '{}' not found", name)))?;
        vol.mount_path.to_string_lossy().to_string()
    };

    // STORAGE-0006: Remove from MountTracker BEFORE unmount to prevent
    // persistence task re-mounting the device we're releasing.
    #[cfg(target_os = "linux")]
    {
        let mut tracker = state.mount_tracker.write().await;
        if tracker.remove(&_mount_path).is_some() {
            debug!(mount_path = %_mount_path, "Removed from mount tracker before release");
        }
    }

    #[cfg(target_os = "linux")]
    unmount_device(&_mount_path).await.map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "UNMOUNT_FAILED",
            &e.to_string(),
        )
    })?;

    let pulse = DomainPulse::storage_event(
        event_types::STORAGE_RELEASED,
        format!("Seed bank '{}' released", name),
        "info",
        None,
        Some(serde_json::json!({ "name": name })),
    );
    let _ = state.pulse_tx.send(PulseEvent::Domain(pulse));

    if let Err(e) = garden_common::console::print_storage_released_ribbon(&name) {
        warn!("Failed to print released ribbon: {}", e);
    }

    // STORAGE-0011: Remove management from the released volume
    {
        let mut map = state.volumes.write().await;
        if let Some(vol) = map.values_mut().find(|v| {
            v.management.as_ref().is_some_and(|m| m.name == name)
        }) {
            vol.management = None;
            debug!(name = %name, "Cleared management from released volume");
        }
    }

    // STORAGE-0003: Update local storage cache AND broadcast beacon
    // TOOLS-0003: Refresh registry + broadcast storage beacon
    let tools_state = state.clone();
    tokio::spawn(async move {
        tools_state.refresh_local_tools_projection().await;
        let roles = crate::domain::storage::roles_snapshot(&tools_state.volumes).await;
        let pins = crate::domain::storage::pins_snapshot(&tools_state.volumes).await;
        if let Err(e) = crate::infra::storage::broadcast_beacon(
            &tools_state.stone_id,
            &tools_state.stone_name,
            &tools_state.self_entry.read().await.address.http_base(),
            &tools_state.volumes,
            Some(&roles),
            Some(&pins),
        )
        .await
        {
            warn!(error = %e, "Failed to broadcast storage beacon");
        }
    });

    info!(name = %name, "Bank released");
    Ok(Json(ApiResponse::new(ReleaseResponse {
        released: true,
        name,
        message: "Bank safely released. You may now remove the device.".to_string(),
    })))
}

// ============================================================================
// PATCH /api/v1/stone/storage/banks/:name/rename - Rename Bank
// ============================================================================

/// Rename a seed bank
pub async fn rename_bank_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<RenameStorageRequest>,
) -> Result<Json<ApiResponse<StorageInfo>>, (StatusCode, Json<ApiErrorResponse>)> {
    // Validate new name
    if request.new_name.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "INVALID_NAME",
            "New name cannot be empty",
        ));
    }

    let mount_path = {
        let map = state.volumes.read().await;
        let vol = map.values()
            .find(|v| v.management.as_ref().is_some_and(|m| m.name == name))
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &format!("Bank '{}' not found", name)))?;
        vol.mount_path.to_string_lossy().to_string()
    };

    // Renaming into an existing name is allowed — joins a replica group (STORAGE-0006)

    // Update manifest on device
    update_manifest_name(&mount_path, &request.new_name)
        .await
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "RENAME_FAILED",
                &e.to_string(),
            )
        })?;

    // Sync name to volume management
    {
        let mut map = state.volumes.write().await;
        if let Some(vol) = map.values_mut().find(|v| {
            v.management.as_ref().is_some_and(|m| m.name == name)
        }) {
            if let Some(ref mut mgmt) = vol.management {
                mgmt.name = request.new_name.clone();
            }
        }
    }

    // Re-read updated info from volumes
    let updated = {
        let map = state.volumes.read().await;
        map.values()
            .find(|v| v.management.as_ref().is_some_and(|m| m.name == request.new_name))
            .and_then(|v| v.to_storage_info())
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", "Bank not found after rename"))?
    };

    info!(old_name = %name, new_name = %request.new_name, "Bank renamed");

    let tools_state = state.clone();
    let nudge = state.orchestration_nudge.clone();
    tokio::spawn(async move {
        tools_state.refresh_local_tools_projection().await;
        let roles = crate::domain::storage::roles_snapshot(&tools_state.volumes).await;
        let pins = crate::domain::storage::pins_snapshot(&tools_state.volumes).await;
        if let Err(e) = crate::infra::storage::broadcast_beacon(
            &tools_state.stone_id,
            &tools_state.stone_name,
            &tools_state.self_entry.read().await.address.http_base(),
            &tools_state.volumes,
            Some(&roles),
            Some(&pins),
        )
        .await
        {
            warn!(error = %e, "Failed to broadcast storage beacon after rename");
        }
        // Nudge orchestration so role resolution happens immediately
        nudge.notify_one();
    });

    Ok(Json(ApiResponse::new(updated.clone())))
}

// ============================================================================
// GET /api/v1/stone/storage/candidates
// ============================================================================

/// List eligible devices awaiting preparation.
///
/// Returns unmanaged, removable, online volumes from the Volumes domain.
pub async fn list_candidates_v1(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<CandidatesResponse>), (StatusCode, Json<ApiErrorResponse>)> {
    // Space candidates: mounted volumes that are unmanaged, removable, and online.
    let volumes_map = state.volumes.read().await;
    let spaces: Vec<StorageDetectedInfo> = volumes_map
        .values()
        .filter(|v| !v.is_managed() && v.removable && v.online)
        .map(|v| StorageDetectedInfo {
            device: v.path.clone(),
            mount_path: Some(v.mount_path.to_string_lossy().to_string()),
            label: v.label.clone(),
            capacity_bytes: v.capacity_bytes,
            state: DeviceState::Empty,
            eligible: true,
            removable: v.removable,
            ineligible_reason: None,
        })
        .collect();

    // Medium candidates: physical disks (USB/external only).
    let media_map = state.media.read().await;
    let media: Vec<MediumInfo> = media_map
        .values()
        .filter(|m| m.removable)
        .map(|m| {
            let managed = m.has_managed_space(&volumes_map);
            let suggested_action = match m.condition {
                crate::infra::storage::platform::MediumCondition::Unreadable => {
                    MediumAction::Unreadable
                }
                crate::infra::storage::platform::MediumCondition::Raw => {
                    MediumAction::NeedsPartition
                }
                crate::infra::storage::platform::MediumCondition::Partitioned => {
                    if managed {
                        MediumAction::AlreadyManaged
                    } else if m.has_mounted_space() {
                        MediumAction::Ready
                    } else {
                        MediumAction::NeedsFormat
                    }
                }
            };

            MediumInfo {
                device_id: m.device_id.clone(),
                model: m.model.clone(),
                bus_type: garden_common::storage::BusType::from(m.bus_type),
                size_bytes: m.size_bytes,
                removable: m.removable,
                condition: garden_common::storage::MediumCondition::from(m.condition),
                partitions: m
                    .partitions
                    .iter()
                    .map(|p| MediumPartitionInfo {
                        index: p.index,
                        size_bytes: p.size_bytes,
                        filesystem: p.filesystem.clone(),
                        mount_path: p.mount_path.clone(),
                        label: p.label.clone(),
                    })
                    .collect(),
                managed,
                suggested_action,
            }
        })
        .collect();
    drop(volumes_map);
    drop(media_map);

    Ok((
        StatusCode::OK,
        Json(CandidatesResponse { spaces, media }),
    ))
}

// ============================================================================
// POST /api/v1/stone/storage/add — Unified storage add (STORAGE-0010)
// ============================================================================

/// Add storage — inspects target and does the right thing.
///
/// - Block device with no filesystem → formats, mounts, creates `.zen-garden/`
/// - Block device with filesystem, no files → mounts, creates `.zen-garden/`
/// - Block device or directory with existing files → creates `.zen-garden/`, catalogs content
/// - Path with existing `.zen-garden/` → 409 Conflict
pub async fn add_storage_v1(
    State(state): State<AppState>,
    Json(request): Json<AddStorageRequest>,
) -> Result<Json<ApiResponse<AddStorageResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let target = &request.target;
    let target_path = std::path::Path::new(target);

    // Determine if target is a block device or a directory
    let is_block_device = target.starts_with("/dev/");

    // ── Block device path ──────────────────────────────────────────────
    if is_block_device {
        // Check prepare-job guard
        {
            let preparing = preparing_devices().lock().await;
            if preparing.contains(target) {
                return Err(err(
                    StatusCode::CONFLICT,
                    "DEVICE_BUSY",
                    &format!("Device {} is already being added — wait for it to finish", target),
                ));
            }
        }

        // Analyze device state
        let device_info = analyze_device(target).map_err(|e| {
            err(StatusCode::BAD_REQUEST, "DEVICE_ANALYSIS_FAILED", &e.to_string())
        })?;

        // Validate: already managed → conflict
        if device_info.state == DeviceState::Prepared {
            return Err(err(
                StatusCode::CONFLICT,
                "ALREADY_MANAGED",
                "This device is already managed. Did you mean `storage status`?",
            ));
        }

        // Validate format flag consistency
        if request.format && device_info.state == DeviceState::HasData {
            return Err(err(
                StatusCode::CONFLICT,
                "DEVICE_HAS_DATA",
                "Cannot format a device that contains existing files",
            ));
        }

        let needs_format = request.format
            || device_info.state == DeviceState::Unformatted
            || device_info.state == DeviceState::Unpartitioned;

        if !needs_format && device_info.state == DeviceState::Unformatted {
            return Err(err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "NO_FILESYSTEM",
                "Device has no filesystem. Set format to true.",
            ));
        }

        // Determine name
        let name = request.name.clone().unwrap_or_else(|| {
            if request.encrypted {
                DEFAULT_PRIVATE_STORAGE_NAME.to_string()
            } else {
                generate_storage_name()
            }
        });

        // Same-name replicas are fine (STORAGE-0006)
        {
            let map = state.volumes.read().await;
            if map.values().any(|v| v.management.as_ref().is_some_and(|m| m.name == name)) {
                info!(name = %name, "Same-name storage exists — new device will be a replica");
            }
        }

        if needs_format {
            // ── Async format job (returns 200 with job_id) ──────────────
            let job_id = garden_common::utils::ids::generate_guidv7();
            info!(device = %target, name = %name, job_id = %job_id, "Accepted storage add (format) request");

            {
                let mut preparing = preparing_devices().lock().await;
                preparing.insert(target.to_string());
            }

            let job_id_clone = job_id.clone();
            let name_clone = name.clone();
            let device = target.to_string();
            let filesystem = request.filesystem.clone();
            let encrypted = request.encrypted;
            let roles = request.roles.clone();
            let stone_name = state.stone_name.clone();
            let api_port = state.api_port;
            let pulse_tx = state.pulse_tx.clone();
            let tools_state = state.clone();
            let guard_device = target.to_string();

            tokio::spawn(async move {
                let _guard = PrepareGuard { device: guard_device };

                match run_format_and_add(
                    &job_id_clone, &device, &name_clone, &filesystem,
                    encrypted, &roles, &stone_name, pulse_tx.clone(),
                ).await {
                    Ok(()) => {
                        tools_state.refresh_local_tools_projection().await;
                        let roles = crate::domain::storage::roles_snapshot(&tools_state.volumes).await;
                        let pins = crate::domain::storage::pins_snapshot(&tools_state.volumes).await;
                        if let Err(e) = crate::infra::storage::broadcast_beacon(
                            &tools_state.stone_id,
                            &tools_state.stone_name,
                            &format!("http://{}:{}", stone_name, api_port),
                            &tools_state.volumes,
                            Some(&roles),
                            Some(&pins),
                        ).await {
                            warn!(error = %e, "Failed to broadcast beacon after format");
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            job_id = %job_id_clone, device = %device, name = %name_clone,
                            error = %e, error_chain = ?e, "Storage add (format) FAILED"
                        );
                        let pulse = DomainPulse::storage_event(
                            event_types::STORAGE_ADD_PROGRESS,
                            format!("Add failed: {} - {}", name_clone, e),
                            "error",
                            Some(job_id_clone.clone()),
                            Some(serde_json::json!({ "name": name_clone, "error": e.to_string() })),
                        );
                        let _ = pulse_tx.send(PulseEvent::Domain(pulse));
                    }
                }
            });

            // For format jobs, we return a synthetic response with the job_id
            let response = AddStorageResponse {
                id: String::new(), // Not yet known — will be set after format
                name: name.clone(),
                mount_path: String::new(),
                formatted: true,
                cataloged: 0,
                job_id: Some(job_id),
            };
            return Ok(Json(ApiResponse::new(response)));
        }

        // ── Device has filesystem but no format needed ──────────────────
        // Mount the device, then fall through to the adopt path
        let visibility = if request.encrypted {
            StorageVisibility::Closed
        } else {
            StorageVisibility::Open
        };

        let manifest = StorageManifest::with_roles(
            &name, &state.stone_name, "unknown", visibility, request.roles.clone(),
        );

        let data_dir = garden_common::constants::paths::data_dir();
        let mount_dir = PathBuf::from(manifest.derive_mount_path(&data_dir));

        #[cfg(target_os = "linux")]
        {
            #[allow(unused_imports)]
            use anyhow::Context;
            let output = tokio::process::Command::new("sudo")
                .args(["mkdir", "-p", &mount_dir.to_string_lossy()])
                .output().await.map_err(|e| err(
                    StatusCode::INTERNAL_SERVER_ERROR, "MOUNT_FAILED",
                    &format!("Failed to create mount dir: {}", e),
                ))?;
            if !output.status.success() {
                return Err(err(StatusCode::INTERNAL_SERVER_ERROR, "MOUNT_FAILED",
                    &format!("mkdir failed: {}", String::from_utf8_lossy(&output.stderr))));
            }
            mount_device(target, &mount_dir).await.map_err(|e| {
                err(StatusCode::INTERNAL_SERVER_ERROR, "MOUNT_FAILED", &e.to_string())
            })?;
        }

        return add_at_path(state, &mount_dir, manifest, true).await;
    }

    // ── Directory path ─────────────────────────────────────────────────
    if !target_path.is_dir() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "PATH_NOT_FOUND",
            &format!("Path '{}' does not exist or is not a directory", target),
        ));
    }

    // Check if already managed
    let dotfolder = target_path.join(paths::STORAGE_DOTFOLDER);
    if dotfolder.join("manifest.json").exists() {
        return Err(err(
            StatusCode::CONFLICT,
            "ALREADY_MANAGED",
            "This path is already managed. Did you mean `storage status`?",
        ));
    }

    let name = request.name.clone().unwrap_or_else(generate_storage_name);

    let visibility = if request.encrypted {
        StorageVisibility::Closed
    } else {
        StorageVisibility::Open
    };

    let manifest = StorageManifest::with_roles(
        &name, &state.stone_name, "unknown", visibility, request.roles.clone(),
    );

    add_at_path(state, target_path, manifest, false).await
}

/// Shared logic: initialize layout, write manifest, catalog content, broadcast.
async fn add_at_path(
    state: AppState,
    mount_path: &std::path::Path,
    manifest: StorageManifest,
    formatted: bool,
) -> Result<Json<ApiResponse<AddStorageResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    // Migrate legacy layout if present
    layout::migrate_legacy_layout(mount_path).await.map_err(|e| {
        err(StatusCode::INTERNAL_SERVER_ERROR, "MIGRATION_FAILED",
            &format!("Failed to migrate legacy layout: {}", e))
    })?;

    // Initialize layout (creates all subdirs + symlink, idempotent)
    layout::initialize_layout(mount_path).await.map_err(|e| {
        err(StatusCode::INTERNAL_SERVER_ERROR, "LAYOUT_INIT_FAILED",
            &format!("Failed to initialize storage layout: {}", e))
    })?;

    // Write manifest atomically
    let manifest_path = mount_path.join(paths::STORAGE_DOTFOLDER).join("manifest.json");
    write_manifest_atomic(&manifest_path, &manifest).await.map_err(|e| {
        err(StatusCode::INTERNAL_SERVER_ERROR, "MANIFEST_WRITE_FAILED",
            &format!("Failed to write manifest: {}", e))
    })?;

    // Catalog existing content for replication baseline
    let cataloged = {
        let store = ContentStore::new_public(mount_path);
        match store.catalog_existing_content().await {
            Ok(count) => count,
            Err(e) => {
                warn!(error = %e, "Content catalog failed — storage added without baseline");
                0
            }
        }
    };

    let response = AddStorageResponse {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        mount_path: mount_path.to_string_lossy().to_string(),
        formatted,
        cataloged,
        job_id: None,
    };

    info!(
        name = %manifest.name, id = %manifest.id,
        path = %mount_path.display(), cataloged,
        "Storage added"
    );

    // Signal the volume watcher to re-scan. The existing pipeline will
    // re-classify this volume (finding the new manifest), update candidates
    // notification, emit pulses, and broadcast the beacon.
    state.request_volume_rescan();

    // Refresh tools + broadcast beacon
    state.refresh_local_tools_projection().await;
    {
        let roles = crate::domain::storage::roles_snapshot(&state.volumes).await;
        let pins = crate::domain::storage::pins_snapshot(&state.volumes).await;
        if let Err(e) = crate::infra::storage::broadcast_beacon(
            &state.stone_id, &state.stone_name,
            &format!("http://{}:{}", state.stone_name, state.api_port),
            &state.volumes,
            Some(&roles),
            Some(&pins),
        ).await {
            warn!(error = %e, "Failed to broadcast beacon after add");
        }
    }

    let storages = crate::domain::storage::name_id_pairs(&state.volumes).await;
    if let Err(e) = crate::infra::storage::refresh_signpost(
        &state.stone_name, state.api_port, &storages,
    ).await {
        warn!(error = %e, "Failed to refresh signpost after add");
    }

    Ok(Json(ApiResponse::new(response)))
}

/// Run format-and-add job in background (for block devices needing formatting).
#[allow(clippy::too_many_arguments)]
async fn run_format_and_add(
    job_id: &str,
    device: &str,
    name: &str,
    filesystem: &str,
    encrypted: bool,
    roles: &[String],
    stone_name: &str,
    pulse_tx: tokio::sync::broadcast::Sender<PulseEvent>,
) -> anyhow::Result<()> {
    use anyhow::Context;

    info!(job_id, device, name, encrypted, "Starting storage add (format)");
    emit_progress(&pulse_tx, job_id, name, "analyzing", "Analyzing device...");

    let actual_fs = if filesystem == "btrfs" && check_btrfs_support().await {
        "btrfs"
    } else {
        if filesystem == "btrfs" {
            warn!("btrfs not supported, falling back to ext4");
        }
        "ext4"
    };

    let visibility = if encrypted {
        StorageVisibility::Closed
    } else {
        StorageVisibility::Open
    };

    let manifest = StorageManifest::with_roles(
        name, stone_name, actual_fs, visibility, roles.to_vec(),
    );

    let data_dir = garden_common::constants::paths::data_dir();
    let mount_dir = PathBuf::from(manifest.derive_mount_path(&data_dir));

    #[cfg(target_os = "linux")]
    {
        let output = tokio::process::Command::new("sudo")
            .args(["mkdir", "-p", &mount_dir.to_string_lossy()])
            .output().await
            .context("Failed to run sudo mkdir")?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Failed to create mount directory: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    #[cfg(not(target_os = "linux"))]
    tokio::fs::create_dir_all(&mount_dir).await
        .context("Failed to create mount directory")?;

    emit_progress(&pulse_tx, job_id, name, "formatting", &format!("Formatting as {}...", actual_fs));

    #[cfg(target_os = "linux")]
    format_device(device, actual_fs).await
        .context("Failed to format device")?;

    emit_progress(&pulse_tx, job_id, name, "mounting", "Mounting filesystem...");

    #[cfg(target_os = "linux")]
    mount_device(device, &mount_dir).await
        .context("Failed to mount device")?;

    #[cfg(target_os = "linux")]
    {
        let output = tokio::process::Command::new("sudo")
            .args(["chown", "-R", "stone:stone", &mount_dir.to_string_lossy()])
            .output().await
            .context("Failed to run chown")?;
        if !output.status.success() {
            warn!("Failed to chown mount directory: {}", String::from_utf8_lossy(&output.stderr));
        }
    }

    emit_progress(&pulse_tx, job_id, name, "creating", "Creating storage structure...");

    // Initialize canonical layout
    layout::initialize_layout(&mount_dir).await
        .context("Failed to initialize storage layout")?;

    // Write manifest atomically
    let manifest_path = mount_dir.join(paths::STORAGE_DOTFOLDER).join("manifest.json");
    write_manifest_atomic(&manifest_path, &manifest).await
        .context("Failed to write manifest")?;

    // Sync filesystem
    #[cfg(target_os = "linux")]
    let _ = tokio::process::Command::new("sync").output().await;

    // Emit completion
    let pulse = DomainPulse::storage_event(
        event_types::STORAGE_CONNECTED,
        format!("Storage '{}' added at {}", name, mount_dir.display()),
        "info",
        Some(job_id.to_string()),
        Some(serde_json::json!({ "name": name, "mount_path": mount_dir.to_string_lossy() })),
    );
    let _ = pulse_tx.send(PulseEvent::Domain(pulse));

    if let Err(e) = garden_common::console::print_storage_connected_ribbon(
        name, &manifest.roles, 0,
    ) {
        warn!("Failed to print connected ribbon: {}", e);
    }

    info!(name, "Storage add completed");
    Ok(())
}

fn emit_progress(
    tx: &tokio::sync::broadcast::Sender<PulseEvent>,
    job_id: &str,
    name: &str,
    phase: &str,
    message: &str,
) {
    let pulse = DomainPulse::storage_event(
        event_types::STORAGE_ADD_PROGRESS,
        format!("{}: {}", phase, message),
        "info",
        Some(job_id.to_string()),
        Some(serde_json::json!({ "name": name, "phase": phase })),
    );
    let _ = tx.send(PulseEvent::Domain(pulse));
}

async fn check_btrfs_support() -> bool {
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = tokio::process::Command::new("which")
            .arg("mkfs.btrfs")
            .output()
            .await
        {
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
        .args([cmd])
        .args(&args)
        .output()
        .await
        .context(format!("Failed to run sudo {}", cmd))?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Format failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let _ = tokio::process::Command::new("sync").output().await;
    Ok(())
}

#[cfg(target_os = "linux")]
async fn mount_device(device: &str, mount_point: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context;
    let output = tokio::process::Command::new("sudo")
        .args(["mount", device, &mount_point.to_string_lossy()])
        .output()
        .await
        .context("Failed to run sudo mount")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Mount failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn generate_storage_name() -> String {
    use rand::seq::SliceRandom;
    const ADJECTIVES: &[&str] = &[
        "kind", "wise", "calm", "bold", "swift", "quiet", "bright", "deep", "warm", "cool",
        "fresh", "clear", "soft", "strong", "gentle",
    ];
    const NOUNS: &[&str] = &[
        "meadow", "valley", "river", "forest", "garden", "grove", "brook", "stone", "path",
        "spring", "hill", "field", "shore", "cliff", "peak",
    ];
    let mut rng = rand::thread_rng();
    format!(
        "seed-{}-{}",
        ADJECTIVES.choose(&mut rng).unwrap(),
        NOUNS.choose(&mut rng).unwrap()
    )
}

// ============================================================================
// PATCH /api/v1/stone/storage/:name/visibility
// ============================================================================

/// Change seed bank visibility (updates manifest on device)
pub async fn set_visibility_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<SetVisibilityRequest>,
) -> Result<(StatusCode, Json<StorageInfo>), (StatusCode, Json<ApiErrorResponse>)> {
    let mount_path = {
        let map = state.volumes.read().await;
        let vol = map.values()
            .find(|v| v.management.as_ref().is_some_and(|m| m.name == name))
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "SEED_BANK_NOT_FOUND", &format!("Seed bank '{}' not found", name)))?;
        vol.mount_path.to_string_lossy().to_string()
    };

    update_manifest_visibility(&mount_path, request.visibility)
        .await
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "MANIFEST_UPDATE_FAILED",
                &e.to_string(),
            )
        })?;

    // Sync visibility to volume management
    {
        let mut map = state.volumes.write().await;
        if let Some(vol) = map.values_mut().find(|v| {
            v.management.as_ref().is_some_and(|m| m.name == name)
        }) {
            if let Some(ref mut mgmt) = vol.management {
                mgmt.visibility = request.visibility;
            }
        }
    }

    // Re-read updated info
    let updated = {
        let map = state.volumes.read().await;
        map.values()
            .find(|v| v.management.as_ref().is_some_and(|m| m.name == name))
            .and_then(|v| v.to_storage_info())
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "SEED_BANK_NOT_FOUND", "Seed bank disappeared after update"))?
    };

    info!(name = %name, visibility = ?request.visibility, "Seed bank visibility updated");

    // STORAGE-0003: Refresh tools AND broadcast beacon
    let tools_state = state.clone();
    tokio::spawn(async move {
        tools_state.refresh_local_tools_projection().await;
        let roles = crate::domain::storage::roles_snapshot(&tools_state.volumes).await;
        let pins = crate::domain::storage::pins_snapshot(&tools_state.volumes).await;
        if let Err(e) = crate::infra::storage::broadcast_beacon(
            &tools_state.stone_id,
            &tools_state.stone_name,
            &tools_state.self_entry.read().await.address.http_base(),
            &tools_state.volumes,
            Some(&roles),
            Some(&pins),
        )
        .await
        {
            warn!(error = %e, "Failed to broadcast storage beacon after visibility change");
        }
    });

    Ok((StatusCode::OK, Json(updated.clone())))
}

async fn update_manifest_visibility(
    mount_path: &str,
    visibility: garden_common::storage::StorageVisibility,
) -> anyhow::Result<()> {
    use anyhow::Context;
    let manifest_path = std::path::Path::new(mount_path).join(".zen-garden/manifest.json");
    let content = tokio::fs::read_to_string(&manifest_path)
        .await
        .context("Failed to read manifest")?;
    let mut manifest: garden_common::storage::StorageManifest =
        serde_json::from_str(&content).context("Failed to parse manifest")?;
    manifest.visibility = visibility;
    write_manifest_atomic(&manifest_path, &manifest).await
}

async fn update_manifest_roles(mount_path: &str, roles: &[String]) -> anyhow::Result<()> {
    use anyhow::Context;
    let manifest_path = std::path::Path::new(mount_path).join(".zen-garden/manifest.json");
    let content = tokio::fs::read_to_string(&manifest_path)
        .await
        .context("Failed to read manifest")?;
    let mut manifest: garden_common::storage::StorageManifest =
        serde_json::from_str(&content).context("Failed to parse manifest")?;
    manifest.roles = roles.to_vec();
    write_manifest_atomic(&manifest_path, &manifest).await
}

async fn update_manifest_name(mount_path: &str, new_name: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    let manifest_path = std::path::Path::new(mount_path).join(".zen-garden/manifest.json");
    let content = tokio::fs::read_to_string(&manifest_path)
        .await
        .context("Failed to read manifest")?;
    let mut manifest: garden_common::storage::StorageManifest =
        serde_json::from_str(&content).context("Failed to parse manifest")?;
    manifest.name = new_name.to_string();
    write_manifest_atomic(&manifest_path, &manifest).await
}

/// Atomic manifest write: serialize to tmp file, then rename over original.
/// Crash-safe — on power loss, either old or new content survives, never partial.
async fn write_manifest_atomic(
    manifest_path: &std::path::Path,
    manifest: &garden_common::storage::StorageManifest,
) -> anyhow::Result<()> {
    use anyhow::Context;
    let tmp_path = manifest_path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(manifest).context("Failed to serialize manifest")?;
    tokio::fs::write(&tmp_path, &json)
        .await
        .context("Failed to write manifest temp file")?;

    // Windows doesn't support atomic rename over existing file
    #[cfg(windows)]
    if manifest_path.exists() {
        let _ = tokio::fs::remove_file(manifest_path).await;
    }

    tokio::fs::rename(&tmp_path, manifest_path)
        .await
        .context("Failed to rename manifest temp file")?;
    Ok(())
}

#[cfg(target_os = "linux")]
async fn unmount_device(mount_path: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    let _ = tokio::process::Command::new("sync").output().await;
    let output = tokio::process::Command::new("sudo")
        .args(["umount", mount_path])
        .output()
        .await
        .context("Failed to run umount")?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Unmount failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

// ============================================================================
// PATCH /api/v1/stone/storage/banks/:name/roles - Set Roles
// ============================================================================

/// Set composable roles on a managed storage (STORAGE-0009).
///
/// Roles are strings like `"seed-bank"`, `"archive"`, etc. They replace the
/// current roles array entirely.
pub async fn set_roles_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<SetRolesRequest>,
) -> Result<Json<ApiResponse<StorageInfo>>, (StatusCode, Json<ApiErrorResponse>)> {
    let mount_path = {
        let map = state.volumes.read().await;
        let vol = map.values()
            .find(|v| v.management.as_ref().is_some_and(|m| m.name == name))
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &format!("Bank '{}' not found", name)))?;
        vol.mount_path.to_string_lossy().to_string()
    };

    update_manifest_roles(&mount_path, &request.roles)
        .await
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ROLES_UPDATE_FAILED",
                &e.to_string(),
            )
        })?;

    // Sync roles to volume management
    {
        let mut map = state.volumes.write().await;
        if let Some(vol) = map.values_mut().find(|v| {
            v.management.as_ref().is_some_and(|m| m.name == name)
        }) {
            if let Some(ref mut mgmt) = vol.management {
                mgmt.roles = request.roles.clone();
            }
        }
    }

    // Re-read updated info
    let updated = {
        let map = state.volumes.read().await;
        map.values()
            .find(|v| v.management.as_ref().is_some_and(|m| m.name == name))
            .and_then(|v| v.to_storage_info())
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", "Bank not found after role update"))?
    };

    info!(name = %name, roles = ?request.roles, "Bank roles updated");

    // Refresh tools + broadcast beacon
    let tools_state = state.clone();
    tokio::spawn(async move {
        tools_state.refresh_local_tools_projection().await;
        let roles = crate::domain::storage::roles_snapshot(&tools_state.volumes).await;
        let pins = crate::domain::storage::pins_snapshot(&tools_state.volumes).await;
        if let Err(e) = crate::infra::storage::broadcast_beacon(
            &tools_state.stone_id,
            &tools_state.stone_name,
            &tools_state.self_entry.read().await.address.http_base(),
            &tools_state.volumes,
            Some(&roles),
            Some(&pins),
        )
        .await
        {
            warn!(error = %e, "Failed to broadcast storage beacon after roles change");
        }
    });

    Ok(Json(ApiResponse::new(updated.clone())))
}

// ============================================================================
// POST /api/v1/stone/storage/release-all
// ============================================================================

/// Safely unmount all seed banks
pub async fn release_all_seed_banks_v1(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<Vec<ReleaseResponse>>), (StatusCode, Json<ApiErrorResponse>)> {
    // Collect managed bank info before mutating
    let managed: Vec<(String, String)> = {
        let map = state.volumes.read().await;
        map.values()
            .filter_map(|v| {
                let mgmt = v.management.as_ref()?;
                Some((mgmt.name.clone(), v.mount_path.to_string_lossy().to_string()))
            })
            .collect()
    };

    let mut results = Vec::new();

    for (name, _mount_path) in &managed {
        // STORAGE-0006: Remove from MountTracker BEFORE unmount to prevent
        // persistence task re-mounting the device we're releasing.
        #[cfg(target_os = "linux")]
        {
            let mut tracker = state.mount_tracker.write().await;
            if tracker.remove(_mount_path.as_str()).is_some() {
                debug!(mount_path = %_mount_path, "Removed from mount tracker before release-all");
            }
        }

        #[cfg(target_os = "linux")]
        {
            match unmount_device(_mount_path).await {
                Ok(_) => {
                    results.push(ReleaseResponse {
                        released: true,
                        name: name.clone(),
                        message: "Seed bank safely released.".to_string(),
                    });
                }
                Err(e) => {
                    warn!(name = %name, error = %e, "Failed to unmount seed bank");
                    results.push(ReleaseResponse {
                        released: false,
                        name: name.clone(),
                        message: format!("Failed to unmount: {}", e),
                    });
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        results.push(ReleaseResponse {
            released: true,
            name: name.clone(),
            message: "Seed bank released (non-Linux).".to_string(),
        });
    }

    let pulse = DomainPulse::storage_event(
        event_types::STORAGE_RELEASED,
        format!("Released {} seed banks", results.len()),
        "info",
        None,
        Some(serde_json::json!({ "count": results.len() })),
    );
    let _ = state.pulse_tx.send(PulseEvent::Domain(pulse));

    // STORAGE-0011: Clear management from all volumes (all banks released)
    {
        let mut map = state.volumes.write().await;
        for vol in map.values_mut() {
            vol.management = None;
        }
    }

    let tools_state = state.clone();
    tokio::spawn(async move {
        tools_state.refresh_local_tools_projection().await;
        let roles = crate::domain::storage::roles_snapshot(&tools_state.volumes).await;
        let pins = crate::domain::storage::pins_snapshot(&tools_state.volumes).await;
        if let Err(e) = crate::infra::storage::broadcast_beacon(
            &tools_state.stone_id,
            &tools_state.stone_name,
            &tools_state.self_entry.read().await.address.http_base(),
            &tools_state.volumes,
            Some(&roles),
            Some(&pins),
        )
        .await
        {
            warn!(error = %e, "Failed to broadcast storage beacon after release-all");
        }
    });

    info!(count = results.len(), "All seed banks released");
    Ok((StatusCode::OK, Json(results)))
}

// ============================================================================
// Replication Endpoints (STORAGE-0006 Phase 4)
// ============================================================================

/// Query parameters for the changes pull endpoint.
#[derive(Debug, Deserialize)]
pub struct ChangesQuery {
    /// Cursor (GUIDv7) to resume from. If absent, returns all changelog entries.
    pub since: Option<String>,
}

// ============================================================================
// Pin / Unpin (STORAGE-0006 Phase 5)
// ============================================================================

/// Response for pin/unpin operations.
#[derive(Debug, Serialize)]
pub struct PinSeedBankResponse {
    pub name: String,
    pub pinned: bool,
    pub message: String,
}

/// POST /api/v1/stone/storage/banks/:name/pin
///
/// Pin the Primary role for a logical seed bank. Any stone holding a replica
/// can pin — this claims Primary with a GUIDv7-based pin_id. Last-pin-wins:
/// a newer pin_id (higher GUIDv7) overrides an older one garden-wide.
/// The pin is propagated via beacons so all stones resolve the winner.
pub async fn pin_bank_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<PinSeedBankResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "EMPTY_NAME",
            "Seed bank name is required",
        ));
    }

    // STORAGE-0011: Pin via Volume — writes pin.json, sets role to Primary.
    let pin_id = {
        let mut map = state.volumes.write().await;
        let vol = map
            .values_mut()
            .find(|v| v.management.as_ref().is_some_and(|m| m.name == name))
            .ok_or_else(|| {
                err(
                    StatusCode::NOT_FOUND,
                    "BANK_NOT_FOUND",
                    &format!("No seed bank named '{}' on this stone", name),
                )
            })?;

        vol.pin().await.map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "PIN_FAILED",
                &e.to_string(),
            )
        })?
    };

    // Re-broadcast beacon so other stones see the pin
    let tools_state = state.clone();
    let nudge = state.orchestration_nudge.clone();
    tokio::spawn(async move {
        tools_state.refresh_local_tools_projection().await;
        let roles = crate::domain::storage::roles_snapshot(&tools_state.volumes).await;
        let pins = crate::domain::storage::pins_snapshot(&tools_state.volumes).await;
        if let Err(e) = crate::infra::storage::broadcast_beacon(
            &tools_state.stone_id,
            &tools_state.stone_name,
            &tools_state.self_entry.read().await.address.http_base(),
            &tools_state.volumes,
            Some(&roles),
            Some(&pins),
        )
        .await
        {
            warn!(error = %e, "Failed to broadcast storage beacon after pin");
        }
        nudge.notify_one();
    });

    Ok(Json(ApiResponse::new(PinSeedBankResponse {
        name: name.clone(),
        pinned: true,
        message: format!(
            "Primary role for '{}' pinned to this stone (pin_id: {})",
            name, pin_id
        ),
    })))
}

/// POST /api/v1/stone/storage/banks/:name/unpin
///
/// Remove the Primary role pin for a logical seed bank. Returns to normal
/// first-online-wins orchestration.
pub async fn unpin_bank_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<PinSeedBankResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "EMPTY_NAME",
            "Seed bank name is required",
        ));
    }

    // STORAGE-0011: Unpin via Volume — clears pin + deletes disk file.
    let _was_pinned = {
        let mut map = state.volumes.write().await;
        if let Some(vol) = map.values_mut().find(|v| {
            v.management.as_ref().is_some_and(|m| m.name == name)
        }) {
            match vol.unpin().await {
                Ok(old_pin_id) => old_pin_id,
                Err(e) => {
                    warn!(name = %name, error = %e, "Unpin encountered an error");
                    None
                }
            }
        } else {
            debug!(name = %name, "Volume not found for unpin — no-op");
            None
        }
    };

    // Re-broadcast beacon
    let tools_state = state.clone();
    let nudge = state.orchestration_nudge.clone();
    tokio::spawn(async move {
        tools_state.refresh_local_tools_projection().await;
        let roles = crate::domain::storage::roles_snapshot(&tools_state.volumes).await;
        let pins = crate::domain::storage::pins_snapshot(&tools_state.volumes).await;
        if let Err(e) = crate::infra::storage::broadcast_beacon(
            &tools_state.stone_id,
            &tools_state.stone_name,
            &tools_state.self_entry.read().await.address.http_base(),
            &tools_state.volumes,
            Some(&roles),
            Some(&pins),
        )
        .await
        {
            warn!(error = %e, "Failed to broadcast storage beacon after unpin");
        }
        nudge.notify_one();
    });

    Ok(Json(ApiResponse::new(PinSeedBankResponse {
        name: name.clone(),
        pinned: false,
        message: format!("Primary role for '{}' is now unpinned", name),
    })))
}

// ============================================================================
// Replication Changelog (STORAGE-0006 Phase 4)
// ============================================================================

/// GET /api/v1/stone/storage/banks/:name/changes?since={cursor}
///
/// Pull changelog entries from a Primary seed bank.
/// Dormant replicas call this on remote Primaries to fetch mutations.
///
/// Returns `ChangesResponse { cursor, changes }`.
pub async fn bank_changes_v1(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(params): Query<ChangesQuery>,
) -> Result<
    Json<ApiResponse<garden_common::storage::ChangesResponse>>,
    (StatusCode, Json<ApiErrorResponse>),
> {
    let mount_path = {
        let map = state.volumes.read().await;
        let vol = map.values()
            .find(|v| v.management.as_ref().is_some_and(|m| m.name == name))
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &format!("Bank '{}' not found", name)))?;
        vol.mount_path.to_string_lossy().to_string()
    };

    if let Err(msg) = validate_seed_bank_layout(&mount_path) {
        return Err(err(StatusCode::CONFLICT, "BANK_NONCANONICAL", &msg));
    }

    let store = ContentStore::new_public(&mount_path);
    let resp = store
        .changes_since(params.since.as_deref())
        .await
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CHANGELOG_READ_FAILED",
                &e.to_string(),
            )
        })?;

    debug!(
        bank_name = %name,
        cursor = %resp.cursor,
        entries = resp.changes.len(),
        "Served changelog changes"
    );

    Ok(Json(ApiResponse::new(resp)))
}

/// Query parameters for the storage SSE stream.
#[derive(Debug, Deserialize)]
pub struct StorageStreamQuery {
    /// Seed bank name to filter ticks for. If absent, all ticks are forwarded.
    #[serde(rename = "seed-bank")]
    pub storage: Option<String>,
}

/// GET /api/v1/stone/storage/stream?seed-bank={name}
///
/// SSE "doorbell" for storage mutations on this stone.
/// Emits lightweight `storage.tick` events when the changelog advances.
/// Dormant replicas subscribe to this on the Primary stone.
///
/// Each event is ~100 bytes — the actual data is fetched via the
/// `/bank/{id}/changes` pull endpoint.
pub async fn stream_storage_v1(
    Query(query): Query<StorageStreamQuery>,
    State(state): State<AppState>,
) -> axum::response::sse::Sse<
    impl futures_util::stream::Stream<
        Item = Result<axum::response::sse::Event, std::convert::Infallible>,
    >,
> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use std::convert::Infallible;
    use tokio_stream::StreamExt;

    let token = state.shutdown_token.child_token();
    let rx = state.storage_agg_tx.subscribe();
    let filter_name = query.storage.clone();

    info!(
        seed_bank = ?filter_name,
        "Storage stream client connected"
    );

    let inner = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |result| {
        let filter_name = filter_name.clone();
        match result {
            Ok(tick) => {
                // If a filter is set, only emit ticks for that seed bank
                if let Some(ref name) = filter_name {
                    if tick.storage != *name {
                        return None;
                    }
                }
                let json = serde_json::to_string(&tick).unwrap_or_default();
                Some(Event::default().event("storage.tick").data(json))
            }
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                warn!(lagged = n, "Storage stream client lagged");
                None
            }
        }
    });

    let stream = async_stream::stream! {
        tokio::pin!(inner);
        loop {
            tokio::select! {
                item = inner.next() => {
                    match item {
                        Some(event) => yield Ok::<Event, Infallible>(event),
                        None => break,
                    }
                }
                _ = token.cancelled() => {
                    tracing::debug!("Storage stream: shutdown token cancelled");
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}
