//! Model resolution — resolve filenames to download URLs via cascade.
//!
//! Priority:
//! 1. CivitAI modelVersionIds (direct, no search)
//! 2. CivitAI hash lookup (from image meta hashes)
//! 3. Local dependency cache (already downloaded)
//! 4. ComfyUI Manager registry (curated model list)
//! 5. Unresolved (user provides URL)

use std::collections::HashMap;
use reqwest::Client;

use super::civitai;
use crate::skills::cache::DependencyManifest;

// ── Resolution Result ─────────────────────────────────────────

/// Resolution status for a single model dependency.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ModelResolution {
    Resolved {
        filename: String,
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        size_bytes: Option<u64>,
        model_type: String,
        source: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        license: Option<String>,
    },
    Cached {
        filename: String,
        model_type: String,
    },
    /// URL resolved but download requires authentication.
    AuthRequired {
        filename: String,
        url: String,
        model_type: String,
        source: String,
        /// Which secret key is needed (e.g., "civitai").
        secret_key: String,
    },
    Unresolved {
        filename: String,
        model_type: String,
        reason: String,
    },
}

impl ModelResolution {
    pub fn filename(&self) -> &str {
        match self {
            Self::Resolved { filename, .. } => filename,
            Self::Cached { filename, .. } => filename,
            Self::AuthRequired { filename, .. } => filename,
            Self::Unresolved { filename, .. } => filename,
        }
    }
}

// ── ComfyUI Manager Registry ─────────────────────────────────

const MANAGER_MODEL_LIST_URL: &str =
    "https://raw.githubusercontent.com/ltdrdata/ComfyUI-Manager/main/model-list.json";

/// Cached ComfyUI Manager model registry. Maps filename → download URL.
#[derive(Debug, Default)]
pub struct ManagerRegistry {
    entries: HashMap<String, ManagerEntry>,
}

#[derive(Debug, Clone)]
pub struct ManagerEntry {
    pub url: String,
    pub model_type: String,
}

impl ManagerRegistry {
    /// Fetch and cache the registry. Failures return empty — never blocks the pipeline.
    pub async fn fetch(http: &Client) -> Self {
        let resp = match http.get(MANAGER_MODEL_LIST_URL).timeout(std::time::Duration::from_secs(30)).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                tracing::warn!(status = %r.status(), "ComfyUI Manager registry fetch failed");
                return Self::default();
            }
            Err(e) => {
                tracing::warn!(error = %e, "ComfyUI Manager registry unreachable");
                return Self::default();
            }
        };

        #[derive(serde::Deserialize)]
        struct Entry {
            #[serde(default)]
            filename: String,
            #[serde(default)]
            url: String,
            #[serde(default, rename = "type")]
            model_type: String,
        }

        #[derive(serde::Deserialize)]
        struct RegistryResponse {
            #[serde(default)]
            models: Vec<Entry>,
        }

        // The registry is {"models": [...]} not a flat array
        let raw: Vec<Entry> = match resp.json::<RegistryResponse>().await {
            Ok(r) => r.models,
            Err(e) => {
                tracing::warn!(error = %e, "ComfyUI Manager registry parse failed");
                return Self::default();
            }
        };

        let mut entries = HashMap::new();
        for e in raw {
            if !e.filename.is_empty() && !e.url.is_empty() {
                entries.insert(e.filename.clone(), ManagerEntry {
                    url: e.url,
                    model_type: e.model_type,
                });
            }
        }

        tracing::info!(count = entries.len(), "ComfyUI Manager registry loaded");
        Self { entries }
    }

    pub fn resolve(&self, filename: &str) -> Option<&ManagerEntry> {
        self.entries.get(filename)
    }
}

// ── Resolution Context ────────────────────────────────────────

/// All available resolution sources, collected before the cascade runs.
pub struct ResolutionContext {
    /// Models resolved from CivitAI modelVersionIds (most reliable).
    pub civitai_models: Vec<civitai::ResolvedModel>,
    /// Hashes from CivitAI image meta: "type:filename" → "hash".
    pub hashes: Vec<(String, String)>,
    /// Local dependency cache manifest.
    pub cache_manifest: DependencyManifest,
    /// ComfyUI Manager registry.
    pub manager: ManagerRegistry,
}

// ── Cascade ───────────────────────────────────────────────────

/// Resolve a list of model filenames through the cascade.
/// Each filename gets the best resolution available. Failures produce Unresolved, not errors.
/// After resolution, probes each URL with HEAD to detect auth requirements.
pub async fn resolve_all(
    civitai: &civitai::CivitaiClient,
    filenames: &[(String, String)], // (filename, model_type)
    ctx: &ResolutionContext,
) -> Vec<ModelResolution> {
    let mut results = Vec::new();

    for (filename, model_type) in filenames {
        let mut resolution = resolve_one(civitai, filename, model_type, ctx).await;

        // Probe resolved URLs to detect auth requirements
        if let ModelResolution::Resolved { ref url, ref filename, ref model_type, ref source, .. } = resolution {
            if let Some(probe_result) = probe_url(civitai.http(), url).await {
                if probe_result == ProbeResult::AuthRequired {
                    let secret_key = if url.contains("civitai.com") { "civitai" }
                        else if url.contains("huggingface.co") { "huggingface" }
                        else { "unknown" };

                    resolution = ModelResolution::AuthRequired {
                        filename: filename.clone(),
                        url: url.clone(),
                        model_type: model_type.clone(),
                        source: source.clone(),
                        secret_key: secret_key.to_string(),
                    };
                }
            }
        }

        results.push(resolution);
    }

    results
}

#[derive(PartialEq)]
enum ProbeResult {
    Ok,
    AuthRequired,
    Error,
}

/// Probe a URL with HEAD to check if it's downloadable.
async fn probe_url(http: &Client, url: &str) -> Option<ProbeResult> {
    let resp = http
        .head(url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .ok()?;

    match resp.status().as_u16() {
        200..=299 => Some(ProbeResult::Ok),
        401 | 403 => Some(ProbeResult::AuthRequired),
        _ => Some(ProbeResult::Error),
    }
}

async fn resolve_one(
    civitai: &civitai::CivitaiClient,
    filename: &str,
    model_type: &str,
    ctx: &ResolutionContext,
) -> ModelResolution {
    // Priority 1: CivitAI modelVersionIds (already resolved)
    // Match by filename — the version resolution gives us the exact filename
    if let Some(resolved) = ctx.civitai_models.iter().find(|m| m.filename == filename) {
        return ModelResolution::Resolved {
            filename: filename.to_string(),
            url: resolved.download_url.clone(),
            sha256: resolved.sha256.clone(),
            size_bytes: resolved.size_bytes,
            model_type: civitai_type_to_comfyui(model_type, &resolved.model_type),
            source: "civitai".into(),
            display_name: Some(format!("{} / {}", resolved.model_name, resolved.version_name)),
            license: None,
        };
    }

    // Also check by model name stem (the meta.Model field often doesn't have the extension)
    let stem = filename.split('.').next().unwrap_or(filename);
    if let Some(resolved) = ctx.civitai_models.iter().find(|m| {
        m.filename.starts_with(stem) || m.model_name.to_lowercase().contains(&stem.to_lowercase())
    }) {
        return ModelResolution::Resolved {
            filename: resolved.filename.clone(),
            url: resolved.download_url.clone(),
            sha256: resolved.sha256.clone(),
            size_bytes: resolved.size_bytes,
            model_type: civitai_type_to_comfyui(model_type, &resolved.model_type),
            source: "civitai".into(),
            display_name: Some(format!("{} / {}", resolved.model_name, resolved.version_name)),
            license: None,
        };
    }

    // Priority 2: CivitAI hash lookup (from image meta hashes)
    for (hash_key, hash_value) in &ctx.hashes {
        // hash_key is "type:filename" or "model"
        let matches = hash_key.contains(filename)
            || hash_key.contains(stem)
            || hash_key == "model";

        if matches && !hash_value.is_empty() {
            if let Some(resolved) = civitai::resolve_by_hash(civitai, hash_value).await {
                return ModelResolution::Resolved {
                    filename: resolved.filename.clone(),
                    url: resolved.download_url.clone(),
                    sha256: resolved.sha256.clone(),
                    size_bytes: resolved.size_bytes,
                    model_type: civitai_type_to_comfyui(model_type, &resolved.model_type),
                    source: "civitai-hash".into(),
                    display_name: Some(format!("{} / {}", resolved.model_name, resolved.version_name)),
                    license: None,
                };
            }
        }
    }

    // Priority 3: Local cache
    let resolved_name = ctx.cache_manifest.resolve(filename);
    if ctx.cache_manifest.files.contains_key(&resolved_name) {
        return ModelResolution::Cached {
            filename: filename.to_string(),
            model_type: model_type.to_string(),
        };
    }

    // Priority 4: ComfyUI Manager registry
    if let Some(entry) = ctx.manager.resolve(filename) {
        return ModelResolution::Resolved {
            filename: filename.to_string(),
            url: entry.url.clone(),
            sha256: None,
            size_bytes: None,
            model_type: if entry.model_type.is_empty() { model_type.to_string() } else { entry.model_type.clone() },
            source: "comfyui-manager".into(),
            display_name: None,
            license: None,
        };
    }

    // Priority 5: Well-known models registry (HuggingFace ecosystem models)
    if let Some(known) = super::known_models::lookup(filename) {
        return ModelResolution::Resolved {
            filename: known.filename.clone(),
            url: known.url.clone(),
            sha256: known.sha256.clone(),
            size_bytes: Some(known.size_bytes),
            model_type: known.model_type.clone(),
            source: "known-models".into(),
            display_name: Some(known.description.clone()),
            license: None,
        };
    }

    // Unresolved
    ModelResolution::Unresolved {
        filename: filename.to_string(),
        model_type: model_type.to_string(),
        reason: "not found in CivitAI, local cache, ComfyUI Manager, or known models registry".into(),
    }
}

/// Map CivitAI model type to ComfyUI model directory name.
fn civitai_type_to_comfyui(parser_type: &str, civitai_type: &str) -> String {
    // If the parser already identified the type, prefer that
    if !parser_type.is_empty() && parser_type != "unknown" {
        return parser_type.to_string();
    }

    match civitai_type.to_lowercase().as_str() {
        "checkpoint" => "checkpoints",
        "lora" => "loras",
        "vae" => "vae",
        "upscaler" => "upscale_models",
        "controlnet" => "controlnet",
        "embedding" | "textualinversion" => "embeddings",
        _ => "checkpoints",
    }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civitai_type_mapping() {
        assert_eq!(civitai_type_to_comfyui("", "Checkpoint"), "checkpoints");
        assert_eq!(civitai_type_to_comfyui("", "LORA"), "loras");
        assert_eq!(civitai_type_to_comfyui("", "Upscaler"), "upscale_models");
        assert_eq!(civitai_type_to_comfyui("upscale_models", "Checkpoint"), "upscale_models");
    }
}
