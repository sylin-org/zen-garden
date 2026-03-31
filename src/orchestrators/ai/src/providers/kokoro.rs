//! Kokoro provider — fast local text-to-speech via Kokoro-FastAPI.
//!
//! Kokoro-FastAPI API (OpenAI-compatible TTS):
//! - Health: GET /health → {"status": "healthy"}
//! - Models: GET /v1/models → {"data": [{"id": "kokoro"}, ...]}
//! - Voices: GET /v1/audio/voices → list of voice objects
//! - Speech: POST /v1/audio/speech → audio stream (OpenAI-compatible)
//! - Default port: 8880
//!
//! The provider is mostly pass-through for speech requests since
//! Kokoro-FastAPI implements the OpenAI TTS API contract.

use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::traits::{
    BoxFuture, DiscoveryConfig, FormSchema, ProbeResult, Provider, ProviderContext, ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const INFER_TIMEOUT: Duration = Duration::from_secs(120);

const KOKORO_CAPABILITIES: &[Capability] = &[Capability::Speech];

pub struct KokoroProvider {
    http: Client,
}

impl KokoroProvider {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(4)
            .build()
            .expect("HTTP client");
        Self { http }
    }
}

impl Provider for KokoroProvider {
    fn kind(&self) -> OfferingKind {
        OfferingKind::Kokoro
    }

    fn capabilities(&self) -> &[Capability] {
        KOKORO_CAPABILITIES
    }

    fn discovery(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "kokoro".to_string(),
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
                .context("probe kokoro /health")?;

            if !resp.status().is_success() {
                anyhow::bail!("kokoro health check failed: HTTP {}", resp.status());
            }

            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let status = body.get("status").and_then(|s| s.as_str()).unwrap_or("");
            if status != "healthy" {
                anyhow::bail!("kokoro health check: status={status}");
            }

            Ok(ProbeResult {
                version: None,
                capabilities: KOKORO_CAPABILITIES.to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({"provider": "kokoro"}),
            })
        })
    }

    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let resp = self
                .http
                .get(format!("{endpoint}/v1/models"))
                .timeout(PROBE_TIMEOUT)
                .send()
                .await
                .context("GET kokoro /v1/models")?;

            if !resp.status().is_success() {
                return Ok(vec![]);
            }

            let body: serde_json::Value =
                resp.json().await.context("parse kokoro models response")?;

            let models = body
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            let name = m.get("id").and_then(|id| id.as_str())?;
                            Some(ServiceModel {
                                name: name.to_string(),
                                capabilities: KOKORO_CAPABILITIES.to_vec(),
                                specializations: vec![],
                                vram_bytes: None,
                                metadata: serde_json::json!({
                                    "provider": "kokoro",
                                }),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            Ok(models)
        })
    }

    fn speak(
        &self,
        ctx: &ProviderContext,
        req: SpeechRequest,
    ) -> BoxFuture<'_, Result<SpeechResponse>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            // Kokoro-FastAPI is OpenAI-compatible — pass through directly
            let resp = self
                .http
                .post(format!("{endpoint}/v1/audio/speech"))
                .json(&req)
                .timeout(INFER_TIMEOUT)
                .send()
                .await
                .context("POST kokoro /v1/audio/speech")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("kokoro TTS HTTP {status}: {text}");
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

    // ── Form Schema (ORCH-0017) ──────────────────────────────────

    fn form_schema(&self, _model: &str, capability: Capability) -> FormSchema {
        match capability {
            Capability::Speech => FormSchema {
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "input": {
                            "type": "string",
                            "title": "Text",
                            "minLength": 1
                        },
                        "voice": {
                            "type": "string",
                            "title": "Voice",
                            "enum": [
                                "af_heart", "af_alloy", "af_aoede", "af_bella", "af_jessica",
                                "af_kore", "af_nicole", "af_nova", "af_river", "af_sarah", "af_sky",
                                "am_adam", "am_echo", "am_eric", "am_fenrir", "am_liam",
                                "am_michael", "am_onyx", "am_puck",
                                "bf_alice", "bf_emma", "bf_lily",
                                "bm_daniel", "bm_fable", "bm_george", "bm_lewis"
                            ],
                            "default": "af_heart"
                        },
                        "speed": {
                            "type": "number",
                            "title": "Speed",
                            "minimum": 0.25,
                            "maximum": 4.0,
                            "default": 1.0
                        },
                        "response_format": {
                            "type": "string",
                            "title": "Format",
                            "enum": ["mp3", "wav", "opus", "flac"],
                            "default": "mp3"
                        }
                    },
                    "required": ["input"]
                }),
                ui_schema: serde_json::json!({
                    "input": {
                        "ui:widget": "textarea",
                        "ui:options": { "rows": 3 }
                    },
                    "speed": {
                        "ui:widget": "range"
                    }
                }),
            },
            _ => FormSchema::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_and_capabilities() {
        let p = KokoroProvider::new();
        assert_eq!(p.kind(), OfferingKind::Kokoro);
        assert_eq!(p.capabilities(), &[Capability::Speech]);
    }

    #[test]
    fn discovery_returns_topology_filter() {
        let p = KokoroProvider::new();
        match p.discovery() {
            DiscoveryConfig::TopologyFilter { offering_name } => {
                assert_eq!(offering_name, "kokoro");
            }
            _ => panic!("expected TopologyFilter"),
        }
    }

    #[test]
    fn form_schema_speech_has_voice_enum() {
        let p = KokoroProvider::new();
        let schema = p.form_schema("kokoro", Capability::Speech);
        let voices = schema.schema["properties"]["voice"]["enum"]
            .as_array()
            .expect("voice enum should be an array");
        assert!(voices.len() > 20, "should have 20+ voice options");
        assert!(voices.contains(&serde_json::json!("af_heart")));
    }
}
