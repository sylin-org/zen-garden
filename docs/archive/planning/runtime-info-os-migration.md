# Refactoring: OS Information Moved to RuntimeInfo

**Date**: 2026-01-25  
**Type**: Breaking Change (Data Structure)  
**Impact**: Backend only (API compatible via serialization)

---

## Summary

Moved OS and kernel version information from `HardwareInventory` to `RuntimeInfo` to properly separate **physical hardware** from **runtime environment**.

---

## Rationale

### Problem
`HardwareInventory` mixed physical hardware (CPU, memory, GPU) with runtime context (OS version, kernel version):

```rust
pub struct HardwareInventory {
    pub cpu: CpuCapabilities,     // Physical
    pub memory: MemoryCapabilities, // Physical
    pub gpus: Vec<GpuInfo>,       // Physical
    pub os_version: Option<String>,   // ❌ Runtime context
    pub kernel_version: Option<String>, // ❌ Runtime context
}
```

**Why this is wrong**:
- OS/kernel are **runtime environment**, not hardware components
- Violates Domain-Driven Design separation of concerns
- Confuses users when all info grouped under "HARDWARE" label
- Detection rules are now OS-specific (`detection.{os}`) - OS family should be first-class

### Solution
Use existing `RuntimeInfo` struct for OS information:

```rust
pub struct RuntimeInfo {
    pub docker_version: Option<String>,
    pub os: String,            // OS family + version: "windows/Windows 11 Pro"
    pub kernel: Option<String>, // Kernel version
}
```

---

## Changes

### 1. Data Structure

**Before**:
```rust
HardwareCapabilities {
    hardware: HardwareInventory {
        os_version: Some("Windows 11 Pro"),
        kernel_version: Some("10.0.22631"),
        // ...
    },
    runtime: Some(RuntimeInfo {
        os: "windows",  // Just OS family
        kernel: None,
    }),
}
```

**After**:
```rust
HardwareCapabilities {
    hardware: HardwareInventory {
        // os_version REMOVED
        // kernel_version REMOVED
        // Only physical hardware remains
    },
    runtime: Some(RuntimeInfo {
        os: "windows/Windows 11 Pro",  // Enhanced format
        kernel: Some("10.0.22631"),
    }),
}
```

### 2. RuntimeInfo.os Format

**New format**: `{os_family}[/{os_version}]`

Examples:
- `"windows"` → Just OS family (no version detected)
- `"windows/Windows 11 Pro"` → With version
- `"linux/Ubuntu 22.04.3 LTS"` → With version
- `"macos/macOS 14.2.1"` → With version

### 3. UI Changes

#### observe command
**Before**:
```
HARDWARE
    Architecture                        x86_64
    CPU Cores                           4 cores
    Memory                              7 GB
```

**After**:
```
ENVIRONMENT
    Operating System                    Windows 11 Pro
    Docker                              ✓ 24.0.7

HARDWARE
    Architecture                        x86_64
    CPU Cores                           4 cores
    Memory                              7 GB
```

#### status command
**Before**:
```
MEMORY              7 GB
OS                  Windows 11 Pro
KERNEL              10.0.22631
```

**After**:
```
MEMORY              7 GB
OS                  Windows 11 Pro
KERNEL              10.0.22631
```
(Same display, but reads from `runtime` instead of `hardware`)

---

## Implementation

### Files Modified

**Core Types** (garden-common):
- `src/common/src/types.rs`: Removed `os_version` and `kernel_version` from `HardwareInventory`

**Hardware Detection** (garden-moss):
- `src/moss/src/infra/hardware.rs`: Build OS string as `{family}/{version}`, populate `RuntimeInfo`
- `src/moss/src/tasks/hardware_detection.rs`: Update `RuntimeInfo` instead of `HardwareInventory`
- `src/moss/src/api/v1/garden.rs`: Populate `RuntimeInfo` in synchronous detection
- `src/moss/src/bootstrap/startup.rs`: Remove duplicate fields from skeleton
- `src/moss/src/domain/constraints.rs`: Remove duplicate fields from test fixtures

**UI Display** (garden-rake):
- `src/rake/src/commands/discovery/observe.rs`: 
  - Added ENVIRONMENT section
  - Parse `runtime.os` format
  - Display Docker availability
- `src/rake/src/commands/discovery/status.rs`: Read from `runtime` instead of `hardware`
- `src/rake/src/recommendation_tests.rs`: Remove duplicate fields from test fixtures

### Parsing Logic

```rust
// Extract display name from runtime.os
let os_display = if runtime.os.contains('/') {
    // "windows/Windows 11 Pro" → "Windows 11 Pro"
    let parts: Vec<&str> = runtime.os.split('/').collect();
    parts[1].to_string()
} else {
    // "windows" → "Windows"
    match runtime.os.as_str() {
        "windows" => "Windows".to_string(),
        "linux" => "Linux".to_string(),
        "macos" => "macOS".to_string(),
        other => other.to_string(),
    }
};
```

---

## Migration

### Backwards Compatibility

✅ **API Compatible**: JSON serialization unchanged (fields moved, not removed from response)
✅ **Cache Compatible**: Old cached capabilities will deserialize (missing `runtime` fields default to None)
⚠️ **Display Only**: UI changes visible immediately

### Migration Steps

1. **Users**: No action required
2. **Developers**: Update any code reading `hardware.os_version` → `runtime.os`
3. **Tests**: Update test fixtures to remove duplicate fields
4. **Cache**: Auto-refreshed on next hardware detection cycle

---

## Benefits

### 1. Conceptual Clarity
- **Hardware** = Physical components (CPU, RAM, GPU, disk)
- **Environment** = Runtime context (OS, kernel, Docker)
- **Runtime** = Software environment (Docker, OS version, kernel)

### 2. UI Improvements
```
ENVIRONMENT
    Operating System                    Windows 11 Pro
    Docker                              ✓ 24.0.7

HARDWARE
    Architecture                        x86_64
    CPU Cores                           4 cores
    Memory                              7 GB
    Storage                             59 GB SSD
```

**Why better**:
- Clear separation between what you install (environment) vs what you buy (hardware)
- Docker availability now prominently shown (critical for Windows+Docker detection spec)
- OS information matches manifest detection structure (`detection.{os}`)

### 3. Alignment with Detection System
Manifest structure:
```yaml
detection:
  windows:  # OS family matches RuntimeInfo.os
    - method: command
      config:
        command: ollama --version
```

Now `RuntimeInfo.os` directly matches manifest detection keys.

---

## Related

- [Windows Docker Adoption Spec](../specs/windows-docker-adoption-spec.md) - Uses RuntimeInfo for Docker availability
- [OS-Aware Detection v2](../guides/os-aware-detection-v2.md) - Detection rules grouped by OS
- [Container Namespace Collision](../decisions/OFFER-0002-container-namespace-collision.md) - Platform-specific behaviors

---

## Testing

### Verify Detection
```powershell
# Start Moss with fresh detection
rm F:\Replica\NAS\Files\repo\github\zen-garden\.zen-garden\cache\capabilities.json
cargo run --bin garden-moss

# Check output shows ENVIRONMENT section
cargo run --bin garden-rake -- observe
```

### Expected Output
```
stone-crystal-forest                        [thriving]

    ENVIRONMENT
        Operating System                    Windows 11 Pro
        Docker                              Not available

    HARDWARE
        Architecture                        x86_64
        CPU Cores                           4 cores
        Memory                              7 GB
```

---

## Summary

**What Changed**: OS version and kernel moved from `HardwareInventory` to `RuntimeInfo`  
**Why**: Separate physical hardware from runtime environment (DDD/SoC)  
**Impact**: UI now has distinct ENVIRONMENT and HARDWARE sections  
**Migration**: Automatic (backwards compatible serialization)  
**Benefit**: Clear separation, better UX, aligns with OS-specific detection structure
