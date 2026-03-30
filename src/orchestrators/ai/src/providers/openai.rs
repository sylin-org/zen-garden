//! OpenAI provider — unified lifecycle + inference for OpenAI-compatible APIs.
//!
//! Near pass-through — our canonical types ARE OpenAI-shaped.
//! Works for OpenAI, Groq, Together, and any provider that implements
//! the OpenAI `/v1/models` and `/v1/chat/completions` API format.
//!
//! Auth: `Authorization: Bearer {api_key}` header on all requests.

use anyhow::{Context, Result};
use bytes::BytesMut;
use futures_util::stream::Stream;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Deserialize;
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

/// Timeout for audio calls (speech, transcription).
const AUDIO_TIMEOUT: Duration = Duration::from_secs(120);

const OPENAI_CAPABILITIES: &[Capability] = &[
    Capability::Chat,
    Capability::Embed,
    Capability::Vision,
    Capability::Tools,
    Capability::Think,
    Capability::Image,
    Capability::Speech,
    Capability::Transcribe,
];

/// OpenAI provider — stateless, receives all per-request state via `ProviderContext`.
pub struct OpenAiProvider {
    http: Client,
}

impl OpenAiProvider {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client build");
        Self { http }
    }
}

impl Default for OpenAiProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// OpenAI `/v1/models` response shape.
#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
    #[serde(default)]
    owned_by: Option<String>,
}

/// Require an API key from the context, or bail.
fn require_api_key(ctx: &ProviderContext) -> Result<String> {
    ctx.api_key
        .as_ref()
        .filter(|k| !k.is_empty())
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no API key configured for OpenAI provider"))
}

impl Provider for OpenAiProvider {
    fn kind(&self) -> OfferingKind {
        OfferingKind::OpenAi
    }

    fn capabilities(&self) -> &[Capability] {
        OPENAI_CAPABILITIES
    }

    fn discovery(&self) -> DiscoveryConfig {
        DiscoveryConfig::Configured
    }

    // ── Lifecycle ───────────────────────────────────────────────

    fn probe(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<ProbeResult>> {
        let url = format!("{}/v1/models", ctx.endpoint);
        let api_key_result = require_api_key(ctx);

        Box::pin(async move {
            let api_key = api_key_result?;

            let resp = self
                .http
                .get(&url)
                .bearer_auth(&api_key)
                .timeout(CLOUD_TIMEOUT)
                .send()
                .await
                .context("probe OpenAI /v1/models")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let summary = if body.len() > 256 {
                    format!("{}...", &body[..256])
                } else {
                    body
                };
                anyhow::bail!("OpenAI probe failed: HTTP {status}: {summary}");
            }

            Ok(ProbeResult {
                version: None,
                capabilities: OPENAI_CAPABILITIES.to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({
                    "provider": "openai",
                    "base_url": url,
                }),
            })
        })
    }

    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let url = format!("{}/v1/models", ctx.endpoint);
        let api_key = ctx
            .api_key
            .as_ref()
            .cloned()
            .unwrap_or_default();

        Box::pin(async move {
            if api_key.is_empty() {
                return Ok(Vec::new());
            }

            let resp = self
                .http
                .get(&url)
                .bearer_auth(&api_key)
                .timeout(CLOUD_TIMEOUT)
                .send()
                .await
                .context("enumerate OpenAI /v1/models")?;

            if !resp.status().is_success() {
                anyhow::bail!("enumerate failed: HTTP {}", resp.status());
            }

            let models_resp: ModelsResponse =
                resp.json().await.context("parse /v1/models response")?;

            let models = models_resp
                .data
                .into_iter()
                .map(|m| ServiceModel {
                    name: m.id.clone(),
                    capabilities: vec![Capability::Chat],
                    specializations: vec![],
                    vram_bytes: None,
                    metadata: serde_json::json!({
                        "owned_by": m.owned_by,
                        "cloud": true,
                    }),
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
        let url = format!("{}/v1/chat/completions", ctx.endpoint);
        let api_key = ctx.api_key.clone();

        Box::pin(async move {
            let mut builder = self
                .http
                .post(&url)
                .json(&req)
                .timeout(INFER_TIMEOUT);

            if let Some(ref key) = api_key {
                builder = builder.bearer_auth(key);
            }

            let resp = builder.send().await.context("POST /v1/chat/completions")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("OpenAI /v1/chat/completions HTTP {status}: {text}");
            }

            let response: InferenceResponse = resp
                .json()
                .await
                .context("parse OpenAI chat response")?;
            Ok(response)
        })
    }

    fn infer_stream(
        &self,
        ctx: &ProviderContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<BoxStream<'static, Result<InferenceChunk>>>> {
        let url = format!("{}/v1/chat/completions", ctx.endpoint);
        let api_key = ctx.api_key.clone();
        let http = self.http.clone();

        // Force stream: true in the request body.
        let mut body = serde_json::to_value(&req).unwrap_or_default();
        body["stream"] = serde_json::Value::Bool(true);

        Box::pin(async move {
            let mut builder = http.post(&url).json(&body);

            if let Some(ref key) = api_key {
                builder = builder.bearer_auth(key);
            }

            let resp = builder
                .send()
                .await
                .context("POST /v1/chat/completions stream")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("OpenAI /v1/chat/completions stream HTTP {status}: {text}");
            }

            let stream = resp.bytes_stream();
            Ok(
                Box::pin(OpenAiSseStream::new(stream))
                    as BoxStream<'static, Result<InferenceChunk>>,
            )
        })
    }

    fn embed(
        &self,
        ctx: &ProviderContext,
        req: EmbedRequest,
    ) -> BoxFuture<'_, Result<EmbedResponse>> {
        let url = format!("{}/v1/embeddings", ctx.endpoint);
        let api_key = ctx.api_key.clone();

        Box::pin(async move {
            let mut builder = self
                .http
                .post(&url)
                .json(&req)
                .timeout(EMBED_TIMEOUT);

            if let Some(ref key) = api_key {
                builder = builder.bearer_auth(key);
            }

            let resp = builder.send().await.context("POST /v1/embeddings")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("OpenAI /v1/embeddings HTTP {status}: {text}");
            }

            let response: EmbedResponse = resp
                .json()
                .await
                .context("parse OpenAI embed response")?;
            Ok(response)
        })
    }

    fn speak(
        &self,
        ctx: &ProviderContext,
        req: SpeechRequest,
    ) -> BoxFuture<'_, Result<SpeechResponse>> {
        let url = format!("{}/v1/audio/speech", ctx.endpoint);
        let api_key = ctx.api_key.clone();

        Box::pin(async move {
            let mut builder = self
                .http
                .post(&url)
                .json(&req)
                .timeout(AUDIO_TIMEOUT);

            if let Some(ref key) = api_key {
                builder = builder.bearer_auth(key);
            }

            let resp = builder.send().await.context("POST /v1/audio/speech")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("OpenAI /v1/audio/speech HTTP {status}: {text}");
            }

            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("audio/mpeg")
                .to_string();

            let stream = resp
                .bytes_stream()
                .map(|r| r.map_err(|e| anyhow::anyhow!("stream error: {e}")));

            Ok(SpeechResponse {
                content_type,
                audio: SpeechAudio::Stream(Box::pin(stream)),
            })
        })
    }

    fn transcribe(
        &self,
        ctx: &ProviderContext,
        req: TranscribeRequest,
    ) -> BoxFuture<'_, Result<TranscribeResponse>> {
        let url = format!("{}/v1/audio/transcriptions", ctx.endpoint);
        let api_key = ctx.api_key.clone();

        Box::pin(async move {
            let file_part = reqwest::multipart::Part::bytes(req.audio)
                .file_name(req.filename)
                .mime_str("application/octet-stream")
                .context("build multipart file part")?;

            let mut form = reqwest::multipart::Form::new()
                .part("file", file_part)
                .text("model", req.model);

            if let Some(lang) = req.language {
                form = form.text("language", lang);
            }
            if let Some(fmt) = req.response_format {
                form = form.text("response_format", fmt);
            }

            let mut builder = self
                .http
                .post(&url)
                .multipart(form)
                .timeout(AUDIO_TIMEOUT);

            if let Some(ref key) = api_key {
                builder = builder.bearer_auth(key);
            }

            let resp = builder
                .send()
                .await
                .context("POST /v1/audio/transcriptions")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("OpenAI /v1/audio/transcriptions HTTP {status}: {text}");
            }

            let response: TranscribeResponse = resp
                .json()
                .await
                .context("parse transcription response")?;
            Ok(response)
        })
    }
}

// ── SSE Stream Adapter ─────────────────────────────────────────

/// Adapter that converts an OpenAI SSE byte stream into `InferenceChunk`s.
///
/// OpenAI sends `data: {...}\n\n` lines, terminated by `data: [DONE]\n\n`.
/// TCP chunks may split across SSE boundaries — this adapter buffers until
/// a complete `\n\n`-delimited segment is available, then parses.
struct OpenAiSseStream<S> {
    inner: S,
    buffer: BytesMut,
    done: bool,
}

impl<S> OpenAiSseStream<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: BytesMut::with_capacity(4096),
            done: false,
        }
    }
}

impl<S> Stream for OpenAiSseStream<S>
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
            if let Some(chunk) = try_parse_sse_segment(&mut this.buffer) {
                match chunk {
                    SseSegment::Done => {
                        this.done = true;
                        return Poll::Ready(None);
                    }
                    SseSegment::Chunk(c) => {
                        return Poll::Ready(Some(Ok(c)));
                    }
                    SseSegment::Skip => continue,
                    SseSegment::Error(e) => {
                        return Poll::Ready(Some(Err(e)));
                    }
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

enum SseSegment {
    Chunk(InferenceChunk),
    Done,
    Skip,
    Error(anyhow::Error),
}

/// Try to extract a complete SSE segment from the buffer.
///
/// Returns `Some(...)` if a `\n\n`-delimited segment was found and consumed,
/// `None` if we need more data.
fn try_parse_sse_segment(buffer: &mut BytesMut) -> Option<SseSegment> {
    let delimiter_pos = find_double_newline(buffer)?;

    let delim_len = if buffer[delimiter_pos..].starts_with(b"\r\n\r\n") {
        4
    } else {
        2
    };

    let segment = buffer.split_to(delimiter_pos);
    // Consume the delimiter itself.
    let _ = buffer.split_to(delim_len);

    let segment_str = match std::str::from_utf8(&segment) {
        Ok(s) => s.trim(),
        Err(_) => return Some(SseSegment::Skip),
    };

    if segment_str.is_empty() {
        return Some(SseSegment::Skip);
    }

    // Extract the JSON payload after `data: ` prefix.
    let data = extract_data_field(segment_str)?;

    if data == "[DONE]" {
        return Some(SseSegment::Done);
    }

    match serde_json::from_str::<InferenceChunk>(data) {
        Ok(chunk) => Some(SseSegment::Chunk(chunk)),
        Err(e) => Some(SseSegment::Error(anyhow::anyhow!("parse SSE chunk: {e}"))),
    }
}

/// Find the position of the first `\n\n` or `\r\n\r\n` in the buffer.
fn find_double_newline(buf: &[u8]) -> Option<usize> {
    for i in 0..buf.len().saturating_sub(1) {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some(i);
        }
        if i + 3 < buf.len()
            && buf[i] == b'\r'
            && buf[i + 1] == b'\n'
            && buf[i + 2] == b'\r'
            && buf[i + 3] == b'\n'
        {
            return Some(i);
        }
    }
    None
}

/// Extract the value after `data: ` from an SSE segment.
fn extract_data_field(segment: &str) -> Option<&str> {
    for line in segment.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("data:") {
            return Some(rest.trim());
        }
    }
    None
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sse_segment_chunk() {
        let raw = concat!(
            "data: {\"id\":\"chatcmpl-abc\",\"object\":\"chat.completion.chunk\",",
            "\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":",
            "{\"role\":\"assistant\",\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
        );

        let mut buf = BytesMut::from(raw);
        let seg = try_parse_sse_segment(&mut buf);

        match seg {
            Some(SseSegment::Chunk(chunk)) => {
                assert_eq!(chunk.id, "chatcmpl-abc");
                assert_eq!(chunk.model, "gpt-4");
                assert_eq!(chunk.choices.len(), 1);
                let content = chunk.choices[0].delta.content.as_ref().unwrap();
                assert_eq!(content.as_str().unwrap(), "Hello");
            }
            other => panic!("expected Chunk, got {other:?}"),
        }
    }

    #[test]
    fn parse_sse_segment_done() {
        let raw = "data: [DONE]\n\n";
        let mut buf = BytesMut::from(raw);
        let seg = try_parse_sse_segment(&mut buf);

        assert!(matches!(seg, Some(SseSegment::Done)));
    }

    #[test]
    fn parse_sse_handles_incomplete_buffer() {
        let raw = "data: {\"id\":\"partial\"";
        let mut buf = BytesMut::from(raw);
        let seg = try_parse_sse_segment(&mut buf);

        // No double-newline yet — should return None.
        assert!(seg.is_none());
    }

    #[test]
    fn parse_sse_handles_crlf_delimiter() {
        let raw = "data: {\"id\":\"crlf\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hi\"},\"finish_reason\":null}]}\r\n\r\n";
        let mut buf = BytesMut::from(raw);
        let seg = try_parse_sse_segment(&mut buf);

        match seg {
            Some(SseSegment::Chunk(chunk)) => {
                assert_eq!(chunk.id, "crlf");
            }
            other => panic!("expected Chunk, got {other:?}"),
        }
    }

    impl std::fmt::Debug for SseSegment {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                SseSegment::Chunk(c) => write!(f, "Chunk({:?})", c.id),
                SseSegment::Done => write!(f, "Done"),
                SseSegment::Skip => write!(f, "Skip"),
                SseSegment::Error(e) => write!(f, "Error({e})"),
            }
        }
    }
}
