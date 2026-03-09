---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-09
---

# STORAGE-0013: Replica Set Identity and Storage Naming

**Date**: 2026-03-09
**Status**: Accepted
**Supersedes**: STORAGE-0006 §1 (Identity Model — "Name IS the FQN"), STORAGE-0006 §3 (Prepare Flow)
**Depends on**: STORAGE-0006 (Replication mechanics), STORAGE-0009 (Managed Storage), STORAGE-0012 (Cloud Filter)
**Approach**: Break-and-rebuild. No shims, no backwards compatibility. All deprecated code paths removed.

## Context

### The Conflation Problem

STORAGE-0006 established that the storage `name` field serves triple duty:

1. **Device display name** — human-readable label for a physical device ("seed-clear-valley")
2. **Replica group key** — two devices with the same name are replicas
3. **Cloud Filter folder name** — Explorer shows each name as a folder

This conflation creates cascading failures:

- **Rename breaks replication.** Renaming a Primary's `name` silently splits the replica group. Dormant replicas still carry the old name in their manifests and can no longer find their Primary. The replication task (`storage_replication.rs:175`) routes via `route_to_primary(name)` — after rename, this returns nothing for Dormants.

- **Rename propagation is absent.** The rename API handler (`api/v1/storage.rs:588`) updates the local manifest and broadcasts a beacon, but Dormant replicas have no protocol to detect or apply the name change.

- **No federation concept.** Users think in terms of logical storage spaces ("my photos", "shared storage"), not individual device names. Two replicas of the same content appear as two separate folders in Explorer. The system has no representation of a "storage space" as a first-class entity.

- **Cloud Filter shows wrong abstraction.** Explorer displays individual device names (`seed-clear-valley`, `seed-gentle-brook`) instead of the logical space the user cares about. Each `storage add` with a random name creates a new placeholder folder under `%USERPROFILE%\Zen Garden\` that accumulates across invocations, shows permanent "sync pending", and blocks rename with CfApi error `0x8007017B`.

- **Pinned Primary serves stale data.** A pinned device currently asserts Primary immediately on reconnect (`storage_orchestration.rs` — pinned never yields in `resolve_role()`), before syncing changes it missed while offline.

### The Missing Abstraction

Storage devices are analogous to stones — each is a unique physical unit with its own identity. But stones serve a garden (a higher-level grouping). Storages lack this higher-level grouping. The mental model should be:

> "The default storage is composed of: seed-01 (50 GB) and seed-02 (1 TB)."

Where "default storage" is a **replica set** — a logical storage space served by one or more physical devices. The user sees and interacts with replica sets. Device names are operational details visible in management UIs.

A single stone can host devices belonging to different replica sets (e.g., one USB drive in the default `storage` set and another in `storage::images`).

### Current Code Inventory (Name-as-Group-Key)

Every function below uses `name` as the replica group key and must be rebuilt:

**Domain:**
- `storage.rs:715` — `find_by_name(volumes, name)` searches `management.name`
- `storage.rs:741` — `roles_snapshot()` returns `HashMap<name, role>`
- `storage.rs:753` — `pins_snapshot()` returns `HashMap<name, pin_id>`
- `storage_service.rs:114/130/216` — `resolve_read/write/find_local(name)`
- `garden_registry.rs:538/552/568` — `storage_by_name/primary/route_to_primary(name)`

**Orchestration:**
- `storage_orchestration.rs:155` — `for name in &local_names` groups by name
- `storage_orchestration.rs:158` — `find_remote_primary_with_pin(&reg, name)`
- `storage_orchestration.rs:239` — applies role where `mgmt.name == *name`

**Replication:**
- `storage_replication.rs:142` — `for (name, id, mount_path)` syncs by name
- `storage_replication.rs:175` — `route_to_primary(name)` for sync target

**Infrastructure:**
- `beacon.rs:39` — `stamp_announcement(&mut ann, &info.name, roles, pins)`
- `cloud_filter/mod.rs:221` — `collect_storage_names()` unions `mgmt.name` + `entry.tool.fqid`
- `tools/projector.rs:45` — `canonical_storage_name(&mgmt.name)` for registry key

**API:**
- `api/v1/storage.rs` — all 15+ handlers route by `{name}` URL parameter
- `api/v1/garden_storage/mod.rs` — `discover_v1(name)` finds replicas by name

## Decision

### 1. Two-Level Identity Model

Separate storage unit identity from replica set identity. Both levels have an immutable GUIDv7 and a mutable display name (sugar).

#### Storage Unit (physical device)

Each physical storage device (USB drive, partition, directory) has its own identity, like a stone.

| Field | Type | Purpose |
|-------|------|---------|
| `id` | GUIDv7 | Immutable. Unique per physical device. Never changes. |
| `name` | String | Display sugar. e.g., `"seed-01"`, `"seed-primary"`. Renameable locally. |

The `name` is purely cosmetic — for operational visibility ("the default storage is composed of seed-01 and seed-02"). It has no role in replication, routing, or orchestration.

#### Replica Set (logical storage space)

Multiple devices form a replica set — a logical storage space that users interact with.

| Field | Type | Purpose |
|-------|------|---------|
| `replica_set_id` | GUIDv7 | Immutable. Unique per replica set. Shared by all member devices. The binding key for replication, orchestration, and routing. |
| `replica_set_name` | String | Instance name (sugar). `""` = default set. Renameable; propagates to all members. |
| `replica_set_name_updated_at` | DateTime\<Utc\> | Timestamp of last rename. Enables offline catch-up. |

### 2. Replica Set FQN Convention

Replica set names follow the offering FQN pattern:

| `replica_set_name` value | Full FQN | Cloud Filter folder | Notes |
|--------------------------|----------|---------------------|-------|
| `""` (empty/null) | `storage` | `storage` | Default set. Reserved moniker. |
| `"images"` | `storage::images` | `images` | Named set. |
| `"personal"` | `storage::personal` | `personal` | Named set. |

- The `storage` moniker is **reserved** for the default set (empty `replica_set_name`).
- Named sets use `storage::{name}` as the full FQN but display only the instance name in Cloud Filter and user-facing UIs.
- The manifest stores only the instance part (`""`, `"images"`, `"personal"`). The `storage::` prefix is constructed at display time, mirroring how offering FQNs work.

### 3. Manifest Structure (v5, Greenfield)

No migration from v4. Existing manifests are invalid and must be recreated. This is a break-and-rebuild.

```rust
pub struct StorageManifest {
    pub version: u32,                              // 5

    // Device identity
    pub id: String,                                // GUIDv7 per physical device
    pub name: String,                              // device display sugar

    // Replica set identity
    pub replica_set_id: String,                    // GUIDv7, immutable binding key
    pub replica_set_name: String,                  // instance name ("" = default)
    pub replica_set_name_updated_at: DateTime<Utc>, // for offline rename catch-up

    // Existing fields (unchanged)
    pub visibility: StorageVisibility,
    pub origin_stone: String,
    pub filesystem: String,
    pub created_at: DateTime<Utc>,
    pub encrypted: bool,
    pub pond_fingerprint: Option<String>,
    pub roles: Vec<String>,
}
```

**Removed:**
- `DEFAULT_PUBLIC_STORAGE_NAME` constant ("zen-garden") — replaced by default replica set
- `DEFAULT_PRIVATE_STORAGE_NAME` constant ("private") — replaced by named replica set
- `StorageManifest::logical_name()` — the name is just `name`, no logic needed

### 4. Binding Key Changes

`replica_set_id` replaces `name` as the universal grouping key. No code path uses `name` for grouping, routing, or replication.

| System | Before (removed) | After |
|--------|-------------------|-------|
| Orchestration (Primary/Dormant) | Group by `name` | Group by `replica_set_id` |
| Replication sync | `route_to_primary(name)` | `route_to_primary_by_set(replica_set_id)` |
| Beacon announcement | `name` as identifier | `replica_set_id` + `replica_set_name` |
| Registry `fqid` | Storage `name` | `replica_set_id` |
| Cloud Filter display | `mgmt.name` | Deduplicated `replica_set_name` per unique `replica_set_id` |
| API routing | `/banks/{name}` matches `mgmt.name` | `/banks/{set_name}` matches `replica_set_name` display FQN |
| `roles_snapshot()` | `HashMap<name, role>` | `HashMap<replica_set_id, role>` |
| `pins_snapshot()` | `HashMap<name, pin_id>` | `HashMap<replica_set_id, pin_id>` |
| `find_by_name()` | Searches `mgmt.name` | **Removed.** Use `find_by_set_id()` or `find_by_set_name()`. |

### 5. Rename Semantics

Two distinct rename operations. Both are announced via beacon/chirp.

#### 5a. Rename Storage Unit (device-level)

Changes the device display name (e.g., `seed-01` → `seed-primary`).

- Updates `manifest.name` on the local device
- Announced via beacon so other stones update their view of this device
- Does **NOT** affect the replica set, replication, or Cloud Filter
- Scope: local stone only

#### 5b. Rename Replica Set (set-level, propagated)

Changes the logical storage space name (e.g., `storage::photos` → `storage::memories`).

- Updates `replica_set_name` + `replica_set_name_updated_at` on the local manifest
- Broadcasts beacon with new name + timestamp
- **Online members:** Dormants receive the beacon, match by `replica_set_id`, detect `replica_set_name_updated_at` is newer, update their local manifests and write to disk
- **Offline members:** On reconnect, the Dormant's replication handshake compares `replica_set_name_updated_at` with the Primary's. If the Primary's timestamp is newer, the Dormant adopts the new name before starting sync.
- Updates Cloud Filter placeholder (old folder removed, new folder created)

Renaming the default set (`""` → `"photos"`) is allowed. The `storage` moniker moves to whichever set has `replica_set_name = ""`. If no set has an empty name, the `storage` moniker is unused.

### 6. Pin Refinement — Primary-Designate

A pinned device is a **Primary-designate**, not an immediate Primary. This prevents serving stale data after offline periods.

**Scenario:** seed-02 is pinned as Primary. It goes offline. While offline, seed-01 and seed-03 use first-online-wins among themselves. When seed-02 comes back:

1. seed-02 joins as **Dormant** (does NOT assert Primary despite pin)
2. seed-02 pulls changelog from the current Primary until caught up
3. Once caught up (cursor matches Primary's head cursor), seed-02 asserts **Primary** via its pin
4. seed-01/seed-03 detect the pinned assertion, cede to **Dormant**, enter replication mode

**Unpinned behavior** remains unchanged from STORAGE-0006 §2:
- First-online-wins
- 3-second startup reconciliation window
- Dual-primary resolution: lower `stone_id` yields
- Last-pin-wins with GUIDv7 comparison for conflicting pins

### 7. `storage add` — Default Set Join

New storages join the **default replica set** (`replica_set_name = ""`, FQN: `storage`) unless the user specifies otherwise.

#### API (non-interactive)

```json
{
  "target": "G:\\",
  "replica_set_id": null,
  "replica_set_name": null
}
```

| Request | Behavior |
|---------|----------|
| Both null/omitted | Join default set. If no default set exists, create one (generate `replica_set_id`). |
| Explicit `replica_set_id` | Join that specific set. |
| Explicit `replica_set_name` (no ID) | Find set by display name across garden, join it. Error if not found. |

#### CLI Wizard (interactive)

When the garden has multiple replica sets, `rake storage add` presents a picker:

```
Available storage sets:
  1. storage        (default, 2 devices, 1.5 TB)
  2. images         (1 device, 4 TB)
  3. personal       (1 device, 500 GB)
  4. Create new set...

Which set should this device join? [1]:
```

- Option 4 prompts for a name and checks for collisions garden-wide
- When offline or only one set exists (the default), skip the wizard — join default silently
- The wizard queries `GET /api/v1/garden/storage/sets` to populate the menu

#### New API Endpoint

```
GET /api/v1/garden/storage/sets
```

Returns all known replica sets aggregated across the garden:

```json
{
  "sets": [
    {
      "replica_set_id": "019c6d5a-...",
      "replica_set_name": "",
      "display_fqn": "storage",
      "member_count": 2,
      "total_capacity_bytes": 1610612736,
      "total_used_bytes": 524288000,
      "primary_stone": "stone-crystal-forest",
      "primary_device_name": "seed-01"
    }
  ]
}
```

### 8. Set Operations

#### 8a. Split (move device to new set)

A device leaves its current set and forms a new set. **Files are kept** — they become the founding content of the new set. This is a fork, not a migration.

- Generates new `replica_set_id` (GUIDv7)
- Accepts a new `replica_set_name` (required — the new set needs a name)
- Old set loses this member; replication among remaining members continues
- The device becomes the sole member (and thus Primary) of the new set
- Its existing files are the set's initial content

#### 8b. Join (add device to existing set)

A device joins an existing set. Two modes:

**Wipe + join (clean slate):**
- Device content is wiped
- Becomes empty Dormant
- Pulls everything from Primary via normal replication
- Safe, simple, recommended for most cases

**Merge + join (content union):**
- Device keeps its existing files
- Joins as Dormant
- Initial replication sync performs a full directory walk + hash comparison:
  - Files on device but not on Primary → treated as external writes (changelog entries generated, pushed to Primary)
  - Files on Primary but not on device → pulled down to device
  - Conflicts (same path, different content) → **Primary wins** (last-write-wins by timestamp)
- The joining device always enters as Dormant; orchestration resolves roles normally

### 9. Cloud Filter Changes

Cloud Filter shows **one folder per unique replica set**, not per device.

| Before | After |
|--------|-------|
| `seed-clear-valley/` | `storage/` |
| `seed-gentle-brook/` | `images/` |
| (two folders for replicated content) | (one folder per logical space) |

#### Implementation

- `collect_storage_names()` → `collect_replica_set_names()`: enumerates unique `replica_set_name` values from local volumes + registry, deduplicated by `replica_set_id`
- `list_storages()` in `provider.rs` → returns unique replica set display names
- Placeholder creation uses the Cloud Filter display name (`"storage"`, `"images"`)
- The `storage_watcher` reconcile loop:
  - On startup, seeds `known` from existing placeholder directories on disk (fixes stale accumulation bug)
  - Detects renames (old name disappears from `current`, new name appears) and removes/creates placeholders accordingly

### 10. Beacon Evolution

`StorageAnnouncement` carries full identity at both levels:

```rust
pub struct StorageAnnouncement {
    // Device identity
    pub id: String,                                // device GUIDv7
    pub name: String,                              // device display sugar

    // Replica set identity
    pub replica_set_id: String,                    // immutable binding key
    pub replica_set_name: String,                  // display sugar (instance name)
    pub replica_set_name_updated_at: DateTime<Utc>, // for offline rename catch-up

    // Runtime state (unchanged)
    pub role: StorageRole,
    pub visibility: String,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
    pub encrypted: bool,
    pub pin_id: Option<String>,
    pub roles: Vec<String>,
    pub protocols: Vec<String>,
}
```

Receiving stones:
- Use `replica_set_id` for orchestration role resolution
- Compare `replica_set_name_updated_at` for rename catch-up
- Use `id` + `name` for device-level visibility

### 11. Architecture: DDD Boundaries and Event-Driven Design

This section is the **gold standard mandate** for the storage subsystem. No exceptions.

#### 11.1. Core Principle: AppState Is the Boundary

**AppState** is the single boundary between layers. All infrastructure, tasks, and API handlers access domain state through AppState methods. No layer holds or reads raw domain aggregates directly.

```
┌──────────────────────────────────────────────────────────┐
│  API Handlers (thin)                                     │
│    ↓ calls                                               │
│  AppState                                                │
│    ↓ delegates to                                        │
│  Domain Services (StorageService, GardenRegistry)        │
│    ↓ owns                                                │
│  Aggregates (Volumes, Registry) — private, never exposed │
└──────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────┐
│  Tasks (orchestration, replication, nurturing)            │
│    ↓ calls                                               │
│  AppState                                                │
│    ↓ delegates to                                        │
│  Domain Services                                         │
└──────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────┐
│  Infrastructure (beacon, cloud filter, watcher)          │
│    ↓ subscribes to events from                           │
│  AppState broadcast channels                             │
│    ↓ pulls data from                                     │
│  AppState query methods                                  │
└──────────────────────────────────────────────────────────┘
```

**Rules:**
1. `Volumes` and `GardenRegistry` are **private** on AppState. No `pub` access.
2. All reads go through AppState methods that delegate to domain services.
3. All writes go through AppState methods that delegate to domain services.
4. No layer constructs `StorageService` inline from raw references.
5. Tasks call `state.storage_*()` methods. Never `state.volumes.read()`.
6. Infra subscribes to AppState broadcast channels. Never polls aggregates.
7. API handlers call `state.storage_*()` methods. Never inline domain logic.

#### 11.2. Core Principle: Event-Driven, Not Polling

**No polling.** Every component subscribes to domain events and reacts. Pulling fresh data from the AppState boundary after receiving an event is correct. Polling aggregates on a timer is not.

**Event channels on AppState:**

| Channel | Type | Emitted When | Consumers |
|---------|------|--------------|-----------|
| `storage_changed_tx` | `broadcast::Sender<StorageChanged>` | Volume add/remove/reclassify, role change, rename, pin/unpin | Cloud Filter watcher, filesystem watcher reconcile, metrics |
| `storage_tick_tx` | `broadcast::Sender<StorageTick>` | Changelog entry written (API write or fs-watcher) | Replication task, Cloud Filter provider |
| `orchestration_nudge_tx` | `mpsc::Sender<()>` | Beacon received, storage added, pin changed | Orchestration task |
| `volume_event_tx` | `broadcast::Sender<VolumeEvent>` | OS-level mount/unmount/change | Volume reconcile loop |

**Pattern for all consumers:**

```rust
// CORRECT — event-driven with boundary pull
loop {
    tokio::select! {
        _ = shutdown.cancelled() => break,
        _ = storage_changed_rx.recv() => {
            // React: pull fresh state from boundary
            let sets = state.list_replica_sets().await;
            // Do work with fresh data
            reconcile_placeholders(&sets).await;
        }
    }
}

// WRONG — polling aggregates
loop {
    tokio::time::sleep(Duration::from_secs(10)).await;
    let map = state.volumes.read().await;  // NO: direct aggregate
    // compute from raw data                // NO: domain logic in infra
}
```

#### 11.3. AppState Storage API Surface

AppState exposes these methods. All delegate to domain services internally. Grouped by concern:

**Query — Replica Sets:**
- `list_replica_sets() -> Vec<ReplicaSetSummary>` — all known sets (local + registry)
- `get_replica_set(replica_set_id) -> Option<ReplicaSetDetail>` — single set with members
- `get_replica_set_by_name(name) -> Option<ReplicaSetDetail>` — lookup by display name

**Query — Storage Devices:**
- `list_managed_storages() -> Vec<ManagedStorageInfo>` — all local managed devices
- `get_managed_storage(device_id) -> Option<ManagedStorageInfo>` — single device
- `list_candidate_devices() -> Vec<CandidateInfo>` — eligible for `storage add`
- `has_managed_storage() -> bool` — quick check

**Query — Role/Pin State:**
- `get_roles() -> HashMap<ReplicaSetId, StorageRole>` — current role per set
- `get_pins() -> HashMap<ReplicaSetId, PinId>` — current pins per set
- `get_primary_storages() -> Vec<ManagedStorageInfo>` — all local Primaries
- `get_dormant_storages() -> Vec<ManagedStorageInfo>` — all local Dormants

**Query — Routing:**
- `resolve_storage_read(replica_set_name) -> Result<StorageRoute>` — read target
- `resolve_storage_write(replica_set_name) -> Result<StorageRoute>` — write target (Primary only)
- `get_content_store(device_id) -> Result<ContentStore>` — for replication

**Query — Beacon Data:**
- `build_beacon_data() -> Vec<StorageAnnouncement>` — for beacon broadcast

**Query — Health/Overview:**
- `storage_overview() -> StorageOverview` — aggregated health view
- `storage_health() -> StorageHealth` — health status

**Mutation — Storage Lifecycle:**
- `add_storage(request) -> Result<AddStorageResponse>` — add device to set
- `remove_storage(device_id) -> Result<()>` — remove device
- `release_storage(device_id) -> Result<()>` — unmount

**Mutation — Naming:**
- `rename_device(device_id, new_name) -> Result<()>` — device sugar rename
- `rename_replica_set(replica_set_id, new_name) -> Result<()>` — set rename (propagated)

**Mutation — Roles:**
- `set_role(replica_set_id, role) -> Result<()>` — assign role
- `pin_storage(replica_set_id) -> Result<()>` — pin as Primary-designate
- `unpin_storage(replica_set_id) -> Result<()>` — remove pin
- `set_visibility(replica_set_id, visibility) -> Result<()>` — change visibility
- `set_storage_roles(replica_set_id, roles) -> Result<()>` — composable roles

**Mutation — Disk:**
- `refresh_disk_usage() -> Result<()>` — update capacity/used from OS

**Trigger — Direct Domain Calls:**

Some operations are imperative triggers, not query/mutation through the boundary. These call domain directly:

- `request_volume_rescan()` — poke the volume watcher to re-scan
- `nudge_orchestration()` — wake the orchestration loop
- `broadcast_beacon()` — trigger immediate beacon

#### 11.4. Boundary Violations Audit (Current Codebase)

The following 28 violations exist in the current codebase and are **all eliminated** in this rebuild:

**Infra Layer — Direct Aggregate Access (8 violations):**

| # | File | Lines | Violation |
|---|------|-------|-----------|
| 1 | `cloud_filter/mod.rs` | 140-191 | `storage_watcher()` polls `Volumes` + `GardenRegistry` every 10s |
| 2 | `cloud_filter/mod.rs` | 221-244 | `collect_storage_names()` directly reads both aggregates |
| 3 | `cloud_filter/provider.rs` | 28-36 | `ZenGardenProvider` struct holds `Volumes` + `GardenRegistry` fields |
| 4 | `cloud_filter/provider.rs` | 47-54 | `storage_service()` constructs domain service inline from raw refs |
| 5 | `cloud_filter/provider.rs` | 74-112 | `list_storages()` reads both aggregates, does manual collection |
| 6 | `storage/beacon.rs` | 24-51 | `build_beacon()` reads `Volumes` directly, iterates manually |
| 7 | `storage/beacon.rs` | 94-114 | `broadcast_if_has_storage()` reads `Volumes` to check managed |
| 8 | `storage/watcher.rs` | 56-104 | `StorageWatcherSet` holds `Volumes`, `reconcile()` reads directly |

**Tasks Layer — Direct Aggregate Access (12 violations):**

| # | File | Lines | Violation |
|---|------|-------|-----------|
| 9 | `storage_orchestration.rs` | 125-142 | `orchestration_tick()` reads `Volumes` + `Registry` directly |
| 10 | `storage_orchestration.rs` | 155-212 | Role resolution reads registry directly via `find_remote_primary_with_pin()` |
| 11 | `storage_orchestration.rs` | 218-243 | Directly mutates `vol.management.role` via write lock |
| 12 | `storage_orchestration.rs` | 281-317 | `compact_primary_changelogs()` reads Volumes for Primary filtering |
| 13 | `storage_replication.rs` | 123-136 | `replication_tick()` reads Volumes, manual Dormant filtering |
| 14 | `storage_replication.rs` | 174-184 | `sync_dormant_bank()` reads Registry for Primary resolution |
| 15 | `storage_replication.rs` | 190-204 | Reads Volumes again to extract ContentStore |
| 16 | `nurturing_scheduler.rs` | 308-314 | `find_available_seed_banks()` reads Volumes directly |
| 17 | `nurturing_scheduler.rs` | 327-340 | `select_targets()` reads Volumes for Primary filtering |
| 18 | `nurturing_scheduler.rs` | 388-403 | Manual store extraction from Volumes |
| 19 | `metrics_collector.rs` | 76-81, 170-181 | Reads Volumes for candidate detection |
| 20 | `metrics_collector.rs` | 158-166 | Directly mutates `vol.used_bytes`/`vol.capacity_bytes` |

**Tasks Layer — Registry Mutations (2 violations):**

| # | File | Lines | Violation |
|---|------|-------|-----------|
| 21 | `coordinator.rs` | 93-96 | Directly calls `registry.write().reap_expired()` |
| 22 | `coordinator.rs` | 223-256 | Calls domain helpers directly instead of through AppState |

**API Layer — Inline Domain Logic (5 violations):**

| # | File | Lines | Violation |
|---|------|-------|-----------|
| 23 | `api/v1/storage.rs` | 242-296 | `storage_overview_v1` reads Volumes + Registry, does inline aggregation |
| 24 | `api/v1/storage.rs` | 321-396 | `storage_health_v1`, `list_banks_v1` read Volumes directly |
| 25 | `api/v1/storage.rs` | 410-442 | `get_bank_v1`, `delete_bank_v1` read Volumes directly |
| 26 | `api/v1/storage.rs` | 208-228 | `validate_seed_bank_layout` is domain logic in API layer |
| 27 | `api/v1/garden_storage/mod.rs` | 272-333 | `list_storages_v1`, `discover_v1` read Registry directly |

**AppState — Aggregate Exposure (1 violation):**

| # | File | Lines | Violation |
|---|------|-------|-----------|
| 28 | `app_state.rs` | 139, 258 | `pub registry`, `pub volumes` exposes raw aggregates |

#### 11.5. Value Objects (garden_common)

Pure data types, no behavior beyond serialization:

- `StorageManifest` — on-disk representation (v5)
- `StorageAnnouncement` — beacon payload
- `StorageTick` — replication doorbell
- `StorageChanged` — event payload for storage state changes
- `StorageRole` — Primary/Dormant enum
- `StorageVisibility` — open/closed/read-only
- `ChangelogEntry`, `ChangelogOp` — replication changelog

#### 11.6. Domain Entities (domain/storage.rs)

- `Volume` — universal representation of a mounted volume
- `Management` — managed storage state (device identity + replica set identity + runtime role)
- `PinState` — pin identity for Primary-designate
- `ReplicaSetSummary` — lightweight view of a set (for listings)
- `ManagedStorageInfo` — device info for AppState consumers

#### 11.7. Domain Services (domain/storage_service.rs)

`StorageService` is the **sole authority** for storage business logic. It owns `Volumes` and reads `GardenRegistry`. All query and mutation methods listed in §11.3 are implemented here.

No other module constructs `StorageService` inline. It is created once by AppState and exposed via AppState methods.

#### 11.8. Infrastructure (no business logic, event-driven)

| Module | Subscribes To | Pulls From | Pushes To |
|--------|---------------|------------|-----------|
| `cloud_filter/` | `storage_changed_tx` | `state.list_replica_sets()` | CfApi placeholders |
| `storage/beacon.rs` | Called by domain on state change | `state.build_beacon_data()` | UDP broadcast |
| `storage/watcher.rs` | `storage_changed_tx` | `state.list_managed_storages()` | `storage_tick_tx` (changelog) |
| `storage/platform.rs` | OS events (udev/WMI) | OS APIs | `volume_event_tx` |

#### 11.9. Tasks (event-driven, AppState boundary)

| Task | Subscribes To | Pulls From | Pushes To |
|------|---------------|------------|-----------|
| `storage_orchestration` | `orchestration_nudge_tx` | `state.get_roles()`, `state.get_pins()`, registry queries | `state.set_role()`, `state.broadcast_beacon()` |
| `storage_replication` | `storage_tick_tx` | `state.get_dormant_storages()`, `state.resolve_storage_write()` | File I/O, `state.storage_tick_tx` |
| `nurturing_scheduler` | Existing scheduling events | `state.get_primary_storages()`, `state.get_content_store()` | File I/O |
| `metrics_collector` | `storage_changed_tx` | `state.list_managed_storages()`, `state.list_candidate_devices()` | Metrics/gauges |

#### 11.10. API Handlers (thin, stateless)

Every handler follows the same pattern:

```rust
async fn handler(State(state): State<Arc<AppState>>, ...) -> Result<...> {
    // 1. Parse/validate request
    // 2. Call state.storage_*() method
    // 3. Map result to HTTP response
    // Nothing else. No aggregate reads. No domain logic.
}
```

### 12. Additional Storage Subsystems (Full Coverage)

The core sections above (§1–§11) cover the identity model, naming, orchestration, and DDD architecture. This section ensures **every** storage-related module is accounted for.

#### 12.1. S3 Gateway (`api/v1/s3_gateway.rs`, ~1020 lines)

S3-compatible object storage frontend. Routes through `StorageService` for resolution.

- **Current**: Constructs `StorageService` inline per request via `state.storage_service()`
- **After**: Calls `state.resolve_storage_read/write(set_name)` directly. No inline service construction.
- **Routing change**: S3 buckets map to replica set names. `GET /api/v1/storage/s3/images/photo.jpg` resolves via `state.resolve_storage_read("images")`.

#### 12.2. WebDAV (`api/v1/webdav.rs`, ~387 lines)

RFC 4918 WebDAV file access via `dav-server` crate. Used by macOS Finder, Linux file managers.

- **Current**: Creates `StorageService` inline, proxies to Primary if remote.
- **After**: Routes through `state.resolve_storage_read/write(set_name)`. The `{name}` in `/dav/{name}/{*path}` becomes the replica set display name.
- **Mutation recording**: `ContentStore.record_external_change()` still called for write ops — but ContentStore obtained via `state.get_content_store(device_id)`.

#### 12.3. Garden Storage Handlers (`api/v1/garden_storage/`)

Four submodules for garden-level storage access:

| Module | Endpoints | After |
|--------|-----------|-------|
| `mod.rs` | Discovery, shared types | Route by `replica_set_name`, return `ReplicaSetDetail` |
| `files.rs` | `GET/PUT/DELETE/HEAD .../files/{*path}` | `state.resolve_storage_read/write(set_name)` |
| `objects.rs` | `GET/PUT/DELETE/HEAD .../objects/{*path}` | Same routing through AppState |
| `memories.rs` | `GET .../memories/...` | Read-only, `state.resolve_storage_read(set_name)` |

All currently construct `StorageService` inline. All switch to AppState boundary methods.

#### 12.4. Shell Integration (`infra/shell_integration.rs`, ~150 lines, Windows-only)

Windows Explorer context menu: "Add Storage to Garden" on removable drives.

- **Current**: Registers shell commands that invoke `garden-rake storage adopt --path`
- **After**: No storage identity changes needed — this is a CLI entry point. The rake command handles set selection (wizard or default).
- **Boundary**: Pure infra. Reads no domain state. No violations.

#### 12.5. Portrait & Presence (`api/v1/portrait.rs`, `api/v1/presence.rs`)

Both include storage state in their snapshots for UI consumption.

- **portrait.rs**: Imports `StorageRole`, includes managed storage info in stone portrait
- **presence.rs**: Imports `StorageSummary`, includes storage summary in presence snapshots (SSE)
- **After**: Both call `state.list_managed_storages()` or `state.storage_overview()` instead of reading aggregates. StorageSummary includes `replica_set_id` + `replica_set_name`.

#### 12.6. Mount Tracker (Linux-only, `app_state.rs`)

Prevents fight-loop: release handler removes device from tracker, persistence task won't re-mount it.

- **Current**: `pub mount_tracker` on AppState, accessed by coordinator + release handler
- **After**: Mount tracker becomes an internal detail of `StorageService`. Release/add operations go through AppState methods which coordinate tracker state internally.
- **Exposed as**: `state.release_storage(device_id)` handles tracker removal. No direct tracker access.

#### 12.7. Volume Lifecycle State Machine (`domain/storage.rs`)

Core volume state transitions:

| Function | Purpose | Boundary |
|----------|---------|----------|
| `initial_scan()` | Enumerate OS volumes, classify, populate Volumes | Called once at bootstrap, through AppState |
| `ingest_event(VolumeEvent)` | Handle mount/unmount events, mutate Volumes | Called by volume watcher via AppState method |
| `reconcile()` | Re-classify all volumes from OS snapshot | Called via AppState on rescan signal |
| `health_tick_all()` | Probe disk usage on all online volumes | Called via AppState after reconcile |
| `Volume::classify()` | Read manifest, determine if managed | Internal to domain |
| `Volume::pin()/unpin()` | Pin state transitions | Called via AppState mutation methods |

**After**: All called through AppState. Bootstrap calls `state.initial_volume_scan()`. Volume watcher calls `state.ingest_volume_event(event)`. No function is called directly by infra or tasks.

#### 12.8. Storage Tick Architecture

Two-tier event channel for replication notifications:

```
Write to storage
    ↓
storage_tick_tx (raw ticks, per-file granularity)
    ↓
Tick aggregator task (quantizes by storage, 1-second window)
    ↓
storage_agg_tx (aggregated ticks, per-storage per-second)
    ↓ subscribers:
    - SSE stream endpoint (/api/v1/stone/storage/stream)
    - Replication task (as doorbell)
```

- **Current**: `storage_tick_tx` and `storage_agg_tx` are `pub` on AppState
- **After**: `storage_tick_tx` is internal to `StorageService` (only the service emits raw ticks). `storage_agg_tx` remains on AppState for SSE stream and replication subscriptions. The tick aggregator task subscribes internally.
- **StorageTick** gains `replica_set_id` field (in addition to existing `storage` name field which becomes `replica_set_name`).

#### 12.9. Release-All Endpoint (`api/v1/storage.rs`)

Bulk unmount operation: `POST /api/v1/stone/storage/release-all`.

- **Current**: Reads Volumes directly, iterates managed, unmounts each, clears management state, removes from mount tracker
- **After**: `state.release_all_storages() -> Result<Vec<ReleaseResult>>`. All logic moves to domain service. Handler is thin.

#### 12.10. Pin/Unpin Endpoints (`api/v1/storage.rs`)

Primary role ownership: `POST /banks/{name}/pin` and `/unpin`.

- **Current**: Reads Volumes, finds by name, calls `vol.pin()/unpin()` directly, then async refresh
- **After**: `state.pin_storage(replica_set_id)` and `state.unpin_storage(replica_set_id)`. Handler is thin. Domain service coordinates pin state, beacon broadcast, and orchestration nudge internally.

#### 12.11. Volume Watcher Loop (`bootstrap/run.rs`)

The volume watcher loop in bootstrap currently contains domain logic:

```rust
// CURRENT (violation): bootstrap loop calls domain functions directly
handle_volume_event(event, &volumes, ...).await;  // calls ingest_event
scan_volumes().await;                              // calls reconcile
health_tick_all(&volumes).await;                   // reads/writes volumes
```

**After**: The loop becomes pure event routing:

```rust
// AFTER: bootstrap loop routes events through AppState
loop {
    tokio::select! {
        event = volume_event_rx.recv() => {
            state.ingest_volume_event(event).await;
            // AppState internally: ingest → reconcile → health_tick → emit storage_changed
        }
        _ = rescan_rx.recv() => {
            state.rescan_volumes().await;
            // AppState internally: scan → reconcile → health_tick → emit storage_changed
        }
    }
}
```

No domain function calls in bootstrap. AppState orchestrates the full pipeline and emits `storage_changed_tx` events for downstream consumers.

### 13. Deduplication (Fewer Moving Parts)

This rebuild eliminates all redundant representations. One concept = one type = one code path.

#### 13.1. Storage Metadata — Three Structs Become Two

**Current (redundant):**
- `StorageInfo` (common) — 10 fields: id, name, device, mount_path, capacity, used, online, encrypted, roles, ...
- `StorageAnnouncement` (common) — 9 fields: id, name, role, capacity, used, encrypted, roles, ... (overlaps 6 fields)
- `StorageMetadata` (common/tools) — 7 fields: capacity, used, visibility, encrypted, pin_id, protocols, roles (overlaps 5 fields)

**After:**
- `StorageAnnouncement` — the canonical wire format for beacons and registry. Contains device identity, replica set identity, role, capacity, visibility. **Single struct, no mirrors.**
- `StorageMetadata` in `GardenTool` — **removed.** The GardenTool for storage entries directly embeds a `StorageAnnouncement` reference or extracts what it needs during projection. No separate metadata struct.
- `StorageInfo` — **removed.** Replaced by `ManagedStorageInfo` (domain entity) for internal use, and `StorageAnnouncement` for wire/API use. No field duplication.

#### 13.2. API Response Types — Defined Once in Common

**Current (duplicated):**
- `GardenBankInfo` defined in **both** `moss/api/v1/storage.rs:101` and `rake/commands/storage.rs:38` — identical structs.
- `StorageInstance` in `garden_storage/mod.rs:87` — yet another storage representation for discovery.
- `SeedBankHealth` in `storage.rs:155` — health-specific view.

**After:**
- `ReplicaSetSummary` (common) — the single API response type for set listings. Contains: `replica_set_id`, `replica_set_name`, display FQN, member count, total capacity, primary location.
- `StorageDeviceInfo` (common) — the single API response type for device-level detail. Contains: device id, name, mount path, capacity, used, role, set membership.
- Both defined in `garden_common`. Rake deserializes from common types. No duplication.
- `SeedBankHealth` folded into `StorageDeviceInfo` (health is an attribute, not a separate type).
- `StorageInstance` removed — `StorageDeviceInfo` covers discovery responses.

#### 13.3. Volume Finders — One Path, Not Two

**Current (redundant):**
- `domain/storage.rs` has standalone helpers: `find_by_name()`, `find_by_id()`, `list_managed()`, `list_candidates()`
- `domain/storage_service.rs` has methods: `find_local()`, `find_local_by_id()`, `list_local()`
- Both do the same thing: acquire read lock on `Volumes`, filter, return.

**After:**
- `StorageService` is the **sole** accessor. The standalone helpers in `domain/storage.rs` are deleted.
- All callers go through AppState → StorageService. One code path for each query.
- `name_id_pairs()` removed — consumers call `list_managed_storages()` and project what they need.

#### 13.4. Path Validation — One Function

**Current (3 implementations):**
- `garden_storage/files.rs` — `validate_file_path()`: blocks `.zen-garden` with string prefix check
- `webdav.rs` — `is_dotfolder_access()`: 3 redundant checks (`starts_with`, `contains("/.zen-garden")`, `contains("\\.zen-garden")`)
- `storage/watcher.rs` — inline check: `s == dotfolder || s == "Zen Garden"`

**After:**
- Single `is_internal_path(rel_path: &str) -> bool` in `garden_common::constants::paths`
- Checks: `.zen-garden` prefix (any separator), `Zen Garden` symlink
- Used by: files.rs, webdav.rs, watcher.rs, cloud filter provider
- No inline reimplementations.

#### 13.5. Layout Validation — One Function

**Current (scattered):**
- `api/v1/storage.rs:208` — `validate_seed_bank_layout()` checks 2 required subdirs
- `garden_storage/memories.rs:52` — `validate_storage_layout()` checks memories subdir only
- Both are domain logic misplaced in the API layer

**After:**
- `StorageService::validate_layout(device_id) -> Result<()>` — single domain-level function
- Validates all required subdirs (memories, objects, changelog)
- Called by API handlers via AppState. No inline validation.

#### 13.6. Snapshot Functions — Absorbed into StorageService

**Current (standalone helpers):**
- `roles_snapshot(volumes) -> HashMap<String, StorageRole>` — standalone async fn
- `pins_snapshot(volumes) -> HashMap<String, String>` — standalone async fn
- `name_id_pairs(volumes) -> Vec<(String, String)>` — standalone async fn

**After:**
- `StorageService::get_roles()`, `StorageService::get_pins()` — methods on the service
- `name_id_pairs` removed entirely (callers use `list_managed_storages()`)
- No standalone functions that take raw `&Volumes`

### 14. Removed Code Paths

The following are deleted with no replacement or shim:

| Removed | Reason |
|---------|--------|
| `DEFAULT_PUBLIC_STORAGE_NAME` ("zen-garden") | Replaced by default replica set (`""`) |
| `DEFAULT_PRIVATE_STORAGE_NAME` ("private") | Replaced by named replica set |
| `StorageManifest::logical_name()` | Name is just `name`; no disambiguation needed |
| `StorageInfo` struct | Replaced by `ManagedStorageInfo` (domain) + `StorageAnnouncement` (wire) |
| `StorageMetadata` struct | Folded into `StorageAnnouncement` |
| `GardenBankInfo` (both copies) | Replaced by `ReplicaSetSummary` / `StorageDeviceInfo` in common |
| `StorageInstance` struct | Replaced by `StorageDeviceInfo` |
| `SeedBankHealth` struct | Health folded into `StorageDeviceInfo` |
| `find_by_name(volumes, name)` | Absorbed into `StorageService` |
| `find_by_id(volumes, id)` | Absorbed into `StorageService` |
| `list_managed(volumes)` | Absorbed into `StorageService` |
| `list_candidates(volumes)` | Absorbed into `StorageService` |
| `roles_snapshot(volumes)` | Absorbed into `StorageService` |
| `pins_snapshot(volumes)` | Absorbed into `StorageService` |
| `name_id_pairs(volumes)` | Removed; callers use `list_managed_storages()` |
| `storage_by_name(name)` in registry | Replaced by `storage_by_set_id()` |
| `storage_primary(name)` in registry | Replaced by `storage_primary_by_set_id()` |
| `route_to_primary(name)` in registry | Replaced by `route_to_primary_by_set_id()` |
| `canonical_storage_name()` in projector | Registry key uses `replica_set_id` |
| `validate_seed_bank_layout()` in API | Moved to `StorageService::validate_layout()` |
| `validate_file_path()` in files.rs | Replaced by shared `is_internal_path()` |
| `is_dotfolder_access()` in webdav.rs | Replaced by shared `is_internal_path()` |
| v4 manifest support | Greenfield; no v4 reading, no migration shim |
| Name-equality replica detection | Replicas share `replica_set_id`, not `name` |

## Consequences

### Positive

- **Rename is safe.** Renaming a replica set propagates to all members without breaking replication. The `replica_set_id` binding is immutable.
- **Cloud Filter shows the right abstraction.** Users see logical storage spaces (`storage`, `images`, `personal`), not device topology.
- **Offline catch-up works.** Timestamp comparison on reconnect ensures offline members adopt name changes.
- **Pinned Primary is safe.** Sync-then-assert prevents serving stale data.
- **Default set is zero-friction.** New users get a single shared storage pool without any configuration.
- **Single stone, multiple sets.** A stone can host devices in different replica sets.
- **Clean codebase.** No shims, no legacy paths, no name-as-group-key confusion.

### Negative

- **Breaking change.** Existing v4 manifests are invalid. All storages must be re-added.
- **API surface grows.** New `/sets` endpoint, split rename endpoints, wizard support.
- **Beacon payload grows.** Three new fields per storage announcement.
- **CLI changes required.** Rake storage commands need replica set awareness and wizard flow.

### Risks

- **Data on existing storages.** Users with data on v4 storages must re-add them. Since `storage add` on a directory with existing `.zen-garden/` already handles re-initialization, the workflow is: delete old `.zen-garden/manifest.json`, re-run `storage add`.
- **Multi-stone upgrade.** All stones in a garden must upgrade simultaneously. Mixed v4/v5 gardens are not supported (no shims).
