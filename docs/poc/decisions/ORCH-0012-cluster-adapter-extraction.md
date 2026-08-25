---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-25
---

# ORCH-0012: Cluster Adapter Extraction — Shared Primitives for Stateful Orchestrators

**Date**: 2026-03-25
**Status**: Proposed
**Depends on**: ORCH-0007 (MongoDB Orchestrator), ORCH-0008 (Orchestrator Common)

## Context

The MongoDB orchestrator (ORCH-0007) implements ~4,000 lines of cluster lifecycle
management. Approximately 40% of that logic is database-agnostic: instance tracking,
FQN-keyed logical sets, group lifecycle state machines, membership event detection,
IP drift recovery, health polling cadence, and action queuing with retry.

The original ORCH-0003 proposal envisioned a generic "Database Choreographer." That
abstraction was too ambitious — each database engine has radically different clustering
semantics. However, the operational patterns above are genuinely shared. A future
SQL Server Availability Group orchestrator, PostgreSQL streaming replication manager,
or Redis Sentinel coordinator would all need instance awareness, logical set grouping,
and drift detection.

The Ollama orchestrator validates this: it independently reimplemented instance
tracking and health polling because `orchestrator-common` did not provide those
primitives. The Ollama domain layer (`OllamaInstance`, `InstanceHealth`, demand
tracking) parallels MongoDB's (`MongoInstance`, `InstanceHealth`, membership tracking`)
at the structural level despite having completely different semantics.

## Decision

Extract the database-agnostic cluster management primitives from the MongoDB
orchestrator into a new `orchestrator-common::cluster` submodule. Database-specific
behavior is injected via a `ClusterAdapter` trait.

### Abstraction layers

| Layer | Location | Responsibility |
|-------|----------|----------------|
| **Infrastructure** | `orchestrator-common` (existing) | Discovery, topology, tools stream, gateway, persistence |
| **Cluster primitives** | `orchestrator-common::cluster` (new) | Instance registry, logical sets, lifecycle, membership, drift, actions |
| **Adapter** | Each orchestrator crate | Database-specific probe, bootstrap, add/remove, health interpretation |

### Trait surface

```rust
/// A single instance of a clustered service.
pub trait ClusterInstance: Clone + Send + Sync + 'static {
    fn endpoint(&self) -> &str;
    fn stone_id(&self) -> &str;
    fn stone_name(&self) -> &str;
    fn health(&self) -> InstanceHealth;
}

/// Database-specific operations.
pub trait ClusterAdapter: Send + Sync + 'static {
    type Instance: ClusterInstance;
    type SetState: Send + Sync;
    type Action: Send + Sync;

    /// Probe an instance to classify its cluster status.
    fn probe(&self, instance: &Self::Instance)
        -> impl Future<Output = ProbeResult> + Send;

    /// Bootstrap a new logical set from its first instance.
    fn bootstrap(&self, set_name: &str, instance: &Self::Instance)
        -> impl Future<Output = Result<Self::SetState>> + Send;

    /// Add an instance to an existing set.
    fn add_member(&self, set: &Self::SetState, instance: &Self::Instance)
        -> impl Future<Output = Result<()>> + Send;

    /// Remove an instance from a set.
    fn remove_member(&self, set: &Self::SetState, endpoint: &str)
        -> impl Future<Output = Result<()>> + Send;

    /// Check health of all members in a set.
    fn health_check(&self, set: &Self::SetState)
        -> impl Future<Output = Vec<MemberHealth>> + Send;
}
```

### Generic primitives provided

| Primitive | Replaces (in MongoDB) | What it does |
|-----------|----------------------|-------------|
| `InstanceRegistry<I>` | `app_state.instances` | Endpoint-keyed instance map with upsert, removal, health transitions |
| `LogicalSet<S>` | `app_state.replica_sets` + `groups` | FQN-keyed set membership with phase tracking |
| `SetPhase` | `GroupPhase` | Lifecycle: New → Configuring → Healthy → Degraded → Drifted |
| `MembershipEvent<I>` | `MembershipEvent` | Instance added/removed/role-changed/health-changed |
| `DriftDetector` | `compute_drift_mapping()` | DHCP-aware endpoint reconciliation across set members |
| `ActionQueue<A>` | `pending_actions` | Queued mutations with retry count and persistence |
| `HealthPoller` | conductor.rs 15s loop | Configurable-interval health check driver |

### What stays in adapters

| Concern | Why |
|---------|-----|
| Wire protocol | MongoDB driver vs libpq vs Redis protocol — no common abstraction |
| Bootstrap commands | `rs.initiate()` vs `CREATE SUBSCRIPTION` vs `SENTINEL MONITOR` |
| Health interpretation | `replSetGetStatus` response codes vs `pg_stat_replication` columns |
| Replication metrics | Oplog window vs WAL position vs replication offset |
| Cache/performance tuning | WiredTiger is a MongoDB concept |
| Dashboard API | Each database surfaces different operational concerns |
| CLI commands | `rake policy mongodb clustered` ≠ `rake policy sqlserver ag` |

### Migration path

1. Extract generic types from `mongodb/src/domain/` to `common/src/cluster/`
2. Parameterize MongoDB's `AppState` over `ClusterAdapter`
3. Replace direct `MongoInstance` usage with trait-bounded generics where generic
4. Keep MongoDB-specific domain files intact (oplog, cache_advisor, placement)
5. Validate by implementing a minimal second adapter (e.g., PostgreSQL probe-only)

## Rationale

- **60% code reuse** for new orchestrators — instance tracking, sets, health, drift
  are provided; only the adapter trait needs implementation
- **Consistent operational behavior** across all stateful orchestrators — DHCP drift,
  membership events, degradation semantics are identical
- **Tested once** — generic primitives get property tests; adapters test only
  database-specific behavior
- **No over-abstraction** — dashboard, CLI, wire protocol, and replication metrics
  stay database-specific. The trait surface is minimal (5 methods).

## Consequences

### Positive

- New orchestrators (SQL Server, PostgreSQL, Redis) skip 60% of the lifecycle code
- MongoDB orchestrator loses no functionality — extraction is additive
- `orchestrator-common` gains a cohesive cluster management capability

### Negative

- Generic types add indirection compared to direct `MongoInstance` usage
- Migration requires careful trait-boundary design to avoid leaking generics everywhere
- Risk of premature abstraction if only one adapter ever exists

### Neutral

- Ollama orchestrator is not affected — it has a fundamentally different domain model
  (model placement, not cluster membership). It continues using `orchestrator-common`
  infrastructure directly without the cluster module.

## Implementation phases

| Phase | Scope | Effort |
|-------|-------|--------|
| 1 | Extract types: `InstanceRegistry`, `LogicalSet`, `SetPhase`, `MembershipEvent` | 1-2 sessions |
| 2 | Extract `DriftDetector` and `ActionQueue` | 1 session |
| 3 | Define `ClusterAdapter` trait, refactor MongoDB to implement it | 2-3 sessions |
| 4 | Add `HealthPoller` generic driver | 1 session |
| 5 | Validate with PostgreSQL probe-only adapter (streaming replication status) | 1-2 sessions |
