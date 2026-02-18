---
status: Accepted
date: 2026-02-17
supersedes: STORAGE-0002 (partially)
---

# STORAGE-0008: Garden / Stone API Split for Storage

## Status

**Accepted** — Implemented in v1 API

## Context

STORAGE-0002 established all storage file operations under `/api/v1/stone/storage/bank/{id}/{*path}`, addressing files by their seed bank's GUIDv7 identifier. This creates two problems:

1. **Clients must know internal IDs.** The GUIDv7 is a physical replica identifier — two stones hosting the same logical seed bank have different IDs. Clients shouldn't need to care which replica they're talking to.
2. **No write routing.** A PUT to a Dormant replica writes locally instead of routing to the Primary. This violates STORAGE-0006's single-writer model and creates divergent state.

The SDK gateway (`/api/v1/storage/`) already solves both problems — it resolves by name and proxies writes to Primary. But the main Moss API still uses raw IDs.

## Decision

Split storage file operations into two tiers based on the existing Zen Garden API hierarchy:

### Garden tier — `/api/v1/garden/storage/{name}/...`

Name-based, distributed. "I want to work with this seed bank; route it for me."

- **All file ops** (GET/PUT/DELETE/HEAD) resolve to the Primary replica.
- If the receiving stone IS Primary → execute locally.
- If the receiving stone is NOT Primary → proxy to the Primary's endpoint.
- Any Moss in the garden is a valid entry point.
- Loop guard: `X-Zen-Proxied` header prevents infinite proxy chains.
- Reads also go to Primary because replication is eventually consistent — serving from Dormant cannot guarantee freshness.

**Routes:**
```
GET    /api/v1/garden/storage/{name}/{*path}   → file read (Primary-or-proxy)
PUT    /api/v1/garden/storage/{name}/{*path}   → file write (Primary-or-proxy)
DELETE /api/v1/garden/storage/{name}/{*path}   → file delete (Primary-or-proxy)
HEAD   /api/v1/garden/storage/{name}/{*path}   → file metadata (Primary-or-proxy)
GET    /api/v1/garden/storage/{name}           → discovery (all replicas)
```

### Stone tier — `/api/v1/stone/storage/bank/{id}/...`

ID-based, stone-local. "I know which physical replica I want."

- **Read-only file ops** (GET/HEAD) — direct local access, no proxying.
- **Admin ops** (visibility, rename, release, delete, changes) — always local.
- No PUT/DELETE on file paths — writes go through the garden tier.

**Routes (unchanged except write removal):**
```
GET    /api/v1/stone/storage/bank/{id}/{*path}   → local file read
HEAD   /api/v1/stone/storage/bank/{id}/{*path}   → local file metadata
GET    /api/v1/stone/storage/bank/{id}            → bank detail (local)
DELETE /api/v1/stone/storage/bank/{id}            → delete bank (local)
PATCH  /api/v1/stone/storage/bank/{id}/visibility → set visibility (local)
PATCH  /api/v1/stone/storage/bank/{id}/rename     → rename (local)
POST   /api/v1/stone/storage/bank/{id}/release    → release (local)
GET    /api/v1/stone/storage/bank/{id}/changes    → replication changelog (local)
```

### Discovery endpoint

`GET /api/v1/garden/storage/{name}` returns all known replicas for a seed bank name, built from the local registry + storage cache beacons:

```json
{
  "data": {
    "name": "seed-clear-valley",
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

### Unchanged routes

- `GET /api/v1/stone/storage` — overview
- `GET /api/v1/stone/storage/health` — health
- `GET /api/v1/stone/storage/candidates` — candidate devices
- `POST /api/v1/stone/storage/prepare` — prepare new bank
- `POST /api/v1/stone/storage/release-all` — release all
- `GET /api/v1/stone/storage/bank` — list local banks
- `POST /api/v1/stone/storage/bank/pin` — pin (name-based orchestration)
- `POST /api/v1/stone/storage/bank/unpin` — unpin (name-based orchestration)
- `GET /api/v1/stone/storage/stream` — replication SSE

### Unchanged companion layers

- SDK gateway (`/api/v1/storage/`) — already name-based with proxy
- S3 gateway (`/api/v1/storage/s3/`) — already name-based with proxy

## Consequences

### Positive
- Clients use names, not IDs. The main UX is simple and location-agnostic.
- Writes always reach Primary. No accidental Dormant writes.
- Direct ID-based reads remain available for power users, debugging, or tools that need replica-specific access.
- Discovery endpoint enables informed pinning decisions and dashboards.

### Negative
- Garden-tier reads add a network hop when the entry point is a Dormant stone. Acceptable since freshness is guaranteed.
- Two route families to maintain. Mitigated by sharing helpers between modules.

### Risks
- Proxy loops during orchestration transitions (both stones briefly Dormant). Mitigated by `X-Zen-Proxied` header — one hop max.
