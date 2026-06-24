//! Client module providing shared client-side utilities
//!
//! Contains reusable client patterns:
//! - Typed Stone API client (ARCH-0012)
//!
//! Stone discovery (TTL cache, DiscoveryProvider trait, mDNS/UDP) moved
//! to the `garden-discovery` crate per DISC-0001.

pub mod stone_api;

// Re-export typed client (ARCH-0012)
pub use stone_api::{PondSigning, StoneApi, StoneApiError};
