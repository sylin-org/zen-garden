//! Docker Registry API client for querying image versions
//!
//! Generic client for Docker Hub and compatible registries (Docker Registry HTTP API V2).
//! Supports version queries, digest resolution, and semantic version comparison.
//!
//! # Example
//! ```ignore
//! use crate::infra::registry_client::{query_image_tags, find_newer_version};
//!
//! let config = RegistryConfig::default();
//! let tags = query_image_tags("redis:7.2.3", &config).await?;
//! if let Some(newer) = find_newer_version("7.2.3", &tags) {
//!     println!("Update available: {}", newer);
//! }
//! ```

use anyhow::{Context, Result};
use serde::Deserialize;

/// Registry configuration
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    pub registry_url: String,
    pub timeout_seconds: u64,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            registry_url: "https://registry.hub.docker.com".to_string(),
            timeout_seconds: 30,
        }
    }
}

/// Image reference parser
///
/// Parses Docker image references into registry, repository, and tag components.
#[derive(Debug, Clone)]
pub struct ImageRef {
    pub registry: String,
    pub repository: String,
    pub tag: String,
}

impl ImageRef {
    /// Parse Docker image reference
    ///
    /// Supports formats:
    /// - `redis` → docker.io/library/redis:latest
    /// - `redis:7.2.3` → docker.io/library/redis:7.2.3
    /// - `bitnami/redis:7.2` → docker.io/bitnami/redis:7.2
    /// - `ghcr.io/owner/repo:v1` → ghcr.io/owner/repo:v1
    pub fn parse(image: &str) -> Result<Self> {
        // Default registry is Docker Hub
        let (registry, rest) =
            if image.contains('/') && image.split('/').next().unwrap().contains('.') {
                // Has explicit registry (contains dot)
                let parts: Vec<&str> = image.splitn(2, '/').collect();
                (parts[0].to_string(), parts[1])
            } else {
                ("docker.io".to_string(), image)
            };

        // Parse repository and tag
        let (repository, tag) = if let Some(idx) = rest.rfind(':') {
            (rest[..idx].to_string(), rest[idx + 1..].to_string())
        } else {
            (rest.to_string(), "latest".to_string())
        };

        // Normalize Docker Hub repository names (add "library/" prefix if missing)
        let repository = if registry == "docker.io" && !repository.contains('/') {
            format!("library/{}", repository)
        } else {
            repository
        };

        Ok(ImageRef {
            registry,
            repository,
            tag,
        })
    }
}

/// Registry API response for tags list
#[derive(Debug, Deserialize)]
struct TagsResponse {
    tags: Vec<String>,
}

/// Docker Hub API token response
#[derive(Debug, Deserialize)]
struct TokenResponse {
    token: String,
}

/// Query available tags for an image
///
/// Returns sorted list of version tags (semantic versioning where possible).
/// Filters out non-version tags like "latest", "alpine", etc.
pub async fn query_image_tags(image: &str, config: &RegistryConfig) -> Result<Vec<String>> {
    let image_ref = ImageRef::parse(image)?;

    let client = crate::http::client_builder()
        .timeout(std::time::Duration::from_secs(config.timeout_seconds))
        .build()
        .context("Failed to create HTTP client")?;

    if image_ref.registry == "docker.io" {
        query_docker_hub_tags(&client, &image_ref).await
    } else {
        query_registry_v2_tags(&client, &image_ref, config).await
    }
}

/// Query Docker Hub specifically (requires authentication)
async fn query_docker_hub_tags(
    client: &reqwest::Client,
    image_ref: &ImageRef,
) -> Result<Vec<String>> {
    // Step 1: Get authentication token from Docker Hub
    let token_url = format!(
        "https://auth.docker.io/token?service=registry.docker.io&scope=repository:{}:pull",
        image_ref.repository
    );

    let token_response = client
        .get(&token_url)
        .send()
        .await
        .context("Failed to request Docker Hub token")?
        .json::<TokenResponse>()
        .await
        .context("Failed to parse token response")?;

    // Step 2: Query tags using the token
    let tags_url = format!(
        "https://registry.hub.docker.com/v2/{}/tags/list",
        image_ref.repository
    );

    let response = client
        .get(&tags_url)
        .header("Authorization", format!("Bearer {}", token_response.token))
        .send()
        .await
        .context("Failed to query Docker Hub tags")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Docker Hub tags query failed: {} - {}",
            response.status(),
            response.text().await.unwrap_or_default()
        );
    }

    let tags_response = response
        .json::<TagsResponse>()
        .await
        .context("Failed to parse tags response")?;

    Ok(filter_and_sort_tags(tags_response.tags))
}

/// Query standard Docker Registry V2 API
async fn query_registry_v2_tags(
    client: &reqwest::Client,
    image_ref: &ImageRef,
    config: &RegistryConfig,
) -> Result<Vec<String>> {
    let tags_url = format!(
        "{}/v2/{}/tags/list",
        config.registry_url, image_ref.repository
    );

    let response = client
        .get(&tags_url)
        .send()
        .await
        .context("Failed to query registry tags")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Registry tags query failed: {} - {}",
            response.status(),
            response.text().await.unwrap_or_default()
        );
    }

    let tags_response = response
        .json::<TagsResponse>()
        .await
        .context("Failed to parse tags response")?;

    Ok(filter_and_sort_tags(tags_response.tags))
}

/// Filter non-version tags and sort by semantic versioning
fn filter_and_sort_tags(mut tags: Vec<String>) -> Vec<String> {
    // Filter: Keep only tags that look like versions (start with digit or 'v')
    tags.retain(|tag| {
        let first_char = tag.chars().next();
        matches!(first_char, Some('0'..='9') | Some('v'))
            && !tag.contains("alpine")
            && !tag.contains("slim")
            && !tag.contains("latest")
    });

    // Sort by semantic version (best effort)
    tags.sort_by(|a, b| {
        let a_ver = parse_version(a);
        let b_ver = parse_version(b);
        b_ver.cmp(&a_ver) // Reverse order (newest first)
    });

    tags
}

/// Parse version string into comparable tuple
pub fn parse_version(tag: &str) -> (u32, u32, u32) {
    let tag = tag.strip_prefix('v').unwrap_or(tag);
    let parts: Vec<&str> = tag.split(&['.', '-'][..]).collect();

    let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    (major, minor, patch)
}

/// Find latest version newer than current
///
/// Compares semantic versions and returns the newest available version
/// that is newer than the current version.
pub fn find_newer_version(current: &str, available: &[String]) -> Option<String> {
    let current_ver = parse_version(current);

    available
        .iter()
        .filter(|tag| parse_version(tag) > current_ver)
        .max_by_key(|tag| parse_version(tag))
        .cloned()
}

/// Get image digest (SHA256) for a specific tag from registry
///
/// Resolves what a tag like "latest" or "7.4.0" actually points to.
pub async fn get_image_digest(image: &str, config: &RegistryConfig) -> Result<String> {
    let image_ref = ImageRef::parse(image)?;

    let client = crate::http::client_builder()
        .timeout(std::time::Duration::from_secs(config.timeout_seconds))
        .build()
        .context("Failed to create HTTP client")?;

    if image_ref.registry == "docker.io" {
        get_docker_hub_digest(&client, &image_ref).await
    } else {
        get_registry_v2_digest(&client, &image_ref, config).await
    }
}

/// Get digest from Docker Hub
async fn get_docker_hub_digest(client: &reqwest::Client, image_ref: &ImageRef) -> Result<String> {
    // Get authentication token
    let token_url = format!(
        "https://auth.docker.io/token?service=registry.docker.io&scope=repository:{}:pull",
        image_ref.repository
    );

    let token_response = client
        .get(&token_url)
        .send()
        .await
        .context("Failed to request Docker Hub token")?
        .json::<TokenResponse>()
        .await
        .context("Failed to parse token response")?;

    // Query manifest to get digest
    let manifest_url = format!(
        "https://registry.hub.docker.com/v2/{}/manifests/{}",
        image_ref.repository, image_ref.tag
    );

    let response = client
        .get(&manifest_url)
        .header("Authorization", format!("Bearer {}", token_response.token))
        .header(
            "Accept",
            "application/vnd.docker.distribution.manifest.v2+json",
        )
        .send()
        .await
        .context("Failed to query Docker Hub manifest")?;

    // Extract digest from Docker-Content-Digest header
    response
        .headers()
        .get("Docker-Content-Digest")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .context("No digest in response")
}

/// Get digest from generic Docker Registry V2
async fn get_registry_v2_digest(
    client: &reqwest::Client,
    image_ref: &ImageRef,
    config: &RegistryConfig,
) -> Result<String> {
    let manifest_url = format!(
        "{}/v2/{}/manifests/{}",
        config.registry_url, image_ref.repository, image_ref.tag
    );

    let response = client
        .get(&manifest_url)
        .header(
            "Accept",
            "application/vnd.docker.distribution.manifest.v2+json",
        )
        .send()
        .await
        .context("Failed to query registry manifest")?;

    response
        .headers()
        .get("Docker-Content-Digest")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .context("No digest in response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_ref_parse_simple() {
        let img = ImageRef::parse("redis").unwrap();
        assert_eq!(img.registry, "docker.io");
        assert_eq!(img.repository, "library/redis");
        assert_eq!(img.tag, "latest");
    }

    #[test]
    fn test_image_ref_parse_with_tag() {
        let img = ImageRef::parse("redis:7.2.3").unwrap();
        assert_eq!(img.registry, "docker.io");
        assert_eq!(img.repository, "library/redis");
        assert_eq!(img.tag, "7.2.3");
    }

    #[test]
    fn test_image_ref_parse_with_org() {
        let img = ImageRef::parse("bitnami/redis:7.2").unwrap();
        assert_eq!(img.registry, "docker.io");
        assert_eq!(img.repository, "bitnami/redis");
        assert_eq!(img.tag, "7.2");
    }

    #[test]
    fn test_image_ref_parse_with_registry() {
        let img = ImageRef::parse("ghcr.io/owner/repo:v1.0.0").unwrap();
        assert_eq!(img.registry, "ghcr.io");
        assert_eq!(img.repository, "owner/repo");
        assert_eq!(img.tag, "v1.0.0");
    }

    #[test]
    fn test_version_parsing() {
        assert_eq!(parse_version("7.2.3"), (7, 2, 3));
        assert_eq!(parse_version("v1.0.0"), (1, 0, 0));
        assert_eq!(parse_version("10.5"), (10, 5, 0));
    }

    #[test]
    fn test_version_comparison() {
        let versions = vec![
            "7.2.1".to_string(),
            "7.2.3".to_string(),
            "7.2.2".to_string(),
            "7.3.0".to_string(),
        ];

        let newer = find_newer_version("7.2.2", &versions);
        assert_eq!(newer, Some("7.3.0".to_string()));
    }

    #[test]
    fn test_filter_tags() {
        let tags = vec![
            "latest".to_string(),
            "7.2.3".to_string(),
            "7.2.3-alpine".to_string(),
            "7.2.4".to_string(),
            "7.2".to_string(),
            "bookworm".to_string(),
        ];

        let filtered = filter_and_sort_tags(tags);
        assert_eq!(filtered, vec!["7.2.4", "7.2.3", "7.2"]);
    }

    #[test]
    fn test_no_newer_version() {
        let versions = vec!["7.2.1".to_string(), "7.2.2".to_string()];
        let newer = find_newer_version("7.2.3", &versions);
        assert_eq!(newer, None);
    }
}
