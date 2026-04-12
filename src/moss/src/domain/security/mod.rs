//! Security domain — pond enrollment, ceremony coordination, inter-stone TLS.
//!
//! DDD aggregate per ARCH-0027 (Book IX of ARCH-0017).
//!
//! The aggregate owns enrollment state, ceremony infrastructure, and HTTPS
//! lifecycle. External code interacts via typed commands and queries.

pub mod aggregate;
pub mod ceremony_persistence;
pub mod event;
pub mod pond_client;
pub mod pond_lifecycle;

#[cfg(test)]
mod tests;

pub use aggregate::Security;
pub use ceremony_persistence::CeremonyPersistence;
pub use event::{SecurityChangeKind, SecurityChanged};
pub use pond_client::PondClient;
