---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-15
---

# ARCH-0005: Structural Quality Pass

**Date**: 2026-03-14
**Status**: Accepted (implementation in progress)
**Depends on**: ARCH-0003 (Code Standards Compliance), ARCH-0004 (AppState Domain Context Extraction)

## Context

ARCH-0003 addressed naming, value objects, and channel conventions. ARCH-0004 addressed
`AppState` domain context extraction and `FromRef` handler narrowing. Both are structural
migrations — they fix how things are named and organized.

A full codebase audit validates that the 15 rules in `docs/code-standards.md` are now
structurally satisfied: naming, nesting, channel conventions, state machines, visibility,
and canonical types are all compliant. **The structural migration worked.**

This ADR addresses the debt that remains: **behavioral duplication, boundary violations,
magic constants, DTO proliferation, and scope misplacement**. These cannot be fixed by
renaming or restructuring fields. They require moving logic between layers, extracting
abstractions, consolidating codepaths, and centralizing constants.

The audit identified twelve categories of debt. Each is described below with its symptoms,
every known instance, and the proposed fix.

---

## Issue 1: Domain Layer Imports Infra Directly

### Symptoms

15 domain files import concrete infra types instead of depending on trait abstractions:

| Domain file                                | Infra import                                         |
| ------------------------------------------ | ---------------------------------------------------- |
| `domain/adoption.rs`                       | `crate::infra::ManifestRegistry`                     |
| `domain/ceremony/phases/collect.rs`        | `crate::infra::create_harvest`                       |
| `domain/ceremony/phases/water.rs`          | `crate::infra::restore_harvest`                      |
| `domain/connectivity.rs`                   | `crate::infra::detection::*`                         |
| `domain/infrastructure/mod.rs`             | `crate::infra::ManifestRegistry`                     |
| `domain/infrastructure/docker_registry.rs` | `crate::infra::docker_config`                        |
| `domain/modes/detection.rs`                | `crate::infra::detection::*`                         |
| `domain/offering_resolution.rs`            | `crate::infra::image_inspect::*`                     |
| `domain/offerings.rs`                      | `crate::infra::ManifestRegistry`                     |
| `domain/security/ceremony.rs`              | `crate::infra::CeremonyJournal`                      |
| `domain/security/mod.rs`                   | `crate::infra::stone_client::StoneClient`            |
| `domain/service_manager.rs`                | `crate::infra::ContainerRuntime`                     |
| `domain/storage/bank.rs`                   | `crate::infra::storage::platform::VolumeSnapshot`    |
| `domain/storage/mod.rs`                    | `crate::infra::storage::{platform, ContentStore}`    |
| `domain/storage_service.rs`                | `crate::infra::storage::{ContentStore, ObjectStore}` |

### Root cause

The domain layer was built with concrete infra types because traits for those capabilities
did not exist. Only one domain trait exists today: `InfrastructureHandler`.

### Consequence

- Domain logic cannot be unit-tested without standing up real Docker, filesystem, and
  network infrastructure
- Infra implementation changes break domain compilation
- The stated architecture rule ("domain NEVER imports infra") is violated in 15 places

### Fix

Extract domain-side traits for each infra capability. The domain defines what it needs;
infra implements it.

| Domain trait (new)    | Methods                                                    | Infra implementor          |
| --------------------- | ---------------------------------------------------------- | -------------------------- |
| `ManifestLookup`      | `get(&OfferingFqn)`, `list_category(&str)`, `search(&str)` | `ManifestRegistry`         |
| `ContainerOps`        | `start()`, `stop()`, `remove()`, `inspect()`, `logs()`     | `ContainerRuntime`         |
| `HarvestOps`          | `create_harvest()`, `restore_harvest()`                    | `infra::harvest` functions |
| `ImageInspector`      | `inspect_image(&str)`                                      | `infra::image_inspect`     |
| `StoragePlatform`     | `snapshot()`, `disk_usage()`, `mount()`, `unmount()`       | `infra::storage::platform` |
| `ContentStoreOps`     | `read()`, `write()`, `list()`, `delete()`                  | `ContentStore`             |
| `CeremonyPersistence` | `load()`, `save()`, `append()`                             | `CeremonyJournal`          |
| `InterStoneClient`    | `get()`, `post()`, `forward()`                             | `StoneClient`              |
| `DockerConfigOps`     | `read_config()`, `write_insecure_registries()`             | `docker_config` functions  |
| `DetectionOps`        | `detect_container()`, `inspect_container()`                | `detection::*` functions   |

Traits live in `domain/traits/` (or inline in the domain module that uses them).
`InfrastructureHandler` is already correct and stays as-is.

### Migration approach

One trait at a time, in order of isolation (fewest callers first):

1. `CeremonyPersistence` (1 caller)
2. `DockerConfigOps` (1 caller)
3. `HarvestOps` (2 callers)
4. `ImageInspector` (1 caller)
5. `DetectionOps` (2 callers)
6. `InterStoneClient` (2 callers)
7. `ManifestLookup` (3 callers)
8. `ContainerOps` (1 caller, but large surface)
9. `StoragePlatform` (2 callers, large surface)
10. `ContentStoreOps` (2 callers, large surface)

Each trait extraction is one commit: define trait, implement for existing type, replace
`use crate::infra::X` with trait bound or injected `Arc<dyn Trait>`.

---

## Issue 2: God Module — `domain/storage/mod.rs` (~40K lines)

### Symptoms

A single file handles seven distinct concerns:

1. Volume lifecycle (online/degraded/offline state machine)
2. Replica set management (identity, names, TTL tracking)
3. Health probing (disk usage via `platform::disk_usage()`)
4. Pin reconciliation (read pin.json from disk)
5. Manifest parsing (`.zen-garden/manifest.json` classification)
6. ContentStore integration (direct `ContentStore::new()` instantiation)
7. Management namespace (identity, roles, visibility, encryption)

### Fix

Split into concept-aligned submodules within `domain/storage/`:

```
domain/storage/
  mod.rs          — Storage struct, re-exports (< 100 lines)
  volume.rs       — Volume lifecycle state machine
  replica.rs      — Replica set identity, names, TTL
  health.rs       — Health probing, disk usage evaluation
  pin.rs          — Pin reconciliation logic
  manifest.rs     — Storage manifest parsing/classification
  management.rs   — Identity, roles, visibility, encryption settings
  media.rs        — Physical storage device abstraction
```

Two-commit discipline per file: rename commit (pure `git mv`), then content commit.

---

## Issue 3: Handler Thickness — Business Logic in API Layer

### Symptoms

`api/v1/services.rs` (~1,900 lines) contains:

- Compatibility fallback selection (lines 449–465)
- Image-direct deployment detection and handling (lines 265–372)
- Self-heal adoption logic (lines 374–414)
- Job creation, registry updates, and async task spawning (lines 490–564)
- 19 separate `state.offerings.read().await` lock acquisitions
- 12 separate `persist_offerings()` calls
- 40+ `error_response()` inline calls

Similar patterns in `api/v1/offerings.rs` (offerings filtering, compatibility resolution)
and `api/v1/nourishment.rs` (update check logic, parallel stone querying).

### Fix

Extract domain services that encapsulate multi-step operations. Handlers become thin
dispatchers.

**`domain/service_lifecycle.rs`** — consolidates the four install entry points:

```rust
pub struct ServiceLifecycle { /* injected deps */ }

impl ServiceLifecycle {
    /// Single entry point for all install/deploy operations.
    /// Handles: direct install, image-direct, adoption, health repair.
    pub async fn install(&self, request: InstallRequest) -> Result<Job> { ... }
    pub async fn remove(&self, fqn: &OfferingFqn) -> Result<()> { ... }
    pub async fn restart(&self, fqn: &OfferingFqn) -> Result<()> { ... }
    pub async fn update(&self, fqn: &OfferingFqn, spec: UpdateSpec) -> Result<Job> { ... }
}
```

**`domain/offering_lifecycle.rs`** — consolidates the five offering mutation paths:

```rust
pub struct OfferingLifecycle { /* injected deps */ }

impl OfferingLifecycle {
    /// Unified mutation gateway. All offering state changes go through here.
    /// Handles: upsert + persist + sync + chirp in one atomic operation.
    pub async fn upsert(&self, offering: Offering, auto_chirp: bool) -> Result<()> { ... }
    pub async fn remove(&self, fqn: &OfferingFqn) -> Result<()> { ... }
    pub async fn update<F>(&self, fqn: &OfferingFqn, f: F) -> Result<bool>
    where F: FnOnce(&mut Offering) -> bool { ... }
    pub async fn batch_update<F>(&self, f: F) -> Result<usize>
    where F: FnMut(&mut Offering) -> bool { ... }
}
```

---

## Issue 4: Duplicate Codepaths for Same Operation

### Symptoms — Exhaustive Inventory

**4a. Offering lock-acquire-find pattern (60+ instances)**

The pattern `state.offerings.read().await` followed by find/filter/map appears in:

- `api/v1/services.rs`: lines 220, 274, 382, 468, 592, 662, 714, 790, 941, 1039, 1234,
  1304, 1535, 1563, 1616, 1719, 1820, 1913, 1926 (19 instances)
- `api/v1/adoption.rs`: lines 45, 112, 322, 416, 506 (5 instances)
- `api/v1/config.rs`: lines 91, 168, 268, 445 (4 instances)
- `api/v1/nurturing.rs`: lines 93, 148, 224, 316, 381, 505 (6 instances)
- `api/v1/offerings.rs`: lines 65, 170, 880 (3 instances)
- `api/v1/nourishment.rs`: lines 593, 692, 859 (3 instances)
- `api/v1/greenhouse.rs`: lines 295, 774 (2 instances)
- `api/v1/portrait.rs`: lines 508, 706 (2 instances)
- `api/v1/presence.rs`: lines 132–135
- `api/v1/offering_capabilities.rs`: line 1231
- `app_state.rs`: lines 233, 321, 558, 586
- `tasks/health_monitor.rs`: lines 144, 187
- `tasks/offering_orchestration.rs`: line 520
- `tasks/job_executors.rs`: line 828

Sub-patterns: find-by-FQN, find-by-string-name, find-by-offering-id, iterate-and-map.

**4b. Persist offerings + log error (43+ instances)**

The pattern `persist_offerings().await` with error logging appears in:

- `api/v1/services.rs`: lines 339, 398, 549, 628, 755, 898, 970, 1072, 1465, 1596, 1974
  (12 instances)
- `api/v1/adoption.rs`: lines 257, 332, 474, 516 (4 instances)
- `api/v1/config.rs`: lines 230, 329 (2 instances)
- `api/v1/offering_capabilities.rs`: line 117
- `app_state.rs`: lines 425, 441, 457, 509, 528 (5 instances)
- `domain/tools/capability_orchestrator.rs`: line 110
- `tasks/auto_adoption.rs`: line 393
- `tasks/coordinator.rs`: line 456
- `tasks/health_monitor.rs`: line 325
- `tasks/job_executors.rs`: lines 399, 870 (2 instances)
- `tasks/offering_orchestration.rs`: lines 548, 698 (2 instances)

Inconsistent error severity: some use `error!`, others `warn!`, some `let _ =` (silent).

**4c. Error response construction (269 instances in `api/v1/`)**

The `error_response(StatusCode::XXX, "ERROR_CODE", message, None)` pattern appears 269
times across all API handlers. Common duplicated shapes:

- Invalid FQN: `api/v1/adoption.rs:92`, `api/v1/offerings.rs:158`
- Not found: `api/v1/adoption.rs:130`, `api/v1/services.rs` (20+ instances)
- Docker unavailable: `api/v1/offerings.rs:189`
- Bad request: `api/v1/config.rs:96`, `api/v1/console.rs:34`

**4d. SSE stream setup boilerplate (6 instances, 80% identical)**

Each SSE endpoint duplicates the same 40–50 line pattern: create snapshot, subscribe to
broadcast channel, wrap in `async_stream::stream!`, add cancellation token, return
`Sse::new(stream).keep_alive(KeepAlive::default())`.

Instances:

- `api/v1/logs.rs:97`
- `api/v1/presence.rs:71`
- `api/v1/pulse.rs:58`
- `api/v1/services.rs:1219`
- `api/v1/storage.rs:1747`
- `api/v1/tools.rs:81`

**4e. Channel lag handling (18 instances, 95% identical)**

The `Err(BroadcastStreamRecvError::Lagged(n)) => warn + continue` pattern:

- `api/v1/presence.rs:95`, `api/v1/pulse.rs:84`, `api/v1/storage.rs:1768`,
  `api/v1/tools.rs:131` (SSE handlers)
- `infra/cloud_filter/ingest.rs:106`, `infra/cloud_filter/mod.rs:399,424`
  (triggers rescan)
- `infra/event_bus.rs:112` (listener framework)
- `tasks/coordinator.rs:840,1147,1188,1270,1680` (5 instances, mixed warn/debug)
- `tasks/metrics_collector.rs:101`, `tasks/storage_orchestration.rs:427,453`,
  `tasks/storage_replication.rs:97`, `tasks/storage_tick_aggregator.rs:175`

Three behavioral variants: warn-and-skip (SSE), trigger-full-reconcile (cloud filter),
silently-continue (drain loops). A shared handler should encode the variant as a parameter.

**4f. Service discovery internal duplication**

`find_local_services()` (lines 282–353) and `list_all_local_services()` (lines 359–408)
share ~70% identical logic.

**4g. Job creation (3+ instances)**

Same `Job { id: Uuid::now_v7(), offerings, status: Pending, ... }` construction in
`api/v1/services.rs` lines 295, 491 and `api/v1/adoption.rs`.

**4h. HTTP request proxy pattern (garden storage, 4 instances)**

`api/v1/garden_storage/mod.rs:201–249` defines a request builder + response header
extraction pattern duplicated in `memories.rs`, `objects.rs`, and `s3_gateway.rs`.

**4i. `tokio::spawn` with silent failure (6+ instances)**

Violates the critical rule "always log errors in spawned tasks":

- `api/v1/admin.rs:275,329`
- `api/v1/offering_capabilities.rs:395,848`
- `api/v1/services.rs:347`
- `api/v1/garden_storage/memories.rs:106`

### Fix

**4a**: Resolved transitively by Issue 3 (`OfferingLifecycle` becomes the single query
and mutation gateway).

**4b**: Persistence becomes implicit inside `OfferingLifecycle.upsert/remove/update`.
Callers never call `persist_offerings()` directly. Logging severity is consistent within
the lifecycle service.

**4c**: Extract typed error constructors on `ErrorResponse`:

```rust
impl ErrorResponse {
    pub fn not_found(entity: &str, id: &str) -> (StatusCode, Json<ErrorResponse>) { ... }
    pub fn bad_request(code: &str, msg: String) -> (StatusCode, Json<ErrorResponse>) { ... }
    pub fn docker_unavailable() -> (StatusCode, Json<ErrorResponse>) { ... }
    pub fn conflict(code: &str, msg: String) -> (StatusCode, Json<ErrorResponse>) { ... }
    pub fn invalid_fqn(name: &str, err: &str) -> (StatusCode, Json<ErrorResponse>) { ... }
}
```

**4d**: Extract SSE stream factory:

```rust
fn sse_from_broadcast<T, F>(
    rx: broadcast::Receiver<T>,
    token: CancellationToken,
    map_fn: F,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>>
where
    T: Clone + Send + 'static,
    F: Fn(T) -> Option<Event> + Send + 'static,
```

**4e**: Extract lag handler with behavioral variant:

```rust
enum LagBehavior { WarnAndSkip, Reconcile, SilentContinue }
```

**4f**: Extract `resolve_offering_to_service()` shared core.

**4g**: Extract `Job::new(offerings: Vec<String>) -> Job` constructor.

**4h**: Extract `proxy_to_stone()` helper in garden storage module.

**4i**: Audit and add error logging to all silent `tokio::spawn` calls.

---

## Issue 5: Magic Constants Scattered Across Codebase

### Symptoms — Exhaustive Inventory

**5a. Channel capacities (17 distinct locations, no constants)**

| Capacity | Files                                                                                                                                                                 |
| -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 16       | `domain/storage_service.rs:494`, `infra/listeners/pulse.rs:666`, `infra/storage/store.rs:1480`, `tasks/storage_tick_aggregator.rs:404`                                |
| 32       | `mdns.rs:338`, `tasks/coordinator.rs:1391`                                                                                                                            |
| 64       | `bootstrap/run.rs:655,657,659`, `orchestrators/ollama/src/api/proxy.rs:398`                                                                                           |
| 100      | `common/src/infra/communications/p2p.rs:505,544,724`, `api/v1/nourishment.rs:477`, `tasks/docker/mod.rs:94`, `tasks/network_monitor.rs:101`                           |
| 256      | `common/src/infra/cloud_filter/ingest.rs:62`, `lantern/src/app_state.rs:52`, `orchestrators/mongodb/src/app_state.rs:69`, `orchestrators/ollama/src/app_state.rs:127` |
| 512      | `bootstrap/run.rs:147,596`                                                                                                                                            |
| 1024     | `main.rs:67`                                                                                                                                                          |

**5b. Hardcoded durations (40+ instances)**

| Duration           | Notable locations                                                                                   | Context                                  |
| ------------------ | --------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| `from_secs(2)`     | `api/v1/nourishment.rs:739`, `bootstrap/run.rs:1249`                                                | Startup waits                            |
| `from_secs(5)`     | `api/v1/companions.rs:199,302`, `bootstrap/run.rs:1169`, `companion-sdk/src/sse.rs:47`              | Companion timeouts, SSE reconnect        |
| `from_secs(8)`     | `bootstrap/tls.rs:199`                                                                              | TLS retry sleep                          |
| `from_secs(10)`    | `api/v1/nourishment.rs:134,214`                                                                     | Nourishment timeout (2 locations)        |
| `from_secs(15)`    | `api/v1/pond.rs:682`, `api/v1/tools.rs:140`                                                         | Pond join, tools SSE                     |
| `from_secs(20)`    | `api/v1/offering_capabilities.rs:935`                                                               | Capability check                         |
| `from_secs(25)`    | `bootstrap/server.rs:168`                                                                           | Watchdog ping (should match WatchdogSec) |
| `from_millis(200)` | `api/v1/companions.rs:175`                                                                          | Companion startup wait                   |
| `from_millis(500)` | `api/v1/admin.rs:276,330`, `api/v1/stone.rs:701`, `bootstrap/run.rs:958`, `bootstrap/server.rs:289` | Various waits                            |

Existing `constants/timeouts.rs` defines polling intervals via env-overridable functions,
but the values above bypass them entirely.

**5c. Reqwest timeout inconsistency (6 different timeouts, no constants)**

| Timeout | Location                              | Operation                                        |
| ------- | ------------------------------------- | ------------------------------------------------ |
| 5s      | `api/v1/companions.rs:199,302`        | Companion command forwarding                     |
| 10s     | `api/v1/nourishment.rs:134,214`       | Nourishment execution                            |
| 15s     | `api/v1/pond.rs:682`                  | Cornerstone lookup                               |
| 20s     | `api/v1/offering_capabilities.rs:935` | Capability check                                 |
| 5s      | `api/v1/pond.rs:812`                  | Pond operation (inconsistent within same module) |
| 30s     | `domain/metrics_collection.rs:58`     | Metrics fetch (default)                          |

**5d. Server configuration hardcodes**

| Value | Location                     | Purpose                     |
| ----- | ---------------------------- | --------------------------- |
| 8     | `bootstrap/server.rs:35`     | Drain deadline seconds      |
| 15    | `bootstrap/server.rs:36`     | Hard shutdown deadline      |
| 25    | `bootstrap/server.rs:168`    | Watchdog ping interval      |
| 128   | `bootstrap/server.rs:70`     | TCP listen backlog          |
| 30/2  | `bootstrap/startup.rs:30-31` | Docker retry attempts/delay |

**5e. Disconnect retry bypassing constants**

`bootstrap/run.rs:301,587` uses `.with_disconnect_retry(5)` instead of
`DEFAULT_DISCONNECT_RETRY_SECS` constant defined in `tasks/docker/mod.rs:20` and
`tasks/network_monitor.rs:22`.

**5f. Hardcoded path**

`bootstrap/preinstall.rs:36`: hardcoded `"/home/stone/garden-moss-preinstall.json"`
instead of using `constants::paths::stone_home()`.

### Fix

**Create `constants/channels.rs`**:

```rust
pub const STORAGE_TICK: usize = 16;
pub const DISCOVERY_EVENT: usize = 32;
pub const STORAGE_CHANGED: usize = 64;
pub const P2P_ANNOUNCEMENT: usize = 100;
pub const SSE_DASHBOARD: usize = 256;
pub const TOOL_DELTA: usize = 512;
pub const LOG_STREAM: usize = 1024;
```

**Extend `constants/timeouts.rs`** with named functions for each operational timeout:

```rust
pub fn companion_command_timeout() -> Duration { Duration::from_secs(5) }
pub fn nourishment_timeout() -> Duration { Duration::from_secs(10) }
pub fn capability_check_timeout() -> Duration { Duration::from_secs(20) }
pub fn pond_join_timeout() -> Duration { Duration::from_secs(15) }
pub fn watchdog_ping_interval() -> Duration { Duration::from_secs(25) }
```

**Add `constants/server.rs`**:

```rust
pub const DRAIN_DEADLINE_SECS: u64 = 8;
pub const HARD_DEADLINE_SECS: u64 = 15;
pub const TCP_BACKLOG: u32 = 128;
pub const DOCKER_RETRY_ATTEMPTS: u32 = 30;
pub const DOCKER_RETRY_DELAY_SECS: u64 = 2;
```

**Fix `preinstall.rs:36`**: Use `constants::paths::stone_home().join(...)`.

**Fix `run.rs:301,587`**: Use `DEFAULT_DISCONNECT_RETRY_SECS` constant.

---

## Issue 6: DTO / Transport Type Duplication

### Symptoms — Exhaustive Inventory

**6a. `ServiceDiscoveryResponse` — defined in two places**

- `moss/src/domain/service_discovery.rs:260–277`: `timestamp: DateTime<Utc>`
- `rake/src/commands/discovery/find.rs:137–144`: `timestamp: String`

Same struct, different timestamp types.

**6b. `FoundService` — defined in two places**

- `moss/src/domain/service_discovery.rs:218–249`: includes `offering_id`,
  `sub_capabilities`, uses `ResolvedConnection`
- `rake/src/commands/discovery/find.rs:125–133`: lacks `offering_id`,
  `sub_capabilities`, uses `ConnectionInfo`

**6c. `StoneRef` — identical definition in two places**

- `moss/src/domain/service_discovery.rs:252–257`: `{ id, name, endpoint }`
- `rake/src/commands/discovery/find.rs:105–111`: `{ id, name, endpoint }`

Byte-for-byte identical.

**6d. `ConnectionInfo` vs `ResolvedConnection`**

- `rake/src/commands/discovery/find.rs:114–121`: `ConnectionInfo { hostname, ip, port, protocol, uris }`
- `moss/src/domain/service_discovery.rs`: `ResolvedConnection` (different fields)

Two types for the same wire concept.

**6e. `OfferingSlots` — defined in two places**

- `moss/src/domain/nurturing.rs:98+`: domain type
- `rake/src/commands/nurturing.rs:33–40`: CLI deserialization type with extra
  `offering_name` field

**6f. `GardenBankInfo` — API type duplicated in Rake**

- `moss/src/api/v1/storage.rs:102–145`: 16-field response type
- `rake/src/commands/storage.rs`: local deserialization type with same fields

**6g. `OfferingView` vs `OfferingSearchResult`**

- `moss/src/api/v1/offerings.rs:30–53`: `OfferingView` with `name, state, category, description, tags, image, compatibility, health, uptime`
- `common/src/offerings.rs`: `OfferingSearchResult` with overlapping fields
  (`name, category, description, tags, image, compatibility`)

API layer creates its own view instead of enriching the canonical search result.

**6h. `OfferingEntry` / `OfferingCompatibility` — Rake local types**

- `rake/src/commands/offering/mod.rs`: local deserialization types for offerings list
  response, duplicating fields from `OfferingSearchResult`

**6i. `StorageSummary` — name collision**

- `common/src/storage.rs:395–488`: 12-field summary for CLI display
- `common/src/presence/types.rs:85–91`: 3-field summary for presence protocol

Different purposes, confusing identical name.

**6j. `ServiceInfo` — defined twice with different shapes**

- `common/src/types.rs:28`: Full service contract with `offering_id`, `name`, `offering`,
  `version`, `status`, `health`, `ports`, `resources`, `job_id`, `sub_capabilities`,
  `guidance`, `customized_by`
- `common/src/tools/types.rs:71`: Simplified contract with `status`, `ready`, `protocol`,
  `uris`, `hostname`, `ip`, `port`, `uri_template`

Two types named for the same concept with incompatible field sets. Conversion between them
requires manual field-by-field mapping in `moss/src/api/v1/services.rs:24–57`
(`offering_to_service_info()`).

**6k. `Stone` — three representations**

- `common/src/stone.rs:66`: Canonical `Stone { id, name, host }`
- `common/src/tools/types.rs:63`: Simplified `Stone { id, name, endpoint }` for tools
  registry
- `moss/src/domain/current/mod.rs:14`: Another definition for the node's self-model

`StoneRef` (6c) is a fourth representation. `StoneInfo` and `StoneInfoResponse` exist in
Moss API responses. No `From` impls connect them — conversion is manual everywhere.

**6l. `offering_to_service_info()` — manual field mapping boilerplate**

`moss/src/api/v1/services.rs:24–57` is a 33-line function that manually maps `Offering`
fields to `ServiceInfo` fields one by one. This would be eliminated by a `From<&Offering>`
impl on `ServiceInfo`, or by embedding the canonical type.

### Root cause

Rake defines its own response deserialization types rather than importing the canonical
types from `garden-common`. Moss API handlers create view types that duplicate canonical
type fields rather than embedding/flattening them. Within `garden-common` itself, the
`tools/` module defines simplified versions of core types (`Stone`, `ServiceInfo`) instead
of reusing or embedding the canonical definitions.

### Fix

**Move to `garden-common`**: `ServiceDiscoveryResponse`, `FoundService`, `StoneRef`,
`ResolvedConnection` (replacing both `ConnectionInfo` and the current
`ResolvedConnection`), `OfferingSlots`, `GardenBankInfo`.

**Enrich, don't duplicate**: `OfferingView` should embed `OfferingSearchResult` via
`#[serde(flatten)]` and add only computed fields (`health`, `uptime`, `state`).

**Rename collision**: `presence::StorageSummary` → `presence::StoragePresence` to
distinguish from the CLI display type.

**Delete Rake local types**: After moving to common, remove all local deserialization
duplicates in `rake/src/commands/`.

**Unify `ServiceInfo`**: The tools variant (`tools/types.rs:71`) should embed or reference
the canonical `ServiceInfo` from `types.rs`, adding only `ready`, `uri_template` and
connection fields as enrichment. Delete the duplicate definition.

**Unify `Stone` representations**: `tools/types.rs:63` `Stone` should use the canonical
`common/src/stone.rs` `Stone`. Where the tools registry needs `endpoint` instead of
`host`, use `#[serde(flatten)]` on the canonical type and add the extra field.

**Add `From` impls**: Replace `offering_to_service_info()` with
`impl From<&Offering> for ServiceInfo`. Do the same for any other manual field-mapping
functions identified during implementation.

**Delete `StoneRef`**: After unifying `Stone`, `StoneRef { id, name, endpoint }` becomes
redundant — use `Stone` with the appropriate fields, or a shared `StoneIdentity` if
a lightweight variant is genuinely needed.

---

## Issue 7: `garden-common` Scope Creep

### Symptoms

~10K lines in `garden-common` are used by only one crate:

| Module                                | Lines  | Used by                    | Should live in     |
| ------------------------------------- | ------ | -------------------------- | ------------------ |
| `console/`                            | 1,963  | Rake + Moss console output | `rake/src/ui/`     |
| `ui/`                                 | 2,222  | Rake TUI rendering         | `rake/src/ui/`     |
| `cli_colors.rs`                       | 410    | Rake CLI formatting        | `rake/src/ui/`     |
| `manifests/` (validation, generation) | ~3,000 | Moss only                  | `moss/src/domain/` |
| `infra/registry_client.rs`            | 414    | Moss only                  | `moss/src/infra/`  |
| `infra/koi_client.rs`                 | ~300   | Moss only                  | `moss/src/infra/`  |
| `api_manifest.rs`                     | 312    | Moss only                  | `moss/src/api/`    |

Additionally, `types.rs` (2,030 lines) is a monolith mixing service, hardware, discovery,
pond, and Lantern types in one file.

### Fix

**Phase A — Move Rake-specific code** (low risk):

1. Move `console/` → `rake/src/ui/console/`
2. Move `ui/` → `rake/src/ui/rendering/`
3. Move `cli_colors.rs` → `rake/src/ui/colors.rs`
4. Audit `cli/` — if Rake-only, move it too

Moss currently imports `console/` for `ConsolePrinter`. If Moss only uses a subset,
extract a minimal trait in common and implement it in both Moss and Rake.

**Phase B — Move Moss-specific code** (medium risk):

1. Move `infra/registry_client.rs` → `moss/src/infra/registry_client.rs`
2. Move `infra/koi_client.rs` → `moss/src/infra/koi_client.rs`
3. Move `api_manifest.rs` → `moss/src/api/manifest.rs`
4. Move manifest validation/generation from `manifests/` → `moss/src/domain/manifests/`
   Keep only the schema types (`Offering`, `Capability`, `Category` structs) in common
   since Rake reads them

**Phase C — Decompose `types.rs`**:

```
types/
  mod.rs         — re-exports
  service.rs     — ServiceInfo, ServiceStatus, HealthCheck, Ports
  hardware.rs    — HardwareCapabilities, CpuCapabilities, DiskMetrics
  pond.rs        — PondConfig, KeystoneRequest, StoneInviteRequest
  lantern.rs     — LanternTopology, LanternStoneState
```

---

## Issue 8: Cross-Crate Duplication (Rake vs Orchestrators)

### Symptoms

| Concept                | Rake                             | Orchestrators                                                                            | Inconsistency                         |
| ---------------------- | -------------------------------- | ---------------------------------------------------------------------------------------- | ------------------------------------- |
| Stone discovery        | p2p UDP broadcast                | Koi mDNS HTTP SSE                                                                        | Different transports, no shared trait |
| Tending type           | `TendingState` with `SystemTime` | `TendedStone` with `chrono::DateTime<Utc>`                                               | Same concept, different types         |
| HTTP response checking | `extract_data()` helpers         | `check_response()` (common has it; Ollama has inline copy at `infra/stone_discovery.rs`) | Ollama duplicates common's helper     |
| HTTP client setup      | `main.rs` with mTLS              | `main.rs` without TLS                                                                    | mTLS logic only in Rake/Moss          |

### Fix

**Unify `TendedStone`**: Define one type in `garden-common` with `chrono::DateTime<Utc>`.
Include both `stone_id` and `capabilities` as optional fields.

**Remove Ollama's inline `check_status()`**: Replace with
`orchestrator_common::http::check_response()`.

**Discovery abstraction** (lower priority): Extract a `DiscoveryBackend` trait with
`P2pDiscovery` and `KoiDiscovery` implementations.

**HTTP client factory** (lower priority): Create `HttpClientBuilder` in common that
handles mTLS cert loading, pool sizing, and timeouts.

---

## Issue 9: Event Bus Underutilization

### Symptoms

`EventBus` exists with domain event types (`OfferingEvent`, `StorageEvent`, `StoneEvent`,
`JobEvent`, `PondEvent`) but most mutations don't emit events. The 43+ duplicated
"mutate + persist + sync + chirp" call sites could be event-driven instead.

### Fix

Addressed transitively by Issue 3 (`OfferingLifecycle`). The lifecycle service becomes
the single mutation gateway and emits domain events as a side effect. Consumers (SSE
handlers, chirp broadcaster, pulse bridge) subscribe via the event bus.

---

## Issue 10: Direct `.subscribe()` Calls on Domain Channels

### Symptoms

Code standard Rule 13 says domains should expose events through `on_X()` / `X_stream()`
methods rather than allowing external code to call `.subscribe()` directly on internal
channel fields.

Instances found:

- `api/v1/logs.rs:97` — `state.log.subscribe()`
- `api/v1/storage.rs:1747` — `state.orchestration.storage.tick.debounced.subscribe()`
- `api/v1/tools.rs:81` — `state.tool.delta.subscribe()`
- `api/v1/pulse.rs:58` — `state.pulse.subscribe()`
- `api/v1/presence.rs:71` — `state.pulse.subscribe()`
- `api/v1/nourishment.rs:823` — `jobs.get(&job_id).map(|tx| tx.subscribe())`

### Fix

Add domain API methods. Internal channel fields become private:

```rust
impl AppState {
    pub fn log_stream(&self) -> broadcast::Receiver<String> { self.log.subscribe() }
    pub fn pulse_stream(&self) -> broadcast::Receiver<PulseEvent> { self.pulse.subscribe() }
}

impl Tool {
    pub fn delta_stream(&self) -> broadcast::Receiver<ToolDelta> { self.delta.subscribe() }
}

impl Orchestration {
    pub fn storage_tick_stream(&self) -> broadcast::Receiver<StorageTick> {
        self.storage.tick.debounced.subscribe()
    }
}
```

---

## Issue 11: Inconsistent Background Task Error Handling

### Symptoms

The project context says: "Background tasks: tokio::spawn with mandatory error handling."
6+ `tokio::spawn` calls silently discard errors:

- `api/v1/admin.rs:275,329` — `let _ =` inside spawn
- `api/v1/offering_capabilities.rs:395,848` — no error logging
- `api/v1/services.rs:347` — `let _ =` pattern
- `api/v1/garden_storage/memories.rs:106` — no error logging

Additionally, error severity is inconsistent across the 40+ spawned tasks that do handle
errors: some use `error!`, others `warn!`, for the same class of failure.

### Fix

Audit all `tokio::spawn` calls. Add `if let Err(e) = ... { tracing::error!(...) }`
wrapper to every spawn that currently swallows errors. Standardize severity:

- Infrastructure failures (Docker, filesystem, network): `error!`
- Transient/expected failures (timeout, lag): `warn!`
- Diagnostic information: `debug!`

---

## Issue 12: Developer Experience — Re-exports, Type Aliases, and Boilerplate

### 12a. `garden-common` lib.rs is a junk drawer

`common/src/lib.rs` has 91 lines of re-exports in mixed styles:

- **Wildcard re-exports that hide origins**: `pub use types::*;`, `pub use jobs::*;`,
  `pub use responses::*;`, `pub use utils::*;` (lines 52–57). When downstream code
  writes `use garden_common::ServiceInfo`, the origin is invisible without reading lib.rs.
- **Redundant explicit re-exports**: `pub use types::peer_address::PeerAddress;` and
  `pub use types::topology::TopologyEntry;` (lines 54–55) are already covered by the
  `pub use types::*;` wildcard on line 56.
- **62+ constants re-exported individually** across 4 separate `pub use constants::`
  blocks (lines 60–91): `AUTH_BEARER_PREFIX`, `CHECK_FAIL`, `CHECK_PASS`, `COMPAT_FAIL`,
  `EVENT_DEPLOYED`, `ANNOUNCEMENT_STONE_CHIRP`, `SSE_LEVEL_DEBUG`, etc.

### 12b. Same type importable via 5+ paths

`TopologyEntry` can be imported as:

- `garden_common::TopologyEntry` (via wildcard re-export)
- `garden_common::types::topology::TopologyEntry` (direct path)
- `garden_common::types::TopologyEntry` (via types mod re-export)
- `crate::domain::TopologyEntry` (via moss domain mod.rs re-export of garden_common)
- `crate::domain::topology::TopologyEntry` (via domain submodule)

Similar ambiguity for `HardwareCapabilities`, `Stone`, `OfferingFqn`. Developers pick
inconsistently across files.

### 12c. Domain mod.rs re-exports garden_common types as local

`moss/src/domain/mod.rs:104`: `pub use garden_common::TopologyEntry;` — makes a
garden_common type appear to be a moss domain type. Files then import via
`crate::domain::TopologyEntry` thinking it's local.

`moss/src/domain/mod.rs` spans 130+ lines (60–135) of re-export declarations. Combined
with `moss/src/lib.rs` (98 lines of re-exports), creates three possible import paths for
most domain functions.

### 12d. Missing `ApiResult<T>` type alias

The return type `Result<Json<ApiResponse<T>>, (StatusCode, Json<ApiErrorResponse>)>`
appears in 95+ handler signatures across all `api/v1/*.rs` files. A type alias would
save ~50 characters per signature.

### 12e. Response envelope boilerplate

95+ handlers repeat the pattern:

```rust
let suggestions = generate_suggestions(&ctx);
Ok(Json(ApiResponse { data: response, suggestions }))
```

A helper `fn ok<T>(data: T) -> ApiResult<T>` or
`fn ok_with(data: T, suggestions: Vec<Suggestion>) -> ApiResult<T>` would eliminate this.

### 12f. Lock scope — manual `drop()` instead of scoped blocks

10+ handlers in `api/v1/services.rs` use explicit `drop(offerings)` calls to release
read guards. Scoped blocks are the idiomatic pattern:

```rust
// Bad — manual drop
let offerings = state.offerings.read().await;
let info = offerings.iter().find(...).clone();
drop(offerings);

// Good — scoped block
let info = {
    let offerings = state.offerings.read().await;
    offerings.iter().find(...).clone()
};
```

### 12g. Thin wrapper mod.rs files adding unnecessary depth

| File                                  | Content                   | Creates path                          |
| ------------------------------------- | ------------------------- | ------------------------------------- |
| `common/src/constants/storage/mod.rs` | 1 line: `pub mod share;`  | `constants::storage::share::CONSTANT` |
| `lantern/src/bootstrap/mod.rs`        | 1 line: `pub mod router;` | `bootstrap::router::fn`               |

These add a directory level without organizational value.

### 12h. Missing `Display` impls on enums used in string contexts

Several key enums are formatted via `format!()` or manual `.to_string()` instead of
having `Display` impls:

- `OfferingStatus` — used in error messages and API responses
- `OfferingMode` — used in logging
- `StoneStatus` — used in comparisons and logging

Other enums already have correct `Display` impls: `OsKind`, `DeviceState`, `BusType`,
`MediumCondition`, `StorageVisibility`, `StorageRole`.

### 12i. Missing `From` impls for common conversions

Manual conversions that repeat across call sites:

- `offering_to_service_info()` in `api/v1/services.rs:24–57` — 33 lines of field mapping
  that should be `impl From<&Offering> for ServiceInfo`
- Stone/StoneRef/StoneInfo/StoneInfoResponse conversions — no `From` impls connect
  these representations; each conversion site maps fields manually
- `OfferingView` construction from `Offering` — manual field-by-field in
  `api/v1/offerings.rs`

### 12j. Platform `#[cfg]` attribute inconsistency

131 `#[cfg]` attributes across the codebase use mixed conventions:

- 70 uses of `#[cfg(target_os = "windows")]`
- 68 uses of `#[cfg(target_os = "linux")]`
- 20 uses of `#[cfg(not(target_os = "windows"))]` (semantically "linux" but written
  as negation)

Some files use `target_os = "linux"`, others use `not(target_os = "windows")` for the
same intent. Should standardize: `target_os = "linux"` when Linux-specific,
`not(target_os = "windows")` only when genuinely "any non-Windows platform."

### Fix

**12a — Clean up lib.rs re-exports**:

Remove all wildcard `pub use` from `common/src/lib.rs`. Replace with explicit, grouped
re-exports organized by domain concept. Each item has exactly one canonical import path.
Remove the 62+ individual constant re-exports — consumers import from
`garden_common::constants::*` directly.

Target state for `common/src/lib.rs`:

```rust
// Domain types — one canonical path per type
pub use stone::Stone;
pub use storage::{StorageSummary, StorageRole, VolumeState};
pub use offerings::{OfferingFqn, OfferingSearchResult};
pub use types::topology::TopologyEntry;
pub use types::peer_address::PeerAddress;
pub use types::{ServiceInfo, HardwareCapabilities};

// No wildcard re-exports.
// No constant re-exports — use garden_common::constants::{...} directly.
```

**12b — Enforce single import path**: After cleaning lib.rs, grep for all import
variants and normalize to the canonical path. IDE auto-import will follow.

**12c — Remove cross-crate re-exports from domain mod.rs**: Delete
`pub use garden_common::TopologyEntry` from `moss/src/domain/mod.rs`. Call sites import
from `garden_common` directly. Trim moss domain mod.rs and lib.rs re-exports to only
items genuinely defined in those modules.

**12d — Add `ApiResult<T>` type alias**:

```rust
// moss/src/api/mod.rs (or api/types.rs)
pub type ApiResult<T> = Result<Json<ApiResponse<T>>, (StatusCode, Json<ApiErrorResponse>)>;
```

**12e — Add response helpers**:

```rust
pub fn ok<T: Serialize>(data: T) -> ApiResult<T> {
    Ok(Json(ApiResponse { data, suggestions: vec![] }))
}

pub fn ok_with<T: Serialize>(data: T, suggestions: Vec<Suggestion>) -> ApiResult<T> {
    Ok(Json(ApiResponse { data, suggestions }))
}
```

**12f — Replace `drop()` with scoped blocks**: Mechanical find-and-replace. One commit.

**12g — Flatten trivial wrapper modules**: Inline single-child mod.rs files where
the extra directory adds no value.

**12h — Add missing `Display` impls**: Add `Display` to `OfferingStatus`, `OfferingMode`,
`StoneStatus`. One commit per enum.

**12i — Add missing `From` impls**: Replace manual field-mapping functions with `From`
impls. Delete `offering_to_service_info()` after implementing
`impl From<&Offering> for ServiceInfo`. Addressed transitively by Issue 6 type
unification for Stone variants.

**12j — Standardize `#[cfg]` attributes**: Audit all 131 cfg attributes. Use
`target_os = "linux"` for Linux-specific code. Use `not(target_os = "windows")` only
when the code genuinely applies to any non-Windows platform (macOS, BSD, etc.).

**12k — Fix production `.unwrap()`**: `rake/src/route.rs:956` uses
`.strip_prefix("offering_primary:").unwrap()` after a guard clause. Replace with
`.ok_or()` or `unwrap_or_default()`. Audit for any other `.unwrap()` calls outside
tests.

**12l — Track acknowledged TODOs**: 6 TODO/unimplemented stubs exist in production code:

- `tasks/storage_replication.rs:256` — full directory walk reconciliation (Phase 4e+)
- `infra/secrets.rs:94–136` — TPM and platform keyring backends (stubs only)
- `api/v1/services.rs:1211` — Docker container log streaming
- `api/v1/offerings.rs:287` — simplified config → service creation transform
- `api/v1/presence.rs:239` — `is_lantern` field hardcoded to `false`
- `domain/service_discovery.rs:506` — active UDP broadcast discovery

These are acknowledged deferred features, not bugs. No action required beyond tracking.

---

## Decision

Address all twelve issues in dependency order. Issues are grouped into waves by risk
and dependency:

### Wave A — Constants, Cleanup, and DX Ergonomics (Issues 5, 11, 12)

No architectural changes. Centralize magic numbers, fix silent spawns, improve DX.

1. Create `constants/channels.rs` with all channel capacity constants
2. Extend `constants/timeouts.rs` with operational timeout functions
3. Create `constants/server.rs` with server configuration constants
4. Fix `preinstall.rs` hardcoded path
5. Fix `run.rs` disconnect retry bypassing constant
6. Audit and fix all silent `tokio::spawn` calls
7. Standardize error severity in background tasks
8. Clean up `common/src/lib.rs`: remove wildcards, remove redundant re-exports,
   remove 62+ constant re-exports (consumers use `constants::` directly)
9. Remove cross-crate re-exports from `moss/src/domain/mod.rs` and trim
   `moss/src/lib.rs` re-exports
10. Add `ApiResult<T>` type alias in `moss/src/api/`
11. Add `ok()` / `ok_with()` response helpers
12. Replace manual `drop()` with scoped blocks (10+ instances)
13. Flatten trivial single-child wrapper mod.rs files
14. Add `Display` impls to `OfferingStatus`, `OfferingMode`, `StoneStatus`
15. Standardize `#[cfg]` attributes (131 instances, mixed conventions)
16. Fix production `.unwrap()` in `rake/src/route.rs:956`

### Wave B — Low-Risk Deduplication (Issues 4, 10)

Extract helpers, consolidate shared logic, remove inline duplicates.

1. Extract `ErrorResponse` typed constructors (eliminates 269 inline calls)
2. Extract `Job::new()` constructor
3. Extract `resolve_offering_to_service()` in `service_discovery.rs`
4. Extract SSE stream factory function
5. Extract channel lag handler with behavioral variants
6. Extract `proxy_to_stone()` helper in garden storage
7. Add domain event subscription API methods (`.subscribe()` encapsulation)
8. Remove Ollama's inline `check_status()` (use common's)
9. Normalize all import paths to single canonical path per type

### Wave C — DTO Consolidation (Issue 6)

Move shared response types to `garden-common`. Delete Rake-local duplicates. Unify
intra-crate type variants.

1. Move `StoneRef`, `FoundService`, `ServiceDiscoveryResponse`, `ResolvedConnection`
2. Move `OfferingSlots`, `GardenBankInfo`
3. Enrich `OfferingView` via `#[serde(flatten)]` on `OfferingSearchResult`
4. Rename `presence::StorageSummary` → `StoragePresence`
5. Delete all Rake local deserialization duplicates
6. Unify `ServiceInfo`: `tools/types.rs` variant embeds canonical `types.rs` definition
7. Unify `Stone` representations: delete `tools/types.rs` `Stone`, use canonical
8. Add `From` impls: `From<&Offering> for ServiceInfo`, Stone variant conversions
9. Delete `offering_to_service_info()` and other manual field-mapping functions

### Wave D — Domain Service Extraction (Issues 3, 9)

Introduce `ServiceLifecycle` and `OfferingLifecycle` domain services. Handlers delegate
to services. Background tasks delegate to services. Event bus utilization increases
transitively.

Depends on: Wave B (error constructors used by services).

### Wave E — Domain–Infra Trait Boundaries (Issue 1)

Extract 10 domain traits. One trait per commit, in isolation order.

Depends on: Wave D (services define which traits they need).

### Wave F — God Module Decomposition (Issue 2)

Split `storage/mod.rs` into 8 submodules.

Depends on: Wave E (trait boundaries clarify module responsibilities).

### Wave G — Common Scope Migration (Issue 7)

Move ~10K lines out of `garden-common`. Three phases: Rake-specific, Moss-specific,
`types.rs` decomposition.

Independent of Waves A–F. Can run in parallel from Wave C onward.

### Wave H — Cross-Crate Unification (Issue 8)

Unify `TendedStone`, remove Ollama duplicate, optionally extract `DiscoveryBackend`
trait.

Depends on: Wave G (common's scope is settled before adding new shared types).

---

## Specification Corrections

Implementation revealed three inaccuracies in the original audit:

### Correction 1: `console/` is genuinely shared (not Rake-specific)

The original audit (Issue 7) classified `console/` as Rake-specific code that should move
to `rake/src/ui/console/`. Investigation found **30+ import sites in Moss** — including
`ConsolePrinter`, `ConsoleEvent`, `EventCategory`, `EventStatus`, `BootBannerInfo`,
`ShutdownBannerInfo`, and `UpdateBannerInfo`. The console module is a legitimate shared
contract and correctly remains in `garden-common`.

### Correction 2: `tools::ServiceInfo` and canonical `ServiceInfo` are different concepts

The original audit (Issue 6, item 6) prescribed unifying `tools/types.rs::ServiceInfo`
with `types/service.rs::ServiceInfo` via `#[serde(flatten)]`. These are genuinely different
types serving different wire protocols:

- **Canonical `ServiceInfo`**: offering-oriented (health, resources, sub_capabilities,
  guidance, customized_by) — used by `GET /api/v1/stone/services`
- **Tools `ServiceInfo`**: connection-oriented (protocol, URIs, ready, hostname, ip, port,
  uri_template) — used by `/garden/tools` streaming and `/garden/services` discovery

Merging them would break the tools wire format. They remain separate types.

### Correction 3: `koi_client.rs` is genuinely shared (not Moss-only)

The original audit (Issue 7) classified `infra/koi_client.rs` as Moss-specific. Lantern
also imports `koi_client::is_lan_routable` and `koi_client::DiscoveredStone`. It correctly
remains in `garden-common`.

---

## Implementation Status

**Last updated**: 2026-03-15
**Branch**: `arch-0005/common-scope` (9+ commits on `dev`)

### Wave A — Constants, Cleanup, DX: COMPLETE

| Task | Status | Evidence |
|------|--------|---------|
| Create `constants/channels.rs` | Done | 7 named capacities (LOG_STREAM through MONITOR_EVENT) |
| Extend `constants/timeouts.rs` | Done | +8 operational timeouts with env overrides |
| Create `constants/server.rs` | Done | 6 server config constants |
| Fix hardcoded path in preinstall.rs | Done | Uses `stone_home()` |
| Fix disconnect retry constants | Done | Already used named constants |
| Audit silent `tokio::spawn` | Done | 4 silent `let _ =` patterns fixed in admin.rs |
| Add `ApiResult<T>` type alias | Done | `api/mod.rs` with `ok()`, `ok_with()`, `ok_maybe()` |
| Migrate handler return types | Done | 70 handler signatures updated |
| Remove `TopologyEntry` cross-crate re-export | Done | 9 call sites updated to `garden_common::TopologyEntry` |
| Clean lib.rs wildcards | Done | Explicit grouped re-exports |
| Add `Display` impls | Done | `OfferingStatus`, `OfferingMode`, `StoneStatus` already had them |
| Fix production `.unwrap()` | Done | `route.rs:956` already uses `.unwrap_or()` fallback |
| Replace hardcoded channel capacities | Done | 14 sites across 8 files (0 remaining) |
| Replace hardcoded durations | Done | 10 sites across 6 files |

**Not done (low value / mechanical)**: `drop()` → scoped blocks (10+ instances),
`#[cfg]` attribute standardization (131 instances), constant re-export removal from
lib.rs (62+ re-exports, 18 consumer files).

### Wave B — Low-Risk Deduplication: SUBSTANTIAL

| Task | Status | Evidence |
|------|--------|---------|
| Typed error constructors | Done | 8 constructors, 201 of 213 inline calls migrated |
| Persist offerings consolidation | Done | All gateway methods auto-persist; 0 external calls |
| SSE stream factory | Deferred | Inner streams too varied per endpoint; low ROI |
| Channel lag handler | Deferred | 3 behavioral variants need careful enum design |
| `.subscribe()` encapsulation | Deferred | 6 direct calls remain |
| Job::new() constructor | Deferred | Only 3 instances |
| proxy_to_stone() helper | Deferred | Only 4 instances |

### Wave C — DTO Consolidation: SUBSTANTIAL

| Task | Status | Evidence |
|------|--------|---------|
| Move discovery types to common | Done | `common/src/discovery.rs` with 4 canonical types |
| Rename `StorageSummary` collision | Done | → `StoragePresence` in presence/types.rs |
| Delete Rake discovery duplicates | Done | StoneRef, FoundService, ConnectionInfo removed |
| Unify `ServiceInfo` variants | Dropped | See Correction 2 — genuinely different types |
| Unify `Stone` representations | Deferred | tools::Stone vs discovery::StoneRef |
| Move `OfferingSlots` to common | Deferred | Deep dependency on moss-only HarvestManifest |
| Add `From` impls | Deferred | Async variant of offering_to_service_info blocks From |

### Wave D — Domain Service Extraction: PARTIAL

| Task | Status | Evidence |
|------|--------|---------|
| `ServiceLifecycle` (stop/start/restart/remove/destroy) | Done | `domain/service_lifecycle.rs` with 5 operations |
| `services_internal` shared helpers | Done | build_spec_from_manifest, rebuild_missing_container, compose_on_start |
| Thin handlers (stop/start/restart/remove/destroy) | Done | services.rs 2,013 → 1,458 lines (-28%) |
| `OfferingLifecycle` mutation gateway | Partial | Auto-persist done; formal OfferingLifecycle struct not created |
| Install path consolidation | Not started | 4 install entry points still inline |
| Event bus utilization | Not started | Depends on OfferingLifecycle |

### Wave E — Domain–Infra Trait Boundaries: COMPLETE

| Task | Status | Evidence |
|------|--------|---------|
| 14 domain traits defined | Done | `domain/traits/` with 14 trait files |
| All domain→infra imports eliminated | Done | 0 non-test `crate::infra` in domain |
| Value types moved to domain | Done | `ImageInspection`, `VolumeSnapshot`, etc. |
| `Management.store` field removed | Done | Store passed as parameter, not stored |
| Bootstrap wiring | Done | `OsPlatform`, `OsDockerConfig`, `ContentStore` injected |

### Wave F — God Module Decomposition: COMPLETE

| Task | Status | Evidence |
|------|--------|---------|
| Split storage/mod.rs | Done | 72-line facade + 7 submodules |

Submodules: `volume.rs` (380 lines), `collection.rs` (290), `medium.rs` (130),
`automount.rs` (90), `analysis.rs` (100), `platform_types.rs` (190), `bank.rs` (150).

### Wave G — Common Scope Migration: SUBSTANTIAL

| Task | Status | Evidence |
|------|--------|---------|
| Move `ui/` to rake | Done | 2,222 lines → `rake/src/ui/rendering/` |
| Move `cli_colors.rs` to rake | Done | 410 lines → `rake/src/ui/colors.rs` |
| Move `console/` to rake | Dropped | See Correction 1 — genuinely shared |
| Move `registry_client.rs` to moss | Done | 414 lines → `moss/src/infra/` |
| Move `koi_client.rs` to moss | Dropped | See Correction 3 — genuinely shared |
| Decompose `types.rs` | Done | 12 submodules in `types/` |
| Narrow lib.rs re-exports | Done | Wildcards → explicit grouped re-exports |
| Move `api_manifest.rs` to moss | Deferred | 312 lines, low priority |
| Move manifest validation to moss | Deferred | ~3K lines, complex dependency audit needed |

**Net reduction**: ~3,046 lines moved out of `garden-common`. 3 unused deps removed
from `common/Cargo.toml` (`colored`, `terminal_size`, `supports-color`).

### Wave H — Cross-Crate Unification: COMPLETE (prior work)

| Task | Status | Evidence |
|------|--------|---------|
| Ollama dedup via orchestrator-common | Done | PR-8 (prior session) |

---

## Next Steps

### Priority 1: OfferingLifecycle domain service

Create `domain/offering_lifecycle.rs` with `upsert()`, `remove()`, `update()`,
`batch_update()`, `find()`, `find_by_fqn()` methods. This consolidates the 12
`state.offerings.read()` patterns in services.rs, makes event bus emission automatic
on every mutation, and enables the remaining Wave D work (install path consolidation).

**Estimated scope**: ~200 lines of new domain code, ~15 handler files updated.

### Priority 2: Install path consolidation

The `create_service_v1` handler still contains ~300 lines of inline business logic:
image-direct detection, compatibility fallback, self-heal adoption, job creation.
Extract into `ServiceLifecycle::install()` with clear return types
(`InstallationJob`, `AdoptionResult`, etc.).

**Estimated scope**: ~300 lines moved from services.rs to service_lifecycle.rs.

### Priority 3: .subscribe() encapsulation (Issue 10)

Add domain API methods: `log_stream()`, `pulse_stream()`, `delta_stream()`,
`storage_tick_stream()`. Make internal channel fields private. Update 6 call sites.

**Estimated scope**: ~30 lines of new API methods, 6 handler updates.

### Deferred (low priority)

- `OfferingSlots`/`NurturingSnapshot` DTO consolidation (blocked by HarvestManifest dependency)
- Constant re-export removal from lib.rs (18 files, cosmetic)
- `drop()` → scoped blocks (10+ instances, cosmetic)
- `#[cfg]` attribute standardization (131 instances, cosmetic)
- `api_manifest.rs` and manifest validation move to moss (~3.3K lines)
- SSE stream factory (low ROI — inner streams too varied)

---

## Rationale

- **Wave ordering minimizes risk**: constants and cleanup first (zero architectural
  change), then deduplication (safe helper extraction), then DTO consolidation (shared
  types before services), then services (structural but internal), then traits (enables
  testing), then decomposition (benefits from clear boundaries), then cross-crate (widest
  blast radius last)
- **Each wave is independently valuable**: shipping Wave A alone improves the codebase.
  No wave creates debt that requires a later wave to resolve.
- **Domain services before traits**: extracting `ServiceLifecycle` first clarifies which
  infra capabilities the domain actually needs, making trait design precise rather than
  speculative
- **DTO consolidation before services**: domain services should return canonical types,
  not handler-local view types

---

## Consequences

### Positive (realized)

- Domain logic testable without Docker/filesystem/network (14 trait boundaries)
- 201 inline error response calls replaced with 8 typed constructors
- 14 hardcoded channel capacities replaced with named constants
- 10 hardcoded operational durations replaced with named timeout functions
- Discovery types shared canonically between Rake and Moss (4 types in `common/src/discovery.rs`)
- `garden-common` reduced by ~3,046 lines (UI/colors to rake, registry_client to moss)
- `storage/mod.rs` split from 1,155 lines into 7 navigable submodules
- All background tasks log errors consistently (zero silent spawns)
- 70 handler signatures shortened via `ApiResult<T>` + `ok()`/`ok_with()`/`ok_maybe()`
- `common/src/lib.rs` uses explicit grouped re-exports (no wildcards)
- 26 redundant `persist_offerings()` calls eliminated (all gateways auto-persist)
- 5 handler operations (stop/start/restart/remove/destroy) consolidated into `ServiceLifecycle`
- `services.rs` reduced from 2,013 to 1,458 lines (-28%)

### Negative (realized)

- 14 trait definitions add indirection (acceptable — enables domain-level testing)
- `Management.store` field removal required threading store params through 7 callers
- Moving UI code to rake required updating ~55 import sites
- `console/` could not move as planned (genuinely shared, not Rake-specific)
- `tools::ServiceInfo` unification was incorrect (genuinely different wire protocols)

### Neutral

- Total line count roughly unchanged — code moved between files, layers, and crates
- Test count stable at 493 (moss) + 66 (rake) throughout refactoring

---

## Alternatives Considered

### Alternative 1: Trait boundaries first, services second

- **Description**: Extract all 10 domain traits before introducing domain services
- **Rejected because**: services inform trait design, not the other way around.
  Trait design without knowing which operations the domain services need results in
  over-broad traits that get narrowed later.

### Alternative 2: Leave `garden-common` as-is

- **Description**: Accept that common contains Moss-specific and Rake-specific code
- **Rejected because**: scope creep compounds. Orchestrators compile 5K lines of TUI
  code they never call. New contributors cannot distinguish shared contracts from
  single-crate code.

### Alternative 3: Full event-sourcing for offering mutations

- **Description**: All offering state changes become events; state is derived
- **Rejected because**: `OfferingLifecycle` as a mutation gateway achieves the
  consolidation benefit without the architectural overhead of event-sourcing a
  daemon that manages authoritative local state.

### Alternative 4: Macro-based error response generation

- **Description**: Use a proc macro to generate error response types from a DSL
- **Rejected because**: typed constructors on `ErrorResponse` are simpler, debuggable,
  and IDE-navigable. A macro adds build complexity for a pattern that is solved with
  plain methods.

---

## References

- ARCH-0003: Code Standards Compliance Migration
- ARCH-0004: AppState Domain Context Extraction
- `docs/code-standards.md`: Authoritative Rust code standards
