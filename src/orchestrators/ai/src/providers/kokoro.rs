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
    Capability, CapabilityAnnouncement, ParameterType, ParameterWidget, SkillParameter,
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
        let announcement =
            build_capability_announcement(&self.name, !self.instances.is_empty());
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

        let voice = resolve_voice(
            request.selectors.model.as_deref(),
            request
                .payload
                .pointer("/audio/voice/id")
                .and_then(|v| v.as_str()),
            &self.default_voice,
            self.voices,
        )?;

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

// ── Pure helpers (testable without runtime) ──────────────────

/// Build the capability announcement Kokoro publishes given the
/// current instance pool state. Pure function — no IO, no &self —
/// so unit tests can exercise the wire shape directly.
///
/// Kokoro declares a single `AudioGenerate` capability with an
/// empty `media_inputs` list — TTS produces media, it does not
/// consume any.
fn build_capability_announcement(
    name: &ProviderName,
    has_instances: bool,
) -> CapabilityAnnouncement {
    CapabilityAnnouncement {
        provider: name.clone(),
        enabled: has_instances,
        capabilities: vec![Capability {
            primitive: Primitive::AudioGenerate,
            media_inputs: Vec::new(), // TTS produces media; consumes none
            parameters: vec![
                SkillParameter { field: "text.prompt.user".into(), required: true, label: Some("Text".into()), field_type: Some(ParameterType::String), widget: Some(ParameterWidget::Textarea), placeholder: Some("Text to speak...".into()), ..Default::default() },
                SkillParameter { field: "audio.voice".into(), required: false, label: Some("Voice".into()), field_type: Some(ParameterType::String), widget: Some(ParameterWidget::Select), ..Default::default() },
                SkillParameter { field: "audio.speed".into(), required: false, label: Some("Speed".into()), field_type: Some(ParameterType::Number), widget: Some(ParameterWidget::Slider), default: Some(serde_json::json!(1.0)), min: Some(0.5), max: Some(2.0), step: Some(0.1), ..Default::default() },
            ],
        }],
        skills: Vec::new(),
    }
}

/// Resolve the caller's voice selector against the Kokoro voice
/// catalog.
///
/// Precedence (preserved from the original `KokoroProvider::resolve_voice`):
///
/// 1. `selector` (from `request.selectors.model`) set:
///    - `recommended:*` → `default_voice`.
///    - known voice in `voices` → passes through.
///    - unknown selector → try `payload_voice`: if it is a known
///      voice, use it; otherwise return `PinNotServable { model: selector }`.
/// 2. `selector` absent, `payload_voice` present:
///    - known voice → passes through.
///    - unknown voice → `PinNotServable { model: payload_voice }`.
/// 3. Neither set → `default_voice`.
fn resolve_voice(
    selector: Option<&str>,
    payload_voice: Option<&str>,
    default_voice: &str,
    voices: &[&str],
) -> Result<String, ProviderError> {
    // Case 1: caller set selectors.model.
    if let Some(sel) = selector {
        if sel.starts_with("recommended:") {
            return Ok(default_voice.to_string());
        }
        if voices.contains(&sel) {
            return Ok(sel.to_string());
        }
        // The selector was concrete but not in the Kokoro voice
        // catalog. Give the payload fallback one chance — a caller
        // that knows what they're doing may have pinned a voice
        // directly in `audio.voice.id` without touching the model
        // selector.
        if let Some(pv) = payload_voice {
            if voices.contains(&pv) {
                return Ok(pv.to_string());
            }
        }
        return Err(ProviderError::PinNotServable {
            model: sel.to_string(),
            reason: format!(
                "voice not in kokoro catalog (available: {})",
                voices.join(", ")
            ),
        });
    }

    // Case 2: caller omitted selectors.model, try the payload.
    if let Some(pv) = payload_voice {
        if voices.contains(&pv) {
            return Ok(pv.to_string());
        }
        return Err(ProviderError::PinNotServable {
            model: pv.to_string(),
            reason: format!(
                "voice not in kokoro catalog (available: {})",
                voices.join(", ")
            ),
        });
    }

    // Case 3: nothing pinned, use the default.
    Ok(default_voice.to_string())
}

// ── Tests (ORCH-0030 R2 M4) ──────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_name() -> ProviderName {
        ProviderName::new(keys::providers::KOKORO)
    }

    fn voices() -> &'static [&'static str] {
        KOKORO_VOICES
    }

    // ── Capability publication ──

    #[test]
    fn announcement_disabled_when_no_instances() {
        let ann = build_capability_announcement(&provider_name(), false);
        assert_eq!(ann.provider.as_str(), "kokoro");
        assert!(!ann.enabled);
        assert_eq!(ann.capabilities.len(), 1);
        assert!(ann.skills.is_empty());
    }

    #[test]
    fn announcement_enabled_when_instances_present() {
        let ann = build_capability_announcement(&provider_name(), true);
        assert!(ann.enabled);
    }

    #[test]
    fn announcement_declares_audio_generate_without_media_inputs() {
        let ann = build_capability_announcement(&provider_name(), true);
        assert_eq!(ann.capabilities.len(), 1);
        let cap = &ann.capabilities[0];
        assert_eq!(cap.primitive, Primitive::AudioGenerate);
        // TTS adapters PRODUCE media; they do not CONSUME media inputs.
        assert!(cap.media_inputs.is_empty());
    }

    // ── Voice resolution ──

    #[test]
    fn resolve_voice_no_input_returns_default() {
        let v = resolve_voice(None, None, "af_bella", voices()).unwrap();
        assert_eq!(v, "af_bella");
    }

    #[test]
    fn resolve_voice_recommended_moniker_returns_default() {
        let v = resolve_voice(Some("recommended:tts"), None, "af_bella", voices()).unwrap();
        assert_eq!(v, "af_bella");
    }

    #[test]
    fn resolve_voice_known_selector_passes_through() {
        let v = resolve_voice(Some("am_adam"), None, "af_bella", voices()).unwrap();
        assert_eq!(v, "am_adam");
    }

    #[test]
    fn resolve_voice_falls_back_to_payload_voice_when_no_selector() {
        // With no selector set, a payload voice that IS in the catalog
        // passes through. The adapter validates payload voices against
        // the catalog (not passed through unvalidated).
        let v = resolve_voice(None, Some("am_michael"), "af_bella", voices()).unwrap();
        assert_eq!(v, "am_michael");
    }

    #[test]
    fn resolve_voice_unknown_selector_returns_pin_not_servable() {
        let err =
            resolve_voice(Some("nope-voice"), None, "af_bella", voices()).unwrap_err();
        match err {
            ProviderError::PinNotServable { model, .. } => {
                assert_eq!(model, "nope-voice");
            }
            other => panic!("expected PinNotServable, got {other:?}"),
        }
    }

    #[test]
    fn resolve_voice_unknown_payload_voice_returns_pin_not_servable() {
        // With no selector, an unknown payload voice is validated
        // against the catalog and rejected.
        let err =
            resolve_voice(None, Some("zz_ghost"), "af_bella", voices()).unwrap_err();
        match err {
            ProviderError::PinNotServable { model, .. } => {
                assert_eq!(model, "zz_ghost");
            }
            other => panic!("expected PinNotServable, got {other:?}"),
        }
    }
}
