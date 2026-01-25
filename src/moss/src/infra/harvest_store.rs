//! Harvest storage and retrieval
//!
//! Manages harvest manifests and their associated volume archives on disk.
//! Provides listing, loading, saving, and cleanup operations.

use crate::domain::harvest::{HarvestId, HarvestManifest};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Store for harvest manifests and archives
pub struct HarvestStore {
    base_dir: PathBuf,
}

impl HarvestStore {
    /// Create a new harvest store with the given base directory
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Create a harvest store using the default path from configuration
    pub fn default_store() -> Self {
        Self::new(garden_common::paths::harvest_dir())
    }

    /// Get the base directory
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Get path for a harvest directory
    pub fn harvest_path(&self, id: &HarvestId) -> PathBuf {
        self.base_dir.join(id)
    }

    /// Get manifest path for a harvest
    pub fn manifest_path(&self, id: &HarvestId) -> PathBuf {
        self.harvest_path(id).join("manifest.json")
    }

    /// Get volumes directory for a harvest
    pub fn volumes_path(&self, id: &HarvestId) -> PathBuf {
        self.harvest_path(id).join("volumes")
    }

    /// Ensure base directory exists
    pub async fn ensure_dir(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.base_dir)
            .await
            .context("Failed to create harvest store directory")?;
        Ok(())
    }

    /// Save harvest manifest
    pub async fn save_manifest(&self, manifest: &HarvestManifest) -> Result<()> {
        let path = self.manifest_path(&manifest.id);

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create harvest directory")?;
        }

        let json = serde_json::to_string_pretty(manifest).context("Failed to serialize manifest")?;

        tokio::fs::write(&path, json)
            .await
            .context("Failed to write manifest")?;

        tracing::debug!(
            harvest_id = %manifest.id,
            path = %path.display(),
            "Saved harvest manifest"
        );

        Ok(())
    }

    /// Load harvest manifest by ID
    pub async fn load_manifest(&self, id: &HarvestId) -> Result<HarvestManifest> {
        let path = self.manifest_path(id);
        let json = tokio::fs::read_to_string(&path)
            .await
            .context(format!("Failed to read manifest for harvest {}", id))?;

        serde_json::from_str(&json).context("Failed to parse manifest")
    }

    /// Check if a harvest exists
    pub async fn exists(&self, id: &HarvestId) -> bool {
        self.manifest_path(id).exists()
    }

    /// List all harvests
    pub async fn list_all(&self) -> Result<Vec<HarvestManifest>> {
        let mut manifests = Vec::new();

        if !self.base_dir.exists() {
            return Ok(manifests);
        }

        let mut entries = tokio::fs::read_dir(&self.base_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let id = entry.file_name().to_string_lossy().to_string();
                if let Ok(manifest) = self.load_manifest(&id).await {
                    manifests.push(manifest);
                }
            }
        }

        // Sort by creation time, newest first
        manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(manifests)
    }

    /// List harvests for a specific offering
    pub async fn list_for_offering(&self, offering: &str) -> Result<Vec<HarvestManifest>> {
        let all = self.list_all().await?;
        Ok(all.into_iter().filter(|m| m.offering == offering).collect())
    }

    /// Get the latest harvest for an offering (if any)
    pub async fn latest_for_offering(&self, offering: &str) -> Result<Option<HarvestManifest>> {
        let harvests = self.list_for_offering(offering).await?;
        Ok(harvests.into_iter().next()) // Already sorted newest first
    }

    /// Delete a harvest and all its archives
    pub async fn delete(&self, id: &HarvestId) -> Result<()> {
        let path = self.harvest_path(id);
        if path.exists() {
            tokio::fs::remove_dir_all(&path)
                .await
                .context(format!("Failed to delete harvest {}", id))?;

            tracing::info!(harvest_id = %id, "Deleted harvest");
        }
        Ok(())
    }

    /// Prune harvests older than the specified duration
    pub async fn prune(&self, older_than: chrono::Duration) -> Result<usize> {
        let cutoff = chrono::Utc::now() - older_than;
        let mut pruned = 0;

        for manifest in self.list_all().await? {
            if manifest.created_at < cutoff {
                self.delete(&manifest.id).await?;
                pruned += 1;
            }
        }

        if pruned > 0 {
            tracing::info!(count = pruned, "Pruned old harvests");
        }

        Ok(pruned)
    }

    /// Prune expired harvests (based on their expires_at field)
    pub async fn prune_expired(&self) -> Result<usize> {
        let mut pruned = 0;

        for manifest in self.list_all().await? {
            if manifest.is_expired() {
                self.delete(&manifest.id).await?;
                pruned += 1;
            }
        }

        if pruned > 0 {
            tracing::info!(count = pruned, "Pruned expired harvests");
        }

        Ok(pruned)
    }

    /// Get total storage used by harvests (in bytes)
    pub async fn total_size(&self) -> Result<u64> {
        let manifests = self.list_all().await?;
        Ok(manifests.iter().map(|m| m.total_size_bytes()).sum())
    }

    /// Get storage used by a specific offering's harvests
    pub async fn size_for_offering(&self, offering: &str) -> Result<u64> {
        let manifests = self.list_for_offering(offering).await?;
        Ok(manifests.iter().map(|m| m.total_size_bytes()).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_harvest_store_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let store = HarvestStore::new(temp_dir.path());

        let manifest = HarvestManifest::new("mongodb", "stone-01", "mongo:7.0.4");

        store.save_manifest(&manifest).await.unwrap();
        assert!(store.exists(&manifest.id).await);

        let loaded = store.load_manifest(&manifest.id).await.unwrap();
        assert_eq!(loaded.offering, "mongodb");
        assert_eq!(loaded.source_stone, "stone-01");
    }

    #[tokio::test]
    async fn test_harvest_store_list() {
        let temp_dir = TempDir::new().unwrap();
        let store = HarvestStore::new(temp_dir.path());

        // Create multiple harvests
        let m1 = HarvestManifest::new("mongodb", "stone-01", "mongo:7.0.4");
        let m2 = HarvestManifest::new("redis", "stone-01", "redis:7");
        let m3 = HarvestManifest::new("mongodb", "stone-02", "mongo:7.0.5");

        store.save_manifest(&m1).await.unwrap();
        store.save_manifest(&m2).await.unwrap();
        store.save_manifest(&m3).await.unwrap();

        let all = store.list_all().await.unwrap();
        assert_eq!(all.len(), 3);

        let mongodb = store.list_for_offering("mongodb").await.unwrap();
        assert_eq!(mongodb.len(), 2);
    }

    #[tokio::test]
    async fn test_harvest_store_delete() {
        let temp_dir = TempDir::new().unwrap();
        let store = HarvestStore::new(temp_dir.path());

        let manifest = HarvestManifest::new("test", "stone-01", "test:1");
        store.save_manifest(&manifest).await.unwrap();

        assert!(store.exists(&manifest.id).await);

        store.delete(&manifest.id).await.unwrap();

        assert!(!store.exists(&manifest.id).await);
    }
}
