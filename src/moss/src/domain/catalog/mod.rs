//! Catalog bounded context — Book V of [ARCH-0017].
//!
//! ## Chapter 3 state
//!
//! `Catalog` is a full DDD aggregate with typed commands (`load`,
//! `rebuild`), typed queries (`get_manifest`, `get_compiled`,
//! `compiled_snapshot`, `stats`, `is_loaded`, `manifest_count`,
//! `find_hw_manifest`, `manifests`), a `CatalogChanged` internal event
//! stream with two kinds (`Loaded`, `Rebuilt`), `Arc<Metrics>`
//! injection, a `CatalogCache` persistence port, and the first typed
//! `CatalogError` enum in the epic.
//!
//! During Ch3 the existing free functions in [`legacy`]
//! (`ensure_offerings_index`, `get_compiled_offering`,
//! `rebuild_offerings_index`) remain the coordination layer between the
//! legacy `AppState::manifest_registry` / `AppState::offerings_index`
//! fields. Ch4 migrates their callers to the aggregate's typed
//! commands. Ch5 deletes the legacy module.
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
pub mod fingerprint;
pub mod index;
pub mod legacy;
mod state;

#[cfg(test)]
mod tests;

pub use aggregate::{Catalog, CatalogStats};
pub use cache::{CatalogCache, FileCatalogCache};
pub use entry::CompiledOffering;
pub use error::CatalogError;
pub use event::{CatalogChanged, ChangeKind as CatalogChangeKind, LoadSource};
pub use fingerprint::{current_capabilities_hash, manifests_hash, moss_version_string};
pub use index::{OfferingsFingerprint, OfferingsIndex};
pub use legacy::{ensure_offerings_index, get_compiled_offering, rebuild_offerings_index};
