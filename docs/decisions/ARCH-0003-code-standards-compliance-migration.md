---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-11
---

# ARCH-0003: Code Standards Compliance Migration

**Date**: 2026-03-11
**Status**: Accepted
**Depends on**: ARCH-0001 (SoC/DDD Architecture), ARCH-0002 (PlatformRuntime Trait)

## Context

The Zen Garden codebase accumulated a structural debt over several development phases. The primary symptoms:

1. **Flat `AppState`** — a 64-field struct where underscore-separated names encode sub-concepts that belong in the type system. Fields like `storage_tick_tx`, `pond_ceremony_host`, and `https_started` encode namespace, domain membership, type kind, and state machine roles as name suffixes rather than as types.

2. **Type-duplicating names** — `_tx`, `_arc`, `_clone`, `_flag` suffixes repeat information the type already declares. `https_started: AtomicBool` carries the word "Bool" twice.

3. **Missing domain context structs** — related state is scattered across a flat namespace. `Storage`, `Security`, `Companions`, and `Presence` are not types; they are name prefixes. The compiler has no model of the domain.

4. **Primitive obsession** — `stone_id: String`, `stone_name: String`, and similar fields accept any string. Transpositions are runtime bugs, not compile errors.

5. **Bool flag pairs** — `pond_active: Arc<AtomicBool>` and `https_started: AtomicBool` encode multi-state machines as independent booleans. Invalid combinations are representable.

6. **API handlers taking full `AppState`** — handlers declare a dependency on the entire world rather than on the two or three fields they actually need. Axum's `FromRef` exists to enforce minimal dependency surfaces, but it is not used.

7. **`anyhow` inside domain logic** — stringly-typed errors lose structure, are not matchable by callers, and allocate on the heap. Typed `thiserror` enums are the correct model for domain boundaries.

These are not stylistic preferences. They mean the compiler does not understand the domain model. Structural invariants that could be enforced at compile time are instead enforced (or not) at runtime.

The authoritative standard is documented in `docs/code-standards.md`.

## Decision

Migrate the entire codebase to full compliance with `docs/code-standards.md` in dependency-graph order, one crate at a time, with `cargo check --all` passing at every commit.

### Migration Order

The workspace dependency graph determines the order. Crates lower in the graph are refactored first so that changes in foundational types cascade correctly to dependents.

```
Wave 0  garden-build-utils          (no deps; trivial)
Wave 1  garden-common               (root; newtypes, error enums)
Wave 2  garden-companion-sdk        (common)
Wave 3  garden-lantern              (common)
        garden-probe                (common)
Wave 4  garden-cricket              (common + sdk)
        garden-firefly              (common + sdk)
Wave 5  garden-rake                 (common)
Wave 6  garden-moss                 (common; the primary target)
```

Within each wave, changes are applied in a fixed sequence:

| Pass | Rules | Rationale |
|------|-------|-----------|
| a | Naming — types, fields, locals | Mechanical; no API surface change |
| b | Newtypes for domain identifiers | Additive; old `String` sites still compile until migrated |
| c | Channel field naming conventions | Struct fields only; local `tx`/`rx` destructuring is idiomatic and unchanged |
| d | Namespace extraction (nested structs) | Structural; cascades to dependents |
| e | State machine enums, typestate | Replaces bool flags and `Option` in long-lived structs |
| f | Typed domain error enums | Domain logic only; application boundaries keep `anyhow` |
| g | `FromRef` + `#[must_use]` | Axum handlers; enforces minimal dependency surfaces |

### moss Sub-Waves

`garden-moss` is ~8 000 lines across three layers. Its Wave 6 is divided:

```
6a  moss/domain/         Pure logic — no infra deps; start here
6b  moss/infra/          External integrations
6c  AppState restructure  Introduce 8 domain context structs; migrate all fields;
                          add FromRef impls; update all handlers in one commit
6d  moss/api/            Handler cleanup after 6c
6e  bootstrap/run.rs     Declarative pipeline replacing the sequential God function
```

Step 6c is intentionally a single large commit. A partially-restructured `AppState` leaves the codebase in an inconsistent state that is harder to reason about and harder to build on than either the old form or the new form. The commit boundary is the atomic unit of architectural change.

### Additive-Then-Prune

For changes that cascade across call sites (newtypes, domain context structs):

1. **Add** the new type alongside the old representation
2. **Migrate** call sites incrementally within the same wave
3. **Remove** the old representation when all call sites are updated
4. Commit at each stage — the build must pass between add, migrate, and remove

This avoids a single uncommittable diff spanning hundreds of files.

### Build Gate

Every commit in this migration must pass:

```bash
cargo check --all
cargo clippy --package <crate-under-change> -- -D warnings
```

The CI gate on the `dev` branch enforces this automatically.

## Consequences

**Positive**:
- `AppState` becomes a thin facade over 8 typed domain contexts; handler dependency surfaces become explicit and compiler-enforced
- `StoneId`, `StoneName`, `VolumeName` and similar newtypes eliminate a class of transposition bugs
- Domain errors become matchable; callers can handle specific failure modes rather than parsing strings
- New contributors can read the type hierarchy and understand domain boundaries without reading implementation files
- Each domain context is independently testable — no whole-`AppState` setup required

**Negative / Trade-offs**:
- Wave 6c is the highest-risk commit: all handlers, all `AppState` field accesses, all `FromRef` impls change simultaneously. It cannot be made smaller without leaving the codebase in a worse inconsistent state
- Newtypes require `From<String>` / `Into<String>` conversions at boundaries (serialization, HTTP response formatting). This is mechanical but adds boilerplate at those sites
- Some existing code is correct-by-convention (e.g. `_tx` fields that are always senders). The migration makes the convention redundant — `broadcast::Sender<T>` carries the direction. The removal of the suffix is not a loss of information

## Out of Scope

- Per-domain `platform.rs` infrastructure differences — correct as-is per ARCH-0002
- Orchestrator crates (`src/orchestrators/`) — standalone builds, separate migration if needed
- `anyhow` at application boundaries (`main`, top-level Axum handlers) — remains correct; only domain internals migrate to typed enums
- Test code style — tests follow production conventions where practical, but are not gated by this migration
