//! Infinity provider — unified lifecycle + inference for Infinity embedding instances.
//!
//! Infinity speaks OpenAI-compatible embed format with one difference:
//! the path is `/embeddings` (no `/v1/` prefix -- live-verified).
//!
//! Capabilities: Embed, Rerank.

use anyhow::{Context, Result};
use std::time::Duration;

use crate::catalog::inference::*;
use crate::catalog::traits::{
    BoxFuture, DiscoveryConfig, ProbeResult, Provider, ProviderContext, ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind};
use crate::offerings::infinity::client::InfinityClient;

// ── Provider ───────────────────────────────────────────────────

/// Infinity provider.
///
/// Delegates protocol operations to `InfinityClient` for lifecycle
/// and implements embed inference inline.
pub struct InfinityProvider {
    client: InfinityClient,
}

impl InfinityProvider {
    pub fn new() -> Self {
        Self {
            client: InfinityClient::new(),
        }
    }
}

impl Default for InfinityProvider {
    fn default() -> Self {
        Self::new()
    }
}

const INFINITY_CAPABILITIES: &[Capability] = &[Capability::Embed, Capability::Rerank];

impl Provider for InfinityProvider {
    fn kind(&self) -> OfferingKind {
        OfferingKind::Infinity
    }

    fn capabilities(&self) -> &[Capability] {
        INFINITY_CAPABILITIES
    }

    fn discovery(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "infinity".into(),
        }
    }

    // ── Lifecycle ───────────────────────────────────────────────

    fn probe(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<ProbeResult>> {
        let endpoint = ctx.endpoint.clone();
        Box::pin(async move {
            let health = self
                .client
                .health(&endpoint)
                .await
                .context("probe health check")?;

            Ok(ProbeResult {
                version: None,
                capabilities: INFINITY_CAPABILITIES.to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({
                    "server_start_unix": health.unix,
                }),
            })
        })
    }

    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = ctx.endpoint.clone();
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
                    capabilities: INFINITY_CAPABILITIES.to_vec(),
                    specializations: vec![],
                    vram_bytes: None,
                    metadata: serde_json::json!({
                        "object": entry.object,
                    }),
                })
                .collect();

            Ok(models)
        })
    }

    // ── Inference ───────────────────────────────────────────────

    fn embed(
        &self,
        ctx: &ProviderContext,
        req: EmbedRequest,
    ) -> BoxFuture<'_, Result<EmbedResponse>> {
        // Infinity uses `/embeddings` -- no `/v1/` prefix.
        let url = format!("{}/embeddings", ctx.endpoint);

        Box::pin(async move {
            let http = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .pool_max_idle_per_host(4)
                .build()
                .context("build embed HTTP client")?;

            let resp = http
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
    fn provider_returns_correct_kind() {
        let provider = InfinityProvider::new();
        assert_eq!(provider.kind(), OfferingKind::Infinity);
    }

    #[test]
    fn provider_capabilities_include_embed_and_rerank() {
        let provider = InfinityProvider::new();
        let caps = provider.capabilities();
        assert!(caps.contains(&Capability::Embed));
        assert!(caps.contains(&Capability::Rerank));
        assert_eq!(caps.len(), 2);
    }

    #[test]
    fn discovery_returns_topology_filter() {
        let provider = InfinityProvider::new();
        match provider.discovery() {
            DiscoveryConfig::TopologyFilter { offering_name } => {
                assert_eq!(offering_name, "infinity");
            }
            _ => panic!("expected TopologyFilter"),
        }
    }

    #[test]
    fn unsupported_infer_returns_error() {
        let provider = InfinityProvider::new();
        let ctx = ProviderContext {
            endpoint: "http://localhost:7997".into(),
            model: Some("BAAI/bge-small-en-v1.5".into()),
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

        // The default infer() returns a "not supported" error (no network call).
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(provider.infer(&ctx, req));

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not supported"));
    }
}
