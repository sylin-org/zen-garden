//! Anthropic provider — unified lifecycle + inference for Claude models.
//!
//! Translates OpenAI-format requests (the orchestrator's lingua franca) to
//! Anthropic Messages API format, and translates responses back.
//!
//! Key differences handled:
//! - System messages extracted from messages array -> top-level `system` field
//! - `stop` -> `stop_sequences`, `max_tokens` required (default 4096)
//! - Temperature clamped to 0-1.0 (Anthropic range)
//! - Tool definitions unwrapped from `{type: "function", function: {..}}`
//! - Strict user/assistant message alternation enforced
//! - Auth: `x-api-key` header + `anthropic-version` header

use anyhow::{Context, Result};
use bytes::BytesMut;
use futures_util::stream::Stream;
use reqwest::Client;
use serde_json::Value;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::traits::{
    BoxFuture, DiscoveryConfig, ProbeResult, Provider, ProviderContext, ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind};

/// Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Default max_tokens when the client omits it (Anthropic requires this field).
const DEFAULT_MAX_TOKENS: u64 = 4096;

/// Timeout for cloud API probe/enumerate calls.
const CLOUD_TIMEOUT: Duration = Duration::from_secs(15);

/// Timeout for non-streaming inference calls.
const INFER_TIMEOUT: Duration = Duration::from_secs(300);

/// Hardcoded model list — Anthropic's /v1/models endpoint may not always be available.
const ANTHROPIC_MODELS: &[&str] = &[
    "claude-sonnet-4-20250514",
    "claude-haiku-4-20250514",
    "claude-opus-4-20250514",
    "claude-3-5-sonnet-20241022",
    "claude-3-5-haiku-20241022",
    "claude-3-opus-20240229",
];

const ANTHROPIC_CAPABILITIES: &[Capability] = &[
    Capability::Chat,
    Capability::Vision,
    Capability::Tools,
    Capability::Think,
];

/// Anthropic provider — stateless, receives all per-request state via `ProviderContext`.
pub struct AnthropicProvider {
    http: Client,
}

impl AnthropicProvider {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client build");
        Self { http }
    }
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Require an API key from the context, or bail.
fn require_api_key(ctx: &ProviderContext) -> Result<String> {
    ctx.api_key
        .as_ref()
        .filter(|k| !k.is_empty())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no API key configured for Anthropic provider"))
}

impl Provider for AnthropicProvider {
    fn kind(&self) -> OfferingKind {
        OfferingKind::Anthropic
    }

    fn capabilities(&self) -> &[Capability] {
        ANTHROPIC_CAPABILITIES
    }

    fn discovery(&self) -> DiscoveryConfig {
        DiscoveryConfig::Configured
    }

    // ── Lifecycle ───────────────────────────────────────────────

    fn probe(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<ProbeResult>> {
        let url = format!("{}/v1/messages", ctx.endpoint);
        let api_key_result = require_api_key(ctx);

        Box::pin(async move {
            let api_key = api_key_result?;

            // Minimal probe: send a tiny request to verify the key works.
            let probe_body = serde_json::json!({
                "model": "claude-sonnet-4-20250514",
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "hi"}]
            });

            let resp = self
                .http
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&probe_body)
                .timeout(CLOUD_TIMEOUT)
                .send()
                .await
                .context("probe Anthropic /v1/messages")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let summary = if body.len() > 256 {
                    format!("{}...", &body[..256])
                } else {
                    body
                };
                anyhow::bail!("Anthropic probe failed: HTTP {status}: {summary}");
            }

            Ok(ProbeResult {
                version: None,
                capabilities: ANTHROPIC_CAPABILITIES.to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({
                    "provider": "anthropic",
                }),
            })
        })
    }

    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let api_key = ctx.api_key.clone();
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let api_key = match api_key.as_ref().filter(|k| !k.is_empty()) {
                Some(k) => k,
                None => return Ok(Vec::new()),
            };

            // Try the /v1/models endpoint first, fall back to hardcoded list.
            let models = match self.try_enumerate_from_api(&endpoint, api_key).await {
                Ok(m) if !m.is_empty() => m,
                _ => ANTHROPIC_MODELS
                    .iter()
                    .map(|&name| ServiceModel {
                        name: name.to_string(),
                        capabilities: ANTHROPIC_CAPABILITIES.to_vec(),
                        specializations: vec![],
                        vram_bytes: None,
                        metadata: serde_json::json!({
                            "cloud": true,
                            "provider": "anthropic",
                        }),
                    })
                    .collect(),
            };

            Ok(models)
        })
    }

    // ── Inference ───────────────────────────────────────────────

    fn infer(
        &self,
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<InferenceResponse>> {
        let url = format!("{}/v1/messages", ctx.endpoint);
        let model = ctx
            .model
            .as_deref()
            .unwrap_or(&req.model);
        let model = model.to_string();
        let api_key = ctx.api_key.clone().unwrap_or_default();
        let body = build_anthropic_request(&model, &req, false);

        Box::pin(async move {
            let resp = self
                .http
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&body)
                .timeout(INFER_TIMEOUT)
                .send()
                .await
                .context("POST /v1/messages")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Anthropic /v1/messages HTTP {status}: {text}");
            }

            let anthropic: Value = resp.json().await.context("parse Anthropic response")?;
            Ok(anthropic_response_to_canonical(&model, &anthropic))
        })
    }

    fn infer_stream(
        &self,
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<BoxStream<'static, Result<InferenceChunk>>>> {
        let url = format!("{}/v1/messages", ctx.endpoint);
        let model = ctx
            .model
            .as_deref()
            .unwrap_or(&req.model);
        let model = model.to_string();
        let api_key = ctx.api_key.clone().unwrap_or_default();
        let body = build_anthropic_request(&model, &req, true);
        let http = self.http.clone();

        Box::pin(async move {
            let resp = http
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .context("POST /v1/messages stream")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Anthropic /v1/messages stream HTTP {status}: {text}");
            }

            let stream = resp.bytes_stream();
            Ok(
                Box::pin(AnthropicSseStream::new(stream, model))
                    as BoxStream<'static, Result<InferenceChunk>>,
            )
        })
    }
}

impl AnthropicProvider {
    /// Try to enumerate models from the Anthropic /v1/models endpoint.
    async fn try_enumerate_from_api(
        &self,
        endpoint: &str,
        api_key: &str,
    ) -> Result<Vec<ServiceModel>> {
        let url = format!("{endpoint}/v1/models");

        let resp = self
            .http
            .get(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .timeout(CLOUD_TIMEOUT)
            .send()
            .await
            .context("GET /v1/models")?;

        if !resp.status().is_success() {
            anyhow::bail!("enumerate failed: HTTP {}", resp.status());
        }

        let body: Value = resp.json().await.context("parse /v1/models")?;
        let models = body
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let id = m.get("id").and_then(|v| v.as_str())?;
                        Some(ServiceModel {
                            name: id.to_string(),
                            capabilities: ANTHROPIC_CAPABILITIES.to_vec(),
                            specializations: vec![],
                            vram_bytes: None,
                            metadata: serde_json::json!({
                                "cloud": true,
                                "provider": "anthropic",
                            }),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(models)
    }
}

// ── Request Translation (OpenAI -> Anthropic) ──────────────────

/// Build an Anthropic Messages API request body from canonical `InferenceRequest`.
fn build_anthropic_request(model: &str, req: &InferenceRequest, stream: bool) -> Value {
    let mut body = serde_json::json!({
        "model": model,
    });

    // 1. Extract system messages from the messages array -> top-level `system`.
    let mut system_parts: Vec<String> = Vec::new();
    let mut non_system: Vec<Value> = Vec::new();

    for msg in &req.messages {
        if msg.role == "system" {
            if let Some(ref content) = msg.content {
                if let Some(text) = content.as_str() {
                    system_parts.push(text.to_string());
                }
            }
        } else {
            let mut anthropic_msg = serde_json::json!({ "role": msg.role });
            if let Some(ref content) = msg.content {
                anthropic_msg["content"] = content.clone();
            }
            if let Some(ref tool_call_id) = msg.tool_call_id {
                anthropic_msg["tool_call_id"] = Value::String(tool_call_id.clone());
            }
            if let Some(ref tool_calls) = msg.tool_calls {
                anthropic_msg["tool_calls"] = Value::Array(tool_calls.clone());
            }
            non_system.push(anthropic_msg);
        }
    }

    if !system_parts.is_empty() {
        body["system"] = Value::String(system_parts.join("\n\n"));
    }

    // 2. Enforce strict user/assistant alternation — merge consecutive same-role.
    let merged = merge_consecutive_roles(&non_system);
    body["messages"] = Value::Array(merged);

    // 3. max_tokens — required by Anthropic, inject default if absent.
    body["max_tokens"] = Value::Number(
        req.max_tokens
            .unwrap_or(DEFAULT_MAX_TOKENS)
            .into(),
    );

    // 4. Clamp temperature to 0-1.0 (OpenAI allows 0-2.0, Anthropic 0-1.0).
    if let Some(temp) = req.temperature {
        let clamped = temp.clamp(0.0, 1.0);
        if let Some(n) = serde_json::Number::from_f64(clamped) {
            body["temperature"] = Value::Number(n);
        }
    }

    // 5. top_p pass-through
    if let Some(top_p) = req.top_p {
        if let Some(n) = serde_json::Number::from_f64(top_p) {
            body["top_p"] = Value::Number(n);
        }
    }

    // 6. stop -> stop_sequences
    if let Some(ref stop) = req.stop {
        let sequences = match stop {
            Value::String(s) => Value::Array(vec![Value::String(s.clone())]),
            Value::Array(_) => stop.clone(),
            _ => Value::Array(vec![]),
        };
        body["stop_sequences"] = sequences;
    }

    // 7. Tool definitions: unwrap OpenAI wrapper -> Anthropic format.
    if let Some(ref tools) = req.tools {
        let translated: Vec<Value> = tools
            .iter()
            .filter_map(translate_tool_definition)
            .collect();
        if !translated.is_empty() {
            body["tools"] = Value::Array(translated);
        }
    }

    // 8. Stream flag
    if stream {
        body["stream"] = Value::Bool(true);
    }

    body
}

/// Translate a single OpenAI tool definition to Anthropic format.
///
/// OpenAI: `{type: "function", function: {name, description, parameters}}`
/// Anthropic: `{name, description, input_schema}`
fn translate_tool_definition(tool: &Value) -> Option<Value> {
    let func = tool.get("function")?;
    let name = func.get("name")?.clone();
    let description = func.get("description").cloned().unwrap_or(Value::Null);
    let input_schema = func
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));

    Some(serde_json::json!({
        "name": name,
        "description": description,
        "input_schema": input_schema,
    }))
}

/// Merge consecutive same-role messages (Anthropic requires strict alternation).
fn merge_consecutive_roles(messages: &[Value]) -> Vec<Value> {
    let mut merged: Vec<Value> = Vec::with_capacity(messages.len());

    for msg in messages {
        let role = match msg.get("role").and_then(|r| r.as_str()) {
            Some(r) => r,
            None => {
                merged.push(msg.clone());
                continue;
            }
        };

        let should_merge = merged
            .last()
            .is_some_and(|prev| prev.get("role").and_then(|r| r.as_str()) == Some(role));

        if should_merge {
            let prev = merged.last_mut().expect("checked above");
            let prev_content = prev
                .get("content")
                .cloned()
                .unwrap_or(Value::String(String::new()));
            let this_content = msg
                .get("content")
                .cloned()
                .unwrap_or(Value::String(String::new()));

            let combined = match (prev_content, this_content) {
                (Value::String(a), Value::String(b)) => Value::String(format!("{a}\n\n{b}")),
                (Value::Array(mut a), Value::Array(b)) => {
                    a.extend(b);
                    Value::Array(a)
                }
                (Value::String(a), Value::Array(mut b)) => {
                    b.insert(0, serde_json::json!({"type": "text", "text": a}));
                    Value::Array(b)
                }
                (Value::Array(mut a), Value::String(b)) => {
                    a.push(serde_json::json!({"type": "text", "text": b}));
                    Value::Array(a)
                }
                (a, _) => a,
            };

            if let Some(obj) = prev.as_object_mut() {
                obj.insert("content".to_string(), combined);
            }
        } else {
            merged.push(msg.clone());
        }
    }

    merged
}

// ── Response Translation (Anthropic -> Canonical) ──────────────

/// Convert a non-streaming Anthropic response to canonical `InferenceResponse`.
fn anthropic_response_to_canonical(model: &str, resp: &Value) -> InferenceResponse {
    let obj = resp.as_object();

    // Extract text content and tool_use blocks.
    let content_blocks = resp.get("content").and_then(|c| c.as_array());

    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<Value> = Vec::new();

    if let Some(blocks) = content_blocks {
        for (idx, block) in blocks.iter().enumerate() {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match block_type {
                "text" => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        text_parts.push(text.to_string());
                    }
                }
                "tool_use" => {
                    let id = block
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let arguments = block
                        .get("input")
                        .map(|i| serde_json::to_string(i).unwrap_or_default())
                        .unwrap_or_default();

                    tool_calls.push(serde_json::json!({
                        "id": id,
                        "index": idx,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": arguments,
                        }
                    }));
                }
                _ => {} // skip thinking, etc.
            }
        }
    }

    // Map stop_reason -> finish_reason.
    let stop_reason = resp
        .get("stop_reason")
        .and_then(|s| s.as_str())
        .unwrap_or("end_turn");
    let finish_reason = match stop_reason {
        "end_turn" => "stop",
        "max_tokens" => "length",
        "tool_use" => "tool_calls",
        "stop_sequence" => "stop",
        other => other,
    };

    let content = if text_parts.is_empty() {
        None
    } else {
        Some(Value::String(text_parts.join("")))
    };

    let tool_calls_opt = if tool_calls.is_empty() {
        None
    } else {
        Some(tool_calls)
    };

    // Map usage: input_tokens -> prompt_tokens, output_tokens -> completion_tokens.
    let usage_obj = obj.and_then(|o| o.get("usage")).and_then(|u| u.as_object());
    let prompt_tokens = usage_obj
        .and_then(|u| u.get("input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion_tokens = usage_obj
        .and_then(|u| u.get("output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let id = resp
        .get("id")
        .and_then(|i| i.as_str())
        .unwrap_or("chatcmpl-anthropic")
        .to_string();

    InferenceResponse {
        id,
        object: "chat.completion".to_string(),
        model: model.to_string(),
        choices: vec![InferenceChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content,
                tool_calls: tool_calls_opt,
                tool_call_id: None,
                extra: serde_json::Map::new(),
            },
            finish_reason: Some(finish_reason.to_string()),
        }],
        usage: Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
    }
}

// ── SSE Stream Adapter ──────────────────────────────────────────

/// Adapter that converts an Anthropic SSE byte stream into `InferenceChunk`s.
///
/// Anthropic uses named events (not just `data:` lines):
/// ```text
/// event: content_block_delta
/// data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}
/// ```
struct AnthropicSseStream<S> {
    inner: S,
    buffer: BytesMut,
    model: String,
    chunk_id: String,
    /// Accumulated tool arguments (partial JSON) per content block index.
    tool_args: Vec<String>,
    /// Tool name per content block index.
    tool_names: Vec<String>,
    /// Tool ID per content block index.
    tool_ids: Vec<String>,
    done: bool,
}

impl<S> AnthropicSseStream<S> {
    fn new(inner: S, model: String) -> Self {
        let chunk_id = format!("chatcmpl-anthropic-{}", chrono::Utc::now().timestamp_millis());
        Self {
            inner,
            buffer: BytesMut::with_capacity(4096),
            model,
            chunk_id,
            tool_args: Vec::new(),
            tool_names: Vec::new(),
            tool_ids: Vec::new(),
            done: false,
        }
    }
}

/// Parse an SSE segment into (event_type, data_json).
fn parse_sse_segment(segment: &str) -> Option<(String, Value)> {
    let mut event_type = String::new();
    let mut data_line = String::new();

    for line in segment.lines() {
        if let Some(rest) = line.strip_prefix("event:") {
            event_type = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_line = rest.trim().to_string();
        }
    }

    if data_line.is_empty() {
        return None;
    }

    let data: Value = serde_json::from_str(&data_line).ok()?;
    Some((event_type, data))
}

impl<S> Stream for AnthropicSseStream<S>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<InferenceChunk>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if this.done {
            return Poll::Ready(None);
        }

        loop {
            // Check buffer for a complete SSE segment (delimited by \n\n).
            if let Some(pos) = find_double_newline(&this.buffer) {
                let segment_bytes = this.buffer.split_to(pos + 2);
                let segment = String::from_utf8_lossy(&segment_bytes);
                let segment = segment.trim();

                if segment.is_empty() {
                    continue;
                }

                let Some((event_type, data)) = parse_sse_segment(segment) else {
                    continue;
                };

                match event_type.as_str() {
                    "message_start" => {
                        let usage = data.get("message").and_then(|m| m.get("usage"));
                        let prompt_tokens = usage
                            .and_then(|u| u.get("input_tokens"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);

                        let chunk = InferenceChunk {
                            id: this.chunk_id.clone(),
                            object: "chat.completion.chunk".to_string(),
                            model: this.model.clone(),
                            choices: vec![ChunkChoice {
                                index: 0,
                                delta: ChatMessage {
                                    role: "assistant".to_string(),
                                    content: None,
                                    tool_calls: None,
                                    tool_call_id: None,
                                    extra: serde_json::Map::new(),
                                },
                                finish_reason: None,
                            }],
                            usage: if prompt_tokens > 0 {
                                Some(Usage {
                                    prompt_tokens,
                                    completion_tokens: 0,
                                    total_tokens: prompt_tokens,
                                })
                            } else {
                                None
                            },
                        };
                        return Poll::Ready(Some(Ok(chunk)));
                    }

                    "content_block_start" => {
                        let block = data.get("content_block");
                        let block_type = block
                            .and_then(|b| b.get("type"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("");

                        let idx = data
                            .get("index")
                            .and_then(|i| i.as_u64())
                            .unwrap_or(0) as usize;

                        while this.tool_args.len() <= idx {
                            this.tool_args.push(String::new());
                            this.tool_names.push(String::new());
                            this.tool_ids.push(String::new());
                        }

                        if block_type == "tool_use" {
                            this.tool_names[idx] = block
                                .and_then(|b| b.get("name"))
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .to_string();
                            this.tool_ids[idx] = block
                                .and_then(|b| b.get("id"))
                                .and_then(|i| i.as_str())
                                .unwrap_or("")
                                .to_string();
                        }

                        continue;
                    }

                    "content_block_delta" => {
                        let delta = data.get("delta");
                        let delta_type = delta
                            .and_then(|d| d.get("type"))
                            .and_then(|t| t.as_str())
                            .unwrap_or("");

                        match delta_type {
                            "text_delta" => {
                                let text = delta
                                    .and_then(|d| d.get("text"))
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("");

                                let chunk = InferenceChunk {
                                    id: this.chunk_id.clone(),
                                    object: "chat.completion.chunk".to_string(),
                                    model: this.model.clone(),
                                    choices: vec![ChunkChoice {
                                        index: 0,
                                        delta: ChatMessage {
                                            role: String::new(),
                                            content: Some(Value::String(text.to_string())),
                                            tool_calls: None,
                                            tool_call_id: None,
                                            extra: serde_json::Map::new(),
                                        },
                                        finish_reason: None,
                                    }],
                                    usage: None,
                                };
                                return Poll::Ready(Some(Ok(chunk)));
                            }
                            "input_json_delta" => {
                                let partial = delta
                                    .and_then(|d| d.get("partial_json"))
                                    .and_then(|p| p.as_str())
                                    .unwrap_or("");
                                let idx = data
                                    .get("index")
                                    .and_then(|i| i.as_u64())
                                    .unwrap_or(0) as usize;
                                if idx < this.tool_args.len() {
                                    this.tool_args[idx].push_str(partial);
                                }
                                continue;
                            }
                            _ => continue,
                        }
                    }

                    "content_block_stop" => {
                        let idx = data
                            .get("index")
                            .and_then(|i| i.as_u64())
                            .unwrap_or(0) as usize;

                        if idx < this.tool_names.len() && !this.tool_names[idx].is_empty() {
                            let tool_call = serde_json::json!({
                                "id": this.tool_ids[idx],
                                "index": idx,
                                "type": "function",
                                "function": {
                                    "name": this.tool_names[idx],
                                    "arguments": this.tool_args[idx],
                                }
                            });

                            let chunk = InferenceChunk {
                                id: this.chunk_id.clone(),
                                object: "chat.completion.chunk".to_string(),
                                model: this.model.clone(),
                                choices: vec![ChunkChoice {
                                    index: 0,
                                    delta: ChatMessage {
                                        role: String::new(),
                                        content: None,
                                        tool_calls: Some(vec![tool_call]),
                                        tool_call_id: None,
                                        extra: serde_json::Map::new(),
                                    },
                                    finish_reason: None,
                                }],
                                usage: None,
                            };
                            return Poll::Ready(Some(Ok(chunk)));
                        }
                        continue;
                    }

                    "message_delta" => {
                        let stop_reason = data
                            .get("delta")
                            .and_then(|d| d.get("stop_reason"))
                            .and_then(|s| s.as_str())
                            .unwrap_or("end_turn");

                        let finish_reason = match stop_reason {
                            "end_turn" => "stop",
                            "max_tokens" => "length",
                            "tool_use" => "tool_calls",
                            "stop_sequence" => "stop",
                            other => other,
                        };

                        let usage_obj = data.get("usage");
                        let output_tokens = usage_obj
                            .and_then(|u| u.get("output_tokens"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);

                        let chunk = InferenceChunk {
                            id: this.chunk_id.clone(),
                            object: "chat.completion.chunk".to_string(),
                            model: this.model.clone(),
                            choices: vec![ChunkChoice {
                                index: 0,
                                delta: ChatMessage {
                                    role: String::new(),
                                    content: None,
                                    tool_calls: None,
                                    tool_call_id: None,
                                    extra: serde_json::Map::new(),
                                },
                                finish_reason: Some(finish_reason.to_string()),
                            }],
                            usage: Some(Usage {
                                prompt_tokens: 0,
                                completion_tokens: output_tokens,
                                total_tokens: output_tokens,
                            }),
                        };
                        return Poll::Ready(Some(Ok(chunk)));
                    }

                    "message_stop" => {
                        this.done = true;
                        return Poll::Ready(None);
                    }

                    // ping, error, or unknown — skip.
                    _ => continue,
                }
            }

            // Need more data from the inner stream.
            match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    this.buffer.extend_from_slice(&bytes);
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(anyhow::anyhow!("stream error: {e}"))));
                }
                Poll::Ready(None) => {
                    this.done = true;
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Find the position of a double newline (`\n\n`) in the buffer.
fn find_double_newline(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\n\n")
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(messages: Vec<ChatMessage>) -> InferenceRequest {
        InferenceRequest {
            model: "claude-sonnet-4-20250514".into(),
            messages,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            tools: None,
            tool_choice: None,
            stream: false,
            extra: serde_json::Map::new(),
        }
    }

    fn system_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: "system".into(),
            content: Some(Value::String(text.into())),
            tool_calls: None,
            tool_call_id: None,
            extra: serde_json::Map::new(),
        }
    }

    fn user_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: "user".into(),
            content: Some(Value::String(text.into())),
            tool_calls: None,
            tool_call_id: None,
            extra: serde_json::Map::new(),
        }
    }

    #[test]
    fn request_extracts_system_and_sets_max_tokens() {
        let req = make_request(vec![system_msg("Be helpful."), user_msg("Hi")]);
        let body = build_anthropic_request("claude-sonnet-4-20250514", &req, false);

        assert_eq!(body["system"], "Be helpful.");
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert!(body.get("stream").is_none());
    }

    #[test]
    fn request_clamps_temperature_and_maps_stop() {
        let mut req = make_request(vec![user_msg("Hi")]);
        req.temperature = Some(1.8);
        req.stop = Some(Value::String("END".into()));
        req.max_tokens = Some(200);

        let body = build_anthropic_request("claude-sonnet-4-20250514", &req, false);

        let temp = body["temperature"].as_f64().unwrap();
        assert!(
            (temp - 1.0).abs() < f64::EPSILON,
            "temperature should be clamped to 1.0"
        );

        let seqs = body["stop_sequences"].as_array().unwrap();
        assert_eq!(seqs.len(), 1);
        assert_eq!(seqs[0], "END");
        assert_eq!(body["max_tokens"], 200);
    }

    #[test]
    fn request_translates_tool_definitions() {
        let mut req = make_request(vec![user_msg("Hi")]);
        req.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get weather info",
                "parameters": {
                    "type": "object",
                    "properties": { "city": {"type": "string"} }
                }
            }
        })]);

        let body = build_anthropic_request("claude-sonnet-4-20250514", &req, false);

        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "get_weather");
        assert_eq!(tools[0]["description"], "Get weather info");
        assert!(tools[0].get("input_schema").is_some());
        assert!(tools[0].get("function").is_none());
        assert!(tools[0].get("type").is_none());
    }

    #[test]
    fn request_merges_consecutive_user_messages() {
        let req = make_request(vec![
            user_msg("Hello"),
            user_msg("How are you?"),
        ]);
        let body = build_anthropic_request("claude-sonnet-4-20250514", &req, false);

        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["content"], "Hello\n\nHow are you?");
    }

    #[test]
    fn response_translates_text_and_usage() {
        let resp = serde_json::json!({
            "id": "msg_abc",
            "model": "claude-sonnet-4-20250514",
            "content": [
                {"type": "text", "text": "Hello "},
                {"type": "text", "text": "world"}
            ],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5
            }
        });

        let canonical = anthropic_response_to_canonical("claude-sonnet-4-20250514", &resp);

        assert_eq!(canonical.id, "msg_abc");
        assert_eq!(
            canonical.choices[0].message.content,
            Some(Value::String("Hello world".into()))
        );
        assert_eq!(
            canonical.choices[0].finish_reason.as_deref(),
            Some("stop")
        );
        assert_eq!(canonical.usage.prompt_tokens, 10);
        assert_eq!(canonical.usage.completion_tokens, 5);
        assert_eq!(canonical.usage.total_tokens, 15);
    }

    #[test]
    fn response_translates_tool_use_with_stringified_args() {
        let resp = serde_json::json!({
            "id": "msg_tools",
            "model": "claude-sonnet-4-20250514",
            "content": [{
                "type": "tool_use",
                "id": "call_1",
                "name": "get_weather",
                "input": {"city": "Tokyo"}
            }],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 20, "output_tokens": 10}
        });

        let canonical = anthropic_response_to_canonical("claude-sonnet-4-20250514", &resp);

        assert_eq!(
            canonical.choices[0].finish_reason.as_deref(),
            Some("tool_calls")
        );
        let tool_calls = canonical.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls[0]["function"]["name"], "get_weather");
        assert_eq!(tool_calls[0]["id"], "call_1");
        let args = tool_calls[0]["function"]["arguments"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(args).unwrap();
        assert_eq!(parsed["city"], "Tokyo");
    }

    #[test]
    fn response_maps_max_tokens_reason() {
        let resp = serde_json::json!({
            "id": "msg_789",
            "model": "claude-sonnet-4-20250514",
            "content": [{"type": "text", "text": "partial"}],
            "stop_reason": "max_tokens",
            "usage": {"input_tokens": 5, "output_tokens": 100}
        });

        let canonical = anthropic_response_to_canonical("claude-sonnet-4-20250514", &resp);
        assert_eq!(
            canonical.choices[0].finish_reason.as_deref(),
            Some("length")
        );
    }
}
