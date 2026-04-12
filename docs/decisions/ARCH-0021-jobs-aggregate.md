---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-11
canonical: true
---

# ARCH-0021: Jobs Aggregate — Book IV of ARCH-0017

**Date**: 2026-04-11
**Status**: Accepted
**Book**: IV of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Depends on**: [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) (epic), [ARCH-0018](ARCH-0018-metrics-aggregate.md) (`Arc<Metrics>` injection), [ARCH-0019](ARCH-0019-tool-aggregate.md) (dual event streams precedent), [ARCH-0020](ARCH-0020-topology-aggregate.md) (field-level strangler + `maintain` command precedent)

## Context

Book IV extracts the `Jobs` bounded context. Today it is the thinnest bag-of-primitives field on `AppState`:

```rust
// src/moss/src/app_state.rs
pub enum JobStatus { Pending, Running, Completed, Failed }

pub struct Job {
    pub id: String,
    pub offerings: Vec<String>,       // repurposed: holds service names OR capability names
    pub status: JobStatus,
    pub completed: Vec<String>,
    pub failed: HashMap<String, String>,
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
}

pub struct AppState {
    // ...
    pub jobs: Arc<RwLock<HashMap<String, Job>>>,
    // ...
}
```

There is no aggregate, no mutation gateway, no event stream, no persistence, no reaper. Every caller holds a write guard and mutates fields directly. The `Job` domain type is defined in `app_state.rs` (code-standards §14 violation: a 600-line file mixing unrelated domain definitions with the `AppState` struct).

Per the Discovery Mandate, Chapter 1 re-evaluates the plan against the actual code. The findings reshape Book IV from a ~700-line thin extraction into a fuller, but still bounded, refactor.

### What the re-evaluation found

1. **Types live in `app_state.rs`.** `Job` and `JobStatus` are defined at [app_state.rs:28-45](../../src/moss/src/app_state.rs#L28-L45) alongside re-exports of offering types. Code-standards §14 requires one file per concept; `AppState` should not define domain types. Book IV moves `Job` + `JobStatus` to a new `domain/jobs/` module as Chapter 2 work. This is a cut-paste extraction, not a wholesale `git mv` (the file's other contents stay in `app_state.rs`), so Ch2 is a single content commit — a minor deviation from code-standards §14's "pure git-mv first" preference that the standard accommodates for extractions where the source file cannot move as a whole.

2. **38 call sites across 5 files.** Total touch surface:

   | File | Read | Write | Notes |
   |------|------|-------|-------|
   | `tasks/job_executors.rs` | 0 | 28 | Every executor mutates via `state.jobs.write().await; get_mut(job_id); …` |
   | `api/v1/offering_capabilities.rs` | 2 | 2 | Duplicate-job detection (read) + insert-new-job (write) |
   | `api/v1/jobs.rs` | 2 | 0 | List + get-by-id HTTP handlers |
   | `bootstrap/run.rs` | 1 | 1 | Pre-install manifest batch job + 5-second completion-poll loop |
   | `domain/service_lifecycle.rs` | 0 | 2 | Image-direct install + legacy install flows |

   Book III's surface was ~80 sites; Book II's was ~50. Book IV is smaller (38) but *denser* — 28 of them are in a single 1,980-line executors file.

3. **Mutation patterns are extremely repetitive.** The write sites all follow three shapes:

   - **Start**: `job.status = JobStatus::Running;` (~5 sites)
   - **Mark-completed item**: `job.completed.push(x); ` (~4 sites) or `job.failed.insert(key, error);` (~12 sites)
   - **Finalize**: `job.status = Completed|Failed; job.completed_at = Some(now);` (~9 sites; often mixed with a single-item insert)

   The right shape is a typed command surface with ~6-7 methods. Every `state.jobs.write().await.get_mut(job_id)` dance disappears at the call site. This is the cleanest ergonomic win in the epic so far.

4. **A precursor typed helper already exists.** [job_executors.rs:1787-1794](../../src/moss/src/tasks/job_executors.rs#L1787-L1794) defines a private `mark_job_failed(state, job_id, key, error)` helper that is exactly the shape of the aggregate command — private to the executors module, called from the capability tasks. Book IV's command API is this helper generalized and promoted onto the aggregate.

5. **No TTL or reaper exists.** `AppState::jobs` accumulates forever in memory; only a process restart clears it. Production stones that complete thousands of jobs per day leak memory bounded only by uptime. Book IV adds a `maintain()` command (matching Book III's `Topology::maintain`) that evicts terminal jobs past a TTL, plus a `JobsReaperTask` that calls it periodically. This is an **operational hygiene fix that falls naturally out of the aggregate pattern** and is folded into book scope per the tenet (architectural leanness, not code leanness).

6. **No persistence — ephemeral aggregate.** Jobs do not survive restart today, and Book IV keeps it that way. No `JobStore` port, no `load` on construction, no `save` in `finalize`. Matches Metrics (Book I), Resources (Book I), and Tool (Book II). Cites the "Ephemeral aggregates" deviation in the pattern spec.

7. **Dual event streams.** An existing wire-format stream `JobEvent` already exists at [domain/events.rs:164-193](../../src/moss/src/domain/events.rs#L164-L193), broadcast via `EventBus::emit()` and consumed by `infra/listeners/pulse.rs` for the pulse SSE firehose. `JobEvent` is a public wire contract — SSE clients (dashboard, rake) subscribe to it. Book IV **keeps** `JobEvent` as the wire-format stream and adds a new `Jobs::changes()` internal `JobsChanged` event stream with richer metadata for process-local subscribers. Matches Book II's `ToolChanged` + `ToolDelta` shape; cites the "Dual event streams" deviation in the pattern spec. Every command feeds *both* streams atomically.

8. **`bootstrap/run.rs` has a 5-second completion-poll loop.** [bootstrap/run.rs:1376-1403](../../src/moss/src/bootstrap/run.rs#L1376-L1403) spawns a task that loops every 5s reading the jobs map, waiting for a batch install to reach `Completed | Failed` so it can delete the pre-install manifest. This is exactly the "poll state where you should subscribe to an event" anti-pattern the epic exists to fix. Book IV converts this site to subscribe to `Jobs::changes()` and filter for the terminal transition of the specific job id. Clean clean-architecture win; Ch5 scope.

9. **Duplicate-job detection is prefix-scan.** [offering_capabilities.rs:331-347](../../src/moss/src/api/v1/offering_capabilities.rs#L331-L347) and [:740-766](../../src/moss/src/api/v1/offering_capabilities.rs#L740-L766) both iterate all jobs looking for a key prefix + active status. Book IV promotes this to a typed query: `Jobs::find_active_by_prefix(prefix) -> Option<Job>`. One-line call sites, one typed method instead of open-coded iteration across two handlers.

10. **`Job.offerings` is a semantic wart.** For install jobs the field holds service names; for refresh-capabilities and add-capability jobs it holds capability names. The field is serialized in `/api/v1/jobs` responses — it is a **public wire field**. Renaming it to `targets` would be a breaking API change. Book IV **does not rename** the field. Instead, it adds an entry to `docs/scaffolding.md` under "Deferred renames" (the same tracker Book I used for `PlacementMetrics`) to be reconsidered in the post-epic API realignment project.

11. **`JobStatus::Pending` is effectively dead state.** Every executor call path inserts the job as `Pending` and immediately (within the same task) sets it to `Running`. No queuing layer actually uses `Pending`. Book IV **keeps** `Pending` as a public enum variant — it is part of the wire contract, it is checked in duplicate-job detection (`Running | Pending` match), and collapsing it is not in scope. A future queueing layer can bring it back to life without schema churn.

12. **404 stub body in `get_job_status`.** [api/v1/jobs.rs:54-70](../../src/moss/src/api/v1/jobs.rs#L54-L70) returns a 404 with a hallucinated stub `Job` body (all-empty fields, `status = Failed`). This is an odd wire contract — 404 with a placeholder. Book IV **preserves** the quirk; changing the response shape is a wire break out of scope. The handler simply dispatches to a typed query; the stub construction stays at the handler.

13. **Size reality.** The ARCH-0017 estimate was ~700 lines for the smallest remaining book. With the type extraction (Ch2, ~80 lines), the aggregate skeleton (Ch3, ~400 lines of aggregate + ~250 lines of tests), the executor migration (Ch4, net near-zero lines because the command methods replace inline mutation patterns with shorter calls), the remaining site migration + reaper task + bootstrap event-subscription rewrite (Ch5, ~200 lines), and the Ch6 closure docs — realistic book size is **~1,000-1,200 lines**, moderately above the 700-line estimate. Surfaced to user per the tenet: don't pre-trim scope to hit line budgets.

## Decision

Book IV extracts `Jobs` as a full DDD aggregate with private state, typed commands, typed queries, a `JobsChanged` internal event stream, a parallel `JobEvent` wire-format stream preserved via `EventBus::emit()`, `Arc<Metrics>` injection, and a `maintain` command for TTL-based eviction. The aggregate is **ephemeral** — no `JobStore` port, no persistence. The `Job` + `JobStatus` types move out of `app_state.rs` into `domain/jobs/entry.rs`. The `AppState::jobs: Arc<RwLock<HashMap<String, Job>>>` field is deleted and replaced with `AppState::jobs: Arc<Jobs>`. A `JobsReaperTask` background task is registered with the supervisor to periodically call `maintain`.

### Module layout (target state)

```
src/moss/src/domain/jobs/
├── mod.rs            — re-exports Jobs, Job, JobStatus, JobsChanged, JobsError
├── aggregate.rs      — `Jobs` struct, typed commands, queries, changes()
├── state.rs          — `JobsState` (HashMap<String, Job>, optional reaper cursor)
├── entry.rs          — `Job`, `JobStatus` (moved from app_state.rs)
├── event.rs          — `JobsChanged` enum + `ChangeKind` for metrics
├── error.rs          — `JobsError`
├── maintenance.rs    — TTL policy, eviction logic, `ReapReport`
└── tests.rs          — unit tests
```

### Aggregate API

```rust
pub struct Jobs {
    state: RwLock<JobsState>,
    metrics: Arc<Metrics>,
    changes: broadcast::Sender<JobsChanged>,
    event_bus: EventBus,   // for parallel wire-format JobEvent emission
}

impl Jobs {
    pub const NAME: &'static str = "jobs";

    pub async fn new(metrics: Arc<Metrics>, event_bus: EventBus) -> Self {
        metrics.register_domain(Self::NAME, ChangeKind::ALL_NAMES).await;
        // ephemeral: no load, state starts empty
        Self { /* ... */ }
    }

    // ── Commands ────────────────────────────────────────────────────────
    /// Submit a new job; transitions Pending. Emits Submitted.
    pub async fn submit(&self, id: String, operation: &str, targets: Vec<String>) -> Job;

    /// Move Pending → Running. Emits Started + wire JobEvent::Started.
    pub async fn start(&self, id: &str, operation: &str, offering: &str);

    /// Record a successful item (append to completed). Emits ItemCompleted.
    pub async fn record_item_completed(&self, id: &str, item: String);

    /// Record a failed item (insert into failed). Emits ItemFailed + wire JobEvent::Failed.
    pub async fn record_item_failed(&self, id: &str, item: String, error: String);

    /// Finalize as Completed. Sets completed_at. Emits Completed + wire JobEvent::Completed.
    pub async fn complete(&self, id: &str, offering: &str);

    /// Finalize as Failed. Sets completed_at. Emits Failed + wire JobEvent::Failed.
    /// Optional last-error (key, error) pair to record before finalizing.
    pub async fn fail(&self, id: &str, offering: &str, last_error: Option<(String, String)>);

    /// Reap terminal jobs past TTL. Returns `ReapReport { evicted, kept }`.
    pub async fn maintain(&self) -> ReapReport;

    // ── Queries ─────────────────────────────────────────────────────────
    pub async fn get(&self, id: &str) -> Option<Job>;
    pub async fn snapshot(&self) -> Vec<Job>;
    pub async fn list_active(&self) -> Vec<Job>;
    pub async fn active_count(&self) -> usize;
    /// Find an active job whose id starts with `prefix` — replaces the
    /// prefix-scan in offering_capabilities.rs duplicate-job detection.
    pub async fn find_active_by_prefix(&self, prefix: &str) -> Option<Job>;

    // ── Events ──────────────────────────────────────────────────────────
    pub fn changes(&self) -> broadcast::Receiver<JobsChanged>;
}
```

### `JobsChanged` event

```rust
#[derive(Debug, Clone, Serialize)]
pub enum JobsChanged {
    Submitted  { id: String, operation: String, target_count: usize },
    Started    { id: String, offering: String },
    ItemCompleted { id: String, item: String, completed_total: usize },
    ItemFailed { id: String, item: String, error: String, failed_total: usize },
    Completed  { id: String, offering: String, duration_ms: u64 },
    Failed     { id: String, offering: String, duration_ms: u64, failure_count: usize },
    Evicted    { id: String, reason: EvictionReason },
}

pub enum ChangeKind {
    Submitted, Started, ItemCompleted, ItemFailed, Completed, Failed, Evicted,
}
```

### Wire-format stream — `JobEvent` (preserved)

The pre-existing `JobEvent` (Started/Progress/Completed/Failed) continues to flow through `EventBus::emit()` exactly as today. Every command that maps to a `JobEvent` variant emits *both*:

1. The rich internal `JobsChanged` event via the aggregate's `broadcast::Sender<JobsChanged>`.
2. The existing public wire `JobEvent` via `EventBus::emit()`, consumed by `infra/listeners/pulse.rs`.

Emission is atomic — inside the `finalize` helper, after metrics recording, both sends happen before the command returns. Failure of either send does not fail the command (broadcast lag is warn-and-continue per code-standards §13).

This is the Book II "Dual event streams" pattern deviation — cite it in the closure ADR but do not re-derive.

### Metrics integration

Register domain `jobs` with seven kinds (`submitted`, `started`, `item_completed`, `item_failed`, `completed`, `failed`, `evicted`). Every command records mutation latency; every event records per-kind counter via `record_domain_event`.

### Reaper task

```rust
// src/moss/src/tasks/jobs_reaper.rs
pub struct JobsReaperTask {
    jobs: Arc<Jobs>,
}

impl BackgroundTask for JobsReaperTask {
    const NAME: &'static str = "jobs-reaper";

    async fn run(&self, token: CancellationToken) -> Result<(), TaskError> {
        let mut ticker = interval(Duration::from_secs(60 * 10)); // every 10 minutes
        loop {
            tokio::select! {
                _ = token.cancelled() => return Ok(()),
                _ = ticker.tick() => {
                    let report = self.jobs.maintain().await;
                    if report.evicted > 0 {
                        tracing::info!(evicted = report.evicted, "Jobs reaper swept terminal jobs");
                    }
                }
            }
        }
    }
}
```

**TTL policy**: a terminal job (Completed or Failed) older than **24 hours** (measured from `completed_at`) is evicted. Active jobs (Pending, Running) are never evicted regardless of age — a stuck job is a bug worth surfacing, not a memory leak to quietly hide. The constant lives in `domain/jobs/maintenance.rs` and is surfaced in the pattern deviation section of the Ch6 closure.

### What Book IV does not do

- **No wire-format changes.** `Job.offerings` keeps its overloaded semantics; `/api/v1/jobs` returns the same shape. The 404 stub body stays. The `Pending` enum variant stays.
- **No executor refactor.** The executors are migrated site-by-site to typed commands in Ch4; the control flow of each executor stays identical. Longer-term executor simplification (collapsing the repeated shell patterns around the new command surface) is not in Book IV's scope.
- **No `NourishmentOrchestration.jobs` work.** That field (`state.orchestration.nourishment.jobs`) is a `HashMap<String, broadcast::Sender<String>>` of SSE progress channels — a different concept that happens to share the word "jobs". It stays inside the Orchestration context and will be addressed by Book XI (Orchestration Deep-Clean).
- **No queue semantics.** `Pending` state is kept for wire compatibility but no queueing layer is introduced. A future queue can bring Pending back to life without schema churn.
- **No persistence.** If a future requirement surfaces job history across restarts, a `JobStore` port can be added later. Book IV ships ephemeral.

## Chapter plan

| Ch | Scope |
|----|-------|
| 1  | ADR (this), revision history entry in ARCH-0017 |
| 2  | `domain/jobs/` module created with `Job` + `JobStatus` extracted from `app_state.rs` into `domain/jobs/entry.rs`; re-exports wired via `mod.rs`; no aggregate yet; `AppState::jobs` field unchanged |
| 3  | `Jobs` aggregate skeleton: state, `JobsState`, typed commands (`submit`, `start`, `record_item_completed`, `record_item_failed`, `complete`, `fail`, `maintain`), typed queries (`get`, `snapshot`, `list_active`, `active_count`, `find_active_by_prefix`), `JobsChanged` event + `changes()` broadcast, `Arc<Metrics>` injection with register_domain, ~15 unit tests incl. concurrency stress. Field-level strangler: `AppState::jobs` becomes `Arc<Jobs>` with the old `Arc<RwLock<HashMap>>` still alongside as `AppState::_jobs_legacy` for migration. |
| 4  | `tasks/job_executors.rs` migration: 28 sites → typed commands. `mark_job_failed` private helper deleted (replaced by `Jobs::record_item_failed` + `Jobs::fail`). Executors no longer touch raw `state.jobs.write().await.get_mut(...)`. |
| 5  | Remaining site migration: `api/v1/jobs.rs` (2 reads → `Jobs::get` / `Jobs::snapshot`), `api/v1/offering_capabilities.rs` (4 sites → `Jobs::find_active_by_prefix` + `Jobs::submit`), `bootstrap/run.rs` (insert → `Jobs::submit`, 5s poll loop → `changes()` subscription), `domain/service_lifecycle.rs` (2 inserts → `Jobs::submit`). Delete `AppState::_jobs_legacy`. Add `JobsReaperTask` to the supervisor. |
| 6  | Closure: context-map update (move Jobs from Absent to Full), glossary entries (Jobs aggregate, ReapReport, terminal transition), pattern-spec cite (no new deviations — reuses Ephemeral + Dual event streams), ARCH-0021 frontmatter `completed: <date>`, ARCH-0017 revision history entry. `docs/scaffolding.md` gains a "Deferred renames" entry for `Job.offerings → Job.targets`. Final exit-criteria grep. |

## Exit criteria

Book IV is closed when:

1. `rg 'state\.jobs\.(read|write)' src/moss/src/` returns 0 matches
2. `rg 'pub jobs: Arc<RwLock<HashMap<String, Job>>>' src/moss/src/app_state.rs` returns 0 matches (field replaced)
3. `rg 'pub (struct|enum) Job\b|pub (struct|enum) JobStatus\b' src/moss/src/app_state.rs` returns 0 matches (types moved)
4. `rg 'mark_job_failed' src/moss/src/tasks/job_executors.rs` returns 0 matches (private helper removed)
5. `rg 'state\.jobs\.write\|state\.jobs\.read' src/moss/src/` returns 0 matches
6. A `JobsReaperTask` is registered in the supervisor; `SupervisorHandle::running_tasks()` lists it on a live stone.
7. `cargo check --all && cargo test --package garden-moss --lib && cargo clippy --package garden-moss --lib -- -D warnings` all green
8. Manual smoke on a live stone: `POST /api/v1/stone/offerings` creates a job and the installer completes it; `GET /api/v1/jobs` lists it; after 24 hours (or a forced `maintain()`), the terminal job is evicted; `/api/v1/stone/metrics/domains/jobs` shows per-kind counters.

## Pattern deviations

Book IV is an **ephemeral aggregate** (no Store port, no persistence) with a **dual event stream** (`JobsChanged` internal + `JobEvent` wire format). Both deviations are already first-class entries in [`docs/specs/domain-aggregates.md`](../specs/domain-aggregates.md); Book IV adds `Jobs` to the "Current instances" tables rather than introducing a new deviation section.

One **minor pattern note** — not a deviation, but worth recording: the `Jobs::submit` command takes an owned `String` id rather than generating one internally. All current callers derive the id from an external format (`uuid::Uuid::now_v7()` for install jobs, formatted composite keys like `add-capability-{offering}-{name}-{uuid}` for capability jobs). A future version could offer both `submit_with_id(id)` and `submit_new() -> id` — Book IV only provides the explicit-id form to avoid over-designing.

## Alternatives considered

### Alternative A — Persist jobs with a `JobStore` port (rejected)

Option A would add a `JobStore` port (file-backed, matching `FileTopologyStore`) so jobs survive restart. Rejected: jobs today do not survive restart and no consumer expects them to. Adding persistence would (a) expand Ch3 scope significantly (serialization, atomic writes, load on construction), (b) require a compaction / retention story for historical jobs, and (c) break the ephemeral-aggregate pattern that Metrics, Resources, and Tool all share. If a future requirement surfaces the need, a port can be added later — the aggregate shape supports it.

### Alternative B — Collapse `JobEvent` into `JobsChanged` (rejected)

Option B would delete `domain/events.rs::JobEvent` and route all job lifecycle events through the new `JobsChanged` stream. Rejected: `JobEvent` is a **public wire contract** — SSE clients (rake, dashboards) already consume it via the `/pulse` firehose. Collapsing it is a breaking API change out of Book IV's scope. Dual event streams is a first-class pattern deviation (established by Book II's `ToolDelta`); Book IV reuses it.

### Alternative C — Fold `Pending` out of `JobStatus` (rejected)

Option C would delete the `JobStatus::Pending` variant since no code actually queues jobs. Rejected: `Pending` is a wire-format enum variant in `/api/v1/jobs` responses, and the duplicate-job detection logic in `offering_capabilities.rs` matches `Running | Pending` to decide whether to return `InProgress`. Removing `Pending` changes the match behavior and the wire contract. Ship ephemeral; leave `Pending` alone.

### Alternative D — Rename `Job.offerings → Job.targets` (deferred)

Option D renames the semantically-overloaded field so its name matches its usage (service names OR capability names). Rejected for Book IV: the field is serialized in the public `/api/v1/jobs` response. `garden-rake` deserializes it with an explicit `offerings` name. Renaming is a coordinated breaking API change that belongs to the post-epic API realignment project. **Logged in `docs/scaffolding.md`** under "Deferred renames" (entry id: `deferred-job-offerings-field`) so it surfaces when the realignment happens.

### Alternative E — Keep the 5-second completion-poll loop (rejected)

Option E would leave `bootstrap/run.rs:1376-1403` as-is — a task that polls `state.jobs.read()` every 5 seconds waiting for a specific job to reach terminal state. Rejected: this is the exact "poll where you should subscribe" anti-pattern the epic exists to eliminate. Book IV's `Jobs::changes()` makes the subscribe-form trivial (one `tokio::select!` arm over a `broadcast::Receiver<JobsChanged>` filtered for the target id). The rewrite is ~15 lines shorter than the poll loop and races zero times.

## References

- [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) — the epic
- [ARCH-0018](ARCH-0018-metrics-aggregate.md) — Metrics aggregate; register-with-kinds pattern reused
- [ARCH-0019](ARCH-0019-tool-aggregate.md) — Tool aggregate; dual event streams precedent
- [ARCH-0020](ARCH-0020-topology-aggregate.md) — Topology aggregate; `maintain` command precedent
- [docs/specs/domain-aggregates.md](../specs/domain-aggregates.md) — pattern spec; Ephemeral aggregates + Dual event streams both apply
- [docs/scaffolding.md](../scaffolding.md) — Deferred renames section; new entry for `Job.offerings → Job.targets`
- [code-standards.md](../code-standards.md) §14 — one file per concept (the `Job`/`JobStatus` extraction rationale)
