//! OpenedaiSpeech provider — `audio.generate` via the
//! openedai-speech project (https://github.com/matatonic/openedai-speech).
//!
//! openedai-speech implements OpenAI's `POST /v1/audio/speech` wire
//! format (same body shape: `{model, input, voice, response_format,
//! speed?}`, raw audio bytes in the response). After ORCH-0030 R2 M3
//! the shared `OpenAiCompatTts` helper is gone — this adapter carries
//! its own HTTP client, its own request translation, its own response
//! parser, and publishes its own capability announcement.
//!
//! Cross-adapter duplication with the sibling Kokoro adapter is
//! deliberate: M3 favors self-contained adapters over compat helpers,
//! per §5.7 of the M3 contract.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::domain::capability_announcement::{
    Capability as AnnCapability, CapabilityAnnouncement,
};
use crate::domain::events::EventBus;
use crate::domain::ids::ProviderName;
use crate::domain::keys;
use crate::domain::media::MediaSource;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{Provider, ProviderError, ProviderOutcome};
use crate::domain::request::OrchestratorRequest;
use crate::services::directory_subscriber::publish_capability_announcement;
use crate::services::garden_discovery::GardenDiscovery;

use super::common::{
    build_http_client, check_status, map_reqwest_error, InstancePool, PerFqnInstances,
};

// ── Constants ────────────────────────────────────────────────

/// FQNs openedai-speech adapts. Both the snake_case and kebab-case
/// spellings appear in the wild; subscribe to both so discovery
/// picks up either form.
const FQNS: &[&'static str] = &["openedai_speech", "openedai-speech"];

/// Voice catalog mirrors OpenAI's `tts-1` so existing clients switch
/// transparently.
const VOICES: &[&'static str] = &["alloy", "echo", "fable", "onyx", "nova", "shimmer"];

// ── Config ───────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct OpenedaiSpeechConfig {
    pub api_key: Option<String>,
}

// ── Provider struct ──────────────────────────────────────────

pub struct OpenedaiSpeechProvider {
    name: ProviderName,
    instances: Arc<InstancePool>,
    api_key: Option<String>,
    /// Sent in the wire body's `model` field. openedai-speech accepts
    /// `"tts-1"` (mirrors OpenAI).
    tts_model_id: String,
    default_voice: String,
    default_format: String,
    voices: &'static [&'static str],
    http: Client,
    events: Arc<EventBus>,
}

impl OpenedaiSpeechProvider {
    pub fn new(
        config: OpenedaiSpeechConfig,
        discovery: Arc<GardenDiscovery>,
        events: Arc<EventBus>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let provider = Arc::new(Self {
            name: ProviderName::new(keys::providers::OPENEDAI_SPEECH),
            instances: Arc::new(InstancePool::new()),
            api_key: config.api_key,
            tts_model_id: "tts-1".to_string(),
            default_voice: "alloy".to_string(),
            default_format: "mp3".to_string(),
            voices: VOICES,
            http: build_http_client(),
            events,
        });
        spawn_subscriber(provider.clone(), discovery, shutdown);
        provider
    }

    /// Attach bearer auth if a key is configured.
    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(k) => rb.bearer_auth(k),
            None => rb,
        }
    }

    /// Round-robin pick an instance URL. Returns `Unreachable` when
    /// no healthy instance is in the pool.
    fn pick(&self) -> Result<String, ProviderError> {
        self.instances.pick().ok_or_else(|| {
            ProviderError::Unreachable(format!("no {} instances in the garden", self.name))
        })
    }

    /// Resolve the voice selected for this request.
    ///
    /// Precedence:
    /// 1. `request.selectors.model` — treated as a voice id. A
    ///    `recommended:*` moniker falls back to `default_voice`. A
    ///    concrete value must appear in `VOICES` or in the payload's
    ///    `audio.voice.id`; otherwise `PinNotServable`.
    /// 2. `payload./audio/voice/id` — caller-supplied voice id.
    /// 3. `self.default_voice`.
    fn resolve_voice(
        &self,
        request: &OrchestratorRequest,
    ) -> Result<String, ProviderError> {
        let payload_voice = request
            .payload
            .pointer("/audio/voice/id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(selector) = request.selectors.model.as_deref() {
            if selector.starts_with("recommended:") {
                return Ok(self.default_voice.clone());
            }
            if self.voices.iter().any(|v| *v == selector) {
                return Ok(selector.to_string());
            }
            if let Some(ref pv) = payload_voice {
                if self.voices.iter().any(|v| *v == pv.as_str()) {
                    return Ok(pv.clone());
                }
            }
            return Err(ProviderError::PinNotServable {
                model: selector.to_string(),
                reason: format!(
                    "voice not in openedai-speech catalog (supported: {})",
                    self.voices.join(", ")
                ),
            });
        }

        Ok(payload_voice.unwrap_or_else(|| self.default_voice.clone()))
    }

    /// Build a full capability announcement from the current instance
    /// pool and publish it to the bus.
    async fn publish_capabilities(&self) {
        let enabled = !self.instances.is_empty();
        // TTS has no media inputs — the caller supplies text, not
        // bytes. Empty media_inputs list per the M3 contract.
        let capabilities = vec![AnnCapability {
            primitive: Primitive::AudioGenerate,
            media_inputs: Vec::new(),
        }];
        let announcement = CapabilityAnnouncement {
            provider: self.name.clone(),
            enabled,
            capabilities,
            skills: Vec::new(),
        };
        publish_capability_announcement(&self.events, &announcement).await;
    }

    /// Apply a freshly merged URL list from the discovery subscriber
    /// and, if the pool changed, republish the capability announcement.
    async fn apply_merged(&self, urls: Vec<String>) {
        if !self.instances.set(urls) {
            return;
        }
        self.publish_capabilities().await;
    }
}

// ── Discovery subscriber ─────────────────────────────────────

fn spawn_subscriber(
    provider: Arc<OpenedaiSpeechProvider>,
    discovery: Arc<GardenDiscovery>,
    shutdown: CancellationToken,
) {
    tokio::spawn(async move {
        let pool = PerFqnInstances::new();
        let mut rx = discovery.subscribe(FQNS).await;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                event = rx.recv() => {
                    let Some(event) = event else { break };
                    let urls: Vec<String> =
                        event.instances.into_iter().map(|i| i.url).collect();
                    pool.set(&event.fqn, urls);
                    provider.apply_merged(pool.flatten()).await;
                }
            }
        }
    });
}

// ── Provider trait impl ──────────────────────────────────────

#[async_trait]
impl Provider for OpenedaiSpeechProvider {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }

    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        if request.action.primitive != Primitive::AudioGenerate {
            return Err(ProviderError::Unsupported(format!(
                "{} does not serve {}",
                self.name,
                request.action.primitive.dotted()
            )));
        }

        // ── Extract wire inputs from the canonical payload ────
        let text = request
            .payload
            .pointer("/audio/text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProviderError::Unsupported("missing audio.text".to_string()))?
            .to_string();

        let voice = self.resolve_voice(&request)?;

        let format = request
            .payload
            .pointer("/audio/format/codec")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.default_format)
            .to_string();

        let speed = request
            .payload
            .pointer("/audio/voice/speed")
            .and_then(|v| v.as_f64());

        // ── Build OpenAI-compatible `/v1/audio/speech` body ───
        let mut body = json!({
            "model": self.tts_model_id,
            "input": text,
            "voice": voice,
            "response_format": format,
        });
        if let Some(s) = speed {
            body["speed"] = json!(s);
        }

        // ── Dispatch to a healthy instance ────────────────────
        let base = self.pick()?;
        let endpoint = format!("{}/v1/audio/speech", base.trim_end_matches('/'));

        tracing::debug!(
            provider = %self.name,
            request_id = %request.id,
            voice = %voice,
            format = %format,
            instance = %endpoint,
            "openedai_speech dispatching tts request",
        );

        let resp = self
            .auth(self.http.post(&endpoint).json(&body))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "openedai_speech tts").await?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        // ── Map format → content type ─────────────────────────
        let content_type = match format.as_str() {
            "mp3" => "audio/mpeg",
            "wav" => "audio/wav",
            "opus" => "audio/ogg",
            "flac" => "audio/flac",
            "aac" => "audio/aac",
            "pcm" => "audio/L16",
            _ => "application/octet-stream",
        };

        // ── Store the generated bytes in the media store ──────
        let entry = request
            .context
            .media_store
            .put(
                bytes,
                content_type.to_string(),
                MediaSource::generated(
                    self.name.clone(),
                    request.action.dotted(),
                    request.id.clone(),
                ),
            )
            .await
            .map_err(|e| ProviderError::Internal(format!("media store: {e}")))?;

        // ── Build canonical output ────────────────────────────
        let mut out = Output::new();
        out.set(&keys::audio::MEDIA_ID, entry.id.as_str());
        out.set(&keys::audio::FORMAT, format);
        Ok(ProviderOutcome::Sync(out))
    }
}
