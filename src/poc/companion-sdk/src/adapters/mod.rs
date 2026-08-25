//! Adapters bounded context.
//!
//! The second of two bounded contexts in the companion SDK (the other
//! being [Garden]). Owns the extension contract — the [`Adapter`] trait —
//! and the supervisor that manages adapter lifecycle (discovery,
//! spawn/reap, subscription filtering, grace window for device bounce).
//!
//! See [COMPANION-0007] for the book ADR.
//!
//! [Garden]: crate::garden
//! [COMPANION-0007]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0007-adapters.md

pub mod adapter;
pub mod exit;
pub mod factory;
pub mod status;
pub mod supervisor;

pub use adapter::{Adapter, AdapterInfo, AdapterProfile, DeliveryPolicy};
pub use exit::{AdapterExitReason, AdapterExited};
pub use factory::AdapterFactory;
pub use status::AdapterStatus;
pub use supervisor::Adapters;
