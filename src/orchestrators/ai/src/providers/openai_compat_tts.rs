//! Shared implementation for OpenAI-compatible TTS providers (Kokoro,
//! OpenedaiSpeech). Both expose `POST /v1/audio/speech` with the same
//! body shape as OpenAI, returning raw audio bytes.
//!
//! This is a private helper — the public providers
//! [`crate::providers::kokoro`] and [`crate::providers::openedai_speech`]
//! wrap this with their own names, defaults, and model catalogs.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use tokio::sync::watch;

use crate::domain::ids::{ModelFqn, ProviderName, RegistrationId};
use crate::domain::keys;
use crate::domain::media::MediaSource;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{
    HonoredField, MediaOutputSpec, Model, Provider, ProviderError, ProviderHealth,
    ProviderOutcome, ProviderState, ProviderStatePublisher, Registration, RegistrationStrategy,
};
use crate::domain::request::OrchestratorRequest;

use crate::services::garden_discovery::GardenDiscovery;
use tokio_util::sync::CancellationToken;

use super::common::{
    build_http_client, check_status, map_reqwest_error, InstancePool, PerFqnInstances,
};

pub struct OpenAiCompatTts {
    pub(crate) name: ProviderName,
    pub(crate) instances: Arc<InstancePool>,
    pub(crate) api_key: Option<String>,
    /// The TTS engine model identifier sent in the wire body's
    /// `model` field. Distinct from the voice id — Kokoro uses
    /// `"kokoro"`, openedai-speech accepts `"tts-1"`. The selected
    /// voice goes into the body's `voice` field.
    pub(crate) tts_model_id: String,
    pub(crate) default_voice: String,
    pub(crate) default_format: String,
    pub(crate) voices: Vec<&'static str>,
    pub(crate) http: Client,
    pub(crate) publisher: ProviderStatePublisher,
}

fn build_registration(name: &ProviderName) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: name.clone(),
        primitive: Primitive::AudioGenerate,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![
            HonoredField::new(keys::audio::TEXT).required(),
            HonoredField::new(keys::audio::VOICE_ID),
            HonoredField::new(keys::audio::VOICE_SPEED),
            HonoredField::new(keys::audio::FORMAT_CODEC),
        ],
        media_inputs: Vec::new(),
        media_outputs: vec![MediaOutputSpec {
            field: keys::audio::MEDIA_ID,
            content_type: "audio/mpeg".to_string(),
        }],
    }
}

fn build_models(name: &ProviderName, voices: &[&'static str]) -> Vec<Model> {
    voices
        .iter()
        .map(|v| Model {
            fqn: ModelFqn::new(name, *v),
            short_name: v.to_string(),
            primitives: vec![Primitive::AudioGenerate],
            capability_tags: vec!["tts".to_string(), "voice".to_string()],
            size_bytes: None,
            context_length: None,
            parameter_count: None,
        })
        .collect()
}

impl OpenAiCompatTts {
    pub fn new(
        name: &'static str,
        fqns: &'static [&'static str],
        api_key: Option<String>,
        tts_model_id: String,
        default_voice: String,
        default_format: String,
        voices: Vec<&'static str>,
        discovery: Arc<GardenDiscovery>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let provider_name = ProviderName::new(name);
        let initial = ProviderState {
            health: ProviderHealth::Offline {
                reason: "no garden instances discovered yet".to_string(),
            },
            registrations: vec![build_registration(&provider_name)],
            models: build_models(&provider_name, &voices),
            performance_hints: Vec::new(),
        };

        let provider = Arc::new(Self {
            name: provider_name,
            instances: Arc::new(InstancePool::new()),
            api_key,
            tts_model_id,
            default_voice,
            default_format,
            voices,
            http: build_http_client(),
            publisher: ProviderStatePublisher::new(initial),
        });
        spawn_subscriber(provider.clone(), fqns, discovery, shutdown);
        provider
    }

    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(k) => rb.bearer_auth(k),
            None => rb,
        }
    }

    fn pick(&self) -> Result<String, ProviderError> {
        self.instances.pick().ok_or_else(|| {
            ProviderError::Unreachable(format!(
                "no {} instances in the garden",
                self.name
            ))
        })
    }
}

impl OpenAiCompatTts {
    fn apply_merged(&self, urls: Vec<String>) {
        if !self.instances.set(urls) {
            return;
        }
        let count = self.instances.len();
        let name = self.name.clone();
        let voices = self.voices.clone();
        self.publisher.modify(move |mut state| {
            state.health = if count == 0 {
                ProviderHealth::Offline {
                    reason: "no garden instances discovered".to_string(),
                }
            } else {
                ProviderHealth::Healthy
            };
            state.registrations = vec![build_registration(&name)];
            state.models = build_models(&name, &voices);
            state
        });
    }
}

fn spawn_subscriber(
    provider: Arc<OpenAiCompatTts>,
    fqns: &'static [&'static str],
    discovery: Arc<GardenDiscovery>,
    shutdown: CancellationToken,
) {
    tokio::spawn(async move {
        let pool = PerFqnInstances::new();
        let mut rx = discovery.subscribe(fqns).await;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                event = rx.recv() => {
                    let Some(event) = event else { break };
                    let urls: Vec<String> = event.instances.into_iter().map(|i| i.url).collect();
                    pool.set(&event.fqn, urls);
                    provider.apply_merged(pool.flatten());
                }
            }
        }
    });
}

#[async_trait]
impl Provider for OpenAiCompatTts {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }

    fn state(&self) -> Arc<ProviderState> {
        self.publisher.snapshot()
    }

    fn subscribe(&self) -> watch::Receiver<Arc<ProviderState>> {
        self.publisher.subscribe()
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

        let text = request
            .payload
            .pointer("/audio/text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProviderError::Unsupported("missing audio.text".to_string()))?
            .to_string();
        let voice = request
            .resolved_model
            .as_ref()
            .map(|m| m.short_name.clone())
            .or_else(|| {
                request
                    .payload
                    .pointer("/audio/voice/id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| self.default_voice.clone());
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

        let mut body = json!({
            "model": self.tts_model_id,
            "input": text,
            "voice": voice,
            "response_format": format,
        });
        if let Some(s) = speed {
            body["speed"] = json!(s);
        }

        let base = self.pick()?;
        let endpoint = format!(
            "{}/v1/audio/speech",
            base.trim_end_matches('/')
        );
        let resp = self
            .auth(self.http.post(&endpoint).json(&body))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "tts speech").await?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let content_type = match format.as_str() {
            "mp3" => "audio/mpeg",
            "wav" => "audio/wav",
            "opus" => "audio/ogg",
            "flac" => "audio/flac",
            "aac" => "audio/aac",
            "pcm" => "audio/L16",
            _ => "application/octet-stream",
        };

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

        let mut out = Output::new();
        out.set(&keys::audio::MEDIA_ID, entry.id.as_str());
        out.set(&keys::audio::FORMAT, format);
        Ok(ProviderOutcome::Sync(out))
    }
}
