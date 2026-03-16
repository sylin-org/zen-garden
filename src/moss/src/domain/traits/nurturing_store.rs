//! Nurturing store operations trait.
//!
//! Abstracts the A/B backup slot management for offerings.
//! The concrete `NurturingStore` in infra owns Docker and HarvestStore
//! internally — domain/API callers see only domain types.

use anyhow::Result;
use async_trait::async_trait;

use crate::domain::harvest::HarvestManifest;
use crate::domain::nurturing::{
    NurturingIndex, NurturingResult, NurturingSlot, OfferingSlots, RemoteNurturingIndex,
    ReplicationResult,
};
use crate::domain::traits::ContentStoreOps;
use garden_common::storage::MemoriesOfferingManifest;

/// A/B backup slot management operations.
///
/// Callers interact with nurturing through this trait without
/// depending on the concrete `NurturingStore` in infra. Docker
/// and HarvestStore are internalized in the implementor.
#[async_trait]
pub trait NurturingStoreOps: Send + Sync {
    /// Load the nurturing index from disk.
    async fn load_index(&self) -> Result<NurturingIndex>;

    /// Get the A/B slots for a specific offering.
    async fn get_offering_slots(&self, offering_id: &str) -> Result<Option<OfferingSlots>>;

    /// Create a local snapshot (A/B backup) for an offering.
    async fn create_snapshot(
        &self,
        offering_id: &str,
        offering_name: &str,
        stone_id: &str,
        commit_image: bool,
    ) -> Result<NurturingResult>;

    /// Restore an offering from a local snapshot.
    async fn restore_snapshot(
        &self,
        offering_id: &str,
        slot: Option<NurturingSlot>,
    ) -> Result<HarvestManifest>;

    /// Delete all nurturing data for an offering.
    async fn delete_offering(&self, offering_id: &str) -> Result<()>;

    /// Replicate a snapshot to a seed bank.
    async fn replicate_to_seed_bank(
        &self,
        offering_id: &str,
        store: &dyn ContentStoreOps,
        seed_bank_id: &str,
        storage_name: &str,
        stone_id: &str,
        hydration_manifest: Option<MemoriesOfferingManifest>,
    ) -> Result<ReplicationResult>;

    /// List remote snapshots on a seed bank.
    async fn list_remote_snapshots(
        &self,
        store: &dyn ContentStoreOps,
        seed_bank_id: &str,
    ) -> Result<RemoteNurturingIndex>;

    /// Restore from a remote seed bank snapshot.
    async fn restore_from_seed_bank(
        &self,
        store: &dyn ContentStoreOps,
        seed_bank_id: &str,
        offering_id: &str,
        harvest_id: Option<&str>,
    ) -> Result<HarvestManifest>;
}
