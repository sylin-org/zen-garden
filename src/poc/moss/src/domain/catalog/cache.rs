//! `CatalogCache` port and its file-system adapter.
//!
//! Moved from `domain/traits/offerings_cache.rs` (trait) and
//! `infra/persistence.rs` (adapter) in Ch2 of ARCH-0022 (Book V of
//! ARCH-0017). The old names `OfferingsCachePersistence` (trait) and
//! `OsOfferingsCache` (adapter) are replaced with `CatalogCache` and
//! `FileCatalogCache` respectively — every call site is migrated in
//! the same commit so no compatibility shim is needed.
//!
//! The `FileCatalogCache` adapter delegates to the existing
//! `infra::persistence::{load_offerings_cache, save_offerings_cache}`
//! free functions which own the on-disk layout
//! (`{config_dir}/offerings.cache.json`) and the atomic-write
//! invariants. Ch5 may absorb the delegation when
//! `domain/offerings/catalog.rs` is deleted; for Ch2 the adapter is
//! intentionally thin to keep the pure-move spirit of the chapter.

use anyhow::Result;
use std::future::Future;
use std::pin::Pin;

use super::index::OfferingsIndex;

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Persistence port for the compiled catalog snapshot.
///
/// The Catalog aggregate holds an `Arc<dyn CatalogCache>` injected at
/// construction time. `load` is called on startup by the `load()`
/// command and returns `None` when no cache exists (first boot).
/// `save` is called after every successful rebuild so subsequent cold
/// starts can skip manifest compilation when the fingerprint matches.
pub trait CatalogCache: Send + Sync {
    /// Load the cached offerings index from persistent storage.
    /// Returns `Ok(None)` when no cache exists.
    fn load(&self) -> BoxFut<'_, Result<Option<OfferingsIndex>>>;

    /// Save the offerings index to persistent storage. Atomic write.
    fn save<'a>(&'a self, cache: &'a OfferingsIndex) -> BoxFut<'a, Result<()>>;
}

/// File-system adapter — reads and writes
/// `{config_dir}/offerings.cache.json` via the existing
/// `infra::persistence` helpers.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileCatalogCache;

impl CatalogCache for FileCatalogCache {
    fn load(&self) -> BoxFut<'_, Result<Option<OfferingsIndex>>> {
        Box::pin(crate::infra::persistence::load_offerings_cache())
    }

    fn save<'a>(&'a self, cache: &'a OfferingsIndex) -> BoxFut<'a, Result<()>> {
        Box::pin(crate::infra::persistence::save_offerings_cache(cache))
    }
}
