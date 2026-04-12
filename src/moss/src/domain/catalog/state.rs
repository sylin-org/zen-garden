//! `CatalogState` — the mutable state owned by the `Catalog`
//! aggregate.
//!
//! Holds an `Option<OfferingsIndex>` that starts `None` and is
//! populated by `load` or `rebuild`. The struct is thin — its purpose
//! is symmetry with other Book aggregates (Jobs, Topology) so the
//! internal representation is a named type rather than a raw
//! `Option<OfferingsIndex>` behind a lock.

use super::index::OfferingsIndex;

/// Compiled catalog snapshot state.
///
/// `None` means the catalog has not been loaded yet.
pub(super) struct CatalogState {
    pub(super) index: Option<OfferingsIndex>,
}

impl CatalogState {
    /// Create an empty state — catalog not yet loaded.
    pub(super) fn empty() -> Self {
        Self { index: None }
    }
}
