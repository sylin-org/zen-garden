//! Model resolution cascade — resolve filenames to download URLs (ORCH-0023).
//!
//! Priority chain:
//! 1. Local dependency cache (exact filename match)
//! 2. ComfyUI Manager model-list.json (527+ curated entries)
//! 3. CivitAI hash lookup (when hash available from image metadata)
//! 4. Unresolved (user must provide URL)

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use anyhow::{Context, Result};
use reqwest::Client;

use crate::skills::cache::DependencyManifest;
use super::civitai::{self, CivitaiResource, ResolvedModel};

const COMFYUI_MANAGER_MODEL_LIST_URL: &str =
    "https://raw.githubusercontent.com/ltdrdata/ComfyUI-Manager/main/model-list.json";

// ── Resolution Result ─────────────────────────────────────────

/// Resolution status for a single model.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ModelResolution {
    /// Already in the local dependency cache.
    Cached { filename: String },
    /// Resolved via ComfyUI Manager or CivitAI — download URL known.
    Resolved {
        filename: String,
        url: String,
        sha256: Option<String>,
        size_bytes: Option<u64>,
        source: String,
    },
    /// Could not resolve automatically — user must provide URL.
    Unresolved {
        filename: String,
        reason: String,
    },
}

impl ModelResolution {
    pub fn filename(&self) -> &str {
        match self {
            Self::Cached { filename } => filename,
            Self::Resolved { filename, .. } => filename,
            Self::Unresolved { filename, .. } => filename,
        }
    }

    pub fn is_resolved(&self) -> bool {
        !matches!(self, Self::Unresolved { .. })
    }
}

// ── ComfyUI Manager Registry ─────────────────────────────────

/// Cached ComfyUI Manager model-list.json.
/// Maps exact filenames to download URLs.
#[derive(Debug, Default)]
pub struct ManagerRegistry {
    /// filename → download URL
    entries: HashMap<String, ManagerEntry>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ManagerModelEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    filename: String,
    #[serde(default)]
    url: String,
    #[serde(default, rename = "type")]
    model_type: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Clone)]
pub struct ManagerEntry {
    pub filename: String,
    pub url: String,
    pub model_type: String,
    pub description: String,
}

impl ManagerRegistry {
    /// Fetch the model-list.json from GitHub and parse it.
    pub async fn fetch(http: &Client) -> Self {
        tracing::info!("fetching ComfyUI Manager model-list.json");

        let resp = http
            .get(COMFYUI_MANAGER_MODEL_LIST_URL)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await;

        let resp = match resp {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                tracing::warn!(status = %r.status(), "failed to fetch ComfyUI Manager model list");
                return Self::default();
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to fetch ComfyUI Manager model list");
                return Self::default();
            }
        };

        let raw: Vec<ManagerModelEntry> = match resp.json().await {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(error = %e, "failed to parse ComfyUI Manager model list");
                return Self::default();
            }
        };

        let mut entries = HashMap::new();
        for entry in raw {
            if !entry.filename.is_empty() && !entry.url.is_empty() {
                entries.insert(entry.filename.clone(), ManagerEntry {
                    filename: entry.filename,
                    url: entry.url,
                    model_type: entry.model_type,
                    description: entry.description,
                });
            }
        }

        tracing::info!(count = entries.len(), "ComfyUI Manager model registry loaded");
        Self { entries }
    }

    /// Look up a filename in the registry.
    pub fn resolve(&self, filename: &str) -> Option<&ManagerEntry> {
        self.entries.get(filename)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

// ── Resolution Cascade ────────────────────────────────────────

/// Run the full model resolution cascade for a list of model filenames.
///
/// Uses: local cache → ComfyUI Manager → CivitAI hash lookup → unresolved.
pub async fn resolve_models(
    http: &Client,
    filenames: &[String],
    model_types: &HashMap<String, String>,
    cache_manifest: &DependencyManifest,
    manager_registry: &ManagerRegistry,
    civitai_resources: &[CivitaiResource],
) -> Vec<ModelResolution> {
    let mut results = Vec::new();

    for filename in filenames {
        // Priority 1: already in local cache
        if cache_manifest.files.contains_key(filename)
            || cache_manifest.aliases.contains_key(filename)
        {
            results.push(ModelResolution::Cached {
                filename: filename.clone(),
            });
            continue;
        }

        // Priority 2: ComfyUI Manager registry (exact filename match)
        if let Some(entry) = manager_registry.resolve(filename) {
            results.push(ModelResolution::Resolved {
                filename: filename.clone(),
                url: entry.url.clone(),
                sha256: None,
                size_bytes: None,
                source: "comfyui-manager".into(),
            });
            continue;
        }

        // Priority 3: CivitAI hash lookup (if we have a hash from image metadata)
        let civitai_hash = civitai_resources.iter().find_map(|r| {
            // Match by name similarity — the resource name often contains the model name
            if r.hash.is_some() {
                let name = r.name.as_deref().unwrap_or("");
                let stem = filename.split('.').next().unwrap_or(filename);
                if name.to_lowercase().contains(&stem.to_lowercase())
                    || stem.to_lowercase().contains(&name.to_lowercase())
                {
                    r.hash.clone()
                } else {
                    None
                }
            } else {
                None
            }
        });

        if let Some(hash) = civitai_hash {
            if let Ok(Some(resolved)) = civitai::resolve_model_by_hash(http, &hash).await {
                results.push(ModelResolution::Resolved {
                    filename: filename.clone(),
                    url: resolved.download_url,
                    sha256: resolved.sha256,
                    size_bytes: resolved.size_bytes,
                    source: "civitai".into(),
                });
                continue;
            }
        }

        // Unresolved — user must provide
        results.push(ModelResolution::Unresolved {
            filename: filename.clone(),
            reason: "no match in cache, ComfyUI Manager, or CivitAI".into(),
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_model_resolves_immediately() {
        let mut manifest = DependencyManifest::default();
        manifest.files.insert("model.pth".into(), "sha256:abc".into());

        let rt = tokio::runtime::Runtime::new().unwrap();
        let http = Client::new();
        let manager = ManagerRegistry::default();

        let results = rt.block_on(resolve_models(
            &http,
            &["model.pth".into()],
            &HashMap::new(),
            &manifest,
            &manager,
            &[],
        ));

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], ModelResolution::Cached { .. }));
    }

    #[test]
    fn unknown_model_is_unresolved() {
        let manifest = DependencyManifest::default();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let http = Client::new();
        let manager = ManagerRegistry::default();

        let results = rt.block_on(resolve_models(
            &http,
            &["unknown_model.safetensors".into()],
            &HashMap::new(),
            &manifest,
            &manager,
            &[],
        ));

        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], ModelResolution::Unresolved { .. }));
    }
}
