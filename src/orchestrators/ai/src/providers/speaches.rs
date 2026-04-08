//! Speaches provider — `audio.transcribe`.
//!
//! Speaches (https://github.com/speaches-ai/speaches) is a GPU-friendly
//! Whisper server exposing an OpenAI-compatible transcription
//! endpoint. Reuses the shared STT helper.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::domain::keys;
use crate::services::garden_discovery::GardenDiscovery;

use super::openai_compat_stt::OpenAiCompatStt;

const FQNS: &[&'static str] = &["speaches"];

#[derive(Debug, Clone)]
pub struct SpeachesConfig {
    pub default_model: String,
    pub api_key: Option<String>,
}

impl Default for SpeachesConfig {
    fn default() -> Self {
        Self {
            default_model: "Systran/faster-distil-whisper-large-v3".to_string(),
            api_key: None,
        }
    }
}

const MODELS: &[&str] = &[
    "Systran/faster-distil-whisper-large-v3",
    "Systran/faster-whisper-large-v3",
    "Systran/faster-whisper-medium",
    "Systran/faster-whisper-small",
];

pub struct SpeachesProvider;

impl SpeachesProvider {
    pub fn new(
        config: SpeachesConfig,
        discovery: Arc<GardenDiscovery>,
        shutdown: CancellationToken,
    ) -> Arc<OpenAiCompatStt> {
        OpenAiCompatStt::new(
            keys::providers::SPEACHES,
            FQNS,
            config.api_key,
            config.default_model,
            MODELS.to_vec(),
            discovery,
            shutdown,
        )
    }
}
