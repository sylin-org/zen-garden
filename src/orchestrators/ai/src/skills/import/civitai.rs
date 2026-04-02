//! CivitAI API client — resilient, no panics, descriptive errors.

use anyhow::{Context, Result};
use reqwest::Client;
use std::time::Duration;

const API_BASE: &str = "https://civitai.com/api/v1";
const TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

// ── Image Metadata ────────────────────────────────────────────

/// CivitAI image metadata — everything we can extract from the API.
#[derive(Debug)]
pub struct ImageMetadata {
    pub image_id: u64,
    pub image_url: String,
    pub width: u32,
    pub height: u32,
    pub username: String,
    pub base_model: Option<String>,
    pub model_version_ids: Vec<u64>,
    pub generation: Option<GenerationMeta>,
}

/// Parsed generation parameters from CivitAI's meta.meta object.
#[derive(Debug)]
pub struct GenerationMeta {
    pub prompt: String,
    pub negative_prompt: String,
    pub seed: Option<u64>,
    pub steps: Option<u32>,
    pub cfg_scale: Option<f64>,
    pub sampler: Option<String>,
    pub clip_skip: Option<u32>,
    pub model_name: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub version: Option<String>,
    /// Hash map: "type:filename" → "AutoV2 hash"
    pub hashes: Vec<(String, String)>,
    /// CivitAI-resolved resources with version IDs.
    pub civitai_resources: Vec<CivitaiResource>,
}

#[derive(Debug, Clone)]
pub struct CivitaiResource {
    pub resource_type: String,
    pub model_version_id: u64,
}

/// Fetch image metadata from the CivitAI API.
pub async fn fetch_image(http: &Client, image_id: u64) -> Result<ImageMetadata> {
    let url = format!("{API_BASE}/images?imageId={image_id}&limit=1");

    let resp = http
        .get(&url)
        .timeout(TIMEOUT)
        .send()
        .await
        .with_context(|| format!("CivitAI API request failed for image {image_id}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("CivitAI API returned HTTP {} for image {image_id}", resp.status());
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .context("failed to parse CivitAI API response")?;

    let item = data
        .get("items")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .with_context(|| format!("CivitAI image {image_id} not found"))?;

    let image_url = item["url"].as_str().unwrap_or("").to_string();
    let width = item["width"].as_u64().unwrap_or(0) as u32;
    let height = item["height"].as_u64().unwrap_or(0) as u32;
    let username = item["username"].as_str().unwrap_or("").to_string();
    let base_model = item["baseModel"].as_str().map(String::from);

    let model_version_ids: Vec<u64> = item
        .get("modelVersionIds")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
        .unwrap_or_default();

    // Parse the doubly-nested meta
    let generation = parse_generation_meta(item.get("meta"));

    Ok(ImageMetadata {
        image_id,
        image_url,
        width,
        height,
        username,
        base_model,
        model_version_ids,
        generation,
    })
}

fn parse_generation_meta(meta_outer: Option<&serde_json::Value>) -> Option<GenerationMeta> {
    let meta = meta_outer?
        .get("meta")?;

    // meta can be null
    if meta.is_null() {
        return None;
    }

    let prompt = meta.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let negative_prompt = meta.get("negativePrompt").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // If prompt is empty, this isn't useful generation data
    if prompt.is_empty() && negative_prompt.is_empty() {
        // Check if there's any other useful field
        if meta.get("Model").is_none() && meta.get("steps").is_none() {
            return None;
        }
    }

    let hashes: Vec<(String, String)> = meta
        .get("hashes")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .unwrap_or_default();

    let civitai_resources: Vec<CivitaiResource> = meta
        .get("civitaiResources")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| {
                    Some(CivitaiResource {
                        resource_type: r.get("type")?.as_str()?.to_string(),
                        model_version_id: r.get("modelVersionId")?.as_u64()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Some(GenerationMeta {
        prompt,
        negative_prompt,
        seed: meta.get("seed").and_then(|v| v.as_u64()),
        steps: meta.get("steps").and_then(|v| v.as_u64()).map(|v| v as u32),
        cfg_scale: meta.get("cfgScale").and_then(|v| v.as_f64()),
        sampler: meta.get("sampler").and_then(|v| v.as_str()).map(String::from),
        clip_skip: meta.get("clipSkip").and_then(|v| v.as_u64()).map(|v| v as u32),
        model_name: meta.get("Model").and_then(|v| v.as_str()).map(String::from),
        width: meta.get("width").and_then(|v| v.as_u64()).map(|v| v as u32),
        height: meta.get("height").and_then(|v| v.as_u64()).map(|v| v as u32),
        version: meta.get("Version").and_then(|v| v.as_str()).map(String::from),
        hashes,
        civitai_resources,
    })
}

// ── Image Download ────────────────────────────────────────────

/// Download the original image bytes from CivitAI.
/// Follows redirects to get the actual original (may be PNG even if URL says .jpeg).
pub async fn download_original_image(http: &Client, image_url: &str) -> Result<Vec<u8>> {
    let resp = http
        .get(image_url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .with_context(|| format!("failed to download image: {image_url}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("image download returned HTTP {}", resp.status());
    }

    let bytes = resp.bytes().await.context("failed to read image bytes")?;
    Ok(bytes.to_vec())
}

// ── Model Version Resolution ──────────────────────────────────

/// Resolved model from CivitAI.
#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub filename: String,
    pub model_name: String,
    pub version_name: String,
    pub model_type: String,
    pub base_model: String,
    pub download_url: String,
    pub sha256: Option<String>,
    pub size_bytes: Option<u64>,
}

/// Resolve a CivitAI model version ID to full details.
/// Returns None on failure (API down, model deleted, etc.) — never panics.
pub async fn resolve_model_version(http: &Client, version_id: u64) -> Option<ResolvedModel> {
    let url = format!("{API_BASE}/model-versions/{version_id}");

    let resp = http
        .get(&url)
        .timeout(TIMEOUT)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        tracing::debug!(version_id, status = %resp.status(), "CivitAI model version not found");
        return None;
    }

    let data: serde_json::Value = resp.json().await.ok()?;

    let model_name = data.get("model")?.get("name")?.as_str()?.to_string();
    let model_type = data.get("model")?.get("type")?.as_str()?.to_string();
    let version_name = data.get("name")?.as_str()?.to_string();
    let base_model = data.get("baseModel").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let file = data.get("files")?.as_array()?.first()?;
    let filename = file.get("name")?.as_str()?.to_string();
    let sha256 = file
        .get("hashes")
        .and_then(|h| h.get("SHA256"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let size_bytes = file
        .get("sizeKB")
        .and_then(|v| v.as_f64())
        .map(|kb| (kb * 1024.0) as u64);

    let download_url = format!("https://civitai.com/api/download/models/{version_id}");

    Some(ResolvedModel {
        filename,
        model_name,
        version_name,
        model_type,
        base_model,
        download_url,
        sha256,
        size_bytes,
    })
}

/// Resolve a model by its AutoV2 hash.
pub async fn resolve_by_hash(http: &Client, hash: &str) -> Option<ResolvedModel> {
    let url = format!("{API_BASE}/model-versions/by-hash/{hash}");

    let resp = http.get(&url).timeout(TIMEOUT).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let data: serde_json::Value = resp.json().await.ok()?;
    let version_id = data.get("id")?.as_u64()?;

    // Reuse the version resolver
    let model_name = data.get("model")?.get("name")?.as_str()?.to_string();
    let model_type = data.get("model")?.get("type")?.as_str()?.to_string();
    let version_name = data.get("name")?.as_str()?.to_string();
    let base_model = data.get("baseModel").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let file = data.get("files")?.as_array()?.first()?;
    let filename = file.get("name")?.as_str()?.to_string();
    let sha256 = file.get("hashes").and_then(|h| h.get("SHA256")).and_then(|v| v.as_str()).map(String::from);
    let size_bytes = file.get("sizeKB").and_then(|v| v.as_f64()).map(|kb| (kb * 1024.0) as u64);

    Some(ResolvedModel {
        filename,
        model_name,
        version_name,
        model_type,
        base_model,
        download_url: format!("https://civitai.com/api/download/models/{version_id}"),
        sha256,
        size_bytes,
    })
}

// ── Unsupported Generator Detection ───────────────────────────

/// Check if the base model indicates a non-importable generator.
pub fn is_unsupported_generator(base_model: Option<&str>) -> Option<&str> {
    match base_model {
        Some("OpenAI") => Some("OpenAI (DALL-E / GPT Image)"),
        Some("Midjourney") => Some("Midjourney"),
        Some("Google") => Some("Google Imagen"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_generators() {
        assert!(is_unsupported_generator(Some("OpenAI")).is_some());
        assert!(is_unsupported_generator(Some("Midjourney")).is_some());
        assert!(is_unsupported_generator(Some("Illustrious")).is_none());
        assert!(is_unsupported_generator(None).is_none());
    }
}
