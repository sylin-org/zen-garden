//! LibreTranslate provider — lifecycle only (probe, enumerate).
//!
//! LibreTranslate serves language translation capabilities. No inference
//! methods are implemented -- all use the Provider trait defaults which
//! return "not supported". Translation is handled via the native proxy.

use anyhow::{Context, Result};

use crate::catalog::traits::{
    BoxFuture, DiscoveryConfig, ProbeResult, Provider, ProviderContext, ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind};
use crate::offerings::libretranslate::client::LibreTranslateClient;

// ── Provider ───────────────────────────────────────────────────

/// LibreTranslate provider.
///
/// Delegates protocol operations to `LibreTranslateClient` for lifecycle.
/// No inference methods -- translation flows through the native proxy.
pub struct LibreTranslateProvider {
    client: LibreTranslateClient,
}

impl LibreTranslateProvider {
    pub fn new() -> Self {
        Self {
            client: LibreTranslateClient::new(),
        }
    }
}

impl Default for LibreTranslateProvider {
    fn default() -> Self {
        Self::new()
    }
}

const LIBRETRANSLATE_CAPABILITIES: &[Capability] = &[Capability::Translate];

impl Provider for LibreTranslateProvider {
    fn kind(&self) -> OfferingKind {
        OfferingKind::LibreTranslate
    }

    fn capabilities(&self) -> &[Capability] {
        LIBRETRANSLATE_CAPABILITIES
    }

    fn discovery(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "libretranslate".into(),
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

            if health.status != "ok" {
                anyhow::bail!(
                    "probe failed: {endpoint}/health returned status '{}'",
                    health.status
                );
            }

            Ok(ProbeResult {
                version: None,
                capabilities: LIBRETRANSLATE_CAPABILITIES.to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({}),
            })
        })
    }

    fn enumerate(&self, ctx: &ProviderContext) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = ctx.endpoint.clone();
        Box::pin(async move {
            let languages = self
                .client
                .languages(&endpoint)
                .await
                .context("enumerate languages")?;

            // One ServiceModel per source language, listing its available targets.
            let models = languages
                .into_iter()
                .map(|lang| ServiceModel {
                    name: lang.code.clone(),
                    capabilities: vec![Capability::Translate],
                    specializations: vec![],
                    vram_bytes: None,
                    metadata: serde_json::json!({
                        "language_name": lang.name,
                        "targets": lang.targets,
                    }),
                })
                .collect();

            Ok(models)
        })
    }

    // No inference methods -- all use trait defaults ("not supported").
    // Translation flows through the native proxy path.
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_returns_correct_kind() {
        let provider = LibreTranslateProvider::new();
        assert_eq!(provider.kind(), OfferingKind::LibreTranslate);
    }

    #[test]
    fn provider_capabilities_include_translate() {
        let provider = LibreTranslateProvider::new();
        let caps = provider.capabilities();
        assert_eq!(caps, &[Capability::Translate]);
    }

    #[test]
    fn discovery_returns_topology_filter() {
        let provider = LibreTranslateProvider::new();
        match provider.discovery() {
            DiscoveryConfig::TopologyFilter { offering_name } => {
                assert_eq!(offering_name, "libretranslate");
            }
            _ => panic!("expected TopologyFilter"),
        }
    }

    #[test]
    fn unsupported_methods_return_errors() {
        let provider = LibreTranslateProvider::new();
        let ctx = ProviderContext {
            endpoint: "http://localhost:5000".into(),
            model: None,
            api_key: None,
        };

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        // infer
        let req = crate::catalog::inference::InferenceRequest {
            model: "en".into(),
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
        let result = rt.block_on(provider.infer(&ctx, req));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not supported"));

        // embed
        let embed_req = crate::catalog::inference::EmbedRequest {
            model: "en".into(),
            input: serde_json::json!("hello"),
            extra: serde_json::Map::new(),
        };
        let result = rt.block_on(provider.embed(&ctx, embed_req));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not supported"));

        // speak
        let speech_req = crate::catalog::inference::SpeechRequest {
            model: "en".into(),
            input: "hello".into(),
            voice: "default".into(),
            response_format: None,
            speed: None,
        };
        let result = rt.block_on(provider.speak(&ctx, speech_req));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not supported"));
    }

    #[test]
    fn vram_estimate_returns_none() {
        let provider = LibreTranslateProvider::new();
        let model = ServiceModel {
            name: "en".into(),
            capabilities: vec![Capability::Translate],
            specializations: vec![],
            vram_bytes: None,
            metadata: serde_json::json!({}),
        };
        assert_eq!(provider.vram_estimate(&model), None);
    }
}
