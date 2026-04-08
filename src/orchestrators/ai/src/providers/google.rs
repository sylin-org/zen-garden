//! Google Gemini provider — `text.chat`, `text.embed`,
//! `image.analyze`.
//!
//! Wire API (`https://generativelanguage.googleapis.com/v1beta`):
//!
//! - `POST /models/{model}:generateContent?key=<key>` — chat + vision
//! - `POST /models/{model}:embedContent?key=<key>` — embeddings
//!
//! Content shape: `{contents: [{role, parts: [{text|inlineData}]}]}`.
//! Vision uses Base64 delivery inline — the `inlineData` part carries
//! `{mimeType, data}`.
//!
//! Scope note: Google also exposes Cloud TTS, Cloud Speech, and
//! Imagen for audio/image generation, but those are distinct APIs
//! with separate auth (service account) and are deliberately not
//! registered by this provider. A caller targeting
//! `audio.generate` / `audio.transcribe` / `image.generate` on
//! Google would need a dedicated provider; OpenAI and local
//! providers already cover those primitives.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
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

use super::common::{build_http_client, check_status, map_reqwest_error};

#[derive(Debug, Clone)]
pub struct GoogleConfig {
    pub base_url: String,
    pub api_key: String,
}

impl Default for GoogleConfig {
    fn default() -> Self {
        Self {
            base_url: "https://generativelanguage.googleapis.com".to_string(),
            api_key: String::new(),
        }
    }
}

const CHAT_MODELS: &[&str] = &[
    "gemini-2.0-flash-exp",
    "gemini-1.5-pro",
    "gemini-1.5-flash",
    "gemini-1.5-flash-8b",
];
const EMBED_MODELS: &[&str] = &["text-embedding-004", "embedding-001"];

pub struct GoogleProvider {
    name: ProviderName,
    config: GoogleConfig,
    http: Client,
    publisher: ProviderStatePublisher,
}

impl GoogleProvider {
    pub fn new(config: GoogleConfig) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::GOOGLE);

        let registrations = vec![
            build_chat_registration(&name),
            build_embed_registration(&name),
            build_analyze_registration(&name),
        ];

        let mut models: Vec<Model> = Vec::new();
        for m in CHAT_MODELS {
            models.push(Model {
                fqn: ModelFqn::new(&name, *m),
                short_name: m.to_string(),
                primitives: vec![Primitive::TextChat, Primitive::ImageAnalyze],
                capability_tags: vec!["chat".to_string(), "vision".to_string()],
                size_bytes: None,
                context_length: Some(1_000_000),
                parameter_count: None,
            });
        }
        for m in EMBED_MODELS {
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

        let initial = ProviderState {
            health: if config.api_key.is_empty() {
                ProviderHealth::Degraded {
                    reason: "missing GOOGLE_API_KEY".to_string(),
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
}

#[async_trait]
impl Provider for GoogleProvider {
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
                "google api key is not configured".to_string(),
            ));
        }
        match request.action.primitive {
            Primitive::TextChat => self.do_chat(request, false).await,
            Primitive::ImageAnalyze => self.do_chat(request, true).await,
            Primitive::TextEmbed => self.do_embed(request).await,
            other => Err(ProviderError::Unsupported(format!(
                "google does not serve {}",
                other.dotted()
            ))),
        }
    }
}

impl GoogleProvider {
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

        // Build contents array. Gemini uses `role: "user"` or
        // `role: "model"` and each message holds `parts: [...]`.
        let mut contents: Vec<Value> = Vec::new();
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
                    contents.push(json!({"role": "user", "parts": [{"text": u}]}));
                    contents.push(json!({"role": "model", "parts": [{"text": a}]}));
                }
            }
        }

        let user_text = request
            .payload
            .pointer("/text/prompt/user")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut parts: Vec<Value> = Vec::new();
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
            let mime = request
                .payload
                .pointer("/image/source/content_type")
                .and_then(|v| v.as_str())
                .unwrap_or("image/png")
                .to_string();
            parts.push(json!({"inlineData": {"mimeType": mime, "data": b64}}));
        }
        parts.push(json!({"text": if user_text.is_empty() && with_image {
            "Describe this image.".to_string()
        } else {
            user_text
        }}));
        contents.push(json!({"role": "user", "parts": parts}));

        // System instruction (Gemini calls it `systemInstruction`).
        let mut body = json!({"contents": contents});
        if let Some(sys) = request
            .payload
            .pointer("/text/prompt/system")
            .and_then(|v| v.as_str())
        {
            body["systemInstruction"] = json!({"parts": [{"text": sys}]});
        }
        let mut generation_config = serde_json::Map::new();
        if let Some(v) = request
            .payload
            .pointer("/text/tokens/max")
            .and_then(|v| v.as_u64())
        {
            generation_config.insert("maxOutputTokens".to_string(), json!(v));
        }
        if let Some(v) = request
            .payload
            .pointer("/text/sampling/temperature")
            .and_then(|v| v.as_f64())
        {
            generation_config.insert("temperature".to_string(), json!(v));
        }
        if let Some(v) = request
            .payload
            .pointer("/text/sampling/top_p")
            .and_then(|v| v.as_f64())
        {
            generation_config.insert("topP".to_string(), json!(v));
        }
        if let Some(v) = request
            .payload
            .pointer("/text/sampling/top_k")
            .and_then(|v| v.as_i64())
        {
            generation_config.insert("topK".to_string(), json!(v));
        }
        if let Some(stops) = request
            .payload
            .pointer("/text/stop/sequences")
            .and_then(|v| v.as_array())
        {
            generation_config.insert("stopSequences".to_string(), Value::Array(stops.clone()));
        }
        if !generation_config.is_empty() {
            body["generationConfig"] = Value::Object(generation_config);
        }

        let endpoint = format!(
            "{}/v1beta/models/{model}:generateContent?key={}",
            self.config.base_url.trim_end_matches('/'),
            urlencode(&self.config.api_key),
        );
        let resp = self
            .http
            .post(&endpoint)
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "google generateContent").await?;
        let wire: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        let candidate = wire
            .candidates
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::Upstream("no candidate in response".to_string()))?;

        let mut text = String::new();
        for part in candidate.content.parts {
            if let Some(t) = part.text {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&t);
            }
        }

        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, text);
        let finish = match candidate.finish_reason.as_deref() {
            Some("STOP") => keys::text::values::FINISH_REASON_STOP,
            Some("MAX_TOKENS") => keys::text::values::FINISH_REASON_LENGTH,
            Some("SAFETY") | Some("RECITATION") => {
                keys::text::values::FINISH_REASON_CONTENT_FILTER
            }
            _ => keys::text::values::FINISH_REASON_STOP,
        };
        out.set(&keys::text::FINISH_REASON, finish);
        if let Some(u) = wire.usage_metadata {
            out.set(&keys::usage::TOKENS_INPUT, u.prompt_token_count);
            out.set(&keys::usage::TOKENS_OUTPUT, u.candidates_token_count);
            out.set(&keys::usage::TOKENS_TOTAL, u.total_token_count);
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

        // Gemini requires one request per document for embedContent.
        // Run them sequentially — parallel is possible but adds
        // complexity for v1.
        let endpoint_template = format!(
            "{}/v1beta/models/{model}:embedContent?key={}",
            self.config.base_url.trim_end_matches('/'),
            urlencode(&self.config.api_key),
        );

        let mut vectors: Vec<Vec<f32>> = Vec::with_capacity(inputs.len());
        for text in &inputs {
            let body = json!({
                "content": {"parts": [{"text": text}]}
            });
            let resp = self
                .http
                .post(&endpoint_template)
                .json(&body)
                .send()
                .await
                .map_err(map_reqwest_error)?;
            let resp = check_status(resp, "google embedContent").await?;
            let wire: EmbedResponse = resp
                .json()
                .await
                .map_err(|e| ProviderError::Upstream(e.to_string()))?;
            vectors.push(wire.embedding.values);
        }

        let mut out = Output::new();
        out.set(
            &keys::text::EMBEDDINGS,
            serde_json::to_value(vectors).unwrap_or(Value::Null),
        );
        Ok(ProviderOutcome::Sync(out))
    }
}

fn urlencode(s: &str) -> String {
    // Minimal URL-encoder for API keys (alphanumeric + a handful of
    // safe chars).
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            for byte in c.to_string().bytes() {
                out.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    out
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
            HonoredField::new(keys::text::SAMPLING_TOP_K),
            HonoredField::new(keys::text::STOP_SEQUENCES),
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
        honored_fields: vec![HonoredField::new(keys::text::INPUT).required()],
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
        ],
        media_inputs: vec![MediaInputSpec {
            field: keys::image::SOURCE,
            delivery: MediaDelivery::Base64,
            accepted_types: vec![
                "image/png".to_string(),
                "image/jpeg".to_string(),
                "image/webp".to_string(),
                "image/heic".to_string(),
                "image/heif".to_string(),
            ],
            overlay: None,
        }],
        media_outputs: Vec::new(),
    }
}

// ── Wire types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GenerateResponse {
    candidates: Vec<GenerateCandidate>,
    #[serde(default, rename = "usageMetadata")]
    usage_metadata: Option<UsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GenerateCandidate {
    content: GenerateContent,
    #[serde(default, rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GenerateContent {
    parts: Vec<GeneratePart>,
}

#[derive(Debug, Deserialize)]
struct GeneratePart {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UsageMetadata {
    #[serde(default, rename = "promptTokenCount")]
    prompt_token_count: u64,
    #[serde(default, rename = "candidatesTokenCount")]
    candidates_token_count: u64,
    #[serde(default, rename = "totalTokenCount")]
    total_token_count: u64,
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embedding: EmbeddingValues,
}

#[derive(Debug, Deserialize)]
struct EmbeddingValues {
    values: Vec<f32>,
}
