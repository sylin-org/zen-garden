---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017]
completed: 2026-04-12
---

# ARCH-0034: EventBus / Pulse Dissolution

**Date**: 2026-04-12
**Status**: Accepted
**Book**: XVI of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Bounded context**: Events (dissolved)

## Context

ARCH-0017 Book XVI specifies: "Unify `EventBus`, `PulseEvent`, per-domain
`changes()` streams, and `PulseDomainBridge` into a coherent cross-cutting
surface. Deliverables: `domain/events/` module, `PulseProjectionTask`
subscribes to every domain's `changes()` and translates to `PulseEvent`,
retire ad-hoc `state.event_bus.emit(...)` calls in favor of domain
mutations that emit naturally."

Chapter 1's discovery mandate requires re-evaluating this against the
actual code.

### Discovery findings (8 findings)

1. **EventBus and per-aggregate `changes()` serve different event
   populations.** The 10 per-aggregate `changes()` channels (Metrics,
   Tool, Topology, Jobs, Catalog, Subsystems, Health, Security, Discovery,
   Offerings) carry internal state-transition notifications
   (`OfferingsChanged::Added`, `JobsChanged::Submitted`,
   `TopologyChanged::PeerUpdated`). These are consumed by infrastructure
   subscribers (beacon transport, projections, reconciliation tasks).
   EventBus carries user-facing domain events (`OfferingEvent::deployed`,
   `StoneEvent::health_changed`, `StorageEvent::storage_connected`) with
   different type shapes, different consumers, and different semantics.
   They are not the same event population.

2. **The proposed `PulseProjectionTask` would duplicate, not replace,
   EventBus.** Subscribing to every aggregate's `changes()` and
   translating to `PulseEvent` would require a per-aggregate translation
   layer that maps internal change kinds to user-facing event types. This
   is exactly what the current `event_bus.emit(OfferingEvent::deployed(...))`
   call sites already do — they emit at the point where the user-visible
   action occurs, with the right user-facing payload. Moving this to a
   projection task would scatter the translation logic and lose the
   call-site context that determines what to include in the event payload.

3. **EventBus has exactly three well-scoped listeners.**
   - `ChirpListener`: Filters for offering topology changes, triggers
     UDP chirp announcements with debouncing. Only cares about
     `OfferingEvent` variants where `should_chirp()` is true.
   - `PulseDomainBridge`: Translates `DomainEvent` to `DomainPulse` and
     sends to the pulse broadcast channel for SSE consumers.
   - `TimerListener`: Reacts to offering deploy/remove/rename for
     nurturing schedule timers with debouncing.
   Each has a single responsibility. The listener pattern is clean.

4. **Pulse is a correctly-designed SSE firehose.** The pulse channel
   merges two distinct event sources:
   - `PulseEvent::Domain(DomainPulse)` — domain events translated by
     `PulseDomainBridge` (via EventBus)
   - `PulseEvent::Transport(TransportPulse)` — raw UDP announcements
     surfaced by `spawn_transport_tap`
   SSE consumers (`/api/v1/stone/pulse/stream`) get a unified stream.
   The presence stream (`/api/v1/stone/presence/stream`) filters to
   domain-only events in the Companion vocabulary. This separation is
   architecturally correct.

5. **Direct `pulse.send()` calls are intentional, not a smell.** Five
   sites in `storage.rs` and `volume_monitor.rs` emit ad-hoc storage
   progress events directly to pulse, bypassing EventBus. These are
   long-running operation progress updates (e.g., "formatting device",
   "mounting filesystem") that do not need chirp broadcasting or timer
   management. Routing them through EventBus would add noise to the
   chirp and timer listeners. The `DomainPulse::storage_event()` builder
   exists specifically for this use case.

6. **`DomainEvent` is a stable wire contract.** The `DomainEvent` enum
   (`domain/events.rs`) is serialized for SSE payloads consumed by
   Rake, Firefly, Cricket, and the pulse HTML dashboard. Its variants
   (`Offering`, `Storage`, `Stone`, `Job`, `Pond`) are a public API.
   Wrapping this in an "Events aggregate" would add a layer of
   indirection without changing the wire format.

7. **The Security dual-stream note (context-map) is already correct.**
   Security emits `SecurityChanged` via `changes()` for internal
   consumers AND `PondEvent::EnrollmentChanged` via EventBus for the
   pond enrollment listener. The context-map says "preserved until
   Book XVI." Discovery reveals this dual-stream pattern is the right
   design — the two events serve different consumers with different
   payloads. No unification needed.

8. **No domain invariants exist in the event dispatch path.** EventBus
   is a broadcast channel with a fire-and-forget `emit()`. PulseDomainBridge
   is a stateless translator. Neither holds mutable state, enforces
   invariants, or requires persistence. These are infrastructure
   plumbing, equivalent to `shutdown_token` or `log` — cross-cutting
   channels, not domain concepts.

## Decision

**Dissolve Book XVI.** EventBus and Pulse do not warrant unification
into a bounded context. The existing architecture correctly separates
three concerns:

- **EventBus** = domain event dispatch with pluggable listeners (chirp,
  pulse bridge, timer). Cross-cutting infrastructure.
- **Pulse channel** = SSE firehose merging domain events (via bridge)
  with transport events (via tap). Transport infrastructure.
- **Per-aggregate `changes()`** = internal state-transition notifications
  for infrastructure subscribers. Domain-owned, per-aggregate.

These three channels serve different event populations, different
consumers, and different semantic levels. Merging them would conflate
internal change notifications with user-facing events and transport
telemetry.

### Actions taken

1. **No `domain/events/` aggregate module created** — there is no domain
   state to own, no invariants to enforce, no mutable state to protect.

2. **No `PulseProjectionTask`** — the existing PulseDomainBridge
   (EventBus listener) and spawn_transport_tap pattern is simpler and
   more correct than a task that subscribes to 10+ aggregate channels.

3. **EventBus stays as `AppState::event_bus`** — it is cross-cutting
   infrastructure plumbing, like `shutdown_token` and `log`.

4. **Pulse stays as `AppState::pulse`** — it is the SSE firehose channel,
   correctly fed by PulseDomainBridge + transport tap.

5. **`domain/events.rs` stays as-is** — the `DomainEvent` enum and its
   sub-event types are value objects (wire contracts), not aggregate state.

6. **`infra/listeners/` stays as-is** — ChirpListener, PulseDomainBridge,
   and TimerListener are infrastructure listeners in the right layer.

7. **Security dual-stream preserved** — `PondEvent::EnrollmentChanged`
   on EventBus and `SecurityChanged` via `changes()` serve different
   consumers. The context-map note is updated to reflect that this is
   the intended design, not deferred debt.

8. **Context map updated** — EventBus/Pulse marked as dissolved with
   rationale.

## Consequences

- `EventBus`, `PulseEvent`, `DomainEvent`, and the three listeners
  remain in their current locations. No code changes.
- The ~25 `event_bus.emit(...)` call sites remain — these are the
  correct points to emit user-facing domain events, at the call site
  where the action occurs, with full context for payload construction.
- The 5 direct `pulse.send()` call sites in storage remain — these are
  intentional bypasses for progress events that do not need chirp/timer
  handling.
- Future aggregates that need SSE presence should emit through EventBus
  (for chirp/pulse/timer handling) or directly to pulse (for
  progress-only events).
- If a future requirement needs cross-aggregate event correlation or
  event sourcing, a new ADR should evaluate the scope at that time.
