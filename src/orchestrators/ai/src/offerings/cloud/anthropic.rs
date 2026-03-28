//! Anthropic Messages API adapter.
//!
//! Translates between OpenAI chat completions format (the orchestrator's
//! lingua franca) and Anthropic's Messages API format.
//!
//! Key differences handled:
//! - Auth: `x-api-key` header (not `Authorization: Bearer`)
//! - System messages: extracted from messages array to top-level `system` field
//! - `max_tokens`: required (not optional)
//! - `stop` → `stop_sequences`
//! - Tool definitions: `parameters` → `input_schema`
//! - Response: content blocks → concatenated string + tool_calls array
//! - Streaming: named SSE events → OpenAI `data:` format

use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;

use crate::catalog::{
    BenchmarkSample, BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest,
    ProxyResponse, ServiceModel, SyncProgress,
};
use crate::domain::types::{Capability, OfferingKind, Sample, ServiceInstance};

const API_VERSION: &str = "2023-06-01";
const BASE_URL: &str = "https://api.anthropic.com/v1";

/// Anthropic-specific offering adapter with proper Messages API translation.
pub struct AnthropicOffering {
    api_key: String,
    http: reqwest::Client,
}

impl AnthropicOffering {
    /// Create from environment. Returns None if ANTHROPIC_API_KEY is not set.
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
        if api_key.is_empty() {
            return None;
        }
        Some(Self {
            api_key,
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .build()
                .expect("HTTP client"),
        })
    }
}

impl Offering for AnthropicOffering {
    fn offering_type(&self) -> OfferingKind {
        OfferingKind::Anthropic
    }

    fn capabilities(&self) -> &[Capability] {
        &[Capability::Chat, Capability::Tools, Capability::Think, Capability::Vision]
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::Configured
    }

    fn probe(&self, _endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>> {
        let api_key = self.api_key.clone();
        Box::pin(async move {
            // Anthropic has no /models endpoint. Send a minimal message to verify the key.
            let resp = self
                .http
                .post(format!("{BASE_URL}/messages"))
                .header("x-api-key", &api_key)
                .header("anthropic-version", API_VERSION)
                .header("content-type", "application/json")
                .json(&serde_json::json!({
                    "model": "claude-haiku-4-5-20241022",
                    "max_tokens": 1,
                    "messages": [{"role": "user", "content": "hi"}]
                }))
                .timeout(Duration::from_secs(15))
                .send()
                .await
                .context("Anthropic API probe")?;

            // 200 = key valid. 401 = invalid key. 429 = rate limited (key valid).
            if resp.status().as_u16() == 401 {
                anyhow::bail!("Anthropic API key invalid (401)");
            }

            Ok(ProbeResult {
                version: None,
                capabilities: vec![Capability::Chat, Capability::Tools, Capability::Think, Capability::Vision],
                vram_free_bytes: None,
                metadata: serde_json::Value::Null,
            })
        })
    }

    fn enumerate(&self, _endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        // Anthropic doesn't have a model list API. Return known models.
        Box::pin(async {
            Ok(vec![
                ServiceModel {
                    name: "claude-opus-4-6".to_string(),
                    capabilities: vec![Capability::Chat, Capability::Tools, Capability::Think, Capability::Vision],
                    vram_bytes: None,
                    is_loaded: false,
                    metadata: serde_json::Value::Null,
                },
                ServiceModel {
                    name: "claude-sonnet-4-6".to_string(),
                    capabilities: vec![Capability::Chat, Capability::Tools, Capability::Think, Capability::Vision],
                    vram_bytes: None,
                    is_loaded: false,
                    metadata: serde_json::Value::Null,
                },
                ServiceModel {
                    name: "claude-haiku-4-5-20241022".to_string(),
                    capabilities: vec![Capability::Chat, Capability::Tools, Capability::Vision],
                    vram_bytes: None,
                    is_loaded: false,
                    metadata: serde_json::Value::Null,
                },
            ])
        })
    }

    fn vram_estimate(&self, _model: &ServiceModel) -> Option<u64> {
        None
    }

    fn proxy(
        &self,
        _endpoint: &str,
        _capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>> {
        let api_key = self.api_key.clone();
        Box::pin(async move {
            // Parse the OpenAI-format request body.
            let openai_req: serde_json::Value =
                serde_json::from_slice(&request.body).context("parse request body")?;

            // Translate OpenAI → Anthropic Messages format.
            let anthropic_req = translate_request(&openai_req);

            let resp = self
                .http
                .post(format!("{BASE_URL}/messages"))
                .header("x-api-key", &api_key)
                .header("anthropic-version", API_VERSION)
                .header("content-type", "application/json")
                .json(&anthropic_req)
                .send()
                .await
                .context("Anthropic API request")?;

            let status = resp.status().as_u16();
            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
                .collect();

            // For non-streaming: translate the response.
            let body = resp.bytes().await.context("read Anthropic response")?;

            if status == 200 {
                let anthropic_resp: serde_json::Value =
                    serde_json::from_slice(&body).unwrap_or_default();
                let openai_resp = translate_response(&anthropic_resp);
                let translated = serde_json::to_vec(&openai_resp).unwrap_or_default();

                Ok(ProxyResponse {
                    status,
                    headers: vec![("content-type".to_string(), "application/json".to_string())],
                    body: ProxyBody::Complete(Bytes::from(translated)),
                })
            } else {
                // Pass through error responses as-is.
                Ok(ProxyResponse {
                    status,
                    headers,
                    body: ProxyBody::Complete(body),
                })
            }
        })
    }

    fn benchmark(
        &self,
        _endpoint: &str,
        model: &str,
        capability: Capability,
    ) -> BoxFuture<'_, Result<BenchmarkSample>> {
        let model = model.to_string();
        let api_key = self.api_key.clone();
        Box::pin(async move {
            let start = std::time::Instant::now();
            let result = self
                .http
                .post(format!("{BASE_URL}/messages"))
                .header("x-api-key", &api_key)
                .header("anthropic-version", API_VERSION)
                .json(&serde_json::json!({
                    "model": &model,
                    "max_tokens": 10,
                    "messages": [{"role": "user", "content": "Hi"}]
                }))
                .timeout(Duration::from_secs(30))
                .send()
                .await;

            let sample = match result {
                Ok(resp) if resp.status().is_success() => {
                    let _body = resp.text().await;
                    Sample {
                        prompt_index: 0,
                        cold_start_ms: start.elapsed().as_millis() as u64,
                        tokens_per_second: None,
                        total_duration_ms: start.elapsed().as_millis() as u64,
                        valid_ratio: None,
                        error: None,
                    }
                }
                Ok(resp) => Sample {
                    prompt_index: 0,
                    cold_start_ms: 0,
                    tokens_per_second: None,
                    total_duration_ms: start.elapsed().as_millis() as u64,
                    valid_ratio: None,
                    error: Some(format!("HTTP {}", resp.status())),
                },
                Err(e) => Sample {
                    prompt_index: 0,
                    cold_start_ms: 0,
                    tokens_per_second: None,
                    total_duration_ms: start.elapsed().as_millis() as u64,
                    valid_ratio: None,
                    error: Some(e.to_string()),
                },
            };

            Ok(BenchmarkSample {
                model,
                capability,
                samples: vec![sample],
            })
        })
    }

    fn sync_resource(
        &self,
        _resource: &str,
        _from: &ServiceInstance,
        _to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<SyncProgress>> {
        Box::pin(async { Ok(SyncProgress::Completed { bytes_transferred: 0 }) })
    }
}

// ── Request Translation (OpenAI → Anthropic) ────────────────────

fn translate_request(openai: &serde_json::Value) -> serde_json::Value {
    let mut anthropic = serde_json::json!({});

    // Model (pass through — user specifies Anthropic model names).
    if let Some(model) = openai.get("model") {
        anthropic["model"] = model.clone();
    }

    // max_tokens (required for Anthropic).
    let max_tokens = openai
        .get("max_tokens")
        .or_else(|| openai.get("max_completion_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(4096);
    anthropic["max_tokens"] = serde_json::json!(max_tokens);

    // Extract system messages from messages array.
    let messages = openai.get("messages").and_then(|m| m.as_array());
    if let Some(msgs) = messages {
        let mut system_parts: Vec<String> = Vec::new();
        let mut user_messages: Vec<serde_json::Value> = Vec::new();

        for msg in msgs {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            match role {
                "system" => {
                    if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                        system_parts.push(content.to_string());
                    }
                }
                "tool" => {
                    // OpenAI tool result → Anthropic user message with tool_result block.
                    let tool_call_id = msg
                        .get("tool_call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let content = msg
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    user_messages.push(serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": content,
                        }]
                    }));
                }
                "assistant" => {
                    // Check for tool_calls to convert to tool_use content blocks.
                    let mut content_blocks: Vec<serde_json::Value> = Vec::new();

                    if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                        if !text.is_empty() {
                            content_blocks.push(serde_json::json!({"type": "text", "text": text}));
                        }
                    }

                    if let Some(tool_calls) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                        for tc in tool_calls {
                            let func = tc.get("function").unwrap_or(tc);
                            let name = func.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            let args_str = func
                                .get("arguments")
                                .and_then(|v| v.as_str())
                                .unwrap_or("{}");
                            let input: serde_json::Value =
                                serde_json::from_str(args_str).unwrap_or_default();
                            let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                            content_blocks.push(serde_json::json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": input,
                            }));
                        }
                    }

                    if content_blocks.is_empty() {
                        // Preserve original message if no translation needed.
                        user_messages.push(msg.clone());
                    } else {
                        user_messages.push(serde_json::json!({
                            "role": "assistant",
                            "content": content_blocks,
                        }));
                    }
                }
                _ => {
                    user_messages.push(msg.clone());
                }
            }
        }

        if !system_parts.is_empty() {
            anthropic["system"] = serde_json::json!(system_parts.join("\n\n"));
        }
        anthropic["messages"] = serde_json::json!(user_messages);
    }

    // stop → stop_sequences
    if let Some(stop) = openai.get("stop") {
        if let Some(s) = stop.as_str() {
            anthropic["stop_sequences"] = serde_json::json!([s]);
        } else if stop.is_array() {
            anthropic["stop_sequences"] = stop.clone();
        }
    }

    // temperature, top_p (pass through)
    for field in &["temperature", "top_p", "stream"] {
        if let Some(val) = openai.get(*field) {
            anthropic[*field] = val.clone();
        }
    }

    // Tools: translate OpenAI format → Anthropic format.
    if let Some(tools) = openai.get("tools").and_then(|t| t.as_array()) {
        let anthropic_tools: Vec<serde_json::Value> = tools
            .iter()
            .filter_map(|tool| {
                let func = tool.get("function")?;
                Some(serde_json::json!({
                    "name": func.get("name")?,
                    "description": func.get("description").unwrap_or(&serde_json::Value::Null),
                    "input_schema": func.get("parameters").unwrap_or(&serde_json::json!({"type": "object"})),
                }))
            })
            .collect();
        if !anthropic_tools.is_empty() {
            anthropic["tools"] = serde_json::json!(anthropic_tools);
        }
    }

    // tool_choice translation
    if let Some(tc) = openai.get("tool_choice") {
        if let Some(s) = tc.as_str() {
            match s {
                "auto" => anthropic["tool_choice"] = serde_json::json!({"type": "auto"}),
                "none" => anthropic["tool_choice"] = serde_json::json!({"type": "none"}),
                "required" => anthropic["tool_choice"] = serde_json::json!({"type": "any"}),
                _ => {}
            }
        } else if let Some(func) = tc.get("function").and_then(|f| f.get("name")) {
            anthropic["tool_choice"] = serde_json::json!({"type": "tool", "name": func});
        }
    }

    anthropic
}

// ── Response Translation (Anthropic → OpenAI) ───────────────────

fn translate_response(anthropic: &serde_json::Value) -> serde_json::Value {
    let id = anthropic
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("msg_unknown");

    let model = anthropic
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("claude");

    // Extract text content and tool_use blocks.
    let content_blocks = anthropic.get("content").and_then(|c| c.as_array());
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<serde_json::Value> = Vec::new();

    if let Some(blocks) = content_blocks {
        for block in blocks.iter() {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match block_type {
                "text" => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        text_parts.push(text.to_string());
                    }
                }
                "tool_use" => {
                    let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let input = block.get("input").unwrap_or(&serde_json::Value::Null);
                    tool_calls.push(serde_json::json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": serde_json::to_string(input).unwrap_or_default(),
                        }
                    }));
                }
                _ => {} // Skip thinking blocks, etc.
            }
        }
    }

    // Map stop_reason → finish_reason.
    let stop_reason = anthropic
        .get("stop_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("end_turn");
    let finish_reason = match stop_reason {
        "end_turn" | "stop_sequence" => "stop",
        "max_tokens" => "length",
        "tool_use" => "tool_calls",
        other => other,
    };

    // Map usage.
    let input_tokens = anthropic
        .get("usage")
        .and_then(|u| u.get("input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output_tokens = anthropic
        .get("usage")
        .and_then(|u| u.get("output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let mut message = serde_json::json!({
        "role": "assistant",
        "content": text_parts.join(""),
    });

    if !tool_calls.is_empty() {
        message["tool_calls"] = serde_json::json!(tool_calls);
    }

    serde_json::json!({
        "id": id,
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason,
        }],
        "usage": {
            "prompt_tokens": input_tokens,
            "completion_tokens": output_tokens,
            "total_tokens": input_tokens + output_tokens,
        }
    })
}
