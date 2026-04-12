---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017]
completed: 2026-04-12
---

# ARCH-0032: Persistence Consolidation Dissolution

**Date**: 2026-04-12
**Status**: Accepted
**Book**: XIV of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Bounded context**: Persistence (dissolved)

## Context

ARCH-0017 Book XIV specifies: "Every domain has its own `Store` port by
now. Unify the file-backed adapter helpers so atomic-write invariants,
directory creation, temp-file naming, and error conversion happen in one
place. Deliverables: `AtomicJsonStore<T>`, `DirectoryCache<K, V>`,
canonical error conversion."

Chapter 1's discovery mandate requires re-evaluating this against the
actual code.

### Discovery findings (6 findings)

1. **`garden_common::persistence` already provides the consolidation
   target.** `atomic_write_file()` (atomic tmp+sync+rename with parent
   directory creation), `JsonStorage<T>` (generic JSON load/save/delete
   over `PersistenceProvider<T>` trait), `read_file()`, `file_exists()`,
   `delete_file()` — all exist with tests. The `AtomicJsonStore<T>`
   deliverable is `JsonStorage<T>` under a different name.

2. **Store ports are already thin.** Each domain's `Store` port is a
   2-4 method trait (`load`/`save`, occasionally `delete`). The
   file-backed adapters (`FileOfferingStore`, `FileTopologyStore`,
   `FileCatalogCache`) are 10-20 lines each. There is no abstraction
   to extract — the adapters *are* the abstraction, and they delegate
   to existing helpers.

3. **`DirectoryCache<K, V>` has exactly one consumer.**
   `CeremonyJournal` is the only directory-based persistence pattern
   (active/archive subdirectories keyed by ceremony ID). Creating a
   generic directory cache for one consumer violates the "don't create
   abstractions for the sake of it" principle.

4. **Duplicate atomic write implementations exist but are not a DDD
   concern.** Six moss infra modules hand-roll
   `tmp+write+rename` instead of calling
   `garden_common::persistence::atomic_write_file()`:
   `infra/persistence.rs`, `infra/hardware.rs`, `infra/task_store.rs`,
   `infra/nurturing_store.rs`, `infra/network/state.rs`, and
   `infra/storage/store.rs`. This is mechanical cleanup, not domain
   architecture work. It can be done as a standalone chore commit
   outside the epic.

5. **Canonical error conversion already exists.** `PersistenceError`
   in `garden_common::traits::persistence` has 4 variants
   (`ReadFailed`, `WriteFailed`, `SerializationFailed`,
   `CorruptedData`). Per-domain `Store` ports use `anyhow::Result`
   because they are invoked from domain code that already uses
   `anyhow`. Forcing them onto `PersistenceError` would add
   error-type conversion boilerplate with no correctness benefit.

6. **No new consumers are anticipated.** The remaining books
   (XV–XX) introduce Logging, Events, HttpApi, Bootstrap, Shutdown,
   and the Offerings strangler cleanup — none of which add new
   file-backed Store ports.

## Decision

**Dissolve Book XIV.** Persistence consolidation does not warrant a
bounded context, a module, or new helper types. The consolidation
target (`garden_common::persistence`) already exists and is adequate.

### Actions taken

1. **No `infra/persistence/` module created** — the existing
   `garden_common::persistence` module already provides
   `atomic_write_file`, `JsonStorage<T>`, and `PersistenceProvider<T>`.

2. **No `AtomicJsonStore<T>` created** — `JsonStorage<T>` already
   serves this role with identical semantics.

3. **No `DirectoryCache<K, V>` created** — only one consumer
   (`CeremonyJournal`), which is already well-encapsulated.

4. **No canonical error conversion forced** — `PersistenceError`
   exists in garden-common but domain Store ports correctly use
   `anyhow::Result` for ergonomics.

5. **Context map updated** — Persistence marked as dissolved with
   rationale.

### Deferred mechanical cleanup

The 6 duplicate atomic write implementations in moss infra could be
migrated to `garden_common::persistence::atomic_write_file()` as a
standalone chore. This is not part of the ARCH-0017 epic because it
is mechanical deduplication, not domain architecture.

## Consequences

- `garden_common::persistence` remains the canonical persistence
  utility module. No moss-specific persistence module is needed.
- Each Store port adapter remains in its owning domain/infra module
  (e.g., `domain/offerings/store.rs`, `domain/topology/store.rs`).
  This is the correct location per code-standards §14.
- New aggregates that need file persistence should use
  `garden_common::persistence::JsonStorage<T>` or
  `atomic_write_file()` directly in their adapter.
- The inline atomic write duplicates in moss infra are technical
  debt, not architectural debt.
