//! Ollama offering adapter — bounded context for all Ollama-specific logic.
//!
//! Nothing outside this module knows about Ollama's API shapes, NDJSON
//! format, or model pull protocol. The rest of the orchestrator sees only
//! `Offering`, `ServiceModel`, `ProbeResult`, etc.

pub mod client;
pub mod types;

use anyhow::{Context, Result};
use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt};

use client::OllamaClient;

use crate::catalog::{
    BoxFuture, DiscoveryConfig, Offering, ProbeResult, ProxyBody, ProxyRequest, ProxyResponse,
    ServiceModel, SyncProgress,
};
use crate::domain::types::{Capability, OfferingKind, ServiceInstance};

/// Ollama offering adapter.
///
/// Implements the `Offering` trait, encapsulating all knowledge of
/// Ollama's HTTP API, NDJSON streaming, and model management protocol.
pub struct OllamaOffering {
    client: OllamaClient,
}

impl OllamaOffering {
    pub fn new() -> Self {
        Self {
            client: OllamaClient::new(),
        }
    }

    /// Expose the client for tasks that need direct access (profiling, sync).
    pub fn client(&self) -> &OllamaClient {
        &self.client
    }
}

impl Default for OllamaOffering {
    fn default() -> Self {
        Self::new()
    }
}

/// Capabilities that Ollama instances can provide.
///
/// Individual models may support a subset (e.g. embedding models only
/// support `Embed`). This is the union of all possible capabilities
/// an Ollama instance could serve.
const OLLAMA_CAPABILITIES: &[Capability] = &[
    Capability::Generate, // raw generation (/api/generate)
    Capability::Chat,     // conversational (/api/chat)
    Capability::Embed,
    Capability::Vision,
    Capability::Tools,
    Capability::Think,
];

impl Offering for OllamaOffering {
    fn offering_type(&self) -> OfferingKind {
        OfferingKind::Ollama
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn capabilities(&self) -> &[Capability] {
        OLLAMA_CAPABILITIES
    }

    fn discovery_config(&self) -> DiscoveryConfig {
        DiscoveryConfig::TopologyFilter {
            offering_name: "ollama".into(),
        }
    }

    fn probe(&self, endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>> {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            // Health check: GET / should return "Ollama is running"
            let health_url = format!("{endpoint}/");
            let resp = self
                .client
                .forward_request(
                    &endpoint,
                    "/",
                    reqwest::Method::GET,
                    Bytes::new(),
                    reqwest::header::HeaderMap::new(),
                )
                .await
                .context("probe health check")?;

            if !resp.status().is_success() {
                anyhow::bail!(
                    "probe failed: {} returned HTTP {}",
                    health_url,
                    resp.status()
                );
            }

            // Version query
            let version = self
                .client
                .get_version(&endpoint)
                .await
                .ok()
                .map(|v| v.version);

            Ok(ProbeResult {
                version,
                capabilities: OLLAMA_CAPABILITIES.to_vec(),
                vram_free_bytes: None, // Ollama does not report free VRAM
                metadata: serde_json::json!({}),
            })
        })
    }

    fn enumerate(&self, endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>> {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            let (_, _, model_infos, _) = self
                .client
                .full_profile(&endpoint)
                .await
                .context("enumerate models")?;

            let models = model_infos
                .into_iter()
                .map(|info| {
                    let capabilities = ollama_capabilities_from_strings(&info.capabilities);
                    let specializations = infer_specializations(&info.name, &info.family);
                    ServiceModel {
                        name: info.name,
                        capabilities,
                        specializations,
                        vram_bytes: info.vram_bytes,
                        metadata: serde_json::json!({
                            "parameter_count": info.parameter_count,
                            "parameter_size": info.parameter_size,
                            "quantization_level": info.quantization_level,
                            "family": info.family,
                            "families": info.families,
                            "format": info.format,
                            "size_disk": info.size_disk,
                            "context_length": info.context_length,
                        }),
                    }
                })
                .collect();

            Ok(models)
        })
    }

    fn vram_estimate(&self, model: &ServiceModel) -> Option<u64> {
        // If we have authoritative VRAM from /api/ps, use it
        if let Some(vram) = model.vram_bytes {
            return Some(vram);
        }

        // Fallback: estimate from disk size (GGUF models are roughly
        // 1.1x disk size when loaded into VRAM due to KV cache overhead)
        let size_disk = model.metadata.get("size_disk")?.as_u64()?;
        if size_disk > 0 {
            Some((size_disk as f64 * 1.1) as u64)
        } else {
            None
        }
    }

    fn proxy(
        &self,
        endpoint: &str,
        _capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>> {
        let endpoint = endpoint.to_string();
        Box::pin(async move {
            // Extract body bytes from ProxyRequest
            let body_bytes = match request.body {
                ProxyBody::Complete(bytes) => Bytes::from(bytes),
                ProxyBody::Stream(_) => {
                    // For streaming request bodies, we'd need to collect them.
                    // Ollama inference requests are always complete JSON bodies.
                    anyhow::bail!("streaming request bodies not supported for Ollama proxy");
                }
            };

            // Convert axum HeaderMap to reqwest HeaderMap
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
                .context("proxy forward to Ollama")?;

            let status = resp.status().as_u16();

            // Collect response headers
            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .filter_map(|(k, v)| {
                    v.to_str().ok().map(|val| (k.as_str().to_string(), val.to_string()))
                })
                .collect();

            // Determine if response is streaming (Ollama uses NDJSON)
            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            let is_streaming = content_type.contains("application/x-ndjson")
                || content_type.contains("text/event-stream");

            let body = if is_streaming {
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
        resource: &str,
        _from: &ServiceInstance,
        to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<SyncProgress>> {
        let model = resource.to_string();
        let target_endpoint = to.endpoint.clone();
        Box::pin(async move {
            // Pull the model on the target instance
            let mut stream = self
                .client
                .pull_model(&target_endpoint, &model)
                .await
                .context("initiate model pull")?;

            let mut bytes_transferred: u64 = 0;

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.context("pull stream chunk")?;
                // Parse NDJSON progress lines
                if let Ok(progress) =
                    serde_json::from_slice::<types::OllamaPullProgress>(&chunk)
                    && let Some(completed) = progress.completed
                {
                    bytes_transferred = completed;
                }
            }

            Ok(SyncProgress::Completed { bytes_transferred })
        })
    }
}

/// Map Ollama capability strings (from `/api/show`) to domain `Capability` variants.
fn ollama_capabilities_from_strings(ollama_caps: &[String]) -> Vec<Capability> {
    let mut caps = Vec::new();

    for cap_str in ollama_caps {
        match cap_str.as_str() {
            "completion" | "chat" => {
                if !caps.contains(&Capability::Chat) {
                    caps.push(Capability::Chat);
                }
            }
            "embedding" | "embed" => {
                if !caps.contains(&Capability::Embed) {
                    caps.push(Capability::Embed);
                }
            }
            "vision" => caps.push(Capability::Vision),
            "tools" => caps.push(Capability::Tools),
            "thinking" | "think" => caps.push(Capability::Think),
            _ => {
                tracing::trace!(capability = %cap_str, "unknown Ollama capability, ignoring");
            }
        }
    }

    // If no capabilities were reported but the model exists, assume at least Chat
    if caps.is_empty() {
        caps.push(Capability::Chat);
    }

    caps
}

/// Infer specialization tags from model name and family.
///
/// These are finer-grained than capabilities. A model with `Capability::Vision`
/// might specialize in OCR, and a model with `Capability::Chat` might specialize
/// in reasoning or coding. The dashboard uses these for display/filtering.
fn infer_specializations(name: &str, family: &Option<String>) -> Vec<String> {
    let lower = name.to_lowercase();
    let family_lower = family.as_deref().unwrap_or("").to_lowercase();
    let mut tags = Vec::new();

    // OCR models
    if lower.contains("ocr") {
        tags.push("ocr".to_string());
    }

    // Embedding models
    if lower.contains("embed") || lower.contains("minilm") || lower.contains("bge") {
        tags.push("embedding".to_string());
    }

    // Reasoning / thinking models
    if lower.contains("deepseek-r1") || lower.contains("reasoning") {
        tags.push("reasoning".to_string());
    }

    // Coding models
    if lower.contains("code") || lower.contains("starcoder") || lower.contains("codellama") {
        tags.push("coding".to_string());
    }

    // Multilingual
    if lower.contains("aya") || lower.contains("translate") || lower.contains("multilingual") {
        tags.push("multilingual".to_string());
    }

    // Small/fast models
    if lower.contains("tiny") || lower.contains("mini") || lower.contains("small") {
        tags.push("compact".to_string());
    }

    // Vision-specific
    if lower.contains("vision") || lower.contains("vl") || family_lower.contains("clip") {
        tags.push("vision".to_string());
    }

    tags
}
