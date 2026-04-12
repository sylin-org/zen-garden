---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-04-12
depends_on: [ARCH-0017, ARCH-0025, STORAGE-0009]
---

# ARCH-0026: Storage API Surface — Bank Endpoints and Data Plane Commands

**Date**: 2026-04-12
**Status**: Accepted
**Book**: VIII-b (Storage API Surface)
**Epic**: [ARCH-0017](ARCH-0017-ddd-monolith-epic.md)

## Context

Book VIII-a (ARCH-0025) introduced the Bank as a first-class domain entity
with typed commands and queries. The API surface still routes all bank
operations through the legacy `/api/v1/stone/storage/banks/{name}` path,
which embeds "storage" as a namespace prefix even though the user-facing
concept is "bank".

The garden tier has no bank-specific endpoints — `list_storages_v1` and
`discover_v1` aggregate raw registry entries rather than projecting
through the Bank aggregate.

Additionally, the Bank aggregate lacks data-plane commands (read, write,
delete). Every protocol handler (S3, WebDAV, REST) independently
constructs `ContentStore` / `ObjectStore` instances from routing results.
A unified write path through the Bank aggregate would centralise
changelog ticks and domain event emission.

## Decision

### New endpoint routes

Introduce first-class `/banks` routes at both tiers:

```
# Garden-wide (aggregated across stones)
GET  /v1/garden/banks                      → all banks in the garden
GET  /v1/garden/banks/{moniker}            → bank details + volume locations
GET  /v1/garden/banks/{moniker}/volumes    → volumes, their stones, roles

# Stone-local (banks represented on this stone)
GET  /v1/stone/banks                       → banks with a local volume
GET  /v1/stone/banks/{moniker}             → local volume details for this bank
POST /v1/stone/banks/{moniker}/pin         → claim Primary
POST /v1/stone/banks/{moniker}/unpin       → release Primary
```

### Backward-compatible redirects

Old `/v1/stone/storage/banks/{name}` paths return 301 Moved Permanently
to `/v1/stone/banks/{name}`. The redirect preserves sub-paths (pin,
unpin, rename, etc.). All existing rake and dashboard callers follow
redirects automatically.

### Bank data-plane commands

Add `write`, `read`, `delete` commands on the Bank aggregate that:
1. Resolve the bank to a local volume via `StorageRoute`
2. Construct the appropriate `ContentStore`
3. Execute the I/O operation
4. Emit `BankChanged` domain events on mutations

These are the unified write path that future protocol handler refactors
will converge on. VIII-b does NOT migrate existing S3/WebDAV/REST
handlers — that is future work.

### What VIII-b does NOT do

- Does NOT rewrite S3/WebDAV/REST handler internals
- Does NOT restructure garden_storage/ handlers
- Does NOT migrate internal callers to Bank data-plane commands yet

## Deliverables

1. New `/v1/stone/banks` and `/v1/garden/banks` routes in router
2. Handler module `api/v1/banks.rs` for stone-local bank endpoints
3. Handler additions in garden_storage for garden-tier bank endpoints
4. 301 redirects from old `/v1/stone/storage/banks/{name}` paths
5. `Bank::write()`, `Bank::read()`, `Bank::delete()` data-plane commands
6. Tests for new endpoints and data-plane commands

## Exit criteria

- New routes registered and handlers compile
- Old `/v1/stone/storage/banks/{name}` redirects to `/v1/stone/banks/{name}`
- Data-plane commands tested
- 735+ tests pass
- `cargo clippy --package garden-moss --lib -- -D warnings` clean

## Consequences

- Bank becomes the canonical API noun. "storage" remains as a legacy
  prefix with redirects.
- Data-plane commands provide a single chokepoint for future protocol
  handler consolidation.
- Existing callers continue working through either redirects or the
  unchanged original handlers.
