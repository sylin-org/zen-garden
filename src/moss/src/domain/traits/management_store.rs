//! Management store operations trait.
//!
//! Abstracts the I/O operations that managed storage volumes need:
//! pin persistence and last-known-good snapshots.
//!
//! The concrete `ContentStore` in infra implements this trait.

use anyhow::Result;
use std::future::Future;

/// Storage management I/O operations.
///
/// Used by `Management` (domain) for pin persistence and LKG snapshots
/// without depending on the concrete `ContentStore` in infra.
pub trait ManagementStoreOps: Send + Sync {
    /// Read persisted pin_id from the storage device, if any.
    fn read_pin(&self) -> impl Future<Output = Option<String>> + Send;

    /// Persist a pin_id to the storage device.
    fn write_pin(&self, pin_id: &str) -> impl Future<Output = Result<()>> + Send;

    /// Delete the persisted pin file.
    fn delete_pin(&self) -> impl Future<Output = Result<()>> + Send;

    /// Snapshot critical files to `last-known-good/` for resilience.
    fn snapshot_lkg(&self) -> impl Future<Output = Result<()>> + Send;
}
