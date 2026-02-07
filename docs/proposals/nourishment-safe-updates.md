# Zen Garden Nourishment Specification

**Safe updates for offerings and stones with ceremony-based orchestration**

**Status:** V0 Implemented (2026-01-24), V1 Planned  
**Date:** January 2026  
**Authors:** Collaborative design session  
**Dependencies:** [ceremonies.md](garden-distributed-ceremonies.md), [stone-lifecycle.md](stone-lifecycle-operations.md)

---

## Table of Contents

1. [V0 Implementation Status](#v0-implementation-status) ✅ **NEW**
2. [Overview](#overview)
3. [Vocabulary](#vocabulary)
4. [CLI Specification](#cli-specification)
5. [Ceremony Policies](#ceremony-policies)
6. [Nourishment Flows](#nourishment-flows)
7. [Stored Offerings](#stored-offerings)
8. [Vacate Ceremony](#vacate-ceremony)
9. [Ceremony Engine](#ceremony-engine)
10. [Implementation Roadmap](#implementation-roadmap)
11. [API Specification](#api-specification)

---

## V0 Implementation Status

**Implementation Date:** January 24, 2026  
**Phase:** Detection & Reporting (No Execution)

### What's Implemented

✅ **Unified Type System** (`garden_common::nourishment`)
- Single source of truth for all nourishment types
- Shared between moss (API) and rake (CLI)
- Eliminated 140+ lines of duplicate code

✅ **Docker Image Update Detection**
- Registry API integration (Docker Hub + Registry V2)
- **Digest-based comparison** (resolves "latest" vs "7.4.0" properly)
- Semantic version parsing and sorting
- Running container image ID resolution

✅ **Firmware Update Detection**
- fwupd integration on Linux via `fwupdmgr get-updates`
- Detects BIOS/UEFI firmware updates from LVFS
- Reports reboot requirements
- Added fwupd to preseed template for future stones

✅ **Constraint Checking**
- Hardware capability detection (CPU features: AVX, SSE4.2)
- Requirement validation (e.g., MongoDB 5.0+ requires AVX)
- Blocked updates with reasons displayed to user

✅ **REST API Endpoints**
```
GET  /api/v1/nourishment/check         # Detect available updates
POST /api/v1/nourishment/execute       # Execute updates (stub, returns job_id)
GET  /api/v1/nourishment/status/:id    # Query job status (stub)
GET  /api/v1/nourishment/stream/:id    # SSE progress stream (stub)
```

✅ **Rake Command**
```bash
garden-rake nourish                    # Garden-wide update check
garden-rake nourish --updates-only     # Report only, no interactive prompt
```

Interactive UI with:
- Garden-wide summary (X available, Y blocked)
- Per-stone breakdown
- Firmware updates with reboot indicators
- Blocked updates with constraint violation reasons
- Options: [A] All updates, [O] Offerings only, [S] Stone-specific, [Q] Cancel

✅ **Architecture Improvements**
- Domain/Infra separation maintained
- `execute_on_stone` pattern for discovery
- Shared contracts via `garden_common`
- AI convention enforcement via `.cursorrules` and `.github/copilot-instructions.md`

### What's NOT Implemented (V1)

❌ **Execution** - No actual updates applied yet (returns stub job_id)  
❌ **Harvest/Backup** - No data backup before updates  
❌ **Ceremony Engine** - No multi-phase orchestration  
❌ **Rollback** - No automatic rollback on failure  
❌ **Stored Offerings** - No portable snapshots  
❌ **Vacate** - No stone evacuation for firmware updates  
❌ **Quiesce/Resume** - No graceful service pausing

### Known Issues

⚠️ **Digest Comparison** - Implemented but needs production testing  
⚠️ **Tag Parsing** - Complex tags (e.g., "7.4.0-v8-x86_64") may not sort correctly  
⚠️ **Registry Auth** - Only supports public Docker Hub (no private registries yet)

### Example Output

```
📦 Garden-wide Update Status

Summary: 7 available, 1 blocked

───────────────────────────────────────────────

  stone-crystal-forest
    AVAILABLE:
      • memcached 1.6 → 1.6.40-trixie
      • System Firmware 1.17.0 → 1.38.0 (reboot required)

  stone-coral-prairie
    AVAILABLE:
      • redis latest → 7.4.0-v8-x86_64
      • rabbitmq 3-management-alpine → 4.2.3-management
      • vault 1.18 → 1.21.2
      • System Firmware 1.7.1 → 1.38.0 (reboot required)
    BLOCKED:
      ⚠ mongodb 4.4 → 8.2.3: Requires AVX (CPU: Pentium Silver J5005)

  stone-bronze-canyon
    AVAILABLE:
      • mariadb 11 → 12.2.1-ubi10-rc

───────────────────────────────────────────────

Use [A] to apply all, [O] for offerings only

Apply updates:
  [A] All updates
  [O] Offerings only
  [S] Stone-specific (TODO)
  [Q] Cancel
```

### V0 Technical Details

**Shared Types** ([garden_common/nourishment.rs](../../src/common/src/nourishment.rs)):
```rust
pub enum Update {
    Offering {
        name: String,
        current: String,
        available: String,
        age_days: Option<u32>,
    },
    Firmware {
        device_id: String,
        name: String,
        vendor: String,
        current: String,
        available: String,
        requires_reboot: bool,
        description: Option<String>,
    },
}

pub struct Updates {
    pub available: Vec<Update>,
    pub blocked: Vec<BlockedUpdate>,
}

pub struct BlockedUpdate {
    #[serde(flatten)]
    pub update: Update,
    pub reason: String,
}
```

**Digest Resolution** ([moss/infra/registry.rs](../../src/moss/src/infra/registry.rs)):
- `get_service_image_id()` - Returns actual running image SHA256
- `get_image_digest()` - Resolves tag to digest from registry
- Compares digests instead of symbolic tags

**Constraint System** ([moss/domain/constraints.rs](../../src/moss/src/domain/constraints.rs)):
- `check_constraints()` - Validates requirements against hardware
- Returns `Ok(())` or `Err(Violation)` with user-friendly message

### V0 Summary

**What works:** Detection, reporting, constraint checking  
**What doesn't:** Execution, backup, rollback, ceremony orchestration  
**Next step:** Phase 1 - Harvest infrastructure for safe updates

---

## Overview

### What is Nourishment?

**Nourishment** is the process of updating offerings (container images) and stones (firmware/BIOS) to newer versions while preserving data integrity and minimizing downtime.

**V0 Focus:** Detection and reporting only - identify what needs updating  
**V1 Goal:** Full ceremony-based execution with safety guarantees

Unlike simple `docker pull && docker restart`, nourishment (V1) will be a **ceremony** - a deliberate, multi-phase operation with:
- Pre-flight safety checks
- Data backup (harvest/store)
- Graceful service transitions
- Automatic rollback on failure
- Full audit trail

### Design Principles

**V0 (Implemented):**
1. **Detection accuracy** - Digest-based comparison, not symbolic tags
2. **Honest reporting** - Show what's available, what's blocked, and why
3. **Shared contracts** - Single source of truth for types (DRY principle)
4. **Hardware awareness** - Validate CPU requirements before suggesting updates

**V1 (Planned):**
1. **Safety by default** - Stateful offerings require backup before update
2. **Explicit risk** - `recklessly` modifier bypasses safeguards intentionally
3. **Honest reporting** - Partial success is reported honestly, not hidden
4. **Ceremony semantics** - Operations are intentional, not rushed

---

## Vocabulary

**V0 Terms (Implemented):**
| Term | Definition | Scope |
|------|------------|-------|
| **nourish** | Detect and report available updates | Offerings + Stones |
| **blocked** | Update prevented by constraint violation | Single update |
| **digest** | SHA256 hash identifying exact image version | Docker image |

**V1 Terms (Planned):**

**V1 Terms (Planned):**
| Term | Definition | Scope |
|------|------------|-------|
| **nourish** | Update to newer version | Offerings + Stones |
| **collect** | Stop + create harvest (internal, pre-nourish) | Single offering |
| **store** | Create portable snapshot (user-initiated) | Single offering |
| **harvest** | Safety backup archive (internal artifact) | Single offering |
| **stored offering** | Portable package: container + volumes + manifest | Artifact |
| **vacate** | Move all offerings off a stone | Single stone |
| **replant** | Move offering from stone A to stone B | Single offering |
| **plant** | Create offering from stored/template | Single offering |
| **water** | Bring service up after nourishment | Single offering |
| **recklessly** | Bypass safety checks (no backup) | Modifier |

---

## CLI Specification

### V0 Implementation (Current)

```bash
# Detection only (implemented)
garden-rake nourish                    # Show garden-wide updates (interactive)
garden-rake nourish --updates-only     # Report only, no prompt
```

**Interactive UI:**
- Shows available updates per stone
- Shows blocked updates with reasons
- Firmware updates with reboot indicators
- Options: [A] All, [O] Offerings only, [Q] Cancel
- Execution returns stub job_id (no actual update yet)

### V1 Zen Syntax (Planned)

### V1 Zen Syntax (Planned)

```bash
# Report only (default)
garden-rake nourish                    # Show what can be updated

# Offering scope
garden-rake nourish offerings          # Interactive - select which to update
garden-rake nourish all offerings      # Update all offerings (with ceremony)
garden-rake nourish mongodb            # Update specific offering
garden-rake nourish all offerings recklessly  # Skip backup phase

# Stone scope
garden-rake nourish stones             # Interactive - select which to update
garden-rake nourish all stones         # Update all stones (with vacate option)
garden-rake nourish stone-01           # Update specific stone
garden-rake nourish stones recklessly  # No vacate, offerings go down

# Combined
garden-rake nourish all                # Everything (offerings + stones)
garden-rake nourish all recklessly     # Everything, no safety nets
```

### V1 Normative Syntax (Planned)

```bash
# Offerings
garden-rake offerings upgrade                    # = nourish offerings
garden-rake offerings upgrade --all              # = nourish all offerings
garden-rake offerings upgrade --all --force      # = nourish all offerings recklessly

# Stones (firmware)
garden-rake firmware upgrade                     # = nourish stones
garden-rake firmware upgrade --stone stone-01   # = nourish stone-01
garden-rake firmware upgrade --all --force      # = nourish stones recklessly
```

### V1 Supporting Commands (Planned)

```bash
# Stored offerings
garden-rake store mongodb              # Create stored offering (live snapshot)
garden-rake stored                     # List stored offerings
garden-rake plant mongodb --from stored:mongodb-2026-01-24

# Vacate
garden-rake vacate stone-01            # Move all offerings off stone
garden-rake vacate stone-01 to stone-02  # Explicit destination

# Harvests (internal backups)
garden-rake harvests                   # List available harvests
garden-rake harvests prune             # Clean old harvests
garden-rake revert mongodb             # Restore from latest harvest

# Ceremony monitoring
garden-rake ceremonies                 # List active/recent ceremonies
garden-rake watch ceremony vacate-stone01-20260124
```

---

## Ceremony Policies (V1)

### Template Schema Extension

```yaml
# mongodb.snippet.yaml
name: mongodb
image: mongo:7.0.5
category: data

# Ceremony policies
ceremony:
  # Mode determines snapshot strategy
  # - unsafe (default): Must stop before snapshot
  # - quiesceable: Can freeze/thaw without stopping
  # - stateless: Commit anytime, no data risk
  mode: quiesceable

  # Required for quiesceable mode
  quiesce:
    exec: ["mongosh", "--eval", "db.fsyncLock()"]
    timeout_seconds: 30

  resume:
    exec: ["mongosh", "--eval", "db.fsyncUnlock()"]
    timeout_seconds: 10

  # Health check after resume/water
  verify:
    exec: ["mongosh", "--eval", "db.runCommand('ping')"]
    timeout_seconds: 5

  # Maximum quiesce duration (helps scheduler)
  max_quiesce_seconds: 60

  # Rollback behavior
  rollback:
    automatic: true           # false = require user confirmation
    max_attempts: 2
    preserve_harvest: true    # Keep harvest after successful nourish
    harvest_retention: 168h   # 7 days
```

### Mode Definitions

| Mode | Behavior | Use Case |
|------|----------|----------|
| `unsafe` | Must stop container before snapshot | Unknown apps, legacy, no quiesce support |
| `quiesceable` | Can freeze/thaw in-flight | Databases with fsync/lock support |
| `stateless` | Commit anytime, no data risk | Web servers, proxies, workers |

### Validation

Templates with `mode: quiesceable` MUST define both `quiesce` and `resume` commands. Manifest loader validates at startup.

---

## Nourishment Flows

### Offering Nourishment (Single Stone)

```
┌─────────────────────────────────────────────────────────────────────┐
│  NOURISH OFFERING CEREMONY                                          │
│  Self-hosted: Local Moss coordinates                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  PHASE 1: COLLECT                                                   │
│  ├── Check disk space (archive size * 1.2)                          │
│  ├── Quiesce or stop based on ceremony.mode                         │
│  │   ├── stateless: skip                                            │
│  │   ├── quiesceable: exec quiesce command                          │
│  │   └── unsafe: docker stop                                        │
│  ├── docker commit → harvest image                                  │
│  ├── Archive volumes → harvest tarball                              │
│  ├── Create harvest manifest with checksums                         │
│  └── Resume if quiesceable (service still running)                  │
│                                                                     │
│  PHASE 2: NOURISH                                                   │
│  ├── Pull new image                                                 │
│  ├── Stop container (if not already stopped)                        │
│  ├── Remove old container                                           │
│  └── Create new container with new image + same volumes             │
│                                                                     │
│  PHASE 3: WATER                                                     │
│  ├── Start container                                                │
│  ├── Run health checks (ceremony.verify or default)                 │
│  │   ├── Success → Mark complete, keep harvest for retention        │
│  │   └── Failure → ROLLBACK                                         │
│  │       ├── Stop failed container                                  │
│  │       ├── Restore volumes from harvest                           │
│  │       ├── Create container with original image                   │
│  │       ├── Start container                                        │
│  │       └── Verify rollback succeeded                              │
│  └── Update registry with new version                               │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Stone Nourishment (Firmware/BIOS)

```
┌─────────────────────────────────────────────────────────────────────┐
│  NOURISH STONE CEREMONY                                             │
│  Requires reboot - offerings go down                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  PRE-FLIGHT                                                         │
│  ├── Check: Other stones available for vacate?                      │
│  │   ├── Yes → Offer vacate option                                  │
│  │   └── No → Warn: "recklessly or skip"                            │
│  ├── Check: AC power (if firmware.requires_ac_power)                │
│  └── Confirm with user                                              │
│                                                                     │
│  OPTION A: WITH VACATE (zero downtime for offerings)                │
│  ├── Vacate ceremony (see below)                                    │
│  ├── All offerings now on other stones                              │
│  ├── Download firmware via LVFS                                     │
│  ├── Apply firmware update                                          │
│  ├── Reboot stone                                                   │
│  ├── Wait for stone to come back online                             │
│  ├── Verify firmware version updated                                │
│  └── (Optional) Repopulate offerings back                           │
│                                                                     │
│  OPTION B: RECKLESSLY (offerings down during reboot)                │
│  ├── Download firmware via LVFS                                     │
│  ├── Stop all offerings                                             │
│  ├── Apply firmware update                                          │
│  ├── Reboot stone                                                   │
│  ├── Wait for stone to come back online                             │
│  ├── Verify firmware version updated                                │
│  └── Start all offerings                                            │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Rolling Stone Nourishment (Multi-Stone)

```
Garden: [stone-01, stone-02, stone-03]

Phase 1: Nourish stone-01
  ├── Vacate offerings to stone-02, stone-03
  ├── Update firmware, reboot
  └── Verify healthy

Phase 2: Nourish stone-02
  ├── Vacate offerings to stone-01 (now available), stone-03
  ├── Update firmware, reboot
  └── Verify healthy

Phase 3: Nourish stone-03
  └── ...

Result: All stones nourished, zero downtime for offerings
```

---

## Stored Offerings

### What is a Stored Offering?

A **Stored Offering** is a portable package containing everything needed to recreate an offering elsewhere:
- Committed container image (with runtime state)
- Archived volume data
- Manifest with checksums and config

### Structure

```
/var/lib/zen-garden/stored/
└── mongodb-2026-01-24T14:30:00/
    ├── manifest.yaml           # Metadata, checksums, config
    ├── image.tar.zst           # Committed container image
    └── volumes/
        └── data.tar.zst        # Volume archives
```

### Manifest Schema

```yaml
# manifest.yaml
offering: mongodb
stored_at: 2026-01-24T14:30:00Z
source_stone: stone-01
version: 7.0.4

image:
  original: mongo:7.0.4
  committed: zen-stored/mongodb:2026-01-24T14-30-00
  size_bytes: 524288000
  checksum: blake3:abc123...

volumes:
  - name: data
    container_path: /data/db
    archive: volumes/data.tar.zst
    size_bytes: 2147483648
    checksum: blake3:def456...

config:
  ports: [27017]
  environment_hash: sha256:789...

ceremony:
  mode: quiesceable
  quiesced_at: 2026-01-24T14:29:55Z
  resumed_at: 2026-01-24T14:30:05Z
```

### Harvest vs Stored

| Aspect | Harvest | Stored Offering |
|--------|---------|-----------------|
| Purpose | Safety net (rollback) | Portability (move/clone) |
| Initiated by | System (automatic during nourish) | User (explicit command) |
| Location | Internal, same stone | Can be transferred |
| Retention | Configurable (default 7 days) | Until deleted |
| Contains | Volumes only (image tag saved) | Image + Volumes |

---

## Vacate Ceremony

### Overview

**Vacate** moves all offerings off a stone, typically before maintenance (firmware update, hardware replacement).

### Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│  VACATE CEREMONY                                                    │
│  Multi-party: Target stone coordinates                              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  For each offering on source stone:                                 │
│                                                                     │
│  1. ELECT CANDIDATE                                                 │
│     ├── Query garden for available stones                           │
│     ├── Filter: sufficient resources, compatible arch               │
│     ├── Rank: available capacity, reliability, anti-affinity        │
│     └── Select best candidate (or fail if none)                     │
│                                                                     │
│  2. STORE (on source stone)                                         │
│     ├── Quiesce or stop based on ceremony.mode                      │
│     ├── docker commit → stored image                                │
│     ├── Archive volumes                                             │
│     └── Create manifest with checksums                              │
│                                                                     │
│  3. TRANSFER (source → candidate)                                   │
│     ├── Push image to candidate (direct or via registry)            │
│     ├── Transfer volume archives                                    │
│     └── Transfer manifest                                           │
│                                                                     │
│  4. PLANT (on candidate stone)                                      │
│     ├── Load image                                                  │
│     ├── Restore volumes from archives                               │
│     ├── Create container with original config                       │
│     └── Start container                                             │
│                                                                     │
│  5. VERIFY                                                          │
│     ├── Run health checks                                           │
│     └── Confirm service responding                                  │
│                                                                     │
│  6. COMMIT                                                          │
│     ├── Source confirms target receipt                              │
│     ├── Source releases offering (stop + remove)                    │
│     ├── Update garden discovery/DNS                                 │
│     └── Clean up source artifacts                                   │
│                                                                     │
│  ROLLBACK (if verify fails)                                         │
│     ├── Stop on candidate                                           │
│     ├── Resume on source (from stored state)                        │
│     └── Report failure                                              │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Coordinator Selection

| Garden Size | Coordinator | Rationale |
|-------------|-------------|-----------|
| 1 stone | N/A | Vacate impossible |
| 2 stones | Target | Target controls reception |
| 3+ stones | Third-party (Elder preferred) | Survives source/target failure |

### Two-Phase Handoff

```
Source: "Here's the stored offering" [TRANSFER]
Target: "Received and verified checksum" [ACK]
Source: "Confirmed. Releasing my copy." [RELEASE]
Target: "Planting now." [PLANT]
Target: "Healthy. Commit complete." [COMMIT]
```

Source does NOT go offline until target confirms receipt.

---

## Ceremony Engine

### Overview

The **Ceremony Engine** is the runtime component that orchestrates multi-step, long-running operations. It leverages the existing Job infrastructure for individual task tracking while adding coordination semantics.

### Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  CEREMONY ENGINE                                                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐ │
│  │  Ceremony       │    │  Phase          │    │  Job            │ │
│  │  Registry       │───▶│  Executor       │───▶│  Executor       │ │
│  │                 │    │                 │    │  (existing)     │ │
│  └─────────────────┘    └─────────────────┘    └─────────────────┘ │
│         │                       │                      │           │
│         ▼                       ▼                      ▼           │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐ │
│  │  Ceremony       │    │  Phase          │    │  Job            │ │
│  │  Journal        │    │  Journal        │    │  Status         │ │
│  │  (persistent)   │    │  (in ceremony)  │    │  (existing)     │ │
│  └─────────────────┘    └─────────────────┘    └─────────────────┘ │
│                                                                     │
│  Events: ceremony.started, ceremony.phase, ceremony.completed       │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Core Types

```rust
// domain/ceremony/types.rs

/// Ceremony identifier
pub type CeremonyId = String;  // Format: "{type}-{target}-{timestamp}"

/// Ceremony types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CeremonyType {
    NourishOffering { offering: String },
    NourishStone { stone: String },
    NourishAll,
    Vacate { stone: String },
    Replant { offering: String, from: String, to: String },
    Store { offering: String },
}

/// Ceremony lifecycle state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CeremonyState {
    Initiated,      // Created, not started
    Planning,       // Determining steps
    Executing,      // Running phases
    Completed,      // All phases succeeded
    Failed,         // Unrecoverable failure
    RolledBack,     // Failed but recovered
    Cancelled,      // User cancelled
}

/// A ceremony is a sequence of phases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ceremony {
    pub id: CeremonyId,
    pub ceremony_type: CeremonyType,
    pub state: CeremonyState,
    pub coordinator: String,  // Stone ID coordinating this ceremony
    pub participants: Vec<String>,  // Stones involved

    pub phases: Vec<Phase>,
    pub current_phase: usize,

    pub initiated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,

    pub initiator: CeremonyInitiator,
    pub options: CeremonyOptions,
}

/// Who/what initiated the ceremony
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CeremonyInitiator {
    pub source: String,  // "cli", "api", "scheduled", "self-heal"
    pub stone_id: Option<String>,
    pub user: Option<String>,
    pub command: Option<String>,
}

/// Ceremony options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CeremonyOptions {
    pub recklessly: bool,
    pub dry_run: bool,
    pub skip_backup: bool,
    pub auto_rollback: bool,
}

/// A phase is a step in a ceremony, containing jobs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Phase {
    pub name: String,  // "collect", "nourish", "water"
    pub state: PhaseState,
    pub jobs: Vec<String>,  // Job IDs
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PhaseState {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}
```

### Ceremony Registry

```rust
// domain/ceremony/registry.rs

/// Thread-safe ceremony registry
pub struct CeremonyRegistry {
    ceremonies: RwLock<HashMap<CeremonyId, Ceremony>>,
    journal: CeremonyJournal,
}

impl CeremonyRegistry {
    /// Create a new ceremony
    pub async fn create(&self, ceremony_type: CeremonyType, options: CeremonyOptions) -> Result<CeremonyId>;

    /// Get ceremony by ID
    pub async fn get(&self, id: &CeremonyId) -> Option<Ceremony>;

    /// List ceremonies by state
    pub async fn list_by_state(&self, state: CeremonyState) -> Vec<Ceremony>;

    /// List active ceremonies (Initiated, Planning, Executing)
    pub async fn list_active(&self) -> Vec<Ceremony>;

    /// Update ceremony state
    pub async fn update_state(&self, id: &CeremonyId, state: CeremonyState) -> Result<()>;

    /// Advance to next phase
    pub async fn advance_phase(&self, id: &CeremonyId) -> Result<()>;

    /// Record phase failure
    pub async fn fail_phase(&self, id: &CeremonyId, error: &str) -> Result<()>;

    /// Cancel ceremony
    pub async fn cancel(&self, id: &CeremonyId) -> Result<()>;
}
```

### Ceremony Journal

```rust
// infra/ceremony_journal.rs

/// Persistent journal for ceremony recovery
pub struct CeremonyJournal {
    path: PathBuf,  // /var/lib/zen-garden/ceremonies/
}

impl CeremonyJournal {
    /// Write ceremony state to disk
    pub fn persist(&self, ceremony: &Ceremony) -> Result<()>;

    /// Load all incomplete ceremonies (for recovery on restart)
    pub fn load_incomplete(&self) -> Result<Vec<Ceremony>>;

    /// Archive completed ceremony
    pub fn archive(&self, ceremony: &Ceremony) -> Result<()>;

    /// Prune old archives
    pub fn prune(&self, older_than: Duration) -> Result<usize>;
}
```

### Phase Executor

```rust
// domain/ceremony/executor.rs

/// Executes ceremony phases
pub struct PhaseExecutor {
    job_executor: Arc<JobExecutor>,
    docker: Arc<DockerManager>,
    event_tx: broadcast::Sender<MossEvent>,
}

impl PhaseExecutor {
    /// Execute a phase, creating and running jobs
    pub async fn execute_phase(
        &self,
        ceremony: &mut Ceremony,
        phase_index: usize,
    ) -> Result<PhaseResult>;

    /// Rollback a phase
    pub async fn rollback_phase(
        &self,
        ceremony: &mut Ceremony,
        phase_index: usize,
    ) -> Result<()>;
}

pub enum PhaseResult {
    Completed,
    Failed { error: String, rollback_possible: bool },
    Cancelled,
}
```

### Ceremony Executor

```rust
// domain/ceremony/mod.rs

/// Main ceremony orchestrator
pub struct CeremonyExecutor {
    registry: Arc<CeremonyRegistry>,
    phase_executor: Arc<PhaseExecutor>,
    event_tx: broadcast::Sender<MossEvent>,
}

impl CeremonyExecutor {
    /// Start a ceremony
    pub async fn start(&self, id: &CeremonyId) -> Result<()> {
        let mut ceremony = self.registry.get(id).await?;

        self.emit(CeremonyEvent::Started { id: id.clone() });
        self.registry.update_state(id, CeremonyState::Executing).await?;

        for (i, phase) in ceremony.phases.iter().enumerate() {
            self.emit(CeremonyEvent::PhaseStarted {
                ceremony_id: id.clone(),
                phase: phase.name.clone(),
            });

            match self.phase_executor.execute_phase(&mut ceremony, i).await {
                Ok(PhaseResult::Completed) => {
                    self.registry.advance_phase(id).await?;
                }
                Ok(PhaseResult::Failed { error, rollback_possible }) => {
                    if rollback_possible && ceremony.options.auto_rollback {
                        self.rollback(id).await?;
                    } else {
                        self.registry.update_state(id, CeremonyState::Failed).await?;
                    }
                    return Err(anyhow!("Phase {} failed: {}", phase.name, error));
                }
                Ok(PhaseResult::Cancelled) => {
                    self.registry.update_state(id, CeremonyState::Cancelled).await?;
                    return Ok(());
                }
            }
        }

        self.registry.update_state(id, CeremonyState::Completed).await?;
        self.emit(CeremonyEvent::Completed { id: id.clone() });
        Ok(())
    }

    /// Rollback a ceremony
    pub async fn rollback(&self, id: &CeremonyId) -> Result<()>;

    /// Cancel a ceremony
    pub async fn cancel(&self, id: &CeremonyId) -> Result<()>;
}
```

### Ceremony Discovery (UDP)

```rust
// infra/ceremony_discovery.rs

/// UDP broadcast for ceremony discovery across garden
pub struct CeremonyDiscovery {
    socket: UdpSocket,
    registry: Arc<CeremonyRegistry>,
}

/// Compact ceremony announcement (68 bytes)
#[repr(C, packed)]
pub struct CeremonyAnnounce {
    pub stone_id: [u8; 16],      // UUID
    pub ceremony_id: [u8; 16],   // UUID
    pub ceremony_type: u8,       // Enum
    pub phase: u8,               // Current phase index
    pub progress_pct: u8,        // 0-100
    pub offerings_count: u8,     // Number of offerings involved
    pub current_offering: [u8; 32],  // Padded name
}

impl CeremonyDiscovery {
    /// Start broadcasting active ceremonies
    pub async fn start_broadcast(&self, interval: Duration);

    /// Query for active ceremonies in garden
    pub async fn query_garden(&self) -> Vec<CeremonyAnnounce>;
}
```

---

## Implementation Roadmap

### ✅ Phase 0: Detection Foundation (Completed 2026-01-24)

**Milestone:** Unified update detection without execution

**Implemented:**
- ✅ Shared type system (`garden_common::nourishment`)
- ✅ Docker registry API client (Docker Hub + Registry V2)
- ✅ Image digest resolution and comparison
- ✅ Firmware update detection via fwupd
- ✅ Hardware capability detection (AVX, SSE4.2)
- ✅ Constraint checking system
- ✅ REST API endpoints (`/check`, `/execute`, `/status`, `/stream`)
- ✅ Rake command with interactive UI
- ✅ Garden-wide update reporting
- ✅ Blocked update reasons

**Files:**
- `src/common/src/nourishment.rs` - Shared types (82 lines)
- `src/moss/src/api/v1/nourishment.rs` - API implementation (527 lines)
- `src/moss/src/infra/registry.rs` - Docker registry client (395 lines)
- `src/moss/src/infra/firmware.rs` - fwupd integration (70 lines)
- `src/moss/src/domain/constraints.rs` - Constraint validation (115 lines)
- `src/rake/src/commands/nourish.rs` - CLI command (375 lines)

**Gate:** ✅ Can detect Docker + firmware updates across garden

---

### Phase 1: Harvest Infrastructure (Planned)

**Milestone: Can backup and restore a single offering**

| Task | Files | Dependencies | Validation |
|------|-------|--------------|------------|
| Docker commit wrapper | `src/moss/src/docker.rs` | Docker API | Commit a running container |
| Volume archiver | `src/moss/src/infra/backup.rs` | tar/zstd | Archive and restore volumes |
| Harvest manifest | `src/moss/src/domain/harvest.rs` | types | Create manifest with checksums |
| Harvest storage | `src/moss/src/infra/harvest_store.rs` | paths | Store and retrieve harvests |

**Gate:** Can `store` and `restore` a stateless offering manually

**Integration test:**
```rust
#[tokio::test]
async fn test_harvest_roundtrip() {
    // 1. Start nginx container with some state
    // 2. Create harvest
    // 3. Delete container
    // 4. Restore from harvest
    // 5. Verify content matches
}
```

---

### Phase 2: Ceremony Engine Core

**Milestone: Can execute multi-phase ceremonies**

| Task | Files | Dependencies | Validation |
|------|-------|--------------|------------|
| Ceremony registry | `src/moss/src/domain/ceremony/registry.rs` | types | Create/get/list ceremonies |
| Ceremony journal | `src/moss/src/infra/ceremony_journal.rs` | filesystem | Persist and recover |
| Phase executor | `src/moss/src/domain/ceremony/executor.rs` | Job executor | Execute phase with jobs |
| Ceremony executor | `src/moss/src/domain/ceremony/mod.rs` | All above | Orchestrate full ceremony |

**Gate:** Can execute a 3-phase ceremony with journal recovery

**Integration test:**
```rust
#[tokio::test]
async fn test_ceremony_recovery() {
    // 1. Start a ceremony
    // 2. Kill process mid-phase
    // 3. Restart
    // 4. Verify ceremony resumes from journal
}
```

---

### Phase 3: Nourish Offering

**Milestone: Can safely update a single offering**

| Task | Files | Dependencies | Validation |
|------|-------|--------------|------------|
| Quiesce execution | `src/moss/src/domain/ceremony/quiesce.rs` | Docker exec | Freeze MongoDB |
| Collect phase impl | `src/moss/src/domain/ceremony/phases/collect.rs` | Harvest | Create harvest |
| Nourish phase impl | `src/moss/src/domain/ceremony/phases/nourish.rs` | Docker | Pull + recreate |
| Water phase impl | `src/moss/src/domain/ceremony/phases/water.rs` | Health checks | Verify + rollback |
| Nourish API | `src/moss/src/api/v1/nourish.rs` | Ceremony executor | HTTP endpoint |

**Gate:** Can nourish MongoDB 7.0.4 → 7.0.5 with rollback on failure

**Integration test:**
```rust
#[tokio::test]
async fn test_nourish_with_rollback() {
    // 1. Start mongodb:7.0.4 with data
    // 2. Nourish to mongodb:7.0.5-bad (image that fails health check)
    // 3. Verify automatic rollback to 7.0.4
    // 4. Verify data intact
}
```

---

### Phase 4: Rake CLI

**Milestone: Full CLI for nourishment**

| Task | Files | Dependencies | Validation |
|------|-------|--------------|------------|
| Nourish parser | `src/rake/src/commands/nourish/parser.rs` | Clap | Parse all syntax variants |
| Nourish report | `src/rake/src/commands/nourish/report.rs` | Layout | Show nourishment report |
| Nourish execute | `src/rake/src/commands/nourish/mod.rs` | API client | Execute nourishment |
| Store command | `src/rake/src/commands/store.rs` | API client | Create stored offering |
| Harvests command | `src/rake/src/commands/harvests.rs` | API client | List/prune harvests |

**Gate:** `garden-rake nourish` works end-to-end

**Manual validation:**
```bash
garden-rake nourish           # Shows report
garden-rake nourish mongodb   # Updates mongodb
garden-rake harvests          # Lists harvests
garden-rake revert mongodb    # Rolls back
```

---

### Phase 5: Stone-to-Stone Transfer

**Milestone: Can move offerings between stones**

| Task | Files | Dependencies | Validation |
|------|-------|--------------|------------|
| Transfer protocol | `src/moss/src/infra/transfer.rs` | HTTP streaming | Send/receive archives |
| Transfer API | `src/moss/src/api/v1/transfer.rs` | Transfer protocol | Endpoints |
| Replant ceremony | `src/moss/src/domain/ceremony/replant.rs` | Transfer, ceremony | Full replant flow |
| Coordinator election | `src/moss/src/domain/ceremony/election.rs` | Stone discovery | Elect coordinator |

**Gate:** Can replant mongodb from stone-01 to stone-02

**Integration test (2 Moss instances):**
```rust
#[tokio::test]
async fn test_replant_cross_stone() {
    // 1. Start stone-01 with mongodb
    // 2. Start stone-02 empty
    // 3. Replant mongodb to stone-02
    // 4. Verify mongodb running on stone-02
    // 5. Verify mongodb stopped on stone-01
}
```

---

### Phase 6: Vacate Ceremony

**Milestone: Can empty a stone for maintenance**

| Task | Files | Dependencies | Validation |
|------|-------|--------------|------------|
| Vacate ceremony | `src/moss/src/domain/ceremony/vacate.rs` | Replant | Vacate all offerings |
| Candidate selection | `src/moss/src/domain/ceremony/candidates.rs` | Stone discovery | Rank candidates |
| Rake vacate | `src/rake/src/commands/vacate.rs` | API client | CLI command |

**Gate:** `garden-rake vacate stone-01` moves all offerings

---

### Phase 7: Stone Nourishment

**Milestone: Can update stone firmware**

| Task | Files | Dependencies | Validation |
|------|-------|--------------|------------|
| LVFS integration | `src/moss/src/infra/firmware.rs` | fwupd | Check/apply firmware |
| Stone nourish ceremony | `src/moss/src/domain/ceremony/nourish_stone.rs` | Vacate, LVFS | Full flow |
| Rake nourish stones | `src/rake/src/commands/nourish/stones.rs` | API client | CLI command |

**Gate:** Can nourish stone firmware with vacate option

---

### Phase 8: Ceremony Discovery

**Milestone: Can query ceremonies across garden**

| Task | Files | Dependencies | Validation |
|------|-------|--------------|------------|
| UDP discovery | `src/moss/src/infra/ceremony_discovery.rs` | UDP socket | Broadcast/receive |
| Ceremony aggregation | `src/rake/src/commands/ceremonies.rs` | Discovery | Aggregate from garden |
| Watch command | `src/rake/src/commands/watch.rs` | SSE | Real-time monitoring |

**Gate:** `garden-rake ceremonies` shows active ceremonies across all stones

---

## API Specification

### V0 Endpoints (Implemented)

**Nourishment Check:**
```
GET  /api/v1/nourishment/check
     Returns: GardenNourishmentResponse
     {
       "stones": [
         {
           "stone_name": "stone-coral-prairie",
           "updates": {
             "available": [
               {
                 "type": "offering",
                 "name": "redis",
                 "current": "latest",
                 "available": "7.4.0-v8-x86_64",
                 "age_days": null
               },
               {
                 "type": "firmware",
                 "device_id": "...",
                 "name": "System Firmware",
                 "vendor": "Dell Inc.",
                 "current": "1.7.1",
                 "available": "1.38.0",
                 "requires_reboot": true,
                 "description": "..."
               }
             ],
             "blocked": [
               {
                 "type": "offering",
                 "name": "mongodb",
                 "current": "4.4",
                 "available": "8.2.3",
                 "reason": "Requires AVX (CPU: Pentium Silver J5005)"
               }
             ]
           }
         }
       ]
     }
```

**Nourishment Execute (Stub):**
```
POST /api/v1/nourishment/execute
     Body: ExecuteRequest
     {
       "updates": [
         { "type": "offering", "name": "redis" },
         { "type": "firmware", "device_id": "..." }
       ]
     }
     Returns: ExecuteResponse
     { "job_id": "nourish-20260124-abc123" }
     
     Note: Currently returns stub job_id, no actual execution
```

**Job Status (Stub):**
```
GET  /api/v1/nourishment/status/:job_id
     Returns: { "status": "pending" }
     
     Note: Stub implementation, always returns pending
```

**Progress Stream (Stub):**
```
GET  /api/v1/nourishment/stream/:job_id
     Content-Type: text/event-stream
     
     Note: Infrastructure in place, but no real events yet
```

---

### V1 Endpoints (Planned)

**Ceremony Endpoints:**

```
GET  /api/v1/nourishment/report
     Returns available updates for offerings and stones

POST /api/v1/nourishment/offerings
     Body: { offerings: ["mongodb"], recklessly: false }
     Initiates offering nourishment ceremony

POST /api/v1/nourishment/stones
     Body: { stones: ["stone-01"], recklessly: false, vacate: true }
     Initiates stone nourishment ceremony
```

### Ceremony Endpoints

```
GET  /api/v1/ceremonies
     Query: ?state=active|completed|failed
     Lists ceremonies

GET  /api/v1/ceremonies/:id
     Returns ceremony details

POST /api/v1/ceremonies/:id/cancel
     Cancels a ceremony

GET  /api/v1/ceremonies/:id/events
     SSE stream of ceremony events
```

### Harvest Endpoints

```
GET  /api/v1/harvests
     Lists available harvests

GET  /api/v1/harvests/:offering
     Lists harvests for specific offering

POST /api/v1/harvests/:offering/restore
     Body: { harvest_id: "..." }
     Restores from harvest

DELETE /api/v1/harvests
     Query: ?older_than=168h
     Prunes old harvests
```

### Store Endpoints

```
POST /api/v1/stored
     Body: { offering: "mongodb" }
     Creates stored offering

GET  /api/v1/stored
     Lists stored offerings

GET  /api/v1/stored/:id
     Returns stored offering details

POST /api/v1/stored/:id/plant
     Body: { stone: "stone-02" }
     Plants stored offering on target stone
```

### Transfer Endpoints

```
POST /api/v1/transfer/send
     Body: { offering: "mongodb", target_stone: "stone-02" }
     Initiates transfer to target

POST /api/v1/transfer/receive
     Multipart: manifest + archives
     Receives transfer from source
```

---

## Configuration

### moss.toml

```toml
[ceremonies]
# Maximum concurrent ceremonies this stone can coordinate
max_coordinating = 2
# Maximum ceremonies this stone can participate in
max_participating = 5
# Election timeout
election_timeout_seconds = 5
# Retry configuration
max_attempts = 3
retry_backoff_base_ms = 1000
retry_backoff_multiplier = 2.0

[ceremonies.nourish]
# Harvest directory
harvest_dir = "/var/lib/zen-garden/harvests"
# How long to keep harvests after successful nourish
harvest_retention_hours = 168  # 7 days
# Compression algorithm
harvest_compression = "zstd"
# Auto-prune old harvests
auto_prune = true

[ceremonies.vacate]
# Candidate selection
require_same_arch = true
min_available_memory_mb = 1024
min_available_disk_gb = 10
prefer_lower_load = true
# Transfer timeout
transfer_timeout_seconds = 3600

[ceremonies.store]
# Stored offerings directory
store_dir = "/var/lib/zen-garden/stored"
# Maximum stored offerings before warning
max_stored_offerings = 50
```

---

## Summary

### V0 Status (2026-01-24)

**Implemented:**
1. ✅ **Update detection** - Docker registry + firmware (fwupd) integration
2. ✅ **Digest comparison** - Proper image version resolution
3. ✅ **Constraint checking** - Hardware requirement validation
4. ✅ **Shared types** - DRY architecture via `garden_common`
5. ✅ **Interactive CLI** - Garden-wide reporting with per-stone breakdown
6. ✅ **REST API** - Detection endpoint + execution stubs

**What V0 achieves:**
- Accurate detection of available updates across all stones
- Identification of blocked updates with reasons (e.g., missing AVX)
- Foundation for V1 execution infrastructure

---

### V1 Specification (Planned)

This document defines the complete V1 nourishment system:

1. **Nourish command** - Safe updates with Zen and Normative syntax
2. **Ceremony policies** - Template-defined quiesce/resume hooks
3. **Harvest infrastructure** - Volume backup and rollback capability
4. **Stored offerings** - Portable container+data packages
5. **Vacate ceremony** - Zero-downtime stone maintenance
6. **Ceremony engine** - Multi-phase orchestration with Jobs
7. **Multi-phase roadmap** - From detection (done) to full orchestration

The V1 design prioritizes:
- **Safety** - Backup before update, automatic rollback
- **Honesty** - Clear reporting, no hidden failures
- **Intentionality** - Ceremonies are deliberate operations
- **Resilience** - Journal recovery, coordinator election

---

**Next Steps:** Implement Phase 1 (Harvest Infrastructure) to enable safe execution of detected updates.
