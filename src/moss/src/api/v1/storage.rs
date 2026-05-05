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
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use garden_common::api_utils::ApiErrorResponse;
use garden_common::constants::paths;
use garden_common::storage::{
    AddStorageRequest, AddStorageResponse, CandidatesResponse, DEFAULT_REPLICA_SET_DISPLAY,
    DeviceState, MediumAction, MediumInfo, MediumPartitionInfo, RenameStorageRequest,
    SetRolesRequest, SetVisibilityRequest, StorageDetectedInfo, StorageInfo, StorageManifest,
    StorageVisibility,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::domain::storage::analyze_device;
use crate::infra::storage::{ContentStore, OsPlatform, layout};
use crate::infra::{DomainPulse, PulseEvent};
use crate::{Moss, error_response};
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
    /// Unique device ID (GUIDv7)
    pub id: String,
    /// Replica set display name (user-facing identity)
    pub name: String,
    /// Individual volume/device name
    #[serde(default)]
    pub volume_name: String,
    /// Replica set ID (STORAGE-0013)
    #[serde(default)]
    pub replica_set_id: String,
    /// Replica set display name (STORAGE-0013)
    #[serde(default)]
    pub replica_set_name: String,
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

// StorageHealth and SeedBankHealth live in the domain layer (ARCH-0005).
pub use crate::domain::storage::health::{SeedBankHealth, StorageHealth};

// Validation helpers are domain-pure and re-exported for use within this module.
use crate::domain::storage::health::validate_seed_bank_layout;

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

// ============================================================================
// GET /api/v1/stone/storage - Storage Overview
// ============================================================================

/// Get storage overview (types, counts)
///
/// Returns local bank stats plus garden-wide view from registry.
pub async fn storage_overview_v1(
    State(state): State<Moss>,
) -> crate::api::ApiResult<StorageOverview> {
    // Get local banks from the unified Volumes domain (STORAGE-0011)
    let local_banks: Vec<StorageInfo> = {
        let map = state.current.storage.volumes.read().await;
        map.values()
            .filter_map(|v| v.to_storage_info())
            .filter(|b| validate_seed_bank_layout(&b.mount_path).is_ok())
            .collect()
    };
    let total_capacity: u64 = local_banks.iter().map(|b| b.capacity_bytes).sum();
    let total_used: u64 = local_banks.iter().map(|b| b.used_bytes).sum();

    // Get garden-wide view from the Tool aggregate's registry.
    let storage_entries = state.tool.storage_entries().await;
    let local_roles = crate::domain::storage::roles_snapshot(&state.current.storage.volumes).await;
    let local_pins = crate::domain::storage::pins_snapshot(&state.current.storage.volumes).await;
    let mut garden_banks = Vec::new();

    for entry in storage_entries.iter() {
        let is_local = entry.tool.stone.id == state.current.stone.id;
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
                Some(garden_common::constants::ROLE_DORMANT) => {
                    garden_common::storage::StorageRole::Dormant
                }
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
            volume_name: entry.tool.tool.name.clone(),
            replica_set_id: sm.map(|s| s.replica_set_id.clone()).unwrap_or_default(),
            replica_set_name: sm.map(|s| s.replica_set_name.clone()).unwrap_or_default(),
            stone_id: entry.tool.stone.id.clone(),
            stone_name: entry.tool.stone.name.clone(),
            endpoint: entry.tool.stone.endpoint.clone(),
            is_local,
            visibility: sm
                .map(|s| s.visibility.clone())
                .unwrap_or_else(|| garden_common::constants::VISIBILITY_OPEN.to_string()),
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

    crate::api::ok(overview)
}

// ============================================================================
// GET /api/v1/stone/storage/health - Storage Readiness
// ============================================================================

/// Get storage readiness for this stone (mounted + canonical + writable).
pub async fn storage_health_v1(
    State(state): State<Moss>,
) -> crate::api::ApiResult<StorageHealth> {
    let managed: Vec<StorageInfo> = {
        let map = state.current.storage.volumes.read().await;
        map.values().filter_map(|v| v.to_storage_info()).collect()
    };

    let health = crate::domain::storage::health::assess_storage_health(managed).await;
    crate::api::ok(health)
}

// ============================================================================
// GET /api/v1/stone/storage/banks - List Banks
// ============================================================================

/// List all seed banks
pub async fn list_banks_v1(
    State(state): State<Moss>,
) -> crate::api::ApiResult<Vec<StorageInfo>> {
    let banks: Vec<StorageInfo> = {
        let map = state.current.storage.volumes.read().await;
        map.values()
            .filter_map(|v| v.to_storage_info())
            .filter(|b| validate_seed_bank_layout(&b.mount_path).is_ok())
            .collect()
    };
    crate::api::ok(banks)
}

// ============================================================================
// GET /api/v1/stone/storage/banks/:name - Get Bank Details
// ============================================================================

/// Get seed bank details by name
pub async fn get_bank_v1(
    State(state): State<Moss>,
    Path(name): Path<String>,
) -> crate::api::ApiResult<StorageInfo> {
    use crate::domain::storage::bank_aggregate;

    let info = bank_aggregate::volumes_for_bank(&name, &state.current.storage.volumes)
        .await
        .into_iter()
        .find_map(|v| v.to_storage_info())
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "BANK_NOT_FOUND",
                &format!("Bank '{}' not found", name),
            )
        })?;

    if let Err(msg) = validate_seed_bank_layout(&info.mount_path) {
        return Err(err(StatusCode::CONFLICT, "BANK_NONCANONICAL", &msg));
    }

    crate::api::ok(info)
}

// ============================================================================
// DELETE /api/v1/stone/storage/banks/:name - Delete Bank
// ============================================================================

/// Remove seed bank mount directory (device must be unmounted first)
pub async fn delete_bank_v1(
    State(state): State<Moss>,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiErrorResponse>)> {
    use crate::domain::storage::bank_aggregate;

    // Check if still mounted — bank query only returns online volumes
    if bank_aggregate::by_name(&name, &state.current.storage.volumes)
        .await
        .is_some()
    {
        return Err(err(
            StatusCode::CONFLICT,
            "BANK_MOUNTED",
            "Bank must be released before deletion",
        ));
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

    // Emit through EventBus so PulseDomainBridge picks it up for SSE consumers.
    state
        .emit_storage_changed(garden_common::storage::StorageChanged::Reclassified)
        .await;

    info!(name = %name, "Bank mount directory removed");
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================================
// POST /api/v1/stone/storage/banks/:name/release - Release Bank
// ============================================================================

/// Safely unmount a seed bank
pub async fn release_bank_v1(
    State(state): State<Moss>,
    Path(name): Path<String>,
) -> crate::api::ApiResult<ReleaseResponse> {
    use crate::domain::storage::bank_aggregate;

    // Verify bank exists before attempting release
    let _bank = bank_aggregate::by_name(&name, &state.current.storage.volumes)
        .await
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "BANK_NOT_FOUND",
                &format!("Bank '{}' not found", name),
            )
        })?;

    #[cfg(target_os = "linux")]
    if let Some(ref mp) = _bank.mount_path {
        crate::infra::storage::platform::unmount(&mp.to_string_lossy())
            .await
            .map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "UNMOUNT_FAILED",
                    &e.to_string(),
                )
            })?;
    }

    // Domain command — releases all volumes in the bank
    let (events, _mount_paths) = bank_aggregate::release(&name, &state.current.storage.volumes)
        .await
        .map_err(|e| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &e.to_string()))?;

    debug!(name = %name, "Cleared management from released volumes");

    for event in events {
        state.emit_storage_changed(event).await;
    }

    info!(name = %name, "Bank released");
    crate::api::ok(ReleaseResponse {
        released: true,
        name,
        message: "Bank safely released. You may now remove the device.".to_string(),
    })
}

// ============================================================================
// PATCH /api/v1/stone/storage/banks/:name/rename - Rename Bank
// ============================================================================

/// Rename a storage bank.
///
/// The `name` path parameter matches on the bank display name (the user-facing
/// identity). Updates all local volumes that belong to this bank.
pub async fn rename_bank_v1(
    State(state): State<Moss>,
    Path(name): Path<String>,
    Json(request): Json<RenameStorageRequest>,
) -> crate::api::ApiResult<StorageInfo> {
    use crate::domain::storage::bank_aggregate;

    // Domain command — validates name, renames all volumes in the bank
    let result = bank_aggregate::rename(&name, &request.new_name, &state.current.storage.volumes)
        .await
        .map_err(|e| match &e {
            bank_aggregate::BankError::NotFound(_) => {
                err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &e.to_string())
            }
            bank_aggregate::BankError::InvalidName(_) => {
                err(StatusCode::BAD_REQUEST, "INVALID_NAME", &e.to_string())
            }
            _ => err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "RENAME_FAILED",
                &e.to_string(),
            ),
        })?;

    // Persist to disk manifests (infra concern)
    for mp in &result.mount_paths {
        update_manifest_replica_set_name(mp, &request.new_name)
            .await
            .map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "RENAME_FAILED",
                    &e.to_string(),
                )
            })?;
    }

    // Forward domain events from rename
    for event in result.events {
        state.emit_storage_changed(event).await;
    }

    // Re-read updated info
    let updated =
        bank_aggregate::volumes_for_bank(&request.new_name, &state.current.storage.volumes)
            .await
            .into_iter()
            .find_map(|v| v.to_storage_info())
            .ok_or_else(|| {
                err(
                    StatusCode::NOT_FOUND,
                    "BANK_NOT_FOUND",
                    "Bank not found after rename",
                )
            })?;

    info!(old_name = %name, new_name = %request.new_name, volumes = result.mount_paths.len(), "Bank renamed");

    // STORAGE-0013: Emit domain event — beacon subscriber + orchestration react
    state
        .emit_storage_changed(garden_common::storage::StorageChanged::Renamed {
            replica_set_id: result.replica_set_id,
            new_name: request.new_name.clone(),
        })
        .await;
    state.current.storage.coordination.nudge.notify_one();

    crate::api::ok(updated)
}

/// Refine a medium's condition with the connectivity helper's verdict.
///
/// STORAGE-0019: a medium that the platform scanner classified as
/// `Unreachable` (or zero-bytes-with-the-current-classifier) gets a
/// more accurate label after the connectivity stage runs:
///
/// - If recovery succeeded and the device now reports a real size,
///   keep the platform's recovered classification (probably
///   `Adoptable` or `Raw`).
/// - If recovery did not succeed and the connectivity status records
///   I/O errors → stay `Unreachable` (replug hint to the user).
/// - If recovery did not run / did not succeed and there are no I/O
///   errors → reclassify as `NoMedia` (empty enclosure, insert a
///   drive).
fn refine_condition_with_connectivity(
    platform_condition: crate::infra::storage::platform::MediumCondition,
    size_bytes: u64,
    connectivity_status: Option<&garden_common::storage::ConnectivityStatus>,
) -> crate::infra::storage::platform::MediumCondition {
    use crate::infra::storage::platform::MediumCondition as MC;

    // Connectivity didn't run → defer entirely to the platform.
    let Some(status) = connectivity_status else {
        return platform_condition;
    };

    // Recovery succeeded — trust the post-recovery platform
    // classification regardless of the original condition.
    if status.was_recovered() && size_bytes > 0 {
        return platform_condition;
    }

    // Recovery couldn't help (or wasn't tried). Distinguish empty
    // enclosure from a real fault by the presence of I/O errors in
    // the residual warnings.
    let has_io_errors = status.residual_warnings.iter().any(|w| {
        matches!(
            w,
            garden_common::storage::ConnectivityWarning::PriorIoErrors { .. }
        )
    });

    if size_bytes == 0 {
        if has_io_errors {
            MC::Unreachable
        } else {
            MC::NoMedia
        }
    } else {
        platform_condition
    }
}

// ============================================================================
// GET /api/v1/stone/storage/candidates
// ============================================================================

/// List eligible devices awaiting preparation.
///
/// Returns unmanaged, removable, online volumes from the Volumes domain.
pub async fn list_candidates_v1(
    State(state): State<Moss>,
) -> Result<(StatusCode, Json<CandidatesResponse>), (StatusCode, Json<ApiErrorResponse>)> {
    // Space and medium candidates collected under a single scope for both read guards.
    let (spaces, media) = {
        // Space candidates: mounted volumes that are unmanaged, removable, and online.
        let volumes_map = state.current.storage.volumes.read().await;
        let spaces: Vec<StorageDetectedInfo> = volumes_map
            .values()
            .filter(|v| !v.is_managed() && v.removable() && v.state().is_online())
            .map(|v| StorageDetectedInfo {
                device: v.path().to_string(),
                mount_path: Some(v.mount_path().to_string_lossy().to_string()),
                label: v.label().map(|s| s.to_string()),
                capacity_bytes: v.capacity_bytes(),
                state: DeviceState::Empty,
                eligible: true,
                removable: v.removable(),
                ineligible_reason: None,
            })
            .collect();

        // Medium candidates: physical disks (USB/external only).
        // STORAGE-0019: degraded candidates (zero size or already
        // marked Unreachable) flow through the connectivity helper —
        // SCSI rescan / USB re-auth — before the response is built.
        // The per-device retry budget (default 1 attempt of each
        // action per minute) caps the impact of repeated polls.
        let media_map = state.current.storage.media.read().await;
        let connectivity_budget = crate::infra::storage::connectivity::shared_budget();
        let cancel = state.shutdown_token.child_token();

        let mut media: Vec<MediumInfo> = Vec::with_capacity(media_map.len());
        for m in media_map.values().filter(|m| m.removable) {
            let snapshot = m.snapshot();

            // Run connectivity evaluation only when the cached medium
            // already shows signs of trouble. Healthy media skip this
            // path entirely so storage list stays fast.
            let needs_connectivity_check = matches!(
                snapshot.condition,
                crate::infra::storage::platform::MediumCondition::Unreachable
                    | crate::infra::storage::platform::MediumCondition::NoMedia
            ) || snapshot.size_bytes == 0;

            let (effective_snapshot, connectivity_status) = if needs_connectivity_check {
                if let Some(basename) = crate::infra::storage::connectivity::extract_basename(
                    &snapshot.device_id,
                ) {
                    let enriched = crate::infra::storage::connectivity::evaluate_candidate(
                        snapshot.clone(),
                        &basename,
                        connectivity_budget,
                        &cancel,
                    )
                    .await;
                    (enriched.snapshot, Some(enriched.status))
                } else {
                    // Non-Linux device id format — connectivity helper
                    // is sysfs-driven, so skip and forward as-is.
                    (snapshot, None)
                }
            } else {
                (snapshot, None)
            };

            // Refine the condition with the connectivity verdict so
            // NoMedia / Unreachable reflect the post-recovery reality.
            let refined_condition = refine_condition_with_connectivity(
                effective_snapshot.condition,
                effective_snapshot.size_bytes,
                connectivity_status.as_ref(),
            );

            // Inline the partition-level checks that exist on
            // `Medium`; the snapshot doesn't carry domain methods.
            let managed = effective_snapshot.partitions.iter().any(|p| {
                p.mount_path.as_ref().is_some_and(|mp| {
                    volumes_map.values().any(|v| {
                        v.is_managed() && v.mount_path().to_string_lossy() == mp.as_str()
                    })
                })
            });
            // STORAGE-0019: the new five-state MediumCondition
            // taxonomy maps to the existing MediumAction surface
            // until unit 6 lands the full `adopt` / `format` verb
            // split in Rake. Adoptable subsumes the legacy
            // `Partitioned` value; managed `.zen-garden/` → already
            // adopted; mounted filesystem → ready as-is; otherwise
            // the user sees a "needs format" hint that resolves to
            // `garden-rake storage adopt` or `storage format` in
            // the new CLI.
            let suggested_action = match refined_condition {
                crate::infra::storage::platform::MediumCondition::Unreachable
                | crate::infra::storage::platform::MediumCondition::NoMedia => {
                    MediumAction::Unreadable
                }
                crate::infra::storage::platform::MediumCondition::Raw => {
                    MediumAction::NeedsPartition
                }
                crate::infra::storage::platform::MediumCondition::Adoptable
                | crate::infra::storage::platform::MediumCondition::Empty => {
                    let has_mounted = effective_snapshot
                        .partitions
                        .iter()
                        .any(|p| p.mount_path.is_some());
                    if managed {
                        MediumAction::AlreadyManaged
                    } else if has_mounted {
                        MediumAction::Ready
                    } else {
                        MediumAction::NeedsFormat
                    }
                }
            };

            media.push(MediumInfo {
                device_id: effective_snapshot.device_id.clone(),
                model: effective_snapshot.model.clone(),
                bus_type: garden_common::storage::BusType::from(effective_snapshot.bus_type),
                size_bytes: effective_snapshot.size_bytes,
                removable: effective_snapshot.removable,
                condition: garden_common::storage::MediumCondition::from(refined_condition),
                partitions: effective_snapshot
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
                connectivity_status,
            });
        }

        (spaces, media)
    };

    Ok((StatusCode::OK, Json(CandidatesResponse { spaces, media })))
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
    State(state): State<Moss>,
    Json(request): Json<AddStorageRequest>,
) -> crate::api::ApiResult<AddStorageResponse> {
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
                    &format!(
                        "Device {} is already being added — wait for it to finish",
                        target
                    ),
                ));
            }
        }

        // Analyze device state
        let device_info = analyze_device(target, &OsPlatform).map_err(|e| {
            err(
                StatusCode::BAD_REQUEST,
                "DEVICE_ANALYSIS_FAILED",
                &e.to_string(),
            )
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
                DEFAULT_REPLICA_SET_DISPLAY.to_string()
            } else {
                generate_storage_name()
            }
        });

        // Same-name replicas are fine (STORAGE-0006)
        {
            let map = state.current.storage.volumes.read().await;
            if map
                .values()
                .any(|v| v.management().is_some_and(|m| m.name == name))
            {
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

            let task_job_id = job_id.clone();
            let task_name = name.clone();
            let device = target.to_string();
            let filesystem = request.filesystem.clone();
            let encrypted = request.encrypted;
            let roles = request.roles.clone();
            let stone_name = state.current.stone.name.clone();
            let pulse = state.pulse.clone();
            let tools_state = state.clone();
            let guard_device = target.to_string();

            tokio::spawn(async move {
                let _guard = PrepareGuard {
                    device: guard_device,
                };

                match run_format_and_add(
                    &task_job_id,
                    &device,
                    &task_name,
                    &filesystem,
                    encrypted,
                    &roles,
                    &stone_name,
                    pulse.clone(),
                )
                .await
                {
                    Ok(()) => {
                        tools_state
                            .emit_storage_changed(garden_common::storage::StorageChanged::Sensed {
                                name: task_name.clone(),
                                roles: roles.clone(),
                            })
                            .await;
                        tools_state
                            .emit_storage_changed(
                                garden_common::storage::StorageChanged::Reclassified,
                            )
                            .await;
                    }
                    Err(e) => {
                        tracing::error!(
                            job_id = %task_job_id, device = %device, name = %task_name,
                            error = %e, error_chain = ?e, "Storage add (format) FAILED"
                        );
                        let failure_pulse = DomainPulse::storage_event(
                            event_types::STORAGE_ADD_PROGRESS,
                            format!("Add failed: {} - {}", task_name, e),
                            "error",
                            Some(task_job_id.clone()),
                            Some(serde_json::json!({ "name": task_name, "error": e.to_string() })),
                        );
                        let _ = pulse.send(PulseEvent::Domain(failure_pulse));
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
            return crate::api::ok(response);
        }

        // ── Device has filesystem but no format needed ──────────────────
        // Mount the device, then fall through to the adopt path
        let visibility = if request.encrypted {
            StorageVisibility::Closed
        } else {
            StorageVisibility::Open
        };

        let manifest = StorageManifest::with_roles(
            &name,
            &state.current.stone.name,
            "unknown",
            visibility,
            request.roles.clone(),
        );

        let data_dir = garden_common::constants::paths::data_dir();
        let mount_dir = PathBuf::from(manifest.derive_mount_path(&data_dir));

        #[cfg(target_os = "linux")]
        {
            #[expect(unused_imports)]
            use anyhow::Context;
            let output = tokio::process::Command::new("sudo")
                .args(["mkdir", "-p", &mount_dir.to_string_lossy()])
                .output()
                .await
                .map_err(|e| {
                    err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "MOUNT_FAILED",
                        &format!("Failed to create mount dir: {}", e),
                    )
                })?;
            if !output.status.success() {
                return Err(err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "MOUNT_FAILED",
                    &format!("mkdir failed: {}", String::from_utf8_lossy(&output.stderr)),
                ));
            }
            mount_device(target, &mount_dir).await.map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "MOUNT_FAILED",
                    &e.to_string(),
                )
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
        &name,
        &state.current.stone.name,
        "unknown",
        visibility,
        request.roles.clone(),
    );

    add_at_path(state, target_path, manifest, false).await
}

/// Shared logic: initialize layout, write manifest, catalog content, broadcast.
async fn add_at_path(
    state: Moss,
    mount_path: &std::path::Path,
    manifest: StorageManifest,
    formatted: bool,
) -> crate::api::ApiResult<AddStorageResponse> {
    // Migrate legacy layout if present
    layout::migrate_legacy_layout(mount_path)
        .await
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "MIGRATION_FAILED",
                &format!("Failed to migrate legacy layout: {}", e),
            )
        })?;

    // Initialize layout (creates all subdirs + symlink, idempotent)
    layout::initialize_layout(mount_path).await.map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "LAYOUT_INIT_FAILED",
            &format!("Failed to initialize storage layout: {}", e),
        )
    })?;

    // Write manifest atomically
    let manifest_path = mount_path
        .join(paths::STORAGE_DOTFOLDER)
        .join("manifest.json");
    write_manifest_atomic(&manifest_path, &manifest)
        .await
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "MANIFEST_WRITE_FAILED",
                &format!("Failed to write manifest: {}", e),
            )
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
    let _ = state.current.storage.coordination.rescan.try_send(());

    // STORAGE-0013: Emit domain event — beacon subscriber reacts
    state
        .emit_storage_changed(garden_common::storage::StorageChanged::Added {
            device_id: manifest.id.clone(),
            replica_set_id: manifest.replica_set_id.clone(),
        })
        .await;

    state
        .emit_storage_changed(garden_common::storage::StorageChanged::Sensed {
            name: manifest.name.clone(),
            roles: manifest.roles.clone(),
        })
        .await;

    let storages = crate::domain::storage::name_id_pairs(&state.current.storage.volumes).await;
    if let Err(e) = crate::infra::storage::refresh_signpost(
        &state.current.stone.name,
        state.current.api_port,
        &storages,
    )
    .await
    {
        warn!(error = %e, "Failed to refresh signpost after add");
    }

    crate::api::ok(response)
}

/// Run format-and-add job in background (for block devices needing formatting).
#[expect(clippy::too_many_arguments)]
async fn run_format_and_add(
    job_id: &str,
    device: &str,
    name: &str,
    filesystem: &str,
    encrypted: bool,
    roles: &[String],
    stone_name: &str,
    pulse: tokio::sync::broadcast::Sender<PulseEvent>,
) -> anyhow::Result<()> {
    use anyhow::Context;

    info!(
        job_id,
        device, name, encrypted, "Starting storage add (format)"
    );
    emit_progress(&pulse, job_id, name, "analyzing", "Analyzing device...");

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

    let manifest =
        StorageManifest::with_roles(name, stone_name, actual_fs, visibility, roles.to_vec());

    let data_dir = garden_common::constants::paths::data_dir();
    let mount_dir = PathBuf::from(manifest.derive_mount_path(&data_dir));

    #[cfg(target_os = "linux")]
    {
        let output = tokio::process::Command::new("sudo")
            .args(["mkdir", "-p", &mount_dir.to_string_lossy()])
            .output()
            .await
            .context("Failed to run sudo mkdir")?;
        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Failed to create mount directory: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    #[cfg(not(target_os = "linux"))]
    tokio::fs::create_dir_all(&mount_dir)
        .await
        .context("Failed to create mount directory")?;

    emit_progress(
        &pulse,
        job_id,
        name,
        "formatting",
        &format!("Formatting as {}...", actual_fs),
    );

    #[cfg(target_os = "linux")]
    format_device(device, actual_fs)
        .await
        .context("Failed to format device")?;

    emit_progress(&pulse, job_id, name, "mounting", "Mounting filesystem...");

    #[cfg(target_os = "linux")]
    mount_device(device, &mount_dir)
        .await
        .context("Failed to mount device")?;

    #[cfg(target_os = "linux")]
    {
        let output = tokio::process::Command::new("sudo")
            .args(["chown", "-R", "stone:stone", &mount_dir.to_string_lossy()])
            .output()
            .await
            .context("Failed to run chown")?;
        if !output.status.success() {
            warn!(
                "Failed to chown mount directory: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    emit_progress(
        &pulse,
        job_id,
        name,
        "creating",
        "Creating storage structure...",
    );

    // Initialize canonical layout
    layout::initialize_layout(&mount_dir)
        .await
        .context("Failed to initialize storage layout")?;

    // Write manifest atomically
    let manifest_path = mount_dir
        .join(paths::STORAGE_DOTFOLDER)
        .join("manifest.json");
    write_manifest_atomic(&manifest_path, &manifest)
        .await
        .context("Failed to write manifest")?;

    // Sync filesystem
    #[cfg(target_os = "linux")]
    let _ = tokio::process::Command::new("sync").output().await;

    // Emit completion
    let connected_pulse = DomainPulse::storage_event(
        event_types::STORAGE_CONNECTED,
        format!("Storage '{}' added at {}", name, mount_dir.display()),
        "info",
        Some(job_id.to_string()),
        Some(serde_json::json!({ "name": name, "mount_path": mount_dir.to_string_lossy() })),
    );
    let _ = pulse.send(PulseEvent::Domain(connected_pulse));

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
    use rand::prelude::IndexedRandom;
    const ADJECTIVES: &[&str] = &[
        "kind", "wise", "calm", "bold", "swift", "quiet", "bright", "deep", "warm", "cool",
        "fresh", "clear", "soft", "strong", "gentle",
    ];
    const NOUNS: &[&str] = &[
        "meadow", "valley", "river", "forest", "garden", "grove", "brook", "stone", "path",
        "spring", "hill", "field", "shore", "cliff", "peak",
    ];
    let mut rng = rand::rng();
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
    State(state): State<Moss>,
    Path(name): Path<String>,
    Json(request): Json<SetVisibilityRequest>,
) -> Result<(StatusCode, Json<StorageInfo>), (StatusCode, Json<ApiErrorResponse>)> {
    use crate::domain::storage::bank_aggregate;

    // Get mount path for manifest persistence (infra concern)
    let bank = bank_aggregate::by_name(&name, &state.current.storage.volumes)
        .await
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "BANK_NOT_FOUND",
                &format!("Bank '{}' not found", name),
            )
        })?;

    if let Some(ref mp) = bank.mount_path {
        update_manifest_visibility(&mp.to_string_lossy(), request.visibility)
            .await
            .map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "MANIFEST_UPDATE_FAILED",
                    &e.to_string(),
                )
            })?;
    }

    // Domain command
    let events =
        bank_aggregate::set_visibility(&name, request.visibility, &state.current.storage.volumes)
            .await
            .map_err(|e| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &e.to_string()))?;

    for event in events {
        state.emit_storage_changed(event).await;
    }

    // Re-read updated info
    let updated = bank_aggregate::volumes_for_bank(&name, &state.current.storage.volumes)
        .await
        .into_iter()
        .find_map(|v| v.to_storage_info())
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "BANK_NOT_FOUND",
                "Bank disappeared after update",
            )
        })?;

    info!(name = %name, visibility = ?request.visibility, "Bank visibility updated");

    state
        .emit_storage_changed(garden_common::storage::StorageChanged::Reclassified)
        .await;

    Ok((StatusCode::OK, Json(updated)))
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

/// Update the replica set display name in the on-disk manifest.
pub(crate) async fn update_manifest_replica_set_name(
    mount_path: &str,
    new_name: &str,
) -> anyhow::Result<()> {
    use anyhow::Context;
    let manifest_path = std::path::Path::new(mount_path).join(".zen-garden/manifest.json");
    let content = tokio::fs::read_to_string(&manifest_path)
        .await
        .context("Failed to read manifest")?;
    let mut manifest: garden_common::storage::StorageManifest =
        serde_json::from_str(&content).context("Failed to parse manifest")?;
    manifest.replica_set_name = new_name.to_string();
    manifest.replica_set_name_updated_at = Some(chrono::Utc::now());
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

// ============================================================================
// PATCH /api/v1/stone/storage/banks/:name/roles - Set Roles
// ============================================================================

/// Set composable roles on a managed storage (STORAGE-0009).
///
/// Roles are strings like `"seed-bank"`, `"archive"`, etc. They replace the
/// current roles array entirely.
pub async fn set_roles_v1(
    State(state): State<Moss>,
    Path(name): Path<String>,
    Json(request): Json<SetRolesRequest>,
) -> crate::api::ApiResult<StorageInfo> {
    use crate::domain::storage::bank_aggregate;

    // Get mount path for manifest persistence (infra concern)
    let bank = bank_aggregate::by_name(&name, &state.current.storage.volumes)
        .await
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "BANK_NOT_FOUND",
                &format!("Bank '{}' not found", name),
            )
        })?;

    if let Some(ref mp) = bank.mount_path {
        update_manifest_roles(&mp.to_string_lossy(), &request.roles)
            .await
            .map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ROLES_UPDATE_FAILED",
                    &e.to_string(),
                )
            })?;
    }

    // Domain command
    let events =
        bank_aggregate::set_roles(&name, request.roles.clone(), &state.current.storage.volumes)
            .await
            .map_err(|e| err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &e.to_string()))?;

    for event in events {
        state.emit_storage_changed(event).await;
    }

    // Re-read updated info
    let updated = bank_aggregate::volumes_for_bank(&name, &state.current.storage.volumes)
        .await
        .into_iter()
        .find_map(|v| v.to_storage_info())
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "BANK_NOT_FOUND",
                "Bank not found after role update",
            )
        })?;

    info!(name = %name, roles = ?request.roles, "Bank roles updated");

    state
        .emit_storage_changed(garden_common::storage::StorageChanged::Reclassified)
        .await;

    crate::api::ok(updated)
}

// ============================================================================
// POST /api/v1/stone/storage/release-all
// ============================================================================

/// Safely unmount all seed banks
pub async fn release_all_seed_banks_v1(
    State(state): State<Moss>,
) -> Result<(StatusCode, Json<Vec<ReleaseResponse>>), (StatusCode, Json<ApiErrorResponse>)> {
    // Collect managed bank info before mutating
    let managed: Vec<(String, String)> = {
        let map = state.current.storage.volumes.read().await;
        map.values()
            .filter_map(|v| {
                let mgmt = v.management()?;
                Some((
                    mgmt.name.clone(),
                    v.mount_path().to_string_lossy().to_string(),
                ))
            })
            .collect()
    };

    let mut results = Vec::new();

    for (name, _mount_path) in &managed {
        #[cfg(target_os = "linux")]
        {
            match crate::infra::storage::platform::unmount(_mount_path).await {
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
    let _ = state.pulse.send(PulseEvent::Domain(pulse));

    // STORAGE-0011: Clear management from all volumes (all banks released)
    let release_events = {
        let mut map = state.current.storage.volumes.write().await;
        let mut all_events = Vec::new();
        for vol in map.values_mut() {
            if vol.is_managed() {
                all_events.extend(vol.release());
            }
        }
        all_events
    };
    for event in release_events {
        state.emit_storage_changed(event).await;
    }

    // STORAGE-0013: Emit domain event — beacon subscriber reacts
    state
        .emit_storage_changed(garden_common::storage::StorageChanged::Reclassified)
        .await;

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
    State(state): State<Moss>,
    Path(name): Path<String>,
) -> crate::api::ApiResult<PinSeedBankResponse> {
    use crate::domain::storage::bank_aggregate;

    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "EMPTY_NAME",
            "Bank name is required",
        ));
    }

    // Domain command — pin via Bank aggregate
    let events = bank_aggregate::pin(&name, &state.current.storage.volumes, |path: PathBuf| {
        std::sync::Arc::new(ContentStore::new(path, None))
    })
    .await
    .map_err(|e| match &e {
        bank_aggregate::BankError::NotFound(_) => {
            err(StatusCode::NOT_FOUND, "BANK_NOT_FOUND", &e.to_string())
        }
        _ => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "PIN_FAILED",
            &e.to_string(),
        ),
    })?;

    for event in &events {
        state.emit_storage_changed(event.clone()).await;
    }
    state.current.storage.coordination.nudge.notify_one();

    crate::api::ok(PinSeedBankResponse {
        name: name.clone(),
        pinned: true,
        message: format!("Primary role for '{}' pinned to this stone", name),
    })
}

/// POST /api/v1/stone/storage/banks/:name/unpin
///
/// Remove the Primary role pin for a logical seed bank. Returns to normal
/// first-online-wins orchestration.
pub async fn unpin_bank_v1(
    State(state): State<Moss>,
    Path(name): Path<String>,
) -> crate::api::ApiResult<PinSeedBankResponse> {
    use crate::domain::storage::bank_aggregate;

    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "EMPTY_NAME",
            "Bank name is required",
        ));
    }

    // Domain command — unpin via Bank aggregate
    let events = bank_aggregate::unpin(&name, &state.current.storage.volumes, |path: PathBuf| {
        std::sync::Arc::new(ContentStore::new(path, None))
    })
    .await
    .unwrap_or_else(|e| {
        debug!(name = %name, error = %e, "Unpin no-op — bank not found or not pinned");
        vec![]
    });

    for event in &events {
        state.emit_storage_changed(event.clone()).await;
    }
    state.current.storage.coordination.nudge.notify_one();

    crate::api::ok(PinSeedBankResponse {
        name: name.clone(),
        pinned: false,
        message: format!("Primary role for '{}' is now unpinned", name),
    })
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
    State(state): State<Moss>,
    Path(name): Path<String>,
    Query(params): Query<ChangesQuery>,
) -> crate::api::ApiResult<garden_common::storage::ChangesResponse> {
    use crate::domain::storage::bank_aggregate;

    let bank = bank_aggregate::by_name(&name, &state.current.storage.volumes)
        .await
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "BANK_NOT_FOUND",
                &format!("Bank '{}' not found", name),
            )
        })?;

    let mount_path = bank
        .mount_path
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "BANK_NOT_FOUND",
                "Bank has no mounted volume",
            )
        })?;

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

    crate::api::ok(resp)
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
    State(state): State<Moss>,
) -> axum::response::sse::Sse<
    impl futures_util::stream::Stream<
        Item = Result<axum::response::sse::Event, std::convert::Infallible>,
    >,
> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use std::convert::Infallible;
    use tokio_stream::StreamExt;

    let token = state.shutdown_token.child_token();
    let rx = state.current.storage.tick_stream();
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
                if let Some(ref name) = filter_name
                    && tick.storage != *name
                {
                    return None;
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

// ============================================================================
// GET /api/v1/stone/storage/s3/ports - S3 Port Catalog (STORAGE-0016)
// ============================================================================

/// Returns the mapping of replica set name → S3 port for all armed listeners.
///
/// Clients use this to discover which port to connect to for S3 operations
/// on a specific storage.
///
/// Response: `{ "ports": { "storage": 23454, "prod": 23455 } }`
pub async fn s3_port_catalog(State(state): State<Moss>) -> axum::Json<serde_json::Value> {
    let catalog = state
        .current
        .storage
        .coordination
        .s3_listeners
        .port_catalog()
        .await;
    axum::Json(serde_json::json!({ "ports": catalog }))
}
