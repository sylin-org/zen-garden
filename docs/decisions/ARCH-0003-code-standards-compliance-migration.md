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

8. **Transport copies** — `StoneResponse`, `StoneEntry`, `StoneRecord` and similar structs duplicate fields from the canonical `Stone` type. Each copy drifts independently and must be kept in sync manually. The canonical type, defined once in `garden-common`, is the wire format contract.

9. **File/concept mismatch** — `app_state.rs` contains all domains; `coordinator.rs` contains startup logic for all domains; catch-all files (`helpers.rs`) contain whatever didn't fit elsewhere. Files do not reflect the domain model; the module tree is not navigable by domain concept.

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
| b | Domain value objects (`Stone`, `Volume`, `Job`, …) | Additive; old `String` call sites still compile until migrated |
| c | Channel field naming conventions | Struct fields only; local `tx`/`rx` destructuring is idiomatic and unchanged |
| d | Namespace extraction (nested structs) | Structural; cascades to dependents |
| e | State machine enums, typestate | Replaces bool flags and `Option` in long-lived structs |
| f | Typed domain error enums | Domain logic only; application boundaries keep `anyhow` |
| g | `FromRef` + `#[must_use]` | Axum handlers; enforces minimal dependency surfaces |
| h | Domain event API (`on_X()` / `X_stream()`) | Encapsulates channels; SSE handlers use domain API, not raw `.subscribe()` |
| i | File/module reorganization | 1:1 coupling between file name, contained types, and domain concept; rename commit then content commit |

### Wave 0 — garden-build-utils

Build-time helper only (proc macros, build script utilities). No runtime types, no domain state. Pass `a` only: rename any `Context`/`Manager` type suffixes. Expected to be a single small commit.

### Wave 1 — garden-common

The root everyone depends on. Changes here cascade to all other crates, so they must be complete and stable before Wave 2 begins.

Primary targets:
- **Introduce domain value objects**:
  - `Stone { id, name, host }` — `id` is permanent; `name` and `host` are mutable at runtime
  - `Current { stone: Arc<RwLock<Stone>>, environment: Environment }` — the node's mutable self-description; replaces flat `stone_id`, `stone_name`, `stone_host` fields
  - `Environment { os: OsKind }` — static after startup
  - `Volume { name, … }` — canonical storage volume, replaces `VolumeName: String` patterns
  - `Companion { id, name, port, manifest: Manifest }` — replaces `CompanionManifest`; `manifest` is a nested field, not a peer type
  - `Manifest { version, description, commands }` — nested inside `Companion`
  - `Job { id, … }` — replaces bare `job_id: String` parameters
  - Plain `&str` is retained at pure key boundaries (lookups, deletes)
- **Channel naming audit** across any shared channel types — remove `_tx`/`_rx` from any long-lived struct fields in shared types.
- **Shared error enums** — introduce `thiserror`-based enums for error kinds that cross crate boundaries (e.g. nourishment errors, discovery errors). Application-boundary types keep `anyhow`.

**Serde loci for breaking field renames** — these JSON field names change when the canonical types are defined. All consumers must be updated in their respective waves with no backward-compat shims:

| Old JSON field | New JSON field | Consumers to update |
|---|---|---|
| `stone_id` | `id` | `garden-rake`, `koan-framework/Koan.ZenGarden` |
| `stone_name` | `name` | `garden-rake`, `koan-framework/Koan.ZenGarden` |
| `stone_host` | `host` | `garden-rake`, `koan-framework/Koan.ZenGarden` |
| `companion_id`, `companion_name` | `id`, `name` | `garden-rake`, `garden-moss` API |

Waves 2–6 adopt the value objects introduced here, replacing flat `String` parameters at call sites.

### Wave 2 — garden-companion-sdk

Provides the `CommandHandler` trait and companion HTTP server scaffolding.

Primary targets:
- `shutdown_tx: watch::Sender<bool>` in `server.rs` → rename to `shutdown` (type declares direction)
- `CommandHandler` is a trait name — the suffix is acceptable for traits; no change needed
- Local variable naming: shadow `connection` and `context` clones rather than `_clone` suffixes
- Adopt `Companion` value object from Wave 1

### Wave 3 — garden-lantern

Service registry daemon. Small codebase, limited debt.

Primary targets:
- `AppState` type name: acceptable (it is the application root); no rename needed
- `sse_tx: broadcast::Sender<SseEvent>` → rename to `sse` (type declares it is a sender)
- Any flat `String`-typed stone or service identifiers → adopt value objects from Wave 1

**garden-probe**: Diagnostic/health probe. Passes `a` only. Minimal expected violations.

### Wave 4 — garden-cricket, garden-firefly

Companion binaries. Use the companion SDK from Wave 2.

**garden-cricket** primary targets:
- `CricketEventHandler` type name: drop the `Handler` suffix → `CricketEvents` or `CricketInput`
- `tune_manager` field: `Manager` suffix implies architectural role — rename to domain concept (e.g. `playlist`)
- Audit for `_clone` variable patterns; shadow instead

**garden-firefly** primary targets:
- `has_seed_bank: bool` and `has_services: bool` in `AnimationContext` — two related booleans encoding a 4-state machine. Replace with:
  ```rust
  pub enum StoneRole { Bare, WithSeedBank, WithServices, Full }
  ```
- `AnimationContext` type name: drop the `Context` suffix → `Animation` (position in hierarchy provides context)
- `_for_retry` / `_for_shutdown` local variable suffixes in `main.rs` — shadow the original binding within a scoped block instead
- Adopt `Stone` value object from Wave 1; use `stone.name` in place of bare `String`

### Wave 5 — garden-rake

CLI client. Generally cleaner than moss; primary debt is in connection state and command routing.

Primary targets:
- Audit `context.rs` state struct — already relatively clean; verify no `Context`/`State` suffixes remain
- Command dispatch: verify handlers declare minimal state rather than a full connection bag
- Adopt `Stone`, `Volume` value objects from Wave 1 across all API call sites
- Any `_clone` local variable patterns → shadow
- Error handling: CLI boundary is `anyhow` (correct); verify no `anyhow::anyhow!` inside domain logic if any exists

### Wave 6 — garden-moss

~8 000 lines across three layers. Split into sub-waves:

```
6a  moss/domain/         Pure logic — no infra deps; start here
6b  moss/infra/          External integrations
6c  AppState restructure  Introduce 8 domain context structs; migrate all fields;
                          add FromRef impls; update all handlers in one commit
6d  moss/api/            Handler cleanup after 6c
6e  bootstrap/run.rs     Declarative pipeline replacing the sequential God function
```

**6a — domain layer**: Apply passes `a`–`f`. No infra imports means changes are self-contained. Typed error enums for storage, security, discovery, and offerings domains.

**6b — infra layer**: Apply passes `a`–`e`. Infra types (Docker client wrappers, platform adapters) renamed; any local `_clone` shadows fixed.

**6c — AppState restructure**: The highest-risk step; intentionally one commit. Introduce 7 domain context structs, migrate all 64 flat fields, add `FromRef` impls for all handler dependencies, update every handler, and reorganize into per-domain files. A partially-restructured `AppState` is worse than either the old or new form — the commit is the atomic unit of change.

`app_state.rs` becomes a thin re-export after this step. Each domain context lives in its own file. `coordinator.rs` is dissolved — background task startup moves into each domain context's `start()` method.

Target structs and their current flat-field mappings:

| AppState field | Type | Current flat fields |
|---|---|---|
| `current` | `Current` | `stone_id` → `current.stone.id`; `stone_name` → `current.stone.name`; `stone_host` → `current.stone.host`; platform config → `current.environment` |
| `storage` | `Arc<Storage>` | `storage_tick_tx`, `storage_agg_tx`, `storage_changed_tx`, `orchestration_nudge`, `volumes`, `storage_health`, … |
| `security` | `Arc<Security>` | `pond_active`, `pond_ceremony_host`, `https_started`, `ca_cert`, … |
| `companions` | `Arc<Companions>` | `companions`, `companion_ports`, … |
| `presence` | `Arc<Presence>` | `presence_*` fields |
| `discovery` | `Arc<Discovery>` | `topology`, `discovered_stones`, … |
| `infra` | `Arc<Infra>` | `docker`, `runtime`, … |
| `offerings` | `Arc<Offerings>` | `manifests`, `taxonomy`, … |

`Identity` is eliminated. `stone_id`/`stone_name`/`stone_host` collapse into `current.stone` (`Arc<RwLock<Stone>>`). `stone.id` is permanent; `stone.name` and `stone.host` are mutable at runtime (user rename, DHCP renewal). `CompanionManifest` is dissolved into `Companion.manifest`.

**6d — api layer**: Handler cleanup after 6c. Each handler's `State(...)` extractor now names its actual dependency (`State(storage): State<Arc<Storage>>`) rather than the full `AppState`. Pass `g`.

**6e — bootstrap/run.rs**: Replace the ~2 100-line sequential God function with a declarative startup pipeline. Each domain context gains an `init()` and `start(token)` method. `run.rs` becomes an orchestration sequence of named stages, ~200 lines.

### Additive-Then-Prune

For changes that cascade across call sites (value objects, domain context structs):

1. **Add** the new type alongside the old representation
2. **Migrate** call sites incrementally within the same wave
3. **Remove** the old representation when all call sites are updated
4. Commit at each stage — the build must pass between add, migrate, and remove

This avoids a single uncommittable diff spanning hundreds of files.

### File Reorganization

Pass `i` within each wave reorganizes files to match their domain concept. Each reorganization is two commits:

1. **Rename commit** — pure `git mv`, no content changes. Git detects the rename; `git log --follow <file>` traces history across it.
2. **Content commit** — edits to the moved file in a separate commit.

Mixing rename and content in one commit breaks `git log --follow`. The rename commit must be content-free.

Git blame continuity is secondary to architectural correctness — the migration is the priority. The two-commit discipline captures as much history as the tooling supports.

### Acceptance Criteria

A module is not considered finished until it fully adheres to `docs/code-standards.md`. The build gate must pass clean and no new violations may be introduced anywhere in the diff.

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
- Domain value objects (`Stone`, `Volume`, `Companion`) eliminate transposition bugs — `stone.id` and `stone.name` are unambiguous by position, not by wrapper type
- Domain errors become matchable; callers can handle specific failure modes rather than parsing strings
- New contributors can read the type hierarchy and understand domain boundaries without reading implementation files
- Each domain context is independently testable — no whole-`AppState` setup required

**Negative / Trade-offs**:
- Wave 6c is the highest-risk commit: all handlers, all `AppState` field accesses, all `FromRef` impls change simultaneously. It cannot be made smaller without leaving the codebase in a worse inconsistent state
- Value objects require serde derives and field-level serialization at API boundaries. Flat `String` parameters at pure key boundaries (lookups, deletes) remain as-is — they are honest about what they are
- `_tx` / `_rx` suffixes in existing code are correct-by-convention. The migration makes the convention redundant — `broadcast::Sender<T>` carries the direction. Removing the suffix is not a loss of information

## Out of Scope

- Per-domain `platform.rs` infrastructure differences — correct as-is per ARCH-0002
- Orchestrator crates (`src/orchestrators/`) — standalone builds, separate migration if needed
- `anyhow` at application boundaries (`main`, top-level Axum handlers) — remains correct; only domain internals migrate to typed enums
- Test code style — tests follow production conventions where practical, but are not gated by this migration
