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

    /// Build a full capability announcement from the current instance
    /// pool and publish it to the bus.
    async fn publish_capabilities(&self) {
        let announcement =
            build_capability_announcement(&self.name, !self.instances.is_empty());
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

// ── Pure helpers (testable without runtime) ──────────────────

/// Build the capability announcement OpenedaiSpeech publishes given
/// the current instance pool state. Pure function — no IO, no `&self`
/// — so unit tests can exercise the wire shape directly.
///
/// TTS adapters declare `audio.generate` with an empty `media_inputs`
/// list: the caller supplies text in the payload, not media bytes.
fn build_capability_announcement(
    name: &ProviderName,
    has_instances: bool,
) -> CapabilityAnnouncement {
    CapabilityAnnouncement {
        provider: name.clone(),
        enabled: has_instances,
        capabilities: vec![Capability {
            primitive: Primitive::AudioGenerate,
            media_inputs: Vec::new(),
            parameters: vec![
                SkillParameter { field: "text.prompt.user".into(), required: true, label: Some("Text".into()), field_type: Some(ParameterType::String), widget: Some(ParameterWidget::Textarea), placeholder: Some("Text to speak...".into()), ..Default::default() },
                SkillParameter { field: "audio.voice".into(), required: false, label: Some("Voice".into()), field_type: Some(ParameterType::String), widget: Some(ParameterWidget::Select), ..Default::default() },
                SkillParameter { field: "audio.speed".into(), required: false, label: Some("Speed".into()), field_type: Some(ParameterType::Number), widget: Some(ParameterWidget::Slider), default: Some(serde_json::json!(1.0)), min: Some(0.5), max: Some(2.0), step: Some(0.1), ..Default::default() },
            ],
        }],
        skills: Vec::new(),
    }
}

/// Resolve the voice selected for this request.
///
/// Precedence (matches the legacy in-struct implementation):
///
/// 1. If `selector` is `Some(s)`:
///    - `recommended:*` → `default_voice`.
///    - `s` in `voices` → use `s`.
///    - Else if `payload_voice` is in `voices` → use `payload_voice`.
///    - Else `PinNotServable`.
/// 2. If `selector` is `None`:
///    - `payload_voice` if present (passed through unvalidated,
///      matching legacy behaviour).
///    - Else `default_voice`.
fn resolve_voice(
    selector: Option<&str>,
    payload_voice: Option<&str>,
    default_voice: &str,
    voices: &[&str],
) -> Result<String, ProviderError> {
    if let Some(selector) = selector {
        if selector.starts_with("recommended:") {
            return Ok(default_voice.to_string());
        }
        if voices.iter().any(|v| *v == selector) {
            return Ok(selector.to_string());
        }
        if let Some(pv) = payload_voice {
            if voices.iter().any(|v| *v == pv) {
                return Ok(pv.to_string());
            }
        }
        return Err(ProviderError::PinNotServable {
            model: selector.to_string(),
            reason: format!(
                "voice not in openedai-speech catalog (supported: {})",
                voices.join(", ")
            ),
        });
    }

    Ok(payload_voice
        .map(|s| s.to_string())
        .unwrap_or_else(|| default_voice.to_string()))
}

// ── Tests (ORCH-0030 R2 M4) ──────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_name() -> ProviderName {
        ProviderName::new(keys::providers::OPENEDAI_SPEECH)
    }

    fn voices() -> Vec<&'static str> {
        VOICES.to_vec()
    }

    // ── Capability publication ──

    #[test]
    fn announcement_disabled_when_no_instances() {
        let ann = build_capability_announcement(&provider_name(), false);
        assert_eq!(ann.provider.as_str(), "openedai_speech");
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
    fn announcement_declares_audio_generate_with_empty_media_inputs() {
        let ann = build_capability_announcement(&provider_name(), true);
        assert_eq!(ann.capabilities.len(), 1);
        let cap = &ann.capabilities[0];
        assert_eq!(cap.primitive, Primitive::AudioGenerate);
        // TTS produces audio — it does not consume media inputs.
        assert!(cap.media_inputs.is_empty());
    }

    // ── Voice resolution ──

    #[test]
    fn resolve_voice_no_input_returns_default() {
        let v = resolve_voice(None, None, "alloy", &voices()).unwrap();
        assert_eq!(v, "alloy");
    }

    #[test]
    fn resolve_voice_recommended_moniker_returns_default() {
        let v = resolve_voice(Some("recommended:tts"), None, "alloy", &voices()).unwrap();
        assert_eq!(v, "alloy");
    }

    #[test]
    fn resolve_voice_known_selector_passes_through() {
        let v = resolve_voice(Some("echo"), None, "alloy", &voices()).unwrap();
        assert_eq!(v, "echo");
    }

    #[test]
    fn resolve_voice_payload_voice_fallback_when_no_selector() {
        // With no selector, payload voice passes through unvalidated —
        // matching the legacy in-struct behaviour.
        let v = resolve_voice(None, Some("nova"), "alloy", &voices()).unwrap();
        assert_eq!(v, "nova");
    }

    #[test]
    fn resolve_voice_unknown_selector_rescued_by_valid_payload_voice() {
        // Selector is not in the catalog, but the payload carries a
        // valid voice — legacy precedence rescues the request.
        let v = resolve_voice(Some("bogus"), Some("shimmer"), "alloy", &voices()).unwrap();
        assert_eq!(v, "shimmer");
    }

    #[test]
    fn resolve_voice_unknown_selector_returns_pin_not_servable() {
        let err = resolve_voice(Some("nope-voice"), None, "alloy", &voices()).unwrap_err();
        match err {
            ProviderError::PinNotServable { model, reason } => {
                assert_eq!(model, "nope-voice");
                assert!(reason.contains("alloy"));
                assert!(reason.contains("openedai-speech"));
            }
            other => panic!("expected PinNotServable, got {other:?}"),
        }
    }

    #[test]
    fn resolve_voice_unknown_selector_with_unknown_payload_voice_is_pin_not_servable() {
        let err =
            resolve_voice(Some("bogus"), Some("also-bogus"), "alloy", &voices()).unwrap_err();
        assert!(matches!(err, ProviderError::PinNotServable { .. }));
    }
}
