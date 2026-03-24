# Defensive Publication: Distributed Storage with Replica Set Identity and Per-Mount S3 Gateway

**Inventor**: Leonardo Milson Botinelly Soares (Leo Botinelly)
**Disclosure Date**: 2026-03-24
**Field**: Distributed storage systems and object storage gateways
**Keywords**: replica set identity, per-mount S3 gateway, unified namespace, deterministic port allocation, offline rename propagation

---

## 1. Problem Statement

Distributed storage systems that manage physical devices (USB drives, NAS mounts, local directories) across multiple nodes face a fundamental identity problem: the logical name of a storage space is conflated with the physical device identity. When a user renames a storage space, replication breaks because remote replicas still reference the old name. The name serves triple duty as display label, replica group key, and filesystem folder name, causing cascading failures.

Separately, S3-compatible object storage gateways in distributed systems either use a shared port with path prefixes (incompatible with standard S3 clients that expect root-path access) or require complex load balancer configuration. No existing system provides per-mount S3 endpoints with deterministic port assignment that standard S3 clients can connect to without modification.

### Prior Art Differentiation

| System | Physical/Logical Separation | Per-Mount S3 | Unified Namespace | Offline Rename | Graceful Degradation |
|--------|:---:|:---:|:---:|:---:|:---:|
| Ceph | OSD ID vs Pool (partial) | No (RADOS Gateway shared) | No (separate pools) | No | OSD marking |
| MinIO | No (bucket = directory) | No (single gateway) | Single namespace | No | Healing |
| GlusterFS | Brick ID vs Volume | No (no S3) | Volume-level | No | Self-heal |
| AWS S3 | N/A (centralized) | N/A | Single namespace | N/A | N/A |
| **Disclosed system** | **Yes (GUIDv7 + GUIDv7)** | **Yes (port-per-mount)** | **Yes (S3 + WebDAV + REST)** | **Yes (timestamp-based)** | **Yes (503, port stays armed)** |

---

## 2. Description of the Invention

### 2.1 Two-Level Identity Model

The disclosed system separates storage identity into two levels, each with an immutable machine identifier and a mutable human-readable display name.

#### Storage Unit (Physical Device)

Each physical storage device has its own identity:

| Field | Type | Mutability | Purpose |
|-------|------|------------|---------|
| `id` | GUIDv7 | Immutable | Unique per physical device. Generated once at preparation/adoption. Never changes. |
| `name` | String | Mutable | Display sugar (e.g., `"seed-01"`, `"seed-primary"`). Renameable. No role in replication or routing. |

#### Replica Set (Logical Storage Space)

Multiple physical devices form a replica set — a logical storage space that users interact with:

| Field | Type | Mutability | Purpose |
|-------|------|------------|---------|
| `replica_set_id` | GUIDv7 | Immutable | Unique per replica set. Shared by all member devices. The binding key for replication, orchestration, and routing. |
| `replica_set_name` | String | Mutable | Instance display name (e.g., `""` for default, `"images"`, `"personal"`). Renameable with propagation. |
| `replica_set_name_updated_at` | DateTime<Utc> | Updated on rename | Timestamp of last rename. Enables offline catch-up. |

The critical insight is that `replica_set_id` replaces the display name as the universal grouping key. No code path uses the display name for grouping, routing, or replication. The display name is purely cosmetic.

#### Portable Manifest (On-Device Storage)

The manifest is stored on the physical device itself at `.zen-garden/manifest.json` — not in a central database or registry. This means the device carries its full identity (both device ID and replica set membership) wherever it is physically connected. When a USB drive is unplugged from node A and plugged into node B, node B reads the manifest from the device, discovers its `replica_set_id`, and automatically integrates it into the correct replica set. No central registry lookup is required. This portable-manifest design enables cross-node roaming, offline identity, and zero-configuration device adoption.

#### Manifest Structure

```
StorageManifest {
    version:                     u32,            // Schema version (5)
    id:                          String,          // Device GUIDv7
    name:                        String,          // Device display name
    replica_set_id:              String,          // Replica set GUIDv7 (binding key)
    replica_set_name:            String,          // Replica set display name
    replica_set_name_updated_at: DateTime<Utc>,   // Rename timestamp
    visibility:                  StorageVisibility,
    origin_stone:                String,
    filesystem:                  String,
    created_at:                  DateTime<Utc>,
    encrypted:                   bool,
    pond_fingerprint:            Option<String>,
    roles:                       Vec<String>,      // ["seed-bank"], [], etc.
}
```

#### Implementation Evidence

- `StorageManifest` struct defined per STORAGE-0013 decision.
- `replica_set_id` used as key in `roles_snapshot()`, `pins_snapshot()`, orchestration grouping, replication routing.
- ADR: `docs/decisions/STORAGE-0013-replica-set-identity.md`.

### 2.2 Mutable Display Names vs Immutable Binding Keys

The system supports two distinct rename operations:

**Device rename** (cosmetic, local-only):
- Changes `manifest.name` on the local device.
- Announced via beacon so other nodes update their view of this device.
- Does NOT affect the replica set, replication, or routing.

**Replica set rename** (propagated to all members):
- Changes `replica_set_name` and updates `replica_set_name_updated_at` on the local manifest.
- Broadcasts beacon with new name and timestamp.
- Online members: receive beacon, match by `replica_set_id`, detect newer timestamp, update local manifest.
- Offline members: on reconnect, compare `replica_set_name_updated_at` with Primary's timestamp during replication handshake; adopt newer name before starting sync.

### 2.3 Offline Rename Propagation with Timestamp-Based Catch-Up

When a replica set member is offline during a rename, the system ensures eventual consistency:

#### Pseudocode: Offline Rename Catch-Up

```
function replication_handshake(local_manifest, primary_manifest):
    // Same replica set (matched by immutable ID)
    assert local_manifest.replica_set_id == primary_manifest.replica_set_id

    // Check if Primary has a newer rename
    if primary_manifest.replica_set_name_updated_at > local_manifest.replica_set_name_updated_at:
        local_manifest.replica_set_name = primary_manifest.replica_set_name
        local_manifest.replica_set_name_updated_at = primary_manifest.replica_set_name_updated_at
        persist(local_manifest)

    // Proceed with normal replication sync
    sync_changelog(local_manifest, primary_manifest)
```

This is possible because the binding key (`replica_set_id`) never changes. The rename is cosmetic metadata that propagates independently of data replication. A device that was offline for a week catches up its name on the first replication handshake.

#### Split-Brain Rename Resolution

If two nodes independently rename the same replica set while disconnected (concurrent renames), the timestamp acts as a last-writer-wins tie-breaker: the rename with the strictly later `replica_set_name_updated_at` wins. If timestamps are identical (sub-millisecond collision), the lexicographically greater name wins as a deterministic tiebreaker. This ensures all nodes converge to the same name regardless of reconnection order. No human intervention is required because the rename is cosmetic — data replication is unaffected by which name wins.

### 2.4 Port-Per-Storage S3 Gateway

The disclosed system arms a dedicated S3-compatible HTTP listener for each locally mounted storage volume. Each listener serves the standard S3 API at root `/` on its assigned port, making it compatible with unmodified S3 clients (AWS SDK, MinIO SDK, rclone, Cyberduck, s3cmd).

#### Port Allocation

Ports are allocated from a configurable base range:

```
Base port:  23400
Range size: 100     (23400 - 23499)
```

Assignment is deterministic by replica set identity: the system maintains a persistent map from `replica_set_id` to port number. On first mount, the next available port in the range is assigned and persisted. The display name is not used for allocation or lookup — only `replica_set_id` serves as the key. Assignments persist across restarts via a JSON ledger file. When a storage is permanently removed, its port assignment is released after a configurable hold period (default 24 hours) to avoid port reuse races.

When all ports in the range are exhausted (e.g., 100 mounts on a single node), the system logs a warning and the new storage operates without an S3 listener. All other protocols (REST, WebDAV, filesystem) remain available. The port range is configurable via environment variable (`ZG_S3_PORT_BASE`, `ZG_S3_PORT_RANGE`) to allow operators to expand it.

#### Listener Lifecycle

```
S3Listeners {
    assignments:     HashMap<replica_set_id, S3PortAssignment>,  // keyed by immutable ID
    listener_tokens: HashMap<replica_set_id, CancellationToken>,
    base_port:       u16,
    port_range:      u16,     // configurable, default 100
    shutdown_token:  CancellationToken,
}

S3PortAssignment {
    replica_set_id:   String,   // immutable binding key
    replica_set_name: String,   // display only, for port catalog API
    port:             u16,
    storage_id:       String,
    online:           bool,
}
```

**Primary vs Dormant**: Within a replica set, exactly one device is designated "Primary" at any time — it is the authoritative copy that accepts writes and generates changelog entries. All other devices in the set are "Dormant" — they receive replicated changes from the Primary but do not accept direct writes. Primary designation is determined by a "pin" file (`pin.json`) on the device; the first device to claim the pin wins. Primary status can be transferred via explicit user action (pin/unpin) or automatically when the current Primary becomes unreachable.

#### Port Discovery

Clients discover S3 port assignments through multiple mechanisms:

1. **REST endpoint**: `GET /api/v1/stone/storage/s3/ports` returns a JSON map of `{replica_set_name: {port, status, replica_set_id}}` for all active assignments on the node.
2. **SSE stream**: The node's event stream (`GET /api/v1/stone/storage/stream`) emits `connection.port` events whenever a port assignment changes (arm, disarm, status change).
3. **Beacon advertisement**: Storage beacons broadcast via UDP multicast (see Family 6) include S3 port assignments, enabling LAN-wide port discovery without HTTP polling.
4. **Garden API**: `GET /api/v1/garden/storage` aggregates port assignments from all nodes, providing a cluster-wide view.

- **Arm**: When a managed volume is classified as Primary, a listener is spawned on the allocated port.
- **503 Degradation**: When a volume goes offline (USB unplug), the listener stays armed but returns `503 Service Unavailable` for all operations.
- **Disarm**: When a volume is permanently removed, the listener is cancelled and the port is released.
- **Re-arm**: When a volume comes back online, the 503 gate opens and normal operations resume on the same port.

#### Implementation Evidence

- `S3Listeners` struct in `src/moss/src/infra/storage/s3_listener.rs`.
- `S3PortAssignment` with `replica_set_name`, `port`, `storage_id`, `online` fields.
- `S3_LISTENER_BASE_PORT = 23400` constant.
- `MAX_S3_LISTENERS = 100` constant.
- ADR: `docs/decisions/STORAGE-0016-s3-port-per-storage-listener.md`.

### 2.5 Unified Namespace

S3 buckets map directly to directories at the storage mount root. S3 objects and native files (accessed via REST, WebDAV, or the filesystem) share the same namespace, the same changelog, and the same replication.

**Disk layout:**

```
/mnt/storage/
+-- .zen-garden/
|   +-- manifest.json        (infrastructure, excluded from changelog)
|   +-- changelog.jsonl      (replication log)
|   +-- pin.json             (Primary claim)
|   +-- meta/                (S3 metadata sidecars, excluded from changelog)
|       +-- photos/
|           +-- IMG001.jpg.json  (S3 custom metadata sidecar)
+-- photos/                  (S3 bucket = directory, same as native)
    +-- IMG001.jpg           (written by S3, readable by REST/WebDAV/Explorer)
```

**S3 metadata sidecars**: When an S3 `PUT` includes `x-amz-meta-*` headers, the custom metadata is persisted as a JSON sidecar file at `.zen-garden/meta/{bucket}/{key}.json`. The sidecar contains `content_type`, `content_length`, `custom_metadata` (map of user-defined key-value pairs), and `last_modified`. On `GET` and `HEAD`, the sidecar is read and the metadata headers are restored. Sidecars are excluded from the changelog — they replicate alongside their parent object as part of the `.zen-garden/` infrastructure directory. If no custom metadata is provided, no sidecar is created, and default `Content-Type` is inferred from the file extension.

A file written via `PUT /{bucket}/{key}` on the S3 port is the same file accessible via:
- `GET /api/v1/garden/storage/{name}/fs/{bucket}/{key}` (REST API)
- `GET /dav/{name}/{bucket}/{key}` (WebDAV)
- Windows Explorer via Cloud Filter sync provider
- Direct filesystem access on the hosting node

S3 writes generate changelog entries, which trigger automatic replication to all Dormant replicas in the set. There is no separate S3-only replication path.

#### Changelog Entry Format

The changelog is an append-only JSONL file (`changelog.jsonl`) where each line is a self-contained JSON object:

```
ChangelogEntry {
    sequence:   u64,          // monotonically increasing per-device
    operation:  "create" | "update" | "delete" | "mkdir" | "rmdir",
    path:       String,       // relative path from mount root (e.g., "photos/IMG001.jpg")
    timestamp:  DateTime<Utc>,
    size:       Option<u64>,  // file size after operation (None for deletes)
    checksum:   Option<String>, // SHA-256 of file content (None for directories/deletes)
    source:     String,       // protocol that generated the entry: "s3", "rest", "webdav", "fs"
}
```

During replication, the Dormant device requests entries after its last-seen sequence number from the Primary. The Primary streams matching entries, and the Dormant applies them in order. Conflict resolution is Primary-wins: the Primary's changelog is authoritative.

### 2.6 Graceful 503 Degradation

When a storage device is physically removed (USB unplug, NAS disconnection):

1. The node detects removal via filesystem watcher.
2. The S3 listener on the assigned port remains armed (does not tear down).
3. All S3 requests return `503 Service Unavailable` with S3-compatible XML error body.
4. The port catalog endpoint reflects `status: "unavailable"`.
5. SSE stream emits `tool.upsert` with `state: "unavailable"`.

When the device returns:
- **Same node**: Listener resumes on same port. Status changes to `"listening"`.
- **Different node**: New node arms a new listener from its own port range. Original node's listener transitions to `unavailable`, then releases port after hold period.

This behavior preserves port stability for clients and provides synchronous failure signals (503) without requiring event subscription.

### 2.7 Moss-Native HMAC Presigned Tokens

The system implements presigned URLs using a native HMAC token scheme rather than AWS Signature V4:

```
Token = HMAC-SHA256(stone_secret, "{method}\n{bucket}/{key}\n{expires_timestamp}")
```

The `stone_secret` is a 256-bit key derived from the node's persistent identity (stone ID + installation salt). Each node generates its own secret at first startup and persists it in the node's data directory. Tokens are scoped to the issuing node — a token generated by node A is only valid on node A. Cross-node access uses the REST or Garden API (which proxies through the target node) rather than presigned S3 URLs. If a node is re-installed, a new secret is generated and all previously issued tokens become invalid (fail-safe). Tokens are generated via:

```
POST /api/v1/storage/s3/presign
{ "bucket": "photos", "key": "IMG.jpg", "method": "GET", "expires_in_secs": 3600 }
```

Response includes the full URL with `X-Moss-Token` and `X-Moss-Expires` query parameters. On receiving a request with these parameters, the node recomputes the HMAC and validates expiry. Invalid or expired tokens receive 403 Forbidden.

This provides time-limited, operation-scoped access without the ~500-line complexity of SigV4 canonical request construction.

### 2.8 Multipart Upload with Automatic Garbage Collection

Large files use the standard S3 multipart protocol:

1. `POST /{bucket}/{key}?uploads` — returns `UploadId` (GUIDv7).
2. `PUT /{bucket}/{key}?partNumber=N&uploadId=ID` — stores part in staging directory `.zen-garden/multipart/{upload_id}/`.
3. `POST /{bucket}/{key}?uploadId=ID` — assembles parts into final object at mount root via normal write path (triggers changelog + replication).
4. `DELETE /{bucket}/{key}?uploadId=ID` — aborts and cleans up staging.

Staging lives under `.zen-garden/` so parts are excluded from the changelog (only the final assembled object enters the changelog). Incomplete uploads are garbage-collected after 24 hours by a periodic lifecycle task.

The `upload_id` is validated as a UUID before use in filesystem paths, preventing path traversal attacks.

### 2.9 Role-Based Storage Composition

Storage is the universal managed entity. Roles are composable behaviors:

| Role | Behavior |
|------|----------|
| `seed-bank` | Receives offering backups (harvests) in `.zen-garden/memories/` |
| (no roles) | Fully managed: replication, WebDAV, S3, Cloud Filter — just no platform backups |
| `archive` (future) | Write-once, no deletes |
| `cache` (future) | Ephemeral, no replication |
| `shared` (future) | Multi-node write access |

Roles are flags in the manifest. Replication and roles are orthogonal: a personal NAS replicates to all nodes without the seed-bank role.

#### Implementation Evidence

- `StorageManifest.roles: Vec<String>` field.
- `StorageService` domain entry point in `src/moss/src/domain/storage_service.rs`.
- ADR: `docs/decisions/STORAGE-0009-managed-storage-and-file-sharing.md`.

---

## 3. Claims

1. A distributed storage system with two-level identity comprising: a physical device identity (immutable globally unique identifier per device, such as GUIDv7, UUIDv4, ULID, Snowflake ID, or any other scheme that produces a collision-resistant opaque identifier) and a logical replica set identity (immutable globally unique identifier shared by all member devices, using the same or different generation scheme); mutable display names at both levels that have no role in replication, routing, or orchestration; the replica set identifier serving as the sole binding key for all grouping operations; wherein renaming a storage space cannot break replication because the binding key is immutable. The disclosed embodiment uses GUIDv7 for both levels, but the invention applies to any immutable unique identifier scheme.

2. An offline rename propagation mechanism comprising: a timestamp field (`replica_set_name_updated_at`) recorded on each rename operation; online members receiving rename announcements via beacon and updating their local manifests; offline members comparing timestamps during replication handshake and adopting the newer name before starting data synchronization; a deterministic tiebreaker for concurrent renames (last-writer-wins by timestamp, with lexicographic ordering as a secondary tiebreaker when timestamps are equal); wherein the propagation mechanism is independent of data replication and converges to a single name regardless of how long a member was offline or how many concurrent renames occurred.

3. A per-mount S3 gateway system comprising: a dedicated S3-compatible HTTP listener spawned for each locally mounted storage volume; deterministic port allocation from a configurable base range with persistent assignments across restarts; standard S3 API served at root `/` on each port; compatibility with unmodified S3 clients without path prefix configuration; wherein each storage volume is independently addressable by port without ambient state (headers, query parameters, session tokens).

4. A unified namespace for multi-protocol storage access comprising: S3 buckets mapped directly to directories at the storage mount root; S3 objects and native files sharing the same filesystem namespace, changelog, and replication; writes via any protocol (S3, REST, WebDAV) producing changelog entries that trigger replica synchronization; wherein a file written via S3 is immediately accessible via REST, WebDAV, and native filesystem access without additional synchronization steps.

5. A graceful degradation mechanism for per-mount S3 listeners comprising: the listener remaining armed (bound to its port) when the underlying storage device is removed; all S3 requests returning 503 Service Unavailable with protocol-compatible error responses; the port catalog reflecting unavailable status; SSE events emitting state changes; the listener resuming normal operation when the device returns; port assignment preserved across removal/return cycles with a configurable hold period before release.

6. A native HMAC presigned token scheme for S3-compatible object access comprising: token generation from a node-persistent secret using HMAC-SHA256 over method, path, and expiry timestamp; time-limited and operation-scoped access without implementing AWS Signature V4; token validation by recomputing the HMAC on the receiving node; providing equivalent security to SigV4 for controlled-environment deployments with substantially reduced implementation complexity.

7. A role-based storage composition system comprising: a universal managed storage entity with replication, health monitoring, and multi-protocol access; composable role flags (seed-bank, archive, cache, shared) that add specific behaviors to the base entity; roles orthogonal to replication policy; wherein adding or removing a role does not affect the storage's participation in replication or its accessibility via any protocol.

---

## 4. Implementation Evidence

| Component | Location |
|-----------|----------|
| S3 listener manager | `src/moss/src/infra/storage/s3_listener.rs` — `S3Listeners`, `S3PortAssignment` |
| S3 gateway handlers | `src/moss/src/api/v1/s3_gateway.rs` |
| S3 presigned tokens | `src/moss/src/api/v1/s3_presign.rs` |
| S3 XML responses | `src/moss/src/api/v1/s3_xml.rs` |
| Storage domain service | `src/moss/src/domain/storage_service.rs` — `StorageService` |
| Storage volume types | `src/moss/src/domain/storage/volume.rs`, `bank.rs` |
| Storage orchestration | `src/moss/src/tasks/storage_orchestration.rs` — role resolution by `replica_set_id` |
| Storage replication | `src/moss/src/tasks/storage_replication.rs` — changelog-driven sync |
| Storage API | `src/moss/src/api/v1/storage.rs` |
| Garden storage API | `src/moss/src/api/v1/garden_storage/mod.rs` |
| Shared storage types | `src/common/src/storage.rs` — `StorageBank`, `ReplicaSet` |
| Identity model ADR | `docs/decisions/STORAGE-0013-replica-set-identity.md` |
| S3 gateway ADR | `docs/decisions/STORAGE-0016-s3-port-per-storage-listener.md` |
| Managed storage ADR | `docs/decisions/STORAGE-0009-managed-storage-and-file-sharing.md` |

---

## 5. Public Domain Dedication

This document is published as a defensive disclosure to establish prior art. The inventor(s) dedicate this disclosure to the public domain and assert no patent rights over the described inventions. All rights to use, implement, and build upon these inventions are hereby granted to the public.

---

## Antagonist Review Log

### Pass 1
**Antagonist:** (1) Port allocation keyed by `replica_set_name` contradicts core claim that names have no routing role. (2) Offline rename pseudocode omits split-brain concurrent rename scenario. (3) Claim 1 specifies GUIDv7 exclusively — alternative ID schemes patentable. (4) No port exhaustion behavior described. (5) "Primary" vs "Dormant" never defined.
**Author revision:** Fixed S3Listeners struct to key by `replica_set_id`. Added split-brain rename resolution with timestamp + lexicographic tiebreaker. Broadened Claim 1 to cover any collision-resistant identifier scheme. Added port exhaustion behavior (graceful degradation, configurable range). Added Primary/Dormant definition with pin-file mechanism.

### Pass 2
**Antagonist:** (1) Changelog entry format undescribed — competitor could patent JSONL replication protocol. (2) S3 metadata sidecars mentioned but not specified. (3) HMAC key rotation and cross-node token validation unspecified.
**Author revision:** Added full ChangelogEntry format with sequence, operation, path, timestamp, size, checksum, source fields. Added S3 metadata sidecar specification (content_type, custom_metadata map, lifecycle). Added stone_secret derivation details, per-node token scoping, and re-installation invalidation behavior.

### Pass 3
**Antagonist:** (1) Port discovery mechanism not described — only catalog endpoint mentioned in passing. (2) Claim 2 omits the lexicographic tiebreaker that is only in body text.
**Author revision:** Added Port Discovery section with four mechanisms (REST, SSE, beacon, Garden API). Updated Claim 2 to include deterministic tiebreaker for concurrent renames.

### Pass 4
**Antagonist:** Manifest storage location (on-device vs central DB) not explicitly stated — competitor could patent portable device manifests enabling cross-node roaming.
**Author revision:** Added "Portable Manifest (On-Device Storage)" subsection explicitly describing on-device storage, cross-node roaming, and zero-configuration adoption.

### Final Status
CLEARED -- Antagonist found no further weaknesses. Safe to publish.
