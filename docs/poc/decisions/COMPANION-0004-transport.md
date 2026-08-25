---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-13
canonical: true
completed: 2026-04-13
---

# COMPANION-0004: Transport — Book III of COMPANION-0001

**Date**: 2026-04-13
**Status**: Completed (2026-04-13)
**Book**: III of [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)
**Depends on**: [COMPANION-0003](COMPANION-0003-pulse.md) (Pulse orchestrator), [COMPANION-0002](COMPANION-0002-event-envelope.md) (event envelope), [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) (epic + pattern spec)

## Context

Book III lands the input side of the Garden context: event sources. Two initial implementations ship together because they are the symmetric pair the pattern spec commits to — `SseTransport` (moss presence stream in) and `CommandTransport` (HTTP commands in, command results out). Both publish into `Pulse`; adapters subscribe.

Per the Discovery Mandate in COMPANION-0001, Ch0 re-evaluated the plan against the live code. Findings:

### What the re-evaluation found

1. **Wire event kinds don't match our namespace.** `garden-common::presence::event_types` defines kinds as two-level strings: `"presence.snapshot"`, `"stone.load.updated"`, `"service.started"`, `"storage.connected"`. Our `is_valid_kind` requires at least three dot-separated parts, and our namespace convention reserves `core.*` for SDK-defined events. **SseTransport is therefore an anti-corruption layer** (per DDD) — it translates wire kinds to canonical kinds (`"stone.load.updated"` → `"core.stone.load.updated"`) when wrapping raw frames in `Event` envelopes. Moss keeps emitting its legacy kinds; the translation happens once, at the transport boundary.

2. **Wire types can implement `EventPayload` directly.** `PresenceSnapshot`, `StoneHealthChangedPayload`, `StoneLoadUpdatedPayload` live in `garden-common::presence`. `EventPayload` is a local trait on `companion-sdk`. Local trait + foreign type is allowed by the orphan rule, so Book III implements `EventPayload` for each wire type — with the translated `core.*` kind string. Zero duplication between wire format and payload shape. Book V (Garden) will project from these wire-type payloads into the shared domain types from Book IV, but Book III does not need domain types to compile.

3. **`tokio-util` must become a companion-sdk direct dep.** Moss uses `tokio_util::sync::CancellationToken` for its shutdown coordination; the `Transport` trait in Book III accepts a `CancellationToken` parameter. `tokio-util = "0.7"` is not a workspace dep but is a moss direct dep; `companion-sdk` adds it the same way.

4. **The old paths (`SseClient`, `CompanionRuntime`, `CommandHandler`) stay in place during Book III.** Firefly and cricket still use them; Book VIII replaces both crates wholesale and removes the old paths then. Book III is purely additive to `companion-sdk`. Three scaffold entries are registered in [scaffolding.md](../scaffolding.md) — `companion-old-sse-client`, `companion-old-command-handler`, `companion-old-companion-runtime` — with removal trigger Book VIII Ch6.

5. **CommandInvocation stays generic.** The current `CommandHandler` trait deals with `raw_args: Vec<String>`. Rather than introduce typed command events per companion (`firefly.command.brightness` etc.) in Book III, define **one generic command event** with `correlation_id` + `raw_args`. Adapters (Book VIII) parse args themselves. Typed per-companion command events can evolve later without changing the transport. One payload, one pattern, one concern at a time.

6. **No automatic coalesce-flush timer in Book III.** `Pulse::flush_coalesced` is a method the transports don't call. The `Companion` struct in Book VII wires a `tokio::time::interval` that drives the flush. For Book III, integration tests that exercise coalesced events call `flush_coalesced` explicitly.

No plan change vs COMPANION-0001 beyond what was already anticipated. Book III's scope holds.

## Decision

Introduce `Transport` as a public trait and ship two implementations (`SseTransport`, `CommandTransport`) plus the generic command event pair (`CommandInvocation`, `CommandResult`). All new code under `src/companion-sdk/src/garden/` alongside the existing `event.rs` and `pulse.rs`.

### The Transport trait

```rust
use tokio_util::sync::CancellationToken;
use std::future::Future;
use std::pin::Pin;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Transport: Send + 'static {
    /// Run until `shutdown` is cancelled. The transport publishes events
    /// into the provided Pulse and (optionally) observes command-result
    /// events for request/response correlation.
    fn run(
        self: Box<Self>,
        pulse: Arc<Pulse>,
        shutdown: CancellationToken,
    ) -> BoxFuture<'static, ()>;

    /// Kinds this transport emits. Called by Companion at construction
    /// to register namespaces with Pulse automatically.
    fn emitted_kinds(&self) -> &'static [&'static str];
}
```

The `emitted_kinds` method is a small quality-of-life addition — `Companion::run` (Book VII) walks every registered transport and calls `pulse.register_namespace(...)` with the namespace prefix of each emitted kind. No transport needs to remember to register its namespace.

### Wire-type payloads

Book III declares `EventPayload` impls for the existing `garden-common::presence` wire types. Each impl carries its own `core.*` kind. The full list (matching the existing moss event surface):

```rust
// garden-common::presence::PresenceSnapshot
impl EventPayload for PresenceSnapshot {
    const KIND: &'static str = "core.presence.snapshot";
    fn as_any(&self) -> &dyn Any { self }
}

// garden-common::presence::StoneHealthChangedPayload
impl EventPayload for StoneHealthChangedPayload {
    const KIND: &'static str = "core.stone.health.changed";
    fn as_any(&self) -> &dyn Any { self }
}

// garden-common::presence::StoneLoadUpdatedPayload
impl EventPayload for StoneLoadUpdatedPayload {
    const KIND: &'static str = "core.stone.load.updated";
    const COALESCING: bool = true;   // state-delta — coalesces
    fn as_any(&self) -> &dyn Any { self }
}
```

Other wire events (`service.started`, `service.stopped`, `stone.tended`, `storage.connected`, `storage.removed`, `storage.detected`, etc.) follow the same shape. Events whose payload is generic JSON get a thin `GenericWirePayload { data: serde_json::Value }` wrapper so all wire events can pass through even if we haven't defined a typed payload for them yet.

Mapping table (wire kind → our kind) lives in a private `kind_map` helper in the SseTransport module. Extending the table is the only code change needed to handle a new moss event type.

### SseTransport

```rust
pub struct SseTransport {
    endpoint: String,            // e.g. "http://localhost:7185"
    path: String,                // e.g. PRESENCE_STREAM_PATH
    reconnect_delay: Duration,   // exponential backoff base
}

impl SseTransport {
    pub fn new(endpoint: impl Into<String>) -> Self;
    pub fn with_path(self, path: impl Into<String>) -> Self;
}

impl Transport for SseTransport { ... }
```

Flow inside `run`:
1. Connect to `{endpoint}{path}` via `reqwest`.
2. Stream frames using the same parser logic as `SseClient` (salvaged verbatim, including the recent multi-line data fix).
3. For each SSE frame `(event_type, data)`:
   - Translate `event_type` to our canonical kind via `kind_map`.
   - Deserialize `data` into the corresponding payload type (`serde_json::from_str`).
   - Wrap in `Event::new(payload)`.
   - Call `pulse.ingest(event)`.
4. On connection drop, log + exponential-backoff reconnect (1, 2, 4, 8, 16, 32 seconds, cap).
5. On shutdown token cancellation, exit cleanly.

Lag / error handling mirrors the moss warn-and-continue pattern: unrecognized event types log warn and continue; malformed JSON logs warn and continues; connection errors retry.

### CommandTransport

```rust
pub struct CommandTransport {
    port: u16,
    response_timeout: Duration,   // default 5s
}

impl CommandTransport {
    pub fn new(port: u16) -> Self;
    pub fn with_timeout(self, timeout: Duration) -> Self;
}

impl Transport for CommandTransport { ... }
```

Three HTTP endpoints, served by axum:

- `POST /command { raw_args: [...] }` — publish a `CommandInvocation` event, wait for matching `CommandResult`s up to `response_timeout`, return aggregated response.
- `POST /shutdown` — triggers the transport's shutdown (and the containing Companion's shutdown via the token).
- `GET /health` — returns `{"status":"healthy"}`.

### Command event pair

```rust
#[derive(Debug, Clone)]
pub struct CommandInvocation {
    /// GUIDv7 linking invocation to its result(s).
    pub correlation_id: EventId,

    /// Raw positional args (first is the command name).
    pub raw_args: Vec<String>,
}

impl EventPayload for CommandInvocation {
    const KIND: &'static str = "core.command.invocation";
    fn as_any(&self) -> &dyn Any { self }
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub correlation_id: EventId,
    pub outcome: CommandOutcome,
    /// Identifier of the adapter that produced this result, for observability.
    pub from: String,
}

impl EventPayload for CommandResult {
    const KIND: &'static str = "core.command.result";
    fn as_any(&self) -> &dyn Any { self }
}

#[derive(Debug, Clone)]
pub enum CommandOutcome {
    Success { output: Option<String> },
    Error { message: String },
}
```

Flow inside `CommandTransport::run`:
1. Spawn an axum server on `127.0.0.1:{port}`.
2. Spawn a correlation-collector task that subscribes to `pulse` and, for each `CommandResult` event, routes it into a correlation map keyed by `correlation_id`.
3. The HTTP handler for `POST /command`:
   - Generates a `correlation_id` via `new_event_id()`.
   - Registers an `mpsc::UnboundedSender<CommandResult>` in the correlation map.
   - Publishes `Event::new(CommandInvocation { correlation_id, raw_args })` to pulse.
   - Collects results from the receiver until `response_timeout` elapses OR the receiver yields a sentinel (future enhancement).
   - Removes the correlation entry.
   - Aggregates the results (see below) and returns as `CommandResponse`.

### Result aggregation

Returned to the HTTP caller as a `CommandResponse` (existing `garden_common::command_manifest::CommandResponse`) with these rules:

- **Zero results** within timeout → `CommandResponse::error("No handler responded within {timeout}ms")`.
- **Exactly one result** → translate `CommandOutcome::Success { output }` to `CommandResponse::success(output)`, or `Error { message }` to `CommandResponse::error(message)`.
- **Multiple results** → aggregate. If all succeeded, return success with a joined output summary. If any errored, return error with a joined error summary including the `from` field.

### Namespace registration

`Companion::run` (Book VII) walks `transport.emitted_kinds()` and calls `pulse.register_namespace(...)` for each unique namespace prefix. For Book III the emitted kinds are `"core.*"`; an `SseTransport + CommandTransport` pair registers just `"core"`. Tests register manually.

### Scaffold entries (registered in scaffolding.md)

Three scaffolds are introduced by this book, all removed by Book VIII Ch6:

- `companion-old-sse-client`: `src/companion-sdk/src/sse.rs` (`SseClient`, `SseEvent`, `EventHandler` trait) coexists with `SseTransport` until firefly/cricket migrate to the new arch.
- `companion-old-command-handler`: `src/companion-sdk/src/handler.rs` (`CommandHandler` trait) + `src/companion-sdk/src/server.rs` (old HTTP server) coexist with `CommandTransport`.
- `companion-old-companion-runtime`: `src/companion-sdk/src/runtime.rs` (`CompanionRuntime`) coexists with Book VII's `Companion`.

## Implementation plan

**Chapter 1 (this ADR)** — land this document.

**Chapter 2** — Transport trait + wire payloads + SseTransport:
- `src/companion-sdk/src/garden/transport.rs`: `Transport` trait, `BoxFuture` re-export
- `src/companion-sdk/src/garden/core_payloads.rs`: `EventPayload` impls on `garden-common::presence` wire types + `GenericWirePayload`
- `src/companion-sdk/src/garden/sse_transport.rs`: `SseTransport`, wire-kind translation map, salvaged parser + backoff
- `tokio-util = "0.7"` added to `src/companion-sdk/Cargo.toml`
- Re-exports from `garden/mod.rs` and prelude
- Unit tests: trait object construction, wire-kind translation, parser correctness, reconnect backoff timing

**Chapter 3** — CommandTransport + correlation:
- `src/companion-sdk/src/garden/command_transport.rs`: `CommandTransport`, `CommandInvocation`, `CommandResult`, `CommandOutcome`, correlation collector task, axum server, result aggregator
- Re-exports from `garden/mod.rs` and prelude
- Integration-style tests (single-process): publish invocation, feed mock results, verify HTTP response aggregation

**Chapter 4** — scaffolding + book close:
- Register three scaffold entries in `docs/scaffolding.md`
- Update COMPANION-0001 revision history
- Amend pattern spec if any book-close adjustments are needed (e.g., confirming the generic command event design)

Each chapter ships green to `dev`.

## Exit criteria

1. `use garden_companion_sdk::garden::{Transport, SseTransport, CommandTransport, CommandInvocation, CommandResult};` compiles.
2. A `Transport`-typed boxed object can be constructed from each impl and stored in a `Vec<Box<dyn Transport>>`.
3. Given a running moss at `127.0.0.1:7185`, an SseTransport subscribed to `/api/v1/stone/presence/stream` receives and publishes at least one event to a test Pulse (integration test; skipped if no moss available).
4. A `CommandTransport` + `Pulse` pair round-trips: a test publishes a `CommandResult` for a generated `correlation_id` in response to a `CommandInvocation`, and the HTTP handler returns the aggregated `CommandResponse`.
5. Wire kinds are translated correctly: `stone.load.updated` → `core.stone.load.updated`, and so on for each supported kind.
6. `cargo check --all` green.
7. `cargo test --package garden-companion-sdk garden::transport garden::sse_transport garden::command_transport` green.
8. `cargo clippy --package garden-companion-sdk -- -D warnings` green.
9. `docs/scaffolding.md` lists three new `companion-*` entries with removal trigger Book VIII Ch6.
10. COMPANION-0001 revision history amended with Book III closure.

## Out of scope (deferred)

| Item | Book |
|------|------|
| Typed per-companion command events (`firefly.command.brightness`) | Book VIII or later refinement ADR |
| Automatic coalesce-flush timer | Book VII (Companion) |
| Graceful HTTP shutdown (drain in-flight requests) | Book VII (Companion handles the shutdown orchestration) |
| Unknown-wire-kind handling beyond log-warn-continue | Case-by-case later |
| Additional transports (MQTT, file-watch, webhook) | Future targeted ADRs after the epic closes |
| Removing `SseClient` / `CompanionRuntime` / `CommandHandler` | Book VIII Ch6 |

## Closure notes (2026-04-13)

Book III closed with all exit criteria met. Summary of what shipped:

- **Three new modules** under `src/companion-sdk/src/garden/`: `transport.rs` (`Transport` trait + `BoxFuture` alias), `core_payloads.rs` (9 typed payloads + `wire_to_core_kind` anti-corruption translator + `WIRE_KIND_MAP` / `SSE_EMITTED_KINDS`), `sse_transport.rs` (`SseTransport` with salvaged frame parser, exponential backoff, cancellation-aware main loop), `command_transport.rs` (`CommandTransport` + `CommandInvocation`/`CommandResult`/`CommandOutcome` payloads + correlation map + axum HTTP server + result aggregation).
- **`EventPayload` impls on three `garden-common::presence` wire types** (PresenceSnapshot, StoneHealthChangedPayload, StoneLoadUpdatedPayload) — via orphan rule, local trait on foreign types. Six new SDK-local typed payloads for discrete events (Tended, ServiceStarted/Stopped, StorageConnected/Detected/Removed).
- **One new direct dep**: `tokio-util = "0.7"` for `CancellationToken`, matching moss's existing pattern.
- **Three scaffold entries** registered in [scaffolding.md](../scaffolding.md): `companion-old-sse-client`, `companion-old-command-handler`, `companion-old-companion-runtime`. All three removal-triggered by Book VIII Chapter 6.
- **58 new tests** (2 transport, 9 core_payloads, 14 sse_transport, 13 command_transport, plus the in-module tests in transport.rs). **80 total garden tests** passing across Books I-III.
- **Verification**: `cargo check --all`, `cargo test --package garden-companion-sdk garden::`, `cargo clippy --package garden-companion-sdk -- -D warnings` — all green.

### Minor refinements during implementation (not plan changes)

- **`GenericWirePayload` dropped.** The initial draft included a generic JSON-carrying payload for kinds we hadn't typed yet. The blanket `DynPayload` impl makes runtime-kind-bearing payloads awkward (the const is the only source of truth). Rewrote to define one typed payload per supported kind instead; unknown wire kinds log at trace and are skipped. Cleaner end-state, no hack.
- **`SSE_EMITTED_KINDS` / `WIRE_KIND_MAP` as crate-visible constants.** Used by `SseTransport::emitted_kinds` and by the integrity test that asserts the two sets match. Guarantees adding a new supported kind can't drift.
- **Per-kind dispatch in `build_event`.** `match core_kind { KIND_X => from_str::<X>(data).ok().map(Event::new), ... }` — the same kind string appears in the `EventPayload` impl and in this dispatcher. Slightly repetitive but completely explicit; Book V can reconsider if a declarative registry is wanted.

### Follow-on work picked up by later books

- Book IV (Domain) promotes shared domain types into `garden-common::domain` and wires them through Garden's projection.
- Book V (Garden) consumes these core payloads and maintains the read-model.
- Book VII (Companion) wires `Companion::run` to walk transports + call `pulse.register_namespace` for every namespace in `transport.emitted_kinds()`.
- Book VIII Chapter 6 deletes `SseClient`, `CommandHandler`, `CompanionRuntime` and the scaffolding entries collapse to zero.

## References

- [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) — the epic
- [COMPANION-0002](COMPANION-0002-event-envelope.md) — Event envelope (Book I)
- [COMPANION-0003](COMPANION-0003-pulse.md) — Pulse orchestrator (Book II)
- [companion-architecture.md §Transport trait](../specs/companion-architecture.md#transport-trait)
- [companion-architecture.md §Commands as events](../specs/companion-architecture.md) (Principle 7)
- [scaffolding.md §Active scaffolds](../scaffolding.md#active-scaffolds) — three `companion-old-*` entries awaiting Book VIII Ch6
