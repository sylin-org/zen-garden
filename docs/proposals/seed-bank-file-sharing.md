---
audience: [contributor, operator]
doc_type: proposal
status: draft
last_verified: 2026-03-07
---

# Managed Storage and File Sharing

**Author**: Design session (Leon + Claude)
**Date**: 2026-03-07
**Complements**: Storage Capability Model, Seed Bank Replication (STORAGE-0006)
**Depends on**: Storage Adapters, WebDAV integration

---

## Problem Statement

Storage in Zen Garden is conflated with a single purpose: backup. "Seed banks" are opaque, Moss-managed backup devices. Users cannot browse what's stored, cannot place their own files alongside offering data, and cannot access content from their desktop. This creates a paradox: Zen Garden has a fully-featured storage layer with replication, encryption, and health monitoring — but it only serves one use case, and the user is outside the loop.

Three specific gaps:

1. **No visibility.** Users cannot see what offerings have stored, what harvests exist, or how much space is used — without CLI commands or API calls.
2. **No personal storage.** There is no place for user files (documents, photos, media) within the garden's storage fabric.
3. **No native file access.** Content is only accessible through REST APIs. There is no way to browse content from a file explorer on any platform.

A deeper structural problem: **storage and seed bank are the same concept today.** Every managed storage device is a seed bank. Every seed bank receives offering backups. There's no way to have a NAS for family photos that's managed by Zen Garden (replication, health monitoring, file sharing) without it also becoming a dump target for MongoDB snapshots.

## Proposed Solution

Separate **storage** (the managed entity) from **seed bank** (a role that a storage can opt into). Transform managed storage from dedicated backup devices into a **personal storage fabric** — where user content lives alongside (but separated from) managed offering data, accessible from any device in the garden through native file system protocols.

### Storage as entity, seed bank as role

**Storage** is the universal entity: any filesystem that Zen Garden manages. It has a name, an adapter (USB, NAS, local path), a mount point, health monitoring, replication policy, and the `.zen-garden/` dotfolder. All managed storage shares the same infrastructure — changelog, encryption, beacons, Primary/Dormant roles.

**Seed bank** is a role — a set of behaviors that a storage can opt into. When a storage has the `seed-bank` role, it receives offering harvests from nurturing cycles. When it doesn't, it's still fully managed — replication, WebDAV access, Cloud Filter integration — it just doesn't receive platform backups.

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

Both storages replicate. Both are accessible via WebDAV and Cloud Filter. Both have health monitoring. The only difference: `zen-garden` receives offering backups in `.zen-garden/memories/`, `personal` doesn't.

This makes roles composable. Future roles can be added as checkboxes — `archive` (write-once, no deletes), `cache` (ephemeral, no replication), `shared` (multi-stone write access) — without rearchitecting. The storage entity stays the same; roles compose behaviors onto it.

The user configures this through a simple UI:

```
Storage: personal
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Adapter:      NAS (nas.local:/photos)
Status:       Healthy

Roles:
  [x] Seed Bank     Receive offering backups
  [ ] Archive       Write-once, no deletes

Replication:
  (o) All stones
  ( ) Offsite only
  ( ) None

Visibility:
  (o) Open          Visible to all stones
  ( ) Private       Only this stone
```

### Core principles

- **The filesystem is the user's.** The root of a managed storage mount belongs to the user. Zen Garden's bookkeeping lives in a `.zen-garden/` dotfolder.
- **Full transparency.** Users can browse, edit, and manage all Zen Garden internals. The system is robust against mistakes, not locked against access.
- **Storage identity is stable.** File shares follow the storage name, not the stone it's plugged into. `\\zen-garden\personal` works regardless of which stone hosts the drive.
- **Platform-native access.** WebDAV on Linux/macOS, Cloud Filter API on Windows. No protocol bridging, no shims.
- **Roles are opt-in behaviors.** Storage is managed by default. Seed bank (receiving offering backups) is a role, not an identity.

### Filesystem layout

The `.zen-garden/` dotfolder replaces the current `garden/` top-level directory. All Zen Garden managed data moves into the dotfolder, leaving the mount root as user space:

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

**Key details:**

- `.zen-garden/` is hidden on all platforms (dotfolder convention on Unix, DOS hidden attribute via Samba's `hide dot files` on Windows).
- `Zen Garden` symlink at the root provides optional visibility into managed data. Present by default. Users can delete it; Moss does not recreate it unless asked.
- `last-known-good/` contains copies of critical structural files (manifest, pin, cursor). Updated at known-safe moments (successful mount, replication cycle, clean health tick). Used for automatic recovery when corruption is detected.

### Adopting populated storage

A new `adopt` flow allows bringing existing storage into the garden without formatting or data relocation:

```bash
garden-rake storage adopt /mnt/nas-volume --name family-nas
garden-rake storage adopt /dev/sdb1 --name portable-backup
```

This:
1. Creates `.zen-garden/` and writes `manifest.json`
2. Creates the `Zen Garden` symlink
3. Leaves all existing files untouched
4. Optionally performs an initial content scan (baseline catalog)

The existing `prepare` flow remains for blank devices. `adopt` is the non-destructive equivalent.

### Storage adapters

Different storage media have different lifecycle requirements. An adapter trait abstracts the differences:

```rust
trait StorageAdapter: Send + Sync {
    async fn is_available(&self) -> bool;
    async fn mount(&self) -> Result<PathBuf>;
    async fn unmount(&self) -> Result<()>;
    async fn health_check(&self) -> HealthStatus;
    fn adapter_type(&self) -> AdapterType;
}
```

| Adapter | Lifecycle | Discovery |
|---------|-----------|-----------|
| **USB** | Hot-plug detection, auto-mount, stale cleanup | Device scan (existing) |
| **NAS (NFS/SMB)** | Persistent mount, fstab/autofs, reconnect on failure | Configuration-based |
| **Local path** | Always available, no mount/unmount | Configuration-based |

All adapters produce the same `ManagedStorage` domain object. The rest of the system — replication, APIs, file sharing — is adapter-agnostic.

### Architecture: domain-driven storage

The current storage code mixes concerns. API handlers in `storage.rs`, `s3_gateway.rs`, `storage_gateway.rs`, and `garden_storage.rs` each independently resolve seed banks, check roles, decide whether to proxy, and call into `SeedBankStore` or `ObjectStore` directly. The "find bank, check role, proxy or execute" pattern is reimplemented four times.

This proposal restructures storage around proper domain/infra/API separation:

```
┌─────────────────────────────────────────────────────────────────┐
│  ACCESS PROTOCOLS (API layer — thin, no business logic)         │
│                                                                 │
│  WebDAV handler    REST handler    S3 handler    Cloud Filter   │
│  /dav/{name}/      /garden/...     /storage/s3   (Windows)      │
│       │                │               │              │         │
│       └────────────────┴───────────────┴──────────────┘         │
│                            │                                    │
├────────────────────────────┼────────────────────────────────────┤
│  DOMAIN (business logic — pure, no I/O)                         │
│                            ▼                                    │
│  ┌──────────────────────────────────────────┐                   │
│  │  StorageService                          │                   │
│  │                                          │                   │
│  │  resolve(name) → ManagedStorage           │                   │
│  │  route(name) → Local | Proxy(endpoint)   │                   │
│  │  read_file(name, path) → bytes           │                   │
│  │  write_file(name, path, bytes) → result  │                   │
│  │  read_object(name, bucket, key) → bytes  │                   │
│  │  write_object(name, bucket, key) → res   │                   │
│  │  list_memories(name) → harvests          │                   │
│  │  adopt(path, name) → ManagedStorage       │                   │
│  │  prepare(device, name) → ManagedStorage   │                   │
│  │                                          │                   │
│  │  Owns: routing decisions, role checks,   │                   │
│  │  changelog policy, replication triggers   │                   │
│  └──────────────────────────────────────────┘                   │
│                            │                                    │
│            ┌───────────────┼───────────────┐                    │
│            ▼               ▼               ▼                    │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │ FileStore    │  │ ObjectStore  │  │ MemoryStore  │          │
│  │ (user files) │  │ (S3 objects) │  │ (harvests)   │          │
│  │              │  │              │  │              │          │
│  │ Trait-based  │  │ Trait-based  │  │ Trait-based  │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│  INFRASTRUCTURE (I/O — filesystem, network, encryption)         │
│                                                                 │
│  ┌──────────────────────────────────────────┐                   │
│  │  ContentStore (I/O chokepoint)           │                   │
│  │  ├── read/write with optional encryption │                   │
│  │  ├── changelog append                    │                   │
│  │  └── atomic writes (tmp + rename)        │                   │
│  └──────────────────────────────────────────┘                   │
│                                                                 │
│  ┌──────────────────────────────────────────┐                   │
│  │  StorageAdapter (device lifecycle)       │                   │
│  │  ├── UsbAdapter                          │                   │
│  │  ├── NasAdapter                          │                   │
│  │  └── PathAdapter                         │                   │
│  └──────────────────────────────────────────┘                   │
│                                                                 │
│  ┌──────────────────────────────────────────┐                   │
│  │  StorageProxy (remote forwarding)        │                   │
│  │  └── HTTP client to remote stone         │                   │
│  └──────────────────────────────────────────┘                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**What changes from the current design:**

1. **`StorageService` becomes the single domain entry point.** All four access protocols (WebDAV, REST, S3, Cloud Filter) call the same service. Routing logic (local vs proxy), role checks (Primary vs Dormant), and changelog policy live here once — not reimplemented per handler.

2. **Three content stores replace the monolithic `ObjectStore`.** `FileStore` handles user content at the storage root. `ObjectStore` handles S3 objects under `.zen-garden/storage/`. `MemoryStore` handles harvests under `.zen-garden/memories/` (only for storages with the `seed-bank` role). All three delegate I/O to `ContentStore` — the encryption/changelog chokepoint stays.

3. **`StorageProxy` is extracted from API handlers.** Currently each handler has its own proxy-to-remote logic (build URL, forward request, check `X-Zen-Proxied`). This becomes a single infra component that `StorageService` delegates to when routing decides "not local."

4. **API handlers become thin.** A WebDAV `PUT` handler extracts the path and body, calls `storage_service.write_file(name, path, bytes)`, maps the result to a WebDAV response. An S3 `PutObject` handler does the same but calls `storage_service.write_object(name, bucket, key, bytes)` and maps to XML. The handlers own serialization format, the service owns behavior.

5. **Storage adapters are a proper trait.** The current `StorageDevice` + `SeedBankRegistry` mix lifecycle management with device detection. The adapter trait separates "how do I mount this?" from "what do I do with it once mounted?" USB hot-plug detection, NAS reconnection, and local path validation each implement the same interface.

**Trait definitions:**

```rust
/// Domain service — single entry point for all storage operations.
/// Owns routing, role checks, changelog policy.
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

/// Routing decision — returned by StorageService after resolving a managed storage.
enum StorageRoute {
    /// Storage is local and Primary — execute directly.
    Local { storage: ManagedStorage },
    /// Storage is remote — proxy to this endpoint.
    Proxy { endpoint: String },
    /// Storage exists locally but is Dormant — read OK, write must proxy.
    LocalReadOnly { storage: ManagedStorage, primary_endpoint: String },
}
```

This design means adding a new access protocol (e.g., a future NFS server, or a FUSE mount) requires only a new thin handler that calls `StorageService`. No routing logic, no role checks, no changelog management — that's all done once in the domain layer.

### File sharing: WebDAV

Moss serves WebDAV endpoints for each locally-hosted seed bank, using the `dav-server` Rust crate integrated into Moss's existing axum/hyper HTTP stack.

**Route:** `/dav/{seed-bank-name}/`

**Filesystem backend:** A custom `DavFileSystem` implementation backed by the seed bank mount. User content is served directly from the filesystem. Writes flow through Moss, which records changelog entries for replication coherence.

**Capabilities:**
- Full read/write access to the seed bank root (user content + `.zen-garden/` via symlink)
- RFC 4918 compliant (PROPFIND, GET, PUT, DELETE, MKCOL, COPY, MOVE)
- Lock support via `FakeLs` (sufficient for macOS/Windows single-user)
- Served on the existing Moss HTTP port (7185)

**Platform mapping:**

| Platform | How users connect |
|----------|-------------------|
| macOS | Finder > Connect to Server > `http://stone-name.local:7185/dav/personal/` |
| Linux | File manager > Connect to Server, or `davfs2` mount |
| Windows | Map Network Drive (while WebClient service exists), or Cloud Filter (preferred) |

**Routing for remote seed banks:** When a WebDAV request arrives for a seed bank hosted on another stone, Moss proxies the request to the hosting stone — the same Primary-or-proxy pattern used by garden storage APIs today. Any stone is an entry point; the seed bank name resolves to wherever the Primary currently lives.

### File sharing: Cloud Filter API (Windows)

On Windows, Moss registers as a Cloud Sync Provider using the `cloud-filter` Rust crate. This makes seed banks appear natively in Explorer without WebDAV or SMB dependencies.

**Behavior:**

- On startup, Windows-running Moss registers a "Zen Garden" sync root in Explorer's navigation pane.
- As storage beacons arrive, each discovered seed bank appears as a folder under the sync root.
- Files are fetched on demand from the hosting stone's storage API. Placeholder files show directory structure without downloading content.
- Saves push back through the storage API to the Primary stone.
- When a seed bank moves between stones (unplug from stone-A, plug into stone-B), the beacon updates route transparently. The Explorer folder stays the same.

**Architecture:** No separate client app. Moss itself is the Cloud Filter provider. A Windows stone with a local seed bank serves files directly (no network round-trip). A Windows stone without local storage proxies to the hosting stone — same APIs, same auth, same replication.

### Discovery and namespace

The garden-wide namespace is `zen-garden\{seed-bank-name}`:

```
zen-garden\
├── personal        → wherever "personal" is plugged in
├── platform        → wherever "platform" is hosted
└── archive         → wherever "archive" lives
```

**On Windows (Cloud Filter):** Appears natively in Explorer's navigation pane. Seed banks appear/disappear as beacons flow. Zero configuration.

**On macOS/Linux (WebDAV):** Users connect once per seed bank. The tending stone can serve as a stable entry point — `http://zen-garden.local:7185/dav/personal/` — proxying to the actual host.

**SMB signpost share (optional):** For environments that want zero-config discovery across all platforms, a lightweight Samba share on the tending stone can serve as a billboard — a directory of `.url` shortcuts pointing to WebDAV endpoints for each seed bank. The share auto-appears in network browsers via mDNS (`_smb._tcp.local`). Samba's role is discovery only, not file serving.

### Replication of user content

User content (files at the seed bank root, outside `.zen-garden/`) follows the same replication model as managed content:

- **Changelog tracking:** Writes through WebDAV or Cloud Filter go through Moss, which records changelog entries. Replication propagates to Dormant replicas.
- **Direct filesystem writes:** Files written via SMB signpost (if enabled) or local access bypass Moss. Detected via `inotify`/`fanotify` (Linux) or periodic scan, then reconciled into the changelog.
- **Policy per seed bank:** Replication of user content is configurable in the manifest. A `platform` seed bank might replicate to every stone. A `personal` seed bank might replicate only to an offsite backup.

### Multiple managed storages

Different storages serve different purposes. The naming convention and roles make this explicit:

```bash
garden-rake storage adopt /mnt/platform-ssd --name zen-garden --roles seed-bank
garden-rake storage adopt /mnt/nas-personal --name personal
garden-rake storage adopt /mnt/usb-archive --name archive --roles seed-bank
```

Each has independent roles, replication policy, visibility, and access patterns. Space accounting is natural — each storage is a separate filesystem with its own capacity. No quotas or reservation policies needed.

Roles can be changed at any time through the API or UI:

```bash
garden-rake storage roles personal --add seed-bank     # start receiving backups
garden-rake storage roles personal --remove seed-bank   # stop receiving backups
```

## API Rework

The current storage API surface has four overlapping object access paths to the same seed bank content. This proposal consolidates them.

### Current state (42 endpoints across 4 modules)

```
/api/v1/stone/storage/bank/{id}/*path    → Stone-local, read-only, by bank ID
/api/v1/storage/s3/{bucket}/*key         → S3-compatible XML, seed bank by header/query
/api/v1/storage/{*path}                  → REST JSON, seed bank by header/query
/api/v1/garden/storage/{name}/*path      → Name-based, Primary-or-proxy routing
```

**Problems:**

1. **Four ways to read the same file.** The S3 gateway, REST gateway, garden storage, and stone storage all reach the same `SeedBankStore`. Each has its own routing logic, error handling, and response format. This is maintenance burden with no user benefit.
2. **Seed bank selection is implicit.** The S3 and REST gateways select seed banks via `X-Seed-Bank` header or `?seed-bank=` query param, defaulting to `public`. This is fragile — the default is arbitrary and the selection mechanism is hidden.
3. **ID-based vs name-based split is artificial.** Stone-local routes use bank ID (`/bank/{id}`), garden routes use bank name (`/storage/{name}`). With the new model where seed bank names are stable identities, ID-based access adds complexity without value.
4. **S3 gateway serves a different audience.** The S3-compatible XML surface exists for apps that speak S3. It shouldn't be conflated with human-facing or garden-internal storage APIs.
5. **User content has no API surface.** The current APIs only expose `.zen-garden/storage/` (S3 objects) and `.zen-garden/memories/` (harvests). There's no endpoint for the seed bank root where user files live.

### Target state

Consolidate to three purpose-driven surfaces:

```
/api/v1/garden/storage/{name}/*path      → Garden-tier: name-based, Primary-or-proxy (reworked)
/api/v1/stone/storage/...                → Stone-tier: local admin operations (simplified)
/api/v1/storage/s3/{bucket}/*key         → S3 gateway: offering/app use only (unchanged)
/dav/{name}/                             → WebDAV: human file access (new)
```

#### Garden-tier storage (reworked)

The garden storage API becomes the **single REST entry point** for all seed bank content — user files, managed storage, and metadata. The name in the path is the seed bank name. Primary-or-proxy routing is preserved.

```
GET    /api/v1/garden/storage                           → List all known seed banks
GET    /api/v1/garden/storage/{name}                    → Seed bank details + discovery
GET    /api/v1/garden/storage/{name}/files/*path         → Read file (user content at root)
PUT    /api/v1/garden/storage/{name}/files/*path         → Write file
DELETE /api/v1/garden/storage/{name}/files/*path         → Delete file
HEAD   /api/v1/garden/storage/{name}/files/*path         → File metadata
GET    /api/v1/garden/storage/{name}/objects/*path       → Read S3 object (.zen-garden/storage/)
PUT    /api/v1/garden/storage/{name}/objects/*path       → Write S3 object
DELETE /api/v1/garden/storage/{name}/objects/*path       → Delete S3 object
HEAD   /api/v1/garden/storage/{name}/objects/*path       → S3 object metadata
GET    /api/v1/garden/storage/{name}/memories            → List harvests
GET    /api/v1/garden/storage/{name}/memories/*path      → Read harvest artifact
```

**Key changes:**
- `/files/` exposes the seed bank root (user content). This is new — previously no API covered user files.
- `/objects/` replaces the implicit S3 bucket routing. Explicit namespace, no header-based selection.
- `/memories/` provides read access to offering backups for visibility.
- Seed bank name is always in the path. No more `X-Seed-Bank` header or `?seed-bank=` query param for routing.

#### Stone-tier storage (simplified)

Local admin operations. No object read/write — that moves to garden-tier and WebDAV.

```
GET    /api/v1/stone/storage                            → Overview (counts, capacity)
GET    /api/v1/stone/storage/health                     → Health status
GET    /api/v1/stone/storage/candidates                 → Eligible devices for adopt/prepare
POST   /api/v1/stone/storage/prepare                    → Format blank device as seed bank
POST   /api/v1/stone/storage/adopt                      → Adopt populated storage (new)
GET    /api/v1/stone/storage/banks                      → List local seed banks
GET    /api/v1/stone/storage/banks/{name}               → Local bank details
DELETE /api/v1/stone/storage/banks/{name}               → Remove seed bank
POST   /api/v1/stone/storage/banks/{name}/release       → Unmount
POST   /api/v1/stone/storage/banks/{name}/pin           → Claim Primary role
POST   /api/v1/stone/storage/banks/{name}/unpin         → Release Primary role
PATCH  /api/v1/stone/storage/banks/{name}/visibility    → Set visibility
PATCH  /api/v1/stone/storage/banks/{name}/rename        → Rename
PATCH  /api/v1/stone/storage/banks/{name}/roles         → Set roles (seed-bank, archive, etc.)
GET    /api/v1/stone/storage/banks/{name}/changes       → Replication changelog
GET    /api/v1/stone/storage/stream                     → SSE replication stream
```

**Key changes:**
- Routes use seed bank name instead of internal ID. Names are the stable identity now.
- Object read/write endpoints (`/bank/{id}/*path`) are removed. Content access moves to garden-tier (`/files/`, `/objects/`) and WebDAV.
- `POST /adopt` added for populated storage onboarding.
- `POST /release-all` removed (rarely used, dangerous).
- Pin/unpin moves to per-bank routes instead of top-level body-based routing.

#### S3 gateway (preserved, scoped)

```
GET    /api/v1/storage/s3                               → List buckets (XML)
GET    /api/v1/storage/s3/{bucket}                      → List objects (XML)
GET    /api/v1/storage/s3/{bucket}/*key                 → Get object
PUT    /api/v1/storage/s3/{bucket}/*key                 → Put object
HEAD   /api/v1/storage/s3/{bucket}/*key                 → Object metadata
DELETE /api/v1/storage/s3/{bucket}/*key                 → Delete object
```

**Unchanged in shape**, but the backend shifts: objects are now stored under `.zen-garden/storage/` instead of `garden/storage/`. Seed bank selection via `X-Seed-Bank` header is preserved here — the S3 gateway is the one place where header-based selection makes sense, because S3 clients expect a flat endpoint.

This gateway accesses **only** the S3 object namespace (`.zen-garden/storage/`). It cannot see user files or memories. Apps that need S3 get S3. Humans use WebDAV or the garden REST API.

#### WebDAV (new)

```
/dav/{name}/                             → Full seed bank filesystem (RFC 4918)
```

Serves the entire seed bank root — user content and `.zen-garden/` (via symlink). This is the primary human-facing file access protocol. See the [WebDAV section](#file-sharing-webdav) for details.

#### REST gateway removal

The current `/api/v1/storage/{*path}` REST gateway (JSON responses, `X-Seed-Bank` selection) is **removed entirely**. Its functionality is absorbed by the reworked garden-tier `/api/v1/garden/storage/{name}/objects/` routes, which are more explicit and don't rely on implicit seed bank selection.

Any SDK or orchestrator currently using `/api/v1/storage/` migrates to `/api/v1/garden/storage/{name}/objects/`. The migration is mechanical — add the seed bank name to the path, drop the `X-Seed-Bank` header.

### Migration path

| Current endpoint | Target endpoint | Notes |
|-----------------|-----------------|-------|
| `GET /api/v1/stone/storage/bank/{id}/*path` | `GET /api/v1/garden/storage/{name}/files/*path` | ID → name, stone → garden |
| `GET /api/v1/storage/{*path}` | `GET /api/v1/garden/storage/{name}/objects/*path` | Header-based → path-based bank selection |
| `PUT /api/v1/storage/{*path}` | `PUT /api/v1/garden/storage/{name}/objects/*path` | Same |
| `GET /api/v1/storage/s3/{bucket}/*key` | Unchanged | S3 gateway preserved |
| `GET /api/v1/garden/storage/{name}/*path` | Split into `/files/` and `/objects/` | Explicit namespace separation |

SDKs and orchestrators that currently hit the REST gateway should migrate during Phase 2 (WebDAV), when the new garden-tier routes ship. The old REST gateway can be deprecated with a warning header before removal.

## Alternatives Considered

### SMB as the primary file sharing protocol

- **Pros**: Best OS integration across all platforms, auto-discovery via mDNS, native feel
- **Cons**: Requires Samba daemon, DFS namespace for stable naming, filesystem writes bypass Moss (changelog gap), Microsoft deprecating WebClient doesn't affect this but SMB config is complex
- **Why not**: WebDAV through Moss gives better consistency (writes go through Moss directly), simpler deployment (no Samba), and the Cloud Filter API provides superior Windows integration. SMB remains available as a discovery signpost.

### DFS referrals to WebDAV endpoints

- **Pros**: Transparent — user sees `\\zen-garden\primary`, Windows follows DFS referral to WebDAV on the hosting stone
- **Cons**: Microsoft deprecated the WebClient service. Known bugs since Windows 7 where DFS-to-WebDAV links stop resolving. Building on a deprecated foundation.
- **Why not**: The WebClient service is being removed from Windows. Cloud Filter API is Microsoft's intended replacement.

### Curated visibility (selective symlinks instead of full `.zen-garden/` access)

- **Pros**: Prevents accidental corruption of manifest, changelog, pin files
- **Cons**: Removes user agency. Inconsistent with Zen Garden's transparency ethos (SSH access, inspectable offerings, streamable logs).
- **Why not**: Resilience (`last-known-good/`) is a better safeguard than access restriction. Users should be able to manage their own garden.

## Impact

### What changes

- **Conceptual model**: Storage becomes the entity; seed bank becomes a role. `SeedBank` domain object → `ManagedStorage`. `SeedBankStore` → `ContentStore`.
- Filesystem layout: `garden/` top-level directories move to `.zen-garden/` dotfolder
- New `adopt` onboarding flow for populated storage
- Moss gains WebDAV server capability (new dependency: `dav-server` crate)
- Windows Moss gains Cloud Filter provider capability (new dependency: `cloud-filter` crate)
- Storage infrastructure layer gains adapter trait (USB, NAS, local path)
- `ContentStore` must handle changelog entries for user-content writes
- Storage API consolidated from 4 overlapping surfaces to 3 purpose-driven ones
- REST gateway (`/api/v1/storage/`) removed — absorbed by garden-tier
- Stone-tier storage routes lose object read/write (moves to garden-tier + WebDAV)
- All storage routes use storage name instead of internal ID
- Garden-tier gains explicit `/files/`, `/objects/`, `/memories/` namespaces
- Manifest gains `roles` array — behaviors (seed-bank, archive, etc.) are opt-in flags
- Roles configurable at runtime via API and UI

### What breaks

- Existing seed bank layout (migration needed: move `garden/*` into `.zen-garden/`)
- Tools that assume seed bank roots contain only `garden/` directory
- REST gateway consumers (`/api/v1/storage/`) must migrate to garden-tier routes
- Stone-tier object read consumers (`/bank/{id}/*path`) must migrate to garden-tier or WebDAV
- Any code using seed bank IDs in routes must switch to names
- Domain types: `SeedBank` struct → `ManagedStorage`, `SeedBankStore` → `ContentStore`
- All existing storages gain `roles: ["seed-bank"]` during migration (backward-compatible default)

### What gets easier

- Users can browse all seed bank content from their desktop file explorer
- Users can store personal files within the garden's replication fabric
- Populated drives (NAS volumes, existing USB drives) can join the garden without formatting
- Windows users get native Explorer integration without WebDAV or SMB configuration
- One clear entry point for each audience: WebDAV for humans, garden-tier REST for internal services, S3 for apps
- No more implicit seed bank selection via headers — the name is always in the path

## Delivery Phases

### Phase 1: Foundation
- Domain rename: `SeedBank` → `ManagedStorage`, `SeedBankStore` → `ContentStore`
- Manifest gains `roles` array; existing seed banks migrate with `roles: ["seed-bank"]`
- `.zen-garden/` layout with `last-known-good/`
- `Zen Garden` symlink
- `garden-rake storage adopt` command (with optional `--roles`)
- Migration path for existing seed banks (`garden/` to `.zen-garden/`)
- Storage adapter trait + USB adapter (refactor from current code)
- `StorageService` domain service (single entry point, replaces per-handler logic)

### Phase 2: API rework
- Rework garden-tier storage routes: `/files/`, `/objects/`, `/memories/` namespaces
- Switch stone-tier routes from bank ID to bank name
- Remove stone-tier object read/write endpoints
- Deprecate REST gateway (`/api/v1/storage/`) with warning header
- Migrate Ollama orchestrator and any SDK consumers to new routes
- Update `api-endpoints.md` reference

### Phase 3: WebDAV
- `dav-server` integration in Moss
- Custom `DavFileSystem` backend for seed banks
- Changelog recording for WebDAV writes
- Proxy routing for remote seed banks
- Remove deprecated REST gateway

### Phase 4: Cloud Filter (Windows)
- `cloud-filter` integration in Windows Moss builds
- Sync root registration on startup
- Beacon-driven seed bank discovery in Explorer
- On-demand fetch/push through garden storage API

### Phase 5: Extended adapters
- NAS adapter (NFS/SMB persistent mounts)
- Local path adapter
- Filesystem watchers for non-API writes (`inotify`/periodic scan)

### Phase 6: Discovery polish
- SMB signpost share (optional, for mDNS network browser visibility)
- `garden-rake storage status` with space breakdown per seed bank
- Initial content catalog on adopt

## Open Questions

- **Lock semantics**: `dav-server`'s `FakeLs` is sufficient for single-user. If multiple users (e.g., household members) access the same seed bank via WebDAV, do we need real locks? Pond membership could scope access, but concurrent write conflicts need a policy.
- **Cloud Filter offline behavior**: When the hosting stone is unreachable, should placeholder files show an error state or cache the last-known content? The `cloud-filter` crate supports hydration states but the caching policy needs design.
- **Symlink on Windows**: The `Zen Garden → .zen-garden/` symlink works naturally on Linux/macOS. On Windows NTFS, symlinks require elevated privileges or Developer Mode. May need a junction point instead, or skip the symlink on Windows entirely (Cloud Filter renders the content anyway).
- **Filesystem watcher scope**: Should `inotify`/`fanotify` watch the entire seed bank root (including user content for replication) or only `.zen-garden/` (for managed content coherence)? Watching large trees has performance implications.
- **mDNS name for tending stone**: Should the tending stone claim `zen-garden.local` as a secondary mDNS hostname for WebDAV entry point stability? Or use the existing stone name?

## References

- [Storage Capability Model](ongoing/storage-capability-model.md) — protocol vs offering model, gateway architecture
- [STORAGE-0005: Manifest-First Discovery](../decisions/STORAGE-0005-manifest-first-discovery.md) — `.zen-garden/manifest.json` as source of truth
- [STORAGE-0006: Seed Bank Replication](../decisions/STORAGE-0006-seed-bank-replication.md) — Primary/Dormant roles, changelog-driven sync
- [STORAGE-0007: Storage Lifecycle Objects](../decisions/STORAGE-0007-storage-lifecycle-objects.md) — StorageDevice + SeedBank composition
- [STORAGE-0008: Garden/Stone API Split](../decisions/STORAGE-0008-garden-stone-api-split.md) — Primary-or-proxy routing
- [dav-server crate](https://crates.io/crates/dav-server) — Rust WebDAV server library (RFC 4918)
- [cloud-filter crate](https://crates.io/crates/cloud-filter) — Rust wrapper for Windows Cloud Filter API
