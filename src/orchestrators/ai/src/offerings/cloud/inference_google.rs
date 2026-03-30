//! Google Gemini inference adapter.
//!
//! Translates canonical OpenAI-shaped types to/from the Google Gemini API.
//!
//! Key translations (verified against Gemini API docs):
//! - Auth: `?key={api_key}` query parameter (not a header)
//! - Endpoint: `{base}/v1beta/models/{model}:generateContent` (non-streaming)
//!             `{base}/v1beta/models/{model}:streamGenerateContent?alt=sse` (streaming)
//! - Messages: `messages` → `contents`, role `"assistant"` → `"model"`
//! - System prompt: extracted → `systemInstruction: {parts: [{text: "..."}]}`
//! - Parameters: nested in `generationConfig` (`maxOutputTokens`, `temperature`, etc.)
//! - Vision: OpenAI `image_url` → Gemini `inline_data`
//! - Tools: OpenAI wrapper → Gemini `functionDeclarations`
//! - Tool args: Gemini `args` (object) → canonical `function.arguments` (JSON string)
//! - Finish: `STOP` → `stop`, `MAX_TOKENS` → `length`, `SAFETY` → `content_filter`
//! - Usage: `promptTokenCount` → `prompt_tokens`, `candidatesTokenCount` → `completion_tokens`
//! - Stream: Standard SSE `data:` lines (no named events); no `[DONE]` sentinel — EOF = done
//! - Embed: `embedContent` endpoint, sequential calls for batch

use anyhow::{Context, Result};
use bytes::BytesMut;
use futures_util::stream::Stream;
use reqwest::Client;
use serde_json::Value;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::BoxFuture;

/// Google Gemini inference adapter — stateless protocol translator.
pub struct GoogleInferenceAdapter {
    http: Client,
}

impl GoogleInferenceAdapter {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client");
        Self { http }
    }
}

impl InferenceAdapter for GoogleInferenceAdapter {
    fn infer(
        &self,
        ctx: &AdapterContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<InferenceResponse>> {
        let api_key = ctx.api_key.clone().unwrap_or_default();
        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            ctx.endpoint, ctx.model, api_key
        );
        let model = ctx.model.clone();
        let body = build_gemini_request(&req);

        Box::pin(async move {
            let resp = self
                .http
                .post(&url)
                .header("content-type", "application/json")
                .json(&body)
                .timeout(Duration::from_secs(300))
                .send()
                .await
                .context("POST generateContent")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Gemini generateContent HTTP {status}: {text}");
            }

            let gemini: Value = resp.json().await.context("parse Gemini response")?;
            Ok(gemini_response_to_canonical(&model, &gemini))
        })
    }

    fn infer_stream(
        &self,
        ctx: &AdapterContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<BoxStream<'static, Result<InferenceChunk>>>> {
        let api_key = ctx.api_key.clone().unwrap_or_default();
        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent?key={}&alt=sse",
            ctx.endpoint, ctx.model, api_key
        );
        let model = ctx.model.clone();
        let body = build_gemini_request(&req);
        let http = self.http.clone();

        Box::pin(async move {
            let resp = http
                .post(&url)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
                .context("POST streamGenerateContent")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Gemini streamGenerateContent HTTP {status}: {text}");
            }

            let stream = resp.bytes_stream();
            Ok(Box::pin(GeminiSseStream::new(stream, model))
                as BoxStream<'static, Result<InferenceChunk>>)
        })
    }

    fn embed(
        &self,
        ctx: &AdapterContext,
        req: EmbedRequest,
    ) -> BoxFuture<'_, Result<EmbedResponse>> {
        let api_key = ctx.api_key.clone().unwrap_or_default();
        let model = ctx.model.clone();
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            // Collect inputs — OpenAI input can be a string or array of strings.
            let inputs: Vec<String> = match &req.input {
                Value::String(s) => vec![s.clone()],
                Value::Array(arr) => arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect(),
                _ => vec![req.input.to_string()],
            };

            let mut data: Vec<EmbeddingData> = Vec::with_capacity(inputs.len());
            let mut total_prompt_tokens: u64 = 0;

            // Sequential calls — Gemini embedContent takes a single input.
            for (idx, text) in inputs.iter().enumerate() {
                let url = format!(
                    "{}/v1beta/models/{}:embedContent?key={}",
                    endpoint, model, api_key
                );

                let body = serde_json::json!({
                    "content": {
                        "parts": [{"text": text}]
                    }
                });

                let resp = self
                    .http
                    .post(&url)
                    .header("content-type", "application/json")
                    .json(&body)
                    .timeout(Duration::from_secs(60))
                    .send()
                    .await
                    .context("POST embedContent")?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let err_text = resp.text().await.unwrap_or_default();
                    anyhow::bail!("Gemini embedContent HTTP {status}: {err_text}");
                }

                let gemini: Value =
                    resp.json().await.context("parse embedContent response")?;

                // Gemini returns {embedding: {values: [f64...]}}
                let values = gemini
                    .get("embedding")
                    .and_then(|e| e.get("values"))
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect::<Vec<f64>>())
                    .unwrap_or_default();

                // Gemini may report token count in metadata
                let tokens = gemini
                    .get("metadata")
                    .and_then(|m| m.get("billableCharacterCount"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                total_prompt_tokens += tokens;

                data.push(EmbeddingData {
                    object: "embedding".to_string(),
                    index: idx as u32,
                    embedding: values,
                });
            }

            Ok(EmbedResponse {
                object: "list".to_string(),
                data,
                model,
                usage: Usage {
                    prompt_tokens: total_prompt_tokens,
                    completion_tokens: 0,
                    total_tokens: total_prompt_tokens,
                },
            })
        })
    }
}

// ── Request Translation (OpenAI → Gemini) ─────────────────────

/// Build a Gemini `generateContent` request body from canonical `InferenceRequest`.
fn build_gemini_request(req: &InferenceRequest) -> Value {
    let mut body = serde_json::json!({});

    // 1. Extract system messages → systemInstruction.
    let mut system_parts: Vec<String> = Vec::new();
    let mut contents: Vec<Value> = Vec::new();

    for msg in &req.messages {
        if msg.role == "system" {
            if let Some(ref content) = msg.content {
                if let Some(text) = content.as_str() {
                    system_parts.push(text.to_string());
                }
            }
            continue;
        }

        // Map role: "assistant" → "model", "user" stays.
        let role = match msg.role.as_str() {
            "assistant" => "model",
            other => other,
        };

        let parts = build_gemini_parts(msg);

        contents.push(serde_json::json!({
            "role": role,
            "parts": parts,
        }));
    }

    if !system_parts.is_empty() {
        body["systemInstruction"] = serde_json::json!({
            "parts": [{"text": system_parts.join("\n\n")}]
        });
    }

    body["contents"] = Value::Array(contents);

    // 2. Generation config.
    let mut gen_config = serde_json::Map::new();

    if let Some(temp) = req.temperature {
        if let Some(n) = serde_json::Number::from_f64(temp) {
            gen_config.insert("temperature".into(), Value::Number(n));
        }
    }
    if let Some(top_p) = req.top_p {
        if let Some(n) = serde_json::Number::from_f64(top_p) {
            gen_config.insert("topP".into(), Value::Number(n));
        }
    }
    if let Some(max_tokens) = req.max_tokens {
        gen_config.insert("maxOutputTokens".into(), Value::Number(max_tokens.into()));
    }
    if let Some(ref stop) = req.stop {
        let sequences = match stop {
            Value::String(s) => vec![Value::String(s.clone())],
            Value::Array(arr) => arr.clone(),
            _ => vec![],
        };
        if !sequences.is_empty() {
            gen_config.insert("stopSequences".into(), Value::Array(sequences));
        }
    }

    if !gen_config.is_empty() {
        body["generationConfig"] = Value::Object(gen_config);
    }

    // 3. Tool definitions.
    if let Some(ref tools) = req.tools {
        let declarations: Vec<Value> = tools
            .iter()
            .filter_map(|tool| translate_tool_to_gemini(tool))
            .collect();
        if !declarations.is_empty() {
            body["tools"] = serde_json::json!([{
                "functionDeclarations": declarations
            }]);
        }
    }

    body
}

/// Build Gemini `parts` array from a `ChatMessage`.
///
/// Handles text content, vision (image_url → inline_data), and tool results.
fn build_gemini_parts(msg: &ChatMessage) -> Vec<Value> {
    let mut parts = Vec::new();

    if let Some(ref content) = msg.content {
        match content {
            Value::String(text) => {
                parts.push(serde_json::json!({"text": text}));
            }
            Value::Array(arr) => {
                for part in arr {
                    let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match part_type {
                        "text" => {
                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                parts.push(serde_json::json!({"text": text}));
                            }
                        }
                        "image_url" => {
                            if let Some(url) = part
                                .get("image_url")
                                .and_then(|v| v.get("url"))
                                .and_then(|v| v.as_str())
                            {
                                // Parse data URI: "data:image/jpeg;base64,..."
                                if let Some((prefix, data)) = url.split_once(",") {
                                    let mime = prefix
                                        .strip_prefix("data:")
                                        .and_then(|s| s.split(';').next())
                                        .unwrap_or("image/jpeg");
                                    parts.push(serde_json::json!({
                                        "inline_data": {
                                            "mime_type": mime,
                                            "data": data,
                                        }
                                    }));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                parts.push(serde_json::json!({"text": content.to_string()}));
            }
        }
    }

    // Tool call results (function response).
    if let Some(ref tool_calls) = msg.tool_calls {
        for tc in tool_calls {
            if let Some(func) = tc.get("function") {
                let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = func.get("arguments");

                // Parse JSON string args back to object for Gemini.
                let args_obj = match args {
                    Some(Value::String(s)) => {
                        serde_json::from_str::<Value>(s).unwrap_or(Value::Object(serde_json::Map::new()))
                    }
                    Some(v) => v.clone(),
                    None => Value::Object(serde_json::Map::new()),
                };

                parts.push(serde_json::json!({
                    "functionCall": {
                        "name": name,
                        "args": args_obj,
                    }
                }));
            }
        }
    }

    if parts.is_empty() {
        parts.push(serde_json::json!({"text": ""}));
    }

    parts
}

/// Translate a single OpenAI tool definition to Gemini format.
///
/// OpenAI: `{type: "function", function: {name, description, parameters}}`
/// Gemini: `{name, description, parameters}`
fn translate_tool_to_gemini(tool: &Value) -> Option<Value> {
    let func = tool.get("function")?;
    let name = func.get("name")?.clone();
    let description = func.get("description").cloned().unwrap_or(Value::Null);
    let parameters = func
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));

    Some(serde_json::json!({
        "name": name,
        "description": description,
        "parameters": parameters,
    }))
}

// ── Response Translation (Gemini → Canonical) ─────────────────

/// Convert a non-streaming Gemini response to canonical `InferenceResponse`.
fn gemini_response_to_canonical(model: &str, resp: &Value) -> InferenceResponse {
    let candidate = resp
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first());

    let (content, tool_calls, finish_reason) = extract_candidate(candidate);

    // Usage metadata.
    let usage_meta = resp.get("usageMetadata");
    let prompt_tokens = usage_meta
        .and_then(|u| u.get("promptTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion_tokens = usage_meta
        .and_then(|u| u.get("candidatesTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    InferenceResponse {
        id: format!("gemini-{}", chrono::Utc::now().timestamp_millis()),
        object: "chat.completion".to_string(),
        model: model.to_string(),
        choices: vec![InferenceChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content,
                tool_calls,
                tool_call_id: None,
                extra: serde_json::Map::new(),
            },
            finish_reason: Some(finish_reason),
        }],
        usage: Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
    }
}

/// Extract content, tool calls, and finish reason from a Gemini candidate.
fn extract_candidate(
    candidate: Option<&Value>,
) -> (Option<Value>, Option<Vec<Value>>, String) {
    let Some(candidate) = candidate else {
        return (None, None, "stop".to_string());
    };

    let parts = candidate
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array());

    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<Value> = Vec::new();

    if let Some(parts) = parts {
        for (idx, part) in parts.iter().enumerate() {
            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                text_parts.push(text.to_string());
            }
            if let Some(fc) = part.get("functionCall") {
                let name = fc
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let args = fc
                    .get("args")
                    .map(|a| serde_json::to_string(a).unwrap_or_default())
                    .unwrap_or_default();

                tool_calls.push(serde_json::json!({
                    "id": format!("call_{idx}"),
                    "index": idx,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": args,
                    }
                }));
            }
        }
    }

    // Map finishReason.
    let raw_reason = candidate
        .get("finishReason")
        .and_then(|r| r.as_str())
        .unwrap_or("STOP");
    let finish_reason = match raw_reason {
        "STOP" => "stop",
        "MAX_TOKENS" => "length",
        "SAFETY" => "content_filter",
        "RECITATION" => "content_filter",
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

    (content, tool_calls_opt, finish_reason.to_string())
}

// ── SSE Stream Adapter ────────────────────────────────────────

/// Adapter that converts a Gemini SSE byte stream into `InferenceChunk`s.
///
/// Gemini uses standard SSE format (`data: {...}\n\n`), no named events.
/// Each chunk contains a complete partial Gemini response (not a delta).
/// No `[DONE]` sentinel — connection close means done.
struct GeminiSseStream<S> {
    inner: S,
    buffer: BytesMut,
    model: String,
    chunk_id: String,
    done: bool,
}

impl<S> GeminiSseStream<S> {
    fn new(inner: S, model: String) -> Self {
        let chunk_id = format!("gemini-{}", chrono::Utc::now().timestamp_millis());
        Self {
            inner,
            buffer: BytesMut::with_capacity(4096),
            model,
            chunk_id,
            done: false,
        }
    }
}

impl<S> Stream for GeminiSseStream<S>
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

                // Parse "data: {...}" line.
                let data_str = if let Some(rest) = segment.strip_prefix("data:") {
                    rest.trim()
                } else {
                    // Skip non-data lines
                    continue;
                };

                if data_str.is_empty() {
                    continue;
                }

                let gemini: Value = match serde_json::from_str(data_str) {
                    Ok(v) => v,
                    Err(e) => {
                        return Poll::Ready(Some(Err(anyhow::anyhow!(
                            "parse Gemini SSE chunk: {e}"
                        ))));
                    }
                };

                let candidate = gemini
                    .get("candidates")
                    .and_then(|c| c.as_array())
                    .and_then(|arr| arr.first());

                let (content, tool_calls, finish_reason_str) = extract_candidate(candidate);

                // Determine if this is the final chunk.
                let has_finish = candidate
                    .and_then(|c| c.get("finishReason"))
                    .is_some();

                let finish_reason = if has_finish {
                    Some(finish_reason_str)
                } else {
                    None
                };

                // Usage from this chunk (usually present in the last chunk).
                let usage_meta = gemini.get("usageMetadata");
                let usage = if usage_meta.is_some() {
                    let prompt_tokens = usage_meta
                        .and_then(|u| u.get("promptTokenCount"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let completion_tokens = usage_meta
                        .and_then(|u| u.get("candidatesTokenCount"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    Some(Usage {
                        prompt_tokens,
                        completion_tokens,
                        total_tokens: prompt_tokens + completion_tokens,
                    })
                } else {
                    None
                };

                let chunk = InferenceChunk {
                    id: this.chunk_id.clone(),
                    object: "chat.completion.chunk".to_string(),
                    model: this.model.clone(),
                    choices: vec![ChunkChoice {
                        index: 0,
                        delta: ChatMessage {
                            role: if content.is_some() || tool_calls.is_some() {
                                "assistant".to_string()
                            } else {
                                String::new()
                            },
                            content,
                            tool_calls,
                            tool_call_id: None,
                            extra: serde_json::Map::new(),
                        },
                        finish_reason,
                    }],
                    usage,
                };
                return Poll::Ready(Some(Ok(chunk)));
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
                    // Process any remaining data in buffer.
                    let remaining = std::mem::take(&mut this.buffer);
                    let text = String::from_utf8_lossy(&remaining);
                    let text = text.trim();
                    if !text.is_empty() {
                        if let Some(data_str) = text.strip_prefix("data:") {
                            let data_str = data_str.trim();
                            if let Ok(gemini) = serde_json::from_str::<Value>(data_str) {
                                let candidate = gemini
                                    .get("candidates")
                                    .and_then(|c| c.as_array())
                                    .and_then(|arr| arr.first());
                                let (content, tool_calls, finish_reason_str) =
                                    extract_candidate(candidate);

                                let chunk = InferenceChunk {
                                    id: this.chunk_id.clone(),
                                    object: "chat.completion.chunk".to_string(),
                                    model: this.model.clone(),
                                    choices: vec![ChunkChoice {
                                        index: 0,
                                        delta: ChatMessage {
                                            role: "assistant".to_string(),
                                            content,
                                            tool_calls,
                                            tool_call_id: None,
                                            extra: serde_json::Map::new(),
                                        },
                                        finish_reason: Some(finish_reason_str),
                                    }],
                                    usage: None,
                                };
                                return Poll::Ready(Some(Ok(chunk)));
                            }
                        }
                    }
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

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(messages: Vec<ChatMessage>) -> InferenceRequest {
        InferenceRequest {
            model: "gemini-2.0-flash".into(),
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

    fn assistant_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: "assistant".into(),
            content: Some(Value::String(text.into())),
            tool_calls: None,
            tool_call_id: None,
            extra: serde_json::Map::new(),
        }
    }

    #[test]
    fn request_translates_messages_and_system() {
        let req = make_request(vec![
            system_msg("You are helpful."),
            user_msg("Hello"),
            assistant_msg("Hi there"),
            user_msg("How are you?"),
        ]);
        let body = build_gemini_request(&req);

        // System instruction extracted.
        let sys = &body["systemInstruction"]["parts"][0]["text"];
        assert_eq!(sys, "You are helpful.");

        // Contents should have 3 messages (no system).
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 3);

        // Role mapping: "assistant" → "model"
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[1]["role"], "model");
        assert_eq!(contents[2]["role"], "user");

        // Parts contain text.
        assert_eq!(contents[0]["parts"][0]["text"], "Hello");
    }

    #[test]
    fn request_translates_vision_content() {
        let msg = ChatMessage {
            role: "user".into(),
            content: Some(serde_json::json!([
                {"type": "text", "text": "What is this?"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,abc123"}}
            ])),
            tool_calls: None,
            tool_call_id: None,
            extra: serde_json::Map::new(),
        };
        let req = make_request(vec![msg]);
        let body = build_gemini_request(&req);

        let parts = body["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["text"], "What is this?");
        assert_eq!(parts[1]["inline_data"]["mime_type"], "image/png");
        assert_eq!(parts[1]["inline_data"]["data"], "abc123");
    }

    #[test]
    fn request_translates_tools() {
        let mut req = make_request(vec![user_msg("Weather?")]);
        req.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get weather",
                "parameters": {
                    "type": "object",
                    "properties": {"city": {"type": "string"}}
                }
            }
        })]);

        let body = build_gemini_request(&req);
        let decls = body["tools"][0]["functionDeclarations"].as_array().unwrap();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0]["name"], "get_weather");
        assert_eq!(decls[0]["description"], "Get weather");
        assert!(decls[0].get("parameters").is_some());
    }

    #[test]
    fn request_maps_generation_config() {
        let mut req = make_request(vec![user_msg("Hi")]);
        req.temperature = Some(0.5);
        req.max_tokens = Some(1024);
        req.top_p = Some(0.9);
        req.stop = Some(serde_json::json!(["END"]));

        let body = build_gemini_request(&req);
        let config = &body["generationConfig"];
        assert_eq!(config["temperature"], 0.5);
        assert_eq!(config["maxOutputTokens"], 1024);
        assert_eq!(config["topP"], 0.9);
        assert_eq!(config["stopSequences"][0], "END");
    }

    #[test]
    fn response_translates_text_and_usage() {
        let resp = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello world"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5
            }
        });

        let canonical = gemini_response_to_canonical("gemini-2.0-flash", &resp);

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
    fn response_translates_function_call() {
        let resp = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "name": "get_weather",
                            "args": {"city": "Tokyo"}
                        }
                    }],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 15,
                "candidatesTokenCount": 8
            }
        });

        let canonical = gemini_response_to_canonical("gemini-2.0-flash", &resp);
        let tool_calls = canonical.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls[0]["function"]["name"], "get_weather");
        // Arguments should be a JSON string.
        let args = tool_calls[0]["function"]["arguments"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(args).unwrap();
        assert_eq!(parsed["city"], "Tokyo");
    }

    #[test]
    fn response_maps_safety_finish_reason() {
        let resp = serde_json::json!({
            "candidates": [{
                "content": {"parts": [{"text": ""}], "role": "model"},
                "finishReason": "SAFETY"
            }],
            "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 0}
        });

        let canonical = gemini_response_to_canonical("gemini-2.0-flash", &resp);
        assert_eq!(
            canonical.choices[0].finish_reason.as_deref(),
            Some("content_filter")
        );
    }
}
