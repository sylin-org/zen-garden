# Windows Self-Update Implementation Plan
**Date**: 2026-01-26  
**Status**: ✅ **PROPOSAL APPROVED - READY FOR IMPLEMENTATION**

---

## Proposed Flow Evaluation

**User Proposal**:
```
1. Extract package to staging
2. Copy garden-moss.exe → garden-moss-temp.exe
3. Spawn: garden-moss-temp.exe --install-package
4. Original moss shuts down
5. Temp process waits for shutdown
6. Temp process: clean/copy manifests, replace executables
7. Temp process starts new garden-moss.exe
8. Temp process exits
```

### ✅ **VERDICT: VIABLE AND RECOMMENDED**

This is the correct approach for Windows self-update. It solves all critical issues:

1. ✅ **File locking**: Temp process can replace locked executables
2. ✅ **Clean separation**: Update logic isolated from daemon logic
3. ✅ **Existing pattern**: `finalize_service_update()` already implements steps 5-7
4. ✅ **No external deps**: Pure Rust, works with current service setup
5. ✅ **Works for both**: Service and standalone modes

---

## Architecture Comparison

### Linux (Current - Working)
```
API stages → /var/lib/zen-garden/staging/validated/
     ↓
Moss shuts down
     ↓
Systemd restarts
     ↓
ExecStartPre: garden-upgrade.sh (copies staged → /usr/local/bin)
     ↓
ExecStart: garden-moss (runs with NEW binaries)
```

### Windows (Proposed)
```
API stages → .zen-garden/staging/validated/
     ↓
Moss copies self → garden-moss-temp.exe
     ↓
Moss spawns: garden-moss-temp.exe --finalize-update
     ↓
Moss shuts down
     ↓
Temp process waits for exit (30s timeout)
     ↓
Temp process copies: staging → installation directory
     ↓
Temp process restarts: garden-moss.exe (service or standalone)
     ↓
Temp process exits + self-cleanup
```

**Key Difference**: Windows uses **separate updater process** instead of pre-start systemd script.

---

## Implementation Details

### 1. API Endpoint Modification

**File**: `src/moss/src/api/v1/stone.rs`  
**Function**: `deploy_stone_v1()`

**Current behavior** (lines 570-575):
```rust
if contains_moss {
    tracing::info!("Package contains garden-moss, initiating graceful shutdown for upgrade");
    state.shutdown_tx.notify_one();
    // Returns 202 Accepted
}
```

**New behavior**:
```rust
if contains_moss {
    tracing::info!("Package contains garden-moss, spawning updater process");
    
    #[cfg(target_os = "windows")]
    {
        spawn_windows_updater().await?;
        // Shutdown will be triggered by updater after spawn
    }
    
    #[cfg(target_os = "linux")]
    {
        // Linux: systemd handles via ExecStartPre scripts
        state.shutdown_tx.notify_one();
    }
}
```

### 2. New Function: `spawn_windows_updater()`

**File**: `src/moss/src/infra/service.rs` (new function)

```rust
#[cfg(target_os = "windows")]
pub async fn spawn_windows_updater() -> anyhow::Result<()> {
    use std::process::Command;
    use anyhow::Context;

    let current_exe = std::env::current_exe()
        .context("Failed to get current executable path")?;
    let exe_dir = current_exe.parent()
        .ok_or_else(|| anyhow::anyhow!("No parent directory"))?;
    
    let temp_updater = exe_dir.join("garden-moss-temp.exe");
    
    tracing::info!(
        source = ?current_exe,
        temp = ?temp_updater,
        "Copying self to temporary updater"
    );
    
    // Copy current executable to temp location
    std::fs::copy(&current_exe, &temp_updater)
        .context("Failed to copy executable to temp location")?;
    
    // Spawn updater process (detached, does not wait)
    tracing::info!("Spawning updater process: garden-moss-temp.exe --finalize-update");
    
    let _child = Command::new(&temp_updater)
        .arg("--finalize-update")
        .spawn()
        .context("Failed to spawn updater process")?;
    
    tracing::info!("Updater spawned successfully, triggering shutdown");
    
    Ok(())
}
```

**Key Details**:
- Copies current exe to `garden-moss-temp.exe` (not `.new` to avoid confusion)
- Spawns with `--finalize-update` flag (reuses existing CLI pattern)
- Returns immediately (doesn't wait for updater)
- Caller triggers shutdown after successful spawn

### 3. Update `finalize_service_update()`

**File**: `src/moss/src/infra/service.rs`  
**Current**: Only handles binary replacement  
**Needed**: Add manifest handling, add validation, add rollback

**Enhanced Implementation**:

```rust
#[cfg(target_os = "windows")]
pub async fn finalize_service_update() -> anyhow::Result<()> {
    use std::process::Command;
    use anyhow::Context;

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

    // Step 1: Wait for old process to exit (up to 30 seconds)
    println!();
    println!("⏳ Waiting for old Moss process to exit...");
    for attempt in 1..=60 {
        let output = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq garden-moss.exe"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.contains("garden-moss.exe") {
            println!("✓ Old process exited");
            break;
        }

        if attempt == 60 {
            eprintln!("✗ Timeout waiting for old process to exit");
            return Err(anyhow::anyhow!("Old process did not exit after 30s"));
        }

        if attempt % 10 == 0 {
            print!(".");
            use std::io::Write;
            std::io::stdout().flush().ok();
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
    
    println!();

    // Step 2: Backup current binaries (rollback safety)
    println!("💾 Creating backup of current binaries...");
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
                }
            }
        }
    }
    println!();

    // Step 3: Install new binaries
    println!("📥 Installing new binaries...");
    for entry in std::fs::read_dir(&validated_bin)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        
        if name.ends_with(".exe") {
            let target = exe_dir.join(&file_name);
            
            // Validate binary before copying (basic check: file size > 0)
            let metadata = std::fs::metadata(entry.path())?;
            if metadata.len() == 0 {
                eprintln!("✗ Invalid binary (size 0): {}", name);
                return Err(anyhow::anyhow!("Corrupt staged binary: {}", name));
            }
            
            std::fs::copy(entry.path(), &target)
                .with_context(|| format!("Failed to install {}", name))?;
            
            println!("  ✓ Installed: {}", name);
        }
    }
    println!();

    // Step 4: Handle manifests (if present in package)
    let manifests_src = format!("{}/manifests", validated_dir);
    if std::path::Path::new(&manifests_src).exists() {
        println!("📚 Installing manifests...");
        
        // Determine manifests target directory
        let manifests_target = if let Ok(dir) = std::env::var("GARDEN_MANIFESTS_DIR") {
            dir
        } else {
            // Default: .zen-garden/manifests (next to binaries)
            exe_dir.join(".zen-garden").join("manifests")
                .to_string_lossy().to_string()
        };
        
        // Remove old manifests, copy new ones
        if std::path::Path::new(&manifests_target).exists() {
            std::fs::remove_dir_all(&manifests_target)
                .context("Failed to remove old manifests")?;
        }
        
        // Recursive copy of manifests directory
        copy_dir_recursive(&manifests_src, &manifests_target)?;
        
        println!("  ✓ Manifests installed to: {}", manifests_target);
        println!();
    }

    // Step 5: Cleanup staging directory
    println!("🧹 Cleaning up staging directory...");
    std::fs::remove_dir_all(&staging_dir)
        .context("Failed to cleanup staging directory")?;
    println!("  ✓ Staging cleaned");
    println!();

    // Step 6: Restart service
    let is_service = std::env::var("RUNNING_AS_SERVICE").is_ok();

    if is_service {
        println!("🚀 Starting Moss service...");
        let output = Command::new("sc")
            .args(["start", "ZenGardenMoss"])
            .output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("⚠️  Service start failed: {}", stderr);
            return Err(anyhow::anyhow!("Failed to start service"));
        }
        
        println!("✓ Service started successfully");
    } else {
        println!("🚀 Launching new Moss...");
        Command::new(&target_exe)
            .arg("--cleanup-updater")  // New flag
            .spawn()
            .context("Failed to spawn new moss process")?;
        println!("✓ New Moss launched");
    }
    
    println!();
    println!("═══════════════════════════════════════════════");
    println!("✅ Update complete!");
    println!("═══════════════════════════════════════════════");
    println!();
    println!("This updater process will now exit.");
    
    // Self-cleanup: remove garden-moss-temp.exe
    // (Will fail because we're running it, but next boot cleanup will get it)
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        let _ = std::fs::remove_file(&current_exe);
    });

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

### 4. New CLI Flag: `--cleanup-updater`

**File**: `src/moss/src/cli.rs`

Add to `Cli` struct:
```rust
#[arg(long, hide = true)]
pub cleanup_updater: bool,
```

**File**: `src/moss/src/main.rs`

Add after existing `--cleanup-old` check:
```rust
#[cfg(target_os = "windows")]
if cli.cleanup_updater {
    return cleanup_updater_process().await;
}
```

**New function** in `src/moss/src/infra/service.rs`:
```rust
#[cfg(target_os = "windows")]
pub async fn cleanup_updater_process() -> anyhow::Result<()> {
    use std::process::Command;
    
    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe.parent()
        .ok_or_else(|| anyhow::anyhow!("No parent directory"))?;
    let temp_exe = exe_dir.join("garden-moss-temp.exe");
    
    if temp_exe.exists() {
        // Wait for updater process to exit
        for _ in 1..=40 {
            let output = Command::new("tasklist")
                .args(["/FI", "IMAGENAME eq garden-moss-temp.exe"])
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.contains("garden-moss-temp.exe") {
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // Remove temp updater
        std::fs::remove_file(&temp_exe).ok();
        tracing::info!("Cleaned up updater process");
    }
    
    // Continue with normal startup
    Ok(())
}
```

### 5. Update `deploy_stone_v1()` Integration

**File**: `src/moss/src/api/v1/stone.rs` (lines 570-594)

Replace:
```rust
if contains_moss {
    tracing::info!("Package contains garden-moss, initiating graceful shutdown for upgrade");
    state.shutdown_tx.notify_one();

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "accepted",
            "message": "Package validated and staged. Service restart initiated.",
            "staged_path": validated_dir,
            "sha256": actual_hash,
            "size": body.len(),
        })),
    )
}
```

With:
```rust
if contains_moss {
    tracing::info!("Package contains garden-moss, initiating upgrade sequence");
    
    #[cfg(target_os = "windows")]
    {
        use crate::infra::spawn_windows_updater;
        
        if let Err(e) = spawn_windows_updater().await {
            tracing::error!(error = ?e, "Failed to spawn updater");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "status": "error",
                    "message": "Failed to spawn updater process",
                    "error": format!("{}", e),
                })),
            );
        }
        
        // Shutdown will be triggered after updater spawns
        state.shutdown_tx.notify_one();
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        // Linux: systemd ExecStartPre handles binary installation
        state.shutdown_tx.notify_one();
    }

    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "accepted",
            "message": "Package validated and staged. Service restart initiated.",
            "staged_path": validated_dir,
            "sha256": actual_hash,
            "size": body.len(),
        })),
    )
}
```

---

## Error Handling & Edge Cases

### 1. Corrupt Staged Binary
**Detection**: Check file size > 0 before copying  
**Response**: Abort update, keep old binaries  
**Recovery**: `.old` backups remain for manual rollback

### 2. Updater Process Crashes Mid-Update
**Impact**: Old binaries replaced, manifests partially copied  
**Recovery**: `.old` backups can be renamed manually  
**Future**: Add transaction log for atomic multi-file updates

### 3. Old Process Won't Exit
**Timeout**: 30 seconds  
**Response**: Abort update with error  
**User Action**: Kill stuck process manually, re-deploy package

### 4. Service Start Fails After Update
**Detection**: `sc start ZenGardenMoss` returns error  
**Response**: Updater reports failure, exits  
**Recovery**: Check event logs, restore from `.old` backups if needed

### 5. Updater Process Never Exits
**Symptom**: `garden-moss-temp.exe` remains in process list  
**Cleanup**: Next moss startup runs `--cleanup-updater`  
**Prevention**: Updater self-deletes after 2s delay

### 6. Manifests Directory Mismatch
**Scenario**: `GARDEN_MANIFESTS_DIR` points to different location  
**Handling**: Updater respects env var, copies to correct location  
**Default**: `.zen-garden/manifests` (next to binaries)

---

## Testing Strategy

### Unit Tests
1. `spawn_windows_updater()` - mock file operations
2. `copy_dir_recursive()` - test nested directories
3. Binary validation - zero-byte file detection

### Integration Tests

**Test 1: Standalone Mode Update**
```powershell
# Setup
.\garden-moss.exe &  # Run in background
$pid = $LASTEXITCODE

# Deploy package
Invoke-RestMethod -Method POST `
    -Uri "http://localhost:7185/api/v1/stone:deploy" `
    -Headers @{"X-Package-SHA256"="<hash>"} `
    -InFile "package.zip"

# Wait for update sequence
Start-Sleep -Seconds 10

# Verify
if (Get-Process -Name "garden-moss-temp" -ErrorAction SilentlyContinue) {
    Write-Host "FAIL: Updater still running" -ForegroundColor Red
}

$newVersion = .\garden-moss.exe --version
if ($newVersion -ne "0.1.999999") {
    Write-Host "FAIL: Version not updated" -ForegroundColor Red
}

Write-Host "PASS: Standalone update successful" -ForegroundColor Green
```

**Test 2: Service Mode Update**
```powershell
# Install as service
.\garden-moss.exe install-service
Start-Sleep -Seconds 5

# Deploy package
Invoke-RestMethod -Method POST `
    -Uri "http://localhost:7185/api/v1/stone:deploy" `
    -Headers @{"X-Package-SHA256"="<hash>"} `
    -InFile "package.zip"

# Wait for update + restart
Start-Sleep -Seconds 15

# Verify service running
$status = sc query ZenGardenMoss
if ($status -notmatch "RUNNING") {
    Write-Host "FAIL: Service not running" -ForegroundColor Red
}

# Verify version
$newVersion = Invoke-RestMethod "http://localhost:7185/api/v1/stone/info"
if ($newVersion.version -ne "0.1.999999") {
    Write-Host "FAIL: Version not updated" -ForegroundColor Red
}

Write-Host "PASS: Service update successful" -ForegroundColor Green
```

**Test 3: Rollback from Backup**
```powershell
# Corrupt staged binary (simulate failure)
$staging = ".zen-garden\staging\validated\bin"
"" | Out-File "$staging\garden-moss.exe"  # Zero-byte file

# Attempt deploy (should fail)
Invoke-RestMethod -Method POST `
    -Uri "http://localhost:7185/api/v1/stone:deploy" `
    -Headers @{"X-Package-SHA256"="<hash>"} `
    -InFile "package.zip"

Start-Sleep -Seconds 5

# Verify old binary still works
if (Test-Path "garden-moss.exe.old") {
    Write-Host "PASS: Backup created" -ForegroundColor Green
} else {
    Write-Host "FAIL: No backup found" -ForegroundColor Red
}

# Manual rollback
Copy-Item "garden-moss.exe.old" "garden-moss.exe" -Force
```

---

## Implementation Checklist

### Phase 1: Core Updater Logic
- [ ] Add `spawn_windows_updater()` to `service.rs`
- [ ] Enhance `finalize_service_update()` with:
  - [ ] Wait for old process exit (30s timeout)
  - [ ] Backup current binaries (`.old` suffix)
  - [ ] Validate staged binaries (size check)
  - [ ] Copy binaries to installation directory
  - [ ] Handle manifests (recursive copy)
  - [ ] Cleanup staging directory
  - [ ] Restart service (sc start) or standalone
- [ ] Add `copy_dir_recursive()` helper function
- [ ] Add `--cleanup-updater` CLI flag
- [ ] Add `cleanup_updater_process()` function

### Phase 2: API Integration
- [ ] Update `deploy_stone_v1()` to call `spawn_windows_updater()`
- [ ] Add platform-specific shutdown logic (`#[cfg(target_os = "windows")]`)
- [ ] Export new functions from `infra/mod.rs`

### Phase 3: Testing
- [ ] Manual test: Standalone mode update
- [ ] Manual test: Service mode update
- [ ] Manual test: Rollback from `.old` backups
- [ ] Manual test: Corrupt binary detection
- [ ] Manual test: Updater process cleanup
- [ ] Integration test: Add to `tests/test-first-boot.ps1`

### Phase 4: Documentation
- [ ] Update [ARCHITECTURE-REFERENCE.md](../ARCHITECTURE-REFERENCE.md)
- [ ] Update [CHANGELOG.md](../CHANGELOG.md)
- [ ] Add Windows-specific update notes to README
- [ ] Document rollback procedure

---

## Timeline Estimate

| Task | Effort | Risk |
|------|--------|------|
| Implement `spawn_windows_updater()` | 1h | Low |
| Enhance `finalize_service_update()` | 3h | Medium |
| Add manifest handling | 1h | Low |
| API integration | 1h | Low |
| CLI flag + cleanup logic | 1h | Low |
| Manual testing (standalone) | 2h | - |
| Manual testing (service) | 2h | - |
| Error scenario testing | 2h | - |
| Documentation | 1h | - |
| **Total** | **14h** | **Medium** |

---

## Comparison to Alternative Approaches

### ❌ Startup Check in main.rs (Previous Option 1)
**Problem**: Windows locks running `.exe` files - can't replace self  
**Workaround**: Would still need temp process + spawn logic  
**Verdict**: Adds unnecessary complexity, not cleaner than updater process

### ❌ Windows Service Wrapper (Previous Option 2)
**Problem**: External dependency (NSSM), changes service installation  
**Verdict**: Overengineered, adds fragility

### ✅ Spawn Updater Process (Current Proposal)
**Advantages**:
- Solves file locking cleanly
- Isolated update logic
- Works for service + standalone
- No external dependencies
- Reuses existing `finalize_service_update()` code

**Disadvantages**:
- Two-process coordination (handled by existing pattern)
- Cleanup complexity (solved by `--cleanup-updater`)

---

## Future Enhancements

### 1. Transaction Log
Record each step of update process to `.zen-garden/update.log`:
```
2026-01-26 14:32:10 - UPDATE_START
2026-01-26 14:32:15 - BACKUP_COMPLETE
2026-01-26 14:32:18 - BINARIES_INSTALLED
2026-01-26 14:32:20 - MANIFESTS_INSTALLED
2026-01-26 14:32:22 - UPDATE_COMPLETE
```

Allows recovery from partial updates.

### 2. Automatic Rollback
If new binary fails to start, automatically restore from `.old` backups:
```rust
// In bootstrap startup logic
if update_failed_flag_exists() {
    restore_from_backups()?;
}
```

### 3. Pre-Update Validation
Before triggering update:
- Verify binary architecture (x64/arm64)
- Check free disk space
- Validate manifest schemas
- Test new binary in isolated environment

### 4. Progress Notifications
Stream update progress to SSE endpoint:
```
event: update_progress
data: {"stage": "waiting_for_exit", "percent": 20}

event: update_progress
data: {"stage": "installing_binaries", "percent": 60}
```

---

## Conclusion

✅ **The proposed flow is sound and implementable.**

Key strengths:
1. Solves Windows file locking elegantly
2. Reuses existing code patterns
3. No external dependencies
4. Works for both service and standalone modes
5. Provides rollback safety (`.old` backups)

Implementation is straightforward with clear testing path. Estimated 14 hours from start to fully tested + documented.

**Recommendation**: Proceed with implementation.
