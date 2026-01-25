# Zen Garden - Capabilities Directory
**Version**: 0.1.0  
**Last Updated**: 2026-01-24  
**Purpose**: Essential conventions and existing utilities reference

> **For AI Assistants**: Read this FIRST. Don't reinvent wheels. Use what exists.

---

## 🎯 Core Conventions

### **Use GUIDv7 for IDs**
All unique identifiers use timestamp-based GUIDv7 format for natural sorting and uniqueness.

### **Platform-Aware Paths**
Always use `garden_common::constants::paths::*` functions - they handle Windows vs Linux automatically.

### **Error Handling**
- Domain: `anyhow::Result` with `.context()`
- API: Convert to HTTP status codes with standard error codes from `garden_common::constants`
- Never unwrap in production code

### **Separation of Concerns**
- Domain = pure business logic (no external deps)
- Infra = external integrations (Docker, filesystem, network)
- API = HTTP handlers (thin wrappers around domain)
- Domain NEVER imports from infra (use traits)

### **Async Patterns**
- File I/O: Always use `tokio::fs`, never blocking `std::fs` in async context
- HTTP: Use `reqwest` with 30s timeout (see `constants::timeouts`)
- Background tasks: Use `tokio::spawn` with error logging

### **Shared Contracts/Models**
**CRITICAL**: Moss and Rake MUST share the majority of contracts/models via `garden_common`.
- API request/response types: Define once in `common/src/*.rs`
- NO bespoke structures unless necessary and explicitly approved
- Reduces drift, ensures compatibility, simplifies maintenance
- Example: `garden_common::nourishment::*` used by both moss API and rake CLI

---

## 📚 Table of Contents

1. [Existing Utilities](#existing-utilities) - What's already available
2. [Standard Patterns](#standard-patterns) - Architectural conventions
3. [Environment Variables](#environment-variables) - All env vars documented
4. [Phase 1 Utilities](#phase-1-utilities--implemented-2026-01-24) - DRY improvements complete

---

## Existing Utilities

### ✅ **Formatting** → `common/src/utils.rs`
- `format_bytes(u64)` - "1.00 GB" format (2 decimals)
- `format_uptime(u64)` - "1h 30m" format

**DON'T create custom `format_size()` methods** - use these!

### ✅ **Platform Paths** → `common/src/constants/paths.rs`
- `data_dir()` - Auto: `/var/lib/zen-garden` (Linux) or `.zen-garden` (Windows)
- `config_dir()` - Auto: `/etc/zen-garden` (Linux) or `.zen-garden` (Windows)
- `harvest_dir()`, `stored_dir()`, `stone_home()`, `stone_user()`, `first_run_flag()`

**DON'T use `#[cfg(target_os = "windows")]` conditionals** - use path functions!

### ✅ **Network** → `common/src/constants/mod.rs`
- Ports: `DISCOVERY_UDP = 7184`, `MOSS_HTTP = 7185`, `LANTERN_HTTP = 7186`

### ✅ **Error Codes** → `common/src/constants/mod.rs`
Standard codes for API responses: `INVALID_REQUEST`, `SERVICE_NOT_FOUND`, `DOCKER_ERROR`, `INTERNAL_ERROR`, etc.

### ✅ **Timeouts** → `common/src/constants/timeouts.rs`
All timeout values: `DISCOVERY_TIMEOUT_MS = 3000`, `HTTP_REQUEST_TIMEOUT_MS = 30000`, etc.

### ✅ **Limits** → `common/src/constants/limits.rs`
Resource limits: `MAX_OFFERING_NAME_LENGTH = 64`, `MAX_SERVICES_PER_STONE = 100`, etc.

---

## Standard Patterns

### **Domain/Infra Separation**
```
src/moss/src/
├── domain/      # Pure business logic (no external deps)
├── infra/       # External integrations (Docker, filesystem)
├── api/         # HTTP handlers (thin wrappers)
├── tasks/       # Background tasks
└── bootstrap/   # Initialization
```
**Rule**: Domain never imports infra. Infra implements domain traits.

### **AppState Pattern** → `moss/src/app_state.rs`
Shared state via `Arc<RwLock<T>>` passed to API handlers via Axum State extractor.

### **Error Handling**
- Domain: `anyhow::Result` with `.context("...")`
- API: Convert to `(StatusCode, Json<ErrorResponse>)` with standard error codes

### **Persistence** → `moss/src/infra/persistence.rs`
- `load_json<T>()` / `save_json<T>()` - Atomic writes with temp files
- Auto directory creation, pretty-print JSON

### **Docker** → `moss/src/infra/docker.rs`
Wrapper around bollard: container lifecycle, images, volumes, networks

### **Manifests** → `moss/src/infra/manifests/`
Load YAML frontmatter + markdown from `manifests/sw/` and `manifests/hw/`

### **HTTP Client** → `common/src/client.rs`
`ApiClient` with JSON auto-serialization, 30s timeout, error context

---

## Environment Variables

### ✅ Environment Variables

**Known Environment Variables** (use these keys consistently):

**Paths**:
- `GARDEN_DATA_DIR` - Override data directory
- `GARDEN_CONFIG_DIR` - Override config directory
- `GARDEN_HARVEST_DIR` - Override harvest storage
- `GARDEN_STAGING_DIR` - Override staging area for deployments
- `GARDEN_STORED_DIR` - Override stored offerings directory

**Stone Configuration**:
- `GARDEN_STONE_NAME` - Override stone hostname
- `GARDEN_STONE_HOST` - Force static IP for announcements
- `GARDEN_STONE_HOME` - Override stone user home
- `GARDEN_STONE_USER` - Override stone username
- `GARDEN_FIRST_RUN_FLAG` - Override first-run flag path

**Endpoints**:
- `GARDEN_STONE` - Direct endpoint for rake (skip discovery)
- `LANTERN_ENDPOINT` - Lantern registry endpoint

**Runtime Flags**:
- `NO_COLOR` - Disable ANSI colors (standard convention)
- `GARDEN_NO_COLOR` - Alternative color disable flag
- `GARDEN_UNICODE` - Enable unicode symbols in output
- `GARDEN_QUIET` - Suppress non-essential output
- `RUNNING_AS_SERVICE` - Set when running as Windows service
- `ZEN_GARDEN_CONTAINER` - Set when running in container

**External Tool Detection**:
- `CUDA_PATH` - NVIDIA CUDA installation path
- `INTEL_OPENVINO_DIR` - Intel OpenVINO installation
- `SystemRoot` - Windows system directory
- `PROGRAMDATA` - Windows program data directory
- `HOME` - User home directory (Unix)

**Current Access Pattern**: Direct `std::env::var()` calls  
**Planned Improvement**: Typed `EnvConfig` helper (see [Planned Additions](#planned-additions))

---

### ✅ Binary Names

```rust
pub const MOSS_BINARY: &str = "garden-moss";
pub const RAKE_BINARY: &str = "garden-rake";
pub const LANTERN_BINARY: &str = "garden-lantern";

pub const MOSS_CONFIG: &str = "garden-moss.toml";
pub const LANTERN_CONFIG: &str = "garden-lantern.toml";

pub const MOSS_SERVICE: &str = "garden-moss.service";
pub const LANTERN_SERVICE: &str = "garden-lantern.service";
```

---

## Domain Patterns

### ✅ Domain/Infra Separation

**Architecture**: Clean separation of concerns

```
src/moss/src/
├── domain/          # Business logic (pure Rust, no external deps)
│   ├── placement.rs     # Offering placement orchestration
│   ├── adoption.rs      # Container adoption logic
│   ├── harvest.rs       # Backup artifacts
│   └── health.rs        # Health check definitions
├── infra/           # External integrations (Docker, filesystem, etc.)
│   ├── docker.rs        # Docker client wrapper
│   ├── filesystem.rs    # File I/O operations
│   ├── persistence.rs   # State persistence
│   └── hardware.rs      # Hardware capability detection
├── api/             # HTTP endpoints (Axum handlers)
│   └── v1/              # Versioned API routes
├── tasks/           # Background tasks (health monitoring, etc.)
└── bootstrap/       # Application initialization
```

**Pattern**: Domain modules should NOT import from infra. Infra implements domain traits.

---

### ✅ State Management

**Module**: `moss/src/app_state.rs`
**All env vars** (use these exact keys):

**Paths**: `GARDEN_DATA_DIR`, `GARDEN_CONFIG_DIR`, `GARDEN_HARVEST_DIR`, `GARDEN_STAGING_DIR`, `GARDEN_STORED_DIR`

**Stone**: `GARDEN_STONE_NAME`, `GARDEN_STONE_HOST`, `GARDEN_STONE_HOME`, `GARDEN_STONE_USER`, `GARDEN_FIRST_RUN_FLAG`

**Endpoints**: `GARDEN_STONE` (skip discovery), `LANTERN_ENDPOINT`

**Flags**: `NO_COLOR`, `GARDEN_NO_COLOR`, `GARDEN_UNICODE`, `GARDEN_QUIET`, `RUNNING_AS_SERVICE`, `ZEN_GARDEN_CONTAINER`

**External**: `CUDA_PATH`, `INTEL_OPENVINO_DIR`, `SystemRoot`, `PROGRAMDATA`, `HOME`

**Current**: See Phase 1 Utilities - `env.rs` provides typed `EnvConfig` helper  
**Migration**: Replace direct `std::env::var()` calls with typed accessors

---

## Phase 1 Utilities (✅ Implemented 2026-01-24)

DRY alignment initiative - consolidating common patterns across codebase.

### **formatting.rs** (src/common/src/utils/)
- `format_bytes_precision()`, `format_bytes_short()`, `format_bytes_whole()`, `format_memory_mb()`
- `format_uptime()` - **Consolidated** from moss/console.rs duplicate
- 104 lines, 3 test functions

### **env.rs** (src/common/src/utils/)
- `EnvConfig` with typed accessors for all `GARDEN_*` and `MOSS_*` environment variables
- `keys` module - Centralized registry of variable names
- Replaces 30+ direct `std::env::var()` calls
- 147 lines, 4 test functions

### **fs.rs** (src/common/src/utils/)
- `ensure_dir()`, `read_file()`, `write_file()` with async variants
- Automatic parent directory creation, error context
- 107 lines, 5 test functions

### **platform.rs** (src/common/src/utils/)
- `PlatformPaths` trait with Windows/Unix implementations
- `get_platform_paths()`, `data_dir()`, `config_dir()`
- 89 lines, 4 test functions

### **ids.rs** (src/common/src/utils/)
- GUIDv7 (RFC 9562) generation using existing `uuid` crate
- `generate_guidv7()`, `generate_id(prefix)`, `generate_timestamp_id()`
- Thin wrapper (~24 lines) - **DRY principle**: reuses existing dependency

### **json.rs** (src/common/src/utils/)
- `parse<T>()`, `stringify<T>()`, `stringify_pretty<T>()` with error context
- Wrappers for `serde_json` with better error messages
- 74 lines, 4 test functions

### **strings.rs** (src/common/src/utils/)
- `truncate()`, `to_kebab_case()`, `to_snake_case()`, `is_valid_identifier()`
- **Consolidated** `truncate_name()` from rake/ui.rs duplicate
- 126 lines, 6 test functions

### **validation.rs** (src/common/src/utils/)
- `validate_name()`, `validate_port()`, `validate_url()`, `validate_safe_path()`
- Business rule validation (distinct from `api_utils/sanitize.rs`)
- 135 lines, 6 test functions

**Status**: ✅ 8 modules complete, 122+ tests passing, 2 duplicates consolidated

---

## Platform Paths (✅ Available Now)
**Benefits**: Automatic parent directory creation, consistent error messages, reduced boilerplate

---

### 🔄 Platform Abstraction

**Module**: `common/src/utils/platform.rs` (PLANNED)

```rust
// Trait-based platform abstraction for testability

pub trait PlatformPaths {
    fn data_dir(&self) -> PathBuf;
    fn config_dir(&self) -> PathBuf;
    fn temp_dir(&self) -> PathBuf;
}

pub struct WindowsPaths;
impl PlatformPaths for WindowsPaths { /* ... */ }

pub struct UnixPaths;
impl PlatformPaths for UnixPaths { /* ... */ }

pub fn get_platform_paths() -> Box<dyn PlatformPaths>

// Convenience functions
pub fn data_dir() -> PathBuf
pub fn config_dir() -> PathBuf
```

**Replaces**: Scattered `#[cfg(target_os = "windows")]` conditionals  
**Benefits**: Testability (inject mock platform), centralized OS logic

---

## How to Use This Document

### For Developers

1. **Before implementing a utility**: Search this document - check Existing Utilities and Phase 1 Utilities
2. **When adding new utilities**: Update the appropriate section with implementation details
3. **Follow existing patterns**: Use the same error handling, naming conventions, test structure
4. **Update version date**: When adding significant new capabilities
5. **Consolidate duplicates**: If you find similar code, consider using Phase 1 utilities

### For AI Assistants

1. **Read this first**: Before suggesting new code, check what's already available
2. **Use existing utilities**: Always prefer `garden_common` utilities over custom implementations
3. **Don't duplicate**: If a utility exists, use it. If it's planned, mention it needs implementation
4. **Update this document**: When creating new utilities, add them to "Planned Additions" or promote to main sections

### Quick Decision Tree

```
Need a utility?
│
├─ Does it exist in "Common Utilities"?
│  └─ YES → Use it! (see examples)
│
├─ Is it in "Planned Additions"?
│  └─ YES → Mention it needs implementation first
│
└─ Is it truly new?
   └─ YES → Implement in appropriate module, then update this doc
```

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-01-24 | Initial capabilities directory created |

---

## See Also

- [DRY Alignment Analysis](refactoring/DRY-ALIGNMENT-ANALYSIS.md) - Detailed duplication analysis
- [Architecture Decisions](decisions/README.md) - ADRs for design patterns
- [Rust Refactoring Proposal](proposals/implemented/rust-refactoring-proposal.md) - Module organization
- [Common Package README](../src/common/README.md) - garden-common documentation
DRY Alignment in progress (2026-01-24)  
> **Details**: [docs/refactoring/DRY-ALIGNMENT-ANALYSIS.md](refactoring/DRY-ALIGNMENT-ANALYSIS.md)

### 🔄 `common/utils/formatting.rs`
- `format_bytes_short(u64)` - 1 decimal (replaces custom `format_size()` methods)
- `format_bytes_precision(u64, usize)` - Configurable decimals
- `format_memory_mb(u64)` - MB to display format

### 🔄 `common/utils/env.rs`
Typed `EnvConfig` with methods like:
- `data_dir()`, `config_dir()`, `staging_dir()`
- `stone_name()`, `stone_endpoint()`, `lantern_endpoint()`
- `is_no_color()`, `is_quiet()`, `is_running_as_service()`

Replaces 30+ direct `std::env::var()` calls.

### 🔄 `common/utils/fs.rs`
- `ensure_dir()` / `ensure_dir_async()` - Consistent error handling
- `ensure_parent_dir()` / `ensure_parent_dir_async()`
- `read_file()` / `write_file()` - With auto parent dir creation
- `path_to_string()`, `join_path()` - Reduces boilerplate

Replaces 25+ `fs::create_dir_all()` calls.

### 🔄 `common/utils/platform.rs`
Trait-based platform abstraction (for testing):
- `PlatformPaths` trait with `WindowsPaths` and `UnixPaths` implementations
- Replaces scattered `#[cfg(target_os = "windows")]` conditionalsQuick Reference

**Before writing code**:
1. Check [Existing Utilities](#existing-utilities) - does it exist?
2. Check [Planned Utilities](#planned-utilities) - is it in progress?
3. If new, add to appropriate section and update version history

**Key files to reference**:
- `common/src/utils.rs` - Formatting utilities
- `common/src/constants/paths.rs` - Platform paths
- `common/src/constants/mod.rs` - All constants
- `moss/src/infra/persistence.rs` - JSON I/O patterns
- `moss/src/infra/docker.rs` - Docker operations

---

## See Also

- [DRY Alignment Analysis](refactoring/DRY-ALIGNMENT-ANALYSIS.md) - Duplication analysis & implementation plan
- [Architecture Decisions](decisions/README.md) - Design pattern ADRs  
- [Rust Refactoring](proposals/implemented/rust-refactoring-proposal.md) - Module organiz