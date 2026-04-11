---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-11
canonical: true
---

# ARCH-0018: Metrics Aggregate — Book I of ARCH-0017

**Date**: 2026-04-11
**Status**: Accepted
**Book**: I of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Depends on**: [ARCH-0004](ARCH-0004-appstate-domain-context-extraction.md) (domain context extraction), [ARCH-0015](ARCH-0015-task-supervisor-registry.md) (`BackgroundTask` / `SupervisorHandle`), [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md) (first aggregate, retrofitted by this book), [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) (the epic and its pattern spec)

## Context

Book I is the first concrete application of the domain aggregate pattern codified in [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) and [domain-aggregates.md](../specs/domain-aggregates.md). It introduces the `Metrics` bounded context — the stone's self-observation surface — and retrofits [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md)'s `Offerings` aggregate to use it.

Per the Discovery Mandate in ARCH-0017, Chapter 1 began with a re-evaluation of the plan against the current code, which surfaced several material changes to Book I's scope. Those changes were confirmed with the user and logged in ARCH-0017's revision history before this ADR was written. This ADR reflects the revised plan.

### What the re-evaluation found

1. **Name collision.** The string `metrics` in moss today refers to hardware resources, not observability:
   - `GET /api/v1/stone/metrics` returns `MetricsSnapshot { cpu, memory, disk, network, uptime_seconds }` — a hardware resource snapshot.
   - `garden_common::MetricsSnapshot` is the shared type.
   - `src/moss/src/domain/metrics_collection.rs` contains hardware resource normalization (`StoneMetrics`) used for placement scoring.
   - `StoneInfoApi::metrics()` in the typed client hits the hardware endpoint.
   - The self-describing API manifest entry for `/metrics` claims it returns Prometheus text format; the code returns JSON. The docstring on the typed client also says Prometheus. Both are false.

   Every canonical reference uses "metrics" to mean "hardware resource snapshot." Building a `Metrics` aggregate for observability in this context would collide with all of it.

2. **`SupervisorHandle` already owns task lifecycle state.** ARCH-0015's `SupervisorHandle` exposes `TaskStatus { name, state, ready, waiting_on }` and `SupervisorStatus { total, running, completed, failed, tasks }`. It tracks `Waiting` / `Running` / `Completed` / `Cancelled` / `Failed { error }` transitions. What it does *not* track: timing (`started_at`, `ready_at`), event counts (`events_received`, `events_lagged`), mutation latencies. A Metrics aggregate that duplicated task state would violate ARCH-0015's clean scope.

3. **No scattered production counters.** The original Book I plan assumed ad-hoc `AtomicU64` counters would need to be replaced. In production code, there are none — all atomics in moss are readiness flags (`AtomicBool` for network-ready, docker-ready, pond-active), which are Book VI (Subsystems) territory, not Metrics.

4. **Rake doesn't hit `/metrics` by string.** Rake uses the typed `StoneInfoApi::metrics()` method. Renaming the URL path is a clean internal refactor that propagates through compile errors.

5. **Orchestrators have their own `/metrics`.** The AI orchestrator crate registers its own `/metrics` route at `src/orchestrators/ai/src/http/router.rs:17`. It is a separate process in a separate crate, out of ARCH-0017 scope.

### The user-committed URL surface

After discussion, the four-endpoint design below was adopted. The reasoning is documented inline in ARCH-0017's revision history and summarized here:

| Path | Question it answers | Source |
|------|--------------------|--------|
| `/api/v1/stone/capabilities` | "What hardware do you have?" (static) | Existing, unchanged |
| `/api/v1/stone/resources` | "What are you using right now?" (dynamic hardware) | **Renamed** from `/metrics` |
| `/api/v1/stone/tasks` | "What are your background tasks doing?" (lifecycle state) | Existing `SupervisorHandle`, unchanged |
| `/api/v1/stone/metrics` | "How is your software behaving?" (observability counters, timings, event flow) | **New** — this book's deliverable |

Sub-paths inside `/metrics` provide slices and live updates:

```
GET /api/v1/stone/metrics                 # full snapshot { global, domains, tasks }
GET /api/v1/stone/metrics/global          # process-wide counters (uptime, total events)
GET /api/v1/stone/metrics/domains         # all domain observability
GET /api/v1/stone/metrics/domains/{name}  # one domain (e.g. offerings)
GET /api/v1/stone/metrics/tasks           # all task observability (timings, event counts)
GET /api/v1/stone/metrics/tasks/{name}    # one task
GET /api/v1/stone/metrics/stream          # SSE of MetricsChanged events (transitions only)
```

Plus one minor improvement adjacent to this book:

```
GET /api/v1/stone/tasks/{name}            # NEW — singular task status lookup
                                          # (complements existing plural /tasks)
```

## Decision

Introduce a `Metrics` bounded context at `src/moss/src/domain/metrics/` that conforms to the [domain aggregate pattern spec](../specs/domain-aggregates.md) with one documented exception: **no persistence port.** Metrics is in-memory only; counters reset on restart. This is standard Prometheus-style behavior (Prometheus counters also reset on process restart) and the aggregate's pattern checklist explicitly allows contexts to omit the `Store` port when persistence would not serve the domain.

Before introducing the Metrics aggregate, **rename the existing hardware-resource surface from "metrics" to "resources"** as a coordinated single-chapter refactor. This frees the ubiquitous-language term "metrics" for its proper meaning.

### The Metrics aggregate

```rust
// src/moss/src/domain/metrics/aggregate.rs

pub struct Metrics {
    state:   RwLock<MetricsState>,
    changes: broadcast::Sender<MetricsChanged>,
}
```

Only two internal fields: the state behind a read/write lock, and the event channel. No persistence port (in-memory only). No cross-cutting metrics injection (Metrics *is* the metrics context — it doesn't meter itself; it observes others).

### The state

```rust
pub(super) struct MetricsState {
    global:  GlobalMetrics,
    domains: HashMap<&'static str, Arc<DomainMetrics>>,
    tasks:   HashMap<&'static str, Arc<TaskMetrics>>,
}
```

`HashMap` keys are `&'static str` because context and task names are compile-time constants (`Offerings::NAME`, the `BackgroundTask::name()` return). Values are `Arc<DomainMetrics>` / `Arc<TaskMetrics>` so the hot path can grab a reference *inside* a read lock, drop the lock, and increment atomics on the referenced struct without further synchronization.

### `DomainMetrics` — lock-free hot path

```rust
pub struct DomainMetrics {
    /// Cumulative count of mutation events emitted by this domain,
    /// across all ChangeKind variants.
    pub events_total: AtomicU64,

    /// Per-kind event counts. Keyed by the kind's stable &'static str
    /// name (e.g. "promoted", "demoted", "upserted").
    pub events_by_kind: DashMap<&'static str, AtomicU64>,

    /// Timestamp (milliseconds since epoch) of the most recent event.
    /// Zero if no events have been recorded.
    pub last_event_at_ms: AtomicI64,

    /// Mutation latency histogram (count + bucket counts).
    /// Bucket boundaries: 1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s.
    pub mutation_latency_ms: LatencyHistogram,

    /// Count of subscriber-lag events observed on this domain's
    /// broadcast channel. Incremented by projection tasks that see
    /// RecvError::Lagged.
    pub subscribers_lagged_total: AtomicU64,
}
```

`LatencyHistogram` is a small struct with fixed buckets as `AtomicU64` fields plus a `total_count: AtomicU64` and `total_ms: AtomicU64`. It is lock-free and Prometheus-histogram-compatible. Detailed shape in Chapter 3.

### `TaskMetrics`

```rust
pub struct TaskMetrics {
    /// Wall clock when the task was spawned.
    pub started_at: DateTime<Utc>,

    /// Wall clock when the task called `ctx.ready.signal()`. None
    /// until the signal fires.
    pub ready_at: AtomicI64,  // milliseconds since epoch; 0 = unset

    /// Cumulative broadcast events received by this task (if it is
    /// a projection subscriber).
    pub events_received_total: AtomicU64,

    /// Cumulative lag events (RecvError::Lagged) observed by this task.
    pub events_lagged_total: AtomicU64,

    /// Timestamp of the most recent event received.
    pub last_event_at_ms: AtomicI64,
}
```

### `GlobalMetrics`

```rust
pub struct GlobalMetrics {
    /// Process start time (set at aggregate construction).
    pub started_at: DateTime<Utc>,

    /// Sum of all domain events across all contexts.
    pub events_total: AtomicU64,

    /// Sum of subscriber lag events across all tasks.
    pub lag_total: AtomicU64,
}
```

### Mutation API (no `Result` — mutations are infallible)

```rust
impl Metrics {
    /// Register a domain. Called once at bootstrap by the aggregate
    /// that owns the context. Subsequent registrations of the same
    /// name are no-ops. Not an error.
    pub async fn register_domain(&self, name: &'static str);

    /// Register a task. Called by the task supervisor when a task
    /// is spawned.
    pub async fn register_task(&self, name: &'static str);

    /// Record a domain mutation event. Hot-path: takes a read lock,
    /// looks up the domain's Arc<DomainMetrics>, increments atomics,
    /// releases the lock. No write lock. No allocation on the hot path.
    pub async fn record_domain_event(
        &self,
        domain: &'static str,
        kind: &'static str,
    );

    /// Record a mutation latency observation.
    pub async fn record_mutation_latency(
        &self,
        domain: &'static str,
        elapsed: Duration,
    );

    /// Record a task ready transition. Sets `ready_at` on the task
    /// metrics and publishes a MetricsChanged::TaskReady event.
    pub async fn record_task_ready(&self, task: &'static str);

    /// Record a task state transition. Publishes a
    /// MetricsChanged::TaskStateChanged event.
    pub async fn record_task_transition(
        &self,
        task: &'static str,
        state: &'static str,
    );

    /// Record that a projection task saw RecvError::Lagged.
    /// Publishes a MetricsChanged::SubscriberLag event.
    pub async fn record_subscriber_lag(
        &self,
        task: &'static str,
        skipped: u64,
    );
}
```

Mutation methods return `()`, not `Result`. **Metrics mutations are infallible** — they either succeed (context/task was registered) or are silently dropped (context/task was not registered, which is a programming error that should never happen in production). This is an explicit deviation from the pattern spec's "typed errors" rule and is justified inline: a failing metrics record must never break the caller's hot path. A bug in metrics recording gets a `tracing::error!` log, not a propagated error.

### Read API

```rust
impl Metrics {
    /// Full observability snapshot — clones the entire current state.
    pub async fn snapshot(&self) -> MetricsSnapshot;

    /// Process-wide counters only.
    pub async fn global(&self) -> GlobalSnapshot;

    /// Snapshot of all domains.
    pub async fn domains(&self) -> Vec<DomainSnapshot>;

    /// Snapshot of one domain. Returns None if the name is not registered.
    pub async fn domain(&self, name: &str) -> Option<DomainSnapshot>;

    /// Snapshot of all tasks.
    pub async fn tasks(&self) -> Vec<TaskSnapshot>;

    /// Snapshot of one task. Returns None if the name is not registered.
    pub async fn task(&self, name: &str) -> Option<TaskSnapshot>;
}
```

The `*Snapshot` types are `Clone + Serialize` value types (all `u64` / `i64` / `DateTime<Utc>` / `String` fields, no atomics). They are the wire format for the API handlers and SSE stream.

### Event API

```rust
impl Metrics {
    pub fn changes(&self) -> broadcast::Receiver<MetricsChanged>;
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum MetricsChanged {
    DomainRegistered { domain: &'static str },
    TaskRegistered { task: &'static str },
    TaskReady { task: &'static str, ready_at: DateTime<Utc> },
    TaskStateChanged { task: &'static str, state: &'static str },
    SubscriberLagDetected { task: &'static str, skipped: u64 },
    ThresholdCrossed { metric: String, value: f64, threshold: f64 },
}
```

**Events fire only on interesting transitions.** Counter increments do NOT fire events — they would flood the broadcast channel at thousands per second during bulk operations. Consumers that want counter values poll `snapshot()`. This matches the push/pull duality established in the epic.

`MetricsChanged` does not carry `affected: Vec<String>` like other domain events because it describes global transitions, not per-item changes. This is another documented deviation from the pattern template — justified because Metrics observes other domains and the "affected" field would be inappropriate.

### Error type

Because mutation methods are infallible, `MetricsError` is used only by read methods that may fail at the API boundary (e.g., `domain(name)` when the name is unknown — though that returns `Option`, not `Result`). In practice the error type is minimal:

```rust
#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    // Reserved for future use — currently Metrics returns no errors
    // from public methods. The enum exists so handlers can use a
    // uniform Result<T, MetricsError> shape.
    #[error("metrics unavailable")]
    Unavailable,
}
```

Minimal enum with one placeholder variant. This is documented and will be extended if future Metrics features (threshold alerts, rate-based metrics) introduce failure modes.

### Tracing

Per the pattern spec, every public method has `#[tracing::instrument(level = "debug", skip(self))]` except for the hot-path recorders (`record_domain_event`, `record_mutation_latency`, `record_subscriber_lag`), which use `level = "trace"` to avoid flooding logs during high-frequency mutation.

### Integration with `Offerings`

After the Metrics aggregate exists, `Offerings` is retrofitted to inject it:

```rust
pub struct Offerings {
    state:   RwLock<OfferingsState>,
    store:   Arc<dyn OfferingStore>,
    metrics: Arc<Metrics>,                 // NEW
    changes: broadcast::Sender<OfferingsChanged>,
}

impl Offerings {
    pub fn new(
        store: Arc<dyn OfferingStore>,
        metrics: Arc<Metrics>,             // NEW parameter
    ) -> Self { ... }
}
```

Inside `Offerings::finalize`:

```rust
async fn finalize(
    &self,
    all: Vec<Offering>,
    kind: ChangeKind,
    affected: Vec<String>,
) {
    let started = std::time::Instant::now();

    if let Err(e) = self.store.save(&all).await {
        tracing::error!(kind = ?kind, error = ?e, "Failed to persist");
    }

    self.metrics.record_mutation_latency("offerings", started.elapsed()).await;
    self.metrics.record_domain_event("offerings", kind.name()).await;

    let _ = self.changes.send(OfferingsChanged::new(kind, affected));
}
```

`ChangeKind::name()` returns the stable `&'static str` form (`"upserted"`, `"promoted"`, etc.) — this method is added in Chapter 4.

### The rename (Chapter 2)

Before the Metrics aggregate lands, rename the existing hardware-resource surface in one coordinated chapter:

| File / Type / Path | Before | After |
|--------------------|--------|-------|
| Common type | `garden_common::MetricsSnapshot` | `garden_common::ResourcesSnapshot` |
| Common type | `garden_common::StoneResources` | unchanged (already correct) |
| Common module | `garden_common::metrics::system` | `garden_common::resources::system` |
| Common module | `garden_common::metrics` (the parent) | `garden_common::resources` |
| Moss handler | `src/moss/src/api/v1/metrics.rs` | `src/moss/src/api/v1/resources.rs` |
| Moss handler fn | `get_metrics` | `get_resources` |
| Moss domain | `src/moss/src/domain/metrics_collection.rs` | `src/moss/src/domain/resources.rs` (or `resources/mod.rs` + `collection.rs` if multi-file) |
| Moss domain type | `StoneMetrics` (normalized) | `NormalizedResources` |
| Moss fetcher | `fetch_stone_metrics` | `fetch_stone_resources` |
| Moss route | `/api/v1/stone/metrics` | `/api/v1/stone/resources` |
| Typed client | `StoneInfoApi::metrics()` | `StoneInfoApi::resources()` |
| Typed client docstring | "Prometheus metrics" | "Stone hardware resource snapshot" |
| Manifest entry | "Prometheus metrics" at `/metrics` | "Hardware resource snapshot" at `/resources` |

This is a mechanical rename that triggers compile errors at every call site. The goal: after this chapter commits, `rg 'MetricsSnapshot' src/` returns zero matches and `rg 'metrics_collection' src/` returns zero matches.

Callers of the rename:
- `bootstrap/router.rs` — registers the `/metrics` route (becomes `/resources`)
- `bootstrap/router.rs` manifest — describes the endpoint (text corrected)
- `domain/placement.rs`, `domain/scoring.rs`, any file using `StoneMetrics` — propagated by compile errors
- `src/common/src/client/stone_api.rs` — typed client method and docstring

### AppState integration

`AppState` gains one field:

```rust
pub struct AppState {
    // ...
    pub metrics: Arc<Metrics>,     // NEW
    // ...
}
```

Plus a `FromRef<AppState> for Arc<Metrics>` impl.

Construction at bootstrap:

```rust
// bootstrap/run.rs, early phase
let metrics = Arc::new(Metrics::new());

// Later, when Offerings is constructed
let offerings = Arc::new(
    Offerings::load(offering_store, metrics.clone()).await?
);
```

Metrics must be constructed before any aggregate that injects it.

### Task registration

Every `BackgroundTask` that spawns needs to be registered with Metrics at construction time. Chapter 3 extends the task supervisor's spawn loop with one call:

```rust
// tasks/supervisor.rs inside the spawn loop
metrics.register_task(name).await;
// ... existing spawn code
```

Tasks that want to observe their own subscriber lag call `metrics.record_subscriber_lag(Self::NAME, skipped)` inside their `RecvError::Lagged` arm. The projection tasks added by subsequent books all follow this pattern.

## Chapter breakdown

Per the standard six-chapter template, with adjustments for the rename-first flow:

### Chapter 1 — Scope & ADR (this commit)

- This ADR lands.
- ARCH-0017 revision history is amended (done in the preceding commit `d176f0db`).
- No Rust code changes in this chapter.

### Chapter 2 — Rename metrics → resources

- All files and types renamed per the table above.
- `git mv` is used for file moves so `git log --follow` preserves history.
- Content changes (e.g., docstring corrections) go in the same commit as the mechanical rename — this is a rename commit per code-standards §14, but the content delta is tiny and directly tied to the rename itself.
- Manifest entry corrected: `description` changes from "Prometheus metrics" to "Hardware resource snapshot" and `response_type` from `text/plain` to `application/json`.
- Exit criterion: `cargo check --all` green, `cargo test --package garden-moss` passes, `/api/v1/stone/resources` returns the same data the old `/metrics` used to return, `rg 'MetricsSnapshot' src/` returns zero matches.

### Chapter 3 — Extract the Metrics aggregate

- `src/moss/src/domain/metrics/` directory with `mod.rs`, `aggregate.rs`, `state.rs`, `event.rs`, `error.rs`, `tests.rs`.
- `MetricsState`, `DomainMetrics`, `TaskMetrics`, `GlobalMetrics`, `LatencyHistogram` types.
- `Metrics` aggregate with all commands and queries.
- `MetricsChanged` event enum.
- `MetricsError` enum (placeholder).
- Test scaffold: fakes (none required — no ports), required tests (registration, recording increments counters, snapshot round-trip, concurrent recording from multiple tasks), plus tests specific to this aggregate (event firing on transitions, no event firing on counter increments, lag event semantics).
- `Arc<Metrics>` field added to `AppState`.
- `FromRef<AppState> for Arc<Metrics>` impl.
- `bootstrap/run.rs` constructs the aggregate before any other aggregate that will inject it.
- `testing.rs` constructs a default Metrics for test AppState.
- Exit criterion: aggregate builds, tests pass, no other code touches `Metrics` yet.

### Chapter 4 — Wire events & projections + retrofit Offerings

- Task supervisor (`src/moss/src/tasks/supervisor.rs`) calls `metrics.register_task(name)` inside the spawn loop and `metrics.record_task_transition(name, state)` on state transitions. This is the only place the supervisor talks to Metrics; everything else stays in ARCH-0015's clean scope.
- `OfferingsProjectionTask` (from ARCH-0016) calls `metrics.record_subscriber_lag(Self::NAME, skipped)` in its `RecvError::Lagged` arm.
- `Offerings::new` and `Offerings::load` add an `Arc<Metrics>` parameter.
- `Offerings::finalize` records mutation latency and domain event via Metrics (see integration snippet above).
- `ChangeKind::name() -> &'static str` added as a new method on the existing enum.
- `Offerings::NAME` constant added (`"offerings"`).
- `bootstrap/run.rs` wires Metrics into the Offerings constructor.
- `testing.rs` wires Metrics into the test Offerings.
- Exit criterion: Offerings tests still pass, `cargo test --package garden-moss` green, `rg 'record_domain_event\|record_mutation_latency' src/moss/src/domain/offerings/` returns matches.

### Chapter 5 — API endpoints + SSE stream

- `src/moss/src/api/v1/metrics.rs` — NEW (replacing the old content which moved to `resources.rs` in Chapter 2).
- Handlers: `get_metrics`, `get_metrics_global`, `get_metrics_domains`, `get_metrics_domain`, `get_metrics_tasks`, `get_metrics_task`, `metrics_stream`.
- Each handler is a ~3-line thin dispatcher using `FromRef<AppState> for Arc<Metrics>` extraction.
- Routes registered in `bootstrap/router.rs`.
- `get_task_status_single` handler for new `/api/v1/stone/tasks/{name}` endpoint — reads from `SupervisorHandle`, returns 404 if not found.
- Manifest entries added for all new endpoints with accurate descriptions.
- `StoneInfoApi::metrics_snapshot()` typed client method added (the old `.metrics()` was renamed to `.resources()` in Chapter 2; the new `.metrics_snapshot()` hits the new endpoint and returns the typed snapshot).
- Exit criterion: curl each new endpoint against a live moss and verify response shapes; `/api/v1/stone/metrics/stream` emits SSE events when a forced `Offerings::upsert` runs.

### Chapter 6 — Verify & document

- Run the verification invariants from the exit criteria table.
- Update `docs/reference/context-map.md` — Metrics entry moves to "Full contexts" section, Resources entry marked as Partial→Full (it's a facade, not a new aggregate, but the rename completed).
- Update `docs/glossary.md` if new terms appeared (e.g., "latency histogram" if used anywhere externally).
- Update `docs/scaffolding.md` — no new scaffolds introduced in Book I; existing ARCH-0016 ActiveGuard entry unchanged.
- This ADR's status remains `Accepted` (no changes needed).
- Exit criterion: all checks pass, context map updated, Book I closes.

## Rationale

- **Name collision is fatal.** Building `Metrics` without renaming the existing surface would create a codebase where the word "metrics" meant two different things depending on context. The pattern spec explicitly names ubiquitous language violations as bugs. The rename is not optional.

- **In-memory only, no `Store` port.** Prometheus-style counters reset on restart. This is correct behavior for observability — historical metrics are consumed by external time-series databases (Prometheus, InfluxDB, Datadog), not persisted inside the process. Persisting counters between restarts would introduce consistency problems (what does "X events since last reset" mean if reset happens arbitrarily?) without benefit.

- **Push/pull duality.** The `changes()` event channel fires only on interesting transitions so that subscribers don't drown in counter-increment events. Consumers that want counter values poll the snapshot. This matches real observability systems (Prometheus scrapes, alerts are separate) and keeps the broadcast channel bounded.

- **Lock-free hot path.** `record_domain_event` is called on every mutation across every context. It must not contend on a write lock. The `Arc<DomainMetrics>` + atomic-field design achieves this: read lock for the map lookup (shared, short), atomic increment (no lock), no allocation on the path. Under load the hot path is essentially free.

- **Complementary to SupervisorHandle, not a replacement.** ARCH-0015's supervisor is a clean, focused context. Collapsing task state into Metrics would break that separation. Two endpoints with correlated data (join by task name) is the right shape.

- **Mutation methods are infallible.** This deviation from the pattern spec is explicit and justified: a metrics recording failure must not break the caller. Metrics is observing the hot path; it cannot *be* the hot path's failure mode.

- **No `affected` field on `MetricsChanged`.** Another explicit deviation, justified because Metrics observes other contexts and does not have per-item identity of its own. Events describe global transitions.

- **Prometheus deferred.** Building the aggregate right makes a future Prometheus exporter trivial — it would be a `BackgroundTask` that periodically reads `metrics.snapshot()` and formats it in Prometheus text. That exporter is an adapter, not a domain concern, and should not bloat Book I.

## Consequences

### Positive

- **Every subsequent book gets observability for free.** Metrics exists from day one; aggregates injecting `Arc<Metrics>` get mutation-latency histograms, event counts, and subscriber-lag tracking without writing any metrics code themselves.
- **Unified observability surface.** `/api/v1/stone/metrics` becomes the single place to look for "what's happening inside this stone." Combined with `/tasks` for lifecycle and `/resources` for hardware, the stone's self-description surface is coherent.
- **Future dashboard/Prometheus integration is unblocked.** ORCH-0031 dashboard, an eventual Prometheus exporter, or third-party tooling can all read the aggregate's snapshot without touching any other context.
- **The false "Prometheus format" claim is corrected.** The manifest and typed client will accurately describe what the endpoints return.
- **Name hygiene.** "Metrics" means observability, "Resources" means hardware. No more confusion.

### Negative

- **Rename surface is large.** The rename touches ~20 files across `garden-common`, `garden-moss`, and the typed client. Mechanical, but still a meaningful diff to review.
- **Mutation methods are infallible.** This is an explicit deviation from the pattern spec. Future contexts that need infallible mutation paths now have a precedent, which is good, but it complicates the "every mutation is typed-error" story.
- **No persistence means no historical trends inside moss.** If someone wants "what was the event rate 10 minutes ago?" they need an external scraper (Prometheus). This is a feature, not a bug, but it's worth documenting.
- **`MetricsChanged` events do not carry `affected`.** Consumers that want item-level detail must query the specific context's `changes()` stream. Metrics is not a source of truth for "what changed" — the originating context is.

### Neutral

- **The `garden_common::MetricsSnapshot` type rename propagates to Rake via compile.** Rake does not use this type by string literal, so the rename is safe. A clean `cargo build --all` after Chapter 2 proves the propagation is complete.
- **Orchestrators have their own `/metrics` endpoint** which this book does not touch. Garden-wide consistency (all stones and orchestrators using the same `/resources` + `/metrics` split) is a future concern beyond ARCH-0017's scope.
- **`SupervisorHandle` stays exactly as it is.** No extension, no deprecation. The only coupling is the one call to `metrics.register_task(name)` inside the spawn loop, which is additive.

## Migration plan

Six commits, one per chapter:

1. **Chapter 1** (this commit) — ADR lands, no code changes.
2. **Chapter 2** — Rename metrics → resources across common/moss/typed client/manifest. Build green, existing tests pass, `/api/v1/stone/resources` returns the hardware snapshot.
3. **Chapter 3** — Extract `Metrics` aggregate, add to `AppState`, construct in bootstrap. No other code touches it yet. Tests pass.
4. **Chapter 4** — Wire task supervisor and Offerings retrofit. `Arc<Metrics>` flows through the aggregate construction path. Build green.
5. **Chapter 5** — API endpoints, SSE stream, typed client method, manifest entries. Curl verification documented.
6. **Chapter 6** — Verification, context map update, book closes.

Each commit lands green to `dev`. No cross-chapter atomicity. Book I is complete when Chapter 6 commits.

## Deferred renames

One rename that Chapter 2 would have done inside the "metrics → resources" consolidation is deliberately deferred because it crosses an external wire-format boundary:

- `moss::domain::placement::PlacementMetrics` (struct) → would be `PlacementResources`
- `moss::domain::placement::PlacementRecommendation.metrics` (field) → would be `resources`

Both are part of the JSON response body of the placement endpoint and consumed by Rake's own `PlacementRecommendation` struct at [src/rake/src/commands/offering/mod.rs:61](../../src/rake/src/commands/offering/mod.rs). Renaming the moss Rust symbols would change the wire shape from `{"metrics": {...}}` to `{"resources": {...}}` and break Rake deserialization. Per ARCH-0017, external API contracts stay stable throughout the epic.

This case is tracked in [scaffolding.md → Deferred renames → `deferred-placement-metrics`](../scaffolding.md#deferred-placement-metrics-placementmetrics-struct-and-metrics-field) and will be revisited in a post-moss-epic API realignment that renames the wire shape in lockstep across moss, Rake, the typed `StoneApi` client, and any other consumers.

## Out of scope

- **Prometheus exporter.** Deferred to a future book or post-epic adapter.
- **OpenTelemetry integration.** Same.
- **Threshold alert rules.** The `MetricsChanged::ThresholdCrossed` variant is defined in the event enum for forward compatibility but no rule engine exists in Book I. Future extension.
- **Historical time-series storage.** Counters reset on restart; external scrapers are expected for historical data.
- **Orchestrator `/metrics` endpoint** at `src/orchestrators/ai/src/http/router.rs:17`. Separate crate, separate epic if it ever happens.
- **Storage context's `emit_storage_changed` metric integration.** Storage is Book VIII; its integration happens there, not in Book I. Book I covers Offerings only because it is the sole domain aggregate in moss today.

## References

- [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) — the epic this book serves; see revision history for the Book I plan change that produced this ADR
- [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md) — the aggregate retrofitted in Chapter 4
- [ARCH-0015](ARCH-0015-task-supervisor-registry.md) — `SupervisorHandle` that Metrics is complementary to
- [ARCH-0004](ARCH-0004-appstate-domain-context-extraction.md) — domain context extraction pattern
- [domain-aggregates.md](../specs/domain-aggregates.md) — the pattern spec; Metrics conforms with three documented deviations (no `Store` port, infallible mutations, `MetricsChanged` has no `affected` field)
- [context-map.md](../reference/context-map.md) — Metrics entry updated in commit `d176f0db`
- [glossary.md](../glossary.md) — ubiquitous language
- [scaffolding.md](../scaffolding.md) — no new scaffolds introduced in Book I
