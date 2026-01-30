# Phase 3: Complete Non-Concern Code Elimination

**Objective**: Extract ALL remaining infrastructure/utility code from moss and rake to common, leaving only pure domain logic.

**Total Target**: ~5,500 lines of pure infrastructure code

---

## Batch 3A: Console System (~1,300 lines)

**Priority**: HIGH - Large, self-contained, zero domain coupling

### Files
- `moss/src/console.rs` (1,306 lines) → `common/src/console/`

### Extraction Plan
```
common/src/console/
├── mod.rs                 # Public API
├── modes.rs               # ConsoleMode enum, parsing
├── events.rs              # EventCategory system
├── dedup.rs               # Event deduplication logic
└── legacy_tty.rs          # Legacy first-boot TTY output
```

### Key Types
- `ConsoleMode` (Silent/Minimal/Informative/Verbose)
- `EventCategory` (System/Config/Manifests/Offerings/etc.)
- `ConsoleEvent` struct
- Deduplication cache

### Dependencies
- None (uses only std, anyhow, chrono)

### Post-Extraction
- `moss/src/console.rs` → Re-export from `garden_common::console`

---

## Batch 3B: Configuration Management (~500 lines)

**Priority**: HIGH - Critical infrastructure, clean abstractions

### Files
1. `moss/src/infra/config.rs` (232 lines) → `common/src/config/moss.rs`
2. `moss/src/infra/secrets.rs` (357 lines) → `common/src/secrets/`

### Extraction Plan
```
common/src/config/
├── mod.rs
└── moss.rs                # MossConfig, AdoptionConfig

common/src/secrets/
├── mod.rs
├── manager.rs             # SecretsManager
├── backend.rs             # SecretBackend trait
├── tpm.rs                 # TPM backend (stub)
├── keyring.rs             # Platform keyring backend
└── encrypted_file.rs      # Encrypted file fallback
```

### Key Types
- `MossConfig` - TOML configuration
- `AdoptionConfig` - Adoption settings
- `SecretsManager` - Backend selector
- `SecretBackend` trait - Storage abstraction

### Dependencies to Add
- `toml` (already in moss) → add to common
- `aes-gcm`, `chacha20poly1305` (encryption)

### Post-Extraction
- `moss/src/infra/config.rs` → Re-export
- `moss/src/infra/secrets.rs` → Re-export

---

## Batch 3C: Rake Infrastructure (~1,100 lines)

**Priority**: HIGH - Eliminates rake infrastructure duplication

### Files
1. `rake/src/tending.rs` (448 lines) → `common/src/client/tending.rs`
2. `rake/src/discovery.rs` (497 lines) → `common/src/client/discovery.rs`
3. `rake/src/context.rs` (131 lines) → `common/src/client/context.rs`

### Extraction Plan
```
common/src/client/
├── mod.rs
├── tending.rs             # TendingState, StoneError, persistence
├── discovery.rs           # Lantern/Moss discovery (mDNS, UDP, auto)
├── context.rs             # CommandContext, URL building
└── stone_cache.rs         # Already extracted (Phase 1)
```

### Key Types
- `TendingState` - Persistent stone connection
- `StoneError` - Connection/response/processing errors
- `StoneCandidate` - Discovery result
- `CommandContext` - CLI execution context

### Dependencies
- `mdns-sd` (already in common)
- `dirs` (for tending.rs path resolution) → add to common

### Post-Extraction
- All rake files → Re-export from common

---

## Batch 3D: Data Stores (~600 lines)

**Priority**: MEDIUM - Storage abstractions

### Files
1. `moss/src/infra/harvest_store.rs` (198 lines) → `common/src/storage/harvest.rs`
2. `moss/src/infra/ceremony_journal.rs` (233 lines) → `common/src/storage/journal.rs`
3. `moss/src/infra/filesystem.rs` (79 lines) → `common/src/storage/filesystem.rs`
4. `moss/src/infra/persistence.rs` (156 lines) → `common/src/persistence/json.rs`

### Extraction Plan
```
common/src/storage/
├── mod.rs
├── harvest.rs             # HarvestStore path management
├── journal.rs             # CeremonyJournal event logging
└── filesystem.rs          # FilesystemConfig, data dirs

common/src/persistence/
├── mod.rs
└── json.rs                # Atomic JSON read/write (already partially extracted)
```

### Key Types
- `HarvestStore` - Harvest artifact paths
- `CeremonyJournal` - Event log storage
- `FilesystemConfig` - Data directory abstraction

### Dependencies
- None (uses std, tokio::fs, garden_common types)

### Post-Extraction
- All moss/infra files → Re-export

---

## Batch 3E: Hardware & Firmware (~400 lines)

**Priority**: MEDIUM - System integration helpers

### Files
1. `moss/src/infra/hardware.rs` (212 lines) → `common/src/hardware/capabilities.rs`
2. `moss/src/infra/firmware.rs` (183 lines) → `common/src/hardware/firmware.rs`

### Extraction Plan
```
common/src/hardware/
├── mod.rs
├── capabilities.rs        # HardwareCapabilities skeleton creation
└── firmware.rs            # Firmware update helpers (fwupd)
```

### Key Functions
- `create_skeleton()` - Empty HardwareCapabilities
- `check_fwupd_updates()` - Query firmware updates
- `apply_firmware_update()` - Execute fwupd

### Dependencies
- None (uses std::process for fwupd CLI)

### Post-Extraction
- moss/infra files → Re-export

---

## Batch 3F: Service Infrastructure (~270 lines)

**Priority**: LOW - Small utilities

### Files
1. `moss/src/infra/service.rs` (194 lines) → `common/src/service/helpers.rs`
2. `moss/src/infra/container.rs` (80 lines) → `common/src/service/container.rs`

### Extraction Plan
```
common/src/service/
├── mod.rs
├── helpers.rs             # Service state management
└── container.rs           # Container utilities
```

### Note
- May have moss-specific dependencies - evaluate carefully
- Consider leaving in moss if tightly coupled to domain

---

## Batch 3G: Rake Command Manifest (~1,700 lines)

**Priority**: LOW - CLI-specific metadata

### Files
- `rake/src/command_manifest.rs` (1,724 lines)

### Decision
- **Keep in rake** unless lantern needs CLI commands
- This is rake-specific domain logic (command definitions)
- Not pure infrastructure

---

## Batch 3H: API Helpers (~200 lines)

**Priority**: MEDIUM - Consolidate API utilities

### Files
1. `moss/src/infra/api_helpers.rs` (25 lines) → Already in `common/src/api_utils/errors.rs`
2. `moss/src/api/responses.rs` (42 lines) → Merge into `common/src/api_utils/responses.rs`
3. `moss/src/api/suggestions.rs` (87 lines) → `common/src/api_utils/suggestions.rs`
4. `rake/src/suggestions.rs` (53 lines) → `common/src/cli/suggestions.rs`

### Extraction Plan
```
common/src/api_utils/
├── suggestions.rs         # API suggestion engine (NEW)

common/src/cli/
├── suggestions.rs         # CLI suggestion engine (NEW)
```

### Post-Extraction
- Remove moss/infra/api_helpers.rs (already in common)
- moss/api/responses.rs → Re-export
- moss/api/suggestions.rs → Re-export
- rake/suggestions.rs → Re-export

---

## Batch 3I: Bootstrap Utilities (~400 lines)

**Priority**: LOW - Moss-specific setup

### Files
1. `moss/src/bootstrap/config.rs` (137 lines)
2. `moss/src/bootstrap/first_boot.rs` (64 lines)
3. `moss/src/bootstrap/preinstall.rs` (62 lines)

### Decision
- **Evaluate** - May be moss-specific domain logic
- If pure system checks → Extract to `common/src/bootstrap/`
- If coupled to moss startup → Keep in moss

---

## Execution Order

### Phase 3A (Day 1)
1. **Batch 3A: Console System** (~1,300 lines)
   - Largest, most isolated
   - High impact

### Phase 3B (Day 2)
2. **Batch 3B: Configuration** (~500 lines)
3. **Batch 3C: Rake Infrastructure** (~1,100 lines)
   - Related modules, extract together

### Phase 3C (Day 3)
4. **Batch 3D: Data Stores** (~600 lines)
5. **Batch 3H: API Helpers** (~200 lines)
   - Consolidation and cleanup

### Phase 3D (Day 4)
6. **Batch 3E: Hardware & Firmware** (~400 lines)
7. **Batch 3F: Service Infrastructure** (~270 lines)
   - Final infrastructure modules

---

## Success Criteria

✅ **moss/src/** contains ONLY:
- `domain/` - Pure business logic
- `api/` - Thin HTTP handlers (use common types)
- `tasks/` - Background services (orchestration)
- `docker.rs` - Domain-specific Docker abstraction
- `mdns.rs`, `announcement.rs` - Domain protocols
- `bootstrap/` - Startup orchestration (may keep some)
- `app_state.rs`, `lib.rs`, `main.rs`, `cli.rs` - Entry points

✅ **rake/src/** contains ONLY:
- `main.rs` - CLI entry point
- `command_manifest.rs` - Command definitions (CLI domain)
- `commands/` - Command handlers (business logic)
- `dispatch.rs` - Command dispatch (orchestration)
- Re-exports from common

✅ **common/src/** contains ALL:
- Pure infrastructure (network, storage, parsing)
- Reusable utilities (formatting, validation)
- Shared types (contracts between moss/rake)
- Platform abstractions
- API/CLI helpers

---

## Metrics

**Before Phase 3**:
- Total extracted: ~6,123 lines
- moss infra: ~4,000 lines remaining
- rake infra: ~2,500 lines remaining

**After Phase 3**:
- Total extracted: ~11,600+ lines
- moss infra: ~500 lines remaining (docker.rs, mdns.rs only)
- rake infra: ~200 lines remaining (command_manifest.rs, main.rs only)

**Code Reduction**:
- moss: 75% infrastructure eliminated
- rake: 90% infrastructure eliminated
- common: Comprehensive shared library

---

## Notes

- **Do NOT extract**:
  - Domain logic (service_discovery, placement, topology, etc.)
  - Orchestration (tasks/, bootstrap/run.rs)
  - Domain-specific Companions (docker.rs, mdns.rs)

- **Already Extracted** (Phases 1 & 2):
  - Archive, Process, Platform
  - Registry Client, Network
  - Parser, Stone Cache, Metrics
  - UI, Layout
  - Detection (command, http_probe)
  - Manifests (hw, sw, registry)

- **Validation**:
  - Each batch must compile independently
  - No circular dependencies
  - Zero domain coupling
