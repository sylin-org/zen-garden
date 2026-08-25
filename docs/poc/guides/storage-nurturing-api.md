---
audience: [operator, developer]
doc_type: guide
status: current
last_verified: 2026-02-05
canonical: true
---

# Storage + Backup API Guide

This guide explains how to use the **storage** and **backup** APIs, from simple operations to advanced, cross‑stone workflows.

If you are new to seed banks, see `docs/guides/seed-banks.md` for device setup and terminology.

---

## 1. Mental Model

Use these APIs for different purposes:

- **Storage**: app/user data (object storage under `garden/storage` on seed banks)
- **Backup**: offering snapshots (A/B local snapshots, replication to seed banks)
- **Snapshots**: read‑only access to backup snapshots for hydration and external orchestrators

Three scopes exist:

- **Stone‑local**: `/api/v1/stone/...` (always targets the local stone; file ops are read‑only)
- **Garden‑tier**: `/api/v1/garden/storage/{name}/...` (name‑based, Primary‑or‑proxy; any Moss is a valid entry point)
- **SDK gateways**: `/api/v1/storage/...`, `/api/v1/storage/s3/...`, `/api/v1/snapshots` (convenience layers)

Seed bank identifiers:

- **Seed bank name**: human‑readable name, used by garden‑tier, gateway, and snapshots APIs
- **Seed bank id**: GUIDv7, used by stone‑local admin endpoints only

Default seed bank name (when none provided):

- `seed-bank-zen-garden`

> **STORAGE‑0008**: Writes (PUT/DELETE) go through the garden tier, which
> routes to the Primary replica. Stone‑local file routes are read‑only
> (GET/HEAD). See §3a below.

---

## 2. Quick Start (Read‑Only)

### Check storage readiness on a stone

```bash
curl http://stone-01:7185/api/v1/stone/storage/health
```

### List seed banks on a stone

```bash
curl http://stone-01:7185/api/v1/stone/storage/bank
```

### List buckets via garden storage gateway

```bash
curl http://stone-01:7185/api/v1/storage
```

### List available backup snapshots

```bash
curl http://stone-01:7185/api/v1/snapshots
```

---

## 3. Storage Basics (Gateway API)

The storage gateway is the **default** way to read/write seed bank objects.

### 3.1 Upload an object

```bash
curl -X PUT \
  -H "Content-Type: text/plain" \
  -H "X-Seed-Bank: seed-swift-shore" \
  --data "hello garden" \
  http://stone-01:7185/api/v1/storage/my-bucket/hello.txt
```

Query-string selection (dash is the standard):

```bash
curl -X PUT \
  -H "Content-Type: text/plain" \
  --data "hello garden" \
  "http://stone-01:7185/api/v1/storage/my-bucket/hello.txt?seed-bank=seed-swift-shore"
```

Notes:

- `my-bucket` maps to `{seed_bank}/garden/storage/my-bucket`
- `X-Seed-Bank` is optional; omit to use the default seed bank
- `seed-bank` is the only supported query parameter for seed bank selection (header wins)

### 3.2 List objects in a bucket

```bash
curl http://stone-01:7185/api/v1/storage/my-bucket/
```

Optional query parameters:

- `prefix=path/`
- `delimiter=/`
- `marker=key`
- `max-keys=1000`

### 3.3 Download an object

```bash
curl http://stone-01:7185/api/v1/storage/my-bucket/hello.txt
```

### 3.4 Delete an object

```bash
curl -X DELETE http://stone-01:7185/api/v1/storage/my-bucket/hello.txt
```

---

## 3a. Garden‑Tier Storage (STORAGE‑0008)

The **garden‑tier** is the primary way to read/write seed bank objects. Any Moss
can be the entry point — if the local bank is Primary, requests execute locally;
otherwise they are proxied to the stone hosting the Primary replica.

### 3a.1 Discover all replicas

```bash
curl http://stone-01:7185/api/v1/garden/storage/seed-swift-shore
```

Response shows every stone that hosts this seed bank:

```json
{
  "data": {
    "name": "seed-swift-shore",
    "instances": [
      {
        "stone_id": "abc123",
        "stone_name": "stone-pearl-harbor",
        "bank_id": "019c0789-...",
        "role": "primary",
        "pinned": true,
        "pin_id": "019c6df7-...",
        "endpoint": "http://192.168.1.241:7185",
        "visibility": "open",
        "health": "healthy"
      }
    ]
  }
}
```

### 3a.2 Write an object (any Moss)

```bash
curl -X PUT \
  -H "Content-Type: text/plain" \
  --data "hello garden" \
  http://stone-02:7185/api/v1/garden/storage/seed-swift-shore/my-bucket/hello.txt
```

If stone‑02 is the Primary holder → writes locally.
If stone‑02 is Dormant → proxies to the Primary stone transparently.

### 3a.3 Read an object

```bash
curl http://stone-02:7185/api/v1/garden/storage/seed-swift-shore/my-bucket/hello.txt
```

### 3a.4 Delete an object

```bash
curl -X DELETE \
  http://stone-02:7185/api/v1/garden/storage/seed-swift-shore/my-bucket/hello.txt
```

### 3a.5 Object metadata (HEAD)

```bash
curl -I http://stone-01:7185/api/v1/garden/storage/seed-swift-shore/my-bucket/hello.txt
```

Returns `Content-Type`, `Content-Length`, `ETag`, `Last-Modified` headers.

### 3a.6 Loop guard

All proxied requests include `X-Zen-Proxied: true`. If a proxied request
reaches a non‑Primary stone (e.g. during orchestration transitions), it
returns `503 PROXY_LOOP` instead of chaining further.

---

## 4. Storage (Stone‑Local, by Seed Bank ID)

> **Note (STORAGE‑0008):** Stone‑local file routes are **read‑only** (GET/HEAD).
> For writes, use the garden‑tier routes in §3a or the SDK gateway in §3.

Use these endpoints when you know the **seed bank id** and want direct, local access.

### 4.1 Find the seed bank id

```bash
curl http://stone-01:7185/api/v1/stone/storage/bank
```

Example response snippet:

```json
{
  "data": [
    {
      "id": "019c26fc-5e46-7ac1-9fbb-f1664790dead",
      "name": "seed-swift-shore"
    }
  ]
}
```

### 4.2 Get object by id (read‑only)

```bash
curl http://stone-01:7185/api/v1/stone/storage/bank/019c26fc-5e46-7ac1-9fbb-f1664790dead/my-bucket/value.json
```

### 4.3 List bucket contents by id

```bash
curl http://stone-01:7185/api/v1/stone/storage/bank/019c26fc-5e46-7ac1-9fbb-f1664790dead/my-bucket/
```

Optional query parameter:

- `depth=1` (default), `depth=3`, `depth=all`

### 4.4 Head object by id

```bash
curl -I http://stone-01:7185/api/v1/stone/storage/bank/019c26fc-5e46-7ac1-9fbb-f1664790dead/my-bucket/value.json
```

---

## 5. Storage (Cross‑Stone, Automatic Routing)

The gateway (`/api/v1/storage`) automatically routes to the stone that hosts the selected seed bank.

### 5.1 Upload from a different stone

```bash
curl -X PUT \
  -H "Content-Type: text/plain" \
  -H "X-Seed-Bank: seed-swift-shore" \
  --data "from another stone" \
  http://stone-02:7185/api/v1/storage/shared-bucket/remote.txt
```

### 5.2 Verify from the original stone

```bash
curl http://stone-01:7185/api/v1/storage/shared-bucket/remote.txt \
  -H "X-Seed-Bank: seed-swift-shore"
```

If the seed bank is unplugged or not announced, the gateway returns `503` with `NO_SEED_BANK`.

---

## 6. Storage (S3‑Compatible Surface)

Use this when an S3 client is required.

Endpoints:

- `GET /api/v1/storage/s3` (list buckets)
- `GET /api/v1/storage/s3/{bucket}` (list objects)
- `PUT /api/v1/storage/s3/{bucket}/{key}`
- `GET /api/v1/storage/s3/{bucket}/{key}`
- `DELETE /api/v1/storage/s3/{bucket}/{key}`

Example:

```bash
curl -X PUT \
  -H "Content-Type: text/plain" \
  --data "s3 payload" \
  http://stone-01:7185/api/v1/storage/s3/my-bucket/hello.txt
```

The same `X-Seed-Bank` header or `seed-bank` query param is supported here.

---

## 7. Backup Basics (Local A/B Snapshots)

### 7.1 List all offerings with slots

```bash
curl http://stone-01:7185/api/v1/stone/snapshots
```

### 7.2 Create a snapshot (A/B rotation)

```bash
curl -X POST \
  -H "Content-Type: application/json" \
  -d '{"commit_image": true}' \
  http://stone-01:7185/api/v1/stone/snapshots/immich
```

### 7.3 Inspect slots for an offering

```bash
curl http://stone-01:7185/api/v1/stone/snapshots/immich
```

### 7.4 Restore from a slot

```bash
curl -X POST \
  -H "Content-Type: application/json" \
  -d '{"slot":"A"}' \
  http://stone-01:7185/api/v1/stone/snapshots/immich/restore
```

---

## 8. Backup + Seed Banks (Replication)

Replication is **stone‑local**. The seed bank must be attached to the stone performing the operation.

### 8.1 Replicate to a seed bank

```bash
curl -X POST \
  -H "Content-Type: application/json" \
  -d '{"seed_bank":"seed-swift-shore"}' \
  http://stone-01:7185/api/v1/stone/snapshots/immich/replicate
```

### 8.2 List remote snapshots stored on a seed bank

```bash
curl http://stone-01:7185/api/v1/stone/snapshots/remote/seed-swift-shore
```

### 8.3 Restore from a seed bank

```bash
curl -X POST \
  -H "Content-Type: application/json" \
  -d '{"seed_bank":"seed-swift-shore","harvest_id":null}' \
  http://stone-01:7185/api/v1/stone/snapshots/immich/restore-remote
```

---

## 9. Snapshots API (Read‑Only Hydration)

Snapshots are garden‑wide, read‑only, and **audited**. Use them for hydration and external orchestrators.

### 9.1 List all snapshots

```bash
curl http://stone-01:7185/api/v1/snapshots
```

### 9.2 List snapshots for one offering

```bash
curl http://stone-01:7185/api/v1/snapshots/{offering_id}
```

### 9.3 Fetch hydration manifest

```bash
curl http://stone-01:7185/api/v1/snapshots/{offering_id}/manifest
```

### 9.4 Download a snapshot

```bash
curl -o snapshot.tar.gz \
  http://stone-01:7185/api/v1/snapshots/{offering_id}/{harvest_id}
```

Optional audit metadata:

- `X-Requesting-Stone-ID`
- `X-Requesting-Stone-Name`

Optional seed bank selection:

- `X-Seed-Bank: seed-swift-shore`
- `?seed-bank=seed-swift-shore`

---

## 10. Backup Automation (Triggers)

These endpoints are designed for timers (systemd/Task Scheduler) and run the **full workflow**:

Local snapshot → seed bank routing → replication.

### 10.1 Trigger one offering

```bash
curl -X POST http://stone-01:7185/api/v1/snapshots/immich/trigger
```

### 10.2 Trigger all offerings

```bash
curl -X POST http://stone-01:7185/api/v1/snapshots/trigger-all
```

---

## 11. Troubleshooting Patterns

### Storage gateway returns `NO_SEED_BANK`

Likely causes:

- Seed bank not plugged in
- Seed bank not announced yet
- Name mismatch (header or query param)

### Stone‑local storage returns `BANK_NOT_FOUND`

Likely causes:

- Using seed bank **name** where **id** is required
- Seed bank is connected to a different stone

### Backup replication fails

Likely causes:

- Seed bank not attached to the local stone
- Seed bank layout missing `garden/storage` or `garden/snapshots`

---

## 12. API Summary (Storage + Backup)

Storage (garden‑tier — name-based, Primary‑or‑proxy):

- `GET    /api/v1/garden/storage/{name}`               — discover replicas
- `GET    /api/v1/garden/storage/{name}/{bucket}/{key}` — get object
- `PUT    /api/v1/garden/storage/{name}/{bucket}/{key}` — put object
- `DELETE /api/v1/garden/storage/{name}/{bucket}/{key}` — delete object
- `HEAD   /api/v1/garden/storage/{name}/{bucket}/{key}` — object metadata

Storage (stone‑local — read‑only file ops):

- `GET  /api/v1/stone/storage`              — overview
- `GET  /api/v1/stone/storage/health`       — health
- `GET  /api/v1/stone/storage/bank`         — list banks
- `GET  /api/v1/stone/storage/bank/{id}`    — bank detail
- `GET  /api/v1/stone/storage/bank/{id}/{*path}`  — get object (local)
- `HEAD /api/v1/stone/storage/bank/{id}/{*path}`  — head object (local)

Storage (SDK gateway — convenience):

- `GET /api/v1/storage`
- `PUT /api/v1/storage/{bucket}/{key}`
- `GET /api/v1/storage/{bucket}/{key}`
- `DELETE /api/v1/storage/{bucket}/{key}`
- `GET /api/v1/storage/s3`
- `PUT /api/v1/storage/s3/{bucket}/{key}`
- `GET /api/v1/storage/s3/{bucket}/{key}`

Backup (stone‑local):

- `GET /api/v1/stone/snapshots`
- `GET /api/v1/stone/snapshots/{offering}`
- `POST /api/v1/stone/snapshots/{offering}`
- `POST /api/v1/stone/snapshots/{offering}/restore`
- `POST /api/v1/stone/snapshots/{offering}/replicate`
- `POST /api/v1/stone/snapshots/{offering}/restore-remote`
- `GET /api/v1/stone/snapshots/remote/{seed_bank}`

Snapshots (garden‑wide, read‑only):

- `GET /api/v1/snapshots`
- `GET /api/v1/snapshots/{offering_id}`
- `GET /api/v1/snapshots/{offering_id}/manifest`
- `GET /api/v1/snapshots/{offering_id}/{harvest_id}`

---

*This guide reflects the live implementation and aligns with the current storage/backup design.*
