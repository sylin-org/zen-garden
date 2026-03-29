//! LibreTranslate offering adapter — bounded context for all
//! LibreTranslate-specific logic.
//!
//! Nothing outside this module knows about LibreTranslate's API shapes
//! or language-pair enumeration. The rest of the orchestrator sees only
//! `Offering`, `ServiceModel`, `ProbeResult`, etc.

pub mod client;
pub mod types;

use anyhow::{Context, Result};
use bytes::Bytes;

use client::LibreTranslateClient;

use crate::catalog::{
    BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest, ProxyResponse,
    ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind, ServiceInstance};

/// LibreTranslate offering adapter.
pub struct LibreTranslateOffering {
    client: LibreTranslateClient,
}

impl LibreTranslateOffering {
    pub fn new() -> Self {
        Self {
            client: LibreTranslateClient::new(),
        }
    }
}

impl Default for LibreTranslateOffering {
    fn default() -> Self {
        Self::new()
    }
}

const LIBRETRANSLATE_CAPABILITIES: &[Capability] = &[Capability::Translate];

impl Offering for LibreTranslateOffering {
    fn offering_type(&self) -> OfferingKind {
        OfferingKind::LibreTranslate
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn capabilities(&self) -> &[Capability] {
        LIBRETRANSLATE_CAPABILITIES
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "libretranslate".into(),
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
                capabilities: LIBRETRANSLATE_CAPABILITIES.to_vec(),
                vram_free_bytes: None,
                metadata: serde_json::json!({}),
            })
        })
    }

    fn enumerate(&self, endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = endpoint.to_string();
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

    fn vram_estimate(&self, _model: &ServiceModel) -> Option<u64> {
        None // CPU-only
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
                    anyhow::bail!("streaming request bodies not supported for LibreTranslate proxy");
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
                .context("proxy forward to LibreTranslate")?;

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

            let bytes = resp.bytes().await.context("read response body")?;
            let body = ProxyBody::Complete(bytes.to_vec());

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
                reason: "LibreTranslate manages its own model downloads".to_string(),
            })
        })
    }
}
