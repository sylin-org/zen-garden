//! Well-known model registry — static lookup for popular models not covered
//! by CivitAI or ComfyUI Manager.
//!
//! Data lives in `known_models.json` (editable without recompilation on disk).
//! Embedded at compile time via `include_str!`, parsed once on first access.
//!
//! Future: supplement with SearXNG search for dynamic discovery.

use std::sync::LazyLock;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct KnownModel {
    pub filename: String,
    pub url: String,
    pub model_type: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub description: String,
}

static REGISTRY: LazyLock<Vec<KnownModel>> = LazyLock::new(|| {
    let json = include_str!("known_models.json");
    let raw: Vec<serde_json::Value> = serde_json::from_str(json).expect("known_models.json is valid JSON");
    raw.into_iter()
        .filter(|v| v.get("filename").is_some()) // skip _comment entries
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect()
});

/// Look up a known model by filename. Supports partial matching for renamed files.
pub fn lookup(filename: &str) -> Option<&'static KnownModel> {
    let registry = &*REGISTRY;

    // Exact match first
    if let Some(m) = registry.iter().find(|m| m.filename == filename) {
        return Some(m);
    }

    // Partial match: stem-based (handles renames like "zimageturbo-vae-ae.safetensors" → "ae.safetensors")
    let stem = filename.rsplit_once('.').map(|(s, _)| s).unwrap_or(filename);
    registry.iter().find(|m| {
        let known_stem = m.filename.rsplit_once('.').map(|(s, _)| s).unwrap_or(&m.filename);
        stem.ends_with(known_stem) || known_stem.ends_with(stem)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads() {
        assert!(!REGISTRY.is_empty());
    }

    #[test]
    fn exact_match() {
        let m = lookup("qwen_3_4b.safetensors").unwrap();
        assert_eq!(m.model_type, "clip");
        assert!(m.url.contains("huggingface.co"));
    }

    #[test]
    fn partial_match_vae_rename() {
        let m = lookup("zimageturbo-vae-ae.safetensors").unwrap();
        assert_eq!(m.filename, "ae.safetensors");
        assert_eq!(m.model_type, "vae");
    }

    #[test]
    fn no_match() {
        assert!(lookup("totally_unknown_model.bin").is_none());
    }
}
