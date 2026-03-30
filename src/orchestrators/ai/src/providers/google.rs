//! Google Gemini provider — unified lifecycle + inference for Gemini API.
//!
//! Translates canonical OpenAI-shaped types to/from the Google Gemini API.
//!
//! Key translations:
//! - Auth: `?key={api_key}` query parameter (NOT a header)
//! - Endpoint: `{base}/v1beta/models/{model}:generateContent` (non-streaming)
//!             `{base}/v1beta/models/{model}:streamGenerateContent?alt=sse` (streaming)
//! - Messages: `messages` -> `contents`, role `"assistant"` -> `"model"`
//! - System prompt: extracted -> `systemInstruction: {parts: [{text: "..."}]}`
//! - Parameters: nested in `generationConfig`
//! - Vision: OpenAI `image_url` -> Gemini `inline_data`
//! - Tools: OpenAI wrapper -> Gemini `functionDeclarations`
//! - Finish: `STOP` -> `stop`, `MAX_TOKENS` -> `length`, `SAFETY` -> `content_filter`
//! - Stream: Standard SSE `data:` lines (no named events); no `[DONE]` — EOF = done
//! - Embed: `embedContent` endpoint, sequential calls for batch

use anyhow::{Context, Result};
use base64::prelude::*;
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

/// Timeout for cloud API probe/enumerate calls.
const CLOUD_TIMEOUT: Duration = Duration::from_secs(15);

/// Timeout for non-streaming inference calls.
const INFER_TIMEOUT: Duration = Duration::from_secs(300);

/// Timeout for embedding calls.
const EMBED_TIMEOUT: Duration = Duration::from_secs(60);

const GOOGLE_CAPABILITIES: &[Capability] = &[
    Capability::Chat,
    Capability::Think,
    Capability::Tools,
    Capability::Vision,
    Capability::Embed,
    Capability::Image,
    Capability::Video,
    Capability::Speech,
    Capability::Music,
    Capability::Transcribe,
    Capability::Translate,
];

/// Google Gemini provider — stateless, receives all per-request state via `ProviderContext`.
pub struct GoogleProvider {
    http: Client,
}

impl GoogleProvider {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client build");
        Self { http }
    }
}

impl Default for GoogleProvider {
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
        .ok_or_else(|| anyhow::anyhow!("no API key configured for Google provider"))
}

impl Provider for GoogleProvider {
    fn kind(&self) -> OfferingKind {
        OfferingKind::Google
    }

    fn capabilities(&self) -> &[Capability] {
        GOOGLE_CAPABILITIES
    }

    fn discovery(&self) -> DiscoveryConfig {
        DiscoveryConfig::Configured
    }

    // ── Lifecycle ───────────────────────────────────────────────

    fn probe(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<ProbeResult>> {
        let endpoint = ctx.endpoint.clone();
        let api_key_result = require_api_key(ctx);

        Box::pin(async move {
            let api_key = api_key_result?;

            let resp = self
                .http
                .get(format!("{endpoint}/v1beta/models"))
                .query(&[("key", &api_key)])
                .timeout(CLOUD_TIMEOUT)
                .send()
                .await
                .context("probe Google /v1beta/models")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let summary = if body.len() > 256 {
                    format!("{}...", &body[..256])
                } else {
                    body
                };
                anyhow::bail!("Google probe failed: HTTP {status}: {summary}");
            }

            Ok(ProbeResult {
                version: None,
                capabilities: GOOGLE_CAPABILITIES.to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({
                    "provider": "google",
                }),
            })
        })
    }

    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = ctx.endpoint.clone();
        let api_key = ctx.api_key.clone().unwrap_or_default();

        Box::pin(async move {
            if api_key.is_empty() {
                return Ok(Vec::new());
            }

            let resp = self
                .http
                .get(format!("{endpoint}/v1beta/models"))
                .query(&[("key", &api_key)])
                .timeout(CLOUD_TIMEOUT)
                .send()
                .await
                .context("enumerate Google /v1beta/models")?;

            if !resp.status().is_success() {
                anyhow::bail!("enumerate failed: HTTP {}", resp.status());
            }

            let body: Value = resp.json().await.context("parse /v1/models response")?;
            let models = body
                .get("models")
                .and_then(|m| m.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            let raw_name = m.get("name").and_then(|v| v.as_str())?;
                            // Strip "models/" prefix: "models/gemini-2.0-flash" -> "gemini-2.0-flash"
                            let name = raw_name
                                .strip_prefix("models/")
                                .unwrap_or(raw_name)
                                .to_string();

                            let supported_methods: Vec<String> = m
                                .get("supportedGenerationMethods")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default();

                            let capabilities = model_capabilities(&name, &supported_methods);

                            // Filter out non-inference models (empty capabilities)
                            if capabilities.is_empty() {
                                return None;
                            }

                            let input_limit = m.get("inputTokenLimit").and_then(|v| v.as_u64());
                            let output_limit = m.get("outputTokenLimit").and_then(|v| v.as_u64());

                            Some(ServiceModel {
                                name,
                                capabilities,
                                specializations: vec!["cloud".to_string()],
                                vram_bytes: None,
                                metadata: serde_json::json!({
                                    "cloud": true,
                                    "provider": "google",
                                    "input_token_limit": input_limit,
                                    "output_token_limit": output_limit,
                                    "methods": supported_methods,
                                }),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            Ok(models)
        })
    }

    // ── Inference ───────────────────────────────────────────────

    fn infer(
        &self,
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<InferenceResponse>> {
        let model = ctx.model.as_deref().unwrap_or(&req.model);

        if is_imagen_model(model) {
            return self.infer_imagen(ctx, req);
        }
        if is_veo_model(model) {
            return self.infer_veo(ctx, req);
        }
        if is_bidi_model(model) {
            return Box::pin(async {
                anyhow::bail!("Live audio models require WebSocket — not supported via REST API")
            });
        }

        // Default: generateContent
        self.infer_generate_content(ctx, req)
    }

    fn infer_stream(
        &self,
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<BoxStream<'static, Result<InferenceChunk>>>> {
        let model = ctx.model.as_deref().unwrap_or(&req.model);

        if is_imagen_model(model) || is_veo_model(model) {
            let model = model.to_string();
            return Box::pin(async move {
                anyhow::bail!("{model} does not support streaming — use non-streaming inference")
            });
        }
        if is_bidi_model(model) {
            return Box::pin(async {
                anyhow::bail!("Live audio models require WebSocket — not supported via REST API")
            });
        }

        let api_key = ctx.api_key.clone().unwrap_or_default();
        let model = model.to_string();
        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent",
            ctx.endpoint, model
        );
        let body = build_gemini_request(&req);
        let http = self.http.clone();

        Box::pin(async move {
            let resp = http
                .post(&url)
                .query(&[("key", &api_key), ("alt", &"sse".to_string())])
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
            Ok(
                Box::pin(GeminiSseStream::new(stream, model))
                    as BoxStream<'static, Result<InferenceChunk>>,
            )
        })
    }

    fn embed(
        &self,
        ctx: &ProviderContext,
        req: EmbedRequest,
    ) -> BoxFuture<'_, Result<EmbedResponse>> {
        let api_key = ctx.api_key.clone().unwrap_or_default();
        let model = ctx
            .model
            .as_deref()
            .unwrap_or(&req.model);
        let model = model.to_string();
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
                    "{}/v1beta/models/{}:embedContent",
                    endpoint, model
                );

                let body = serde_json::json!({
                    "content": {
                        "parts": [{"text": text}]
                    }
                });

                let resp = self
                    .http
                    .post(&url)
                    .query(&[("key", &api_key)])
                    .header("content-type", "application/json")
                    .json(&body)
                    .timeout(EMBED_TIMEOUT)
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

    // ── TTS ────────────────────────────────────────────────────

    fn speak(
        &self,
        ctx: &ProviderContext,
        req: SpeechRequest,
    ) -> BoxFuture<'_, Result<SpeechResponse>> {
        let api_key = ctx.api_key.clone().unwrap_or_default();
        let model = ctx
            .model
            .as_deref()
            .unwrap_or("gemini-2.5-flash-preview-tts")
            .to_string();
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let url = format!("{}/v1beta/models/{}:generateContent", endpoint, model);

            let body = serde_json::json!({
                "contents": [{"parts": [{"text": req.input}]}],
                "generationConfig": {
                    "responseModalities": ["AUDIO"],
                    "speechConfig": {
                        "voiceConfig": {
                            "prebuiltVoiceConfig": {
                                "voiceName": map_voice_to_gemini(&req.voice)
                            }
                        }
                    }
                }
            });

            let resp = self
                .http
                .post(&url)
                .query(&[("key", &api_key)])
                .json(&body)
                .timeout(INFER_TIMEOUT)
                .send()
                .await
                .context("POST generateContent (TTS)")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Gemini TTS HTTP {status}: {text}");
            }

            let gemini: Value = resp.json().await.context("parse TTS response")?;

            // Extract audio from inlineData parts.
            let parts = gemini
                .get("candidates")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|c| c.get("content"))
                .and_then(|c| c.get("parts"))
                .and_then(|p| p.as_array());

            if let Some(parts) = parts {
                for part in parts {
                    if let Some(inline) = part.get("inlineData") {
                        let data_b64 = inline
                            .get("data")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        let audio_bytes = BASE64_STANDARD
                            .decode(data_b64)
                            .context("decode TTS audio base64")?;

                        // Gemini returns PCM audio (24 kHz, 16-bit mono).
                        // Wrap in WAV header for universal compatibility.
                        let wav = pcm_to_wav(&audio_bytes, 24000, 16, 1);

                        return Ok(SpeechResponse {
                            content_type: "audio/wav".to_string(),
                            audio: SpeechAudio::Complete(wav),
                        });
                    }
                }
            }

            anyhow::bail!("Gemini TTS response contained no audio data")
        })
    }

    // ── Transcription ──────────────────────────────────────────

    fn transcribe(
        &self,
        ctx: &ProviderContext,
        req: TranscribeRequest,
    ) -> BoxFuture<'_, Result<TranscribeResponse>> {
        let api_key = ctx.api_key.clone().unwrap_or_default();
        let model = ctx
            .model
            .as_deref()
            .unwrap_or("gemini-2.5-flash")
            .to_string();
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let url = format!("{}/v1beta/models/{}:generateContent", endpoint, model);

            let mime = mime_from_filename(&req.filename);
            let audio_b64 = BASE64_STANDARD.encode(&req.audio);

            let prompt = if let Some(ref lang) = req.language {
                format!("Transcribe this audio. Language: {lang}")
            } else {
                "Transcribe this audio accurately.".to_string()
            };

            let body = serde_json::json!({
                "contents": [{
                    "parts": [
                        {"text": prompt},
                        {"inlineData": {"mimeType": mime, "data": audio_b64}}
                    ]
                }]
            });

            let resp = self
                .http
                .post(&url)
                .query(&[("key", &api_key)])
                .json(&body)
                .timeout(INFER_TIMEOUT)
                .send()
                .await
                .context("POST generateContent (transcribe)")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Gemini transcribe HTTP {status}: {text}");
            }

            let gemini: Value = resp.json().await.context("parse transcribe response")?;

            let text = gemini
                .get("candidates")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|c| c.get("content"))
                .and_then(|c| c.get("parts"))
                .and_then(|p| p.as_array())
                .and_then(|parts| parts.first())
                .and_then(|p| p.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();

            Ok(TranscribeResponse { text })
        })
    }
}

// ── Endpoint-Specific Inference ───────────────────────────────

impl GoogleProvider {
    /// generateContent — standard chat, native image gen, TTS, Gemma, etc.
    fn infer_generate_content(
        &self,
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<InferenceResponse>> {
        let api_key = ctx.api_key.clone().unwrap_or_default();
        let model = ctx.model.as_deref().unwrap_or(&req.model);
        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            ctx.endpoint, model
        );
        let model = model.to_string();
        let body = build_gemini_request(&req);

        Box::pin(async move {
            let resp = self
                .http
                .post(&url)
                .query(&[("key", &api_key)])
                .header("content-type", "application/json")
                .json(&body)
                .timeout(INFER_TIMEOUT)
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

    /// `:predict` — Imagen image generation.
    fn infer_imagen(
        &self,
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<InferenceResponse>> {
        let api_key = ctx.api_key.clone().unwrap_or_default();
        let model = ctx
            .model
            .as_deref()
            .unwrap_or(&req.model)
            .to_string();
        let endpoint = ctx.endpoint.clone();

        // Extract the prompt from the last user message.
        let prompt = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .and_then(|m| m.content.as_ref())
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        Box::pin(async move {
            let url = format!("{}/v1beta/models/{}:predict", endpoint, model);

            let body = serde_json::json!({
                "instances": [{"prompt": prompt}],
                "parameters": {
                    "sampleCount": 1,
                    "aspectRatio": "1:1",
                }
            });

            let resp = self
                .http
                .post(&url)
                .query(&[("key", &api_key)])
                .json(&body)
                .timeout(Duration::from_secs(120))
                .send()
                .await
                .context("POST :predict (Imagen)")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Imagen predict HTTP {status}: {text}");
            }

            let data: Value = resp.json().await.context("parse Imagen response")?;

            // predictions[].bytesBase64Encoded + mimeType
            let predictions = data.get("predictions").and_then(|p| p.as_array());

            let mut content_parts = Vec::new();
            if let Some(preds) = predictions {
                for pred in preds {
                    let mime = pred
                        .get("mimeType")
                        .and_then(|v| v.as_str())
                        .unwrap_or("image/png");
                    let b64 = pred
                        .get("bytesBase64Encoded")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !b64.is_empty() {
                        content_parts.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": {"url": format!("data:{mime};base64,{b64}")}
                        }));
                    }
                }
            }

            let content = if content_parts.is_empty() {
                None
            } else {
                Some(Value::Array(content_parts))
            };

            Ok(InferenceResponse {
                id: format!("imagen-{}", chrono::Utc::now().timestamp_millis()),
                object: "chat.completion".to_string(),
                model,
                choices: vec![InferenceChoice {
                    index: 0,
                    message: ChatMessage {
                        role: "assistant".to_string(),
                        content,
                        tool_calls: None,
                        tool_call_id: None,
                        extra: serde_json::Map::new(),
                    },
                    finish_reason: Some("stop".to_string()),
                }],
                usage: Usage::default(),
            })
        })
    }

    /// `:predictLongRunning` — Veo video generation with polling.
    fn infer_veo(
        &self,
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<InferenceResponse>> {
        let api_key = ctx.api_key.clone().unwrap_or_default();
        let model = ctx
            .model
            .as_deref()
            .unwrap_or(&req.model)
            .to_string();
        let endpoint = ctx.endpoint.clone();

        let prompt = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .and_then(|m| m.content.as_ref())
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        Box::pin(async move {
            let url = format!(
                "{}/v1beta/models/{}:predictLongRunning",
                endpoint, model
            );

            let body = serde_json::json!({
                "instances": [{"prompt": prompt}],
                "parameters": {
                    "aspectRatio": "16:9",
                    "durationSeconds": "8",
                    "generateAudio": true,
                }
            });

            // Start the operation.
            let resp = self
                .http
                .post(&url)
                .query(&[("key", &api_key)])
                .json(&body)
                .timeout(Duration::from_secs(30))
                .send()
                .await
                .context("POST :predictLongRunning (Veo)")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Veo predictLongRunning HTTP {status}: {text}");
            }

            let op: Value = resp.json().await.context("parse Veo operation")?;
            let op_name = op
                .get("name")
                .and_then(|n| n.as_str())
                .ok_or_else(|| anyhow::anyhow!("Veo response missing operation name"))?
                .to_string();

            // Poll for completion (max 6 minutes).
            let poll_url = format!("{}/v1beta/{}", endpoint, op_name);
            let max_polls = 36; // 36 * 10s = 6 minutes

            for _ in 0..max_polls {
                tokio::time::sleep(Duration::from_secs(10)).await;

                let poll_resp = self
                    .http
                    .get(&poll_url)
                    .query(&[("key", &api_key)])
                    .timeout(Duration::from_secs(30))
                    .send()
                    .await
                    .context("poll Veo operation")?;

                if !poll_resp.status().is_success() {
                    continue; // retry on transient errors
                }

                let status: Value =
                    poll_resp.json().await.context("parse poll response")?;

                let done = status
                    .get("done")
                    .and_then(|d| d.as_bool())
                    .unwrap_or(false);
                if !done {
                    continue;
                }

                // Check for error.
                if let Some(err) = status.get("error") {
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Video generation failed");
                    anyhow::bail!("Veo generation failed: {msg}");
                }

                // Extract video URI.
                let video_uri = status
                    .get("response")
                    .and_then(|r| r.get("generateVideoResponse"))
                    .and_then(|g| g.get("generatedSamples"))
                    .and_then(|s| s.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|s| s.get("video"))
                    .and_then(|v| v.get("uri"))
                    .and_then(|u| u.as_str());

                if let Some(uri) = video_uri {
                    // Download the video.
                    let video_resp = self
                        .http
                        .get(uri)
                        .query(&[("key", &api_key)])
                        .send()
                        .await
                        .context("download Veo video")?;

                    if video_resp.status().is_success() {
                        let video_bytes =
                            video_resp.bytes().await.context("read video bytes")?;
                        let b64 = BASE64_STANDARD.encode(&video_bytes);

                        return Ok(InferenceResponse {
                            id: format!(
                                "veo-{}",
                                chrono::Utc::now().timestamp_millis()
                            ),
                            object: "chat.completion".to_string(),
                            model,
                            choices: vec![InferenceChoice {
                                index: 0,
                                message: ChatMessage {
                                    role: "assistant".to_string(),
                                    content: Some(serde_json::json!([{
                                        "type": "image_url",
                                        "image_url": {
                                            "url": format!("data:video/mp4;base64,{b64}")
                                        }
                                    }])),
                                    tool_calls: None,
                                    tool_call_id: None,
                                    extra: serde_json::Map::new(),
                                },
                                finish_reason: Some("stop".to_string()),
                            }],
                            usage: Usage::default(),
                        });
                    }
                }

                anyhow::bail!(
                    "Veo generation completed but no video URI in response"
                );
            }

            anyhow::bail!("Veo generation timed out after 6 minutes")
        })
    }
}

// ── Helper Functions ───────────────────────────────────────────

/// Determine per-model capabilities from model name and supported generation methods.
///
/// Every model is tagged with what it CAN do — the adapter handles endpoint dispatch.
fn model_capabilities(name: &str, methods: &[String]) -> Vec<Capability> {
    // Embedding models
    if methods.iter().any(|m| m.contains("embed")) && name.contains("embedding") {
        return vec![Capability::Embed];
    }

    // Imagen image generation (uses :predict)
    if methods.iter().any(|m| m == "predict") && name.starts_with("imagen-") {
        return vec![Capability::Image];
    }

    // Veo video generation (uses :predictLongRunning)
    if methods.iter().any(|m| m == "predictLongRunning") && name.starts_with("veo-") {
        return vec![Capability::Video];
    }

    // Nano Banana / native image gen (uses generateContent with responseModalities)
    if methods.iter().any(|m| m == "generateContent")
        && (name.contains("-image") || name.contains("nano-banana"))
    {
        return vec![Capability::Image];
    }

    // TTS (uses generateContent with responseModalities: AUDIO)
    if methods.iter().any(|m| m == "generateContent") && name.contains("-tts") {
        return vec![Capability::Speech];
    }

    // Music (Lyria — uses generateContent)
    if methods.iter().any(|m| m == "generateContent") && name.starts_with("lyria-") {
        return vec![Capability::Music];
    }

    // Live/realtime audio (uses bidiGenerateContent — WebSocket)
    if methods.iter().any(|m| m == "bidiGenerateContent") {
        // Tag with capabilities even though we don't support WebSocket yet —
        // the adapter will return "not supported" at call time
        return vec![Capability::Speech, Capability::Transcribe];
    }

    // Gemma open models (text only)
    if name.starts_with("gemma-") && methods.iter().any(|m| m == "generateContent") {
        return vec![Capability::Chat];
    }

    // Skip non-inference models
    if name == "aqa"
        || name.contains("robotics")
        || name.contains("computer-use")
        || name.contains("deep-research")
    {
        return vec![];
    }

    // Standard chat models
    if methods.iter().any(|m| m == "generateContent") {
        let mut caps = vec![
            Capability::Chat,
            Capability::Vision,
            Capability::Tools,
            Capability::Transcribe,
            Capability::Translate,
        ];
        if name.contains("2.5") || name.contains("3.") || name.contains("3-") {
            caps.push(Capability::Think);
        }
        return caps;
    }

    vec![]
}

/// Returns `true` for Imagen models that use the `:predict` endpoint.
fn is_imagen_model(name: &str) -> bool {
    name.starts_with("imagen-")
}

/// Returns `true` for Veo models that use the `:predictLongRunning` endpoint.
fn is_veo_model(name: &str) -> bool {
    name.starts_with("veo-")
}

/// Returns `true` for live/realtime audio models that require WebSocket (bidiGenerateContent).
fn is_bidi_model(name: &str) -> bool {
    name.contains("-live-") || name.contains("native-audio")
}

/// Returns `true` for models that generate images via generateContent (responseModalities).
///
/// Imagen models are NOT included here — they use the `:predict` endpoint and are dispatched
/// separately by `is_imagen_model()`.
fn is_generate_content_image_model(model: &str) -> bool {
    model.contains("-image") || model.contains("nano-banana")
}

/// Map OpenAI voice names to Gemini prebuilt voice names.
fn map_voice_to_gemini(voice: &str) -> &str {
    match voice {
        "alloy" => "Aoede",
        "echo" => "Charon",
        "fable" => "Fenrir",
        "onyx" => "Orus",
        "nova" => "Kore",
        "shimmer" => "Leda",
        _ => voice, // pass through Gemini-native voice names
    }
}

/// Wrap raw PCM audio in a WAV header for universal playback.
fn pcm_to_wav(pcm: &[u8], sample_rate: u32, bits_per_sample: u16, channels: u16) -> Vec<u8> {
    let data_size = pcm.len() as u32;
    let byte_rate = sample_rate * (channels as u32) * (bits_per_sample as u32) / 8;
    let block_align = channels * bits_per_sample / 8;

    let mut wav = Vec::with_capacity(44 + pcm.len());
    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(pcm);
    wav
}

/// Determine MIME type from audio filename extension.
fn mime_from_filename(filename: &str) -> &str {
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext.to_lowercase().as_str() {
        "mp3" => "audio/mp3",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        "m4a" => "audio/mp4",
        "webm" => "audio/webm",
        _ => "audio/wav",
    }
}

// ── Request Translation (OpenAI -> Gemini) ─────────────────────

/// Build a Gemini `generateContent` request body from canonical `InferenceRequest`.
fn build_gemini_request(req: &InferenceRequest) -> Value {
    let mut body = serde_json::json!({});

    // 1. Extract system messages -> systemInstruction.
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

        // Map role: "assistant" -> "model", "user" stays.
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

    // Native image generation models need responseModalities to produce images.
    if is_generate_content_image_model(&req.model) {
        gen_config.insert(
            "responseModalities".into(),
            serde_json::json!(["TEXT", "IMAGE"]),
        );
    }

    if !gen_config.is_empty() {
        body["generationConfig"] = Value::Object(gen_config);
    }

    // 3. Tool definitions.
    if let Some(ref tools) = req.tools {
        let declarations: Vec<Value> = tools
            .iter()
            .filter_map(translate_tool_to_gemini)
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
/// Handles text content, vision (image_url -> inline_data), and tool results.
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
                    Some(Value::String(s)) => serde_json::from_str::<Value>(s)
                        .unwrap_or(Value::Object(serde_json::Map::new())),
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

// ── Response Translation (Gemini -> Canonical) ─────────────────

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
fn extract_candidate(candidate: Option<&Value>) -> (Option<Value>, Option<Vec<Value>>, String) {
    let Some(candidate) = candidate else {
        return (None, None, "stop".to_string());
    };

    let parts = candidate
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array());

    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<Value> = Vec::new();
    let mut image_parts: Vec<Value> = Vec::new();

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
            if let Some(inline) = part.get("inlineData") {
                let mime = inline
                    .get("mimeType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("image/png");
                let data = inline
                    .get("data")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                image_parts.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:{mime};base64,{data}")
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

    // When images are present, return multimodal content (array of content parts).
    let content = if !image_parts.is_empty() {
        let mut parts = Vec::new();
        if !text_parts.is_empty() {
            parts.push(serde_json::json!({"type": "text", "text": text_parts.join("")}));
        }
        parts.extend(image_parts);
        Some(Value::Array(parts))
    } else if text_parts.is_empty() {
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

// ── SSE Stream Adapter ──────────────────────────────────────────

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

                let has_finish = candidate
                    .and_then(|c| c.get("finishReason"))
                    .is_some();

                let finish_reason = if has_finish {
                    Some(finish_reason_str)
                } else {
                    None
                };

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

// ── Tests ───────────────────────────────────────────────────────

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

        let sys = &body["systemInstruction"]["parts"][0]["text"];
        assert_eq!(sys, "You are helpful.");

        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 3);

        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[1]["role"], "model");
        assert_eq!(contents[2]["role"], "user");

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

    // ── model_capabilities tests ───────────────────────────────

    #[test]
    fn capabilities_embedding_model() {
        let caps = model_capabilities("text-embedding-004", &["embedContent".into()]);
        assert_eq!(caps, vec![Capability::Embed]);
    }

    #[test]
    fn capabilities_image_model() {
        let caps = model_capabilities("gemini-2.0-flash-image", &["generateContent".into()]);
        assert_eq!(caps, vec![Capability::Image]);
    }

    #[test]
    fn capabilities_nano_banana() {
        let caps = model_capabilities("nano-banana-001", &["generateContent".into()]);
        assert_eq!(caps, vec![Capability::Image]);
    }

    #[test]
    fn capabilities_tts_model() {
        let caps =
            model_capabilities("gemini-2.5-flash-preview-tts", &["generateContent".into()]);
        assert_eq!(caps, vec![Capability::Speech]);
    }

    #[test]
    fn capabilities_veo_model() {
        let caps = model_capabilities("veo-2.0-generate-001", &["predictLongRunning".into()]);
        assert_eq!(caps, vec![Capability::Video]);
    }

    #[test]
    fn capabilities_lyria_model() {
        let caps = model_capabilities("lyria-realtime-exp", &["generateContent".into()]);
        assert_eq!(caps, vec![Capability::Music]);
    }

    #[test]
    fn capabilities_live_audio_model() {
        let caps = model_capabilities(
            "gemini-2.0-flash-live-001",
            &["bidiGenerateContent".into()],
        );
        assert_eq!(caps, vec![Capability::Speech, Capability::Transcribe]);
    }

    #[test]
    fn capabilities_native_audio_model() {
        let caps = model_capabilities(
            "gemini-2.5-flash-native-audio",
            &["bidiGenerateContent".into()],
        );
        assert_eq!(caps, vec![Capability::Speech, Capability::Transcribe]);
    }

    #[test]
    fn capabilities_gemma_model() {
        let caps = model_capabilities("gemma-3-27b-it", &["generateContent".into()]);
        assert_eq!(caps, vec![Capability::Chat]);
    }

    #[test]
    fn capabilities_non_inference_model_filtered() {
        assert!(model_capabilities("aqa", &["generateAnswer".into()]).is_empty());
        assert!(
            model_capabilities("gemini-robotics-er", &["generateContent".into()]).is_empty()
        );
    }

    #[test]
    fn capabilities_standard_chat_model_2_0() {
        let caps = model_capabilities("gemini-2.0-flash", &["generateContent".into()]);
        assert!(caps.contains(&Capability::Chat));
        assert!(caps.contains(&Capability::Vision));
        assert!(caps.contains(&Capability::Tools));
        assert!(caps.contains(&Capability::Transcribe));
        assert!(caps.contains(&Capability::Translate));
        // 2.0 does NOT have Think
        assert!(!caps.contains(&Capability::Think));
    }

    #[test]
    fn capabilities_standard_chat_model_2_5() {
        let caps = model_capabilities("gemini-2.5-flash", &["generateContent".into()]);
        assert!(caps.contains(&Capability::Chat));
        assert!(caps.contains(&Capability::Think));
    }

    #[test]
    fn capabilities_standard_chat_model_3_x() {
        let caps = model_capabilities("gemini-3-flash", &["generateContent".into()]);
        assert!(caps.contains(&Capability::Chat));
        assert!(caps.contains(&Capability::Think));
    }

    // ── model classification helper tests ─────────────────────

    #[test]
    fn generate_content_image_model_detects_correctly() {
        assert!(is_generate_content_image_model("gemini-2.0-flash-image"));
        assert!(is_generate_content_image_model("nano-banana-001"));
        // Imagen uses :predict, not generateContent
        assert!(!is_generate_content_image_model("imagen-3.0-generate-001"));
        assert!(!is_generate_content_image_model("gemini-2.5-flash"));
        assert!(!is_generate_content_image_model("gemma-3-27b-it"));
    }

    #[test]
    fn is_imagen_model_detects_correctly() {
        assert!(is_imagen_model("imagen-4.0-generate-001"));
        assert!(is_imagen_model("imagen-3.0-generate-001"));
        assert!(!is_imagen_model("gemini-2.0-flash-image"));
        assert!(!is_imagen_model("nano-banana-001"));
    }

    #[test]
    fn is_veo_model_detects_correctly() {
        assert!(is_veo_model("veo-3.1-generate-preview"));
        assert!(is_veo_model("veo-2.0-generate-001"));
        assert!(!is_veo_model("gemini-2.5-flash"));
    }

    #[test]
    fn is_bidi_model_detects_correctly() {
        assert!(is_bidi_model("gemini-2.0-flash-live-001"));
        assert!(is_bidi_model("gemini-2.5-flash-native-audio"));
        assert!(!is_bidi_model("gemini-2.5-flash"));
        assert!(!is_bidi_model("gemma-3-27b-it"));
    }

    #[test]
    fn capabilities_imagen_model() {
        let caps = model_capabilities("imagen-4.0-generate-001", &["predict".into()]);
        assert_eq!(caps, vec![Capability::Image]);
    }

    // ── pcm_to_wav tests ───────────────────────────────────────

    #[test]
    fn pcm_to_wav_header_structure() {
        let pcm = vec![0u8; 100];
        let wav = pcm_to_wav(&pcm, 24000, 16, 1);

        // Total size: 44 byte header + 100 byte data
        assert_eq!(wav.len(), 144);
        // RIFF header
        assert_eq!(&wav[0..4], b"RIFF");
        // File size minus 8 bytes
        let file_size = u32::from_le_bytes(wav[4..8].try_into().unwrap());
        assert_eq!(file_size, 136); // 144 - 8
        // WAVE
        assert_eq!(&wav[8..12], b"WAVE");
        // fmt chunk
        assert_eq!(&wav[12..16], b"fmt ");
        let fmt_size = u32::from_le_bytes(wav[16..20].try_into().unwrap());
        assert_eq!(fmt_size, 16);
        let format = u16::from_le_bytes(wav[20..22].try_into().unwrap());
        assert_eq!(format, 1); // PCM
        let channels = u16::from_le_bytes(wav[22..24].try_into().unwrap());
        assert_eq!(channels, 1);
        let sample_rate = u32::from_le_bytes(wav[24..28].try_into().unwrap());
        assert_eq!(sample_rate, 24000);
        // data chunk
        assert_eq!(&wav[36..40], b"data");
        let data_size = u32::from_le_bytes(wav[40..44].try_into().unwrap());
        assert_eq!(data_size, 100);
    }

    // ── extract_candidate with inlineData ──────────────────────

    #[test]
    fn response_translates_inline_data_image() {
        let resp = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "Here is your image"},
                        {"inlineData": {"mimeType": "image/png", "data": "iVBOR..."}}
                    ],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 5}
        });

        let canonical = gemini_response_to_canonical("gemini-2.0-flash-image", &resp);
        let content = canonical.choices[0].message.content.as_ref().unwrap();

        // Should be an array (multimodal content parts)
        let parts = content.as_array().unwrap();
        assert_eq!(parts.len(), 2);

        // First part: text
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[0]["text"], "Here is your image");

        // Second part: image
        assert_eq!(parts[1]["type"], "image_url");
        let url = parts[1]["image_url"]["url"].as_str().unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(url.contains("iVBOR..."));
    }

    #[test]
    fn response_image_only_no_text() {
        let resp = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"inlineData": {"mimeType": "image/jpeg", "data": "abc123"}}
                    ],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 3}
        });

        let canonical = gemini_response_to_canonical("gemini-2.0-flash-image", &resp);
        let content = canonical.choices[0].message.content.as_ref().unwrap();
        let parts = content.as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["type"], "image_url");
    }

    // ── voice mapping tests ────────────────────────────────────

    #[test]
    fn voice_mapping_openai_to_gemini() {
        assert_eq!(map_voice_to_gemini("alloy"), "Aoede");
        assert_eq!(map_voice_to_gemini("echo"), "Charon");
        assert_eq!(map_voice_to_gemini("nova"), "Kore");
        // Pass-through for Gemini-native names
        assert_eq!(map_voice_to_gemini("Puck"), "Puck");
    }

    // ── mime_from_filename tests ───────────────────────────────

    #[test]
    fn mime_detection_from_extension() {
        assert_eq!(mime_from_filename("audio.mp3"), "audio/mp3");
        assert_eq!(mime_from_filename("recording.wav"), "audio/wav");
        assert_eq!(mime_from_filename("podcast.ogg"), "audio/ogg");
        assert_eq!(mime_from_filename("music.flac"), "audio/flac");
        assert_eq!(mime_from_filename("voice.m4a"), "audio/mp4");
        assert_eq!(mime_from_filename("stream.webm"), "audio/webm");
        assert_eq!(mime_from_filename("unknown.xyz"), "audio/wav");
    }

    // ── image model request sets responseModalities ────────────

    #[test]
    fn request_sets_response_modalities_for_image_model() {
        let mut req = make_request(vec![user_msg("Generate a cat")]);
        req.model = "gemini-2.0-flash-image".into();
        let body = build_gemini_request(&req);

        let modalities = body["generationConfig"]["responseModalities"]
            .as_array()
            .unwrap();
        assert_eq!(modalities.len(), 2);
        assert_eq!(modalities[0], "TEXT");
        assert_eq!(modalities[1], "IMAGE");
    }

    #[test]
    fn request_no_response_modalities_for_text_model() {
        let req = make_request(vec![user_msg("Hello")]);
        let body = build_gemini_request(&req);

        // Should not have responseModalities for a standard text model
        assert!(body.get("generationConfig").is_none() || body["generationConfig"]
            .get("responseModalities")
            .is_none());
    }
}
