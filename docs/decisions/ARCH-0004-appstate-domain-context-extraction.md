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
appState.security.pond.ceremony.host
appState.security.pond.active
```

A field like `appState.pond_active` is not a domain boundary — it is a name with an
underscore. The compiler does not see Security; neither does anyone reading the code cold.

### Data Plane vs. Coordination Plane

A critical architectural distinction drives the domain split:

- **Domain types define what things *are*** — Storage is volumes and media. Security is
  a pond and its ceremony infrastructure. These are the data plane.
- **Orchestration defines how we *coordinate* across domains** — tick signals, rescan
  triggers, nurturing schedulers. These are coordination primitives, not storage data.

`Storage { volumes, media }` is correct. `Storage { volumes, media, tick, nudge, harvest }`
conflates the data plane with its coordination layer.

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

These must be separated. `Fqn` is a first-class type in the codebase (like DNS — an
acronym that names a concept). `state.fqn.registry` is the home for external handler
registrations; the internal coordination plane is `state.orchestration.*`.

### `current` Is This Node's Self-Model

`current` is not only identity (`stone_id`, `stone_name`) — it is the node's complete
live self-model: who it is AND what it sees. Topology is a perception, not a garden-wide
truth; different nodes may have different views.

```rust
pub struct Current {
    pub stone:    Arc<RwLock<Stone>>,   // who this node IS
    pub topology: TopologyContext,      // what this node SEES
}

pub struct TopologyContext {
    pub cache:      TopologyCache,
    pub dirty:      TopologyDirtyFlag,
    pub self_entry: Arc<RwLock<TopologyEntry>>,
}
```

`topology_cache`, `topology_dirty`, and `self_entry` all collapse here. Topology is
this instance's perception of the garden — it belongs on `current`, not floating at the
`AppState` level.

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
domain/current/mod.rs               → Current
domain/current/topology.rs          → TopologyContext
domain/storage/mod.rs               → Storage          (already moved; content needs correction)
domain/orchestration/mod.rs         → Orchestration
domain/orchestration/storage.rs     → StorageOrchestration
domain/orchestration/nurturing.rs   → NurturingOrchestration
domain/orchestration/nourishment.rs → NourishmentOrchestration
domain/security/mod.rs              → Security
domain/security/pond.rs             → Pond
domain/security/ceremony.rs         → Ceremony
domain/discovery/mod.rs             → Discovery
domain/fqn/mod.rs                   → Fqn
domain/companion/mod.rs             → Companion
domain/infra/mod.rs                 → Infra
domain/presence/mod.rs              → Presence
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
    pub stone:    Arc<RwLock<Stone>>,   // permanent id; mutable name and host
    pub topology: TopologyContext,      // this node's live garden perception
    pub resources: Arc<RwLock<Option<StoneResources>>>,
    pub metrics:   CurrentMetrics,
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

### `storage` — Data Plane

Storage is what physically exists: volumes and media. Nothing else.

```rust
pub struct Storage {
    pub volumes: Volumes,
    pub media:   Media,
    pub changed: broadcast::Sender<StorageChanged>,  // domain event
}
```

| Flat field | Domain path |
|---|---|
| `volumes` | `storage.volumes` ✓ migrated |
| `media` | `storage.media` ✓ migrated |
| `storage_changed` | `storage.changed` ✓ migrated |

### `orchestration` — Coordination Plane

Orchestration coordinates operations across domains. It owns no data.

```rust
pub struct Orchestration {
    pub storage:     StorageOrchestration,
    pub nurturing:   NurturingOrchestration,
    pub nourishment: NourishmentOrchestration,
}

pub struct StorageOrchestration {
    pub tick:   broadcast::Sender<StorageTick>,
    pub agg:    broadcast::Sender<StorageTick>,
    pub nudge:  Arc<Notify>,
    pub rescan: mpsc::Sender<()>,
}

pub struct NurturingOrchestration {
    pub harvest:   Arc<HarvestStore>,
    pub store:     Arc<NurturingStore>,
}

pub struct NourishmentOrchestration {
    pub jobs: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
}
```

| Flat field | Domain path |
|---|---|
| `storage_tick` | `orchestration.storage.tick` |
| `storage_agg` | `orchestration.storage.agg` |
| `orchestration_nudge` | `orchestration.storage.nudge` (currently at `storage.orchestration.nudge` — needs correction) |
| `volume_rescan` | `orchestration.storage.rescan` (currently at `storage.orchestration.rescan` — needs correction) |
| `harvest_store` | `orchestration.nurturing.harvest` |
| `nurturing_store` | `orchestration.nurturing.store` |
| `nourishment_jobs` | `orchestration.nourishment.jobs` |

### `security` — Trust Domain

```rust
pub struct Security {
    pub pond:         Pond,
    pub stone_client: Arc<StoneClient>,
    pub https:        Arc<AtomicBool>,   // HTTPS listener started guard
}

pub struct Pond {
    pub state:    PondState,             // enrollment state
    pub active:   Arc<AtomicBool>,       // CA initialized and unlocked
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

### `fqn` — FQN Handler Registry

External processes (ollama, mongodb orchestrators) that register as handlers for
`OfferingFqn` patterns. Registrations carry a physical location (local port or remote
stone) and are used by FIND operation resolution.

Currently these entries are mixed into `GardenRegistry.gateway_entries()`. They are
extracted into a dedicated domain context.

```rust
pub struct Fqn {
    pub registry: Arc<RwLock<FqnHandlerRegistry>>,
}

pub struct FqnHandlerEntry {
    pub fqn:          OfferingFqn,
    pub handler_for:  Vec<String>,
    pub location:     FqnHandlerLocation,
    pub registered_at: DateTime<Utc>,
}

pub enum FqnHandlerLocation {
    Local  { port: u16 },
    Remote { stone_id: String, port: u16 },
}
```

The `PUT /api/v1/garden/gateway/{offering}` handler writes to `state.fqn.registry`.
FIND resolution reads from `state.fqn.registry` to locate the handler before proxying.

`GardenRegistry` (`state.registry`) retains stone discovery and tool projection entries.
Gateway entries are removed from it.

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
| `tools` | Cross-domain tool delta stream |
| `log` | Cross-domain log broadcast |
| `registry` | `GardenRegistry` for stone/tool discovery (after FQN gateway entries extracted) |
| `offerings` | Offering lifecycle spans all domains |
| `manifest_registry` | Read by offerings, storage, security, infra |
| `jobs` | Background job tracker; spans domains |
| `offerings_index` | Computed view; read-only cache |
| `subsystems` | Boot readiness flags |

---

## Migration Order

Fields are migrated in dependency order — most isolated first, most pervasive last.
Each field is one commit; the build must pass between commits.

### Phase 1 — Correct `Storage` and create `Orchestration` (in progress)

`Storage` currently contains `orchestration`, `harvest`, `nurturing`, `nourishment`
fields that belong in the new `Orchestration` domain.

1. Create `domain/orchestration/mod.rs` with all sub-structs
2. Strip `Storage` to `{ volumes, media, changed }`
3. Move call sites: `storage.orchestration.*` → `orchestration.storage.*`
4. Move call sites: `storage.harvest/nurturing/nourishment` → `orchestration.nurturing.*`
   and `orchestration.nourishment.*`
5. Add `orchestration: Arc<Orchestration>` to `AppState`

### Phase 2 — `fqn`

1. Create `domain/fqn/mod.rs` with `Fqn` and `FqnHandlerRegistry`
2. Migrate gateway registration writes → `state.fqn.registry`
3. Migrate gateway reads (FIND resolution, `gateway_entries()`) → `state.fqn.registry`
4. Remove gateway methods from `GardenRegistry`

### Phase 3 — `security`

1. Create `domain/security/mod.rs`, `pond.rs`, `ceremony.rs`
2. Migrate: `pond`, `pond_active`, `https_started`, `stone_client`, ceremony fields

### Phase 4 — `discovery`

1. Create `domain/discovery/mod.rs`
2. Migrate: `mdns_handle`, `koi_handle`

### Phase 5 — `companion`

1. Create `domain/companion/mod.rs`
2. Migrate: `companion_registry`

### Phase 6 — `infra`

1. Create `domain/infra/mod.rs`
2. Migrate: `docker`, `runtime`, `network`, `infrastructure_handlers`

### Phase 7 — `presence`

1. Create `domain/presence/mod.rs`
2. Migrate: `elections`, `notifications`

### Phase 8 — `current`

1. Create `domain/current/mod.rs`, `domain/current/topology.rs`
2. Migrate: `topology_cache`, `topology_dirty`, `self_entry` → `current.topology`
3. Migrate: `system_resources`, `network_metrics_cache`, `gpu_utilization`
4. Migrate: `stone_id`, `stone_name` → `current.stone` (most pervasive — done last)

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
- FQN resolution has a dedicated home separate from stone discovery
- The data plane (`storage.*`) is never polluted by coordination primitives
- `current` captures the complete self-model: identity and garden perception
- New code has an unambiguous home — the file path tells you where to put it
- Domain contexts are independently constructable and testable

**Negative / Trade-offs**:
- `current.stone.read().id` at every identity call site is more verbose than `stone_id`;
  this is the correct trade — explicitness over convenience
- The gateway/FQN split requires touching both `api/v1/gateway.rs` and `GardenRegistry`
- Background tasks that currently clone the full `AppState` must be audited and narrowed

## Out of Scope

- `anyhow` to typed error enum migration (ARCH-0003 pass `f`) — separate concern
- `bootstrap/run.rs` declarative pipeline (ARCH-0003 Wave 6e) — separate concern
- Orchestrator crates (`src/orchestrators/`) — standalone builds, separate migration
- `garden-common` value objects — Wave 1 of ARCH-0003; tackled separately
