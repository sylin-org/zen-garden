//! WhisperCpp provider — `audio.transcribe`.
//!
//! whisper.cpp's server (`whisper-server`) ships an
//! OpenAI-compatible `/v1/audio/transcriptions` endpoint. This
//! provider reuses the shared STT helper.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::domain::keys;
use crate::services::garden_discovery::GardenDiscovery;

use super::openai_compat_stt::OpenAiCompatStt;

#[derive(Debug, Clone)]
pub struct WhisperCppConfig {
    pub default_model: String,
}

impl Default for WhisperCppConfig {
    fn default() -> Self {
        Self {
            default_model: "whisper-1".to_string(),
        }
    }
}

const FQNS: &[&'static str] = &["whispercpp", "whisper-cpp"];
const MODELS: &[&str] = &["whisper-1"];

pub struct WhisperCppProvider;

impl WhisperCppProvider {
    pub fn new(
        config: WhisperCppConfig,
        discovery: Arc<GardenDiscovery>,
        shutdown: CancellationToken,
    ) -> Arc<OpenAiCompatStt> {
        OpenAiCompatStt::new(
            keys::providers::WHISPERCPP,
            FQNS,
            None,
            config.default_model,
            MODELS.to_vec(),
            discovery,
            shutdown,
        )
    }
}
