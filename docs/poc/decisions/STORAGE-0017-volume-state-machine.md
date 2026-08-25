---
audience: [developer, ai]
doc_type: decision
status: proposed
last_verified: 2026-04-04
---

# STORAGE-0017: Volume Domain Object with Encapsulated State Machine

**Date**: 2026-04-04
**Status**: Proposed
**Depends on**: STORAGE-0014 (Storage Platform Architecture)

## Context

### The Symptom

Unplugging a USB storage device produced an infinite stream of "Storage Removed"
ribbons on tty1, one every polling cycle. The platform monitor emitted repeated
`Disconnected` events for the same device path, and each one triggered
`StorageBank::on_vanished()`, which unconditionally emitted
`StorageChanged::Released` because it never checked whether the volume was
already offline.

A one-line guard (`if vol.state == VolumeState::Offline { return; }`) fixed the
immediate symptom, but the root cause is architectural: nothing prevents
arbitrary state overwrites on the `Volume` struct.

### Structural Debt

**Volume is a bag of public fields.** Any code holding `&mut Volume` can
directly assign `vol.state`, `vol.used_bytes`, `vol.management`, etc. There are
7 sites across 3 files that set `vol.state =`:

| Site | File | Transition |
|------|------|------------|
| `on_appeared` re-appeared path | bank.rs:120 | `→ Online` |
| `on_vanished` | bank.rs:157 | `→ Offline` |
| `reconcile` existing | collection.rs:62 | `→ Online` |
| `reconcile` departed | collection.rs:124 | `→ Offline` |
| `probe_health` zero capacity | volume.rs:262 | `→ Degraded` |
| `probe_health` success | volume.rs:266 | `→ Online` |
| `probe_health` probe error | volume.rs:270 | `→ Degraded` |

None of these check the current state before writing (except the ad-hoc guard
added for the ribbon bug). Invalid transitions are representable — e.g.,
`probe_health` could theoretically resurrect an Offline volume if the guard at
line 253 were removed.

**Events are emitted by the caller, not the object.** `StorageBank` decides
whether a state change is interesting and what event to emit. `Volume` has no
say. This means every new caller that mutates state must also remember to emit
the right event — a coupling that invites bugs.

**API handlers bypass the domain.** Rename, pin/unpin, set-roles, and
set-visibility operations in `api/v1/storage.rs` directly mutate `Volume` fields
and manually emit events. They don't go through `StorageBank`, so the domain
bridge is not the single authority it claims to be.

**`health_tick_all` holds the write lock for 5-10 seconds.** It iterates every
volume and calls `probe_health()` which does blocking disk I/O (`statvfs`) while
holding `Volumes.write().await`. Platform monitor events queue behind this lock,
delaying connect/disconnect detection.

### Design Principle Violated

The Zen Garden code standards (section 8) specify: *State machines as enums, not
flag fields.* The current `VolumeState` is an enum, but it's used as a mutable
field — the enum carries no enforcement. The standard intends that enum matching
is the *only* way to change state, making invalid transitions unrepresentable.

## Decision

### Volume becomes an opaque domain object with method-based state transitions.

**Fields go private.** Read access via getters. Mutation only through domain
methods that enforce valid transitions and return the resulting events.

**The Volume decides what changed.** OS facts flow in via methods. The Volume
inspects its current state, applies the transition if valid, and returns a
`Vec<StorageChanged>` describing what (if anything) changed. If nothing changed,
the vec is empty. The caller forwards events to the broadcast channel but never
decides what to emit.

**StorageBank becomes a router.** It receives OS events, looks up the Volume,
calls the appropriate method, and forwards returned events. It never inspects
Volume internals.

### API

```rust
impl Volume {
    // ── OS fact ingestion ───────────────────────────────────

    /// Device appeared. Transitions Offline → Online or is a no-op if
    /// already Online. Updates metrics. Returns events to emit.
    pub fn connect(&mut self, metrics: DiskMetrics) -> Vec<StorageChanged>;

    /// Device disappeared. Transitions Online|Degraded → Offline or is
    /// a no-op if already Offline. Returns events to emit.
    pub fn disconnect(&mut self) -> Vec<StorageChanged>;

    /// Periodic health observation. May transition Online ↔ Degraded.
    /// Never touches Offline volumes. Returns events only on actual
    /// state change.
    pub fn observe_metrics(&mut self, metrics: DiskMetrics) -> Vec<StorageChanged>;

    // ── Domain operations (from API handlers) ───────────────

    pub fn rename(&mut self, new_name: String) -> Vec<StorageChanged>;
    pub fn set_roles(&mut self, roles: Vec<String>) -> Vec<StorageChanged>;
    pub fn set_visibility(&mut self, visible: bool) -> Vec<StorageChanged>;
    pub fn pin(&mut self) -> Vec<StorageChanged>;
    pub fn unpin(&mut self) -> Vec<StorageChanged>;

    // ── Read access ─────────────────────────────────────────

    pub fn state(&self) -> &VolumeState;
    pub fn display_name(&self) -> &str;
    pub fn is_managed(&self) -> bool;
    pub fn is_online(&self) -> bool;
    pub fn capacity_bytes(&self) -> u64;
    pub fn used_bytes(&self) -> u64;
    // ... etc
}
```

### State Machine

```
              connect(metrics)
    Offline ──────────────────→ Online
       ↑                         ↑ ↓
       │ disconnect()            │ │ observe_metrics()
       │                         │ ↓
       ←──────────────────── Degraded
              disconnect()
```

- `connect()` from `Offline` → `Online` + emit `Connected`. From
  `Online`/`Degraded` → update metrics, no state change, no event
  (idempotent).
- `disconnect()` from `Online`/`Degraded` → `Offline` + emit `Released`.
  From `Offline` → no-op, empty vec.
- `observe_metrics()` from `Online` with bad metrics → `Degraded` + emit
  `HealthChanged`. From `Degraded` with good metrics → `Online` + emit
  `HealthChanged`. From `Offline` → no-op (never resurrects).

### Lock Hold Reduction

Today:
```rust
// Holds write lock for 5-10s (blocking I/O inside)
let mut map = volumes.write().await;
for vol in map.values_mut() {
    vol.probe_health(platform);  // calls statvfs, reads pin files
}
```

After:
```rust
// Phase 1: read lock, snapshot device paths
let paths: Vec<String> = volumes.read().await.keys().cloned().collect();

// Phase 2: no lock, parallel I/O
let metrics: Vec<(String, DiskMetrics)> = spawn_blocking(move || {
    paths.iter().map(|p| (p.clone(), measure_disk(p))).collect()
}).await;

// Phase 3: brief write lock, apply results
let mut events = Vec::new();
let mut map = volumes.write().await;
for (path, m) in metrics {
    if let Some(vol) = map.get_mut(&path) {
        events.extend(vol.observe_metrics(m));
    }
}
drop(map);

// Phase 4: no lock, emit events
for event in events {
    let _ = changed.send(event);
}
```

Write lock hold time drops from seconds to microseconds.

### Event Emission Authority

| Today | After |
|-------|-------|
| StorageBank emits events | Volume returns events, StorageBank forwards |
| API handlers emit events | API handlers call Volume methods, forward returned events |
| Background tasks emit Reclassified | Background tasks call Volume methods, forward returned events |
| 15+ emission sites | 1 forwarding site per caller (StorageBank, API handler) |

The broadcast channel stays in `StorageBank` (or `Storage`). Volume never
touches it — Volume is a pure domain object with no async, no channels, no Arc.
Fully testable with `assert_eq!(vol.connect(metrics), vec![StorageChanged::Connected { ... }])`.

## Consequences

### Positive

- **Invalid transitions become unrepresentable.** `disconnect()` on an Offline
  volume returns empty vec — no guard needed, no bug possible.
- **Event correctness is structural.** The Volume decides what changed. Callers
  can't emit wrong events because they don't decide what to emit.
- **Lock hold drops 1000x.** Health probing moves outside the lock. Connect and
  disconnect detection is no longer delayed by health ticks.
- **Testable without tokio.** Volume methods are synchronous, pure logic. Unit
  tests are `let events = vol.connect(metrics); assert!(...)`.
- **API handlers simplified.** Instead of field mutation + manual event emission,
  each handler calls one Volume method and forwards events.

### Negative

- **Migration touches many files.** Every site that currently does
  `vol.state =` or `vol.field =` must be converted to a method call. Estimated:
  bank.rs, collection.rs, volume.rs, storage.rs (API), storage_tasks.rs,
  storage_lifecycle.rs.
- **Getter boilerplate.** Private fields require explicit getters for read
  access. Mitigated by keeping getters minimal (`pub fn state(&self) ->
  &VolumeState`).
- **Serialization needs adjustment.** If fields are private, serde derive won't
  work directly. Use `#[serde(into = "VolumeSnapshot")]` or keep a separate
  serializable snapshot type.

### Neutral

- The `Volumes` type remains `Arc<RwLock<HashMap<String, Volume>>>`. The
  container doesn't change — only the Volume inside becomes opaque.
- `StorageChanged` enum doesn't change. The events are the same; only who
  produces them changes (Volume instead of callers).

## Migration Path

### Phase 1: Encapsulate state transitions

- Add `connect()`, `disconnect()`, `observe_metrics()` to Volume.
- Each method enforces the state machine and returns `Vec<StorageChanged>`.
- Convert `bank.rs` to call these methods instead of direct field access.
- Convert `collection.rs` reconcile and health_tick to use new methods.
- Fields remain `pub` temporarily for backward compatibility.

### Phase 2: Encapsulate domain operations

- Add `rename()`, `set_roles()`, `set_visibility()`, `pin()`, `unpin()`.
- Convert `api/v1/storage.rs` handlers to call Volume methods.
- Remove direct event emission from API handlers.

### Phase 3: Make fields private

- Change all `pub` fields to `pub(super)` or private.
- Add getters for read access.
- Fix compilation errors (each one reveals a coupling violation).
- Adjust serialization (snapshot type or serde attributes).

### Phase 4: Split health tick I/O

- Extract disk measurement out of the write lock.
- Read phase (paths), I/O phase (spawn_blocking), write phase (apply metrics).
- Remove the long lock hold from `health_tick_all`.

## Files Affected

| File | Change |
|------|--------|
| `src/moss/src/domain/storage/volume.rs` | State machine methods, private fields, getters |
| `src/moss/src/domain/storage/bank.rs` | Call Volume methods, forward returned events |
| `src/moss/src/domain/storage/collection.rs` | Reconcile and health_tick use new methods |
| `src/moss/src/api/v1/storage.rs` | Handlers call Volume methods instead of direct mutation |
| `src/moss/src/tasks/storage_tasks.rs` | Forward events from Volume methods |
| `src/moss/src/tasks/task_defs/storage_lifecycle.rs` | Health tick split into phases |
| `src/moss/src/infra/installer/tests.rs` | New unit tests for state machine transitions |
