//! Nurturing store operations trait.
//!
//! Abstracts the A/B backup slot management for offerings.
//! The concrete `NurturingStore` in infra owns Docker and HarvestStore
//! internally — domain/API callers see only domain types.

use anyhow::Result;
use std::future::Future;

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
pub trait NurturingStoreOps: Send + Sync {
    /// Load the nurturing index from disk.
    fn load_index(&self) -> impl Future<Output = Result<NurturingIndex>> + Send;

    /// Get the A/B slots for a specific offering.
    fn get_offering_slots(
        &self,
        offering_id: &str,
    ) -> impl Future<Output = Result<Option<OfferingSlots>>> + Send;

    /// Create a local snapshot (A/B backup) for an offering.
    fn create_snapshot(
        &self,
        offering_id: &str,
        offering_name: &str,
        stone_id: &str,
        commit_image: bool,
    ) -> impl Future<Output = Result<NurturingResult>> + Send;

    /// Restore an offering from a local snapshot.
    fn restore_snapshot(
        &self,
        offering_id: &str,
        slot: Option<NurturingSlot>,
    ) -> impl Future<Output = Result<HarvestManifest>> + Send;

    /// Delete all nurturing data for an offering.
    fn delete_offering(&self, offering_id: &str) -> impl Future<Output = Result<()>> + Send;

    /// Replicate a snapshot to a seed bank.
    fn replicate_to_seed_bank(
        &self,
        offering_id: &str,
        store: &(impl ContentStoreOps + ?Sized),
        seed_bank_id: &str,
        storage_name: &str,
        stone_id: &str,
        hydration_manifest: Option<MemoriesOfferingManifest>,
    ) -> impl Future<Output = Result<ReplicationResult>> + Send;

    /// List remote snapshots on a seed bank.
    fn list_remote_snapshots(
        &self,
        store: &(impl ContentStoreOps + ?Sized),
        seed_bank_id: &str,
    ) -> impl Future<Output = Result<RemoteNurturingIndex>> + Send;

    /// Restore from a remote seed bank snapshot.
    fn restore_from_seed_bank(
        &self,
        store: &(impl ContentStoreOps + ?Sized),
        seed_bank_id: &str,
        offering_id: &str,
        harvest_id: Option<&str>,
    ) -> impl Future<Output = Result<HarvestManifest>> + Send;
}
