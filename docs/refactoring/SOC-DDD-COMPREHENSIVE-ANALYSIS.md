# Comprehensive SoC/DDD Refactoring Analysis
**Date**: 2026-01-25  
**Objective**: Identify ALL code candidates for moving from Moss/Rake to Common  
**Goal**: Eliminate non-concern code, achieve perfect domain separation

---

## Executive Summary

**Current State**: Moss and Rake contain significant infrastructure and utility code that violates SoC/DDD principles.

**Target State**: Clean domain boundaries where:
- Moss = Stone lifecycle domain logic only
- Rake = CLI workflow domain logic only  
- Common = ALL reusable infrastructure, utilities, types

**Impact**: ~2,500+ lines identified for extraction to common

---

## Critical Violations (Priority 1 - MUST FIX)

### 1. Archive Operations (`moss/infra/archive.rs` → `common/infra/archive.rs`)
**Lines**: 295 lines  
**Concern**: Infrastructure (compression, extraction, checksums)  
**Usage**: Harvest, stored offerings, future archive needs  
**Why Common**: Zero moss-specific logic, pure infrastructure utility  
**Duplicat Risk**: HIGH - Rake will need archive operations for local backups

```rust
// Current: moss/src/infra/archive.rs
pub struct Archiver;
pub fn create_archive() -> Result<ArchiveInfo>
pub fn extract_archive() -> Result<()>
pub fn calculate_checksum() -> Result<String>
pub fn format_bytes(u64) -> String  // ⚠️ DUPLICATE of common/utils.rs
```

**Action**: Move entire module to `common/infra/archive.rs`

---

### 2. Registry Client (`moss/infra/registry.rs` → `common/infra/registry_client.rs`)
**Lines**: 389 lines  
**Concern**: Docker Registry HTTP API client  
**Usage**: Query image versions, check updates  
**Why Common**: Reusable HTTP client, no moss domain logic  
**Duplication Risk**: HIGH - Rake/Lantern may need registry queries

```rust
// Current: moss/src/infra/registry.rs
pub struct ImageRef { registry, repository, tag }
pub fn parse(image: &str) -> Result<Self>  // Generic Docker image parsing
pub async fn query_image_tags() -> Result<Vec<String>>
pub async fn get_image_digest() -> Result<String>
pub fn find_newer_version() -> Option<String>  // Version comparison logic
```

**Action**: Move to `common/infra/registry_client.rs`

---

### 3. Metrics Collection (`moss/metrics.rs` → `common/metrics/`)
**Lines**: 1,284 lines  
**Concern**: Hardware/system metrics detection  
**Usage**: CPU, GPU, storage, OS version, container runtime  
**Why Common**: Platform utilities, no moss-specific logic  
**Duplication Risk**: MEDIUM - Rake might need system info for local features

```rust
// Current: moss/src/metrics.rs
pub fn get_cpu_info() -> Result<(String, Vec<String>, String)>
pub fn collect_stone_resources() -> Result<StoneResources>
pub fn detect_disk_type_for_mount(mount_point: &str) -> Option<String>
pub fn detect_gpus() -> Vec<GpuInfo>
pub fn detect_storage() -> Vec<StorageDevice>
pub fn detect_os_version() -> Option<String>
pub fn detect_kernel_version() -> Option<String>
pub fn detect_swap() -> Option<u64>
pub fn detect_container_runtime() -> Option<String>
```

**Action**: Move to `common/metrics/system.rs`, `common/metrics/hardware.rs`

---

### 4. Process Management (`moss/infra/process.rs` → `common/infra/process.rs`)
**Lines**: ~150 lines  
**Concern**: Process detection and management  
**Usage**: Check/kill moss processes, graceful shutdown  
**Why Common**: Generic process utilities, reusable across binaries

```rust
// Current: moss/src/infra/process.rs
pub fn check_moss_processes_exist() -> bool
pub fn kill_existing_moss_processes() -> Result<()>
pub async fn kill_existing_moss_processes_graceful() -> Result<()>
```

**Action**: Generalize and move to `common/infra/process.rs`

---

### 5. Platform Detection (`moss/infra/platform.rs` → `common/infra/platform.rs`)
**Lines**: ~100 lines  
**Concern**: Platform-specific utilities  
**Usage**: Removable media detection, shutdown signals  
**Why Common**: Cross-platform utilities, no domain logic

```rust
// Current: moss/src/infra/platform.rs
pub fn is_running_from_removable_media(exe_path: &Path) -> Result<bool>
pub async fn shutdown_signal()
```

**Action**: Move to `common/infra/platform.rs`

---

### 6. Network Utilities (`moss/infra/network.rs` → Already partially in common)
**Lines**: ~250 lines  
**Status**: WoL already moved, but IP detection logic remains  
**Concern**: Network interface enumeration, IP/MAC detection  

```rust
// Current: moss/src/infra/network.rs
pub fn get_local_ip() -> String  // Priority-based IP selection
pub fn get_local_ip_and_mac() -> (String, Option<String>)
// ⚠️ WoL already moved to common ✅
```

**Action**: Move get_local_ip logic to `common/infra/network.rs`

---

### 7. Parser Module (`rake/parser.rs` → `common/cli/zen_parser.rs`)
**Lines**: 390 lines  
**Concern**: Zen syntax parsing  
**Usage**: Positional keyword extraction (`on`, `from`, `quietly`)  
**Why Common**: Future binaries may adopt zen syntax  
**Duplication Risk**: LOW - Unique to CLI binaries

```rust
// Current: rake/src/parser.rs
pub enum CommandStyle { Zen, Normative }
pub struct ParsedCommand { style, verb, args, keywords }
pub fn parse_args(args: Vec<String>) -> Result<ParsedCommand>
pub fn is_zen_verb(word: &str) -> bool
```

**Action**: Move to `common/cli/zen_parser.rs` (future-proof for other CLIs)

---

### 8. Stone Cache (`rake/stone_cache.rs` → `common/client/stone_cache.rs`)
**Lines**: 169 lines  
**Concern**: Discovery result caching  
**Usage**: Hot cache architecture (90s TTL)  
**Why Common**: Reusable client-side cache pattern  
**Duplication Risk**: HIGH - Any HTTP client will want caching

```rust
// Current: rake/src/stone_cache.rs
pub static GLOBAL_CACHE: Lazy<StoneCache>
pub struct StoneCache { stones: HashMap<String, CachedStone> }
pub fn get(&self, stone_name: &str) -> Option<CachedStone>
pub fn insert(&self, endpoint, capabilities)
```

**Action**: Move to `common/client/stone_cache.rs`

---

## High-Priority Violations (Priority 2 - SHOULD FIX)

### 9. Console Module (`moss/console.rs` → `common/console/`)
**Lines**: ~1,350 lines  
**Concern**: TTY output, banners, ANSI colors, event formatting  
**Usage**: Boot banner, MOTD, first-run ceremony, hostname management  
**Why Common**: Reusable console utilities  
**Issue**: Mixed domain/infrastructure

**Breakdown**:
- ✅ **Pure Infra** (move to common):
  - `detect_platform_console_mode()` → `common/console/detection.rs`
  - `tty_write()` → `common/console/tty.rs`
  - `FormatHint`, `AnsiColor` enums → `common/console/types.rs`
  - `ConsoleEvent`, `ConsoleChannel` → `common/console/events.rs`
  
- ❌ **Domain Logic** (stay in moss):
  - `is_first_run()` - Moss ceremony concern
  - `mark_first_run_complete()` - Moss ceremony concern
  - `generate_unique_name()` - Stone naming logic
  - `set_hostname()`, `update_hosts_file()` - Stone provisioning
  - `write_motd()` - Moss-specific MOTD
  - `print_boot_banner()`, `print_shutdown_banner()` - Moss lifecycle

**Action**: Extract infrastructure (~700 lines) to `common/console/`, keep domain logic in moss

---

### 10. UI Module (`rake/ui.rs` → `common/ui/`)
**Lines**: ~650 lines  
**Concern**: CLI formatting, terminal detection, colored output  
**Why Common**: Reusable UI utilities for any CLI  

```rust
// Current: rake/src/ui.rs
pub struct TerminalInfo { color, unicode, supports_emoji }
pub struct CliFormatter { /* output helpers */ }
pub struct TableBuilder { /* ASCII tables */ }
pub fn section_header(title: &str) -> String
pub fn kv_line(label: &str, value: &str) -> String
pub fn format_elapsed_time(elapsed: Duration) -> String
pub fn status_indicator(status: &str) -> String
pub fn pad_visible(s: &str, width: usize) -> String
```

**Action**: Move to `common/ui/` with submodules:
- `common/ui/terminal.rs` - TerminalInfo, detection
- `common/ui/formatting.rs` - Formatters, colors
- `common/ui/table.rs` - TableBuilder
- `common/ui/time.rs` - Time formatting

---

### 11. Layout Module (`rake/layout.rs` → `common/ui/layout.rs`)
**Lines**: ~600 lines  
**Concern**: CLI layout builder pattern  
**Why Common**: Reusable for any CLI

```rust
// Current: rake/src/layout.rs
pub struct Layout { term, formatter }
pub struct HeaderBuilder, FieldBuilder, LineBuilder, StatusBuilder
pub enum IndentLevel { Base, L1, L2, L3 }
```

**Action**: Move to `common/ui/layout.rs`

---

### 12. Tending Module (`rake/tending.rs` → Stay in Rake + Extract Cache Logic)
**Lines**: ~450 lines  
**Status**: MIXED domain + infrastructure  
**Domain Logic** (stay): Tending workflow, stone candidate discovery  
**Infrastructure** (extract):

```rust
// Extract to common/persistence/tending_cache.rs
pub fn read_tending() -> Result<TendingState>  // File I/O
pub fn write_tending(stone, endpoint) -> Result<()>  // File I/O
pub fn clear_tending() -> Result<()>  // File I/O
```

**Action**: Extract cache I/O to `common/persistence/tending_cache.rs`, keep domain in rake

---

### 13. Detection Modules (`moss/infra/detection/` → `common/detection/`)
**Lines**: ~300 lines  
**Concern**: Container inspection, HTTP probes, command detection  

```rust
// moss/infra/detection/container_inspect.rs
pub fn inspect_container_env() -> Result<HashMap<String, String>>

// moss/infra/detection/http_probe.rs  
pub async fn probe_http_endpoint(url: &str) -> Result<ProbeResult>

// moss/infra/detection/command.rs
pub fn detect_command_availability(cmd: &str) -> bool
```

**Action**: Move to `common/detection/`

---

### 14. Manifest Parsers (`moss/infra/manifests/` → `common/manifests/`)
**Lines**: ~800 lines  
**Concern**: YAML frontmatter parsing for sw/ and hw/ manifests  
**Why Common**: Reusable manifest format across binaries

```rust
// moss/infra/manifests/sw.rs
pub struct SwManifest { metadata, template }
pub fn parse_template(&self) -> Result<ServiceTemplate>

// moss/infra/manifests/hw.rs
pub struct HwManifest { metadata, detection }
```

**Action**: Move to `common/manifests/` (already listed in COMMON-EXTRACTION-ANALYSIS.md)

---

## Medium-Priority Violations (Priority 3 - NICE TO HAVE)

### 15. Filesystem Module (`moss/infra/filesystem.rs` → `common/infra/filesystem.rs`)
**Lines**: ~60 lines  
**Concern**: Directory helpers, path management  
**Status**: Partially overlaps with `common/utils/fs.rs`

```rust
// Current: moss/src/infra/filesystem.rs
pub struct FilesystemOps { base_dir }
pub fn data_dir(&self) -> &Path
pub fn data_file(&self, filename: &str) -> PathBuf
```

**Action**: Merge with `common/utils/fs.rs` or move to `common/infra/filesystem.rs`

---

### 16. Config Module (`moss/infra/config.rs` → Partial Extract)
**Lines**: ~270 lines  
**Status**: MIXED - Moss domain config + generic config utilities  
**Domain** (stay): AdoptionConfig, MossConfig fields  
**Infrastructure** (extract):

```rust
// Extract to common/config/loader.rs
pub fn load() -> Option<Self>  // Generic config loading pattern
pub fn save(&self) -> Result<()>  // Generic config saving pattern
```

**Action**: Extract config I/O patterns to common, keep domain structs in moss

---

### 17. Secrets Module (`moss/infra/secrets.rs` → `common/infra/secrets.rs`)
**Lines**: ~80 lines  
**Concern**: Generic secret storage trait  
**Usage**: Linux keyring, Windows credential manager, macOS keychain  
**Why Common**: Reusable secrets abstraction

```rust
// Current: moss/src/infra/secrets.rs
pub trait SecretStore {
    fn store(&self, key: &str, value: &str) -> Result<()>;
    fn retrieve(&self, key: &str) -> Result<Option<String>>;
    fn delete(&self, key: &str) -> Result<()>;
}
```

**Action**: Move to `common/infra/secrets.rs`

---

### 18. Auth Middleware (`moss/infra/auth.rs` → `common/api/auth.rs`)
**Lines**: ~50 lines  
**Concern**: HTTP Bearer token validation  
**Why Common**: Reusable for any HTTP server (Lantern, future services)

```rust
// Current: moss/src/infra/auth.rs
pub struct AuthState { default_stone: String }
pub fn new(default_stone: impl Into<String>) -> Self
// Tower middleware for Axum
```

**Action**: Move to `common/api/auth.rs`

---

### 19. API Helpers (`moss/infra/api_helpers.rs` → `common/api/helpers.rs`)
**Lines**: ~30 lines  
**Concern**: HTTP error response formatting  

```rust
// Current: moss/src/infra/api_helpers.rs
pub fn error_response(code: StatusCode, message: &str) -> Response
```

**Action**: Move to `common/api/helpers.rs`

---

### 20. Ceremony Journal (`moss/infra/ceremony_journal.rs` → Stay in Moss)
**Lines**: ~40 lines  
**Status**: Domain-specific (first-run ceremony tracking)  
**Action**: KEEP in moss (not infrastructure)

---

### 21. Service Management (`moss/infra/service.rs` → Moss-Specific)
**Lines**: ~200 lines  
**Status**: Domain-specific (moss service installation)  
**Action**: KEEP in moss (not generic)

---

## Duplication Analysis

### Confirmed Duplicates (CRITICAL)

1. **format_bytes()**
   - `moss/infra/archive.rs:232` → Duplicate of `common/utils/formatting.rs`
   - **Action**: Delete from archive.rs, import from common

2. **Version Comparison Logic**
   - `moss/infra/registry.rs::find_newer_version()` 
   - Similar logic might exist in nourishment
   - **Action**: Centralize in `common/version/comparison.rs`

3. **IP Detection**
   - `moss/infra/network.rs::get_local_ip()`
   - `moss/console.rs::get_local_ip_sync()`
   - **Action**: Single implementation in `common/infra/network.rs`

---

## Proposed Common Module Structure

```
common/src/
├── api/
│   ├── auth.rs             ← moss/infra/auth.rs
│   └── helpers.rs          ← moss/infra/api_helpers.rs
├── cli/
│   └── zen_parser.rs       ← rake/parser.rs
├── client/
│   └── stone_cache.rs      ← rake/stone_cache.rs
├── console/
│   ├── detection.rs        ← moss/console.rs (platform detection)
│   ├── events.rs           ← moss/console.rs (event types)
│   ├── tty.rs              ← moss/console.rs (TTY utilities)
│   └── types.rs            ← moss/console.rs (FormatHint, AnsiColor)
├── detection/
│   ├── command.rs          ← moss/infra/detection/command.rs
│   ├── container.rs        ← moss/infra/detection/container_inspect.rs
│   └── http_probe.rs       ← moss/infra/detection/http_probe.rs
├── infra/
│   ├── archive.rs          ← moss/infra/archive.rs
│   ├── filesystem.rs       ← moss/infra/filesystem.rs
│   ├── platform.rs         ← moss/infra/platform.rs
│   ├── process.rs          ← moss/infra/process.rs
│   ├── registry_client.rs  ← moss/infra/registry.rs
│   └── secrets.rs          ← moss/infra/secrets.rs
├── manifests/
│   ├── hw.rs               ← moss/infra/manifests/hw.rs
│   └── sw.rs               ← moss/infra/manifests/sw.rs
├── metrics/
│   ├── hardware.rs         ← moss/metrics.rs (GPU, storage, CPU)
│   └── system.rs           ← moss/metrics.rs (OS, kernel, swap)
├── persistence/
│   └── tending_cache.rs    ← rake/tending.rs (file I/O only)
├── ui/
│   ├── formatting.rs       ← rake/ui.rs (formatters)
│   ├── layout.rs           ← rake/layout.rs
│   ├── table.rs            ← rake/ui.rs (TableBuilder)
│   ├── terminal.rs         ← rake/ui.rs (TerminalInfo)
│   └── time.rs             ← rake/ui.rs (time formatting)
└── version/
    └── comparison.rs       ← moss/infra/registry.rs (version logic)
```

---

## Impact Analysis

### Lines of Code Movement

| Module | Source | Lines | Priority |
|--------|--------|-------|----------|
| Archive | moss/infra | 295 | P1 |
| Registry Client | moss/infra | 389 | P1 |
| Metrics | moss | 1,284 | P1 |
| Process | moss/infra | 150 | P1 |
| Platform | moss/infra | 100 | P1 |
| Network Utils | moss/infra | 150 | P1 |
| Parser | rake | 390 | P1 |
| Stone Cache | rake | 169 | P1 |
| Console (infra) | moss | 700 | P2 |
| UI | rake | 650 | P2 |
| Layout | rake | 600 | P2 |
| Detection | moss/infra | 300 | P2 |
| Manifests | moss/infra | 800 | P2 |
| Filesystem | moss/infra | 60 | P3 |
| Secrets | moss/infra | 80 | P3 |
| Auth | moss/infra | 50 | P3 |
| API Helpers | moss/infra | 30 | P3 |
| **TOTAL** | | **~6,197** | |

### Priority 1 (Critical)**: ~2,927 lines**  
**Priority 2 (High)**: ~2,750 lines  
**Priority 3 (Medium)**: ~520 lines

---

## Implementation Strategy

### Phase 1: Infrastructure Utilities (P1)
**Goal**: Move pure infrastructure with zero domain logic

1. **Week 1**: Archive, Registry Client, Network Utils
2. **Week 2**: Metrics (hardware/system split)
3. **Week 3**: Process, Platform, Secrets
4. **Validation**: Full workspace build, integration tests

### Phase 2: Client-Side Utilities (P1 + P2)
**Goal**: Move CLI/client reusable code

1. **Week 4**: Parser, Stone Cache, Tending Cache I/O
2. **Week 5**: UI Module (terminal, formatting, time)
3. **Week 6**: Layout, Table, Console (infra parts)
4. **Validation**: Rake builds cleanly, UX unchanged

### Phase 3: Manifest & Detection (P2)
**Goal**: Move manifest parsing and detection logic

1. **Week 7**: Manifests (hw/sw parsers)
2. **Week 8**: Detection modules (command, container, HTTP)
3. **Validation**: Offering detection works

### Phase 4: API & Cleanup (P3)
**Goal**: Final extraction and duplication removal

1. **Week 9**: Auth, API Helpers, Filesystem merge
2. **Week 10**: Version comparison, final duplication scan
3. **Week 11**: Documentation, ARCHITECTURE-REFERENCE.md update
4. **Validation**: Zero duplication, clean builds

---

## Success Criteria

1. ✅ **Zero Duplication**: `format_bytes()`, IP detection, version comparison consolidated
2. ✅ **Clean Imports**: All moss/rake use `garden_common::*` for infrastructure
3. ✅ **Domain Purity**: Moss = stone lifecycle, Rake = CLI workflow, zero infrastructure
4. ✅ **Reusability**: Future binaries can use common utilities
5. ✅ **Build Success**: Full workspace compiles with 0 errors
6. ✅ **Test Coverage**: All moved code has unit tests

---

## Risk Mitigation

### Breaking Changes
- **Risk**: Moving code breaks existing imports
- **Mitigation**: Incremental moves, update imports simultaneously

### Test Coverage
- **Risk**: Moved code lacks tests
- **Mitigation**: Add tests to common as part of move

### Domain Contamination
- **Risk**: Moving domain logic to common
- **Mitigation**: Strict review - only pure infrastructure/utilities move

---

## Verification Checklist

After each move:
- [ ] Workspace compiles (`cargo check --workspace`)
- [ ] No import errors
- [ ] No duplication (grep for moved function names)
- [ ] ARCHITECTURE-REFERENCE.md updated
- [ ] Tests pass (`cargo test --workspace`)
- [ ] Git commit with clear message

---

## Appendix: Key Questions

### Q: Should Console move to common?
**A**: Partially. Pure infrastructure (TTY, ANSI colors, platform detection) → common. Domain logic (first-run ceremony, hostname management) → stays in moss.

### Q: Should Parser move to common?
**A**: Yes. Zen syntax is a CLI pattern that could be reused by future binaries.

### Q: Should Stone Cache move to common?
**A**: Yes. Client-side caching is a general pattern, not rake-specific.

### Q: Should Manifests move to common?
**A**: Yes. Manifest format is architectural, not moss-specific. Lantern/Rake may parse manifests too.

### Q: Should Metrics move to common?
**A**: Yes. Hardware/system detection is pure infrastructure, reusable for any binary needing system info.

---

## Next Steps

1. **Review this analysis** with team
2. **Prioritize P1 modules** for immediate extraction
3. **Create tracking issues** per module
4. **Execute Phase 1** (weeks 1-3)
5. **Update ARCHITECTURE-REFERENCE.md** after each phase

---

**Document Version**: 1.0  
**Last Updated**: 2026-01-25  
**Reviewers**: Architecture Team  
**Status**: Draft - Awaiting Review
