//! Harvest (backup/restore) operations trait.

use crate::domain::harvest::HarvestManifest;
use anyhow::Result;
use std::future::Future;

/// Backup and restore operations for offering state.
///
/// Used by ceremony phases (collect/water) to create and restore
/// harvests before/after nourishment.
pub trait HarvestOps: Send + Sync {
    /// Create a harvest for an offering.
    ///
    /// Captures the current state (container image + volumes) so we
    /// can roll back if the update fails.
    fn create_harvest(
        &self,
        offering: &str,
        source_stone: &str,
        commit_image: bool,
    ) -> impl Future<Output = Result<HarvestManifest>> + Send;

    /// Restore an offering from a previous harvest.
    fn restore_harvest(&self, harvest_id: &str) -> impl Future<Output = Result<()>> + Send;
}
