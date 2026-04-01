---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0020: Domain-Owned State with Watch Snapshots

**Date**: 2026-04-01
**Status**: Accepted
**Supersedes**: The implicit "flat AppState with RwLock" pattern

---

## Problem

The AI orchestrator's `AppState` is a flat struct with 27 fields, 19 of which are
`Arc<RwLock<T>>`. Every API handler and every background task receives the full
`AppState`, acquires individual locks, and contends on shared lock instances.

### Measured Contention

**The cascade**: `upsert_instance()` acquires 9 locks per call:

```
upsert_instance()
  → queue_depths.write()           // WRITE #1
  → instances.read()               // READ  #2 (stale check)
  → queue_depths.write()           // WRITE #3 (evict stale)
  → instances.write()              // WRITE #4 (insert)
  → recompute_tiers()
      → instances.read()           // READ  #5 (re-acquire)
      → tiers.write()              // WRITE #6
  → refresh_recommendations()
      → directory.read()           // READ  #7
      → instances.read()           // READ  #8 (re-acquire again)
      → benchmark_run.read()       // READ  #9
      → config.read()              // READ  #10
      → recommended_models.write() // WRITE #11
  → emit_event()
```

This runs for every discovered instance during topology refresh (every 30 seconds),
potentially 5-10 times in succession.

**The dashboard blocker**: `get_status` acquires 6 read locks simultaneously,
holding all of them while serializing the full JSON response. Any write lock from
discovery blocks the entire dashboard.

**The inference hot path**: `route_model` acquires 5 locks and clones the entire
`ModelDirectory` on every inference request.

**The skill API blocker**: `list_skills` held read locks on `skill_registry` and
`instances` across response building. During skill provisioning (which holds write
locks on `skill_registry`), every `/v1/skills` request blocked for minutes.

### Root Cause

The architecture treats AppState as a shared mutable bag. Domains have no
boundaries — any code can lock any field at any time, creating unpredictable
contention chains. There is no separation between write-side state (domain logic)
and read-side projections (API responses).

---

## Decision

Replace the flat `AppState` with **five domain objects**, each owning its mutable
state privately and publishing immutable snapshots via `tokio::sync::watch`.
API handlers read snapshots — zero locks, zero contention.

### Architecture

```
Writer (discovery, health_check, provisioning)
  │
  ▼
Domain Object (private mutable state)
  │
  ├── mutate() → internal lock, publish snapshot
  │
  └── watch::Sender<Arc<Snapshot>>
                    │
                    ▼
            watch::Receiver
                    │
        ┌───────────┴───────────┐
        │                       │
  API Handler              Reactive Domain
  (.borrow() →             (subscribes to
   zero locks)              changes, derives
                            new state)
```

`watch::Receiver::borrow()` is a single atomic load. No allocation, no cloning,
no lock contention. The only cost is an Arc reference count increment.

### Domain 1: Registry

**Owns**: Instance map, VRAM tiers, queue depth counters.

**Snapshot**:

```rust
#[derive(Clone)]
pub struct RegistrySnapshot {
    pub instances: Arc<HashMap<String, ServiceInstance>>,
    pub tiers: Arc<Vec<Tier>>,
    pub queue_counters: Arc<HashMap<String, Arc<AtomicU32>>>,
}
```

**Interface**:

```rust
pub struct Registry {
    state: RwLock<RegistryState>,       // private
    snapshot: watch::Sender<Arc<RegistrySnapshot>>,
}

impl Registry {
    pub fn snapshot(&self) -> watch::Ref<Arc<RegistrySnapshot>>;
    pub fn subscribe(&self) -> watch::Receiver<Arc<RegistrySnapshot>>;

    pub async fn upsert(&self, instance: ServiceInstance);
    pub async fn remove(&self, endpoint: &str);
    pub async fn set_health(&self, endpoint: &str, health: InstanceHealth);
    pub async fn update_models(&self, endpoint: &str, available: Vec<String>, loaded: Vec<String>);
    pub async fn queue_counter(&self, endpoint: &str) -> Arc<AtomicU32>;
}
```

Tiers are recomputed inside `publish()` — instances and tiers are always
consistent within the same snapshot.

**Writers**: discovery, health_check
**Readers**: every inference handler, dashboard, routing, skill provisioning

### Domain 2: Directory

**Owns**: ModelDirectory (model → capabilities → instances mapping).

**Snapshot**:

```rust
#[derive(Clone)]
pub struct DirectorySnapshot {
    pub directory: Arc<ModelDirectory>,
}
```

**Interface**:

```rust
pub struct Directory {
    state: RwLock<ModelDirectory>,
    snapshot: watch::Sender<Arc<DirectorySnapshot>>,
}

impl Directory {
    pub fn snapshot(&self) -> watch::Ref<Arc<DirectorySnapshot>>;
    pub fn subscribe(&self) -> watch::Receiver<Arc<DirectorySnapshot>>;

    pub async fn upsert(&self, fqn: ModelFqn, caps: Vec<Capability>,
                         specs: Vec<String>, meta: ModelMetadata);
    pub async fn remove_provider(&self, source: &str, locator: &str);
}
```

**Writers**: discovery (enumerate), cloud_sync
**Readers**: routing, dashboard, model form, recommendations

### Domain 3: Intelligence

**Owns**: Recommendations, placement plan, topology advice, lease manager.

**Subscribes to**: Registry changes, Directory changes.

**Runs its own background task** — never blocks any writer.

**Snapshot**:

```rust
#[derive(Clone)]
pub struct IntelligenceSnapshot {
    pub recommendations: Arc<HashMap<String, String>>,
    pub placement: Arc<PlacementPlan>,
    pub advice: Arc<TopologyAdvice>,
}
```

**Interface**:

```rust
pub struct Intelligence {
    snapshot: watch::Sender<Arc<IntelligenceSnapshot>>,
    leases: RwLock<LeaseManager>,  // per-request, short-lived
}

impl Intelligence {
    pub fn snapshot(&self) -> watch::Ref<Arc<IntelligenceSnapshot>>;
    pub async fn acquire_lease(&self, ...) -> ...;
    pub async fn release_lease(&self, ...);
}
```

**Reactive loop**:

```rust
async fn intelligence_loop(
    mut registry_rx: watch::Receiver<Arc<RegistrySnapshot>>,
    mut directory_rx: watch::Receiver<Arc<DirectorySnapshot>>,
    benchmark: Arc<RwLock<BenchmarkRun>>,
    config: Arc<RwLock<OrchestratorConfig>>,
    output: watch::Sender<Arc<IntelligenceSnapshot>>,
) {
    loop {
        tokio::select! {
            _ = registry_rx.changed() => {}
            _ = directory_rx.changed() => {}
        }
        let reg = registry_rx.borrow().clone();
        let dir = directory_rx.borrow().clone();
        let gpu_matrix = benchmark.read().await.gpu_matrix.clone();
        let pins = config.read().await.features.pins.clone();

        let recommendations = compute_all_recommendations(&dir, &reg, &gpu_matrix, &pins);
        let placement = compute_placement(&dir, &reg);
        let advice = compute_advice(&dir, &reg);

        let _ = output.send(Arc::new(IntelligenceSnapshot {
            recommendations: Arc::new(recommendations),
            placement: Arc::new(placement),
            advice: Arc::new(advice),
        }));
    }
}
```

This **eliminates the cascade entirely**. Discovery writes finish instantly.
Intelligence catches up milliseconds later. Recommendations become eventually
consistent — acceptable because they are advisory, not transactional.

### Domain 4: Observability

**Owns**: MetricsEngine, DemandLedger, jobs.

**Input**: `MetricEvent` channel (already mpsc — no change).

**Snapshot**:

```rust
#[derive(Clone)]
pub struct ObservabilitySnapshot {
    pub demand_shares: Arc<HashMap<String, f64>>,
    pub jobs: Arc<Vec<OrchestratorJob>>,
}
```

**Interface**:

```rust
pub struct Observability {
    metrics: RwLock<MetricsEngine>,     // private, written by processor
    demand: RwLock<DemandLedger>,       // private
    jobs: RwLock<VecDeque<OrchestratorJob>>,
    snapshot: watch::Sender<Arc<ObservabilitySnapshot>>,
}

impl Observability {
    pub fn snapshot(&self) -> watch::Ref<Arc<ObservabilitySnapshot>>;

    pub async fn create_job(&self, kind: JobKind) -> String;
    pub async fn complete_job(&self, id: &str);
    pub async fn fail_job(&self, id: &str, error: &str);
}
```

The metrics processor task calls `observability.process_event()` which
updates internal state and periodically publishes a new snapshot.

Routing reads `demand_shares` from the snapshot — never locks MetricsEngine.

### Domain 5: Skills

**Owns**: SkillRegistry, workflow jobs, provisioning state.

**Snapshot**:

```rust
#[derive(Clone)]
pub struct SkillsSnapshot {
    pub skills: Arc<Vec<SkillInfo>>,
    pub workflow_jobs: Arc<HashMap<String, WorkflowJob>>,
}
```

**Interface**:

```rust
pub struct Skills {
    registry: RwLock<SkillRegistry>,
    workflow_jobs: RwLock<HashMap<String, WorkflowJob>>,
    snapshot: watch::Sender<Arc<SkillsSnapshot>>,
}

impl Skills {
    pub fn snapshot(&self) -> watch::Ref<Arc<SkillsSnapshot>>;

    pub async fn register(&self, skill: SkillDefinition);
    pub async fn update_status(&self, name: &str, status: SkillStatus);
    pub async fn submit_job(&self, job: WorkflowJob);
    pub async fn complete_job(&self, id: &str, result: WorkflowJob);
}
```

`SkillsSnapshot.skills` includes per-stone availability pre-computed at
publish time — the API handler does zero computation.

### AppState Becomes a Thin Facade

```rust
#[derive(Clone)]
pub struct AppState {
    // ── Domains ──
    pub registry: Arc<Registry>,
    pub directory: Arc<Directory>,
    pub intelligence: Arc<Intelligence>,
    pub observability: Arc<Observability>,
    pub skills: Arc<Skills>,

    // ── Immutable (set at startup) ──
    pub providers: Arc<ProviderRegistry>,
    pub ollama_client: OllamaClient,
    pub dashboard_port: u16,
    pub koi_endpoint: String,
    pub explicit_stone: Option<String>,
    pub data_dir: String,
    pub start_time: Instant,

    // ── Rarely mutated (user action only) ──
    pub config: Arc<RwLock<OrchestratorConfig>>,
    pub cloud_store: Arc<RwLock<CloudProviderStore>>,

    // ── Channels (already lock-free) ──
    pub dashboard_tx: broadcast::Sender<DashboardEvent>,
    pub metrics_tx: mpsc::UnboundedSender<MetricEvent>,

    // ── Lifecycle ──
    pub shutdown: CancellationToken,

    // ── Tending (rarely mutated) ──
    pub tended_stone: Arc<RwLock<Option<TendedStone>>>,

    // ── Fitness (rarely mutated) ──
    pub benchmark_run: Arc<RwLock<BenchmarkRun>>,
    pub benchmark_cancel: Arc<RwLock<Option<CancellationToken>>>,
}
```

27 fields → 20 fields. 19 RwLocks → 5 (config, cloud_store, tended_stone,
benchmark_run, benchmark_cancel — all rarely written).

The 5 domains contain their own internal locks but never expose them. API
handlers touch zero RwLocks.

---

## Contention Analysis: Before vs After

### Inference Request (the hot path)

**Before** (5 locks + 1 full clone):
```
instances.read()          → may block behind discovery write
directory.read() + clone  → full ModelDirectory clone every request
tiers.read()              → may block behind recompute_tiers write
benchmark_run.read()      → may block behind benchmark write
queue_depths.read()       → brief, low risk
metrics.read()            → may block behind metrics_processor write
```

**After** (0 locks):
```
registry.snapshot().borrow()      → atomic load
directory.snapshot().borrow()     → atomic load
intelligence.snapshot().borrow()  → atomic load (includes recommendations)
observability.snapshot().borrow() → atomic load (includes demand_shares)
```

### Dashboard get_status

**Before** (6 simultaneous read locks held across serialization):
```
capabilities, stones, instances, models, config, jobs, recommendations
→ all locked simultaneously while building JSON response
```

**After** (0 locks):
```
registry.snapshot()       → borrow
directory.snapshot()      → borrow
intelligence.snapshot()   → borrow
observability.snapshot()  → borrow
skills.snapshot()         → borrow
config.read()             → brief (rarely contended)
```

### Discovery upsert_instance

**Before** (9-11 lock acquisitions, cascade):
```
queue_depths.write → instances.read → queue_depths.write → instances.write
→ instances.read → tiers.write → directory.read → instances.read
→ benchmark_run.read → config.read → recommended_models.write
```

**After** (1 internal lock + publish):
```
registry.upsert(instance)
  → internal: state.write() + compute tiers + publish snapshot
  → Intelligence reactively recomputes (separate task)
```

### Skill Provisioning

**Before**: provisioning task holds write locks on `skill_registry` for status
updates, blocking every `/v1/skills` request.

**After**: `skills.update_status()` acquires a brief internal lock, publishes
a new snapshot. API handler reads previous snapshot instantly — never blocks.

---

## Migration Plan

### Phase 1: Registry Domain

Extract `instances`, `tiers`, `queue_depths` into `Registry`.
- Create `src/orchestrators/ai/src/domain/registry.rs`
- `Registry::upsert()`, `remove()`, `set_health()`, `update_models()`
- Internal `publish()` computes tiers + emits snapshot via `watch`
- Update discovery, health_check to call `Registry` methods
- Update `unified.rs`, `proxy.rs` to read from `registry.snapshot()`
- Remove `instances`, `tiers`, `queue_depths` from `AppState`

### Phase 2: Directory Domain

Extract `directory` into `Directory`.
- Create `src/orchestrators/ai/src/domain/directory_domain.rs`
- `Directory::upsert()`, `remove_provider()`
- Update discovery, cloud_sync to call `Directory` methods
- Update routing to read from `directory.snapshot()`

### Phase 3: Intelligence Domain (async recommendations)

Extract `recommended_models`, `placement`, `advisor`, `leases`.
- Create `src/orchestrators/ai/src/domain/intelligence.rs`
- Background task subscribes to Registry + Directory watch channels
- Remove `refresh_recommendations()` from every mutation path
- Update proxy model resolution to read from `intelligence.snapshot()`

### Phase 4: Observability Domain

Extract `metrics`, `demand_ledger`, `jobs`.
- Create `src/orchestrators/ai/src/domain/observability.rs`
- Metrics processor publishes demand_shares snapshot
- Routing reads pre-computed demand_shares
- Job CRUD through `observability.create_job()` etc.

### Phase 5: Skills Domain

Extract `skill_registry`, `workflow_jobs`.
- Create `src/orchestrators/ai/src/domain/skills_domain.rs`
- `Skills::register()`, `update_status()`, `submit_job()`
- Pre-computes per-stone availability in snapshot
- API handler reads `skills.snapshot()` — zero computation

---

## Field Migration Map

| Current AppState Field | Target Domain | Lock Eliminated? |
|----------------------|---------------|-----------------|
| `instances` | Registry | Yes → watch snapshot |
| `tiers` | Registry | Yes → computed in snapshot |
| `queue_depths` | Registry | Partially → map in snapshot, AtomicU32 lock-free |
| `directory` | Directory | Yes → watch snapshot |
| `recommended_models` | Intelligence | Yes → watch snapshot |
| `placement` | Intelligence | Yes → watch snapshot |
| `advisor` | Intelligence | Yes → watch snapshot |
| `leases` | Intelligence | Stays → per-request, short-lived |
| `metrics` | Observability | Yes → demand_shares in snapshot |
| `demand_ledger` | Observability | Yes → internal to domain |
| `jobs` | Observability | Yes → jobs vec in snapshot |
| `skill_registry` | Skills | Yes → watch snapshot |
| `workflow_jobs` | Skills | Yes → watch snapshot |
| `config` | AppState (stays) | Stays → rarely written |
| `cloud_store` | AppState (stays) | Stays → rarely written |
| `benchmark_run` | AppState (stays) | Stays → rarely written |
| `benchmark_cancel` | AppState (stays) | Stays → rarely written |
| `tended_stone` | AppState (stays) | Stays → rarely written |
| `providers` | AppState (stays) | None → immutable |
| `ollama_client` | AppState (stays) | None → stateless |
| `dashboard_tx` | AppState (stays) | None → broadcast is atomic |
| `metrics_tx` | AppState (stays) | None → mpsc is lock-free |
| `shutdown` | AppState (stays) | None → CancellationToken |
| `start_time` | AppState (stays) | None → immutable |
| `data_dir` | AppState (stays) | None → immutable |
| `koi_endpoint` | AppState (stays) | None → immutable |
| `explicit_stone` | AppState (stays) | None → immutable |

**Locks eliminated**: 14 of 19 (74%)
**Remaining locks**: 5 (all rarely written, no contention risk)

---

## Consequences

- API handlers never block on domain mutations. Inference latency becomes
  deterministic — no lock queuing behind discovery or provisioning.
- The 9-lock cascade in `upsert_instance` is eliminated entirely.
- Dashboard `get_status` goes from 6 simultaneous locks to zero.
- Skill provisioning (multi-minute model downloads) never blocks the API.
- Recommendations become eventually consistent (millisecond lag) rather than
  synchronously blocking every mutation.
- Each domain is a black box — methods, events, properties. No external code
  acquires internal locks.
- `watch::borrow()` cost is one atomic load. At 1000 inference requests/second,
  this is negligible compared to the current 5-lock + full-clone overhead.
- Snapshot publication involves one `Arc::new()` + one `clone()` per domain
  change. At worst-case 10 changes/second, this is ~10 allocations/second —
  orders of magnitude cheaper than the current lock contention.
- The migration is incremental — each phase extracts one domain, the rest of
  AppState continues working. No big-bang rewrite.
