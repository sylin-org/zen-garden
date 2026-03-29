//! Anthropic cloud provider adapter.
//!
//! Implements the `Offering` trait for Claude models via the Anthropic
//! Messages API. Translates OpenAI-format requests (the orchestrator's
//! lingua franca) to Anthropic Messages API format, and translates
//! responses back.
//!
//! Key differences handled:
//! - System messages extracted from messages array → top-level `system` field
//! - `stop` → `stop_sequences`, `max_tokens` required (default 4096)
//! - Temperature clamped to 0-1.0 (Anthropic range)
//! - Tool definitions unwrapped from `{type: "function", function: {..}}`
//! - Strict user/assistant message alternation enforced
//! - Auth: `x-api-key` header + `anthropic-version` header

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use crate::catalog::{
    BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest, ProxyResponse,
    ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind, ServiceInstance};

use super::types::CloudProviderConfig;

/// Timeout for Anthropic API calls.
const CLOUD_TIMEOUT: Duration = Duration::from_secs(15);

/// Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Default max_tokens when the client omits it (Anthropic requires this field).
const DEFAULT_MAX_TOKENS: u64 = 4096;

/// Hardcoded model list — Anthropic does not expose a model listing endpoint.
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

/// Anthropic cloud provider adapter.
pub struct AnthropicProvider {
    config: CloudProviderConfig,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(config: CloudProviderConfig) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client build");
        Self { config, client }
    }

    fn base_url(&self) -> &str {
        &self.config.base_url
    }

    fn api_key(&self) -> &str {
        &self.config.api_key
    }
}

impl Offering for AnthropicProvider {
    fn offering_type(&self) -> OfferingKind {
        OfferingKind::Anthropic
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn capabilities(&self) -> &[Capability] {
        if self.config.capabilities.is_empty() {
            ANTHROPIC_CAPABILITIES
        } else {
            &self.config.capabilities
        }
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::Configured
    }

    fn probe(&self, _endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>> {
        let url = format!("{}/v1/messages", self.base_url());
        let api_key = self.api_key().to_string();

        Box::pin(async move {
            if api_key.is_empty() {
                anyhow::bail!("no API key configured for Anthropic provider");
            }

            // Minimal probe: send a tiny request to verify the key works.
            let probe_body = serde_json::json!({
                "model": "claude-sonnet-4-20250514",
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "hi"}]
            });

            let resp = self
                .client
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
                capabilities: self.capabilities().to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({
                    "provider": "anthropic",
                    "base_url": self.config.base_url,
                }),
            })
        })
    }

    fn enumerate(&self, _endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let model_filter = self.config.models.clone();

        Box::pin(async move {
            if self.api_key().is_empty() {
                return Ok(Vec::new());
            }

            // Anthropic has no /models endpoint — return hardcoded list.
            let models = ANTHROPIC_MODELS
                .iter()
                .filter(|m| {
                    model_filter.is_empty() || model_filter.iter().any(|f| m.contains(f.as_str()))
                })
                .map(|&name| ServiceModel {
                    name: name.to_string(),
                    capabilities: ANTHROPIC_CAPABILITIES.to_vec(),
                    vram_bytes: None,
                    metadata: serde_json::json!({
                        "cloud": true,
                        "provider": "anthropic",
                    }),
                })
                .collect();

            Ok(models)
        })
    }

    fn vram_estimate(&self, _model: &ServiceModel) -> Option<u64> {
        None // cloud — VRAM not applicable
    }

    fn proxy(
        &self,
        _endpoint: &str,
        _capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>> {
        let base_url = self.base_url().to_string();
        let api_key = self.api_key().to_string();

        Box::pin(async move {
            if api_key.is_empty() {
                anyhow::bail!("no API key configured for Anthropic provider");
            }

            let body_bytes = match request.body {
                ProxyBody::Complete(bytes) => bytes,
                ProxyBody::Stream(_) => {
                    anyhow::bail!("streaming request bodies not supported for cloud proxy");
                }
            };

            // Parse the incoming OpenAI-format body.
            let openai_body: Value = serde_json::from_slice(&body_bytes)
                .context("parse OpenAI-format request body")?;

            // Translate OpenAI format → Anthropic Messages API format.
            let anthropic_body = translate_request(openai_body);

            // Always POST to /v1/messages regardless of the incoming path.
            let url = format!("{base_url}/v1/messages");

            let resp = self
                .client
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&anthropic_body)
                .send()
                .await
                .context("proxy forward to Anthropic")?;

            let status = resp.status().as_u16();

            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| {
                    v.to_str()
                        .ok()
                        .map(|val| (k.as_str().to_string(), val.to_string()))
                })
                .collect();

            // Read the full response (non-streaming only).
            let resp_bytes = resp.bytes().await.context("read Anthropic response body")?;

            // If the upstream returned an error, pass it through without translation.
            if status >= 400 {
                return Ok(ProxyResponse {
                    status,
                    headers,
                    body: ProxyBody::Complete(resp_bytes.to_vec()),
                });
            }

            // Translate Anthropic response → OpenAI format.
            let anthropic_resp: Value = serde_json::from_slice(&resp_bytes)
                .unwrap_or(Value::Null);

            let openai_resp = translate_response(anthropic_resp);

            let openai_bytes = serde_json::to_vec(&openai_resp)
                .unwrap_or_else(|_| resp_bytes.to_vec());

            // Replace content-type in forwarded headers to ensure JSON.
            let translated_headers: Vec<(String, String)> = headers
                .into_iter()
                .map(|(k, v)| {
                    if k == "content-type" {
                        (k, "application/json".to_string())
                    } else {
                        (k, v)
                    }
                })
                .collect();

            Ok(ProxyResponse {
                status,
                headers: translated_headers,
                body: ProxyBody::Complete(openai_bytes),
            })
        })
    }

    fn sync_resource(
        &self,
        _resource: &str,
        _from: &ServiceInstance,
        _to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<crate::catalog::SyncProgress>> {
        Box::pin(async {
            Ok(crate::catalog::SyncProgress::Failed {
                reason: "cloud providers do not support resource sync".to_string(),
            })
        })
    }
}

// ── Request Translation (OpenAI → Anthropic) ──────────────────────

/// Translate an OpenAI-format request body into Anthropic Messages API format.
fn translate_request(mut body: Value) -> Value {
    let obj = match body.as_object_mut() {
        Some(o) => o,
        None => return body,
    };

    // 1. Strip `stream: true` — force non-streaming for now.
    obj.remove("stream");

    // 2. Extract system messages from `messages[]` → top-level `system`.
    //    Then enforce strict user/assistant alternation.
    if let Some(Value::Array(mut messages)) = obj.remove("messages") {
        let mut system_parts: Vec<String> = Vec::new();
        messages.retain(|msg| {
            if msg.get("role").and_then(|r| r.as_str()) == Some("system") {
                if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                    system_parts.push(content.to_string());
                }
                false // remove from messages array
            } else {
                true
            }
        });

        if !system_parts.is_empty() {
            obj.insert("system".to_string(), Value::String(system_parts.join("\n\n")));
        }

        // 6. Enforce strict user/assistant alternation — merge consecutive same-role.
        let merged = merge_consecutive_roles(&messages);
        obj.insert("messages".to_string(), Value::Array(merged));
    }

    // 3. `stop` → `stop_sequences`
    if let Some(stop) = obj.remove("stop") {
        let sequences = match stop {
            Value::String(s) => Value::Array(vec![Value::String(s)]),
            Value::Array(_) => stop,
            _ => Value::Array(vec![]),
        };
        obj.insert("stop_sequences".to_string(), sequences);
    }

    // 4. `max_tokens` — required by Anthropic, inject default if absent.
    if !obj.contains_key("max_tokens") {
        obj.insert("max_tokens".to_string(), Value::Number(DEFAULT_MAX_TOKENS.into()));
    }

    // 5. Clamp temperature to 0-1.0 (OpenAI allows 0-2.0, Anthropic 0-1.0).
    if let Some(temp) = obj.get("temperature").and_then(|v| v.as_f64()) {
        let clamped = temp.clamp(0.0, 1.0);
        obj.insert(
            "temperature".to_string(),
            serde_json::Number::from_f64(clamped)
                .map(Value::Number)
                .unwrap_or(Value::Number(0.into())),
        );
    }

    // 7. Tool definitions: unwrap OpenAI wrapper.
    //    OpenAI: `{type: "function", function: {name, description, parameters}}`
    //    Anthropic: `{name, description, input_schema}`
    if let Some(tools) = obj.get_mut("tools").and_then(|v| v.as_array_mut()) {
        let translated: Vec<Value> = tools
            .iter()
            .filter_map(|tool| translate_tool_definition(tool))
            .collect();
        *tools = translated;
    }

    // Remove OpenAI-specific fields that Anthropic doesn't understand.
    for field in &[
        "frequency_penalty",
        "presence_penalty",
        "logprobs",
        "top_logprobs",
        "logit_bias",
        "n",
        "seed",
        "user",
        "response_format",
        "service_tier",
    ] {
        obj.remove(*field);
    }

    body
}

/// Translate a single OpenAI tool definition to Anthropic format.
fn translate_tool_definition(tool: &Value) -> Option<Value> {
    // OpenAI format: {type: "function", function: {name, description, parameters}}
    // Anthropic format: {name, description, input_schema}
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

        let should_merge = merged.last().is_some_and(|prev| {
            prev.get("role").and_then(|r| r.as_str()) == Some(role)
        });

        if should_merge {
            // Append content to the previous message's content.
            let prev = merged.last_mut().expect("checked above");
            let prev_content = prev.get("content").cloned().unwrap_or(Value::String(String::new()));
            let this_content = msg.get("content").cloned().unwrap_or(Value::String(String::new()));

            let combined = match (prev_content, this_content) {
                (Value::String(a), Value::String(b)) => {
                    Value::String(format!("{a}\n\n{b}"))
                }
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
                (a, _b) => a, // fallback: keep previous
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

// ── Response Translation (Anthropic → OpenAI) ─────────────────────

/// Translate an Anthropic Messages API response into OpenAI chat completion format.
fn translate_response(resp: Value) -> Value {
    let obj = match resp.as_object() {
        Some(o) => o,
        None => return resp,
    };

    // Extract text content from content blocks.
    let content_blocks = obj.get("content").and_then(|c| c.as_array());

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
                    // Anthropic: {type: "tool_use", id, name, input}
                    // OpenAI: {id, type: "function", function: {name, arguments}}
                    let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
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

    // Map stop_reason → finish_reason.
    let stop_reason = obj.get("stop_reason").and_then(|s| s.as_str()).unwrap_or("end_turn");
    let finish_reason = match stop_reason {
        "end_turn" => "stop",
        "max_tokens" => "length",
        "tool_use" => "tool_calls",
        "stop_sequence" => "stop",
        other => other,
    };

    // Build the message object.
    let content = if text_parts.is_empty() {
        Value::Null
    } else {
        Value::String(text_parts.join(""))
    };

    let mut message = serde_json::json!({
        "role": "assistant",
        "content": content,
    });
    if !tool_calls.is_empty() {
        message
            .as_object_mut()
            .expect("just created")
            .insert("tool_calls".to_string(), Value::Array(tool_calls));
    }

    // Map usage: input_tokens → prompt_tokens, output_tokens → completion_tokens.
    let usage = obj.get("usage").and_then(|u| u.as_object());
    let prompt_tokens = usage
        .and_then(|u| u.get("input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion_tokens = usage
        .and_then(|u| u.get("output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let model = obj
        .get("model")
        .cloned()
        .unwrap_or(Value::String("unknown".to_string()));
    let id = obj
        .get("id")
        .cloned()
        .unwrap_or(Value::String("chatcmpl-anthropic".to_string()));

    serde_json::json!({
        "id": id,
        "object": "chat.completion",
        "model": model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason,
        }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": prompt_tokens + completion_tokens,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_request_extracts_system_messages() {
        let input = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {"role": "user", "content": "Hello"}
            ],
            "max_tokens": 100
        });
        let result = translate_request(input);
        assert_eq!(result["system"], "You are helpful.");
        let messages = result["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
    }

    #[test]
    fn translate_request_injects_default_max_tokens() {
        let input = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let result = translate_request(input);
        assert_eq!(result["max_tokens"], DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn translate_request_clamps_temperature() {
        let input = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [{"role": "user", "content": "hi"}],
            "temperature": 1.8,
            "max_tokens": 100
        });
        let result = translate_request(input);
        let temp = result["temperature"].as_f64().unwrap();
        assert!((temp - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn translate_request_converts_stop_to_stop_sequences() {
        let input = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [{"role": "user", "content": "hi"}],
            "stop": ["END", "STOP"],
            "max_tokens": 100
        });
        let result = translate_request(input);
        assert!(result.get("stop").is_none());
        let seqs = result["stop_sequences"].as_array().unwrap();
        assert_eq!(seqs.len(), 2);
    }

    #[test]
    fn translate_request_strips_stream_true() {
        let input = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [{"role": "user", "content": "hi"}],
            "stream": true,
            "max_tokens": 100
        });
        let result = translate_request(input);
        assert!(result.get("stream").is_none());
    }

    #[test]
    fn translate_request_converts_tool_definitions() {
        let input = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get weather",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "city": {"type": "string"}
                        }
                    }
                }
            }],
            "max_tokens": 100
        });
        let result = translate_request(input);
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools[0]["name"], "get_weather");
        assert!(tools[0].get("input_schema").is_some());
        assert!(tools[0].get("function").is_none());
    }

    #[test]
    fn translate_request_merges_consecutive_user_messages() {
        let input = serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {"role": "user", "content": "Hello"},
                {"role": "user", "content": "How are you?"},
                {"role": "assistant", "content": "Fine"},
                {"role": "user", "content": "Great"}
            ],
            "max_tokens": 100
        });
        let result = translate_request(input);
        let messages = result["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["content"], "Hello\n\nHow are you?");
    }

    #[test]
    fn translate_response_maps_text_content() {
        let input = serde_json::json!({
            "id": "msg_123",
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
        let result = translate_response(input);
        assert_eq!(result["choices"][0]["message"]["content"], "Hello world");
        assert_eq!(result["choices"][0]["finish_reason"], "stop");
        assert_eq!(result["usage"]["prompt_tokens"], 10);
        assert_eq!(result["usage"]["completion_tokens"], 5);
        assert_eq!(result["usage"]["total_tokens"], 15);
    }

    #[test]
    fn translate_response_maps_tool_use() {
        let input = serde_json::json!({
            "id": "msg_456",
            "model": "claude-sonnet-4-20250514",
            "content": [
                {"type": "tool_use", "id": "call_1", "name": "get_weather", "input": {"city": "Tokyo"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 20, "output_tokens": 10}
        });
        let result = translate_response(input);
        assert_eq!(result["choices"][0]["finish_reason"], "tool_calls");
        let tool_calls = result["choices"][0]["message"]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls[0]["function"]["name"], "get_weather");
        assert_eq!(tool_calls[0]["id"], "call_1");
    }

    #[test]
    fn translate_response_maps_max_tokens_reason() {
        let input = serde_json::json!({
            "id": "msg_789",
            "model": "claude-sonnet-4-20250514",
            "content": [{"type": "text", "text": "partial"}],
            "stop_reason": "max_tokens",
            "usage": {"input_tokens": 5, "output_tokens": 100}
        });
        let result = translate_response(input);
        assert_eq!(result["choices"][0]["finish_reason"], "length");
    }
}
