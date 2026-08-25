---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-14
canonical: true
---

# COMPANION-0014: Companions Query Moss Directly — Retire the Client-Side Garden Aggregate

**Date**: 2026-04-14
**Status**: Accepted — **implementation pending**
**Supersedes (in part)**: [COMPANION-0006](COMPANION-0006-garden-aggregate.md) — the Garden read-model
**Depends on**: [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) (epic), [COMPANION-0012](COMPANION-0012-device-bus.md) (bus model)

## Context

Companions run on the same stone as moss. Their HTTP loopback to `127.0.0.1:7185` is the canonical, authoritative source of stone state. Moss already exposes that state via its REST API.

Book V of COMPANION-0001 introduced [`Garden`](COMPANION-0006-garden-aggregate.md) — a client-side read-model that *projects* moss state from the SSE event stream and exposes it to adapters. This made sense as a CQRS pattern but introduces three concrete problems we have run into in practice:

1. **Subscription race**. SSE doesn't replay events. An adapter spawned after moss has already emitted the initial `core.presence.snapshot` never sees it; Garden stays empty until the next live event arrives. Symptoms: OLED v2 stuck on the firmware boot placeholder ("Zen Garden") because the snapshot was missed and only `stone.health.changed` arrived to trigger the dashboard render.

2. **State derived from event timing**. Fields like `uptime_seconds` are wire-format snapshots of a value that is intrinsically a function of "now minus boot time." Treating them as event-delivered scalars couples client state to event arrival order. We patched `is_ready`, rehydration on adapter spawn, and AdapterExited re-attach to keep the client model "fresh enough" — all symptoms of the wrong abstraction.

3. **Duplication**. Garden's projection logic mirrors what moss already does. When moss adds a field, we have to extend Garden in lock-step. Two implementations of the same concept.

The architectural correction: stop building a client-side read-model. Moss IS the read-model. Companions should query it directly when they need state, and treat SSE strictly as an invalidation/notification channel for live deltas.

## Decision

Companions query moss over HTTP for state. SSE remains for live deltas only.

### The new shape

```
moss (port 7185)
  ├── HTTP API ─────────► MossLocalClient (per-companion, on demand)
  │                              │
  │                              └──► hydrate adapter at spawn
  │                                   re-query on demand mid-run
  │
  └── SSE /presence/stream ──► SseTransport → Pulse → mpsc filter
                                                          │
                                                          ▼
                                                     adapter: react to deltas
                                                     (re-render or re-query)
```

One read path (HTTP). One delta path (SSE). Both originate from the same authoritative source. No client-side state aggregate; no projection; no race; no `is_ready`.

### `MossLocalClient`

A thin wrapper around `http://127.0.0.1:7185` exposing typed methods for the surface companions consume. It reuses `garden-common::client::StoneApi` where possible since that's already a typed StoneApi facade. Methods needed for the firefly + cricket use cases:

- `presence_snapshot() -> PresenceSnapshot` — full stone state, the shape adapters render from.
- `services() -> Vec<Service>` — for any adapter that wants service-by-service detail.
- `capabilities() -> StoneCapabilities` — for adapters that gate behaviour on hardware.

Most adapters will only use `presence_snapshot()`. Construction takes the moss URL (typically derived from `Companion::stone_url()`).

### Adapter trait change

```rust
fn run(
    self: Box<Self>,
    events: mpsc::Receiver<Event>,
    moss: Arc<MossLocalClient>,        // replaces  garden: Arc<Garden>
    pulse: Arc<Pulse>,
    shutdown: CancellationToken,
) -> BoxFuture<'static, ()>;
```

`garden: Arc<Garden>` is removed from the run signature. The supervisor constructs / clones a `MossLocalClient` from the Companion's configured stone URL and passes it to each adapter.

### Adapter pattern

State-rendering adapters (the four firefly variants):

```rust
async fn run(...) {
    // 1. Hydrate. Deterministic. Either succeeds or retries.
    let initial = match moss.presence_snapshot().await {
        Ok(s) => s,
        Err(e) => { /* retry with backoff or exit */ }
    };
    render(&initial);

    // 2. React to live deltas.
    loop {
        select! {
            _ = shutdown.cancelled() => break,
            Some(event) = events.recv() => {
                // Either apply delta payload directly...
                apply(&event);
                // ...or re-fetch when delta isn't self-contained.
            }
        }
    }
}
```

Delta-driven adapters (cricket — audio plays on event arrival, no state to render):

```rust
async fn run(...) {
    // No hydration step. Cricket has no UI; just react to deltas.
    loop {
        select! {
            _ = shutdown.cancelled() => break,
            Some(event) = events.recv() => play_audio_for(&event),
        }
    }
}
```

Cricket gets a `MossLocalClient` in its signature but doesn't have to use it. No-op cost.

### What gets deleted from `companion-sdk`

| Artifact | Reason |
|---|---|
| `garden::Garden` | Client-side read-model — moss is the read-model. |
| `garden::GardenState` | Same. |
| `garden::GardenSnapshot` (synthetic event) | Same. |
| `garden::GardenSubscription` | Subscribe-with-snapshot dance — replaced by HTTP request/response. |
| `Garden::projection_task` and the apply_* functions | Projection lives in moss. |
| `is_ready` flag | Determinism replaces "wait for first event." |
| Garden tests + fixtures | ~600 LOC. |
| Adapter rehydration block (added 2026-04-14, b57cddcd) | Replaced by HTTP call at adapter startup. |

Surviving Garden constants (e.g. `kind_namespace`, `is_valid_kind`) move to `garden::event` directly — they were never aggregate-scoped in spirit.

### What stays unchanged

- `Pulse`: still the local fan-out for SSE-derived events.
- `SseTransport`, `CommandTransport`: unchanged.
- `DeviceBus`, identity protocols, adapter registrations, exit-event channel: all unchanged.
- The set of event kinds and payloads: unchanged. They flow from moss → SseTransport → Pulse → adapters as deltas.

### What's added

- `companion-sdk::moss_client::MossLocalClient` (~100 LOC). Thin facade over `garden-common::client::StoneApi`.

### `uptime_seconds` and friends

The "should we project uptime in GardenState" question becomes moot. Moss already returns it as part of the presence snapshot. Adapters that want to display uptime call `moss.presence_snapshot()` and read `stone.uptime_seconds` from the response. No client-side derivation, no time-based reasoning.

If the operator complains about wall-clock skew or stale uptime values, that's a moss-side concern (its own clock), not a companion-sdk concern.

## Implementation plan

Single chapter — the change is internally cohesive and breaks adapter signatures atomically. No staged rollout because all adapter implementations live in this repo.

1. **ADR** (this document).
2. **`MossLocalClient`** module + tests.
3. **Adapter trait change**: `garden: Arc<Garden>` → `moss: Arc<MossLocalClient>` in `Adapter::run` signature.
4. **Refactor four firefly adapters**: drop `garden` references, hydrate via `moss.presence_snapshot()` at startup, react to deltas as before.
5. **Update cricket**: signature change only (cricket doesn't use the param).
6. **Update Companion runtime**: stop constructing Garden, stop spawning projection task. Wire `MossLocalClient` into the supervisor.
7. **Delete Garden module + tests + fixtures**.
8. **Update SDK exports / prelude**: drop Garden re-exports; add MossLocalClient.
9. **Update bus runtime + integration tests**: anywhere referencing Garden becomes either MossLocalClient or just disappears.
10. **`cargo check --all` + tests + clippy**.
11. **`./installer/build-all-platforms.ps1`** to build + push to all stones.

Net code delta estimate: **~800 LOC removed, ~200 LOC added**.

## Exit criteria

1. `Garden` and friends do not exist in the source tree.
2. `MossLocalClient` exists and is the canonical way for adapters to read stone state.
3. All four firefly adapters hydrate via HTTP at startup; OLED v2 shows the real stone name on first frame, deterministically.
4. Cricket continues to function as a delta-driven adapter (no regression).
5. SDK lib + integration tests green.
6. Build + deploy across all stones succeeds.

## Out of scope (deferred)

| Item | Deferred |
|---|---|
| `MossLocalClient` for non-localhost moss (cross-stone querying) | Companions are stone-local by design; revisit if/when a cross-stone use case appears |
| Caching layer in front of MossLocalClient | Premature; HTTP loopback is microseconds. Adapters that re-query on every delta are fine |
| Removing `core.presence.snapshot` from the SSE stream | Moss-side change; orthogonal. Companions ignore it from the read perspective; they may still react to it as an invalidation signal |

## References

- [COMPANION-0006](COMPANION-0006-garden-aggregate.md) — original Garden ADR, partially superseded
- [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) — epic context
- [COMPANION-0012](COMPANION-0012-device-bus.md) — device bus (unaffected)
- `garden-common::client::StoneApi` — the typed HTTP client `MossLocalClient` builds on
