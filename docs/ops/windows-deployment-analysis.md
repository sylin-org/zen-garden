# Windows Deployment Analysis - Critical Gaps Identified
**Date**: 2026-01-26  
**Status**: ⚠️ **INCOMPLETE IMPLEMENTATION**  
**Severity**: **HIGH** - Windows self-update non-functional

---

## Executive Summary

**FINDING**: Windows package deployment is **fundamentally broken** for self-update scenarios. While the API successfully stages binaries, there is **no mechanism to apply staged binaries after moss restarts**.

**Impact**: Windows stones cannot perform self-updates via HTTP API. Moss restarts from original location, staged binaries remain unused indefinitely.

**Root Cause**: Linux relies on systemd `ExecStartPre` scripts (`garden-upgrade.sh`, `moss-update-helper.sh`) to apply staged binaries before service startup. Windows has **no equivalent mechanism**.

---

## Component-by-Component Analysis

### 1. Package Structure ✅ **WORKING**

**Linux Package** (`zen-garden-0.1.202601260950-linux-amd64.tar.gz`):
```
zen-garden-0.1.202601260950-linux-amd64/
├── bin/
│   ├── garden-moss
│   ├── garden-rake
│   └── garden-lantern
├── scripts/
│   ├── moss-update-helper.sh      ← CRITICAL: Missing on Windows
│   └── garden-upgrade.sh           ← CRITICAL: Missing on Windows
├── manifests/
│   ├── hw/ (hardware detection manifests)
│   └── sw/ (software offerings)
└── package.json
```

**Windows Package** (`zen-garden-0.1.202601260950-windows-amd64.zip`):
```
zen-garden-0.1.202601260950-windows-amd64/
├── bin/
│   ├── garden-moss.exe
│   ├── garden-rake.exe
│   └── garden-lantern.exe
├── manifests/
│   ├── hw/
│   └── sw/
└── package.json (✅ correct: platform="windows", arch="amd64")
```

**Status**: Package structure correct, package.json metadata correct.  
**Gap**: Missing `scripts/` directory with Windows-equivalent startup scripts.

---

### 2. API Upload & Staging ✅ **WORKING**

**Endpoint**: `POST /api/v1/stone:deploy`  
**Headers**: `X-Package-SHA256: <hash>`  
**Body**: Raw bytes (`.tar.gz` or `.zip`)

**Flow**:
1. ✅ Receives package bytes
2. ✅ Validates SHA256 checksum
3. ✅ Extracts to `.zen-garden/staging/extract-<hash>/`
4. ✅ Parses `package.json`, validates platform match
5. ✅ Copies binaries to `.zen-garden/staging/validated/bin/`
6. ✅ Detects if package contains `garden-moss`
7. ✅ Triggers graceful shutdown (`state.shutdown_tx.notify_one()`)

**Location** (after recent path fix):
- Linux: `/var/lib/zen-garden/staging/validated/bin/`
- Windows: `.zen-garden/staging/validated/bin/` (relative to service working directory)

**Status**: ✅ API endpoint works perfectly on both platforms.  
**Code**: [`src/moss/src/api/v1/stone.rs:287-594`](../src/moss/src/api/v1/stone.rs)

---

### 3. Service Restart ⚠️ **PLATFORM DIVERGENCE**

#### Linux Workflow ✅ **COMPLETE**

**Systemd Unit File** (`garden-moss.service`):
```systemd
[Service]
Type=simple
ExecStartPre=/usr/local/bin/moss-update-helper.sh    ← Checks for staged binaries
ExecStartPre=/usr/local/bin/garden-upgrade.sh        ← Installs staged binaries
ExecStart=/usr/local/bin/garden-moss
Restart=always
RestartSec=5s
```

**Sequence**:
1. API stages binaries to `/var/lib/zen-garden/staging/validated/bin/`
2. Moss shuts down gracefully
3. Systemd triggers restart
4. **`moss-update-helper.sh` runs BEFORE moss starts**: Checks for staged binaries
5. **`garden-upgrade.sh` runs BEFORE moss starts**: 
   - Copies staged binaries to `/usr/local/bin/`
   - Sets executable permissions
   - Removes staging directory
6. Moss starts with new binaries

**Scripts**:
- [`installer/moss-update-helper.sh`](../../installer/moss-update-helper.sh) - Detection
- [`installer/garden-upgrade.sh`](../../installer/garden-upgrade.sh) - Installation

#### Windows Workflow ❌ **INCOMPLETE**

**Current Behavior**:
1. API stages binaries to `.zen-garden/staging/validated/bin/`
2. Moss shuts down gracefully
3. Windows Service Manager restarts `garden-moss.exe` from **original location**
4. **NO PRE-START SCRIPT RUNS**
5. Moss starts with **old binaries** (staged binaries never touched)

**What Exists**:
- [`src/moss/src/infra/service.rs`](../../src/moss/src/infra/service.rs):
  - `finalize_service_update()` - Called when running as `garden-moss-new.exe` after update
  - `cleanup_after_service_update()` - Removes old `garden-moss-new.exe` after successful update
  - **NEVER CALLED** in current workflow

**What's Missing**:
1. **No startup check** in `main.rs` to detect and apply staged binaries
2. **No Windows service wrapper** (like NSSM with pre-start hooks)
3. **No scheduled task** to check staging on boot
4. **No self-replacing logic** in moss startup path

**Status**: ❌ **CRITICAL GAP** - Self-update non-functional on Windows.

---

### 4. Push2All Deployment Script ⚠️ **WORKAROUND EXISTS**

**Script**: [`installer/push2all.ps1`](../../installer/push2all.ps1)  
**Methods**:
1. **HTTP API** (default): Calls `/api/v1/stone:deploy` with package
   - ✅ Works on Linux (systemd scripts apply staged binaries)
   - ❌ **BROKEN on Windows** (staged binaries never applied)

2. **SSH Method** (fallback): Direct file copy via `pscp`, service restart via `plink`
   - ✅ Works on both platforms (bypasses staging, direct replacement)
   - Requires SSH credentials (`stone:stone`)
   - **Current Windows workaround** until self-update fixed

**Menu Options**:
- Build binaries: Yes/No
- Deployment method: HTTP API / SSH
- Publish mode: Full Package / moss+rake / moss only

**Status**: SSH method provides working deployment path, but doesn't test self-update mechanism.

---

## Windows-Specific Code Inventory

### Files with Windows Logic

1. **[`src/moss/src/main.rs`](../../src/moss/src/main.rs)**:
   - CLI flags: `--update-finalize`, `--cleanup-old`
   - Commands: `take-root`, `install-service`
   - **Currently unused** for self-update

2. **[`src/moss/src/infra/service.rs`](../../src/moss/src/infra/service.rs)**:
   - `install_windows_service()` - Initial service setup (works)
   - `finalize_service_update()` - **Dead code** (never called)
   - `cleanup_after_service_update()` - **Dead code** (never called)

3. **[`src/moss/src/api/v1/stone.rs`](../../src/moss/src/api/v1/stone.rs)**:
   - `deploy_stone_v1()` - Package deployment (✅ works)
   - `upgrade_stone_v1()` - Individual binary staging (✅ works)
   - Shutdown trigger: `state.shutdown_tx.notify_one()` (✅ works)

4. **[`src/common/src/constants/paths.rs`](../../src/common/src/constants/paths.rs)**:
   - `data_dir()` - Returns `.zen-garden` on Windows (✅ correct)
   - `staging_dir()` - Returns `.zen-garden/staging` (✅ fixed today)

5. **[`src/moss/src/infra/manifests/sw.rs`](../../src/moss/src/infra/manifests/sw.rs)**:
   - `RUNTIME_TEMPLATES_DIR` - `.zen-garden/templates` (✅ fixed today)
   - Volumes base path - `.zen-garden/volumes` (✅ fixed today)

6. **[`src/moss/src/infra/manifests/hw.rs`](../../src/moss/src/infra/manifests/hw.rs)**:
   - `RUNTIME_HW_MANIFESTS_DIR` - `.zen-garden/hw-manifests` (✅ fixed today)

---

## Comparison Matrix

| Component | Linux Status | Windows Status | Gap Severity |
|-----------|-------------|----------------|--------------|
| Package structure | ✅ Complete | ⚠️ Missing scripts/ | Medium |
| API upload | ✅ Works | ✅ Works | None |
| SHA256 validation | ✅ Works | ✅ Works | None |
| Extraction | ✅ Works | ✅ Works | None |
| Platform validation | ✅ Works | ✅ Works | None |
| Binary staging | ✅ Works | ✅ Works | None |
| Shutdown trigger | ✅ Works | ✅ Works | None |
| **Pre-start check** | ✅ **systemd ExecStartPre** | ❌ **MISSING** | **CRITICAL** |
| **Binary installation** | ✅ **garden-upgrade.sh** | ❌ **MISSING** | **CRITICAL** |
| Service restart | ✅ Works | ✅ Works | None |
| New binary execution | ✅ Automatic | ❌ Uses old binary | **CRITICAL** |
| Push2all HTTP method | ✅ Works | ❌ Non-functional | **HIGH** |
| Push2all SSH method | ✅ Workaround | ✅ Workaround | None |

---

## Solution Options

### Option 1: Startup Check in main.rs (RECOMMENDED)
**Complexity**: Low  
**Risk**: Low  
**Implementation**:

Add to `src/moss/src/main.rs` before `run_daemon()`:

```rust
#[cfg(target_os = "windows")]
{
    // Check for staged binaries before starting daemon
    if let Err(e) = apply_staged_binaries().await {
        tracing::warn!(error = ?e, "Failed to apply staged binaries");
    }
}

async fn apply_staged_binaries() -> anyhow::Result<()> {
    let staging_dir = garden_common::constants::paths::staging_dir();
    let validated_bin = format!("{}/validated/bin", staging_dir);
    
    if !std::path::Path::new(&validated_bin).exists() {
        return Ok(()); // No staged binaries
    }
    
    tracing::info!("Staged binaries detected, applying update...");
    
    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe.parent().ok_or_else(|| anyhow::anyhow!("No parent dir"))?;
    
    // Copy each staged binary to installation directory
    for entry in std::fs::read_dir(&validated_bin)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let target = exe_dir.join(&file_name);
        
        // Write to .new first, then atomic rename
        let temp_target = exe_dir.join(format!("{}.new", file_name.to_string_lossy()));
        std::fs::copy(entry.path(), &temp_target)?;
        std::fs::rename(&temp_target, &target)?;
        
        tracing::info!(binary = ?file_name, "Installed updated binary");
    }
    
    // Remove staging directory
    std::fs::remove_dir_all(&staging_dir)?;
    tracing::info!("Staging cleanup complete");
    
    Ok(())
}
```

**Pros**:
- Simple, self-contained
- No external dependencies
- Aligns with Linux workflow (apply before startup)
- Works for both service and standalone execution

**Cons**:
- Windows locks running executables (can't replace self)
- Would need to spawn new process and exit

---

### Option 2: Windows Service Wrapper (NSSM/WinSW)
**Complexity**: Medium  
**Risk**: Medium  
**Implementation**:

Use NSSM (Non-Sucking Service Manager) with pre-start PowerShell script:

```powershell
# Pre-start script: apply-staged-binaries.ps1
$staging = ".zen-garden\staging\validated\bin"
if (Test-Path $staging) {
    Write-Host "Applying staged binaries..."
    Copy-Item "$staging\*" -Destination "." -Force
    Remove-Item ".zen-garden\staging" -Recurse -Force
}
```

**Pros**:
- Clean separation of concerns
- Mimics Linux systemd approach
- Can handle file locking better

**Cons**:
- External dependency (NSSM)
- Changes service installation process
- Additional complexity in installer

---

### Option 3: Self-Spawning Updater Process
**Complexity**: High  
**Risk**: High  
**Implementation**:

1. On shutdown, spawn separate updater process
2. Updater waits for moss to exit
3. Updater replaces binaries
4. Updater restarts service

(This is what `finalize_service_update()` attempts, but never gets called)

**Pros**:
- No external dependencies
- Can handle file locking

**Cons**:
- Complex process lifecycle management
- Race conditions
- Error handling complexity
- Existing implementation incomplete

---

### Option 4: Windows Task Scheduler (AVOID)
**Complexity**: Medium  
**Risk**: High  
**Implementation**:

Scheduled task runs every 5 minutes, checks staging, applies if found.

**Pros**:
- No code changes

**Cons**:
- Unreliable timing
- Not atomic with restart
- Poor user experience
- Task scheduler fragility

---

## Recommendation

**Implement Option 1: Startup Check in main.rs**

**Rationale**:
1. **Lowest risk**: Simple, testable, self-contained
2. **Best UX**: Immediate application of updates on restart
3. **Platform alignment**: Mirrors Linux pre-start behavior
4. **No external deps**: Works with current Windows service installation
5. **Works for both**: Service and standalone modes

**Refinement for Windows File Locking**:

Windows doesn't allow replacing a running `.exe`. Adjust strategy:

1. **If NOT running as service**: 
   - Apply staged binaries normally (no locks)
   - Continue startup with new binaries

2. **If running as service**:
   - Detect staged binaries
   - Copy new `garden-moss.exe` to `garden-moss-new.exe`
   - Spawn `garden-moss-new.exe --update-finalize`
   - Current process exits
   - New process waits for old service to die
   - New process replaces old binary
   - New process restarts service
   - (This uses existing `finalize_service_update()` code)

---

## Implementation Checklist

- [ ] Add `apply_staged_binaries()` function to `src/moss/src/main.rs`
- [ ] Handle Windows file locking (spawn-and-replace pattern)
- [ ] Test standalone mode (moss running in terminal)
- [ ] Test service mode (moss as Windows service)
- [ ] Test rollback scenario (corrupt staged binary)
- [ ] Update Windows package to include startup scripts (for future)
- [ ] Document Windows-specific update behavior
- [ ] Update push2all.ps1 to verify HTTP method after fix
- [ ] Add integration test for Windows self-update

---

## Testing Strategy

### Manual Test (Windows)

1. Build package: `installer\dist.ps1`
2. Install moss as service: `.\garden-moss.exe install-service`
3. Deploy new package: `Invoke-RestMethod -Method POST -Uri "http://localhost:7185/api/v1/stone:deploy" -Headers @{"X-Package-SHA256"="<hash>"} -InFile "package.zip"`
4. Verify staged binaries: `ls .zen-garden\staging\validated\bin\`
5. Wait for moss restart (automatic after API call)
6. **Expected**: New binaries in installation directory
7. **Currently**: Old binaries still running, staged binaries remain

### Automated Test

Add to `tests/test-first-boot.ps1`:

```powershell
# Test Windows self-update
$oldVersion = (.\garden-moss.exe --version)
$package = "zen-garden-0.1.999999-windows-amd64.zip"
Invoke-RestMethod -Method POST -Uri "http://localhost:7185/api/v1/stone:deploy" `
    -Headers @{"X-Package-SHA256"="<hash>"} -InFile $package
Start-Sleep -Seconds 10  # Wait for restart
$newVersion = (.\garden-moss.exe --version)
if ($newVersion -ne "0.1.999999") {
    Write-Host "FAIL: Version not updated" -ForegroundColor Red
    exit 1
}
```

---

## Timeline Estimate

| Task | Effort | Priority |
|------|--------|----------|
| Implement startup check logic | 4 hours | P0 |
| Handle Windows file locking | 2 hours | P0 |
| Manual testing (standalone) | 1 hour | P0 |
| Manual testing (service) | 2 hours | P0 |
| Integration test | 2 hours | P1 |
| Documentation | 1 hour | P1 |
| **Total** | **12 hours** | - |

---

## Appendix: Path Corrections Made Today

**Before** (hardcoded Windows paths):
```rust
// src/moss/src/api/v1/stone.rs
let staging_dir = "C:\\ProgramData\\ZenGarden\\staging";

// src/moss/src/infra/manifests/sw.rs
pub const RUNTIME_TEMPLATES_DIR: &str = "C:\\ProgramData\\ZenGarden\\templates";
let base = "C:\\ProgramData\\ZenGarden\\volumes";

// src/moss/src/infra/manifests/hw.rs
pub const RUNTIME_HW_MANIFESTS_DIR: &str = "C:\\ProgramData\\ZenGarden\\hw-manifests";
```

**After** (relative to service working directory):
```rust
// src/common/src/constants/paths.rs
pub fn staging_dir() -> String {
    std::env::var("GARDEN_STAGING_DIR").unwrap_or_else(|| {
        format!("{}/staging", data_dir())  // .zen-garden/staging
    })
}

// src/moss/src/api/v1/stone.rs
let staging_dir = garden_common::constants::paths::staging_dir();

// src/moss/src/infra/manifests/sw.rs
#[cfg(target_os = "windows")]
pub const RUNTIME_TEMPLATES_DIR: &str = ".zen-garden/templates";
let base = ".zen-garden/volumes";

// src/moss/src/infra/manifests/hw.rs
#[cfg(target_os = "windows")]
pub const RUNTIME_HW_MANIFESTS_DIR: &str = ".zen-garden/hw-manifests";
```

**Impact**: All Windows runtime state now resides in `.zen-garden/` alongside service binaries, consistent with project architecture.

---

## Appendix: Related Documentation

- [ARCHITECTURE-REFERENCE.md](../ARCHITECTURE-REFERENCE.md#operations) - SSH access to stones
- [api-v1.md](../specs/api-v1.md) - API endpoint specifications
- [COMM-0001-p2p-transport-singleton.md](../decisions/COMM-0001-p2p-transport-singleton.md) - P2P transport patterns
- [BUILD-0001-versioning.md](../decisions/BUILD-0001-versioning.md) - Version numbering

---

**Document Status**: Complete assessment  
**Next Action**: Implement Option 1 (startup check in main.rs)  
**Owner**: Development team  
**Reviewed**: 2026-01-26
