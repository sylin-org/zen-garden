---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-21
---

# ARCH-0007: Rust 1.92 Modernization — Monomorphic Traits, Edition 2024, Tooling

**Date**: 2026-03-21
**Status**: Accepted
**Depends on**: ARCH-0004 (domain context extraction), ARCH-0005 (structural quality pass)

## Context

The MSRV bump from 1.75 to 1.92 and the removal of the `async-trait` crate
(120 annotations across 94 files) left the codebase in a transitional state:
native `async fn` in non-dyn traits, but manual `Pin<Box<dyn Future>>` desugaring
on all 13 domain traits that were historically stored as `Arc<dyn Trait>`.

An audit identified multiple modernization opportunities beyond the trait system:
edition 2021 → 2024, workspace hygiene, lint enforcement, dependency drift, and
testing infrastructure gaps. This ADR covers the full scope.

## Decision

A five-phase modernization bringing the codebase to bleeding-edge stable Rust.
Each phase is independently shippable and produces a compilable, testable state.

---

## Phase A: Workspace Hygiene

**Goal**: Single source of truth for versions, editions, lints, and vulnerability policy.

### A1. Workspace dependency inheritance

Convert all workspace members to `dep.workspace = true` for shared dependencies.
Currently `common`, `moss`, `rake`, and `lantern` hardcode versions locally while
`companion-sdk`, `cricket`, and `firefly` correctly use workspace inheritance.

**Before** (duplicated across 4 crates):
```toml
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
chrono = "0.4"
```

**After** (in each member):
```toml
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
anyhow.workspace = true
chrono.workspace = true
```

Files: `src/common/Cargo.toml`, `src/moss/Cargo.toml`, `src/rake/Cargo.toml`,
`src/lantern/Cargo.toml`, root `Cargo.toml` (add missing workspace deps).

### A2. Edition and rust-version inheritance

Convert all workspace members to `edition.workspace = true` and
`rust-version.workspace = true`. Currently only companion-sdk, cricket, and
firefly use inheritance. The other 5 members hardcode both values.

Files: `src/common/Cargo.toml`, `src/moss/Cargo.toml`, `src/rake/Cargo.toml`,
`src/lantern/Cargo.toml`, `src/build-utils/Cargo.toml`, `src/probe/Cargo.toml`.

### A3. Workspace lint configuration

Add `[workspace.lints]` to the root `Cargo.toml` encoding the project's code
standards as compiler-enforced policy:

```toml
[workspace.lints.clippy]
unwrap_used = "warn"              # code standards §17
todo = "warn"                     # flag incomplete work
missing_safety_doc = "deny"       # code standards §16
manual_async_fn = "warn"          # prefer native async fn
type_complexity = "allow"         # needed for Pin<Box<dyn Future>> (until ARCH-0007 completes)
```

Each member crate adds:
```toml
[lints]
workspace = true
```

### A4. Vulnerability scanning with `cargo-deny`

Create `deny.toml` at workspace root:
- License audit (allow MIT, Apache-2.0, BSD variants; deny GPL in deps)
- Vulnerability advisory database check
- Duplicate crate detection (flag when two versions of the same crate exist)

---

## Phase B: Dependency Modernization

**Goal**: Current deps, no unmaintained crates, no duplicates.

### B1. Remove `once_cell` → `std::sync::LazyLock`

Replace all 6 sites across zen-garden and koi with `std::sync::LazyLock`
(stable since 1.80). Remove `once_cell` from `Cargo.toml` in `common`, `rake`,
and koi workspace.

Sites:
- `src/common/src/client/discovery.rs` — `STONE` singleton
- `src/rake/src/discovery.rs` — `LANTERN_CACHE` (2 statics)
- `src/rake/src/command_manifest.rs` — `MANIFEST` static
- koi `crates/koi/src/surface.rs` — `MANIFEST` static

### B2. `rand` 0.8 → 0.9

API changes: `thread_rng()` → `rng()`, minor trait renames. ~20 call sites
across zen-garden + koi.

### B3. `base64` 0.21 → 0.22 in moss

Rake and koi already use 0.22. Align moss. Engine API changed slightly.

### B4. `notify` 6 → 7 in cricket

Moss already uses notify 7. Cricket uses 6. Unify.

### B5. Network interface consolidation

Replace `local-ip-address` + `network-interface` with `if-addrs` (used by koi,
actively maintained, simpler API). Affects common, moss, rake.

### B6. `serde_yaml` 0.9 → `serde_yml`

`serde_yaml` is unmaintained. `serde_yml` is a maintained fork with identical
API. 30 call sites — drop-in replacement, verify with tests.

---

## Phase C: Edition 2024

**Goal**: Adopt the latest stable Rust edition.

### C1. Flip `edition = "2024"` in workspace

Since A2 established `edition.workspace = true`, only one file changes.

### C2. Fix `unsafe_op_in_unsafe_fn` warnings

Edition 2024 makes this a default warning. All `unsafe` blocks inside `unsafe fn`
bodies must be explicitly marked. Primary sites:
- `src/moss/src/infra/storage/platform.rs` (Windows FFI, ~10 blocks)
- `src/moss/src/infra/cloud_filter/` (Windows Cloud Files API)
- `src/common/src/infra/platform.rs` (signal handling)
- `src/cricket/src/mixer.rs` (unsafe Send/Sync)

Each block already has `// SAFETY:` comments (added in this session's review
fixes), so the fix is purely syntactic: wrap inner operations in `unsafe {}`.

### C3. Verify lifetime capture changes

Edition 2024 changes `impl Trait` lifetime capture rules (RFC 3498). The 11
trait methods using `-> impl Future + Send` in common and companion-sdk may
need `+ use<'_>` annotations if the new capture semantics cause borrow-check
errors. Likely benign since all methods borrow from `&self`.

---

## Phase D: Monomorphic Domain Traits

**Goal**: Zero-cost trait dispatch for single-implementation domain traits.

### Inventory

13 domain traits, all with exactly one production implementation:

| Trait | Sole Impl | Async Methods | dyn mandatory? |
|---|---|---|---|
| PondClient | StoneClient | 0 (sync) | No |
| TaskRegistryPersistence | TaskStore | 2 | No |
| OfferingsCachePersistence | OsOfferingsCache | 2 | No |
| ServiceDetector | ContainerDetector | 1 | No |
| HarvestOps | OsHarvestOps | 2 | No |
| CeremonyPersistence | CeremonyJournal | 5 | No |
| DockerConfigOps | OsDockerConfig | 5 | No |
| StoragePlatform | OsPlatform | 3 async + 11 sync | No |
| ManagementStoreOps | ContentStore | 4 | No (factory) |
| ContentStoreOps | ContentStore | 6 | No |
| NurturingStoreOps | NurturingStore | 8 | No |
| ServiceRuntime | ContainerRuntime | 9 | No |
| CompanionOps | CompanionRegistry | 13 | No |

Traits that **stay `dyn`** (genuine runtime polymorphism):
- PlatformRuntime (2 impls, cfg-selected)
- NetworkPlatform (3 impls, runtime-probed)
- SecretBackend (3 impls, runtime cascade)
- VolumeMonitor (3 impls, platform-selected)
- StateProvider (2 impls, boot-time swap)
- InfrastructureHandler (extensible registry)
- Command (60+ impls, CLI dispatch)
- JobExecutor (extensible executor registry)

### Migration pattern

**Trait definitions** — from `Pin<Box<dyn Future>>` to `-> impl Future + Send`:

```rust
// Before (dyn-compatible, heap-allocated)
pub trait CeremonyPersistence: Send + Sync {
    fn persist<'a>(&'a self, record: &'a CeremonyRecord)
        -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

// After (native, zero-cost)
pub trait CeremonyPersistence: Send + Sync {
    fn persist(&self, record: &CeremonyRecord)
        -> impl Future<Output = Result<()>> + Send;
}
```

**Domain structs** — generic with default type parameter:

```rust
pub struct Ceremony<J: CeremonyPersistence = crate::infra::CeremonyJournal> {
    pub journal: Arc<J>,
}
```

**Impl blocks** — from `Box::pin(async move { ... })` to `async fn`:

```rust
impl CeremonyPersistence for CeremonyJournal {
    async fn persist(&self, record: &CeremonyRecord) -> Result<()> {
        // body unchanged
    }
}
```

### Waves

| Wave | Traits | Rationale |
|------|--------|-----------|
| D1 | PondClient, TaskRegistryPersistence | Proof of concept — simplest |
| D2 | OfferingsCachePersistence, ServiceDetector, HarvestOps, CeremonyPersistence | Low-consumer |
| D3 | DockerConfigOps, StoragePlatform, ManagementStoreOps, ContentStoreOps | Mid-complexity, factory pattern |
| D4 | NurturingStoreOps, ServiceRuntime, CompanionOps | High-consumer (13-14 call sites each) |
| D5 | Cleanup: dead imports, `#[expect(type_complexity)]`, docs update | Polish |

### Per-trait checklist

1. [ ] Trait methods: `Pin<Box<dyn Future>>` → `impl Future + Send`
2. [ ] Impl methods: `Box::pin(async move { ... })` → `async fn`
3. [ ] Domain struct: `Arc<dyn T>` → `Arc<I>` with default type parameter
4. [ ] Bootstrap wiring: type inference handles this
5. [ ] Test mocks: use the generic parameter
6. [ ] `cargo check --package garden-moss`
7. [ ] `cargo test --package garden-moss`

---

## Phase E: Code Quality Sweep

**Goal**: Mechanical enforcement of code standards, modern testing.

### E1. `#[allow(lint)]` → `#[expect(lint)]`

Convert ~87 annotations across zen-garden. `#[expect]` warns when the
suppression becomes stale, catching dead annotations automatically.

### E2. `#[must_use]` audit

Add `#[must_use]` to public functions where silently discarding the return value
is likely a bug: broadcast `.send()` wrappers, `.notify()` calls, builder methods.

### E3. `const fn` opportunities

Mark pure constructors and constant-computing functions as `const fn`:
- `PeerAddress::new()`
- `JobResult::success()`, `JobResult::failure()`
- Constants in `garden_common::constants`

### E4. Testing infrastructure

Add as dev-dependencies:
- `proptest` — property-based testing for parsers (`OfferingFqn::parse()`)
- `insta` — snapshot testing for rendering output and S3 XML responses
- `rstest` — parametric tests for validation rules

### E5. Domain error typing

Replace `anyhow::Result` in domain trait signatures with typed error enums.
Coupled to Phase D: as each trait is monomorphized, type its errors.

Priority targets:
- `docker.rs` — 48 `anyhow::anyhow!` calls → `DockerError` enum
- `ServiceRuntime` — container errors → `ContainerError`
- `CompanionOps` — companion errors → `CompanionError`

### E6. `FromRef` for API handlers

Replace `State(state): State<AppState>` with narrower extracted types via
`FromRef`. Coupled to ARCH-0004 domain context extraction.

---

## Consequences

### Benefits

1. **Zero-cost domain boundaries** — 13 trait boundaries become monomorphic,
   eliminating ~60 heap allocations per async call and enabling cross-boundary
   inlining.

2. **Single-source configuration** — workspace deps, edition, rust-version,
   and lint policy each defined in one place.

3. **Mechanical code standards enforcement** — `[workspace.lints]` turns code
   review findings into compiler errors.

4. **Current dependencies** — no unmaintained crates, no version drift between
   workspace members, no duplicate deps.

5. **Edition 2024** — cleaner lifetime capture semantics, enforced unsafe
   discipline, and forward-compatibility with future Rust features.

### Costs

1. **Edition 2024 `unsafe` audit** — ~20 unsafe blocks need inner `unsafe {}`
   wrapping. Mechanical but must be verified.

2. **Generic propagation** — domain structs gain type parameters. Mitigated by
   default type parameters that resolve to production types.

3. **Dependency bumps** — `rand` 0.9 changes `thread_rng()` API, affecting
   ~20 call sites. `serde_yml` requires testing 30 YAML parsing sites.

4. **Testing investment** — proptest, insta, rstest require learning and
   initial test conversion effort.

## Phase Dependencies

```
Phase A (workspace hygiene) ──┬── Phase B (dep bumps)
                              └── Phase C (edition 2024) ── requires A2
Phase D (monomorphic traits) ──── independent, can start after A3
Phase E (code quality) ──── E5 coupled to D, rest independent
```

Phases A, B, D can run in parallel on separate branches. Phase C requires A2
(edition inheritance). Phase E is a continuous effort that runs alongside D.

## References

- [ARCH-0004: AppState Domain Context Extraction](ARCH-0004-appstate-domain-context-extraction.md)
- [ARCH-0005: Structural Quality Pass](ARCH-0005-structural-quality-pass.md)
- [Rust Edition 2024 Guide](https://doc.rust-lang.org/edition-guide/rust-2024/)
- [docs/code-standards.md](../code-standards.md) — §6 FromRef, §10 domain errors, §11 must_use, §16 unsafe, §17 unwrap
