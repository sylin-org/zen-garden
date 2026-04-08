//! Google Gemini provider — capability-event driven (ORCH-0030 R2 M3).
//!
//! Google is the only cloud adapter that ships in M1. Unlike
//! instance-pool adapters (Ollama), it has no discovery loop: there
//! is exactly one fixed cloud endpoint
//! (`https://generativelanguage.googleapis.com` by default), one
//! API key, and a **static** list of supported models that the
//! adapter resolves `selectors.model` against via
//! [`crate::providers::cloud_common::resolve_cloud_model`].
//!
//! # Wire API
//!
//! - `POST /v1beta/models/{model}:generateContent?key=<key>` — chat +
//!   vision (unified `contents` array of `{role, parts}` objects).
//! - `POST /v1beta/models/{model}:embedContent?key=<key>` —
//!   embeddings (one request per document).
//!
//! Vision uses Base64 delivery inline — the `inlineData` part of the
//! user message carries `{mimeType, data}`.
//!
//! # Scope note
//!
//! Google also exposes Cloud TTS, Cloud Speech, and Imagen for
//! audio/image generation, but those are distinct APIs with separate
//! auth (service account) and are deliberately not declared here. A
//! caller targeting `audio.generate`, `audio.transcribe`, or
//! `image.generate` on Google would need a dedicated provider;
//! Ollama/ComfyUI/WhisperCpp/Kokoro already cover those primitives.
//!
//! # Capability publication
//!
//! Because cloud adapters have no discovery subscriber to trigger
//! the first publish, [`GoogleProvider::new`] spawns a one-shot task
//! that calls [`GoogleProvider::publish_capabilities`] immediately so
//! the [`crate::services::directory_subscriber::CapabilityDirectory`]
//! sees Google as available right away. `enabled: true` is constant
//! — runtime failures (expired keys, quota exhaustion, network
//! blips) surface inside `onboard` as `ProviderError` variants and
//! never flip the announcement.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::domain::capability_announcement::{
    Capability as AnnCapability, CapabilityAnnouncement, CapabilityMediaInput,
};
use crate::domain::events::EventBus;
use crate::domain::ids::ProviderName;
use crate::domain::keys;
use crate::domain::output::Output;
use crate::domain::primitive::Primitive;
use crate::domain::provider::{Provider, ProviderError, ProviderOutcome};
use crate::domain::request::OrchestratorRequest;
use crate::providers::cloud_common::resolve_cloud_model;
use crate::services::directory_subscriber::publish_capability_announcement;

use super::common::{build_http_client, check_status, map_reqwest_error};

// ── Config ───────────────────────────────────────────────────

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

// ── Static supported model list ──────────────────────────────
//
// Cloud adapters resolve `selectors.model` against this fixed list
// via `cloud_common::resolve_cloud_model`. Any `recommended:*`
// moniker collapses to `DEFAULT_MODEL`; an unknown concrete name
// returns `ProviderError::PinNotServable`.

const SUPPORTED_MODELS: &[&str] = &[
    "gemini-2.0-flash",
    "gemini-2.0-flash-exp",
    "gemini-2.0-pro",
    "gemini-2.0-pro-exp",
    "gemini-1.5-pro",
    "gemini-1.5-flash",
];

const DEFAULT_MODEL: &str = "gemini-2.0-flash";

// Accepted MIME types for `image.source` on `image.analyze`. Matches
// Gemini's documented vision input set.
const ACCEPTED_IMAGE_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/heic",
    "image/heif",
];

// ── Provider struct ──────────────────────────────────────────

pub struct GoogleProvider {
    name: ProviderName,
    base_url: String,
    api_key: String,
    http: Client,
    events: Arc<EventBus>,
}

impl GoogleProvider {
    /// Construct a Google/Gemini adapter.
    ///
    /// Cloud adapters take no `discovery` and no `shutdown` — there
    /// is nothing to subscribe to or cancel. They do take an
    /// [`EventBus`] handle because they publish a static
    /// [`CapabilityAnnouncement`] once at startup so the
    /// [`crate::services::directory_subscriber::CapabilityDirectory`]
    /// sees the adapter immediately.
    pub fn new(config: GoogleConfig, events: Arc<EventBus>) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::GOOGLE);
        let provider = Arc::new(Self {
            name,
            base_url: config.base_url,
            api_key: config.api_key,
            http: build_http_client(),
            events,
        });

        // Publish the static capability list immediately. `new` is
        // sync, so we spawn a one-shot task rather than blocking on
        // an async call. Idempotent re-publishing is allowed by the
        // M3 contract, so there's no harm if the directory
        // subscriber isn't running yet — the event stays on the bus
        // for late subscribers.
        let publisher = provider.clone();
        tokio::spawn(async move {
            publisher.publish_capabilities().await;
        });

        provider
    }

    /// Build and publish the static capability announcement.
    ///
    /// Google/Gemini's primitive surface is fixed at compile time:
    ///
    /// - `text.chat` — no media inputs
    /// - `text.embed` — no media inputs
    /// - `image.analyze` — one `image.source` input, Base64 delivery
    ///
    /// `enabled: true` is constant for cloud adapters; runtime
    /// failures surface inside `onboard` as `ProviderError` variants.
    async fn publish_capabilities(&self) {
        let announcement = CapabilityAnnouncement {
            provider: self.name.clone(),
            enabled: true,
            capabilities: vec![
                AnnCapability {
                    primitive: Primitive::TextChat,
                    media_inputs: Vec::new(),
                },
                AnnCapability {
                    primitive: Primitive::TextEmbed,
                    media_inputs: Vec::new(),
                },
                AnnCapability {
                    primitive: Primitive::ImageAnalyze,
                    media_inputs: vec![CapabilityMediaInput::base64(
                        keys::image::SOURCE.as_str().to_string(),
                        ACCEPTED_IMAGE_TYPES.iter().map(|s| s.to_string()).collect(),
                    )],
                },
            ],
            skills: Vec::new(),
        };
        publish_capability_announcement(&self.events, &announcement).await;
    }
}

// ── Provider trait impl ──────────────────────────────────────

#[async_trait]
impl Provider for GoogleProvider {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }

    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        if self.api_key.is_empty() {
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

// ── Wire-format dispatch ─────────────────────────────────────

impl GoogleProvider {
    async fn do_chat(
        &self,
        request: OrchestratorRequest,
        with_image: bool,
    ) -> Result<ProviderOutcome, ProviderError> {
        let model = resolve_cloud_model(
            request.selectors.model.as_deref(),
            DEFAULT_MODEL,
            SUPPORTED_MODELS,
        )?;

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
            self.base_url.trim_end_matches('/'),
            urlencode(&self.api_key),
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
        let model = resolve_cloud_model(
            request.selectors.model.as_deref(),
            DEFAULT_MODEL,
            SUPPORTED_MODELS,
        )?;
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
            self.base_url.trim_end_matches('/'),
            urlencode(&self.api_key),
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

// ── URL-encoding helper ──────────────────────────────────────

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

// ── Wire types ───────────────────────────────────────────────

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
