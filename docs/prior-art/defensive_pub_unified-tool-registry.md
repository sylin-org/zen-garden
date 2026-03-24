# Defensive Publication: Unified Tool Registry with Origin-Tracked Write-Through Cache

**Inventor**: Leonardo Milson Botinelly Soares (Leo Botinelly)
**Disclosure Date**: 2026-03-24
**Field**: Distributed service registries and real-time state synchronization
**Keywords**: unified registry, origin tracking, TTL eviction, cursor-based SSE, capability queries, write-through cache

---

## 1. Problem Statement

Distributed systems that manage heterogeneous infrastructure resources — container-based services, storage volumes, AI model orchestrators, database gateways — require registries that track what is available and where. Existing solutions impose separate registries for each resource type. Consul tracks services. etcd stores key-value pairs. Kubernetes maintains typed API resources. ZooKeeper provides hierarchical coordination.

No existing system unifies these heterogeneous resource types under a single registry with the following combined properties:

1. **Origin tracking** that records how each entry entered the registry (local projection, peer announcement, or gateway self-registration) and ties lifecycle management to the origin.
2. **TTL-based eviction** that automatically reaps entries from crash-prone external registrants without requiring explicit deregistration.
3. **Cursor-monotonic delta streaming** that enables clients to reconnect and replay only the changes they missed, without receiving a full snapshot every time.
4. **Capability-predicate queries** with AND semantics across structured capability types, enabling queries such as "find all entries that have both `model:llama3` AND `model:nomic-embed-text`."
5. **A single beacon type** that replaces per-domain announcement protocols (previously separate beacons for tools, storage, and topology carried overlapping data on the same transport).

### Prior Art Differentiation

| System | Unified Types | Origin Tracking | TTL Eviction | Cursor SSE | Capability Queries |
|--------|:---:|:---:|:---:|:---:|:---:|
| Consul | No (services only) | No | TTL (health-check based) | Blocking queries (not SSE) | Tags (unstructured, no AND) |
| etcd | No (key-value) | No | Lease-based TTL | Watch (gRPC, not SSE) | No (prefix scan only) |
| Kubernetes API | No (typed resources) | No | Garbage collection | Watch (HTTP, no cursor replay) | Label selectors (no capability structure) |
| ZooKeeper | No (hierarchical nodes) | No | Ephemeral nodes (session-bound) | Watches (single-fire) | No |
| **Disclosed system** | **Yes** | **Yes (3 origins)** | **Yes (per-entry, configurable)** | **Yes (cursor + replay)** | **Yes (AND semantics)** |

---

## 2. Description of the Invention

### 2.1 Registry Data Model

The disclosed system maintains a single write-through cache per node (called a "stone" in the implementation) holding all infrastructure tool entries from all known nodes. The registry is the authoritative source of truth for all query endpoints. No endpoint reads from source state directly.

The cache is "write-through" in the sense that every mutation to the in-memory `BTreeMap` simultaneously produces a delta event on the broadcast channel — the write is visible to all subscribers (SSE clients, beacon emitters) at the moment it occurs. The registry is not persisted to durable storage; it is reconstructed from authoritative sources on restart (local offering state from disk, remote entries from beacons, gateway entries from re-registration). Alternative implementations could add a persistence layer (e.g., SQLite WAL, append-only log, Raft consensus log) beneath the in-memory map without altering the write-through broadcast semantics. The term "write-through" refers to the immediate broadcast propagation path, not to a durable storage tier.

#### Entry Structure

Each entry in the registry consists of:

```
RegistryEntry {
    tool:       GardenTool,         // The resource description (TOOLS-0002 contract)
    version:    u64,                // Per-entry monotonic version, incremented on each upsert
    origin:     EntryOrigin,        // Who wrote this entry (Local, Gateway, Announced)
    expires_at: Option<Instant>,    // TTL deadline (Gateway entries only; None = permanent)
}
```

The registry key is a composite string: `"{stone_id}:{fqid}:{category}"`, where:
- `stone_id` is the unique identifier of the node that hosts the resource.
- `fqid` is the fully-qualified identifier of the resource (e.g., `"mongodb"`, `"ollama:prod"`, a replica set ID for storage).
- `category` is one of `"offering"`, `"orchestrator"`, or `"storage"`.

This composite key is deterministic and prefix-scannable by node, enabling efficient per-node operations.

The `GardenTool` structure contains:

```
GardenTool {
    fqid:         String,           // Bare canonical name
    tool:         ToolIdentity,     // Category, type, version, capabilities
    stone:        Stone,            // Node identity (id, name, endpoint)
    service:      ServiceInfo,      // Runtime status, health, ports, connection
    capabilities: Vec<Capability>,  // Structured capability declarations
}
```

Each capability is a typed key-value pair (e.g., `cap_type: "model"`, `item: "llama3"`), enabling structured queries rather than unstructured tag matching.

#### Implementation Evidence

- `GardenRegistryInner` struct in `src/moss/src/domain/garden_registry.rs` — the core registry with `BTreeMap<String, RegistryEntry>`, monotonic cursor, and bounded delta history (`VecDeque<ToolDelta>`).
- `GardenTool` struct in `src/common/src/tools/types.rs` — the shared contract.
- `ToolQuery` struct in `src/moss/src/domain/garden_registry.rs` — the filter predicate with `fqid`, `category`, `status`, `stone_id`, and `capabilities` fields.
- ADR: `docs/decisions/TOOLS-0003-unified-garden-registry.md`.

### 2.2 Entry Origin and Lifecycle Ownership

Each entry carries an `EntryOrigin` enum that determines who owns its lifecycle:

```
enum EntryOrigin {
    Local,                          // Projected from local offerings or storage volumes
    Gateway,                        // Registered directly by an orchestrator gateway
    Announced { stone_id: String }, // Received from a remote node via beacon
}
```

**Lifecycle rules by origin:**

| Origin | Writer | Lifecycle Owner | Persistence |
|--------|--------|-----------------|-------------|
| `Local` | `reconcile_local()` — called when offerings change or storage mounts/unmounts | The local node's offering and storage subsystems | Persisted via offering state and storage manifests on disk |
| `Gateway` | `PUT /api/v1/garden/gateway` — orchestrators self-register | TTL reaper task (runs every 15 seconds) | Not persisted; orchestrators re-register on restart |
| `Announced` | Beacon reception from remote nodes | Beacon reconciliation; entries removed on `STONE_GOODBYE` | Not persisted; re-received via beacons |

Origins are **mutually exclusive per entry key**. A single entry cannot have multiple origins simultaneously. This is enforced by the composite key structure: an entry for the same resource on the same node always has the same key, so an upsert from a different origin overwrites the previous origin. The design deliberately avoids "multi-origin" tracking because it would create ambiguous lifecycle ownership (who is responsible for removing the entry?). If a local offering is also visible to remote peers, the local node holds a `Local` entry and each remote node independently holds an `Announced` entry — these are separate entries with different `stone_id` prefixes in their composite keys.

This origin-based ownership eliminates a class of bugs where different subsystems independently manage overlapping state. Each entry has exactly one lifecycle owner determined by its origin.

### 2.3 Write Paths

All mutations flow through `GardenRegistryInner::upsert()` and `GardenRegistryInner::remove()`. There are no separate caches, no separate state stores. The write paths are:

1. **Local offering change** (plant, remove, status change): The offering subsystem calls `reconcile_local()`, which projects the current offering list into `GardenTool` entries and upserts them. Entries for removed offerings are deleted.

2. **Beacon reception** (remote node announces its tools): The beacon handler calls `merge_remote(stone_id, remote_entries)`, which upserts all received entries with `Announced` origin. Entries previously held for that node but absent from the new beacon are removed.

3. **Gateway registration** (orchestrator self-registers): The `PUT /api/v1/garden/gateway` handler calls `upsert_with_expiry()` with a configurable TTL (default 60 seconds). The orchestrator refreshes every 30 seconds. If the orchestrator crashes, the entry expires silently.

4. **Storage mount/unmount**: The storage subsystem calls `reconcile_local()` with storage entries projected from the current volume state.

Each `upsert` performs content-equivalence checking. Two entries are content-equivalent when the following fields of their `GardenTool` are identical: `service.status`, `service.health`, `service.connection` (endpoint URL and port), and `capabilities` (the full ordered list of capability pairs). Fields excluded from the equivalence check include `service.uptime`, `service.metrics`, and any other volatile runtime statistics that change frequently without representing a meaningful state transition. The origin must also match. If the incoming tool is content-equivalent to the existing entry and the origin matches, the operation is a silent no-op (only refreshing TTL if applicable). This eliminates redundant delta emissions on periodic reconciliation.

When content differs, the method increments the per-entry version, increments the global cursor, appends a `ToolDelta` to the bounded history ring buffer, and publishes the delta to a broadcast channel.

#### Pseudocode: Upsert with Content Deduplication

```
function upsert(tool, origin, expires_at):
    key = build_key(tool.stone_id, tool.fqid, tool.category)

    if entries[key] exists AND content_equivalent(entries[key].tool, tool) AND entries[key].origin == origin:
        // Content unchanged — refresh TTL only
        if expires_at is not None:
            entries[key].expires_at = expires_at
        return None  // No delta emitted

    version = (entries[key].version + 1) if entries[key] exists else 1

    entries[key] = RegistryEntry {
        tool:       tool,
        version:    version,
        origin:     origin,
        expires_at: expires_at,
    }

    delta = ToolDelta {
        event_id:  generate_guidv7(),
        cursor:    increment_global_cursor(),
        timestamp: now_utc(),
        kind:      Upsert,
        fqid:      tool.fqid,
        tool_key:  key,
        revision:  version,
        tool:      Some(tool),
    }

    append_to_history(delta)
    broadcast(delta)
    return Some(delta)
```

### 2.4 TTL-Based Eviction for Gateway Entries

Gateway entries (orchestrators such as an Ollama proxy or MongoDB coordinator) register via HTTP PUT and include a TTL. A background reaper task runs every 15 seconds, scanning all entries for expired `expires_at` timestamps. Expired entries are removed, emitting removal deltas that propagate to all SSE subscribers.

This design provides crash-tolerant deregistration: if an orchestrator process terminates unexpectedly, its entries disappear within one TTL period (default 60 seconds) without requiring any external health check mechanism.

The TTL is per-entry, not per-session. Each PUT refreshes the individual entry's expiry. An orchestrator managing multiple entries (e.g., multiple model endpoints) can selectively refresh only the entries that remain valid.

### 2.5 Cursor-Monotonic Delta Streaming

The registry maintains a global monotonic cursor (u64) that increments with every mutation. Each `ToolDelta` records the cursor value at the time of the mutation. A bounded ring buffer (`VecDeque<ToolDelta>`, default capacity 4096) retains recent deltas for replay.

#### SSE Stream Protocol

The SSE endpoint (`GET /api/v1/garden/tools/stream`) implements a three-phase stream:

**Phase 1 — Snapshot**: On connection, the server reads the registry under a lock, generates a filtered snapshot of all matching entries, and emits it as a single `tools.snapshot` SSE event with the current cursor as the event ID. If the server has restarted since the client's last connection, the cursor namespace has reset. The client detects this because the snapshot's cursor value will be lower than or close to its stored cursor. Since the snapshot provides complete current state, the client replaces its local mirror entirely with the snapshot contents, discards its stored cursor, and adopts the new cursor from the snapshot. This ensures convergence across server restarts without requiring persistent cursor storage on the server.

**Phase 2 — Replay**: If the client provides a `since` query parameter or a `Last-Event-ID` header, the server replays all deltas with cursor values greater than the provided value. This enables gap-free reconnection.

**Phase 3 — Live stream**: The server subscribes to the broadcast channel and emits each new delta as it arrives, filtered by the client's query parameters. Deltas with cursor values at or below the snapshot cursor are suppressed (deduplication against the snapshot).

A heartbeat event is emitted every 15 seconds to maintain the SSE connection through proxies.

**Stale cursor handling**: When a client provides a `since` cursor older than the oldest delta in the ring buffer (i.e., the buffer has wrapped), the server cannot replay the gap. In this case, the snapshot (Phase 1) already provides the complete current state, so the replay phase is skipped entirely and the stream transitions directly to live mode. The client converges via the snapshot alone. This is functionally equivalent to a fresh connection. The ring buffer capacity (default 4096) is sized so that under normal operation (sub-second mutation rates), the buffer covers several hours of history. For deployments with higher mutation rates, the capacity is configurable. Alternative implementations could use unbounded logs with compaction, persistent event stores, or snapshot-plus-WAL architectures to extend replay depth.

#### Pseudocode: SSE Stream with Cursor Replay

```
function stream_tools(query, resume_cursor):
    filter = parse_query(query)

    // Phase 1: Snapshot under lock
    lock registry:
        (cursor, tools) = registry.snapshot(filter)
        replay = registry.deltas_since(resume_cursor, filter) if resume_cursor > 0 else []

    emit SSE event { type: "tools.snapshot", id: cursor, data: { cursor, tools } }

    // Phase 2: Replay missed deltas
    for delta in replay:
        emit SSE event { type: delta_event_type(delta), id: delta.event_id, data: delta }

    // Phase 3: Live stream
    subscribe to broadcast channel
    loop:
        select:
            delta = receive from channel:
                if delta.cursor > cursor AND filter.matches(delta):
                    emit SSE event { type: delta_event_type(delta), id: delta.event_id, data: delta }
            shutdown_token cancelled:
                break
            15 seconds elapsed:
                emit SSE event { type: "tools.heartbeat", data: { cursor, timestamp: now() } }
```

#### Implementation Evidence

- `stream_garden_tools_v1()` in `src/moss/src/api/v1/tools.rs` — the SSE endpoint with snapshot, replay, and live phases.
- `ToolsSnapshotResponse` struct with `cursor`, `tools`, and `replay` fields.
- `deltas_since()` method on `GardenRegistryInner`.
- `Last-Event-ID` header parsing for browser-native SSE reconnection.

### 2.6 Capability-Predicate Queries with AND Semantics

Query parameters support structured capability filtering:

```
GET /api/v1/garden/tools?capability=model:llama3,model:nomic-embed-text
```

The capability string is parsed into a list of `CapabilitySelector { cap_type, item }` pairs. The parsing grammar is: the query parameter value is split on `,` to produce individual selector strings; each selector string is split on the first `:` to produce `(cap_type, item)`. If no `:` is present, the entire string is treated as `cap_type` with a wildcard `item` (matches any item of that type). Colons within the `item` portion are literal (e.g., `model:llama3:70b` parses as `cap_type="model"`, `item="llama3:70b"`).

An entry matches only if it satisfies **all** selectors (AND semantics). Each selector checks whether the entry's capabilities list contains a matching `(cap_type, item)` pair. A wildcard-item selector matches any capability of the given type.

This enables queries such as "find all Ollama instances that have both llama3 and nomic-embed-text loaded" — a query that tag-based systems (Consul, Kubernetes labels) cannot express without client-side filtering. Future extensions could add OR semantics via repeated query parameters (`?capability=model:llama3&capability=model:mistral` for OR groups) or negation (`!model:deprecated`) without changing the fundamental parsing grammar.

### 2.7 Single Beacon Type

The disclosed system consolidates three previously separate beacon types (`TOOLS_BEACON`, `STORAGE_BEACON`, `STONE_CHIRP` services list) into a single `REGISTRY_BEACON` that carries the node's complete tool snapshot. This eliminates redundant broadcast traffic on the UDP multicast group and removes the class of bugs where one beacon type is refreshed but another is stale.

#### Beacon Wire Format

The beacon payload is a JSON-serialized array of `GardenTool` entries representing the sending node's complete tool set. The payload is wrapped in a `UdpAnnouncement` envelope containing the announcement type discriminator, the sender's `stone_id`, and a UTC timestamp. The envelope is serialized to bytes and sent via the P2P transport singleton (see Multicast-First Discovery defensive publication for transport details).

**MTU handling**: The beacon payload is subject to UDP datagram size limits. For payloads exceeding the network MTU (typically 1500 bytes on Ethernet, minus IP/UDP headers), the system relies on IP-layer fragmentation. In practice, a node with 10-20 tools produces a payload of 2-8 KB, which fragments into 2-6 UDP datagrams at the IP layer. For deployments with very large tool sets (100+ entries), alternative approaches include: (a) delta-only beacons that send only changes since the last beacon, (b) application-layer chunking with sequence numbers, or (c) switching to TCP for large payloads while keeping UDP for lightweight heartbeats.

The beacon includes entry versions for deduplication. Remote nodes merge with last-writer-wins semantics (higher version for the same key wins). When a beacon is received, the receiver calls `merge_remote(stone_id, entries)` which upserts all received entries and removes entries previously held for that `stone_id` that are absent from the new beacon (full-state reconciliation).

### 2.8 Client-Side Mirror

### 2.8a Registry Federation and Partitioning

The disclosed system operates one registry instance per node. In the "tended" model, one node (the tending node) aggregates all entries from all peers into its registry via beacon reception, becoming the authoritative query endpoint for the garden. This is a single-writer-per-origin, multi-reader federation: each node is the sole writer for its `Local` entries, and the tended node aggregates all origins.

**Split-brain behavior**: During a network partition, two nodes may both accept `Gateway` registrations for the same orchestrator or both project `Local` entries for their own resources. When the partition heals and beacons resume, entries reconverge via full-state beacon reconciliation (Section 2.7): the receiving node replaces all `Announced` entries for a given `stone_id` with the beacon's contents. `Local` entries are never conflicted because they are keyed by `stone_id` — each node's local entries have a unique prefix.

For larger deployments, the registry could be partitioned by category (offerings on one shard, storage on another) or by node group, with cross-shard queries federated at the API layer. The disclosed architecture supports this because the composite key includes category as a component, enabling prefix-scan partitioning.

### 2.8b Client-Side Mirror

On the consumer side, a client library (implemented in .NET as `ZenGardenClient`) maintains a `ConcurrentDictionary<string, ZenGardenToolSnapshot>` that mirrors the server-side registry. The client connects to the SSE stream, processes the snapshot event to populate the dictionary, and applies each subsequent delta (upsert or remove) atomically. On reconnection, the client sends its last-known cursor via the `since` parameter, receiving only the deltas it missed.

---

## 3. Claims

1. A unified service registry for distributed infrastructure comprising: a single write-through cache holding entries from heterogeneous resource types (container services, storage volumes, orchestrator gateways) under a common entry structure; each entry carrying a composite key of node identifier, resource identifier, and category; wherein all query endpoints read exclusively from this registry and no endpoint reads from source state directly.

2. An origin-tracking mechanism for registry entries comprising: an enumerated origin type (Local, Gateway, Announced) assigned to each entry at write time; lifecycle ownership rules determined by the origin type; wherein Local entries are managed by local state reconciliation, Gateway entries are managed by TTL expiration, and Announced entries are managed by beacon reconciliation; the origin assignment preventing lifecycle ownership conflicts across subsystems.

3. A TTL-based eviction mechanism for self-registered entries comprising: per-entry expiration timestamps set at registration time; a periodic reaper task that scans entries and removes those past their expiration; removal emitting delta events to all subscribers; wherein the registrant refreshes expiry on each heartbeat PUT and crash-induced absence causes automatic deregistration within one TTL period without external health checking.

4. A cursor-monotonic delta streaming protocol for real-time state synchronization comprising: a global monotonic cursor incremented on each registry mutation; a bounded ring buffer of recent deltas; an SSE endpoint that emits a filtered snapshot on connection, replays deltas since a client-provided cursor value, and then streams live deltas; cursor-based deduplication preventing duplicate delivery of events present in both the snapshot and the replay; wherein clients reconnect with their last-known cursor and receive exactly the mutations they missed.

5. A capability-predicate query mechanism for a heterogeneous infrastructure registry comprising: structured capability declarations on each registry entry as typed key-value pairs where the type discriminates the capability domain (e.g., `model`, `gpu`, `protocol`) and the item identifies the specific capability within that domain; query parameters parsed by splitting on comma delimiters and then on the first colon to produce a list of `(type, item)` capability selectors; server-side AND-semantics filtering where an entry matches only if its capabilities list contains a matching pair for every provided selector; the filtering applied at the registry read path under the same lock as snapshot generation to ensure consistency between query results and the SSE cursor; enabling cross-type queries such as "find entries with both `model:llama3` AND `gpu:cuda`" without requiring client-side post-filtering or multiple round-trips.

6. A content-equivalence deduplication mechanism for registry upserts comprising: comparison of incoming entry content against existing entry content on a defined subset of fields (service status, health, connection endpoint, and capability list) while excluding volatile runtime statistics (uptime, metrics counters); silent TTL refresh when the compared fields are unchanged and the origin matches; delta emission and version increment only when the compared fields materially differ; reducing broadcast traffic and SSE event volume during periodic reconciliation cycles where entries are re-projected without change.

---

## 4. Implementation Evidence

| Component | Location |
|-----------|----------|
| Registry core | `src/moss/src/domain/garden_registry.rs` — `GardenRegistryInner` |
| Shared types | `src/common/src/tools/types.rs` — `GardenTool`, `ToolDelta`, `CapabilitySelector` |
| SSE endpoint | `src/moss/src/api/v1/tools.rs` — `stream_garden_tools_v1()`, `list_garden_tools_v1()` |
| Gateway API | `src/moss/src/api/v1/gateway.rs` — PUT handler with TTL |
| ADR | `docs/decisions/TOOLS-0003-unified-garden-registry.md` |
| Client mirror | Koan Framework `ZenGardenClient.cs` — `ConcurrentDictionary<string, ZenGardenToolSnapshot>` |

---

## 5. Public Domain Dedication

This document is published as a defensive disclosure to establish prior art. The inventor(s) dedicate this disclosure to the public domain and assert no patent rights over the described inventions. All rights to use, implement, and build upon these inventions are hereby granted to the public.

---

## Antagonist Review Log

### Pass 1
**Antagonist:** (1) "Write-through cache" undefined — write-through to what? Competitor could patent persistent variant. (2) Beacon wire format missing — MTU handling unspecified, competitor could patent chunked beacons. (3) Capability selector parsing grammar missing — delimiter ambiguity. (4) Ring buffer overflow behavior undocumented — stale cursor reconnection not covered. (5) "Content-equivalence" never formally defined.
**Author revision:** Added write-through semantics clarification (Section 2.1), beacon wire format and MTU handling (Section 2.7), capability parsing grammar with colon/wildcard rules (Section 2.6), stale cursor fallback-to-snapshot behavior (Section 2.5), and formal content-equivalence field list (Section 2.3).

### Pass 2
**Antagonist:** (1) Server restart resets cursor namespace — client convergence unspecified. (2) Origin mutual exclusivity unstated — competitor could patent multi-origin tracking. (3) Claim 5 reads as abstract set filtering (section 101 exposure).
**Author revision:** Added server restart cursor handling via snapshot replacement (Section 2.5 Phase 1), origin mutual exclusivity explanation with rationale (Section 2.2), and strengthened Claim 5 with concrete parsing mechanics and lock consistency.

### Pass 3
**Antagonist:** (1) No mention of registry partitioning/federation — competitor could patent partitioned variant. (2) Split-brain behavior during network partition undocumented.
**Author revision:** Added Section 2.8a covering federation model, split-brain reconciliation via full-state beacons, and category-based partitioning extensibility.

### Pass 4
**Antagonist:** Minor: Claim 6 should reference the specific equivalence fields defined in Section 2.3.
**Author revision:** Updated Claim 6 to enumerate the specific compared fields and exclusions.

### Final Status
CLEARED — Antagonist found no further weaknesses. Safe to publish.
