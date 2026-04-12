//! Offerings domain module — runtime DDD aggregate owning the active
//! and adopted-candidate offering pools on this stone.
//!
//! The **compile-time catalog** (`CompiledOffering`, `OfferingsIndex`,
//! `OfferingsFingerprint`) lives in [`crate::domain::catalog`] — it
//! was extracted out of `domain/offerings/catalog.rs` in Ch2 of
//! ARCH-0022 (Book V of ARCH-0017) because catalog and runtime state
//! are different bounded contexts (the former is "what offerings
//! exist in principle"; the latter is "what offerings exist on this
//! stone right now"). The aggregate was first introduced by
//! [ARCH-0016](../../../../../docs/decisions/ARCH-0016-offerings-aggregate-domain.md).

pub mod aggregate;
pub mod event;
pub mod guard;
pub mod store;

pub use aggregate::Offerings;
pub use event::{ChangeKind, OfferingsChanged};
pub use guard::{ActiveGuard, CandidatesGuard};
pub use store::{FileOfferingStore, NoopOfferingStore, OfferingStore};
