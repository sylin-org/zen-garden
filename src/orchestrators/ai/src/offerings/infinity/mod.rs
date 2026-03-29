//! Infinity offering adapter — bounded context for all Infinity-specific logic.
//!
//! Nothing outside this module knows about Infinity's API shapes or model
//! enumeration format. The rest of the orchestrator sees only `Offering`,
//! `ServiceModel`, `ProbeResult`, etc.

pub mod client;
pub mod types;

use anyhow::{Context, Result};
use bytes::Bytes;

use client::InfinityClient;

use crate::catalog::{
    BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest, ProxyResponse,
    ServiceModel,
};
use crate::domain::types::{Capability, OfferingKind, ServiceInstance};

/// Infinity offering adapter.
pub struct InfinityOffering {
    client: InfinityClient,
}

impl InfinityOffering {
    pub fn new() -> Self {
        Self {
            client: InfinityClient::new(),
        }
    }
}

impl Default for InfinityOffering {
    fn default() -> Self {
        Self::new()
    }
}

const INFINITY_CAPABILITIES: &[Capability] = &[Capability::Embed, Capability::Rerank];

impl Offering for InfinityOffering {
    fn offering_type(&self) -> OfferingKind {
        OfferingKind::Infinity
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn capabilities(&self) -> &[Capability] {
        INFINITY_CAPABILITIES
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "infinity".into(),
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
                    capabilities: INFINITY_CAPABILITIES.to_vec(),
                    vram_bytes: None,
                    metadata: serde_json::json!({
                        "object": entry.object,
                    }),
                })
                .collect();

            Ok(models)
        })
    }

    fn vram_estimate(&self, _model: &ServiceModel) -> Option<u64> {
        None // Varies per model, not exposed by Infinity
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
                    anyhow::bail!("streaming request bodies not supported for Infinity proxy");
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
                .context("proxy forward to Infinity")?;

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
                reason: "Infinity models are specified at startup, not dynamically synced"
                    .to_string(),
            })
        })
    }
}
