# Storage Capability Specification

**Status:** Proposal  
**Date:** January 2026  
**Authors:** Design session (Leon + Claude)  
**Complements:** Cultivation Specification, Storage API Specification  
**Depends on:** Service Resolution Specification

---

## Executive Summary

This specification defines how storage is exposed, discovered, and resolved in Zen Garden. It bridges the gap between **infrastructure** (seed banks, physical storage) and **services** (S3 API, application storage).

Key insight: **Storage is singular. Access is distributed.**

A USB drive or NAS is one physical location. But multiple stones can provide access to it, acting as **gateways**. Apps request `zen-garden:s3//` (S3 protocol) or `zen-garden:storage//` (agnostic API) and get routed to the best available provider—without knowing or caring where the actual storage lives.

### Protocol vs Offering

- **`s3`** is a **protocol** (wire format for S3-compatible API)
- **`storage`** is a **protocol** (Zen Garden's agnostic storage API)
- **`minio`** is an **offering** (software that supports the `s3` protocol)

```
zen-garden:s3//              → Any S3-compatible provider (MinIO, built-in gateway)
zen-garden:s3//minio         → MinIO specifically, using S3 protocol
zen-garden:storage//         → Any storage provider, using agnostic API
zen-garden:storage//minio    → MinIO using agnostic storage API
```

When a dedicated storage offering (MinIO) is deployed, it becomes the preferred provider for S3 protocol requests.

---

## Alignment Note (2026-02-05)

This proposal predates the seed-bank realignment. Apply these updates when reading:
- `garden/storage/{bucket}/{key}` is the only S3/REST storage root (no `apps/`).
- App scoping is client-side (SDKs default to `{app}/{bucket}`), not server-enforced.
- S3 gateway lives at `/api/v1/storage/s3/*`.
- REST storage surface is `/api/v1/storage/*` (non-S3).
- Seed bank selection uses `X-Seed-Bank` or `seed-bank` (no `X-App-Name`).
- Offering backups use `garden/memories`; `garden/offerings` is reserved for listing active services.

## Table of Contents

1. [Design Philosophy](#design-philosophy)
2. [The Two Layers](#the-two-layers)
3. [Built-in S3 Capability](#built-in-s3-capability)
4. [Gateway Architecture](#gateway-architecture)
5. [Storage Offerings](#storage-offerings)
6. [Resolution Priority](#resolution-priority)
7. [Configuration](#configuration)
8. [Discovery and Announcement](#discovery-and-announcement)
9. [The Capability Ladder](#the-capability-ladder)
10. [Integration with Cultivation](#integration-with-cultivation)
11. [Examples](#examples)
12. [API Reference](#api-reference)

---

## Design Philosophy

### The Problem

Storage has two faces:

1. **Infrastructure**: Where bytes physically live (USB, NAS, local disk)
2. **Service**: How applications access those bytes (S3 API)

Traditional approaches conflate these. You either:
- Configure apps to point directly at storage (brittle, location-dependent)
- Deploy a storage service like MinIO (overhead for simple cases)

### The Zen Garden Approach

Separate the concerns:

```
┌─────────────────────────────────────────────────────────────────┐
│  APPLICATION                                                    │
│                                                                 │
│    connect("zen-garden:s3//myapp")                              │
│    "I need S3 storage"                                          │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│  CAPABILITY RESOLUTION                                          │
│                                                                 │
│    Find best S3 provider:                                       │
│      1. Storage offering (MinIO)? → Use it                      │
│      2. Built-in gateways? → Use them                           │
│      3. Neither? → Error                                        │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│  PROVIDERS                                                      │
│                                                                 │
│    ┌─────────────────┐    ┌─────────────────┐                  │
│    │ MinIO Offering  │    │ Built-in S3     │                  │
│    │ (if deployed)   │    │ (always avail)  │                  │
│    └────────┬────────┘    └────────┬────────┘                  │
│             │                      │                            │
│             │                      ▼                            │
│             │             ┌─────────────────┐                  │
│             │             │ Gateways        │                  │
│             │             │ (Moss endpoints)│                  │
│             │             └────────┬────────┘                  │
│             │                      │                            │
│             ▼                      ▼                            │
├─────────────────────────────────────────────────────────────────┤
│  INFRASTRUCTURE                                                 │
│                                                                 │
│    MinIO's own storage    Seed Banks (USB, NAS, disk)          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

Apps ask for capability. The garden provides the best available implementation.

### Design Principles

| Principle | Application |
|-----------|-------------|
| **DRY** | One resolution path for S3, regardless of provider |
| **YAGNI** | USB seed bank works immediately, no MinIO needed |
| **KISS** | `zen-garden:s3//myapp` always works |
| **SoC** | Infrastructure (seed banks) vs Services (S3 API) clearly separated |

---

## The Two Layers

### Infrastructure Layer: Seed Banks

Seed banks are **physical storage locations** configured per-stone:

```toml
# moss.toml
[[cultivation.seed_banks]]
type = "path"
path = "/mnt/usb-backup"
name = "seed-usb-01"

[[cultivation.seed_banks]]
type = "network"
protocol = "nfs"
host = "nas.local"
path = "/volume1/zen-garden"
name = "seed-nas-main"
```

Seed banks are:
- Where cultivation writes backups
- Physical storage (USB, NAS, local disk)
- Stone-level configuration
- **NOT** directly exposed to apps

### Service Layer: S3 Capability

The S3 capability is what apps consume:

```python
storage = connect("zen-garden:s3//myapp")
storage.put("config.json", data)
```

S3 capability can be provided by:
1. **Storage offerings** (MinIO, SeaweedFS) — full-featured, takes precedence
2. **Built-in gateways** — Moss endpoints backed by seed banks

Apps don't know or care which provides it.

---

## Built-in S3 Capability

### What It Is

Moss provides a built-in S3-compatible API backed by seed bank storage. This ensures S3 capability exists in every garden with configured storage, without requiring a dedicated offering.

### Characteristics

| Aspect | Built-in S3 |
|--------|-------------|
| **Protocol** | S3-compatible subset |
| **Backend** | Seed bank filesystem |
| **Federation** | Pool (of gateways) |
| **Process** | Garden-selected |
| **Consistency** | None (gateways stateless) |

### What It Supports

Core S3 operations:

| Operation | S3 Equivalent | Supported |
|-----------|---------------|-----------|
| Write object | `PutObject` | ✓ |
| Read object | `GetObject` | ✓ |
| Delete object | `DeleteObject` | ✓ |
| Check exists | `HeadObject` | ✓ |
| List objects | `ListObjectsV2` | ✓ |
| Copy object | `CopyObject` | ✓ |
| Multipart upload | `CreateMultipartUpload` | ✓ (simplified) |

### What It Doesn't Support

Complex S3 features handled by dedicated offerings:

| Feature | Reason |
|---------|--------|
| Buckets | Path prefixes instead |
| ACLs | Pond handles security |
| Versioning | Timestamped directories |
| Lifecycle policies | Cultivation handles retention |
| Server-side encryption | Pond encrypts at app level |
| Replication | Use MinIO for this |

---

## Gateway Architecture

### The Key Insight

**Storage is singular. Access is distributed.**

A seed bank exists in one place. But multiple stones can access it, each becoming a **gateway**:

```
┌─────────────────────────────────────────────────────────────────┐
│  STORAGE (Singular)                                             │
│                                                                 │
│    NAS at nas.local:/volume1/zen-garden                         │
│    One physical location for data                               │
│                                                                 │
└───────────────────────────┬─────────────────────────────────────┘
                            │
            ┌───────────────┼───────────────┐
            │               │               │
            ▼               ▼               ▼
┌───────────────┐  ┌───────────────┐  ┌───────────────┐
│   stone-01    │  │   stone-02    │  │   stone-03    │
│   Gateway     │  │   Gateway     │  │   (no access) │
│   (direct)    │  │   (direct)    │  │               │
│   :7185/s3    │  │   :7185/s3    │  │               │
└───────────────┘  └───────────────┘  └───────────────┘
        │               │                    │
        └───────────────┼────────────────────┘
                        │
                        ▼
              ┌───────────────────┐
              │   Applications    │
              │   (anywhere)      │
              └───────────────────┘
```

### Gateway Types

| Type | Description | TXT Record |
|------|-------------|------------|
| **Direct** | Stone can mount/access storage directly | `storage_access=direct` |
| **Proxy** | Stone forwards to another gateway | `storage_access=proxy` |

### Direct Gateways

A stone with direct storage access:

```
stone-01:
  - NAS mounted at /mnt/nas
  - Announces: protocols=s3,storage, access=direct, storage_id=seed-nas-main
  - Serves S3 requests directly from filesystem
```

### Proxy Gateways

A stone that can't reach storage directly but can reach another gateway:

```
stone-03:
  - Cannot mount NAS
  - Can reach stone-01 (which can)
  - Announces: protocols=s3,storage, access=proxy, proxy_via=stone-01
  - Forwards S3 requests to stone-01
```

Proxy gateways extend reach. Apps on stone-03's subnet can access storage through stone-03, even though stone-03 can't access storage directly.

### Gateway Selection

When resolving `zen-garden:s3//`:

```
1. Discover all offerings supporting s3 protocol
2. Filter by storage_id (if specified in connection string)
3. Prefer storage offerings > built-in gateways
4. For gateways: prefer direct > proxy
5. Among equals, select by:
   - Health (healthy > degraded)
   - Load (fewer connections better)
   - Latency (closer better)
6. Return selected endpoint
```

### Stateless Design

Gateways are **stateless proxies**. They:
- Don't cache data
- Don't maintain sessions
- Don't coordinate with each other

Any gateway can serve any request. This enables:
- Simple failover (gateway dies, use another)
- Load distribution (spread requests across gateways)
- No split-brain concerns (no state to diverge)

---

## Storage Offerings

### When Built-in Isn't Enough

Built-in S3 is intentionally simple. For advanced needs, deploy a storage offering:

| Need | Solution |
|------|----------|
| Replication | MinIO erasure coding |
| High availability | MinIO cluster |
| Advanced S3 features | MinIO, SeaweedFS |
| Performance at scale | Dedicated storage offering |

### MinIO Example

```yaml
# minio.manifest.yaml
name: minio
category: storage
tags: [s3, object-storage]
protocols:
  - name: s3
    port: 9000
    default: true
  - name: storage
    port: 8080
    sidecar: true

admission:
  default: communal
  allow_override: true

federation:
  mode: cluster
  choreography:
    startup_args: []
    initiate:
      on: first_instance
      command: "minio server /data --console-address :9001"
    add:
      on: new_instance
      command: "minio server http://stone-{01...04}/data --console-address :9001"

process:
  mode: client
  connection:
    template: "http://{{hosts}}:9000"

consistency:
  mode: replicated
```

### The `protocols` Declaration

Offerings declare which protocols they support:

```yaml
protocols:
  - name: s3       # S3-compatible API
    port: 9000
    default: true  # This is the default protocol for this offering
  - name: storage  # Agnostic storage API (via sidecar)
    port: 8080
    sidecar: true
```

When an app requests `zen-garden:s3//`:
1. Garden finds offerings with `s3` in their protocols list
2. If found, resolve to offering (three-concern model)
3. If not, fall back to built-in gateways

### Precedence

**Offerings always take precedence over built-in.**

Why? Offerings are more capable:

| Aspect | Built-in | MinIO Offering |
|--------|----------|----------------|
| Replication | None | Erasure coding |
| Availability | Gateway pool | Distributed cluster |
| Features | S3 subset | Full S3 |
| Performance | Filesystem | Optimized |

---

## Resolution Priority

### The Full Resolution Path

```
zen-garden:s3//myapp
         │
         ▼
┌────────────────────────────────────────────────────────────────┐
│  1. PROTOCOL REQUEST                                           │
│     Target "s3" is a protocol (wire format), not an offering   │
└────────────────────────────────────────────────────────────────┘
         │
         ▼
┌────────────────────────────────────────────────────────────────┐
│  2. FIND OFFERINGS SUPPORTING S3 PROTOCOL                      │
│                                                                │
│     MinIO deployed (has protocols: [s3, ...])?                 │
│       → YES: Use MinIO (cluster/client/replicated)             │
│       → NO: Continue to step 3                                 │
└────────────────────────────────────────────────────────────────┘
         │
         ▼
┌────────────────────────────────────────────────────────────────┐
│  3. FIND BUILT-IN GATEWAYS                                     │
│                                                                │
│     Any stone with storage-gateway supporting s3?              │
│       → YES: Select best gateway (direct > proxy, health/load) │
│       → NO: Continue to step 4                                 │
└────────────────────────────────────────────────────────────────┘
         │
         ▼
┌────────────────────────────────────────────────────────────────┐
│  4. NO S3 PROTOCOL AVAILABLE                                   │
│                                                                │
│     Error: "No offering supports s3 protocol in garden"        │
│     Hint: "Configure a seed bank or deploy MinIO"              │
└────────────────────────────────────────────────────────────────┘
```

### Direct Offering Request

Apps can request a specific offering:

```
zen-garden:minio           → MinIO using its default protocol (s3)
zen-garden:s3//minio       → MinIO explicitly using S3 protocol
zen-garden:minio:prod      → MinIO instance named "prod"
```

This is useful when you need MinIO-specific features not available through other providers.

---

## Configuration

### Seed Bank Configuration

```toml
# moss.toml

[cultivation]
enabled = true
schedule = "0 3 * * *"  # Daily at 3 AM

# USB drive
[[cultivation.seed_banks]]
type = "path"
path = "/mnt/usb-backup"
name = "seed-usb-01"
announce_s3 = true  # Enable S3 gateway

# NAS via NFS
[[cultivation.seed_banks]]
type = "network"
protocol = "nfs"
host = "nas.local"
path = "/volume1/zen-garden"
name = "seed-nas-main"
announce_s3 = true

# NAS via SMB (Windows-compatible)
[[cultivation.seed_banks]]
type = "network"
protocol = "smb"
host = "nas.local"
share = "zen-garden"
username = "${NAS_USER}"
password = "${NAS_PASS}"
name = "seed-nas-smb"
announce_s3 = true
```

### Configuration Fields

| Field | Required | Description |
|-------|----------|-------------|
| `type` | Yes | `path` or `network` |
| `path` | Yes* | Filesystem path (for `type=path` or NFS) |
| `name` | No | Human-readable identifier |
| `announce_s3` | No | Enable S3 gateway (default: true) |
| `protocol` | Yes* | `nfs`, `smb`, `cifs` (for `type=network`) |
| `host` | Yes* | Network host (for `type=network`) |
| `share` | Yes* | SMB share name (for SMB) |
| `username` | No | Authentication (supports `${ENV_VAR}`) |
| `password` | No | Authentication (supports `${ENV_VAR}`) |

### Disabling S3 Gateway

To use a seed bank only for cultivation (not app storage):

```toml
[[cultivation.seed_banks]]
type = "path"
path = "/mnt/backup-only"
announce_s3 = false  # Don't expose as S3 gateway
```

### Proxy Configuration

Stones that can't access storage directly but can proxy:

```toml
# moss.toml on stone with no direct storage access

[s3_proxy]
enabled = true
discover_gateways = true  # Find gateways via mDNS
preferred_gateway = "stone-01"  # Optional: prefer specific gateway
```

---

## Discovery and Announcement

### mDNS Announcement

Gateways announce via mDNS using the standard `_koan-stone._tcp.local.` service type:

```
_koan-stone._tcp.local.
Instance: stone-01._koan-stone._tcp.local.
Port: 7185
TXT:
  offering=storage-gateway
  protocols=s3,storage
  protocol_default=s3
  admission=communal
  storage_access=direct
  storage_id=seed-nas-main
  storage_name=NAS Main
  health=healthy
```

### TXT Record Fields

| Field | Values | Description |
|-------|--------|-------------|
| `offering` | `storage-gateway` | Built-in gateway offering name |
| `protocols` | `s3,storage` | Supported protocols (comma-separated) |
| `protocol_default` | `s3` | Default protocol for this offering |
| `admission` | `communal` | Always communal for built-in gateways |
| `storage_access` | `direct`, `proxy` | Access type (informational) |
| `storage_id` | string | Unique seed bank identifier |
| `storage_name` | string | Human-readable name |
| `proxy_via` | stone name | Gateway to proxy through (if `access=proxy`) |
| `health` | `healthy`, `degraded` | Gateway health status |

### Discovery Flow

```
1. App: connect("zen-garden:s3//")

2. SDK queries mDNS: _koan-stone._tcp.local.
   Filter: protocols CONTAINS s3

3. Results:
   - stone-01: protocols=s3,storage, access=direct, storage_id=seed-nas-main
   - stone-02: protocols=s3,storage, access=direct, storage_id=seed-nas-main
   - stone-03: protocols=s3,storage, access=proxy, proxy_via=stone-02

4. Selection:
   - Prefer direct over proxy
   - stone-01 and stone-02 are direct
   - stone-02 has lower load
   - Select stone-02

5. Return: http://stone-02.local:7180/api/v1/storage/s3
```

---

## The Capability Ladder

### Progressive Enhancement

S3 protocol support grows as infrastructure grows:

```
┌─────────────────────────────────────────────────────────────────┐
│  LEVEL 1: Single USB Drive                                      │
│                                                                 │
│    garden-rake seed-bank add /mnt/usb --name seed-usb-01        │
│                                                                 │
│    stone-01 announces protocols=[s3,storage] (direct)           │
│    Single gateway. Basic. Works.                                │
│                                                                 │
│    zen-garden:s3// → http://stone-01:7185/api/v1/storage/s3    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  LEVEL 2: NAS with Multiple Gateways                            │
│                                                                 │
│    garden-rake seed-bank add nas.local:/zg --name seed-nas      │
│                                                                 │
│    stone-01 announces s3 (direct)                               │
│    stone-02 announces s3 (direct)                               │
│    Multiple gateways. Load balanced.                            │
│                                                                 │
│    zen-garden:s3// → best of stone-01, stone-02                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  LEVEL 3: Proxy Extends Reach                                   │
│                                                                 │
│    stone-03 can't reach NAS, but can reach stone-02             │
│    stone-03 announces s3 (proxy via stone-02)                   │
│                                                                 │
│    Apps on stone-03's subnet use stone-03 as gateway            │
│    stone-03 forwards to stone-02                                │
│                                                                 │
│    zen-garden:s3// → stone-03 (proxies to stone-02)            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  LEVEL 4: MinIO Offering                                        │
│                                                                 │
│    garden-rake offer minio                                      │
│                                                                 │
│    MinIO supports protocols: [s3, storage]                      │
│    Takes precedence over built-in gateways                      │
│                                                                 │
│    zen-garden:s3// → http://stone-01:9000 (MinIO)              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  LEVEL 5: MinIO Cluster                                         │
│                                                                 │
│    garden-rake offer minio  # on stone-02                       │
│    garden-rake offer minio  # on stone-03                       │
│                                                                 │
│    MinIO cluster: erasure-coded, distributed                    │
│    Client-mode routing                                          │
│                                                                 │
│    zen-garden:s3//myapp → http://stone-{01,02,03}:9000         │
└─────────────────────────────────────────────────────────────────┘
```

**Same connection string at every level.** App code never changes.

### When to Upgrade

| Situation | Recommendation |
|-----------|----------------|
| Single machine, simple storage | Level 1-2 (built-in) |
| Multiple stones, shared NAS | Level 2-3 (built-in + proxies) |
| Need replication | Level 4 (MinIO) |
| High availability required | Level 5 (MinIO cluster) |
| Production workloads | Level 4-5 (MinIO) |

---

## Integration with Cultivation

### Dual Purpose of Seed Banks

Seed banks serve two purposes:

1. **Cultivation**: Where offering backups are stored
2. **S3 Gateway**: Where app data is stored (if `announce_s3 = true`)

Both use the same physical storage, different namespaces:

```
/mnt/seed-bank/
└── garden/
    ├── memories/              # Nurturing backups
    │   ├── index.json
    │   └── {offering_id}/
    │       ├── offering.json
    │       └── {harvest_id}.tar.gz
    └── storage/               # S3/REST storage root
        └── {bucket}/
            └── {key}
```

### Namespace Isolation

App scoping is **client-side only**. The server enforces path traversal protection
and restricts access to `garden/storage` for storage operations.

```python
storage = connect("zen-garden:s3//myapp")

storage.put("config.json", data)     # → garden/storage/myapp/config.json ✓
storage.get("config.json")           # ← garden/storage/myapp/config.json ✓
storage.get("../garden/memories")    # ✗ Error: path traversal denied
```

### Backup of App Data

App data in seed banks is backed up as part of normal cultivation:

```
Offering: myapp (containerized)
Volumes:
  - Uses zen-garden:s3//myapp for storage

Cultivation:
  - Backs up container state
  - App data already in seed bank (no separate backup needed)
```

---

## Examples

### Example 1: USB Seed Bank

```bash
# Plug in USB drive, format and mount
sudo mount /dev/sdb1 /mnt/usb

# Designate as seed bank
garden-rake seed-bank add /mnt/usb --name seed-usb-01

# Moss detects mount, updates config, announces s3 protocol support

# App anywhere in garden
storage = connect("zen-garden:s3//")
storage.put("myapp/data.json", '{"key": "value"}')
# → Written to /mnt/usb/garden/storage/myapp/data.json
```

### Example 2: NAS with Multiple Gateways

```bash
# On stone-01 and stone-02, mount NAS
sudo mount -t nfs nas.local:/volume1/zen-garden /mnt/nas

# Designate as seed bank (on either stone)
garden-rake seed-bank add /mnt/nas --name seed-nas-main

# Both stones announce s3 protocol support
# Apps are load-balanced across both gateways

# App on stone-03
storage = connect("zen-garden:s3//")
# → Resolves to stone-01 or stone-02 based on load
```

### Example 3: Adding MinIO

```bash
# Garden already has built-in S3 via seed bank
storage = connect("zen-garden:s3//")  # Uses built-in gateway

# Deploy MinIO for better features
garden-rake offer minio

# Same code, now uses MinIO (offerings take precedence)
storage = connect("zen-garden:s3//")  # Uses MinIO

# Explicitly request MinIO
storage = connect("zen-garden:s3//minio")  # MinIO via S3 protocol

# Explicitly request built-in gateway (by storage_id)
storage = connect("zen-garden:s3//@seed-nas-main")  # Force built-in
```

### Example 4: Proxy Gateway

```bash
# stone-01 and stone-02 can reach NAS
# stone-03 cannot reach NAS but can reach stone-02

# On stone-03, enable proxy
# (in moss.toml)
[s3_proxy]
enabled = true

# stone-03 now announces s3 protocol (as proxy)
# Apps on stone-03's subnet use local gateway

# App on stone-03's subnet
storage = connect("zen-garden:s3//")
# → http://stone-03:7185/api/v1/storage/s3
# → stone-03 forwards to stone-02
# → stone-02 accesses NAS
```

### Example 5: Multiple Seed Banks

```bash
# USB for local backup (fast)
garden-rake seed-bank add /mnt/usb --name seed-usb-fast

# NAS for shared storage (larger)
garden-rake seed-bank add nas.local:/zg --name seed-nas-shared

# Apps can specify preference (@ targets storage_id)
storage = connect("zen-garden:s3//@seed-usb-fast")   # Force USB
storage = connect("zen-garden:s3//@seed-nas-shared") # Force NAS
storage = connect("zen-garden:s3//")                 # Any available
```

---

## API Reference

### S3 Gateway Endpoint

S3 Base URL: `http://{stone}:7185/api/v1/storage/s3`  
REST Base URL: `http://{stone}:7185/api/v1/storage`

The storage APIs are served on the same port as all other Moss APIs (7185).

### Operations

#### Put Object

```http
PUT /api/v1/storage/{path}
Content-Type: application/octet-stream
X-Seed-Bank: portable-backup   # optional

{binary data}
```

Response:
```http
201 Created
ETag: "abc123..."
X-Content-SHA256: sha256:...
```

#### Get Object

```http
GET /api/v1/storage/{path}
X-Seed-Bank: portable-backup   # optional
```

Response:
```http
200 OK
Content-Type: application/octet-stream
ETag: "abc123..."
Content-Length: 1234

{binary data}
```

#### Head Object

```http
HEAD /api/v1/storage/{path}
X-Seed-Bank: portable-backup   # optional
```

Response:
```http
200 OK
ETag: "abc123..."
Content-Length: 1234
Content-Type: application/octet-stream
Last-Modified: Wed, 28 Jan 2026 12:00:00 GMT
```

#### Delete Object

```http
DELETE /api/v1/storage/{path}
X-Seed-Bank: portable-backup   # optional
```

Response:
```http
204 No Content
```

#### List Objects

```http
GET /api/v1/storage/{bucket}/?list=true&prefix={prefix}&max-keys={n}&marker={marker}
X-Seed-Bank: portable-backup   # optional
```

Response:
```http
200 OK
Content-Type: application/json

{
  "contents": [
    {
      "key": "data/file1.json",
      "size": 1234,
      "etag": "abc123...",
      "last_modified": "2026-01-28T12:00:00Z"
    }
  ],
  "is_truncated": false,
  "next_continuation_token": null
}
```

### Error Responses

```http
400 Bad Request
Content-Type: application/json

{
  "error": "InvalidPath",
  "message": "Path contains invalid segments"
}
```

```http
404 Not Found
Content-Type: application/json

{
  "error": "NoSuchKey",
  "message": "Object not found",
  "key": "data/missing.json"
}
```

```http
503 Service Unavailable
Content-Type: application/json

{
  "error": "StorageUnavailable",
  "message": "Seed bank not accessible",
  "storage_id": "seed-nas-main"
}
```

---

## Summary

### Key Concepts

| Concept | Definition |
|---------|------------|
| **Protocol** | Wire format for access (`s3`, `storage`) — the "how" |
| **Seed Bank** | Physical storage (infrastructure) — the "where" |
| **Gateway** | Moss S3 endpoint (access point) |
| **Storage Offering** | Dedicated S3 service (MinIO) — the "what" |

### The Model

1. **Protocols vs Offerings** — `s3` is a protocol (wire format), `minio` is an offering (software)
2. **Storage is infrastructure** — configured per-stone in `moss.toml`
3. **Access is distributed** — multiple gateways to same storage
4. **Offerings supersede built-in** — MinIO takes precedence when deployed
5. **Same connection string** — `zen-garden:s3//` works at every level

### The Promise

> Plug in a USB drive, get S3. Deploy MinIO, get better S3. Same code.

---

## References

- [Service Resolution Specification](discovery-service-resolution.md) — Three concerns model
- [Cultivation Specification](../storage-cultivation-system.md) — Backup system
- [Storage API Specification](../storage-api-design.md) — Full S3 API details

---

**End of Specification**
