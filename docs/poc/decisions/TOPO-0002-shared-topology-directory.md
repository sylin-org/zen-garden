---
audience: developer
doc_type: decision
status: accepted
last_verified: 2026-02-07
---

# TOPO-0002: Shared Topology Directory

**Date**: 2026-02-07
**Status**: Accepted

## Context

Moss maintains an in-memory topology cache of all discovered stones. Koan.ZenGarden (the .NET client) discovers Moss via UDP, connects via HTTP, and hydrates topology through `GET /api/v1/garden/topology` and the SSE tools stream. The client persists learned stone metadata to a `stones.json` file for failover.

This works when Moss is reachable. Two gaps remain:

1. **Container cold-start with Moss down.** A container booting for the first time with an empty cache and an unreachable Moss has no topology knowledge. It cannot discover via UDP (multicast doesn't cross Docker bridge networks) and has no persisted roster to seed from.

2. **No authoritative pre-warmed topology for containers.** Moss knows the full mesh at all times but only exposes it via HTTP. If the HTTP endpoint is temporarily unreachable during a container's first resolution attempt, that knowledge is inaccessible.

Additionally, the client's roster file `stones.json` has a generic name that doesn't convey its relationship to Zen Garden's topology system.

## Decision

### Two-file shared topology directory

Introduce a shared directory on the host containing two files with distinct ownership:

| File | Writer | Schema | TTL | Purpose |
|------|--------|--------|-----|---------|
| `garden-topology.json` | Moss | `TopologyEntry[]` (full mesh) | 45s offline / 24h eviction | Authoritative mesh snapshot |
| `garden-stones.json` | Clients | `CachedMossStone[]` (lean) | 7-day retention | Client operational roster |

Moss and clients never write to each other's file. The files coexist in the same directory and are distinguished by name and ownership convention.

### Host path

The topology directory lives under `shared_data_dir()` — a stable, absolute, system-wide location for cross-process data. Unlike `data_dir()` (which is relative on Windows for development), `shared_data_dir()` always resolves to an absolute path so external processes can locate it by well-known convention.

| Platform | `shared_data_dir()` | Topology path |
|----------|---------------------|---------------|
| Linux | `/var/lib/zen-garden` | `/var/lib/zen-garden/topology/` |
| Windows | `{ProgramData}\zen-garden` | `{ProgramData}\zen-garden\topology\` |

`topology_dir()` = `{shared_data_dir()}/topology/`.

Override: `GARDEN_SHARED_DATA_DIR` (base) or implicit via `GARDEN_DATA_DIR` on Linux (where both resolve identically).

### Container mount

Moss's Docker handler auto-injects a bind mount for every managed container:

```
Host:       {topology_dir()}
Container:  /app/cache/zen-garden/
```

This matches the existing Koan.ZenGarden container path convention (`StoneRosterPathResolver` already resolves to `/app/cache/zen-garden/` when `DOTNET_RUNNING_IN_CONTAINER=true` and `/app/cache` exists).

### Docker handler injection (not per-manifest)

The topology mount is a cross-cutting infrastructure concern. It is injected in `DockerManager::install_service()` alongside manifest-defined volumes, not declared in individual offering manifests. This ensures:

- Every managed container gets it automatically
- No manifest author action required
- Cannot be accidentally omitted
- Existing and future offerings benefit without changes

### Moss persistence strategy

Moss persists its in-memory topology cache to `garden-topology.json` using:

- **Dirty flag** on cache mutation (`upsert_from_chirp`, `mark_stone_offline`, `forget_stone`)
- **Debounced write**: 500ms after last mutation
- **Periodic flush**: every 30s if dirty (aligned with existing maintenance interval)
- **Graceful shutdown**: immediate flush
- **Atomic writes**: write to `.tmp`, sync, rename (using existing `atomic_write_file`)

### File format vs API format

The file is a **bare JSON array** of `TopologyEntry` objects — not the HTTP API envelope:

```json
[
  { "stone_id": "...", "stone_name": "...", "endpoint": "...", ... },
  { "stone_id": "...", "stone_name": "...", "endpoint": "...", ... }
]
```

The HTTP API (`GET /api/v1/garden/topology`) wraps this in `ApiResponse<T>`:

```json
{ "data": [ ... ], "suggestions": [ ... ] }
```

The file omits the envelope because it is not an HTTP response. The `data`/`suggestions` wrapper is an API transport concern, not a domain concern. Clients reading the file deserialize `TopologyEntry[]` directly. Clients reading the HTTP endpoint unwrap `{"data": [...]}` first.

Self entry is written first, then peers — same ordering as the API response payload.

### Client rename

`stones.json` is renamed to `garden-stones.json` so the two files pair semantically:

```
/app/cache/zen-garden/
  garden-topology.json    ← Moss authority
  garden-stones.json      ← Client projection
```

Clients implement a one-time migration: if `garden-stones.json` does not exist but `stones.json` does, rename it.

### Client secondary seed

On cold start, the client's seeding logic reads both files:

1. Own roster (`garden-stones.json`) — primary seed, client's operational entries
2. Moss topology (`garden-topology.json`) — secondary seed, fills gaps for stones the client has never directly contacted

Own roster entries take priority (by `CacheKey`). Moss topology entries are converted to the client's lean schema and added only if not already present.

## Consequences

**Positive:**

- Containers get pre-warmed topology on first cold start, even if Moss is momentarily unreachable
- Works identically on Linux and Windows (different host path, same container path)
- Composes with existing discovery layers without replacing any
- No manifest changes needed for any existing or future offering
- Semantic file naming clarifies ownership and purpose

**Tradeoffs:**

- Topology file can be stale (up to 30s + debounce). Clients must not treat it as real-time truth — it seeds the cache, the SSE stream is the live path.
- The mount is read-write (clients need to write `garden-stones.json`), so a misbehaving container could theoretically overwrite `garden-topology.json`. This is convention-enforced, not filesystem-enforced.

**Neutral:**

- Non-containerized apps (native Rake, host-side Koan apps) are unaffected. They continue using UDP discovery, mDNS, and the topology HTTP API.
- The SSE stream remains the primary real-time topology path for connected clients.

## Implementation

Split across two repositories:

**zen-garden (Moss, Rust):**
1. Add `topology_dir()` path constant
2. Add persistence to topology maintenance (dirty flag, debounce, atomic write)
3. Auto-inject topology bind mount in `DockerManager::install_service()`

**koan-framework (Koan.ZenGarden, .NET):**
1. Rename roster file to `garden-stones.json` with migration shim
2. Add secondary seed from `garden-topology.json`

See: `koan-framework/docs/decisions/DATA-0090-shared-topology-directory.md` for Koan-side implementation spec.

## References

- `docs/proposals/discovery-topology-caching.md` — Original topology persistence proposal
- `docs/decisions/TOPO-0001-unified-announcement-system.md` — Chirp-based topology discovery
- `src/moss/src/domain/topology.rs` — Current in-memory cache implementation
- `src/moss/src/docker.rs:386-504` — Docker container creation (injection point)
- `src/moss/src/api/v1/garden.rs:242-287` — Topology API endpoint
- `koan-framework/src/Koan.ZenGarden/Persistence/StoneRosterStore.cs` — Client roster persistence
- `koan-framework/src/Koan.ZenGarden/Persistence/StoneRosterPathResolver.cs` — Container path resolution
