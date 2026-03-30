//! OpenedAI Speech TTS inference adapter.
//!
//! Very thin — speaks OpenAI-compatible TTS format at `/v1/audio/speech`.
//! No auth needed (local service).

use anyhow::{Context, Result};
use reqwest::Client;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::BoxFuture;

/// OpenedAI Speech TTS inference adapter.
pub struct OpenedaiSpeechAdapter {
    http: Client,
}

impl OpenedaiSpeechAdapter {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client");
        Self { http }
    }
}

impl Default for OpenedaiSpeechAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl InferenceAdapter for OpenedaiSpeechAdapter {
    fn speak(
        &self,
        ctx: &AdapterContext,
        req: SpeechRequest,
    ) -> BoxFuture<'_, Result<SpeechResponse>> {
        let url = format!("{}/v1/audio/speech", ctx.endpoint);

        Box::pin(async move {
            let resp = self
                .http
                .post(&url)
                .json(&req)
                .timeout(Duration::from_secs(120))
                .send()
                .await
                .context("POST /v1/audio/speech")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("OpenedAI Speech /v1/audio/speech HTTP {status}: {text}");
            }

            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("audio/wav")
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
}

// Need StreamExt for .map() on bytes_stream
use futures_util::StreamExt;

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_builds_without_panic() {
        let adapter = OpenedaiSpeechAdapter::new();
        let ctx = AdapterContext {
            endpoint: "http://localhost:8000".into(),
            model: "tts-1".into(),
            api_key: None,
        };

        // Calling infer (unsupported) should return an error.
        let req = InferenceRequest {
            model: "test".into(),
            messages: vec![],
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            tools: None,
            tool_choice: None,
            stream: false,
            extra: serde_json::Map::new(),
        };

        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(adapter.infer(&ctx, req));

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not supported"),
        );
    }
}
