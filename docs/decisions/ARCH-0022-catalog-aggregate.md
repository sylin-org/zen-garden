---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-11
canonical: true
---

# ARCH-0022: Catalog Aggregate — Book V of ARCH-0017

**Date**: 2026-04-11
**Status**: Accepted
**Book**: V of [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)
**Depends on**: [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) (epic), [ARCH-0018](ARCH-0018-metrics-aggregate.md) (`Arc<Metrics>` injection), [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md) (Store-port precedent; Catalog is the third persistent aggregate after Offerings and Topology)

## Context

Book V extracts the `Catalog` bounded context. Today its state and logic are scattered across three locations with two raw fields on `AppState`:

1. **`AppState::manifest_registry: Arc<ManifestRegistry>`** — the frozen source-of-truth for offering templates (managed + borrowed + adopted) and hardware manifests. Loaded synchronously in `bootstrap::run::build_state()` via `infra::load_sw_manifests_with_overlay()` + `infra::load_embedded_adopted_offerings()` before `AppState` is constructed. Never mutated afterward (0 mutation sites in the moss crate after bootstrap). Read-only through 19 `state.manifest_registry.*` access sites across 20 files.

2. **`AppState::offerings_index: Arc<RwLock<Option<OfferingsIndex>>>`** — the compiled catalog snapshot (per-offering compatibility evaluation, image resolution, port/volume/env resolution, coordination mode). Starts `None`. Lazily populated by the `catalog-builder` background task via `ensure_offerings_index()`. Rebuilt later in `hardware-detection` after the full capabilities snapshot is available. 20 `state.offerings_index.read()/write()` sites across 13 files (API handlers, placement, orchestration tasks, state provider, ceremony nourishment).

3. **`src/moss/src/domain/offerings/catalog.rs`** — 367 lines of free functions (`ensure_offerings_index`, `get_compiled_offering`, `rebuild_offerings_index`, `moss_version_string`, `manifests_hash`, `current_capabilities_hash`) plus the `CompiledOffering`, `OfferingsFingerprint`, and `OfferingsIndex` type definitions. The file is *misfiled* inside `domain/offerings/` — it has nothing to do with the runtime `Offerings` aggregate (live deployment state); it holds the compile-time catalog that Offerings *consumes*. Code-standards §14 violation: "catalog" is its own concept and deserves its own module.

The `infra::persistence::OsOfferingsCache` adapter already implements `OfferingsCachePersistence` (defined in `domain/traits/offerings_cache.rs`) — a pre-existing port for disk persistence of the compiled index. Book V inherits this port and makes it canonical as the Catalog aggregate's `CatalogCache` port, relocated into `domain/catalog/cache.rs`.

Per the Discovery Mandate, Chapter 1 re-evaluates the plan against the current code. The findings reshape Book V from a textbook "consolidate two raw fields" into a more nuanced extraction that preserves the cross-crate `ManifestRegistry` boundary, keeps the compiled-type names stable to minimize cascading renames, and explicitly documents the dual-rebuild invariant that both the early catalog-builder and the late hardware-detection paths depend on.

### What the re-evaluation found

1. **`ManifestRegistry` is a cross-crate type, not moss-owned.** It lives in [`garden_common::manifests::registry`](../../src/common/src/manifests/registry.rs) and is used by rake, orchestrators, and peer stones. Book V **cannot rename or restructure this type**. The aggregate holds it as an immutable `Arc<ManifestRegistry>` — a frozen input rather than mutable state. `ManifestRegistry::empty()`, `load()`, `from_sw_manifests()`, `get_offering()`, `upsert_offering()` stay where they are. Book V only removes the raw `state.manifest_registry.*` access pattern from the moss crate, not the type itself.

2. **`ManifestRegistry` is effectively immutable after bootstrap.** A `rg 'state\.manifest_registry\.(get_offering_mut|upsert|sw\.get_mut)'` search returns zero hits in the moss crate. The registry is built once in `bootstrap::run::build_state()` (from embedded assets + filesystem overlay + embedded adopted offerings) and read-only thereafter. The aggregate can safely hold it as `Arc<ManifestRegistry>` without an internal lock — there's no write path to guard.

3. **19 `state.manifest_registry.*` access sites across 20 files** — all reads. Dominant patterns:
   - `state.manifest_registry.sw.get(name)` — 14 sites — "is this offering in our catalog?"
   - `state.manifest_registry.get_offering(name)` — 5 sites — wrapper that's equivalent
   - `state.manifest_registry.hw.find_matching(...)` — 1 site — hardware manifest lookup
   - `state.manifest_registry.sw.entries.len()` — 2 sites — manifest count for logging
   All of these become typed query methods on the aggregate: `catalog.get_manifest(name)`, `catalog.find_hw_manifest(...)`, `catalog.manifest_count()`.

4. **20 `state.offerings_index.read()/write()` sites across 13 files** — 13 read-only, 3 via `domain/offerings/catalog.rs` write paths, plus 4 inline reads inside the catalog.rs file itself. Call-site patterns all follow "read lock, match as_ref(), iterate offerings, find by name, clone":
   ```rust
   let idx_guard = state.offerings_index.read().await;
   let offerings_index = idx_guard.as_ref().ok_or_else(|| ...)?;
   if let Some(offering) = offerings_index.offerings.iter().find(|o| o.name == name) { ... }
   ```
   Every site collapses into one typed query call: `state.catalog.get_compiled(name).await?`. The strangler-migration fanout is the same shape as Book II Tool's 14 API/domain/task read sites.

5. **25 free-function caller sites** for `ensure_offerings_index`, `rebuild_offerings_index`, `get_compiled_offering`. These currently act as the coordination layer between `manifest_registry`, `offerings_index`, the disk cache, and capability re-evaluation. The aggregate absorbs all of this into typed commands (`load`, `rebuild`, `get_compiled`) that return `Result<T, CatalogError>`.

6. **Catalog is a persistent aggregate** (like Offerings and Topology). It has a `CatalogCache` port (the existing `OfferingsCachePersistence` trait, relocated into the aggregate's module) with a `FileCatalogCache` adapter (the existing `OsOfferingsCache`, relocated in-place or renamed for clarity). The compiled index is written to disk on every successful rebuild so subsequent startups can short-circuit manifest re-parsing. Book V is the **third persistent aggregate** in the epic.

7. **The compiled-index types are public across the moss crate.** `CompiledOffering` leaks into 8 files (placement.rs, api/v1/offerings.rs, api/v1/updates.rs, services_internal.rs, service_lifecycle.rs, offering_resolution.rs, ceremony/phases/nourish.rs, job_executors.rs). `OfferingsFingerprint` and `OfferingsIndex` also appear in the trait file and module re-exports. Book V **keeps these names stable** — renaming `CompiledOffering → CatalogEntry` or similar would cascade through 8 files with no architectural benefit. The types move to `domain/catalog/entry.rs` (definitions) and `domain/catalog/index.rs` (the `OfferingsIndex` container + `OfferingsFingerprint`), with re-exports from `domain/catalog/mod.rs` and `domain/mod.rs` preserving the old import paths.

8. **Dual-rebuild invariant must be preserved.** Boot order today:
   - `catalog-builder` task (depends on `registry-loader`) calls `ensure_offerings_index(force=false)` — builds the compiled index with **zero or partial capabilities** (GPU detection takes 2-6 seconds on Windows, so the first rebuild happens with whatever is in `current.capabilities` at catalog-builder time, typically `None`).
   - `hardware-detection` task (parallel, no deps) runs through its phases (CPU → GPU → system), then calls `ensure_offerings_index(force=true)` — forces a **second rebuild** with the complete capabilities snapshot to refresh compatibility decisions (e.g., "no GPU → ollama incompatible" transitions to "CUDA detected → ollama compatible").

   The aggregate command surface must preserve both entry points: `Catalog::load()` (idempotent, skips if cache fresh or memory populated) for the catalog-builder path, and `Catalog::rebuild()` (force-rebuild with latest capabilities, ignoring current state) for the hardware-detection path. Both must tolerate being called before capabilities are ready (first call) and after (second call).

9. **`ensure_offerings_index(force=false/true)` decomposes into two typed commands.** The `force` bool is a smell — it conflates "load from cache if possible" with "rebuild unconditionally". Book V splits:
   - `Catalog::load() -> Result<(), CatalogError>` — load from disk cache if fingerprint matches, otherwise rebuild. Idempotent: no-op if memory is already populated.
   - `Catalog::rebuild() -> Result<(), CatalogError>` — force a rebuild from current `ManifestRegistry` + current capabilities, bypassing disk cache. Persists the new index back to cache on success. Used by `hardware-detection` after capabilities change.

   `get_compiled_offering` becomes a pure query `Catalog::get_compiled(name) -> Option<CompiledOffering>` that assumes the catalog is loaded — returns `None` both for "catalog not loaded yet" and "offering not in catalog". Callers that need "load-then-get" call the two in sequence; this matches the existing code pattern where most callers already assume the catalog is built.

10. **The catalog's rebuild is fallible.** `manifests_hash` can fail on template parse errors; `compile_compatibility` can propagate parse errors for malformed compatibility rules. Book V introduces a typed `CatalogError` enum:
    ```rust
    pub enum CatalogError {
        ManifestHashFailed(anyhow::Error),
        CompilationFailed { offering: String, source: anyhow::Error },
        CacheReadFailed(anyhow::Error),
        CacheWriteFailed(anyhow::Error),
    }
    ```
    Commands return `Result<T, CatalogError>`. This is the **first domain aggregate with a typed error enum** in the epic — Metrics, Tool, Topology, and Jobs are all infallible or propagate `anyhow::Result` through port boundaries. ARCH-0022 elevates typed errors to a first-class shape (code-standards §10 says to do this; prior books ducked by being infallible or by not having meaningful error shapes). The pattern spec gains a "Typed errors" section in Ch6.

11. **`catalog.rs` also defines hash/fingerprint helpers.** `moss_version_string()`, `current_capabilities_hash()`, `manifests_hash()` are pub free functions currently. All three are implementation details of the aggregate's `rebuild` command and have zero external callers (verified by grep). Book V makes them `pub(super)` inside `domain/catalog/fingerprint.rs` — the fingerprint module of the aggregate.

12. **Port lives with the aggregate.** The existing `OfferingsCachePersistence` trait lives in `domain/traits/offerings_cache.rs` — a cross-aggregate "traits" bucket that code-standards §14 discourages. Book V moves it into `domain/catalog/cache.rs` alongside the aggregate, matching the Tool/Topology convention (port-in-domain, adapter-in-infra). The trait is renamed to `CatalogCache` to match the aggregate. `OsOfferingsCache` (in `infra::persistence`) implements the new trait; the old `OfferingsCachePersistence` name is deleted. The `domain/traits/` directory shrinks or disappears — if `offerings_cache.rs` was its only content, the directory dies; otherwise other entries stay for a later book to clean up.

13. **The `catalog_builder` task gets a surface change.** Currently calls `crate::domain::ensure_offerings_index(state, false, &OsOfferingsCache)` and logs the offerings count by reading `state.offerings_index`. After migration, it calls `state.catalog.load().await?` and reads `state.catalog.stats().await` (or equivalent typed query) for the logging payload.

14. **`hardware_detection::detect_capabilities_background` ends with a catalog rebuild.** `ensure_offerings_index(&state, true, &OsOfferingsCache).await` at the end of the hardware detection chain. Book V changes this to `state.catalog.rebuild().await?`. The explicit dependency on `&OsOfferingsCache` goes away — the aggregate holds its injected port internally.

15. **Events are minimal.** The catalog is mostly inert — it loads once, rebuilds twice per process start, and is read from thereafter. `CatalogChanged` fires on `Loaded` (first time state transitions from None to Some) and `Rebuilt` (when a command-triggered rebuild swaps the compiled index). No per-offering events; the granularity is the whole catalog. Two kinds is enough: `Loaded`, `Rebuilt`. `CatalogError` results from commands are NOT events — they propagate as `Result::Err` and are logged at the call site, matching the "errors are not events" pattern in code-standards §10.

16. **`registry-loader` task is misnamed.** It has nothing to do with loading the manifest registry (which is synchronous in `build_state()` before any task starts). It reconciles the `Offerings` aggregate against live Docker container state. Ch6 of Book V notes this as a **deferred rename** tracked in `docs/scaffolding.md` (new entry: `deferred-registry-loader-task-rename` — target name something like `offerings-reconciler` or `offerings-bootstrap-sync`). The rename is deferred because `registry-loader` is referenced as a dependency by `catalog-builder`, `initial-service-sync`, and two others; touching it cascades into the supervisor dependency graph and the task name wire format (exposed via `/api/v1/stone/tasks`). Out of scope for Book V.

17. **Blast radius.** `rg -l 'manifest_registry|offerings_index|ensure_offerings_index|get_compiled_offering|rebuild_offerings_index|CompiledOffering|OfferingsFingerprint|OfferingsIndex|OfferingsCachePersistence' src/moss/src/` returns **40 files**. Book V migrates every one of them through `state.catalog.*` typed calls. Not all need content changes — some are just imports that follow the new `use crate::domain::catalog::*` path.

## Decision

Book V extracts `Catalog` as a full DDD aggregate with private state (the `Arc<ManifestRegistry>` as an immutable frozen input, plus an `RwLock<Option<OfferingsIndex>>` for the compiled snapshot), typed commands (`load`, `rebuild`), typed queries (`get_manifest`, `get_compiled`, `manifest_count`, `compiled_snapshot`, `stats`, …), a `CatalogChanged` event stream with two kinds (`Loaded`, `Rebuilt`), `Arc<Metrics>` injection, a `CatalogCache` persistence port, and the first typed `CatalogError` enum in the epic. The raw `AppState::manifest_registry` and `AppState::offerings_index` fields are deleted and replaced with a single `AppState::catalog: Arc<Catalog>`. The existing `domain/offerings/catalog.rs` file is dissolved — its 367 lines split across `domain/catalog/` submodules. The `OfferingsCachePersistence` trait is renamed to `CatalogCache` and relocated from `domain/traits/` to `domain/catalog/cache.rs`.

### Module layout (target state)

```
src/moss/src/domain/catalog/
├── mod.rs            — re-exports Catalog, CompiledOffering, OfferingsIndex, OfferingsFingerprint, CatalogChanged, CatalogError, CatalogCache, FileCatalogCache
├── aggregate.rs      — `Catalog` struct, typed commands, typed queries, changes()
├── state.rs          — `CatalogState` (holds Option<OfferingsIndex>) — thin wrapper for symmetry with other books
├── entry.rs          — `CompiledOffering` (moved from offerings/catalog.rs)
├── index.rs          — `OfferingsIndex` + `OfferingsFingerprint` (moved)
├── fingerprint.rs    — `moss_version_string`, `current_capabilities_hash`, `manifests_hash` (pub(super))
├── event.rs          — `CatalogChanged` enum + `ChangeKind` (2 kinds: Loaded, Rebuilt)
├── error.rs          — `CatalogError` enum
├── cache.rs          — `CatalogCache` port + `FileCatalogCache` adapter (the existing `OsOfferingsCache`, relocated)
└── tests.rs          — unit tests
```

The existing `src/moss/src/domain/offerings/catalog.rs` is **deleted**. The existing `src/moss/src/domain/traits/offerings_cache.rs` is **deleted** (its content moves into `domain/catalog/cache.rs`). If `domain/traits/` becomes empty after Book V, the directory is also deleted; otherwise the remaining entries stay for a later book.

### Aggregate API

```rust
pub struct Catalog {
    /// Frozen source-of-truth manifest registry. Loaded in `bootstrap::build_state`
    /// before the aggregate is constructed. No internal lock — immutable.
    manifests: Arc<ManifestRegistry>,

    /// Compiled catalog snapshot. Starts `None`; populated by `load` or
    /// `rebuild`. Interior mutability via `RwLock` so commands can swap
    /// the snapshot without rebuilding the aggregate struct.
    state: RwLock<CatalogState>,

    /// Hardware capabilities snapshot source (shared with `current::Resources`).
    /// The aggregate does not own capabilities — it reads them via this handle
    /// at rebuild time. Book V does not change `current.capabilities`; it only
    /// references it.
    capabilities: Arc<RwLock<Option<HardwareCapabilities>>>,

    /// Injected persistence port.
    cache: Arc<dyn CatalogCache>,

    /// Metrics aggregate.
    metrics: Arc<Metrics>,

    /// Internal domain event broadcast.
    changes: broadcast::Sender<CatalogChanged>,
}

impl Catalog {
    pub const NAME: &'static str = "catalog";

    pub async fn new(
        manifests: Arc<ManifestRegistry>,
        capabilities: Arc<RwLock<Option<HardwareCapabilities>>>,
        cache: Arc<dyn CatalogCache>,
        metrics: Arc<Metrics>,
    ) -> Self {
        metrics.register_domain(Self::NAME, ChangeKind::ALL_NAMES).await;
        let (changes, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            manifests,
            state: RwLock::new(CatalogState::empty()),
            capabilities,
            cache,
            metrics,
            changes,
        }
    }

    // ── Commands ────────────────────────────────────────────────────────

    /// Load the catalog from disk cache if fingerprint matches, else
    /// rebuild and persist. Idempotent: no-op if memory is already
    /// populated. Called by the `catalog-builder` task at startup.
    pub async fn load(&self) -> Result<(), CatalogError>;

    /// Force a rebuild from current manifests + current capabilities,
    /// bypassing disk cache for the read path. Persists the new index
    /// on success. Called by `hardware-detection` after capabilities
    /// become available.
    pub async fn rebuild(&self) -> Result<(), CatalogError>;

    // ── Queries ─────────────────────────────────────────────────────────

    /// Return the manifest entry for `name`, or `None` if unknown.
    /// Owned-value query (clones the Offering). Replaces the 14
    /// `state.manifest_registry.sw.get(name)` sites.
    pub fn get_manifest(&self, name: &str) -> Option<Offering>;

    /// Hardware manifest lookup (delegates to ManifestRegistry::hw::find_matching).
    pub fn find_hw_manifest(&self, query: &HwQuery) -> Option<HwEntry>;

    /// Total manifest count — useful for logging.
    pub fn manifest_count(&self) -> usize;

    /// Compiled offering by name. Returns `None` for either "catalog not
    /// loaded" or "unknown offering". Clones the `CompiledOffering`.
    pub async fn get_compiled(&self, name: &str) -> Option<CompiledOffering>;

    /// Full compiled snapshot. Clones the whole vector (at most ~100 items).
    pub async fn compiled_snapshot(&self) -> Option<Vec<CompiledOffering>>;

    /// Summary stats: `(manifest_count, compiled_count, fingerprint)`.
    pub async fn stats(&self) -> CatalogStats;

    /// Whether the compiled snapshot has been loaded yet.
    pub async fn is_loaded(&self) -> bool;

    // ── Events ──────────────────────────────────────────────────────────

    pub fn changes(&self) -> broadcast::Receiver<CatalogChanged>;
}
```

### `CatalogChanged` event

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum CatalogChanged {
    /// First successful load (from cache or fresh rebuild).
    Loaded {
        compiled_count: usize,
        fingerprint: OfferingsFingerprint,
        source: LoadSource, // DiskCache | FreshRebuild
    },
    /// Force-rebuild completed. Fingerprint may match (no-op transition)
    /// or differ (actual swap).
    Rebuilt {
        compiled_count: usize,
        fingerprint: OfferingsFingerprint,
        fingerprint_changed: bool,
    },
}

pub enum ChangeKind { Loaded, Rebuilt }
```

### `CatalogError` (typed errors — first in the epic)

```rust
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("failed to hash manifests for fingerprint")]
    ManifestHashFailed(#[source] anyhow::Error),

    #[error("failed to compile offering {offering}")]
    CompilationFailed {
        offering: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("failed to read catalog cache from disk")]
    CacheReadFailed(#[source] anyhow::Error),

    #[error("failed to write catalog cache to disk")]
    CacheWriteFailed(#[source] anyhow::Error),
}
```

Commands return `Result<(), CatalogError>`. API handlers map `CatalogError` to `5xx` responses with structured error codes (out of scope for Book V — the current handlers just log-and-propagate through `anyhow`).

### `CatalogCache` port

```rust
pub trait CatalogCache: Send + Sync {
    fn load<'a>(&'a self) -> BoxFut<'a, Result<Option<OfferingsIndex>, CatalogError>>;
    fn save<'a>(&'a self, index: &'a OfferingsIndex) -> BoxFut<'a, Result<(), CatalogError>>;
}

// Production adapter (relocated from `infra::persistence::OsOfferingsCache`).
pub struct FileCatalogCache;

impl CatalogCache for FileCatalogCache {
    // Delegates to `load_offerings_cache` / `save_offerings_cache` in infra
    // for the existing disk layout (`{config_dir}/offerings.cache.json`).
}
```

### Metrics integration

Register domain `catalog` with two kinds (`loaded`, `rebuilt`). Every command records mutation latency via `record_mutation_latency`; every successful command emits a `CatalogChanged` event and records `record_domain_event` per kind. Failed commands record latency but not per-kind events — errors propagate through `Result::Err` and are not part of the event stream (see code-standards §10).

### What Book V does not do

- **No `ManifestRegistry` rewrite.** The type is cross-crate (used by rake, orchestrators, peer stones). Book V holds it as an immutable input and provides typed wrappers, but does not touch `garden_common::manifests::*`.
- **No `CompiledOffering` / `OfferingsFingerprint` / `OfferingsIndex` rename.** These types leak into 8 files across the crate and serving the rename cascade buys nothing architectural. The definitions move to `domain/catalog/entry.rs` and `domain/catalog/index.rs` with the names unchanged.
- **No `registry-loader` task rename.** The task is misnamed but touching it cascades into the supervisor dependency graph (`catalog-builder`, `initial-service-sync`, and two others depend on it) and the task-name wire format (`/api/v1/stone/tasks`). Deferred to `docs/scaffolding.md` under a new `deferred-registry-loader-task-rename` entry.
- **No API error restructuring.** Current handlers log-and-propagate through `anyhow` for catalog failures. Book V returns `CatalogError` from the aggregate, but handlers continue to wrap it in `anyhow::Error` for the current `5xx` path. Book XVII (HTTP API thin layer) can revisit.
- **No multi-writer support.** The catalog has exactly one writer at a time today — `load` runs from catalog-builder, `rebuild` runs from hardware-detection (and a few API handlers that force rebuild). Book V keeps the single `RwLock<CatalogState>` pattern; there's no concurrency story to invent.
- **No catalog as first-class HTTP surface.** There is no `/api/v1/stone/catalog` endpoint today, and Book V does not add one. `/api/v1/offerings?state=available` is the closest wire surface, and it stays as-is — the handler just switches to `state.catalog.compiled_snapshot().await` internally.

## Chapter plan

| Ch | Scope |
|----|-------|
| 1  | ADR (this), revision history entry in ARCH-0017 |
| 2  | Module skeleton creation + pure moves — `domain/catalog/` directory with empty submodule files, then move `CompiledOffering` / `OfferingsIndex` / `OfferingsFingerprint` type definitions from `domain/offerings/catalog.rs` into `domain/catalog/{entry,index}.rs`, move `moss_version_string` / `manifests_hash` / `current_capabilities_hash` into `domain/catalog/fingerprint.rs`, move the `OfferingsCachePersistence` trait from `domain/traits/offerings_cache.rs` into `domain/catalog/cache.rs` (renaming to `CatalogCache`), relocate `OsOfferingsCache` → `FileCatalogCache` alongside or re-export. Every call site that imports these types gets its import path rewritten in the same commit. `AppState::offerings_index` stays as the raw strangler field during this chapter; `AppState::manifest_registry` likewise. No aggregate yet. |
| 3  | `Catalog` aggregate skeleton: state, typed commands (`load`, `rebuild`), typed queries (`get_manifest`, `find_hw_manifest`, `manifest_count`, `get_compiled`, `compiled_snapshot`, `stats`, `is_loaded`), `CatalogChanged` event with 2 kinds, `CatalogError` typed enum, `Arc<Metrics>` injection, ~20 unit tests. Aggregate is constructed in `bootstrap::build_state()` alongside the two legacy strangler fields `AppState::manifest_registry` and `AppState::offerings_index`. `FromRef<AppState> for Arc<Catalog>` impl added. |
| 4  | Migrate the 19 `state.manifest_registry.*` sites to `state.catalog.get_manifest(name)` / `catalog.find_hw_manifest(...)` / `catalog.manifest_count()`. Migrate the 20 `state.offerings_index.read()` sites to `state.catalog.get_compiled(name)` / `catalog.compiled_snapshot()`. Migrate the 25 free-function caller sites (`ensure_offerings_index`, `get_compiled_offering`, `rebuild_offerings_index`) to the aggregate's typed commands. `catalog-builder` task calls `state.catalog.load().await?`; `hardware_detection::detect_capabilities_background` calls `state.catalog.rebuild().await?`. |
| 5  | Delete legacy strangler fields: `AppState::manifest_registry`, `AppState::offerings_index`. Delete `domain/offerings/catalog.rs` entirely. Delete `domain/traits/offerings_cache.rs`. Delete the corresponding `FromRef<AppState> for Arc<ManifestRegistry>` impl. Delete the `pub use` re-exports in `domain/mod.rs`, `domain/offerings/mod.rs`, and `lib.rs` for `ensure_offerings_index` / `get_compiled_offering` / `rebuild_offerings_index` / `moss_version_string` / `manifests_hash`. Prune `domain/traits/` directory if it becomes empty. |
| 6  | Closure: context-map update (move Catalog from Absent to Full, target-state table row ✅ COMPLETE), glossary additions for Book V terms (Catalog aggregate, dual-rebuild invariant, frozen input, typed errors), pattern-spec addition for "Typed errors" as the fourth deviation, ARCH-0022 frontmatter `completed: <date>`, ARCH-0017 revision history entry, `docs/scaffolding.md` gains the `deferred-registry-loader-task-rename` entry. Final exit-criteria grep. |

## Exit criteria

Book V is closed when:

1. `rg 'state\.manifest_registry|state\.offerings_index' src/moss/src/` returns 0 matches
2. `rg 'pub manifest_registry:|pub offerings_index:' src/moss/src/app_state.rs` returns 0 matches (fields deleted)
3. `src/moss/src/domain/offerings/catalog.rs` does not exist (file deleted)
4. `src/moss/src/domain/traits/offerings_cache.rs` does not exist (file deleted — trait moved to `domain/catalog/cache.rs`)
5. `rg 'ensure_offerings_index|rebuild_offerings_index|OfferingsCachePersistence' src/moss/src/` returns 0 matches (free functions and old trait name deleted)
6. `src/moss/src/domain/catalog/` contains at minimum `mod.rs`, `aggregate.rs`, `state.rs`, `entry.rs`, `index.rs`, `fingerprint.rs`, `event.rs`, `error.rs`, `cache.rs`, `tests.rs`
7. `cargo check --all && cargo test --package garden-moss --lib && cargo clippy --package garden-moss --lib -- -D warnings` all green
8. Manual smoke on a live stone: (a) cold start — `catalog-builder` task loads the catalog, `/api/v1/offerings?state=available` returns the full compiled list within seconds; (b) hardware-detection rebuild — after GPU detection, a second `CatalogChanged::Rebuilt` event fires and the ollama compatibility decision flips; (c) warm start — second startup reads from the disk cache (no re-compile) and `/api/v1/stone/metrics/domains/catalog` shows `loaded` counter = 1, `rebuilt` counter = 0 or 1.

## Pattern deviations

Book V introduces one new deviation and reuses two existing ones:

- **NEW: Typed errors.** First domain aggregate with a typed `CatalogError` enum. Commands return `Result<T, CatalogError>` instead of `anyhow::Result<T>`. Matches code-standards §10 "Domain errors as enums". The pattern spec gains a "Typed errors" section in Ch6 alongside the existing Ephemeral / Dual event streams / Owned queries deviations. Rationale: Catalog is the first aggregate where mutations have structured failure modes worth propagating (disk read/write, per-offering compile errors) — Metrics / Tool / Topology / Jobs are either infallible or propagate `anyhow::Result` at port boundaries. The deviation is "typed errors are now the preferred shape when commands can meaningfully fail; use `anyhow::Result` only at adapter boundaries where the underlying error is already unstructured (Bollard, tokio::fs, etc.)".

- **REUSED: Persistent aggregate (Store port).** Catalog is the third persistent aggregate after Offerings (ARCH-0016) and Topology (ARCH-0020). The `CatalogCache` port follows the same shape as `OfferingStore` / `TopologyStore` — `load` on construction (or on first `load()` call), `save` after every successful `rebuild`.

- **REUSED: Frozen input fields.** The `Arc<ManifestRegistry>` field is immutable — no interior lock, no mutation commands, no dirty flag. This is the first aggregate to explicitly hold a cross-crate frozen input as a struct field. The pattern is: "a frozen input is part of the aggregate's state *shape* (you can query it through typed methods) but not part of its *identity* (you can't mutate it, persist it, or subscribe to its changes)". Documented in-line in the Ch6 glossary but not elevated to a spec deviation — it's a natural consequence of cross-crate types.

## Alternatives considered

### Alternative A — Split Catalog into ManifestRegistry and CompiledCatalog (rejected)

Option A would extract two aggregates: `ManifestRegistry` (already a type in common) wrapping the frozen source-of-truth, and `CompiledCatalog` wrapping the compiled snapshot. Rejected: the two are inseparable in practice — every call site that reads a manifest also reads (or soon will read) the compiled view, and the compiled view is derived from the manifests via a pure function. Splitting forces every call site to thread two dependencies where one suffices. The tenet says "SoC trumps DRY" — but here SoC is not violated because the two "things" are one bounded context (compile-time catalog state). Book V keeps them together.

### Alternative B — Rename `CompiledOffering` to `CatalogEntry` (rejected)

Option B would rename the compiled-type to match its new home (`domain/catalog/entry.rs` would hold a `CatalogEntry` struct). Rejected: the type appears in 8 non-module files (placement.rs, api/v1/offerings.rs, api/v1/updates.rs, services_internal.rs, service_lifecycle.rs, offering_resolution.rs, ceremony/phases/nourish.rs, job_executors.rs). The rename has no architectural benefit — `CompiledOffering` is a clear name that says what it is — and cascades through ~50 import sites for zero value. Book V keeps the name.

### Alternative C — Add persistence for `ManifestRegistry` (rejected)

Option C would give `ManifestRegistry` a `Store` port so the catalog survives a change to the embedded assets (e.g., moss binary upgrade) by comparing hashes. Rejected: the `ManifestRegistry` is *built from* embedded assets + filesystem overlay at every startup — there is no "previous state" to persist and compare. The `OfferingsIndex` (compiled snapshot) already persists and uses a fingerprint that includes `manifests_hash()`, so a moss upgrade automatically invalidates the cache. Adding a second persistence layer for the source-of-truth would be redundant.

### Alternative D — Collapse `load` and `rebuild` into one command (rejected)

Option D would expose a single `refresh(force: bool)` command matching the current `ensure_offerings_index(force)` signature. Rejected for the reasons outlined in Finding 9 — the `force` bool is a smell that conflates two distinct intents (idempotent-load-from-cache vs force-fresh-rebuild). Splitting into `load` and `rebuild` makes the call sites self-documenting and lets the aggregate optimize each path independently (e.g., `load` short-circuits on in-memory state without touching the disk cache at all).

### Alternative E — Make the catalog ephemeral (no persistence) (rejected)

Option E would drop the disk cache and rebuild from scratch on every startup. Rejected: the rebuild is the slow path — multi-second on stones with dozens of manifests — and the cache is already the existing invariant. Dropping it would regress cold-start latency for no architectural benefit. The pattern deviation is "ephemeral" for Metrics/Tool/Jobs/Resources because those aggregates have no meaningful state to persist; Catalog has meaningful state (the compiled fingerprint + per-offering compatibility decisions), so it is persistent.

## References

- [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) — the epic
- [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md) — Offerings aggregate; first persistent aggregate with a `Store` port (Catalog is the third)
- [ARCH-0018](ARCH-0018-metrics-aggregate.md) — Metrics aggregate; `Arc<Metrics>` injection precedent and register-with-kinds pattern
- [ARCH-0019](ARCH-0019-tool-aggregate.md) — Tool aggregate; owned-value queries precedent
- [ARCH-0020](ARCH-0020-topology-aggregate.md) — Topology aggregate; second persistent aggregate with a `Store` port
- [ARCH-0021](ARCH-0021-jobs-aggregate.md) — Jobs aggregate; infallible-mutations precedent (Catalog takes the opposite path with typed errors)
- [docs/specs/domain-aggregates.md](../specs/domain-aggregates.md) — pattern spec; Book V adds a new "Typed errors" deviation in Ch6
- [docs/code-standards.md §10](../code-standards.md) — "Domain errors as enums" — the rule Book V is the first aggregate to follow
- [docs/scaffolding.md](../scaffolding.md) — Deferred renames section; new entry for `deferred-registry-loader-task-rename` lands in Ch6
