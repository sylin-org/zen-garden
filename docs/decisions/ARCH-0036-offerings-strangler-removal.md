---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017, ARCH-0016]
completed: 2026-04-12
---

# ARCH-0036: Offerings Strangler Removal

**Date**: 2026-04-12
**Status**: Accepted
**Book**: XVIII of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Bounded context**: Offerings

## Context

ARCH-0016 introduced the `Offerings` aggregate with an `ActiveGuard`
strangler vine — `Offerings::read()` returns an `ActiveGuard` that
derefs to `&Vec<Offering>`, allowing 82 existing `.read().await` call
sites to keep compiling while they migrate opportunistically to typed
aggregate queries. The `docs/scaffolding.md` tracker lists this as an
active scaffold with removal trigger "Book XVIII."

After seventeen books of aggregate extraction, the strangler has served
its purpose. The aggregate's typed query API (`snapshot`, `find_by_id`,
`find_by_name`, `with_active`, `with_candidates`, `count_active`,
`candidates_snapshot`) has been stable since ARCH-0016 and is already
used by callers added after the aggregate was introduced.

### Discovery findings (5 findings)

1. **81 `.read().await` sites remain across 31 files.** These sites
   span API handlers (offerings, services, adoption, config, snapshots,
   updates, greenhouse, portrait, presence, offering_capabilities),
   domain modules (offering_lifecycle, services, services_internal,
   maintenance, reconciliation, adoption), and task modules
   (health_monitor, job_executors, task_scheduler, state_provider,
   nurturing_scheduler, auto_adoption, registry_loader,
   offering_reconciliation, offering_orchestration, docker_events,
   discovery, registry). Every site reads and drops the guard within
   a single scope — no guard escapes across an `.await` point.

2. **Every `.read().await` site maps to an existing typed query.**
   The access patterns are:
   - Find by name/FQN: `.iter().find(|o| o.name.fqn_eq(name))` →
     `find_by_name(name)`
   - Find by ID: `.iter().find(|o| o.offering_id == id)` →
     `find_by_id(id)`
   - Clone all: `.clone()` → `snapshot()`
   - Iterate with filter/map: `.iter().filter(...).map(...).collect()`
     → `with_active(|o| ...)`
   - Count: `.iter().filter(...).count()` → `with_active(|o| ...)`
   - Any/exists: `.iter().any(...)` → `with_active(|o| ...)`
   No new typed query methods are needed.

3. **`read_candidates()` has zero external callers.** The
   `CandidatesGuard` type and `read_candidates()` method are dead code
   outside the aggregate module itself (which uses the internal
   `self.state.read().await.candidates` directly).

4. **`AppState` delegate methods wrap typed queries already.** The six
   delegates (`get_offerings`, `get_managed_offerings`,
   `get_adopted_offerings`, `get_borrowed_offerings`, `find_offering`,
   `find_offering_by_id`) already delegate to `snapshot()`,
   `with_active()`, `find_by_name()`, `find_by_id()`. They are thin
   redirections. Their 8 callers can call the aggregate directly.

5. **`offering_lifecycle.rs` queries duplicate aggregate methods.**
   Eight functions in `offering_lifecycle.rs` (`find_by_id`,
   `find_by_name`, `find_managed`, `id_for_name`, `id_for_managed`,
   `list_all`, `exists`, `has_status`) use `.read().await` to
   re-implement queries the aggregate already exposes. Callers of these
   functions can be migrated to the aggregate directly, and the
   functions deleted.

### Plan (confirmed — no plan change)

The ARCH-0017 plan for Book XVIII is confirmed as-is:

1. Migrate all 81 `.read().await` sites to typed aggregate queries.
2. Delete `offering_lifecycle.rs` query functions (callers migrated).
3. Delete `AppState` delegate methods (callers migrated).
4. Delete `guard.rs` (`ActiveGuard`, `CandidatesGuard`).
5. Delete `Offerings::read()` and `Offerings::read_candidates()`.
6. Mark scaffold `arch-0016-active-guard` as removed in
   `docs/scaffolding.md`.

No new typed queries needed. No plan change from ARCH-0017.

## Decision

Remove the ARCH-0016 `ActiveGuard` strangler vine by migrating all
remaining `.read().await` sites to the aggregate's typed query API,
then deleting the guard types, the back-compat `read()` methods, the
`AppState` delegate methods, and the `offering_lifecycle.rs` query
functions.

## Consequences

- **Positive**: `Offerings` aggregate becomes the sole read API for
  offering state. No more bypassing the aggregate boundary through a
  raw lock guard. Every read site is a typed, documented query method.
- **Positive**: `guard.rs` module deleted. `ActiveGuard`/
  `CandidatesGuard` removed from the public API surface.
- **Positive**: `docs/scaffolding.md` active scaffold count drops to 0.
- **Neutral**: 81-site migration is mechanical — each site maps to an
  existing query. No behavioral change.

## Exit criteria

- `rg 'state\.offerings\.read\(\)' src/moss/src/` returns 0 matches
- `rg 'ActiveGuard|CandidatesGuard' src/moss/src/` returns 0 matches
- `rg 'get_offerings|get_managed_offerings|get_adopted_offerings|get_borrowed_offerings|find_offering\b' src/moss/src/app_state.rs` returns 0 matches
- `docs/scaffolding.md` entry `arch-0016-active-guard` has `status: removed`
- `cargo test --package garden-moss` passes (764+ tests)
- `cargo clippy --package garden-moss -- -D warnings` clean
