# STORAGE-0002: Storage API Structure

**Status:** Partially superseded by [STORAGE-0008](STORAGE-0008-garden-stone-api-split.md)  
**Date:** 2026-01-28  
**Context:** S3 interface implementation for seed bank object storage

> **Note:** STORAGE-0008 splits file operations into garden-tier (name-based,
> Primary-or-proxy) and stone-tier (ID-based, local-only reads). The stone-tier
> file routes no longer support PUT/DELETE. See STORAGE-0008 for the current API
> surface.

## Decision

Structure the storage API with clear separation between native Moss endpoints and S3-compatible gateway.

### API Structure

```
/api/v1/stone/storage/                     GET    → Overview (bank types, counts)
/api/v1/stone/storage/bank/                GET    → List all seed banks
/api/v1/stone/storage/bank/:id             GET    → Seed bank details + list root objects
/api/v1/stone/storage/bank/:id/*path       GET    → Get object (raw bytes)
/api/v1/stone/storage/bank/:id/*path       PUT    → Create/update object
/api/v1/stone/storage/bank/:id/*path       DELETE → Delete object
/api/v1/stone/storage/bank/:id/*path       HEAD   → Object metadata

/api/v1/storage/s3/:bucket/*key            → S3-compatible gateway (XML responses)
```

### Design Principles

1. **Native Moss API** (`/bank/`) uses `ApiResponse<T>` JSON format consistently
2. **S3 Gateway** (`/storage/s3/`) is fully S3-spec compliant with XML responses and proper headers
3. **PUT idempotency** - PUT creates or updates; same result regardless of prior state
4. **Bank abstraction** - Banks are storage backends; currently only "seed-bank" type exists, but structure allows `/cache/`, `/archive/` in future

### S3 Gateway Mapping

S3 clients see standard bucket/key paths:
```
PUT /api/v1/storage/s3/myapp/config.json
```

Internally maps to:
```
{default-bank-mount}/garden/storage/myapp/config.json
```

- Default bank: `seed-bank-zen-garden` (or first available)
- Bucket = S3 bucket name
- Optional `X-Seed-Bank` header or `seed-bank` query param selects a named seed bank

### Native API vs S3 Gateway

| Aspect | Native (`/bank/`) | S3 (`/storage/s3/`) |
|--------|-------------------|-------------|
| Response format | JSON ApiResponse | XML/raw bytes |
| Bank selection | Explicit `:id` | Default bank |
| Authentication | Moss auth (future) | S3 signatures (future) |
| Use case | Rake CLI, internal | S3 clients, SDKs |

### Migration

Greenfield alignment: `/api/v1/stone/storage/s3/*` is removed.  
Use `/api/v1/storage/s3/*` for the canonical S3 gateway.

## Consequences

- Clear separation of concerns between native and S3 APIs
- S3 clients work unchanged with path-style URLs
- Rake CLI uses native endpoints with proper ApiResponse
- Future storage types (cache, archive) slot in naturally

## References

- [S3-API-REFERENCE.md](../reference/s3-api-reference.md)
- [STORAGE-0001](STORAGE-0001-seed-bank-live-scan.md)
