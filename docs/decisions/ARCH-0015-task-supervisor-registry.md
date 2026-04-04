---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-03
canonical: true
---

# ARCH-0015: Task Supervisor Registry — Declarative Background Task Lifecycle

**Date**: 2026-04-03
**Status**: Accepted
**Depends on**: [ARCH-0007](ARCH-0007-monomorphic-domain-traits.md) (edition 2024, TaskSupervisor)

## Context

Background tasks in Moss are spawned through two redundant code paths in
`coordinator.rs` (~1600 lines):

1. `start_all_background_tasks()` — legacy path, spawns 14 tasks via
   `tokio::spawn` through helper functions.
2. `start_background_tasks()` — supervisor path, spawns 32 tasks via
   `supervisor.spawn("name", ...)` across Phases 11–17.

Both paths enumerate tasks at call sites with ad-hoc parameter passing. They
diverged silently: when the Tier 2 topology probe (ARCH-0014) was added to
Path 1, it never ran in production because stones use Path 2. The bug was
invisible — no error, no log, no panic — because the task was simply never
spawned.

The root cause is structural:

- **Task startup is defined by enumeration at call sites**, not by declaration
  at definition sites. Adding a task means finding every spawn location and
  hoping you got them all.
- **Each task has a unique function signature.** The coordinator must know the
  intimate parameter details of every task. No common interface, no registry,
  no discoverability.
- **Implicit ordering dependencies.** Docker-dependent tasks are spawned after
  Docker initialization, but this ordering is encoded in source code position
  (line number), not in a declared dependency graph. Reordering lines breaks
  things silently.
- **No standard lifecycle contract.** Tasks finish by returning `()`. The
  supervisor knows a task ended but not whether it succeeded, failed, or was
  cancelled. Panics are caught but reported generically.
- **No readiness signaling.** A task that produces data (hardware detection,
  registry loading) has no way to tell dependents "my data is available now."
  Dependents either chain sequentially (blocking the spawned task) or use
  arbitrary `sleep()` delays.

The coordinator is 1600 lines of orchestration boilerplate that grows linearly
with every new task.

## Decision

Replace both startup paths with a single **task registry** backed by a
**`BackgroundTask` trait** with standard lifecycle, dependency declaration,
readiness signaling, and outcome reporting.

### The Trait

```rust
/// What every background task returns when it finishes.
pub enum TaskOutcome {
    /// Normal completion.
    Completed,
    /// Graceful shutdown via cancellation token.
    Cancelled,
    /// Task-level failure (not a panic — panics are caught by the supervisor).
    Failed { error: String },
}

/// Every background task implements this trait.
pub trait BackgroundTask: Send + 'static {
    /// Unique task name (used in logs, tracing spans, supervisor status).
    fn name(&self) -> &'static str;

    /// Tasks this one waits for before starting meaningful work.
    /// Returns names that must have called `ctx.signal_ready()` first.
    /// Default: no dependencies (start immediately).
    fn dependencies(&self) -> &[&str] { &[] }

    /// Run the task. Called exactly once by the supervisor.
    ///
    /// - Call `ctx.wait_for_dependencies()` at the top to block until
    ///   all declared dependencies have signaled ready.
    /// - Call `ctx.signal_ready()` when this task's data is available
    ///   for dependents. May be called before the task finishes (e.g.,
    ///   after loading cache but before running a full probe).
    /// - Return `TaskOutcome` when done. The supervisor records it.
    fn run(
        self: Box<Self>,
        ctx: TaskContext,
    ) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>>;
}
```

### Task Context

```rust
/// Provided to every task by the supervisor.
pub struct TaskContext {
    pub state: AppState,
    pub token: CancellationToken,
    deps: DependencyGate,
    ready: ReadySignal,
}

impl TaskContext {
    /// Block until all declared dependencies have signaled ready.
    /// Returns immediately if no dependencies declared.
    pub async fn wait_for_dependencies(&mut self) {
        self.deps.wait().await;
    }

    /// Signal that this task's data/service is available for dependents.
    /// Idempotent — safe to call multiple times.
    pub fn signal_ready(&self) {
        self.ready.signal();
    }
}
```

`DependencyGate` holds `watch::Receiver<bool>` channels, one per declared
dependency. `wait()` blocks until all receivers see `true`. `ReadySignal`
wraps a `watch::Sender<bool>` that sets `true` on first call.

### The Registry

A single function returns all tasks:

```rust
pub fn build_task_registry(config: &TaskConfig) -> Vec<Box<dyn BackgroundTask>> {
    let mut tasks: Vec<Box<dyn BackgroundTask>> = vec![
        // ── Core detection ──
        Box::new(HardwareDetectionTask),
        Box::new(TopologyProbeTask),

        // ── Docker ──
        Box::new(DockerEventsTask),
        Box::new(RegistryLoaderTask),

        // ── Discovery & presence ──
        Box::new(ElectionListenerTask),
        Box::new(DiscoveryHandlerTask),
        Box::new(PeriodicAnnouncerTask),
        Box::new(PresenceLoadMonitorTask),
        Box::new(PresenceHealthMonitorTask),
        Box::new(CompanionScanTask),
        Box::new(InitialServiceSyncTask),

        // ── Health & metrics ──
        Box::new(HealthMonitorTask),
        Box::new(MetricsCollectorTask),
        Box::new(MaintenanceSweepTask),

        // ── Offerings & catalog ──
        Box::new(CatalogBuilderTask),
        Box::new(OfferingOrchestrationTask),

        // ── Storage ──
        Box::new(VolumeMonitorTask),
        Box::new(MediaWatcherTask),
        Box::new(StorageLifecycleTask),
        Box::new(StorageOrchestrationTask),
        Box::new(StorageReplicationTask),
        Box::new(StorageBeaconSubscriberTask),
        Box::new(StorageTickAggregatorTask),
        Box::new(S3ListenerLifecycleTask),
        Box::new(FsWatcherTask),
        Box::new(StorageConsoleTask),

        // ── Network ──
        Box::new(IpChangeHandlerTask),
        Box::new(TopologyMaintenanceTask),
        Box::new(RegistryMaintenanceTask),

        // ── Scheduling ──
        Box::new(TaskSchedulerTask),
    ];

    // Conditional tasks
    if config.adoption_enabled {
        tasks.push(Box::new(AutoAdoptionTask::new(config.adoption.clone())));
    }
    if config.mdns_enabled {
        tasks.push(Box::new(MdnsHealthListenerTask));
        tasks.push(Box::new(MdnsLurkListenerTask));
    }
    if !config.use_static_host {
        // ip-change-handler only needed with dynamic networking
    }

    tasks
}
```

Adding a task = adding one line. Removing a task = removing one line.
No second path to forget.

### The Supervisor

```rust
pub struct TaskSupervisor {
    entries: Vec<TaskEntry>,
}

struct TaskEntry {
    name: &'static str,
    ready: watch::Receiver<bool>,
    outcome: watch::Receiver<Option<TaskOutcome>>,
    handle: JoinHandle<()>,
}

impl TaskSupervisor {
    /// Build dependency graph, create channels, spawn all tasks.
    pub fn start(
        registry: Vec<Box<dyn BackgroundTask>>,
        state: AppState,
        token: CancellationToken,
    ) -> Self {
        // 1. Create ready channels: HashMap<&str, (Sender, Receiver)>
        // 2. Validate dependency graph (no cycles, no missing deps)
        // 3. For each task:
        //    a. Resolve dependency receivers
        //    b. Build TaskContext
        //    c. Spawn with tracing span, panic catch, outcome channel
        // 4. Return supervisor with handles
    }

    /// Summary: total, running, completed, failed, panicked.
    pub fn status(&self) -> SupervisorStatus { ... }

    /// Per-task detail: name, state, ready status, outcome.
    pub fn tasks(&self) -> Vec<TaskStatus> { ... }

    /// Wait for all tasks to finish (shutdown path).
    pub async fn join_all(&mut self) -> Vec<(&str, TaskOutcome)> { ... }
}
```

The supervisor validates the dependency graph at startup — a cycle or missing
dependency is a hard error, not a runtime deadlock.

### Dependency Graph

Declared dependencies replace implicit phase ordering. Each task declares
what it needs; the supervisor resolves the graph.

```
hardware-detection          (no deps)           → signals: caps available
topology-probe              (no deps)           → signals: topology cached/probed
docker-events               (no deps)           → signals: docker stream open
registry-loader             [docker-events]     → signals: offerings loaded
catalog-builder             [registry-loader]   → signals: offerings indexed
health-monitor              [docker-events]     → signals: health loop running
auto-adoption               [docker-events]     → signals: adoption scan done
offering-orchestration      [catalog-builder, health-monitor] → signals: orch ready
metrics-collector           (no deps)           → signals: metrics flowing
periodic-announcer          [hardware-detection] → signals: announcing
companion-scan              (no deps)           → signals: companions started
volume-monitor              (no deps)           → signals: volume events flowing
storage-lifecycle           [volume-monitor]    → signals: storage mounts ready
storage-orchestration       [storage-lifecycle] → signals: replication ready
storage-replication         [storage-orchestration] → signals: replication active
s3-listener-lifecycle       [storage-lifecycle] → signals: S3 ports active
topology-maintenance        (no deps)           → signals: (long-running)
maintenance-sweep           (no deps)           → signals: (periodic)
```

Tasks with no dependencies start immediately and race. Tasks with dependencies
block on `ctx.wait_for_dependencies()` until their predecessors call
`ctx.signal_ready()`. No phase numbers, no line-order sensitivity.

### Standard Spawn Wrapper

Every task is spawned through one codepath:

```rust
supervisor.spawn(name, async move {
    let span = tracing::info_span!("task", name = name);
    let _guard = span.enter();

    tracing::info!("Task starting");

    let result = std::panic::AssertUnwindSafe(task.run(ctx))
        .catch_unwind()
        .await;

    match result {
        Ok(outcome) => {
            match &outcome {
                TaskOutcome::Completed => tracing::info!("Task completed"),
                TaskOutcome::Cancelled => tracing::info!("Task cancelled"),
                TaskOutcome::Failed { error } => tracing::error!(error, "Task failed"),
            }
            outcome_tx.send(Some(outcome)).ok();
        }
        Err(panic) => {
            let msg = panic_message(&panic);
            tracing::error!(error = msg, "Task PANICKED");
            outcome_tx.send(Some(TaskOutcome::Failed {
                error: format!("panic: {}", msg),
            })).ok();
        }
    }
});
```

Every task gets a tracing span, panic protection, and outcome reporting.
No task can die silently.

### What Gets Deleted

- `start_all_background_tasks()` — entire function (legacy path)
- `start_background_tasks()` Phase 11–17 orchestration blocks — replaced by
  registry + supervisor
- All `start_*` helper functions that just wrap `tokio::spawn` — inlined into
  task `run()` methods
- Implicit phase ordering comments (`// Phase 12`, `// Phase 15-17`) —
  replaced by declared dependencies

The coordinator shrinks from ~1600 lines to ~100 lines (registry + supervisor
start + shutdown).

### Task Implementation Pattern

Each task is a struct in its own file under `tasks/`:

```rust
// tasks/topology_probe.rs

pub struct TopologyProbeTask;

impl BackgroundTask for TopologyProbeTask {
    fn name(&self) -> &'static str { "topology-probe" }
    // no dependencies — loads its own cache, probes independently

    fn run(self: Box<Self>, mut ctx: TaskContext)
        -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>>
    {
        Box::pin(async move {
            // Load cache → serve immediately
            if let Some(cached) = load_cache().await {
                update_state(&ctx.state, cached).await;
                ctx.signal_ready(); // dependents unblock NOW
            }

            // Check if re-probe needed
            let fp = compute_fingerprint().await;
            if cache_valid(&fp) {
                if !ctx.ready.is_signaled() { ctx.signal_ready(); }
                return TaskOutcome::Completed;
            }

            // Full probe
            match probe_full(fp).await {
                Ok(topo) => {
                    update_state(&ctx.state, topo).await;
                    ctx.signal_ready();
                    TaskOutcome::Completed
                }
                Err(e) => TaskOutcome::Failed { error: e.to_string() },
            }
        })
    }
}
```

### API Endpoint for Task Status

A new endpoint exposes supervisor status:

```
GET /api/v1/stone/tasks → Vec<TaskStatus>
```

```json
[
  { "name": "hardware-detection", "state": "completed", "ready": true },
  { "name": "topology-probe", "state": "running", "ready": true },
  { "name": "docker-events", "state": "running", "ready": true },
  { "name": "registry-loader", "state": "completed", "ready": true },
  { "name": "storage-replication", "state": "waiting", "ready": false,
    "waiting_on": ["storage-orchestration"] }
]
```

This makes `rake status` able to show which tasks are running, waiting, or
failed — replacing blind log grepping with structured observability.

## Rationale

- **Single source of truth for task registration.** One vec, one spawn loop.
  Adding a task cannot be forgotten in a second path because there is no
  second path.
- **Declared dependencies replace implicit phase ordering.** The dependency
  graph is inspectable, validated at startup (cycle detection), and
  self-documenting. Reordering lines in the registry vec doesn't break
  anything.
- **Readiness signaling decouples producers from consumers.** A task that
  loads cache can signal ready immediately, then continue probing in the
  background. Dependents don't wait for post-ready work.
- **Standard outcome reporting.** Every task returns `TaskOutcome`. The
  supervisor knows completed vs cancelled vs failed vs panicked. No silent
  deaths.
- **Tracing span per task.** Every log line from a task carries `task=name`
  in its span. `grep topology-probe` catches everything from that task,
  including library code it calls.
- **Coordinator shrinks from 1600 lines to ~100.** Task logic moves to task
  files where it belongs. The coordinator becomes a thin registry builder.

## Consequences

### Positive

- The ARCH-0014 topology probe bug (spawned in one path, missing from the
  other) becomes structurally impossible.
- New tasks require one line in the registry. No hunting for spawn sites.
- Task dependencies are visible in code and at runtime via the `/tasks` API.
- Panics, failures, and cancellations are all reported uniformly.
- Startup debugging changes from "grep logs and hope" to "GET /tasks and
  read the state."

### Negative

- Break-and-rebuild migration: all 32 tasks must be converted to the trait.
  This is a significant diff touching every task file. No incremental path
  — the old coordinator is deleted, not shimmed.
- The `BackgroundTask` trait uses `Pin<Box<dyn Future>>` for the return type
  — one heap allocation per task start. Negligible cost (32 allocations at
  boot) but not zero-cost.
- Dependency cycle detection adds startup validation. A misconfigured
  dependency is a hard boot failure, not a runtime hang. This is intentional
  — fail loud, fail early.

### Neutral

- Conditional tasks (auto-adoption, mDNS listeners) are handled by
  conditional inclusion in the registry vec, not by cfg-gating the trait
  impl. The task struct always exists; the registry decides whether to
  include it.
- Ad-hoc spawns within task bodies (e.g., chirp listener spawning a beacon
  broadcast on discovery) remain as `tokio::spawn` inside the task's `run()`.
  The trait governs the top-level lifecycle, not every internal spawn.

## Migration Plan

### Phase 1: Infrastructure (no task changes)

1. Define `BackgroundTask` trait, `TaskOutcome`, `TaskContext`,
   `DependencyGate`, `ReadySignal` in `tasks/supervisor.rs`.
2. Implement `TaskSupervisor::start()` with dependency graph validation.
3. Add `GET /api/v1/stone/tasks` endpoint.

### Phase 2: Convert All Tasks

Convert each of the 32 supervisor tasks to `BackgroundTask` impls. Each
conversion is mechanical:

1. Create task struct (e.g., `pub struct HealthMonitorTask;`)
2. Move the task body from the coordinator's inline `async move { ... }`
   into the `run()` method.
3. Declare dependencies (read from the implicit phase ordering).
4. Add `ctx.signal_ready()` at the appropriate point.
5. Return `TaskOutcome` instead of `()`.

### Phase 3: Switch Over

1. Replace `start_background_tasks()` with:
   ```rust
   let registry = build_task_registry(&config);
   let supervisor = TaskSupervisor::start(registry, state, token);
   ```
2. Delete `start_all_background_tasks()`.
3. Delete all `start_*` helper functions.
4. Delete Phase 11–17 orchestration blocks.

### Phase 4: Cleanup

1. Remove dead code from coordinator.
2. Update documentation and `CONTEXT.md`.
3. Verify all tasks appear in `GET /api/v1/stone/tasks`.

## References

- [ARCH-0007](ARCH-0007-monomorphic-domain-traits.md) — TaskSupervisor (JoinSet-based) that this ADR replaces
- [ARCH-0014](ARCH-0014-two-tier-hardware-capabilities.md) — topology probe that exposed the dual-path bug
- Implementation: `src/moss/src/tasks/coordinator.rs` (~1600 lines, to be replaced)
- Implementation: `src/moss/src/tasks/supervisor.rs` (existing JoinSet wrapper, to be extended)
