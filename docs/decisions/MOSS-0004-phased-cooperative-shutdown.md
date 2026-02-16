---
audience: [contributor, maintainer, ai]
doc_type: adr
status: accepted
last_verified: 2026-02-15
canonical: true
---

# MOSS-0004: Phased Cooperative Shutdown

**Status**: Accepted
**Date**: 2026-02-15
**Deciders**: Leo, Copilot
**Tags**: [shutdown, reliability, systemd, cancellation]

---

## Context

On 2026-02-15, deploying an update to 10 Linux stones bricked 7 of them.
The root cause was a cascade of problems in the shutdown path:

1. **`notify_one()` race**: `tokio::sync::Notify::notify_one()` only wakes one
   of two `.notified()` waiters (HTTP + HTTPS servers). The HTTPS waiter won
   the race; the HTTP server never got the signal and the process hung.

2. **SSE drain stall**: Even after fixing to `notify_waiters()`, axum's
   `with_graceful_shutdown()` waits for ALL active connections to drain.
   SSE presence streams (held by Firefly, Cricket) are infinite — they block
   drain indefinitely.

3. **No exit guarantee**: The Linux path had no `process::exit()`. It relied on
   the tokio runtime dropping 60+ fire-and-forget tasks, which never completed
   because OS threads (udev) and SSE connections held the process alive.

4. **No cooperative cancellation**: None of the 60+ `tokio::spawn` tasks have
   any shutdown awareness. There is no `CancellationToken`, no `JoinSet`, no
   way to signal or wait for background work to finish.

5. **systemd blindness**: The unit file uses `Type=simple` with no `WatchdogSec`
   or `TimeoutStopSec`. systemd has zero visibility into daemon health and waits
   90 seconds (default) before SIGKILL on a stalled shutdown.

The immediate fix (`notify_waiters` + drain deadline + hard watchdog) was a
spot-fix. This ADR formalizes the holistic architecture.

---

## Decision

Replace the "exit by fiat" shutdown model with a **four-phase cooperative
shutdown** using `CancellationToken`, `async_stream` cancellation, and
`sd_notify`.

### Phase 1 — Signal (0s)
Cancel the root `CancellationToken`. Stop accepting new connections.

### Phase 2 — Cooperate (0–3s)
All background tasks and SSE streams see the token cancellation and exit their
loops. Companion shutdown is triggered for deploy-initiated shutdowns.

### Phase 3 — Drain (3–8s)
axum drains remaining in-flight HTTP requests (8-second deadline).
Background tasks exit via their `CancellationToken` branches.

### Phase 4 — Exit (8s)
Flush final state (topology, roster). Send `sd_notify(STOPPING)`.
Call `process::exit(0)`. Hard-deadline watchdog at 15s calls
`process::exit(1)` as last resort.

### Components

1. **`tokio_util::sync::CancellationToken`** added to `AppState`. Child tokens
   passed to all spawned tasks and SSE streams.

2. **SSE streams** are wrapped in `async_stream::stream!` with
   `tokio::select!` checking `token.cancelled()`. This pattern was chosen
   over `.take_until()` because `tokio_stream::BroadcastStream` and
   `futures_util::StreamExt` resolve to different `futures_core` versions,
   making `.take_until()` fail to compile.

3. **Background tasks** wrap their main loop in `tokio::select!` with
   `token.cancelled()` to exit cooperatively.

4. **`JoinSet`** (deferred): Originally planned to replace fire-and-forget
   `tokio::spawn` with tracked tasks. Deferred because `CancellationToken`
   cooperative exit combined with `process::exit(0)` provides sufficient
   shutdown guarantees. The added complexity of threading a `JoinSet`
   through 60+ spawn sites offers no material benefit given the hard exit.

5. **systemd integration**: Unit file changed to `Type=notify` with
   `WatchdogSec=60`, `TimeoutStopSec=20`. Daemon sends `READY=1` after
   server binds, periodic `WATCHDOG=1` pings, and `STOPPING=1` on shutdown.

---

## Rationale

- **CancellationToken over broadcast channel**: Composable, clone-cheap,
  hierarchical (child tokens). The standard pattern in the tokio ecosystem.
  Does not require changing function signatures to pass a `Receiver`.

- **`async_stream` + `select!` on SSE streams**: Eliminates the root cause
  (infinite SSE blocking drain) at the source rather than working around it
  with timeouts. The `async_stream::stream!` macro is used instead of
  `.take_until()` due to a `futures_core` trait version mismatch between
  `tokio_stream` and `futures_util`.

- **JoinSet deferred**: While `JoinSet` would give visibility into running
  tasks and catch panics, the combination of `CancellationToken` (cooperative
  exit) and `process::exit(0)` (hard exit) already guarantees shutdown
  within 15 seconds. Adding `JoinSet` across 60+ spawn sites adds
  significant plumbing for marginal benefit. Can be revisited if shutdown
  debugging becomes a recurring need.

- **sd_notify**: systemd becomes the true supervisor. `WatchdogSec=60` means
  a deadlocked process gets SIGKILLed automatically. `TimeoutStopSec=20`
  bounds the shutdown window. This is defense in depth — the in-process
  watchdog catches bugs before systemd does.

- **`process::exit(0)` is retained**: Even with cooperative cancellation,
  a hard exit is the correct safety net for OS threads, leaked handles,
  and bugs that haven't been written yet.

---

## Consequences

### Positive
- Process guaranteed to exit within 15 seconds of shutdown signal
- SSE streams and background tasks exit cooperatively (no data dropped mid-write)
- systemd has full lifecycle visibility (ready, healthy, stopping)
- Future stateful workloads (WAL, queue flush) can participate in Phase 2
- JoinSet can be added incrementally if task lifecycle debugging is needed

### Negative
- All interval-loop and long-running tasks must accept `CancellationToken`
- `tokio_util` added as a dependency
- `sd-notify` crate added as a Linux dependency
- Slight code complexity increase: `tokio::select!` in every task loop

### Neutral
- Companions still survive non-deploy shutdowns (by design, `kill_on_drop(false)`)
- Windows uses identical CancellationToken pattern but not sd_notify
- The drain deadline (8s) and hard watchdog (15s) remain as safety nets

---

## Alternatives Considered

### Alternative 1: Broadcast channel for shutdown
- **Description**: Replace `Notify` with `broadcast::Sender<()>`
- **Pros**: No new dependency
- **Cons**: Not composable, requires recv in every task, no hierarchy.
  `CancellationToken` is strictly superior.
- **Rejected because**: CancellationToken is the idiomatic tokio pattern

### Alternative 2: Keep fire-and-forget + just fix SSE
- **Description**: Only add `take_until` to SSE streams, keep all tasks as-is
- **Pros**: Minimal change
- **Cons**: No visibility into task lifecycle, no cooperative shutdown,
  no systemd integration. Next class of bug hits the same wall.
- **Rejected because**: Spot-fix mentality that caused the bricking incident

### Alternative 3: `process::exit()` immediately on shutdown signal
- **Description**: Skip graceful shutdown entirely, just exit
- **Pros**: Simple, guaranteed fast
- **Cons**: In-flight writes corrupted, goodbye announcement lost,
  topology not flushed, no drain for HTTP responses
- **Rejected because**: Too aggressive, loses important state

---

## References

- [ARCH-0001](ARCH-0001-soc-ddd-architecture.md) — SoC/DDD architecture
- [PRESENCE-0001](PRESENCE-0001-stone-presence-protocol.md) — Presence streaming protocol
- [BUILD-0002](BUILD-0002-unified-deployment-packages.md) — Deploy/update flow
- `src/moss/src/bootstrap/server.rs` — HTTP server lifecycle
- `src/moss/src/bootstrap/tls.rs` — HTTPS server lifecycle
- `tokio_util::sync::CancellationToken` — [docs.rs](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html)
