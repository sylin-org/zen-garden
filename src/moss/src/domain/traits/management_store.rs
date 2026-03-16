//! Management store operations trait.
//!
//! Abstracts the I/O operations that managed storage volumes need:
//! pin persistence and last-known-good snapshots.
//!
//! The concrete `ContentStore` in infra implements this trait.

use anyhow::Result;
use async_trait::async_trait;

/// Storage management I/O operations.
///
/// Used by `Management` (domain) for pin persistence and LKG snapshots
/// without depending on the concrete `ContentStore` in infra.
#[async_trait]
pub trait ManagementStoreOps: Send + Sync {
    /// Read persisted pin_id from the storage device, if any.
    async fn read_pin(&self) -> Option<String>;

    /// Persist a pin_id to the storage device.
    async fn write_pin(&self, pin_id: &str) -> Result<()>;

    /// Delete the persisted pin file.
    async fn delete_pin(&self) -> Result<()>;

    /// Snapshot critical files to `last-known-good/` for resilience.
    async fn snapshot_lkg(&self) -> Result<()>;
}
