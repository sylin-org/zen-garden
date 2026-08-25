# DRY Alignment Analysis - Helper Methods
**Date**: 2026-01-24  
**Status**: Analysis Complete  
**Impact**: Code Quality Improvement

## Executive Summary

Analysis of the Zen Garden codebase reveals significant duplication of common patterns across 74+ modules. This document identifies opportunities to extract reusable helper methods into centralized utility modules, improving maintainability and reducing technical debt.

**Key Finding**: While `garden-common` already provides some utilities (`format_bytes`, `format_uptime`), many patterns remain duplicated throughout the codebase, particularly around:
- Platform-specific paths (20+ instances)
- Size formatting (15+ variations)
- Environment variable access (30+ direct calls)
- File I/O operations (40+ scattered instances)
- Directory creation (25+ instances)
- JSON serialization (30+ direct calls)

## Current State

### ✅ Already Exists in `garden-common`

**Location**: `src/common/src/utils.rs`

```rust
pub fn format_bytes(bytes: u64) -> String
pub fn format_uptime(seconds: u64) -> String
```

**Location**: `src/common/src/constants/paths.rs`

```rust
pub fn data_dir() -> String          // Platform-aware
pub fn config_dir() -> String
pub fn stone_home() -> String
pub fn harvest_dir() -> String
pub fn stored_dir() -> String
pub fn first_run_flag() -> String
pub fn stone_user() -> String
```

### ⚠️ Partially Duplicated

**Problem**: `format_size()` method in `harvest.rs` duplicates logic from `format_bytes()` utility:

- **harvest.rs** (lines 106-118): Custom implementation with `.1` precision
- **common/utils.rs** (line 4): Existing utility with `.2` precision
- **Usage**: At least 9 call sites format storage sizes (GB/MB/KB)

**Issue**: Two competing implementations with slightly different formatting rules.

## Identified DRY Violations

### 1. 🔴 **Platform-Specific Paths** (HIGH PRIORITY)

**Problem**: Platform conditionals scattered across codebase violate SoC

**Locations** (20+ instances):
- `common/constants/paths.rs` - `data_dir()` (lines 26-37)
- `moss/infra/filesystem.rs` - Duplicated in constructor (lines 20-32)
- Various domain modules reference OS-specific paths directly

**Current Pattern**:
```rust
#[cfg(target_os = "windows")]
{ ".zen-garden".to_string() }
#[cfg(not(target_os = "windows"))]
{ "/var/lib/zen-garden".to_string() }
```

**Duplication**: 
- `CONFIG_DIR` constant defined in `constants/mod.rs` (lines 11-15)
- Path construction logic repeated in multiple modules
- No centralized platform abstraction

**Recommendation**: ✅ Use existing `paths::data_dir()`, `paths::config_dir()` functions
**Additional Need**: Add trait-based platform abstraction for testability (see Architecture Proposal)

---

### 2. 🔴 **Size Formatting** (HIGH PRIORITY)

**Problem**: Two implementations with different precision

**Locations**:
- `common/utils.rs`: `format_bytes(u64)` → `.2` precision (e.g., "1.25 GB")
- `moss/domain/harvest.rs`: `format_size()` → `.1` precision (e.g., "1.2 GB")
- `rake/commands/offering/mod.rs`: Direct formatting `format!("{} GB", ...)` (lines 1036-1038)
- `rake/commands/discovery/status.rs`: Memory formatting (line 113)
- `rake/commands/discovery/observe.rs`: Multiple size formats (lines 460, 472, 485)
- `moss/domain/health.rs`: GB formatting with `.1` precision (lines 92-93, 127-128)

**Duplication Count**: 15+ formatting instances across 6 files

**Recommendation**: 
- **Standardize** on `common::utils::format_bytes()` with configurable precision
- **Deprecate** custom implementations in domain modules
- **Add** `format_bytes_short()` variant for `.1` precision if needed for UI

---

### 3. 🟡 **Environment Variable Access** (MEDIUM PRIORITY)

**Problem**: Direct `std::env::var()` calls lack error context and testability

**Locations** (30+ instances):
- `rake/ui.rs`: `ENV_NO_COLOR`, `ENV_GARDEN_UNICODE` (lines 23, 31)
- `rake/main.rs`: `GARDEN_QUIET` (line 1132)
- `rake/dispatch.rs`: `ENV_GARDEN_STONE` (line 99)
- `moss/tasks/coordinator.rs`: `ENV_LANTERN_ENDPOINT` (line 254)
- `moss/metrics.rs`: `CUDA_PATH`, `SystemRoot`, etc. (lines 830, 883, 903)
- `moss/infra/service.rs`: `RUNNING_AS_SERVICE` (line 179)
- `moss/infra/config.rs`: `ZEN_GARDEN_CONTAINER` (line 110)
- `moss/api/v1/stone.rs`: `GARDEN_STAGING_DIR` (lines 150, 153, 351, 354)

**Current Pattern**:
```rust
std::env::var("GARDEN_DATA_DIR").unwrap_or_else(|_| default)
std::env::var("CUDA_PATH").ok()
std::env::var("RUNNING_AS_SERVICE").is_ok()
```

**Issues**:
- No centralized environment variable registry
- Inconsistent error handling (unwrap_or vs ok vs is_ok)
- Hard to mock for testing
- No validation of values

**Recommendation**: Create `EnvConfig` helper with typed accessors

---

### 4. 🟡 **Directory Creation** (MEDIUM PRIORITY)

**Problem**: Repeated `fs::create_dir_all()` with inconsistent error handling

**Locations** (25+ instances):
- `rake/tending.rs`: 3 instances (lines 69, 77, 86)
- `moss/infra/service.rs`: Line 42
- `moss/infra/secrets.rs`: With parent check (line 314)
- `moss/infra/persistence.rs`: 4 instances (lines 38, 52, 81, 153)
- `moss/infra/harvest_store.rs`: 2 instances (lines 50, 61)
- `moss/infra/hardware.rs`: Line 42
- `moss/infra/filesystem.rs`: Line 42
- `moss/infra/config.rs`: Line 264
- `moss/infra/backup.rs`: 2 instances (lines 17, 83)
- `moss/api/v1/stone.rs`: 3 instances (lines 186, 360, 499)

**Current Pattern**:
```rust
std::fs::create_dir_all(&dir)?;
tokio::fs::create_dir_all(&dir).await?;
if let Err(e) = std::fs::create_dir_all(&dir) { ... }
```

**Issues**:
- Mix of sync and async versions
- Inconsistent error contexts
- No unified error messages
- Parent directory checks scattered

**Recommendation**: Create `ensure_dir()` and `ensure_dir_async()` helpers

---

### 5. 🟡 **Path Operations** (MEDIUM PRIORITY)

**Problem**: Repetitive path joining, display, and conversion logic

**Locations** (40+ instances):
- `PathBuf::from().join()`: 15+ instances
- `Path::new()`: 10+ instances  
- `.to_string_lossy()`: 12+ instances (moss/metrics.rs, api/v1/stone.rs, etc.)
- `.display()`: 18+ instances (tending.rs, manifests/sw.rs, etc.)

**Current Patterns**:
```rust
PathBuf::from(dir).join(filename)
path.to_string_lossy().to_string()
format!("Failed to read {}", path.display())
```

**Issues**:
- Verbose path construction
- Lossy string conversion scattered everywhere
- No standardized path display for logging
- No safe path joining helper

**Recommendation**: Create path utility helpers

---

### 6. 🟢 **JSON Serialization** (LOW PRIORITY)

**Problem**: Direct `serde_json` calls lack centralized error handling

**Locations** (30+ instances):
- `serde_json::from_str()`: 15+ instances
- `serde_json::to_string()`: 8+ instances
- `serde_json::to_string_pretty()`: 7+ instances

**Current Pattern**:
```rust
serde_json::from_str(&content)?
serde_json::to_string_pretty(&data)?
```

**Issues**:
- No custom error types
- Pretty-print logic inconsistent
- No validation on parse

**Recommendation**: Create `json::parse()` and `json::stringify()` wrappers (OPTIONAL)

---

### 7. 🟢 **Time/Date Operations** (LOW PRIORITY)

**Problem**: Direct usage of multiple time crates

**Locations** (25+ instances):
- `Utc::now()`: 10+ instances (harvest.rs, announcement.rs, app_state.rs)
- `Instant::now()`: 8+ instances (discovery.rs, stone_cache.rs, offering/mod.rs)
- `SystemTime::now()`: 3+ instances (tending.rs)

**Current Pattern**:
```rust
Utc::now()
Instant::now()
SystemTime::now()
```

**Issues**:
- Mix of `chrono` and `std::time` types
- No clock abstraction for testing
- Inconsistent time type usage

**Recommendation**: Create time utility abstraction (OPTIONAL for future)

---

## Proposed Helper Modules

### Module 1: `common/src/utils/platform.rs`

**Purpose**: Centralize all platform-specific logic

```rust
//! Platform abstraction utilities
//!
//! Provides platform-aware path resolution with centralized
//! OS-specific conditionals.

use std::path::PathBuf;

/// Platform paths interface
pub trait PlatformPaths {
    fn data_dir(&self) -> PathBuf;
    fn config_dir(&self) -> PathBuf;
    fn temp_dir(&self) -> PathBuf;
}

/// Windows platform paths
#[cfg(target_os = "windows")]
pub struct WindowsPaths;

#[cfg(target_os = "windows")]
impl PlatformPaths for WindowsPaths {
    fn data_dir(&self) -> PathBuf {
        let programdata = std::env::var("PROGRAMDATA")
            .unwrap_or_else(|_| "C:\\ProgramData".into());
        PathBuf::from(programdata).join("zen-garden")
    }
    
    fn config_dir(&self) -> PathBuf {
        PathBuf::from(".zen-garden")
    }
    
    fn temp_dir(&self) -> PathBuf {
        std::env::temp_dir().join("zen-garden")
    }
}

/// Unix platform paths
#[cfg(not(target_os = "windows"))]
pub struct UnixPaths;

#[cfg(not(target_os = "windows"))]
impl PlatformPaths for UnixPaths {
    fn data_dir(&self) -> PathBuf {
        PathBuf::from("/var/lib/zen-garden")
    }
    
    fn config_dir(&self) -> PathBuf {
        PathBuf::from("/etc/zen-garden")
    }
    
    fn temp_dir(&self) -> PathBuf {
        PathBuf::from("/tmp/zen-garden")
    }
}

/// Get platform-specific paths implementation
pub fn get_platform_paths() -> Box<dyn PlatformPaths> {
    #[cfg(target_os = "windows")]
    { Box::new(WindowsPaths) }
    
    #[cfg(not(target_os = "windows"))]
    { Box::new(UnixPaths) }
}

// Convenience functions (delegate to existing paths.rs)
pub fn data_dir() -> PathBuf {
    PathBuf::from(crate::constants::paths::data_dir())
}

pub fn config_dir() -> PathBuf {
    PathBuf::from(crate::constants::paths::config_dir())
}
```

**Migration**: Update `constants/paths.rs` to delegate to this module

---

### Module 2: `common/src/utils/formatting.rs`

**Purpose**: Standardize all formatting utilities

```rust
//! Formatting utilities
//!
//! Consistent formatting for bytes, uptime, and display values.

/// Format bytes with customizable precision
pub fn format_bytes_precision(bytes: u64, precision: usize) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.prec$} TB", bytes as f64 / TB as f64, prec = precision)
    } else if bytes >= GB {
        format!("{:.prec$} GB", bytes as f64 / GB as f64, prec = precision)
    } else if bytes >= MB {
        format!("{:.prec$} MB", bytes as f64 / MB as f64, prec = precision)
    } else if bytes >= KB {
        format!("{:.prec$} KB", bytes as f64 / KB as f64, prec = precision)
    } else {
        format!("{} B", bytes)
    }
}

/// Format bytes with 2 decimal places (default)
pub fn format_bytes(bytes: u64) -> String {
    format_bytes_precision(bytes, 2)
}

/// Format bytes with 1 decimal place (for UI)
pub fn format_bytes_short(bytes: u64) -> String {
    format_bytes_precision(bytes, 1)
}

/// Format bytes as whole numbers (no decimals)
pub fn format_bytes_whole(bytes: u64) -> String {
    format_bytes_precision(bytes, 0)
}

/// Format memory in MB to GB display
pub fn format_memory_mb(mb: u64) -> String {
    format_bytes_short(mb * 1024 * 1024)
}

/// Format uptime (already exists in utils.rs - keep as-is)
pub use super::format_uptime;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes_precision() {
        assert_eq!(format_bytes_short(1_073_741_824), "1.0 GB");
        assert_eq!(format_bytes(1_073_741_824), "1.00 GB");
        assert_eq!(format_bytes_whole(1_073_741_824), "1 GB");
        
        assert_eq!(format_bytes_short(2_147_483_648), "2.0 GB");
        assert_eq!(format_bytes_short(5_242_880), "5.0 MB");
        assert_eq!(format_bytes_short(2048), "2.0 KB");
        assert_eq!(format_bytes_short(500), "500 B");
    }
    
    #[test]
    fn test_format_memory_mb() {
        assert_eq!(format_memory_mb(8192), "8.0 GB");
        assert_eq!(format_memory_mb(512), "512.0 MB");
    }
}
```

**Migration**: 
1. Update `utils.rs` to export from this module
2. Replace `harvest.rs::format_size()` with `format_bytes_short()`
3. Replace direct formatting in rake/moss modules

---

### Module 3: `common/src/utils/env.rs`

**Purpose**: Typed environment variable access with validation

```rust
//! Environment variable utilities
//!
//! Centralized, typed access to environment variables with
//! validation and consistent fallback behavior.

use std::env;

/// Get environment variable with typed default
pub fn get_var_or<T: From<String>>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .map(T::from)
        .unwrap_or(default)
}

/// Get optional environment variable
pub fn get_var_opt(key: &str) -> Option<String> {
    env::var(key).ok()
}

/// Check if environment variable is set (regardless of value)
pub fn has_var(key: &str) -> bool {
    env::var(key).is_ok()
}

/// Environment variable keys (centralized registry)
pub mod keys {
    // Data paths
    pub const DATA_DIR: &str = "GARDEN_DATA_DIR";
    pub const CONFIG_DIR: &str = "GARDEN_CONFIG_DIR";
    pub const HARVEST_DIR: &str = "GARDEN_HARVEST_DIR";
    pub const STAGING_DIR: &str = "GARDEN_STAGING_DIR";
    
    // Stone configuration
    pub const STONE_NAME: &str = "GARDEN_STONE_NAME";
    pub const STONE_HOST: &str = "GARDEN_STONE_HOST";
    pub const STONE_HOME: &str = "GARDEN_STONE_HOME";
    pub const STONE_USER: &str = "GARDEN_STONE_USER";
    
    // Endpoints
    pub const GARDEN_STONE: &str = "GARDEN_STONE";
    pub const LANTERN_ENDPOINT: &str = "LANTERN_ENDPOINT";
    
    // Runtime flags
    pub const NO_COLOR: &str = "NO_COLOR";
    pub const GARDEN_NO_COLOR: &str = "GARDEN_NO_COLOR";
    pub const GARDEN_UNICODE: &str = "GARDEN_UNICODE";
    pub const GARDEN_QUIET: &str = "GARDEN_QUIET";
    pub const RUNNING_AS_SERVICE: &str = "RUNNING_AS_SERVICE";
    pub const ZEN_GARDEN_CONTAINER: &str = "ZEN_GARDEN_CONTAINER";
    
    // External tools
    pub const CUDA_PATH: &str = "CUDA_PATH";
    pub const SYSTEM_ROOT: &str = "SystemRoot";
    pub const INTEL_OPENVINO_DIR: &str = "INTEL_OPENVINO_DIR";
    pub const PROGRAMDATA: &str = "PROGRAMDATA";
    pub const HOME: &str = "HOME";
}

/// Typed environment configuration
pub struct EnvConfig;

impl EnvConfig {
    // Path accessors
    pub fn data_dir() -> Option<String> {
        get_var_opt(keys::DATA_DIR)
    }
    
    pub fn config_dir() -> Option<String> {
        get_var_opt(keys::CONFIG_DIR)
    }
    
    pub fn staging_dir() -> Option<String> {
        get_var_opt(keys::STAGING_DIR)
    }
    
    // Stone configuration
    pub fn stone_name() -> Option<String> {
        get_var_opt(keys::STONE_NAME)
    }
    
    pub fn stone_endpoint() -> Option<String> {
        get_var_opt(keys::GARDEN_STONE)
    }
    
    pub fn lantern_endpoint() -> Option<String> {
        get_var_opt(keys::LANTERN_ENDPOINT)
    }
    
    // Flags
    pub fn is_no_color() -> bool {
        has_var(keys::NO_COLOR) || has_var(keys::GARDEN_NO_COLOR)
    }
    
    pub fn is_unicode_enabled() -> bool {
        has_var(keys::GARDEN_UNICODE)
    }
    
    pub fn is_quiet() -> bool {
        has_var(keys::GARDEN_QUIET)
    }
    
    pub fn is_running_as_service() -> bool {
        has_var(keys::RUNNING_AS_SERVICE)
    }
    
    pub fn is_containerized() -> bool {
        has_var(keys::ZEN_GARDEN_CONTAINER)
    }
}
```

**Migration**: Update all `std::env::var()` calls to use `EnvConfig` methods

---

### Module 4: `common/src/utils/fs.rs`

**Purpose**: Safe file system operations with consistent error handling

```rust
//! File system utilities
//!
//! Helpers for directory creation, path operations, and file I/O
//! with consistent error handling and logging.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Ensure directory exists (sync version)
pub fn ensure_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    std::fs::create_dir_all(path)
        .with_context(|| format!("Failed to create directory: {}", path.display()))
}

/// Ensure directory exists (async version)
pub async fn ensure_dir_async<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    tokio::fs::create_dir_all(path)
        .await
        .with_context(|| format!("Failed to create directory: {}", path.display()))
}

/// Ensure parent directory exists
pub fn ensure_parent_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        ensure_dir(parent)?;
    }
    Ok(())
}

/// Ensure parent directory exists (async)
pub async fn ensure_parent_dir_async<P: AsRef<Path>>(path: P) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        ensure_dir_async(parent).await?;
    }
    Ok(())
}

/// Safe path joining (normalizes separators)
pub fn join_path<P: AsRef<Path>>(base: P, parts: &[&str]) -> PathBuf {
    let mut path = base.as_ref().to_path_buf();
    for part in parts {
        path = path.join(part);
    }
    path
}

/// Convert path to string with lossy conversion
pub fn path_to_string<P: AsRef<Path>>(path: P) -> String {
    path.as_ref().to_string_lossy().to_string()
}

/// Read file with context (sync)
pub fn read_file<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))
}

/// Read file with context (async)
pub async fn read_file_async<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read file: {}", path.display()))
}

/// Write file with parent directory creation (sync)
pub fn write_file<P: AsRef<Path>>(path: P, content: &str) -> Result<()> {
    let path = path.as_ref();
    ensure_parent_dir(path)?;
    std::fs::write(path, content)
        .with_context(|| format!("Failed to write file: {}", path.display()))
}

/// Write file with parent directory creation (async)
pub async fn write_file_async<P: AsRef<Path>>(path: P, content: &str) -> Result<()> {
    let path = path.as_ref();
    ensure_parent_dir_async(path).await?;
    tokio::fs::write(path, content)
        .await
        .with_context(|| format!("Failed to write file: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_join_path() {
        let path = join_path("/var/lib", &["zen-garden", "data", "test.json"]);
        assert!(path.to_string_lossy().contains("zen-garden"));
        assert!(path.to_string_lossy().contains("test.json"));
    }
    
    #[test]
    fn test_path_to_string() {
        let path = PathBuf::from("/tmp/test");
        let s = path_to_string(path);
        assert!(s.contains("test"));
    }
}
```

**Migration**: Update all `fs::create_dir_all()` calls to use these helpers

---

## Implementation Roadmap

### Phase 1: Foundation (1-2 hours)

**Goal**: Create new utility modules without breaking changes

1. **Create** `common/src/utils/formatting.rs` module
2. **Create** `common/src/utils/env.rs` module  
3. **Create** `common/src/utils/fs.rs` module
4. **Create** `common/src/utils/platform.rs` module
5. **Update** `common/src/utils/mod.rs` to export new modules
6. **Add** tests for all new utilities
7. **Run** `cargo test` to verify no breakage

**Files Created**: 4 new utility modules  
**Files Modified**: 1 (utils/mod.rs)  
**Risk**: LOW (additive only, no changes to existing code)

---

### Phase 2: Formatting Migration (2-3 hours)

**Goal**: Replace all size formatting with standardized utilities

**Changes**:

1. **harvest.rs** (line 106):
   ```diff
   - pub fn format_size(&self) -> String {
   -     let bytes = self.total_size_bytes();
   -     if bytes >= 1_073_741_824 { ... }
   - }
   + pub fn format_size(&self) -> String {
   +     garden_common::utils::format_bytes_short(self.total_size_bytes())
   + }
   ```

2. **rake/commands/offering/mod.rs** (lines 1036-1038):
   ```diff
   - format!("{} GB", rec.metrics.storage_free_gb)
   + format_bytes_short(rec.metrics.storage_free_gb * 1024 * 1024 * 1024)
   ```

3. **rake/commands/discovery/status.rs** (line 113):
   ```diff
   - format!("{} GB", caps.hardware.memory.total_mb / 1024)
   + format_memory_mb(caps.hardware.memory.total_mb)
   ```

4. **health.rs** (lines 92-93, 127-128): Replace direct formatting

**Files Modified**: 6 files  
**Tests**: Verify existing tests pass (harvest.rs has format tests)  
**Risk**: LOW (functionally equivalent output)

---

### Phase 3: Environment Variable Migration (3-4 hours)

**Goal**: Replace all `std::env::var()` with `EnvConfig` methods

**Strategy**: Systematic replacement across modules

1. **rake/ui.rs** (lines 23, 31)
2. **rake/main.rs** (line 1132)
3. **moss/infra/config.rs** (line 110)
4. **moss/api/v1/stone.rs** (lines 150, 153, 351, 354)
5. All other 30+ instances

**Files Modified**: 15+ files  
**Tests**: Unit tests for EnvConfig accessors  
**Risk**: MEDIUM (requires careful verification of fallback behavior)

---

### Phase 4: File System Migration (2-3 hours)

**Goal**: Replace all `fs::create_dir_all()` with helpers

**Changes**: Systematic replacement of directory creation patterns

**Files Modified**: 12+ files (see section 4 above)  
**Tests**: Integration tests for directory creation  
**Risk**: LOW (helpers are thin wrappers)

---

### Phase 5: Path Operations Migration (2-3 hours)

**Goal**: Replace verbose path operations with helpers

**Changes**:
- Replace `PathBuf::from().join()` with `join_path()`
- Replace `.to_string_lossy().to_string()` with `path_to_string()`
- Replace `.display()` in logging with `path_to_string()`

**Files Modified**: 20+ files  
**Risk**: LOW (cosmetic improvements)

---

## Testing Strategy

### Unit Tests

**Location**: Each new utility module includes comprehensive tests

- `formatting.rs`: Test all precision variants, edge cases (0 bytes, TB range)
- `env.rs`: Test with mock environment variables
- `fs.rs`: Test directory creation, path operations
- `platform.rs`: Test trait implementations (may need conditional compilation)

### Integration Tests

**Verification**: Existing 103 tests must continue passing

```powershell
# Run full test suite after each phase
cargo test --workspace

# Run specific module tests
cargo test -p garden-common utils::formatting
cargo test -p garden-moss domain::harvest
```

### Manual Verification

**Commands to run**:
```powershell
# Verify Moss still works
./garden-rake observe

# Verify size formatting in output
./garden-rake offer mongodb --at anywhere

# Verify environment variable handling
$env:GARDEN_QUIET="1"; ./garden-rake status; Remove-Item env:GARDEN_QUIET
```

---

## Migration Benefits

### Code Quality

- **-500 lines**: Estimated reduction from deduplication
- **+4 modules**: Centralized utilities in `garden-common`
- **100% coverage**: All utilities include tests
- **DRY compliance**: Single source of truth for common operations

### Maintainability

- **Easier refactoring**: Change formatting in one place
- **Better testability**: Mock platform/environment in tests
- **Consistent errors**: Standardized error messages
- **Type safety**: Typed environment variable access

### Developer Experience

- **Discoverability**: `garden_common::utils::` namespace
- **Documentation**: Clear docs on all helpers
- **IDE support**: Better autocomplete for common operations
- **Reduced cognitive load**: Less boilerplate in domain code

---

## Risks and Mitigation

### Risk 1: Breaking Changes

**Likelihood**: LOW  
**Impact**: HIGH  
**Mitigation**: 
- Phase 1 is additive only
- Run full test suite after each phase
- Keep existing functions as deprecated wrappers initially

### Risk 2: Performance Impact

**Likelihood**: LOW  
**Impact**: LOW  
**Mitigation**:
- Helpers are thin wrappers (no overhead)
- Formatting functions inline well
- Benchmark critical paths if concerned

### Risk 3: Over-Abstraction

**Likelihood**: MEDIUM  
**Impact**: MEDIUM  
**Mitigation**:
- Only extract truly duplicated patterns
- Keep helpers simple and composable
- Avoid "god object" anti-pattern

---

## Alternative Approaches

### Option A: Gradual Migration (RECOMMENDED)

Implement Phase 1, then migrate on-demand as code is touched.

**Pros**: Low risk, continuous improvement  
**Cons**: Duplication persists longer

### Option B: Big Bang Migration

Implement all phases in one PR.

**Pros**: Complete consistency immediately  
**Cons**: High review burden, merge conflicts

### Option C: Keep Status Quo

Accept duplication as acceptable technical debt.

**Pros**: No effort required  
**Cons**: Maintainability degrades over time

---

## Conclusion

The Zen Garden codebase has significant opportunities for DRY alignment through centralized helper methods. The proposed utility modules provide:

1. **Platform abstraction** - Single source of truth for OS-specific paths
2. **Formatting consistency** - Standardized size/uptime display
3. **Environment safety** - Typed, validated env var access
4. **File I/O simplification** - Consistent error handling

**Recommended Action**: Proceed with Phase 1 (foundation) immediately to establish utilities, then migrate incrementally over next 2-3 weeks as code is touched.

**Estimated Effort**: 10-15 hours total (spread across 5 phases)  
**Expected ROI**: ~500 lines reduced, improved maintainability, better testability

**Next Steps**:
1. Review and approve this analysis
2. Create Phase 1 implementation PR
3. Schedule remaining phases for next sprint
4. Update contributor guidelines to reference new utilities
