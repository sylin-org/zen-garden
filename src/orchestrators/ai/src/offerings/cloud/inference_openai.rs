//! OpenAI inference adapter.
//!
//! Near pass-through — our canonical types ARE OpenAI-shaped.
//! Key work: auth header, SSE stream parsing, multipart transcription.

use anyhow::{Context, Result};
use bytes::BytesMut;
use futures_util::stream::Stream;
use reqwest::Client;
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::BoxFuture;

/// OpenAI inference adapter.
pub struct OpenAiAdapter {
    http: Client,
}

impl OpenAiAdapter {
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

impl Default for OpenAiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl InferenceAdapter for OpenAiAdapter {
    fn infer(
        &self,
        ctx: &AdapterContext,
        req: InferenceRequest,
    ) -> BoxFuture<'_, Result<InferenceResponse>> {
        let url = format!("{}/v1/chat/completions", ctx.endpoint);
        let api_key = ctx.api_key.clone();

        Box::pin(async move {
            let mut builder = self
                .http
                .post(&url)
                .json(&req)
                .timeout(Duration::from_secs(300));

            if let Some(key) = &api_key {
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
        ctx: &AdapterContext,
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

            if let Some(key) = &api_key {
                builder = builder.bearer_auth(key);
            }

            let resp = builder.send().await.context("POST /v1/chat/completions stream")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("OpenAI /v1/chat/completions stream HTTP {status}: {text}");
            }

            let stream = resp.bytes_stream();
            Ok(Box::pin(OpenAiSseStream::new(stream)) as BoxStream<'static, Result<InferenceChunk>>)
        })
    }

    fn embed(
        &self,
        ctx: &AdapterContext,
        req: EmbedRequest,
    ) -> BoxFuture<'_, Result<EmbedResponse>> {
        let url = format!("{}/v1/embeddings", ctx.endpoint);
        let api_key = ctx.api_key.clone();

        Box::pin(async move {
            let mut builder = self
                .http
                .post(&url)
                .json(&req)
                .timeout(Duration::from_secs(60));

            if let Some(key) = &api_key {
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
        ctx: &AdapterContext,
        req: SpeechRequest,
    ) -> BoxFuture<'_, Result<SpeechResponse>> {
        let url = format!("{}/v1/audio/speech", ctx.endpoint);
        let api_key = ctx.api_key.clone();

        Box::pin(async move {
            let mut builder = self
                .http
                .post(&url)
                .json(&req)
                .timeout(Duration::from_secs(120));

            if let Some(key) = &api_key {
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
        ctx: &AdapterContext,
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
                .timeout(Duration::from_secs(120));

            if let Some(key) = &api_key {
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

use futures_util::StreamExt;

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

    fn poll_next(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Option<Self::Item>> {
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
                    // Loop back to check for complete segments.
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
    // Find the double-newline SSE delimiter.
    let delimiter_pos = find_double_newline(buffer)?;

    // Determine delimiter length before mutably borrowing buffer.
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
    // SSE segments may have multiple lines (event:, data:, etc.) — we want the data line.
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
