# Nourishment V0 - Viability Test Specification

**Date:** 2026-01-24  
**Purpose:** Zero-day viability test for unified software/firmware update system

## Overview

V0 nourishment provides a simple update listing and execution system for both software offerings and hardware firmware. It follows the distributed query pattern established by the `observe` command.

## Architecture

### Distributed Pattern (Following Observe)

1. **Local Check** - Each stone exposes update information
2. **Orchestration** - Tended stone queries all stones in parallel
3. **Aggregation** - Results collected and presented to user
4. **Execution** - Selected updates executed with live status

### Constraint Validation

Hardware requirements checked before presenting updates:
- CPU features (AVX, SSE4.2, etc.)
- Memory requirements
- Architecture compatibility (x86_64, ARM64)

Blocked updates shown separately with human-readable reason.

## API Structure

### Unified Update Model

Collection named `updates` (not "offerings" to avoid entity name collision).
Type-discriminated items for software vs firmware:

```json
{
  "stone_name": "stone-coral-prairie",
  "updates": {
    "available": [
      {
        "type": "offering",
        "name": "redis",
        "current": "7.2.3",
        "available": "7.2.4",
        "age_days": 45
      },
      {
        "type": "firmware",
        "name": "System BIOS",
        "device_id": "com.dell.bios",
        "vendor": "Dell Inc.",
        "current": "1.2.3",
        "available": "1.2.4",
        "requires_reboot": true
      }
    ],
    "blocked": [
      {
        "type": "offering",
        "name": "mongodb",
        "current": "5.0.9",
        "available": "7.0.5",
        "reason": "Requires AVX (CPU: Intel Celeron J4105)"
      }
    ]
  }
}
```

### Endpoints

#### Local Stone Check
```
GET /api/v1/nourishment/check
```
Returns updates available for this stone only.

#### Garden-Wide Check (Orchestrated)
```
GET /api/v1/garden/nourishment
```
Queries all stones in parallel, returns aggregated results.

#### Execute Update
```
POST /api/v1/nourishment/execute
{
  "updates": [
    { "type": "offering", "name": "redis" },
    { "type": "firmware", "device_id": "com.dell.bios" }
  ]
}
```
Returns job ID for status tracking.

#### Live Status Stream
```
GET /api/v1/nourishment/stream/:job_id
```
Server-Sent Events stream for execution progress.

## Implementation Plan

### Phase 1: Constraint Checking (~150 lines)
**File:** `src/moss/src/domain/constraints.rs` (NEW)

```rust
pub struct Requirements {
    pub cpu_features: Vec<String>,
    pub min_memory_mb: Option<u64>,
    pub architectures: Vec<String>,
}

pub enum ConstraintViolation {
    MissingCpuFeature { required: String, cpu_model: String },
    InsufficientMemory { required: u64, available: u64 },
    IncompatibleArchitecture { required: Vec<String>, current: String },
}

pub fn check_constraints(
    requirements: &Requirements,
    hardware: &HardwareInfo
) -> Result<(), ConstraintViolation>
```

### Phase 2: Nourishment API (~400 lines)
**File:** `src/moss/src/api/v1/nourishment.rs` (NEW)

- Local check: Query offering versions via Docker registry API, detect firmware updates
- Garden-wide: Parallel stone queries (like observe)
- Execute: Start job, return ID (supports multi-stone execution)
- Stream: SSE for live status

### Phase 3: Rake Command (~500 lines)
**File:** `src/rake/src/commands/nourish.rs` (NEW)

Flow:
1. Discovery (find tended stone)
2. Parallel checks (query all stones)
3. Display (grouped by stone, available/blocked)
4. Selection (interactive menu with select-all options)
5. Execute across multiple stones (with ESC to detach)

UI Elements:
- Group updates by stone
- Show type badges (📦 offering, 🔧 firmware)
- Indicate reboot requirements (⚠️)
- Display blocked items with reason
- Interactive selection with checkboxes
- **Select-all options**: "All offerings", "All stones", "All updates"
- Multi-stone execution with parallel status display
- ESC to detach from execution

### Phase 4: Wire Up (~50 lines)
- `src/moss/src/api/v1/mod.rs`: Add nourishment module
- `src/rake/src/main.rs`: Add Commands::Nourish handler

## Testing

### Manual Viability Tests

1. **Offering Update**
   ```bash
   garden-rake nourish
   # Select redis 7.2.3 → 7.2.4
   # Verify constraint checking
   # Verify execution
   ```

2. **Blocked Update**
   ```bash
   # On Celeron J4105 (no AVX)
   garden-rake nourish
   # Verify MongoDB 7.x shown as blocked
   # Verify reason displayed clearly
   ```

3. **Firmware Update** (Linux only)
   ```bash
   garden-rake nourish
   # Verify LVFS firmware detected
   # Verify reboot warning shown
   ```

4. **Multi-Stone**
   ```bash
   # On tended stone with 2+ stones
   garden-rake nourish
   # Verify updates grouped by stone
   # Verify parallel queries fast
   ```

5. **Detachment**
   ```bash
   garden-rake nourish
   # Start update
   # Press ESC
   # Verify detaches cleanly
   # Verify job continues
   ```

## Known Limitations (V0)

1. **No Rollback** - Harvest creation not implemented
2. **No Conflict Detection** - Dependency conflicts not checked
3. **No Scheduling** - Immediate execution only
4. **No Progress Aggregation** - Multi-stone execution shows individual streams, not aggregated view

## Future Phases

- **V1:** Add harvest creation and rollback capability
- **V2:** Query Docker registry for actual versions
- **V3:** Multi-stone coordinated updates
- **V4:** Dependency conflict detection
- **V5:** Full ceremony system with vacate/nourish/restore

## Estimated Implementation Time

- Phase 1 (Constraints): 1 hour
- Phase 2 (API + Docker Registry): 3 hours  
- Phase 3 (Rake + Multi-Stone): 3 hours
- Phase 4 (Wire-up): 30 minutes
- Testing: 1.5 hours

**Total:** ~9 hours for complete V0 implementation

## Success Criteria

✅ List software offerings with available updates  
✅ List firmware updates (Linux with LVFS)  
✅ Block incompatible updates with clear reason  
✅ Execute selected updates with live status  
✅ ESC detaches without stopping job  
✅ Multi-stone orchestration works  
✅ Hardware constraints validated correctly
