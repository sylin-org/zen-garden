//! Speaches provider — STT + TTS via OpenAI-compatible endpoints.
//!
//! Speaches (formerly faster-whisper-server) provides:
//! - STT via `/v1/audio/transcriptions` (OpenAI-compatible)
//! - TTS via `/v1/audio/speech` (OpenAI-compatible)
//! - Model listing via `/v1/models`
//! - Health check via `/health`
//!
//! Both STT and TTS are essentially pass-through since Speaches
//! speaks native OpenAI format. The provider tags models with
//! Transcribe or Speech based on the model's task.

use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::traits::{
    BoxFuture, DiscoveryConfig, ProbeResult, Provider, ProviderContext, ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind};

const CLOUD_TIMEOUT: Duration = Duration::from_secs(15);
const INFER_TIMEOUT: Duration = Duration::from_secs(300);

const SPEACHES_CAPABILITIES: &[Capability] = &[Capability::Transcribe, Capability::Speech];

pub struct SpeachesProvider {
    http: Client,
}

impl SpeachesProvider {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client");
        Self { http }
    }
}

impl Provider for SpeachesProvider {
    fn kind(&self) -> OfferingKind {
        OfferingKind::Speaches
    }

    fn capabilities(&self) -> &[Capability] {
        SPEACHES_CAPABILITIES
    }

    fn discovery(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "speaches".to_string(),
        }
    }

    fn probe(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<ProbeResult>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let resp = self
                .http
                .get(format!("{endpoint}/health"))
                .timeout(CLOUD_TIMEOUT)
                .send()
                .await
                .context("probe Speaches /health")?;

            if !resp.status().is_success() {
                anyhow::bail!("Speaches health check failed: HTTP {}", resp.status());
            }

            Ok(ProbeResult {
                version: None,
                capabilities: SPEACHES_CAPABILITIES.to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({"provider": "speaches"}),
            })
        })
    }

    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let resp = self
                .http
                .get(format!("{endpoint}/v1/models"))
                .timeout(CLOUD_TIMEOUT)
                .send()
                .await
                .context("enumerate Speaches /v1/models")?;

            if !resp.status().is_success() {
                anyhow::bail!("Speaches enumerate failed: HTTP {}", resp.status());
            }

            let body: serde_json::Value =
                resp.json().await.context("parse /v1/models response")?;

            let models = body
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            let id = m.get("id").and_then(|v| v.as_str())?;
                            let name = id.to_string();

                            // Determine capability from model name patterns
                            let capabilities = classify_model_capabilities(&name);
                            if capabilities.is_empty() {
                                return None;
                            }

                            Some(ServiceModel {
                                name,
                                capabilities,
                                specializations: vec![],
                                vram_bytes: None,
                                metadata: serde_json::json!({"provider": "speaches"}),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            Ok(models)
        })
    }

    // ── Inference ───────────────────────────────────────────────

    fn speak(
        &self,
        ctx: &ProviderContext,
        req: SpeechRequest,
    ) -> BoxFuture<'_, Result<SpeechResponse>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let resp = self
                .http
                .post(format!("{endpoint}/v1/audio/speech"))
                .json(&req)
                .timeout(INFER_TIMEOUT)
                .send()
                .await
                .context("POST Speaches /v1/audio/speech")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Speaches TTS HTTP {status}: {text}");
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
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let mime = mime_from_filename(&req.filename).to_string();
            let filename = req.filename;

            let mut form = reqwest::multipart::Form::new();
            let file_part = reqwest::multipart::Part::bytes(req.audio)
                .file_name(filename)
                .mime_str(&mime)
                .context("build multipart file")?;
            form = form.part("file", file_part);
            form = form.text("model", req.model);

            if let Some(lang) = req.language {
                form = form.text("language", lang);
            }
            if let Some(fmt) = req.response_format {
                form = form.text("response_format", fmt);
            }

            let resp = self
                .http
                .post(format!("{endpoint}/v1/audio/transcriptions"))
                .multipart(form)
                .timeout(INFER_TIMEOUT)
                .send()
                .await
                .context("POST Speaches /v1/audio/transcriptions")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Speaches STT HTTP {status}: {text}");
            }

            let body: serde_json::Value =
                resp.json().await.context("parse transcription response")?;

            let text = body
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();

            Ok(TranscribeResponse { text })
        })
    }
}

// ── Helpers ─────────────────────────────────────────────────────

/// Classify a Speaches model's capabilities based on its name/ID.
///
/// STT models (Whisper variants) get `Transcribe`.
/// TTS models (Kokoro, Piper) get `Speech`.
fn classify_model_capabilities(model: &str) -> Vec<Capability> {
    let lower = model.to_lowercase();

    // TTS models
    if lower.contains("kokoro") || lower.contains("piper") || lower.contains("tts") {
        return vec![Capability::Speech];
    }

    // STT models (Whisper variants)
    if lower.contains("whisper")
        || lower.contains("distil-whisper")
        || lower.contains("faster-whisper")
    {
        return vec![Capability::Transcribe];
    }

    // Default: assume STT (most Speaches models are Whisper variants)
    vec![Capability::Transcribe]
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

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_and_capabilities() {
        let p = SpeachesProvider::new();
        assert_eq!(p.kind(), OfferingKind::Speaches);
        assert!(p.capabilities().contains(&Capability::Transcribe));
        assert!(p.capabilities().contains(&Capability::Speech));
    }

    #[test]
    fn classify_whisper_model() {
        let caps = classify_model_capabilities("Systran/faster-distil-whisper-small.en");
        assert_eq!(caps, vec![Capability::Transcribe]);
    }

    #[test]
    fn classify_kokoro_model() {
        let caps = classify_model_capabilities("speaches-ai/Kokoro-82M-v1.0-ONNX");
        assert_eq!(caps, vec![Capability::Speech]);
    }

    #[test]
    fn classify_unknown_defaults_to_stt() {
        let caps = classify_model_capabilities("some-unknown-model");
        assert_eq!(caps, vec![Capability::Transcribe]);
    }

    #[test]
    fn discovery_returns_topology_filter() {
        let p = SpeachesProvider::new();
        match p.discovery() {
            DiscoveryConfig::TopologyFilter { offering_name } => {
                assert_eq!(offering_name, "speaches");
            }
            _ => panic!("expected TopologyFilter"),
        }
    }
}
