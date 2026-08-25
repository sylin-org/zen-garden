---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-11
canonical: true
completed: 2026-04-11
---

# ARCH-0019: Tool Aggregate — Book II of ARCH-0017

**Date**: 2026-04-11
**Status**: Accepted
**Book**: II of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Depends on**: [ARCH-0004](ARCH-0004-appstate-domain-context-extraction.md), [ARCH-0015](ARCH-0015-task-supervisor-registry.md), [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md) (`Offerings::changes()` event subscription), [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) (epic and pattern spec), [ARCH-0018](ARCH-0018-metrics-aggregate.md) (`Arc<Metrics>` injection from day one), [TOOLS-0003](TOOLS-0003-unified-garden-registry.md) (`GardenRegistry` as the unified write-through cache)

## Context

Book II extracts the `Tool` bounded context from its current hybrid form — partly in `domain/tool/` (aggregate shell with a `pub` registry field), partly in `domain/tools/` (projector, capability orchestrator, events), partly in a free-standing `domain/garden_registry.rs` (the real state, 1085 lines of rich query API), partly on `AppState` (four methods for projection/publish/ingest/remove), and partly in background tasks that hold `Arc<RwLock<GardenRegistryInner>>` handles directly (beacon ingress, goodbye removal, TTL reaping).

Per the Discovery Mandate in ARCH-0017, Chapter 1 began with a re-evaluation of Book II's plan against the current code. The original plan assumed a straightforward three-method migration off `AppState` (`refresh_local_tools_projection`, `publish_tool_deltas`, plus the dead-code siblings `ingest_tools_beacon` and `remove_tools_for_stone`) and a `ToolProjectionTask` subscribing to `Offerings::changes()` and a non-existent Storage changed stream. The re-evaluation surfaced nine material findings that reshape the book. They were confirmed with the user and logged in ARCH-0017's revision history before this ADR was written. This ADR reflects the revised plan.

### What the re-evaluation found

1. **Two modules, both active.** `src/moss/src/domain/tool/mod.rs` (41 lines) defines the aggregate shell with `pub registry: GardenRegistry` and `pub(crate) delta: broadcast::Sender<ToolDelta>`. `src/moss/src/domain/tools/` (plural, ~395 lines across `projector.rs`, `capability_orchestrator.rs`, `events.rs`) holds the projection and event plumbing. `src/moss/src/domain/garden_registry.rs` (1085 lines) holds `GardenRegistryInner` — the real state with the rich query API (`ToolQuery`, `snapshot`, `deltas_since`, `upsert_with_expiry`, `reap_expired_gateways`, `apply_remote_beacon`, `remove_stone`, storage routing helpers). Three locations, one concept, no clear ownership boundary. Code-standards §14 (one concept per file) is violated.

2. **`pub GardenRegistry` is the exact anti-pattern ARCH-0016 fixed.** `Tool::registry` is a `pub Arc<RwLock<GardenRegistryInner>>`. Fifty call sites across twenty-plus files reach into it via `state.tool.registry.read().await` / `.write().await`. The `Tool` struct today is a struct of public handles, not an aggregate.

3. **Two of the four planned AppState methods are dead code.** `grep -r 'ingest_tools_beacon\|remove_tools_for_stone' src/` finds only the two definitions in `src/moss/src/app_state.rs`, no callers. The real beacon ingress is in `src/moss/src/tasks/discovery.rs:335` (`TOOLS_BEACON` match arm → `reg.apply_remote_beacon(&beacon)`) and the real stone-removal is in `src/moss/src/tasks/discovery.rs:281` (`STONE_GOODBYE` match arm → `reg.remove_stone(&goodbye.stone_id)`). Both hold a `registry` handle cloned from `AppState` at task startup and write directly, bypassing any would-be aggregate gateway.

4. **Fifty direct `tool.registry.read/write` sites.** `s3_gateway.rs` alone has twelve. `garden_storage/{files,objects,snapshots,mod}.rs` have another fifteen. Plus `portrait.rs`, `webdav.rs`, `storage.rs` (API), `service_discovery.rs`, `announcer.rs`, `offering_orchestration.rs`, `state_provider.rs`, `storage_orchestration.rs`, `storage_replication.rs`, `tools.rs`, `app_state.rs`, `gateway.rs`, `bootstrap/run.rs`, `tasks/registry.rs`, `tasks/discovery.rs`, `tasks/coordinator.rs`. The reads are mostly routing and discovery queries against `GardenRegistryInner` helpers; the writes are gateway register/deregister (API), TTL reaping (task), local projection reconcile (AppState), remote beacon ingest (task), stone goodbye removal (task).

5. **Projector is deeply coupled to `AppState`.** `project_local_tools(state: &AppState)` reaches into five `AppState` fields (`offerings`, `current.storage.volumes`, `manifest_registry`, `current.address`, `current.stone`) and calls `connection::resolve_connection` for URI composition. Before the aggregate can own projection, those dependencies become explicit arguments to a typed command method or injected readers — the aggregate cannot hold a back-reference to `AppState` without defeating the boundary.

6. **Storage does not emit events today.** The original Book II plan called for `ToolProjectionTask` to subscribe to `Offerings::changes()` *and* a Storage changed stream. Only the first exists. The Storage aggregate is Book VIII. Every storage-mutation site currently triggers a tool refresh imperatively (`app_state.rs:660` is the canonical example: after `current.storage.update_volume(...)`, the code calls `self.refresh_local_tools_projection()`). Book II cannot predicate its projection task on events that Book VIII will emit.

7. **`ToolsBeaconTransport` port is straightforward.** `infra::broadcast_tools_beacon` is a free async function called directly from `AppState::publish_tool_deltas`. Wrapping it in a typed port is mechanical: `infra/tools/beacon.rs` becomes the adapter.

8. **No Store port — ephemeral by design.** The tool registry is rebuilt on startup from offerings + storage + remote beacons. Gateway entries expire by TTL. No persistence. This matches the Metrics pattern deviation. With Tool as the third consecutive ephemeral aggregate (Metrics, Resources, Tool), the pattern spec should codify "ephemeral aggregates" as a documented deviation rather than a per-ADR footnote.

9. **URL surface is already user-facing; add one missing singular.** `/api/v1/stone/tools` (read), `/api/v1/garden/tools` (aggregate), `/api/v1/stone/tools/stream` (SSE), `/api/v1/garden/tools/stream` (SSE) already exist and are already consumer-facing. No rename needed. One missing endpoint: `/api/v1/stone/tools/{fqid}` for single-tool lookup — there is no single-tool endpoint today. Book II adds it as a free win while the read surface is being rebuilt on the aggregate.

### What Book II does not do

- **No Storage aggregate work.** Book VIII owns that. The imperative edges from storage mutation sites to `state.tool.refresh_local()` are honest couplings, not scaffolds — they document the prerequisite.
- **No Topology consolidation.** Book III owns that. `build_self_entry` / `sync_self_services` / `sync_self_capabilities` / `update_stone_health` / `announce_resolution_change` stay on `AppState` until Book III.
- **No garden-wide registry consolidation.** Book II cleans up the single-stone `Tool` aggregate. Cross-stone federation (beacon ingestion, goodbye reconciliation) happens inside `Tool::apply_remote_beacon` / `Tool::remove_stone` — a typed command, not a raw write — but the fundamental one-registry-per-stone shape stays.
- **No Prometheus exporter.** Out of scope (same as Book I — deferred to a post-epic adapter).

## Decision

Book II extracts `Tool` as a proper bounded context with private state, typed reads, typed commands, a `ToolChanged` broadcast stream, an injected `ToolsBeaconTransport` port, `Arc<Metrics>` injection, and a `ToolProjectionTask` subscribing to `Offerings::changes()`. Two predecessor modules (`domain/tool/`, `domain/tools/`) and one stand-alone file (`domain/garden_registry.rs`) collapse into a single singular `domain/tool/` module laid out per code-standards §14. Fifty direct registry access sites migrate through an `ActiveGuard` strangler that retires inside Book II — no scaffold crosses into Book III. Two dead-code methods on `AppState` are deleted. The real beacon-ingress and goodbye paths in `tasks/discovery.rs` are migrated to typed aggregate commands.

### Module layout (target state)

```
src/moss/src/domain/tool/
├── mod.rs            — re-exports, public API surface
├── aggregate.rs      — `Tool` struct, typed commands and queries
├── state.rs          — `ToolState` wrapping the registry inner
├── registry.rs       — `RegistryInner` (former `garden_registry.rs`), ToolQuery, EntryOrigin, RegistryEntry
├── event.rs          — `ToolChanged` (wraps `ToolDelta`), `ChangeKind`
├── projection.rs     — `project_local_tools` (former `tools/projector.rs`), takes explicit arguments
├── capability.rs     — `record_capability_added/removed` (former `tools/capability_orchestrator.rs`)
├── transport.rs      — `ToolsBeaconTransport` port trait
├── guard.rs          — `ActiveGuard` strangler (retired at Ch6)
├── error.rs          — `ToolError` enum
└── tests.rs          — unit tests
```

`domain/tools/` (plural) is deleted at Ch6. `domain/garden_registry.rs` is deleted at Ch2 (`git mv` into `domain/tool/registry.rs`).

### Aggregate API

```rust
pub struct Tool {
    state: RwLock<ToolState>,
    beacon: Arc<dyn ToolsBeaconTransport>,
    metrics: Arc<Metrics>,
    changes: broadcast::Sender<ToolChanged>,
}

impl Tool {
    pub const NAME: &'static str = "tool";

    pub async fn new(
        beacon: Arc<dyn ToolsBeaconTransport>,
        metrics: Arc<Metrics>,
    ) -> Self {
        metrics.register_domain(Self::NAME, ChangeKind::ALL_NAMES).await;
        // ...
    }

    // ── Commands (write) ──────────────────────────────────────────────
    pub async fn refresh_local(&self, ctx: LocalProjectionInputs) -> Vec<ToolChanged>;
    pub async fn register_gateway(&self, tool: GardenTool, ttl: Duration) -> Option<ToolChanged>;
    pub async fn deregister_gateway(&self, offering: &str, stone_id: &str) -> Option<ToolChanged>;
    pub async fn reap_expired_gateways(&self) -> Vec<ToolChanged>;
    pub async fn apply_remote_beacon(&self, beacon: &ToolsBeacon) -> Vec<ToolChanged>;
    pub async fn remove_stone(&self, stone_id: &str) -> Vec<ToolChanged>;
    pub async fn record_capability_added(&self, offering: &str, cap_type: &str, cap: &str) -> Result<(), ToolError>;
    pub async fn record_capability_removed(&self, offering: &str, cap_type: &str, cap: &str) -> Result<(), ToolError>;

    // ── Queries (read) ────────────────────────────────────────────────
    pub async fn snapshot(&self, query: &ToolQuery) -> (u64, Vec<GardenTool>);
    pub async fn deltas_since(&self, cursor: u64, query: &ToolQuery) -> Vec<ToolDelta>;
    pub async fn get(&self, key: &str) -> Option<GardenTool>;
    pub async fn current_cursor(&self) -> u64;
    pub async fn storage_by_name(&self, name: &str) -> Vec<RegistryEntry>;
    pub async fn storage_primary(&self, name: &str) -> Option<RegistryEntry>;
    pub async fn storage_by_id(&self, id: &str) -> Option<RegistryEntry>;
    pub async fn storage_grouped_by_stone(&self) -> BTreeMap<String, Vec<RegistryEntry>>;
    pub async fn storage_stone_count(&self) -> usize;
    pub async fn storage_count(&self) -> usize;
    pub async fn route_to_primary(&self, name: &str, stone_id: &str) -> RouteDecision;
    pub async fn find_s3_gateways(&self) -> Vec<RegistryEntry>;
    pub async fn stone_endpoint(&self, stone_id: &str) -> Option<String>;
    pub async fn handles_offering(&self, offering: &str) -> bool;
    pub async fn handled_offerings(&self) -> HashSet<String>;

    // ── Events ────────────────────────────────────────────────────────
    pub fn changes(&self) -> broadcast::Receiver<ToolChanged>;
    pub fn delta_stream(&self) -> broadcast::Receiver<ToolDelta>;  // existing, preserved
}
```

All queries return owned values (clones), not borrowed references into the inner state — the old `&RegistryEntry` return shape is incompatible with `RwLock<ToolState>` and forced the current `pub registry` shortcut. Clone cost is negligible against the lock-acquire cost for these infrequent queries; hot-path callers get the dedicated typed method (`route_to_primary`, `storage_primary`, etc.) rather than iterating.

### LocalProjectionInputs

The projection command takes an explicit input struct rather than a back-reference to `AppState`:

```rust
pub struct LocalProjectionInputs {
    pub stone: Stone,                         // id, name, endpoint
    pub offerings: Vec<Offering>,             // snapshot read from Offerings aggregate
    pub managed_volumes: Vec<Volume>,         // snapshot read from Storage domain
    pub manifest_registry: Arc<ManifestRegistry>,  // for connection templates
}
```

`ToolProjectionTask` assembles these from `AppState` before calling `tool.refresh_local(inputs).await`. The aggregate never touches `AppState`.

### `ToolChanged` vs `ToolDelta`

`ToolDelta` is the existing wire format (serde on GardenTool + ToolDeltaKind), consumed by SSE subscribers and the UDP `ToolsBeacon`. It stays. `ToolChanged` is the domain event — a typed enum over delta + metadata (origin, cursor, timestamp) — consumed by projection tasks, metrics, and internal subscribers. `Tool::delta_stream()` (the existing SSE contract) is preserved verbatim; `Tool::changes()` is the new internal subscriber API. Both are fed from the same command gateway.

```rust
pub enum ToolChanged {
    Upserted { entry: RegistryEntry, origin: EntryOrigin, cursor: u64 },
    Removed  { key: String, fqid: String, stone_id: String, cursor: u64 },
    Reaped   { count: usize, cursor: u64 },  // batch TTL reap
}

impl ToolChanged {
    pub fn kind(&self) -> ChangeKind;  // for Metrics
    pub fn to_delta(&self) -> Option<ToolDelta>;  // for wire format
}

pub enum ChangeKind { Upserted, Removed, Reaped, BeaconApplied, StoneRemoved }
impl ChangeKind { pub const ALL_NAMES: &[&str] = &["upserted", "removed", ...]; }
```

### `ToolsBeaconTransport` port

```rust
#[async_trait]  // via Pin<Box<Future>> per code-standards §ARCH-0007
pub trait ToolsBeaconTransport: Send + Sync {
    async fn broadcast_incremental(&self, beacon: ToolsBeacon) -> Result<(), BeaconError>;
    async fn broadcast_snapshot(&self, beacon: ToolsBeacon) -> Result<(), BeaconError>;
}
```

Adapter: `src/moss/src/infra/tools/beacon.rs` already has `broadcast_tools_beacon` and `broadcast_tools_snapshot_beacon` as free functions. Ch4 wraps them in a `P2pBeaconTransport` struct implementing the trait.

### Strangler guard (retired inside Book II)

The 50 `tool.registry.read/write` sites compile unchanged after Ch3 via:

```rust
impl Tool {
    pub async fn active_read(&self) -> ActiveGuard<'_>;
    pub async fn active_write(&self) -> ActiveGuardMut<'_>;
}
```

`ActiveGuard` derefs to `&RegistryInner` so existing call sites work with a single-word substitution (`state.tool.registry.read().await` → `state.tool.active_read().await`). Ch6 migrates every call site to a typed method and deletes the guard. Exit criterion: `rg 'active_read\|active_write' src/moss/src/ == 0` outside `domain/tool/tests.rs`.

The guard does not cross into Book III. This is a deliberate contrast with ARCH-0016's `ActiveGuard`, which still lingers under "Active scaffolds" in `docs/scaffolding.md` (removal trigger: Book XVIII). Book II owns its full migration.

### Projection task

`ToolProjectionTask` (in `src/moss/src/tasks/task_defs/`):

```rust
impl BackgroundTask for ToolProjectionTask {
    async fn run(mut self, ctx: TaskContext) -> Result<()> {
        let mut offerings_feed = ctx.state.offerings.changes();
        // seed-on-boot
        self.reproject(&ctx.state).await;
        loop {
            tokio::select! {
                event = offerings_feed.recv() => match event {
                    Ok(_)                          => self.reproject(&ctx.state).await,
                    Err(RecvError::Lagged(n))      => {
                        ctx.state.current.metrics
                            .record_subscriber_lag("tool-projection", n).await;
                        self.reproject(&ctx.state).await;  // force reconcile on lag
                    }
                    Err(RecvError::Closed)         => break,
                }
                _ = ctx.cancellation.cancelled() => break,
            }
        }
        Ok(())
    }
}
```

The imperative edges from storage mutation sites (`state.tool.refresh_local()` via the aggregate gateway) stay as explicit couplings. Book VIII flips them to event subscription.

### Metrics integration

Register domain `tool` with kinds `["upserted", "removed", "reaped", "beacon-applied", "stone-removed"]` at `Tool::new`. Every command records mutation latency + event through the `finalize()` pattern established in ARCH-0018.

### Dead code deletion

`AppState::ingest_tools_beacon` and `AppState::remove_tools_for_stone` are deleted outright in Ch5. The real callers in `tasks/discovery.rs` migrate to `state.tool.apply_remote_beacon(&beacon).await` and `state.tool.remove_stone(&goodbye.stone_id).await` — typed commands, no direct registry handle.

### Singular tool lookup endpoint

New handler `GET /api/v1/stone/tools/{fqid}` returning `Option<GardenTool>` or 404. Added at Ch6 alongside the read surface cleanup. Manifest entry plus `StoneApi::tools().get(&fqid)` client method.

### Ephemeral aggregates as a documented pattern deviation

Book II adds a "Ephemeral aggregates" section to `docs/specs/domain-aggregates.md` codifying the deviation established by Metrics and Resources: when an aggregate is rebuilt on startup from other domains and has no persistence needs, omit the Store port. Metrics, Resources, and Tool all fit. Future aggregates either fit or justify divergence.

## Chapter plan

| Ch | Scope | Commit shape |
|----|-------|--------------|
| 1  | ADR (this), revision history entry in ARCH-0017, plan deltas surfaced to user | 1 commit (docs-only) |
| 2  | Module consolidation: `git mv` of `domain/garden_registry.rs` and `domain/tools/` into `domain/tool/`, pure rename commits per code-standards §14 | 2 commits (pure rename + follow-up content edits) |
| 3  | `Tool` aggregate — private state, typed commands, typed queries, `ToolChanged` broadcast, `ActiveGuard` strangler, `Arc<Metrics>` injection, unit tests. 50 call sites compile via guard. | 1 commit |
| 4  | `ToolsBeaconTransport` port + adapter; `Tool` owns publish path; `AppState::publish_tool_deltas` becomes a thin forwarder kept for Ch5 migration | 1 commit |
| 5  | `ToolProjectionTask`; delete `refresh_local_tools_projection` / `publish_tool_deltas` from `AppState`; delete dead `ingest_tools_beacon` / `remove_tools_for_stone`; migrate beacon ingress in `tasks/discovery.rs` to typed commands; migrate gateway handlers, reaper, coordinator to typed commands | 1 commit |
| 6  | Migrate all 50 `tool.registry.read/write` sites to typed methods; delete `ActiveGuard`; delete `domain/tools/` plural module; add `GET /api/v1/stone/tools/{fqid}`; update pattern spec with "Ephemeral aggregates" deviation; context-map + glossary + ARCH-0019 frontmatter + ARCH-0017 revision history | 1 commit (docs + code) |

Total: ~7 commits. Every chapter lands green on `dev`.

## Exit criteria

Book II is closed when every line below is true:

1. `rg 'refresh_local_tools_projection' src/moss/src/ | wc -l` = 0
2. `rg 'ingest_tools_beacon\|remove_tools_for_stone' src/moss/src/ | wc -l` = 0
3. `rg 'publish_tool_deltas' src/moss/src/ | wc -l` = 0
4. `rg 'state\.tool\.registry\.(read\|write)\|self\.tool\.registry\.(read\|write)' src/moss/src/ | wc -l` = 0 outside `src/moss/src/domain/tool/`
5. `rg 'active_read\|active_write' src/moss/src/ | wc -l` = 0 outside `src/moss/src/domain/tool/tests.rs`
6. `rg 'crate::domain::garden_registry\|domain::tools::' src/moss/src/ | wc -l` = 0
7. `rg 'broadcast_tools_beacon\|broadcast_tools_snapshot_beacon' src/moss/src/ | wc -l` = 0 outside `src/moss/src/infra/tools/` and `src/moss/src/domain/tool/transport.rs`
8. `cargo check --all && cargo test --package garden-moss && cargo clippy -- -D warnings` on the final chapter commit
9. `scripts/check-scaffolding.sh` green
10. Manual smoke: `garden-rake list` on a live stone shows local offerings, local seed-banks, remote-announced tools, and gateway entries; gateway registration via PUT still works; TTL reaping still fires; `STONE_GOODBYE` still clears entries for the departed stone.

## Pattern deviations (documented here, promoted to pattern spec at Ch6)

1. **No Store port** — tool registry is ephemeral, rebuilt from offerings + storage + remote beacons + TTL. Persistence would duplicate state and introduce recovery invariants the aggregate does not need.

2. **Dual event streams** — `Tool::changes()` (domain `ToolChanged`) and `Tool::delta_stream()` (wire `ToolDelta`) exist side by side. `ToolDelta` is the existing SSE and UDP beacon contract consumed by rake, garden dashboards, and peer stones; it cannot be removed without a coordinated wire-format migration. Both streams are fed from the same command gateway — every command emits the internal event and publishes the wire delta atomically.

3. **Queries return owned values, not references** — the query surface returns `Vec<GardenTool>`, `Option<RegistryEntry>`, `BTreeMap<...>` etc. rather than borrowed references. The `RwLock<ToolState>` private state cannot hand out references past the guard lifetime, and the existing `pub registry` shortcut that did was precisely the leak this book closes.

## Consequences

### Positive

- `Tool` joins `Offerings` and `Metrics` as a proper bounded context. The house style from ARCH-0016 now covers three aggregates uniformly.
- Dead code eliminated: two unused `AppState` methods, one redundant plural module directory, fifty scattered `.read/.write` access patterns.
- Gateway registration, TTL reaping, beacon ingestion, stone goodbye, local projection — five write paths that previously held raw registry handles — all flow through one typed command gateway. Future bugs of the `promote_adopted`-bypass-the-gateway class (ARCH-0016 motivation) cannot happen in `Tool`.
- Storage routing helpers (`storage_by_name`, `storage_primary`, `route_to_primary`, `find_s3_gateways`) become first-class query methods on the aggregate, compile-checked, with clear names. S3 gateway code reads cleaner.
- Every command is observable: latency histogram per command type, domain event counters per change kind, subscriber lag tracked in the projection task, all via the `Arc<Metrics>` injected at construction time.
- The projector's explicit `LocalProjectionInputs` breaks its `AppState` back-reference. Testing the projector becomes straightforward — no `AppState` mock needed.

### Negative

- Queries now clone. Every `tool.storage_by_name(...)` that currently returns `&[&RegistryEntry]` now returns `Vec<RegistryEntry>`. Measured against lock-acquire cost, clone cost is noise (sub-microsecond on a 2-field struct), but hot paths that iterate S3 gateway discovery every request will allocate where they previously didn't. Mitigation: `find_s3_gateways` and `route_to_primary` stay as typed methods that return already-filtered results rather than raw iterators, so the allocation count is bounded.
- Ch2 and Ch6 have the highest test surface. Ch2 is pure mechanical moves (rename commits) so the compile is the test. Ch6 is the 50-site migration — the coverage here is the exit-criterion grep count plus the smoke test.
- Book VIII will have to flip the imperative storage → tool refresh edges to event subscription when Storage becomes an aggregate. That's a deliberate deferral, not a regression.

### Neutral

- `ToolDelta` wire format unchanged. SSE consumers, UDP beacon receivers, and rake all continue to work without recompilation.
- `GardenRegistryInner`'s query implementation is preserved inside `domain/tool/registry.rs` — only the call sites and the ownership model change.

## Alternatives considered

### Alternative A — Cross-book strangler (rejected)

Book II could leave `ActiveGuard` in place and defer the 50-site migration to Book III or later, the way ARCH-0016's `ActiveGuard` is still active for offerings. This was rejected: the offerings guard spans 82 sites across domains (service APIs, discovery, announcement, health, election) that are touched by multiple subsequent books; a single-book retirement would entangle Book II with work it doesn't own. For Tool, the 50 sites are all queries against registry helpers — they belong inside Book II's scope and the user confirmed the preference for full retirement inside the book.

### Alternative B — Keep `domain/tools/` plural as the module (rejected)

Plural would match file naming (`tools.rs` handler, `tools/` directory). Singular matches aggregate naming (`Tool` struct, `ToolChanged` event, `ToolQuery`, `GardenTool`) and code-standards §3 (type names name the concept, not the architectural role). The ARCH-0017 book list already uses "Tool" singular. Singular wins.

### Alternative C — Absorb `GardenRegistry` into `domain/tool/state.rs` inline (rejected)

The 1085-line inner would balloon `state.rs` to ~1400 lines with the aggregate wrapper. Splitting to `registry.rs` keeps `state.rs` focused on the aggregate's ownership shape and leaves registry internals in their own file with a clear name.

### Alternative D — Event-first projection task, flip storage edges in Book II (rejected)

Book II could emit a minimal `StorageChanged` event from Storage mutation sites and have `ToolProjectionTask` subscribe. That's a one-chapter intrusion into Book VIII's scope. Rejected: Book II keeps the imperative storage → tool edge explicit and honest. Book VIII owns the full Storage aggregate and will flip that edge as part of its own work.

### Alternative E — Collapse `Tool::changes()` and `Tool::delta_stream()` into one (rejected)

A single stream carrying both internal and wire events would simplify the aggregate. Rejected: `ToolDelta` is a consumer-facing wire format; `ToolChanged` is a richer domain type with fields that would never survive serialization (raw cursor, origin enum, batch-reap count). Keeping them separate is a documented deviation, not an accident.

## References

- [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) — the epic, revision history, book list
- [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md) — first aggregate, strangler-vine pattern precedent
- [ARCH-0018](ARCH-0018-metrics-aggregate.md) — Metrics aggregate, first ephemeral deviation
- [TOOLS-0003](TOOLS-0003-unified-garden-registry.md) — `GardenRegistry` as the unified cache (this book migrates ownership of that cache into the `Tool` aggregate; TOOLS-0003's invariants are preserved)
- `docs/specs/domain-aggregates.md` — pattern spec; Ch6 adds "Ephemeral aggregates" section
- `docs/reference/context-map.md` — Tool entry moves from "Partial" to "Full" at Ch6
- `docs/glossary.md` — Ch6 adds "Ephemeral aggregate", "Wire vs domain event" if not already present
