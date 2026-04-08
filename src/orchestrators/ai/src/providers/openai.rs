//! OpenAI provider — the widest vendor surface.
//!
//! Primitives:
//! - `text.chat`             → POST /v1/chat/completions
//! - `text.embed`            → POST /v1/embeddings
//! - `image.analyze`         → POST /v1/chat/completions with an
//!                             `image_url` content part (base64 data URL).
//! - `image.generate`        → POST /v1/images/generations
//! - `audio.generate`        → POST /v1/audio/speech (bytes)
//! - `audio.transcribe`      → POST /v1/audio/transcriptions
//!                             (multipart upload — Transfer mode)
//!
//! Auth: `Authorization: Bearer <api_key>`.

use std::sync::Arc;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::watch;

use crate::domain::ids::{ModelFqn, ProviderName, RegistrationId};
use crate::domain::keys;
use crate::domain::media::{MediaDelivery, MediaSource};
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{
    HonoredField, MediaInputSpec, Model, ModelDescriptor, Provider, ProviderError,
    ProviderHealth, ProviderOutcome, ProviderState, ProviderStatePublisher, Registration,
    RegistrationStrategy,
};
use crate::domain::request::OrchestratorRequest;

use super::common::{build_http_client, check_status, map_reqwest_error};

#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    pub base_url: String,
    pub api_key: String,
    /// Optional organization header, mirrored from the environment.
    pub organization: Option<String>,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com".to_string(),
            api_key: String::new(),
            organization: None,
        }
    }
}

/// Curated model list. OpenAI's /v1/models endpoint returns every
/// model including deprecated ones — using a curated list keeps the
/// directory focused on what actually serves each primitive. The
/// list is updated via code review when OpenAI publishes new models.
const TEXT_CHAT_MODELS: &[&str] = &[
    "gpt-4o",
    "gpt-4o-mini",
    "gpt-4-turbo",
    "gpt-4",
    "gpt-3.5-turbo",
];
const TEXT_EMBED_MODELS: &[&str] = &[
    "text-embedding-3-large",
    "text-embedding-3-small",
    "text-embedding-ada-002",
];
const IMAGE_GEN_MODELS: &[&str] = &["dall-e-3", "dall-e-2", "gpt-image-1"];
const AUDIO_TTS_MODELS: &[&str] = &["tts-1", "tts-1-hd"];
const AUDIO_STT_MODELS: &[&str] = &["whisper-1"];

pub struct OpenAiProvider {
    name: ProviderName,
    config: OpenAiConfig,
    http: Client,
    publisher: ProviderStatePublisher,
}

impl OpenAiProvider {
    pub fn new(config: OpenAiConfig) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::OPENAI);

        let registrations = vec![
            build_chat_registration(&name),
            build_embed_registration(&name),
            build_analyze_registration(&name),
            build_image_generate_registration(&name),
            build_audio_generate_registration(&name),
            build_audio_transcribe_registration(&name),
        ];

        let mut models: Vec<Model> = Vec::new();
        for m in TEXT_CHAT_MODELS {
            models.push(Model {
                fqn: ModelFqn::new(&name, *m),
                short_name: m.to_string(),
                primitives: vec![Primitive::TextChat, Primitive::ImageAnalyze],
                capability_tags: vec!["chat".to_string(), "vision".to_string()],
                size_bytes: None,
                context_length: Some(128_000),
                parameter_count: None,
            });
        }
        for m in TEXT_EMBED_MODELS {
            models.push(Model {
                fqn: ModelFqn::new(&name, *m),
                short_name: m.to_string(),
                primitives: vec![Primitive::TextEmbed],
                capability_tags: vec!["embed".to_string()],
                size_bytes: None,
                context_length: None,
                parameter_count: None,
            });
        }
        for m in IMAGE_GEN_MODELS {
            models.push(Model {
                fqn: ModelFqn::new(&name, *m),
                short_name: m.to_string(),
                primitives: vec![Primitive::ImageGenerate],
                capability_tags: vec!["generate".to_string()],
                size_bytes: None,
                context_length: None,
                parameter_count: None,
            });
        }
        for m in AUDIO_TTS_MODELS {
            models.push(Model {
                fqn: ModelFqn::new(&name, *m),
                short_name: m.to_string(),
                primitives: vec![Primitive::AudioGenerate],
                capability_tags: vec!["tts".to_string()],
                size_bytes: None,
                context_length: None,
                parameter_count: None,
            });
        }
        for m in AUDIO_STT_MODELS {
            models.push(Model {
                fqn: ModelFqn::new(&name, *m),
                short_name: m.to_string(),
                primitives: vec![Primitive::AudioTranscribe],
                capability_tags: vec!["stt".to_string(), "whisper".to_string()],
                size_bytes: None,
                context_length: None,
                parameter_count: None,
            });
        }

        let _ = (&MODEL_DESCRIPTORS_UNUSED_HINT, &models);
        let initial = ProviderState {
            health: if config.api_key.is_empty() {
                ProviderHealth::Degraded {
                    reason: "missing OPENAI_API_KEY".to_string(),
                }
            } else {
                ProviderHealth::Healthy
            },
            registrations,
            models,
            performance_hints: Vec::new(),
        };

        Arc::new(Self {
            name,
            config,
            http: build_http_client(),
            publisher: ProviderStatePublisher::new(initial),
        })
    }

    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut rb = rb.bearer_auth(&self.config.api_key);
        if let Some(org) = &self.config.organization {
            rb = rb.header("OpenAI-Organization", org);
        }
        rb
    }
}

/// Placeholder to silence unused-import diagnostics on
/// `ModelDescriptor` (we build `Model` instances directly rather
/// than using the descriptor path because the OpenAI primitives
/// span multiple per-primitive model lists).
#[allow(dead_code)]
const MODEL_DESCRIPTORS_UNUSED_HINT: Option<ModelDescriptor> = None;

#[async_trait]
impl Provider for OpenAiProvider {
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
        if self.config.api_key.is_empty() {
            return Err(ProviderError::AuthFailed(
                "openai api key is not configured".to_string(),
            ));
        }
        match request.action.primitive {
            Primitive::TextChat => self.do_chat(request, false).await,
            Primitive::ImageAnalyze => self.do_chat(request, true).await,
            Primitive::TextEmbed => self.do_embed(request).await,
            Primitive::ImageGenerate => self.do_image_generate(request).await,
            Primitive::AudioGenerate => self.do_audio_generate(request).await,
            Primitive::AudioTranscribe => self.do_audio_transcribe(request).await,
            other => Err(ProviderError::Unsupported(format!(
                "openai does not serve {}",
                other.dotted()
            ))),
        }
    }
}

impl OpenAiProvider {
    async fn do_chat(
        &self,
        request: OrchestratorRequest,
        with_image: bool,
    ) -> Result<ProviderOutcome, ProviderError> {
        let model = request
            .resolved_model
            .as_ref()
            .ok_or_else(|| ProviderError::Unsupported("model selector required".to_string()))?
            .short_name
            .clone();

        // Build messages array.
        let mut messages: Vec<Value> = Vec::new();
        if let Some(system) = request
            .payload
            .pointer("/text/prompt/system")
            .and_then(|v| v.as_str())
        {
            messages.push(json!({"role": "system", "content": system}));
        }
        if let Some(previous) = request
            .payload
            .pointer("/text/prompt/previous")
            .and_then(|v| v.as_array())
        {
            for turn in previous {
                if let (Some(u), Some(a)) = (
                    turn.get("user").and_then(|v| v.as_str()),
                    turn.get("assistant").and_then(|v| v.as_str()),
                ) {
                    messages.push(json!({"role": "user", "content": u}));
                    messages.push(json!({"role": "assistant", "content": a}));
                }
            }
        }
        let user_text = request
            .payload
            .pointer("/text/prompt/user")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if with_image {
            let b64 = request
                .payload
                .pointer("/image/source/base64")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ProviderError::Unsupported(
                        "image.source.base64 missing — media resolver should have inlined it"
                            .to_string(),
                    )
                })?
                .to_string();
            let ct = request
                .payload
                .pointer("/image/source/content_type")
                .and_then(|v| v.as_str())
                .unwrap_or("image/png")
                .to_string();
            let data_url = format!("data:{ct};base64,{b64}");
            messages.push(json!({
                "role": "user",
                "content": [
                    {"type": "text", "text": if user_text.is_empty() { "Describe this image.".to_string() } else { user_text }},
                    {"type": "image_url", "image_url": {"url": data_url}}
                ]
            }));
        } else {
            messages.push(json!({"role": "user", "content": user_text}));
        }

        let mut body = json!({
            "model": model,
            "messages": messages,
        });
        if let Some(v) = request
            .payload
            .pointer("/text/tokens/max")
            .and_then(|v| v.as_u64())
        {
            body["max_tokens"] = json!(v);
        }
        if let Some(v) = request
            .payload
            .pointer("/text/sampling/temperature")
            .and_then(|v| v.as_f64())
        {
            body["temperature"] = json!(v);
        }
        if let Some(v) = request
            .payload
            .pointer("/text/sampling/top_p")
            .and_then(|v| v.as_f64())
        {
            body["top_p"] = json!(v);
        }
        if let Some(v) = request
            .payload
            .pointer("/text/sampling/seed")
            .and_then(|v| v.as_i64())
        {
            body["seed"] = json!(v);
        }
        if let Some(v) = request
            .payload
            .pointer("/text/stop/sequences")
            .and_then(|v| v.as_array())
        {
            body["stop"] = Value::Array(v.clone());
        }
        if let Some(tools) = request
            .payload
            .pointer("/text/tools/definitions")
            .and_then(|v| v.as_array())
        {
            body["tools"] = Value::Array(tools.clone());
        }
        if let Some(c) = request
            .payload
            .pointer("/text/tools/choice")
            .and_then(|v| v.as_str())
        {
            body["tool_choice"] = Value::String(c.to_string());
        }
        if request
            .payload
            .pointer("/text/format/response")
            .and_then(|v| v.as_str())
            == Some("json")
        {
            body["response_format"] = json!({"type": "json_object"});
        }

        let endpoint = format!(
            "{}/v1/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let resp = self
            .auth(self.http.post(&endpoint).json(&body))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "openai chat").await?;
        let wire: ChatResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let choice = wire
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::Upstream("no choice in response".to_string()))?;

        let mut out = Output::new();
        if let Some(content) = choice.message.content {
            out.set(&keys::text::RESPONSE, content);
        }
        let finish = match choice.finish_reason.as_deref() {
            Some("stop") => keys::text::values::FINISH_REASON_STOP,
            Some("length") => keys::text::values::FINISH_REASON_LENGTH,
            Some("tool_calls") | Some("function_call") => {
                keys::text::values::FINISH_REASON_TOOL_CALLS
            }
            Some("content_filter") => keys::text::values::FINISH_REASON_CONTENT_FILTER,
            _ => keys::text::values::FINISH_REASON_STOP,
        };
        out.set(&keys::text::FINISH_REASON, finish);
        if let Some(tc) = choice.message.tool_calls {
            out.set(
                &keys::text::TOOL_CALLS,
                serde_json::to_value(tc).unwrap_or(Value::Null),
            );
        }
        if let Some(u) = wire.usage {
            out.set(&keys::usage::TOKENS_INPUT, u.prompt_tokens);
            out.set(&keys::usage::TOKENS_OUTPUT, u.completion_tokens);
            out.set(&keys::usage::TOKENS_TOTAL, u.total_tokens);
        }
        Ok(ProviderOutcome::Sync(out))
    }

    async fn do_embed(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        let model = request
            .resolved_model
            .as_ref()
            .ok_or_else(|| ProviderError::Unsupported("model selector required".to_string()))?
            .short_name
            .clone();
        let input = request
            .payload
            .pointer("/text/input")
            .cloned()
            .ok_or_else(|| ProviderError::Unsupported("missing text.input".to_string()))?;
        let inputs: Vec<String> = match input {
            Value::String(s) => vec![s],
            Value::Array(a) => a
                .into_iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            _ => {
                return Err(ProviderError::Unsupported(
                    "text.input must be string or array".to_string(),
                ));
            }
        };

        let mut body = json!({"model": model, "input": inputs});
        if let Some(dims) = request
            .payload
            .pointer("/text/dimensions")
            .and_then(|v| v.as_u64())
        {
            body["dimensions"] = json!(dims);
        }

        let endpoint = format!(
            "{}/v1/embeddings",
            self.config.base_url.trim_end_matches('/')
        );
        let resp = self
            .auth(self.http.post(&endpoint).json(&body))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "openai embeddings").await?;
        let wire: EmbeddingsResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let mut out = Output::new();
        let vectors: Vec<Vec<f32>> = wire.data.into_iter().map(|e| e.embedding).collect();
        out.set(
            &keys::text::EMBEDDINGS,
            serde_json::to_value(vectors).unwrap_or(Value::Null),
        );
        if let Some(u) = wire.usage {
            out.set(&keys::usage::TOKENS_INPUT, u.prompt_tokens);
            out.set(&keys::usage::TOKENS_TOTAL, u.total_tokens);
        }
        Ok(ProviderOutcome::Sync(out))
    }

    async fn do_image_generate(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        let model = request
            .resolved_model
            .as_ref()
            .map(|m| m.short_name.clone())
            .unwrap_or_else(|| "dall-e-3".to_string());
        let prompt = request
            .payload
            .pointer("/image/prompt/positive")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProviderError::Unsupported("missing image.prompt.positive".to_string()))?
            .to_string();

        let mut body = json!({
            "model": model,
            "prompt": prompt,
            "n": 1,
            "response_format": "b64_json",
        });
        let width = request
            .payload
            .pointer("/image/dimensions/width")
            .and_then(|v| v.as_u64());
        let height = request
            .payload
            .pointer("/image/dimensions/height")
            .and_then(|v| v.as_u64());
        if let (Some(w), Some(h)) = (width, height) {
            body["size"] = Value::String(format!("{w}x{h}"));
        }
        if let Some(q) = request
            .payload
            .pointer("/image/style/quality")
            .and_then(|v| v.as_str())
        {
            body["quality"] = Value::String(q.to_string());
        }

        let endpoint = format!(
            "{}/v1/images/generations",
            self.config.base_url.trim_end_matches('/')
        );
        let resp = self
            .auth(self.http.post(&endpoint).json(&body))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "openai images").await?;
        let wire: ImagesResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let first = wire
            .data
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::Upstream("no image in response".to_string()))?;
        let bytes_raw = first
            .b64_json
            .ok_or_else(|| ProviderError::Upstream("no b64_json in image response".to_string()))?;
        let bytes = BASE64
            .decode(bytes_raw.as_bytes())
            .map_err(|e| ProviderError::Upstream(format!("base64 decode: {e}")))?;

        // Store in the media store so the response carries a media_id.
        let entry = request
            .context
            .media_store
            .put(
                bytes::Bytes::from(bytes),
                "image/png".to_string(),
                MediaSource::generated(
                    self.name.clone(),
                    request.action.dotted(),
                    request.id.clone(),
                ),
            )
            .await
            .map_err(|e| ProviderError::Internal(format!("media store: {e}")))?;

        let mut out = Output::new();
        out.set(&keys::image::MEDIA_ID, entry.id.as_str());
        if let Some(w) = width {
            out.set(&keys::image::WIDTH, w);
        }
        if let Some(h) = height {
            out.set(&keys::image::HEIGHT, h);
        }
        out.set(&keys::image::MODEL, model);
        Ok(ProviderOutcome::Sync(out))
    }

    async fn do_audio_generate(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        let model = request
            .resolved_model
            .as_ref()
            .map(|m| m.short_name.clone())
            .unwrap_or_else(|| "tts-1".to_string());
        let input = request
            .payload
            .pointer("/audio/text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ProviderError::Unsupported("missing audio.text".to_string()))?
            .to_string();
        let voice = request
            .payload
            .pointer("/audio/voice/id")
            .and_then(|v| v.as_str())
            .unwrap_or("alloy")
            .to_string();
        let format = request
            .payload
            .pointer("/audio/format/codec")
            .and_then(|v| v.as_str())
            .unwrap_or("mp3")
            .to_string();

        let body = json!({
            "model": model,
            "input": input,
            "voice": voice,
            "response_format": format,
        });
        let endpoint = format!(
            "{}/v1/audio/speech",
            self.config.base_url.trim_end_matches('/')
        );
        let resp = self
            .auth(self.http.post(&endpoint).json(&body))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "openai audio speech").await?;
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

    async fn do_audio_transcribe(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        let model = request
            .resolved_model
            .as_ref()
            .map(|m| m.short_name.clone())
            .unwrap_or_else(|| "whisper-1".to_string());

        // Transfer mode: grab the referenced audio via the media store,
        // construct a multipart body ourselves, and post.
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

        let endpoint = format!(
            "{}/v1/audio/transcriptions",
            self.config.base_url.trim_end_matches('/')
        );
        let resp = self
            .auth(self.http.post(&endpoint).multipart(form))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "openai transcription").await?;
        let wire: TranscriptionResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, wire.text);
        Ok(ProviderOutcome::Sync(out))
    }
}

// ── Registration builders ─────────────────────────────────────

fn build_chat_registration(provider: &ProviderName) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: provider.clone(),
        primitive: Primitive::TextChat,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![
            HonoredField::new(keys::text::PROMPT_USER).required(),
            HonoredField::new(keys::text::PROMPT_SYSTEM),
            HonoredField::new(keys::text::PROMPT_PREVIOUS),
            HonoredField::new(keys::text::TOKENS_MAX),
            HonoredField::new(keys::text::SAMPLING_TEMPERATURE),
            HonoredField::new(keys::text::SAMPLING_TOP_P),
            HonoredField::new(keys::text::SAMPLING_SEED),
            HonoredField::new(keys::text::STOP_SEQUENCES),
            HonoredField::new(keys::text::TOOLS_DEFINITIONS),
            HonoredField::new(keys::text::TOOLS_CHOICE),
            HonoredField::new(keys::text::FORMAT_RESPONSE),
        ],
        media_inputs: Vec::new(),
        media_outputs: Vec::new(),
    }
}

fn build_embed_registration(provider: &ProviderName) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: provider.clone(),
        primitive: Primitive::TextEmbed,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![
            HonoredField::new(keys::text::INPUT).required(),
            HonoredField::new(keys::text::DIMENSIONS),
        ],
        media_inputs: Vec::new(),
        media_outputs: Vec::new(),
    }
}

fn build_analyze_registration(provider: &ProviderName) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: provider.clone(),
        primitive: Primitive::ImageAnalyze,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![
            HonoredField::new(keys::image::SOURCE).required(),
            HonoredField::new(keys::text::PROMPT_USER),
            HonoredField::new(keys::text::TOKENS_MAX),
        ],
        media_inputs: vec![MediaInputSpec {
            field: keys::image::SOURCE,
            delivery: MediaDelivery::Base64,
            accepted_types: vec![
                "image/png".to_string(),
                "image/jpeg".to_string(),
                "image/webp".to_string(),
                "image/gif".to_string(),
            ],
            overlay: None,
        }],
        media_outputs: Vec::new(),
    }
}

fn build_image_generate_registration(provider: &ProviderName) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: provider.clone(),
        primitive: Primitive::ImageGenerate,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![
            HonoredField::new(keys::image::PROMPT_POSITIVE).required(),
            HonoredField::new(keys::image::DIMENSIONS_WIDTH),
            HonoredField::new(keys::image::DIMENSIONS_HEIGHT),
            HonoredField::new(keys::image::STYLE_QUALITY),
        ],
        media_inputs: Vec::new(),
        media_outputs: vec![crate::domain::provider::MediaOutputSpec {
            field: keys::image::MEDIA_ID,
            content_type: "image/png".to_string(),
        }],
    }
}

fn build_audio_generate_registration(provider: &ProviderName) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: provider.clone(),
        primitive: Primitive::AudioGenerate,
        strategy: RegistrationStrategy::Bare,
        honored_fields: vec![
            HonoredField::new(keys::audio::TEXT).required(),
            HonoredField::new(keys::audio::VOICE_ID),
            HonoredField::new(keys::audio::FORMAT_CODEC),
        ],
        media_inputs: Vec::new(),
        media_outputs: vec![crate::domain::provider::MediaOutputSpec {
            field: keys::audio::MEDIA_ID,
            content_type: "audio/mpeg".to_string(),
        }],
    }
}

fn build_audio_transcribe_registration(provider: &ProviderName) -> Registration {
    Registration {
        id: RegistrationId::generate(),
        provider: provider.clone(),
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
                "audio/wav".to_string(),
                "audio/ogg".to_string(),
                "audio/flac".to_string(),
                "audio/webm".to_string(),
                "audio/mp4".to_string(),
            ],
            overlay: None,
        }],
        media_outputs: Vec::new(),
    }
}

// ── Wire types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<Value>>,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingEntry>,
    #[serde(default)]
    usage: Option<EmbeddingsUsage>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingEntry {
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingsUsage {
    prompt_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct ImagesResponse {
    data: Vec<ImageEntry>,
}

#[derive(Debug, Deserialize)]
struct ImageEntry {
    #[serde(default)]
    b64_json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
}
