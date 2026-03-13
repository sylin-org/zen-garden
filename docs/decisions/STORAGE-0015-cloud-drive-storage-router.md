---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-12
---

# STORAGE-0015: Cloud Drive — StorageRouter and Domain Policy Extraction

**Date**: 2026-03-12
**Status**: Accepted
**Evolves**: STORAGE-0012 (Cloud Filter rebuild), STORAGE-0014 Phase 3 (StorageGateway)

## Context

### The Problem

STORAGE-0012 rebuilt the Cloud Filter module into four focused files with clean
separation of concerns. The CfApi callbacks worked, placeholders populated, and
basic hydration (download path) functioned correctly. However, audit of the write
path (upload, rename, move, copy) revealed that `provider.rs` had grown to 1,123
lines — accumulating business logic, HTTP proxy dispatch, filesystem I/O, and
tree-walking algorithms inside what was intended to be a thin CfApi callback adapter.

### Root Cause: Missing Unified Dispatch

The core tension was that `StorageRoute` made a routing decision (local vs proxy)
but left execution to the caller. Every adapter independently implemented the same
`match Local => tokio::fs::..., Proxy => reqwest::...` dispatch pattern:

| Operation | Cloud Filter provider | Cloud Filter ingest | REST files.rs | ContentStore |
|---|---|---|---|---|
| Read file | `read_from_route` | — | `tokio::fs::read` + proxy | `read()` |
| Write file | `write_to_route` | `tokio::fs::copy` | `tokio::fs::write` + proxy | `write()` |
| Delete file | `do_delete` | — | `tokio::fs::remove_*` + proxy | `delete()` |
| List dir | `list_local_dir` / `list_remote_dir` | — | `list_directory` + proxy | — |
| Copy tree | `copy_tree_to_local` / `upload_tree_to_proxy` | — | — | — |
| Rename | `do_rename_subpath` | — | — | — |
| Cross-storage move | `do_cross_storage_move` | — | — | — |

`ContentStore` only covered read/write/delete plus changelog, cursor, and pin state.
It did not expose list, rename, move, copy, or metadata. Every adapter reinvented
these operations on top of raw `tokio::fs` and `reqwest`.

### Domain Logic in the Adapter

Business rules had leaked into the CfApi adapter:

- "Top-level folder rename = replica set name change" — domain rule, lived in
  `provider.rs` `do_rename_storage`
- "Cross-storage move = copy + delete" — domain rule, lived in `provider.rs`
  `do_cross_storage_move`
- "Unknown top-level folder = stray root item, ingest into storage" — domain rule,
  lived in the rename callback's `is_known_storage` branch
- Storage name resolution — duplicated in `provider.rs` (`is_known_storage`),
  `ingest.rs` (`resolve_mount`), and `mod.rs` (`enumerate_storage_names`)

### CfApi Constraint

Windows Cloud Filter has no CREATE callback. File creation in the sync root cannot
be intercepted — only detected post-facto via filesystem notify watcher. Rejectable
operations are limited to: rename (has ticket), delete (has ticket), dehydrate (has
ticket), fetch_data, and fetch_placeholders.

### Cloud Drive Functional Gaps

Five user-facing operations were broken or missing:

1. **Root-level write protection** — users could paste files directly under
   `Zen Garden\`. No CfApi prevention possible; cleanup ran only on 60s heartbeat.
2. **Cross-storage move** — moving files between storages was rejected with
   `CloudErrorKind::NotSupported`.
3. **Move stray root item into storage** — treated as cross-storage move and rejected.
4. **Drag from outside sync root** — `rename` callback failed on source path
   resolution (`NotUnderSyncRoot`).
5. **Ingest reliability** — `ingest.rs` duplicated mount resolution and only
   handled local targets.

### Design Constraints

Two constraints guided the approach:

1. **Break and rebuild** — no backward compatibility shims. Harvest working content
   from existing code and delete all deprecated implementations.
2. **No bespoke code** — use composition with existing, properly designed storage
   capabilities. `ContentStore` handles local I/O. The REST API endpoints handle
   remote operations. A new router composes these; it does not reimplement them.

## Decision

### 1. StorageRouter — Resolution + Dispatch

STORAGE-0014 Phase 3 identified a `StorageGateway` routing port. This ADR
materialized it as `StorageRouter` — a resolved handle to a storage that
encapsulates both routing and dispatch.

`StorageRoute` (the enum with `Local` and `Proxy` variants) remained as the
internal routing decision. `StorageRouter` wrapped it, exposing file operations
directly:

```
infra/
  storage/router.rs   — StorageRouter struct + cross-storage free functions
```

```rust
/// Resolved handle to a storage — local or remote.
///
/// Callers never match on Local vs Proxy. They call operations directly.
/// Local → ContentStore. Remote → HTTP to existing REST endpoints.
pub struct StorageRouter {
    route: StorageRoute,
    storage_name: String,
}
```

Resolution (replaces `StorageRoute::for_read` / `for_write` at call sites):

```rust
impl StorageRouter {
    pub async fn for_read(name, volumes, registry, stone_id) -> Result<Self>;
    pub async fn for_write(name, volumes, registry, stone_id) -> Result<Self>;
}
```

Operations — the caller never sees local vs proxy:

```rust
impl StorageRouter {
    pub async fn read(&self, path: &str) -> Result<Vec<u8>>;
    pub async fn write(&self, path: &str, data: &[u8]) -> Result<()>;
    pub async fn delete_file(&self, path: &str) -> Result<()>;
    pub async fn delete_dir(&self, path: &str) -> Result<()>;
    pub async fn list(&self, path: &str) -> Result<Vec<FileEntry>>;
    pub async fn rename(&self, old: &str, new: &str) -> Result<()>;
    pub async fn mkdir(&self, path: &str) -> Result<()>;
    pub async fn metadata(&self, path: &str) -> Result<FileMeta>;
    pub async fn exists(&self, path: &str) -> Result<bool>;
}
```

Each method dispatched in a single match:

- `Local` → `ContentStore` methods (extended — see below)
- `Proxy` → HTTP to `{endpoint}/api/v1/garden/storage/{name}/files/{path}`
  using existing REST endpoints that already handled these operations

Cross-storage operations composed two routers:

```rust
/// Move a file between two storages.
pub async fn transfer(src: &StorageRouter, src_path, dst: &StorageRouter, dst_path) -> Result<()>;

/// Copy a directory tree between two storages.
pub async fn transfer_tree(src: &StorageRouter, src_path, dst: &StorageRouter, dst_path) -> Result<()>;
```

No special-casing for local-to-local, local-to-remote, remote-to-local. The router
handled routing transparently. Both functions were generic free functions composing
router primitives.

### 2. ContentStore Extension

`ContentStore` was the properly designed local storage chokepoint. It was extended
with the missing primitives needed by the router:

```rust
impl ContentStore {
    // Existing: read, write, delete, exists, read_string, write_string, full_path
    // Added:
    pub async fn list_dir(&self, rel: &str) -> Result<Vec<(String, bool, u64, Option<DateTime>)>>;
    pub async fn rename_path(&self, old: &str, new: &str) -> Result<()>;
    pub async fn delete_dir(&self, rel: &str) -> Result<()>;
    pub async fn delete_file(&self, rel: &str) -> Result<()>;
    pub async fn read_file(&self, rel: &str) -> Result<Vec<u8>>;
    pub async fn write_file(&self, rel: &str, data: &[u8]) -> Result<()>;
    pub async fn file_metadata(&self, rel: &str) -> Result<Metadata>;
    pub async fn mkdir(&self, rel: &str) -> Result<()>;
}
```

Mutations (`rename_path`, `delete_dir`) recorded changelog entries, following the
same pattern as the existing `write` and `delete` methods.

### 3. CloudDrive Domain Policy

A pure domain policy module extracted business rules from the CfApi adapter:

```
domain/
  cloud_drive.rs   — DriveAction enum + classify_rename pure function
```

```rust
pub enum DriveAction {
    IngestFromOutside { source: PathBuf, storage: String, path: String, is_dir: bool },
    DeleteFromStorage { storage: String, path: String, is_dir: bool },
    RenameInStorage { storage: String, old: String, new: String },
    CrossStorageMove { src_storage: String, src: String, dst_storage: String, dst: String, is_dir: bool },
    RenameStorage { old_name: String, new_name: String },
    IngestStray { stray_path: PathBuf, storage: String, path: String, is_dir: bool },
    Reject { reason: &'static str },
}
```

The rename decision tree (90 lines of nested if/else in the `rename` callback)
became a single pure function:

```rust
pub fn classify_rename(
    source_in_scope: bool,
    target_in_scope: bool,
    old_storage: &str,
    old_rel: &str,
    new_storage: &str,
    new_rel: &str,
    is_dir: bool,
    is_known_storage: bool,
    source_path: &Path,
    sync_root_path: &Path,
) -> DriveAction;
```

Decision tree:

1. `!source_in_scope && target_in_scope` → `IngestFromOutside`
2. `source_in_scope && !target_in_scope` → `DeleteFromStorage`
3. Both in scope, `old_storage` empty → `Reject` (sync root level)
4. Both in scope, `old_rel` and `new_rel` empty, names differ → `RenameStorage`
5. Both in scope, storages differ, source not a known storage → `IngestStray`
6. Both in scope, storages differ, source is a known storage → `CrossStorageMove`
7. Both in scope, same storage → `RenameInStorage`

Testable without CfApi, without filesystem, without network.

### 4. Adapter Rebuild

**`provider.rs`** was rebuilt as a CfApi translation layer. Each callback parsed
the CfApi request into domain terms, delegated to `StorageRouter` and `CloudDrive`,
and mapped results back to CfApi tickets:

```rust
async fn rename(&self, request, ticket, rename_info) -> CResult<()> {
    let action = cloud_drive::classify_rename(..);
    match action {
        DriveAction::IngestFromOutside { source, storage, path } => {
            let dst = StorageRouter::for_write(&storage, ..).await?;
            let data = tokio::fs::read(&source).await?;
            dst.write(&path, &data).await?;
        }
        DriveAction::CrossStorageMove { src_storage, src, dst_storage, dst } => {
            let src_r = StorageRouter::for_read(&src_storage, ..).await?;
            let dst_r = StorageRouter::for_write(&dst_storage, ..).await?;
            storage_router::transfer(&src_r, &src, &dst_r, &dst).await?;
        }
        // ...
    };
    ticket.pass()?;
    Ok(())
}
```

**`ingest.rs`** was rebuilt to use `StorageRouter` for file writes. The watcher,
debounce, and `should_ingest` filtering remained (legitimate adapter orchestration).
`resolve_mount` and duplicate copy logic were deleted.

**`files.rs`** was rebuilt to use `StorageRouter` for all operations. Validation
logic remained. All `match route { Local => ..., Proxy => ... }` blocks were deleted.

### Root-Level Stray Cleanup

`purge_stray_root_items` was added to `reconcile_placeholders` in `mod.rs`.
On each reconciliation pass (event-driven + 60s heartbeat), any file or directory
at the sync root level that was not a known storage name was removed. This was the
best available mitigation given CfApi's lack of a CREATE callback.

## Code Removed

All of the following were deleted from `provider.rs`:

- `do_ingest_from_outside` — replaced by `StorageRouter::write` / `tokio::fs::read`
- `do_cross_storage_move` — replaced by `storage_router::transfer`
- `do_cross_storage_copy_tree` — replaced by `storage_router::transfer_tree`
- `read_from_route` / `write_to_route` — replaced by `StorageRouter::read` / `write`
- `copy_tree_to_local` / `upload_tree_to_proxy` — replaced by `ContentStore::copy_tree`
  and `StorageRouter::write`
- `list_local_dir` / `list_remote_dir` — replaced by `StorageRouter::list`
- `http_client` — proxy dispatch moved to `router.rs`

Deleted from `ingest.rs`:

- `resolve_mount` — replaced by `StorageRouter::for_write`

Deleted from `files.rs`:

- `list_directory` — replaced by `StorageRouter::list`
- All `match route { Local => tokio::fs::..., Proxy => proxy_request(...) }` blocks

## Module Structure

```
domain/
  storage_service.rs    — StorageRoute enum, LocalStorage, ProxyTarget (retained)
  cloud_drive.rs        — DriveAction + classify_rename (new)

infra/
  storage/router.rs     — StorageRouter + transfer/transfer_tree (new)
  storage/store.rs      — ContentStore (extended: list, rename, delete_dir, mkdir, metadata)
  cloud_filter/
    provider.rs         — CfApi translation + do_rename_storage (~588 lines, down from 1,123)
    ingest.rs           — watcher + filter + StorageRouter calls (~447 lines, down from 439)
    mod.rs              — lifecycle, reconciliation, stray cleanup (extended)
    placeholders.rs     — unchanged
    registration.rs     — unchanged

api/v1/
  garden_storage/
    files.rs            — validation + StorageRouter calls (~413 lines, down from 487)
```

## Consequences

### Positive

- **Single dispatch site** — local-vs-proxy routing resolved once in `StorageRouter`,
  not in every adapter. Adding a new file operation requires one implementation, not four.
- **Testable domain policy** — `classify_rename` is a pure function. Every branch of the
  rename decision tree can be unit tested without CfApi, filesystem, or network.
- **Cloud drive fully functional** — all five gaps addressed: drag-and-drop into storage,
  cross-storage move, stray root item ingest, move from outside sync root, root-level
  cleanup.
- **ContentStore completeness** — list, rename, mkdir, metadata added with changelog
  integration. Local storage mutations flow through a single chokepoint.
- **Provider reduced ~2x** — from 1,123 lines of mixed concerns to 588 lines. Not the
  4x reduction originally targeted (see Addendum: Remaining Domain Leak).
- **No bespoke code** — `StorageRouter` composes `ContentStore` (local) and existing REST
  endpoints (proxy). No parallel I/O stack.

### Negative

- `StorageRouter` introduces a new indirection layer between adapters and `ContentStore`.
  Callers can no longer directly choose local-only behavior without going through the
  router. For admin operations that must be stone-local, `StorageRoute::find_local`
  is retained.
- `ContentStore` grows by ~8 methods. Each is small, but the store's surface area
  increases.

### Risks

- The proxy path for `rename` requires corresponding REST endpoints.
  The current file API has GET/PUT/DELETE/HEAD but no MOVE or COPY verb. Two options:
  implement them as composed operations (read + write + delete via existing endpoints)
  or add dedicated endpoints. The composed approach was chosen to avoid API expansion.
- `transfer_tree` performs one HTTP request per file for cross-storage remote copies.
  For large directory trees this could be slow. Acceptable for initial implementation;
  a bulk transfer endpoint could be added later if profiling warrants it.

---

## Addendum: Implementation Audit (2026-03-12)

Post-implementation review identified gaps between the ADR's stated intent and the
realized code. This addendum records those gaps, their severity, and the plan to
resolve each. The same "break and rebuild" constraint applies.

### A1. Replication Ticks Lost Through Router (Functional)

**Problem**: `StorageRouter` constructs `ContentStore` via `local.content_store()`,
which passes `None` for the notification channel. Writes through the router append
changelog entries (correct) but do not emit `StorageTick` events. The replication
loop subscribes to ticks as its real-time wake signal. Without ticks, replication
discovers changes only on its next polling interval.

Contrast with WebDAV, S3 gateway, and replication tasks — all use
`local.notifying_content_store(Some(&tick_tx))`.

**Fix**: `StorageRouter::for_write` accepts an optional `&broadcast::Sender<StorageTick>`.
When present, local writes go through `notifying_content_store`. Read-only routers
continue to use the bare store. Callers that need replication (cloud filter, ingest,
file API writes) pass the tick sender; callers that don't (read-only queries) omit it.

### A2. Remote Directory Rename Broken (Correctness)

**Problem**: `StorageRouter::rename` for remote targets does
`read(old) → write(new) → delete_file(old)`. This works for files. For directories,
`read()` hits the GET endpoint which returns a JSON listing — writing that as a file
at the destination corrupts the rename. `RenameInStorage` in `DriveAction` does not
carry `is_dir`, so the caller cannot branch.

**Fix**: Two changes:
1. Add `is_dir: bool` to `DriveAction::RenameInStorage`.
2. `StorageRouter::rename` gains an `is_dir` parameter. For directories on remote
   targets, the operation becomes `transfer_tree → delete`. For local targets,
   `tokio::fs::rename` handles both (unchanged).

### A3. `do_rename_storage` Still in Adapter (Architectural)

**Problem**: The ADR's Context section (line 49) identified `do_rename_storage` as
domain logic that had leaked into the CfApi adapter. The implementation still has it
in `provider.rs` (lines 115–173). It directly writes to `Volumes` and calls
`update_manifest_replica_set_name`.

**Fix**: Extract to a storage domain service method on `Storage` or `StorageBank`.
The provider's `RenameStorage` match arm becomes a one-line delegation. This also
removes `provider.rs`'s direct dependency on `crate::api::v1::storage`.

### A4. Missing `copy_within` / `copy` / `copy_tree` (Feature Gap)

**Problem**: The original ADR specified `copy_within` on `StorageRouter` and
`copy` / `copy_tree` on `ContentStore`. Neither was implemented. Within-storage
copy (Ctrl+C → Ctrl+V in same folder) falls through to CfApi default handling.

**Fix**: Implement `ContentStore::copy_file` and `ContentStore::copy_tree` with
changelog entries. Add `StorageRouter::copy_within` that delegates to ContentStore
locally and to `read → write` for remote. Low priority — CfApi default handling
works for the common case; this matters for remote targets.

### A5. Per-Call HTTP Client (Performance)

**Problem**: `router.rs` creates a new `reqwest::Client` on every operation.
TLS session reuse and connection pooling are wasted. The same pattern (including
`danger_accept_invalid_certs(true)`) is duplicated in `garden_storage/mod.rs` and
`webdav.rs`.

**Fix**: Shared `static Lazy<Client>` in a common location. All three call sites
use the same client. The `danger_accept_invalid_certs(true)` configuration is
required for Pond mTLS proxying and should be a single, documented decision.

### A6. Dead Fields on `ZenGardenProvider` (Cleanup)

**Problem**: `tick` and `local_endpoint` fields carry `#[allow(dead_code)]`. They
were needed for the old provider that did its own HTTP dispatch. Now that the router
handles everything, they are vestiges.

**Fix**: Remove both fields. If A1 (replication ticks) requires `tick` access in the
provider, re-add it without the dead_code annotation as part of that fix.

### A7. `is_blocked_path` is Fragile (Correctness Edge Case)

**Problem**: `share.rs` checks 7 string patterns per blocked name. Mixed separators
(e.g. `foo\$RECYCLE.BIN/bar`) can slip through because each check looks for one
separator style at a time.

**Fix**: Split on both separators and check each component:
```rust
pub fn is_blocked_path(rel_path: &str) -> bool {
    rel_path.split(&['/', '\\'])
        .any(|component| BLOCKED.contains(&component))
}
```

### A8. Blocking I/O in Ingest (Known Constraint)

**Problem**: `ingest.rs` uses `std::fs::metadata` (for `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`
check) and `std::fs::OpenOptions` (for `mark_in_sync` via the `cloud-filter` crate).
Both are blocking calls in an async context.

**Status**: Accepted limitation. The `cloud-filter` crate v0.0.6 requires
`std::fs::File`. The metadata check is a single stat call on a local file. Both are
fast enough to not warrant `spawn_blocking`. Document as known constraints.

### A9. `match Local/Proxy` Still Pervasive (Architectural — Expanded Scope)

**Problem**: The original ADR scoped its rebuild to `provider.rs`, `ingest.rs`, and
`files.rs`. But the root cause analysis identified the systemic `match Local/Proxy`
pattern as the problem. Fifteen dispatch sites remain across four files:

| File | Handlers | Local backend | Proxy mechanism |
|------|----------|---------------|-----------------|
| `objects.rs` | 4 (`get`, `put`, `delete`, `head`) | `ObjectStore` | `proxy_request()` |
| `memories.rs` | 4 (`list_memories`, `list_snapshots`, `get_manifest`, `download`) | `ContentStore` / raw `tokio::fs` | `proxy_memories_request()` |
| `s3_gateway.rs` | 6 (`put`, `get`, `head`, `delete`, `list_buckets`, `list_objects`) | `ObjectStore` | `proxy_s3_request()` |
| `webdav.rs` | 1 (`handle_webdav`) | `DavHandler` + `LocalFs` | `proxy_webdav()` |

Every handler follows the same shape:

```rust
match route {
    StorageRoute::Local(local) => {
        let store = local.object_store();  // or content_store() or DavHandler
        // ... local operation ...
    }
    StorageRoute::Proxy(target) => {
        proxy_request(&target.endpoint, method, path, body).await
    }
}
```

This is the same anti-pattern STORAGE-0015 eliminated from `files.rs`. Each new
object/memory operation requires implementing the dispatch twice.

**Fix**: Superseded by A10 (StorageHandle consolidation). Rather than replicating
the router pattern three more times (`ObjectRouter`, `MemoriesRouter`, WebDAV wrapper),
all operation families merge into a single `StorageHandle`. See A10 for the complete
design.

### A10. Structural Consolidation — `StorageHandle` (Architectural)

Post-implementation analysis of the type graph revealed that `StorageRouter` solved
the dispatch problem for files but created a template for proliferating router types:
the natural "next step" for A9 would have been `ObjectRouter`, `MemoriesRouter`, and a
WebDAV wrapper — three new types following the same pattern. The pattern itself is the
abstraction; the operation family is not.

#### Current type graph

```
StorageRoute (domain enum — public, matched at 15+ call sites)
├── LocalStorage (value object: id, name, mount_path, role, ...)
│   ├── .content_store()          → ContentStore (no ticks — BUG A1)
│   ├── .notifying_content_store() → ContentStore (with ticks)
│   ├── .object_store()           → ObjectStore (wraps ContentStore)
│   └── .notifying_object_store()  → ObjectStore (with ticks)
└── ProxyTarget (endpoint + stone_id)

StorageRouter (infra, wraps StorageRoute — file ops only)
├── read, write, delete, list, rename, mkdir, metadata, exists
└── always constructs non-notifying ContentStore

ObjectStore (infra, wraps ContentStore + S3 key-to-path mapping)
└── get, put, delete, head, list — constructed at 10 call sites
```

**Four structural redundancies**:

1. **Three planned routers are the same pattern** — `ObjectRouter`, `MemoriesRouter`,
   and a WebDAV wrapper all wrap `LocalStorage`/`ProxyTarget` and dispatch. The
   operation family varies; the dispatch does not.

2. **`ObjectStore` is `ContentStore` + a path prefix** — it maps
   `(bucket, key)` → `.zen-garden/storage/{bucket}/{key}` then delegates to
   `ContentStore`. This is a naming convention, not a separate store.

3. **Resolution args repeated everywhere** — every handler writes the same 4-arg
   resolution: `StorageRoute::for_read(&name, &state.volumes, &state.registry, &state.stone_id)`.
   This appears 24 times. The resolution context is static for the lifetime of a
   request.

4. **`content_store()` vs `notifying_content_store()` is a footgun** — two
   constructors for the same type, differing only in whether ticks fire. The router
   uses the wrong one (A1). The fix is not "pass tick_tx to the router" — it is
   to eliminate the split entirely. A `ContentStore` always holds
   `Option<Sender<StorageTick>>`. Whether ticks fire depends on whether the sender
   is present, not on which constructor was called.

#### Decision: one handle, all operation families

**`StorageHandle`** replaces `StorageRouter` and absorbs the planned
`ObjectRouter` / `MemoriesRouter`. One resolved type, multiple operation surfaces
as method groups. The tick sender is wired at construction — A1 fixed by design.

```rust
/// Resolved handle to a named storage — local or remote.
///
/// Constructed once per request. Carries everything needed to dispatch
/// file, object, and memory operations. Callers never match on
/// local vs proxy.
pub struct StorageHandle {
    name: String,
    local: Option<LocalStorage>,
    remote: Option<ProxyTarget>,
    tick: Option<broadcast::Sender<StorageTick>>,
}
```

**`StorageResolver`** captures the four resolution args once per request,
eliminating 24 copies of the same argument list:

```rust
/// Resolution context — constructed from AppState at the start of a request.
pub struct StorageResolver<'a> {
    volumes: &'a Volumes,
    registry: &'a GardenRegistry,
    stone_id: &'a str,
    tick: Option<&'a broadcast::Sender<StorageTick>>,
}

impl StorageResolver<'_> {
    pub async fn read(&self, name: &str) -> Result<StorageHandle>;
    pub async fn write(&self, name: &str) -> Result<StorageHandle>;
    pub async fn find_local(&self, name: &str) -> Option<LocalStorage>;
}
```

Handler before:
```rust
let router = StorageRouter::for_write(&name, &state.current.storage.volumes,
    &state.tool.registry, &state.current.stone.id).await?;
router.write_file(path, &data).await?;
```

Handler after:
```rust
let resolve = StorageResolver::from(&state);
let handle = resolve.write(&name).await?;
handle.write_file(path, &data).await?;
```

#### Operation families — method groups on `StorageHandle`

Each family dispatches the same way internally: `if let Some(local) → use store`,
`else → HTTP to endpoint`. The URL path template and local backend differ per family.

**File operations** (replaces `StorageRouter`):

```rust
impl StorageHandle {
    // Streaming I/O — primary API for data transfer (A11j)
    pub async fn open_read(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>>;
    pub async fn open_write(&self, path: &str, source: impl AsyncRead + Send) -> Result<u64>;
    pub async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>>;

    // Buffered I/O — encrypted content, small files, backward compat
    pub async fn read_file(&self, path: &str) -> Result<Vec<u8>>;
    pub async fn write_file(&self, path: &str, data: &[u8]) -> Result<()>;

    // Structural operations
    pub async fn delete_file(&self, path: &str) -> Result<()>;
    pub async fn delete_dir(&self, path: &str) -> Result<()>;
    pub async fn list_dir(&self, path: &str) -> Result<Vec<FileEntry>>;
    pub async fn rename(&self, old: &str, new: &str, is_dir: bool) -> Result<()>;
    pub async fn copy_within(&self, src: &str, dst: &str) -> Result<()>;
    pub async fn mkdir(&self, path: &str) -> Result<()>;
    pub async fn metadata(&self, path: &str) -> Result<FileMeta>;
    pub async fn exists(&self, path: &str) -> Result<bool>;
}
// Local: ContentStore. Remote: /api/v1/garden/storage/{name}/files/{path}
// Encrypted stores: open_read/open_write fall back to buffered internally.
// See A11j for the three-mechanism I/O model and encryption constraint.
```

**Object operations** (eliminates 10 dispatch sites in `objects.rs` + `s3_gateway.rs`):

```rust
impl StorageHandle {
    pub async fn get_object(&self, bucket: &str, key: &str) -> Result<Vec<u8>>;
    pub async fn put_object(&self, bucket: &str, key: &str, data: &[u8]) -> Result<PutResult>;
    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<()>;
    pub async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMetadata>;
    pub async fn list_objects(&self, bucket: &str, prefix: &str, ...) -> Result<ListResult>;
}
// Local: ObjectStore (ContentStore + path prefix). Remote: /api/v1/garden/storage/{name}/objects/{key}
```

**Memory operations** (eliminates 4 dispatch sites in `memories.rs`):

```rust
impl StorageHandle {
    pub async fn list_memories(&self) -> Result<Vec<MemoryEntry>>;
    pub async fn list_snapshots(&self, offering: &str) -> Result<Vec<SnapshotEntry>>;
    pub async fn read_manifest(&self, offering: &str) -> Result<String>;
    pub async fn download_snapshot(&self, offering: &str, harvest: &str) -> Result<Vec<u8>>;
}
// Local: ContentStore with .zen-garden/memories/ paths. Remote: /api/v1/garden/storage/{name}/memories/...
```

**Cross-storage** (free functions, streaming where possible):

```rust
/// Stream-based single file transfer — zero full-buffer for unencrypted paths.
pub async fn transfer(src: &StorageHandle, src_path: &str, dst: &StorageHandle, dst_path: &str) -> Result<u64>;

/// Recursive tree copy with depth limit (A11l). Each file uses streaming transfer.
pub async fn transfer_tree(src: &StorageHandle, src_path: &str, dst: &StorageHandle, dst_path: &str) -> Result<()>;

/// Ingest from arbitrary filesystem path into storage.
pub async fn ingest(source: &Path, dst: &StorageHandle, dst_path: &str, is_dir: bool) -> Result<()>;
```

**WebDAV** — the one structural exception. WebDAV proxies entire HTTP requests,
not individual operations. A thin wrapper resolves the handle and branches:

```rust
let handle = resolve.write(&storage_name).await?;
if let Some(mount) = handle.mount_path() {
    serve_local(mount, ...).await
} else {
    proxy_webdav(handle.endpoint(), ...).await
}
```

This is not a `match Local/Proxy` on `StorageRoute` — it uses handle accessors.
WebDAV is the sole remaining site with an explicit local/remote branch, and its
structure (forward an opaque HTTP request) makes generic dispatch inappropriate.

#### What this eliminates

| Before | After |
|--------|-------|
| `StorageRoute` (public enum, matched 15+ times) | Internal to `StorageHandle` |
| `StorageRouter` (file ops only) | `StorageHandle` file methods |
| Planned `ObjectRouter` | `StorageHandle` object methods |
| Planned `MemoriesRouter` | `StorageHandle` memory methods |
| `ObjectStore` at call sites (10 sites) | Internal to handle |
| `content_store()` vs `notifying_content_store()` footgun | Tick always wired (or None) — A1 fixed by design |
| 24 copies of 4-arg resolution call | `StorageResolver::from(&state)` |
| 15 `match Local/Proxy` dispatch sites | Zero (all in `StorageHandle` internals) |
| 3 separate `reqwest::Client` instantiations | Shared `static Lazy<Client>` on handle |

**Net type count**: `StorageRoute` + `StorageRouter` + planned `ObjectRouter` +
planned `MemoriesRouter` → `StorageHandle` + `StorageResolver`. Four types become two.

#### What stays

- **`ContentStore`** — local filesystem chokepoint. Used internally by the handle,
  and directly by replication/watcher tasks that need raw store access.
- **`ObjectStore`** — internal to the handle's object methods. Still exists as a
  type for the S3 key-to-path mapping. Not visible to callers.
- **`LocalStorage` / `ProxyTarget`** — value objects. The handle holds them;
  callers access mount path or endpoint via handle accessors, never pattern-match.
- **`DriveAction` / `classify_rename`** — pure domain policy, unchanged.
- **`StorageRoute::find_local`** — retained on `StorageResolver` for admin
  operations that must be stone-local (proxy loop guards, rename storage).

#### Module structure after consolidation

```
domain/
  storage_service.rs    — StorageRoute (now pub(crate)), LocalStorage, ProxyTarget
  cloud_drive.rs        — DriveAction + classify_rename

infra/
  storage/handle.rs     — StorageHandle + StorageResolver + cross-storage functions
  storage/store.rs      — ContentStore (tick always wired as Option)
  storage/objects.rs    — ObjectStore (internal to handle, not called directly)
  cloud_filter/
    provider.rs         — CfApi translation only (do_rename_storage extracted)
    ingest.rs           — watcher + filter + handle calls
    mod.rs              — lifecycle, reconciliation, stray cleanup

api/v1/
  garden_storage/
    files.rs            — validation + handle file methods
    objects.rs           — validation + handle object methods
    memories.rs          — validation + handle memory methods
  s3_gateway.rs          — S3 protocol translation + handle object methods
  webdav.rs              — WebDAV protocol translation + handle mount/endpoint accessors
```

### A11. Robustness Audit (2026-03-12)

Systems integration review of the implementation. Each finding evaluated against
whether the proposed fix serves the actual problem, not a proxy for it.

#### A11a. String-Based Error Classification → `RouterError` (Correctness)

**Problem**: `files.rs` detects 404 via substring matching on `anyhow::Error` messages:

```rust
if msg.contains("not found") || msg.contains("NotFound") || msg.contains("404") {
```

Three copies (GET, DELETE, HEAD). If any upstream error changes wording, the handler
returns 500 instead of 404. The remote proxy path already has structured status codes
that get flattened into `anyhow::bail!` — the information exists but is discarded.

**Fix**: `StorageHandle` (A10) returns a typed error. Only two variants are needed —
callers distinguish exactly two failure modes:

```rust
pub enum RouterError {
    NotFound(String),      // carries the path for diagnostics
    Other(anyhow::Error),
}
```

The remote path maps HTTP 404 to `NotFound`; all other failures to `Other`. The local
path maps `io::ErrorKind::NotFound` to `NotFound`. `NotFound` carries the path string
for structured logging. No other variants — callers don't match on anything else today,
and speculative variants violate YAGNI.

**Scope**: Implemented on `StorageRouter` now. Absorbed by `StorageHandle` (A10) when
that consolidation lands.

#### A11b. `is_blocked_path` String Surgery → Split-and-Check (Correctness)

**Problem**: Seven `format!` + `starts_with`/`ends_with`/`contains` checks with mixed
`/` and `\\` separators. Misses edge cases (double separators, trailing separators).

**Fix**: Confirmed correct — split on both separators, check each component:

```rust
pub fn is_blocked_path(rel_path: &str) -> bool {
    rel_path.split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .any(|component| BLOCKED.contains(&component))
}
```

No allocations, handles all separator combinations. Case-sensitive matching preserved
(parity with existing behavior). Case-insensitive matching on Windows is a separate
concern — `$RECYCLE.BIN` could appear as `$Recycle.Bin` — tracked but not blocked by
this fix.

**Priority**: Fix now (trivial, standalone).

#### A11c. Stray Purge Races with Ingest → Coordination, Not Grace Period (Data Loss)

**Problem**: `purge_stray_root_items` runs on the 60-second reconciliation heartbeat.
`ingest.rs` processes files with a 2-second debounce. If reconciliation fires between
paste and ingest transfer, the stray file is deleted before ingest copies it to storage.

**Rejected fix**: Grace period (skip items younger than N seconds). This is a timing
heuristic — it reduces the window but doesn't close it. A large file, slow network, or
remote proxy extends ingest time past any fixed grace period.

**Correct fix (option A — coordination)**: Ingest already maintains a `pending: HashMap<PathBuf, ()>`.
Expose the set of pending storage-name-level parents so `purge_stray_root_items` skips
items that ingest is actively tracking. This is a contract, not a heuristic.

**Correct fix (option B — reduce aggression)**: Stray root items are cosmetically
undesirable but functionally inert. They don't break storage operations, don't corrupt
data, and don't confuse replication. The risk/reward of aggressive 60-second cleanup is
poor. Move stray purge to a much longer interval (hourly or on-demand) and accept that
root-level clutter may persist briefly. The CfApi constraint (no CREATE callback) means
this is inherently best-effort.

**Recommendation**: Option B (reduce frequency) as the immediate fix. Option A if
user feedback demands faster cleanup.

#### A11d. Replica Set Name Fallback Scattered → `Management::display_name()` (Divergence)

**Problem**: Three+ copies of the same pattern:

```rust
let display = if mgmt.replica_set_name.is_empty() {
    DEFAULT_REPLICA_SET_DISPLAY
} else {
    &mgmt.replica_set_name
};
```

In `storage_service.rs`, `cloud_filter/mod.rs`, and `provider.rs`. If the fallback
logic changes (e.g., device name as secondary fallback), all copies must be found and
updated.

**Fix**: Add method to `Management`:

```rust
impl Management {
    pub fn display_name(&self) -> &str {
        if self.replica_set_name.is_empty() {
            DEFAULT_REPLICA_SET_DISPLAY
        } else {
            &self.replica_set_name
        }
    }
}
```

All three call sites collapse to `mgmt.display_name()`.

**Priority**: Fix now (trivial, standalone).

#### A11e. `is_known_storage` Full Enumeration → Short-Circuit (Performance)

**Problem**: `provider.rs:86` acquires two `RwLock` read guards, iterates all volumes +
all registry entries, builds a `HashSet`, then calls `.contains()` once. Fires on every
rename callback (human-frequency — not a hot path).

**Fix**: Short-circuit: try `find_local` first (single lock), then check registry only
if not found:

```rust
async fn is_known_storage(&self, name: &str) -> bool {
    if StorageRoute::find_local(name, &self.volumes).await.is_some() {
        return true;
    }
    let reg = self.registry.read().await;
    reg.storage_entries().iter().any(|e| e.tool.fqid == name)
}
```

Marginally better (skips HashSet allocation when local). Not transformative.

**Priority**: Fix if touching the file, don't make a special trip.

#### A11f. `has_path_traversal` Duplicated → Shared Validation (Security)

**Problem**: Identical function in `garden_storage/mod.rs:174` and `s3_gateway.rs:77`.
This is a security-relevant function — path traversal checks must be consistent across
all access paths. A fix to one copy that doesn't reach the other is a vulnerability.

**Fix**: Extract to `garden_common::utils::validation` (or similar shared location).
Both call sites import from there.

**Priority**: Higher than the duplication suggests. Security-relevant code must have a
single source of truth. Fix now.

#### A11g. Magic Hex Constants in `mark_in_sync` → Named Constants (Readability)

**Problem**: `ingest.rs:392` uses `0x0012_0003` (composite Win32 access mode) and
`0x0200_0000` (`FILE_FLAG_BACKUP_SEMANTICS`) without names. The composite is
`FILE_LIST_DIRECTORY | FILE_ADD_FILE | SYNCHRONIZE | READ_CONTROL` — invisible without
the Win32 docs open.

**Fix**: If `windows-sys` is already a dependency, use its constants with `|`
composition. If not, define named constants in a `cloud_filter::win32` module with
doc comments explaining the composition. The local
`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` at `ingest.rs:163` is correctly named but should
be shared if used elsewhere.

**Priority**: Fix if touching the file.

#### A11h. Remote List Silent Empty → Improved Observability (Resilience)

**Problem**: `router.rs:349` parses remote directory listing JSON. If the shape doesn't
match `{"data": {"entries": [...]}}`, the function returns an empty list with a `warn!`.

**Reviewed assessment**: The proposed hard-error fix is **wrong** for the CfApi path.
`fetch_placeholders` calls `router.list()` — returning an error would surface as a
broken Explorer folder. An empty listing is cosmetically wrong but survivable; an error
icon is worse UX for a transient issue.

For REST API callers (`files.rs` GET on a directory), an empty listing for a parse
failure is silently misleading — the caller can't distinguish "genuinely empty directory"
from "remote returned garbage."

**Fix (CfApi path)**: Keep empty-list fallback for resilience. Promote `warn!` to
`error!` with structured fields indicating which part of parsing failed (missing `data`
key, missing `entries` key, not an array).

**Fix (REST API path)**: `StorageHandle` (A10) can return a richer result type for
listings that distinguishes `Ok(entries)` from `Ok(empty, parse_failed: true)`, or
carry the distinction as a response header. Deferred to A10 design.

**Priority**: Improve logging now (trivial); structural fix deferred to A10.

#### A11i. Identical `find_remote` / `find_remote_primary` → Collapse (Clarity)

**Problem**: `storage_service.rs:266` and `storage_service.rs:282` — identical bodies.
Both call `reg.route_to_primary()`. The name distinction implies different behavior that
doesn't exist.

**Why they're identical today**: `route_to_primary` only knows about Primaries. If the
registry later learns about Dormant replicas, these functions would diverge. But
speculative divergence points are worse than actual duplication — they mislead readers
into thinking the distinction already matters.

**Fix**: Collapse to a single `find_remote` with a clear doc comment. Re-split if/when
the registry gains Dormant awareness.

**Priority**: Fix now (trivial, standalone).

#### A11j. I/O Model: Buffered Everywhere → Three Mechanisms (Correctness / Performance)

**Problem**: Every file operation buffers the entire content in `Vec<u8>`. This affects
every path through the system:

| Operation | Current behavior | Impact |
|-----------|-----------------|--------|
| `fetch_data` (CfApi hydration) | `router.read()` → full file → slice to 64 KiB range | OOM on files >100 MB; kills daemon on 4 GB file |
| `transfer` / `transfer_tree` | `src.read()` → full `Vec<u8>` → `dst.write()` | 2 GB cross-storage move requires 2 GB RAM |
| REST GET (`files.rs`) | `router.read()` → full `Vec<u8>` → `Response` body | Unbounded memory per concurrent request |
| REST PUT (`files.rs`) | Axum `Bytes` extractor → `router.write()` | Already buffered by Axum; acceptable for now |
| Proxy forwarding | `resp.bytes()` → full body → forward | Proxied 4 GB file buffered on intermediate stone |

The `fetch_data` path is a **functional correctness issue** — a 4 GB file read into
`Vec<u8>` on a 4 GB RAM stone kills the daemon. The transfer path is a scalability
constraint. The REST paths are silent memory bombs under concurrent load.

**Encryption constraint**: `ContentStore` supports optional ChaCha20-Poly1305 AEAD
encryption (whole-file encrypt/decrypt). AEAD requires reading and authenticating the
entire ciphertext before any plaintext byte is available. Streaming through encrypted
content is impossible without re-architecting to segmented encryption (1 MB segments
with independent nonce+tag). This is a separate project — not in scope here.

Consequence: encrypted storages must continue to buffer entire files. The fix is to
stream unencrypted paths and range-read where appropriate, while accepting that
encrypted paths remain buffered.

**Fix — three mechanisms, not one**:

| Mechanism | When | Why |
|-----------|------|-----|
| **Ranged read** | CfApi `fetch_data` | CfApi requests bounded ranges (64 KiB–1 MB). Seek + read_exact is natural. |
| **Streaming** | transfer, ingest, REST GET/PUT, proxy | Unbounded size, must not buffer. |
| **Buffered** | Encrypted content, metadata, listings, small files | AEAD constraint, or small payloads. |

Ranged read (local): `tokio::fs::File::seek(SeekFrom::Start(offset))` + `read_exact`.
Ranged read (remote): HTTP `Range: bytes={start}-{end}` header. The REST GET handler
must honor `Range` headers (it doesn't currently — a gap in `files.rs`).

Streaming read (local): Open file → return `impl AsyncRead`. For unencrypted stores,
this is `tokio::fs::File`. For encrypted stores, fall back to buffered read + cursor.
Streaming read (remote): `reqwest::Response::bytes_stream()` → `impl Stream<Item=Bytes>`.

Streaming write (local): `impl AsyncRead` → `tokio::io::copy` to file handle.
Streaming write (remote): `reqwest::Body::wrap_stream()` for chunked upload.

Streaming response (REST GET): `axum::body::Body::from_stream()` instead of buffering
the entire file into the response body.

**`StorageHandle` API surface** (replaces A10's file operations):

```rust
impl StorageHandle {
    // Ranged read — CfApi hydration, partial content
    async fn read_range(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>>;

    // Streaming read — REST GET, transfer source, proxy forwarding
    async fn open_read(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>>;

    // Streaming write — REST PUT, transfer destination
    async fn open_write(&self, path: &str, source: impl AsyncRead + Send) -> Result<u64>;

    // Buffered read — encrypted content, small files, metadata
    async fn read_file(&self, path: &str) -> Result<Vec<u8>>;

    // Buffered write — encrypted content, small files
    async fn write_file(&self, path: &str, data: &[u8]) -> Result<()>;

    // ... list, rename, mkdir, metadata, exists unchanged
}
```

The handle knows whether the storage is encrypted. `open_read` on an encrypted store
falls back to `read_file` + `Cursor::new()` internally — the caller doesn't branch.
`read_range` on an encrypted store reads + decrypts the full file and slices (same as
today, but the caller expresses intent; the handle can optimize later when segmented
encryption lands).

**Transfer using streams** (replaces buffered transfer):

```rust
pub async fn transfer(src: &StorageHandle, src_path: &str, dst: &StorageHandle, dst_path: &str) -> Result<u64> {
    let reader = src.open_read(src_path).await?;
    dst.open_write(dst_path, reader).await
}
```

Zero full-file buffers for unencrypted-to-unencrypted transfers. Encrypted paths
fall back to buffered internally — the composition is transparent.

**REST GET using streaming response**:

```rust
async fn get_file_v1(..) -> Response {
    let reader = handle.open_read(path).await?;
    let stream = ReaderStream::new(reader);
    Response::builder()
        .header(CONTENT_TYPE, mime)
        .body(Body::from_stream(stream))
}
```

No `Vec<u8>` allocation for the response body. The file streams directly from disk
(or from the remote stone's response body) to the HTTP client.

**Priority**: Critical. `read_range` for `fetch_data` is the minimum viable fix —
without it, any file >100 MB risks OOM. Streaming for transfer and REST is high
priority but can follow.

#### A11k. No Timeout on Remote HTTP Calls → Layered Timeout Model (Reliability)

**Problem**: `http_client()` in `router.rs:505` creates a `reqwest::Client` with no
`.timeout()`. A hung remote stone blocks the calling thread indefinitely. For CfApi
callbacks, this freezes the Explorer UI — Windows eventually kills the provider.

Other `reqwest::Client` instances in the codebase set timeouts (e.g., MongoDB
orchestrator uses 30s/60s). The router's client is the only one without.

**Naive fix rejected**: A flat `.timeout(30s)` kills legitimate large transfers — a
2 GB file over a 100 Mbps LAN link takes ~160 seconds. The total-timeout model is
wrong for streaming operations.

**Correct fix — layered timeouts**:

| Timeout | Value | Applies to | Purpose |
|---------|-------|------------|---------|
| **Connect** | 10s | All requests | TCP + TLS handshake stall |
| **Stall** | 30s | Streaming ops | No bytes for 30s → abort (hung disk, frozen remote) |
| **Total** | 30s | Metadata ops | HEAD, list, exists, mkdir — always fast |
| **Total** | None | Streaming ops | Large transfers take as long as they take |

`reqwest` supports `.connect_timeout()` (connect phase only) and `.timeout()` (total
request). It does not support a native stall/inactivity timeout.

For streaming reads, the stall timeout wraps each chunk:
```rust
loop {
    match tokio::time::timeout(STALL_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(chunk))) => { /* process */ }
        Ok(Some(Err(e)))    => return Err(e),
        Ok(None)            => break, // stream complete
        Err(_)              => bail!("remote stalled for {STALL_TIMEOUT:?}"),
    }
}
```

For non-streaming operations (metadata, list, exists), the total timeout is fine.

**Implementation on shared client**:

```rust
static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .danger_accept_invalid_certs(true) // Pond mTLS — documented decision
        .pool_max_idle_per_host(4)
        .build()
        .unwrap_or_default()
});
```

No `.timeout()` on the client — total timeout is per-call for metadata ops, stall
timeout is per-chunk for streaming ops. Absorbed by A10 (shared client on handle).

**Interim fix**: Add `.connect_timeout(10s)` to the current `http_client()` — one-line
change, catches the worst case (unreachable stone), doesn't break large transfers.

**Priority**: Interim fix now (one-line); full layered model with A10.

#### A11l. Recursive Tree Operations Without Depth Limit (Robustness)

**Problem**: `transfer_tree` and `ingest_tree` in `router.rs` recurse via `Box::pin`
with no depth limit. A deeply nested directory structure (malicious or accidental —
e.g., symlink loops on the source, or filesystem corruption) causes unbounded recursion.
Each level adds a future + stack frame. At ~1000 levels, the task overflows.

**Fix**: Add a `max_depth` constant (`MAX_TREE_DEPTH = 64`). Return error at depth
limit. For `ingest_tree` (reads from arbitrary filesystem paths), also skip symlinks
to prevent loops.

**Priority**: Fix if touching the file. Unlikely in practice (user-facing storage
rarely exceeds 20 levels), but a robustness gap.

#### A11m. Cross-Storage Move Non-Atomicity (Known Constraint)

**Problem**: Cross-storage move in `provider.rs:504-522` does
`transfer (or transfer_tree) → delete source`. If the process crashes after partial
copy but before delete, both storages have partial data. No rollback.

**Reviewed assessment**: This is inherent to cross-storage operations when the two
storages may be on different physical devices (or different stones entirely). True
atomicity requires a two-phase commit protocol or a journal — disproportionate
complexity for a file move.

**Mitigation (not a fix)**: Log the operation start and completion. On startup, scan
for incomplete cross-storage moves (journal file in `.zen-garden/`) and offer cleanup.
This is a future enhancement, not a prerequisite.

**Priority**: Document as known constraint. Same category as A8.

### Resolution Priority

| # | Issue | Severity | Fix | Status |
|---|-------|----------|-----|--------|
| A2 | Remote directory rename | **Correctness** | `is_dir` on `RenameInStorage` + `router.rename` | **Done** |
| A7/A11b | Fragile `is_blocked_path` | **Correctness** | Split-and-check rewrite | **Done** |
| A11a | String-based error detection | **Correctness** | `RouterError` with 2 variants | **Done** |
| A11f | `has_path_traversal` duplicated | **Security** | Extract to `garden_common` | **Done** |
| A11c | Stray purge races with ingest | **Data loss** window | Reduce purge to heartbeat-only (option B) | **Done** |
| A11d | Display name fallback scattered | Divergence risk | `Management::display_name()` method | **Done** |
| A11i | Identical `find_remote*` functions | Clarity | Collapse to one | **Done** |
| A11j | Buffered I/O everywhere | **Correctness** — OOM | Three-mechanism I/O: `read_range` (CfApi), streaming read/write (`open_read`, `write_from_reader`), buffered fallback (encrypted). REST GET, WebDAV proxy, `transfer`, and `ingest` all stream. | **Done** |
| A11k | No HTTP timeout | **Reliability** | Connect 10s + per-call metadata 30s + pool tuning | **Done** |
| A11l | Recursive tree ops without depth limit | Robustness | `MAX_TREE_DEPTH = 64`, skip symlinks | **Done** |
| A11e | `is_known_storage` full enumeration | Minor performance | Short-circuit via `find_local` | **Done** |
| A11g | Magic hex constants | Readability | Named constants in `ingest.rs` | **Done** |
| A11h | Remote list silent empty | Resilience | Structured `error!` logging per parse stage | **Done** |
| A5 | Per-call HTTP client | Performance | `OnceLock` singleton, `pool_max_idle_per_host(4)` | **Done** |
| A1 | Replication ticks lost | **Functional** | `StorageHandle` carries tick, writes use `notifying_content_store` | **Done** |
| A3 | `do_rename_storage` in adapter | Architectural | `rename_replica_set()` extracted to `storage_service.rs` | **Done** |
| A4 | Missing copy operations | Feature gap | `copy_file`, `copy_tree` on `StorageHandle` | **Done** |
| A6 | Dead provider fields | Cleanup | `tick` wired via resolver; `local_endpoint` removed | **Done** |
| A9 | 15 remaining dispatch sites | Architectural | All migrated to `StorageHandle` via `StorageResolver` | **Done** |
| A10 | StorageHandle consolidation | Architectural | `StorageHandle` + `StorageResolver` replace `StorageRouter`. Absorbs A1, A3–A6, A9. Streaming I/O (A11j) deferred to separate effort. | **Done** |
| A8 | Blocking I/O in ingest | Known constraint | Accepted limitation | N/A |
| A11m | Cross-storage move non-atomic | Known constraint | Document only; journal as future enhancement | N/A |
