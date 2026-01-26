//! Update transaction logging for rollback safety
//!
//! Provides transaction log for Windows self-update process:
//! - Records each step of update
//! - Enables automatic rollback on failure
//! - Allows recovery from interrupted updates

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::{Context, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTransaction {
    pub version: u32,
    pub update_id: String,
    pub timestamp_start: String,
    pub timestamp_current: String,
    pub status: UpdateStatus,
    pub stage: UpdateStage,
    pub package_hash: String,
    pub package_version: String,
    pub old_version: String,
    pub is_service_mode: bool,
    pub steps_completed: Vec<UpdateStep>,
    pub backups_created: Vec<String>,
    pub error: Option<String>,
    pub timestamp_end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStatus {
    Started,
    InProgress,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStage {
    Started,
    WaitingForExit,
    OldProcessExited,
    BackingUp,
    BackupComplete,
    ValidatingBinaries,
    InstallingBinaries,
    InstallingManifests,
    CleanupStaging,
    Restarting,
    Verifying,
    Verified,
    RollingBack,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStep {
    pub stage: UpdateStage,
    pub timestamp: String,
}

impl UpdateTransaction {
    /// Create new transaction
    pub fn new(
        package_hash: String,
        package_version: String,
        old_version: String,
        is_service_mode: bool,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        let update_id = garden_common::utils::ids::generate_guidv7();
        
        Self {
            version: 1,
            update_id,
            timestamp_start: now.clone(),
            timestamp_current: now.clone(),
            status: UpdateStatus::Started,
            stage: UpdateStage::Started,
            package_hash,
            package_version,
            old_version,
            is_service_mode,
            steps_completed: vec![UpdateStep {
                stage: UpdateStage::Started,
                timestamp: now,
            }],
            backups_created: Vec::new(),
            error: None,
            timestamp_end: None,
        }
    }
    
    /// Advance to next stage
    pub fn advance_stage(&mut self, stage: UpdateStage) {
        let now = chrono::Utc::now().to_rfc3339();
        self.stage = stage.clone();
        self.timestamp_current = now.clone();
        self.steps_completed.push(UpdateStep {
            stage,
            timestamp: now,
        });
    }
    
    /// Mark as complete
    pub fn mark_complete(&mut self) {
        let now = chrono::Utc::now().to_rfc3339();
        self.status = UpdateStatus::Complete;
        self.stage = UpdateStage::Verified;
        self.timestamp_current = now.clone();
        self.timestamp_end = Some(now);
    }
    
    /// Mark as failed
    pub fn mark_failed(&mut self, error: String) {
        let now = chrono::Utc::now().to_rfc3339();
        self.status = UpdateStatus::Failed;
        self.error = Some(error);
        self.timestamp_current = now.clone();
        self.timestamp_end = Some(now);
    }
    
    /// Add backup file record
    pub fn add_backup(&mut self, filename: String) {
        self.backups_created.push(filename);
    }
    
    /// Get log file path
    fn log_path() -> PathBuf {
        let data_dir = garden_common::constants::paths::data_dir();
        PathBuf::from(data_dir).join("update.json")
    }
    
    /// Save transaction to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::log_path();
        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize transaction")?;
        std::fs::write(&path, json)
            .context("Failed to write transaction log")?;
        Ok(())
    }
    
    /// Load transaction from disk
    pub fn load() -> Result<Option<Self>> {
        let path = Self::log_path();
        if !path.exists() {
            return Ok(None);
        }
        
        let json = std::fs::read_to_string(&path)
            .context("Failed to read transaction log")?;
        let tx = serde_json::from_str(&json)
            .context("Failed to parse transaction log")?;
        Ok(Some(tx))
    }
    
    /// Remove transaction log
    pub fn remove() -> Result<()> {
        let path = Self::log_path();
        if path.exists() {
            std::fs::remove_file(&path)
                .context("Failed to remove transaction log")?;
        }
        Ok(())
    }
}
