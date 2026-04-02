//! Secrets store — API keys and tokens, stored separately from config.
//!
//! Stored in `{data_dir}/secrets.json`. Never returned in full via API.
//! Read returns masked values ("sk-...abc123"). Write replaces the value.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Known secret keys.
pub const KEY_CIVITAI: &str = "civitai";
pub const KEY_HUGGINGFACE: &str = "huggingface";

/// All known secret key definitions.
pub const ALL_KEYS: &[SecretKeyDef] = &[
    SecretKeyDef { key: KEY_CIVITAI, label: "CivitAI", description: "API token for downloading creator-restricted models. Get yours at civitai.com/user/account" },
    SecretKeyDef { key: KEY_HUGGINGFACE, label: "Hugging Face", description: "Access token for gated models (FLUX, Llama, etc.). Get yours at huggingface.co/settings/tokens" },
];

pub struct SecretKeyDef {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

/// The secrets store — thread-safe, persisted to disk.
#[derive(Clone)]
pub struct SecretsStore {
    path: PathBuf,
    inner: Arc<RwLock<SecretsData>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SecretsData {
    #[serde(default)]
    keys: HashMap<String, String>,
}

impl SecretsStore {
    /// Load from disk, or create empty.
    pub async fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("secrets.json");
        let data = match tokio::fs::read_to_string(&path).await {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => SecretsData::default(),
        };

        let count = data.keys.len();
        if count > 0 {
            tracing::info!(count, "secrets loaded");
        }

        Self {
            path,
            inner: Arc::new(RwLock::new(data)),
        }
    }

    /// Get a secret value. Returns None if not set.
    pub async fn get(&self, key: &str) -> Option<String> {
        let data = self.inner.read().await;
        data.keys.get(key).cloned()
    }

    /// Set a secret value. Persists to disk immediately.
    pub async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        {
            let mut data = self.inner.write().await;
            if value.is_empty() {
                data.keys.remove(key);
            } else {
                data.keys.insert(key.to_string(), value.to_string());
            }
        }
        self.save().await
    }

    /// Delete a secret. Persists to disk immediately.
    pub async fn delete(&self, key: &str) -> anyhow::Result<()> {
        {
            let mut data = self.inner.write().await;
            data.keys.remove(key);
        }
        self.save().await
    }

    /// List all secret keys with masked values.
    pub async fn list_masked(&self) -> Vec<MaskedSecret> {
        let data = self.inner.read().await;
        ALL_KEYS
            .iter()
            .map(|def| {
                let value = data.keys.get(def.key);
                MaskedSecret {
                    key: def.key.to_string(),
                    label: def.label.to_string(),
                    description: def.description.to_string(),
                    is_set: value.is_some(),
                    masked_value: value.map(|v| mask_value(v)),
                }
            })
            .collect()
    }

    /// Check if a specific key is configured.
    pub async fn has(&self, key: &str) -> bool {
        let data = self.inner.read().await;
        data.keys.contains_key(key)
    }

    async fn save(&self) -> anyhow::Result<()> {
        let data = self.inner.read().await;
        let json = serde_json::to_string_pretty(&*data)?;
        let tmp = self.path.with_extension("json.tmp");
        tokio::fs::write(&tmp, json).await?;
        tokio::fs::rename(&tmp, &self.path).await?;
        Ok(())
    }
}

/// A secret with its value masked for API responses.
#[derive(Debug, Clone, Serialize)]
pub struct MaskedSecret {
    pub key: String,
    pub label: String,
    pub description: String,
    pub is_set: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_value: Option<String>,
}

/// Mask a secret value: show first 4 and last 4 chars.
fn mask_value(value: &str) -> String {
    if value.len() <= 8 {
        return "••••••••".to_string();
    }
    let prefix: String = value.chars().take(4).collect();
    let suffix: String = value.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    let dots = "•".repeat(8);
    format!("{prefix}{dots}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_short_value() {
        assert_eq!(mask_value("abc"), "••••••••");
    }

    #[test]
    fn mask_long_value() {
        let masked = mask_value("sk-1234567890abcdef");
        assert!(masked.starts_with("sk-1"));
        assert!(masked.ends_with("cdef"));
        assert!(masked.contains("••••••••"));
    }

    #[test]
    fn all_keys_defined() {
        assert!(ALL_KEYS.len() >= 2);
        assert!(ALL_KEYS.iter().any(|k| k.key == "civitai"));
        assert!(ALL_KEYS.iter().any(|k| k.key == "huggingface"));
    }
}
