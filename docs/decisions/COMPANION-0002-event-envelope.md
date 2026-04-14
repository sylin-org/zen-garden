---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-13
canonical: true
---

# COMPANION-0002: Event Envelope — Book I of COMPANION-0001

**Date**: 2026-04-13
**Status**: Accepted
**Book**: I of [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)
**Depends on**: [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) (epic + pattern spec)

## Context

Book I is the foundation of the companion integration platform: the uniform event envelope that every subsequent book stacks on. No event bus exists yet, no orchestrator, no adapters — just the type that every future piece of code will pass around.

Per the Discovery Mandate in COMPANION-0001, Chapter 0 re-evaluated the plan against the live code. Findings:

### What the re-evaluation found

1. **`uuid` and `chrono` are workspace dependencies but not direct `companion-sdk` dependencies.** `garden-common` uses both. The SDK inherits them transitively through its dependency on `garden-common`, but to expose them in the SDK's public API (as `EventId = uuid::Uuid`, `timestamp: DateTime<Utc>`) they must be declared as direct deps.

2. **A GUIDv7 helper exists in `garden-common::utils::ids` but returns `String`.** `generate_guidv7() -> String` wraps `uuid::Uuid::now_v7().to_string()`. For `EventId`, a string is wasteful — we want a typed `Uuid` for efficient hashing (dedup cache), natural ordering (time-sort), and direct comparison. Book I uses `uuid::Uuid::now_v7()` directly; the existing string helper stays untouched for callers that need string form.

3. **The SDK's current layout is flat** (`cli.rs`, `runtime.rs`, `sse.rs`, etc.). The COMPANION-0001 pattern spec anticipates a `garden/` subdirectory holding the Garden-context modules. Book I creates this directory; Books II–V populate its siblings.

4. **Rust `Any` / trait-object downcasting**. `Arc<dyn EventPayload>` cannot be downcast to `&T` directly. The idiomatic fix is an `as_any(&self) -> &dyn Any` method on the trait. This is minimal boilerplate (one-line impl per payload) and works across all Rust versions. Trait upcasting for `Any` is stable in Rust 1.86+, but using `as_any` keeps Book I compatible with earlier toolchains and sidesteps subtle coercion gotchas.

5. **Coalescing flag accessibility at runtime.** `EventPayload::COALESCING` is a const — accessible when the payload type is known, invisible behind `Arc<dyn EventPayload>`. Pulse (Book II) will need to read it without knowing the type. Solution: add a default trait method `fn is_coalescing(&self) -> bool { Self::COALESCING }`. Zero-boilerplate for implementations; runtime-accessible for Pulse.

No plan changes versus COMPANION-0001. Book I's scope holds.

## Decision

Introduce the event envelope in a new `garden::event` module within `companion-sdk`, matching the pattern-spec layout. The envelope, payload trait, and identifier type form a complete self-contained unit — no dependencies on Pulse, Garden, Transport, or Adapter (which land in Books II–VI).

### The types

```rust
// src/companion-sdk/src/garden/event.rs

pub type EventId = uuid::Uuid;

pub fn new_event_id() -> EventId {
    uuid::Uuid::now_v7()
}

pub trait EventPayload: Any + Send + Sync + Debug {
    /// Namespaced kind identifier matching the envelope's `kind` field.
    const KIND: &'static str;

    /// True if Pulse may coalesce rapid bursts to the latest value.
    /// State-delta events (LoadUpdated, HealthChanged) override to true.
    const COALESCING: bool = false;

    /// Downcast support — implementations return `self`.
    fn as_any(&self) -> &dyn Any;

    /// Runtime accessor for COALESCING (Pulse reads this through the trait object).
    fn is_coalescing(&self) -> bool {
        Self::COALESCING
    }
}

#[derive(Clone, Debug)]
pub struct Event {
    pub id: EventId,
    pub timestamp: DateTime<Utc>,
    pub kind: &'static str,
    pub payload: Arc<dyn EventPayload>,
}

impl Event {
    pub fn new<P: EventPayload>(payload: P) -> Self;
    pub fn with_metadata<P: EventPayload>(id: EventId, timestamp: DateTime<Utc>, payload: P) -> Self;
    pub fn payload<T: EventPayload>(&self) -> Option<&T>;
    pub fn is<T: EventPayload>(&self) -> bool;
    pub fn on<T: EventPayload>(&self, f: impl FnOnce(&T)) -> &Self;
}
```

### Kind validation

Two free functions; Book II's Pulse will call them at `ingest()`.

```rust
/// True if `kind` follows the namespace convention (at least 3 dot-separated
/// lowercase-ASCII parts).
pub fn is_valid_kind(kind: &str) -> bool;

/// Extract the namespace prefix (first dot-separated part).
pub fn kind_namespace(kind: &str) -> Option<&str>;
```

Book I ships only **syntactic** validation (shape of the string). Book II's Pulse will add **semantic** validation (is the namespace registered).

### Module layout

```
src/companion-sdk/src/
├── garden/            # NEW — Garden bounded context root
│   ├── mod.rs         # re-exports
│   └── event.rs       # Event, EventId, EventPayload, kind helpers
├── cli.rs             # unchanged
├── dependencies.rs    # unchanged
├── handler.rs         # unchanged
├── lib.rs             # + pub use garden::event::*
├── runtime.rs         # unchanged
├── server.rs          # unchanged
├── sse.rs             # unchanged
└── state.rs           # unchanged
```

Nothing existing is touched. Book I is purely additive.

### Dependencies

Add to `src/companion-sdk/Cargo.toml`:

```toml
uuid = { workspace = true, features = ["v7"] }
chrono = { workspace = true, features = ["serde"] }
```

Already in workspace; just promoting to direct deps.

## Implementation plan

**Chapter 1 (this ADR)** — land this document.

**Chapter 2** — implement the event module:
- Add `uuid` + `chrono` direct deps to `src/companion-sdk/Cargo.toml`
- Create `src/companion-sdk/src/garden/mod.rs` and `event.rs` with the types above
- Re-export from `src/companion-sdk/src/lib.rs` prelude
- Doc comments on every public item

**Chapter 3** — unit tests (in `event.rs`'s `tests` submodule):
- Envelope construction via `Event::new` produces a unique id and current timestamp
- `Event::payload::<T>()` returns `Some(&T)` when kind matches, `None` when it doesn't
- `Event::is::<T>()` agrees with the downcast result
- `Event::on::<T>(...)` invokes the closure only when the kind matches; fluent chaining works
- GUIDv7 ordering: two events created 2ms apart sort in creation order
- `EventPayload::is_coalescing()` reflects the const
- `is_valid_kind`: accepts `"core.stone.health.changed"`, `"firefly.command.brightness"`; rejects uppercase, empty parts, fewer than 3 parts, non-ASCII, symbols other than `-`
- `kind_namespace`: extracts `"core"` from `"core.stone.health.changed"`, `None` from `"flat"`

**Chapter 4** — verify + close:
- `cargo check --package garden-companion-sdk` green
- `cargo test --package garden-companion-sdk` green
- `cargo clippy --package garden-companion-sdk -- -D warnings` green
- Update COMPANION-0001 revision history (Book I closed)

Ships green to `dev` at each chapter commit.

## Exit criteria

1. `use garden_companion_sdk::{Event, EventId, EventPayload};` compiles in a downstream consumer.
2. A test payload type can be defined in ~5 lines, wrapped in `Event::new`, and downcast round-trip.
3. `is_valid_kind("core.stone.health.changed")` returns `true`.
4. `is_valid_kind("BAD.kind")` returns `false`.
5. GUIDv7 ordering holds across a 2ms pause.
6. `cargo check --all` green (trivial — additive change).
7. `cargo test --package garden-companion-sdk` green with the new tests passing.
8. COMPANION-0001 revision history amended with Book I closure.

## Out of scope (deferred to later books)

| Item | Book |
|------|------|
| Core payload types (HealthChanged, LoadUpdated, ServiceStarted, …) | Book V (populated alongside Garden projection) |
| Namespace registration (semantic kind validation) | Book II (Pulse) |
| Serde impls for Event (wire format) | Book III (Transport) |
| Pulse's use of `is_coalescing()` | Book II |
| `GardenSnapshot` synthetic event | Book V |
| Command payload types and `core.command.result` | Book III (CommandTransport) |
| Derive macro for `EventPayload` (`#[derive(EventPayload)]`) | Deferred — only if Book V–VIII boilerplate becomes painful |

## References

- [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) — the epic
- [companion-architecture.md](../specs/companion-architecture.md) — pattern spec (see §The event envelope and §Kind namespace convention)
- [RFC 9562](https://datatracker.ietf.org/doc/rfc9562/) — UUID Version 7 specification
