//! Nurturing store - A/B slot persistence for local backups
//!
//! Manages the NurturingIndex which tracks A/B slots for each offering.
//! Works with HarvestStore for actual backup operations.
//!
//! # Storage Layout
//! ```text
//! {config_dir}/nurturing/
//!   index.json           <- NurturingIndex (slot assignments)
//!
//! {harvest_dir}/         <- Actual backup data (managed by HarvestStore)
//!   {harvest_id}/
//!     manifest.json
//!     volumes/
//!
//! On seed banks:
//! {mount_path}/garden/memories/
//!   index.json                           <- RemoteNurturingIndex
//!   {offering_id}/
//!     {harvest_id}.tar.gz                <- Compressed harvest archive
//! ```

use crate::docker::DockerManager;
use crate::domain::harvest::HarvestManifest;
use crate::domain::nurturing::{
    NurturingIndex, NurturingResult, NurturingSlot, NurturingSnapshot, OfferingSlots,
    RemoteNurturingIndex, RemoteSnapshot, ReplicationResult,
};
use crate::infra::storage::SeedBankStore;
use crate::infra::{create_harvest, HarvestStore};
use anyhow::{Context, Result};
use garden_common::constants::paths;
use garden_common::storage::MemoriesOfferingManifest;
use std::path::{Path, PathBuf};

/// Store for nurturing A/B slots
pub struct NurturingStore {
    /// Path to nurturing index
    index_path: PathBuf,
    /// Underlying harvest store for backup operations
    harvest_store: HarvestStore,
    /// Mutex to serialize index load/modify/save cycles (STORAGE-0006 fix)
    /// Without this, two concurrent snapshots can read the same index,
    /// modify independently, and overwrite each other's changes.
    index_lock: tokio::sync::Mutex<()>,
}

impl NurturingStore {
    /// Create a new nurturing store
    pub fn new(harvest_store: HarvestStore) -> Self {
        let config_dir = PathBuf::from(garden_common::constants::CONFIG_DIR);
        let index_path = config_dir.join("nurturing").join("index.json");

        Self {
            index_path,
            harvest_store,
            index_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// Create with default harvest store
    pub fn default_store() -> Self {
        Self::new(HarvestStore::default_store())
    }

    /// Load the nurturing index from disk
    pub async fn load_index(&self) -> Result<NurturingIndex> {
        if !self.index_path.exists() {
            return Ok(NurturingIndex::new());
        }

        let content = tokio::fs::read_to_string(&self.index_path)
            .await
            .context("Failed to read nurturing index")?;

        serde_json::from_str(&content).context("Failed to parse nurturing index")
    }

    /// Save the nurturing index to disk
    pub async fn save_index(&self, index: &NurturingIndex) -> Result<()> {
        if let Some(parent) = self.index_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create nurturing directory")?;
        }

        let content =
            serde_json::to_string_pretty(index).context("Failed to serialize nurturing index")?;

        // Atomic write
        let tmp_path = self.index_path.with_extension("tmp");
        tokio::fs::write(&tmp_path, &content)
            .await
            .context("Failed to write nurturing index")?;

        // Windows doesn't allow rename over existing file
        #[cfg(windows)]
        if self.index_path.exists() {
            let _ = tokio::fs::remove_file(&self.index_path).await;
        }

        tokio::fs::rename(&tmp_path, &self.index_path)
            .await
            .context("Failed to rename nurturing index")?;

        Ok(())
    }

    /// Get the harvest store
    pub fn harvest_store(&self) -> &HarvestStore {
        &self.harvest_store
    }

    /// Create a nurturing snapshot for an offering
    ///
    /// This performs the following:
    /// 1. Determines which slot (A or B) to use based on rotation
    /// 2. Creates a harvest (backup) of the offering
    /// 3. Updates the nurturing index with the new snapshot
    /// 4. Deletes the old harvest from the slot (if any)
    ///
    /// # Arguments
    /// * `docker` - Docker manager for container operations
    /// * `offering_id` - GUIDv7 identifier for the offering
    /// * `offering_name` - Current name of the offering
    /// * `stone_id` - Stone ID creating the snapshot
    /// * `commit_image` - Whether to commit the container image
    pub async fn create_snapshot(
        &self,
        docker: &DockerManager,
        offering_id: &str,
        offering_name: &str,
        stone_id: &str,
        commit_image: bool,
    ) -> Result<NurturingResult> {
        // STORAGE-0006: Hold index lock for the entire read-modify-save cycle.
        // The harvest creation (slow I/O) happens inside the lock — acceptable
        // because nurturing snapshots are already serialized per stone.
        let _index_guard = self.index_lock.lock().await;

        let mut index = self.load_index().await?;

        // Get or create slots for this offering
        let slots = index.get_or_create(offering_id, offering_name);

        // Determine which slot to use
        let target_slot = slots.next_slot();

        // Get the harvest ID to delete (if slot is occupied)
        let old_harvest_id = slots.get(target_slot).map(|s| s.harvest_id.clone());

        tracing::info!(
            offering_id,
            offering_name,
            slot = %target_slot,
            "Creating nurturing snapshot"
        );

        // Create the harvest
        let harvest = create_harvest(
            docker,
            &self.harvest_store,
            offering_name,
            stone_id,
            commit_image,
        )
        .await
        .context("Failed to create harvest for nurturing snapshot")?;

        // Create the snapshot
        let snapshot = NurturingSnapshot::from_harvest(&harvest, target_slot, offering_id, true);
        let harvest_id = harvest.id.clone();
        let size_bytes = harvest.total_size_bytes();

        // Update the index with the new snapshot
        // Need to re-get slots since we borrowed mutably earlier
        let slots = index.get_or_create(offering_id, offering_name);
        slots.set(target_slot, snapshot);

        // Save the updated index
        self.save_index(&index).await?;

        // Delete the old harvest if there was one
        let replaced_harvest_id = if let Some(old_id) = old_harvest_id {
            if let Err(e) = self.harvest_store.delete(&old_id).await {
                tracing::warn!(
                    harvest_id = %old_id,
                    error = ?e,
                    "Failed to delete old harvest (non-fatal)"
                );
            }
            Some(old_id)
        } else {
            None
        };

        tracing::info!(
            offering_id,
            offering_name,
            slot = %target_slot,
            harvest_id = %harvest_id,
            size = garden_common::utils::format_bytes(size_bytes),
            "Nurturing snapshot created"
        );

        Ok(NurturingResult {
            success: true,
            offering_id: offering_id.to_string(),
            offering_name: offering_name.to_string(),
            slot: target_slot,
            harvest_id,
            replaced_harvest_id,
            size_bytes,
            message: format!("Snapshot created in slot {}", target_slot),
        })
    }

    /// Get the slots for an offering
    pub async fn get_offering_slots(&self, offering_id: &str) -> Result<Option<OfferingSlots>> {
        let index = self.load_index().await?;
        Ok(index.get(offering_id).cloned())
    }

    /// List all offerings with nurturing snapshots
    pub async fn list_offerings(&self) -> Result<Vec<OfferingSlots>> {
        let index = self.load_index().await?;
        Ok(index
            .offerings
            .into_iter()
            .filter(|o| o.has_snapshots())
            .collect())
    }

    /// Restore an offering from a nurturing snapshot
    ///
    /// # Arguments
    /// * `offering_id` - GUIDv7 identifier for the offering
    /// * `slot` - Which slot to restore from (None = current/latest)
    pub async fn restore_snapshot(
        &self,
        docker: &DockerManager,
        offering_id: &str,
        slot: Option<NurturingSlot>,
    ) -> Result<HarvestManifest> {
        let index = self.load_index().await?;

        let slots = index.get(offering_id).ok_or_else(|| {
            anyhow::anyhow!("No nurturing slots found for offering {}", offering_id)
        })?;

        // Determine which snapshot to restore
        let snapshot = match slot {
            Some(s) => slots.get(s).ok_or_else(|| {
                anyhow::anyhow!("Slot {} is empty for offering {}", s, offering_id)
            })?,
            None => slots.current().ok_or_else(|| {
                anyhow::anyhow!("No current snapshot for offering {}", offering_id)
            })?,
        };

        tracing::info!(
            offering_id,
            slot = %snapshot.slot,
            harvest_id = %snapshot.harvest_id,
            "Restoring from nurturing snapshot"
        );

        // Load and restore the harvest
        let manifest = self
            .harvest_store
            .load_manifest(&snapshot.harvest_id)
            .await?;
        crate::infra::restore_harvest(docker, &self.harvest_store, &snapshot.harvest_id).await?;

        Ok(manifest)
    }

    /// Delete all nurturing data for an offering
    ///
    /// Removes both the index entry and all associated harvests.
    pub async fn delete_offering(&self, offering_id: &str) -> Result<()> {
        let _index_guard = self.index_lock.lock().await;
        let mut index = self.load_index().await?;

        if let Some(slots) = index.remove(offering_id) {
            // Delete all harvests
            for harvest_id in slots.harvest_ids() {
                if let Err(e) = self.harvest_store.delete(&harvest_id).await {
                    tracing::warn!(
                        harvest_id = %harvest_id,
                        error = ?e,
                        "Failed to delete harvest (non-fatal)"
                    );
                }
            }

            self.save_index(&index).await?;

            tracing::info!(offering_id, "Deleted nurturing data for offering");
        }

        Ok(())
    }

    /// Get total storage used by nurturing snapshots
    pub async fn total_size(&self) -> Result<u64> {
        let index = self.load_index().await?;
        Ok(index.total_size())
    }

    // ========================================================================
    // Remote Seed Bank Operations
    // ========================================================================

    /// Replicate a local nurturing snapshot to a seed bank
    ///
    /// Creates a compressed archive of the harvest and stores it on the seed bank.
    /// Updates the remote index to track the snapshot.
    ///
    /// # Arguments
    /// * `offering_id` - GUIDv7 identifier for the offering
    /// * `seed_bank_mount` - Mount path of the seed bank
    /// * `seed_bank_id` - ID of the seed bank
    /// * `seed_bank_name` - Name of the seed bank
    /// * `stone_id` - This stone's ID
    pub async fn replicate_to_seed_bank(
        &self,
        offering_id: &str,
        store: &SeedBankStore,
        seed_bank_id: &str,
        seed_bank_name: &str,
        stone_id: &str,
        hydration_manifest: Option<MemoriesOfferingManifest>,
    ) -> Result<ReplicationResult> {
        // Get local slots for this offering
        let index = self.load_index().await?;
        let slots = index.get(offering_id).ok_or_else(|| {
            anyhow::anyhow!("No local nurturing slots for offering {}", offering_id)
        })?;

        // Get current snapshot
        let snapshot = slots
            .current()
            .ok_or_else(|| anyhow::anyhow!("No current snapshot for offering {}", offering_id))?;

        let harvest_id = &snapshot.harvest_id;
        let offering_name = &snapshot.offering_name;

        tracing::info!(
            offering_id,
            offering_name,
            harvest_id,
            seed_bank = seed_bank_name,
            "Replicating nurturing snapshot to seed bank"
        );

        // Create archive of the harvest
        let harvest_path = self.harvest_store.harvest_path(harvest_id);
        let archive_data = self.create_harvest_archive(&harvest_path).await?;
        let size_bytes = archive_data.len() as u64;

        // Store on seed bank under garden/memories (through SeedBankStore chokepoint)
        let object_key = format!("{}/{}.tar.gz", offering_id, harvest_id);
        let archive_rel = memories_rel_path(&object_key);
        store
            .write(&archive_rel, &archive_data)
            .await
            .context("Failed to store snapshot on seed bank")?;

        // Store hydration manifest (offering definition + metadata)
        if let Some(manifest) = hydration_manifest {
            self.store_offering_manifest(store, &manifest)
                .await
                .context("Failed to store offering manifest on seed bank")?;
        }

        // Update remote index with retention enforcement
        let mut remote_index = self.load_remote_index(store, seed_bank_id).await?;
        let pruned = remote_index.add_with_retention(RemoteSnapshot {
            offering_id: offering_id.to_string(),
            offering_name: offering_name.to_string(),
            harvest_id: harvest_id.to_string(),
            seed_bank_id: seed_bank_id.to_string(),
            seed_bank_name: seed_bank_name.to_string(),
            source_stone: stone_id.to_string(),
            created_at: snapshot.created_at,
            size_bytes,
            object_key: object_key.clone(),
        });
        self.save_remote_index(store, &remote_index).await?;

        // Delete pruned snapshots (retention policy enforcement)
        for old_snapshot in &pruned {
            let old_rel = memories_rel_path(&old_snapshot.object_key);
            if let Err(e) = store.delete(&old_rel).await {
                tracing::warn!(
                    harvest_id = %old_snapshot.harvest_id,
                    error = ?e,
                    "Failed to delete pruned remote snapshot (non-fatal)"
                );
            } else {
                tracing::info!(
                    harvest_id = %old_snapshot.harvest_id,
                    offering_id,
                    "Pruned old snapshot (retention policy)"
                );
            }
        }

        let pruned_count = pruned.len();
        let pruned_harvest_ids: Vec<String> = pruned.iter().map(|s| s.harvest_id.clone()).collect();

        tracing::info!(
            offering_id,
            harvest_id,
            seed_bank = seed_bank_name,
            size = garden_common::utils::format_bytes(size_bytes),
            pruned_count,
            "Snapshot replicated to seed bank"
        );

        let message = if pruned_count > 0 {
            format!(
                "Replicated to {} ({}), pruned {} old snapshot(s)",
                seed_bank_name,
                garden_common::utils::format_bytes(size_bytes),
                pruned_count
            )
        } else {
            format!(
                "Replicated to {} ({})",
                seed_bank_name,
                garden_common::utils::format_bytes(size_bytes)
            )
        };

        Ok(ReplicationResult {
            success: true,
            offering_id: offering_id.to_string(),
            harvest_id: harvest_id.to_string(),
            seed_bank_id: seed_bank_id.to_string(),
            seed_bank_name: seed_bank_name.to_string(),
            size_bytes,
            pruned_harvest_ids,
            message,
        })
    }

    /// List remote snapshots on a seed bank
    pub async fn list_remote_snapshots(
        &self,
        store: &SeedBankStore,
        seed_bank_id: &str,
    ) -> Result<RemoteNurturingIndex> {
        self.load_remote_index(store, seed_bank_id).await
    }

    /// Restore from a remote seed bank snapshot
    ///
    /// Downloads the snapshot from the seed bank, extracts it, and restores.
    ///
    /// # Arguments
    /// * `docker` - Docker manager for container operations
    /// * `seed_bank_mount` - Mount path of the seed bank
    /// * `offering_id` - Offering to restore
    /// * `harvest_id` - Optional specific harvest (defaults to latest)
    pub async fn restore_from_seed_bank(
        &self,
        docker: &DockerManager,
        store: &SeedBankStore,
        seed_bank_id: &str,
        offering_id: &str,
        harvest_id: Option<&str>,
    ) -> Result<HarvestManifest> {
        let remote_index = self.load_remote_index(store, seed_bank_id).await?;

        // Find the snapshot
        let snapshot = if let Some(id) = harvest_id {
            remote_index
                .snapshots
                .iter()
                .find(|s| s.harvest_id == id && s.offering_id == offering_id)
                .ok_or_else(|| anyhow::anyhow!("Harvest {} not found on seed bank", id))?
        } else {
            // Get latest for this offering
            remote_index
                .get_for_offering(offering_id)
                .first()
                .copied()
                .ok_or_else(|| {
                    anyhow::anyhow!("No snapshots for offering {} on seed bank", offering_id)
                })?
        };

        tracing::info!(
            offering_id,
            harvest_id = %snapshot.harvest_id,
            seed_bank_id,
            "Restoring from remote seed bank snapshot"
        );

        // Download the archive (through SeedBankStore — decrypts if encrypted)
        let archive_rel = memories_rel_path(&snapshot.object_key);
        let archive_data = store
            .read(&archive_rel)
            .await
            .context("Failed to read snapshot from seed bank")?;

        // Extract to local harvest store
        let harvest_path = self.harvest_store.harvest_path(&snapshot.harvest_id);
        self.extract_harvest_archive(&harvest_path, &archive_data)
            .await?;

        // Load manifest and restore
        let manifest = self
            .harvest_store
            .load_manifest(&snapshot.harvest_id)
            .await?;
        crate::infra::restore_harvest(docker, &self.harvest_store, &snapshot.harvest_id).await?;

        tracing::info!(
            offering_id,
            harvest_id = %snapshot.harvest_id,
            "Restored from remote snapshot"
        );

        Ok(manifest)
    }

    /// Delete a remote snapshot from a seed bank
    pub async fn delete_remote_snapshot(
        &self,
        store: &SeedBankStore,
        seed_bank_id: &str,
        harvest_id: &str,
    ) -> Result<bool> {
        let mut remote_index = self.load_remote_index(store, seed_bank_id).await?;

        if let Some(snapshot) = remote_index.remove(harvest_id) {
            // Delete the object through SeedBankStore
            let archive_rel = memories_rel_path(&snapshot.object_key);
            let _ = store.delete(&archive_rel).await;

            // Save updated index
            self.save_remote_index(store, &remote_index).await?;

            tracing::info!(harvest_id, seed_bank_id, "Deleted remote snapshot");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // ========================================================================
    // Private helpers for remote operations
    // ========================================================================

    /// Load the remote nurturing index from a seed bank
    async fn load_remote_index(
        &self,
        store: &SeedBankStore,
        seed_bank_id: &str,
    ) -> Result<RemoteNurturingIndex> {
        let index_rel = memories_index_rel();
        if !store.exists(&index_rel).await {
            return Ok(RemoteNurturingIndex::new(seed_bank_id));
        }

        let json = store
            .read_string(&index_rel)
            .await
            .context("Failed to read remote nurturing index")?;
        serde_json::from_str(&json).context("Failed to parse remote nurturing index")
    }

    /// Save the remote nurturing index to a seed bank
    async fn save_remote_index(
        &self,
        store: &SeedBankStore,
        index: &RemoteNurturingIndex,
    ) -> Result<()> {
        let json =
            serde_json::to_string_pretty(index).context("Failed to serialize remote index")?;
        let index_rel = memories_index_rel();
        store
            .write_string(&index_rel, &json)
            .await
            .context("Failed to save remote nurturing index")?;

        Ok(())
    }

    async fn store_offering_manifest(
        &self,
        store: &SeedBankStore,
        manifest: &MemoriesOfferingManifest,
    ) -> Result<()> {
        let json = serde_json::to_string_pretty(manifest)
            .context("Failed to serialize offering manifest")?;
        let manifest_rel = Path::new(paths::SEED_BANK_MEMORIES_DIR)
            .join(&manifest.offering_id)
            .join(paths::SEED_BANK_MEMORIES_OFFERING_MANIFEST_FILE);
        store
            .write_string(&manifest_rel, &json)
            .await
            .context("Failed to write offering manifest")?;
        Ok(())
    }

    /// Create a compressed archive of a harvest directory and return its contents
    async fn create_harvest_archive(&self, harvest_path: &Path) -> Result<Vec<u8>> {
        // Use temporary file for archive (tar command needs file output)
        let temp_archive = harvest_path.with_extension("tar.gz.tmp");

        // Use existing Archiver from garden_common
        garden_common::infra::archive::create_archive(harvest_path, &temp_archive).await?;

        // Read archive contents
        let data = tokio::fs::read(&temp_archive)
            .await
            .context("Failed to read archive")?;

        // Clean up temp file
        let _ = tokio::fs::remove_file(&temp_archive).await;

        Ok(data)
    }

    /// Extract a harvest archive from data to a directory
    async fn extract_harvest_archive(&self, target_path: &Path, archive_data: &[u8]) -> Result<()> {
        // Write archive data to temp file (tar command needs file input)
        let temp_archive = target_path.with_extension("tar.gz.tmp");

        tokio::fs::write(&temp_archive, archive_data)
            .await
            .context("Failed to write archive to temp file")?;

        // Use existing Archiver from garden_common
        garden_common::infra::archive::extract_archive(&temp_archive, target_path).await?;

        // Clean up temp file
        let _ = tokio::fs::remove_file(&temp_archive).await;

        Ok(())
    }
}

// ========================================================================
// Seed Bank Memories Helpers (relative paths for SeedBankStore)
// ========================================================================

/// Relative path: `garden/memories/index.json`
fn memories_index_rel() -> PathBuf {
    Path::new(paths::SEED_BANK_MEMORIES_DIR).join(paths::SEED_BANK_MEMORIES_INDEX_FILE)
}

/// Relative path: `garden/memories/{object_key}`
fn memories_rel_path(object_key: &str) -> PathBuf {
    Path::new(paths::SEED_BANK_MEMORIES_DIR).join(object_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_index_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let harvest_store = HarvestStore::new(temp_dir.path().join("harvests"));

        let config_dir = temp_dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap();

        // Create store with custom path
        let store = NurturingStore {
            index_path: config_dir.join("nurturing").join("index.json"),
            harvest_store,
            index_lock: tokio::sync::Mutex::new(()),
        };

        // Initially empty
        let index = store.load_index().await.unwrap();
        assert!(index.offerings.is_empty());

        // Add an offering
        let mut index = NurturingIndex::new();
        let slots = index.get_or_create("test-offering-id", "test-offering");
        assert_eq!(slots.offering_id, "test-offering-id");

        // Save and reload
        store.save_index(&index).await.unwrap();
        let loaded = store.load_index().await.unwrap();
        assert_eq!(loaded.offerings.len(), 1);
        assert!(loaded.get("test-offering-id").is_some());
    }
}
