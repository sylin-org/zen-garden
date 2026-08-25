---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-01
---

# ORCH-0008: Handler-Based Election Suppression

**Date**: 2026-03-01
**Status**: Accepted
**Applies to**: `moss` (offering orchestration task, fitness provider)
**Depends on**: ORCH-0004 (Gateway Announcement), ORCH-0006 (Coordination Mode)

## Context

ORCH-0006 introduced `coordination: elected` for stateful offerings (MongoDB,
Redis, PostgreSQL, etc.). When multiple stones run the same elected offering,
Moss runs a fitness-based election every `DEGRADATION_CHECK_INTERVAL_SECS`
(10 seconds) to determine which stone is Primary.

ORCH-0004 introduced gateway registration: any service can register a gateway
with `handler_for: ["mongodb"]`, declaring that it handles the lifecycle of
those FQNs. The registration propagates via chirps into every stone's topology
cache and expires via TTL (60 seconds) if not refreshed.

### The Problem

These two mechanisms operate independently. When a MongoDB orchestrator is
running and actively managing a replica set (primary election, failover, health
monitoring), Moss continues to trigger its own offering primary elections every
10 seconds — unaware that an external service already owns the lifecycle.

Observable symptoms (via `rake pulse`):

```
22:37:58  192.168.1.169  election req  offering primary (mongodb)
22:37:58  emerald-vale   election candidate  emerald-vale, score=477
22:37:58  golden-summit  election candidate  golden-summit, score=876
22:37:59  192.168.1.169  election result
```

This repeats every 10 seconds, generating unnecessary UDP traffic and
potentially conflicting with the orchestrator's own coordination.

The same applies to any future service — a companion, sidecar, or monitoring
agent — that registers a gateway claiming FQN ownership.

### Design Principle

The manifest `coordination: elected` is the ground-truth policy. It must
always work without orchestrators. Orchestrators are optional overlays.

When a service registers a gateway with `handler_for` covering an offering
type, elections for that type are suppressed for the lifetime of the
registration. When the gateway expires (service dies, TTL lapses), elections
resume automatically. No manifest changes. No new types.

## Decision

Suppress offering primary elections when an active gateway's `handler_for`
covers the offering type. The suppression is a runtime overlay — it uses data
already flowing through the topology cache (gateway registrations from chirps).

### Two suppression points

**1. Election triggering** — `orchestration_tick()` in `offering_orchestration.rs`

Before dispatching any offering, scan the topology cache for gateway entries
whose `handler_for` includes the offering type. If found, skip the offering
entirely — no staleness check, no election.

```rust
// Collect offering types covered by any active gateway in the garden
let gateway_handled: HashSet<String> = {
    let cache = state.topology_cache.read().await;
    cache.values()
        .flat_map(|entry| entry.gateways.iter())
        .flat_map(|gw| gw.handler_for.iter().cloned())
        .collect()
};

// In the offering loop:
if gateway_handled.contains(&offering.offering) {
    continue; // A service is handling this FQN — skip election
}
```

**2. Candidate response** — `compute_fitness()` in `state_provider.rs`

When a stone receives an election request for an offering whose type has an
active gateway in the topology cache, return `None` (ineligible). This is a
belt-and-suspenders guard against race windows where one stone hasn't received
the gateway chirp yet and triggers an election before other stones can
suppress it.

### Properties

| Scenario | Behavior |
|----------|----------|
| No gateways registered | Elections run normally per manifest policy |
| Service registers gateway with `handler_for: ["mongodb"]` | All stones suppress mongodb elections |
| Service dies (crash or shutdown) | Gateway TTL expires (60s), removed from chirps, elections resume within ~70s |
| Multiple services for different FQNs | Each `handler_for` claim is independent |
| Service on a different stone | Gateways propagate via chirps to all topology caches |
| Future service types (companion, sidecar, agent) | Same mechanism — register gateway, claim FQNs |

### Why not a new coordination mode?

Adding `Orchestrated` to `CoordinationMode` would tie the suppression to the
manifest, making it a deployment-time decision. But whether an orchestrator is
running is a runtime fact. The same MongoDB manifest must work in gardens with
and without an orchestrator. The gateway's `handler_for` is the runtime signal.

## Consequences

### Positive

- Zero MongoDB elections while orchestrator is running (was: every 10 seconds)
- Reduced UDP traffic (no election req/candidate/result broadcasts)
- Clean separation: orchestrator owns lifecycle, Moss is the fallback
- Any service can claim FQN ownership — not tied to orchestrators
- No manifest changes, no new types, no API changes
- Self-healing: elections resume automatically when handler disappears

### Negative

- ~70 second worst-case delay before elections resume after handler crash
  (60s TTL + 10s tick). Acceptable: the handler was managing the service,
  so a brief gap is expected during failover.
- Candidate-side suppression requires a topology cache read per election
  response. Cost: one `RwLock::read()` — negligible.

## Implementation

### Changed files

| File | Change |
|------|--------|
| `moss::tasks::offering_orchestration` | Build `gateway_handled` set from topology cache; skip offerings in set |
| `moss::tasks::state_provider` | Check topology cache for active gateways before computing fitness; return `None` if handled |

### Verification

```bash
cargo check --all
cargo clippy -- -D warnings
```

Manual: deploy MongoDB orchestrator, run `rake pulse` on a wide terminal,
confirm zero `election req  offering primary (mongodb)` events. Stop the
orchestrator, wait ~70s, confirm elections resume.

## Related

- ORCH-0004: Gateway Announcement (provides the `handler_for` data)
- ORCH-0006: Coordination Mode (provides the `elected` baseline policy)
- ORCH-0001: Replant Ceremony (introduced offering Primary/Dormant roles)
- ELECTION-0001: Distributed Election (the election protocol itself)
