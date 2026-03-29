//! Infinity-specific API response types.
//!
//! These types belong to the Infinity bounded context — nothing outside
//! `offerings/infinity/` should depend on them.

use serde::{Deserialize, Serialize};

// -- GET /health --------------------------------------------------------

/// Response from `GET /health`.
#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    /// Unix timestamp (float) indicating server uptime reference.
    pub unix: f64,
}

// -- GET /models --------------------------------------------------------

/// Response from `GET /models` (OpenAI-compatible format).
#[derive(Debug, Clone, Deserialize)]
pub struct ModelsResponse {
    #[serde(default)]
    pub data: Vec<ModelEntry>,
}

/// A single model entry in the models list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    #[serde(default)]
    pub object: Option<String>,
}
