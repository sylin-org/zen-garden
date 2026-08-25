---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-12
---

# STORAGE-0014: Storage Platform Architecture and Bounded Context Separation

**Date**: 2026-03-12
**Status**: Accepted
**Depends on**: STORAGE-0011 (Unified Storage Domain), STORAGE-0013 (Replica Set Identity)

## Context

### Structural Debt

Storage concerns accumulated in two catch-all files:

- `infra/storage/platform.rs` — 2,100+ lines mixing Linux and Windows volume detection,
  disk measurement, candidate enumeration, and hotplug event construction in a single file
  separated only by `#[cfg]` blocks.

- `domain/storage/mod.rs` — ~1,000 lines conflating physical device lifecycle (detect,
  classify, measure) with logical storage concerns (bank identity, role management,
  replica sets, I/O routing, health tracking).

Neither file has a clear aggregate boundary. Changes to Linux volume detection require
navigating Windows code and vice versa. Changes to rename logic require understanding
physical mounting code.

### The Symptom That Exposed the Problem

Storage ribbons displayed "0 B used" on first connect on both platforms. The root cause:

- `domain/storage/ingest_event()` received a `VolumeEvent::Appeared` and immediately
  emitted `StorageChanged::Connected { used_bytes: vol.used_bytes }`.
- `vol.used_bytes` was always 0 at appear-time because `VolumeSnapshot` carried no
  occupancy data — the snapshot was built from OS device detection, not from a disk
  usage measurement.
- On Linux, the udev hot-path (`build_snapshot_for_device`) never called `disk_usage()`.
  The polling fallback (`scan_volumes()`) did call `df`, firing a second `Connected`
  event ~10 seconds later with the real value — producing a double ribbon.
- On Windows, the API handler (`api/v1/storage.rs`) emitted a premature
  `Connected { used_bytes: 0 }` immediately on the `POST /storage/add` response path,
  before the volume scanner had run.

The fix required detecting that measurement (a platform concern) had been incorrectly
delegated to a heartbeat (`health_tick`) rather than being owned by the component that
detects the device.

### Five Distinct Concerns

Analysis identified five concerns that must not share a module:

**1. Physical Presence** — does the device exist on this machine right now?
Inputs are OS events (udev, WM_DEVICECHANGE, polling). Outputs are raw physical facts:
mount path, metrics. No knowledge of manifests, names, or replica sets.

**2. Storage Bank** — the logical, named unit of storage identified by a manifest GUIDv7.
Commands: Connect, Disconnect, Rename, SetRole, Pin, Unpin, SetVisibility.
Events: BankConnected, BankDisconnected, BankRenamed, RoleChanged, MetricsUpdated.

**3. Replica Set** — logical grouping of banks across stones and devices sharing content.
A replica set rename is a different operation from a bank rename: it must propagate
to all member banks (across all stones holding a replica).

**4. Storage Operations** — agnostic read/write/list/delete/mkdir.
Routes to the correct bank via replica set → role priority (Primary first, Dormant
as fallback). Has no knowledge of physical device paths or OS-level mounts.

**5. Replication** — change tracking and sync between Primary and Dormant replicas.
Already isolated in its own task. Left unchanged by this decision.

### Identity Layers

Three distinct identities must not be conflated:

| Layer | Identifier | Stability |
|---|---|---|
| Physical device | OS path (`/dev/sdb1`, `E:\`) | Volatile — changes on remount |
| Storage bank | GUIDv7 (manifest.id) | Permanent — lives in `.zen-garden/manifest.json` |
| Replica set | GUIDv7 (replica_set_id) | Permanent — shared across all member banks |

Neither renaming a bank nor renaming a replica set touches the physical device path.
Rename is a pure manifest operation.

## Decision

### Monitor Trait: Two Platform Implementations

Extract volume monitoring from `infra/storage/platform.rs` into a `VolumeMonitor` trait
with two platform-specific implementations:

```
infra/storage/
  monitor/
    mod.rs       — VolumeMonitor trait + PhysicalStorageEvent type
    linux.rs     — udev primary + polling fallback
    windows.rs   — polling (WM_DEVICECHANGE future enhancement)
```

The monitor trait:

```rust
pub trait VolumeMonitor: Send + Sync {
    fn start(self: Box<Self>, manager: Arc<StorageBankManager>, token: CancellationToken);
}
```

Each monitor's contract:
1. Detect device presence via platform-appropriate mechanism.
2. Measure occupancy (`disk_usage()`) before emitting — not after, not via heartbeat.
3. Call `manager.on_appeared(mount_path, metrics)` once, with complete data.
4. Call `manager.on_vanished(mount_path)` on disconnect.

The monitor knows nothing about manifests, names, roles, or domain events.

### PhysicalStorageEvent: Physical Facts Only

```rust
pub enum PhysicalStorageEvent {
    Appeared { mount_path: PathBuf, metrics: StorageMetrics },
    Vanished  { mount_path: PathBuf },
}

pub struct StorageMetrics {
    pub capacity_bytes: u64,
    pub used_bytes:     u64,
    pub available_bytes: u64,
}
```

`VolumeSnapshot` retains its existing fields for candidate listing and initial scan but
gains `used_bytes` so it is complete when passed to the domain.

### StorageBankManager: Domain Bridge

Add `domain/storage/bank.rs` with `StorageBankManager` as the single entry point for
physical events entering the domain:

```rust
impl StorageBankManager {
    /// Physical monitor detected a device and measured its occupancy.
    /// Reads the manifest at mount_path, establishes bank identity,
    /// upserts into the bank map, emits BankConnected.
    pub async fn on_appeared(&self, mount_path: PathBuf, metrics: StorageMetrics);

    /// Physical monitor detected a device removal.
    /// Marks the bank offline, emits BankDisconnected.
    pub async fn on_vanished(&self, mount_path: PathBuf);
}
```

`on_appeared` replaces `ingest_event`'s Appeared branch. `BankConnected` is the event
that triggers the ribbon — fired once, with real metrics.

### Rename: Two Explicit Commands

Bank rename and replica set rename are distinct commands with distinct propagation scope:

**Bank rename** — display name of one logical unit on one stone:
```
PATCH /api/v1/stone/storage/banks/{name}/rename
  → StorageBankManager::rename(bank_id, new_name)
  → updates local manifest
  → emits BankRenamed { old_name, new_name }
  → broadcasts storage beacon
```

**Replica set rename** — affects all member banks across all stones:
```
PATCH /api/v1/stone/storage/banks/{name}/rename  (body: { scope: "replica_set" })
  → StorageBankManager::rename_replica_set(replica_set_id, new_name)
  → updates local manifest for all locally-connected members
  → broadcasts rename beacon; peer stones apply on next beacon receipt
```

### Storage Operations: Routing Port

`domain/storage/operations.rs` owns read/write/list/delete routing:

```rust
pub struct StorageGateway { ... }

impl StorageGateway {
    pub async fn read(&self, replica_set: &str, path: &Path) -> Result<Bytes>;
    pub async fn write(&self, replica_set: &str, path: &Path, data: Bytes) -> Result<()>;
    pub async fn list(&self, replica_set: &str, path: &Path) -> Result<Vec<Entry>>;
    pub async fn delete(&self, replica_set: &str, path: &Path) -> Result<()>;
}
```

Routing logic: resolve replica set → find Primary bank → if offline, fallback to Dormant.
No knowledge of physical device paths. `ContentStore` remains the I/O adapter.

### Target Module Structure

```
infra/storage/
  monitor/
    mod.rs          — VolumeMonitor trait, PhysicalStorageEvent, StorageMetrics
    linux.rs        — udev + polling, disk_usage() before emit
    windows.rs      — polling, GetDiskFreeSpaceExW before emit
  platform.rs       — scan_volumes(), MediumSnapshot, candidate listing (no watcher code)
  beacon.rs         — unchanged
  watcher.rs        — unchanged (replication filesystem watcher)
  store.rs          — unchanged
  layout.rs         — unchanged
  adapter.rs        — unchanged

domain/storage/
  mod.rs            — Volumes type alias, re-exports, startup helpers
  bank.rs           — StorageBank aggregate, StorageBankManager, BankConnected/Disconnected/Renamed
  replica_set.rs    — ReplicaSet aggregate, rename propagation
  orchestration.rs  — role resolution (Primary election, pin logic)
  operations.rs     — StorageGateway (read/write/list/delete routing)
```

### Startup Wiring

```rust
// coordinator.rs — one monitor, selected at compile time
#[cfg(target_os = "linux")]
let monitor = Box::new(infra::storage::monitor::linux::LinuxVolumeMonitor::new());
#[cfg(target_os = "windows")]
let monitor = Box::new(infra::storage::monitor::windows::WindowsVolumeMonitor::new());

monitor.start(state.current.storage.bank_manager.clone(), token);
```

## Consequences

### Immediate Fixes

- Storage ribbon fires once, with real used bytes, on both platforms.
- Double ribbon on Linux eliminated: udev path measures before emitting.
- Premature "0 B" ribbon on Windows eliminated: API handler no longer emits `Connected`.
- `StorageChanged::Sensed` retained in the enum for other consumers but ignored by the
  console task (ribbon consumer).

### Structural Improvements

- `infra/storage/platform.rs` reduced from ~2,100 lines to candidate/scan helpers only.
- Linux and Windows monitoring code no longer share a file.
- `domain/storage/mod.rs` can be decomposed incrementally as bounded contexts are
  extracted to their own files.
- Rename scope (bank vs replica set) is explicit in the command model.

### Migration Path

This ADR describes the target architecture. Implementation is incremental:

1. **Phase 1** (current): Extract `monitor/` from `platform.rs`; add `StorageBankManager`
   with `on_appeared`/`on_vanished`; fix 0 B ribbon.
2. **Phase 2**: Extract `bank.rs` and `replica_set.rs` from `domain/storage/mod.rs`.
3. **Phase 3**: Extract `operations.rs` (StorageGateway routing) from scattered API handlers.
4. **Phase 4**: Explicit replica set rename command and cross-stone propagation protocol.

Existing `ingest_event`, `reconcile`, and `health_tick` remain in place during Phase 1
and are retired as each bounded context is extracted.
