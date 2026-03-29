//! OpenedAI Speech-specific API response types.
//!
//! These types belong to the OpenedAI Speech bounded context — nothing
//! outside `offerings/openedai_speech/` should depend on them.

use serde::{Deserialize, Serialize};

// -- GET /health --------------------------------------------------------

/// Response from `GET /health`.
#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    pub status: String,
}

// -- GET /v1/models -----------------------------------------------------

/// Response from `GET /v1/models` (OpenAI-compatible format).
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
    #[serde(default)]
    pub owned_by: Option<String>,
}
