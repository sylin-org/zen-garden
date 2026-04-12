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

`Moss` is the daemon's runtime — a thin dependency container. Each domain owns its fields. Struct nesting makes domain boundaries visible to the compiler.

```rust
#[derive(Clone)]
pub struct Moss {
    // This node's mutable self-description
    pub current: Current,

    // Cross-cutting — no single domain owner
    pub shutdown_token: CancellationToken,
    pub event_bus:      EventBus,
    pub console:        Arc<ConsolePrinter>,

    // Domain aggregates — each domain owns its state
    pub offerings:  Arc<Offerings>,
    pub metrics:    Arc<Metrics>,
    pub catalog:    Arc<Catalog>,
    pub jobs:       Arc<Jobs>,
    pub tool:       Arc<Tool>,
    pub topology:   Arc<Topology>,
    pub security:   Arc<Security>,
    pub discovery:  Arc<Discovery>,
    pub presence:   Arc<Presence>,
    pub companion:  Arc<Companion>,
    pub health:     Arc<Health>,
    pub subsystems: Arc<Subsystems>,
}
```

A field that crosses domain boundaries in its name (e.g. `storage_orchestration_nudge` in a flat struct) signals that the domain boundary is missing from the type system.

### The `current` namespace

`current` is the node's self-description — what this running instance *is* right now. It is not a domain context (it holds no operational state); it is the node's own identity, network address, and runtime metrics.

```rust
pub struct Current {
    pub stone:   Arc<Stone>,                // immutable: set at startup, never changes
    pub address: Arc<RwLock<PeerAddress>>,   // mutable: IP/hostname change on DHCP renewal
    pub health:  Arc<RwLock<String>>,        // mutable: updated by health checks
    pub metrics: Arc<Metrics>,               // mutable: individual locks inside
    pub storage: Arc<Storage>,               // domain context for local volumes
    // ...
}

pub struct Stone {
    pub id:   String,   // permanent — cryptographic/install identity, never changes
    pub name: String,   // user-assigned display name — fixed for the process lifetime
}
```

`Stone` is immutable after startup — both `id` and `name` are fixed for the process lifetime. The stone identity is behind `Arc<Stone>` (shared, not locked) because it never changes. Network address (`PeerAddress`) is the mutable identity, behind `Arc<RwLock<>>` because it changes on DHCP renewal.

```rust
state.current.stone.id                         // who am I — permanent, no lock needed
state.current.stone.name                       // my display name — fixed, no lock needed
state.current.address.read().await.hostname()  // my network address — mutable, locked
state.current.health.read().await              // current health — mutable, locked
```

---

## 6. Handler dependency declaration via `FromRef`

Handlers declare only the dependencies they actually need. Axum's `FromRef` enforces this at the type level.

```rust
impl FromRef<Moss> for Arc<Storage> {
    fn from_ref(state: &Moss) -> Self { state.storage.clone() }
}

// Handler's dependency surface is explicit and compiler-enforced
async fn list_volumes(State(storage): State<Arc<Storage>>) -> impl IntoResponse { ... }

// Not this — implicit dependency on everything
async fn list_volumes(State(state): State<Moss>) -> impl IntoResponse { ... }
```

---

## 7. Domain value objects over primitive identifiers

If a function takes `id: String, name: String`, those parameters have no namespace — the function accepts any string for either argument. Transpositions are silent runtime bugs.

The fix is not a newtype — `StoneId` is just an underscore in a different coat, violating rule 1. The fix is a value object: a struct whose position in the hierarchy makes its fields unambiguous.

```rust
// Bad — flat primitives; transpositions are runtime bugs
fn register(stone_id: String, stone_name: String) { ... }

// Also bad — StoneId is StoneId(String): still name-encoded, not namespace-encoded
fn register(id: StoneId, name: StoneName) { ... }

// Good — struct is the namespace; fields are plain, unambiguous names
pub struct Stone {
    pub id:   String,
    pub name: String,
    pub host: String,
}

fn register(stone: &Stone) { ... }
```

At the call site: `stone.id` and `stone.name` — the struct carries the namespace. No `StoneId`, no `StoneName`.

For boundaries where only a key is passed (lookup, delete), a plain `&str` is honest and correct — it is just a key, not a domain concept requiring a wrapper.

```rust
fn find(id: &str) -> Option<Stone> { ... }      // fine — just a key
fn delete(stone: &Stone) -> Result<()> { ... }  // full object where identity matters
```

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

## 13. Domain event subscription API

Domains expose events through two distinct method patterns. The pattern encodes the cardinality; the return type enforces it.

**`on_{event}()`** — lifecycle events that happen once or rarely per run. Returns a `watch::Receiver<T>` so the caller can read current state or await a transition.

```rust
// Bad — caller subscribes to a raw channel with no semantic name
let rx = state.security.pond_status.subscribe();

// Good — method names the lifecycle moment; type encodes watch semantics
pub fn on_pond_joined(&self) -> watch::Receiver<PondStatus> {
    self.pond.status.subscribe()
}

// Call site
let pond_ready = state.security.on_pond_joined();
```

**`{noun}_stream()`** — continuous event streams that fire repeatedly. Returns a `broadcast::Receiver<T>`.

```rust
// Bad — caller navigates internal channel path
let rx = state.communication.topology.chirp.subscribe();

// Good — method names the stream; broadcast semantics are implicit
pub fn chirp_stream(&self) -> broadcast::Receiver<StoneChirp> {
    self.chirp.subscribe()
}

// Call site — name the consumer's purpose, not the type
let discovery_feed = state.communication.topology.chirp_stream();
let sse_feed       = state.communication.topology.chirp_stream();
```

The internal channel fields (`self.chirp`, `self.pond.status`) remain private to the domain. External code never calls `.subscribe()` directly on a domain's channel fields.

**Lagged receiver handling** is always: warn and continue — never break the stream on lag.

```rust
match feed.recv().await {
    Ok(event)                 => handle(event),
    Err(RecvError::Lagged(n)) => tracing::warn!(skipped = n, "consumer lagged"),
    Err(RecvError::Closed)    => break,
}
```

**SSE emitters** are the canonical cross-cutting consumer: subscribe via the domain API, loop, map to wire format, yield.

```rust
async fn storage_stream(
    State(storage): State<Arc<Storage>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut feed = storage.on_device_connected();

    Sse::new(async_stream::stream! {
        loop {
            match feed.recv().await {
                Ok(event)                 => yield Ok(Event::default().json_data(&event).unwrap()),
                Err(RecvError::Lagged(n)) => tracing::warn!(skipped = n, "SSE lagged"),
                Err(RecvError::Closed)    => break,
            }
        }
    }).keep_alive(KeepAlive::default())
}
```

Domain event types always `#[derive(Clone, Serialize)]` — `Clone` is required by `broadcast`; `Serialize` makes the type the wire format contract with no intermediate mapping step.

| Pattern | Semantics | Return type | Channel kind |
|---------|-----------|-------------|--------------|
| `on_{event}()` | Lifecycle — once or rarely | `watch::Receiver<T>` | `watch` |
| `{noun}_stream()` | Continuous — every occurrence | `broadcast::Receiver<T>` | `broadcast` |

---

## 14. Canonical types and file coupling

### One type per concept, used everywhere

Define each domain type once in `garden-common`. Use it directly in API responses, storage, business logic, and SSE payloads. Domain types derive `Serialize`/`Deserialize` from the start — they are the wire format contract.

```rust
// Bad — Stone fields duplicated into a transport copy
#[derive(Serialize)]
pub struct StoneResponse {
    pub id:        String,  // copy of Stone::id
    pub name:      String,  // copy of Stone::name
    pub is_online: bool,
}

// Good — embed the canonical type; only add what genuinely differs
#[derive(Serialize)]
pub struct StoneResponse {
    #[serde(flatten)]
    pub stone:     Stone,
    pub is_online: bool,    // computed — not a copy
}

// Better — if nothing extra is needed, return the canonical type directly
async fn get_stone(...) -> Json<Stone> { ... }
```

**Rule**: enrich or embed, never duplicate fields. A new struct is only justified when the shape genuinely differs from the domain type.

### One file per concept

File names and their contents must have a 1:1 coupling. A file named `stone.rs` contains stone types and stone logic — nothing else. A file named `storage.rs` contains storage domain code — not coordination helpers, not unrelated API types.

```
// Bad
app_state.rs      — 64-field flat struct mixing all domains
coordinator.rs    — background task coordination for all domains

// Good
stone.rs          — Stone value object and stone domain logic
storage/mod.rs    — Storage domain context
storage/volume.rs — Volume types and volume logic
```

`app_state.rs` becomes a thin re-export after domain contexts are extracted to their own files. Catch-all files (`helpers.rs`, `utils.rs`, `common.rs`) are not permitted — every item finds its home in a file named for its concept.

### Renaming files without losing history

File reorganization commits are split in two:

1. **Rename commit** — pure `git mv`, no content changes. Git detects the rename; `git log --follow` traces history across it.
2. **Content commit** — changes to the moved file's content, in a separate commit.

Never mix renaming and content edits in a single commit.

---

## 15. Module visibility

Domain internals are `pub(crate)` by default. Only types and methods that genuinely cross a crate boundary are `pub`.

```rust
// Bad — everything pub; no boundary enforcement
pub struct Volume { ... }
pub fn mount(&self, path: &Path) -> Result<()> { ... }

// Good — internal types stay internal; only the event API and value objects cross the boundary
pub(crate) struct VolumeMount { ... }          // internal to storage domain
pub(crate) fn mount(&self, path: &Path) -> Result<()> { ... }

pub struct Volume { pub name: String, ... }    // value object — crosses crate boundary
pub fn device_stream(&self) -> broadcast::Receiver<StorageChanged> { ... }  // event API
```

The rule applies within a crate too: infra types used only by the infra layer are `pub(crate)` or `pub(super)`. A type that leaks into the domain layer through a `pub` declaration is an architecture violation, not just a visibility preference.

---

## 16. `unsafe` requires documented invariants

Every `unsafe` block carries a `// SAFETY:` comment directly above it. The comment states:
1. What invariant the `unsafe` call relies on.
2. Why that invariant holds at this call site.

```rust
// Bad — reviewer cannot audit correctness
let len = unsafe { GetLogicalDriveStringsW(buf.len() as u32, buf.as_mut_ptr()) };

// Good — invariant is documented and verifiable
// SAFETY: `buf` is a stack-allocated [u16; 256] valid for writes.
// `buf.len() as u32` correctly represents the buffer capacity.
let len = unsafe { GetLogicalDriveStringsW(buf.len() as u32, buf.as_mut_ptr()) };
```

No exceptions. If the invariant is "this is always safe", write that and cite the API documentation.

---

## 17. `.unwrap()` discipline

**Never `.unwrap()` when the input is external.** External means: user input, query parameters, HTTP headers, file content, network responses, or any value not generated by this process in this run.

```rust
// Bad — upload_id is a query parameter; attacker controls it
let dir = base_path.join(upload_id);           // path traversal
let hdr: HeaderValue = name.parse().unwrap();  // panics on non-ASCII

// Good — validate before use
uuid::Uuid::parse_str(upload_id)?;             // reject non-UUID
if let Ok(val) = name.parse::<HeaderValue>() { // skip invalid
    builder = builder.header(key, val);
}
```

**`.unwrap()` is acceptable when:**
- The value is a compile-time constant (`"application/json".parse().unwrap()`)
- The invariant is enforced by the type system or a preceding check
- The call site is in test code

**`.expect("reason")` is acceptable when:**
- A structural invariant guarantees `Some`/`Ok` but the type system cannot prove it
- The reason string documents the invariant: `.expect("handle is always local or remote")`

**`Response::builder().body().unwrap()`**: If any header in the builder chain came from external data, use `build_response()` or handle the `Result`. Constant-only builders may use `.unwrap()`.

---

## 18. Validate at I/O boundaries, trust internally

External data is validated once, at the boundary where it enters the system. After validation, internal code trusts the validated type.

```rust
// Boundary: S3 gateway handler (validates once)
fn validate_upload_id(id: &str) -> Result<()> {
    uuid::Uuid::parse_str(id).map_err(|_| anyhow!("Invalid upload ID"))?;
    Ok(())
}

// Internal: MultipartStore (trusts validated ID)
fn upload_dir(&self, upload_id: &str) -> Result<PathBuf> {
    Self::validate_upload_id(upload_id)?;
    Ok(self.base_path.join(upload_id))
}
```

Path traversal checks (`../`, `..\\`) are mandatory for any user-supplied value used in a filesystem path. Use `has_path_traversal()` from `garden_common::constants::storage::share`.

---

## 19. Share expensive resources

Connection pools, HTTP clients, and TLS contexts are created once at startup and shared via `Arc` or static singletons. Never construct them per-request.

```rust
// Bad — new connection pool per request (~2ms overhead, defeats pooling)
async fn proxy_request(...) -> Response {
    let client = reqwest::Client::new();
    client.get(url).send().await
}

// Good — shared singleton, connections reused
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("HTTP client")
});
```

---

## 20. Memory budgets for accumulation

Any operation that accumulates data in memory (concatenation, collection, buffering) must have an explicit size limit. Unbounded accumulation turns a data-plane issue into a process-level OOM.

```rust
// Bad — 5 GB multipart upload → 5 GB Vec<u8>
let mut assembled = Vec::new();
for part in parts {
    assembled.extend_from_slice(&part);
}

// Good — reject before accumulating
if total_size > MAX_ASSEMBLED_SIZE {
    anyhow::bail!("Object too large for in-memory assembly");
}
let mut assembled = Vec::with_capacity(total_size as usize);
```

When the size is known upfront, use `Vec::with_capacity()` to avoid reallocations.

---

## Summary

| Smell | Fix |
|---|---|
| Underscore in field name | Extract a struct |
| Type duplicated in name | Remove the redundant part |
| `Context` / `Manager` / `Service` suffix | Drop the suffix |
| `_tx` / `_rx` in struct field | Name the concept; type declares direction |
| Flat 64-field root struct | Domain aggregates with `Arc<T>` grouping |
| `bool` flag pairs | State machine enum |
| `Option<T>` in long-lived struct | Typestate phases |
| `anyhow` inside domain logic | Typed error enum |
| Handler takes full `Moss` | `FromRef` with minimal context |
| `let x_clone = x.clone()` | Shadow: `let x = x.clone()` |
| `fn f(stone_id: String, stone_name: String)` | Domain value object: `fn f(stone: &Stone)` |
| Duplicated struct fields across types | Embed canonical type; enrich with `#[serde(flatten)]` |
| `app_state.rs` containing all domains | One file per concept; root struct becomes thin container |
| `helpers.rs` / `utils.rs` catch-alls | Each item moves to its concept's file |
| `.subscribe()` called outside domain | Expose `on_X()` / `X_stream()` instead |
| SSE handler navigates internal channels | Subscribe via domain event API |
| Domain internals declared `pub` | `pub(crate)` or `pub(super)`; only boundary types are `pub` |
| Mutable node identity as flat fields | `current.stone: Arc<RwLock<Stone>>`; `current.environment` |
| `unsafe` without `// SAFETY:` | Document the invariant and why it holds |
| `.unwrap()` on external input | Validate at boundary; use `?`, `expect`, or skip |
| User value in filesystem path | `has_path_traversal()` check + UUID/format validation |
| `reqwest::Client::new()` per request | Static singleton or `Arc` shared from startup |
| Unbounded `Vec` accumulation | Size guard + `Vec::with_capacity()` |
