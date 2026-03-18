//! Nurturing domain — A/B local backup management
//!
//! Pure data types live in `garden_common::nurturing`.
//! This module re-exports them and adds moss-specific logic
//! (anything that depends on moss-only types like `HarvestManifest`).

// Re-export all shared types so existing `crate::domain::nurturing::X` paths keep working.
pub use garden_common::nurturing::{
    NurturingIndex, NurturingResult, NurturingSlot, NurturingSnapshot, OfferingSlots,
    RemoteNurturingIndex, RemoteSnapshot, ReplicationResult, DEFAULT_RETENTION_SLOTS,
};

use crate::domain::harvest::HarvestManifest;
use garden_common::manifests::Offering as OfferingManifest;
use garden_common::storage::MemoriesOfferingManifest;
use garden_common::types::Offering;

// ============================================================================
// Moss-specific constructors
// ============================================================================

/// Create a new snapshot from a harvest (moss-only — requires HarvestManifest)
pub fn snapshot_from_harvest(
    harvest: &HarvestManifest,
    slot: NurturingSlot,
    offering_id: &str,
    is_current: bool,
) -> NurturingSnapshot {
    NurturingSnapshot {
        slot,
        offering_id: offering_id.to_string(),
        offering_name: harvest.offering.clone(),
        harvest_id: harvest.id.clone(),
        created_at: harvest.created_at,
        size_bytes: harvest.total_size_bytes(),
        is_current,
    }
}

// ============================================================================
// Hydration Helpers
// ============================================================================

/// Build a hydration manifest for a runtime offering.
pub fn build_memories_manifest(
    offering: &Offering,
    manifest: Option<OfferingManifest>,
    stone_id: &str,
    stone_name: &str,
) -> MemoriesOfferingManifest {
    MemoriesOfferingManifest::from_offering(offering, manifest, stone_id, stone_name)
}
