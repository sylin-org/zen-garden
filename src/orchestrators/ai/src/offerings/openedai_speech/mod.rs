//! OpenedAI Speech offering adapter — bounded context for all
//! OpenedAI Speech-specific logic.
//!
//! Nothing outside this module knows about OpenedAI Speech's API shapes
//! or TTS streaming format. The rest of the orchestrator sees only
//! `Offering`, `ServiceModel`, `ProbeResult`, etc.

pub mod client;
pub mod types;

use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::TryStreamExt;

use client::OpenedaiSpeechClient;

use crate::catalog::{
    BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest, ProxyResponse,
    ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind, ServiceInstance};

/// OpenedAI Speech offering adapter.
pub struct OpenedaiSpeechOffering {
    client: OpenedaiSpeechClient,
}

impl OpenedaiSpeechOffering {
    pub fn new() -> Self {
        Self {
            client: OpenedaiSpeechClient::new(),
        }
    }
}

impl Default for OpenedaiSpeechOffering {
    fn default() -> Self {
        Self::new()
    }
}

const OPENEDAI_SPEECH_CAPABILITIES: &[Capability] = &[Capability::Speak];

impl Offering for OpenedaiSpeechOffering {
    fn offering_type(&self) -> OfferingKind {
        OfferingKind::OpenedaiSpeech
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn capabilities(&self) -> &[Capability] {
        OPENEDAI_SPEECH_CAPABILITIES
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "openedai-speech".into(),
        }
    }

    fn probe(&self, endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>> {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            let health = self
                .client
                .health(&endpoint)
                .await
                .context("probe health check")?;

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

    fn enumerate(&self, endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            let models_resp = self
                .client
                .models(&endpoint)
                .await
                .context("enumerate models")?;

            let models = models_resp
                .data
                .into_iter()
                .map(|entry| ServiceModel {
                    name: entry.id.clone(),
                    capabilities: vec![Capability::Speak],
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

    fn vram_estimate(&self, _model: &ServiceModel) -> Option<u64> {
        None // Depends on engine: Piper=CPU, XTTS=~3GB — not deterministic
    }

    fn proxy(
        &self,
        endpoint: &str,
        _capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>> {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            let body_bytes = match request.body {
                ProxyBody::Complete(bytes) => Bytes::from(bytes),
                ProxyBody::Stream(_) => {
                    anyhow::bail!(
                        "streaming request bodies not supported for OpenedAI Speech proxy"
                    );
                }
            };

            let mut reqwest_headers = reqwest::header::HeaderMap::new();
            for (key, value) in request.headers.iter() {
                if let (Ok(name), Ok(val)) = (
                    reqwest::header::HeaderName::from_bytes(key.as_str().as_bytes()),
                    reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
                ) {
                    reqwest_headers.insert(name, val);
                }
            }

            let method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
                .unwrap_or(reqwest::Method::POST);

            let resp = self
                .client
                .forward_request(&endpoint, &request.path, method, body_bytes, reqwest_headers)
                .await
                .context("proxy forward to OpenedAI Speech")?;

            let status = resp.status().as_u16();

            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| {
                    v.to_str()
                        .ok()
                        .map(|val| (k.as_str().to_string(), val.to_string()))
                })
                .collect();

            // Audio speech responses are streaming — use ProxyBody::Stream
            // so the caller can pipe bytes directly to the downstream client.
            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            let is_audio = content_type.starts_with("audio/");

            let body = if is_audio {
                let stream = resp
                    .bytes_stream()
                    .map_err(|e| anyhow::anyhow!("stream error: {e}"));
                ProxyBody::Stream(Box::pin(stream))
            } else {
                let bytes = resp.bytes().await.context("read response body")?;
                ProxyBody::Complete(bytes.to_vec())
            };

            Ok(ProxyResponse {
                status,
                headers,
                body,
            })
        })
    }

    fn sync_resource(
        &self,
        _resource: &str,
        _from: &ServiceInstance,
        _to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<crate::catalog::SyncProgress>> {
        Box::pin(async {
            Ok(crate::catalog::SyncProgress::Failed {
                reason: "OpenedAI Speech voices are pre-installed, not dynamically synced"
                    .to_string(),
            })
        })
    }
}
