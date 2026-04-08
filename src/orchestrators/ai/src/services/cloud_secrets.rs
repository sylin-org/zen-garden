//! Cloud-provider secrets loader.
//!
//! Cloud providers (Anthropic, OpenAI, Google) live outside the
//! garden's discovery surface. Their API keys are loaded once at
//! startup from `{data_dir}/cloud_providers.json`. Missing keys
//! mean the corresponding provider is simply not registered — the
//! catalog reflects what is actually available.
//!
//! File format (all fields optional):
//!
//! ```json
//! {
//!   "anthropic": {
//!     "api_key": "sk-ant-...",
//!     "base_url": "https://api.anthropic.com"
//!   },
//!   "openai": {
//!     "api_key": "sk-...",
//!     "base_url": "https://api.openai.com",
//!     "organization": null
//!   },
//!   "google": {
//!     "api_key": "...",
//!     "base_url": "https://generativelanguage.googleapis.com"
//!   }
//! }
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CloudSecrets {
    #[serde(default)]
    pub anthropic: Option<AnthropicSecret>,
    #[serde(default)]
    pub openai: Option<OpenAiSecret>,
    #[serde(default)]
    pub google: Option<GoogleSecret>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicSecret {
    pub api_key: String,
    #[serde(default = "default_anthropic_base_url")]
    pub base_url: String,
}

fn default_anthropic_base_url() -> String {
    "https://api.anthropic.com".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiSecret {
    pub api_key: String,
    #[serde(default = "default_openai_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub organization: Option<String>,
}

fn default_openai_base_url() -> String {
    "https://api.openai.com".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleSecret {
    pub api_key: String,
    #[serde(default = "default_google_base_url")]
    pub base_url: String,
}

fn default_google_base_url() -> String {
    "https://generativelanguage.googleapis.com".to_string()
}

impl CloudSecrets {
    /// Load `{data_dir}/cloud_providers.json`. Missing file → empty
    /// secrets (no cloud providers registered). Parse errors are
    /// logged and treated as empty.
    pub async fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("cloud_providers.json");
        let bytes = match tokio::fs::read(&path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!(
                    path = %path.display(),
                    "no cloud_providers.json — cloud providers will not be registered"
                );
                return Self::default();
            }
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "failed to read cloud_providers.json"
                );
                return Self::default();
            }
        };
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(secrets) => secrets,
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "failed to parse cloud_providers.json"
                );
                Self::default()
            }
        }
    }
}
