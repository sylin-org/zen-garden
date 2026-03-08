---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-07
---

# STORAGE-0009: Managed Storage and File Sharing

**Date**: 2026-03-07
**Status**: Accepted
**Supersedes**: STORAGE-0002 (fully), STORAGE-0008 (API surface)
**Evolves**: STORAGE-0007 (lifecycle objects)
**Depends on**: STORAGE-0005 (Manifest-First), STORAGE-0006 (Replication), STORAGE-0007 (Lifecycle Objects)

## Context

Storage in Zen Garden was conflated with a single purpose: backup. Every managed storage device was a seed bank, every seed bank received offering backups, and users could not interact with stored content except through REST APIs. Three gaps drove this decision:

1. **No visibility.** Users could not see what offerings had stored, what harvests existed, or how much space was used — without CLI commands or API calls.
2. **No personal storage.** There was no place for user files (documents, photos, media) within the garden's storage fabric.
3. **No native file access.** Content was only accessible through REST APIs. No way to browse from a file explorer on any platform.

A deeper structural problem: **storage and seed bank were the same concept.** There was no way to have a NAS for family photos managed by Zen Garden (replication, health monitoring, file sharing) without it also becoming a target for MongoDB snapshots.

The API surface had also grown organically into four overlapping access paths to the same content:

```
/api/v1/stone/storage/bank/{id}/*path    → Stone-local, read-only, by bank ID
/api/v1/storage/s3/{bucket}/*key         → S3-compatible XML, bank by header/query
/api/v1/storage/{*path}                  → REST JSON, bank by header/query
/api/v1/garden/storage/{name}/*path      → Name-based, Primary-or-proxy routing
```

Each handler independently resolved seed banks, checked roles, decided whether to proxy, and called into `SeedBankStore` or `ObjectStore`. The "find bank, check role, proxy or execute" pattern was reimplemented four times.

## Decision

### 1. Storage as entity, seed bank as role

**Storage** (renamed from `SeedBank`) is the universal managed entity: any filesystem that Zen Garden manages. It has a name, an adapter (USB, NAS, local path), a mount point, health monitoring, replication policy, and the `.zen-garden/` dotfolder.

**Seed bank** becomes a composable role — a set of behaviors a storage opts into. When a storage has the `seed-bank` role, it receives offering harvests from nurturing cycles. When it doesn't, it's still fully managed — replication, WebDAV access, Cloud Filter integration — it just doesn't receive platform backups.

Roles are flags in the manifest:

```json
{
  "name": "zen-garden",
  "roles": ["seed-bank"],
  "replication": { "policy": "all-stones" }
}
```

```json
{
  "name": "personal",
  "roles": [],
  "replication": { "policy": "all-stones" }
}
```

Both storages replicate. Both are accessible via WebDAV and Cloud Filter. Both have health monitoring. The only difference: `zen-garden` receives offering backups in `.zen-garden/memories/`; `personal` doesn't.

Roles are composable. Future roles can be added without rearchitecting — `archive` (write-once, no deletes), `cache` (ephemeral, no replication), `shared` (multi-stone write access). The storage entity stays the same; roles compose behaviors onto it.

Replication and roles are orthogonal. A personal NAS replicates to all stones without the seed-bank role. Replication policy is configured independently of role assignment.

### 2. `.zen-garden/` dotfolder layout

The `garden/` top-level directory was replaced by a `.zen-garden/` dotfolder. All Zen Garden managed data moved into the dotfolder, leaving the mount root as user space:

```
/mnt/storage/
├── .zen-garden/                # Zen Garden managed (hidden by default)
│   ├── manifest.json           # Identity, roles, replication policy
│   ├── changelog.jsonl         # Replication log
│   ├── pin.json                # Primary claim
│   ├── last_cursor             # Replication cursor
│   ├── last-known-good/        # Resilience snapshot
│   │   ├── manifest.json
│   │   ├── pin.json
│   │   └── last_cursor
│   ├── memories/               # Offering backups (only if seed-bank role)
│   │   └── {offering}/{harvest}.tar.gz
│   └── storage/                # S3 object store (API-managed)
│       └── {bucket}/{key}
│
├── Zen Garden →                # Symlink to .zen-garden/ (user-togglable)
│
├── Photos/                     # User content
├── Documents/                  # User content
└── anything-else/              # User content
```

Design principles:

- **The filesystem is the user's.** The root of a managed storage mount belongs to the user.
- **Full transparency.** Users can browse, edit, and manage all Zen Garden internals via the symlink. The system is robust against mistakes (`last-known-good/`), not locked against access.
- **`last-known-good/`** contains copies of critical structural files (manifest, pin, cursor). Updated at known-safe moments (successful mount, replication cycle, clean health tick). Used for automatic recovery when corruption is detected.
- **`Zen Garden` symlink** at the root provides visibility into managed data. Present by default. Users can delete it; Moss does not recreate it unless asked.

### 3. Domain-driven storage architecture

The storage layer was restructured around proper domain/infra/API separation with a single domain entry point:

```
ACCESS PROTOCOLS (thin handlers — own serialization format only)
  WebDAV handler    REST handler    S3 handler    Cloud Filter
  /dav/{name}/      /garden/...     /storage/s3   (Windows)
        │                │               │              │
        └────────────────┴───────────────┴──────────────┘
                             │
DOMAIN (business logic — pure, no I/O)
                             ▼
  StorageService
    resolve(name) → ManagedStorage
    route(name) → Local | Proxy(endpoint) | LocalReadOnly
    read_file / write_file / delete_file
    read_object / write_object / delete_object
    list_memories
    adopt / prepare

    Owns: routing decisions, role checks, changelog policy, replication triggers
                             │
             ┌───────────────┼───────────────┐
             ▼               ▼               ▼
    FileStore         ObjectStore       MemoryStore
    (user files)      (S3 objects)      (harvests)
                             │
INFRASTRUCTURE (I/O — filesystem, network, encryption)

  ContentStore (I/O chokepoint)
    ├── read/write with optional encryption
    ├── changelog append
    └── atomic writes (tmp + rename)

  StorageAdapter (device lifecycle)
    ├── UsbAdapter
    ├── NasAdapter
    └── PathAdapter

  StorageProxy (remote forwarding)
    └── HTTP client to remote stone
```

#### Domain renames

| Before | After | Rationale |
|--------|-------|-----------|
| `SeedBank` | `ManagedStorage` | Storage is the entity; seed bank is a role |
| `SeedBankStore` | `ContentStore` | I/O chokepoint serves all content types, not just seed bank data |
| `SeedBankRegistry` | Absorbed into `StorageService` | Single entry point for resolution and routing |

#### Key structural changes

1. **`StorageService` is the single domain entry point.** All four access protocols (WebDAV, REST, S3, Cloud Filter) call the same service. Routing logic (local vs proxy), role checks (Primary vs Dormant), and changelog policy live here once — not reimplemented per handler.

2. **Three content stores replace the monolithic `ObjectStore`.** `FileStore` handles user content at the storage root. `ObjectStore` handles S3 objects under `.zen-garden/storage/`. `MemoryStore` handles harvests under `.zen-garden/memories/` (only for storages with the `seed-bank` role). All three delegate I/O to `ContentStore`.

3. **`StorageProxy` is extracted from API handlers.** Each handler previously had its own proxy-to-remote logic (build URL, forward request, check `X-Zen-Proxied`). This became a single infra component that `StorageService` delegates to when routing decides "not local."

4. **API handlers became thin.** A WebDAV `PUT` handler extracts the path and body, calls `storage_service.write_file(name, path, bytes)`, maps the result to a WebDAV response. Handlers own serialization format; the service owns behavior.

5. **Storage adapters are a proper trait.** The `StorageDevice` + `SeedBankRegistry` mix of lifecycle management and device detection was separated. "How do I mount this?" (adapter) is distinct from "what do I do with it once mounted?" (domain service).

#### Trait definitions

```rust
/// Domain service — single entry point for all storage operations.
trait StorageOperations: Send + Sync {
    async fn resolve(&self, name: &str) -> Result<StorageRef>;
    async fn read_file(&self, name: &str, path: &str) -> Result<FileContent>;
    async fn write_file(&self, name: &str, path: &str, content: Bytes) -> Result<WriteResult>;
    async fn delete_file(&self, name: &str, path: &str) -> Result<()>;
    async fn read_object(&self, name: &str, bucket: &str, key: &str) -> Result<FileContent>;
    async fn write_object(&self, name: &str, bucket: &str, key: &str, content: Bytes) -> Result<WriteResult>;
    async fn delete_object(&self, name: &str, bucket: &str, key: &str) -> Result<()>;
    async fn list_memories(&self, name: &str) -> Result<Vec<HarvestSummary>>;
}

/// Routing decision — returned by StorageService after resolving.
enum StorageRoute {
    Local { storage: ManagedStorage },
    Proxy { endpoint: String },
    LocalReadOnly { storage: ManagedStorage, primary_endpoint: String },
}

/// Device lifecycle abstraction.
trait StorageAdapter: Send + Sync {
    async fn is_available(&self) -> bool;
    async fn mount(&self) -> Result<PathBuf>;
    async fn unmount(&self) -> Result<()>;
    async fn health_check(&self) -> HealthStatus;
    fn adapter_type(&self) -> AdapterType;
}
```

Adding a new access protocol (e.g., a future NFS server, or a FUSE mount) requires only a new thin handler that calls `StorageService`. No routing logic, no role checks, no changelog management.

#### Storage adapters

| Adapter | Lifecycle | Discovery |
|---------|-----------|-----------|
| **USB** | Hot-plug detection, auto-mount, stale cleanup | Device scan (existing) |
| **NAS (NFS/SMB)** | Persistent mount, fstab/autofs, reconnect on failure | Configuration-based |
| **Local path** | Always available, no mount/unmount | Configuration-based |

All adapters produce the same `ManagedStorage` domain object. The rest of the system — replication, APIs, file sharing — is adapter-agnostic.

#### Adopting populated storage

A new `adopt` flow allows bringing existing storage into the garden without formatting:

```bash
garden-rake storage adopt /mnt/nas-volume --name family-nas
garden-rake storage adopt /mnt/usb --name archive --roles seed-bank
```

This creates `.zen-garden/` and writes `manifest.json`, creates the `Zen Garden` symlink, and leaves all existing files untouched. The existing `prepare` flow remains for blank devices.

### 4. API consolidation

Four overlapping access paths were consolidated to three purpose-driven surfaces plus WebDAV:

#### Garden-tier storage (reworked)

The garden storage API became the single REST entry point for all content — user files, S3 objects, and harvest metadata. Primary-or-proxy routing preserved from STORAGE-0008.

```
GET    /api/v1/garden/storage                           → List all known storages
GET    /api/v1/garden/storage/{name}                    → Storage details + discovery
GET    /api/v1/garden/storage/{name}/files/*path         → Read user file
PUT    /api/v1/garden/storage/{name}/files/*path         → Write user file
DELETE /api/v1/garden/storage/{name}/files/*path         → Delete user file
HEAD   /api/v1/garden/storage/{name}/files/*path         → File metadata
GET    /api/v1/garden/storage/{name}/objects/*path       → Read S3 object
PUT    /api/v1/garden/storage/{name}/objects/*path       → Write S3 object
DELETE /api/v1/garden/storage/{name}/objects/*path       → Delete S3 object
HEAD   /api/v1/garden/storage/{name}/objects/*path       → Object metadata
GET    /api/v1/garden/storage/{name}/memories            → List harvests
GET    /api/v1/garden/storage/{name}/memories/*path      → Read harvest artifact
```

Key changes from STORAGE-0008: explicit `/files/`, `/objects/`, `/memories/` namespaces replace the undifferentiated `/*path`. The `/files/` namespace exposes user content at the storage root — previously no API covered user files.

#### Stone-tier storage (simplified)

Local admin operations only. Object read/write moved to garden-tier and WebDAV. Routes switched from bank ID to bank name.

```
GET    /api/v1/stone/storage                            → Overview
GET    /api/v1/stone/storage/health                     → Health status
GET    /api/v1/stone/storage/candidates                 → Eligible devices
POST   /api/v1/stone/storage/prepare                    → Format blank device
POST   /api/v1/stone/storage/adopt                      → Adopt populated storage
GET    /api/v1/stone/storage/banks                      → List local storages
GET    /api/v1/stone/storage/banks/{name}               → Storage details
DELETE /api/v1/stone/storage/banks/{name}               → Remove storage
POST   /api/v1/stone/storage/banks/{name}/release       → Unmount
POST   /api/v1/stone/storage/banks/{name}/pin           → Claim Primary
POST   /api/v1/stone/storage/banks/{name}/unpin         → Release Primary
PATCH  /api/v1/stone/storage/banks/{name}/visibility    → Set visibility
PATCH  /api/v1/stone/storage/banks/{name}/rename        → Rename
PATCH  /api/v1/stone/storage/banks/{name}/roles         → Set roles
GET    /api/v1/stone/storage/banks/{name}/changes       → Replication changelog
GET    /api/v1/stone/storage/stream                     → SSE replication stream
```

#### S3 gateway (preserved, scoped)

Unchanged in shape. Backend shifted: objects stored under `.zen-garden/storage/` instead of `garden/storage/`. `X-Seed-Bank` header preserved — S3 clients expect a flat endpoint. This gateway accesses only the S3 object namespace.

#### REST gateway removed

The `/api/v1/storage/{*path}` REST gateway (JSON responses, `X-Seed-Bank` selection) was removed entirely. Its functionality was absorbed by the reworked garden-tier `/api/v1/garden/storage/{name}/objects/` routes. Migration is mechanical: add the storage name to the path, drop the `X-Seed-Bank` header.

#### Migration path

| Before | After | Notes |
|--------|-------|-------|
| `GET /api/v1/stone/storage/bank/{id}/*path` | `GET /api/v1/garden/storage/{name}/files/*path` | ID → name, stone → garden |
| `GET /api/v1/storage/{*path}` | `GET /api/v1/garden/storage/{name}/objects/*path` | Header → path-based selection |
| `PUT /api/v1/storage/{*path}` | `PUT /api/v1/garden/storage/{name}/objects/*path` | Same |
| `GET /api/v1/storage/s3/{bucket}/*key` | Unchanged | S3 gateway preserved |
| `GET /api/v1/garden/storage/{name}/*path` | Split into `/files/` and `/objects/` | Explicit namespace |

### 5. File sharing protocols

#### WebDAV

Moss serves WebDAV endpoints for each locally-hosted storage, using the `dav-server` Rust crate integrated into Moss's existing axum/hyper HTTP stack.

- **Route**: `/dav/{name}/`
- **Backend**: Custom `DavFileSystem` implementation backed by the storage mount
- **Capabilities**: Full read/write, RFC 4918 compliant (PROPFIND, GET, PUT, DELETE, MKCOL, COPY, MOVE)
- **Lock support**: `FakeLs` (sufficient for macOS/Windows single-user)
- **Port**: Existing Moss HTTP port (7185)
- **Changelog**: Writes through WebDAV go through Moss, which records changelog entries for replication coherence
- **Routing**: When a WebDAV request arrives for a storage hosted on another stone, Moss proxies to the hosting stone — same Primary-or-proxy pattern

| Platform | Connection method |
|----------|-------------------|
| macOS | Finder > Connect to Server > `http://stone-name.local:7185/dav/personal/` |
| Linux | File manager > Connect to Server, or `davfs2` mount |
| Windows | Map Network Drive, or Cloud Filter (preferred) |

#### Cloud Filter API (Windows)

On Windows, Moss registers as a Cloud Sync Provider using the `cloud-filter` Rust crate. Storage appears natively in Explorer without WebDAV or SMB dependencies.

- On startup, Windows Moss registers a "Zen Garden" sync root in Explorer's navigation pane.
- As storage beacons arrive, each discovered storage appears as a folder under the sync root.
- Files are fetched on demand from the hosting stone's storage API. Placeholder files show directory structure without downloading content.
- Saves push back through the storage API to the Primary stone.
- When a storage moves between stones (unplug from stone-A, plug into stone-B), beacon updates route transparently. The Explorer folder stays the same.

No separate client app — Moss itself is the Cloud Filter provider. A Windows stone with local storage serves files directly (no network round-trip). A Windows stone without local storage proxies to the hosting stone.

### 6. Discovery and namespace

The garden-wide namespace is `zen-garden\{storage-name}`:

```
zen-garden\
├── personal        → wherever "personal" is hosted
├── platform        → wherever "platform" is hosted
└── archive         → wherever "archive" lives
```

Storage identity is stable — file shares follow the storage name, not the stone it's plugged into. `\\zen-garden\personal` works regardless of which stone hosts the drive.

- **Windows (Cloud Filter)**: Appears natively in Explorer. Storage appears/disappears as beacons flow.
- **macOS/Linux (WebDAV)**: The tending stone can serve as a stable entry point, proxying to the actual host.
- **SMB signpost share (optional)**: A lightweight Samba share on the tending stone serves as a billboard — `.url` shortcuts pointing to WebDAV endpoints. Discovery only, not file serving.

## Consequences

### Positive

- **User agency over storage.** Users browse, edit, and manage files through their desktop file explorer. The filesystem belongs to them; Zen Garden bookkeeping is tucked away.
- **Personal storage without backup coupling.** A NAS for family photos gets replication and health monitoring without receiving MongoDB snapshots.
- **One domain entry point.** `StorageService` replaces four independent handler implementations. Routing, role checks, and changelog policy are written once.
- **Extensible protocols.** Adding a new access method (NFS server, FUSE mount) requires only a thin handler calling `StorageService`.
- **Extensible media.** Adding NAS or local path support requires only a new `StorageAdapter` implementation.
- **Clear API audiences.** WebDAV for humans, garden-tier REST for internal services, S3 for apps. No more implicit seed bank selection via headers.
- **Windows-native integration.** Cloud Filter API gives Explorer integration without deprecated WebClient dependencies.

### Negative

- **Migration effort.** Domain rename (`SeedBank` → `ManagedStorage`), filesystem layout change (`garden/` → `.zen-garden/`), API surface rework, and consumer migration all need coordination.
- **Breaking changes.** REST gateway consumers, stone-tier object read consumers, and any code using seed bank IDs in routes must migrate.
- **New dependencies.** `dav-server` crate for WebDAV, `cloud-filter` crate for Windows Cloud Filter. Both are external dependencies with their own maintenance lifecycle.
- **Changelog coherence gap for direct writes.** Files written via local filesystem access bypass Moss. Detection via `inotify`/periodic scan adds latency before replication catches up.

### Neutral

- **S3 gateway preserved.** The S3-compatible surface is unchanged in shape, only shifted internally to `.zen-garden/storage/`.
- **Existing storages migrate transparently.** All current seed banks gain `roles: ["seed-bank"]` during migration — backward-compatible default.
- **`StorageCache` (beacon topology) remains separate.** It tracks remote stones' storage via beacons — external state, not local lifecycle.

## Delivery Phases

1. **Foundation** — Domain rename, manifest `roles` array, `.zen-garden/` layout with `last-known-good/`, `Zen Garden` symlink, `adopt` command, adapter trait + USB adapter, `StorageService` domain service, migration path for existing seed banks
2. **API rework** — Garden-tier `/files/`, `/objects/`, `/memories/` namespaces; stone-tier ID → name switch; REST gateway deprecation; consumer migration
3. **WebDAV** — `dav-server` integration, custom `DavFileSystem` backend, changelog recording for writes, proxy routing for remote storage, REST gateway removal
4. **Cloud Filter (Windows)** — `cloud-filter` integration, sync root registration, beacon-driven discovery, on-demand fetch/push
5. **Extended adapters** — NAS adapter (NFS/SMB), local path adapter, filesystem watchers for non-API writes
6. **Discovery polish** — SMB signpost share, `garden-rake storage status` with space breakdown, initial content catalog on adopt

## Open Questions

- **Lock semantics**: `dav-server`'s `FakeLs` is sufficient for single-user. Multi-user concurrent write conflicts need a policy if household members access the same storage via WebDAV.
- **Cloud Filter offline behavior**: When the hosting stone is unreachable, should placeholder files show an error state or cache last-known content?
- **Symlink on Windows**: `Zen Garden → .zen-garden/` symlinks require elevated privileges or Developer Mode on NTFS. May need junction points instead, or skip the symlink on Windows (Cloud Filter renders the content anyway).
- **Filesystem watcher scope**: Should `inotify`/`fanotify` watch the entire storage root (user content for replication) or only `.zen-garden/` (managed content coherence)? Large trees have performance implications.

## Related

- [Managed Storage and File Sharing Proposal](../proposals/seed-bank-file-sharing.md) — full design exploration with alternatives considered
- [STORAGE-0002](STORAGE-0002-api-structure.md) — original API structure (superseded)
- [STORAGE-0005](STORAGE-0005-manifest-first-discovery.md) — manifest-first discovery (`.zen-garden/manifest.json` as source of truth)
- [STORAGE-0006](STORAGE-0006-seed-bank-replication.md) — replication, Primary/Dormant roles, changelog-driven sync
- [STORAGE-0007](STORAGE-0007-storage-lifecycle-objects.md) — lifecycle objects (`SeedBank` + `Storage` composition, evolved by this ADR)
- [STORAGE-0008](STORAGE-0008-garden-stone-api-split.md) — garden/stone API split (API surface superseded by this ADR)
