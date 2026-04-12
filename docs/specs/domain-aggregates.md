---
audience: [developer, ai]
doc_type: spec
status: canonical
last_verified: 2026-04-11
---

# Domain Aggregate Pattern

**Purpose:** The canonical structure every bounded context in moss follows.
**Audience:** Developers building new contexts, extending existing contexts, or reviewing code.
**Scope:** All Rust code under `src/moss/src/domain/`.

> This document is the reference every [ARCH-0017](../decisions/ARCH-0017-ddd-monolith-epic.md) book applies. A context that deviates from this pattern without a documented reason is a bug.

---

## Contents

- [When to apply](#when-to-apply)
- [Module layout](#module-layout)
- [The aggregate root](#the-aggregate-root)
- [State management](#state-management)
- [Read API](#read-api)
- [Mutation API](#mutation-api)
- [The finalize pipeline](#the-finalize-pipeline)
- [Events](#events)
- [Errors](#errors)
- [Ports and adapters](#ports-and-adapters)
- [Metrics integration](#metrics-integration)
- [Tracing](#tracing)
- [Projection tasks](#projection-tasks)
- [Test scaffold](#test-scaffold)
- [Anti-patterns](#anti-patterns)
- [Worked example — Offerings](#worked-example--offerings)
- [Checklist for new contexts](#checklist-for-new-contexts)

---

## When to apply

The pattern applies to every **bounded context** in moss. A bounded context is a module that owns a slice of state, a slice of behavior, or a slice of coordination and has an explicit contract with the rest of the system.

**Apply this pattern when:**

- A context holds mutable state shared across threads.
- A context needs to enforce invariants across related data.
- Other contexts need to react to changes in this context.
- Persistence, networking, or other I/O happens at the context's boundary.

**Do not apply this pattern when:**

- A type is a pure value object (no mutation, no events, no ports). Use a plain `struct` with methods.
- A context is a facade over a single infrastructure dependency and has no state. Use a plain service struct with commands.
- A module provides reusable helpers with no domain meaning (formatters, parsers). Use free functions.

A bounded context that does not fit the aggregate pattern for a good reason documents that reason in its own ADR.

---

## Module layout

Every bounded context lives under `src/moss/src/domain/<context>/`. The directory has a fixed set of files:

```
src/moss/src/domain/<context>/
├── mod.rs          # module root + public re-exports
├── aggregate.rs    # the aggregate root type
├── state.rs        # private state struct (optional; inline in aggregate.rs for small contexts)
├── event.rs        # event types (Changed enum + ChangeKind)
├── error.rs        # typed error enum
├── port.rs         # trait definitions for infrastructure (may split per port)
├── tests.rs        # unit tests with fake ports
└── <sub>.rs        # optional sub-aggregates if the context is large enough
                    # (Storage::Volumes, Storage::Banks, Storage::Replication)
```

For small contexts, `state.rs` can merge into `aggregate.rs` and `error.rs` can be a single enum inside `aggregate.rs`. The goal is one file per concept (per code-standards §14), not mandatory file count.

The `mod.rs` re-exports the public surface:

```rust
//! <Context> bounded context.
//!
//! <One-paragraph description of what this context owns and why.>

pub mod aggregate;
pub mod event;
pub mod error;
pub mod port;

#[cfg(test)]
mod tests;

pub use aggregate::<Context>;
pub use event::{<Context>Changed, ChangeKind};
pub use error::<Context>Error;
pub use port::{<Context>Store, ...};
```

Internal state types (`<Context>State`, helper structs) stay `pub(super)` or `pub(crate)` as appropriate. They never leak across the module boundary.

---

## The aggregate root

The aggregate root is the public face of the context. It holds infrastructure dependencies as injected ports and exposes typed methods.

```rust
// domain/<context>/aggregate.rs

use super::event::{<Context>Changed, ChangeKind};
use super::error::<Context>Error;
use super::port::<Context>Store;
use super::state::<Context>State;
use crate::domain::Metrics;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

pub struct <Context> {
    /// Private interior state. No public accessor.
    state: RwLock<<Context>State>,

    /// Persistence port. Injected at construction.
    store: Arc<dyn <Context>Store>,

    /// Cross-cutting metrics port. Injected at construction.
    metrics: Arc<Metrics>,

    /// Event publication channel. Subscribers use `changes()`.
    changes: broadcast::Sender<<Context>Changed>,
}
```

### Rules the compiler enforces

1. **`state` is private.** No `pub`, no `pub(crate)`, no method that returns a raw `RwLockWriteGuard` or `RwLockReadGuard` to it.
2. **`changes` is private.** Subscription goes through a `changes()` method that returns a fresh `broadcast::Receiver`.
3. **`store` and `metrics` (and any other ports) are `Arc<dyn T>`.** Never concrete types. Injected at construction time.
4. **No `pub` fields on the aggregate.** The only way to read state is through a typed query method; the only way to write state is through a typed command method.

### Construction

```rust
impl <Context> {
    pub fn new(
        store: Arc<dyn <Context>Store>,
        metrics: Arc<Metrics>,
    ) -> Self {
        let (changes, _) = broadcast::channel(
            garden_common::constants::channels::<CONTEXT>_EVENT,
        );
        Self {
            state: RwLock::new(<Context>State::default()),
            store,
            metrics,
            changes,
        }
    }

    /// Alternative constructor for contexts that load initial state from the store.
    pub async fn load(
        store: Arc<dyn <Context>Store>,
        metrics: Arc<Metrics>,
    ) -> Result<Self, <Context>Error> {
        let initial = store.load().await.map_err(<Context>Error::Persistence)?;
        let (changes, _) = broadcast::channel(
            garden_common::constants::channels::<CONTEXT>_EVENT,
        );
        metrics.register_domain(<Context>::NAME);
        Ok(Self {
            state: RwLock::new(<Context>State::from_loaded(initial)),
            store,
            metrics,
            changes,
        })
    }

    pub const NAME: &'static str = "<context>";
}
```

The constructor registers the context with `Metrics::register_domain` so per-domain counters are initialized.

---

## State management

State lives in a `pub(super)` struct inside the context:

```rust
// domain/<context>/state.rs (or inline in aggregate.rs)

pub(super) struct <Context>State {
    pub(super) items: Vec<Item>,
    // other fields...
}

impl <Context>State {
    pub(super) fn default() -> Self {
        Self { items: Vec::new() }
    }

    pub(super) fn from_loaded(items: Vec<Item>) -> Self {
        Self { items }
    }

    /// Produce a persistence snapshot. Called inside `finalize`
    /// while the write lock is held, so the clone reflects the
    /// post-mutation state exactly.
    pub(super) fn snapshot(&self) -> Vec<Item> {
        self.items.clone()
    }
}
```

The state struct fields are `pub(super)` — visible to sibling modules inside the context (`aggregate.rs`, `guard.rs` during strangler migration) but not to code outside the context. This is the compiler-enforced privacy boundary that makes invariants robust.

---

## Read API

Every aggregate provides at least these read shapes, where they make sense for the context:

```rust
impl <Context> {
    /// Clone of the primary collection.
    pub async fn snapshot(&self) -> Vec<Item> {
        self.state.read().await.items.clone()
    }

    /// Find one by ID.
    pub async fn find_by_id(&self, id: &str) -> Option<Item> {
        self.state.read().await.items.iter().find(|i| i.id == id).cloned()
    }

    /// Find one by name (if the context has FQN-style names).
    pub async fn find_by_name(&self, name: &str) -> Option<Item> {
        self.state.read().await.items.iter().find(|i| i.name == name).cloned()
    }

    /// Scoped borrow — the closure runs inside the read lock. The closure
    /// must not await (compiler enforces through lifetimes).
    pub async fn with_items<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[Item]) -> R,
    {
        let st = self.state.read().await;
        f(&st.items)
    }

    /// Count.
    pub async fn count(&self) -> usize {
        self.state.read().await.items.len()
    }
}
```

### Which shapes to use

| Shape | When to use |
|-------|-------------|
| `snapshot()` | Callers that need the full collection and are fine with a clone. API handlers returning JSON, dashboard queries, bulk iteration. |
| `find_by_*()` | Callers that need exactly one item. API handlers for `GET /X/:id`, internal lookups. |
| `with_*()` | Callers that need to iterate without cloning. Projections, count-or-check operations, hot paths. The closure bounds the lock scope. |
| `count()` | Callers that only need a count. Status endpoints, dashboard summaries. |

**Never return a raw `RwLockReadGuard`.** If a caller needs to hold a borrow longer than a method call, that caller should be refactored to use `with_*`.

---

## Mutation API

Command methods use imperative verbs specific to the context. Every mutation method has the same shape: acquire the write lock, mutate, clone a snapshot for the `finalize` pipeline, release the lock, call `finalize`.

```rust
impl <Context> {
    /// Insert or update an item.
    #[tracing::instrument(level = "debug", skip(self, item), fields(<context>.id = %item.id))]
    pub async fn upsert(&self, item: Item) -> Result<(), <Context>Error> {
        let id = item.id.clone();
        let snapshot = {
            let mut st = self.state.write().await;
            if let Some(pos) = st.items.iter().position(|i| i.id == item.id) {
                st.items[pos] = item;
            } else {
                st.items.push(item);
            }
            st.snapshot()
        };
        self.finalize(snapshot, ChangeKind::Upserted, vec![id]).await
    }

    /// Remove an item by ID. Returns whether anything was removed.
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn remove(&self, id: &str) -> Result<bool, <Context>Error> {
        let snapshot = {
            let mut st = self.state.write().await;
            let before = st.items.len();
            st.items.retain(|i| i.id != id);
            if st.items.len() == before {
                return Ok(false);
            }
            st.snapshot()
        };
        self.finalize(snapshot, ChangeKind::Removed, vec![id.to_string()]).await?;
        Ok(true)
    }

    /// Update one item in place. Closure returns `true` if it changed anything.
    #[tracing::instrument(level = "debug", skip(self, mutator))]
    pub async fn update<F>(&self, id: &str, mutator: F) -> Result<bool, <Context>Error>
    where
        F: FnOnce(&mut Item) -> bool,
    {
        let snapshot = {
            let mut st = self.state.write().await;
            let Some(item) = st.items.iter_mut().find(|i| i.id == id) else {
                return Ok(false);
            };
            if !mutator(item) {
                return Ok(false);
            }
            st.snapshot()
        };
        self.finalize(snapshot, ChangeKind::Updated, vec![id.to_string()]).await?;
        Ok(true)
    }
}
```

### Rules for mutation methods

1. **Each method acquires the write lock exactly once.** No release-and-reacquire.
2. **The `snapshot` is produced inside the lock scope**, so it reflects the post-mutation state. Cloning happens before the lock is released.
3. **`finalize` is called outside the lock scope**, because `store.save(...).await` is async I/O and must not hold a lock across await points.
4. **Return `Ok(false)` on no-op cases** (item not found, closure reported no change) *without* calling `finalize`. No event fires on no-op.
5. **Early returns inside the lock scope use `return Ok(...)`** to release the lock cleanly.
6. **The `fields(...)` on `#[tracing::instrument]` include the mutation identifier** (usually the ID being mutated).

### Context-specific verbs

Beyond the standard `upsert` / `remove` / `update`, contexts define verbs that match their domain:

- `Offerings::promote`, `Offerings::demote` (cross-collection moves)
- `Jobs::start`, `Jobs::complete`, `Jobs::fail`
- `Pond::join`, `Pond::invite`, `Pond::revoke`
- `Tool::register_local`, `Tool::apply_remote_beacon`

Every such method follows the same shape: acquire, mutate, snapshot, release, finalize.

---

## The finalize pipeline

`finalize` is the one private method every mutation method calls. It enforces the persist+meter+emit invariant.

```rust
impl <Context> {
    async fn finalize(
        &self,
        snapshot: Vec<Item>,
        kind: ChangeKind,
        affected: Vec<String>,
    ) -> Result<(), <Context>Error> {
        let started = std::time::Instant::now();

        // 1. Persist
        self.store
            .save(&snapshot)
            .await
            .map_err(<Context>Error::Persistence)?;

        // 2. Meter
        self.metrics.record_mutation_latency(Self::NAME, started.elapsed());
        self.metrics.record_domain_event(Self::NAME, kind.name());

        // 3. Emit
        let event = <Context>Changed {
            kind,
            affected,
            timestamp: chrono::Utc::now(),
        };
        let _ = self.changes.send(event);
        // send() returns Err only when there are no receivers, which is fine —
        // projection tasks may not have spawned yet at early boot.

        Ok(())
    }
}
```

### Ordering matters

1. **Persist first.** If persistence fails, no event fires. The aggregate's in-memory state is still mutated, but consumers never learn about a mutation that didn't land on disk. Callers see the error and can retry.
2. **Meter second.** Metrics recording is lock-free (atomic increments on `Arc<DomainMetrics>`) so it never fails.
3. **Emit third.** The event describes a completed, persisted change. Subscribers can trust that `store.load()` will return a state consistent with the event.

### What `finalize` never does

- **It does not retry.** Transient persistence failures propagate to the caller.
- **It does not roll back in-memory state on persistence failure.** The in-memory state is already mutated under the lock. If persistence fails, the aggregate's in-memory state disagrees with disk, but the next successful mutation reconciles them. This is accepted because file-backed persistence failures are rare and the alternative (rollback) doubles the code path complexity.
- **It does not call other aggregates.** Cross-context coordination happens through event subscription, not direct calls inside `finalize`.

---

## Events

Each context publishes one event type. The event describes *what happened* and *which items were affected*, not the full new state.

```rust
// domain/<context>/event.rs

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, serde::Serialize)]
pub struct <Context>Changed {
    pub kind: ChangeKind,
    pub affected: Vec<String>,   // IDs, FQNs, or whatever identifies "what"
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeKind {
    Upserted,
    Removed,
    Updated,
    // context-specific variants:
    // Promoted, Demoted, Completed, Failed, Replaced, Coalesced...
}

impl ChangeKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Upserted => "upserted",
            Self::Removed => "removed",
            Self::Updated => "updated",
            // ...
        }
    }

    /// Whether this kind of change should trigger an immediate topology chirp.
    /// Override per context — some changes are topology-visible, some are not.
    pub fn should_chirp(self) -> bool {
        matches!(self, Self::Upserted | Self::Removed | Self::Updated)
    }
}
```

### Rules for events

1. **One event type per context.** Not one per method. The `kind` field discriminates.
2. **`affected` carries identifiers, not full items.** Subscribers that need full items call `find_by_id()` on the producer.
3. **Events are `Clone`.** Required by `broadcast::Sender`.
4. **Events are `Serialize`.** They are the wire format for SSE and cross-stone beacons. No separate DTO.
5. **Events never carry infrastructure types** (no `PathBuf`, no `reqwest::Error`, no `DateTime<Local>`). Use the domain's own types plus `chrono::DateTime<Utc>`.

### Subscription

```rust
impl <Context> {
    /// Subscribe to mutation events.
    ///
    /// Subscribers should `select!` on the returned receiver alongside their
    /// shutdown token. Lagged receivers must do a full reconcile via
    /// `snapshot()` rather than break the stream.
    pub fn changes(&self) -> broadcast::Receiver<<Context>Changed> {
        self.changes.subscribe()
    }
}
```

Per [code-standards.md §13](../code-standards.md), this is the `{noun}_stream` shape (broadcast, fires on every change). If the context also has lifecycle events (once-per-run transitions), those use the `on_{event}()` shape returning `watch::Receiver<T>`.

---

## Errors

Each context defines a typed error enum. `anyhow` is forbidden as a return type from any public method on the aggregate.

```rust
// domain/<context>/error.rs

use thiserror::Error;

#[derive(Debug, Error)]
pub enum <Context>Error {
    #[error("<context> not found: {id}")]
    NotFound { id: String },

    #[error("invariant violation: {reason}")]
    Invariant { reason: String },

    #[error("persistence failed")]
    Persistence(#[source] anyhow::Error),

    #[error("{0}")]
    // Context-specific variants go here.
    Custom(String),
}
```

### Rules

1. **Use `thiserror`** for the enum. It is already a workspace dependency.
2. **`Persistence` wraps `anyhow::Error`** from the port. This is the one place `anyhow` appears inside the domain, and only as a source type — it never leaves the aggregate method as `anyhow::Error`.
3. **Variants are specific.** `NotFound { id }` is better than `Generic(String)`. The compiler can then drive API error mapping.
4. **The `Display` impl is user-facing.** It flows into API error responses.
5. **Domain invariants violations use their own variant**, not `Invariant { reason }` with a stringly-typed reason. For example, `Offerings` might have `NameCollision { existing_id, incoming_id }`.

### API layer translation

At the API boundary, typed errors map to HTTP responses through a single `IntoResponse` impl per error type:

```rust
impl IntoResponse for <Context>Error {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::NotFound { .. } => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            Self::Invariant { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "INVARIANT"),
            Self::Persistence(_) => (StatusCode::INTERNAL_SERVER_ERROR, "PERSISTENCE_FAILED"),
            // ...
        };
        let body = Json(ErrorResponse {
            code: code.to_string(),
            message: self.to_string(),
        });
        (status, body).into_response()
    }
}
```

This impl lives near the error type, not in the handler. Handlers return `Result<Json<T>, <Context>Error>` and Axum calls `into_response` automatically.

---

## Ports and adapters

Every context declares the infrastructure it needs as a trait (port). Adapters implement the trait. The context holds `Arc<dyn Port>`.

```rust
// domain/<context>/port.rs

use anyhow::Result;
use std::future::Future;
use std::pin::Pin;

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Persistence port for <Context>.
pub trait <Context>Store: Send + Sync {
    fn load(&self) -> BoxFut<'_, Result<Vec<Item>>>;
    fn save<'a>(&'a self, snapshot: &'a [Item]) -> BoxFut<'a, Result<()>>;
}
```

### The `Pin<Box<Future>>` pattern

`async-trait` was removed in ARCH-0007. Domain traits use `Pin<Box<Future>>` return types instead. The `BoxFut` type alias in each `port.rs` file keeps signatures readable.

### Concrete adapters

Concrete adapters live in `src/moss/src/infra/`:

```rust
// infra/<context>_store.rs

use anyhow::Result;
use garden_common::Item;
use std::future::Future;
use std::pin::Pin;

use crate::domain::<context>::port::<Context>Store;

pub struct File<Context>Store;

impl <Context>Store for File<Context>Store {
    fn load(&self) -> Pin<Box<dyn Future<Output = Result<Vec<Item>>> + Send + '_>> {
        Box::pin(async move {
            crate::infra::persistence::load_json::<Vec<Item>>("<context>.json").await
        })
    }

    fn save<'a>(&'a self, snapshot: &'a [Item]) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            crate::infra::persistence::save_json("<context>.json", snapshot).await
        })
    }
}
```

### Rules for ports

1. **Domain modules never import `crate::infra::*`.** Only the port trait is visible inside `domain/<context>/`. The concrete implementation lives in `infra/`.
2. **One port per infrastructure concern.** `<Context>Store` for persistence, `<Context>Transport` for network I/O, `<Context>Clock` for time (if tests need determinism), etc.
3. **Ports are `Send + Sync`.** All adapters run inside tokio tasks.
4. **Ports never take domain types by owned value.** Pass slices, references, or clone-on-caller. This keeps adapter impls simple.
5. **Error types use `anyhow::Error`** at the port boundary, wrapped into typed domain errors by the aggregate's `finalize` method. Adapters are free to use any concrete error type internally as long as they convert at the trait boundary.

### Bootstrap wiring

Adapters are constructed and injected at bootstrap:

```rust
// bootstrap/run.rs

let <context>_store: Arc<dyn <Context>Store> = Arc::new(File<Context>Store);
let <context> = Arc::new(
    <Context>::load(<context>_store, metrics.clone()).await?
);
```

Tests swap the adapter for an in-memory fake:

```rust
// domain/<context>/tests.rs

struct Fake<Context>Store {
    data: std::sync::Mutex<Vec<Item>>,
}

impl <Context>Store for Fake<Context>Store { /* ... */ }
```

---

## Metrics integration

Every aggregate injects `Arc<Metrics>` and records two things per mutation inside `finalize`:

1. **Mutation latency** — from the `Instant::now()` at method entry to the moment the persist call returns.
2. **Domain event** — one count per `ChangeKind` per context.

```rust
self.metrics.record_mutation_latency(Self::NAME, started.elapsed());
self.metrics.record_domain_event(Self::NAME, kind.name());
```

Hot-path metrics recording is lock-free: `Metrics` holds `Arc<DomainMetrics>` values keyed by context name, and `DomainMetrics` uses atomic counters. A `read` lock on the metrics state is held for the lookup, then atomic fetch-adds happen without any lock.

### When a context records additional metrics

- **Readiness transitions** — `metrics.record_task_transition(task_name, new_state)` when a `BackgroundTask` changes state.
- **Subscriber lag** — `metrics.record_subscriber_lag(task_name, skipped)` when `broadcast::Receiver::recv` returns `Err(Lagged(n))`.
- **Custom counters** — contexts that observe their own internal metrics (queue depth, cache hit rate) register them via `metrics.register_custom_counter(name, initial_value)`.

Metrics itself is the Book I deliverable of the epic. Every subsequent context integrates from day one.

---

## Tracing

Every public method on every aggregate carries a `#[tracing::instrument]` attribute.

```rust
#[tracing::instrument(level = "debug", skip(self), fields(offerings.id = %id))]
pub async fn promote(&self, id: &str) -> Result<bool, OfferingsError> { ... }
```

### Rules

1. **Level:** `debug` for routine mutations, `info` for lifecycle events (startup, shutdown), `warn` for recoverable error paths, `error` for fatal paths.
2. **`skip(self)`** is mandatory. The aggregate holds large state and trait objects that would bloat span fields.
3. **`skip(...)`** also hides any parameter not worth capturing (passwords, large blobs, opaque handles). Never capture things that belong in a debug log but not a span.
4. **`fields(...)`** captures identifiers using `%` for `Display` or `?` for `Debug`. The field name is dotted: `<context>.<field>`.
5. **Span name** is implicit from the method name, yielding spans like `offerings::aggregate::promote`. This is filterable with `RUST_LOG=garden_moss::domain::offerings=debug`.

### Not a replacement for log statements

Tracing spans give you "who called what, how long it took, what fields it carried." They do not replace `tracing::info!` / `tracing::warn!` messages for significant events inside a method. Write both.

---

## Projection tasks

Cross-context coordination happens through subscription. A projection task is a `BackgroundTask` (per [ARCH-0015](../decisions/ARCH-0015-task-supervisor-registry.md)) that subscribes to one or more aggregates' `changes()` streams and rebuilds a derived view.

### The template

```rust
// tasks/task_defs/<consumer>_projection.rs

use std::future::Future;
use std::pin::Pin;
use tokio::sync::broadcast::error::RecvError;

use crate::tasks::task_trait::{BackgroundTask, TaskContext, TaskOutcome};

pub struct <Consumer>ProjectionTask;

impl BackgroundTask for <Consumer>ProjectionTask {
    fn name(&self) -> &'static str {
        "<consumer>-projection"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        // dependencies this projection needs before it starts receiving
        &[]
    }

    fn run(
        self: Box<Self>,
        ctx: TaskContext,
    ) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            // Subscribe BEFORE seed so no events are missed in the window
            // between seeding and entering the receive loop.
            let mut feed = ctx.state.<producer>.changes();

            // Seed the projection from current state.
            ctx.state.<consumer>.rebuild_from(&ctx.state.<producer>).await;
            ctx.ready.signal();

            loop {
                tokio::select! {
                    _ = ctx.token.cancelled() => {
                        return TaskOutcome::Cancelled;
                    }
                    msg = feed.recv() => match msg {
                        Ok(event) => {
                            tracing::debug!(
                                kind = ?event.kind,
                                affected = ?event.affected,
                                "<producer>Changed — refreshing projection",
                            );
                            ctx.state.<consumer>.apply(event).await;
                        }
                        Err(RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "<consumer> projection feed lagged — full reconcile");
                            ctx.metrics.record_subscriber_lag(self.name(), skipped);
                            ctx.state.<consumer>.rebuild_from(&ctx.state.<producer>).await;
                        }
                        Err(RecvError::Closed) => {
                            return TaskOutcome::Completed;
                        }
                    }
                }
            }
        })
    }
}
```

### Three non-negotiable rules

1. **Subscribe before seed.** The subscription is established first, then the initial state refresh runs, then the receive loop starts. If a mutation fires during the seed, the broadcast channel buffers it and the loop catches it on the first iteration. This race is how ARCH-0016's `OfferingsProjectionTask` was designed to behave, and every subsequent projection follows suit.
2. **Lag-tolerant.** On `Err(Lagged(n))`, log a warning, record the metric, do a full reconcile via `rebuild_from(...)`, and continue. Never break the stream on lag.
3. **Shutdown-aware.** The `tokio::select!` races the cancellation token against the receive future. On cancel, return `TaskOutcome::Cancelled` immediately.

### Registration

Every projection task is registered in `src/moss/src/tasks/task_registry.rs`:

```rust
tasks.push(Box::new(<Consumer>ProjectionTask));
```

Conditional tasks use the existing `config` flag gating pattern from ARCH-0015.

---

## Test scaffold

Every aggregate has a `tests.rs` sibling file with the following minimum coverage.

### Fakes

Every port trait gets a fake implementation:

```rust
// domain/<context>/tests.rs

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // ── Fakes ───────────────────────────────────────────────────

    #[derive(Default)]
    struct Fake<Context>Store {
        data: Mutex<Vec<Item>>,
        save_count: Mutex<u64>,
    }

    impl <Context>Store for Fake<Context>Store {
        fn load(&self) -> BoxFut<'_, anyhow::Result<Vec<Item>>> {
            Box::pin(async move {
                Ok(self.data.lock().unwrap().clone())
            })
        }

        fn save<'a>(&'a self, snapshot: &'a [Item]) -> BoxFut<'a, anyhow::Result<()>> {
            Box::pin(async move {
                *self.data.lock().unwrap() = snapshot.to_vec();
                *self.save_count.lock().unwrap() += 1;
                Ok(())
            })
        }
    }

    fn test_aggregate() -> (<Context>, Arc<Fake<Context>Store>, Arc<Metrics>) {
        let store = Arc::new(Fake<Context>Store::default());
        let metrics = Arc::new(Metrics::test_instance());
        let agg = <Context>::new(store.clone(), metrics.clone());
        (agg, store, metrics)
    }
```

### Required tests

Every aggregate has these tests, adapted to its specific verbs:

```rust
    #[tokio::test]
    async fn upsert_persists_and_emits() {
        let (agg, store, _) = test_aggregate();
        let mut rx = agg.changes();

        agg.upsert(make_item("a")).await.unwrap();

        assert_eq!(store.data.lock().unwrap().len(), 1);
        let event = rx.recv().await.unwrap();
        assert_eq!(event.kind, ChangeKind::Upserted);
        assert_eq!(event.affected, vec!["a".to_string()]);
    }

    #[tokio::test]
    async fn remove_nonexistent_does_not_emit() {
        let (agg, store, _) = test_aggregate();
        let mut rx = agg.changes();

        assert_eq!(agg.remove("nope").await.unwrap(), false);
        assert_eq!(*store.save_count.lock().unwrap(), 0);

        // No event fired.
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn update_reports_change_flag() {
        let (agg, _, _) = test_aggregate();
        agg.upsert(make_item("a")).await.unwrap();

        let changed = agg.update("a", |i| {
            i.value = 42;
            true
        }).await.unwrap();

        assert!(changed);
        let got = agg.find_by_id("a").await.unwrap();
        assert_eq!(got.value, 42);
    }

    #[tokio::test]
    async fn changes_subscriber_receives_events_in_order() {
        let (agg, _, _) = test_aggregate();
        let mut rx = agg.changes();

        agg.upsert(make_item("a")).await.unwrap();
        agg.upsert(make_item("b")).await.unwrap();
        agg.remove("a").await.unwrap();

        assert_eq!(rx.recv().await.unwrap().kind, ChangeKind::Upserted);
        assert_eq!(rx.recv().await.unwrap().kind, ChangeKind::Upserted);
        assert_eq!(rx.recv().await.unwrap().kind, ChangeKind::Removed);
    }

    #[tokio::test]
    async fn round_trip_persist_reload_preserves_state() {
        let store = Arc::new(Fake<Context>Store::default());
        let metrics = Arc::new(Metrics::test_instance());

        // First aggregate writes some state.
        let agg1 = <Context>::new(store.clone(), metrics.clone());
        agg1.upsert(make_item("a")).await.unwrap();
        agg1.upsert(make_item("b")).await.unwrap();

        // Second aggregate loads from the same store.
        let agg2 = <Context>::load(store.clone(), metrics.clone()).await.unwrap();
        let reloaded = agg2.snapshot().await;
        assert_eq!(reloaded.len(), 2);
    }

    #[tokio::test]
    async fn persistence_failure_propagates() {
        let store = Arc::new(Failing<Context>Store); // rejects all writes
        let metrics = Arc::new(Metrics::test_instance());
        let agg = <Context>::new(store, metrics);

        let err = agg.upsert(make_item("a")).await.unwrap_err();
        assert!(matches!(err, <Context>Error::Persistence(_)));
    }
}
```

### Additional tests per context

Contexts with custom verbs add tests for each verb. `Offerings` tests `promote` and `demote`. `Jobs` tests `start`, `complete`, `fail`. Pattern: one test per verb, plus boundary cases.

---

## Anti-patterns

These patterns are forbidden in domain code. A reviewer rejecting a PR on any of them is correct.

### ❌ `pub` state

```rust
// BAD
pub struct Offerings {
    pub state: RwLock<OfferingsState>,  // anything can write
}
```

Fix: make `state` private, expose methods.

### ❌ Raw lock guard returns

```rust
// BAD
impl Offerings {
    pub async fn write_lock(&self) -> RwLockWriteGuard<'_, OfferingsState> {
        self.state.write().await
    }
}
```

Fix: the aggregate's mutation methods are the only mutation path. If a caller needs a custom mutation, add a method for it or pass a closure to `update`.

### ❌ `anyhow::Error` in domain return types

```rust
// BAD
pub async fn promote(&self, id: &str) -> anyhow::Result<bool> { ... }
```

Fix: define a `<Context>Error` enum and return it.

### ❌ Cross-context direct mutation

```rust
// BAD — Tool directly mutating Offerings
pub async fn rebuild(tool: &Tool, offerings: &Offerings) {
    offerings.upsert(...).await;  // Tool is not a mutator of Offerings
}
```

Fix: Tool subscribes to `Offerings::changes()` and reacts by rebuilding its own projection. Cross-context mutation happens through the target context's own API called by the API layer or by a coordinator, never by a peer domain.

### ❌ Imperative projection refresh

```rust
// BAD — caller remembers to call refresh after mutation
state.offerings.upsert(o).await?;
state.tool.refresh_local_projection().await;  // ← the bug ARCH-0016 fixed
```

Fix: `Tool` subscribes to `Offerings::changes()` via a `BackgroundTask` projection. The caller does not know about the refresh.

### ❌ Shared mutable state not owned by an aggregate

```rust
// BAD
pub struct AppState {
    pub jobs: Arc<RwLock<HashMap<String, Job>>>,  // who owns this?
}
```

Fix: extract a `Jobs` aggregate.

### ❌ Holding a lock across an `await` point

```rust
// BAD
let mut st = self.state.write().await;
st.items.push(item);
self.store.save(&st.items).await?;  // ← still holding the lock
```

Fix: clone a snapshot inside the lock scope, release the lock, then await:

```rust
// GOOD
let snapshot = {
    let mut st = self.state.write().await;
    st.items.push(item);
    st.snapshot()
};
self.store.save(&snapshot).await?;
```

### ❌ `TODO: migrate later` comments with no tracker entry

```rust
// BAD
// TODO: this bypasses the aggregate — migrate in a follow-up
state.offerings.write().await.push(...);
```

Fix: either migrate now, or add an entry to [docs/scaffolding.md](../scaffolding.md) with a specific removal trigger and action. Silent TODOs are forbidden by the epic.

### ❌ Importing `crate::infra::*` from `domain/`

```rust
// BAD — domain/offerings/aggregate.rs
use crate::infra::save_offerings;
```

Fix: declare a port trait in `domain/offerings/port.rs`, implement it in `infra/offerings_store.rs`, inject at construction.

---

## Worked example — Offerings

[ARCH-0016](../decisions/ARCH-0016-offerings-aggregate-domain.md) implements this pattern as the reference. Read the source in `src/moss/src/domain/offerings/` to see every element in practice:

| Pattern element | Offerings instance |
|-----------------|--------------------|
| Module layout | `domain/offerings/{mod.rs, aggregate.rs, event.rs, guard.rs, store.rs, catalog.rs}` |
| Private state | `OfferingsState { active, candidates }` inside `aggregate.rs`, `pub(super)` visibility |
| Port | `OfferingStore` trait in `store.rs`, `FileOfferingStore` impl wrapping `crate::infra::{load,save}_offerings` |
| Read API | `snapshot`, `candidates_snapshot`, `find_by_id`, `find_by_name`, `with_active`, `with_candidates`, `count_active` |
| Mutation API | `upsert`, `remove`, `remove_by_name`, `update`, `update_by_name`, `update_candidate`, `update_batch`, `replace_active`, `coalesce_duplicates`, `promote`, `demote` |
| `finalize` pipeline | Lock-scoped mutation → snapshot clone → `store.save` → `self.changes.send` |
| Event | `OfferingsChanged { kind: ChangeKind, affected: Vec<String>, timestamp }` with 8 `ChangeKind` variants |
| Projection task | `OfferingsProjectionTask` in `tasks/task_defs/offerings_projection.rs`, subscribes to `offerings.changes()` and calls `refresh_local_tools_projection` + `sync_self_services` |
| Strangler vine | `ActiveGuard`/`CandidatesGuard` in `guard.rs`, tracked in [scaffolding.md](../scaffolding.md) for Book XVIII removal |

The two things Offerings does **differently** from the pattern spec, documented as deliberate Phase 1 compromises in ARCH-0016:

1. **Return type is not yet typed.** Methods return `bool` and `()` rather than `Result<bool, OfferingsError>`. Typed errors arrive when the aggregate is audited against this spec during the epic.
2. **Metrics injection is not yet wired.** The aggregate does not hold `Arc<Metrics>` because Metrics does not yet exist. Book I adds it and Book II audits Offerings to complete the pattern compliance.

Neither is a deviation from the pattern — they are known gaps that close as the epic progresses.

---

## Checklist for new contexts

Before opening a PR introducing a new bounded context, verify every box:

**Planning:**

- [ ] The book's Chapter 1 re-evaluated the epic plan against current code (per [ARCH-0017 Discovery Mandate](../decisions/ARCH-0017-ddd-monolith-epic.md#the-discovery-mandate)). If the plan needed changes, they were made before any code was written and logged in the ARCH-0017 revision history.
- [ ] Material plan changes (scope, sequencing, context name, dependencies) were surfaced to the user for visibility.
- [ ] [docs/reference/context-map.md](../reference/context-map.md) is updated with the context's target-state entry.

**Structure:**

- [ ] `domain/<context>/` directory exists with `mod.rs`, `aggregate.rs`, `event.rs`, `error.rs`, `port.rs`, `tests.rs`.
- [ ] Aggregate struct has `state: RwLock<State>` with no `pub` qualifier.
- [ ] Aggregate struct has `store: Arc<dyn <Context>Store>`, `metrics: Arc<Metrics>`, and `changes: broadcast::Sender<_>` — all private.
- [ ] `new` and/or `load` constructors register the context with metrics (`metrics.register_domain(Self::NAME)`).
- [ ] No `pub` fields on the aggregate.
- [ ] Read API includes at least `snapshot` and one of `find_by_id` / `find_by_name`.
- [ ] Read API includes `with_*` for scoped closure access (if hot-path iteration is expected).
- [ ] Mutation API includes `upsert`, `remove`, `update` (or context-specific verbs that cover the same surface).
- [ ] Every mutation method goes through `finalize(snapshot, kind, affected)`.
- [ ] `finalize` persists first, meters second, emits third.
- [ ] No method holds a lock across an `await` point.
- [ ] No method returns a raw `RwLockReadGuard` or `RwLockWriteGuard`.
- [ ] `<Context>Error` enum uses `thiserror`, with specific variants for domain failure modes.
- [ ] Every public method has `#[tracing::instrument(level = "debug", skip(self))]` with context-scoped field names.
- [ ] `changes()` returns a fresh `broadcast::Receiver` on each call.
- [ ] `ChangeKind::should_chirp()` is implemented with explicit per-variant logic.
- [ ] Ports use `Pin<Box<Future>>` return types, not `async fn` in traits, not `async-trait`.
- [ ] No `use crate::infra::*` inside `domain/<context>/`.
- [ ] `tests.rs` has fakes for every port and the five required tests (upsert, remove no-op, update flag, subscriber order, round-trip).
- [ ] Any projection task subscribing to this context uses subscribe-before-seed ordering.
- [ ] Any projection task handles `RecvError::Lagged` by full reconcile, not by breaking the stream.
- [ ] Context is registered in [context-map.md](../reference/context-map.md) with its owns/emits/subscribes/ports.
- [ ] Any new domain terms are added to [glossary.md](../glossary.md).
- [ ] Any scaffolds introduced (including temporary shims with removal triggers) are logged in [scaffolding.md](../scaffolding.md).

---

## Documented deviations

The pattern below is the default shape. Five deviations are documented as first-class variants rather than special cases in individual ADRs:

### Ephemeral aggregates (no Store port)

Some aggregates have no persistence — their state is rebuilt from other domains plus runtime sources on every startup, and no saved invariant survives a process restart. These aggregates are defined as **ephemeral**:

- **No `Store` port** on the aggregate.
- **No `load` on construction**.
- **No `save` in `finalize`** — the `finalize` step only records metrics and emits events.
- **Same typed-command, typed-query, `changes()` broadcast shape** as persistent aggregates otherwise.

Current instances:

| Aggregate | Rebuilt from | Book |
|-----------|--------------|------|
| **Metrics** | counters start at zero; state is observation data, not domain truth | I (ARCH-0018) |
| **Resources** | `Current::Resources` hardware snapshots read from the OS on demand | I (rename only) |
| **Tool** | `Offerings::changes()` projection + storage volumes + remote beacons + gateway TTL reaping | II (ARCH-0019) |

**When to use**: the aggregate's state is **observation** (metrics, resources) or **cache** (tool registry rebuilt from source-of-truth domains + runtime events). Persistence would duplicate state that already has an authoritative source elsewhere.

**When NOT to use**: the aggregate owns domain truth that must survive restart (offerings, pond, harvests, nurturing). Those stay with `Store` ports.

### Dual event streams (internal + wire format)

Most aggregates expose a single `changes()` stream carrying a domain event type. Some — notably `Tool` — expose **two parallel streams** from the same command gateway:

- `changes()` → internal `XxxChanged` domain event (rich metadata, process-local subscribers).
- `delta_stream()` (or equivalent) → wire-format event type that predates the aggregate extraction and cannot be collapsed without breaking external consumers.

Both streams are fed atomically from every command. The wire format is a pre-existing consumer-facing contract (SSE clients, UDP beacon receivers, peer stones); the domain event is the refactor's richer shape that will never leave the process.

**When to use**: an existing wire format is already consumed by clients that the book is not migrating. Keep both; document the deviation in the ADR.

**When NOT to use**: greenfield aggregates that own their own wire format. Emit one event type.

### Typed errors (first-class domain error enums)

The pattern default is `anyhow::Result` for command return types — sufficient when the aggregate is infallible (Metrics, Jobs) or when failure modes are unstructured (Topology). When commands have structured, domain-meaningful failure modes worth propagating (disk I/O variants, per-item compilation errors, fingerprint hash failures), use a typed `thiserror` enum:

```rust
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("failed to hash manifests for fingerprint")]
    ManifestHashFailed(#[source] anyhow::Error),
    #[error("failed to compile offering {offering}")]
    CompilationFailed { offering: String, #[source] source: anyhow::Error },
    #[error("failed to read catalog cache from disk")]
    CacheReadFailed(#[source] anyhow::Error),
    #[error("failed to write catalog cache to disk")]
    CacheWriteFailed(#[source] anyhow::Error),
}
```

Commands return `Result<(), CatalogError>`. API handlers at the boundary wrap into `anyhow::Error` for the existing 5xx path; domain-internal callers can pattern-match on failure mode.

**When to use**: persistent aggregates with distinct I/O failure paths, or any aggregate where callers benefit from matching on error variants rather than parsing error messages.

**When NOT to use**: ephemeral aggregates with no persistence and no domain invariants to violate (Metrics, Jobs). Use infallible mutations instead.

**First application**: Catalog aggregate (ARCH-0022, Book V) — the first in the epic with typed `CatalogError`.

### Owned-value queries (no borrowed references across locks)

Query methods on an aggregate with `RwLock`-protected state cannot return references into the inner state because the lock guard drops at the method boundary. Two shapes are possible:

1. Return owned clones (`Vec<Offering>`, `Option<RegistryEntry>`). Simple. Clone cost per call.
2. Provide a `with_active<F, R>(&self, f: F) -> R` closure method that holds the guard for the closure's duration.

The pattern default is **owned clones** — they are simpler at the call site, and the clone cost is dwarfed by the lock-acquire cost for all but the hottest paths. Hot-path callers get dedicated typed methods that return already-filtered results (`Tool::storage_primary`, `Tool::find_s3_gateways`) rather than iterating a cloned `Vec`.

**When to use closure-style queries**: proven hot-path performance regressions. Never by default.

### Lock-free state (no internal RwLock)

The standard pattern uses `RwLock<State>` to protect the aggregate's mutable interior. Some aggregates have state that is structurally immutable after bootstrap (the shape of the `HashMap` never changes) and where mutations are handled by an inherently thread-safe primitive (`watch::Sender::send_modify`, atomic operations).

In these cases, the `RwLock` is unnecessary overhead. The aggregate stores a plain `HashMap` populated during single-threaded bootstrap (via `register()` calls) and never structurally modified afterward. Mutations flow through the inherently thread-safe channel primitives.

**When to use**: the aggregate's state map is frozen after a single-threaded registration phase, and mutations go through a thread-safe channel type.

**Aggregate using this deviation**: Subsystems (Book VI — `HashMap<String, watch::Sender<bool>>`, frozen after bootstrap, mutated via `watch::Sender::send_modify`).

---

## References

- [ARCH-0017](../decisions/ARCH-0017-ddd-monolith-epic.md) — the epic this pattern serves
- [ARCH-0016](../decisions/ARCH-0016-offerings-aggregate-domain.md) — the first application of the pattern, referenced as the worked example
- [ARCH-0015](../decisions/ARCH-0015-task-supervisor-registry.md) — the `BackgroundTask` trait used by projection tasks
- [ARCH-0007](../decisions/ARCH-0007-monomorphic-domain-traits.md) — establishes the `Pin<Box<Future>>` port pattern (no `async-trait`)
- [ARCH-0004](../decisions/ARCH-0004-appstate-domain-context-extraction.md) — original domain context extraction that seeded the structure
- [code-standards.md](../code-standards.md) — the authoritative style guide this pattern implements
- [glossary.md](../glossary.md) — ubiquitous language reference
- [context-map.md](../reference/context-map.md) — live map of every bounded context in moss
- [scaffolding.md](../scaffolding.md) — tracker for intermediate-state code
