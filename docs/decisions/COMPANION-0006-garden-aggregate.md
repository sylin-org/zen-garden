---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-13
canonical: true
---

# COMPANION-0006: Garden Aggregate — Book V of COMPANION-0001

**Date**: 2026-04-13
**Status**: Accepted
**Book**: V of [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)
**Depends on**: [COMPANION-0003](COMPANION-0003-pulse.md), [COMPANION-0005](COMPANION-0005-domain-types.md), [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)

## Context

Book V lands the client-side read-model — the [`Garden`] aggregate that adapters query for current state and subscribe to for event streams. This is the second half of the Garden bounded context (transport + pulse being the first half) and the last book on the critical path before the Adapter contract (Book VI).

Per the Discovery Mandate in COMPANION-0001, Ch0 re-evaluated the plan against the live code. Findings:

### What the re-evaluation found

1. **Book IV's domain types lack `Default` impls** (Health, Pond, Load). Needed for `GardenState::default()`. Book V adds them as a small scope expansion: `Health::Dormant`, `Pond::Solo`, `Load::ZERO`.

2. **`tokio::sync::broadcast` has no per-new-subscriber hook.** The pattern spec imagined `Garden::events() -> Receiver<Event>` where the first event delivered is a synthetic `GardenSnapshot`. That's not achievable with plain broadcast semantics. **Book V refinement**: return a `GardenSubscription { snapshot: Event, receiver: broadcast::Receiver<Event> }` struct instead. Adapters render from the snapshot first, then enter the event loop on the receiver. Clearer contract, trivially implementable, no wrapping dance.

3. **OfferingState stays wire-typed in Book V.** The wire type `garden-common::presence::OfferingState` has `status: String` and `health: String`. Promoting these to typed enums is nice but not blocking — Book V keeps `OfferingState` in `GardenState.offerings`. A typed `Offering` DDD promotion can land in a later book without changing Garden's shape.

4. **Projection scope is "update state only".** The pattern spec mentions Garden "emits domain-level events back to Pulse" for state deltas. That's optional enhancement; Book V keeps the projection as a pure state-update function so the minimum viable aggregate ships. Later books can add delta-event emission if adapters want them (adapters can also just read state directly via property accessors on every tick).

5. **Projection task needs tokio runtime and a shutdown token.** Matches the Transport trait's shape — `spawn_projection(&self, shutdown: CancellationToken) -> JoinHandle<()>`. Book VII's Companion will wire it.

No plan change vs COMPANION-0001 beyond the `GardenSubscription` refinement, which is a minor API detail consistent with the design intent.

## Decision

Introduce `Garden` at `src/companion-sdk/src/garden/garden.rs` — a private-state aggregate that projects events from Pulse into `GardenState` and exposes typed accessors.

### Types

```rust
// GardenState — the read-model. Private mutable state behind RwLock.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GardenState {
    pub stone_name: Option<String>,
    pub health: Health,                  // default Dormant
    pub load: Load,                      // default ZERO
    pub offerings: Vec<OfferingState>,   // wire-typed for Book V
    pub seed_bank: Option<SeedBank>,
    pub pond: Pond,                      // default Solo
    pub ready: bool,                     // true after first snapshot received
}

// GardenSnapshot — the synthetic event Garden synthesizes at subscribe time.
#[derive(Debug, Clone)]
pub struct GardenSnapshot {
    pub state: GardenState,
}

impl EventPayload for GardenSnapshot {
    const KIND: &'static str = "core.garden.snapshot";
    fn as_any(&self) -> &dyn Any { self }
}

// GardenSubscription — what subscribe() returns.
pub struct GardenSubscription {
    pub snapshot: Event,                              // wraps GardenSnapshot
    pub receiver: broadcast::Receiver<Event>,
}

// Garden — the aggregate itself. Constructed as Arc<Garden>.
pub struct Garden {
    state: Arc<RwLock<GardenState>>,
    pulse: Arc<Pulse>,
}

impl Garden {
    pub fn new(pulse: Arc<Pulse>) -> Arc<Self>;

    /// Spawn the projection task. Call once after construction.
    pub fn spawn_projection(
        self: &Arc<Self>,
        shutdown: CancellationToken,
    ) -> tokio::task::JoinHandle<()>;

    // --- Typed property accessors (synchronous; lock acquired briefly) ---
    pub fn stone_name(&self) -> Option<String>;
    pub fn health(&self) -> Health;
    pub fn load(&self) -> Load;
    pub fn offerings(&self) -> Vec<OfferingState>;
    pub fn seed_bank(&self) -> Option<SeedBank>;
    pub fn pond(&self) -> Pond;
    pub fn is_ready(&self) -> bool;

    /// Full state clone — useful for Book VI adapter hydration.
    pub fn snapshot(&self) -> GardenState;

    /// Subscribe. Returns a snapshot event + the live receiver.
    pub fn subscribe(&self) -> GardenSubscription;
}
```

### The projection

Each event kind maps to a state update:

| Event kind | Effect on GardenState |
|---|---|
| `core.presence.snapshot` | Replace all fields; set `ready = true` |
| `core.stone.health.changed` | Update `health` from typed payload |
| `core.stone.load.updated` | Update `load` from typed payload |
| `core.service.started` | Append to `offerings` if name absent |
| `core.service.stopped` | Remove from `offerings` by name |
| `core.storage.connected` | Set `seed_bank` |
| `core.storage.removed` | Clear `seed_bank` |
| `core.stone.tended` | No state change (transient event) |
| `core.storage.detected` | No state change (unmanaged — operator action needed) |

All implemented in a single `project(state, event)` function; dispatched via the typed downcast helpers added in Book IV (`StoneHealthChangedExt::health_domain`, `StoneLoadUpdatedExt::load_domain`, etc.).

### Default impls added to Book IV types

Minor Book IV extension needed:

```rust
impl Default for Health { fn default() -> Self { Health::Dormant } }
impl Default for Pond   { fn default() -> Self { Pond::Solo } }
impl Default for Load   { fn default() -> Self { Load::ZERO } }
```

These are trivial and don't change the public API.

## Implementation plan

**Chapter 1 (this ADR)** — land this document.

**Chapter 2** — implement Garden + projection + tests:
- Add `Default` impls to Book IV's `Health`, `Pond`, `Load`
- `src/companion-sdk/src/garden/garden.rs`: `Garden`, `GardenState`, `GardenSnapshot`, `GardenSubscription`, `project` function, `spawn_projection` task
- Re-exports from `garden/mod.rs` and prelude
- Unit tests:
  - `default_garden_state_is_dormant_and_not_ready`
  - `apply_snapshot_replaces_state_and_marks_ready`
  - `apply_health_changed_updates_health_field`
  - `apply_load_updated_updates_load_field`
  - `apply_service_started_adds_to_offerings_once`
  - `apply_service_stopped_removes_from_offerings`
  - `apply_storage_connected_and_removed_toggles_seed_bank`
  - `apply_stone_tended_does_not_mutate_state`
  - `subscribe_returns_snapshot_plus_live_receiver`
  - `projection_task_consumes_events_from_pulse`
  - `projection_task_exits_on_shutdown`
  - Property accessors return expected values after projection

**Chapter 3** — update COMPANION-0001 revision history, close book.

Each chapter ships green to `dev`.

## Exit criteria

1. `use garden_companion_sdk::garden::{Garden, GardenState, GardenSnapshot, GardenSubscription};` compiles.
2. A fresh `Garden::new(pulse)` returns state with `health == Dormant`, `pond == Solo`, `ready == false`, no offerings, no seed_bank.
3. After applying a `core.presence.snapshot` event, `garden.is_ready()` returns `true` and typed accessors reflect the snapshot.
4. After applying a `core.stone.load.updated` event, `garden.load()` returns the new values.
5. `garden.subscribe()` returns a `GardenSubscription` whose `snapshot.payload::<GardenSnapshot>()` equals the current state.
6. `spawn_projection(shutdown)` returns a handle that exits cleanly on token cancellation.
7. `cargo check --all` green.
8. `cargo test --package garden-companion-sdk garden::garden` green.
9. `cargo clippy --package garden-companion-sdk -- -D warnings` green.
10. COMPANION-0001 revision history amended.

## Out of scope (deferred)

| Item | Deferred to |
|------|-------------|
| Typed `Offering` enum replacing wire-typed `OfferingState` | Future enhancement ADR |
| State-delta event emission back to Pulse (e.g. `HealthChanged` derived events) | Book VIII if adapters need it |
| Garbage-collecting stale offerings over time | Not needed — presence snapshots re-seed the list |
| Multi-stone Garden (cross-stone federation) | Future post-epic work |

## References

- [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) — the epic
- [COMPANION-0003](COMPANION-0003-pulse.md) — Pulse (Book II)
- [COMPANION-0005](COMPANION-0005-domain-types.md) — Domain types (Book IV)
- [companion-architecture.md §Garden context](../specs/companion-architecture.md#garden-context)
