# Zen Garden Tools Domain: Refactor + Implementation Proposal

**Status**: Implemented (Greenfield)  
**Date**: 2026-02-06  
**Audience**: Moss maintainers, adapter authors, API consumers  
**Implementation Report**: `docs/archive/proposals/tools-domain-implementation.md`  
**User Guide**: `docs/guides/tools-domain.md`

---

## Executive Summary

Zen Garden needs a **normative, automation-grade** event surface for garden tools, independent from presentation concerns.

This proposal introduces a dedicated **Tools** bounded context with:

1. A canonical identifier format: `tool_fqid = "{tool-type}:{fqid}"`  
   Examples: `offering:ollama::dev`, `seed-bank:seed-beautiful-garden`, `seed-bank:default`
2. A new inter-Moss announcement model: **Tools Beacon** (expanding current storage-beacon concern to all tools)
3. A new garden API surface in Moss: `GET /api/v1/garden/tools` and `GET /api/v1/garden/tools/stream`
4. Strict stream semantics for adapters: snapshot-first, cursored deltas, replay support, correlation fields
5. Capability-aware wishful orchestration: if `ollama:modelv1` is requested and missing, Moss pulls it, persists it, and propagates the updated capability set

Design target: **premium semantics, ultra-low cognitive load**.

---

## Implementation Status (2026-02-06)

This proposal has been implemented in Moss and Rake as a greenfield cut:

1. No backward compatibility shims or legacy wrappers.
2. Normative tools APIs delivered at `/api/v1/garden/tools` and `/api/v1/garden/tools/stream`.
3. Inter-Moss `TOOLS_BEACON` propagation delivered for tools deltas.
4. Event-driven readiness delivered for offering wishful and capability-aware wishful flows.
5. Capability snapshots are persisted and re-projected after restart.

For delivered behavior details and verification commands, see:

- `docs/archive/proposals/tools-domain-implementation.md`
- `docs/guides/tools-domain.md`

---

## Greenfield Constraint

This proposal is **greenfield** and explicitly disallows compatibility layers.

Rules:

1. No backward compatibility paths.
2. No shims, wrappers, or translation bridges.
3. No dual-write or dual-read operation across old/new beacon models.
4. No legacy stream fallback for automation consumers.

The implementation is a clean cut to the Tools domain contract.

---

## Problem

Current behavior mixes three different concerns:

1. **Presentation events** (`/api/v1/stone/presence/stream`)  
   Great for Companions and UI, not specified as a normative automation contract.
2. **Inter-Moss state propagation** (`STONE_CHIRP`, `STORAGE_BEACON`)  
   Distributed across topology and storage caches, each with separate pipelines.
3. **Programmatic readiness workflows** (`wishfully` in Rake)  
   Currently polling/retry oriented, and not capability-aware (for example, offering exists but required model/plugin is missing).

This increases cognitive load for adapter authors and creates unclear semantics for event-driven integration.

---

## Design Principles

1. **One concern, one domain**  
   Tool state and tool change semantics live in one bounded context.
2. **Presentation is separate**  
   Presence remains informative and Companion-oriented.
3. **Normative stream contract**  
   Tools stream is explicit about ordering, dedupe, replay, and identity.
4. **Stable identity + friendly aliases**  
   Adapters consume one canonical identity field and optional aliases.
5. **Clean cutover**  
   Implementation is greenfield with no compatibility windows.

---

## DDD Model

### Bounded Contexts

| Context | Responsibility | Owns |
|---|---|---|
| `Offerings` | runtime lifecycle of offering instances | install/start/stop/state |
| `Storage` | seed bank lifecycle and routing state | mount/visibility/health |
| `Topology` | stone liveness and endpoint awareness | online/offline/last_seen |
| `Tools` (new) | unified read model for automation | `tool_fqid`, state, revision, stream |
| `Presence` | human-facing event representation | UX/Companion vocabulary |

### Ubiquitous Language

- **Tool**: a capability-bearing runtime entity currently including `offering` and `seed-bank`
- **Tool FQID**: typed stable addressing string, `"{tool-type}:{fqid}"`
- **Tool Projection**: normalized state snapshot for one tool
- **Tool Delta**: append-only change event over projection
- **Capability Wish**: declarative request for an offering capability item (for example model, plugin, extension)
- **Capability Snapshot**: current complete capability state for an offering tool, included in projection and beacon payloads

---

## SOLID + SoC Application

1. **SRP**  
   `domain/tools/*` owns tool identity, projection, stream semantics only.
2. **OCP**  
   Add new tool types (for example `companion`, `bridge`) without changing stream contract.
3. **LSP**  
   All tool types satisfy the same projection contract (`state`, `revision`, `ready`).
4. **ISP**  
   Separate APIs: `presence` for UX, `tools` for automation.
5. **DIP**  
   API and beacon handlers depend on `ToolsProjector`/`ToolsRepository` traits, not concrete storage.
6. **SoC**  
   Announcement ingestion, projection updates, and client streaming are separate modules.

---

## Canonical Identity

### Primary Field

`tool_fqid: "{tool-type}:{fqid}"`

Examples:
- `offering:ollama::dev`
- `seed-bank:seed-beautiful-garden`
- `seed-bank:default`

### Stability Rule

Because name/default can change, each projection/event also carries immutable identity:

- `tool_uid` (immutable; GUIDv7 or equivalent stable key)
- `aliases[]` (optional selector forms)

This preserves low cognitive load (human-friendly `tool_fqid`) while keeping deterministic replay and dedupe.

---

## Tools Stream Contract (Normative)

### Endpoints

```http
GET /api/v1/garden/tools
GET /api/v1/garden/tools/stream
```

### Query Filters

- `tool_type=offering|seed-bank`
- `tool_fqid=<value>`
- `state=ready|degraded|unavailable`
- `capability=<type>:<item>[,<type>:<item>...]` (offering tools only; for example `model:modelv1,model:modelv2`)
- `since=<cursor>` (for replay)

### Event Types

- `tools.snapshot` (first event on connect)
- `tool.upsert`
- `tool.remove`
- `tool.capability.sync_started`
- `tool.capability.sync_completed`
- `tool.capability.sync_failed`
- `tools.heartbeat`

### Delivery Semantics

1. **Snapshot-first**: stream starts with full current view plus cursor.
2. **At-least-once**: consumers must dedupe using `event_id`.
3. **Replay-capable**: `since` and `Last-Event-ID` supported while retained.
4. **Monotonic revision**: each tool update increments `revision`.
5. **Capability-complete offering updates**: `tool.upsert` for offering tools includes the full capability snapshot, not partial fragments.

### Event Envelope (Required Fields)

```json
{
  "event_id": "019d....",
  "cursor": "8742",
  "timestamp": "2026-02-06T22:15:00Z",
  "tool_fqid": "offering:ollama::dev",
  "tool_uid": "019c26fc-5e46-7ac1-9fbb-f1664790dead",
  "tool_type": "offering",
  "revision": 14,
  "state": "ready",
  "ready": true,
  "stone_id": "019b....",
  "stone_name": "stone-amber-ridge",
  "capabilities": {
    "model": ["modelv1", "modelv2"]
  },
  "capability_revision": 6,
  "capability_delta": {
    "added": { "model": ["modelv1"] },
    "removed": {}
  },
  "job_id": "job_abc123",
  "request_id": "req_123",
  "aliases": ["offering:ollama", "offering:ollama::dev"]
}
```

---

## Capability-Aware Wishfully

### Target Syntax

Canonical capability wish syntax:

```text
<offering-fqid>[<capability>[,<capability>...]]
```

Examples:
- `ollama[modelv1]`
- `ollama[modelv1,modelv2]`
- `ollama::dev[modelv1,modelv2]`
- `postgres[extension:pgvector]`

Nomenclature rule:
- `capability` is the protocol-level term.
- offering-specific labels (model/extension/module) are display aliases from manifests, not global naming.

Shorthand syntax for low cognitive load:
- `ollama:modelv1`

Shorthand expansion rule:
- `ollama:modelv1` expands to `ollama[modelv1]` when the offering declares a single default capability type.
- If ambiguous, Moss rejects and returns an explicit error requiring typed selector form.

### Execution Flow

1. Parse capability wish into `(offering_fqid, capability_type, capability_item)`.
2. Resolve current tool projection for `offering:<offering_fqid>`.
3. If capability already present, return ready immediately.
4. If missing, enqueue `capability.ensure` job for offering-specific executor.
5. Executor performs native action (for example `ollama pull modelv1`).
6. On success, persist updated capability snapshot in offering state.
7. Projector emits `tool.upsert` with full capability snapshot and delta.
8. Moss emits `TOOLS_BEACON` carrying the same new projection revision.
9. Peer Moss instances ingest beacon and update local tools projection/cache.

### Persistence Rule

Capability snapshots are persisted as part of offering runtime state (for example `sub_capabilities` for managed offerings), then rehydrated on boot before first tools snapshot/beacon emission.

This guarantees capability state survives restart and propagates garden-wide.

---

## Inter-Moss Announcement Refactor

### Current

- `STONE_CHIRP` for topology/service liveness
- `STORAGE_BEACON` for storage routing

### Proposed

Introduce **`TOOLS_BEACON`** as unified tool-change announcement across stones.

- Payload carries normalized tool deltas (`upsert`/`remove`) for offerings + seed banks
- Offering tool beacons include full capability snapshot + `capability_revision`
- `STORAGE_BEACON` is removed from the tool-propagation path
- Beacons remain **Moss-to-Moss control plane**, not client API payloads

This aligns with the intent: expand storage-beacon concern into tool-state propagation.

---

## Domain Merge Opportunities

### 1) Merge split read models into Tools projection

Current:
- offering state from registry + topology/service discovery
- storage state from `storage_cache`

Target:
- one `ToolsCache` projection indexed by `tool_fqid` and `tool_uid`
- one query path for adapter-facing state

### 2) Merge announcement ingestion into one router

Current:
- `tasks/coordinator.rs` matches each announcement type inline

Target:
- `infra/communications/announcement_router.rs`
- delegates to `TopologyIngestor`, `ToolsIngestor`, `ElectionIngestor`

### 3) Merge readiness semantics

Current:
- readiness inferred ad hoc from service status + health + retries

Target:
- one readiness policy in `domain/tools/readiness.rs`
- shared by API, beacons, and `wishfully` orchestration

### 4) Separate presentation from automation

Current:
- presence stream used for both UX and potential automation

Target:
- `presence` remains UX domain
- `tools` is normative automation domain

### 5) Merge capability management under Tools

Current:
- offering capabilities managed in isolated API/logic paths
- wishful provisioning and capability pulling are disconnected

Target:
- one capability orchestration service in tools domain
- one event vocabulary for capability sync lifecycle
- one persisted capability snapshot contract for projection + beacons

---

## Proposed Module Layout

### `garden_common`

- `src/common/src/tools/mod.rs` (new)
- `src/common/src/tools/types.rs` (new)
- `src/common/src/tools/event_types.rs` (new)
- `src/common/src/infra/communications/announcement_types.rs` (add `TOOLS_BEACON`)

Key shared types:
- `ToolType`
- `ToolFqid`
- `ToolProjection`
- `ToolDelta`
- `ToolsBeacon`
- `CapabilityWish`
- `CapabilitySnapshot`
- `CapabilityDelta`

### `moss` Domain

- `src/moss/src/domain/tools/mod.rs` (new)
- `src/moss/src/domain/tools/cache.rs` (new)
- `src/moss/src/domain/tools/projector.rs` (new)
- `src/moss/src/domain/tools/readiness.rs` (new)
- `src/moss/src/domain/tools/events.rs` (new)
- `src/moss/src/domain/tools/capability_orchestrator.rs` (new)

### `moss` Infra + API

- `src/moss/src/infra/tools/beacon.rs` (new)
- `src/moss/src/api/v1/tools.rs` (new)
- `src/moss/src/bootstrap/router.rs` (add tools routes)
- `src/moss/src/app_state.rs` (add tools cache + tools stream channel)

---

## API Draft

### Snapshot

```http
GET /api/v1/garden/tools
```

Response:

```json
{
  "cursor": "8742",
  "tools": [
    {
      "tool_fqid": "offering:ollama::dev",
      "tool_uid": "019c26fc-5e46-7ac1-9fbb-f1664790dead",
      "tool_type": "offering",
      "state": "ready",
      "ready": true,
      "revision": 14,
      "stone": { "id": "019b...", "name": "stone-amber-ridge" },
      "connection": { "protocol": "http", "port": 11434, "uris": ["http://stone-amber-ridge.local:11434"] },
      "capabilities": { "model": ["modelv1", "modelv2"] },
      "capability_revision": 6,
      "aliases": ["offering:ollama"]
    }
  ]
}
```

### Stream

```http
GET /api/v1/garden/tools/stream?tool_type=offering&state=ready
Accept: text/event-stream
```

Sequence:
1. `tools.snapshot`
2. zero or more `tool.upsert` / `tool.remove`
3. periodic `tools.heartbeat`

---

## Refactor Plan

### Phase 0: Semantics Freeze

1. Approve `tool_fqid` grammar and alias behavior.
2. Freeze capability wish grammar and shorthand expansion rules.
3. Freeze tool state vocabulary: `ready`, `degraded`, `unavailable`.
4. Freeze stream delivery guarantees.

### Phase 1: Shared Contracts

1. Add `garden_common::tools` types and parser/validator.
2. Add `TOOLS_BEACON` announcement type.
3. Add capability snapshot/delta structures.
4. Add serialization tests for tool and capability envelopes.

### Phase 2: Moss Tools Projection

1. Implement `ToolsCache` and projector.
2. Feed projector from:
   - offering lifecycle domain events
   - storage lifecycle events
   - offering capability mutation events
   - incoming `TOOLS_BEACON`
3. Emit deterministic revisions.

### Phase 3: Capability Wishful Orchestration

1. Implement `capability.ensure` orchestration path in tools domain.
2. Bind offering-specific executors (for example Ollama model pull).
3. Persist capability snapshots on success and emit capability sync events.

### Phase 4: Moss API

1. Add `GET /api/v1/garden/tools`.
2. Add `GET /api/v1/garden/tools/stream`.
3. Implement capability filter, snapshot-first, cursor/replay behavior.

### Phase 5: Beacon Cutover

1. Remove `STORAGE_BEACON` ingestion and emission for tool propagation.
2. Enable `TOOLS_BEACON` as the single inter-Moss tools announcement.
3. Delete dead paths and tests tied to dual-beacon assumptions.

### Phase 6: Consumer Cutover

1. Update Rake wishful flow to support capability wishes (`ollama:modelv1`) and wait on tools stream by default.
2. Document adapter integration flow in driver docs.
3. Keep presence isolated to UX/Companion concerns only.

---

## Touchpoints (Initial)

- `src/common/src/infra/communications/announcement_types.rs`
- `src/common/src/lib.rs`
- `src/moss/src/app_state.rs`
- `src/moss/src/bootstrap/router.rs`
- `src/moss/src/tasks/coordinator.rs`
- `src/moss/src/domain/mod.rs`
- `src/moss/src/api/v1/mod.rs`
- `src/moss/src/api/v1/offering_capabilities.rs`
- `src/rake/src/commands/discovery/find.rs`

New files:
- `src/common/src/tools/mod.rs`
- `src/common/src/tools/types.rs`
- `src/common/src/tools/event_types.rs`
- `src/moss/src/domain/tools/mod.rs`
- `src/moss/src/domain/tools/cache.rs`
- `src/moss/src/domain/tools/projector.rs`
- `src/moss/src/domain/tools/readiness.rs`
- `src/moss/src/domain/tools/events.rs`
- `src/moss/src/domain/tools/capability_orchestrator.rs`
- `src/moss/src/infra/tools/beacon.rs`
- `src/moss/src/api/v1/tools.rs`

---

## Acceptance Criteria

1. A client can consume `GET /api/v1/garden/tools/stream` and deterministically track tool readiness across stones.
2. `tool_fqid` is present in all tool events and stable under replay.
3. Offerings and seed banks are represented in one normalized tool schema.
4. Programmatic workflows do not depend on `presence` semantics.
5. Moss-to-Moss propagation for tool changes is event-driven and independent from client stream consumption.
6. `wishfully` uses event-driven readiness from tools stream (no polling fallback path).
7. Capability wish `ollama:modelv1` triggers model pull when missing.
8. On successful pull, tools stream emits `tool.upsert` with updated capability snapshot and delta.
9. Updated offering capability snapshot is persisted and re-emitted after restart.
10. `TOOLS_BEACON` propagates updated offering capabilities to peer Moss instances.

---

## Risks and Mitigations

1. **Event storm on startup**
   Mitigation: beacon debounce + projection coalescing + bounded replay buffer.
2. **State divergence across stones**
   Mitigation: snapshot reconciliation on connect and periodic beacon refresh.
3. **Identifier churn for seed-bank aliases**
   Mitigation: immutable `tool_uid` + alias lists in payloads.
4. **Contract drift**
   Mitigation: centralize event constants and payload contracts in `garden_common::tools`.

---

## Cutover Notes

- `presence` remains a UX stream for Companions, not a normative automation contract.
- `TOOLS_BEACON` is the sole inter-Moss tool propagation mechanism.
- No Lantern dependency for this proposal; garden tools stream is served by Moss using local aggregated projection.
- Capability snapshots are persisted in offering runtime state and projected as first-class tool data.

---

## Adapter Integration (Reference)

1. Attempt immediate resolution via `GET /api/v1/garden/tools` filtered by `tool_fqid` and optional `capability`.
2. If missing capability and wishful policy allows, request capability wish (for example `ollama[modelv1,modelv2]`).
3. Subscribe to `GET /api/v1/garden/tools/stream?tool_fqid=<...>&capability=model:modelv1,model:modelv2`.
4. React on `tool.upsert` where `ready=true` and capability set contains all requested items.
5. Reconnect/resume using `Last-Event-ID` after network interruption.
