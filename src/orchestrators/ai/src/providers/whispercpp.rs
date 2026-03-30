//! whisper.cpp provider — lightweight C++ speech-to-text.
//!
//! whisper.cpp uses a custom API (NOT OpenAI-compatible):
//! - Health: GET /health → {"status": "ok"}
//! - Transcribe: POST /inference (multipart form: file, temperature, response_format)
//! - No model listing — single model loaded at startup
//!
//! The provider translates our canonical TranscribeRequest to whisper.cpp's
//! /inference multipart format, and parses the response back.

use anyhow::{Context, Result};
use reqwest::Client;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::traits::{
    BoxFuture, DiscoveryConfig, ProbeResult, Provider, ProviderContext, ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const INFER_TIMEOUT: Duration = Duration::from_secs(300);

const WHISPERCPP_CAPABILITIES: &[Capability] = &[Capability::Transcribe];

pub struct WhisperCppProvider {
    http: Client,
}

impl WhisperCppProvider {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client");
        Self { http }
    }
}

impl Provider for WhisperCppProvider {
    fn kind(&self) -> OfferingKind {
        OfferingKind::WhisperCpp
    }

    fn capabilities(&self) -> &[Capability] {
        WHISPERCPP_CAPABILITIES
    }

    fn discovery(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "whispercpp".to_string(),
        }
    }

    fn probe(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<ProbeResult>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let resp = self
                .http
                .get(format!("{endpoint}/health"))
                .timeout(PROBE_TIMEOUT)
                .send()
                .await
                .context("probe whisper.cpp /health")?;

            if !resp.status().is_success() {
                anyhow::bail!("whisper.cpp health check failed: HTTP {}", resp.status());
            }

            // Parse {"status": "ok"} — verify it's actually whisper.cpp
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let status = body.get("status").and_then(|s| s.as_str()).unwrap_or("");
            if status != "ok" {
                anyhow::bail!("whisper.cpp health check: status={status}");
            }

            Ok(ProbeResult {
                version: None,
                capabilities: WHISPERCPP_CAPABILITIES.to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({"provider": "whispercpp"}),
            })
        })
    }

    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = ctx.endpoint.clone();

        // whisper.cpp loads a single model at startup — no model listing API.
        // We return one model named after the service endpoint.
        // The actual model name (e.g., "ggml-base.en") isn't discoverable via API.
        Box::pin(async move {
            // Probe to confirm it's alive
            let resp = self
                .http
                .get(format!("{endpoint}/health"))
                .timeout(PROBE_TIMEOUT)
                .send()
                .await;

            if resp.is_err() || !resp.unwrap().status().is_success() {
                return Ok(vec![]);
            }

            Ok(vec![ServiceModel {
                name: "whisper.cpp".to_string(),
                capabilities: WHISPERCPP_CAPABILITIES.to_vec(),
                specializations: vec![],
                vram_bytes: None,
                metadata: serde_json::json!({
                    "provider": "whispercpp",
                    "note": "Single model loaded at startup — name not discoverable via API",
                }),
            }])
        })
    }

    fn transcribe(
        &self,
        ctx: &ProviderContext,
        req: TranscribeRequest,
    ) -> BoxFuture<'_, Result<TranscribeResponse>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let mime = mime_from_filename(&req.filename).to_string();
            let filename = req.filename;

            // Build multipart form for whisper.cpp /inference endpoint
            let mut form = reqwest::multipart::Form::new();

            let file_part = reqwest::multipart::Part::bytes(req.audio)
                .file_name(filename)
                .mime_str(&mime)
                .context("build multipart file")?;
            form = form.part("file", file_part);

            // whisper.cpp uses temperature as a form field
            form = form.text("temperature", "0");
            form = form.text("response_format", "json");

            if let Some(lang) = req.language {
                form = form.text("language", lang);
            }

            let resp = self
                .http
                .post(format!("{endpoint}/inference"))
                .multipart(form)
                .timeout(INFER_TIMEOUT)
                .send()
                .await
                .context("POST whisper.cpp /inference")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("whisper.cpp inference HTTP {status}: {text}");
            }

            // Response: {"text": "transcribed text\n"}
            let body: serde_json::Value =
                resp.json().await.context("parse whisper.cpp response")?;

            let text = body
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            Ok(TranscribeResponse { text })
        })
    }
}

fn mime_from_filename(filename: &str) -> &str {
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext.to_lowercase().as_str() {
        "mp3" => "audio/mp3",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "webm" => "audio/webm",
        _ => "audio/wav",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_and_capabilities() {
        let p = WhisperCppProvider::new();
        assert_eq!(p.kind(), OfferingKind::WhisperCpp);
        assert_eq!(p.capabilities(), &[Capability::Transcribe]);
    }

    #[test]
    fn discovery_returns_topology_filter() {
        let p = WhisperCppProvider::new();
        match p.discovery() {
            DiscoveryConfig::TopologyFilter { offering_name } => {
                assert_eq!(offering_name, "whispercpp");
            }
            _ => panic!("expected TopologyFilter"),
        }
    }

    #[test]
    fn mime_detection() {
        assert_eq!(mime_from_filename("audio.wav"), "audio/wav");
        assert_eq!(mime_from_filename("audio.mp3"), "audio/mp3");
        assert_eq!(mime_from_filename("audio.ogg"), "audio/ogg");
        assert_eq!(mime_from_filename("unknown"), "audio/wav");
    }
}
