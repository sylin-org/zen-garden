---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017]
completed: 2026-04-12
---

# ARCH-0037: AppState Dissolution — Rename to Moss

**Date**: 2026-04-12
**Status**: Accepted
**Book**: XIX of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Bounded context**: Root

## Context

After 18 books of aggregate extraction, `AppState` is a clean dependency
injection container: 22 fields, all `Arc<Aggregate>` or cross-cutting
infrastructure (shutdown token, event bus, console, start time). No raw
`Arc<RwLock<Vec<...>>>` fields remain. The only `Arc<RwLock<Option<...>>>`
is `task_supervisor`, which is set post-construction by design.

Eight methods remain on `impl AppState`:

| Method | Kind | Action |
|--------|------|--------|
| `stone_id()` | Delegate | → `self.current.stone.id` |
| `stone_name()` | Delegate | → `self.current.stone.name` |
| `recover_ceremonies()` | Delegate | → `self.security.recover_ceremonies()` |
| `log_stream()` | Delegate | → `self.log.subscribe()` |
| `pulse_stream()` | Delegate | → `self.pulse.subscribe()` |
| `request_volume_rescan()` | Delegate | → `self.current.storage.coordination.rescan` |
| `subscribe_storage_changed()` | Delegate | → `self.current.storage.changed.subscribe()` |
| `emit_storage_changed()` | Cross-cutting | Bridges event bus + broadcast + tool reprojection + orchestration nudge |

Six of seven delegates hide a single field dereference behind a method
call. Per [code standards §3](../../docs/code-standards.md), these
violate "state.\<field\>.\<method\>() is the only call shape" — callers
should navigate the struct directly.

`emit_storage_changed()` is genuinely cross-cutting: it coordinates
across `EventBus`, `current.storage.changed`, `tool::projection`, and
`current.storage.coordination.nudge`. It cannot move into any single
aggregate. It stays on the root struct.

Three backward-compatibility `pub use` re-exports in `app_state.rs`
(`Job`, `JobStatus`, `CompiledOffering`, `OfferingsFingerprint`,
`OfferingsIndex`, plus six `garden_common` offering types) predate their
aggregate extractions. They belong in `lib.rs` if still needed, not in
the struct's module.

## Decision

### 1. Inline delegate methods

Remove all seven delegate methods. Callers navigate the struct directly:

- `state.stone_id()` → `state.current.stone.id`
- `state.stone_name()` → `state.current.stone.name`
- `state.recover_ceremonies()` → `state.security.recover_ceremonies()`
- `state.log_stream()` → `state.log.subscribe()`
- `state.pulse_stream()` → `state.pulse.subscribe()`
- `state.request_volume_rescan()` → send directly on `state.current.storage.coordination.rescan`
- `state.subscribe_storage_changed()` → `state.current.storage.changed.subscribe()`

### 2. Relocate re-exports

Move backward-compat `pub use` re-exports from `app_state.rs` to
`lib.rs`. The struct module contains only the struct, its `FromRef`
impls, and the cross-cutting `emit_storage_changed` method.

### 3. Rename `AppState` → `Moss`

Per [code standards §3](../../docs/code-standards.md): "Type names name
the concept, not the architectural role. Suffixes like `Context`,
`Manager`, `Handler`, `Service` describe what a type does
architecturally, not what it is in the domain."

`AppState` is an architectural-role name. The struct IS the moss daemon's
runtime — its identity, its aggregates, its shutdown lifecycle. `Moss` is
the domain concept.

**Plan change from ARCH-0017:** The rename was estimated at ~1500 lines.
After discovery, the blast radius is 555 occurrences across 97 files.
The rename is mechanical (find-and-replace) but larger than estimated.
Per code standards §14, the rename lands in a dedicated commit (pure
`git mv` + symbol rename, no logic changes) so `git log --follow` can
trace history.

### 4. Retain `emit_storage_changed` on Moss

This method is the only non-trivial logic on the root struct. It
coordinates four aggregates and cannot move into any single one. It stays
as a cross-cutting method on `Moss`. The pattern spec documents this as
legitimate: the root struct may hold cross-cutting coordination methods
that span multiple bounded contexts.

## Consequences

- `Moss` is the canonical type for dependency injection in handlers and tasks
- `FromRef<Moss>` replaces `FromRef<AppState>` (16 impls)
- `app_state.rs` → `moss.rs` (file rename in dedicated commit)
- Callers navigate struct fields directly — no facade methods
- `emit_storage_changed` stays as the only method with cross-cutting logic
