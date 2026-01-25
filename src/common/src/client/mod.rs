//! Client module providing shared client-side utilities
//!
//! Contains reusable client patterns:
//! - Stone discovery caching with TTL

pub mod api;
pub mod stone_cache;

// Re-export API client types for backward compatibility
pub use api::{GardenApiResponse, GardenHttpClient};
