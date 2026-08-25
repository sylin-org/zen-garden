---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-05
---

# TOOLS-0003: Unified Garden Registry

**Date**: 2026-03-05
**Status**: Accepted
**Applies to**: `garden-common`, `moss` (tools domain, storage cache, gateway API, coordinator), orchestrators
**Depends on**: TOOLS-0002 (GardenTool contract), STORAGE-0003 (beacon protocol)
**Supersedes**: TOOLS-0001 (tools cache + TOOLS_BEACON), STORAGE-0003 (StorageCache + STORAGE_BEACON) — partially; retains GardenTool contract from TOOLS-0002

## Context

TOOLS-0001 introduced a tools cache with SSE streaming. STORAGE-0003 introduced a
separate storage beacon and cache. Gateway registrations live in `state.gateways`.
Offerings live in `state.offerings`. Each source has its own announcement mechanism
and its own cache, and query endpoints assemble reality from different subsets:

| Endpoint | Reads from |
|----------|------------|
| `garden/tools` | ToolsCache (projected from offerings + gateways + storage) |
| `garden/services` | TopologyCache + state.gateways + state.offerings |
| `stone/services` | state.offerings + state.gateways |

Three beacon types propagate overlapping data:

| Beacon | Carries |
|--------|---------|
| `STONE_CHIRP` | services list, gateways, stone capabilities |
| `TOOLS_BEACON` | tool snapshot (offerings + gateways + storage) |
| `STORAGE_BEACON` | seed bank announcements with roles |

### Observable Problems

1. **Gateway projection gap**: Registering a gateway via `PUT /api/v1/garden/gateway`
   updates `state.gateways` and chirps, but does not refresh the tools projection.
   Result: `garden/services` returns the gateway, `garden/tools` does not.

2. **Stale tools stream**: The orchestrator subscribes to
   `garden/tools/stream` on the tended stone. If that stone has no local MongoDB
   and remote beacons do not arrive (UDP issues, Windows host, etc.), the
   orchestrator never discovers remote MongoDB instances — despite the topology
   endpoint showing them correctly.

3. **Notification burden**: Every new data source must remember to call
   `refresh_local_tools_projection()`. Forgetting creates silent divergence
   between endpoints. This bug class is structural, not accidental.

4. **Redundant broadcast traffic**: Three beacon types carry overlapping data
   on the same UDP multicast group.

## Decision

Replace `ToolsCache`, `StorageCache`, and `state.gateways` with a single
**Garden Registry** — one write-through cache per stone that holds all
`GardenTool` entries (offerings, gateways, storage) from all known stones.

### Core Principle

**The registry is the domain boundary.** All mutations write through it.
All queries read from it. No endpoint reads source state directly.

### Registry Data Model

```rust
pub struct GardenRegistry {
    entries: BTreeMap<RegistryKey, RegistryEntry>,
    cursor: u64,
    history: VecDeque<RegistryDelta>,   // last N deltas for SSE replay
}

/// Composite key: deterministic, prefix-scannable by stone
pub struct RegistryKey {
    pub stone_id: String,
    pub fqid: String,
    pub category: String,       // "offering", "orchestrator", "storage"
}

pub struct RegistryEntry {
    pub tool: GardenTool,       // TOOLS-0002 contract (unchanged)
    pub version: u64,           // per-entry monotonic version
    pub origin: EntryOrigin,
    pub expires_at: Option<Instant>,
}

pub enum EntryOrigin {
    /// This stone's offerings or storage — persisted to disk
    Local,
    /// Received from a remote stone via beacon
    Announced { stone_id: String },
    /// Registered via gateway API — ephemeral, TTL-based
    Registered,
}
```

### Write Path

Every mutation goes through the registry. No source writes to its own silo:

```rust
// Offering planted → write adapter
registry.upsert(offering_to_entry(&offering, &stone));

// Gateway registered → write adapter
registry.upsert(gateway_to_entry(&registration, &stone));

// Seed bank mounted → write adapter
registry.upsert(seedbank_to_entry(&announcement, &stone));

// Remote beacon received → bulk merge
registry.merge_remote(stone_id, remote_entries);
```

Each `upsert` increments the entry version and the global cursor, appends a
`RegistryDelta` to the history, and publishes to the broadcast channel. The
SSE stream, UDP beacon, and any local subscribers all receive the delta
automatically.

### Read Path

All endpoints project from the registry with filters:

| Endpoint | Filter |
|----------|--------|
| `stone/tools` | `origin == Local \|\| origin == Registered`, this stone |
| `garden/tools` | all entries |
| `stone/services` | this stone, project as `FoundService` |
| `garden/services` | all entries, project as `FoundService` |
| `garden/tools/stream` | subscribe to delta channel |

No endpoint reads from `state.offerings`, `state.gateways`, or a separate
storage cache. The registry is the single source of truth for "what's available."

### Sync Protocol

One beacon type (`REGISTRY_BEACON`) replaces `TOOLS_BEACON` and `STORAGE_BEACON`:

| Trigger | Action |
|---------|--------|
| Local entry changed | Immediate delta broadcast via UDP |
| Every 30s | Full snapshot broadcast for convergence |
| New stone joins (STONE_CHIRP from unknown peer) | Immediate snapshot exchange |
| Stone goes offline (STONE_GOODBYE) | Remove all entries for that stone |

The snapshot includes entry versions for dedup. Remote stones merge
with last-writer-wins semantics (higher version wins for same key).

### Topology Separation

`TopologyCache` remains separate. It answers "who is online and what hardware
do they have" — a different concern from "what tools are available."

`STONE_CHIRP` is stripped of its `services` list. Services are the registry's
domain. Chirps carry stone identity, capabilities, and health only.

### Storage Semantics

Storage entries carry role metadata (Primary/Dormant, pin_id) as tags or
structured metadata within the `GardenTool`:

```rust
tool.tags = vec!["role:primary", "pin:abc123"];
// or
tool.service.metadata = Some(json!({"role": "primary", "pin_id": "abc123"}));
```

The seed bank orchestration task reads entries with `category == "storage"`
from the registry, computes roles, and writes updated entries back. Same logic,
same write path.

### Gateway TTL

Gateway entries have `expires_at`. A reaper task runs every 15s, removing
expired entries and emitting removal deltas. Orchestrators refresh via PUT
every 30s (well within the 60s TTL). No separate `state.gateways` needed.

### Persistence

The registry is in-memory with these persistence rules:

| Origin | Persisted? | Recovery |
|--------|-----------|----------|
| Local (offerings) | Yes — via existing offering persistence | Loaded at startup |
| Local (storage) | Yes — via seed bank manifests | Scanned at startup |
| Registered (gateways) | No | Re-registered by orchestrators |
| Announced (remote) | No | Re-received via beacons |

On startup: load persisted offerings and scan local seed banks → write into
registry → broadcast initial beacon.

## Consequences

### Positive

- **"Forgot to refresh" bugs become impossible** — there is no projection to
  refresh. The registry IS the state. Write adapters are 3-5 lines each.
- **One beacon, one cache** — eliminates redundant UDP traffic and parallel
  data paths.
- **Adding a new tool type** = write a 5-line adapter. No new cache, no new
  beacon, no new projection path, no new query logic.
- **Tools stream sees everything** — orchestrators subscribing to the SSE
  stream get gateway registrations, storage changes, and offering changes
  through one channel.
- **Consistent query results** — `garden/tools` and `garden/services` always
  agree because they read the same data.

### Negative

- **Breaking change to beacon protocol** — all stones must upgrade together.
  Mitigated: stones are managed by the same deployment tooling.
- **Larger beacon payload** — unified beacon carries offerings + gateways +
  storage vs. separate lightweight beacons. Acceptable: total data volume is
  small (dozens of entries), and beacon dedup prevents unnecessary broadcasts.
- **Migration complexity** — three caches and two beacon types must be replaced
  simultaneously. Mitigated: the GardenTool contract (TOOLS-0002) is retained
  unchanged; only the cache and propagation layers are rebuilt.

## What Gets Replaced

| Current | Replaced By |
|---------|-------------|
| `ToolsCache` (`domain/tools/cache.rs`) | `GardenRegistry` |
| `StorageCache` (`domain/storage_cache.rs`) | Registry entries with `category="storage"` |
| `state.gateways` (HashMap) | Registry entries with `origin=Registered`, TTL |
| `TOOLS_BEACON` announcement | `REGISTRY_BEACON` |
| `STORAGE_BEACON` announcement | `REGISTRY_BEACON` |
| `project_local_tools()` | Write adapters (push on change) |
| `service_discovery.rs` scattered find logic | Registry query with filters |
| `TopologyEntry.services` | Removed — registry handles service advertisement |

## What Stays

| Component | Why |
|-----------|-----|
| `GardenTool` contract (TOOLS-0002) | Retained unchanged as the entry type |
| `TopologyCache` | Stone liveness and capabilities (separate concern) |
| `STONE_CHIRP` | Liveness heartbeat (stripped of services) |
| `state.offerings` (disk persistence) | Durable store — writes through to registry |
| Seed bank manifests (disk) | Durable store — writes through to registry |
| SSE streaming infrastructure | Retained — delta channel fed by registry |

## Implementation

### Phase 1: Registry Core

Create `GardenRegistry` in `moss/src/domain/registry.rs` with:
- `BTreeMap<RegistryKey, RegistryEntry>` storage
- `upsert()`, `remove()`, `merge_remote()`, `remove_stone()` mutations
- Delta history + broadcast channel (move from ToolsCache)
- TTL reaper for gateway entries
- Query methods with filter predicates

### Phase 2: Write Adapters

Replace projection with direct writes:
- Offering persist → `registry.upsert(offering_to_entry(...))`
- Gateway PUT/DELETE → `registry.upsert/remove(gateway_to_entry(...))`
- Storage mount/change → `registry.upsert(seedbank_to_entry(...))`
- Remote beacon → `registry.merge_remote(stone_id, entries)`

### Phase 3: Read Migration

Point all query endpoints at the registry:
- `garden/tools` + `garden/tools/stream` → read from registry (replace ToolsCache)
- `garden/services` + `stone/services` → read from registry (replace service_discovery.rs find logic)
- `garden/storage` discovery → read from registry (replace StorageCache)

### Phase 4: Beacon Unification

- Introduce `REGISTRY_BEACON` (carries Vec<GardenTool> + versions)
- Remove `TOOLS_BEACON` and `STORAGE_BEACON` handlers
- Strip `services` from `STONE_CHIRP`
- Update coordinator to handle unified beacon

### Verification

```bash
cargo check --all
cargo test --all
cargo clippy -- -D warnings
cd src/orchestrators/mongodb && cargo check
cd src/orchestrators/ollama && cargo check
```

Manual verification:
- `garden-rake find mongodb` returns same results as `curl /api/v1/garden/tools?fqid=mongodb`
- Gateway registration via orchestrator appears in tools stream within 1s
- Storage beacon changes appear in tools stream within 1s
- New stone joining receives full registry snapshot within 5s

## References

- TOOLS-0001: Garden Tools Domain (predecessor — replaced)
- TOOLS-0002: GardenTool Unified Contract (retained — entry type)
- STORAGE-0003: Storage Beacon Protocol (predecessor — replaced)
- ORCH-0004: Gateway Announcement (gateway registration API retained, cache replaced)
