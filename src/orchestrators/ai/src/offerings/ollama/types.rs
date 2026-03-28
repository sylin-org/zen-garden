//! Ollama API response types.
//!
//! These are irreducibly Ollama-specific — the shapes come from Ollama's
//! REST API and have no generic equivalent.

use serde::{Deserialize, Serialize};

/// Response from `GET /api/tags`.
#[derive(Debug, Clone, Deserialize)]
pub struct TagsResponse {
    #[serde(default)]
    pub models: Vec<ModelTag>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelTag {
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub details: Option<ModelDetails>,
}

#[derive(Debug, Clone, Deserialize)]
#[derive(serde::Serialize)]
pub struct ModelDetails {
    pub format: Option<String>,
    pub family: Option<String>,
    #[serde(default)]
    pub families: Vec<String>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}

/// Response from `GET /api/ps`.
#[derive(Debug, Clone, Deserialize)]
pub struct PsResponse {
    #[serde(default)]
    pub models: Vec<RunningModel>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunningModel {
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub size_vram: u64,
    pub expires_at: Option<String>,
    #[serde(default)]
    pub details: Option<ModelDetails>,
}

/// Response from `POST /api/show`.
#[derive(Debug, Clone, Deserialize)]
#[derive(serde::Serialize)]
pub struct ShowResponse {
    #[serde(default)]
    pub details: Option<ModelDetails>,
    #[serde(default)]
    pub model_info: Option<serde_json::Value>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl ShowResponse {
    /// Extract `general.parameter_count` from `model_info`.
    pub fn parameter_count(&self) -> Option<u64> {
        self.model_info
            .as_ref()?
            .get("general.parameter_count")?
            .as_u64()
    }

    /// Extract `{arch}.context_length` from `model_info`.
    pub fn context_length(&self) -> Option<u64> {
        let info = self.model_info.as_ref()?;
        let arch = info.get("general.architecture")?.as_str()?;
        info.get(format!("{arch}.context_length").as_str())?.as_u64()
    }
}

/// Response from `GET /api/version`.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionResponse {
    pub version: String,
}

/// Final NDJSON object from streaming inference (`done: true`).
#[derive(Debug, Clone, Deserialize)]
pub struct InferenceFinal {
    #[serde(default)]
    pub done: bool,
    pub done_reason: Option<String>,
    #[serde(default)]
    pub total_duration: u64,
    #[serde(default)]
    pub load_duration: u64,
    #[serde(default)]
    pub prompt_eval_count: u64,
    #[serde(default)]
    pub prompt_eval_duration: u64,
    #[serde(default)]
    pub eval_count: u64,
    #[serde(default)]
    pub eval_duration: u64,
}

/// Pull progress event from `POST /api/pull` stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullProgress {
    pub status: String,
    pub digest: Option<String>,
    pub total: Option<u64>,
    pub completed: Option<u64>,
}

/// Response from `POST /api/embed`.
#[derive(Debug, Clone, Deserialize)]
pub struct EmbedResponse {
    #[serde(default)]
    pub total_duration: u64,
    #[serde(default)]
    pub load_duration: u64,
    #[serde(default)]
    pub prompt_eval_count: u64,
}
