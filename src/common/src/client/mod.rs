//! Client module providing shared client-side utilities
//!
//! Contains reusable client patterns:
//! - Stone discovery with TTL

pub mod api;
pub mod discovery;

// Re-export API client types for backward compatibility
pub use api::{GardenApiResponse, GardenHttpClient};
