//! CivitAI API client — resilient, no panics, descriptive errors.
//!
//! All API calls go through `CivitaiClient`, which holds the HTTP client
//! and an optional API token. When a token is present, it's sent as a
//! Bearer header on every request — required for NSFW/restricted images.

use anyhow::{Context, Result};
use reqwest::Client;
use std::time::Duration;

const API_BASE: &str = "https://civitai.com/api/v1";
const TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

// ── Client ───────────────────────────────────────────────────

/// CivitAI API client with optional authentication.
pub struct CivitaiClient {
    http: Client,
    token: Option<String>,
}

impl CivitaiClient {
    /// Create a client without authentication (public API only).
    pub fn new(http: Client) -> Self {
        Self { http, token: None }
    }

    /// Create a client with a CivitAI API token for restricted content.
    pub fn with_token(http: Client, token: String) -> Self {
        Self { http, token: Some(token) }
    }

    /// Build a GET request with auth header if token is present.
    fn get(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self.http.get(url);
        if let Some(ref token) = self.token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        req
    }

    /// Build a download URL with token as query param.
    /// CivitAI's `/api/download/` endpoint uses `?token=` (not Bearer header)
    /// because the CDN redirect strips auth headers.
    fn download_url(&self, url: &str) -> String {
        match &self.token {
            Some(token) => {
                let sep = if url.contains('?') { "&" } else { "?" };
                format!("{url}{sep}token={token}")
            }
            None => url.to_string(),
        }
    }

    /// Access the underlying HTTP client (for non-CivitAI requests).
    pub fn http(&self) -> &Client {
        &self.http
    }
}

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
    pub weight: Option<f64>,
}

/// Fetch image metadata from the CivitAI API.
pub async fn fetch_image(client: &CivitaiClient, image_id: u64) -> Result<ImageMetadata> {
    // browsingLevel=31 includes all NSFW levels (CivitAI filters NSFW by default even with auth)
    let url = format!("{API_BASE}/images?imageId={image_id}&limit=1&browsingLevel=31");

    let resp = client.get(&url)
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

    let items = data
        .get("items")
        .and_then(|v| v.as_array());

    let item = match items {
        Some(arr) if !arr.is_empty() => &arr[0],
        _ => anyhow::bail!(
            "CivitAI image {image_id} not found. \
             The image may be NSFW-restricted or require authentication — \
             the CivitAI public API does not return restricted images."
        ),
    };

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
                        weight: r.get("weight").and_then(|v| v.as_f64()),
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
pub async fn download_original_image(client: &CivitaiClient, image_url: &str) -> Result<Vec<u8>> {
    let resp = client.get(image_url)
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
pub async fn resolve_model_version(client: &CivitaiClient, version_id: u64) -> Option<ResolvedModel> {
    let url = format!("{API_BASE}/model-versions/{version_id}");

    let resp = client.get(&url)
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
pub async fn resolve_by_hash(client: &CivitaiClient, hash: &str) -> Option<ResolvedModel> {
    let url = format!("{API_BASE}/model-versions/by-hash/{hash}");

    let resp = client.get(&url).timeout(TIMEOUT).send().await.ok()?;
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

// ── Model Page / Workflow Download ────────────────────────────

/// Metadata from a CivitAI model page.
#[derive(Debug)]
pub struct ModelPageMeta {
    pub model_name: String,
    pub model_type: String,
    pub version_name: String,
    pub version_id: u64,
    pub base_model: String,
    pub download_url: String,
    pub file_name: String,
    /// Model version IDs of all resources used (from the model's images).
    pub resource_version_ids: Vec<u64>,
    /// Preview image URL (from first image).
    pub preview_url: Option<String>,
    /// Generation metadata from first image with gen data.
    pub generation: Option<GenerationMeta>,
}

/// Fetch model page metadata. If `version_id` is None, uses the latest version.
pub async fn fetch_model_page(
    client: &CivitaiClient,
    model_id: u64,
    version_id: Option<u64>,
) -> Result<ModelPageMeta> {
    let url = format!("{API_BASE}/models/{model_id}");
    let resp = client.get(&url)
        .timeout(TIMEOUT)
        .send()
        .await
        .with_context(|| format!("CivitAI model API failed for model {model_id}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("CivitAI API returned HTTP {} for model {model_id}", resp.status());
    }

    let data: serde_json::Value = resp.json().await.context("parse CivitAI model response")?;

    let model_name = data["name"].as_str().unwrap_or("").to_string();
    let model_type = data["type"].as_str().unwrap_or("").to_string();

    // Find the target version
    let versions = data["modelVersions"].as_array()
        .with_context(|| format!("CivitAI model {model_id} has no versions"))?;

    let version = if let Some(vid) = version_id {
        versions.iter().find(|v| v["id"].as_u64() == Some(vid))
            .with_context(|| format!("Version {vid} not found on model {model_id}"))?
    } else if model_type == "Workflows" {
        // For workflow models, skip placeholder versions (tiny files < 1KB).
        // CivitAI workflow models often have an "IMAGE POSTS HERE" placeholder as the first version.
        versions.iter()
            .find(|v| {
                v["files"].as_array()
                    .and_then(|f| f.first())
                    .and_then(|f| f["sizeKB"].as_f64())
                    .unwrap_or(0.0) > 1.0
            })
            .or(versions.first())
            .with_context(|| format!("Model {model_id} has no versions"))?
    } else {
        versions.first()
            .with_context(|| format!("Model {model_id} has no versions"))?
    };

    let vid = version["id"].as_u64().unwrap_or(0);
    let version_name = version["name"].as_str().unwrap_or("").to_string();
    let base_model = version["baseModel"].as_str().unwrap_or("").to_string();

    let file = version["files"].as_array()
        .and_then(|f| f.first())
        .with_context(|| format!("Version {vid} has no files"))?;
    let file_name = file["name"].as_str().unwrap_or("").to_string();
    let download_url = format!("https://civitai.com/api/download/models/{vid}");

    // Collect resource version IDs and generation data from version images
    let mut resource_version_ids = Vec::new();
    let mut preview_url = None;
    let mut generation = None;

    if let Some(images) = version["images"].as_array() {
        for img in images {
            if preview_url.is_none() {
                preview_url = img["url"].as_str().map(String::from);
            }
            // Collect model version IDs from image resources
            if let Some(ids) = img["modelVersionIds"].as_array() {
                for id in ids {
                    if let Some(vid) = id.as_u64() {
                        if !resource_version_ids.contains(&vid) {
                            resource_version_ids.push(vid);
                        }
                    }
                }
            }
            // Get generation data from first image that has it
            if generation.is_none() {
                generation = parse_generation_meta(img.get("meta"));
            }
        }
    }

    Ok(ModelPageMeta {
        model_name,
        model_type,
        version_name,
        version_id: vid,
        base_model,
        download_url,
        file_name,
        resource_version_ids,
        preview_url,
        generation,
    })
}

/// Download and extract a workflow from a CivitAI model file (zip or JSON).
pub async fn download_workflow(client: &CivitaiClient, download_url: &str) -> Result<serde_json::Value> {
    // CivitAI download URLs need token as query param (CDN strips Bearer headers)
    let url = client.download_url(download_url);
    let resp = client.http()
        .get(&url)
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .with_context(|| format!("download failed: {download_url}"))?;

    if !resp.status().is_success() {
        anyhow::bail!("download returned HTTP {} for workflow file", resp.status());
    }

    let bytes = resp.bytes().await.context("read download bytes")?;

    tracing::info!(
        bytes = bytes.len(),
        first_4 = ?&bytes[..std::cmp::min(4, bytes.len())],
        "download_workflow: received file"
    );

    // Try as zip first
    if bytes.len() > 4 && bytes[0..4] == [0x50, 0x4B, 0x03, 0x04] {
        return extract_workflow_from_zip(&bytes);
    }

    // Try as raw JSON
    let json: serde_json::Value = serde_json::from_slice(&bytes)
        .context("downloaded file is neither a valid zip nor JSON")?;

    // If it's a UI-format workflow, convert to API format
    if super::ui_to_api::is_ui_format(&json) {
        tracing::info!("download_workflow: converting UI-format to API format");
        return super::ui_to_api::convert(&json);
    }

    Ok(json)
}

/// Extract a ComfyUI workflow JSON from a zip archive.
fn extract_workflow_from_zip(bytes: &[u8]) -> Result<serde_json::Value> {
    use std::io::{Cursor, Read};

    let reader = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader).context("invalid zip archive")?;

    tracing::info!(entry_count = archive.len(), "extract_workflow_from_zip: opened archive");

    // Find the first .json file in the archive
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).context("read zip entry")?;
        let name = file.name().to_string();
        tracing::info!(entry = %name, size = file.size(), "extract_workflow_from_zip: entry");

        if name.to_lowercase().ends_with(".json") {
            let mut contents = String::new();
            file.read_to_string(&mut contents).context("read zip JSON entry")?;
            let json: serde_json::Value = serde_json::from_str(&contents)
                .with_context(|| format!("parse JSON from zip entry: {name}"))?;

            let is_api = super::input_detect::is_comfyui_workflow(&json);
            let is_ui = super::ui_to_api::is_ui_format(&json);
            tracing::info!(
                entry = %name,
                is_api_format = is_api,
                is_ui_format = is_ui,
                top_keys = ?json.as_object().map(|o| o.keys().take(10).collect::<Vec<_>>()),
                "extract_workflow_from_zip: parsed JSON"
            );

            // Convert UI format to API format
            if is_ui && !is_api {
                tracing::info!("extract_workflow_from_zip: converting UI-format to API format");
                return super::ui_to_api::convert(&json);
            }

            return Ok(json);
        }
    }

    // List all entries for debugging
    let entry_names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();
    anyhow::bail!("zip archive contains no JSON files. Entries: {:?}", entry_names)
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
