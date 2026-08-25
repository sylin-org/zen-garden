//! Bank API endpoints (ARCH-0026)
//!
//! First-class `/banks` routes replacing the legacy `/storage/banks` paths.
//!
//! ## Stone-local routes
//!
//! ```text
//! GET  /api/v1/stone/banks                 → list banks with a local volume
//! GET  /api/v1/stone/banks/{moniker}       → local volume details
//! POST /api/v1/stone/banks/{moniker}/pin   → claim Primary
//! POST /api/v1/stone/banks/{moniker}/unpin → release Primary
//! ```
//!
//! ## Garden-wide routes
//!
//! ```text
//! GET  /api/v1/garden/banks                    → all banks in the garden
//! GET  /api/v1/garden/banks/{moniker}          → bank details + volume locations
//! GET  /api/v1/garden/banks/{moniker}/volumes  → individual volumes
//! ```

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use garden_common::api_utils::ApiErrorResponse;
use garden_common::storage::{StorageInfo, StorageRole};
use serde::Serialize;
use std::path::PathBuf;
use tracing::debug;

use crate::domain::storage::bank_aggregate;
use crate::infra::storage::ContentStore;
use crate::{Moss, error_response};

// ============================================================================
// Shared helpers
// ============================================================================

fn err(status: StatusCode, code: &str, msg: &str) -> (StatusCode, Json<ApiErrorResponse>) {
    error_response(status, code, msg, None)
}

// ============================================================================
// Response types
// ============================================================================

/// Summary of a bank visible across the garden.
#[derive(Debug, Serialize)]
pub struct GardenBankSummary {
    pub name: String,
    pub replica_count: usize,
    pub primary_stone: Option<String>,
    pub roles: Vec<String>,
}

/// A single volume within a bank.
#[derive(Debug, Serialize)]
pub struct BankVolume {
    pub stone_id: String,
    pub stone_name: String,
    pub volume_id: String,
    pub role: StorageRole,
    pub endpoint: String,
    pub roles: Vec<String>,
}

/// Bank details including volumes across the garden.
#[derive(Debug, Serialize)]
pub struct GardenBankDetail {
    pub name: String,
    pub replica_count: usize,
    pub primary_stone: Option<String>,
    pub roles: Vec<String>,
    pub volumes: Vec<BankVolume>,
}

/// Pin/unpin response.
#[derive(Debug, Serialize)]
pub struct PinResponse {
    pub name: String,
    pub pinned: bool,
    pub message: String,
}

// ============================================================================
// Stone-local: GET /api/v1/stone/banks
// ============================================================================

/// List all banks with a local volume on this stone.
pub async fn list_banks(State(state): State<Moss>) -> crate::api::ApiResult<Vec<StorageInfo>> {
    let infos = bank_aggregate::bank_infos(&state.current.storage.volumes).await;
    crate::api::ok(infos)
}

// ============================================================================
// Stone-local: GET /api/v1/stone/banks/{moniker}
// ============================================================================

/// Get bank details for a single bank on this stone.
pub async fn get_bank(
    State(state): State<Moss>,
    Path(moniker): Path<String>,
) -> crate::api::ApiResult<StorageInfo> {
    let info = bank_aggregate::volumes_for_bank(&moniker, &state.current.storage.volumes)
        .await
        .into_iter()
        .find_map(|v| v.to_storage_info())
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "BANK_NOT_FOUND",
                &format!("Bank '{}' not found", moniker),
            )
        })?;

    crate::api::ok(info)
}

// ============================================================================
// Stone-local: POST /api/v1/stone/banks/{moniker}/pin
// ============================================================================

/// Pin the Primary role for a bank on this stone.
pub async fn pin_bank(
    State(state): State<Moss>,
    Path(moniker): Path<String>,
) -> crate::api::ApiResult<PinResponse> {
    let moniker = moniker.trim().to_string();
    if moniker.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "EMPTY_NAME",
            "Bank name is required",
        ));
    }

    let events = bank_aggregate::pin(&moniker, &state.current.storage.volumes, |path: PathBuf| {
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

    crate::api::ok(PinResponse {
        name: moniker.clone(),
        pinned: true,
        message: format!("Primary role for '{}' pinned to this stone", moniker),
    })
}

// ============================================================================
// Stone-local: POST /api/v1/stone/banks/{moniker}/unpin
// ============================================================================

/// Release the Primary role pin for a bank on this stone.
pub async fn unpin_bank(
    State(state): State<Moss>,
    Path(moniker): Path<String>,
) -> crate::api::ApiResult<PinResponse> {
    let moniker = moniker.trim().to_string();
    if moniker.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "EMPTY_NAME",
            "Bank name is required",
        ));
    }

    let events =
        bank_aggregate::unpin(&moniker, &state.current.storage.volumes, |path: PathBuf| {
            std::sync::Arc::new(ContentStore::new(path, None))
        })
        .await
        .unwrap_or_else(|e| {
            debug!(name = %moniker, error = %e, "Unpin no-op — bank not found or not pinned");
            vec![]
        });

    for event in &events {
        state.emit_storage_changed(event.clone()).await;
    }
    state.current.storage.coordination.nudge.notify_one();

    crate::api::ok(PinResponse {
        name: moniker.clone(),
        pinned: false,
        message: format!("Primary role for '{}' is now unpinned", moniker),
    })
}

// ============================================================================
// Garden-wide: GET /api/v1/garden/banks
// ============================================================================

/// List all banks visible across the garden.
///
/// Aggregates local managed storages with remote registry beacons.
pub async fn list_garden_banks(
    State(state): State<Moss>,
) -> crate::api::ApiResult<Vec<GardenBankSummary>> {
    let mut by_name: std::collections::HashMap<String, GardenBankSummary> =
        std::collections::HashMap::new();

    // Local banks
    for bank in bank_aggregate::local_banks(&state.current.storage.volumes).await {
        let entry = by_name
            .entry(bank.name.clone())
            .or_insert_with(|| GardenBankSummary {
                name: bank.name.clone(),
                replica_count: 0,
                primary_stone: None,
                roles: bank.roles.clone(),
            });
        entry.replica_count += bank.local_volume_count;
        if bank.has_local_primary() {
            entry.primary_stone = Some(state.current.stone.name.clone());
        }
    }

    // Remote banks from the Tool aggregate's registry
    for storage_entry in state.tool.storage_entries().await {
        if storage_entry.tool.stone.id == state.current.stone.id {
            continue;
        }
        let sm = storage_entry.tool.storage.as_ref();
        let name = &storage_entry.tool.tool.name;
        let entry = by_name
            .entry(name.clone())
            .or_insert_with(|| GardenBankSummary {
                name: name.clone(),
                replica_count: 0,
                primary_stone: None,
                roles: sm.map(|s| s.roles.clone()).unwrap_or_default(),
            });
        entry.replica_count += 1;
        if sm.is_some_and(|s| s.role.as_deref() == Some("Primary")) && entry.primary_stone.is_none()
        {
            entry.primary_stone = Some(storage_entry.tool.stone.name.clone());
        }
    }

    let banks: Vec<GardenBankSummary> = by_name.into_values().collect();
    crate::api::ok(banks)
}

// ============================================================================
// Garden-wide: GET /api/v1/garden/banks/{moniker}
// ============================================================================

/// Get bank details including volume locations across the garden.
pub async fn get_garden_bank(
    State(state): State<Moss>,
    Path(moniker): Path<String>,
) -> crate::api::ApiResult<GardenBankDetail> {
    let volumes = build_bank_volumes(&moniker, &state).await;

    if volumes.is_empty() {
        return Err(err(
            StatusCode::NOT_FOUND,
            "BANK_NOT_FOUND",
            &format!("Bank '{}' not found in the garden", moniker),
        ));
    }

    let primary_stone = volumes
        .iter()
        .find(|v| v.role == StorageRole::Primary)
        .map(|v| v.stone_name.clone());
    let roles = volumes.first().map(|v| v.roles.clone()).unwrap_or_default();

    crate::api::ok(GardenBankDetail {
        name: moniker,
        replica_count: volumes.len(),
        primary_stone,
        roles,
        volumes,
    })
}

/// Build the list of volumes for a bank across the garden.
async fn build_bank_volumes(moniker: &str, state: &Moss) -> Vec<BankVolume> {
    let mut volumes = Vec::new();

    // Local volumes
    let local_vols =
        bank_aggregate::volumes_for_bank(moniker, &state.current.storage.volumes).await;
    for vol in &local_vols {
        if let Some(mgmt) = vol.management() {
            volumes.push(BankVolume {
                stone_id: state.current.stone.id.clone(),
                stone_name: state.current.stone.name.clone(),
                volume_id: mgmt.id.clone(),
                role: mgmt.role,
                endpoint: format!(
                    "http://{}:{}",
                    state.current.stone.name,
                    garden_common::constants::MOSS_HTTP
                ),
                roles: mgmt.roles.clone(),
            });
        }
    }

    // Remote volumes from the Tool aggregate's registry
    for storage_entry in state.tool.storage_entries().await {
        if storage_entry.tool.stone.id == state.current.stone.id {
            continue;
        }
        if storage_entry.tool.tool.name != moniker {
            continue;
        }
        let sm = storage_entry.tool.storage.as_ref();
        volumes.push(BankVolume {
            stone_id: storage_entry.tool.stone.id.clone(),
            stone_name: storage_entry.tool.stone.name.clone(),
            volume_id: sm.map(|s| s.replica_set_id.clone()).unwrap_or_default(),
            role: if sm.is_some_and(|s| s.role.as_deref() == Some("Primary")) {
                StorageRole::Primary
            } else {
                StorageRole::Replica
            },
            endpoint: storage_entry.tool.stone.endpoint.clone(),
            roles: sm.map(|s| s.roles.clone()).unwrap_or_default(),
        });
    }

    volumes
}

// ============================================================================
// Garden-wide: GET /api/v1/garden/banks/{moniker}/volumes
// ============================================================================

/// List individual volumes for a bank across the garden.
pub async fn list_garden_bank_volumes(
    State(state): State<Moss>,
    Path(moniker): Path<String>,
) -> crate::api::ApiResult<Vec<BankVolume>> {
    let volumes = build_bank_volumes(&moniker, &state).await;
    if volumes.is_empty() {
        return Err(err(
            StatusCode::NOT_FOUND,
            "BANK_NOT_FOUND",
            &format!("Bank '{}' not found in the garden", moniker),
        ));
    }
    crate::api::ok(volumes)
}
