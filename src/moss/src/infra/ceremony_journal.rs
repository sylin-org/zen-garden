//! Ceremony journal - persistent storage for crash recovery
//!
//! Stores ceremony state to disk so incomplete ceremonies can be
//! detected and handled on restart. Uses simple JSON files.

use crate::domain::ceremony::{Ceremony, CeremonyId};
use crate::domain::traits::CeremonyPersistence;
use anyhow::{Context, Result};
use std::path::PathBuf;

/// Persistent journal for ceremony state
///
/// Directory structure:
/// ```text
/// {base_dir}/
/// ├── active/          # Currently running ceremonies
/// │   └── {id}.json
/// └── archive/         # Completed ceremonies (for history)
///     └── {id}.json
/// ```
pub struct CeremonyJournal {
    base_dir: PathBuf,
}

impl CeremonyJournal {
    /// Create a new journal at the given directory
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// Create using default path from configuration
    pub fn default_journal() -> Self {
        Self::new(garden_common::constants::paths::ceremony_journal_dir())
    }

    /// Get path for an active ceremony
    fn active_path(&self, id: &CeremonyId) -> PathBuf {
        self.base_dir.join("active").join(format!("{}.json", id))
    }

    /// Get path for an archived ceremony
    fn archive_path(&self, id: &CeremonyId) -> PathBuf {
        self.base_dir.join("archive").join(format!("{}.json", id))
    }

    /// Persist ceremony to disk
    ///
    /// Active ceremonies go to active/, terminal ceremonies go to archive/.
    pub async fn persist(&self, ceremony: &Ceremony) -> Result<()> {
        let path = if ceremony.state.is_terminal() {
            self.archive_path(&ceremony.id)
        } else {
            self.active_path(&ceremony.id)
        };

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create journal directory")?;
        }

        let json =
            serde_json::to_string_pretty(ceremony).context("Failed to serialize ceremony")?;

        // Write and sync to disk to ensure durability (auto-flush)
        let file = tokio::fs::File::create(&path)
            .await
            .context("Failed to create journal file")?;
        let mut writer = tokio::io::BufWriter::new(file);
        tokio::io::AsyncWriteExt::write_all(&mut writer, json.as_bytes())
            .await
            .context("Failed to write ceremony journal")?;
        tokio::io::AsyncWriteExt::flush(&mut writer)
            .await
            .context("Failed to flush ceremony journal")?;
        // Sync to disk for durability
        writer
            .get_ref()
            .sync_all()
            .await
            .context("Failed to sync ceremony journal to disk")?;

        // If terminal, remove from active directory
        if ceremony.state.is_terminal() {
            let active_path = self.active_path(&ceremony.id);
            let _ = tokio::fs::remove_file(&active_path).await;
        }

        tracing::debug!(
            ceremony_id = %ceremony.id,
            terminal = ceremony.state.is_terminal(),
            "Persisted ceremony to journal"
        );

        Ok(())
    }

    /// Load all active (non-terminal) ceremonies
    ///
    /// Used on startup to detect incomplete ceremonies from previous run.
    pub async fn load_active(&self) -> Result<Vec<Ceremony>> {
        let active_dir = self.base_dir.join("active");

        if !active_dir.exists() {
            return Ok(Vec::new());
        }

        let mut ceremonies = Vec::new();
        let mut entries = tokio::fs::read_dir(&active_dir)
            .await
            .context("Failed to read active ceremonies directory")?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                match tokio::fs::read_to_string(&path).await {
                    Ok(json) => match serde_json::from_str::<Ceremony>(&json) {
                        Ok(ceremony) => ceremonies.push(ceremony),
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "Failed to parse ceremony file, skipping"
                            );
                        }
                    },
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "Failed to read ceremony file, skipping"
                        );
                    }
                }
            }
        }

        Ok(ceremonies)
    }

    /// Load a specific ceremony by ID (checks both active and archive)
    pub async fn load(&self, id: &CeremonyId) -> Result<Option<Ceremony>> {
        // Check active first
        let active_path = self.active_path(id);
        if active_path.exists() {
            let json = tokio::fs::read_to_string(&active_path).await?;
            return Ok(Some(serde_json::from_str(&json)?));
        }

        // Check archive
        let archive_path = self.archive_path(id);
        if archive_path.exists() {
            let json = tokio::fs::read_to_string(&archive_path).await?;
            return Ok(Some(serde_json::from_str(&json)?));
        }

        Ok(None)
    }

    /// Remove a ceremony from the journal
    pub async fn remove(&self, id: &CeremonyId) -> Result<()> {
        let active_path = self.active_path(id);
        let _ = tokio::fs::remove_file(&active_path).await;

        let archive_path = self.archive_path(id);
        let _ = tokio::fs::remove_file(&archive_path).await;

        Ok(())
    }

    /// Prune archived ceremonies older than the given duration
    pub async fn prune_archive(&self, older_than: chrono::Duration) -> Result<usize> {
        let archive_dir = self.base_dir.join("archive");
        let cutoff = chrono::Utc::now() - older_than;
        let mut pruned = 0;

        if !archive_dir.exists() {
            return Ok(0);
        }

        let mut entries = tokio::fs::read_dir(&archive_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Ok(json) = tokio::fs::read_to_string(&path).await
                && let Ok(ceremony) = serde_json::from_str::<Ceremony>(&json)
                && ceremony.completed_at.map(|t| t < cutoff).unwrap_or(false)
            {
                tokio::fs::remove_file(&path).await?;
                pruned += 1;
            }
        }

        if pruned > 0 {
            tracing::info!(count = pruned, "Pruned old ceremony records");
        }

        Ok(pruned)
    }
}

impl CeremonyPersistence for CeremonyJournal {
    async fn persist(&self, ceremony: &Ceremony) -> Result<()> {
        CeremonyJournal::persist(self, ceremony).await
    }

    async fn load_active(&self) -> Result<Vec<Ceremony>> {
        CeremonyJournal::load_active(self).await
    }

    async fn load(&self, id: &CeremonyId) -> Result<Option<Ceremony>> {
        CeremonyJournal::load(self, id).await
    }

    async fn remove(&self, id: &CeremonyId) -> Result<()> {
        CeremonyJournal::remove(self, id).await
    }

    async fn prune_archive(&self, older_than: chrono::Duration) -> Result<usize> {
        CeremonyJournal::prune_archive(self, older_than).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ceremony::{CeremonyInitiator, CeremonyOptions, CeremonyType};
    use tempfile::TempDir;

    fn test_ceremony() -> Ceremony {
        Ceremony::new(
            CeremonyType::NourishOffering {
                offering: "mongodb".to_string(),
            },
            "stone-01".to_string(),
            CeremonyInitiator {
                source: "test".to_string(),
                stone_id: None,
                command: None,
            },
            CeremonyOptions::default(),
        )
    }

    #[tokio::test]
    async fn test_journal_persist_active() {
        let temp_dir = TempDir::new().unwrap();
        let journal = CeremonyJournal::new(temp_dir.path());

        let ceremony = test_ceremony();
        journal.persist(&ceremony).await.unwrap();

        // Should be in active directory
        let active_path = journal.active_path(&ceremony.id);
        assert!(active_path.exists());
    }

    #[tokio::test]
    async fn test_journal_persist_terminal() {
        let temp_dir = TempDir::new().unwrap();
        let journal = CeremonyJournal::new(temp_dir.path());

        let mut ceremony = test_ceremony();
        journal.persist(&ceremony).await.unwrap();

        // Now complete it
        ceremony.complete();
        journal.persist(&ceremony).await.unwrap();

        // Should be in archive, not active
        let active_path = journal.active_path(&ceremony.id);
        let archive_path = journal.archive_path(&ceremony.id);
        assert!(!active_path.exists());
        assert!(archive_path.exists());
    }

    #[tokio::test]
    async fn test_journal_load_active() {
        let temp_dir = TempDir::new().unwrap();
        let journal = CeremonyJournal::new(temp_dir.path());

        let c1 = test_ceremony();
        let mut c2 = test_ceremony();
        c2.complete();

        journal.persist(&c1).await.unwrap();
        journal.persist(&c2).await.unwrap();

        // Only c1 should be in active
        let active = journal.load_active().await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, c1.id);
    }

    #[tokio::test]
    async fn test_journal_recovery_simulation() {
        let temp_dir = TempDir::new().unwrap();

        // First "run" - create ceremony
        {
            let journal = CeremonyJournal::new(temp_dir.path());
            let ceremony = test_ceremony();
            journal.persist(&ceremony).await.unwrap();
        }

        // Simulate crash - create new journal instance
        {
            let journal = CeremonyJournal::new(temp_dir.path());
            let active = journal.load_active().await.unwrap();
            assert_eq!(active.len(), 1, "Should recover incomplete ceremony");
        }
    }
}
