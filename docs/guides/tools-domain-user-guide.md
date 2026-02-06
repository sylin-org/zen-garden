---
audience: [adapter-author, api-client, operator]
doc_type: guide
status: current
last_verified: 2026-02-06
canonical: true
---

# Tools Domain User Guide

This guide covers the automation-grade Tools domain introduced in Moss:

- Snapshot API: `GET /api/v1/garden/tools`
- SSE stream API: `GET /api/v1/garden/tools/stream`
- Unified tool model for offerings and seed banks
- Event-driven readiness for `wishfully` and adapter workflows

---

## 1. Mental Model

Use each surface for its intended concern:

- `presence` stream (`/api/v1/stone/presence/stream`): human-facing activity feed for Companions and UX
- `tools` APIs (`/api/v1/garden/tools*`): normative automation contract for programmatic readiness and routing
- `TOOLS_BEACON` (UDP announcement): Moss-to-Moss control-plane propagation of tool deltas

If you are building adapters or orchestration code, consume the `tools` APIs.

---

## 2. Tool Identity

Canonical identity:

```text
tool_fqid = "{tool-type}:{fqid}"
```

Examples:

- `offering:ollama`
- `offering:ollama:dev`
- `seed-bank:default`
- `seed-bank:seed-beautiful-garden`

Additional identity fields:

- `tool_uid`: immutable identifier (for durable dedupe and reconciliation)
- `aliases[]`: optional alternative selectors

Matching in API filters is case-insensitive. Prefer lowercase in clients.

---

## 3. Snapshot API

Endpoint:

```http
GET /api/v1/garden/tools
```

Optional query parameters:

- `tool_type=offering|seed-bank`
- `tool_fqid=<value>`
- `state=ready|degraded|unavailable`
- `capability=<type>:<item>[,<type>:<item>...]` (offerings only; `|` also accepted)
- `since=<cursor>` (include replay deltas after a known cursor)

Example:

```bash
curl "http://stone-01:7185/api/v1/garden/tools?tool_type=offering&state=ready"
```

Response shape:

```json
{
  "data": {
    "cursor": 42,
    "tools": [
      {
        "tool_fqid": "offering:ollama",
        "tool_uid": "019d...",
        "tool_type": "offering",
        "state": "ready",
        "ready": true,
        "revision": 7,
        "stone_id": "019c...",
        "stone_name": "stone-amber-ridge",
        "connection": {
          "protocol": "http",
          "hostname": "stone-amber-ridge.local",
          "ip": "192.168.1.25",
          "port": 11434,
          "uris": [
            "http://stone-amber-ridge.local:11434",
            "http://192.168.1.25:11434"
          ]
        },
        "capabilities": {
          "model": ["modelv1", "modelv2"]
        },
        "capability_revision": 3,
        "updated_at": "2026-02-06T22:15:00Z"
      }
    ],
    "replay": []
  }
}
```

---

## 4. Stream API

Endpoint:

```http
GET /api/v1/garden/tools/stream
Accept: text/event-stream
```

Supports the same filters as snapshot API, plus `since=<cursor>`.

### Stream behavior

1. First event is always `tools.snapshot`.
2. Optional replay events are emitted next when `since` or `Last-Event-ID` can be resolved.
3. Live deltas follow.
4. `tools.heartbeat` emits every 15 seconds.

### Emitted event types

- `tools.snapshot`
- `tool.upsert`
- `tool.remove`
- `tools.heartbeat`

### Resume semantics

- You can pass `since=<cursor>`.
- You can send `Last-Event-ID`.
- `Last-Event-ID` accepts either:
  - a numeric cursor (for example `42`)
  - a prior `event_id` value from a `tool.upsert` or `tool.remove` event

### Ordering and dedupe guidance

- Delivery is at-least-once.
- Dedupe by `event_id`.
- Keep per-tool last seen `revision`.
- If replay window is exceeded, re-bootstrap using `GET /api/v1/garden/tools`.

### Remove-event filter caveat

`tool.remove` includes `tool_fqid`, `tool_uid`, and `revision` but no full projection.
Because of that, remove events are only filterable by `tool_fqid` (or unfiltered streams).

---

## 5. Capability-Aware Wishful Flow

A capability wish targets an offering plus a capability item.

Canonical consumption syntax:

```text
<offering-fqn>[<capability>[,<capability>...]]
```

Examples:

- `ollama[model1]`
- `ollama[model1,model2]`
- `ollama:dev[model1,model2]`

Nomenclature rule:

- `capability` is the standard term.
- offering-specific labels such as "model", "extension", or "module" are display aliases from manifests, not protocol-level naming requirements.

Typed selectors are still supported when needed (for offerings with multiple capability types):

- `postgres[extension:pgvector]`
- `some-offering[type:item,type:item]`

Shorthand accepted by `garden-rake find`:

- `ollama:modelv1`

Multiple capabilities can be requested at once:

- `ollama[model1,model2]`
- `ollama[model1|model2]`

### Adapter flow

1. Query current state:
   - `GET /api/v1/garden/tools?tool_fqid=offering:ollama&capability=model:model1,model:model2`
2. If missing and policy allows wishful:
   - `POST /api/v1/stone/offerings/ollama/capabilities`
3. Subscribe and wait:
   - `GET /api/v1/garden/tools/stream?tool_fqid=offering:ollama&capability=model:model1,model:model2`
4. Continue when you receive `tool.upsert` with:
   - `ready == true`
   - capability set containing all requested items

Capability ensure request example:

```bash
curl -X POST "http://stone-01:7185/api/v1/stone/offerings/ollama/capabilities" \
  -H "Content-Type: application/json" \
  -d '{"name":"modelv1","type":"model","dry_run":false}'
```

Possible status variants in response `data.status`:

- `exists`
- `dry_run`
- `in_progress`
- `started`

---

## 6. garden-rake Behavior

`garden-rake find` now waits on tools stream readiness (event-driven), including wishful paths.

Examples:

```bash
garden-rake find mongodb wishfully
garden-rake find ollama:modelv1 wishfully
garden-rake find "ollama[model1,model2]" wishfully
garden-rake find "postgres[extension:pgvector]" wishfully
```

The readiness wait timeout in current implementation is 240 seconds.

---

## 7. Persistence and Propagation

- Offering capability mutations are persisted in offering state (`sub_capabilities`).
- On startup, Moss rebuilds local tools projection from persisted offerings and current storage state.
- Local projection deltas are broadcast to peers through `TOOLS_BEACON`.
- Peer Moss nodes ingest beacons and update their local tools cache/stream.

This keeps capability state durable across restarts and visible garden-wide.

---

## 8. Quick Reference

HTTP endpoints:

- `GET /api/v1/garden/tools`
- `GET /api/v1/garden/tools/stream`
- `POST /api/v1/stone/offerings/{name}/capabilities`
- `DELETE /api/v1/stone/offerings/{name}/capabilities/{capability}`

Internal announcement type:

- `tools_beacon` (`TOOLS_BEACON`)

Related documents:

- `docs/proposals/zen-garden-spec-tools-domain.md`
- `docs/proposals/implemented/tools-domain-implementation.md`
- `docs/guides/sub-capabilities.md`
