//! Inter-stone HTTP client trait — relocated to `domain/security/pond_client.rs`.
//!
//! This re-export keeps existing `use crate::domain::traits::PondClient` sites
//! compiling during the ARCH-0027 migration. Remove in a later cleanup.

pub use crate::domain::security::pond_client::PondClient;
