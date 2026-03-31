//! Ollama provider — unified lifecycle + inference for Ollama instances.
//!
//! Implements the `Provider` trait, encapsulating all knowledge of
//! Ollama's HTTP API, NDJSON streaming, model management, and inference
//! protocol translation.
//!
//! Key translations (verified against live Ollama 0.7+):
//! - `max_tokens` -> `options.num_predict`
//! - `temperature` -> `options.temperature`
//! - `top_p` -> `options.top_p`
//! - Vision: OpenAI `image_url` content parts -> Ollama `images: ["base64"]`
//! - Tool args: Ollama returns object -> canonical expects JSON string
//! - Timing: `eval_count`/`prompt_eval_count` -> `usage` tokens
//! - Stream: NDJSON (one JSON per `\n`) -> `InferenceChunk` (SSE shape)

use anyhow::{Context, Result};
use bytes::BytesMut;
use futures_util::stream::Stream;
use futures_util::StreamExt;
use serde_json::Value;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::traits::{
    BenchmarkSample, BoxFuture, DiscoveryConfig, FormSchema, ProbeResult, Provider,
    ProviderContext, Sample, ServiceModel, SyncProgress,
};
use crate::domain::types::{Capability, OfferingKind, ServiceInstance};
use crate::offerings::ollama::client::OllamaClient;
use crate::offerings::ollama::types::OllamaPullProgress;

// ── Provider ───────────────────────────────────────────────────

/// Ollama provider.
///
/// Delegates protocol operations to `OllamaClient` for lifecycle
/// (probe, enumerate, benchmark, sync) and implements inference
/// translation inline (infer, infer_stream, embed).
pub struct OllamaProvider {
    client: OllamaClient,
}

impl OllamaProvider {
    pub fn new() -> Self {
        Self {
            client: OllamaClient::new(),
        }
    }

    /// Expose the client for tasks that need direct access (profiling, sync).
    pub fn client(&self) -> &OllamaClient {
        &self.client
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Capabilities that Ollama instances can provide.
const OLLAMA_CAPABILITIES: &[Capability] = &[
    Capability::Chat,
    Capability::Embed,
    Capability::Vision,
    Capability::Tools,
    Capability::Think,
];

impl Provider for OllamaProvider {
    fn kind(&self) -> OfferingKind {
        OfferingKind::Ollama
    }

    fn capabilities(&self) -> &[Capability] {
        OLLAMA_CAPABILITIES
    }

    fn discovery(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "ollama".into(),
        }
    }

    // ── Lifecycle ───────────────────────────────────────────────

    fn probe(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<ProbeResult>> {
        let endpoint = ctx.endpoint.clone();
        Box::pin(async move {
            // Health check: GET / should return "Ollama is running"
            let health_url = format!("{endpoint}/");
            let resp = self
                .client
                .forward_request(
                    &endpoint,
                    "/",
                    reqwest::Method::GET,
                    bytes::Bytes::new(),
                    reqwest::header::HeaderMap::new(),
                )
                .await
                .context("probe health check")?;

            if !resp.status().is_success() {
                anyhow::bail!(
                    "probe failed: {} returned HTTP {}",
                    health_url,
                    resp.status()
                );
            }

            // Version query
            let version = self
                .client
                .get_version(&endpoint)
                .await
                .ok()
                .map(|v| v.version);

            Ok(ProbeResult {
                version,
                capabilities: OLLAMA_CAPABILITIES.to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({}),
            })
        })
    }

    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = ctx.endpoint.clone();
        Box::pin(async move {
            let (_, _, model_infos, _) = self
                .client
                .full_profile(&endpoint)
                .await
                .context("enumerate models")?;

            let models = model_infos
                .into_iter()
                .map(|info| {
                    let mut capabilities = ollama_capabilities_from_strings(&info.capabilities);
                    let specializations = infer_specializations(&info.name, &info.family);

                    // Add Ocr capability for OCR-specialized models
                    if specializations.contains(&"ocr".to_string())
                        && !capabilities.contains(&Capability::Ocr)
                    {
                        capabilities.push(Capability::Ocr);
                    }
                    ServiceModel {
                        name: info.name,
                        capabilities,
                        specializations,
                        vram_bytes: info.vram_bytes,
                        metadata: serde_json::json!({
                            "parameter_count": info.parameter_count,
                            "parameter_size": info.parameter_size,
                            "quantization_level": info.quantization_level,
                            "family": info.family,
                            "families": info.families,
                            "format": info.format,
                            "size_disk": info.size_disk,
                            "context_length": info.context_length,
                        }),
                    }
                })
                .collect();

            Ok(models)
        })
    }

    // ── Inference ───────────────────────────────────────────────

    fn infer(
        &self,
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<InferenceResponse>> {
        let endpoint = ctx.endpoint.clone();
        let model = ctx
            .model
            .clone()
            .unwrap_or_else(|| req.model.clone());
        let body = build_ollama_request(&model, &req, false);

        Box::pin(async move {
            let http = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .pool_max_idle_per_host(4)
                .build()
                .context("build inference HTTP client")?;

            let resp = http
                .post(format!("{endpoint}/api/chat"))
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
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<BoxStream<'static, Result<InferenceChunk>>>> {
        let endpoint = ctx.endpoint.clone();
        let model = ctx
            .model
            .clone()
            .unwrap_or_else(|| req.model.clone());
        let body = build_ollama_request(&model, &req, true);

        Box::pin(async move {
            // Build a dedicated client for streaming (no global timeout).
            let http = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .pool_max_idle_per_host(4)
                .build()
                .context("build streaming HTTP client")?;

            let resp = http
                .post(format!("{endpoint}/api/chat"))
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
            Ok(Box::pin(OllamaNdjsonStream::new(stream, model))
                as BoxStream<'static, Result<InferenceChunk>>)
        })
    }

    fn embed(
        &self,
        ctx: &ProviderContext,
        req: EmbedRequest,
    ) -> BoxFuture<'_, Result<EmbedResponse>> {
        let endpoint = ctx.endpoint.clone();
        let model = ctx
            .model
            .clone()
            .unwrap_or_else(|| req.model.clone());

        Box::pin(async move {
            let http = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .pool_max_idle_per_host(4)
                .build()
                .context("build embed HTTP client")?;

            let body = serde_json::json!({
                "model": model,
                "input": req.input,
            });

            let resp = http
                .post(format!("{endpoint}/api/embed"))
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
                .cloned()
                .unwrap_or_default();

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

    // ── Optional ────────────────────────────────────────────────

    fn vram_estimate(&self, model: &ServiceModel) -> Option<u64> {
        // If we have authoritative VRAM from /api/ps, use it
        if let Some(vram) = model.vram_bytes {
            return Some(vram);
        }

        // Fallback: estimate from disk size (GGUF models are roughly
        // 1.1x disk size when loaded into VRAM due to KV cache overhead)
        let size_disk = model.metadata.get("size_disk")?.as_u64()?;
        if size_disk > 0 {
            Some((size_disk as f64 * 1.1) as u64)
        } else {
            None
        }
    }

    fn benchmark(
        &self,
        ctx: &ProviderContext,
        model: &str,
        capability: Capability,
    ) -> BoxFuture<'_, Result<BenchmarkSample>> {
        let endpoint = ctx.endpoint.clone();
        let model = model.to_string();

        Box::pin(async move {
            let sample = match capability {
                Capability::Chat => {
                    let result = self
                        .client
                        .benchmark_generate(&endpoint, &model, "Why is the sky blue?", 80)
                        .await
                        .context("benchmark generate")?;

                    let tps = if result.eval_duration > 0 {
                        (result.eval_count as f64 / result.eval_duration as f64) * 1_000_000_000.0
                    } else {
                        0.0
                    };

                    Sample {
                        cold_start_ms: result.load_duration / 1_000_000,
                        tokens_per_second: tps,
                        total_duration_ms: result.total_duration / 1_000_000,
                    }
                }
                Capability::Embed => {
                    let result = self
                        .client
                        .benchmark_embed(
                            &endpoint,
                            &model,
                            "The quick brown fox jumps over the lazy dog.",
                        )
                        .await
                        .context("benchmark embed")?;

                    Sample {
                        cold_start_ms: result.load_duration / 1_000_000,
                        tokens_per_second: 0.0,
                        total_duration_ms: result.total_duration / 1_000_000,
                    }
                }
                _ => {
                    // Capabilities without specific benchmark prompts
                    return Ok(BenchmarkSample {
                        samples: vec![],
                        capability,
                    });
                }
            };

            Ok(BenchmarkSample {
                samples: vec![sample],
                capability,
            })
        })
    }

    fn sync_resource(
        &self,
        resource: &str,
        _from: &ServiceInstance,
        to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<SyncProgress>> {
        let model = resource.to_string();
        let target_endpoint = to.endpoint.clone();
        Box::pin(async move {
            // Pull the model on the target instance
            let mut stream = self
                .client
                .pull_model(&target_endpoint, &model)
                .await
                .context("initiate model pull")?;

            let mut bytes_transferred: u64 = 0;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.context("pull stream chunk")?;
                // Parse NDJSON progress lines
                if let Ok(progress) = serde_json::from_slice::<OllamaPullProgress>(&chunk)
                    && let Some(completed) = progress.completed
                {
                    bytes_transferred = completed;
                }
            }

            Ok(SyncProgress::Completed { bytes_transferred })
        })
    }

    // ── Form Schema (ORCH-0017) ──────────────────────────────────

    fn form_schema(&self, _model: &str, capability: Capability) -> FormSchema {
        match capability {
            Capability::Chat | Capability::Think | Capability::Tools | Capability::Vision => {
                FormSchema {
                    schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "message": {"type": "string", "title": "Message", "minLength": 1},
                            "temperature": {"type": "number", "title": "Temperature", "minimum": 0, "maximum": 2, "default": 0.7},
                            "max_tokens": {"type": "integer", "title": "Max Tokens", "minimum": 1, "maximum": 128000, "default": 4096},
                            "system": {"type": "string", "title": "System Prompt"}
                        },
                        "required": ["message"]
                    }),
                    ui_schema: serde_json::json!({
                        "message": {"ui:widget": "textarea", "ui:options": {"rows": 3}},
                        "system": {"ui:widget": "textarea", "ui:options": {"rows": 2}},
                        "temperature": {"ui:widget": "range"},
                        "ui:order": ["message", "system", "temperature", "max_tokens"]
                    }),
                }
            }
            Capability::Embed => FormSchema {
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "input": {"type": "string", "title": "Text to embed", "minLength": 1}
                    },
                    "required": ["input"]
                }),
                ui_schema: serde_json::json!({
                    "input": {"ui:widget": "textarea", "ui:options": {"rows": 2}}
                }),
            },
            _ => FormSchema::default(),
        }
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

        // Extract vision images from OpenAI content parts -> Ollama `images` array
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
            // Ollama expects arguments as object -- parse them.
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
                    // Strip data URI prefix: "data:image/jpeg;base64,..." -> "..."
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

    // Translate tool_calls: Ollama returns arguments as object -> canonical needs JSON string
    let tool_calls = message
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .map(|calls| {
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

/// Adapter that converts an Ollama NDJSON byte stream into `InferenceChunk`s.
///
/// Ollama sends one JSON object per `\n`-delimited line. TCP chunks may
/// contain partial lines -- this adapter buffers until a complete line is
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

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
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

    // Translate tool_calls (object args -> JSON string)
    let tool_calls = message
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .map(|calls| {
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

// ── Helpers ─────────────────────────────────────────────────────

/// Map Ollama capability strings (from `/api/show`) to domain `Capability` variants.
fn ollama_capabilities_from_strings(ollama_caps: &[String]) -> Vec<Capability> {
    let mut caps = Vec::new();

    for cap_str in ollama_caps {
        match cap_str.as_str() {
            "completion" | "chat" => {
                if !caps.contains(&Capability::Chat) {
                    caps.push(Capability::Chat);
                }
            }
            "embedding" | "embed" => {
                if !caps.contains(&Capability::Embed) {
                    caps.push(Capability::Embed);
                }
            }
            "vision" => caps.push(Capability::Vision),
            "tools" => caps.push(Capability::Tools),
            "thinking" | "think" => caps.push(Capability::Think),
            _ => {
                tracing::trace!(capability = %cap_str, "unknown Ollama capability, ignoring");
            }
        }
    }

    // If no capabilities were reported but the model exists, assume at least Chat
    if caps.is_empty() {
        caps.push(Capability::Chat);
    }

    caps
}

/// Infer specialization tags from model name and family.
fn infer_specializations(name: &str, family: &Option<String>) -> Vec<String> {
    let lower = name.to_lowercase();
    let family_lower = family.as_deref().unwrap_or("").to_lowercase();
    let mut tags = Vec::new();

    if lower.contains("ocr") {
        tags.push("ocr".to_string());
    }
    if lower.contains("embed") || lower.contains("minilm") || lower.contains("bge") {
        tags.push("embedding".to_string());
    }
    if lower.contains("deepseek-r1") || lower.contains("reasoning") {
        tags.push("reasoning".to_string());
    }
    if lower.contains("code") || lower.contains("starcoder") || lower.contains("codellama") {
        tags.push("coding".to_string());
    }
    if lower.contains("aya") || lower.contains("translate") || lower.contains("multilingual") {
        tags.push("multilingual".to_string());
    }
    if lower.contains("tiny") || lower.contains("mini") || lower.contains("small") {
        tags.push("compact".to_string());
    }
    if lower.contains("vision") || lower.contains("vl") || family_lower.contains("clip") {
        tags.push("vision".to_string());
    }

    tags
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
        assert_eq!(
            chunk.choices[0].delta.content,
            Some(Value::String("Hello".into()))
        );
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

    #[test]
    fn capabilities_from_strings_defaults_to_chat() {
        let caps = ollama_capabilities_from_strings(&[]);
        assert_eq!(caps, vec![Capability::Chat]);
    }

    #[test]
    fn capabilities_from_strings_deduplicates() {
        let caps = ollama_capabilities_from_strings(&[
            "chat".into(),
            "completion".into(),
            "embedding".into(),
        ]);
        assert_eq!(caps.len(), 2);
        assert!(caps.contains(&Capability::Chat));
        assert!(caps.contains(&Capability::Embed));
    }

    #[test]
    fn specializations_detect_tags() {
        let tags = infer_specializations("deepseek-r1:32b", &Some("deepseek".into()));
        assert!(tags.contains(&"reasoning".to_string()));
    }

    #[test]
    fn vram_estimate_uses_authoritative_first() {
        let provider = OllamaProvider::new();
        let model = ServiceModel {
            name: "test".into(),
            capabilities: vec![],
            specializations: vec![],
            vram_bytes: Some(4_000_000_000),
            metadata: serde_json::json!({"size_disk": 2_000_000_000_u64}),
        };
        assert_eq!(provider.vram_estimate(&model), Some(4_000_000_000));
    }

    #[test]
    fn vram_estimate_falls_back_to_disk_size() {
        let provider = OllamaProvider::new();
        let model = ServiceModel {
            name: "test".into(),
            capabilities: vec![],
            specializations: vec![],
            vram_bytes: None,
            metadata: serde_json::json!({"size_disk": 2_000_000_000_u64}),
        };
        let estimate = provider.vram_estimate(&model).unwrap();
        assert_eq!(estimate, (2_000_000_000_f64 * 1.1) as u64);
    }
}
