//! Infinity embed inference adapter.
//!
//! Very thin — Infinity speaks OpenAI-compatible embed format.
//! Only difference: path is `/embeddings` (no `/v1/` prefix — live-verified).

use anyhow::{Context, Result};
use reqwest::Client;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::BoxFuture;

/// Infinity embedding inference adapter.
pub struct InfinityAdapter {
    http: Client,
}

impl InfinityAdapter {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client");
        Self { http }
    }
}

impl Default for InfinityAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl InferenceAdapter for InfinityAdapter {
    fn embed(
        &self,
        ctx: &AdapterContext,
        req: EmbedRequest,
    ) -> BoxFuture<'_, Result<EmbedResponse>> {
        // Infinity uses `/embeddings` — no `/v1/` prefix.
        let url = format!("{}/embeddings", ctx.endpoint);

        Box::pin(async move {
            let resp = self
                .http
                .post(&url)
                .json(&req)
                .timeout(Duration::from_secs(60))
                .send()
                .await
                .context("POST /embeddings")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Infinity /embeddings HTTP {status}: {text}");
            }

            let response: EmbedResponse = resp
                .json()
                .await
                .context("parse Infinity embed response")?;
            Ok(response)
        })
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_builds_without_panic() {
        let adapter = InfinityAdapter::new();
        // Verify the adapter implements InferenceAdapter (compile-time check)
        // and that the HTTP client was created.
        let ctx = AdapterContext {
            endpoint: "http://localhost:7997".into(),
            model: "BAAI/bge-small-en-v1.5".into(),
            api_key: None,
        };

        // Calling infer (unsupported) should return an error future.
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

        // The default infer() returns a "not supported" error synchronously (no network).
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
