//! Cloud provider configuration types.
//!
//! `CloudProviderConfig` holds API keys and settings per provider.
//! `CloudProviderStore` manages the collection, persisting to `providers.json`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::catalog::Offering;
use crate::domain::types::{Capability, OfferingKind};

use super::anthropic::AnthropicProvider;
use super::openai::OpenAiProvider;

// ── Provider Config ─────────────────────────────────────────────

/// Configuration for a single cloud provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudProviderConfig {
    pub kind: OfferingKind,
    /// Display name (e.g., "openai", "anthropic", "groq").
    pub name: String,
    /// The API key. Never logged.
    pub api_key: String,
    /// Base URL (e.g., "https://api.openai.com").
    pub base_url: String,
    /// Whether the provider is enabled for routing.
    pub enabled: bool,
    /// Routing priority (default -10 for cloud fallback).
    #[serde(default = "default_priority")]
    pub priority: i32,
    /// Capabilities this provider supports.
    pub capabilities: Vec<Capability>,
    /// Optional model allowlist. Empty means all models.
    #[serde(default)]
    pub models: Vec<String>,
}

fn default_priority() -> i32 {
    -10
}

impl CloudProviderConfig {
    /// Mask the API key for display (show last 4 chars).
    pub fn masked_key(&self) -> String {
        if self.api_key.len() <= 4 {
            "****".to_string()
        } else {
            format!("****{}", &self.api_key[self.api_key.len() - 4..])
        }
    }
}

// ── Provider Store ──────────────────────────────────────────────

/// Manages all configured cloud providers, persisted to `providers.json`.
pub struct CloudProviderStore {
    providers: Vec<CloudProviderConfig>,
    file_path: PathBuf,
}

impl CloudProviderStore {
    /// Load providers from `{data_dir}/providers.json`.
    /// Returns an empty store if the file does not exist or is invalid.
    pub async fn load(data_dir: &str) -> Self {
        let file_path = Path::new(data_dir).join("providers.json");

        let providers = match tokio::fs::read_to_string(&file_path).await {
            Ok(content) => match serde_json::from_str::<Vec<CloudProviderConfig>>(&content) {
                Ok(configs) => {
                    tracing::info!(
                        count = configs.len(),
                        "loaded cloud provider configs from disk"
                    );
                    configs
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to parse providers.json, starting empty");
                    Vec::new()
                }
            },
            Err(_) => {
                tracing::debug!("no providers.json found, starting with empty provider store");
                Vec::new()
            }
        };

        Self {
            providers,
            file_path,
        }
    }

    /// Persist the current provider list to disk.
    pub async fn save(&self) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(&self.providers)?;
        tokio::fs::write(&self.file_path, json).await?;
        tracing::debug!(count = self.providers.len(), "saved cloud provider configs");
        Ok(())
    }

    /// Add a provider. Replaces any existing provider with the same name.
    pub fn add(&mut self, config: CloudProviderConfig) {
        self.providers.retain(|p| p.name != config.name);
        tracing::info!(
            provider = %config.name,
            kind = %config.kind,
            "adding cloud provider"
        );
        self.providers.push(config);
    }

    /// Remove a provider by name.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.providers.len();
        self.providers.retain(|p| p.name != name);
        let removed = self.providers.len() < before;
        if removed {
            tracing::info!(provider = %name, "removed cloud provider");
        }
        removed
    }

    /// Get a provider config by name.
    pub fn get(&self, name: &str) -> Option<&CloudProviderConfig> {
        self.providers.iter().find(|p| p.name == name)
    }

    /// All configured providers.
    pub fn all(&self) -> &[CloudProviderConfig] {
        &self.providers
    }

    /// Create `Offering` trait objects for all enabled providers.
    ///
    /// Each enabled provider becomes an adapter that reads its API key
    /// from the store at probe/proxy time. The adapters are always
    /// registered; they report unhealthy when no key is configured.
    pub fn create_offerings(&self) -> Vec<Arc<dyn Offering>> {
        let mut offerings: Vec<Arc<dyn Offering>> = Vec::new();
        let mut seen_kinds = std::collections::HashSet::new();

        for config in &self.providers {
            if !config.enabled {
                continue;
            }
            // One adapter per offering kind (not per provider name)
            if seen_kinds.contains(&config.kind) {
                continue;
            }

            match config.kind {
                OfferingKind::OpenAi => {
                    seen_kinds.insert(config.kind);
                    offerings.push(Arc::new(OpenAiProvider::new(config.clone())));
                }
                OfferingKind::Anthropic => {
                    seen_kinds.insert(config.kind);
                    offerings.push(Arc::new(AnthropicProvider::new(config.clone())));
                }
                other => {
                    tracing::warn!(
                        kind = %other,
                        "unsupported cloud provider kind, skipping"
                    );
                }
            }
        }

        offerings
    }
}
