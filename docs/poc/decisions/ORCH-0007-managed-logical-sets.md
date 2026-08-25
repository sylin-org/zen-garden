---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-02-26
---

# ORCH-0007: Managed Logical Sets — Stateful Orchestrator Membership Model

**Date**: 2026-02-26
**Status**: Accepted
**Applies to**: `orchestrator-common`, `orchestrator-mongodb`
**Depends on**: ORCH-0006 (Coordination Mode), ORCH-0004 (Gateway Announcement)

## Context

The MongoDB orchestrator discovers instances via two channels:

1. **Topology bootstrap** — one-shot `GET /api/v1/garden/topology` query at startup
2. **Tools API stream** — continuous SSE from `GET /api/v1/garden/tools/stream`

Both channels register every matching `tool_fqid` as a `MongoInstance` in the
in-memory registry, regardless of whether the offering is actually running.

### The Problem

Moss projects tool state into the SSE stream via `ToolProjection`, which
includes both `state: ToolState` (Ready/Degraded/Unavailable) and `ready: bool`.
The readiness logic in `domain/tools/readiness.rs` sets `ready = true` only when
`status == Running && health == Healthy`. All other combinations — Stopped,
Installing, Degraded, Offline — produce `ready = false`.

The orchestrator's `extract_offering_tool()` ignores both fields. It registers
the instance, the health monitor probes port 27017, finds it refused (container
is stopped), and marks the instance `Unreachable`. The bootstrap task then
considers it a candidate for `rs.add()`, which fails.

This created observable confusion: stone-shadowed-swamp appeared in the
orchestrator dashboard as "unreachable" with UNKNOWN role, even though the
offering was intentionally stopped — not failed.

### Missing Concepts

The orchestrator lacked three things:

1. **Distinction between "down" and "gone"** — a stopped container on a live
   stone is different from a stone that went offline or an offering that was
   uninstalled.

2. **Managed membership** — new discoveries auto-join the logical set, but there
   is no mechanism for a user to exclude an instance or reshape the replica set.

3. **Action scheduling** — membership changes (add/remove) require MongoDB
   commands (`rs.add()`, `rs.remove()`) that can only execute when the target is
   reachable. Offline targets need deferred execution.

## Decision

### Instance Lifecycle States

Add `Stopped` to `InstanceHealth` and introduce `PendingRemoval` as an
action state:

```rust
pub enum InstanceHealth {
    Healthy,        // Running, rs.status() responding
    Unknown,        // Discovered, not yet probed
    Unreachable,    // Was active, probe failed (stone/network issue)
    Degraded,       // Responding but degraded
    Stopped,        // Container down on stone (tools stream: ready = false)
}
```

State transitions:

```
                  tools stream          health monitor
                  ────────────          ──────────────
                  ready: true  ───┐
                                  ├──►  Active (Healthy)
                                  │        │
                                  │        ▼ probe fails
                  ready: false ───┤     Unreachable
                                  │        │ probe succeeds
                                  │        ▼
                                  │     Active (Healthy)
                                  │
                                  └──►  Stopped
                                           │ ready: true
                                           ▼
                                        Unknown → probe → Active
```

Rules:

- **Unreachable**: Set by health monitor when a previously-probed instance
  stops responding. Probe continues on normal cadence. Transitions back to
  Healthy when probe succeeds.

- **Stopped**: Set by discovery when tools stream reports `ready: false`. The
  health monitor skips probing Stopped instances (the container is known to be
  down). Transitions back to Unknown when tools stream reports `ready: true`,
  at which point the health monitor picks it up.

- **`tool.remove` event**: The offering was uninstalled or the stone left the
  garden. Instance is removed from the registry entirely. This is distinct from
  Stopped (container down but offering still registered).

### Managed Membership — Auto-Additive with User Exclusion

Logical sets (one per FQN, e.g. `mongodb`, `mongodb:analytics`) are
auto-additive:

- **New discovery**: If the tools stream reports a new instance with a matching
  FQN, it is automatically added to the logical set and queued for `rs.add()`.

- **Stopped instance**: Shown as disabled in the dashboard. Not probed, not
  included in `rs.add()` candidates. Automatically re-activated when the
  container starts.

- **User removal**: The user can request removal of an instance from the logical
  set via the orchestrator API. This queues a `PendingAction::RemoveMember` that
  executes `rs.remove()` when the target is reachable, then deletes the instance
  from the registry.

### FQN-Based Identity

The Fully Qualified Name (e.g. `mongodb`, `mongodb:analytics`) is the identity
that binds an instance to a logical set. An instance removed via user action and
later rediscovered with the **same FQN** is automatically accepted back. This is
by design — FQN identity means membership follows the offering.

To permanently exclude an instance, the operator must change the identity:

1. **Rename the offering** — change FQN from `mongodb` to `mongodb:secondary`
   (different logical set, different replica set name).
2. **Uninstall the offering** — triggers `tool.remove`, instance disappears from
   discovery entirely.

No permanent blocklist is needed. Removal is a one-time action, not a persistent
exclusion.

### Action Queue — Eventual Consistency

Every membership mutation is a scheduled action:

```rust
pub enum PendingAction {
    /// Remove a member from the replica set and logical set.
    RemoveMember {
        /// MongoDB wire endpoint (e.g. "stone-quartz-fen.local:27017").
        mongo_endpoint: String,
        /// FQN of the logical set.
        fqn: String,
        /// When the action was requested.
        requested_at: DateTime<Utc>,
    },
}
```

Execution rules:

- Actions are **always scheduled**, even for online targets. An online target's
  action executes immediately on the next bootstrap cycle.
- If the target is offline, the action remains queued and retries each cycle.
- The action queue is **persisted** to `{data_dir}/pending-actions.json` so
  pending removals survive orchestrator restart.
- `should_add_members()` checks the action queue and skips instances with a
  pending `RemoveMember` action — preventing the bootstrap from re-adding a
  member that the user requested removed.
- Discovery also checks the action queue: if a `RemoveMember` is pending for an
  endpoint, the tools stream upsert is suppressed until the action completes.

### Replica Set Synchronization

User-initiated removal reshapes both the orchestrator's logical set AND the
MongoDB replica set:

1. User requests removal → `PendingAction::RemoveMember` queued
2. Bootstrap executor finds action, target is reachable → executes
   `rs.remove()` against the current PRIMARY
3. On success → removes instance from in-memory registry, deletes action from
   queue
4. On failure (no primary, target unreachable) → action stays queued, retries
   next cycle

## Consequences

### Positive

- Stopped containers no longer appear as "unreachable" — clear distinction
  between intentionally stopped and failed instances.
- Health monitor skips probing stopped instances — eliminates noise in logs and
  dashboard.
- `should_add_members()` no longer attempts to add stopped or pending-removal
  instances to the replica set.
- User can reshape the logical set without SSH access to individual stones.
- FQN-based identity prevents accidental permanent exclusion — if an operator
  removes and re-deploys the same offering, it rejoins automatically.
- Action queue provides crash-safe eventual consistency for membership changes.

### Negative

- Action queue persistence adds I/O on each mutation (mitigated: writes are
  infrequent — only on user-initiated removal).
- The `ToolStreamEvent` gains a `ready` field, which is a change to the
  orchestrator-common API consumed by both MongoDB and Ollama orchestrators.
- FQN-based auto-readmission means there is no "ban" mechanism. This is
  intentional but may surprise operators who expect removal to be permanent.

## Implementation

### orchestrator-common changes

| File | Change |
|------|--------|
| `tools_stream.rs` | Add `ready: bool` to `ToolStreamEvent::OfferingDiscovered`; extract from `tool.get("ready")` in `extract_offering_tool()` |

### orchestrator-mongodb changes

| File | Change |
|------|--------|
| `domain/types.rs` | Add `InstanceHealth::Stopped`; add `PendingAction` enum |
| `app_state.rs` | Add `pending_actions: Arc<RwLock<Vec<PendingAction>>>`; add `queue_action()`, `complete_action()`, `has_pending_removal()` methods; persist/load actions |
| `tasks/discovery.rs` | Handle `ready: false` → upsert with `health: Stopped`; handle `ready: true` → upsert with `health: Unknown`; suppress upsert if pending removal exists |
| `tasks/bootstrap.rs` | `should_add_members()` skips Stopped and pending-removal instances; add action executor that runs `rs.remove()` when target reachable |
| `tasks/health_monitor.rs` | Skip probing instances with `health == Stopped` |
| `api/cluster.rs` | Add `DELETE /api/v1/cluster/members/:endpoint` → queues `PendingAction::RemoveMember` |

### Ollama orchestrator

The Ollama orchestrator consumes `ToolStreamEvent` from orchestrator-common. It
must be updated to handle the new `ready` field, but the Ollama orchestrator
does not have logical sets or replica set membership — it uses `ready` to decide
whether to include an instance in the routing pool (existing behavior: it
already skips instances that fail health probes).

## Data Flow Example

### Normal lifecycle

```
1. Stone boots, Moss starts MongoDB container
   → tools stream: offering:mongodb, ready: true
   → discovery: upsert MongoInstance(health: Unknown)
   → health monitor: probe succeeds → health: Healthy
   → bootstrap: rs.add() if not already member

2. Container stopped (maintenance, user action)
   → tools stream: offering:mongodb, ready: false
   → discovery: update health → Stopped
   → health monitor: skips probing
   → dashboard: shows instance as "stopped"

3. Container restarted
   → tools stream: offering:mongodb, ready: true
   → discovery: update health → Unknown
   → health monitor: probe succeeds → Healthy
   → bootstrap: already in RS, no action needed

4. Offering uninstalled
   → tools stream: tool.remove event
   → discovery: remove_instance() from registry
```

### User-initiated removal

```
1. User: DELETE /api/v1/cluster/members/stone-quartz-fen.local:27017
   → queue PendingAction::RemoveMember
   → persist to pending-actions.json

2. Bootstrap cycle (15s):
   → executor checks pending actions
   → target reachable? → rs.remove() against PRIMARY
   → success → remove from registry, delete action
   → failure → retry next cycle

3. Same FQN reappears (container redeployed):
   → tools stream: offering:mongodb, ready: true (no pending action)
   → discovery: new instance, auto-added
   → bootstrap: rs.add() on next cycle
```

## Related

- ORCH-0006: Coordination Mode (stateful vs stateless offerings)
- ORCH-0004: Gateway Announcement (orchestrator self-registration)
- `src/moss/src/domain/tools/readiness.rs`: Readiness logic for tool projections
- `src/common/src/tools/types.rs`: `ToolProjection` struct with `ready` and `state` fields
