//! Sub-capability discovery - runtime feature discovery for services
//!
//! Discovers sub-capabilities (models, collections, plugins) from running services.
//! Uses service-specific discovery methods (HTTP endpoints, exec commands, etc.)
//!
//! # Supported Services
//! - ollama: /api/tags → models
//! - chromadb: /api/v1/collections → collections
//! - More services can be added via the `ServiceDiscoverer` trait
//!
//! # Example
//! ```ignore
//! let caps = discover_sub_capabilities(&service_info, &docker).await?;
//! // caps = [SubCapability { type: "model", items: ["llama2", "mistral"] }]
//! ```

use anyhow::{Context, Result};
use garden_common::SubCapability;

use crate::docker::DockerManager;
use garden_common::ServiceInfo;

/// Discover sub-capabilities for a service
///
/// Routes to service-specific discovery based on offering name.
/// Returns empty vec if no discoverable sub-capabilities.
pub async fn discover_sub_capabilities(
    service: &ServiceInfo,
    docker: &DockerManager,
) -> Result<Vec<SubCapability>> {
    match service.offering.to_lowercase().as_str() {
        "ollama" => discover_ollama_models(service, docker).await,
        "chromadb" => discover_chromadb_collections(service).await,
        "milvus" => discover_milvus_collections(service).await,
        _ => Ok(Vec::new()),
    }
}

/// Refresh sub-capabilities for all services in registry
pub async fn refresh_all_sub_capabilities(
    services: &mut [ServiceInfo],
    docker: &DockerManager,
) -> usize {
    let mut updated = 0;

    for service in services.iter_mut() {
        if service.status != garden_common::ServiceStatus::Running {
            continue;
        }

        match discover_sub_capabilities(service, docker).await {
            Ok(caps) if !caps.is_empty() => {
                tracing::debug!(
                    service = %service.name,
                    capabilities = ?caps.iter().map(|c| format!("{}:{}", c.cap_type, c.items.len())).collect::<Vec<_>>(),
                    "Discovered sub-capabilities"
                );
                service.sub_capabilities = caps;
                updated += 1;
            }
            Ok(_) => {
                // No sub-capabilities discovered, clear any stale ones
                if !service.sub_capabilities.is_empty() {
                    service.sub_capabilities.clear();
                }
            }
            Err(e) => {
                tracing::warn!(
                    service = %service.name,
                    error = ?e,
                    "Failed to discover sub-capabilities"
                );
            }
        }
    }

    if updated > 0 {
        tracing::info!(count = updated, "Refreshed sub-capabilities for services");
    }

    updated
}

// ============================================================================
// Service-Specific Discovery Implementations
// ============================================================================

/// Discover Ollama models via /api/tags endpoint
async fn discover_ollama_models(
    service: &ServiceInfo,
    _docker: &DockerManager,
) -> Result<Vec<SubCapability>> {
    let port = service.ports.native;
    let url = format!("http://localhost:{}/api/tags", port);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to Ollama")?;

    if !response.status().is_success() {
        anyhow::bail!("Ollama returned error: {}", response.status());
    }

    #[derive(serde::Deserialize)]
    struct OllamaModel {
        name: String,
    }

    #[derive(serde::Deserialize)]
    struct OllamaResponse {
        models: Vec<OllamaModel>,
    }

    let data: OllamaResponse = response
        .json()
        .await
        .context("Failed to parse Ollama response")?;

    let models: Vec<String> = data.models.into_iter().map(|m| m.name).collect();

    if models.is_empty() {
        return Ok(Vec::new());
    }

    Ok(vec![SubCapability::new("model", models)])
}

/// Discover ChromaDB collections via /api/v1/collections endpoint
async fn discover_chromadb_collections(service: &ServiceInfo) -> Result<Vec<SubCapability>> {
    let port = service.ports.native;
    let url = format!("http://localhost:{}/api/v1/collections", port);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to ChromaDB")?;

    if !response.status().is_success() {
        anyhow::bail!("ChromaDB returned error: {}", response.status());
    }

    #[derive(serde::Deserialize)]
    struct Collection {
        name: String,
    }

    let collections: Vec<Collection> = response
        .json()
        .await
        .context("Failed to parse ChromaDB response")?;

    let names: Vec<String> = collections.into_iter().map(|c| c.name).collect();

    if names.is_empty() {
        return Ok(Vec::new());
    }

    Ok(vec![SubCapability::new("collection", names)])
}

/// Discover Milvus collections via REST API
async fn discover_milvus_collections(service: &ServiceInfo) -> Result<Vec<SubCapability>> {
    let port = service.ports.native;
    let url = format!("http://localhost:{}/v1/vector/collections", port);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to Milvus")?;

    if !response.status().is_success() {
        // Milvus might not have REST API enabled, that's OK
        return Ok(Vec::new());
    }

    #[derive(serde::Deserialize)]
    struct MilvusResponse {
        data: Vec<String>,
    }

    let data: MilvusResponse = response
        .json()
        .await
        .context("Failed to parse Milvus response")?;

    if data.data.is_empty() {
        return Ok(Vec::new());
    }

    Ok(vec![SubCapability::new("collection", data.data)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sub_capability_has() {
        let cap = SubCapability::new("model", vec!["llama2".to_string(), "mistral".to_string()]);
        assert!(cap.has("llama2"));
        assert!(cap.has("Llama2")); // case-insensitive
        assert!(!cap.has("gpt4"));
    }

    #[test]
    fn test_sub_capability_count() {
        let cap = SubCapability::new("model", vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert_eq!(cap.count(), 3);
    }
}
