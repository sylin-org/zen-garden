//! OpenedAI Speech provider — unified lifecycle + inference for TTS.
//!
//! Thin adapter — speaks OpenAI-compatible TTS format at `/v1/audio/speech`.
//! Discovery: found via topology filter (local offering, not cloud).
//! No auth needed (local service).

use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::traits::{
    BoxFuture, DiscoveryConfig, ProbeResult, Provider, ProviderContext, ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind};

/// Timeout for discovery/profiling queries.
const PROFILE_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout for TTS generation.
const TTS_TIMEOUT: Duration = Duration::from_secs(120);

const OPENEDAI_SPEECH_CAPABILITIES: &[Capability] = &[Capability::Speech];

/// OpenedAI Speech provider — stateless, receives all per-request state via `ProviderContext`.
pub struct OpenedaiSpeechProvider {
    http: Client,
}

impl OpenedaiSpeechProvider {
    pub fn new() -> Self {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(2)
            .build()
            .expect("HTTP client build");
        Self { http }
    }
}

impl Default for OpenedaiSpeechProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from `GET /health`.
#[derive(Debug, Deserialize)]
struct HealthResponse {
    status: String,
}

/// Response from `GET /v1/models` (OpenAI-compatible format).
#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

/// A single model entry in the models list.
#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
    #[serde(default)]
    object: Option<String>,
    #[serde(default)]
    owned_by: Option<String>,
}

impl Provider for OpenedaiSpeechProvider {
    fn kind(&self) -> OfferingKind {
        OfferingKind::OpenedaiSpeech
    }

    fn capabilities(&self) -> &[Capability] {
        OPENEDAI_SPEECH_CAPABILITIES
    }

    fn discovery(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "openedai-speech".into(),
        }
    }

    // ── Lifecycle ───────────────────────────────────────────────

    fn probe(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<ProbeResult>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let url = format!("{endpoint}/health");
            let resp = self
                .http
                .get(&url)
                .timeout(PROFILE_TIMEOUT)
                .send()
                .await
                .context("GET /health")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("OpenedAI Speech probe failed: HTTP {status}: {body}");
            }

            let health: HealthResponse = resp.json().await.context("parse /health")?;

            if health.status != "ok" {
                anyhow::bail!(
                    "probe failed: {endpoint}/health returned status '{}'",
                    health.status
                );
            }

            Ok(ProbeResult {
                version: None,
                capabilities: OPENEDAI_SPEECH_CAPABILITIES.to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({}),
            })
        })
    }

    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = ctx.endpoint.clone();

        Box::pin(async move {
            let url = format!("{endpoint}/v1/models");
            let resp = self
                .http
                .get(&url)
                .timeout(PROFILE_TIMEOUT)
                .send()
                .await
                .context("GET /v1/models")?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                anyhow::bail!("OpenedAI Speech enumerate failed: HTTP {status}: {body}");
            }

            let models_resp: ModelsResponse =
                resp.json().await.context("parse /v1/models")?;

            let models = models_resp
                .data
                .into_iter()
                .map(|entry| ServiceModel {
                    name: entry.id.clone(),
                    capabilities: vec![Capability::Speech],
                    specializations: vec![],
                    vram_bytes: None,
                    metadata: serde_json::json!({
                        "object": entry.object,
                        "owned_by": entry.owned_by,
                    }),
                })
                .collect();

            Ok(models)
        })
    }

    // ── Inference ───────────────────────────────────────────────

    fn speak(
        &self,
        ctx: &ProviderContext,
        req: SpeechRequest,
    ) -> BoxFuture<'_, Result<SpeechResponse>> {
        let url = format!("{}/v1/audio/speech", ctx.endpoint);

        Box::pin(async move {
            let resp = self
                .http
                .post(&url)
                .json(&req)
                .timeout(TTS_TIMEOUT)
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

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::traits::Provider;

    #[test]
    fn provider_kind_and_capabilities() {
        let provider = OpenedaiSpeechProvider::new();
        assert_eq!(provider.kind(), OfferingKind::OpenedaiSpeech);
        assert_eq!(provider.capabilities(), &[Capability::Speech]);
    }

    #[test]
    fn discovery_filters_by_topology() {
        let provider = OpenedaiSpeechProvider::new();
        match provider.discovery() {
            DiscoveryConfig::TopologyFilter { offering_name } => {
                assert_eq!(offering_name, "openedai-speech");
            }
            _ => panic!("expected TopologyFilter"),
        }
    }

    #[test]
    fn unsupported_methods_return_error() {
        let provider = OpenedaiSpeechProvider::new();
        let ctx = ProviderContext {
            endpoint: "http://localhost:8001".into(),
            model: Some("tts-1".into()),
            api_key: None,
        };

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
            .block_on(provider.infer(&ctx, req));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not supported"));
    }
}
