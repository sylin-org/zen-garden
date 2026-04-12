//! Catalog bounded context — Book V of [ARCH-0017].
//!
//! ## Chapter 2 state
//!
//! This module holds the **type definitions** and the **persistence
//! port** for the compiled catalog. The `Catalog` aggregate itself
//! lands in Ch3. During Ch2 the existing free functions in
//! `domain/offerings/catalog.rs` (`ensure_offerings_index`,
//! `get_compiled_offering`, `rebuild_offerings_index`) remain the
//! coordination layer between `AppState::manifest_registry`,
//! `AppState::offerings_index`, and this module's types — they stay
//! in place as the strangler surface until Ch4 migrates their callers
//! to the aggregate's typed commands.
//!
//! ## Moved from Ch2
//!
//! - `CompiledOffering` → [`entry`]
//! - `OfferingsIndex` + `OfferingsFingerprint` → [`index`]
//! - `moss_version_string` + `manifests_hash` +
//!   `current_capabilities_hash` → [`fingerprint`]
//! - `OfferingsCachePersistence` trait → [`cache::CatalogCache`]
//!   (renamed)
//! - `OsOfferingsCache` struct → [`cache::FileCatalogCache`]
//!   (renamed and relocated from `infra/persistence.rs`)
//!
//! Type names that cross the moss crate boundary (`CompiledOffering`,
//! `OfferingsIndex`, `OfferingsFingerprint`) are **preserved** — they
//! appear in 8 non-module files and in the disk-cache JSON schema,
//! and renaming would cascade without architectural benefit.
//!
//! [ARCH-0017]: ../../../../docs/decisions/ARCH-0017-ddd-monolith-epic.md

pub mod cache;
pub mod entry;
pub mod fingerprint;
pub mod index;
pub mod legacy;

pub use cache::{CatalogCache, FileCatalogCache};
pub use entry::CompiledOffering;
pub use fingerprint::{current_capabilities_hash, manifests_hash, moss_version_string};
pub use index::{OfferingsFingerprint, OfferingsIndex};
pub use legacy::{ensure_offerings_index, get_compiled_offering, rebuild_offerings_index};
