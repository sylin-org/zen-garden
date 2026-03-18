//! Nurturing API - Local A/B backup management and seed bank replication
//!
//! Provides endpoints for managing local A/B backup slots:
//! - Create nurturing snapshots before updates
//! - List available snapshots per offering
//! - Restore from specific slot (current or previous)
//! - Replicate snapshots to seed banks (remote backup)
//! - Trigger full nurturing workflow (for timers)
//!
//! ## Local Endpoints
//! - GET  /api/v1/stone/nurturing           - List all offerings with nurturing slots
//! - GET  /api/v1/stone/nurturing/:offering - Get slots for specific offering
//! - POST /api/v1/stone/nurturing/:offering - Create new snapshot (A/B rotation)
//! - POST /api/v1/stone/nurturing/:offering/restore - Restore from snapshot
//! - DELETE /api/v1/stone/nurturing/:offering - Delete all snapshots for offering
//!
//! ## Trigger Endpoints (Timer Integration)
//! - POST /api/v1/nurturing/:offering/trigger - Trigger full workflow (local + replicate)
//! - POST /api/v1/nurturing/trigger-all - Trigger workflow for all offerings
//!
//! ## Remote Endpoints (Seed Bank Integration)
//! - POST /api/v1/stone/nurturing/:offering/replicate - Replicate to seed bank
//! - GET  /api/v1/stone/nurturing/remote/:seed_bank - List remote snapshots
//! - POST /api/v1/stone/nurturing/:offering/restore-remote - Restore from seed bank

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::domain::nurturing::{
    build_memories_manifest, NurturingIndex, NurturingResult, NurturingSlot, OfferingSlots,
    RemoteNurturingIndex, ReplicationResult,
};
use crate::tasks::{trigger_all_nurturing, trigger_nurturing, NurturingWorkflowResult};
use crate::AppState;
use garden_common::api_utils::ApiErrorResponse;
use garden_common::offerings::OfferingFqn;

/// Request for creating a snapshot
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CreateSnapshotRequest {
    /// Whether to commit the container image (default: true for stateful)
    #[serde(default)]
    pub commit_image: Option<bool>,
}

/// Request for restoring from a snapshot
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RestoreRequest {
    /// Which slot to restore from ("A", "B", or omit for current)
    #[serde(default)]
    pub slot: Option<String>,
}

// ============================================================================
// GET /api/v1/nurturing - List all offerings with nurturing slots
// ============================================================================

/// Returns the full NurturingIndex - offerings can be filtered client-side
pub async fn list_nurturing(
    State(state): State<AppState>,
) -> crate::api::ApiResult<NurturingIndex> {
    let index = state.orchestration.nurturing.store.load_index().await.map_err(|e| {
        crate::internal(
            "NURTURING_ERROR",
            format!("Failed to load nurturing index: {}", e),
        )
    })?;

    crate::api::ok(index)
}

// ============================================================================
// GET /api/v1/nurturing/:offering - Get slots for specific offering
// ============================================================================

pub async fn get_offering_slots(
    State(state): State<AppState>,
    Path(offering): Path<String>,
) -> crate::api::ApiResult<Option<OfferingSlots>> {
    let offering_lookup =
        normalize_offering_for_lookup(&offering).unwrap_or_else(|| offering.clone());
    // Look up the offering by name to get the offering_id
    let offering_id = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == offering_lookup || o.offering_id == offering)
            .map(|o| o.offering_id.clone())
    };

    let slots = if let Some(id) = offering_id {
        state
            .orchestration.nurturing.store
            .get_offering_slots(&id)
            .await
            .map_err(|e| {
                crate::internal(
                    "NURTURING_ERROR",
                    format!("Failed to get nurturing slots: {}", e),
                )
            })?
    } else {
        // Try looking up directly by offering_id
        state
            .orchestration.nurturing.store
            .get_offering_slots(&offering)
            .await
            .map_err(|e| {
                crate::internal(
                    "NURTURING_ERROR",
                    format!("Failed to get nurturing slots: {}", e),
                )
            })?
    };

    crate::api::ok(slots)
}

// ============================================================================
// POST /api/v1/nurturing/:offering - Create new snapshot (A/B rotation)
// ============================================================================

pub async fn create_snapshot(
    State(state): State<AppState>,
    Path(offering): Path<String>,
    Json(request): Json<CreateSnapshotRequest>,
) -> crate::api::ApiResult<NurturingResult> {
    let offering_lookup =
        normalize_offering_for_lookup(&offering).unwrap_or_else(|| offering.clone());
    // Look up the offering to get offering_id
    let (offering_id, offering_name) = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == offering_lookup)
            .map(|o| (o.offering_id.clone(), o.name.to_string()))
            .ok_or_else(|| {
                crate::not_found(
                    "OFFERING_NOT_FOUND",
                    format!("Offering '{}' not found in registry", offering_lookup),
                )
            })?
    };

    if offering_id.is_empty() {
        return Err(crate::bad_request(
            "NO_OFFERING_ID",
            format!(
                "Offering '{}' has no offering_id - please restart moss to migrate",
                offering_lookup
            ),
        ));
    }

    // Determine whether to commit the image (default: true)
    let commit_image = request.commit_image.unwrap_or(true);

    tracing::info!(
        offering = %offering_name,
        offering_id = %offering_id,
        commit_image,
        "Creating nurturing snapshot"
    );

    let result = state
        .orchestration.nurturing.store
        .create_snapshot(
            &offering_id,
            &offering_name,
            &state.current.stone.id,
            commit_image,
        )
        .await
        .map_err(|e| {
            crate::internal(
                "SNAPSHOT_FAILED",
                format!("Failed to create nurturing snapshot: {}", e),
            )
        })?;

    crate::api::ok(result)
}

// ============================================================================
// POST /api/v1/nurturing/:offering/restore - Restore from snapshot
// ============================================================================

pub async fn restore_snapshot(
    State(state): State<AppState>,
    Path(offering): Path<String>,
    Json(request): Json<RestoreRequest>,
) -> crate::api::ApiResult<crate::domain::HarvestManifest>
{
    let offering_lookup =
        normalize_offering_for_lookup(&offering).unwrap_or_else(|| offering.clone());
    // Look up the offering to get offering_id
    let offering_id = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == offering_lookup)
            .map(|o| o.offering_id.clone())
            .ok_or_else(|| {
                crate::not_found(
                    "OFFERING_NOT_FOUND",
                    format!("Offering '{}' not found in registry", offering_lookup),
                )
            })?
    };

    // Parse slot if specified
    let slot = match request.slot.as_deref() {
        Some("A") | Some("a") => Some(NurturingSlot::A),
        Some("B") | Some("b") => Some(NurturingSlot::B),
        Some(other) => {
            return Err(crate::bad_request(
                "INVALID_SLOT",
                format!("Invalid slot '{}' - must be 'A' or 'B'", other),
            ));
        }
        None => None, // Use current
    };

    tracing::info!(
        offering = %offering_lookup,
        offering_id = %offering_id,
        slot = ?slot,
        "Restoring from nurturing snapshot"
    );

    // Stop the service first
    if let Err(e) = state
        .platform.docker
        .stop_service(&offering_lookup, Some(&state.console))
        .await
    {
        tracing::warn!(error = ?e, "Failed to stop service before restore (continuing anyway)");
    }

    // Restore the snapshot
    let manifest = state
        .orchestration.nurturing.store
        .restore_snapshot(&offering_id, slot)
        .await
        .map_err(|e| {
            crate::internal(
                "RESTORE_FAILED",
                format!("Failed to restore snapshot: {}", e),
            )
        })?;

    // Start the service
    if let Err(e) = state
        .platform.docker
        .start_service(&offering_lookup, Some(&state.console))
        .await
    {
        return Err(crate::internal(
            "START_FAILED",
            format!("Restored data but failed to start service: {}", e),
        ));
    }

    crate::api::ok(manifest)
}

// ============================================================================
// DELETE /api/v1/nurturing/:offering - Delete all snapshots for offering
// ============================================================================

pub async fn delete_nurturing(
    State(state): State<AppState>,
    Path(offering): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiErrorResponse>)> {
    let offering_lookup =
        normalize_offering_for_lookup(&offering).unwrap_or_else(|| offering.clone());
    // Look up the offering to get offering_id
    let offering_id = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == offering_lookup || o.offering_id == offering)
            .map(|o| o.offering_id.clone())
    };

    let offering_id = offering_id.unwrap_or(offering.clone());

    state
        .orchestration.nurturing.store
        .delete_offering(&offering_id)
        .await
        .map_err(|e| {
            crate::internal(
                "DELETE_FAILED",
                format!("Failed to delete nurturing data: {}", e),
            )
        })?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "deleted",
            "offering_id": offering_id,
        })),
    ))
}

// ============================================================================
// Remote Seed Bank Endpoints
// ============================================================================

/// Request for replicating to a seed bank
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ReplicateRequest {
    /// Seed bank name or ID to replicate to
    pub storage: String,
}

/// Request for restoring from a remote seed bank
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RestoreRemoteRequest {
    /// Seed bank name or ID
    pub storage: String,
    /// Optional specific harvest ID (defaults to latest)
    #[serde(default)]
    pub harvest_id: Option<String>,
}

// ============================================================================
// POST /api/v1/stone/nurturing/:offering/replicate - Replicate to seed bank
// ============================================================================

pub async fn replicate_to_seed_bank(
    State(state): State<AppState>,
    Path(offering): Path<String>,
    Json(request): Json<ReplicateRequest>,
) -> crate::api::ApiResult<ReplicationResult> {
    let offering_lookup =
        normalize_offering_for_lookup(&offering).unwrap_or_else(|| offering.clone());
    // Look up the offering to get offering_id
    let offering_entry = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == offering_lookup || o.offering_id == offering)
            .cloned()
            .ok_or_else(|| {
                crate::not_found(
                    "OFFERING_NOT_FOUND",
                    format!("Offering '{}' not found in registry", offering_lookup),
                )
            })?
    };
    let offering_id = offering_entry.offering_id.clone();

    // Find the seed bank
    let seed_bank = find_seed_bank(&state.current.storage.volumes, &request.storage).await.map_err(|e| {
        crate::not_found(
            "SEED_BANK_NOT_FOUND",
            format!("Seed bank '{}' not found: {}", request.storage, e),
        )
    })?;

    tracing::info!(
        offering_id,
        seed_bank = %seed_bank.name,
        "Replicating nurturing snapshot to seed bank"
    );

    let manifest = state
        .manifest_registry
        .get_offering(&offering_entry.offering)
        .cloned();
    let hydration_manifest = build_memories_manifest(
        &offering_entry,
        manifest,
        &state.current.stone.id,
        &state.current.stone.name,
    );

    let store = crate::infra::storage::ContentStore::new_public(&seed_bank.mount_path);

    let result = state
        .orchestration.nurturing.store
        .replicate_to_seed_bank(
            &offering_id,
            &store,
            &seed_bank.id,
            &seed_bank.name,
            &state.current.stone.id,
            Some(hydration_manifest),
        )
        .await
        .map_err(|e| {
            crate::internal(
                "REPLICATION_FAILED",
                format!("Failed to replicate to seed bank: {}", e),
            )
        })?;

    crate::api::ok(result)
}

// ============================================================================
// GET /api/v1/stone/nurturing/remote/:seed_bank - List remote snapshots
// ============================================================================

pub async fn list_remote_snapshots(
    State(state): State<AppState>,
    Path(storage_name): Path<String>,
) -> crate::api::ApiResult<RemoteNurturingIndex> {
    // Find the seed bank
    let seed_bank = find_seed_bank(&state.current.storage.volumes, &storage_name).await.map_err(|e| {
        crate::not_found(
            "SEED_BANK_NOT_FOUND",
            format!("Seed bank '{}' not found: {}", storage_name, e),
        )
    })?;

    let store = crate::infra::storage::ContentStore::new_public(&seed_bank.mount_path);

    let index = state
        .orchestration.nurturing.store
        .list_remote_snapshots(&store, &seed_bank.id)
        .await
        .map_err(|e| {
            crate::internal(
                "REMOTE_LIST_FAILED",
                format!("Failed to list remote snapshots: {}", e),
            )
        })?;

    crate::api::ok(index)
}

// ============================================================================
// POST /api/v1/stone/nurturing/:offering/restore-remote - Restore from seed bank
// ============================================================================

pub async fn restore_from_seed_bank(
    State(state): State<AppState>,
    Path(offering): Path<String>,
    Json(request): Json<RestoreRemoteRequest>,
) -> crate::api::ApiResult<crate::domain::HarvestManifest>
{
    let offering_lookup =
        normalize_offering_for_lookup(&offering).unwrap_or_else(|| offering.clone());
    // Look up the offering to get offering_id
    let (offering_id, offering_name) = {
        let offerings = state.offerings.read().await;
        offerings
            .iter()
            .find(|o| o.name.to_string() == offering_lookup || o.offering_id == offering)
            .map(|o| (o.offering_id.clone(), o.name.to_string()))
            .ok_or_else(|| {
                crate::not_found(
                    "OFFERING_NOT_FOUND",
                    format!("Offering '{}' not found in registry", offering_lookup),
                )
            })?
    };

    // Find the seed bank
    let seed_bank = find_seed_bank(&state.current.storage.volumes, &request.storage).await.map_err(|e| {
        crate::not_found(
            "SEED_BANK_NOT_FOUND",
            format!("Seed bank '{}' not found: {}", request.storage, e),
        )
    })?;

    tracing::info!(
        offering_id,
        offering_name,
        seed_bank = %seed_bank.name,
        harvest_id = ?request.harvest_id,
        "Restoring from remote seed bank snapshot"
    );

    // Stop the service first
    if let Err(e) = state
        .platform.docker
        .stop_service(&offering_name, Some(&state.console))
        .await
    {
        tracing::warn!(error = ?e, "Failed to stop service before restore (continuing anyway)");
    }

    // Restore from seed bank
    let store = crate::infra::storage::ContentStore::new_public(&seed_bank.mount_path);
    let manifest = state
        .orchestration.nurturing.store
        .restore_from_seed_bank(
            &store,
            &seed_bank.id,
            &offering_id,
            request.harvest_id.as_deref(),
        )
        .await
        .map_err(|e| {
            crate::internal(
                "REMOTE_RESTORE_FAILED",
                format!("Failed to restore from seed bank: {}", e),
            )
        })?;

    // Start the service
    if let Err(e) = state
        .platform.docker
        .start_service(&offering_name, Some(&state.console))
        .await
    {
        return Err(crate::internal(
            "START_FAILED",
            format!("Restored data but failed to start service: {}", e),
        ));
    }

    crate::api::ok(manifest)
}

fn normalize_offering_for_lookup(offering: &str) -> Option<String> {
    OfferingFqn::parse(offering).ok().map(|fqn| fqn.fqn())
}

// ============================================================================
// Timer Trigger Endpoints
// ============================================================================

// POST /api/v1/nurturing/:offering/trigger - Trigger full nurturing workflow
//
// This endpoint is called by system timers (systemd/Task Scheduler) to initiate
// the complete nurturing workflow: local snapshot + seed bank replication.

/// Trigger the full nurturing workflow for an offering
///
/// Called by system timers to perform automated backups.
/// Workflow: local A/B snapshot → find seed banks → replicate with failover
pub async fn trigger_offering_nurturing(
    State(state): State<AppState>,
    Path(offering): Path<String>,
) -> crate::api::ApiResult<NurturingWorkflowResult> {
    let offering_lookup =
        normalize_offering_for_lookup(&offering).unwrap_or_else(|| offering.clone());
    tracing::info!(
        offering = %offering_lookup,
        "Nurturing trigger received"
    );

    let result = trigger_nurturing(&state, &offering_lookup)
        .await
        .map_err(|e| {
            crate::internal(
                "NURTURING_WORKFLOW_FAILED",
                format!("Nurturing workflow failed: {}", e),
            )
        })?;

    crate::api::ok(result)
}

// POST /api/v1/nurturing/trigger-all - Trigger workflow for all running offerings

/// Trigger nurturing for all running offerings
///
/// Useful for manual batch operations or testing.
pub async fn trigger_all_offerings_nurturing(
    State(state): State<AppState>,
) -> crate::api::ApiResult<Vec<NurturingWorkflowResult>> {
    tracing::info!("Nurturing trigger-all received");

    let results = trigger_all_nurturing(&state).await;

    crate::api::ok(results)
}

// ============================================================================
// Helper functions
// ============================================================================

/// Find a seed bank by name or ID from the Volumes domain.
async fn find_seed_bank(
    volumes: &crate::domain::storage::Volumes,
    name_or_id: &str,
) -> anyhow::Result<garden_common::storage::StorageInfo> {
    let map = volumes.read().await;
    map.values()
        .find(|v| {
            v.management.as_ref().is_some_and(|m| m.name == name_or_id || m.id == name_or_id)
        })
        .and_then(|v| v.to_storage_info())
        .ok_or_else(|| anyhow::anyhow!("Seed bank not found: {}", name_or_id))
}
