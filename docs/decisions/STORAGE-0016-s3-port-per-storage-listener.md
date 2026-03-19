---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-18
---

# STORAGE-0016: Unified S3 Storage Gateway

**Date**: 2026-03-18
**Status**: Accepted
**Evolves**: STORAGE-0009 (Managed Storage and File Sharing), STORAGE-0015 (StorageRouter), STORAGE-0006 (Seed-Bank Replication)

## Context

### The Problem: Two Separate Write Namespaces

Moss currently maintains two disconnected storage namespaces within each managed
storage mount:

| API | Disk Path | Changelog | Replicated | Cloud Drive |
|-----|-----------|:---------:|:----------:|:-----------:|
| Native REST / WebDAV | `{path}` (mount root) | Yes | Yes | Yes |
| S3 Gateway | `.zen-garden/storage/{bucket}/{key}` | No | No | No |

The S3 gateway writes to `.zen-garden/storage/` — a subdirectory intentionally
excluded from the changelog by the `ContentStore::write()` guard at `store.rs:191`:

```rust
if !rel_str.starts_with(".zen-garden/") {
    self.append_changelog(&entry).await;
}
```

This exclusion was correct for STORAGE-0006: `.zen-garden/` holds infrastructure files
(`manifest.json`, `pin.json`, `changelog.jsonl`) that should not replicate. But S3
objects are application data, not infrastructure. They ended up in `.zen-garden/` by
implementation convenience, not by architectural intent.

The result: files written via S3 are invisible to replication, invisible to the cloud
drive, and invisible to the native REST/WebDAV APIs. S3 is a parallel universe.

### The Problem: Non-Standard S3 Path Prefix

The S3 gateway is served at `/api/v1/storage/s3/{bucket}/{key}` on the main HTTP port
(7185). Standard S3 clients (AWS SDK, MinIO SDK, rclone, Cyberduck) expect operations
at `GET /{bucket}/{key}` on a dedicated `host:port`. The non-root path prefix makes
the gateway incompatible with the entire S3 ecosystem.

### The Problem: Ambient Storage Selection

A single Moss instance may manage multiple storage replica sets (e.g., `"storage"`,
`"images"`, `"archive"`). The current gateway uses `X-Seed-Bank` header or
`?seed-bank=` query parameter for selection — a non-standard mechanism that S3 clients
do not support.

### Bare-Metal Assumption

Moss runs on bare metal (not in containers). Dynamic port allocation and binding is
not constrained by Docker port mapping, Kubernetes services, or firewall orchestration.

## Decision

### 1. Unified Namespace: S3 Objects Live at Mount Root

S3 buckets map directly to directories at the storage mount root. S3 objects and
native files share the same namespace, the same changelog, and the same replication.

**Before (current):**
```
/mnt/storage/
├── .zen-garden/
│   └── storage/          ← S3 objects (isolated, no changelog, no replication)
│       └── photos/
│           └── IMG001.jpg
├── Photos/               ← Native files (changelog, replicated, cloud drive)
│   └── IMG001.jpg
```

**After (unified):**
```
/mnt/storage/
├── .zen-garden/
│   ├── manifest.json     ← Infrastructure (excluded from changelog)
│   ├── pin.json
│   ├── changelog.jsonl
│   └── meta/             ← S3 metadata sidecars (excluded from changelog)
│       └── photos/
│           └── IMG001.jpg.json
├── photos/               ← S3 bucket = directory. Same as native.
│   └── IMG001.jpg        ← Written by S3, readable by REST/WebDAV/Explorer
```

**Changes required:**

1. `ObjectStore.object_rel()` changes from `.zen-garden/storage/{bucket}/{key}` to
   `{bucket}/{key}` (mount root).
2. Metadata sidecars move from `.zen-garden/storage/{bucket}/{key}.meta.json` to
   `.zen-garden/meta/{bucket}/{key}.json`. The `.zen-garden/meta/` path remains
   excluded from changelog — sidecars are derived data, not content.
3. The `ContentStore::write()` changelog guard remains unchanged: it already includes
   all paths outside `.zen-garden/`. Since S3 objects now live at the mount root, they
   are automatically included.
4. `CreateBucket` (explicit `PUT /{bucket}`) creates a directory at the mount root.
   Auto-create on first write is also retained.

**Consequences of unification:**

- `PUT /s3/photos/IMG.jpg` and `PUT /garden/storage/{name}/fs/photos/IMG.jpg` write
  the same file. S3 is just another protocol for the same storage.
- S3 writes generate changelog entries → automatic replication to Dormant replicas.
- S3 objects appear in the cloud drive on Windows (Explorer) and via WebDAV.
- S3 ListBuckets returns the same directories visible in the native REST listing.
- Bucket/directory name collisions between apps are prevented by the Koan bucket
  naming convention: `{AppIdentity.Code}-{container}` (see STOR-0009).

### 2. Port-Per-Storage S3 Listeners

Moss arms a dedicated S3 listener for each locally mounted storage replica set. Each
listener serves the **standard S3 API at root `/`** on its assigned port.

Operations served:

| S3 Operation | Method | Path / Headers | Status |
|---|---|---|---|
| ListBuckets | `GET /` | — | Supported |
| ListObjectsV1 | `GET /{bucket}` | `?prefix=&delimiter=&marker=&max-keys=` | Supported |
| ListObjectsV2 | `GET /{bucket}` | `?list-type=2&prefix=&delimiter=&start-after=&continuation-token=&max-keys=` | **New** |
| GetObject | `GET /{bucket}/{key}` | `Range: bytes=from-to` → HTTP 206 with `Content-Range` | Supported (range: **new**) |
| PutObject | `PUT /{bucket}/{key}` | `x-amz-meta-*` custom metadata honored | Supported (metadata: **new**) |
| CopyObject | `PUT /{bucket}/{key}` | `x-amz-copy-source: /{src-bucket}/{src-key}` | **New** |
| HeadObject | `HEAD /{bucket}/{key}` | — | Supported |
| DeleteObject | `DELETE /{bucket}/{key}` | — | Supported |
| CreateBucket | `PUT /{bucket}` | — | **New** (explicit; auto-create on write also retained) |
| InitiateMultipartUpload | `POST /{bucket}/{key}?uploads` | — | **New** |
| UploadPart | `PUT /{bucket}/{key}?partNumber=N&uploadId=ID` | — | **New** |
| CompleteMultipartUpload | `POST /{bucket}/{key}?uploadId=ID` | XML body | **New** |
| AbortMultipartUpload | `DELETE /{bucket}/{key}?uploadId=ID` | — | **New** |
| PresignURL | `POST /api/v1/storage/s3/presign` | JSON body | **New** (Moss-native, not SigV4) |

Conditional request headers honored on GET, HEAD, PUT, DELETE: `If-Match`,
`If-None-Match`, `If-Modified-Since`, `If-Unmodified-Since`. Returns 304 Not Modified
or 412 Precondition Failed as appropriate.

Operations deferred: AWS Signature V4 authentication (not needed on private network;
Moss-native presigned tokens provide equivalent security for the target environment).

### 2a. GetObject Range Reads

The existing `ContentStore.read_range()` supports seek-based partial reads for
unencrypted content and full-file-decrypt for encrypted content. The S3 gateway parses
the `Range: bytes=N-M` request header and returns:

- HTTP 206 Partial Content
- `Content-Range: bytes N-M/total` header
- `Content-Length` reflecting the range size (not total object size)
- `Accept-Ranges: bytes` header on all GetObject responses

### 2b. CopyObject

Handles `PUT /{bucket}/{key}` with `x-amz-copy-source: /{source-bucket}/{source-key}`
header. Implementation:

- **Same storage**: Filesystem copy via `ObjectStore.copy_object()` — copies content
  file and metadata sidecar atomically. No read-through-write round-trip.
- **Cross-storage proxy**: If source is on a remote stone, fetches via proxy and writes
  locally (stream copy).
- Returns `<CopyObjectResult>` XML with `ETag` and `LastModified` of the new copy.
- Records changelog entry for the destination (replication-aware).

### 2c. ListObjectsV2

Supports `list-type=2` query parameter. Response uses `<ListBucketResult>` with:

- `KeyCount` (number of keys returned)
- `ContinuationToken` / `NextContinuationToken` (opaque, base64-encoded marker)
- `StartAfter` (replaces `Marker` from V1)
- `IsTruncated`, `MaxKeys`, `Prefix`, `Delimiter`, `CommonPrefixes`

V1 (marker-based) remains supported for backward compatibility. Default behavior when
`list-type` is absent: V1.

### 2d. Custom Metadata

`x-amz-meta-*` headers on PutObject are persisted in the metadata sidecar at
`.zen-garden/meta/{bucket}/{key}.json` under a `custom_metadata` map. Returned on
GetObject and HeadObject as response headers.

The existing `/api/v1/storage/s3/` routes on port 7185 remain unchanged for backward
compatibility and internal use. They are updated to use the unified namespace.

### 2e. Multipart Upload

Large file uploads use the standard S3 multipart protocol:

| Operation | Method | Path | Description |
|---|---|---|---|
| InitiateMultipartUpload | `POST /{bucket}/{key}?uploads` | — | Returns `UploadId` (GUIDv7) |
| UploadPart | `PUT /{bucket}/{key}?partNumber={n}&uploadId={id}` | — | Stores part in temp staging |
| CompleteMultipartUpload | `POST /{bucket}/{key}?uploadId={id}` | XML body with part list | Assembles parts → final object |
| AbortMultipartUpload | `DELETE /{bucket}/{key}?uploadId={id}` | — | Cleans up staged parts |

Implementation:

- **Staging directory**: `.zen-garden/multipart/{upload_id}/` — parts stored as numbered files.
  Lives under `.zen-garden/` so parts are excluded from changelog (only the final
  assembled object enters the changelog via `ContentStore::write()`).
- **Upload state**: `.zen-garden/multipart/{upload_id}/manifest.json` — tracks bucket,
  key, content type, creation time, and completed parts with ETag and size.
- **Assembly**: `CompleteMultipartUpload` concatenates parts in order, writes the final
  object via the normal `ObjectStore::put_object()` path (which triggers changelog +
  replication), then cleans up the staging directory.
- **Expiry**: Incomplete uploads are garbage-collected after 24h by the storage
  lifecycle task (same 10s interval that handles health ticks and beacons).
- **MinIO SDK compatibility**: The MinIO .NET SDK uses multipart automatically for
  objects > 16MB. Without this support, the SDK would fail for larger photo/video files.

### 2f. Presigned URLs (Moss-Native Token Scheme)

Moss implements presigned URLs using a native HMAC token scheme (not AWS SigV4).
This provides time-limited, operation-scoped access without requiring full AWS
signature computation.

**Token generation endpoint:**

```
POST /api/v1/storage/s3/presign
Content-Type: application/json

{
  "bucket": "snap-vault-photos",
  "key": "IMG001.jpg",
  "method": "GET",
  "expires_in_secs": 3600
}
```

Response:

```json
{
  "url": "http://stone-01.local:23400/snap-vault-photos/IMG001.jpg?X-Moss-Token={token}&X-Moss-Expires={timestamp}",
  "expires_at": "2026-03-19T13:00:00Z"
}
```

**Token format**: `HMAC-SHA256(stone_secret, "{method}\n{bucket}/{key}\n{expires_timestamp}")`.
The `stone_secret` is derived from the stone's persistent identity (stone_id).

**Validation on request**: When `X-Moss-Token` query parameter is present on any S3
request, Moss recomputes the HMAC and compares. If valid and not expired, the request
proceeds. If invalid or expired, returns 403 Forbidden.

**Why not SigV4**: AWS Signature V4 is a ~500-line spec-compliance exercise with many
edge cases (canonical request construction, chunk-encoded streaming, query parameter
ordering). Since we control both sides (Koan + Moss) and run on a private network,
the Moss-native scheme provides equivalent security without the compliance burden.
SigV4 can be added later if third-party S3 client compatibility is needed.

### 3. Port Range Management

Moss reserves a configurable port range for S3 listeners:

```
[storage.s3]
base_port = 23400
range_size = 100        # 23400..23499
```

Port assignment within the range is **deterministic by replica set**:

- Assignment order: alphabetical by replica set display name at first mount
- Assignments persist in stone state (`stone.toml` or equivalent) across restarts
- When a storage is permanently removed, its port assignment is released after a
  configurable hold period (default: 24h) to avoid port reuse races
- If the range is exhausted, Moss logs a warning and skips the S3 listener for that
  storage (native API remains available)

### 4. Port Catalog Endpoint

New stone-tier endpoint for port discovery (diagnostic/operational use):

```
GET /api/v1/stone/storage/s3/ports
```

Response:

```json
{
  "ports": {
    "storage": {
      "port": 23400,
      "replica_set_id": "019a5aff-79cb-7815-8dae-3700a698f840",
      "status": "listening"
    },
    "images": {
      "port": 23401,
      "replica_set_id": "019b6c00-1234-7000-abcd-000000000001",
      "status": "listening"
    },
    "archive": {
      "port": 23402,
      "replica_set_id": "019c7d11-5678-7000-efab-000000000002",
      "status": "unavailable"
    }
  },
  "range": {
    "base": 23400,
    "size": 100
  }
}
```

Status values:

| Status | Meaning |
|---|---|
| `listening` | Port armed, storage mounted, accepting requests |
| `unavailable` | Port armed, storage removed, returning 503 |
| `disabled` | Port not armed (range exhausted or manually disabled) |

Note: Koan consumers discover S3 ports via ZenGarden SSE tool snapshots
(`connection.port`, `connection.uris`), not via this endpoint. This endpoint serves
diagnostic tooling, CLI commands, and non-SSE consumers.

### 5. Storage Removal Behavior (503 Graceful Degradation)

When a storage device is physically removed (USB unplugged, NAS disconnected):

1. Moss detects removal via filesystem watcher / mount monitor
2. S3 listener on the assigned port **remains armed** (does not tear down)
3. All S3 requests return `503 Service Unavailable` with an S3-compatible XML error:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Error>
    <Code>ServiceUnavailable</Code>
    <Message>Storage replica set is temporarily unavailable</Message>
    <Resource>{bucket}/{key}</Resource>
    <RequestId>{request_id}</RequestId>
</Error>
```

4. Port catalog endpoint reflects `"status": "unavailable"`
5. SSE stream emits `tool.upsert` with `state: "unavailable"` for the storage tool

Rationale: keeping the listener armed preserves the port assignment for when the
storage returns, and provides an immediate signal to clients (503) without requiring
event subscription. Clients with circuit breakers can react synchronously.

### 6. Storage Return Behavior (Recovery)

When a storage device is re-plugged (same stone or different stone):

**Same stone:**

1. Moss detects mount
2. S3 listener resumes normal operation on the same port
3. Port catalog status changes to `"listening"`
4. SSE stream emits `tool.upsert` with `state: "ready"`

**Different stone:**

1. New stone's Moss detects mount, arms a new S3 listener from its own port range
2. New stone's port catalog includes the storage
3. SSE stream emits `tool.upsert` with updated `stone_id`, `stone_name`, `endpoint`
4. Original stone's listener transitions to `unavailable` (if still armed) then
   releases the port assignment after the hold period

### 7. SSE Event Integration

Storage S3 port changes are communicated via the existing tool SSE stream. The
storage tool snapshot includes connection metadata:

```json
{
  "fqid": "seed-bank:storage",
  "category": "storage",
  "state": "ready",
  "connection": {
    "protocol": "s3",
    "hostname": "stone-01.local",
    "ip": "192.168.1.50",
    "port": 23400,
    "uris": ["http://192.168.1.50:23400"]
  }
}
```

The `connection.port` and `connection.uris` fields reflect the S3 listener port, not
the Moss HTTP port (7185). This allows consumers to connect directly via standard S3
without additional discovery steps.

### 8. StorageTick CDC for Content Change Notifications

The existing `StorageTick` broadcast (used by the replication subsystem) is extended
to serve as a CDC (Change Data Capture) doorbell for external consumers.

Since S3 writes now live at the mount root and generate changelog entries (§1), every
PutObject and DeleteObject produces a `StorageTick`. The tick already contains:

```rust
StorageTick {
    cursor: String,          // GUIDv7 (time-sortable)
    storage: String,         // Replica set display name
    replica_set_id: String,
    creates: u64,            // Count of new files in this tick
    modifies: u64,
    deletes: u64,
}
```

Consumers subscribe to storage ticks via the existing SSE endpoint
`GET /api/v1/stone/storage/stream`. To retrieve specific changes, consumers pull:

```
GET /api/v1/stone/storage/banks/{name}/changes?since={cursor}
```

This returns changelog entries since the given cursor:

```json
{
  "entries": [
    { "c": "019d...", "op": "C", "path": "snap-vault-photos/IMG001.jpg", "bytes": 4521000 },
    { "c": "019d...", "op": "D", "path": "snap-vault-photos/old.jpg" }
  ],
  "next_cursor": "019d..."
}
```

This doorbell-then-pull pattern enables reactive processing pipelines in Koan (see
STOR-0009 §10) without Moss needing to understand application semantics.

### 9. Routing and Proxy Behavior

When a client connects to a stone's S3 port for a storage that stone hosts locally,
requests are served directly from the filesystem.

When the storage is hosted on a remote stone (the current stone knows about it via
topology but doesn't have it mounted), the stone proxies the request to the stone
that has it — preserving the same port-per-storage contract for the client. This
mirrors the existing proxy pattern in `StorageRouter` (STORAGE-0015).

## Consequences

### Positive

- S3 and native APIs share the same namespace — files are files, regardless of protocol
- S3 writes are automatically replicated via existing STORAGE-0006 changelog machinery
- S3 objects appear in the cloud drive (Windows Explorer) and via WebDAV
- Standard S3 clients work without modification — MinIO SDK, AWS SDK, rclone,
  Cyberduck, s3cmd all connect to `host:port` and operate at root `/`
- Per-storage port isolation eliminates ambient state (no headers, no query params)
- 503 on removal provides synchronous failure signal — no polling required
- Port assignments persist across restarts — clients can cache endpoints
- StorageTick CDC enables reactive processing pipelines (auto-embed, auto-thumbnail)
- Existing `/api/v1/storage/s3/` routes remain for internal use and backward
  compatibility

### Negative

- Port range consumption — each storage claims one port; 100-port default range
  should be sufficient for home-lab/SMB scenarios but may need expansion for
  large deployments
- Firewall awareness — network administrators must open the S3 port range in
  addition to ports 7183/7185; mitigated by Moss documenting the range in its
  boot report and stone manifest
- Per-stone port assignments are local — when storage moves between stones, the
  port number may differ; clients must re-discover via ZenGarden events
- Namespace unification means S3 buckets are visible in Explorer. Mitigated by
  Koan bucket naming convention (`{AppIdentity.Code}-{container}`) which clearly
  identifies app-managed directories

### Risks

- **Port conflicts** — another service on the host may occupy ports in the range;
  mitigated by health-checking ports before binding and falling back to next
  available port in range
- **Listener resource cost** — each S3 listener consumes a TCP listener and
  associated async runtime resources; for typical home-lab setups (2-10 storages)
  this is negligible
- **Bucket/directory collision** — an app creates a bucket named "Photos" that
  collides with an existing user directory. Mitigated by the Koan naming convention
  (`snap-vault-photos`, not `photos`). Moss does not enforce naming — collision
  prevention is a consumer responsibility

## References

- [STORAGE-0006](STORAGE-0006-seed-bank-replication.md) — Seed-bank replication
  (changelog machinery that now covers S3 objects)
- [STORAGE-0009](STORAGE-0009-managed-storage-and-file-sharing.md) — Managed storage
  and file sharing (S3 gateway origin)
- [STORAGE-0013](STORAGE-0013-replica-set-identity.md) — Replica set identity model
- [STORAGE-0015](STORAGE-0015-cloud-drive-storage-router.md) — StorageRouter and
  domain policy extraction
- Koan Framework: STOR-0009 — Garden-aware S3 storage connector (companion ADR)
