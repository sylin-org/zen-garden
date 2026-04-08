//! Anthropic Messages API provider — `text.chat` and `image.analyze`
//! via vision models.
//!
//! Wire API (`https://api.anthropic.com/v1/messages`):
//!
//! ```text
//! POST /v1/messages
//! headers: x-api-key, anthropic-version, content-type: application/json
//! body: {
//!   "model": "...",
//!   "system": "...",                // extracted from text.prompt.system
//!   "max_tokens": N,                // REQUIRED
//!   "messages": [{"role": "user"|"assistant", "content": ...}],
//!   "temperature": 0..=1,           // clamped
//!   "stop_sequences": [...],
//!   "tools": [...]
//! }
//! ```
//!
//! Narrowing (§ADR Provider inventory / narrowings):
//! - `text.tokens.max` is required (default 4096 if caller omits it).
//! - `text.sampling.temperature` is clamped to `[0.0, 1.0]`.
//! - Vision uses Base64 delivery — the media resolver inlines bytes
//!   before dispatch, and Anthropic's `image` content-part shape is
//!   constructed from the inlined base64.

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
    FieldRange, HonoredField, MediaInputSpec, Model, ModelDescriptor, Provider,
    ProviderError, ProviderHealth, ProviderOutcome, ProviderState, ProviderStatePublisher,
    Registration, RegistrationStrategy,
};
use crate::domain::request::OrchestratorRequest;

use super::common::{build_http_client, check_status, map_reqwest_error};

const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u64 = 4096;

#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub base_url: String,
    pub api_key: String,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.anthropic.com".to_string(),
            api_key: String::new(),
        }
    }
}

/// Hard-coded model list — Anthropic has no reliable public
/// `/v1/models` endpoint for all key types. Updated via code review
/// when Anthropic publishes new models.
const ANTHROPIC_MODELS: &[(&str, &[&str])] = &[
    ("claude-opus-4-20250514", &["chat", "tools", "vision"]),
    ("claude-sonnet-4-20250514", &["chat", "tools", "vision"]),
    ("claude-haiku-4-20250514", &["chat", "tools", "vision"]),
    ("claude-3-5-sonnet-20241022", &["chat", "tools", "vision"]),
    ("claude-3-5-haiku-20241022", &["chat", "tools"]),
    ("claude-3-opus-20240229", &["chat", "tools", "vision"]),
];

pub struct AnthropicProvider {
    name: ProviderName,
    config: AnthropicConfig,
    http: Client,
    publisher: ProviderStatePublisher,
}

impl AnthropicProvider {
    pub fn new(config: AnthropicConfig) -> Arc<Self> {
        let name = ProviderName::new(keys::providers::ANTHROPIC);

        let chat_reg = build_chat_registration(&name);
        let analyze_reg = build_analyze_registration(&name);

        // Build the model catalog once at construction.
        let catalog: Vec<ModelDescriptor> = ANTHROPIC_MODELS
            .iter()
            .map(|(id, tags)| ModelDescriptor {
                short_name: id.to_string(),
                capability_tags: tags.iter().map(|s| s.to_string()).collect(),
                size_bytes: None,
                context_length: Some(200_000),
                parameter_count: None,
            })
            .collect();
        let models: Vec<Model> = catalog
            .iter()
            .map(|d| Model {
                fqn: ModelFqn::new(&name, &d.short_name),
                short_name: d.short_name.clone(),
                primitives: vec![Primitive::TextChat, Primitive::ImageAnalyze],
                capability_tags: d.capability_tags.clone(),
                size_bytes: None,
                context_length: Some(200_000),
                parameter_count: None,
            })
            .collect();

        let initial = ProviderState {
            health: if config.api_key.is_empty() {
                ProviderHealth::Degraded {
                    reason: "missing ANTHROPIC_API_KEY".to_string(),
                }
            } else {
                ProviderHealth::Healthy
            },
            registrations: vec![chat_reg, analyze_reg],
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
        rb.header("x-api-key", &self.config.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
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
                "anthropic api key is not configured".to_string(),
            ));
        }
        match request.action.primitive {
            Primitive::TextChat => self.do_chat(request, false).await,
            Primitive::ImageAnalyze => self.do_chat(request, true).await,
            other => Err(ProviderError::Unsupported(format!(
                "anthropic does not serve {}",
                other.dotted()
            ))),
        }
    }
}

impl AnthropicProvider {
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

        // Extract system prompt (lives top-level in Anthropic's shape).
        let system = request
            .payload
            .pointer("/text/prompt/system")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Build messages array — prior turns + current user turn.
        let mut messages: Vec<Value> = Vec::new();
        if let Some(previous) = request
            .payload
            .pointer("/text/prompt/previous")
            .and_then(|v| v.as_array())
        {
            for turn in previous {
                if let (Some(user), Some(assistant)) = (
                    turn.get("user").and_then(|v| v.as_str()),
                    turn.get("assistant").and_then(|v| v.as_str()),
                ) {
                    messages.push(json!({"role": "user", "content": user}));
                    messages.push(json!({"role": "assistant", "content": assistant}));
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
            // image.analyze: construct a content array with an image
            // block followed by the user text. The media resolver has
            // already inlined `image.source` as `{base64, content_type, size_bytes}`.
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
            let media_type = request
                .payload
                .pointer("/image/source/content_type")
                .and_then(|v| v.as_str())
                .unwrap_or("image/png")
                .to_string();
            messages.push(json!({
                "role": "user",
                "content": [
                    {
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": b64,
                        }
                    },
                    {"type": "text", "text": if user_text.is_empty() {
                        "Describe this image.".to_string()
                    } else {
                        user_text
                    }}
                ]
            }));
        } else {
            messages.push(json!({"role": "user", "content": user_text}));
        }

        // max_tokens is required. Clamp temperature into [0,1] to
        // honor Anthropic's narrowing.
        let max_tokens = request
            .payload
            .pointer("/text/tokens/max")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_MAX_TOKENS);

        let mut body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": messages,
        });
        if let Some(sys) = system {
            body["system"] = Value::String(sys);
        }
        if let Some(temp) = request
            .payload
            .pointer("/text/sampling/temperature")
            .and_then(|v| v.as_f64())
        {
            body["temperature"] = json!(temp.clamp(0.0, 1.0));
        }
        if let Some(top_p) = request
            .payload
            .pointer("/text/sampling/top_p")
            .and_then(|v| v.as_f64())
        {
            body["top_p"] = json!(top_p);
        }
        if let Some(stops) = request
            .payload
            .pointer("/text/stop/sequences")
            .and_then(|v| v.as_array())
        {
            body["stop_sequences"] = Value::Array(stops.clone());
        }
        if let Some(tools) = request
            .payload
            .pointer("/text/tools/definitions")
            .and_then(|v| v.as_array())
        {
            // Anthropic expects `{name, description, input_schema}`.
            // Pass through as-is; callers are responsible for the
            // correct shape.
            body["tools"] = Value::Array(tools.clone());
        }

        let endpoint = format!(
            "{}/v1/messages",
            self.config.base_url.trim_end_matches('/')
        );
        let resp = self
            .auth(self.http.post(&endpoint).json(&body))
            .send()
            .await
            .map_err(map_reqwest_error)?;
        let resp = check_status(resp, "anthropic messages").await?;
        let wire: MessagesResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Upstream(e.to_string()))?;

        // Collate text parts from the response's content array.
        let mut text = String::new();
        let mut tool_calls: Vec<Value> = Vec::new();
        for block in wire.content {
            match block {
                ContentBlock::Text { text: t } => {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&t);
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(json!({
                        "id": id,
                        "name": name,
                        "arguments": input,
                    }));
                }
            }
        }

        let mut out = Output::new();
        out.set(&keys::text::RESPONSE, text);
        let finish = match wire.stop_reason.as_deref() {
            Some("end_turn") | Some("stop_sequence") => {
                keys::text::values::FINISH_REASON_STOP
            }
            Some("max_tokens") => keys::text::values::FINISH_REASON_LENGTH,
            Some("tool_use") => keys::text::values::FINISH_REASON_TOOL_CALLS,
            _ => keys::text::values::FINISH_REASON_STOP,
        };
        out.set(&keys::text::FINISH_REASON, finish);
        if !tool_calls.is_empty() {
            out.set(&keys::text::TOOL_CALLS, Value::Array(tool_calls));
        }
        if let Some(u) = wire.usage {
            out.set(&keys::usage::TOKENS_INPUT, u.input_tokens);
            out.set(&keys::usage::TOKENS_OUTPUT, u.output_tokens);
        }
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
            // Narrowings per ADR.
            HonoredField::new(keys::text::TOKENS_MAX).required().with_range(
                FieldRange::Integer {
                    min: Some(1),
                    max: Some(200_000),
                },
            ),
            HonoredField::new(keys::text::SAMPLING_TEMPERATURE).with_range(
                FieldRange::Number {
                    min: Some(0.0),
                    max: Some(1.0),
                },
            ),
            HonoredField::new(keys::text::SAMPLING_TOP_P),
            HonoredField::new(keys::text::STOP_SEQUENCES),
            HonoredField::new(keys::text::TOOLS_DEFINITIONS),
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
            HonoredField::new(keys::text::TOKENS_MAX).required().with_range(
                FieldRange::Integer {
                    min: Some(1),
                    max: Some(16_000),
                },
            ),
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

// ── Wire types ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    usage: Option<MessagesUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: Value },
}

#[derive(Debug, Deserialize)]
struct MessagesUsage {
    input_tokens: u64,
    output_tokens: u64,
}
