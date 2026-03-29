//! LibreTranslate-specific API response types.
//!
//! These types belong to the LibreTranslate bounded context — nothing
//! outside `offerings/libretranslate/` should depend on them.

use serde::{Deserialize, Serialize};

// -- GET /health --------------------------------------------------------

/// Response from `GET /health`.
#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

// -- GET /languages -----------------------------------------------------

/// A supported language with its translation targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Language {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub targets: Vec<String>,
}
