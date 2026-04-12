//! Ceremony persistence trait — relocated to `domain/security/ceremony_persistence.rs`.
//!
//! This re-export keeps existing `use crate::domain::traits::CeremonyPersistence` sites
//! compiling during the ARCH-0027 migration. Remove in a later cleanup.

pub use crate::domain::security::ceremony_persistence::CeremonyPersistence;
