---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-08
---

# STORAGE-0011: Unified Storage Domain with Platform Adapters

**Date**: 2026-03-08
**Status**: Accepted
**Supersedes**: STORAGE-0004 (resilience), STORAGE-0005 (manifest-first discovery), STORAGE-0007 (lifecycle objects)
**Evolves**: STORAGE-0009 (managed storage), STORAGE-0010 (unified add)

## Context

Storage detection and lifecycle management was Linux-only and scattered across six modules with overlapping responsibilities:

| Module | Responsibilities |
|--------|-----------------|
| `device.rs` | Device analysis (sysfs, lsblk, blkid) — Linux only |
| `monitor.rs` | USB hotplug via udev — Linux only |
| `registry.rs` | Mount scanning, auto-mount, mount tracking, stale cleanup — mostly Linux |
| `lifecycle.rs` | Health ticks using `/proc/mounts` and Linux shell commands |
| `coordinator.rs` | Three separate background tasks for mount persistence, hotplug, health |
| `managed_storage.rs` | Lifecycle objects (passive data structure, no coordination) |

Windows had zero working storage detection. `StorageRegistry::scan()` returned empty results because `get_device_for_mount()` returned `None` (skipping all banks as "not mounted") and `get_disk_usage()` returned `None` (skipping banks as "stale with 0 capacity").

The domain (`managed_storage.rs`) was a passive data structure. All coordination logic lived in infrastructure and task modules, with the same operations (scan, refresh, broadcast) reimplemented in 3-5 places.

Two separate collections tracked storage state: `managed_storages` for managed devices and `candidates_cache` for unmanaged removable devices. A third Linux-only `mount_tracker` handled mount persistence. These three maps tracked the same physical devices from different angles.

## Decision

### 1. Volume as the universal entity

A single `Volume` struct represents any accessible storage — USB drive, NAS mount, local directory. Whether Zen Garden manages it is an attribute (`management: Option<Management>`), not a separate type.

```
Volume
├── path            // device identifier ("/dev/sdb1" or "E:\")
├── mount_path      // where content is accessible
├── label, capacity_bytes, used_bytes, removable, online
├── health          // Healthy / Degraded / Lost
│
└── management: Option<Management>
    ├── id, name    // from .zen-garden/manifest.json
    ├── role        // Primary / Dormant (set by orchestration)
    ├── pin         // Optional GUIDv7 for last-pin-wins
    ├── visibility, roles, encrypted
    └── store       // ContentStore for I/O
```

One collection: `Volumes = Arc<RwLock<HashMap<String, Volume>>>`, keyed by device path. Replaces `managed_storages`, `candidates_cache`, and `mount_tracker`.

### 2. Platform adapters emit agnostic events

Thin, `#[cfg]`-gated adapters observe the OS and emit platform-agnostic events:

```rust
struct VolumeSnapshot { path, mount_path, label, capacity_bytes, removable }

enum VolumeEvent {
    Appeared(VolumeSnapshot),
    Disappeared { path: String },
}
```

Adapters never check manifests, never classify managed vs unmanaged, never emit domain events. They report what the OS sees.

- **Linux**: udev for hotplug, `/sys/block` + `/proc/mounts` for scan, `df` for usage
- **Windows**: `GetLogicalDrives` + `GetDriveType` for scan, polling for hotplug, `GetDiskFreeSpaceEx` for usage

### 3. Domain handles all classification and lifecycle

The storage domain receives adapter events and owns the full pipeline:

1. **Classify**: Read `.zen-garden/manifest.json` → set `management`
2. **Register**: Insert into `Volumes` map
3. **Health**: Probe capacity/mount liveness
4. **Orchestrate**: Resolve Primary/Dormant roles from local state + remote beacons
5. **Publish**: Build beacon data for UDP broadcast
6. **Deregister**: Remove on volume disappearance

### 4. Single background tick replaces scattered tasks

Three coordinator tasks (mount persistence 5s, hotplug detection 10s, health tick 10s) plus the orchestration scan (3s) collapse into one domain tick:

```
every 5s:
  adapter.scan_volumes()           → current OS state
  domain.reconcile(snapshots)      → detect appeared/disappeared
  domain.health_tick()             → probe all online volumes
  domain.resolve_roles(beacons)    → Primary/Dormant assignment
  domain.broadcast_if_changed()    → beacon
```

Orchestration nudge (immediate wakeup on beacon arrival) is preserved — the domain tick can be woken early.

### 5. Mount management stays in adapters

Linux mount/unmount operations (auto-mount, remount on failure) are adapter concerns. The domain sees volumes appear and disappear; it does not call `mount` or `umount`. On Windows, the OS handles mounting entirely — the adapter just reports drive letters.

## Consequences

### Removed
- `infra/storage/device.rs` — logic moves to platform adapter
- `infra/storage/monitor.rs` — logic moves to platform adapter
- `infra/storage/registry.rs` — scan logic moves to domain, manifest reading is a utility
- `domain/managed_storage.rs` — replaced by `Volume` in `domain/storage.rs`
- `AppState::candidates_cache` — `Volumes` map serves both managed and unmanaged queries
- `AppState::mount_tracker` — Linux mount persistence moves to adapter internals
- Three separate coordinator storage tasks — replaced by one domain tick

### Added
- `infra/storage/platform.rs` — OS-specific volume adapter
- `domain/storage.rs` — `Volume`, `Volumes`, domain logic (classify, health, orchestrate)

### Preserved
- `ContentStore`, `ObjectStore` — I/O layer unchanged
- `StorageService` — rewired to query `Volumes`
- Beacon protocol, replication engine, tick aggregator — unchanged
- `GardenRegistry` — remote stone tracking unchanged
- Role resolution algorithm (first-online-wins, pin precedence) — moved to domain method

### Risks
- **Device path semantics**: On Linux `path = "/dev/sdb1"`, on Windows `path = "E:\"`. Code that parsed `/dev/` prefixes was audited and updated.
- **Mount management gap**: If the Linux adapter doesn't auto-mount, the domain won't see prepared devices. Adapter must handle this internally before reporting `Appeared`.
