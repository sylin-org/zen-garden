//! OpenedaiSpeech provider — `audio.generate` via the
//! openedai-speech project (https://github.com/matatonic/openedai-speech).
//! OpenAI-compatible `/v1/audio/speech` endpoint, so we reuse the
//! shared TTS helper.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::domain::keys;
use crate::services::garden_discovery::GardenDiscovery;

use super::openai_compat_tts::OpenAiCompatTts;

#[derive(Debug, Clone, Default)]
pub struct OpenedaiSpeechConfig {
    pub api_key: Option<String>,
}

const FQNS: &[&'static str] = &["openedai_speech", "openedai-speech"];

/// Default voice set matches OpenAI's `tts-1` so existing clients
/// switch transparently.
const VOICES: &[&str] = &[
    "alloy", "echo", "fable", "onyx", "nova", "shimmer",
];

pub struct OpenedaiSpeechProvider;

impl OpenedaiSpeechProvider {
    pub fn new(
        config: OpenedaiSpeechConfig,
        discovery: Arc<GardenDiscovery>,
        shutdown: CancellationToken,
    ) -> Arc<OpenAiCompatTts> {
        OpenAiCompatTts::new(
            keys::providers::OPENEDAI_SPEECH,
            FQNS,
            config.api_key,
            // openedai-speech mirrors OpenAI: model=`tts-1`.
            "tts-1".to_string(),
            "alloy".to_string(),
            "mp3".to_string(),
            VOICES.to_vec(),
            discovery,
            shutdown,
        )
    }
}
