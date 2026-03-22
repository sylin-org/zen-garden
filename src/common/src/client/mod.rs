//! Client module providing shared client-side utilities
//!
//! Contains reusable client patterns:
//! - Stone discovery with TTL
//! - Typed Stone API client (ARCH-0012)

pub mod api;
pub mod discovery;
pub mod stone_api;

// Re-export API client types for backward compatibility
pub use api::{GardenApiResponse, GardenHttpClient};

// Re-export typed client (ARCH-0012)
pub use stone_api::{StoneApi, StoneApiError};
