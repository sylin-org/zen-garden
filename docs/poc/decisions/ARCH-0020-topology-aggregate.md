---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-11
canonical: true
completed: 2026-04-11
---

# ARCH-0020: Topology Aggregate — Book III of ARCH-0017

**Date**: 2026-04-11
**Status**: Accepted
**Book**: III of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Depends on**: [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) (epic), [ARCH-0018](ARCH-0018-metrics-aggregate.md) (`Arc<Metrics>` injection), [ARCH-0019](ARCH-0019-tool-aggregate.md) (Tool aggregate; Topology subscribes to `Tool::changes()` for self-entry refreshes)

## Context

Book III extracts the `Topology` bounded context. Today its state and logic are scattered across three locations:

1. **`src/moss/src/domain/topology.rs`** — 678 lines of free functions operating on raw `TopologyCache = Arc<RwLock<HashMap<String, TopologyEntry>>>` and `TopologyDirtyFlag = Arc<AtomicBool>` handles. 19 pub functions including `upsert_from_chirp`, `upsert_from_chirp_dirty`, `get_all_stones`, `get_online_stones`, `get_stone_by_id`, `get_stone_by_name`, `count_stones`, `count_online_stones`, `maintain_topology`, `maintain_and_persist`, `prune_stale_stones`, `mark_stone_offline`, `mark_stone_offline_dirty`, `forget_stone`, `forget_stone_dirty`, `persist_topology`, `flush_topology`.

2. **`AppState` methods** — five self-entry construction and chirping methods:
   - `build_self_entry()` — reads `current.address`, `current.health`, `current.mac`, `current.capabilities`, `presence.notifications`, `offerings` and assembles a `TopologyEntry`.
   - `sync_self_services(auto_chirp)` — rebuilds self entry and chirps if `subsystems.network.ready`.
   - `sync_self_capabilities(auto_chirp)` — same, for capability updates.
   - `update_stone_health(health, auto_chirp)` — mutates `current.health`, then chirps.
   - `announce_resolution_change(new_ip)` — mutates `current.address` and `current.mac`, re-registers `discovery.mdns`, then chirps.

3. **`current::Topology` sub-struct** — holds `pub cache: TopologyCache` and `pub dirty: TopologyDirtyFlag` as raw fields on `state.current.topology`. The sub-struct exists only to group the two handles; it has no methods.

The "chirp path" — `crate::announcement::announce(&entry)` — is a free async function in `src/moss/src/announcement.rs` that calls `garden_common::infra::communications::p2p::send_announcement` with a `STONE_CHIRP` announcement type. Nine call sites in the crate hit this function directly: four in `AppState`, two in `bootstrap/run.rs`, one in `tasks/announcer.rs`, two in `tasks/coordinator.rs`.

Per the Discovery Mandate in ARCH-0017, Chapter 1 began with a re-evaluation of Book III's plan against the current code. The findings reshape the book's scope in several material ways.

### What the re-evaluation found

1. **42 `topology::` caller sites plus 25 `current.topology.{cache,dirty}` sites plus 14 non-AppState callers of the five self-entry methods.** Total touch surface is ~80 call sites (compared with Book II's 14 API/domain/task sites + 25 infra struct-field sites, which the field-level strangler split between typed migration and infra-layer accessor retention). Book III has more surface but similar distribution: free-function callers in tasks, API handlers, and bootstrap.

2. **`current::Topology` is a hollow sub-struct.** It groups the cache and dirty flag but has no methods. The Book III extraction replaces it: `state.topology: Arc<Topology>` becomes a top-level aggregate field, and `current.topology` is deleted. Two-line mechanical rename at every access site (25 sites).

3. **`build_self_entry` has five upstream dependencies** — `current.address`, `current.health`, `current.mac`, `current.capabilities`, `presence.notifications`, `offerings`, `current.stone`. These cannot be hidden inside the aggregate because they cross domain boundaries (offerings is its own aggregate, presence is a different context, capabilities live under current). The command takes them as an explicit `SelfEntryInputs` struct, same approach Book II took for `LocalProjectionInputs` in the Tool projection path.

4. **`announce_resolution_change` touches `discovery.mdns`.** This is a cross-domain edge into the Discovery context (Book X). Book III keeps this coupling as an explicit input argument (`Option<&MdnsHandle>`) rather than absorbing mDNS into Topology — Discovery is its own book, and the mDNS re-registration is a Discovery concern that Topology triggers.

5. **Chirp path already exists as a free function.** `crate::announcement::announce` is a 40-line function that builds a `STONE_CHIRP` announcement and sends it via `p2p::send_announcement`. Wrapping this in a `ChirpTransport` port is mechanical; the adapter is a ~15-line struct.

6. **Persistence writes JSON files on disk.** `persist_topology` and `flush_topology` write `garden-topology.json` to the shared data directory (per TOPO-0002). This is a real persistence port — `TopologyStore` — not an ephemeral aggregate like Tool or Metrics. Topology has the full Store port.

7. **`upsert_from_chirp` is split in two** — one version mutates, another version does `upsert_from_chirp(); mark_dirty()`. Only the `_dirty` version is used by the UDP listener path. The bare `upsert_from_chirp` is only used by tests. After extraction, the aggregate command always marks dirty (invariant of the mutation path); the `_dirty` variant disappears.

8. **Maintenance is both a query and a mutation.** `maintain_topology` evicts stones past `OFFLINE_EVICTION_HOURS` and marks stones older than `OFFLINE_THRESHOLD_SECS` as offline. It returns `(evicted_count, marked_offline_count)`. This is a periodic task command run by a maintenance background task every 60 seconds.

9. **`StoneStatus::Online` / `StoneStatus::Offline` transitions are the interesting events** — not every `upsert_from_chirp` is a "topology changed" event; only status transitions (Online ↔ Offline) and new-stone discoveries are worth broadcasting on `TopologyChanged`. Peer refreshes of the same entry with the same status should not fire.

## Decision

Book III extracts `Topology` as a full DDD aggregate with private state (cache + dirty flag), typed commands, typed queries, `TopologyChanged` event stream, `ChirpTransport` port, `TopologyStore` port for persistence, and `Arc<Metrics>` injection. The `current::Topology` sub-struct is deleted; `state.topology: Arc<Topology>` becomes a top-level AppState field. Five AppState methods move into typed commands. The chirp path (`crate::announcement::announce`) moves behind the `ChirpTransport` port with a production adapter. `crate::announcement` itself stays as the module that hosts the adapter and the `send_goodbye` function (goodbye is a shutdown-path concern, not a topology concern).

### Module layout (target state)

```
src/moss/src/domain/topology/
├── mod.rs            — re-exports
├── aggregate.rs      — `Topology` struct, typed commands and queries
├── state.rs          — `TopologyState` (cache + dirty flag + last flush timestamp)
├── event.rs          — `TopologyChanged` enum + `ChangeKind` for metrics
├── error.rs          — `TopologyError`
├── transport.rs      — `ChirpTransport` port trait + `NoopChirpTransport` for tests
├── store.rs          — `TopologyStore` port trait + `FileTopologyStore` adapter
├── maintenance.rs    — maintenance policy (threshold constants, eviction logic)
└── tests.rs          — unit tests
```

The existing single-file `domain/topology.rs` (678 lines) splits across these files during Ch2 via pure `git mv` + content commit per code-standards §14. The infra adapter `FileTopologyStore` lives alongside the aggregate in `domain/topology/store.rs` (the same pattern ARCH-0019 used for `P2pBeaconTransport` — adapter-in-infra, port-in-domain — but since the persistence adapter is file-system based and has no external infrastructure dependency beyond `tokio::fs`, it lives with the port for cohesion).

### Aggregate API

```rust
pub struct Topology {
    state: RwLock<TopologyState>,
    chirp: Arc<dyn ChirpTransport>,
    store: Arc<dyn TopologyStore>,
    metrics: Arc<Metrics>,
    changes: broadcast::Sender<TopologyChanged>,
}

impl Topology {
    pub const NAME: &'static str = "topology";

    pub async fn new(
        chirp: Arc<dyn ChirpTransport>,
        store: Arc<dyn TopologyStore>,
        metrics: Arc<Metrics>,
    ) -> Result<Self, TopologyError> {
        metrics.register_domain(Self::NAME, ChangeKind::ALL_NAMES).await;
        let cache = store.load().await?;
        // ... seed state, fire initial events
    }

    // ── Commands ────────────────────────────────────────────────────────
    pub async fn upsert_from_chirp(&self, entry: TopologyEntry) -> Option<TopologyChanged>;
    pub async fn mark_stone_offline(&self, stone_id: &str) -> Option<TopologyChanged>;
    pub async fn forget_stone(&self, stone_name: &str) -> Option<TopologyChanged>;
    pub async fn maintain(&self) -> MaintenanceReport;

    pub async fn build_self_entry(&self, inputs: SelfEntryInputs) -> TopologyEntry;
    pub async fn sync_services(&self, inputs: SelfEntryInputs, auto_chirp: bool) -> Result<()>;
    pub async fn sync_capabilities(&self, inputs: SelfEntryInputs, auto_chirp: bool) -> Result<()>;
    pub async fn update_stone_health(&self, health: String, inputs: SelfEntryInputs, auto_chirp: bool) -> Result<()>;
    pub async fn announce_resolution_change(&self, new_ip: IpAddr, inputs: SelfEntryInputs, mdns: Option<&MdnsHandle>) -> Result<()>;

    pub async fn flush(&self) -> Result<(), TopologyError>;

    // ── Queries ─────────────────────────────────────────────────────────
    pub async fn all_stones(&self) -> Vec<TopologyEntry>;
    pub async fn online_stones(&self) -> Vec<TopologyEntry>;
    pub async fn get_by_id(&self, stone_id: &str) -> Option<TopologyEntry>;
    pub async fn get_by_name(&self, stone_name: &str) -> Option<TopologyEntry>;
    pub async fn count(&self) -> usize;
    pub async fn online_count(&self) -> usize;
    pub async fn is_dirty(&self) -> bool;

    // ── Events ──────────────────────────────────────────────────────────
    pub fn changes(&self) -> broadcast::Receiver<TopologyChanged>;
}
```

### `SelfEntryInputs`

The projection command takes an explicit struct rather than a back-reference to `AppState`:

```rust
pub struct SelfEntryInputs {
    pub stone: Stone,                                      // id + name
    pub address: PeerAddress,
    pub health: String,
    pub mac: Option<String>,
    pub capabilities: Option<HardwareCapabilities>,
    pub tags: Vec<String>,                                  // presence.notifications.compile()
    pub services: Vec<TopologyServiceEntry>,                // offerings.with_active(...)
    pub network_ready: bool,                                // subsystems.network.ready
}
```

Callers assemble this from `AppState` before invoking the command. The aggregate never touches `AppState`.

### `TopologyChanged` event

```rust
pub enum TopologyChanged {
    StoneDiscovered { stone: TopologyEntry },           // new entry, status Online
    StoneOnline     { stone_id: String, stone_name: String },  // transitioned Offline → Online
    StoneOffline    { stone_id: String, stone_name: String },  // transitioned Online → Offline
    StoneForgotten  { stone_name: String },             // explicit forget
    StoneEvicted    { stone_id: String, stone_name: String },  // maintenance eviction past TTL
    SelfEntryChirped { cursor: u64 },                    // local entry was chirped
}

pub enum ChangeKind { Discovered, Online, Offline, Forgotten, Evicted, Chirped }
```

Peer refresh upserts (same entry, same status) do NOT fire `TopologyChanged` — too high-volume for the interesting-transition stream. Maintenance queries that evict zero stones do not fire `MaintenanceReport` events either.

### `ChirpTransport` port

```rust
pub trait ChirpTransport: Send + Sync {
    fn chirp<'a>(&'a self, entry: &'a TopologyEntry) -> BoxFut<'a, Result<()>>;
}

// Adapter
pub struct P2pChirpTransport;

impl ChirpTransport for P2pChirpTransport {
    fn chirp<'a>(&'a self, entry: &'a TopologyEntry) -> BoxFut<'a, Result<()>> {
        Box::pin(async move { crate::announcement::announce(entry).await })
    }
}
```

### `TopologyStore` port

```rust
pub trait TopologyStore: Send + Sync {
    fn load(&self) -> BoxFut<'_, Result<HashMap<String, TopologyEntry>>>;
    fn save<'a>(&'a self, entries: &'a HashMap<String, TopologyEntry>, self_entry: &'a TopologyEntry) -> BoxFut<'a, Result<()>>;
}

// Adapter
pub struct FileTopologyStore;

impl TopologyStore for FileTopologyStore {
    // delegates to persist_topology / flush_topology using tokio::fs
}
```

### Metrics integration

Register domain `topology` with six kinds (`discovered`, `online`, `offline`, `forgotten`, `evicted`, `chirped`). Every command records mutation latency; every event records per-kind counter.

### What Book III does not do

- **No Discovery work.** Book X owns `discovery.mdns`, `koi_client`, peer scanning. Book III passes `Option<&MdnsHandle>` into `announce_resolution_change` as an explicit input argument rather than absorbing mDNS.
- **No Announcement restructuring.** `crate::announcement::send_goodbye` stays as-is — goodbye is a shutdown path, not a topology mutation. `announcement::announce` becomes an internal implementation detail of the `P2pChirpTransport` adapter.
- **No Subsystems work.** `subsystems.network.ready` is an `AtomicBool` on AppState that the `sync_services` / `sync_capabilities` / `update_stone_health` commands consult via the `network_ready` field on `SelfEntryInputs`. Book VI owns the full Subsystems aggregate extraction.
- **No Current refactor.** `state.current` stays as-is during Book III. Only `state.current.topology` sub-struct is deleted (promoted to top-level `state.topology`).

## Chapter plan

| Ch | Scope |
|----|-------|
| 1  | ADR (this), revision history |
| 2  | Module consolidation: pure `git mv` `domain/topology.rs` → `domain/topology/mod.rs`, content follow-up to split into submodule files |
| 3  | `Topology` aggregate: state, commands, queries, events, Metrics injection, unit tests. `ChirpTransport` port declared but not yet wired (Ch4). `TopologyStore` port with `FileTopologyStore` adapter. Five AppState methods remain alongside temporarily. |
| 4  | Wire `ChirpTransport`: replace 9 `crate::announcement::announce` call sites with `state.topology.chirp(&entry)` or equivalent typed command. Adapter construction in `bootstrap/run.rs`. |
| 5  | Delete five AppState methods; migrate 14 non-AppState caller sites to typed commands. Migrate 42 `topology::` free-function callers to typed queries. Delete `current::Topology` sub-struct; promote `state.topology` to top-level `AppState` field; migrate 25 access sites. |
| 6  | Closure: context-map, glossary, pattern-spec deviation entry if any, frontmatter, ARCH-0017 revision history, final exit-criteria grep. |

## Exit criteria

Book III is closed when:

1. `rg 'build_self_entry\|sync_self_services\|sync_self_capabilities\|update_stone_health\|announce_resolution_change' src/moss/src/app_state.rs | wc -l` = 0
2. `rg 'current\.topology\.cache\|current\.topology\.dirty' src/moss/src/ | wc -l` = 0 (sub-struct deleted)
3. `rg 'crate::announcement::announce' src/moss/src/ | wc -l` = 0 outside `src/moss/src/domain/topology/transport.rs` and the inner `announcement::announce_inner` helper
4. `rg 'pub type TopologyCache\|pub type TopologyDirtyFlag' src/moss/src/ | wc -l` = 0 (types private inside `domain/topology/state.rs`)
5. `rg 'crate::domain::topology::(upsert_from_chirp\|get_all_stones\|get_online_stones\|maintain_topology\|persist_topology\|flush_topology)' src/moss/src/ | wc -l` = 0 (free functions deleted)
6. `cargo check --all && cargo test --package garden-moss --lib && cargo clippy --package garden-moss --lib -- -D warnings`
7. Manual smoke: `garden-rake list` on a live stone shows peer stones; a stone goodbye correctly marks a peer offline; `garden-topology.json` still persists across restarts; topology chirps fire every 30s.

## Pattern deviations

Book III is a **persistent aggregate** (unlike Metrics, Resources, Tool) — it has a full `TopologyStore` port and calls `store.save` from a batched `flush()` command called periodically by the maintenance task. The dirty flag is used to skip no-op flushes.

No other deviations from the standard pattern. The five self-entry construction methods take `SelfEntryInputs` rather than a back-reference to `AppState` — this is the standard shape documented in the pattern spec (see ARCH-0019's `LocalProjectionInputs` precedent).

## Alternatives considered

### Alternative A — Absorb mDNS into Topology (rejected)

`announce_resolution_change` re-registers the mDNS service when the IP changes. Option A would move the mDNS handle into the Topology aggregate's state. Rejected: mDNS is a Discovery concept (Book X); absorbing it would pull mDNS lifecycle management into Book III and expand scope significantly.

### Alternative B — Topology owns `current.address`, `current.mac` (rejected)

`announce_resolution_change` writes `current.address` and `current.mac`. Option B would move these fields into `TopologyState`. Rejected: address and MAC are the stone's **current runtime identity** and are read by many non-Topology consumers (HTTP routing, p2p listener bindings, the `current` context itself). Keeping them in `current` and having Topology take them as an input argument preserves the correct ownership boundary.

### Alternative C — Collapse `build_self_entry` into a query (rejected)

`build_self_entry` reads state and returns a `TopologyEntry`. It has no side effects — it's technically a pure query. Option C would expose it as `Topology::build_self_entry(inputs) -> TopologyEntry` without associating it with any command. Rejected: the result is always used to either chirp or persist, and the upstream callers ALWAYS want the "build + chirp" sequence. Book III keeps `build_self_entry` as a query but the four production call sites prefer the combined `sync_services` / `sync_capabilities` / `update_stone_health` / `announce_resolution_change` commands instead.

## References

- [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) — the epic
- [ARCH-0019](ARCH-0019-tool-aggregate.md) — Tool aggregate, `LocalProjectionInputs` precedent for `SelfEntryInputs`
- [ARCH-0018](ARCH-0018-metrics-aggregate.md) — Metrics aggregate, register-with-kinds pattern reused here
- [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md) — first persistent aggregate with Store port (Topology is the second)
- [TOPO-0002](TOPO-0002-shared-topology-directory.md) — shared topology directory spec; `FileTopologyStore` adapter preserves TOPO-0002's invariants
- `docs/specs/domain-aggregates.md` — pattern spec; Book III is a straightforward application with no new deviations
