//! CivitAI API client — fetch image metadata, download PNGs, resolve models.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

const CIVITAI_API_BASE: &str = "https://civitai.com/api/v1";

/// Parsed CivitAI image URL.
pub struct CivitaiImageRef {
    pub image_id: u64,
}

/// Parse a CivitAI image URL into its components.
///
/// Accepts: `https://civitai.com/images/125682754`
pub fn parse_civitai_url(url: &str) -> Option<CivitaiImageRef> {
    // Match /images/{id} pattern
    let url = url.trim();
    let re_patterns = [
        "civitai.com/images/",
        "civitai.com/api/v1/images?imageId=",
    ];

    for pattern in &re_patterns {
        if let Some(pos) = url.find(pattern) {
            let after = &url[pos + pattern.len()..];
            // Extract digits
            let id_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(id) = id_str.parse::<u64>() {
                return Some(CivitaiImageRef { image_id: id });
            }
        }
    }

    None
}

/// CivitAI image API response (minimal fields we need).
#[derive(Debug, Deserialize)]
pub struct CivitaiImageResponse {
    pub items: Vec<CivitaiImage>,
}

#[derive(Debug, Deserialize)]
pub struct CivitaiImage {
    pub id: u64,
    pub url: String,
    #[serde(default)]
    pub meta: Option<serde_json::Value>,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

/// Resource entry from CivitAI image metadata.
#[derive(Debug, Deserialize)]
pub struct CivitaiResource {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub resource_type: Option<String>,
    pub hash: Option<String>,
    pub weight: Option<f64>,
}

/// Fetch image metadata from CivitAI API.
pub async fn fetch_image_metadata(http: &Client, image_id: u64) -> Result<CivitaiImage> {
    let url = format!("{CIVITAI_API_BASE}/images?imageId={image_id}&limit=1");

    tracing::info!(image_id, "fetching CivitAI image metadata");

    let resp = http
        .get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("CivitAI API returned HTTP {}", resp.status());
    }

    let data: CivitaiImageResponse = resp.json().await.context("parse CivitAI response")?;

    data.items
        .into_iter()
        .next()
        .with_context(|| format!("CivitAI image {image_id} not found"))
}

/// Download the original PNG image from CivitAI.
/// Returns the raw bytes (streamed to memory — images are typically <20MB).
pub async fn download_image(http: &Client, image_url: &str) -> Result<Vec<u8>> {
    tracing::info!(url = %image_url, "downloading CivitAI image");

    let resp = http
        .get(image_url)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await
        .with_context(|| format!("GET {image_url}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("image download failed HTTP {}", resp.status());
    }

    let bytes = resp.bytes().await.context("read image bytes")?;
    Ok(bytes.to_vec())
}

/// Extract resource entries (model names + hashes) from CivitAI image metadata.
pub fn extract_resources(meta: &serde_json::Value) -> Vec<CivitaiResource> {
    meta.get("resources")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve a model by its hash via CivitAI API.
/// Returns (model_version_id, download_url, filename) if found.
pub async fn resolve_model_by_hash(
    http: &Client,
    hash: &str,
) -> Result<Option<ResolvedModel>> {
    let url = format!("{CIVITAI_API_BASE}/model-versions/by-hash/{hash}");

    let resp = http
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    if !resp.status().is_success() {
        return Ok(None);
    }

    let data: serde_json::Value = resp.json().await.unwrap_or_default();

    let model_version_id = data.get("id").and_then(|v| v.as_u64());
    let files = data.get("files").and_then(|v| v.as_array());

    if let (Some(version_id), Some(files)) = (model_version_id, files) {
        if let Some(primary) = files.first() {
            let filename = primary
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let download_url = format!(
                "https://civitai.com/api/download/models/{version_id}"
            );

            let sha256 = primary
                .get("hashes")
                .and_then(|h| h.get("SHA256"))
                .and_then(|v| v.as_str())
                .map(String::from);

            let size_bytes = primary
                .get("sizeKB")
                .and_then(|v| v.as_f64())
                .map(|kb| (kb * 1024.0) as u64);

            return Ok(Some(ResolvedModel {
                filename,
                download_url,
                sha256,
                size_bytes,
                source: "civitai".into(),
            }));
        }
    }

    Ok(None)
}

/// A model resolved from an external source.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub filename: String,
    pub download_url: String,
    pub sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_civitai_image_url() {
        let r = parse_civitai_url("https://civitai.com/images/125682754").unwrap();
        assert_eq!(r.image_id, 125682754);
    }

    #[test]
    fn parse_civitai_url_with_params() {
        let r = parse_civitai_url("https://civitai.com/images/125682754?modelVersionId=123").unwrap();
        assert_eq!(r.image_id, 125682754);
    }

    #[test]
    fn parse_non_civitai_returns_none() {
        assert!(parse_civitai_url("https://example.com/images/123").is_none());
        assert!(parse_civitai_url("not a url").is_none());
    }

    #[test]
    fn extract_resources_from_meta() {
        let meta = serde_json::json!({
            "prompt": "a cat",
            "resources": [
                { "name": "DreamShaper", "type": "model", "hash": "abc123" },
                { "name": "add_detail", "type": "lora", "weight": 0.8 }
            ]
        });
        let resources = extract_resources(&meta);
        assert_eq!(resources.len(), 2);
        assert_eq!(resources[0].hash.as_deref(), Some("abc123"));
    }
}
