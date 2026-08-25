---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-14
canonical: true
completed: 2026-04-14
---

# COMPANION-0007: Adapters — Book VI of COMPANION-0001

**Date**: 2026-04-13
**Status**: Completed (2026-04-14)
**Book**: VI of [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)
**Depends on**: [COMPANION-0006](COMPANION-0006-garden-aggregate.md), [COMPANION-0003](COMPANION-0003-pulse.md), [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)

## Context

Book VI lands the extension contract — the `Adapter` trait — and the supervisor that manages adapter lifecycle. This is the last book on COMPANION-0001's critical path; after Book VI, Books VII–IX parallelize freely.

Per the Discovery Mandate in COMPANION-0001, Ch0 validated the trait shape against both target domains:

### Dual-prototype validation

| Scenario | Fits the trait? |
|---|---|
| **RpMatrixAdapter** (hardware, complex): owns serial port, `select!` over events + animation tick + shutdown, `Drop` closes port, brightness is persisted state, subscribes to health/load/service/storage/command kinds with `LatestEvery(33ms)` delivery | ✓ |
| **AudioAdapter** (simple singleton): owns audio sink, `select!` over events + shutdown, factory returns one instance every discovery tick (supervisor dedupes by `info.id`), subscribes to service/tended/storage/command kinds with `Debounced(1s)` delivery | ✓ |

The trait works. No redesign needed before freeze.

### Scope refinements

The pattern spec lists seven cross-cutting concerns the supervisor owns. Book VI ships the **architectural commitment** (concerns live at this layer) plus **core enforcement** (subscription filtering, discover/spawn/reap, grace window). The more elaborate enforcement — rich delivery-policy timers, typed state persistence, HTTP status exposure — is sketched in the types so consumers can declare their intent from day one, but their full enforcement is **deferred to later books or follow-up ADRs** to keep Book VI a bounded landing rather than an epic of its own.

Specifically:

| Concern | Book VI | Deferred |
|---|---|---|
| Subscription filtering | ✓ enforced (supervisor filter task) | — |
| Delivery policy (`All` / `LatestEvery` / `Debounced`) | Types defined; `All` enforced | Timer-driven `LatestEvery` + `Debounced` enforcement — Book VIII if adapters need it, or a post-epic ADR |
| Dependency declaration | ✓ `AdapterFactory::required_dependencies` wired to `ensure_dependencies` | — |
| Grace window (device bounce) | ✓ default 2s; instance replaced after window elapses | — |
| Structured logging span per adapter | ✓ `tracing::info_span!("adapter", kind, id)` on spawn | — |
| Adapter health telemetry | `AdapterStatus` enum + supervisor tracking | HTTP/status exposure — Book VII wires into `CommandTransport` |
| Typed state persistence | Opt-in field on `AdapterProfile`; supervisor no-ops for V1 | Persistence I/O — follow-up ADR once an adapter actually needs it |

This refinement keeps Book VI at ~800-1200 LOC of production code + tests rather than doubling the scope. The **discipline** (cross-cutting concerns live at the supervisor, not in adapters) is preserved; the tenet holds.

### Channel choice

The pattern spec showed `fn run(... events: broadcast::Receiver<Event>, ...)`. After Ch0 validation, the API uses `mpsc::Receiver<Event>` instead:

1. Supervisor owns a filter task per adapter that subscribes to Pulse's broadcast, filters by `AdapterProfile::subscriptions`, and forwards matching events into a per-adapter mpsc channel.
2. Adapter's `run` receives the filtered mpsc.

This makes the subscription-filter boundary explicit: adapters receive only events they asked for, not the full broadcast stream. mpsc also gives better backpressure semantics per-adapter than a shared broadcast.

## Decision

Introduce `Adapter`, `AdapterFactory`, `AdapterProfile`, `AdapterInfo`, `AdapterStatus`, `DeliveryPolicy`, `Adapters` across two new SDK modules: `adapters/` (the bounded context) with its own trait/supervisor split.

### Types

```rust
// src/companion-sdk/src/adapters/adapter.rs
pub trait Adapter: Send + 'static {
    fn info(&self) -> AdapterInfo;
    fn profile(&self) -> AdapterProfile;

    fn run(
        self: Box<Self>,
        events: tokio::sync::mpsc::Receiver<Event>,
        garden: Arc<Garden>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdapterInfo {
    pub kind: &'static str,      // e.g. "firefly.matrix"
    pub id: String,              // unique per instance (port path, serial, "default", ...)
    pub device: Option<String>,  // human-readable device label
}

#[derive(Debug, Clone)]
pub struct AdapterProfile {
    /// Event kinds this adapter consumes. Empty = all (rare; usually a bug).
    pub subscriptions: &'static [&'static str],
    /// How the supervisor paces event delivery.
    pub delivery: DeliveryPolicy,
    /// Whether this adapter opts into typed state persistence (supervisor
    /// will stub this in V1; full I/O lands in a follow-up).
    pub persisted_state: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryPolicy {
    /// Every event delivered. Default.
    All,
    /// Coalesce to latest per kind at this interval. Types only in V1.
    LatestEvery(Duration),
    /// Quiet window after each delivery. Types only in V1.
    Debounced(Duration),
}

// src/companion-sdk/src/adapters/factory.rs
pub trait AdapterFactory: Send + Sync + 'static {
    fn kind(&self) -> &'static str;
    fn required_dependencies(&self) -> &[SystemDependency] { &[] }
    fn discover(&self) -> Vec<Box<dyn Adapter>>;
}

// src/companion-sdk/src/adapters/status.rs
#[derive(Debug, Clone)]
pub enum AdapterStatus {
    Spawning,
    Running { events_handled: u64, last_event_at: Instant },
    Degraded { error: String, since: Instant },
    Stopped,
}

// src/companion-sdk/src/adapters/supervisor.rs
pub struct Adapters {
    factories: Vec<Box<dyn AdapterFactory>>,
    active: Arc<RwLock<HashMap<String, ActiveAdapter>>>,  // keyed by AdapterInfo::id
    garden: Arc<Garden>,
    pulse: Arc<Pulse>,
    discovery_interval: Duration,  // default 5s
    grace_window: Duration,        // default 2s
}

impl Adapters {
    pub fn new(garden: Arc<Garden>, pulse: Arc<Pulse>) -> Self;
    pub fn with_discovery_interval(self, d: Duration) -> Self;
    pub fn with_grace_window(self, d: Duration) -> Self;

    pub fn register<F: AdapterFactory>(&mut self, factory: F);

    /// Run the supervisor — install dependencies, loop discovery, manage
    /// active adapters, until `shutdown` is cancelled.
    pub fn run(&self, shutdown: CancellationToken) -> BoxFuture<'static, ()>;

    pub fn status(&self) -> Vec<(AdapterInfo, AdapterStatus)>;
}
```

### Supervisor loop (simplified pseudocode)

```
install_dependencies()
loop:
    select
        tick every discovery_interval:
            for each factory:
                candidates = factory.discover()
                for c in candidates:
                    if c.info.id not in active:
                        spawn(c)
            for (id, active_adapter) in active:
                if id not in any factory's latest discovery:
                    if now - last_seen > grace_window:
                        reap(id)
        shutdown.cancelled:
            reap_all()
            return

spawn(adapter):
    profile = adapter.profile()
    (tx, rx) = mpsc::channel(64)
    spawn filter_task(pulse.subscribe(), tx, profile.subscriptions)
    span = info_span!("adapter", kind, id)
    handle = tokio::spawn(adapter.run(rx, garden, pulse, shutdown).instrument(span))
    active[id] = ActiveAdapter { info, handle, status, last_seen: now }

filter_task(broadcast_rx, mpsc_tx, subscriptions):
    loop:
        event = broadcast_rx.recv()?
        if event.kind in subscriptions:
            mpsc_tx.send(event)?

reap(id):
    active[id].shutdown.cancel()
    await active[id].handle  (with timeout)
    remove from active
```

### Module layout

```
src/companion-sdk/src/adapters/
├── mod.rs
├── adapter.rs      # Adapter trait, AdapterInfo, AdapterProfile, DeliveryPolicy
├── factory.rs      # AdapterFactory trait
├── status.rs       # AdapterStatus enum
├── supervisor.rs   # Adapters aggregate, ActiveAdapter, filter task
└── tests.rs        # MockAdapter, MockFactory, integration scenarios
```

## Implementation plan

**Chapter 1** (this ADR) — land this document.

**Chapter 2** — traits + status:
- `adapter.rs` / `factory.rs` / `status.rs` with the types above
- Re-exports from `adapters/mod.rs` and prelude
- Unit tests: trait object-safety (`Vec<Box<dyn Adapter>>` + `Vec<Box<dyn AdapterFactory>>`), `AdapterInfo` uniqueness by id, `AdapterProfile` defaults

**Chapter 3** — supervisor + filter task:
- `supervisor.rs` with `Adapters` aggregate, `run`, registration, spawn/reap, filter task, grace window
- `MockAdapter` + `MockFactory` test fixtures
- Integration tests:
  - Register factory → supervisor spawns adapter on first discovery tick
  - Factory returns empty discovery after grace window → supervisor reaps adapter
  - Factory returns the adapter again within grace window → supervisor keeps existing instance alive
  - Subscription filter delivers only matching events
  - Cancellation token ends the supervisor cleanly
  - `status()` reflects Running → Stopped transitions

**Chapter 4** — book close (revision history + any follow-up scaffolds noted).

Each chapter ships green to `dev`.

## Exit criteria

1. `use garden_companion_sdk::{Adapter, AdapterFactory, AdapterProfile, AdapterInfo, AdapterStatus, DeliveryPolicy, Adapters};` compiles.
2. `Vec<Box<dyn Adapter>>` and `Vec<Box<dyn AdapterFactory>>` both work.
3. A `MockFactory` producing a `MockAdapter` registered with `Adapters` spawns within one discovery tick.
4. `Adapters::status()` reports `Running` for the spawned adapter.
5. Removing the mock adapter from the factory's `discover()` results lets the supervisor reap it after the grace window.
6. Subscription filtering delivers only subscribed kinds into the adapter's mpsc.
7. `Adapters::run(shutdown)` exits cleanly on token cancellation; all active adapters' `run` futures complete.
8. `cargo check --all` green.
9. `cargo test --package garden-companion-sdk adapters::` green.
10. `cargo clippy --package garden-companion-sdk -- -D warnings` green.
11. COMPANION-0001 revision history amended.

## Out of scope (deferred)

| Item | Deferred to |
|------|-------------|
| `DeliveryPolicy::LatestEvery` + `Debounced` enforcement (timer tasks) | Book VIII adapter implementations or follow-up ADR |
| Typed state persistence I/O | Follow-up ADR when a real adapter needs it |
| AdapterStatus HTTP exposure | Book VII (CommandTransport gains `/status`) |
| Per-adapter metrics aggregation | Book VII / post-epic |
| Cross-adapter coordination primitives | Out of epic scope |

## Closure notes (2026-04-14)

Book VI closed with all exit criteria met. Critical path through Book VI is complete; Books VII (Companion), VIII (Rebuild), and IX (Integration tests) now parallelize.

Summary:

- **Adapter bounded context** at `src/companion-sdk/src/adapters/` with five files: `adapter.rs`, `factory.rs`, `status.rs`, `supervisor.rs`, `mod.rs`.
- **Trait frozen after Ch0 dual-prototype validation** — matrix (hardware, complex) and audio (singleton) fit without changes.
- **Supervisor** handles discovery loop, filter task (tracing span per adapter + subscription filtering), spawn, grace-window reap, clean shutdown, and AdapterStatus tracking.
- **17 adapter tests** (4 trait + 3 factory + 3 status + 7 supervisor including async integration cases). 133 companion-sdk tests total.
- **Zero new workspace deps** — supervisor uses stdlib + already-direct `tokio-util` and `tracing`.

### Minor refinements during implementation

- **mpsc::Receiver<Event> instead of broadcast::Receiver<Event>** for the adapter's event stream. Declared in the ADR and executed here: supervisor's filter task owns the broadcast subscription and pumps only subscribed kinds into the per-adapter mpsc. Clearer boundary; per-adapter backpressure.
- **Send-safety in tick()**: `RwLockReadGuard` is not Send, so we collect all factory candidates into a Vec before the first await. Documented inline; covered by the fact that the code compiles under the strict `Send` bounds of `tokio::spawn`.
- **Tick cadence**: first tick fires immediately (not after `discovery_interval`), so adapters come up without an initial delay. The built-in interval's "missed tick" behavior is set to `Delay` so late ticks don't burst.
- **Status transitions**: supervisor + filter task both hold `Arc<Mutex<AdapterStatus>>`. Filter task transitions `Spawning → Running` on first forwarded event and increments `events_handled` thereafter. Reap sets `Stopped`. `Degraded` is reserved for a future hook (no auto-transition).

### Deferred work (tracked)

- `DeliveryPolicy::LatestEvery` + `Debounced` enforcement (timer tasks).
- Typed state persistence I/O (the `persisted_state: bool` field is stubbed today).
- `AdapterStatus` HTTP exposure — Book VII's CommandTransport wires `/status`.
- Degraded-state hook for adapters to report their own health.

### Follow-on work picked up by later books

- Book VII (Companion) wires `Adapters::new(garden, pulse)` + `Adapters::run(shutdown)` into the top-level runtime and exposes `/status` via CommandTransport.
- Book VIII implements real `firefly-matrix`, `firefly-oled-v1/v2`, `firefly-tdisplay`, and `cricket-audio` adapters against the frozen trait.
- Book IX provides `MockTransport` + `MockAdapter` integration test harness.

## References

- [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) — the epic
- [COMPANION-0006](COMPANION-0006-garden-aggregate.md) — Garden aggregate (Book V)
- [companion-architecture.md §Adapters context](../specs/companion-architecture.md#adapters-context)
- [companion-architecture.md §Cross-cutting concerns matrix](../specs/companion-architecture.md#cross-cutting-concerns-matrix)
