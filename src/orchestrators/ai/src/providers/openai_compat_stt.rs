//! Shared implementation for OpenAI-compatible STT providers
//! (WhisperCpp, Speaches). Both expose
//! `POST /v1/audio/transcriptions` with the OpenAI multipart shape:
//! `file` field carrying audio bytes, plus `model`/`language`/etc.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::watch;

use crate::domain::ids::{ModelFqn, ProviderName, RegistrationId};
use crate::domain::keys;
use crate::domain::media::MediaDelivery;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{
    HonoredField, MediaInputSpec, Model, Provider, ProviderError, ProviderHealth,
    ProviderOutcome, ProviderState, ProviderStatePublisher, Registration, RegistrationStrategy,
};
use crate::domain::request::OrchestratorRequest;

use crate::services::garden_discovery::GardenDiscovery;
use tokio_util::sync::CancellationToken;

use super::common::{
    build_http_client, check_status, map_reqwest_error, InstancePool, PerFqnInstances,
};

pub struct OpenAiCompatStt {
    name: ProviderName,
    instances: Arc<InstancePool>,
    api_key: Option<String>,
    default_model: String,
    available_models: Vec<&'static str>,
    http: Client,
    publisher: ProviderStatePublisher,
}

fn build_registration(name: &ProviderName) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: name.clone(),
        primitive: Primitive::AudioTranscribe,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![
            HonoredField::new(keys::audio::SOURCE).required(),
            HonoredField::new(keys::audio::LANGUAGE_SOURCE),
        ],
        media_inputs: vec![MediaInputSpec {
            field: keys::audio::SOURCE,
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
        media_outputs: Vec::new(),
    }
}

fn build_models(name: &ProviderName, available: &[&'static str]) -> Vec<Model> {
    available
        .iter()
        .map(|m| Model {
            fqn: ModelFqn::new(name, *m),
            short_name: m.to_string(),
            primitives: vec![Primitive::AudioTranscribe],
            capability_tags: vec!["stt".to_string(), "whisper".to_string()],
            size_bytes: None,
            context_length: None,
            parameter_count: None,
        })
        .collect()
}

impl OpenAiCompatStt {
    pub fn new(
        name: &'static str,
        fqns: &'static [&'static str],
        api_key: Option<String>,
        default_model: String,
        available_models: Vec<&'static str>,
        discovery: Arc<GardenDiscovery>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        let provider_name = ProviderName::new(name);
        let initial = ProviderState {
            health: ProviderHealth::Offline {
                reason: "no garden instances discovered yet".to_string(),
            },
            registrations: vec![build_registration(&provider_name)],
            models: build_models(&provider_name, &available_models),
            performance_hints: Vec::new(),
        };

        let provider = Arc::new(Self {
            name: provider_name,
            instances: Arc::new(InstancePool::new()),
            api_key,
            default_model,
            available_models,
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

impl OpenAiCompatStt {
    fn apply_merged(&self, urls: Vec<String>) {
        if !self.instances.set(urls) {
            return;
        }
        let count = self.instances.len();
        let name = self.name.clone();
        let available = self.available_models.clone();
        self.publisher.modify(move |mut state| {
            state.health = if count == 0 {
                ProviderHealth::Offline {
                    reason: "no garden instances discovered".to_string(),
                }
            } else {
                ProviderHealth::Healthy
            };
            state.registrations = vec![build_registration(&name)];
            state.models = build_models(&name, &available);
            state
        });
    }
}

fn spawn_subscriber(
    provider: Arc<OpenAiCompatStt>,
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
impl Provider for OpenAiCompatStt {
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
        if request.action.primitive != Primitive::AudioTranscribe {
            return Err(ProviderError::Unsupported(format!(
                "{} does not serve {}",
                self.name,
                request.action.primitive.dotted()
            )));
        }

        let model = request
            .resolved_model
            .as_ref()
            .map(|m| m.short_name.clone())
            .unwrap_or_else(|| self.default_model.clone());

        // Transfer delivery: pull bytes from the media store and
        // construct our own multipart body.
        let media_ref = request
            .media
            .find_at_field(&keys::audio::SOURCE)
            .ok_or_else(|| {
                ProviderError::Unsupported("audio.source media reference missing".to_string())
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

        let base = self.pick()?;
        let endpoint = format!(
            "{}/v1/audio/transcriptions",
            base.trim_end_matches('/')
        );
        let resp = self
            .auth(self.http.post(&endpoint).multipart(form))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "stt transcription").await?;
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

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
    #[serde(default)]
    language: Option<String>,
}
