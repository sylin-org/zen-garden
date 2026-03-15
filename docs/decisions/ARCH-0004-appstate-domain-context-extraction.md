---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-11
---

# ARCH-0004: AppState Domain Context Extraction

**Date**: 2026-03-11
**Status**: Accepted
**Depends on**: ARCH-0003 (Code Standards Compliance Migration)

## Context

ARCH-0003 defined a full structural migration plan, including Wave 6c — the `AppState`
restructure that was to introduce 7 domain context structs, migrate all flat fields, add
`FromRef` impls, and enforce minimal handler dependency surfaces. Wave 6c was explicitly
deferred with the note "Leave AppState for after."

The executed passes (A–F) delivered naming and minor structural work but left `AppState`
as a 40-field flat bag. What was **not** delivered:

- **§5 domain ownership through struct nesting** — no grouping context structs were
  created. `AppState` remains flat.
- **§14 file/concept 1:1 coupling** — `domain/` is a flat list of files with no
  structural correspondence between file path, type hierarchy, and runtime instance path.
- **§6 `FromRef` handler narrowing** — handlers still take the full `AppState`.

### The Invariant

Three things must align 1:1 for a domain boundary to be real:

1. **File path** — where the code lives
2. **Type declaration** — the struct/enum hierarchy
3. **Runtime instance** — the field access path on `AppState`

Example for Security:

```
src/moss/src/domain/security/mod.rs        → pub struct Security
src/moss/src/domain/security/pond.rs       → pub struct Pond
src/moss/src/domain/security/ceremony.rs  → pub struct Ceremony
```

```rust
// Type hierarchy
Security { pond: Pond, stone_client: Arc<StoneClient>, https: Arc<AtomicBool> }
Pond { ceremony: Ceremony, active: Arc<AtomicBool>, state: PondState }
Ceremony { host: Arc<CeremonyHost>, registry: Arc<CeremonyRegistry>, ... }

// Runtime path
state.security.pond.ceremony.host
state.security.pond.active
```

A field like `state.pond_active` is not a domain boundary — it is a name with an
underscore. The compiler does not see Security; neither does anyone reading the code cold.

### Data Plane vs. Coordination Plane

A critical architectural distinction drives the domain split:

- **Domain types define what things *are*** — Storage is volumes and media. Security is
  a pond and its ceremony infrastructure. These are the data plane.
- **Orchestration defines how we *coordinate* across domains** — tick signals, rescan
  triggers, nurturing schedulers. These are coordination primitives, not storage data.

`Storage { volumes, media }` is correct. `Storage { volumes, media, tick, nudge, harvest }`
conflates the data plane with its coordination layer.

### `current` vs. Garden-Wide Domains

Every domain has two views:

- **`state.current.*`** — this stone's local, owned, writable state. Authoritative source
  for this stone.
- **`state.*`** — garden-wide aggregate: this stone's current + remote stones' announced
  data merged in. Equivalent to what topology is for stone presence.

| This stone (current) | Garden-wide aggregate |
|---|---|
| `state.current.storage` | `state.storage` |
| `state.current.tool.registry` | `state.tool.registry` |
| `state.current.fqn_handler.registry` | `state.fqn_handler.registry` |
| `state.current.stone` | — (identity; no aggregate equivalent) |
| `state.current.topology` | — (self-entry; feeds into garden topology broadcast) |

Write paths:
- **Local change** → write to `state.current.*`, then propagate/merge into the
  garden-wide aggregate.
- **Remote beacon arrives** → write directly into the garden-wide aggregate `state.*`.

`state.current.storage` is what Phase 1 committed as `state.storage`. The naming is
corrected in Phase 8 when the `current` struct is introduced.

### `fqn` Is a Value Type, Not a Domain

`fqn` (fully qualified name) is a cross-cutting **value type** — `OfferingFqn` in
`garden_common`. It has parse methods, encoding, display. A storage has an FQN; an
offering has an FQN. It is a naming concern, not a domain.

`fqn_handler` is the domain: processes that register as handlers for `OfferingFqn`
patterns and take over FIND request resolution for those FQNs.

### FQN Handlers Are Not Internal Orchestrators

The codebase uses "orchestrator" to mean two unrelated things:

1. **External FQN handlers** — processes like `garden-ollama`, `garden-mongodb` that
   register via `PUT /api/v1/garden/gateway/{offering}` to claim handler responsibility
   for an `OfferingFqn`. They have a physical location (local port or remote stone).
   Currently their registrations are mixed into `GardenRegistry` alongside stone/tool
   discovery data.

2. **Internal coordination primitives** — broadcast channels, notify handles, and
   schedulers that drive Moss's own internal loops (storage ticks, volume rescans,
   nurturing jobs). These live in `orchestration.*`.

These must be separated. FQN handler registrations belong in `state.fqn_handler.*`;
the internal coordination plane is `state.orchestration.*`.

### `current` Is This Node's Self-Model

`current` is the node's complete live self-model: who it is AND what it sees locally.

```rust
pub struct Current {
    pub stone:    Arc<RwLock<Stone>>,   // who this node IS
    pub topology: TopologyContext,      // what this node SEES of itself
}

pub struct TopologyContext {
    pub cache:      TopologyCache,
    pub dirty:      TopologyDirtyFlag,
    pub self_entry: Arc<RwLock<TopologyEntry>>,
}
```

`topology_cache`, `topology_dirty`, and `self_entry` all collapse here. Topology is
this instance's perception of the garden — it belongs on `current`, not floating at
the `AppState` level.

## Decision

Execute the domain context extraction deferred from ARCH-0003, governed by these rules:

### Rule 1: No delegation shims

Old call sites are not kept compiling via delegation methods or re-export aliases.
When `stone_id` is removed from `AppState`, every call site must be updated to
`state.current.stone.read().id`. A method `AppState::stone_id()` that returns
`self.current.stone.read().id` is prohibited — it hides migration debt without paying it.

### Rule 2: File path = type path = runtime path

Every domain context struct lives at the path that names it:

```
domain/current/mod.rs                    → Current
domain/current/topology.rs              → TopologyContext
domain/storage/mod.rs                   → Storage (local; Phase 1 done — renamed in Phase 8)
domain/orchestration/mod.rs             → Orchestration     ✓ Phase 1
domain/orchestration/storage.rs         → StorageOrchestration  ✓ Phase 1
domain/orchestration/nurturing.rs       → NurturingOrchestration  ✓ Phase 1
domain/orchestration/nourishment.rs     → NourishmentOrchestration  ✓ Phase 1
domain/tool/mod.rs                      → Tool
domain/fqn_handler/mod.rs              → FqnHandler
domain/security/mod.rs                 → Security
domain/security/pond.rs                → Pond
domain/security/ceremony.rs            → Ceremony
domain/discovery/mod.rs                → Discovery
domain/companion/mod.rs                → Companion
domain/infra/mod.rs                    → Infra
domain/presence/mod.rs                 → Presence
```

Files that do not match this layout are moved via two-commit discipline: rename commit
(pure `git mv`, no content changes) then content commit.

### Rule 3: Compiler-guided field migration

Each `AppState` flat field is removed, the compiler enumerates all broken call sites,
and each call site is updated to the correct domain path. Fields are migrated in
dependency order — start with the most isolated (fewest callers, clearest domain home).

Each field is one commit. The build must pass between commits.

### Rule 4: `FromRef` at the handler boundary

Once a domain context struct exists as a field on `AppState`, add:

```rust
impl FromRef<AppState> for Arc<Storage> {
    fn from_ref(state: &AppState) -> Self { state.storage.clone() }
}
```

Handlers that previously took `State(state): State<AppState>` are updated
domain-by-domain as their fields are migrated. Background tasks receive individual
domain context clones rather than a full `AppState` clone where they touch a single domain.

---

## Target Domain Context Structs

### `current` — Node Self-Model

```rust
pub struct Current {
    pub stone:    Arc<RwLock<Stone>>,
    pub topology: TopologyContext,
    pub resources: Arc<RwLock<Option<StoneResources>>>,
    pub metrics:  CurrentMetrics,
}

pub struct TopologyContext {
    pub cache:      TopologyCache,
    pub dirty:      TopologyDirtyFlag,
    pub self_entry: Arc<RwLock<TopologyEntry>>,
}

pub struct CurrentMetrics {
    pub network: Arc<RwLock<Option<NetworkMetrics>>>,
    pub gpu:     Arc<RwLock<Option<f32>>>,
}
```

| Flat field | Domain path |
|---|---|
| `stone_id` | `current.stone.read().id` |
| `stone_name` | `current.stone.read().name` |
| `topology_cache` | `current.topology.cache` |
| `topology_dirty` | `current.topology.dirty` |
| `self_entry` | `current.topology.self_entry` |
| `system_resources` | `current.resources` |
| `network_metrics_cache` | `current.metrics.network` |
| `gpu_utilization` | `current.metrics.gpu` |

### `storage` — Local Data Plane → Garden-Wide Aggregate

Phase 1 introduced `state.storage` as this stone's local data plane. In Phase 8,
`state.storage` (local) is re-homed to `state.current.storage`, and `state.storage`
becomes the garden-wide aggregate of all stones' storage.

**Phase 1 (current, local data plane):**

```rust
pub struct Storage {
    pub volumes: Volumes,
    pub media:   Media,
    pub changed: broadcast::Sender<StorageChanged>,
}
```

**Phase 8 target:**

```rust
// state.current.storage — this stone's local data plane (same struct, re-homed)
// state.storage         — garden-wide aggregate (future; new type)
```

| Flat field | Domain path |
|---|---|
| `volumes` | `current.storage.volumes` (currently `storage.volumes` — corrected in Phase 8) |
| `media` | `current.storage.media` (currently `storage.media` — corrected in Phase 8) |

### `orchestration` — Coordination Plane ✓ Phase 1

```rust
pub struct Orchestration {
    pub storage:     StorageOrchestration,
    pub nurturing:   NurturingOrchestration,
    pub nourishment: NourishmentOrchestration,
}

pub struct StorageOrchestration {
    pub tick:   Tick,
    pub nudge:  Arc<Notify>,
    pub rescan: mpsc::Sender<()>,
}

pub struct Tick {
    pub raw:       broadcast::Sender<StorageTick>,
    pub debounced: broadcast::Sender<StorageTick>,
}

pub struct NurturingOrchestration {
    pub harvest: Arc<HarvestStore>,
    pub store:   Arc<NurturingStore>,
}

pub struct NourishmentOrchestration {
    pub jobs: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
}
```

| Flat field | Domain path |
|---|---|
| `storage_tick_raw` | `orchestration.storage.tick.raw` ✓ |
| `storage_tick_debounced` | `orchestration.storage.tick.debounced` ✓ |
| `orchestration_nudge` | `orchestration.storage.nudge` ✓ |
| `volume_rescan` | `orchestration.storage.rescan` ✓ |
| `harvest_store` | `orchestration.nurturing.harvest` ✓ |
| `nurturing_store` | `orchestration.nurturing.store` ✓ |
| `nourishment_jobs` | `orchestration.nourishment.jobs` ✓ |

### `tool` — Garden Tool Registry

Two separate stores that mirror the current/garden-wide pattern:

```rust
// state.current.tool — this stone's tools (authoritative local write side)
pub struct CurrentTool {
    pub registry: ToolRegistry,  // Local-origin GardenTool entries for this stone
}

// state.tool — garden-wide aggregate
pub struct Tool {
    pub registry: GardenRegistry,  // All stones: Local + Announced entries
    pub delta:    broadcast::Sender<ToolDelta>,
}
```

**Write path**: local offering/storage change → write to `state.current.tool.registry`,
propagate into `state.tool.registry`.

**Remote path**: remote beacon → write directly into `state.tool.registry`.

| Flat field | Domain path |
|---|---|
| `registry` (GardenRegistry, Local+Announced) | `tool.registry` |
| `tools` (broadcast::Sender<ToolDelta>) | `tool.delta` |

`state.current.tool.registry` is the local projection; it replaces the
`reconcile_local` write path.

### `fqn_handler` — FQN Handler Registry

External processes that register as handlers for `OfferingFqn` patterns. Registration
means: "when a FIND request arrives for this FQN, forward it to me." Registrations are
ephemeral, TTL-based; handlers refresh every 30 seconds.

Two separate stores:

```rust
// state.current.fqn_handler — handlers registered on this stone
pub struct CurrentFqnHandler {
    pub registry: Arc<RwLock<FqnHandlerRegistry>>,
}

// state.fqn_handler — garden-wide view (this stone's + remote stones')
pub struct FqnHandler {
    pub registry: Arc<RwLock<FqnHandlerRegistry>>,
}

pub struct FqnHandlerEntry {
    pub fqn:          OfferingFqn,
    pub location:     FqnHandlerLocation,
    pub registered_at: DateTime<Utc>,
    pub expires_at:   Instant,
}

pub enum FqnHandlerLocation {
    Local  { port: u16 },
    Remote { stone_id: String, endpoint: String },
}
```

`PUT /api/v1/garden/gateway/{offering}` writes to `state.current.fqn_handler.registry`
and propagates to `state.fqn_handler.registry`.

FIND resolution reads `state.fqn_handler.registry` to locate the handler before
proxying the request.

`EntryOrigin::Registered` entries are removed from `GardenRegistry`. After this phase,
`GardenRegistry` holds only `Local` and `Announced` entries.

### `security` — Trust Domain

```rust
pub struct Security {
    pub pond:         Pond,
    pub stone_client: Arc<StoneClient>,
    pub https:        Arc<AtomicBool>,
}

pub struct Pond {
    pub state:    PondState,
    pub active:   Arc<AtomicBool>,
    pub ceremony: Ceremony,
}

pub struct Ceremony {
    pub host:     Arc<CeremonyHost<PondCeremonyRules>>,
    pub registry: Arc<CeremonyRegistry>,
    pub journal:  Arc<CeremonyJournal>,
}
```

| Flat field | Domain path |
|---|---|
| `pond` | `security.pond.state` |
| `pond_active` | `security.pond.active` |
| `https_started` | `security.https` |
| `stone_client` | `security.stone_client` |
| `ceremony_registry` | `security.pond.ceremony.registry` |
| `ceremony_journal` | `security.pond.ceremony.journal` |
| `pond_ceremony_host` | `security.pond.ceremony.host` |

### `discovery` — Network Presence

```rust
pub struct Discovery {
    pub mdns: Option<Arc<MdnsHandle>>,
    pub koi:  Arc<KoiHandle>,
}
```

| Flat field | Domain path |
|---|---|
| `mdns_handle` | `discovery.mdns` |
| `koi_handle` | `discovery.koi` |

### `companion` — Companion Processes

```rust
pub struct Companion {
    pub registry: Arc<CompanionRegistry>,
}
```

| Flat field | Domain path |
|---|---|
| `companion_registry` | `companion.registry` |

### `infra` — Platform Infrastructure

`infra.handlers` holds local infrastructure reactors (e.g. `DockerRegistry` updates
Docker's `insecure-registries` list when a container registry stone joins the pond).
These react to topology events — they are not FQN handlers.

```rust
pub struct Infra {
    pub docker:   Arc<Client>,
    pub runtime:  Arc<dyn PlatformRuntime>,
    pub network:  Arc<Network>,
    pub handlers: Arc<InfrastructureHandlerRegistry>,
}
```

| Flat field | Domain path |
|---|---|
| `docker` | `infra.docker` |
| `runtime` | `infra.runtime` |
| `network` | `infra.network` |
| `infrastructure_handlers` | `infra.handlers` |

### `presence` — P2P Presence

```rust
pub struct Presence {
    pub elections:     Arc<Elections>,
    pub notifications: Arc<NotificationRegistry>,
}
```

| Flat field | Domain path |
|---|---|
| `elections` | `presence.elections` |
| `notifications` | `presence.notifications` |

---

## Cross-Cutting Fields (Remain Flat on AppState)

These fields genuinely belong at the `AppState` level:

| Field | Reason |
|---|---|
| `event_bus` | All domains publish to it; no single owner |
| `shutdown_token` | Lifecycle primitive; all tasks hold a copy |
| `console` | Output primitive; pre-dates domain structure |
| `start_time` | Stone uptime; belongs to no domain |
| `api_port` | Infrastructure constant |
| `pulse` | Cross-domain broadcast firehose |
| `log` | Cross-domain log broadcast |
| `offerings` | Offering lifecycle spans all domains |
| `manifest_registry` | Read by offerings, storage, security, infra |
| `jobs` | Background job tracker; spans domains |
| `offerings_index` | Computed view; read-only cache |
| `subsystems` | Boot readiness flags |
| `capabilities` | Hardware capability cache; read by offerings, security, infra |

---

## Migration Order

Fields are migrated in dependency order — most isolated first, most pervasive last.
Each field is one commit; the build must pass between commits.

### Phase 1 — Correct `Storage` and create `Orchestration` ✓ Complete

`Storage` stripped to data plane `{ volumes, media, changed }`. `Orchestration` created
with `{ storage, nurturing, nourishment }` sub-structs. All 35 call sites updated.

Commit: `9e6586d` — ARCH-0004: introduce Orchestration domain; correct Storage to data plane only

### Phase 2 — `tool`

1. Create `domain/tool/mod.rs` with `Tool` and `CurrentTool`
2. Add `tool: Arc<Tool>` to `AppState`; remove `registry` and `tools` flat fields
3. Migrate: `state.registry` → `state.tool.registry`
4. Migrate: `state.tools` → `state.tool.delta`
5. `state.current.tool.registry` write path replaces `reconcile_local` direct calls

### Phase 3 — `fqn_handler`

1. Create `domain/fqn_handler/mod.rs` with `FqnHandler`, `CurrentFqnHandler`,
   `FqnHandlerRegistry`, `FqnHandlerEntry`, `FqnHandlerLocation`
2. Add `fqn_handler: Arc<FqnHandler>` to `AppState`
3. Extract `EntryOrigin::Registered` entries from `GardenRegistry` into
   `state.current.fqn_handler.registry`
4. Migrate gateway API writes → `state.current.fqn_handler.registry`
5. Migrate FIND resolution reads → `state.fqn_handler.registry`
6. Remove `gateway_entries()`, `gateway_for_offering()`, `EntryOrigin::Registered`
   from `GardenRegistry`

### Phase 4 — `security`

1. Create `domain/security/mod.rs`, `pond.rs`, `ceremony.rs`
2. Migrate: `pond`, `pond_active`, `https_started`, `stone_client`, ceremony fields

### Phase 5 — `discovery`

1. Create `domain/discovery/mod.rs`
2. Migrate: `mdns_handle`, `koi_handle`

### Phase 6 — `companion`

1. Create `domain/companion/mod.rs`
2. Migrate: `companion_registry`

### Phase 7 — `infra`

1. Create `domain/infra/mod.rs`
2. Migrate: `docker`, `runtime`, `network`, `infrastructure_handlers`

### Phase 8 — `presence`

1. Create `domain/presence/mod.rs`
2. Migrate: `elections`, `notifications`

### Phase 9 — `current`

1. Create `domain/current/mod.rs`, `domain/current/topology.rs`
2. Re-home `state.storage` → `state.current.storage` (corrects Phase 1 naming)
3. Migrate: `topology_cache`, `topology_dirty`, `self_entry` → `current.topology`
4. Migrate: `system_resources`, `network_metrics_cache`, `gpu_utilization`
5. Migrate: `stone_id`, `stone_name` → `current.stone` (most pervasive — done last)

---

## Acceptance Criteria

The migration is complete when:

1. `AppState` holds only domain context structs and the cross-cutting fields listed above
2. Every domain context struct lives in the file whose path matches its type name
3. Every Axum handler takes the narrowest domain context it actually needs via `FromRef`
4. `cargo check --all` and `cargo clippy --all -- -D warnings` pass clean
5. No delegation shim methods exist on `AppState` that proxy to domain context fields

---

## Consequences

**Positive**:
- Handler dependency surfaces are enforced by the compiler
- `fqn_handler` domain separates FIND resolution from stone/tool discovery
- `fqn` value type (`OfferingFqn`) remains a clean cross-cutting concern in `garden_common`
- The `current` / garden-wide symmetry makes local vs. aggregate state explicit at every
  call site
- The data plane (`current.storage.*`) is never polluted by coordination primitives
- New code has an unambiguous home — the file path tells you where to put it
- Domain contexts are independently constructable and testable

**Negative / Trade-offs**:
- `current.stone.read().id` at every identity call site is more verbose than `stone_id`;
  this is the correct trade — explicitness over convenience
- Phase 9 (`current`) requires re-homing `state.storage` to `state.current.storage`,
  touching all storage call sites a second time
- Background tasks that currently clone the full `AppState` must be audited and narrowed

## Out of Scope

- `anyhow` to typed error enum migration (ARCH-0003 pass `f`) — separate concern
- `bootstrap/run.rs` declarative pipeline (ARCH-0003 Wave 6e) — separate concern
- Orchestrator crates (`src/orchestrators/`) — standalone builds, separate migration
- `garden-common` value objects — Wave 1 of ARCH-0003; tackled separately
