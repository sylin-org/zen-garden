//! Offerings cache persistence trait.

use anyhow::Result;
use std::future::Future;

use crate::domain::offerings::OfferingsIndex;

/// Persistence operations for the offerings cache.
///
/// The offerings index is a compiled snapshot of available offerings
/// with compatibility and metadata pre-resolved. Domain code needs to
/// load/save this cache without depending on infra file I/O.
pub trait OfferingsCachePersistence: Send + Sync {
    /// Load the cached offerings index from persistent storage.
    fn load_cache(&self) -> impl Future<Output = Result<Option<OfferingsIndex>>> + Send;

    /// Save the offerings index to persistent storage.
    fn save_cache(&self, cache: &OfferingsIndex) -> impl Future<Output = Result<()>> + Send;
}
