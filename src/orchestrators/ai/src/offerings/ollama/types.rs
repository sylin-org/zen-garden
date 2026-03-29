//! Ollama-specific API response types.
//!
//! These types belong to the Ollama bounded context — nothing outside
//! `offerings/ollama/` should depend on them. Shared types like
//! `ModelInfo` and `LoadedModel` live in `crate::domain::types`.

use serde::{Deserialize, Serialize};

// ── GET /api/tags ───────────────────────────────────────────────

/// Response from `GET /api/tags`.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaTagsResponse {
    #[serde(default)]
    pub models: Vec<OllamaModelTag>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelTag {
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub details: Option<OllamaModelDetails>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelDetails {
    pub format: Option<String>,
    pub family: Option<String>,
    #[serde(default)]
    pub families: Vec<String>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}

// ── GET /api/ps ─────────────────────────────────────────────────

/// Response from `GET /api/ps`.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaPsResponse {
    #[serde(default)]
    pub models: Vec<OllamaRunningModel>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaRunningModel {
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub size_vram: u64,
    pub expires_at: Option<String>,
    #[serde(default)]
    pub details: Option<OllamaModelDetails>,
}

// ── POST /api/show ──────────────────────────────────────────────

/// Response from `POST /api/show`.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaShowResponse {
    #[serde(default)]
    pub details: Option<OllamaModelDetails>,
    #[serde(default)]
    pub model_info: Option<serde_json::Value>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl OllamaShowResponse {
    /// Extract `general.parameter_count` from `model_info`.
    pub fn parameter_count(&self) -> Option<u64> {
        self.model_info
            .as_ref()?
            .get("general.parameter_count")?
            .as_u64()
    }

    /// Extract `{arch}.context_length` from `model_info`.
    ///
    /// Ollama stores context length under an architecture-prefixed key:
    ///   `model_info["general.architecture"]` -> e.g. "bert", "nomic-bert", "qwen2"
    ///   `model_info["{arch}.context_length"]` -> e.g. 8192, 256, 131072
    pub fn context_length(&self) -> Option<u64> {
        let info = self.model_info.as_ref()?;
        let arch = info.get("general.architecture")?.as_str()?;
        info.get(format!("{arch}.context_length").as_str())?.as_u64()
    }
}

// ── GET /api/version ────────────────────────────────────────────

/// Response from `GET /api/version`.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaVersionResponse {
    pub version: String,
}

// ── Inference timing (done: true) ───────────────────────────────

/// Final NDJSON object from streaming inference (`done: true`).
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaInferenceFinal {
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

// ── POST /api/pull progress ─────────────────────────────────────

/// Pull progress event from `POST /api/pull` NDJSON stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaPullProgress {
    pub status: String,
    pub digest: Option<String>,
    pub total: Option<u64>,
    pub completed: Option<u64>,
}

// ── POST /api/embed ─────────────────────────────────────────────

/// Response from `POST /api/embed`.
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaEmbedResponse {
    #[serde(default)]
    pub total_duration: u64,
    #[serde(default)]
    pub load_duration: u64,
    #[serde(default)]
    pub prompt_eval_count: u64,
}
