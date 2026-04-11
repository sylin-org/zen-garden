//! Offerings domain module.
//!
//! This module hosts two distinct but related concepts:
//!
//! 1. **The catalog** (`catalog.rs`) — compile-time catalog of available
//!    offerings derived from `ManifestRegistry`. Types: `CompiledOffering`,
//!    `OfferingsIndex`, `OfferingsFingerprint`.
//!
//! 2. **The aggregate** (`aggregate.rs`) — runtime DDD aggregate that owns
//!    this stone's active and adopted-candidate offering pools. Introduced
//!    by [ARCH-0016](../../../../../docs/decisions/ARCH-0016-offerings-aggregate-domain.md).
//!
//! The catalog is "what offerings exist in principle"; the aggregate is
//! "what offerings exist on this stone right now, and how they change over
//! time."

pub mod aggregate;
pub mod catalog;
pub mod event;
pub mod guard;
pub mod store;

// Catalog types — preserve the previous `domain::offerings::*` surface.
pub use catalog::{
    CompiledOffering, OfferingsFingerprint, OfferingsIndex, current_capabilities_hash,
    ensure_offerings_index, get_compiled_offering, manifests_hash, moss_version_string,
    rebuild_offerings_index,
};

// Aggregate types.
pub use aggregate::Offerings;
pub use event::{ChangeKind, OfferingsChanged};
pub use guard::{ActiveGuard, CandidatesGuard};
pub use store::{FileOfferingStore, OfferingStore};
