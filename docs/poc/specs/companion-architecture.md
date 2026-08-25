---
audience: [developer, ai]
doc_type: spec
status: canonical
last_verified: 2026-04-13
---

# Companion Architecture Pattern

**Purpose:** The canonical structure for the companion SDK and every companion crate.
**Audience:** Developers building adapters, transports, or companion consumers; reviewers validating epic books.
**Scope:** `src/companion-sdk/`, `src/firefly/`, `src/cricket/`, and any future companion crate.

> This document is the reference every [COMPANION-0001](../decisions/COMPANION-0001-companion-integration-epic.md) book applies. An adapter, transport, or companion that deviates from this pattern without a documented reason is a bug.

---

## Contents

- [Architecture overview](#architecture-overview)
- [The event envelope](#the-event-envelope)
- [Kind namespace convention](#kind-namespace-convention)
- [Garden context](#garden-context)
- [Adapters context](#adapters-context)
- [Companion top-level](#companion-top-level)
- [Cross-cutting concerns matrix](#cross-cutting-concerns-matrix)
- [Naming conventions](#naming-conventions)
- [Anti-patterns](#anti-patterns)
- [Worked examples](#worked-examples)

---

## Architecture overview

The companion runtime is a **hexagonal / ports-and-adapters** system. Transports bring events into a pure event-driven core (Garden context); adapters carry events out to local effects (device I/O, audio, metric emission, external APIs).

```
┌──────────────────────────────────────────────────────────────┐
│ Companion (binary)                                           │
│                                                              │
│   ┌──────────────────────────┐   ┌────────────────────────┐  │
│   │ Garden context           │   │ Adapters context       │  │
│   │                          │   │                        │  │
│   │  Transport(s) ──┐        │   │  AdapterFactory(s)     │  │
│   │                 │        │   │        │               │  │
│   │                 ▼        │   │        ▼ discover()    │  │
│   │              Pulse       │   │  Adapter instance      │  │
│   │           (orchestrator) │   │  (one per device)      │  │
│   │                 │        │   │        │               │  │
│   │                 ▼        │   │        │ subscribes    │  │
│   │              Garden      │   │        │ to events     │  │
│   │           (aggregate)    │◄──┼────────┘               │  │
│   │                          │   │                        │  │
│   └──────────────────────────┘   └────────────────────────┘  │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

Two bounded contexts. They collaborate through **one** surface: adapters subscribe to `Garden` events and read `Garden` properties. Adapters never touch `Pulse` directly; `Pulse` never knows adapters exist.

---

## The event envelope

Every event in the system has this shape:

```rust
pub struct Event {
    pub id: EventId,                        // GUIDv7
    pub timestamp: DateTime<Utc>,
    pub kind: &'static str,                 // namespaced identifier
    pub payload: Arc<dyn DynPayload>,       // type-erased, downcastable (see note)
}

pub type EventId = uuid::Uuid;              // generated with GUIDv7 for time-ordering

/// User-facing trait. Implement this for your payload types.
pub trait EventPayload: std::any::Any + Send + Sync + std::fmt::Debug {
    /// Stable type identifier. Must match the kind on any `Event` that carries this payload.
    const KIND: &'static str;

    /// State-delta events that should coalesce under rapid bursts set this to true.
    /// Discrete events that must fire once leave it at the default (false).
    const COALESCING: bool = false;

    /// Downcast handle — implementations return `self`.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Runtime accessor for COALESCING. Default impl returns `Self::COALESCING`.
    fn is_coalescing(&self) -> bool { Self::COALESCING }
}

/// Object-safe runtime trait. Auto-implemented for every `EventPayload` via a
/// blanket impl; users never implement this directly.
///
/// The two-trait design exists because Rust's associated-`const` rules make
/// a trait with `const KIND` not object-safe — `dyn EventPayload` would not
/// compile. `DynPayload` exposes the same information through methods,
/// which is object-safe, so `Arc<dyn DynPayload>` works as the payload
/// storage in `Event`.
pub trait DynPayload: std::any::Any + Send + Sync + std::fmt::Debug {
    fn kind(&self) -> &'static str;
    fn is_coalescing(&self) -> bool;
    fn as_any(&self) -> &dyn std::any::Any;
}

impl<T: EventPayload> DynPayload for T {
    fn kind(&self) -> &'static str { T::KIND }
    fn is_coalescing(&self) -> bool { <T as EventPayload>::is_coalescing(self) }
    fn as_any(&self) -> &dyn std::any::Any { <T as EventPayload>::as_any(self) }
}

impl Event {
    pub fn new<P: EventPayload>(payload: P) -> Self;

    pub fn payload<T: EventPayload>(&self) -> Option<&T> {
        if self.kind == T::KIND {
            self.payload.as_any().downcast_ref::<T>()
        } else {
            None
        }
    }

    pub fn is<T: EventPayload>(&self) -> bool {
        self.kind == T::KIND
    }

    /// Fluent dispatch helper — chain `.on::<T>(...)` calls in adapter event loops.
    pub fn on<T: EventPayload>(&self, f: impl FnOnce(&T)) -> &Self {
        if let Some(p) = self.payload::<T>() {
            f(p);
        }
        self
    }
}
```

**On the two-trait shape**: users see only `EventPayload`. The blanket impl of `DynPayload` for every `EventPayload` is invisible at call sites — `Event::new(my_payload)` works for any type implementing `EventPayload`, and `event.payload::<T>()` works for any `T: EventPayload`. The `DynPayload` trait is a purely internal mechanism that surfaces only when inspecting the concrete type of `Event::payload`. This was a Book I discovery documented in [COMPANION-0002](../decisions/COMPANION-0002-event-envelope.md).

### Concrete payloads

Core payloads live in `garden-companion-sdk::event::core`. Companion-scoped payloads live in each companion's crate under `<companion>::event`. Third-party adapter payloads live with the adapter.

```rust
// SDK core payload — garden-companion-sdk::event::core
#[derive(Debug, Clone)]
pub struct HealthChanged {
    pub from: garden_common::domain::Health,
    pub to: garden_common::domain::Health,
}
impl EventPayload for HealthChanged {
    const KIND: &'static str = "core.stone.health.changed";
}

// SDK core payload — coalesces under rapid bursts
#[derive(Debug, Clone)]
pub struct LoadUpdated;
impl EventPayload for LoadUpdated {
    const KIND: &'static str = "core.stone.load.updated";
    const COALESCING: bool = true;
}

// Companion-scoped command payload (firefly)
#[derive(Debug, Clone)]
pub struct SetBrightness {
    pub level: u8,
}
impl EventPayload for SetBrightness {
    const KIND: &'static str = "firefly.command.brightness";
}
```

---

## Kind namespace convention

Kinds are namespaced strings with a strict grammar:

```
kind := <namespace> "." <domain> "." <event>
     |  <namespace> "." <domain> "." <subject> "." <event>

namespace := "core" | <companion-name>
domain    := <lowercase-word>
event     := <lowercase-word> ("." <lowercase-word>)*
```

### Reserved prefixes

| Prefix | Owner | Examples |
|---|---|---|
| `core.stone.*` | SDK core | `core.stone.health.changed`, `core.stone.load.updated`, `core.stone.tended` |
| `core.service.*` | SDK core | `core.service.started`, `core.service.stopped` |
| `core.storage.*` | SDK core | `core.storage.connected`, `core.storage.removed` |
| `core.command.*` | SDK core | `core.command.result` (used by `CommandTransport` for correlation) |
| `firefly.*` | firefly crate | `firefly.command.brightness`, `firefly.matrix.frame.rendered` |
| `cricket.*` | cricket crate | `cricket.command.play`, `cricket.tune.selected` |

Third-party adapters use their crate name as the namespace (e.g., `prometheus-exporter` would use `prometheus.*`).

### Validation

`Pulse::ingest()` rejects events whose kind doesn't match a registered namespace. Namespace registration happens at `Companion` construction via `companion.register_namespace("firefly")`. This catches typos and unintended third-party emissions into reserved namespaces.

---

## Garden context

### Responsibilities

- Ingest events from transports
- Canonicalize (dedup / validate / coalesce)
- Project event stream into read-model aggregate state
- Fan out events to subscribers

### Module layout

```
src/companion-sdk/src/garden/
├── mod.rs              # module root, re-exports
├── event.rs            # Event, EventPayload, EventId
├── core_payloads.rs    # HealthChanged, LoadUpdated, ServiceStarted, ...
├── pulse.rs            # Pulse orchestrator
├── garden.rs           # Garden aggregate + GardenState projection
├── transport.rs        # Transport trait
└── tests.rs
```

### `Pulse` — the orchestrator

Pulse owns the canonicalization pipeline. Every event enters through `ingest()`; every subscriber reads from the same fan-out channel.

```rust
pub struct Pulse {
    seen: Arc<Mutex<LruCache<EventId, ()>>>,     // dedup window
    pending_coalesce: Arc<DashMap<&'static str, Event>>,
    outbound: broadcast::Sender<Event>,
    registered_namespaces: Arc<RwLock<HashSet<&'static str>>>,
    metrics: Arc<PulseMetrics>,
}

impl Pulse {
    pub fn new(capacity: usize) -> Self { ... }

    pub fn register_namespace(&self, ns: &'static str) { ... }

    /// The single fan-in point.
    pub fn ingest(&self, event: Event) -> IngestResult { ... }

    /// Drain coalesced events — called on a timer (configurable, default 50ms).
    pub fn flush_coalesced(&self) { ... }

    /// Subscribe to the canonical event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> { ... }

    pub fn metrics(&self) -> PulseMetrics { ... }
}

pub enum IngestResult {
    Accepted,
    Duplicate,
    Coalescing,
    Rejected(RejectReason),
}

pub enum RejectReason {
    UnregisteredNamespace,
    KindPayloadMismatch,
}
```

### `Garden` — the read-model aggregate

Garden exposes **properties** (current state, always up-to-date) and an **event stream** (typed domain events). Subscribers choose which they need.

```rust
pub struct Garden {
    state: Arc<RwLock<GardenState>>,
    pulse: Arc<Pulse>,
    _projection_task: tokio::task::JoinHandle<()>,
}

impl Garden {
    // --- Properties (synchronous reads of current state) ---
    pub fn stone(&self) -> StoneView { ... }
    pub fn health(&self) -> Health { ... }
    pub fn load(&self) -> Load { ... }
    pub fn offerings(&self) -> OfferingsView { ... }
    pub fn seed_bank(&self) -> Option<SeedBankView> { ... }
    pub fn pond(&self) -> PondView { ... }
    pub fn is_ready(&self) -> bool { ... }

    // --- Event stream ---
    pub fn events(&self) -> broadcast::Receiver<Event> {
        // Returns a receiver that sees a synthetic GardenSnapshot event first,
        // then the live stream. The snapshot carries the current state so new
        // subscribers (adapters) can hydrate without special-casing init.
        ...
    }
}
```

### `Transport` trait

A transport is an event source or sink. It runs as a task, reads/writes from some external source, and publishes events to `Pulse`. Adapters never interact with transports directly.

```rust
pub trait Transport: Send + 'static {
    /// Run until shutdown. The transport publishes events into the provided Pulse
    /// and (optionally) observes `core.command.result` events for response correlation.
    fn run(
        self: Box<Self>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()>;
}
```

**Initial implementations:**

- `SseTransport` — connects to moss `/presence/stream`, deserializes raw events, publishes to Pulse
- `CommandTransport` — HTTP server; translates `POST /command` into command events, correlates `core.command.result` events back into HTTP responses

---

## Adapters context

### Responsibilities

- Define the extension contract (`Adapter` trait)
- Register adapter factories (`AdapterFactory`)
- Discover physical devices / logical endpoints
- Spawn / reap adapter instances
- Apply cross-cutting concerns (filtering, delivery policy, hydration, logging, dependencies, grace windows, persistence)

### Module layout

```
src/companion-sdk/src/adapters/
├── mod.rs              # re-exports
├── adapter.rs          # Adapter trait, AdapterInfo, AdapterProfile
├── factory.rs          # AdapterFactory trait
├── supervisor.rs       # Adapters aggregate (registry + lifecycle)
├── state.rs            # AdapterStatus, typed per-adapter persistence
└── tests.rs
```

### `Adapter` trait

```rust
pub trait Adapter: Send + 'static {
    fn info(&self) -> AdapterInfo;

    /// Declares subscriptions, delivery policy, required dependencies.
    /// Called once at spawn; values are baked into the supervisor's dispatch.
    fn profile(&self) -> AdapterProfile;

    /// Run until shutdown or device loss. The supervisor has already filtered
    /// `events` per `self.profile()` subscriptions and applied delivery policy.
    fn run(
        self: Box<Self>,
        events: broadcast::Receiver<Event>,
        garden: Arc<Garden>,
        pulse: Arc<Pulse>,           // for emitting events back (command results, telemetry)
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()>;
}

pub struct AdapterInfo {
    pub kind: &'static str,           // e.g., "firefly.matrix"
    pub id: String,                   // unique per instance — e.g., serial number, port path
    pub device: Option<String>,       // human-readable device name
}

pub struct AdapterProfile {
    pub subscriptions: &'static [&'static str],
    pub delivery: DeliveryPolicy,
    pub persisted_state: bool,         // opt into typed state persistence
}

pub enum DeliveryPolicy {
    All,
    LatestEvery(Duration),             // coalesce — deliver newest at interval
    Debounced(Duration),               // quiet window after each delivery
}
```

### `AdapterFactory` trait

Factories produce adapter instances for detected devices/endpoints. The supervisor calls `discover()` periodically.

```rust
pub trait AdapterFactory: Send + Sync + 'static {
    fn kind(&self) -> &'static str;

    /// Dependencies checked/installed before any instance from this factory is spawned.
    fn required_dependencies(&self) -> &[SystemDependency] { &[] }

    /// Return adapter instances for currently-present devices/endpoints.
    /// Called periodically by the supervisor. Stateless — the supervisor tracks
    /// which AdapterInfo::id values are already running.
    fn discover(&self) -> Vec<Box<dyn Adapter>>;
}
```

### `Adapters` supervisor

```rust
pub struct Adapters {
    factories: Vec<Box<dyn AdapterFactory>>,
    active: Arc<DashMap<String, ActiveAdapter>>,  // keyed by AdapterInfo::id
    garden: Arc<Garden>,
    pulse: Arc<Pulse>,
    metrics: Arc<AdaptersMetrics>,
    grace_window: Duration,
}

struct ActiveAdapter {
    info: AdapterInfo,
    handle: tokio::task::JoinHandle<()>,
    shutdown: CancellationToken,
    status: Arc<RwLock<AdapterStatus>>,
    last_seen: Instant,
}

pub enum AdapterStatus {
    Spawning,
    Running { events_handled: u64, last_event_at: Instant },
    Degraded { error: String, since: Instant },
    Stopped,
}
```

The supervisor loop:

1. **Install dependencies** (once per factory, at `Companion::run()`).
2. **Discovery tick** (periodic, e.g. every 5s):
   - For each factory, call `discover()` → new adapter candidates
   - For each candidate whose `info.id` is not in `active`: spawn
   - For each active adapter whose `info.id` is no longer in any factory's discovery: enter grace window; if not reappearing, reap
3. **Spawn**:
   - Create `broadcast::Receiver<Event>` from `pulse.subscribe()`
   - Wrap in a filter that applies `profile.subscriptions` and `profile.delivery`
   - Wrap the adapter task in a `tracing::Span` with `kind`, `id`
   - Spawn task: `adapter.run(filtered_events, garden.clone(), pulse.clone(), shutdown)`
4. **Reap** (on disconnect or shutdown):
   - Cancel shutdown token (signals `run()` to return)
   - `JoinHandle::await` with timeout
   - Adapter instance drops → RAII cleanup runs

---

## Companion top-level

```rust
pub struct Companion {
    config: CompanionConfig,
    pulse: Arc<Pulse>,
    garden: Arc<Garden>,
    adapters: Adapters,
    transports: Vec<Box<dyn Transport>>,
    enabled: Arc<AtomicBool>,            // absorbs CompanionState on/off persistence
    shutdown: CancellationToken,
}

impl Companion {
    pub fn new(config: CompanionConfig) -> Self { ... }

    pub fn with_transport<T: Transport + 'static>(mut self, t: T) -> Self { ... }

    pub fn with_adapter_factory<F: AdapterFactory + 'static>(mut self, f: F) -> Self { ... }

    pub async fn run(self) -> anyhow::Result<()> {
        // 1. Register namespaces, install dependencies
        // 2. Spawn transports
        // 3. Start Garden projection task
        // 4. Run Adapters supervisor
        // 5. Wait for shutdown signal (Ctrl+C / SIGTERM / POST /shutdown)
        // 6. Graceful shutdown: cancel token, await all tasks
        ...
    }
}
```

**Firefly's entire `main.rs`:**

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = CompanionConfig::parse_with(build_manifest());

    Companion::new(config)
        .with_transport(SseTransport::new())
        .with_transport(CommandTransport::new())
        .with_adapter_factory(RpMatrixFactory)
        .with_adapter_factory(OledV1Factory)
        .with_adapter_factory(OledV2Factory)
        .with_adapter_factory(TDisplayFactory)
        .run()
        .await
}
```

**Cricket's entire `main.rs`:**

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = CompanionConfig::parse_with(build_manifest());

    Companion::new(config)
        .with_transport(SseTransport::new())
        .with_transport(CommandTransport::new())
        .with_adapter_factory(AudioFactory)
        .run()
        .await
}
```

---

## Cross-cutting concerns matrix

The organizing principle of the architecture: every concern lives at the single layer that can own it canonically. Adapters stay small by design.

| Concern | Lives at | Rationale |
|---|---|---|
| Event uniqueness (dedup) | Pulse | Orchestrator sees all ingestions; bounded cache is canonical |
| Event validation | Pulse | Single boundary; rejects at the gate |
| State-delta coalescing | Pulse (per-kind) + AdapterProfile (per-adapter) | Two-layer: global reduces fan-out cost; per-adapter matches render cadence |
| Ordering (temporal) | Pulse | GUIDv7 gives total order; orchestrator preserves it |
| Backpressure policy | Pulse (global) + AdapterProfile (per-adapter) | Broadcast lag semantics + declared drop policy |
| Event subscription filtering | Adapters supervisor | One wrapper per adapter; avoids wakeups on unrelated kinds |
| Delivery policy (coalesce / debounce / all) | Adapters supervisor | Enforces `AdapterProfile::delivery` before `Adapter::run` sees events |
| Initial state hydration | Garden (synthetic `GardenSnapshot`) | First event every adapter sees; unifies init and recovery |
| Command-response correlation | CommandTransport | One correlation map in one place; adapters never see HTTP |
| Command routing to specific adapter kinds | Namespace-based subscription | `firefly.command.*` reaches only adapters subscribing to that namespace |
| Structured logging context | Adapters supervisor | `tracing::Span` wraps adapter task; all inner logs inherit fields |
| Dependency installation | Adapters supervisor (via `AdapterFactory::required_dependencies`) | Centralized, pre-spawn verification |
| Graceful cleanup | RAII (Adapter's `Drop`) | Idiomatic Rust; no ordering ambiguity |
| Health telemetry | Adapters supervisor | Derived from event-delivery instrumentation; no adapter code |
| Device bounce (transient disconnect) | Adapters supervisor grace window | Centralized tolerance; no adapter reinvents the wheel |
| Per-adapter persisted state | Adapters supervisor (via `AdapterProfile::persisted_state`) | Typed serde at `{state_dir}/adapters/{kind}/{id}.json` |
| Domain projection (events → state) | Garden projection task | Single place where wire → domain happens |
| Device protocol (serial, audio, etc.) | **Adapter** | Irreducibly device-specific — belongs here |
| Rendering strategy (pixels, audio samples) | **Adapter** | Irreducibly device-specific |
| Device-specific tuning (baud rates, stabilization) | **Adapter** | Irreducibly device-specific |
| Animation FSMs | **Adapter** | Device capability-dependent |

---

## Naming conventions

Per moss's code-standards §3 (type names name the concept, not the architectural role):

### Forbidden suffixes

Do not use `Manager`, `Handler`, `Service`, `Context`, `Runtime` as type suffixes. The concept-name is enough.

- ❌ `CompanionRuntime` → ✅ `Companion`
- ❌ `AdapterManager` → ✅ `Adapters` (the aggregate)
- ❌ `EventHandlerService` → ✅ direct `Adapter::run` with event receiver
- ❌ `TransportContext` → ✅ `Transport`

### Type-information-in-name

Do not encode types into field names. The type says what it is.

- ❌ `tick_tx: broadcast::Sender<Tick>` → ✅ `tick: broadcast::Sender<Tick>`
- ❌ `events_rx: broadcast::Receiver<Event>` → ✅ `events: broadcast::Receiver<Event>`

### Namespace-in-field

Underscores in field names are missing structs. Extract them.

- ❌ `adapter_supervisor_shutdown: CancellationToken`
- ✅ `self.supervisor.shutdown` (via nested struct)

### Event payload struct naming

Event payload structs are the **noun form of what happened**, not verbs or suffixes.

- ✅ `HealthChanged`, `ServiceStarted`, `Tended`, `StorageConnected`
- ❌ `HealthChangeEvent`, `ServiceStartedEvent`, `TendedPayload`

The `KIND` const carries the namespaced identifier; the struct name carries the domain concept.

---

## Anti-patterns

### ❌ Shared-mutex device port

```rust
// WRONG — the bug this architecture exists to prevent
pub struct FireflyConnection {
    serial: Mutex<Option<FireflySerial>>,
}
pub fn with_device<F, T>(&self, f: F) -> Result<T> { /* holds outer lock during I/O */ }
```

**Why**: one I/O call wedges the entire pipeline via the outer lock. This is what caused the replug deadlock fixed immediately before COMPANION-0001 was accepted.

**Right**: adapter owns its own port privately. No sharing. Adapter's `Drop` closes it. Supervisor respawns a fresh instance if the device reappears.

### ❌ Device-type dispatch outside adapter code

```rust
// WRONG — scattered conditional dispatch
if device_type == FireflyDeviceType::Esp8266Oled {
    self.send_oled_snapshot(&snapshot);
} else if device_type == FireflyDeviceType::Esp8266OledV2 {
    self.send_oled_v2_snapshot(&snapshot);
} else if device_type == FireflyDeviceType::Esp32TDisplay {
    tdisplay::send_snapshot(&self.connection, &snapshot);
}
```

**Why**: every device variant touches every dispatch site. Adding a new variant is surgery across the codebase.

**Right**: each variant is its own `Adapter` implementation. The trait dispatches via Rust's vtable, not branches in shared code.

### ❌ Event handler at SDK level, not adapter level

```rust
// WRONG — single EventHandler reaching into multiple subsystems
pub struct FireflyEvents {
    context: Arc<RwLock<Animation>>,        // shared state
    connection: Arc<FireflyConnection>,      // shared port
    state: Arc<CompanionState>,              // shared flag
}
```

**Why**: conflates per-device event handling with cross-device shared state. Every adapter contaminates every other.

**Right**: each adapter has its own event loop inside `Adapter::run`. State is adapter-local; shared state is read from `Garden` properties.

### ❌ Commands via a parallel pathway

```rust
// WRONG — commands and events live in different abstractions
trait CommandHandler { async fn handle(&self, args: &[String]) -> CommandResponse; }
trait EventHandler   { async fn on_event(&self, event: SseEvent); }
```

**Why**: every companion reimplements command parsing, validation, and dispatch. HTTP concerns leak into adapters.

**Right**: commands are events with `kind = "<companion>.command.<action>"`. `CommandTransport` translates HTTP to events and correlates responses. Adapters subscribe to command kinds like any other.

### ❌ Private domain types

```rust
// WRONG — firefly defining its own presence structs
// (literally in src/firefly/src/events.rs today)
#[derive(Deserialize)]
pub struct PresenceSnapshot {
    pub stone: StoneState,
    pub offerings: Vec<OfferingState>,
}
```

**Why**: schema drift on moss's side silently breaks cricket (string keys) or compile-breaks firefly with no single source of truth.

**Right**: domain types live in `garden-common::domain` and are shared by moss and SDK. Wire types stay in `garden-common::presence`; the SDK converts wire → domain once at the transport boundary.

### ❌ Scaffolding without a tracker entry

```rust
// WRONG — untracked intermediate state
// TODO: migrate to new adapter pattern
pub struct OldEventHandler { ... }
```

**Why**: untracked scaffolds become permanent. See ARCH-0017 §"Scaffolding Contract".

**Right**: under the break-and-rebuild tenet, scaffolds should be rare. If one is necessary, log it in [scaffolding.md](../scaffolding.md) with a `companion-*` ID and a removal trigger.

---

## Worked examples

### Example 1: Adding a new firefly device variant

Under the old pattern, adding a device variant required:
- New enum variant in `FireflyDeviceType`
- Update `from_vid` and `refine_from_info`
- Update stabilize_ms match
- Update sort_by_key
- Add `if device_type == NewVariant` branches in events.rs (~5 sites)
- Add reconnect branch in main.rs
- Add test mode case
- Add Display impl arm

Under the new pattern:
- Write `NewVariantAdapter: Adapter`
- Write `NewVariantFactory: AdapterFactory`
- Add `.with_adapter_factory(NewVariantFactory)` in firefly's main.rs

Two files touched instead of ten.

### Example 2: Adding a Prometheus exporter adapter

The `PrometheusAdapter` subscribes to all `core.*` events, maintains counters, exposes `:9090/metrics`. It has no hardware and is not a device variant — but under the architecture, it's the **same operation** as adding a device variant:

```rust
struct PrometheusAdapter { /* counters, HTTP server */ }
impl Adapter for PrometheusAdapter {
    fn info(&self) -> AdapterInfo { ... }
    fn profile(&self) -> AdapterProfile {
        AdapterProfile {
            subscriptions: &["core.stone.health.changed", "core.stone.load.updated", ...],
            delivery: DeliveryPolicy::All,
            persisted_state: false,
        }
    }
    fn run(self: Box<Self>, events: broadcast::Receiver<Event>, ...) -> BoxFuture<'static, ()> {
        // listen, increment counters, serve /metrics
    }
}

struct PrometheusFactory;
impl AdapterFactory for PrometheusFactory {
    fn kind(&self) -> &'static str { "observability.prometheus" }
    fn discover(&self) -> Vec<Box<dyn Adapter>> {
        vec![Box::new(PrometheusAdapter::new())]  // always one
    }
}
```

Register it in any companion's main.rs — same line as `RpMatrixFactory`. No SDK modifications.

### Example 3: Matrix adapter's event loop

```rust
impl Adapter for RpMatrixAdapter {
    fn info(&self) -> AdapterInfo { ... }

    fn profile(&self) -> AdapterProfile {
        AdapterProfile {
            subscriptions: &[
                "core.stone.health.changed",
                "core.stone.tended",
                "core.service.started",
                "core.service.stopped",
                "core.storage.connected",
                "core.storage.removed",
                "firefly.command.brightness",
                "firefly.command.clear",
            ],
            delivery: DeliveryPolicy::LatestEvery(Duration::from_millis(33)),  // 30fps
            persisted_state: true,  // brightness survives restart
        }
    }

    fn run(
        self: Box<Self>,
        mut events: broadcast::Receiver<Event>,
        garden: Arc<Garden>,
        _pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            // Note: events contains GardenSnapshot as the first event (synthetic).
            // Animation loop and event handling interleave.
            loop {
                tokio::select! {
                    Ok(event) = events.recv() => {
                        event
                            .on::<GardenSnapshot>(|s| self.hydrate(s))
                            .on::<HealthChanged>(|h| self.on_health(h.to))
                            .on::<Tended>(|_| self.play_sparkle())
                            .on::<ServiceStarted>(|_| self.trigger_bloom())
                            .on::<SetBrightness>(|b| self.set_brightness(b.level));
                    }
                    _ = self.animation_tick() => {
                        self.render_frame(&garden);  // reads current state from Garden
                    }
                    _ = shutdown.cancelled() => break,
                }
            }
            // Drop impl clears the matrix.
        })
    }
}
```

No `device_type`, no shared state, no connection mutex. The adapter owns its port, its animation engine, its rendering. The trait does nothing device-specific.

---

## References

- [COMPANION-0001](../decisions/COMPANION-0001-companion-integration-epic.md) — the epic this spec supports
- [domain-aggregates.md](domain-aggregates.md) — the moss pattern this mirrors (read-write aggregates)
- [ARCH-0017](../decisions/ARCH-0017-ddd-monolith-epic.md) — precedent for multi-book pattern-enforcement epics
- [glossary.md](../glossary.md) — ubiquitous language
- [scaffolding.md](../scaffolding.md) — scaffolding tracker (shared with ARCH-0017)
