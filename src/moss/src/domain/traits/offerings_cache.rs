//! Offerings cache persistence trait.

use anyhow::Result;
use async_trait::async_trait;

use crate::domain::offerings::OfferingsIndex;

/// Persistence operations for the offerings cache.
///
/// The offerings index is a compiled snapshot of available offerings
/// with compatibility and metadata pre-resolved. Domain code needs to
/// load/save this cache without depending on infra file I/O.
#[async_trait]
pub trait OfferingsCachePersistence: Send + Sync {
    /// Load the cached offerings index from persistent storage.
    async fn load_cache(&self) -> Result<Option<OfferingsIndex>>;

    /// Save the offerings index to persistent storage.
    async fn save_cache(&self, cache: &OfferingsIndex) -> Result<()>;
}
