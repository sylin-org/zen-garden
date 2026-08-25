---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-09
---

# STORAGE-0012: Cloud Filter Architecture Rebuild

**Date**: 2026-03-09
**Status**: Accepted
**Evolves**: STORAGE-0009 (Phase 4 — Cloud Filter integration)

## Context

STORAGE-0009 Phase 4 introduced a Windows Cloud Filter (CfApi) provider so
managed storages appear natively in Explorer under a "Zen Garden" sync root.
The initial implementation suffered from a persistent `0x8007017C`
(`ERROR_CLOUD_FILE_INVALID_REQUEST`) on every placeholder operation — both
the callback path (`pass_with_placeholder`) and the proactive path
(`CfCreatePlaceholders`).

### Root cause analysis

A deep-dive into the `cloud-filter` crate (v0.0.6) internals and the
Microsoft CfApi documentation revealed multiple contributing issues:

1. **Registration was already correct** — `SyncRootId::register()` calls the
   WinRT `StorageProviderSyncRootManager::Register()`, not the lower-level
   `CfRegisterSyncRoot`. The earlier hypothesis of "wrong registration API"
   was wrong.

2. **Zero-timestamp metadata** — `Metadata::directory()` creates
   `CF_FS_METADATA` with all timestamps set to 0 (1601-01-01). The CfApi
   mini-filter driver rejects placeholders whose metadata contains invalid
   timestamps.  This was the primary suspected cause of `0x8007017C`.

3. **Wrong metadata API used** — Our initial timestamp fix used
   `MetadataExt::creation_time(i64)` with manual FILETIME computation.
   The crate's own working integration tests use `nt_time::FileTime::now()`
   with native `Metadata::created(FileTime).written(FileTime)` methods.
   These may produce subtly different results because the native methods
   go through `FileTime::try_into()` (which calls Windows
   `GetSystemTimeAsFileTime` internally), while our manual computation
   used `SystemTime::now()` → nanoseconds → manual epoch offset.

4. **Missing `FileIdentity` blob** — The crate's working tests attach a
   `.blob()` to every placeholder (both files and directories).  While the
   CfApi docs state `FileIdentity` is "required for files (not for
   directories)", the crate's test pattern always includes it.  Our code
   omitted the blob entirely.

5. **Missing `.has_no_children()` on file placeholders** — The working test
   marks file placeholders with `has_no_children()` (which sets
   `CF_PLACEHOLDER_CREATE_FLAG_DISABLE_ON_DEMAND_POPULATION`), telling
   Windows not to attempt directory enumeration on files.

6. **Sentinel file in a CfApi-managed directory** — The sentinel approach
   (`.zen-garden-cfapi-v2`) was fundamentally broken because CfApi
   intercepts all I/O to the sync root, rejecting regular file writes with
   `0x80270005`.

7. **Hardcoded `DISABLE_ON_DEMAND_POPULATION`** — The `cloud-filter` crate
   sets `CF_OPERATION_TRANSFER_PLACEHOLDERS_FLAG_DISABLE_ON_DEMAND_POPULATION`
   on callback-created placeholders (commands.rs:175). This tells Windows
   "directory is fully populated, never ask again." Not a bug for our use
   case — proactive creation handles subsequent changes — but it requires the
   callback and proactive paths to cooperate cleanly.

### CfApi research findings

Analysis of the official Microsoft documentation and community resources:

- **`CfCreatePlaceholders` requires** the base directory to be under a
  registered sync root with a hydration policy that is NOT `ALWAYS_FULL`.
  Our `HydrationType::Full` maps to `StorageProviderHydrationPolicy::Full`
  (not `AlwaysFull`), so this is correct.

- **`CF_PLACEHOLDER_CREATE_INFO.FsMetadata`** — "File system metadata to be
  created with the placeholder, including all timestamps, file attributes
  and file size (optional for directories)."  The "all timestamps" phrasing
  suggests all four FILETIME fields must be populated.

- **`CfExecute` operates in arbitrary thread context** — no COM apartment
  or session constraints documented.  The sync provider process must have
  `WRITE_DATA` or `WRITE_DAC` access to the sync root.

- **Windows 11 24H2+ (builds 26100/26200)** — Known issues with
  cloud-based storage operations after certain updates.  May be a
  contributing factor.

- **Session 0 vs user session** — `PEB::CloudFileFlags` differs for
  services vs interactive users, blocking data hydration for services
  but allowing placeholder population.  Not applicable when moss runs
  as an interactive elevated process.

### `block_on` strategy (critical finding)

The crate's async test uses `futures::executor::block_on` to bridge
async `Filter` callbacks to sync CfApi threads.  **This is NOT compatible
with `tokio::sync::RwLock`** which participates in Tokio's cooperative
scheduling.  When Tokio's coop budget is exhausted, `RwLock::read()` returns
`Pending` even when uncontended.  `futures::executor::block_on` treats this
as "sleep until woken" — causing a deadlock.

Since our `ZenGardenProvider` uses `tokio::sync::RwLock` (for `Volumes` and
`GardenRegistry`), `tokio::fs`, and `reqwest`, we MUST use
`tokio::runtime::Handle::block_on` to properly drive the cooperative
scheduler and tokio IO reactor.  The crate's test only works with
`futures::executor::block_on` because `MemFilter` uses no tokio primitives.

### Structural issues

The original implementation was a monolithic 500-line `mod.rs` mixing
registration, connection, process detection, placeholder creation, and the
storage watcher task. Debugging required reading the entire file to
understand which concern was failing.

## Decision

Break and rebuild the Cloud Filter module into four focused files with clean
separation of concerns. Harvest all working code (provider callbacks, path
resolution, storage enumeration, process detection) and discard what was
broken (sentinel files, zero-timestamp metadata, monolithic layout).

Align placeholder construction exactly with the `cloud-filter` crate's own
integration test patterns:

- Use `nt_time::FileTime::now()` with native `Metadata::created().written()`
- Attach `.blob(name)` to every placeholder
- Use `.has_no_children()` on file placeholders
- Set `.size(0)` explicitly on directory metadata

### Module structure

```
src/moss/src/infra/cloud_filter/
├── mod.rs              # Public API: start() / unregister()
│                       # Lifecycle orchestration only
│
├── registration.rs     # Sync root registration (idempotent)
│                       # ensure_registered() / unregister()
│                       # State via API query, not sentinel files
│
├── provider.rs         # Filter trait impl (CfApi callbacks)
│                       # ZenGardenProvider → delegates to StorageService
│                       # Pure adapter, no state management
│
└── placeholders.rs     # Placeholder creation helpers
                        # build_placeholder() — valid timestamps via FileTime
                        # create/remove storage placeholders
                        # Shared by callback + proactive paths
```

### Key fixes

| Fix | Before | After |
|-----|--------|-------|
| Timestamps | `Metadata::directory()` (all zeros) | `FileTime::now()` + native `Metadata::created().written()` |
| Timestamp API | `MetadataExt::creation_time(i64)` (manual FILETIME) | Native `Metadata::created(FileTime)` (via `nt-time` crate) |
| FileIdentity | Not set | `.blob(name.into())` on every placeholder |
| File flags | None | `.has_no_children()` on file placeholders |
| Dir size | Implicit | `.size(0)` explicit |
| State tracking | Sentinel file in sync root | `SyncRootId::is_registered()` API query |
| Placeholder creation | Two separate metadata paths | Shared `placeholders.rs` for both callback + proactive |
| Log levels | `warn!` for routine operations | `info!` lifecycle, `debug!` polling |
| block_on | `Handle::block_on` | `Handle::block_on` (confirmed: `futures::executor` would deadlock with tokio RwLock) |

### Cooperative population model

- **Callback** (`fetch_placeholders`): Handles initial population when
  Explorer first opens a directory. The crate marks the directory as
  "fully populated" — correct behavior.
- **Proactive** (storage watcher): Handles changes after initial population.
  Uses `CfCreatePlaceholders` for additions, `remove_dir_all` for removals.
- Both paths share `build_placeholder()` from `placeholders.rs`.

### Domain boundary

The Cloud Filter module is pure infrastructure. It delegates all routing
decisions to `StorageService` (domain layer). No business logic in the
CfApi adapter.

## Consequences

### Positive

- Aligns exactly with the crate's working test patterns (timestamps, blob,
  flags) — highest confidence fix for `0x8007017C`
- Removes sentinel file — no more `0x80270005` write failures
- Clean SoC — each file has a single responsibility
- Easier debugging — registration issues in `registration.rs`, callback
  issues in `provider.rs`, timestamp issues in `placeholders.rs`
- Reduced log noise — routine polling at `debug!` level
- Documents the `block_on` constraint for future maintainers

### Negative

- Adds `nt-time` dependency (already a transitive dep of `cloud-filter`)
- The `DISABLE_ON_DEMAND_POPULATION` flag in the `cloud-filter` crate
  remains a constraint — we cannot re-populate a directory via callbacks
  after the first enumeration. Proactive creation compensates.

### Risks

- If `0x8007017C` persists after aligning with the crate's test pattern,
  the next investigation areas are:
  1. Windows 11 24H2+ platform regression (builds 26100/26200 have known
     cloud storage issues)
  2. Elevated process (admin) interacting with non-elevated Explorer for
     CfApi callback dispatch
  3. `DISABLE_ON_DEMAND_POPULATION` interaction with proactive
     `CfCreatePlaceholders`
  4. Sync root path containing spaces (`C:\Users\...\Zen Garden`)
