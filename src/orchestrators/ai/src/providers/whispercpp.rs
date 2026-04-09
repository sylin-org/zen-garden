//! WhisperCpp provider — `audio.transcribe` via whisper.cpp's
//! OpenAI-compatible `/v1/audio/transcriptions` endpoint
//! (ORCH-0030 R2 M3).
//!
//! After M3 this adapter is fully self-contained: its own struct, its
//! own HTTP client, its own multipart builder, its own response
//! parsing, its own discovery subscriber, and its own capability
//! publication path. The previously-shared `OpenAiCompatStt` helper is
//! deleted.
//!
//! whisper.cpp's server (`whisper-server`) is unauthenticated — there
//! is no API key concept. Instances live in the garden and are pushed
//! to the adapter via `GardenDiscovery`.
//!
//! # Dispatch flow
//!
//! On every `audio.transcribe` request:
//!
//! 1. Pick an instance from the round-robin [`InstancePool`].
//! 2. Resolve the model — use `request.selectors.model` if set, else
//!    fall back to `self.default_model`. Whisper has a single static
//!    model (`whisper-1`) so there is no `recommended:*` resolution.
//! 3. Fetch the audio bytes from the media store via the
//!    `audio.source` media reference (`MediaDelivery::Transfer`).
//! 4. Build an OpenAI-shaped multipart form: `model`, `file`, and
//!    optional `language` / `response_format` fields.
//! 5. POST to `{instance}/v1/audio/transcriptions` and parse the
//!    `{text, language?}` JSON response into the canonical output.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::domain::capability_announcement::{
    Capability, CapabilityAnnouncement, CapabilityMediaInput, ParameterType, ParameterWidget,
    SkillParameter,
};
use crate::domain::events::EventBus;
use crate::domain::ids::ProviderName;
use crate::domain::keys;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{Provider, ProviderError, ProviderOutcome};
use crate::domain::request::OrchestratorRequest;
use crate::services::directory_subscriber::publish_capability_announcement;
use crate::services::garden_discovery::GardenDiscovery;

use super::common::{
    build_http_client, check_status, map_reqwest_error, InstancePool, PerFqnInstances,
};

// ── Configuration ────────────────────────────────────────────

const FQNS: &[&'static str] = &["whispercpp", "whisper-cpp"];
const MODELS: &[&str] = &["whisper-1"];

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

// ── Provider struct ──────────────────────────────────────────

pub struct WhisperCppProvider {
    name: ProviderName,
    instances: Arc<InstancePool>,
    default_model: String,
    http: Client,
    events: Arc<EventBus>,
}

impl WhisperCppProvider {
    pub fn new(
        config: WhisperCppConfig,
        discovery: Arc<GardenDiscovery>,
        events: Arc<EventBus>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::WHISPERCPP);
        let provider = Arc::new(Self {
            name,
            instances: Arc::new(InstancePool::new()),
            default_model: config.default_model,
            http: build_http_client(),
            events,
        });
        spawn_subscriber(provider.clone(), discovery, shutdown);
        provider
    }

    /// Publish the current capability set to the bus. Called from the
    /// discovery subscriber whenever the instance pool changes.
    async fn publish_capabilities(&self) {
        let announcement =
            build_capability_announcement(&self.name, !self.instances.is_empty());
        publish_capability_announcement(&self.events, &announcement).await;
    }

    /// Replace the instance pool with the merged URL list and
    /// republish the capability announcement if the pool actually
    /// changed. Called from the discovery subscriber.
    async fn apply_merged(&self, urls: Vec<String>) {
        if !self.instances.set(urls) {
            return;
        }
        self.publish_capabilities().await;
    }

    fn pick_instance(&self) -> Result<String, ProviderError> {
        self.instances.pick().ok_or_else(|| {
            ProviderError::Unreachable(format!(
                "no {} instances in the garden",
                self.name
            ))
        })
    }
}

// ── Discovery subscriber ─────────────────────────────────────

fn spawn_subscriber(
    provider: Arc<WhisperCppProvider>,
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
impl Provider for WhisperCppProvider {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }

    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        if request.action.primitive != Primitive::AudioTranscribe {
            return Err(ProviderError::Unsupported(format!(
                "{} does not serve {}",
                self.name,
                request.action.primitive.dotted()
            )));
        }

        // Model resolution is adapter-local. Whisper has a single
        // static model — there is no `recommended:*` path. If the
        // caller pinned something we don't know about, reject with
        // `PinNotServable`; otherwise use `default_model`.
        let model = resolve_model(request.selectors.model.as_deref(), &self.default_model)?;

        // Transfer delivery: pull bytes from the media store and
        // construct our own multipart body.
        let media_ref = request
            .media
            .find_at_field(&keys::audio::SOURCE)
            .ok_or_else(|| {
                ProviderError::Unsupported(
                    "audio.source media reference missing".to_string(),
                )
            })?;
        let bytes = request
            .context
            .media_store
            .get_bytes(&media_ref.id)
            .await
            .map_err(|e| ProviderError::Internal(format!("media fetch: {e}")))?;
        let meta = request
            .context
            .media_store
            .get_metadata(&media_ref.id)
            .await
            .map_err(|e| ProviderError::Internal(format!("media meta: {e}")))?;

        let filename = format!(
            "{}{}",
            media_ref.id,
            match meta.content_type.as_str() {
                "audio/mpeg" | "audio/mp3" => ".mp3",
                "audio/wav" | "audio/wave" => ".wav",
                "audio/ogg" => ".ogg",
                "audio/flac" => ".flac",
                "audio/webm" => ".webm",
                "audio/mp4" | "audio/m4a" => ".m4a",
                _ => ".bin",
            }
        );
        let file_part = reqwest::multipart::Part::bytes(bytes.to_vec())
            .file_name(filename)
            .mime_str(&meta.content_type)
            .map_err(|e| ProviderError::Internal(format!("mime: {e}")))?;
        let mut form = reqwest::multipart::Form::new()
            .text("model", model)
            .part("file", file_part);
        if let Some(lang) = request
            .payload
            .pointer("/audio/language/source")
            .and_then(|v| v.as_str())
        {
            form = form.text("language", lang.to_string());
        }
        if let Some(fmt) = request
            .payload
            .pointer("/text/format/response")
            .and_then(|v| v.as_str())
        {
            form = form.text("response_format", fmt.to_string());
        }

        let base = self.pick_instance()?;
        let endpoint = format!(
            "{}/v1/audio/transcriptions",
            base.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&endpoint)
            .multipart(form)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "whispercpp transcription").await?;
        let wire: TranscriptionResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, wire.text);
        if let Some(lang) = wire.language {
            out.set(&keys::text::LANGUAGE, lang);
        }
        Ok(ProviderOutcome::Sync(out))
    }
}

// ── Pure helpers (testable without runtime) ──────────────────

/// Build the capability announcement WhisperCpp publishes given the
/// current instance pool state. Pure function — no IO, no &self —
/// so unit tests can exercise the wire shape directly.
fn build_capability_announcement(
    name: &ProviderName,
    has_instances: bool,
) -> CapabilityAnnouncement {
    CapabilityAnnouncement {
        provider: name.clone(),
        enabled: has_instances,
        capabilities: vec![Capability {
            primitive: Primitive::AudioTranscribe,
            media_inputs: vec![CapabilityMediaInput {
                field: keys::audio::SOURCE.as_str().to_string(),
                delivery: crate::domain::media::MediaDelivery::Transfer,
                accepted_types: vec![
                    "audio/mpeg".to_string(),
                    "audio/mp3".to_string(),
                    "audio/wav".to_string(),
                    "audio/wave".to_string(),
                    "audio/ogg".to_string(),
                    "audio/flac".to_string(),
                    "audio/webm".to_string(),
                    "audio/mp4".to_string(),
                    "audio/m4a".to_string(),
                ],
                overlay: None,
            }],
            parameters: vec![
                SkillParameter { field: "audio.source".into(), required: true, label: Some("Audio File".into()), field_type: Some(ParameterType::String), widget: Some(ParameterWidget::File), ..Default::default() },
                SkillParameter { field: "audio.language".into(), required: false, label: Some("Language".into()), field_type: Some(ParameterType::String), widget: Some(ParameterWidget::Select), placeholder: Some("Auto-detect".into()), ..Default::default() },
            ],
        }],
        skills: Vec::new(),
    }
}

/// Resolve `selectors.model` for WhisperCpp.
///
/// - `None` → returns `default_model`.
/// - `Some(name)` where `name` is in [`MODELS`] → returns the name.
/// - `Some(name)` where `name` is **not** in [`MODELS`] →
///   `Err(PinNotServable)`.
///
/// Whisper has no `recommended:*` resolution because the model
/// catalog is a single static entry; cloud-style monikers reach
/// the unknown branch and surface as `PinNotServable`.
fn resolve_model(input: Option<&str>, default_model: &str) -> Result<String, ProviderError> {
    match input {
        None => Ok(default_model.to_string()),
        Some(m) if MODELS.iter().any(|known| *known == m) => Ok(m.to_string()),
        Some(m) => Err(ProviderError::PinNotServable {
            model: m.to_string(),
            reason: format!("whispercpp serves only: {}", MODELS.join(", ")),
        }),
    }
}

// ── Wire types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
    #[serde(default)]
    language: Option<String>,
}

// ── Tests (ORCH-0030 R2 M4) ──────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::media::MediaDelivery;

    fn provider_name() -> ProviderName {
        ProviderName::new(keys::providers::WHISPERCPP)
    }

    // ── Capability publication ──

    #[test]
    fn announcement_disabled_when_no_instances() {
        let ann = build_capability_announcement(&provider_name(), false);
        assert_eq!(ann.provider.as_str(), "whispercpp");
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
    fn announcement_declares_audio_transcribe_with_transfer_media_input() {
        let ann = build_capability_announcement(&provider_name(), true);
        assert_eq!(ann.capabilities.len(), 1);
        let cap = &ann.capabilities[0];
        assert_eq!(cap.primitive, Primitive::AudioTranscribe);
        assert_eq!(cap.media_inputs.len(), 1);
        let media = &cap.media_inputs[0];
        assert_eq!(media.field, "audio.source");
        assert!(matches!(media.delivery, MediaDelivery::Transfer));
        assert!(media.overlay.is_none());
        // Cover the breadth of audio mime types Whisper accepts.
        assert!(media.accepted_types.contains(&"audio/mpeg".to_string()));
        assert!(media.accepted_types.contains(&"audio/wav".to_string()));
        assert!(media.accepted_types.contains(&"audio/flac".to_string()));
        assert!(media.accepted_types.contains(&"audio/webm".to_string()));
        assert!(media.accepted_types.len() >= 9);
    }

    // ── Model resolution ──

    #[test]
    fn resolve_model_none_returns_default() {
        let m = resolve_model(None, "whisper-1").unwrap();
        assert_eq!(m, "whisper-1");
    }

    #[test]
    fn resolve_model_known_passes_through() {
        let m = resolve_model(Some("whisper-1"), "whisper-1").unwrap();
        assert_eq!(m, "whisper-1");
    }

    #[test]
    fn resolve_model_unknown_returns_pin_not_servable() {
        let err = resolve_model(Some("nope-model"), "whisper-1").unwrap_err();
        match err {
            ProviderError::PinNotServable { model, reason } => {
                assert_eq!(model, "nope-model");
                assert!(reason.contains("whisper-1"));
            }
            other => panic!("expected PinNotServable, got {other:?}"),
        }
    }

    #[test]
    fn resolve_model_recommended_moniker_is_pin_not_servable() {
        // Whisper has no recommended:* resolution; the moniker is
        // not in MODELS, so it surfaces as a pin error rather than
        // silently falling back. Cloud adapters fall back to default;
        // this adapter does not.
        let err = resolve_model(Some("recommended:transcribe"), "whisper-1").unwrap_err();
        assert!(matches!(err, ProviderError::PinNotServable { .. }));
    }
}

