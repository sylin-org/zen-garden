//! Kokoro TTS provider — `audio.generate`.
//!
//! Kokoro FastAPI (https://github.com/remsky/Kokoro-FastAPI) exposes
//! an OpenAI-compatible `POST /v1/audio/speech` endpoint. The only
//! real difference from OpenAI is the voice catalog (Kokoro ships
//! with its own set of voices like `af_bella`, `am_adam`, …).

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::domain::keys;
use crate::services::garden_discovery::GardenDiscovery;

use super::openai_compat_tts::OpenAiCompatTts;

#[derive(Debug, Clone, Default)]
pub struct KokoroConfig {
    pub api_key: Option<String>,
}

/// FQNs Kokoro adapts.
const FQNS: &[&'static str] = &["kokoro"];

/// Kokoro's default voice set (subset). Users can extend by editing
/// this list; new voices appear in the catalog after a restart.
const KOKORO_VOICES: &[&str] = &[
    "af_bella",
    "af_sarah",
    "am_adam",
    "am_michael",
    "bf_emma",
    "bf_isabella",
    "bm_george",
    "bm_lewis",
];

pub struct KokoroProvider;

impl KokoroProvider {
    pub fn new(
        config: KokoroConfig,
        discovery: Arc<GardenDiscovery>,
        shutdown: CancellationToken,
    ) -> Arc<OpenAiCompatTts> {
        OpenAiCompatTts::new(
            keys::providers::KOKORO,
            FQNS,
            config.api_key,
            // Kokoro FastAPI accepts "kokoro" or "tts-1" as model
            // identifiers; "kokoro" is the canonical native name.
            "kokoro".to_string(),
            "af_bella".to_string(),
            "mp3".to_string(),
            KOKORO_VOICES.to_vec(),
            discovery,
            shutdown,
        )
    }
}
