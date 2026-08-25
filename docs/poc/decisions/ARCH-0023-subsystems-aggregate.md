---
audience: [developer, ai]
doc_type: decision
status: accepted
completed: 2026-04-12
last_verified: 2026-04-12
canonical: true
---

# ARCH-0023: Subsystems Aggregate — Book VI of ARCH-0017

**Date**: 2026-04-12
**Status**: Accepted
**Book**: VI of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Depends on**: [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) (epic), [ARCH-0018](ARCH-0018-metrics-aggregate.md) (`Arc<Metrics>` injection)

## Context

Book VI extracts the `Subsystems` bounded context. Today subsystem readiness is tracked by a `SubSystems` struct on `AppState` containing two sub-structs (`NetworkSubSystem`, `DockerSubSystem`), each holding a single `pub ready: Arc<AtomicBool>`.

### What the re-evaluation found

1. **Two subsystems exist today.** `NetworkSubSystem` (network stack has a valid LAN IP) and `DockerSubSystem` (Docker daemon is healthy). Both follow the same pattern: a single `Arc<AtomicBool>` flag that starts `false`, is toggled by a background monitor task, and polled by consumer tasks/handlers.

2. **Two producer sites.** `tasks/network_monitor.rs` (`Network::start_with_config` takes `network_ready: Arc<AtomicBool>`, calls `.store()` on IP change) and `tasks/docker/mod.rs` (`DockerMonitor::start_with_config` takes `docker_ready: Arc<AtomicBool>`, calls `.store()` on health change). Both monitors set the flag at construction (initial state) and toggle it in their background task loops.

3. **Six consumer sites — all non-blocking polls.**
   - `tasks/announcer.rs:60` — `subsystems.network.ready.load()` — skip periodic announcements if network not ready
   - `tasks/docker_events.rs:54` — `subsystems.docker.ready.load()` — wait for Docker before subscribing to events
   - `tasks/health_monitor.rs:55` — `subsystems.docker.ready.load()` — skip container health checks
   - `infra/api_helpers.rs:92` — `subsystems.docker.ready.load()` — `require_docker()` API guard (returns 503)
   - `domain/maintenance.rs:218` — `subsystems.docker.ready.load()` — skip Docker sweep
   - `domain/topology/composition.rs:37` — `subsystems.network.ready.load()` — topology self-entry input

4. **No async waiters exist today.** All six consumer sites poll with `load(Ordering::Relaxed)` and continue/skip on false. No task currently blocks awaiting readiness. The aggregate provides `wait_ready()` for future use (e.g., Book X discovery tasks that should block until network is up), but no existing call sites need migration to it.

5. **Bootstrap wiring is straightforward.** `run.rs:448` creates `SubSystems::default()`, passes `.network.ready.clone()` to `Network::start_with_config`, `.docker.ready.clone()` to `DockerMonitor::start_with_config`, and `.clone()` to AppState construction. The aggregate replaces this: monitors receive a reference to the aggregate and call typed commands.

6. **Scope is smaller than ARCH-0017 anticipated.** The epic estimated ~900 lines. Actual blast radius: 2 types to delete (`NetworkSubSystem`, `DockerSubSystem`), 1 struct to delete (`SubSystems`), 2 producer sites to rewire, 6 consumer sites to migrate, 1 testing.rs site. Total estimated: ~500-600 lines including aggregate skeleton, events, tests, and all migrations.

7. **`AtomicBool` exit criterion must be scoped.** `AtomicBool` is used for non-subsystem purposes across moss: `pond_active`, `https`, `enrolled`, topology `dirty` flag. The exit criterion targets only subsystem readiness flags, not all `AtomicBool` usage.

8. **`watch` channel is the right primitive.** `tokio::sync::watch` provides exactly the semantics needed: a single current value that producers update and consumers can poll synchronously (`borrow()`) or await asynchronously (`changed()`). This replaces `Arc<AtomicBool>` with richer semantics (async wait, change notification) while preserving the zero-cost poll path.

### Design

The `Subsystems` aggregate uses a registration pattern (like Metrics) where subsystems are registered by name at bootstrap time. Each registered subsystem gets a `watch::Sender<bool>` (held by the aggregate) and consumers poll via `watch::Receiver<bool>` (obtained through typed queries).

```rust
pub struct Subsystems {
    state: HashMap<String, watch::Sender<bool>>,
    metrics: Arc<Metrics>,
    changes: broadcast::Sender<SubsystemsChanged>,
}
```

**Commands:**
- `register(name)` — register a subsystem (called at bootstrap, panics on duplicate)
- `mark_ready(name)` — transition to ready (fires event on interesting transition only)
- `mark_unready(name, reason)` — transition to not-ready (fires event on interesting transition only)

**Queries:**
- `is_ready(name) -> bool` — synchronous poll (replaces `.ready.load(Ordering::Relaxed)`)
- `wait_ready(name)` — async wait until subsystem is ready (future use)
- `snapshot() -> Vec<SubsystemStatus>` — all subsystems with their current readiness

**Events:**
- `SubsystemsChanged` with kinds `Ready { name }` and `Unready { name, reason }`

**Pattern deviations:**
- **Ephemeral** — no `SubsystemsStore` port, no persistence. Subsystem readiness is runtime-only. Matches Metrics (Book I) and Jobs (Book IV).
- **Infallible mutations** — `mark_ready`/`mark_unready` are no-ops on unknown subsystem names (warn-level trace). No `SubsystemsError` type. Matches Metrics and Jobs.
- **No internal `RwLock`** — state is a plain `HashMap` populated at registration time (before any concurrent access) and never structurally modified afterward. `watch::Sender::send()` is the only mutation and is inherently thread-safe. This is a simplification over the standard `RwLock<State>` pattern.

### Monitor rewiring

Instead of monitors taking `Arc<AtomicBool>` as a constructor parameter, they take `Arc<Subsystems>`. The monitor calls `subsystems.mark_ready("network")` / `subsystems.mark_unready("network", "no valid LAN IP")` instead of `ready.store(true/false, Ordering::Release)`.

Consumer sites replace `state.subsystems.network.ready.load(Ordering::Relaxed)` with `state.subsystems.is_ready("network")`.

The `require_docker()` API helper replaces `state.subsystems.docker.ready.load(Ordering::Relaxed)` with `state.subsystems.is_ready("docker")`.

## Decision

Extract the `Subsystems` bounded context per the design above. Delete `SubSystems`, `NetworkSubSystem`, and `DockerSubSystem` from `app_state.rs`. Rewire both monitor tasks to use typed commands. Migrate all six consumer sites to typed queries.

## Consequences

- Subsystem readiness becomes event-driven rather than flag-polled. Future tasks can `wait_ready()` instead of polling.
- The `SubSystems` struct of `AtomicBool` flags is eliminated — readiness is owned by a proper aggregate with domain events and metrics integration.
- Adding a new subsystem (e.g., "storage", "pond") requires only a `subsystems.register("name")` call at bootstrap and `mark_ready`/`mark_unready` calls from the relevant task — no new struct fields, no new `AtomicBool` wiring.
- Metrics integration records readiness transitions as interesting events.

## Exit criteria

- `rg 'SubSystems|NetworkSubSystem|DockerSubSystem' src/moss/src/` returns 0 matches
- `rg 'subsystems\.\w+\.ready\.(load|store)' src/moss/src/` returns 0 matches
- `rg 'subsystems\.network\.' src/moss/src/` returns 0 matches (outside comments in the aggregate module)
- `rg 'subsystems\.docker\.' src/moss/src/` returns 0 matches (outside comments in the aggregate module)
- Bootstrap ordering preserved (network monitor starts before AppState construction, aggregate constructed first)
- 692+ tests pass
