---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-11
canonical: true
---

# ARCH-0016: Offerings as a DDD Aggregate — Event-Driven Mutation Boundary

**Date**: 2026-04-11
**Status**: Accepted
**Depends on**: [ARCH-0004](ARCH-0004-appstate-domain-context-extraction.md) (domain ownership through struct nesting), [ARCH-0015](ARCH-0015-task-supervisor-registry.md) (BackgroundTask trait)

## Context

Offering state on a Moss stone lives in two `Arc<RwLock<Vec<Offering>>>` fields
on `AppState`:

- `offerings` — active pool (managed, borrowed, detection-confirmed adopted).
  Visible in topology, API, chirps.
- `adopted_candidates` — cold storage for adopted offerings whose detection has
  not yet succeeded on this boot cycle.

A previous refactor (commit `96d38442`, "extract single offering mutation
gateway") introduced `mutate_offerings<F, R>(auto_chirp, mutator)` as *the*
chokepoint for mutations. Every existing write site was routed through it; the
gateway persisted the vec, triggered `sync_self_services` for chirping, and
called `refresh_local_tools_projection` so the tool registry (the canonical
projection consumed by `list`, `find`, and the tools beacon) stayed coherent.

Two commits later (`d73440c5`, "feat: adopted offerings use two-collection
architecture") introduced `promote_adopted()` and `demote_adopted()` to move
offerings between the two collections. Because the gateway was shaped for
single-vec mutation and could not atomically move items across two locks, the
new methods **bypassed the gateway entirely** and wrote with raw `.write()`
calls. The compiler had no objection — the invariant lived in a doc comment
labeled "canonical mutation boundary", not in the type system.

### The observable bug

On `stone-azure-pool` running Windows bare-metal Ollama:

1. Auto-adoption detects Ollama via HTTP probe at 11434.
2. `promote_adopted()` runs, logs `Promoted adopted candidate to active pool`.
3. The active vec now contains Ollama; `observe` (which reads it via topology
   aggregation) counts 7 offerings for that stone.
4. The tool registry was never refreshed. Tools beacon broadcasts 6 deltas in
   perpetuity: `Broadcasting tools beacon (6 deltas)` repeating every minute.
5. `garden-rake list` reads the tool registry → shows 6 services, no Ollama.
6. `garden-rake find ollama` reads the tool registry → "not found".
7. `observe` and `list` disagree. `find` fails. On Moss restart the promotion
   is lost entirely because `persist_offerings()` was never called either.

A one-line fix (`self.persist_offerings().await` inside `promote_adopted`)
patches one symptom. `demote_adopted` has the identical bug. The next feature
that touches adopted offerings will hit the same rake unless the mutation
boundary becomes the *only* reachable path — i.e., enforced by the type
system, not by prose.

### Structural smells

1. **Invariant lives in a comment, not the type system.** Any code holding
   `AppState` can call `state.offerings.write().await.push(...)` and bypass
   the gateway. The compiler permits it.

2. **Anemic domain model.** `offerings: Arc<RwLock<Vec<Offering>>>` is a public
   data field on `AppState` with mutation logic scattered across sibling
   helpers (`promote_adopted`, `demote_adopted`, `persist_offerings`,
   `refresh_local_tools_projection`, `mutate_offerings`, `upsert_offering`,
   `remove_offering`, `update_offering`, `update_offering_by_name`,
   `update_offerings_batch`, `coalesce_duplicate_offerings`,
   `replace_offerings`). There is no `Offerings` type owning its vec privately
   and enforcing rules on every mutation. This violates
   [code-standards.md §5](../code-standards.md) (domain ownership through
   struct nesting).

3. **Projections coupled by convention, not by event.** The tool registry and
   topology `self_entry` are both projections of the offering set. Today the
   binding is "remember to call `refresh_local_tools_projection` after every
   mutation." Projections should be driven by an event the aggregate emits,
   not a method call each mutation site has to remember — the pattern
   [code-standards.md §13](../code-standards.md) prescribes as `on_X` /
   `X_stream`.

## Decision

Introduce an `Offerings` domain aggregate that owns the active and candidate
collections privately, exposes mutations as the only mutation API, persists
through an `OfferingStore` port, and emits `OfferingsChanged` events on every
change. Projections (tool registry, topology/chirp) subscribe to the event
stream — they are no longer poked imperatively.

### The aggregate

```rust
pub struct Offerings {
    state:   RwLock<OfferingsState>,     // private — no external access
    store:   Arc<dyn OfferingStore>,     // persistence port
    changes: broadcast::Sender<OfferingsChanged>,
}

struct OfferingsState {
    active:     Vec<Offering>,
    candidates: Vec<Offering>,
}
```

Both collections are merged under a single `RwLock` to eliminate the
lock-ordering footgun and make cross-collection atomic moves (promote/demote)
trivially correct.

### The mutation API — the only way in

```rust
impl Offerings {
    pub async fn promote(&self, offering_id: &str) -> bool;
    pub async fn demote(&self, offering_id: &str) -> bool;
    pub async fn upsert(&self, offering: Offering);
    pub async fn remove(&self, offering_id: &str) -> bool;
    pub async fn remove_by_name(&self, name: &str) -> bool;
    pub async fn update<F>(&self, id: &str, f: F) -> bool
        where F: FnOnce(&mut Offering) -> bool;
    pub async fn update_by_name<F>(&self, name: &str, f: F) -> bool
        where F: FnOnce(&mut Offering) -> bool;
    pub async fn update_batch<F>(&self, f: F) -> usize
        where F: FnOnce(&mut Vec<Offering>) -> usize;
    pub async fn replace_active(&self, new: Vec<Offering>);
    pub async fn coalesce_duplicates(&self) -> usize;
}
```

Every method follows an identical shape:

1. Acquire the write lock.
2. Perform the change in memory.
3. If something changed, clone a snapshot of the merged state before releasing
   the lock (ensures persistence is consistent with what is in memory).
4. Release the lock.
5. Call `store.save(snapshot).await` (async I/O, never held across the lock).
6. Emit `OfferingsChanged { kind, affected, timestamp }` through the broadcast
   channel.

Steps 3–6 are factored into a private `finalize` helper so every mutation
shares the persist+emit pipeline. There is no public `write()` accessor. There
is no way to mutate without calling one of the methods above. There is no way
to call one of the methods above without persist+emit happening.

### The read API

Three read patterns coexist:

```rust
impl Offerings {
    // BACK-COMPAT SHIM — the strangler vine.
    // Every existing `state.offerings.read().await` compiles unchanged because
    // ActiveGuard derefs to &Vec<Offering>, which derefs to &[Offering].
    pub async fn read(&self) -> ActiveGuard<'_>;

    // Snapshot and typed query methods — preferred for new code.
    pub async fn snapshot(&self) -> Vec<Offering>;
    pub async fn candidates_snapshot(&self) -> Vec<Offering>;
    pub async fn find_by_id(&self, id: &str) -> Option<Offering>;
    pub async fn find_by_name(&self, name: &str) -> Option<Offering>;

    // Scoped borrow — no clone, lock scope bounded to the closure.
    pub async fn with_active<F, R>(&self, f: F) -> R
        where F: FnOnce(&[Offering]) -> R;
    pub async fn with_candidates<F, R>(&self, f: F) -> R
        where F: FnOnce(&[Offering]) -> R;
}
```

`ActiveGuard` is a thin newtype wrapping `RwLockReadGuard<'_, OfferingsState>`
with a `Deref<Target = Vec<Offering>>` impl that projects to the `active`
field. It exists solely to keep the 82 existing `state.offerings.read().await`
sites compiling during migration. When the count reaches zero, it is deleted.

### The event

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct OfferingsChanged {
    pub kind:      ChangeKind,
    pub affected:  Vec<String>,        // offering IDs affected by this change
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ChangeKind {
    Upserted,
    Removed,
    Updated,
    Promoted,
    Demoted,
    Replaced,
    Coalesced,
    BatchUpdated,
}

impl ChangeKind {
    /// Whether this change should trigger an immediate UDP chirp.
    pub fn should_chirp(&self) -> bool { true }
}

impl Offerings {
    pub fn changes(&self) -> broadcast::Receiver<OfferingsChanged> {
        self.changes.subscribe()
    }
}
```

One event per mutation. One receiver per consumer. `OfferingsChanged` is
separate from the existing `OfferingEvent` lifecycle events
(`Deployed`/`Started`/`Stopped`/...) — the two describe different semantics:
`OfferingEvent` is "the real-world service transitioned" (e.g., Docker
container started), `OfferingsChanged` is "the registry membership on this
stone was mutated." They coexist.

### The persistence port

```rust
#[async_trait::async_trait]
pub trait OfferingStore: Send + Sync {
    async fn load(&self) -> anyhow::Result<Vec<Offering>>;
    async fn save(&self, all: &[Offering]) -> anyhow::Result<()>;
}
```

File-backed impl (`FileOfferingStore`) wraps the existing
`crate::infra::{load_offerings, save_offerings}` functions. Test doubles can
be built without touching disk. The aggregate depends on `Arc<dyn
OfferingStore>`, not on `crate::infra::*` directly — domain no longer imports
infra.

### The projection inversion

Today the tool registry refresh is a direct imperative call from
`persist_offerings()`:

```rust
self.persist_offerings().await?;
self.refresh_local_tools_projection().await;  // remember to call this
```

The inversion makes the refresh reactive. A new `OfferingsProjectionTask`
(ARCH-0015 `BackgroundTask`) is registered at bootstrap:

```rust
pub struct OfferingsProjectionTask;

impl BackgroundTask for OfferingsProjectionTask {
    fn name(&self) -> &'static str { "offerings-projection" }

    fn run(self: Box<Self>, mut ctx: TaskContext)
        -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>>
    {
        Box::pin(async move {
            // Seed projection from initial state before entering the loop.
            ctx.state.refresh_local_tools_projection().await;
            ctx.ready.signal();

            let mut feed = ctx.state.offerings.changes();
            loop {
                tokio::select! {
                    _ = ctx.token.cancelled() => return TaskOutcome::Cancelled,
                    msg = feed.recv() => match msg {
                        Ok(event) => {
                            ctx.state.refresh_local_tools_projection().await;
                            ctx.state.sync_self_services(event.kind.should_chirp()).await;
                        }
                        Err(RecvError::Lagged(n)) => {
                            tracing::warn!(skipped = n, "offerings projection feed lagged");
                            ctx.state.refresh_local_tools_projection().await;
                            ctx.state.sync_self_services(true).await;
                        }
                        Err(RecvError::Closed) => return TaskOutcome::Completed,
                    }
                }
            }
        })
    }
}
```

Tool registry and topology chirp are now downstream consumers of the event
stream. They cannot be forgotten by a caller because no caller is responsible
for invoking them.

### AppState changes

```rust
pub struct AppState {
    // Was: pub offerings: Arc<RwLock<Vec<Offering>>>
    // Was: pub adopted_candidates: Arc<RwLock<Vec<Offering>>>
    pub offerings: Arc<Offerings>,  // the domain aggregate
    // ...
}
```

The following `AppState` methods are **deleted** because their behavior now
lives inside `Offerings::apply` and is invoked through the aggregate's public
API:

- `promote_adopted`, `demote_adopted`
- `persist_offerings`, `mutate_offerings`
- `upsert_offering`, `remove_offering`, `remove_service`
- `update_offering`, `update_offering_by_name`, `update_offerings_batch`
- `replace_offerings`, `coalesce_duplicate_offerings`

The following methods **remain** because they serve orthogonal purposes:

- `refresh_local_tools_projection` — invoked by `OfferingsProjectionTask` and
  by `emit_storage_changed` (storage events also affect the projection via
  seed-bank entries).
- `sync_self_services`, `build_self_entry` — the chirp and topology builders.
- `get_offerings`, `get_managed_offerings`, `get_adopted_offerings`,
  `get_borrowed_offerings`, `find_offering`, `find_offering_by_id` — thin
  delegates to the aggregate, kept for source compatibility with existing
  call sites.

### Migration strategy — strangler vine

The 82 `state.offerings.read().await` sites across 31 files are **not
touched** in this change. They compile unchanged because `Offerings::read()`
returns an `ActiveGuard` that derefs to `&Vec<Offering>`. Read-site migration
happens opportunistically as files are edited for other reasons. When
`state.offerings.read().await` reaches zero occurrences, `ActiveGuard` and
`.read()` are deleted in a cleanup commit.

The 25 write call sites are migrated in this change because the old method
signatures on `AppState` are deleted. Each migration is mechanical:

```rust
// Before
state.upsert_offering(offering, true).await;
state.remove_offering(&id, false).await;
state.update_offering(&id, true, |o| { o.health = Healthy; true }).await;

// After
state.offerings.upsert(offering).await;
state.offerings.remove(&id).await;
state.offerings.update(&id, |o| { o.health = Healthy; true }).await;
```

The `auto_chirp` boolean disappears — chirping is now event-driven and
unconditional (every mutation that matters to topology chirps). The few call
sites that used to pass `false` were micro-optimizations saving a single UDP
packet; the periodic announcer still runs on its own cadence and deduplicates
at the transport layer.

## Rationale

- **The compiler enforces the invariant.** Private `state: RwLock<...>`
  closes every bypass. `promote_adopted` style bugs become impossible to
  write because the raw `.write()` handle is not reachable from outside the
  module.
- **One event, one consumer loop, one set of projections.** The tool
  registry and chirp are no longer things a mutation site has to remember;
  they are subscribers to a stream.
- **`ActiveGuard` is a temporary beam, not a permanent API.** It exists
  purely to keep the tree standing while the vine strangles it. 82 read
  sites compile untouched today; they migrate at leisure. When the count
  hits zero, the guard is deleted. The aggregate then has no back-compat
  surface at all.
- **Persistence is a port, not a hardcoded call.** `OfferingStore` is
  mockable for tests and decouples the domain from `crate::infra::*`,
  matching [ARCH-0004](ARCH-0004-appstate-domain-context-extraction.md)'s
  layering.
- **The two-collection split is preserved.** It earns its keep (adopted
  offerings survive restart without appearing in topology until detection
  confirms). It just needs a single owner that treats the pair as one
  aggregate.

## Consequences

### Positive

- The `promote_adopted`/`demote_adopted` bug is fixed at the root: the only
  mutation path is `Offerings::promote/demote`, which goes through the
  persist+emit pipeline. A second bypass cannot be introduced without
  deleting the private qualifier.
- Tool registry, topology chirp, and future projections all become reactive
  to a single event, matching [code-standards.md §13](../code-standards.md).
- `Offerings` is testable in isolation with a mock `OfferingStore`, without
  spinning up the full `AppState`.
- `auto_chirp` boolean plumbing is deleted — chirp semantics live in one
  place (`ChangeKind::should_chirp()`), not scattered across 25 call sites.
- The distinction between "offering lifecycle event" (`OfferingEvent::Started`
  from Docker) and "registry membership changed" (`OfferingsChanged`) is now
  explicit in the type system.

### Negative

- 82 read sites use a transitional `ActiveGuard` shim. They must eventually
  migrate to typed query methods (`snapshot`, `find_by_id`, `with_active`)
  for the shim to be deleted. No deadline; opportunistic.
- The `Offerings` aggregate uses interior mutation (`Arc<Offerings>` rather
  than `Offerings` owned by `AppState`), so method calls are `state.offerings
  .foo(...).await` rather than `state.offerings.foo(...).await` — unchanged
  ergonomically because `Arc` derefs transparently.
- `ChangeKind::should_chirp()` always returns `true` in this iteration.
  Future optimization could introduce a `QuietUpdate` variant if per-mutation
  chirp cost becomes measurable (it currently is not).

### Neutral

- The `Offering` struct in `garden_common` is **not** touched. This refactor
  is moss-internal and does not affect rake, orchestrators, or the wire
  format.
- The `domain/offerings.rs` file is converted to a `domain/offerings/`
  directory with `catalog.rs` holding the previous content (`OfferingsIndex`,
  `CompiledOffering`, compile-time catalog build). The new aggregate lives
  alongside in `aggregate.rs`. The file rename is bundled with content
  changes in this single commit because the two are semantically linked; a
  separate rename commit would obscure the intent.

## Migration Plan

### Phase 1 (this ADR): Aggregate + event inversion

1. Convert `domain/offerings.rs` → `domain/offerings/` module with
   `catalog.rs` (existing content), `aggregate.rs`, `change.rs`, `event.rs`,
   `guard.rs`, `store.rs`.
2. Implement `Offerings`, `OfferingsChanged`, `ChangeKind`, `ActiveGuard`,
   `OfferingStore` + `FileOfferingStore`.
3. Replace `AppState::offerings` and `AppState::adopted_candidates` with
   `AppState::offerings: Arc<Offerings>`.
4. Delete the 12 mutation methods listed above from `AppState`.
5. Keep `refresh_local_tools_projection`, `sync_self_services`,
   `build_self_entry`, and the read-side `get_*`/`find_*` delegates.
6. Create `OfferingsProjectionTask` and register it in the task registry.
7. Update `bootstrap/run.rs` and `testing.rs` to construct the aggregate.
8. Migrate `auto_adoption.rs` to use `state.offerings.promote/demote/
   candidates_snapshot`.
9. Migrate 25 write call sites from old `AppState` methods to aggregate
   methods.
10. Verify against the live stone-azure-pool Moss logs: after Ollama
    adoption, the tools beacon delta count should increase, `garden-rake
    list` should show Ollama, `garden-rake find ollama` should resolve to
    the stone.

### Phase 2 (future, opportunistic): Read-site migration

As files are edited for other reasons, replace `state.offerings.read().await`
→ `state.offerings.snapshot().await` / `.with_active(|o| ...)` /
`.find_by_id(id).await`. No single PR, no deadline. A grep for
`state\.offerings\.read` shows the remaining surface at any point.

### Phase 3 (future): Delete the strangler vine

When Phase 2 reaches zero, delete `Offerings::read()`, `ActiveGuard`, and the
`get_offerings`/`find_offering` delegates on `AppState`. The aggregate then
has no back-compat surface — only the typed read API and the mutation API.

## References

- [ARCH-0004](ARCH-0004-appstate-domain-context-extraction.md) — domain
  ownership through struct nesting (§5 of code standards)
- [ARCH-0015](ARCH-0015-task-supervisor-registry.md) — `BackgroundTask` trait
  used by `OfferingsProjectionTask`
- [OFFER-0005](OFFER-0005-offering-modes.md) — managed/borrowed/adopted mode
  distinctions preserved by the two-collection split
- [code-standards.md §5](../code-standards.md) — domain ownership rule
- [code-standards.md §13](../code-standards.md) — `{noun}_stream` event API
  pattern
- The bug this ADR fixes: `d73440c5` introduced `promote_adopted` /
  `demote_adopted` that bypassed the `mutate_offerings` gateway, leaving the
  tool registry out of sync on Windows bare-metal Ollama adoption
