---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-02
---

# TOOLS-0002: GardenTool Unified Contract

**Date**: 2026-03-02
**Status**: Accepted
**Applies to**: `garden-common`, `moss` (tools domain, services API), `rake` (find, list, config)
**Depends on**: TOOLS-0001 (Garden Tools Domain), ORCH-0004 (Gateway Announcement)
**Supersedes**: TOOLS-0001 (partially — retains streaming/cache/beacon infrastructure, replaces projection model)

## Context

TOOLS-0001 introduced the tools domain as a unified projection of offerings and
seed-banks. The implementation created two parallel data paths:

- `/api/v1/garden/services` — used by `garden-rake find`. Resolves gateways
  (orchestrator priority), applies manifest-based URI templates, returns correct
  composite connection strings (e.g., MongoDB replica set URIs).

- `/api/v1/garden/tools` — used by Koan `ZenGardenClient`. Projects offerings
  directly via `resolve_connection()` per-stone, bypassing gateway awareness
  entirely. Returns single-node URIs instead of orchestrator-resolved endpoints.

### Observable Problems

1. **Wrong connection data**: `?tool_fqid=offering:mongodb` returns
   `mongodb://stone-golden-summit.local:27017` (single node) instead of
   `mongodb://192.168.1.174:27017,192.168.1.169:27017/?replicaSet=zen-garden`
   (replica set via orchestrator gateway).

2. **Redundant `offering:` prefix**: `tool_fqid` values are prefixed with
   `offering:` or `seed-bank:` (e.g., `offering:mongodb`, `seed-bank:seed-clear-valley`).
   The `tool_type` field already discriminates. The prefix forces clients to
   strip it for user-facing queries, creating mismatches — Koan's
   `NormalizeOffering` strips the prefix, then sends `?tool_fqid=mongodb` to
   Moss which expects `offering:mongodb`, returning zero results.

3. **Two divergent response models**: `FoundService` (services) and
   `ToolProjection` (tools) represent the same domain with different field names,
   different connection structures, and different capabilities formats. Drift is
   inevitable.

4. **Missing metadata in tools**: No `category` or `tags` — clients cannot
   distinguish orchestrator entries from data-node entries.

## Decision

Replace `ToolProjection` and `FoundService` with a single shared contract:
**`GardenTool`**. Both `/garden/tools` and `/garden/services` serialize from
this type. The tools projector delegates to `find_services()` for offerings
instead of projecting independently.

### The GardenTool Contract

```rust
pub struct GardenTool {
    pub fqid: String,                   // "mongodb", "mongodb:prod", "ollama:adopted"
    pub tool: ToolIdentity,
    pub stone: Stone,
    pub service: ServiceInfo,
    pub capabilities: Vec<Capability>,
}

pub struct ToolIdentity {
    pub name: String,                   // "" (default), "prod", "adopted"
    pub tool_type: String,              // "mongodb", "ollama", "seed-bank"
    pub category: String,               // "orchestrator", "offering", "storage"
    pub id: String,                     // GUIDv7
    pub tags: Vec<String>,
}

pub struct Stone {
    pub id: String,
    pub name: String,
    pub endpoint: String,
}

pub struct ServiceInfo {
    pub status: String,                 // "running", "stopped", "degraded"
    pub ready: bool,
    pub protocol: String,               // "mongodb", "http", "s3"
    pub uris: Vec<String>,
}

pub struct Capability {
    pub cap_type: String,               // "model", "collection"
    pub items: Vec<String>,
}
```

### Identity Rules

- **`fqid`** is the bare canonical name: `mongodb`, `mongodb:prod`, `ollama:adopted`.
  No `offering:` or `seed-bank:` prefix. The `tool.category` field discriminates.
- **`tool.name`** is the instance qualifier: empty string for default instances,
  `"prod"`, `"dev"`, `"adopted"` for named instances.
- **`tool.tool_type`** is the offering type: `"mongodb"`, `"ollama"`, `"redis"`,
  or `"seed-bank"` for storage entries.
- **`fqid` matching**: `?fqid=mongodb` matches entries where `tool.tool_type == "mongodb"`.
  It does NOT prefix-match (so `ollama` does not match `ollama-cpu`).
  `?fqid=mongodb:prod` matches exact `fqid`.

### Ordering Contract

Results are ordered by category priority within each fqid group:

1. **`orchestrator`** — gateway entries (pinned first; composite connection strings)
2. **`offering`** — direct service instances (individual node endpoints)
3. **`storage`** — seed-bank entries

Clients can safely take `tools[0]` for the common "give me the best endpoint" case.

### Projection Source

The tools projector calls `find_services()` (the same function backing
`/garden/services`) for offerings, then maps each `FoundService` into a
`GardenTool`. This guarantees:

- Gateway/orchestrator entries appear with correct priority
- Connection info uses manifest-based URI templates (replica set URIs, etc.)
- Single source of truth — no divergence between endpoints

Seed-bank projection remains via the existing `storage_cache` path (unchanged).

### Streaming Envelope

The tools streaming infrastructure (SSE, cursor, deltas, beacons) is preserved.
The delta payload carries `GardenTool` instead of `ToolProjection`:

```json
{
  "cursor": 3706,
  "tools": [{ "fqid": "mongodb", "tool": { ... }, "stone": { ... }, "service": { ... } }],
  "replay": [{ "cursor": 3705, "kind": "upsert", "tool": { /* GardenTool */ } }]
}
```

### Query Parameters

| Param | Behavior |
|-------|----------|
| `fqid` | Match by `tool.tool_type` (bare name) or exact `fqid` (with instance) |
| `category` | Filter by `tool.category`: `orchestrator`, `offering`, `storage` |
| `state` | Filter by `service.status`: `running`, `stopped`, `degraded` |
| `capability` | Filter by capability type:item (AND semantics) |
| `since` | Cursor for SSE delta replay |

## Consequences

### Positive

- One contract, two delivery modes (snapshot vs stream) — zero model drift
- Correct connection data from day one (gateway-resolved, manifest-aware)
- No `offering:` prefix — clients query with bare names, matching works
- Category field enables orchestrator-first resolution without client logic
- `garden-rake find` and Koan `ZenGardenClient` consume the same shape

### Negative

- Breaking change for existing tools API consumers (Koan `ZenGardenClient`)
  — must update parser and snapshot types. Mitigated: Koan is the only
  consumer and is updated in the same change.
- `find_services()` becomes a hot path (called on every projection refresh).
  Acceptable: it reads from in-memory topology cache, no I/O.

## Implementation

### Changed files

| Crate | File | Change |
|-------|------|--------|
| `garden-common` | `src/tools/types.rs` | Replace `ToolProjection`, `ToolConnection` with `GardenTool`, `ToolIdentity`, `Stone`, `ServiceInfo`, `Capability`. Remove `build_tool_fqid`. Update `ToolDelta` to carry `GardenTool`. |
| `moss` | `domain/tools/projector.rs` | Rewrite: call `find_services()` for offerings, map to `GardenTool`. Keep seed-bank path via storage_cache. |
| `moss` | `domain/tools/cache.rs` | Update cache to store/filter `GardenTool` instead of `ToolProjection`. |
| `moss` | `domain/tools/readiness.rs` | Simplify: readiness derived from `FoundService.status` mapping. |
| `moss` | `domain/tools/events.rs` | Update snapshot payload type. |
| `moss` | `api/v1/tools.rs` | Update query parsing (`fqid` instead of `tool_fqid`), response types. |
| `moss` | `domain/service_discovery.rs` | Export `FoundService` → `GardenTool` mapping. Ensure `find_services` is callable from projector context. |
| `moss` | `infra/tools/beacon.rs` | Update beacon to carry `GardenTool` deltas. |
| `rake` | `commands/discovery/find.rs` | Update response parsing to `GardenTool` model. Remove local `FoundService` duplicate. |
| `rake` | `commands/discovery/list.rs` | Update response parsing to `GardenTool` model. Remove local `FoundService` duplicate. |
| `rake` | `commands/discovery/config.rs` | Update to read from `GardenTool` fields. |
| `koan-framework` | `Koan.ZenGarden` | Update snapshot parsing, subscription matching, `ZenGardenToolSnapshot` to align with `GardenTool` contract. Remove `offering:` prefix stripping in `NormalizeOffering`. |

### Verification

```bash
# zen-garden
cargo check --all
cargo clippy -- -D warnings
cargo test --all

# koan-framework
dotnet test tests/Koan.ZenGarden.Tests/
```

Manual: run `garden-rake find mongodb --format json` and
`curl /api/v1/garden/tools?fqid=mongodb` — both must return identical
domain data (orchestrator first, replica set URI, bare fqid).

## Related

- TOOLS-0001: Garden Tools Domain (predecessor — streaming/beacon infrastructure retained)
- ORCH-0004: Gateway Announcement (provides orchestrator entries via `handler_for`)
- ORCH-0008: Handler Election Suppression (gateway lifecycle management)
