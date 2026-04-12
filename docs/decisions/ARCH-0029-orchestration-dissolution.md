---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017, ARCH-0004, ARCH-0025]
completed: 2026-04-12
---

# ARCH-0029: Orchestration Dissolution

**Date**: 2026-04-12
**Status**: Accepted
**Book**: XI of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Bounded context**: Orchestration (dissolved)

## Context

ARCH-0017 Book XI specifies: "Deep-clean the existing partial Orchestration
context into Tick, Nurturing, and Election sub-aggregates. Add
OrchestrationTick, NurturingChanged, ElectionResolved events. Add
ElectionTransport port."

Chapter 1's discovery mandate requires re-evaluating this against the
actual code.

### Discovery findings (7 findings)

1. **`domain/orchestration/` is 110 lines across 4 files.** The top-level
   `Orchestration` struct has 3 fields: `storage`, `nurturing`,
   `nourishment`. Zero methods, zero invariants, zero business logic. It is
   a bag of coordination primitives and infrastructure references.

2. **`StorageOrchestration` (51 lines)** holds a `Tick` struct (two
   broadcast senders: raw + debounced), a `Notify` for nudge, an
   `mpsc::Sender` for rescan, and an `Arc<S3Listeners>` infra type. One
   typed method (`tick_stream`) wraps `debounced.subscribe()`. All 4 fields
   are storage coordination primitives — they naturally belong on the
   Storage domain context.

3. **`NurturingOrchestration` (15 lines)** holds two `Arc` infra
   references: `OsHarvestOps` and `NurturingStore`. Zero methods, zero
   logic. These are infrastructure handles consumed by API handlers and
   ceremony phases.

4. **`NourishmentOrchestration` (16 lines)** holds one `Arc<RwLock<HashMap>>`
   of SSE job broadcast channels. Zero methods, zero logic. Three call
   sites (all in `updates.rs`) read/write this map directly.

5. **Election is NOT in Orchestration.** It lives in `Presence::elections`
   and has since ARCH-0004. The ARCH-0017 plan anticipated an `Election`
   sub-aggregate here, but the code never had one. The storage Primary/
   Dormant role assignment is a standalone background task
   (`storage_orchestration.rs`) that has no state of its own.

6. **No sub-namespace qualifies as an aggregate.** The pattern spec says
   "Do not apply when a context is a facade over a single infrastructure
   dependency and has no state." All three sub-namespaces are stateless
   facades over channels or infra references. None hold mutable domain
   state, none enforce invariants, none need events or ports.

7. **Blast radius is moderate.** ~25 sites reference
   `orchestration.storage.*`, ~13 reference `orchestration.nurturing.*`,
   and 3 reference `orchestration.nourishment.*`. All are field-path
   changes, not behavioral changes.

### Plan change

ARCH-0017 anticipated 3 sub-aggregates (Tick, Nurturing, Election) with
events and ports. Reality has 0 aggregates needed: Orchestration is a
coordination bag that should be dissolved into the contexts its fields
naturally serve.

**Changed from**: "Deep-clean into sub-aggregates with events and ports."
**Changed to**: "Dissolve the Orchestration namespace by relocating each
sub-namespace to its natural domain owner. Delete the `Orchestration`
struct."

## Decision

### Dissolution targets

1. **Storage coordination** (`tick`, `nudge`, `rescan`, `s3_listeners`) →
   moves into `current.storage` as a `Coordination` sub-struct. Access path
   changes from `state.orchestration.storage.*` to
   `state.current.storage.coordination.*`. The `tick_stream()` method moves
   to `Storage`.

2. **Nurturing infrastructure** (`harvest_ops`, `store`) → promoted to a
   direct `AppState` field (`state.nurturing: Arc<Nurturing>`). Access path
   changes from `state.orchestration.nurturing.*` to
   `state.nurturing.*`.

3. **Nourishment SSE channels** (`jobs`) → promoted to a direct `AppState`
   field (`state.nourishment: Arc<Nourishment>`). Access path changes from
   `state.orchestration.nourishment.*` to `state.nourishment.*`.

4. **`Orchestration` struct** → deleted along with its `FromRef` impl.

5. **`domain/orchestration/` module** → deleted entirely.

### What is NOT done

- No new aggregate, no `OrchestrationChanged` event, no `ElectionTransport`
  port. These were planned artifacts of a sub-aggregate structure that the
  code does not warrant.
- No changes to `tasks/storage_orchestration.rs` (the background task
  itself). Its logic and tests are sound; only its access paths to the
  coordination primitives change.
- No election changes. Elections stay in Presence.

## Consequences

- `AppState` loses 1 field (`orchestration`) and gains 2 thin fields
  (`nurturing`, `nourishment`), net -1 moving part at the top level.
  `current.storage` gains a `coordination` sub-struct (namespace, not a
  new concept).
- Storage coordination primitives are co-located with the storage data
  they coordinate, following code-standards section 5 (domain ownership
  through struct nesting).
- The `Orchestration` bounded context ceases to exist. The context map
  entry moves to "Retired."
- No new tests needed (the existing `storage_orchestration.rs` tests
  and integration tests validate the same logic via changed access paths).
