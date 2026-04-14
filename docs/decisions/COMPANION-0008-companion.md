---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-14
canonical: true
---

# COMPANION-0008: Companion Top-Level Runtime — Book VII of COMPANION-0001

**Date**: 2026-04-14
**Status**: Accepted
**Book**: VII of [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)
**Depends on**: [COMPANION-0003](COMPANION-0003-pulse.md), [COMPANION-0004](COMPANION-0004-transport.md), [COMPANION-0006](COMPANION-0006-garden-aggregate.md), [COMPANION-0007](COMPANION-0007-adapters.md)

## Context

Book VII is the **glue book** — the top-level `Companion` struct that wires together the pieces built in Books II through VI. Every companion binary (firefly, cricket, future extensions) constructs one of these via a fluent builder and calls `.run()`.

Per the Discovery Mandate, Ch0 re-evaluated the plan against the live code:

### What the re-evaluation found

1. **The pattern spec and book scope are already clear.** The builder API shape (`new` → `with_transport` → `with_adapter_factory` → `run`) is committed in [companion-architecture.md §Companion top-level](../specs/companion-architecture.md#companion-top-level). Nothing to redesign.

2. **Namespace auto-registration** is a small but important Companion-layer concern. Each `Transport::emitted_kinds()` carries the canonical kinds a transport publishes. `Companion::run` walks every transport, extracts the namespace prefix of each kind, and calls `pulse.register_namespace(ns)` for the unique set. This closes the forgetting-to-register-a-namespace bug at the framework level — adapters that consume SSE events through `SseTransport` get `core` registered automatically.

3. **Flush timer for coalesced events**. `Pulse::flush_coalesced` is a manual method; Book VII wires a `tokio::time::interval` that calls it on a configurable cadence (default 50ms per the pattern spec). Without this, `LoadUpdated` events buffered for coalescing would never reach subscribers.

4. **Shutdown coordination**: multiple signals trigger shutdown:
   - OS signal (`ctrl_c` / SIGTERM) — standard for daemons
   - HTTP `POST /shutdown` on `CommandTransport` — already in Book III
   - Programmatic cancellation via the companion's shared `CancellationToken` — useful for embedded test harnesses

   All three cancel the same token. Every spawned task observes cancellation and exits cleanly.

5. **Enabled flag absorbed**. Book III's scaffolding entry `companion-old-companion-runtime` flagged `CompanionRuntime` + `CompanionState` for deletion in Book VIII Ch6. Book VII's `Companion` absorbs the enabled flag directly: `Arc<AtomicBool>` + optional `{state_dir}/enabled` persistence. When the enabled flag is false, adapters still run but `CompanionState`'s existing "don't dispatch SSE events when disabled" behaviour is replicated at the `Companion` boundary by dropping all adapters' event subscriptions. Simpler implementation: a single `enabled` flag surfaced through accessor methods; adapters observe it if they care. Firefly/cricket migrations (Book VIII) decide whether any adapter wants to honour disable — the default is "always dispatch, let adapter choose behaviour".

   Net: Book VII exposes `companion.enabled() -> bool` and `companion.set_enabled(bool)` with persistence. No auto-drop of events; simpler than legacy `CompanionState` and adequate for Book VIII consumers.

6. **No new workspace deps**. Everything Book VII needs (`tokio`, `tokio-util`, `tracing`, file persistence via `std::fs`) is already a direct dependency of `companion-sdk`.

No plan change vs COMPANION-0001.

## Decision

Introduce `Companion` at `src/companion-sdk/src/companion.rs` — a top-level struct that owns Pulse + Garden + Adapters + transports + enabled flag, with a fluent builder and an async `run()` method.

### Type shape

```rust
pub struct Companion {
    name: String,
    pulse: Arc<Pulse>,
    garden: Arc<Garden>,
    adapters: Arc<Adapters>,
    transports: Vec<Box<dyn Transport>>,
    enabled: Arc<AtomicBool>,
    state_dir: Option<PathBuf>,
    shutdown: CancellationToken,
    flush_interval: Duration,  // default 50ms
}

impl Companion {
    /// Construct with a name (for logging / HTTP health) and standard config.
    pub fn new(name: impl Into<String>) -> Self;

    /// Configure persistent state directory (for the enabled flag and,
    /// later, per-adapter state).
    pub fn with_state_dir(mut self, dir: impl Into<PathBuf>) -> Self;

    /// Override the default coalesced-event flush interval.
    pub fn with_flush_interval(mut self, d: Duration) -> Self;

    /// Attach a transport.
    pub fn with_transport<T: Transport>(mut self, transport: T) -> Self;

    /// Register an adapter factory.
    pub fn with_adapter_factory<F: AdapterFactory>(mut self, factory: F) -> Self;

    // --- Accessors for pre-run configuration and post-construction use ---
    pub fn name(&self) -> &str;
    pub fn pulse(&self) -> Arc<Pulse>;
    pub fn garden(&self) -> Arc<Garden>;
    pub fn adapters(&self) -> Arc<Adapters>;
    pub fn shutdown_token(&self) -> CancellationToken;
    pub fn enabled(&self) -> bool;
    pub fn set_enabled(&self, enabled: bool);

    /// Run until shutdown. Returns once all background tasks exit cleanly.
    pub async fn run(self) -> anyhow::Result<()>;
}
```

### `run()` internals

```
1. Load enabled flag from {state_dir}/enabled if present.

2. Auto-register namespaces:
   pulse.register_namespace("core");
   for transport in &transports:
       for kind in transport.emitted_kinds():
           if let Some(ns) = kind_namespace(kind):
               pulse.register_namespace(ns);

3. Spawn flush timer:
   tokio::spawn(flush_loop(pulse.clone(), flush_interval, shutdown.child_token()));

4. Spawn Garden projection:
   let _projection = garden.spawn_projection(shutdown.child_token());

5. Spawn Adapters supervisor:
   let adapters = adapters.clone();
   let supervisor_handle = tokio::spawn(async move {
       adapters.run(shutdown.child_token()).await;
   });

6. Spawn each transport:
   for transport in transports:
       tokio::spawn(transport.run(pulse.clone(), shutdown.child_token()));

7. Wait for shutdown:
   tokio::select! {
       _ = tokio::signal::ctrl_c() => {},
       _ = shutdown.cancelled() => {},
   }
   shutdown.cancel();

8. Await all task handles with a bounded timeout.
```

### Namespace extraction

`kind_namespace` already exists from Book I (`garden_common::domain` isn't involved — this is the SDK's `garden::kind_namespace`). It splits on the first `.` and returns the first part. Returns `None` for malformed kinds, which the registration code tolerates.

Book VII adds a `&'static str` conversion: `pulse.register_namespace` takes `&'static str`, but `kind_namespace` returns `Option<&str>` from a `&'static str` input. Since the `kind` slice has `'static` lifetime, its prefix is also `'static` — though the compiler can't always prove this, we use the same `&'static str` constants from `core_payloads.rs` / transport-specific modules. Validation: the `kind` is always a compile-time constant so the prefix is too.

### Persistence of `enabled` flag

```
{state_dir}/enabled
```

File contents: `"on"` or `"off"` (ASCII, trailing newline tolerated). Matches the legacy `CompanionState` format so an existing `companion-sdk` consumer's state survives migration.

Write-on-change, read-once-at-startup. Errors logged at `warn`; never cause `run()` to fail. (A companion with unreadable state dir still starts; it just doesn't persist changes.)

## Implementation plan

**Chapter 1** (this ADR) — land this document.

**Chapter 2** — implement `Companion` + tests:
- `src/companion-sdk/src/companion.rs` with the type, builder, `run`, and enabled-flag I/O
- Re-export from `lib.rs` prelude
- Tests:
  - `new_creates_companion_with_defaults`
  - `builder_attaches_transports_and_factories`
  - `enabled_defaults_to_true` / `set_enabled_persists_and_returns_new_value`
  - `enabled_loaded_from_state_dir` / `enabled_write_tolerates_missing_dir`
  - `run_spawns_garden_projection_and_supervisor` (integration)
  - `run_auto_registers_namespaces_from_transports` (integration)
  - `run_flushes_coalesced_events_on_timer` (integration)
  - `run_shuts_down_on_cancellation` (integration)
  - End-to-end: `Companion::new` + `CommandTransport` + `MockAdapter` → POST /command succeeds, adapter receives invocation

**Chapter 3** — update COMPANION-0001 revision history, close book.

Each chapter ships green to `dev`.

## Exit criteria

1. `use garden_companion_sdk::Companion;` compiles.
2. A 20-line `main.rs` can construct a companion with `CompanionTransport` + `SseTransport` + one adapter factory and call `.run()`.
3. Namespaces used by attached transports are auto-registered on Pulse.
4. `pulse.flush_coalesced` is called periodically at the configured interval.
5. Cancellation from any source (Ctrl+C, `/shutdown`, programmatic) exits `run` cleanly within a bounded window.
6. `enabled` flag persists to `{state_dir}/enabled` and reloads on next construction.
7. `cargo check --all` green.
8. `cargo test --package garden-companion-sdk companion::` green.
9. `cargo clippy --package garden-companion-sdk -- -D warnings` green.
10. COMPANION-0001 revision history amended.

## Out of scope (deferred)

| Item | Deferred to |
|------|-------------|
| `/status` endpoint on `CommandTransport` exposing `Adapters::status()` | Follow-up ADR or Book VIII if adapters need it in production |
| Typed per-adapter state persistence (beyond the global `enabled` flag) | COMPANION-0007 noted this — future ADR |
| Graceful HTTP drain for in-flight requests on shutdown | `axum::serve().with_graceful_shutdown` already installed by `CommandTransport`; no extra Book VII work |
| Multi-file config (TOML, JSON) driving which transports/factories to attach | Out of epic scope |

## References

- [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) — the epic
- [COMPANION-0003](COMPANION-0003-pulse.md) — Pulse (Book II)
- [COMPANION-0004](COMPANION-0004-transport.md) — Transport (Book III)
- [COMPANION-0006](COMPANION-0006-garden-aggregate.md) — Garden (Book V)
- [COMPANION-0007](COMPANION-0007-adapters.md) — Adapters (Book VI)
- [companion-architecture.md §Companion top-level](../specs/companion-architecture.md#companion-top-level)
