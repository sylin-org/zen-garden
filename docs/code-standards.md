# Zen Garden — Rust Code Standards

> Authoritative standard for all Rust code in this project.
> Principle: **the compiler understands the domain model, not just human readers**.
> If structure only exists in a name string, it does not exist architecturally.

---

## 1. Namespaces over prefixes

If a field name contains an underscore that encodes a sub-concept, that is a missing struct.

```rust
// Bad — underscore doing namespace work
storage_tick_tx
storage_changed_tx
orchestration_nudge
pond_active
pond_ceremony_host

// Good — namespace is a type
storage.orchestration.tick
storage.replication.changed
storage.orchestration.nudge
security.pond.active
security.pond.ceremony.host
```

**Rule**: every underscore in a field name is a namespace waiting to become a struct.

Apply the rule at every level without exception:

```rust
// Bad
pond_ceremony_host: Arc<CeremonyHost>

// Good — ceremony is a sub-domain of pond
pub struct Pond {
    pub ceremony: Ceremony,
    pub active:   Arc<AtomicBool>,
    pub started:  Arc<AtomicBool>,
    pub client:   Arc<StoneClient>,
}

pub struct Ceremony {
    pub host:     Arc<CeremonyHost>,
    pub registry: Arc<CeremonyRegistry>,
    pub journal:  Arc<CeremonyJournal>,
}
```

---

## 2. Types carry type information; names carry domain meaning

Never duplicate what the type already says.

```rust
// Bad — type duplicated in name
tick_tx:      broadcast::Sender<StorageTick>
https_started: AtomicBool
capabilities_arc: Arc<RwLock<Option<HardwareCapabilities>>>

// Good — name is the concept
tick:         broadcast::Sender<StorageTick>
started:      AtomicBool
capabilities: Arc<RwLock<Option<HardwareCapabilities>>>
```

This applies to suffixes that encode architectural role (`_manager`, `_handler`, `_service`, `_cache`, `_flag`) and type metadata (`_arc`, `_clone`, `_ref`).

---

## 3. Type names name the concept, not the architectural role

Suffixes like `Context`, `Manager`, `Handler`, `Service` describe what a type *does architecturally*, not what it *is in the domain*. Omit them.

```rust
// Bad — suffix adds no domain information
pub struct StorageContext { ... }
pub struct SecurityContext { ... }
pub struct PondState { ... }

// Good — position in the hierarchy is the context
pub struct Storage { ... }
pub struct Security { ... }
pub struct Pond { ... }
```

At the call site: `state.storage.orchestration.nudge` — `Storage` is unambiguous because of its position. No suffix needed.

---

## 4. Channel endpoint conventions

**Destructuring** (2-line lifetime before moving each end): use `tx`/`rx` — universal shorthand, universally understood.

```rust
let (tx, rx) = broadcast::channel(512);
```

**Struct fields** (sender stored long-term): name the concept. The type declares the direction.

```rust
pub struct Orchestration {
    pub tick:    broadcast::Sender<StorageTick>,   // not tick_tx
    pub changed: broadcast::Sender<StorageChanged>, // not changed_tx
}
```

**Local subscribers** (receiver created at use site): name the purpose at the consumer, not the type.

```rust
// Bad — echoes the type
let tick_rx = state.storage.orchestration.tick.subscribe();

// Good — names the consumer's intent
let replication_feed = state.storage.orchestration.tick.subscribe();
let sse_feed         = state.storage.orchestration.tick.subscribe();
```

---

## 5. Domain ownership through struct nesting

`AppState` is a facade. Each domain owns its fields. Struct nesting makes domain boundaries visible to the compiler.

```rust
#[derive(Clone)]
pub struct AppState {
    // Cross-cutting — no single domain owner
    pub shutdown_token: CancellationToken,
    pub event_bus:      EventBus,
    pub console:        Arc<ConsolePrinter>,
    pub pulse_tx:       broadcast::Sender<PulseEvent>,

    // Domain contexts — each domain owns its state
    pub identity:   Arc<Identity>,
    pub infra:      Arc<Infra>,
    pub discovery:  Arc<Discovery>,
    pub security:   Arc<Security>,
    pub offerings:  Arc<Offerings>,
    pub storage:    Arc<Storage>,
    pub presence:   Arc<Presence>,
    pub companions: Arc<Companions>,
}
```

A field that crosses domain boundaries in its name (e.g. `storage_orchestration_nudge` in a flat struct) signals that the domain boundary is missing from the type system.

---

## 6. Handler dependency declaration via `FromRef`

Handlers declare only the dependencies they actually need. Axum's `FromRef` enforces this at the type level.

```rust
impl FromRef<AppState> for Arc<Storage> {
    fn from_ref(state: &AppState) -> Self { state.storage.clone() }
}

// Handler's dependency surface is explicit and compiler-enforced
async fn list_volumes(State(storage): State<Arc<Storage>>) -> impl IntoResponse { ... }

// Not this — implicit dependency on everything
async fn list_volumes(State(state): State<AppState>) -> impl IntoResponse { ... }
```

---

## 7. Newtypes over primitives for domain identifiers

```rust
// Bad — any String is accepted; transpositions are runtime bugs
fn register(id: String, name: String) { ... }

// Good — transpositions are compile errors
pub struct StoneId(String);
pub struct StoneName(String);

fn register(id: StoneId, name: StoneName) { ... }
```

Newtypes are `#[repr(transparent)]` — zero runtime cost.

---

## 8. State machines as enums, not flag fields

```rust
// Bad — two bools encoding a 3-state machine; impossible states are representable
pub active:  Arc<AtomicBool>
pub started: Arc<AtomicBool>

// Good — invalid states are unrepresentable
pub enum PondStatus {
    Inactive,
    Active,
    Started,
}
```

Enum matching is a single branch. Boolean flag combinations require multiple atomic loads and allow impossible states.

---

## 9. Typestate for phased initialization

When a struct is progressively enriched across phases, encode the phases as types. Impossible-to-use-before-ready becomes a compile error.

```rust
// Bad — Options live forever, null-checked at every access
struct TopologyEntry {
    ip:           Option<IpAddr>,
    capabilities: Option<Capabilities>,
}

// Good — fields only exist when ready; no Options, no null checks
struct BootingEntry  { stone_id: StoneId, stone_name: StoneName }
struct NetworkedEntry { stone_id: StoneId, ip: IpAddr, ... }
struct ReadyEntry     { stone_id: StoneId, ip: IpAddr, capabilities: Capabilities, ... }

impl BootingEntry {
    fn with_network(self, ip: IpAddr) -> NetworkedEntry { ... }
}
```

Typestate is purely compile-time. Smaller structs, better cache locality, no branch per field access.

---

## 10. Domain errors as enums

```rust
// Bad — stringly typed; loses structure; heap allocation on every error
anyhow::anyhow!("Failed to mount volume: {}", path)

// Good — errors are first-class domain concepts; stack-allocated; matchable
pub enum StorageError {
    MountFailed        { path: PathBuf, reason: io::Error },
    ReplicationConflict { primary: StoneId, contender: StoneId },
    VolumeNotFound     { name: VolumeName },
}
```

Use `anyhow` at application boundaries (main, top-level handlers). Use typed enums within domain logic.

---

## 11. `#[must_use]` on fire-and-forget traps

```rust
#[must_use = "nudge has no effect unless awaited"]
pub fn nudge(&self) -> impl Future<Output = ()> { ... }
```

Silent discard of a meaningful operation becomes a compiler warning.

---

## 12. Local variable naming

Names carry purpose. Shadow freely for clones and child tokens — the shadow communicates that the value serves the same role.

```rust
// Bad — name encodes type or mechanical operation
let state_clone   = state.clone();
let token_child   = token.child_token();
let agg_rx        = state.storage.orchestration.tick.subscribe();

// Good — shadow for same-role values; purpose name for new roles
let state         = state.clone();           // shadow is fine
let token         = token.child_token();     // shadow is fine
let replication_feed = state.storage.orchestration.tick.subscribe();
```

---

## Summary

| Smell | Fix |
|---|---|
| Underscore in field name | Extract a struct |
| Type duplicated in name | Remove the redundant part |
| `Context` / `Manager` / `Service` suffix | Drop the suffix |
| `_tx` / `_rx` in struct field | Name the concept; type declares direction |
| Flat 64-field AppState | Domain contexts with `Arc<T>` grouping |
| `bool` flag pairs | State machine enum |
| `Option<T>` in long-lived struct | Typestate phases |
| `anyhow` inside domain logic | Typed error enum |
| Handler takes full `AppState` | `FromRef` with minimal context |
| `let x_clone = x.clone()` | Shadow: `let x = x.clone()` |
