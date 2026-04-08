//! Kokoro TTS provider — `audio.generate` (ORCH-0030 R2 M3).
//!
//! Kokoro FastAPI (https://github.com/remsky/Kokoro-FastAPI) exposes
//! an OpenAI-compatible `POST /v1/audio/speech` endpoint. The body
//! shape mirrors OpenAI's:
//!
//! ```json
//! {
//!   "model": "kokoro",
//!   "input": "Hello, world.",
//!   "voice": "af_bella",
//!   "response_format": "mp3",
//!   "speed": 1.0
//! }
//! ```
//!
//! The response body is raw audio bytes whose content type depends on
//! the requested `response_format`. Kokoro's defining feature is its
//! own voice catalog (`af_bella`, `am_adam`, …) — the caller pins a
//! voice via `selectors.model` or `audio.voice.id`, and the adapter
//! validates it against [`KOKORO_VOICES`] before dispatching.
//!
//! # M3 shape
//!
//! After the M3 lean-trait switch, Kokoro is a fully self-contained
//! adapter: its own struct, its own HTTP client, its own request
//! body construction, its own response decoding, its own capability
//! publication. The shared `OpenAiCompatTts` helper that previously
//! backed both Kokoro and openedai-speech is deleted.
//!
//! The adapter subscribes to [`GardenDiscovery`] for the `"kokoro"`
//! FQN, maintains a round-robin [`InstancePool`] of healthy endpoint
//! URLs, and republishes a [`CapabilityAnnouncement`] on every change.
//! When the pool is empty the announcement is `enabled: false` and the
//! adapter drops out of the dispatcher's routing pool.

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

/// FQNs Kokoro adapts. Discovery's base-name match picks up `kokoro`
/// and any `kokoro::adopted`, `kokoro::dev`, etc. variants.
const FQNS: &[&'static str] = &["kokoro"];

/// Kokoro's default voice set (subset). New voices can be added by
/// extending this list — they appear in the catalog after a restart.
const KOKORO_VOICES: &[&'static str] = &[
    "af_bella",
    "af_sarah",
    "am_adam",
    "am_michael",
    "bf_emma",
    "bf_isabella",
    "bm_george",
    "bm_lewis",
];

// ── Config ───────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct KokoroConfig {
    pub api_key: Option<String>,
}

// ── Provider ─────────────────────────────────────────────────

pub struct KokoroProvider {
    name: ProviderName,
    instances: Arc<InstancePool>,
    api_key: Option<String>,
    /// TTS engine model identifier sent in the wire body's `model`
    /// field. Distinct from the voice id — Kokoro FastAPI accepts
    /// `"kokoro"` as the canonical native name.
    tts_model_id: String,
    default_voice: String,
    default_format: String,
    voices: &'static [&'static str],
    http: Client,
    events: Arc<EventBus>,
}

impl KokoroProvider {
    pub fn new(
        config: KokoroConfig,
        discovery: Arc<GardenDiscovery>,
        events: Arc<EventBus>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::KOKORO);
        let provider = Arc::new(Self {
            name,
            instances: Arc::new(InstancePool::new()),
            api_key: config.api_key,
            tts_model_id: "kokoro".to_string(),
            default_voice: "af_bella".to_string(),
            default_format: "mp3".to_string(),
            voices: KOKORO_VOICES,
            http: build_http_client(),
            events,
        });
        spawn_subscriber(provider.clone(), discovery, shutdown);
        provider
    }

    /// Attach bearer auth to a request builder if an API key is set.
    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(k) => rb.bearer_auth(k),
            None => rb,
        }
    }

    /// Pick a round-robin instance URL from the pool, or return an
    /// `Unreachable` error if the pool is empty.
    fn pick_instance(&self) -> Result<String, ProviderError> {
        self.instances.pick().ok_or_else(|| {
            ProviderError::Unreachable(format!("no {} instances in the garden", self.name))
        })
    }

    /// Publish the current capability set to the bus. Called at
    /// construction (after the first discovery event) and on every
    /// subsequent instance-pool change.
    ///
    /// Kokoro declares a single `AudioGenerate` capability with an
    /// empty `media_inputs` list — TTS produces media, it does not
    /// consume any.
    async fn publish_capabilities(&self) {
        let enabled = !self.instances.is_empty();
        let announcement = CapabilityAnnouncement {
            provider: self.name.clone(),
            enabled,
            capabilities: vec![AnnCapability {
                primitive: Primitive::AudioGenerate,
                media_inputs: Vec::new(),
            }],
            skills: Vec::new(),
        };
        publish_capability_announcement(&self.events, &announcement).await;
    }

    /// Apply a merged URL list from the discovery subscriber. If the
    /// instance set actually changed, republish the capability
    /// announcement so the directory refreshes its `enabled` flag.
    async fn apply_merged(&self, urls: Vec<String>) {
        if !self.instances.set(urls) {
            return;
        }
        self.publish_capabilities().await;
    }

    /// Resolve the caller's voice selector against [`KOKORO_VOICES`].
    ///
    /// Precedence:
    /// 1. `request.selectors.model` (treated as the voice id).
    /// 2. `payload./audio/voice/id`.
    /// 3. `self.default_voice`.
    ///
    /// `recommended:*` monikers always fall back to the default —
    /// Kokoro has no recommendation engine and every voice is equally
    /// valid. A concrete selector that is not in [`KOKORO_VOICES`] and
    /// does not match the payload fallback returns
    /// [`ProviderError::PinNotServable`].
    fn resolve_voice(&self, request: &OrchestratorRequest) -> Result<String, ProviderError> {
        // Case 1: caller set selectors.model.
        if let Some(sel) = request.selectors.model.as_deref() {
            if sel.starts_with("recommended:") {
                return Ok(self.default_voice.clone());
            }
            if self.voices.contains(&sel) {
                return Ok(sel.to_string());
            }
            // The selector was concrete but not in the Kokoro voice
            // catalog. Give the payload fallback one chance — a caller
            // that knows what they're doing may have pinned a voice
            // directly in `audio.voice.id` without touching the model
            // selector.
            if let Some(payload_voice) = request
                .payload
                .pointer("/audio/voice/id")
                .and_then(|v| v.as_str())
            {
                if self.voices.contains(&payload_voice) {
                    return Ok(payload_voice.to_string());
                }
            }
            return Err(ProviderError::PinNotServable {
                model: sel.to_string(),
                reason: format!(
                    "voice not in kokoro catalog (available: {})",
                    self.voices.join(", ")
                ),
            });
        }

        // Case 2: caller omitted selectors.model, try the payload.
        if let Some(payload_voice) = request
            .payload
            .pointer("/audio/voice/id")
            .and_then(|v| v.as_str())
        {
            if self.voices.contains(&payload_voice) {
                return Ok(payload_voice.to_string());
            }
            return Err(ProviderError::PinNotServable {
                model: payload_voice.to_string(),
                reason: format!(
                    "voice not in kokoro catalog (available: {})",
                    self.voices.join(", ")
                ),
            });
        }

        // Case 3: nothing pinned, use the default.
        Ok(self.default_voice.clone())
    }
}

// ── Discovery subscriber ─────────────────────────────────────

fn spawn_subscriber(
    provider: Arc<KokoroProvider>,
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
impl Provider for KokoroProvider {
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

        // ── Extract inputs from the canonical payload ──
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

        // ── Build the OpenAI-compatible wire body ──
        let mut body = json!({
            "model": self.tts_model_id,
            "input": text,
            "voice": voice,
            "response_format": format,
        });
        if let Some(s) = speed {
            body["speed"] = json!(s);
        }

        // ── Dispatch to a round-robin instance ──
        let base = self.pick_instance()?;
        let endpoint = format!("{}/v1/audio/speech", base.trim_end_matches('/'));
        let resp = self
            .auth(self.http.post(&endpoint).json(&body))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "kokoro speech").await?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        // ── Map format → content type ──
        let content_type = match format.as_str() {
            "mp3" => "audio/mpeg",
            "wav" => "audio/wav",
            "opus" => "audio/ogg",
            "flac" => "audio/flac",
            "aac" => "audio/aac",
            "pcm" => "audio/L16",
            _ => "application/octet-stream",
        };

        // ── Persist the audio bytes to the media store ──
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

        // ── Build the canonical output envelope ──
        let mut out = Output::new();
        out.set(&keys::audio::MEDIA_ID, entry.id.as_str());
        out.set(&keys::audio::FORMAT, format);
        Ok(ProviderOutcome::Sync(out))
    }
}
