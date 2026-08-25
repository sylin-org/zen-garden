//! Catalog bounded context — Book V of [ARCH-0017].
//!
//! `Catalog` is a full DDD aggregate with typed commands (`load`,
//! `rebuild`), typed queries (`get_manifest`, `get_compiled`,
//! `compiled_snapshot`, `stats`, `is_loaded`, `manifest_count`,
//! `find_hw_manifest`, `manifests`), a `CatalogChanged` internal event
//! stream with two kinds (`Loaded`, `Rebuilt`), `Arc<Metrics>`
//! injection, a `CatalogCache` persistence port, and the first typed
//! `CatalogError` enum in the epic.
//!
//! Type names that cross the moss crate boundary (`CompiledOffering`,
//! `OfferingsIndex`, `OfferingsFingerprint`) are **preserved** — they
//! appear in 8 non-module files and in the disk-cache JSON schema,
//! and renaming would cascade without architectural benefit.
//!
//! [ARCH-0017]: ../../../../docs/decisions/ARCH-0017-ddd-monolith-epic.md

pub mod aggregate;
pub mod cache;
pub mod entry;
pub mod error;
pub mod event;
pub(super) mod fingerprint;
pub mod index;
mod state;

#[cfg(test)]
mod tests;

pub use aggregate::{Catalog, CatalogStats};
pub use cache::{CatalogCache, FileCatalogCache};
pub use entry::CompiledOffering;
pub use error::CatalogError;
pub use event::{CatalogChanged, ChangeKind as CatalogChangeKind, LoadSource};
pub use index::{OfferingsFingerprint, OfferingsIndex};
