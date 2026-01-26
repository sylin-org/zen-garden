# Windows Self-Update: Refined Design
**Date**: 2026-01-26  
**Status**: ✅ **DESIGN FINALIZED - READY FOR IMPLEMENTATION**

---

## Key Design Decisions

### 1. Temp Process: `garden-moss-temp.exe` ✓

**Question**: Should temp process be `garden-moss-temp.exe` or `garden-moss.exe --install-package`?

**Answer**: **`garden-moss-temp.exe`** (separate binary copy)

**Rationale**:
- ✅ Avoids Windows file locking (can't run `garden-moss.exe --install-package` while original `garden-moss.exe` is shutting down)
- ✅ Clean process separation (no shared file handles)
- ✅ Simple cleanup (just delete `garden-moss-temp.exe` after success)

### 2. Replace Strategy: Always Replace ✓

**Approach**: Always replace binaries from staging, don't try to detect if update needed

**Rationale**:
- ✅ Simpler logic (no version comparison complexity)
- ✅ Idempotent (can re-run update safely)
- ✅ Handles partial updates (if previous attempt failed mid-way)
- ✅ User already validated package via API endpoint

### 3. Mode Preservation: Same as Original ✓

**Requirement**: Restart in same mode as original process

**Implementation**:
```rust
// Detect mode
let is_service = std::env::var("RUNNING_AS_SERVICE").is_ok();

// Restart accordingly
if is_service {
    Command::new("sc").args(["start", "ZenGardenMoss"]).spawn()?;
} else {
    Command::new("garden-moss.exe").spawn()?;
}
```

**Rationale**:
- ✅ Preserves user's execution context
- ✅ Service Manager sets `RUNNING_AS_SERVICE=1` automatically
- ✅ Standalone mode spawns new process directly

### 4. Safety Mechanisms: Transaction Log + Rollback ✓

**Key Improvements**:
1. **Keep old binary** - Backup to `.old` suffix before replacing
2. **Flag file** - Transaction log documents each step
3. **Verify before exit** - Temp process confirms new moss starts successfully
4. **Rollback on failure** - Restore old binary if update fails

---

## Enhanced Update Flow

### Phase 1: Package Staged (API Endpoint)
```
API receives package
  ↓
Validates SHA256
  ↓
Extracts to .zen-garden/staging/validated/
  ↓
Detects package contains garden-moss
  ↓
Copies garden-moss.exe → garden-moss-temp.exe
  ↓
Spawns: garden-moss-temp.exe --finalize-update
  ↓
Returns 202 Accepted
  ↓
Triggers graceful shutdown
```

### Phase 2: Updater Process Takes Over
```
garden-moss-temp.exe starts
  ↓
Creates transaction log: .zen-garden/update.json
  ↓
Writes: {"status": "started", "timestamp": "...", "stage": "waiting_for_exit"}
  ↓
Waits for garden-moss.exe to exit (30s timeout)
  ↓
Updates log: {"stage": "backing_up"}
  ↓
Backs up current binaries:
  - garden-moss.exe → garden-moss.exe.old
  - garden-rake.exe → garden-rake.exe.old
  ↓
Updates log: {"stage": "installing_binaries"}
  ↓
Validates staged binaries (size > 0, architecture)
  ↓
Copies staging → installation directory
  ↓
Updates log: {"stage": "installing_manifests"}
  ↓
Copies manifests (if present)
  ↓
Updates log: {"stage": "verifying"}
  ↓
Restarts moss (service or standalone)
  ↓
Waits 5s for new process to start
  ↓
Verifies new process is running (tasklist check)
  ↓
If SUCCESS:
    Updates log: {"status": "complete"}
    Removes staging directory
    Self-deletes garden-moss-temp.exe (async)
    Exits
  ↓
If FAILURE:
    Updates log: {"status": "failed", "error": "..."}
    Restores from .old backups
    Removes staging directory
    Exits with error code
```

### Phase 3: New Moss Starts
```
New garden-moss.exe starts
  ↓
Checks for update.json
  ↓
If status="complete":
    Removes .old backups
    Removes update.json
    Removes garden-moss-temp.exe (if still exists)
    Continues normal startup
  ↓
If status="failed":
    Logs error
    Alerts user (if running standalone)
    Continues with restored binaries
```

---

## Transaction Log Format

**File**: `.zen-garden/update.json`

**Structure**:
```json
{
  "version": 1,
  "update_id": "guidv7-...",
  "timestamp_start": "2026-01-26T14:32:10Z",
  "timestamp_current": "2026-01-26T14:32:18Z",
  "status": "in_progress",
  "stage": "installing_binaries",
  "package_hash": "sha256...",
  "package_version": "0.1.202601260950",
  "old_version": "0.1.202601260930",
  "is_service_mode": true,
  "steps_completed": [
    {"stage": "started", "timestamp": "2026-01-26T14:32:10Z"},
    {"stage": "waiting_for_exit", "timestamp": "2026-01-26T14:32:11Z"},
    {"stage": "old_process_exited", "timestamp": "2026-01-26T14:32:15Z"},
    {"stage": "backing_up", "timestamp": "2026-01-26T14:32:15Z"},
    {"stage": "backup_complete", "timestamp": "2026-01-26T14:32:16Z"},
    {"stage": "installing_binaries", "timestamp": "2026-01-26T14:32:18Z"}
  ],
  "backups_created": [
    "garden-moss.exe.old",
    "garden-rake.exe.old"
  ],
  "error": null
}
```

**On Success**:
```json
{
  "status": "complete",
  "stage": "verified",
  "timestamp_end": "2026-01-26T14:32:25Z"
}
```

**On Failure**:
```json
{
  "status": "failed",
  "stage": "verifying",
  "error": "New process failed to start after 5s",
  "rollback_performed": true,
  "timestamp_end": "2026-01-26T14:32:30Z"
}
```

---

## Code Implementation

### 1. Transaction Log Module

**File**: `src/moss/src/infra/update_transaction.rs` (new)

```rust
//! Update transaction logging for rollback safety

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
```

### 2. Enhanced `finalize_service_update()`

**File**: `src/moss/src/infra/service.rs`

```rust
#[cfg(target_os = "windows")]
pub async fn finalize_service_update() -> anyhow::Result<()> {
    use std::process::Command;
    use anyhow::Context;
    use crate::infra::update_transaction::{UpdateTransaction, UpdateStage};

    println!("═══════════════════════════════════════════════");
    println!(" Zen Garden Updater");
    println!("═══════════════════════════════════════════════");
    println!();
    println!("🔄 Finalizing Moss update...");
    println!();

    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe.parent()
        .ok_or_else(|| anyhow::anyhow!("No parent directory"))?;
    let target_exe = exe_dir.join("garden-moss.exe");
    
    // Locate staging directory
    let staging_dir = garden_common::constants::paths::staging_dir();
    let validated_dir = format!("{}/validated", staging_dir);
    let validated_bin = format!("{}/bin", validated_dir);
    
    if !std::path::Path::new(&validated_bin).exists() {
        println!("⚠️  No staged binaries found at {}", validated_bin);
        return Err(anyhow::anyhow!("Staging directory not found"));
    }
    
    println!("📦 Found staged package at: {}", validated_dir);
    
    // Read package metadata
    let package_json_path = format!("{}/../../package.json", validated_bin);
    let package_meta: serde_json::Value = if std::path::Path::new(&package_json_path).exists() {
        let content = std::fs::read_to_string(&package_json_path)?;
        serde_json::from_str(&content)?
    } else {
        serde_json::json!({})
    };
    
    let package_version = package_meta.get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let package_hash = package_meta.get("package_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    
    // Detect mode
    let is_service = std::env::var("RUNNING_AS_SERVICE").is_ok();
    
    // Get old version (if available)
    let old_version = if target_exe.exists() {
        // Try to read version from binary (placeholder - would need actual version extraction)
        "unknown".to_string()
    } else {
        "none".to_string()
    };
    
    // Create transaction log
    let mut tx = UpdateTransaction::new(
        package_hash,
        package_version.clone(),
        old_version,
        is_service,
    );
    tx.save()?;
    
    println!("📝 Transaction log created: {}", tx.update_id);
    println!();

    // STEP 1: Wait for old process to exit
    tx.advance_stage(UpdateStage::WaitingForExit);
    tx.save()?;
    
    println!("⏳ Waiting for old Moss process to exit...");
    let mut exited = false;
    for attempt in 1..=60 {
        let output = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq garden-moss.exe"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.contains("garden-moss.exe") {
            exited = true;
            println!("✓ Old process exited");
            break;
        }

        if attempt == 60 {
            let err = "Old process did not exit after 30s";
            eprintln!("✗ {}", err);
            tx.mark_failed(err.to_string());
            tx.save()?;
            return Err(anyhow::anyhow!(err));
        }

        if attempt % 10 == 0 {
            print!(".");
            use std::io::Write;
            std::io::stdout().flush().ok();
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
    
    if exited {
        tx.advance_stage(UpdateStage::OldProcessExited);
        tx.save()?;
    }
    println!();

    // STEP 2: Backup current binaries
    tx.advance_stage(UpdateStage::BackingUp);
    tx.save()?;
    
    println!("💾 Creating backups...");
    for entry in std::fs::read_dir(exe_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if let Some(name) = path.file_name() {
            let name_str = name.to_string_lossy();
            if name_str.starts_with("garden-") && name_str.ends_with(".exe") {
                let backup = exe_dir.join(format!("{}.old", name_str));
                if let Err(e) = std::fs::copy(&path, &backup) {
                    tracing::warn!(error = ?e, file = %name_str, "Failed to backup binary");
                } else {
                    println!("  ✓ Backed up: {}", name_str);
                    tx.add_backup(format!("{}.old", name_str));
                    tx.save()?;
                }
            }
        }
    }
    
    tx.advance_stage(UpdateStage::BackupComplete);
    tx.save()?;
    println!();

    // STEP 3: Validate staged binaries
    tx.advance_stage(UpdateStage::ValidatingBinaries);
    tx.save()?;
    
    println!("🔍 Validating staged binaries...");
    for entry in std::fs::read_dir(&validated_bin)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        
        if name.ends_with(".exe") {
            let metadata = std::fs::metadata(entry.path())?;
            if metadata.len() == 0 {
                let err = format!("Invalid binary (size 0): {}", name);
                eprintln!("✗ {}", err);
                tx.mark_failed(err);
                tx.save()?;
                
                // ROLLBACK
                rollback_from_backups(exe_dir, &tx).await?;
                return Err(anyhow::anyhow!("Corrupt staged binary: {}", name));
            }
            println!("  ✓ Validated: {}", name);
        }
    }
    println!();

    // STEP 4: Install new binaries
    tx.advance_stage(UpdateStage::InstallingBinaries);
    tx.save()?;
    
    println!("📥 Installing new binaries...");
    for entry in std::fs::read_dir(&validated_bin)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        
        if name.ends_with(".exe") {
            let target = exe_dir.join(&file_name);
            
            if let Err(e) = std::fs::copy(entry.path(), &target) {
                let err = format!("Failed to install {}: {}", name, e);
                eprintln!("✗ {}", err);
                tx.mark_failed(err);
                tx.save()?;
                
                // ROLLBACK
                rollback_from_backups(exe_dir, &tx).await?;
                return Err(anyhow::anyhow!("Installation failed for {}", name));
            }
            
            println!("  ✓ Installed: {}", name);
        }
    }
    println!();

    // STEP 5: Install manifests (if present)
    let manifests_src = format!("{}/manifests", validated_dir);
    if std::path::Path::new(&manifests_src).exists() {
        tx.advance_stage(UpdateStage::InstallingManifests);
        tx.save()?;
        
        println!("📚 Installing manifests...");
        
        let manifests_target = if let Ok(dir) = std::env::var("GARDEN_MANIFESTS_DIR") {
            dir
        } else {
            exe_dir.join(".zen-garden").join("manifests")
                .to_string_lossy().to_string()
        };
        
        if std::path::Path::new(&manifests_target).exists() {
            std::fs::remove_dir_all(&manifests_target)
                .context("Failed to remove old manifests")?;
        }
        
        copy_dir_recursive(&manifests_src, &manifests_target)?;
        
        println!("  ✓ Manifests installed to: {}", manifests_target);
        println!();
    }

    // STEP 6: Cleanup staging
    tx.advance_stage(UpdateStage::CleanupStaging);
    tx.save()?;
    
    println!("🧹 Cleaning up staging...");
    std::fs::remove_dir_all(&staging_dir)
        .context("Failed to cleanup staging directory")?;
    println!("  ✓ Staging cleaned");
    println!();

    // STEP 7: Restart moss
    tx.advance_stage(UpdateStage::Restarting);
    tx.save()?;
    
    println!("🚀 Restarting Moss...");
    
    if is_service {
        let output = Command::new("sc")
            .args(["start", "ZenGardenMoss"])
            .output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let err = format!("Service start failed: {}", stderr);
            eprintln!("✗ {}", err);
            tx.mark_failed(err);
            tx.save()?;
            
            // ROLLBACK
            rollback_from_backups(exe_dir, &tx).await?;
            return Err(anyhow::anyhow!("Failed to start service"));
        }
        
        println!("  ✓ Service start triggered");
    } else {
        Command::new(&target_exe)
            .arg("--cleanup-updater")
            .spawn()
            .context("Failed to spawn new moss process")?;
        println!("  ✓ New Moss launched");
    }
    println!();

    // STEP 8: Verify new process started
    tx.advance_stage(UpdateStage::Verifying);
    tx.save()?;
    
    println!("🔍 Verifying new process...");
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    
    let output = Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq garden-moss.exe"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains("garden-moss.exe") {
        let err = "New process failed to start after 5s";
        eprintln!("✗ {}", err);
        tx.mark_failed(err.to_string());
        tx.save()?;
        
        // ROLLBACK
        rollback_from_backups(exe_dir, &tx).await?;
        return Err(anyhow::anyhow!(err));
    }
    
    println!("  ✓ New process is running");
    println!();

    // SUCCESS!
    tx.mark_complete();
    tx.save()?;
    
    println!("═══════════════════════════════════════════════");
    println!("✅ Update complete!");
    println!("═══════════════════════════════════════════════");
    println!();
    println!("Updated to version: {}", package_version);
    println!("Transaction ID: {}", tx.update_id);
    println!();
    println!("This updater process will now exit.");
    println!("The new Moss will cleanup .old backups and this temp file.");
    
    // Self-cleanup (async, best effort)
    let current_exe_clone = current_exe.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let _ = std::fs::remove_file(&current_exe_clone);
    });

    Ok(())
}

/// Rollback from backups after failed update
#[cfg(target_os = "windows")]
async fn rollback_from_backups(
    exe_dir: &std::path::Path,
    tx: &crate::infra::update_transaction::UpdateTransaction,
) -> anyhow::Result<()> {
    use crate::infra::update_transaction::UpdateStage;
    
    println!();
    println!("⚠️  ROLLING BACK FROM BACKUPS");
    println!();
    
    let mut tx_rollback = tx.clone();
    tx_rollback.advance_stage(UpdateStage::RollingBack);
    tx_rollback.save()?;
    
    for backup_name in &tx.backups_created {
        let backup_path = exe_dir.join(backup_name);
        let target_name = backup_name.trim_end_matches(".old");
        let target_path = exe_dir.join(target_name);
        
        if backup_path.exists() {
            std::fs::copy(&backup_path, &target_path)?;
            println!("  ✓ Restored: {}", target_name);
        }
    }
    
    tx_rollback.advance_stage(UpdateStage::RolledBack);
    tx_rollback.save()?;
    
    println!();
    println!("✓ Rollback complete - old binaries restored");
    
    Ok(())
}

/// Helper: Recursive directory copy
#[cfg(target_os = "windows")]
fn copy_dir_recursive(src: &str, dst: &str) -> anyhow::Result<()> {
    use std::path::Path;
    
    std::fs::create_dir_all(dst)?;
    
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let target = Path::new(dst).join(&file_name);
        
        if path.is_dir() {
            copy_dir_recursive(
                &path.to_string_lossy(),
                &target.to_string_lossy()
            )?;
        } else {
            std::fs::copy(&path, &target)?;
        }
    }
    
    Ok(())
}
```

### 3. Startup Cleanup Handler

**File**: `src/moss/src/bootstrap/mod.rs` (add to startup sequence)

```rust
/// Check for completed/failed updates on startup
#[cfg(target_os = "windows")]
pub async fn handle_update_recovery() -> Result<()> {
    use crate::infra::update_transaction::{UpdateTransaction, UpdateStatus};
    
    if let Some(tx) = UpdateTransaction::load()? {
        match tx.status {
            UpdateStatus::Complete => {
                tracing::info!(
                    update_id = %tx.update_id,
                    version = %tx.package_version,
                    "Previous update completed successfully, cleaning up"
                );
                
                // Remove .old backups
                let exe_dir = std::env::current_exe()?.parent()
                    .ok_or_else(|| anyhow::anyhow!("No parent dir"))?;
                
                for backup_name in &tx.backups_created {
                    let backup_path = exe_dir.join(backup_name);
                    if backup_path.exists() {
                        std::fs::remove_file(&backup_path).ok();
                        tracing::debug!(file = %backup_name, "Removed backup file");
                    }
                }
                
                // Remove temp updater (if exists)
                let temp_updater = exe_dir.join("garden-moss-temp.exe");
                if temp_updater.exists() {
                    std::fs::remove_file(&temp_updater).ok();
                    tracing::debug!("Removed temp updater");
                }
                
                // Remove transaction log
                UpdateTransaction::remove()?;
                tracing::info!("Update cleanup complete");
            }
            
            UpdateStatus::Failed => {
                tracing::error!(
                    update_id = %tx.update_id,
                    error = ?tx.error,
                    stage = ?tx.stage,
                    "Previous update failed - manual intervention may be required"
                );
                
                // Keep transaction log for diagnostics
                // Keep .old backups for manual rollback
                // Alert operator
                eprintln!("\n⚠️  WARNING: Previous update failed at stage: {:?}", tx.stage);
                if let Some(err) = &tx.error {
                    eprintln!("Error: {}", err);
                }
                eprintln!("Transaction log: .zen-garden/update.json");
                eprintln!("Backups available: {}\n", tx.backups_created.join(", "));
            }
            
            UpdateStatus::Started | UpdateStatus::InProgress => {
                tracing::warn!(
                    update_id = %tx.update_id,
                    stage = ?tx.stage,
                    "Previous update was interrupted - attempting recovery"
                );
                
                // Update may have been interrupted (power loss, crash)
                // Keep transaction log for diagnostics
                // Old binaries should still be running (we're here!)
                eprintln!("\n⚠️  WARNING: Previous update was interrupted at stage: {:?}", tx.stage);
                eprintln!("System recovered using old binaries.");
                eprintln!("Transaction log: .zen-garden/update.json\n");
            }
        }
    }
    
    Ok(())
}
```

---

## Summary of Refinements

| Aspect | Original Design | Refined Design |
|--------|----------------|----------------|
| **Temp process name** | `garden-moss-new.exe` | `garden-moss-temp.exe` (clearer purpose) |
| **CLI flag** | `--update-finalize` | `--finalize-update` (matches pattern) |
| **Replace strategy** | "If needed" check | Always replace (simpler, idempotent) |
| **Mode detection** | Manual check | Preserve original mode via `RUNNING_AS_SERVICE` |
| **Safety** | Basic backups | Transaction log + rollback + verification |
| **Verification** | None | Wait 5s, check tasklist for new process |
| **Cleanup** | Manual | Automatic on next startup |
| **Rollback** | Manual only | Automatic on failure |
| **Recovery** | None | Startup checks transaction log, handles failed/interrupted updates |

---

## Key Safety Features

1. ✅ **Transaction Log** - Complete audit trail, enables recovery
2. ✅ **Automatic Backups** - All binaries backed up to `.old` before replacement
3. ✅ **Validation** - Staged binaries checked (size > 0) before installation
4. ✅ **Verification** - New process confirmed running before temp exits
5. ✅ **Rollback** - Automatic restore from backups if update fails
6. ✅ **Recovery** - Startup handler cleans up after success/failure/interruption
7. ✅ **Self-Healing** - Failed updates don't brick the system

---

## Testing Checklist

- [ ] Success path: Package deploys, moss restarts with new binaries
- [ ] Old process timeout: Simulate stuck process (should abort)
- [ ] Corrupt binary: Zero-byte staged file (should rollback)
- [ ] Service start failure: Mock `sc start` error (should rollback)
- [ ] Interrupted update: Kill updater mid-process (should recover on next startup)
- [ ] Transaction log persistence: Verify log survives across restarts
- [ ] Backup cleanup: Verify `.old` files removed after successful update
- [ ] Temp updater cleanup: Verify `garden-moss-temp.exe` removed
- [ ] Mode preservation: Test service mode vs standalone mode

---

**Status**: Design complete, ready for implementation.  
**Estimated effort**: 18 hours (14h + 4h for transaction log module)
