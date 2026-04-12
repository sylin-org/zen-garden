---
audience: [developer, ai]
doc_type: postmortem
status: final
date: 2026-04-12
---

# ARCH-0017 Epic Postmortem

**Epic**: DDD Monolith — Pattern-Enforced Bounded Contexts Across Moss
**Duration**: 2026-04-11 to 2026-04-12 (2 calendar days)
**Commits**: ~81 commits on `dev`
**ADRs produced**: 20 (ARCH-0018 through ARCH-0037)

---

## What landed

### Aggregates extracted: 11

| Aggregate | Type | Book | ADR |
|-----------|------|------|-----|
| Offerings | Persistent (pre-existing, retrofitted) | ARCH-0016 + I + XVIII | ARCH-0016, ARCH-0036 |
| Metrics | Ephemeral, lock-free | I | ARCH-0018 |
| Tool | Ephemeral, dual event streams | II | ARCH-0019 |
| Topology | Persistent, composition layer | III | ARCH-0020 |
| Jobs | Ephemeral, reaper task | IV | ARCH-0021 |
| Catalog | Persistent, typed errors | V | ARCH-0022 |
| Subsystems | Ephemeral, lock-free (watch) | VI | ARCH-0023 |
| Health | Stateless probe facade | VII | ARCH-0024 |
| Storage (Bank) | Bank view aggregate, typed errors | VIII-a/b | ARCH-0025, ARCH-0026 |
| Security | Enrollment state, dual event streams | IX | ARCH-0027 |
| Discovery | mDNS/Koi wrapper | X | ARCH-0028 |

### Dissolutions: 7

Modules evaluated and determined to not warrant the aggregate pattern:

| Module | Reason | Book | ADR |
|--------|--------|------|-----|
| Orchestration | No domain state, no invariants | XI | ARCH-0029 |
| ContainerRuntime | Already a sealed anti-corruption layer | XII | ARCH-0030 |
| Configuration | Loaded once at boot, frozen | XIII | ARCH-0031 |
| Persistence | Consolidation target already exists in garden-common | XIV | ARCH-0032 |
| Logging | Pure infrastructure, single broadcast channel | XV | ARCH-0033 |
| EventBus/Pulse | Different event populations, no unification needed | XVI | ARCH-0034 |
| HttpApi | Handlers already dispatch to typed commands/queries | XVII | ARCH-0035 |

### Other structural changes

- **Book XVIII**: Offerings strangler removal. 81 `.read().await` sites migrated to typed queries. `ActiveGuard`/`CandidatesGuard` deleted.
- **Book XIX**: `AppState` renamed to `Moss` across 97 files (555 occurrences). 7 delegate methods inlined.
- **Book XX**: 3 deferred renames resolved. Scaffolding tracker emptied. Epic closed.

### Test count progression

| Book | Tests |
|------|-------|
| Baseline (pre-epic) | 627 |
| Book I (Metrics) | 627 |
| Book II (Tool) | 649 |
| Book III (Topology) | 649 |
| Book IV (Jobs) | 671 |
| Book V (Catalog) | 692 |
| Book VI (Subsystems) | 707 |
| Book VII (Health) | 724 |
| Book VIII-a (Storage) | 735 |
| Book VIII-b (Storage API) | 741 |
| Book IX (Security) | 753 |
| Book X (Discovery) | 761 |
| Books XI-XX | 764 |

Net: +137 tests over the epic.

---

## The dissolution pattern

The most significant discovery of the epic was that **7 of 20 planned books dissolved** — the module under evaluation turned out to already be correctly structured, or to not warrant the aggregate pattern at all. The dissolution rate was 35%.

This was not a planning failure. The original plan was a hypothesis based on reading code and naming modules. The Discovery Mandate (added after Book 0) required each book's Chapter 1 to re-evaluate the plan against actual code before writing any implementation. When the code said "this is already correct" or "this has no domain state," the right answer was to document that finding and move on.

The dissolutions were:
- **No domain state**: Orchestration (bag of coordination fields), Configuration (frozen at boot), Logging (single broadcast channel)
- **Already correct**: ContainerRuntime (sealed Bollard anti-corruption layer), Persistence (garden-common already provides the helpers), EventBus/Pulse (different event populations serving different consumers)
- **Architecture already clean**: HttpApi (16 books of aggregate extraction made handlers thin dispatchers automatically)

---

## What took longer than expected

1. **Storage (Book VIII)** was split into VIII-a (domain model) and VIII-b (API surface) because the Bank aggregate required both a domain restructure and an API route migration. This was the only book that split.

2. **Security (Book IX)** required relocating `PondClient` and `CeremonyPersistence` port traits from `domain/traits/` into the Security context, and absorbing `PondState` — more migration surface than anticipated.

3. **Tool (Book II)** had 25 infra-layer struct-field sites (`tool.registry.read/write`) that couldn't migrate to typed methods because they were legitimate infrastructure-layer reads. The `pub(crate) registry` strangler was retained intentionally.

## What took less time than expected

1. **Books XI-XVII** (dissolutions) each completed in a single commit. The Chapter 1 evaluation determined no code changes were needed beyond cleanup. Seven books in rapid succession.

2. **Book XIX** (AppState rename) was a mechanical find-and-replace across 97 files — large diff but simple execution.

3. **Book XVIII** (Offerings strangler removal) was methodical but straightforward — 81 call sites, each with a clear migration path to one of five typed query methods.

---

## Design decisions that proved right

1. **The Discovery Mandate.** Requiring Chapter 1 to re-evaluate the plan prevented seven books of unnecessary refactoring. Without it, we would have extracted aggregates from modules that didn't need them, adding complexity for negative architectural value.

2. **Private state + typed commands/queries.** The `RwLock<State>` behind a typed API surface prevented the class of bug that ARCH-0016 originally fixed (mutation bypassing the canonical boundary). No similar bugs were discovered during the epic.

3. **`finalize` pipeline (persist, meter, emit).** The three-step ordering invariant was consistent across all persistent aggregates. No aggregate needed a different ordering.

4. **Ephemeral aggregate pattern.** Recognizing that not every aggregate needs persistence (Metrics, Jobs, Subsystems, Health) kept the pattern lightweight. Lock-free variants (Metrics, Subsystems) were even simpler.

5. **Dual event streams.** Tool and Jobs needed both internal `changes()` events and wire-format events for existing SSE/UDP consumers. Documenting this as a named deviation rather than forcing a single stream preserved backward compatibility.

6. **Scaffolding tracker.** Tracking every inter-book temporary artifact with a stable ID, removal trigger, and check pattern ensured nothing was forgotten. The tracker reached exactly 1 active scaffold at peak (the Offerings ActiveGuard) and ended at 0.

---

## What we would do differently

1. **Start with the dissolution evaluation.** Several books spent Chapter 1 discovering that the module didn't need the aggregate pattern. A pre-epic audit pass (before Book 0) evaluating each module against the "when to apply" criteria would have shortened the book list from 20 to ~13 and made the scope clearer upfront.

2. **Batch the dissolutions.** Books XI-XVII were each a single commit. They could have been batched into a single "dissolution sweep" book with seven sections, reducing ADR overhead.

3. **Wire-format renames earlier.** The deferred renames (Job.offerings, registry-loader) could have been resolved in their respective books using `#[serde(rename)]`. The serde-rename approach preserves wire compatibility with zero consumer-side changes. Deferring them to Book XX added tracking overhead for what turned out to be a 10-minute fix.

---

## Key architectural insights

These insights emerged from the user's architectural direction during the epic:

1. **Architectural leanness over code leanness.** The goal was never to minimize line count. It was to ensure every domain boundary is enforced by the type system, every mutation goes through a typed command, and every state access goes through a typed query. A monolith with proper internal segmentation is architecturally superior to a microservice mess or a flat-struct monolith.

2. **SoC over DRY.** When separation of concerns and DRY conflicted, separation won. Each aggregate owns its state independently, even if that means some patterns are repeated across aggregates. The `finalize` pipeline is nearly identical in every aggregate — and that's fine, because each aggregate owns its pipeline independently.

3. **Consumer ergonomics are non-negotiable.** The typed query API (`snapshot`, `find_by_id`, `find_by_name`, `with_active`) was designed for call-site clarity. Callers never need to understand locking, serialization, or event plumbing. They call a method and get a value.

4. **Bank as aggregate root.** The storage domain discussion (Book VIII) established that Bank is the user-facing aggregate root — users think in terms of named storage banks, not volumes or mount points. Volumes are internal state managed by the VolumeIngestor.

---

## Final state

- **Scaffolding tracker**: 0 active entries, 0 deferred renames
- **Context map**: 11 Full contexts, 4 Partial contexts (Current, Storage, Presence, Companion), 7 dissolved, 2 retired (Platform, AppState)
- **Pattern spec**: complete with 5 design dimensions and decision matrix covering all 11 aggregates
- **Test count**: 764
- **Status**: ARCH-0017 marked `completed`
