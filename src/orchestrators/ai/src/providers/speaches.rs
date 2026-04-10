//! Speaches provider — `audio.transcribe` via Speaches' OpenAI-
//! compatible `/v1/audio/transcriptions` endpoint (ORCH-0030 R2 M3).
//!
//! Speaches (https://github.com/speaches-ai/speaches) is a GPU-friendly
//! Whisper server exposing the OpenAI transcription wire format. After
//! M3 this adapter is fully self-contained: its own struct, its own
//! HTTP client, its own multipart builder, its own response parsing,
//! its own discovery subscriber, and its own capability publication
//! path. The previously-shared `OpenAiCompatStt` helper is deleted.
//!
//! Unlike the unauthenticated whisper.cpp server, Speaches may be
//! deployed behind an API key — the adapter carries an optional
//! `api_key` field and applies it as a bearer token on every request
//! when present.
//!
//! # Dispatch flow
//!
//! On every `audio.transcribe` request:
//!
//! 1. Pick an instance from the round-robin [`InstancePool`].
//! 2. Resolve the model — use `request.selectors.model` when set,
//!    fall back to `self.default_model` on `None` or on any
//!    `recommended:*` moniker. If the caller pinned something outside
//!    `MODELS`, return [`ProviderError::PinNotServable`].
//! 3. Fetch the audio bytes from the media store via the
//!    `audio.source` media reference (`MediaDelivery::Transfer`).
//! 4. Build an OpenAI-shaped multipart form: `model`, `file`, and
//!    optional `language` / `response_format` fields.
//! 5. POST to `{instance}/v1/audio/transcriptions` (optionally with
//!    bearer auth) and parse the `{text, language?}` JSON response
//!    into the canonical output.

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
use crate::domain::media::MediaDelivery;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{
    Provider, ProviderError, ProviderMeta, ProviderResult, WorkspaceDescription,
};
use crate::domain::request::OrchestratorRequest;
use crate::services::directory_subscriber::publish_capability_announcement;
use crate::services::garden_discovery::GardenDiscovery;

use super::common::{
    build_http_client, check_status, map_reqwest_error, truncate_str, InstancePool,
    PerFqnInstances,
};

// ── Configuration ────────────────────────────────────────────

const FQNS: &[&'static str] = &["speaches"];

/// Models Speaches serves. Checked against `request.selectors.model`
/// in `onboard`; any concrete pin outside this list is rejected with
/// `PinNotServable`.
const MODELS: &[&str] = &[
    "Systran/faster-distil-whisper-large-v3",
    "Systran/faster-whisper-large-v3",
    "Systran/faster-whisper-medium",
    "Systran/faster-whisper-small",
];

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

// ── Provider struct ──────────────────────────────────────────

pub struct SpeachesProvider {
    name: ProviderName,
    instances: Arc<InstancePool>,
    api_key: Option<String>,
    default_model: String,
    http: Client,
    events: Arc<EventBus>,
}

impl SpeachesProvider {
    pub fn new(
        config: SpeachesConfig,
        discovery: Arc<GardenDiscovery>,
        events: Arc<EventBus>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::SPEACHES);
        let provider = Arc::new(Self {
            name,
            instances: Arc::new(InstancePool::new()),
            api_key: config.api_key,
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

    /// Round-robin pick from the current pool. Returns
    /// `ProviderError::Unreachable` when no instances are known.
    fn pick_instance(&self) -> Result<String, ProviderError> {
        self.instances.pick().ok_or_else(|| {
            ProviderError::Unreachable(format!(
                "no {} instances in the garden",
                self.name
            ))
        })
    }

    /// Apply bearer auth to a request builder when `api_key` is set.
    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => rb.bearer_auth(key),
            None => rb,
        }
    }
}

// ── Discovery subscriber ─────────────────────────────────────

fn spawn_subscriber(
    provider: Arc<SpeachesProvider>,
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
impl Provider for SpeachesProvider {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }

    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderResult, ProviderError> {
        if request.action.primitive != Primitive::AudioTranscribe {
            return Err(ProviderError::Unsupported(format!(
                "{} does not serve {}",
                self.name,
                request.action.primitive.dotted()
            )));
        }

        // Adapter-local model resolution.
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
        let model_name = model.clone();
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
            .auth(self.http.post(&endpoint).multipart(form))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "speaches transcription").await?;
        let wire: TranscriptionResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let summary = format!("→ '{}'", truncate_str(&wire.text, 30));
        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, wire.text);
        if let Some(lang) = wire.language {
            out.set(&keys::text::LANGUAGE, lang);
        }
        Ok(ProviderResult::sync_with(
            out,
            ProviderMeta {
                model: Some(model_name),
                instance: Some(base),
                summary: Some(summary),
                ..Default::default()
            },
        ))
    }

    async fn describe_workspace(
        &self,
        primitive: Primitive,
        _model_hint: Option<&str>,
    ) -> Option<WorkspaceDescription> {
        if primitive != Primitive::AudioTranscribe {
            return None;
        }
        let has_instances = !self.instances.is_empty();
        let announcement = build_capability_announcement(&self.name, has_instances);
        let cap = announcement
            .capabilities
            .into_iter()
            .find(|c| c.primitive == primitive)?;
        Some(WorkspaceDescription {
            resolved_model: Some(self.default_model.clone()),
            fields: cap.parameters,
            media_inputs: cap.media_inputs,
            examples: cap.examples,
        })
    }
}

// ── Pure helpers (testable without runtime) ──────────────────

/// Build the capability announcement Speaches publishes given the
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
            priority: 0,
            media_inputs: vec![CapabilityMediaInput {
                field: keys::audio::SOURCE.as_str().to_string(),
                delivery: MediaDelivery::Transfer,
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
                SkillParameter { field: "audio.language.source".into(), required: false, label: Some("Language".into()), field_type: Some(ParameterType::String), widget: Some(ParameterWidget::Select), placeholder: Some("Auto-detect".into()), ..Default::default() },
            ],
            examples: vec![], // Transcription requires audio upload
        }],
        skills: Vec::new(),
    }
}

/// Resolve `selectors.model` for Speaches.
///
/// - `None` → returns `default_model`.
/// - `Some("recommended:*")` → returns `default_model` (Speaches
///   treats recommended monikers as "give me your default").
/// - `Some(name)` where `name` is in [`MODELS`] → returns the name.
/// - `Some(name)` where `name` is **not** in [`MODELS`] →
///   `Err(PinNotServable)`.
fn resolve_model(input: Option<&str>, default_model: &str) -> Result<String, ProviderError> {
    match input {
        None => Ok(default_model.to_string()),
        Some(s) if s.starts_with("recommended:") => Ok(default_model.to_string()),
        Some(m) if MODELS.iter().any(|known| *known == m) => Ok(m.to_string()),
        Some(m) => Err(ProviderError::PinNotServable {
            model: m.to_string(),
            reason: format!("speaches serves only: {}", MODELS.join(", ")),
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
        ProviderName::new(keys::providers::SPEACHES)
    }

    // ── Capability publication ──

    #[test]
    fn announcement_disabled_when_no_instances() {
        let ann = build_capability_announcement(&provider_name(), false);
        assert_eq!(ann.provider.as_str(), "speaches");
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
        let cap = &ann.capabilities[0];
        assert_eq!(cap.primitive, Primitive::AudioTranscribe);
        assert_eq!(cap.media_inputs.len(), 1);
        let media = &cap.media_inputs[0];
        assert_eq!(media.field, "audio.source");
        assert!(matches!(media.delivery, MediaDelivery::Transfer));
        assert!(media.accepted_types.contains(&"audio/mpeg".to_string()));
        assert!(media.accepted_types.contains(&"audio/wav".to_string()));
        assert!(media.accepted_types.len() >= 9);
    }

    // ── Model resolution ──

    #[test]
    fn resolve_model_none_returns_default() {
        let m = resolve_model(None, "Systran/faster-distil-whisper-large-v3").unwrap();
        assert_eq!(m, "Systran/faster-distil-whisper-large-v3");
    }

    #[test]
    fn resolve_model_recommended_moniker_returns_default() {
        // Speaches treats recommended:* as "give me your default".
        let m = resolve_model(Some("recommended:transcribe"), "Systran/faster-whisper-medium").unwrap();
        assert_eq!(m, "Systran/faster-whisper-medium");
    }

    #[test]
    fn resolve_model_known_concrete_passes_through() {
        let m = resolve_model(
            Some("Systran/faster-whisper-large-v3"),
            "Systran/faster-distil-whisper-large-v3",
        )
        .unwrap();
        assert_eq!(m, "Systran/faster-whisper-large-v3");
    }

    #[test]
    fn resolve_model_unknown_returns_pin_not_servable() {
        let err = resolve_model(Some("nope-model"), "Systran/faster-distil-whisper-large-v3").unwrap_err();
        match err {
            ProviderError::PinNotServable { model, reason } => {
                assert_eq!(model, "nope-model");
                assert!(reason.contains("Systran"));
            }
            other => panic!("expected PinNotServable, got {other:?}"),
        }
    }
}
