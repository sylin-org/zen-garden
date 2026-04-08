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
    Capability, CapabilityAnnouncement, CapabilityMediaInput,
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
        let enabled = !self.instances.is_empty();
        let announcement = CapabilityAnnouncement {
            provider: self.name.clone(),
            enabled,
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
            }],
            skills: Vec::new(),
        };
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
        let model = match request.selectors.model.as_deref() {
            None => self.default_model.clone(),
            Some(m) if MODELS.iter().any(|known| *known == m) => m.to_string(),
            Some(m) => {
                return Err(ProviderError::PinNotServable {
                    model: m.to_string(),
                    reason: format!(
                        "whispercpp serves only: {}",
                        MODELS.join(", ")
                    ),
                });
            }
        };

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

// ── Wire types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
    #[serde(default)]
    language: Option<String>,
}
