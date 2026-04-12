---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017, ARCH-0016, ARCH-0018]
---

# ARCH-0027: Security Aggregate — Pond Enrollment and Ceremony Coordination

**Date**: 2026-04-12
**Status**: Accepted
**Book**: IX of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Bounded context**: Security

## Context

ARCH-0017 Book IX specifies: "Consolidate pond + ceremony + TLS into a
single Security context with sub-contexts Pond, Ceremonies, Trust."
Chapter 1's discovery mandate requires re-evaluating this against the
actual code.

### Discovery findings (10 findings)

1. **`domain/security/` already exists with 4 files** — `mod.rs` (33
   lines, struct shell), `pond.rs` (17 lines, struct shell), `ceremony.rs`
   (17 lines, struct shell), `pond_lifecycle.rs` (275 lines, free functions
   taking `&AppState`). The struct hierarchy (`Security → Pond → Ceremony`)
   is in place, but all fields are `pub` and lifecycle logic takes
   `&AppState` instead of being encapsulated in the aggregate.

2. **`domain/pond.rs` is a separate top-level module** containing
   `PondState` (value object with `AtomicBool`/`RwLock` fields),
   `PondMetadata`, and `load_pond_metadata`/`save_pond_metadata` free
   functions. This is the enrollment state that the Security aggregate
   should own internally.

3. **`domain/ceremony/` is about offering nourishment, NOT pond
   ceremonies.** `CeremonyType` has variants `NourishOffering`,
   `NourishStone`, `NourishAll`, `Vacate`, `Replant`, `Store` — all
   lifecycle operations. `CeremonyRegistry` tracks update/migration
   workflows. The only "pond ceremony" is `CeremonyHost<PondCeremonyRules>`
   — a foreign type from `koi_common`. **Book IX must not touch
   `domain/ceremony/`.**

4. **Ports already exist as traits in `domain/traits/`** — `PondClient`
   (stone-to-stone HTTP), `CeremonyPersistence` (journal crash recovery).
   These should relocate into the Security context per the epic pattern.

5. **`PondEvent::EnrollmentChanged` already exists** in `domain/events.rs`,
   dispatched via `EventBus`. The aggregate should replace this with a
   typed `SecurityChanged` event via a `changes()` broadcast channel
   (the standard pattern), while preserving the `EventBus` path for
   backward compatibility with the pond enrollment listener task.

6. **`pond_lifecycle.rs` functions take `&AppState`** and reach across 4
   domain boundaries: `state.security.*` (mutations), `state.discovery.*`
   (certmesh core), `state.current.*` (stone identity), and
   `state.event_bus` (event emission). These become typed commands on the
   aggregate, with external dependencies injected or passed as parameters.

7. **`api/v1/pond.rs` is 1573 lines** with massive inline logic including
   certmesh tower service invocation, cornerstone discovery, proxy
   enrollment, cert file I/O, and TOTP rotation. The domain-level
   commands extracted here should be the `init`-related logic from
   `pond_lifecycle.rs`; the handlers remain as thin dispatchers.

8. **The `Security::Trust` sub-context has no separable state.** mTLS is
   managed by `stone_client.reload_tls()` (on enrollment change) and the
   `https: Arc<AtomicBool>` flag. There is no trust-specific aggregate —
   trust is a side-effect of enrollment state transitions. The planned
   `TrustChanged` event and `MtlsAcceptor` port are not warranted.

9. **11 files reference `state.security.*`** — the blast radius is
   moderate. The majority of access is reads (`pond.state.name()`,
   `pond.state.enrolled()`, `pond.active.load()`).

10. **`recover_ceremonies` lives on `AppState`** and accesses
    `state.security.pond.ceremony.journal` and
    `state.security.pond.ceremony.registry` directly. This becomes a
    command on the aggregate.

### Plan revision from ARCH-0017

| Planned | Actual |
|---------|--------|
| 3 sub-contexts (Pond, Ceremonies, Trust) | 1 aggregate — Security owns pond state, ceremony infra, and HTTPS flag together. No Trust sub-context. |
| `PondChanged`, `CeremonyChanged`, `TrustChanged` events | `SecurityChanged` with `ChangeKind` variants covering enrollment transitions and ceremony lifecycle |
| `PondCertStore`, `MtlsAcceptor` ports | `PondClient` (existing), `CeremonyPersistence` (existing) relocated into context. No new ports. |
| Touch `domain/ceremony/` | Do NOT touch — nourishment ceremonies are a separate bounded context |

## Decision

Build a `Security` DDD aggregate that:

1. **Encapsulates private state**: `PondState`, `pond_active`, `https`
   flag, `CeremonyHost`, `CeremonyRegistry`, `CeremonyJournal` — all
   behind private fields.

2. **Typed commands**:
   - `mark_enrolled(cornerstone)` — set enrollment state, emit event
   - `mark_unenrolled()` — clear enrollment state, emit event
   - `refresh_active(is_active)` — update `pond_active` from external check
   - `set_https_started()` / `clear_https_started()` — HTTPS listener state
   - `set_pond_name(name)` — update decorative name
   - `seed_state(enrolled, name)` — boot-time seeding (no event)
   - `recover_ceremonies()` — load incomplete ceremonies from journal

3. **Typed queries**:
   - `enrolled()` → `bool`
   - `pond_active()` → `bool`
   - `cornerstone()` → `Option<String>`
   - `pond_name()` → `Option<String>`
   - `https_started()` → `bool`
   - `ceremony_registry()` → borrowed access for ceremony operations
   - `ceremony_journal()` → borrowed access for persistence

4. **`SecurityChanged` event** with 3 `ChangeKind` variants:
   `Enrolled`, `Unenrolled`, `PondRenamed`. Emitted via `changes()`
   broadcast channel. The existing `PondEvent::EnrollmentChanged` on
   `EventBus` is preserved as a dual-stream (Book II/IV precedent) for
   the pond enrollment listener task until Book XVI unifies the event bus.

5. **Relocated ports**: `PondClient` and `CeremonyPersistence` traits
   move from `domain/traits/` to `domain/security/`.

6. **`Arc<Metrics>` injection** with `register_with_kinds` for enrollment
   and ceremony event counters.

7. **`domain/pond.rs` absorbed**: `PondState` becomes internal to the
   Security aggregate. `PondMetadata` and persistence helpers remain as
   a value object + free functions (they are used at bootstrap before the
   aggregate exists).

### What stays unchanged

- `api/v1/pond.rs` handlers — thin dispatchers, not domain code
- `domain/ceremony/` — nourishment ceremonies, separate bounded context
- `infra/ceremony_journal.rs` — adapter for `CeremonyPersistence` port
- `infra/stone_client.rs` — adapter for `PondClient` port
- `tasks/task_defs/pond_enrollment_listener.rs` — consumer of events
- Certmesh integration (`koi_certmesh`) — delegated, not wrapped

### Exit criteria

- 0 `state.security.pond.state.*` direct field access (use aggregate queries)
- 0 `state.security.pond.active.*` direct field access (use `pond_active()`)
- 0 `state.security.https.*` direct field access (use `https_started()`)
- `PondClient` and `CeremonyPersistence` traits no longer in `domain/traits/`
- `domain/pond.rs` `PondState` no longer re-exported from `domain/mod.rs`
  (absorbed into aggregate internal state)
- `SecurityChanged` event with `changes()` broadcast
- `Arc<Metrics>` injected with domain registration
- Tests for aggregate commands and queries

## Consequences

- Pond enrollment state is private — no scattered `AtomicBool` stores from
  handlers or lifecycle functions.
- Enrollment transitions are observable via `changes()` — tasks can
  subscribe to the typed stream rather than the generic EventBus.
- The `domain/ceremony/` module (nourishment ceremonies) remains
  untouched — its own aggregate extraction happens in a later book.
- The pond API handlers continue to work as thin dispatchers — Book IX
  does not change the HTTP surface.
