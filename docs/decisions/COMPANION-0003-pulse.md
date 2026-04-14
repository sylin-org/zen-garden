---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-13
canonical: true
---

# COMPANION-0003: Pulse — Book II of COMPANION-0001

**Date**: 2026-04-13
**Status**: Accepted
**Book**: II of [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)
**Depends on**: [COMPANION-0002](COMPANION-0002-event-envelope.md) (event envelope), [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) (epic + pattern spec)

## Context

Book II lands the orchestrator — the single fan-in point every event passes through before any subscriber sees it. This is the enforcement layer for the deduplication, validation, and coalescing guarantees adapters depend on. Without it, every adapter would have to dedupe, validate, and throttle independently (the anti-pattern COMPANION-0001 exists to prevent).

Per the Discovery Mandate in COMPANION-0001, Ch0 re-evaluated the plan against the live code. Findings:

### What the re-evaluation found

1. **No LRU or concurrent-map crates in the workspace.** `Cargo.toml` has no `lru`, `dashmap`, or `parking_lot`. Rather than add a workspace dep for what Book II needs, use the stdlib: a `Mutex<VecDeque<EventId> + HashSet<EventId>>` pair for FIFO-evicting dedup, and `Mutex<HashMap<&'static str, Event>>` for the per-kind coalescing map. The dedup cache is small (default 4096 entries), the critical sections are tiny (single insert / contains / pop), and contention is bounded by ingest rate. Zero new deps.

2. **Moss already has a canonical warn-and-continue lag pattern.** Four moss SSE handlers (`presence.rs`, `pulse.rs`, `storage.rs`, `tools.rs`) handle `BroadcastStreamRecvError::Lagged(n)` identically: log warn, continue. Book II mirrors this from the orchestrator side — the orchestrator does not detect subscriber lag directly (that's a subscriber-side concern), but exposes a `dropped_on_fanout` counter in `PulseMetrics` that reflects occasions where `broadcast::Sender::send` returned `Err` because no receivers were subscribed. True lag detection belongs to the subscriber's code, which sees `RecvError::Lagged` and is expected to recover (fall back to snapshot, log, continue).

3. **Validation happens in layers.** Book I shipped syntactic kind validation (`is_valid_kind`). Book II's Pulse adds the semantic checks:
   - **Namespace registration**: the ingested event's kind must start with a namespace that has been registered with `Pulse::register_namespace(ns)`. Companion wires this at construction (e.g., `pulse.register_namespace("core")` + `pulse.register_namespace("firefly")`).
   - **Kind/payload coherence**: the event envelope's `kind` field must match `event.payload.kind()` (the value the payload's `EventPayload::KIND` const returns at runtime via `DynPayload::kind`). Catches programmer errors where someone constructs an `Event { kind: "wrong", payload: ... }` with mismatched values.

4. **Coalescing needs an explicit flush trigger.** Coalesced events sit in the per-kind map waiting for something to push them out. The orchestrator cannot own a timer task (that would couple Pulse to a runtime); instead, Book II exposes `Pulse::flush_coalesced()` as a public method. Book VII's `Companion::run()` will spawn a timer task that calls it on a configurable interval (default 50ms per the pattern spec). For unit testing and step-through scenarios, callers can flush manually.

5. **`tokio::sync::broadcast` is already a direct dep of `companion-sdk`** (via the existing `sse.rs`). No new deps for fan-out.

No plan changes vs COMPANION-0001. Book II's scope holds.

## Decision

Introduce `Pulse` at `src/companion-sdk/src/garden/pulse.rs` — a private-state aggregate with the canonical ingest / validate / dedup / coalesce / fan-out pipeline. All state is behind `std::sync::Mutex` or `std::sync::RwLock`; `PulseMetrics` uses atomic counters for lock-free updates on the hot path.

### Type shape

```rust
pub struct Pulse {
    // All fields private. State mutated only through methods.
    subscribers: broadcast::Sender<Event>,
    dedup: Mutex<DedupCache>,                      // FIFO-evicting
    coalesce: Mutex<HashMap<&'static str, Event>>, // latest-wins per kind
    namespaces: RwLock<HashSet<&'static str>>,
    metrics: Arc<PulseMetrics>,
}

pub struct PulseConfig {
    pub dedup_capacity: usize,      // default 4096
    pub broadcast_capacity: usize,  // default 1024
}

impl Default for PulseConfig { ... }

impl Pulse {
    pub fn new(config: PulseConfig) -> Self;
    pub fn with_defaults() -> Self;

    /// Register a namespace as acceptable. Events whose kind doesn't start
    /// with a registered namespace are rejected with UnregisteredNamespace.
    pub fn register_namespace(&self, ns: &'static str);

    /// The single fan-in point. Validates, dedupes, maybe coalesces, maybe
    /// fans out. Returns IngestResult to let the caller know what happened.
    pub fn ingest(&self, event: Event) -> IngestResult;

    /// Drain the per-kind coalesce buffer, emitting each kept event to
    /// subscribers. Called by Companion's timer (Book VII) or by tests.
    /// Returns number of events flushed.
    pub fn flush_coalesced(&self) -> usize;

    /// Subscribe to the canonical event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<Event>;

    /// Snapshot of current metrics.
    pub fn metrics(&self) -> PulseMetricsSnapshot;

    /// Number of currently-attached subscribers.
    pub fn receiver_count(&self) -> usize;
}

pub enum IngestResult {
    /// Event passed validation + dedup; fanned out to subscribers.
    Accepted { subscribers: usize },

    /// Event is coalescing and was buffered; latest value kept.
    /// Actual delivery happens on the next flush_coalesced().
    Coalescing,

    /// Event id was already seen; silently dropped.
    Duplicate,

    /// Event failed validation.
    Rejected(RejectReason),
}

pub enum RejectReason {
    /// event.kind has syntactic problems (fails is_valid_kind).
    InvalidKindFormat,
    /// event.kind's namespace prefix is not registered.
    UnregisteredNamespace,
    /// event.kind does not match event.payload.kind().
    KindPayloadMismatch,
}

pub struct PulseMetricsSnapshot {
    pub ingested: u64,
    pub accepted: u64,
    pub deduped: u64,
    pub coalescing: u64,
    pub coalesced_flushed: u64,
    pub rejected_invalid_kind: u64,
    pub rejected_unregistered_namespace: u64,
    pub rejected_kind_payload_mismatch: u64,
    pub dropped_on_fanout: u64,
}
```

### Internal dedup cache

```rust
struct DedupCache {
    ids: HashSet<EventId>,
    order: VecDeque<EventId>,
    capacity: usize,
}

impl DedupCache {
    fn new(capacity: usize) -> Self;
    /// Returns true if the id was already present (duplicate).
    fn insert(&mut self, id: EventId) -> bool;
}
```

FIFO eviction (not strict LRU). This is correct for dedup: we want "have we seen this id in the last N events?" — insert order is sufficient. A strict LRU would require moving items on `contains` hits, which is both unnecessary and slower. O(1) amortized insert; O(1) contains.

### Ingest pipeline

```
Pulse::ingest(event) ->
  1. metrics.ingested++
  2. validate:
     a. is_valid_kind(event.kind) — else Rejected(InvalidKindFormat)
     b. namespace registered        — else Rejected(UnregisteredNamespace)
     c. event.kind == event.payload.kind() — else Rejected(KindPayloadMismatch)
  3. dedup:
     if cache.insert(event.id) was "already present" — Duplicate (metrics.deduped++)
  4. maybe coalesce:
     if event.payload.is_coalescing():
       coalesce.insert(event.kind, event) — Coalescing (metrics.coalescing++)
  5. fan out:
     subscribers.send(event) -> Ok(n) => metrics.accepted++, return Accepted { subscribers: n }
                             Err(_)  => metrics.dropped_on_fanout++, return Accepted { subscribers: 0 }
```

### Flush

```
Pulse::flush_coalesced() ->
  drain coalesce map; for each kept event:
    subscribers.send(event)
    metrics.coalesced_flushed++
  return count
```

Flush bypasses dedup (the event was already deduped on ingest) and validation (already validated on ingest). It delivers whatever is in the buffer and clears it.

## Implementation plan

**Chapter 1 (this ADR)** — land this document.

**Chapter 2** — implement the pulse module + tests:
- `src/companion-sdk/src/garden/pulse.rs` with all types above
- Re-export from `garden/mod.rs` and `lib.rs` prelude
- Unit tests (in-file `#[cfg(test)] mod tests`):
  - `accepts_valid_event_and_fans_out`
  - `rejects_invalid_kind_format`
  - `rejects_unregistered_namespace`
  - `rejects_kind_payload_mismatch`
  - `dedupes_by_event_id`
  - `dedup_cache_evicts_fifo_beyond_capacity`
  - `coalesces_events_flagged_coalescing`
  - `coalesce_keeps_latest_per_kind`
  - `flush_coalesced_emits_pending_and_returns_count`
  - `flush_coalesced_clears_buffer`
  - `non_coalescing_events_bypass_buffer`
  - `ingest_with_no_subscribers_reports_zero_subscribers`
  - `metrics_snapshot_reflects_activity`
  - `register_namespace_accepts_after_rejection`
  - `subscribers_receive_in_order`

**Chapter 3** — update COMPANION-0001 revision history, amend pattern spec if needed, close book.

Each chapter ships green to `dev`.

## Exit criteria

1. `use garden_companion_sdk::garden::{Pulse, IngestResult};` compiles.
2. A Pulse constructed with `with_defaults()`, registered for one namespace, accepts a valid event and delivers it to a subscriber.
3. An event with an unregistered-namespace kind is rejected with `RejectReason::UnregisteredNamespace`.
4. An event whose id was already ingested returns `IngestResult::Duplicate` on the second call.
5. An event with `COALESCING=true` returns `IngestResult::Coalescing` and is delivered only after `flush_coalesced()`.
6. `cargo check --all` green.
7. `cargo test --package garden-companion-sdk garden::pulse` green.
8. `cargo clippy --package garden-companion-sdk -- -D warnings` green.
9. COMPANION-0001 revision history amended with Book II closure.

## Out of scope (deferred)

| Item | Book |
|------|------|
| Automatic flush timer | Book VII (Companion wires a `tokio::time::interval` calling `flush_coalesced`) |
| Subscriber-side lag handling | Book VI / Book VIII — each adapter handles `RecvError::Lagged` per the moss pattern |
| Metrics exported over HTTP | Deferred; Book VII's `/health` / `/status` endpoints may expose snapshots |
| Named subscribers / per-subscriber routing | Book VI's supervisor wraps `subscribe()` to filter per `AdapterProfile::subscriptions` |
| Persistence of the dedup cache across process restarts | Not needed — companion process restart is rare and dedup protects a small time window |
| Distributed dedup across multiple companion processes | Not a requirement — each companion has its own Pulse |

## References

- [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) — the epic
- [COMPANION-0002](COMPANION-0002-event-envelope.md) — Event envelope (Book I)
- [companion-architecture.md §Pulse — the orchestrator](../specs/companion-architecture.md#pulse--the-orchestrator)
- [companion-architecture.md §Cross-cutting concerns matrix](../specs/companion-architecture.md#cross-cutting-concerns-matrix)
