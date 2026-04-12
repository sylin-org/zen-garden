---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017]
completed: 2026-04-12
---

# ARCH-0033: Logging Dissolution

**Date**: 2026-04-12
**Status**: Accepted
**Book**: XV of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Bounded context**: Logging (dissolved)

## Context

ARCH-0017 Book XV specifies: "The `AppState.log: broadcast::Sender<String>`
field and file sink become a proper `Logging` context. Deliverables:
`domain/logging/` module, `Logging` aggregate owning the log broadcast
channel and sink handle, `LogSink` port (file + stderr + memory adapters),
SSE log-streaming handler migrates to the context, tracing integration: a
custom tracing layer emits into the aggregate, `LogLineEmitted` event."

Chapter 1's discovery mandate requires re-evaluating this against the
actual code.

### Discovery findings (7 findings)

1. **`AppState::log` is a single `broadcast::Sender<String>` field with
   exactly one consumer.** The only call site is `state.log_stream()` in
   `api/v1/logs.rs:92`, which subscribes to the broadcast channel for SSE
   streaming. The `log_stream()` method is a one-liner:
   `self.log.subscribe()`.

2. **There is no mutable state to protect.** The broadcast channel is
   created once in bootstrap (`main.rs:101`), passed to the tracing
   subscriber layer, and stored in `AppState`. After construction, no
   mutation occurs — the channel is append-only by design (tracing events
   flow in, SSE subscribers read out).

3. **The `LogBroadcastLayer` is pure infrastructure.** Located in
   `infra/log_broadcast.rs`, it implements `tracing_subscriber::Layer` —
   a tracing integration concern, not domain logic. It formats tracing
   events into strings and sends them to the broadcast channel. This is
   the correct layer for this code.

4. **The file sink is entirely managed by `tracing-appender`.** In
   `bootstrap/config.rs::init_tracing()`, a `tracing_appender::rolling`
   daily appender writes to `{data_dir}/logs/garden-moss.log.*`. The
   `WorkerGuard` returned by the non-blocking writer must be held for the
   process lifetime. This is standard tracing infrastructure with no
   domain wrapping needed.

5. **The log file reader is a stateless filesystem operation.**
   `api/v1/logs.rs::get_recent_logs()` reads the most recent log file
   directly from disk, filters by level, and returns the last N lines.
   No aggregate state involved — it is a pure query against the
   filesystem.

6. **No invariants exist.** There are no business rules about logging —
   no log rotation policies to enforce (tracing-appender handles this),
   no log level governance, no log format contracts beyond what tracing
   already provides.

7. **A `LogSink` port would invert the dependency direction.** Tracing
   layers are infrastructure that push into a channel. The planned
   `LogSink` port (file + stderr + memory adapters) would duplicate what
   `tracing-subscriber` already provides — three composable layers with
   per-layer filtering. The existing architecture is correct.

## Decision

**Dissolve Book XV.** Logging does not warrant a bounded context or
aggregate. The existing architecture is well-structured:

- **Tracing subscriber** composes three layers (stderr, file, broadcast)
  with per-layer filtering — this is the `tracing` crate's intended usage.
- **`LogBroadcastLayer`** is correctly in `infra/` — it bridges tracing
  events to a broadcast channel for SSE.
- **`AppState::log`** is a cross-cutting broadcast channel, equivalent to
  `shutdown_token` or `event_bus` — infrastructure plumbing, not domain.

### Actions taken

1. **No `domain/logging/` module created** — there is no domain state to
   own, no invariants to enforce, no events to emit beyond the raw
   tracing output.

2. **No `LogSink` port** — `tracing-subscriber` layers already serve as
   composable output adapters with filtering. A port would be a
   redundant abstraction layer.

3. **No `LogLineEmitted` event** — the broadcast channel IS the event
   stream. Wrapping `String` in `LogLineEmitted { line: String }` adds
   a type alias, not domain meaning.

4. **`LogBroadcastLayer` stays in `infra/log_broadcast.rs`** — it is
   infrastructure (tracing layer), correctly positioned.

5. **`AppState::log` stays as a cross-cutting field** — it is plumbing
   infrastructure like `shutdown_token`, not a domain concept.

6. **Context map updated** — Logging marked as dissolved with rationale.

## Consequences

- `AppState::log: broadcast::Sender<String>` remains as a cross-cutting
  field. This is the correct architecture for a channel that bridges
  infrastructure (tracing) to transport (SSE).
- `infra/log_broadcast.rs` remains unchanged — it is the right module in
  the right layer.
- `api/v1/logs.rs` remains unchanged — both endpoints (file read and SSE
  stream) are thin handlers over existing infrastructure.
- If structured log events are ever needed (typed log entries with
  severity, component, correlation IDs), a new ADR should evaluate the
  scope at that time rather than building speculative infrastructure now.
