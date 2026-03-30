//! Ollama inference adapter.
//!
//! Translates canonical OpenAI-shaped types to/from Ollama's native protocol:
//! - Chat: POST `/api/chat` with Ollama JSON, NDJSON streaming
//! - Embed: POST `/api/embed` with `{model, input}` → `{embeddings: [[...]]}`
//!
//! Key translations (verified against live Ollama 0.7+):
//! - `max_tokens` → `options.num_predict`
//! - `temperature` → `options.temperature`
//! - `top_p` → `options.top_p`
//! - Vision: OpenAI `image_url` content parts → Ollama `images: ["base64"]`
//! - Tool args: Ollama returns object → canonical expects JSON string
//! - Timing: nanosecond `eval_count`/`prompt_eval_count` → `usage` tokens
//! - Stream: NDJSON (one JSON per `\n`) → `InferenceChunk` (SSE shape)

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::BoxFuture;

/// Ollama inference adapter.
pub struct OllamaAdapter {
    http: Client,
}

impl OllamaAdapter {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(4)
            // No client-level timeout — inference streams live until done.
            .build()
            .expect("HTTP client");
        Self { http }
    }
}

impl InferenceAdapter for OllamaAdapter {
    fn infer(
        &self,
        ctx: &AdapterContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<InferenceResponse>> {
        let url = format!("{}/api/chat", ctx.endpoint);
        let model = ctx.model.clone();
        let body = build_ollama_request(&model, &req, false);

        Box::pin(async move {
            let resp = self
                .http
                .post(&url)
                .json(&body)
                .timeout(Duration::from_secs(300))
                .send()
                .await
                .context("POST /api/chat")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Ollama /api/chat HTTP {status}: {text}");
            }

            let ollama: Value = resp.json().await.context("parse Ollama response")?;
            Ok(ollama_response_to_canonical(&model, &ollama))
        })
    }

    fn infer_stream(
        &self,
        ctx: &AdapterContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<BoxStream<'static, Result<InferenceChunk>>>> {
        let url = format!("{}/api/chat", ctx.endpoint);
        let model = ctx.model.clone();
        let body = build_ollama_request(&model, &req, true);
        let http = self.http.clone();

        Box::pin(async move {
            let resp = http
                .post(&url)
                .json(&body)
                .send()
                .await
                .context("POST /api/chat stream")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Ollama /api/chat stream HTTP {status}: {text}");
            }

            let stream = resp.bytes_stream();
            Ok(Box::pin(OllamaNdjsonStream::new(stream, model)) as BoxStream<'static, Result<InferenceChunk>>)
        })
    }

    fn embed(
        &self,
        ctx: &AdapterContext,
        req: EmbedRequest,
    ) -> BoxFuture<'_, Result<EmbedResponse>> {
        let url = format!("{}/api/embed", ctx.endpoint);
        let model = ctx.model.clone();

        Box::pin(async move {
            let body = serde_json::json!({
                "model": model,
                "input": req.input,
            });

            let resp = self
                .http
                .post(&url)
                .json(&body)
                .timeout(Duration::from_secs(60))
                .send()
                .await
                .context("POST /api/embed")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Ollama /api/embed HTTP {status}: {text}");
            }

            let ollama: Value = resp.json().await.context("parse embed response")?;

            // Ollama returns {embeddings: [[f64...], ...], prompt_eval_count, total_duration}
            let embeddings = ollama
                .get("embeddings")
                .and_then(|v| v.as_array())
                .unwrap_or(&Vec::new())
                .clone();

            let prompt_tokens = ollama
                .get("prompt_eval_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let data: Vec<EmbeddingData> = embeddings
                .iter()
                .enumerate()
                .map(|(i, emb)| EmbeddingData {
                    object: "embedding".to_string(),
                    index: i as u32,
                    embedding: emb
                        .as_array()
                        .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
                        .unwrap_or_default(),
                })
                .collect();

            Ok(EmbedResponse {
                object: "list".to_string(),
                data,
                model,
                usage: Usage {
                    prompt_tokens,
                    completion_tokens: 0,
                    total_tokens: prompt_tokens,
                },
            })
        })
    }
}

// ── Request Translation ─────────────────────────────────────────

/// Build Ollama `/api/chat` request body from canonical `InferenceRequest`.
fn build_ollama_request(model: &str, req: &InferenceRequest, stream: bool) -> Value {
    let mut messages = Vec::new();

    for msg in &req.messages {
        let mut ollama_msg = serde_json::json!({
            "role": msg.role,
        });

        // Extract vision images from OpenAI content parts → Ollama `images` array
        let (text_content, images) = extract_content_and_images(msg);

        if let Some(text) = text_content {
            ollama_msg["content"] = Value::String(text);
        }
        if !images.is_empty() {
            ollama_msg["images"] = Value::Array(images);
        }

        // Pass through tool_calls and tool_call_id
        if let Some(ref tool_calls) = msg.tool_calls {
            // Canonical tool_calls have function.arguments as JSON string.
            // Ollama expects arguments as object — parse them.
            let ollama_calls: Vec<Value> = tool_calls
                .iter()
                .map(|tc| {
                    let mut call = tc.clone();
                    if let Some(func) = call.get_mut("function") {
                        if let Some(args_str) = func.get("arguments").and_then(|v| v.as_str()) {
                            if let Ok(parsed) = serde_json::from_str::<Value>(args_str) {
                                func["arguments"] = parsed;
                            }
                        }
                    }
                    call
                })
                .collect();
            ollama_msg["tool_calls"] = Value::Array(ollama_calls);
        }

        messages.push(ollama_msg);
    }

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": stream,
    });

    // Build options from canonical parameters
    let mut options = serde_json::Map::new();
    if let Some(temp) = req.temperature {
        options.insert("temperature".into(), serde_json::json!(temp));
    }
    if let Some(max_tokens) = req.max_tokens {
        options.insert("num_predict".into(), serde_json::json!(max_tokens));
    }
    if let Some(top_p) = req.top_p {
        options.insert("top_p".into(), serde_json::json!(top_p));
    }
    if !options.is_empty() {
        body["options"] = Value::Object(options);
    }

    // Stop sequences
    if let Some(ref stop) = req.stop {
        body["stop"] = stop.clone();
    }

    // Tools pass through (Ollama 0.7+ supports OpenAI tool format)
    if let Some(ref tools) = req.tools {
        body["tools"] = Value::Array(tools.clone());
    }

    body
}

/// Extract text content and base64 images from a ChatMessage.
///
/// OpenAI format: `content: [{type:"text", text:"..."}, {type:"image_url", image_url:{url:"data:image/jpeg;base64,..."}}]`
/// Ollama format: `content: "text", images: ["base64..."]`
fn extract_content_and_images(msg: &ChatMessage) -> (Option<String>, Vec<Value>) {
    let Some(ref content) = msg.content else {
        return (None, vec![]);
    };

    // Simple string content
    if let Some(text) = content.as_str() {
        return (Some(text.to_string()), vec![]);
    }

    // Array of content parts
    let Some(parts) = content.as_array() else {
        return (Some(content.to_string()), vec![]);
    };

    let mut text_parts = Vec::new();
    let mut images = Vec::new();

    for part in parts {
        match part.get("type").and_then(|v| v.as_str()) {
            Some("text") => {
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    text_parts.push(text.to_string());
                }
            }
            Some("image_url") => {
                if let Some(url) = part
                    .get("image_url")
                    .and_then(|v| v.get("url"))
                    .and_then(|v| v.as_str())
                {
                    // Strip data URI prefix: "data:image/jpeg;base64,..." → "..."
                    let base64 = if let Some((_prefix, data)) = url.split_once(",") {
                        data
                    } else {
                        url
                    };
                    images.push(Value::String(base64.to_string()));
                }
            }
            _ => {}
        }
    }

    let text = if text_parts.is_empty() {
        None
    } else {
        Some(text_parts.join("\n"))
    };

    (text, images)
}

// ── Response Translation ────────────────────────────────────────

/// Convert a non-streaming Ollama response to canonical `InferenceResponse`.
fn ollama_response_to_canonical(model: &str, ollama: &Value) -> InferenceResponse {
    let message = ollama.get("message").cloned().unwrap_or(Value::Null);
    let done_reason = ollama
        .get("done_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("stop");

    let finish_reason = match done_reason {
        "stop" => "stop",
        "length" => "length",
        other => other,
    };

    // Build canonical message
    let role = message
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("assistant");
    let content = message.get("content").cloned();

    // Translate tool_calls: Ollama returns arguments as object → canonical needs JSON string
    let tool_calls = message.get("tool_calls").and_then(|v| v.as_array()).map(|calls| {
        calls
            .iter()
            .map(|call| {
                let mut canonical = call.clone();
                if let Some(func) = canonical.get_mut("function") {
                    if let Some(args) = func.get("arguments") {
                        if !args.is_string() {
                            func["arguments"] =
                                Value::String(serde_json::to_string(args).unwrap_or_default());
                        }
                    }
                }
                canonical
            })
            .collect()
    });

    let prompt_tokens = ollama
        .get("prompt_eval_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let completion_tokens = ollama
        .get("eval_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    InferenceResponse {
        id: format!("ollama-{}", chrono::Utc::now().timestamp_millis()),
        object: "chat.completion".to_string(),
        model: model.to_string(),
        choices: vec![InferenceChoice {
            index: 0,
            message: ChatMessage {
                role: role.to_string(),
                content,
                tool_calls,
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

// ── NDJSON Stream Adapter ───────────────────────────────────────

use bytes::BytesMut;
use futures_util::stream::Stream;
use std::pin::Pin;
use std::task::Poll;

/// Adapter that converts an Ollama NDJSON byte stream into `InferenceChunk`s.
///
/// Ollama sends one JSON object per `\n`-delimited line. TCP chunks may
/// contain partial lines — this adapter buffers until a complete line is
/// available, then parses and translates to canonical format.
struct OllamaNdjsonStream<S> {
    inner: S,
    buffer: BytesMut,
    model: String,
    chunk_id: String,
    done: bool,
}

impl<S> OllamaNdjsonStream<S> {
    fn new(inner: S, model: String) -> Self {
        let chunk_id = format!("ollama-{}", chrono::Utc::now().timestamp_millis());
        Self {
            inner,
            buffer: BytesMut::with_capacity(4096),
            model,
            chunk_id,
            done: false,
        }
    }
}

impl<S> Stream for OllamaNdjsonStream<S>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<InferenceChunk>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if this.done {
            return Poll::Ready(None);
        }

        loop {
            // Check buffer for a complete line
            if let Some(newline_pos) = this.buffer.iter().position(|&b| b == b'\n') {
                let line = this.buffer.split_to(newline_pos + 1);
                let line = line.trim_ascii();

                if line.is_empty() {
                    continue;
                }

                match serde_json::from_slice::<Value>(line) {
                    Ok(obj) => {
                        let chunk = ollama_ndjson_to_chunk(&this.chunk_id, &this.model, &obj);
                        if obj.get("done").and_then(|v| v.as_bool()) == Some(true) {
                            this.done = true;
                        }
                        return Poll::Ready(Some(Ok(chunk)));
                    }
                    Err(e) => {
                        return Poll::Ready(Some(Err(anyhow::anyhow!(
                            "parse NDJSON line: {e}"
                        ))));
                    }
                }
            }

            // Need more data from the inner stream
            match Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    this.buffer.extend_from_slice(&bytes);
                    // Loop back to check for complete lines
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(anyhow::anyhow!("stream error: {e}"))));
                }
                Poll::Ready(None) => {
                    this.done = true;
                    // Process any remaining data in buffer
                    if !this.buffer.is_empty() {
                        let remaining = std::mem::take(&mut this.buffer);
                        let trimmed = remaining.trim_ascii();
                        if !trimmed.is_empty() {
                            if let Ok(obj) = serde_json::from_slice::<Value>(trimmed) {
                                let chunk =
                                    ollama_ndjson_to_chunk(&this.chunk_id, &this.model, &obj);
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

/// Convert a single Ollama NDJSON line to an `InferenceChunk`.
fn ollama_ndjson_to_chunk(id: &str, model: &str, obj: &Value) -> InferenceChunk {
    let is_done = obj.get("done").and_then(|v| v.as_bool()) == Some(true);
    let message = obj.get("message").cloned().unwrap_or(Value::Null);

    let content = message.get("content").cloned();
    let role = message
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Translate tool_calls (object args → JSON string)
    let tool_calls = message.get("tool_calls").and_then(|v| v.as_array()).map(|calls| {
        calls
            .iter()
            .map(|call| {
                let mut canonical = call.clone();
                if let Some(func) = canonical.get_mut("function") {
                    if let Some(args) = func.get("arguments") {
                        if !args.is_string() {
                            func["arguments"] =
                                Value::String(serde_json::to_string(args).unwrap_or_default());
                        }
                    }
                }
                canonical
            })
            .collect()
    });

    let finish_reason = if is_done {
        let reason = obj
            .get("done_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("stop");
        Some(
            match reason {
                "stop" => "stop",
                "length" => "length",
                other => other,
            }
            .to_string(),
        )
    } else {
        None
    };

    let usage = if is_done {
        let prompt_tokens = obj
            .get("prompt_eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let completion_tokens = obj.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0);
        Some(Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        })
    } else {
        None
    };

    InferenceChunk {
        id: id.to_string(),
        object: "chat.completion.chunk".to_string(),
        model: model.to_string(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChatMessage {
                role: if role.is_empty() {
                    String::new()
                } else {
                    role
                },
                content,
                tool_calls,
                tool_call_id: None,
                extra: serde_json::Map::new(),
            },
            finish_reason,
        }],
        usage,
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_basic() {
        let req = InferenceRequest {
            model: "test".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: Some(Value::String("Hello".into())),
                tool_calls: None,
                tool_call_id: None,
                extra: serde_json::Map::new(),
            }],
            temperature: Some(0.7),
            max_tokens: Some(100),
            top_p: None,
            stop: None,
            tools: None,
            tool_choice: None,
            stream: false,
            extra: serde_json::Map::new(),
        };

        let body = build_ollama_request("test", &req, false);
        assert_eq!(body["model"], "test");
        assert_eq!(body["stream"], false);
        assert_eq!(body["options"]["temperature"], 0.7);
        assert_eq!(body["options"]["num_predict"], 100);
        assert_eq!(body["messages"][0]["content"], "Hello");
    }

    #[test]
    fn extract_images_from_openai_content() {
        let msg = ChatMessage {
            role: "user".into(),
            content: Some(serde_json::json!([
                {"type": "text", "text": "What is this?"},
                {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,abc123"}}
            ])),
            tool_calls: None,
            tool_call_id: None,
            extra: serde_json::Map::new(),
        };

        let (text, images) = extract_content_and_images(&msg);
        assert_eq!(text.unwrap(), "What is this?");
        assert_eq!(images.len(), 1);
        assert_eq!(images[0], "abc123");
    }

    #[test]
    fn response_translates_tool_args_to_string() {
        let ollama = serde_json::json!({
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "function": {
                        "name": "get_weather",
                        "arguments": {"city": "Tokyo"}
                    }
                }]
            },
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 10,
            "eval_count": 5
        });

        let resp = ollama_response_to_canonical("test", &ollama);
        let tool_calls = resp.choices[0].message.tool_calls.as_ref().unwrap();
        let args = tool_calls[0]["function"]["arguments"].as_str().unwrap();
        // Should be a JSON string, not an object
        assert!(args.contains("Tokyo"));
        let parsed: Value = serde_json::from_str(args).unwrap();
        assert_eq!(parsed["city"], "Tokyo");
    }

    #[test]
    fn ndjson_chunk_mid_stream() {
        let obj = serde_json::json!({
            "message": {"role": "assistant", "content": "Hello"},
            "done": false
        });
        let chunk = ollama_ndjson_to_chunk("test-id", "model", &obj);
        assert_eq!(chunk.choices[0].delta.content, Some(Value::String("Hello".into())));
        assert_eq!(chunk.choices[0].finish_reason, None);
        assert!(chunk.usage.is_none());
    }

    #[test]
    fn ndjson_chunk_final() {
        let obj = serde_json::json!({
            "message": {"role": "assistant", "content": ""},
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 42,
            "eval_count": 10
        });
        let chunk = ollama_ndjson_to_chunk("test-id", "model", &obj);
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
        let usage = chunk.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 42);
        assert_eq!(usage.completion_tokens, 10);
    }
}
