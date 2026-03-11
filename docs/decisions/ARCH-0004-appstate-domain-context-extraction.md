---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-11
---

# ARCH-0004: AppState Domain Context Extraction

**Date**: 2026-03-11
**Status**: Accepted
**Depends on**: ARCH-0003 (Code Standards Compliance Migration)

## Context

ARCH-0003 defined a full structural migration plan, including Wave 6c — the `AppState`
restructure that was to introduce 7 domain context structs, migrate all flat fields, add
`FromRef` impls, and enforce minimal handler dependency surfaces. Wave 6c was explicitly
deferred with the note "Leave AppState for after."

The executed passes (A–F) delivered:

- §3 type renames: role suffixes dropped from type names (`Context`, `Handler`, `Cache`)
- §12 local variable naming: `_clone`, `_arc`, `_tx` suffixes removed from locals
- §4 channel field naming: `_tx`/`_rx` removed from struct fields
- Module file restructuring: several large files split into per-concept files

What was **not** delivered:

- **§5 domain ownership through struct nesting** — no grouping context structs (`Storage`,
  `Security`, `Discovery`) were created. `AppState` remains a 64-field flat bag.
- **§14 file/concept 1:1 coupling** — `domain/` is a flat list of files; there is no
  structural correspondence between file path, type hierarchy, and runtime instance path.
- **§6 `FromRef` handler narrowing** — handlers still take the full `AppState`, declaring
  an implicit dependency on the entire world.

The result is that ARCH-0003's naming work is cosmetic without this structural foundation.
`state.storage_tick` and `state.storage.orchestration.tick` are not equivalent:
the first is a naming convention enforced by humans; the second is a domain boundary
enforced by the compiler. The codebase still has the first.

### The Invariant

Three things must align 1:1 for a domain boundary to be real:

1. **File path** — where the code lives
2. **Type declaration** — the struct/enum hierarchy
3. **Runtime instance** — the field access path on `AppState`

Example for Security:

```
src/moss/src/domain/security/mod.rs       → pub struct Security
src/moss/src/domain/security/pond.rs      → pub struct Pond
src/moss/src/domain/security/ceremony.rs  → pub struct Ceremony
```

```rust
// Type hierarchy
Security { pond: Pond, ceremony: Ceremony, stone_client: Arc<StoneClient>, ... }

// Runtime path
appState.security.pond
appState.security.ceremony
```

A field like `appState.pond_active` — flat, unprefixed, unrouped — is not a domain
boundary. It is a name with an underscore. The compiler does not see Security; neither
does anyone reading the code cold.

## Decision

Execute the domain context extraction deferred from ARCH-0003, governed by these rules:

### Rule 1: No delegation shims

Old call sites are not kept compiling via delegation methods or re-export aliases.
When `storage_tick` is removed from `AppState`, every call site must be updated to its
correct path (`state.storage.orchestration.tick`). A method `AppState::storage_tick()`
that returns `self.storage.orchestration.tick` is prohibited — it is an artificial
redirect that hides the migration debt without paying it.

### Rule 2: File path = type path = runtime path

Every domain context struct lives at the path that names it:

```
domain/storage/mod.rs           → Storage
domain/storage/orchestration.rs → Orchestration
domain/storage/volumes.rs       → Volumes  (already exists; confirmed correct location)
domain/security/mod.rs          → Security
domain/security/pond.rs         → Pond     (currently at domain/pond.rs — must move)
domain/security/ceremony.rs     → Ceremony (currently at domain/ceremony/ — must move)
domain/discovery/mod.rs         → Discovery
domain/discovery/topology.rs    → Topology (currently at domain/topology.rs — must move)
```

Files that do not match this layout are moved. Each move is a two-commit operation:
rename commit (pure `git mv`, no content changes) then content commit.

### Rule 3: Compiler-guided field migration

Each `AppState` flat field is removed, the compiler enumerates all broken call sites,
and each call site is updated to the correct domain path. Fields are migrated in
dependency order — start with the most isolated (fewest callers, clearest domain home)
and end with the most pervasive (`stone_id`, `stone_name`).

Order within the Storage domain (illustrative):

1. `orchestration_nudge` → `storage.orchestration.nudge`
2. `volume_rescan`       → `storage.volumes.rescan`
3. `storage_tick`        → `storage.orchestration.tick`
4. `storage_agg`         → `storage.orchestration.agg`
5. `storage_changed`     → `storage.changed`
6. `volumes`             → `storage.volumes`
7. `media`               → `storage.media`

Each field is one commit. The build must pass between commits.

### Rule 4: `FromRef` at the handler boundary

Once a domain context struct exists as a field on `AppState`, add:

```rust
impl FromRef<AppState> for Arc<Storage> {
    fn from_ref(state: &AppState) -> Self { state.storage.clone() }
}
```

Handlers that previously took `State(state): State<AppState>` are updated
domain-by-domain as their fields are migrated. A handler that only touches storage
fields becomes:

```rust
async fn list_volumes(State(storage): State<Arc<Storage>>) -> impl IntoResponse { ... }
```

Background tasks that are spawned (not Axum handlers) receive individual domain
context clones rather than a full `AppState` clone, where they touch a single domain.
Tasks that genuinely span multiple domains may continue to receive the full `AppState`
or multiple domain context arguments — no artificial narrowing.

### Target domain context structs

| `AppState` field | Domain context | Flat fields it absorbs |
|---|---|---|
| `storage` | `Arc<Storage>` | `storage_tick`, `storage_agg`, `storage_changed`, `orchestration_nudge`, `volumes`, `media`, `volume_rescan`, `harvest_store`, `nurturing_store`, `nourishment_jobs`, `network_metrics_cache` |
| `security` | `Arc<Security>` | `pond`, `pond_active`, `https_started`, `stone_client`, `ceremony_registry`, `ceremony_journal`, `pond_ceremony_host` |
| `discovery` | `Arc<Discovery>` | `topology_cache`, `topology_dirty`, `self_entry`, `mdns_handle`, `koi_handle` |
| `current` | `Current` | `stone_id` → `current.stone.id`, `stone_name` → `current.stone.name` |
| `infra` | `Arc<Infra>` | `docker`, `runtime`, `network` |
| `companions` | `Arc<Companions>` | `companion_registry` |
| `presence` | `Arc<Presence>` | `elections`, `notifications` |

Cross-cutting fields that genuinely belong at the `AppState` level remain flat:
`event_bus`, `shutdown_token`, `console`, `start_time`, `api_port`, `pulse`,
`tools`, `registry`, `offerings`, `manifest_registry`, `jobs`,
`offerings_index`, `capabilities`, `system_resources`, `gpu_utilization`,
`infrastructure_handlers`, `log`, `subsystems`.

### Acceptance criteria

The migration is complete when:

1. `AppState` holds only domain context structs and cross-cutting fields — no flat
   fields whose names encode a sub-domain with an underscore
2. Every domain context struct lives in the file whose path matches its type name
3. Every Axum handler takes the narrowest domain context it actually needs via `FromRef`
4. `cargo check --all` and `cargo clippy --all -- -D warnings` pass clean
5. No delegation shim methods exist on `AppState` that proxy to domain context fields

## Consequences

**Positive**:
- Handler dependency surfaces are enforced by the compiler — a security handler cannot
  accidentally compile if it reaches for a storage field
- New code has an unambiguous home — the file path tells you where to put it
- Domain contexts are independently constructable and testable without full `AppState`
  setup
- `git log --follow` on a domain context file traces its concept history through moves

**Negative / Trade-offs**:
- Background tasks that currently clone the full `AppState` must be audited and narrowed;
  a few coordinator-style tasks that genuinely span domains will still take multiple
  domain context arguments
- The migration is large but incremental — each commit is a single field with all its
  call sites updated. No partial states exist in main history

## Out of Scope

- `anyhow` to typed error enum migration (ARCH-0003 pass `f`) — separate concern
- `bootstrap/run.rs` declarative pipeline (ARCH-0003 Wave 6e) — separate concern
- Orchestrator crates (`src/orchestrators/`) — standalone builds, separate migration
- `garden-common` value objects (`Stone`, `Volume`) — Wave 1 of ARCH-0003; tackled
  separately once the moss structural work is complete
