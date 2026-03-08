---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-02-17
---

# STORAGE-0007: Storage Lifecycle Objects

**Date**: 2026-02-17
**Status**: Accepted
**Depends on**: STORAGE-0004 (Resilience), STORAGE-0005 (Manifest-First Discovery), STORAGE-0006 (Replication & Roles)
**Depended on by**: STORAGE-0009 (Managed Storage and File Sharing — evolves the lifecycle model)

## Context

Live testing of the STORAGE-0006 pin redesign revealed a critical failure mode: a stone's portrait reported a seed bank as Primary and pinned (`pinned: true`, 869 GB capacity), but the underlying USB device was **not actually mounted**. The `pin()` handler wrote `pin.json` to an empty directory path without error — the bytes went nowhere. The mount point directory existed, but `df -h` showed no device attached.

### Root Cause: Scattered State

Seed bank state is spread across **six independent collections** in `AppState`, none of which share a lifecycle or verify consistency:

| Collection | Location | Purpose |
|------------|----------|---------|
| `seed_bank_cache: Vec<SeedBankInfo>` | `app_state.rs:207` | Cached scan results (identity, capacity) |
| `mount_tracker: MountTracker` | `app_state.rs:235` | Device → mount-path tracking |
| `seed_bank_roles: HashMap<String, SeedBankRole>` | `app_state.rs:240` | Role assignments (Primary/Dormant) |
| `seed_bank_pins: HashMap<String, String>` | `app_state.rs:246` | Pin-id per seed bank name |
| `storage_cache: StorageCache` | `app_state.rs:130` | Cross-stone beacon-fed topology |
| `SeedBankStore` (constructed ad-hoc) | `registry.rs`, handlers | Filesystem I/O (constructed from scan results per request) |

These are updated independently by different tasks on different intervals. Nothing ties "mount is live" to "pin write is safe". The orchestrator reads `seed_bank_roles` + `seed_bank_pins` + `storage_cache` in separate lock acquisitions — the set they see may be inconsistent.

### Why This Can't Be Fixed Incrementally

Adding a mount-check before pin writes would fix the immediate symptom. But the same class of bug exists for every I/O operation: nurturing writes, replication syncs, changelog appends. Each would need its own mount guard, all implemented independently, all with the same risk of drift. The problem is structural — there is no single object responsible for the lifecycle of a seed bank.

## Decision

### Two-Layer Composition Model

Introduce two persistent objects following DDD/SoC principles:

```
┌─────────────────────────────────────────────────────┐
│  SeedBank (Domain)                                   │
│                                                      │
│  name, id, short_id                                  │
│  role: SeedBankRole                                  │
│  pin: Option<PinState>                               │
│  store: SeedBankStore                                │
│  replication_cursor: Option<String>                   │
│                                                      │
│  ┌─────────────────────────────────────────────────┐ │
│  │  Storage (Infrastructure)                        │ │
│  │                                                  │ │
│  │  device: String        ("/dev/sda1")             │ │
│  │  mount_path: PathBuf                             │ │
│  │  health: StorageHealth                           │ │
│  │  capacity_bytes: u64                             │ │
│  │  used_bytes: u64                                 │ │
│  │  filesystem: String                              │ │
│  │                                                  │ │
│  │  ensure_mounted() → Result<()>                   │ │
│  │  health_tick() → StorageHealth                   │ │
│  │  remount() → Result<()>                          │ │
│  └─────────────────────────────────────────────────┘ │
│                                                      │
│  pin() → ensures storage mounted first               │
│  unpin()                                             │
│  write() → delegates to store after mount check      │
│  reconcile_pin() → re-reads pin.json from disk       │
│  health_tick() → storage.health_tick() + domain      │
└─────────────────────────────────────────────────────┘
```

#### Layer 1: `Storage` (Infrastructure)

Owns the physical device lifecycle. Knows nothing about seed banks, roles, or replication.

```rust
pub struct Storage {
    pub device: String,           // "/dev/sda1"
    pub mount_path: PathBuf,      // "/var/lib/zen-garden/mounts/seed-clear-valley/019c0789/"
    pub health: StorageHealth,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
    pub filesystem: String,       // "ext4", "vfat"
}

pub enum StorageHealth {
    Healthy,
    Degraded(String),   // mounted but showing issues (capacity 0, I/O errors)
    Unmounted,          // mount point exists but device detached
    Lost,               // device no longer visible in /dev
}

impl Storage {
    /// Check /proc/mounts + liveness probe.  If Unmounted, attempt remount.
    pub async fn ensure_mounted(&mut self) -> Result<()> { ... }

    /// Periodic health check — called every ~10s.
    pub async fn health_tick(&mut self) -> StorageHealth { ... }

    /// Force remount after detecting Unmounted/Lost.
    async fn remount(&mut self) -> Result<()> { ... }
}
```

**Self-healing**: `health_tick()` reads `/proc/mounts` and probes disk capacity. If the device disappeared, it transitions to `Unmounted` and attempts `remount()`. If `remount()` fails (device physically removed), it transitions to `Lost`. Recovery from `Lost` requires re-detection via hotplug.

#### Layer 2: `SeedBank` (Domain)

Composes a `Storage` instance. Adds identity, role, pin, replication, and I/O store.

```rust
pub struct SeedBank {
    // Identity
    pub name: String,
    pub id: String,         // GUIDv7
    pub short_id: String,   // first 8 hex chars

    // Domain state
    pub role: SeedBankRole,
    pub pin: Option<PinState>,
    pub encrypted: bool,

    // Infrastructure (composed)
    pub storage: Storage,
    pub store: SeedBankStore,

    // Replication
    pub replication_cursor: Option<String>,  // changelog timestamp
}

pub struct PinState {
    pub pin_id: String,     // GUIDv7
    pub pinned_at: String,  // ISO 8601
}

impl SeedBank {
    /// Construct from a prepared Storage + manifest.
    pub fn from_storage(storage: Storage, manifest: &SeedBankManifest) -> Self { ... }

    /// Pin this bank as Primary. Verifies mount, writes pin.json, updates role.
    pub async fn pin(&mut self, pin_id: String) -> Result<()> {
        self.storage.ensure_mounted().await?;   // ← the fix
        self.store.write_pin(&pin_id)?;
        self.pin = Some(PinState { pin_id, pinned_at: now_iso() });
        self.role = SeedBankRole::Primary;
        Ok(())
    }

    /// Periodic domain-level health check.
    pub async fn health_tick(&mut self) {
        self.storage.health_tick().await;
        if matches!(self.storage.health, StorageHealth::Healthy) {
            self.reconcile_pin().await;
        }
    }

    /// Re-read pin.json from disk to detect external changes.
    async fn reconcile_pin(&mut self) { ... }
}
```

### AppState Consolidation

Replace the six scattered collections with a single coherent map:

```rust
// Before (6 independent collections):
pub seed_bank_cache:  Arc<RwLock<Vec<SeedBankInfo>>>,
pub mount_tracker:    MountTracker,
pub seed_bank_roles:  Arc<RwLock<HashMap<String, SeedBankRole>>>,
pub seed_bank_pins:   Arc<RwLock<HashMap<String, String>>>,
pub storage_cache:    StorageCache,
// + SeedBankStore constructed ad-hoc per request

// After (2 collections):
pub seed_banks: Arc<RwLock<HashMap<String, SeedBank>>>,         // keyed by id
pub storage_candidates: Arc<RwLock<Vec<Storage>>>,               // detected but not prepared
// storage_cache remains — it tracks remote stones' seed banks via beacons
```

The `storage_cache` (beacon-fed cross-stone topology) remains separate — it represents external state from other stones, not local lifecycle objects. Remote seed banks are still `SeedBankInfo` structs from beacon data.

### Lifecycle Flow

```
USB device detected
        │
        ▼
  Storage::new(device, mount_path)
        │
        ▼
  Has .zen-garden/manifest.json?
       / \
      /   \
    Yes    No
     │      │
     ▼      ▼
  SeedBank::from_storage()    → storage_candidates
     │
     ▼
  seed_banks.insert(id, bank)
     │
     ▼
  Role assignment (orchestration)
     │
     ├── Primary: accept writes, serve API
     │
     └── Dormant: connect to Primary's SSE stream
                  replicate on changelog events
```

**Dormant initialization**: A Dormant seed bank initializes fully — it has a `SeedBank` object with `Storage`, `SeedBankStore`, health ticks, everything. The only difference is behavioral: instead of accepting direct writes, it connects to the Primary stone's SSE stream and replicates changelog events. No second-class objects.

### Health Tick Architecture

A single coordinator tick (every ~10s) iterates all local `SeedBank` objects:

```
health_tick() for each SeedBank:
    1. storage.health_tick()          → check /proc/mounts, probe capacity
    2. if Unmounted → storage.remount() → if fail → mark Lost
    3. if Healthy → reconcile_pin()   → re-read pin.json, detect drift
    4. Update portrait fields         → API reflects truth
```

This eliminates the possibility of stale state: the portrait always reflects the actual device health because it reads from the `SeedBank` object, which is continuously reconciled against the physical device.

### What Gets Absorbed

| Current Component | Absorbed Into | Notes |
|-------------------|--------------|-------|
| `SeedBankRegistry::scan()` | `Storage` detection + `SeedBank::from_storage()` | No more repeated filesystem scans |
| `MountTracker` | `Storage.mount_path` + `Storage.health` | Mount state lives on the object |
| `seed_bank_cache: Vec<SeedBankInfo>` | `seed_banks: HashMap<String, SeedBank>` | One source of truth |
| `seed_bank_roles: HashMap` | `SeedBank.role` | Per-object, not a parallel map |
| `seed_bank_pins: HashMap` | `SeedBank.pin: Option<PinState>` | Per-object, not a parallel map |
| `SeedBankStore` (ad-hoc) | `SeedBank.store` | Created once, lives on object |
| Orchestration role assignment | Operates on `SeedBank` objects directly | Reads/writes `bank.role` |
| Replication cursor tracking | `SeedBank.replication_cursor` | Per-bank, persistent |

### Future Extensibility

The `Storage` layer is deliberately domain-agnostic. If a future feature like `HarvestStore` needs its own persistent backing device, it can compose `Storage` independently:

```rust
pub struct HarvestStore {
    pub storage: Storage,
    // ... harvest-specific domain fields
}
```

Same self-healing, same health ticks, same mount verification — no code duplication.

## Consequences

### Positive

- **Mount-loss bugs become structurally impossible**: Every I/O path goes through `storage.ensure_mounted()`. A dead mount is detected before bytes are written, not after.
- **Single source of truth**: One `SeedBank` object holds identity + role + pin + health + store. No cross-map consistency bugs.
- **Reduced lock contention**: One lock acquisition to read a `SeedBank` instead of 3-4 separate `RwLock` acquisitions.
- **Self-healing**: Transient mount loss is automatically recovered. Only physical device removal causes `Lost` state, which is correctly reflected in the portrait.
- **Simpler API handlers**: Pin handler calls `bank.pin(id)` — mount check, disk write, state update are encapsulated. No scattered ceremony.
- **Testability**: `Storage` and `SeedBank` can be unit-tested independently. Mock `Storage` for domain tests; mock filesystem for infrastructure tests.

### Negative

- **Migration effort**: Touching 6+ files that currently read from scattered collections. Orchestration, replication, handlers, coordinator, portrait builder all need updating.
- **Lock granularity trade-off**: One `RwLock<HashMap<String, SeedBank>>` vs. six independent locks. Write contention on the map increases, but read paths simplify dramatically. If contention becomes an issue, per-bank `RwLock` can be introduced (bank-level sharding).

### Neutral

- **`StorageCache` remains**: Cross-stone beacon topology is still needed for role assignment and replication target resolution. It's external state, not local lifecycle.
- **`SeedBankInfo` survives in common crate**: Still used for beacon payloads and API responses (serialization DTO). The domain `SeedBank` is moss-internal; `SeedBankInfo` is the wire format.

## Migration Strategy

1. **Add `Storage` struct** to `moss::infra::storage` — infrastructure layer, device lifecycle
2. **Add `SeedBank` struct** to `moss::domain::seed_bank` — domain layer, composes Storage
3. **Add `seed_banks: HashMap<String, SeedBank>`** to `AppState`
4. **Migrate detection**: `SeedBankRegistry::scan()` → construct `Storage` → check manifest → `SeedBank::from_storage()`
5. **Migrate orchestration**: Read/write `bank.role` directly instead of `seed_bank_roles` map
6. **Migrate pin handlers**: Call `bank.pin(id)` / `bank.unpin()` instead of manual store construction + map updates
7. **Migrate replication**: Read `bank.replication_cursor`, write through `bank.store`
8. **Migrate portrait builder**: Read from `SeedBank` objects — health, role, pin all in one place
9. **Remove old collections**: `seed_bank_cache`, `mount_tracker`, `seed_bank_roles`, `seed_bank_pins` — replaced by `seed_banks`
10. **Health tick integration**: Single coordinator tick iterates `seed_banks` and calls `bank.health_tick()`

## Related

- [STORAGE-0004](STORAGE-0004-seedbank-resilience.md) — Resilience patterns (mount verification, hotplug) — absorbed into `Storage`
- [STORAGE-0005](STORAGE-0005-manifest-first-discovery.md) — Manifest-first discovery — `SeedBank::from_storage()` reads manifest
- [STORAGE-0006](STORAGE-0006-seed-bank-replication.md) — Replication, roles, pin redesign — `SeedBank` owns role + pin + replication cursor
