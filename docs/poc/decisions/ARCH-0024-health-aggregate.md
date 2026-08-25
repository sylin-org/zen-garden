---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017, ARCH-0016, ARCH-0018, ARCH-0023]
completed: 2026-04-12
---

# ARCH-0024: Health Aggregate — Per-Offering Health Probing as a Bounded Context

**Date**: 2026-04-12
**Status**: Accepted
**Book**: VII of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Bounded context**: Health

## Context

ARCH-0017 Book VII specifies: "Extract health checking and HTTP/TCP probes
into a Health context with pluggable probe adapters." Chapter 1's discovery
mandate requires re-evaluating this against the actual code.

### Discovery findings (12 findings)

1. **`domain/health.rs` is stone-level system health, not per-offering
   probes.** It contains `check_disk_health()`, `check_memory_health()`,
   `build_disk_component()`, `build_memory_component()`,
   `determine_overall_status()` — all free functions computing the
   `/api/health` endpoint response. These are *not* the target of Book VII.

2. **The real health probe logic lives in `tasks/health_monitor.rs`** — a
   383-line monolith running every 30 seconds. It performs 6 distinct
   concerns: (a) container status polling, (b) offering health/status
   mutation, (c) container reconciliation via `ReconciliationCoordinator`,
   (d) topology mount remediation, (e) notification updates, (f) orphan
   container adoption. Only (a) and (b) are health concerns.

3. **No HTTP/TCP probes exist today.** ARCH-0017 anticipated HTTP and TCP
   probes, but the actual code only checks Docker container health via
   Bollard's `inspect_container` (checking `state.health.status` from the
   Docker daemon's own health check). The `HealthProbe` port should wrap
   the Docker health-check mechanism, not hypothetical HTTP/TCP probes.

4. **Per-offering health state lives inside `Offering.health:
   ServiceHealthStatus`** (common-crate type). There is no separate
   per-offering health map. The health monitor mutates offerings directly
   via `state.offerings.update()`.

5. **`current.health: Arc<RwLock<String>>`** is the stone's overall health
   string (fed to topology chirps via `SelfEntryInputs`). It has 3 read
   sites and 1 write site. This is stone-level health, not per-offering
   health — it stays in `Current` per Book VII's boundary.

6. **Docker events also write health.** `tasks/docker_events.rs` maps
   Docker event actions (`start`, `stop`, `die`, `health_status`) to
   `ServiceHealthStatus` and calls `state.offerings.update()`. This is
   a separate write path for health that the Health aggregate must
   coordinate with.

7. **`ReconciliationCoordinator`** (offering_reconciliation.rs, 433 lines)
   is already well-structured with its own backoff tracker and bounded
   concurrency. It is not a health concern — it is a lifecycle
   self-healing concern. It stays as-is in the task layer.

8. **Port reconciliation and protocol reconciliation** (lines 169–243 of
   health_monitor.rs) fix port/protocol drift between Docker and the
   offering registry. These are offering-lifecycle concerns, not health
   concerns.

9. **Topology mount remediation** (lines 265–316) recreates containers
   missing the shared topology mount. This is an infrastructure concern,
   not a health concern.

10. **Orphan container adoption** (lines 335–376) discovers and adopts
    unregistered zen-offering containers. This is an adoption concern,
    not a health concern.

11. **Notification update** (lines 320–333) sets a "degraded offerings"
    notification tag based on overall health. This is a projection of
    health state and naturally belongs to the Health aggregate as a
    side effect of health determination.

12. **`docker_events.rs` health path** is event-driven (real-time) vs.
    health monitor's poll-based path (30s). Both paths write
    `Offering.health` and `Offering.status`. The Health aggregate needs
    to expose commands that both paths call into, rather than both
    reaching directly into `offerings.update()`.

### Material plan change

ARCH-0017 anticipated HTTP/TCP probe adapters. Reality: no such probes
exist. The `HealthProbe` port wraps Docker container health checking
(the only probe mechanism). HTTP/TCP probe adapters are a future extension
point, not a Book VII deliverable.

The Health aggregate is narrower than anticipated: it owns the
**determination** of per-offering health status and the **events** that
flow from transitions, but it does NOT own the `Offering.health` field
itself (that field stays in the common-crate `Offering` struct). Instead,
the aggregate provides typed commands that the health monitor task and
docker events task call, and those commands delegate the actual mutation
to the Offerings aggregate.

### What does the Health aggregate actually own?

- **Probe scheduling**: when and how often to probe each offering
- **Probe execution**: delegated through a `HealthProbe` port
- **Transition detection**: interesting health transitions (healthy→degraded,
  offline→healthy, etc.)
- **Event emission**: `HealthChanged` events on interesting transitions
- **Notification projection**: setting/clearing the degraded-offerings
  notification tag
- **Stone health derivation**: computing the overall stone health string
  from offering health states (currently in `domain/health.rs` and
  `api/v1/health.rs`)

### What stays outside the aggregate?

- `Offering.health` field (common-crate type, stays on the Offering struct)
- `ReconciliationCoordinator` (task-layer self-healing, already well-structured)
- Port/protocol reconciliation (offering-lifecycle fix-up)
- Topology mount remediation (infrastructure concern)
- Orphan container adoption (adoption domain, not health)
- `current.health: Arc<RwLock<String>>` (stone identity, stays in Current)

## Decision

Extract a `Health` bounded context under `domain/health/` with:

### Module layout

```
src/moss/src/domain/health/
├── mod.rs        # module root + public re-exports
├── aggregate.rs  # Health aggregate root
├── event.rs      # HealthChanged event
├── probe.rs      # HealthProbe port trait
├── system.rs     # Stone-level system health (moved from domain/health.rs)
└── tests.rs      # unit tests
```

### Aggregate shape

```rust
pub struct Health {
    metrics: Arc<Metrics>,
    probe: Arc<dyn HealthProbe>,
    events: broadcast::Sender<HealthChanged>,
    notifications: Arc<NotificationRegistry>,
}
```

The Health aggregate is **stateless** — it does not hold a per-offering
health map. Per-offering health state lives on the `Offering` struct. The
aggregate is a **command facade** that orchestrates the probe→transition→
mutation→event→notification pipeline.

This is the "Ephemeral aggregates" pattern deviation (Book I precedent):
the aggregate holds no `RwLock<State>`, no `finalize` pipeline, and no
`Store` port. It is a coordination point with typed commands, event
emission, and metrics integration.

### Commands

- `probe_offering(offerings: &Arc<Offerings>, name: &str, offering_id: &str)`:
  Execute probe via port, compare result with current health, call
  `offerings.update()` if changed, emit `HealthChanged` event on
  interesting transitions.
- `apply_docker_event(offerings: &Arc<Offerings>, offering_id: &str, new_status: OfferingStatus, new_health: ServiceHealthStatus)`:
  Apply a Docker event's status/health to an offering, emit event on
  interesting transition. Replaces the inline mutation in `docker_events.rs`.
- `update_notification(offerings: &Arc<Offerings>)`:
  Scan all offerings for degraded health, set/clear the notification tag.

### Queries

- `changes() -> broadcast::Receiver<HealthChanged>`:
  Subscribe to health transition events.

### Event

```rust
pub struct HealthChanged {
    pub kind: HealthChangeKind,
    pub offering: String,
    pub old_health: ServiceHealthStatus,
    pub new_health: ServiceHealthStatus,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub enum HealthChangeKind {
    Recovered,  // offline/degraded → healthy
    Degraded,   // healthy → degraded
    Failed,     // any → offline
    Probed,     // any change detected by probe cycle
}
```

### Port

```rust
pub trait HealthProbe: Send + Sync {
    fn probe<'a>(
        &'a self,
        name: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<ProbeResult>> + Send + 'a>>;
}

pub struct ProbeResult {
    pub status: OfferingStatus,
    pub health: ServiceHealthStatus,
}
```

Production adapter: `DockerHealthProbe` wraps `DockerClient::get_service_status`
+ `get_service_health`. Test adapter: `FakeHealthProbe` with configurable
responses.

### Health monitor task refactoring

The 383-line `health_monitor_task` function is refactored to delegate its
health concern to the aggregate while keeping its other concerns inline:

```
Phase 1 (status polling)  → health.probe_offering() for each managed offering
Phase 2 (reconciliation)  → ReconciliationCoordinator (unchanged)
Phase 3 (topo mount)      → inline (unchanged)
Phase 4 (notifications)   → health.update_notification()
Phase 5 (orphan adoption) → inline (unchanged)
```

Port reconciliation and protocol reconciliation move to be gated on
status changes from `probe_offering()` rather than reading the offerings
aggregate directly.

### System health

The existing `domain/health.rs` free functions are moved to
`domain/health/system.rs` to avoid confusion. They continue to serve the
`/api/health` endpoint. No behavioral change.

### Metrics integration

Domain `health` registered with four kinds: `probed`, `recovered`,
`degraded`, `failed`. The register-with-kinds hot-path pattern from
Book I applies.

### Size estimate

~800-1000 lines (smaller than ARCH-0017's ~1300 estimate because the
aggregate is stateless and the non-health concerns stay in the task layer).

## Alternatives considered

### A: Health aggregate owns a per-offering health map

Rejected. Duplicating `Offering.health` into a parallel map creates a
consistency hazard. The `Offering` struct already carries health as a
first-class field used by 18 files. The aggregate should orchestrate
mutations through the Offerings aggregate, not shadow its state.

### B: Move all 6 health_monitor concerns into the Health aggregate

Rejected. Reconciliation, adoption, port fixup, protocol fixup, and
topo mount remediation are not health concerns. Moving them into the
Health aggregate violates SoC and creates an oversized aggregate that
owns infrastructure concerns. These stay in the task layer until their
owning books (VIII, XII) extract them properly.

### C: Split health_monitor.rs into 6 separate tasks

Rejected for Book VII scope. The 30s polling loop is the right shape
for a single coordinated sweep — splitting into 6 tasks adds complexity
for no architectural gain. Book XII (ContainerRuntime) may revisit this.

### D: HTTP/TCP probe adapters now

Rejected. No HTTP/TCP probing exists in the codebase. Building adapters
for hypothetical probes is speculative investment. The `HealthProbe` port
is designed to be extended with HTTP/TCP adapters later (the trait is
generic enough), but Book VII delivers only the Docker adapter.

## Exit criteria

1. `domain/health.rs` (single file) replaced by `domain/health/` module
2. `HealthProbe` port trait with `DockerHealthProbe` adapter
3. `HealthChanged` event emitted on interesting health transitions
4. Health monitor task delegates probe execution through the aggregate
5. Docker events task delegates health mutation through the aggregate
6. System health functions moved to `domain/health/system.rs`
7. Notification update delegated to aggregate command
8. No direct `state.platform.docker.get_service_health()` calls from
   the health monitor or docker events (except through the probe port)
9. `cargo check --all && cargo test --package garden-moss --lib && cargo clippy --package garden-moss --lib -- -D warnings`
10. Metrics domain registered with health kinds

## References

- [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) — epic this book belongs to
- [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md) — Offerings aggregate (health field owner)
- [domain-aggregates.md](../specs/domain-aggregates.md) — pattern spec
- [context-map.md](../reference/context-map.md) — live context inventory
