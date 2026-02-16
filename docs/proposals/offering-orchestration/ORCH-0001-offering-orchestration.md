# ORCH-0001: Offering Orchestration & Autonomous Resilience

**Status:** Draft  
**Date:** 2026-02-16  
**Authors:** Leo Botinelly, Claude  
**Supersedes:** Federation/Process/Consistency sections of same-offering-orchestration.md  
**Depends On:** KOI-0001 (Embedded HTTP & UDP Bridging) — Phase 0 prerequisite  
**Related:** OFFER-0003 (FQN), OFFER-0004 (Placement), Koi Embedded Integration, Lantern Registry, Sub-Capabilities Proposal, Nurturing Proposal

---

## Abstract

This proposal defines how Zen Garden orchestrates multiple deployments of the same offering across Stones. It replaces the original declarative three-concern model (federation/process/consistency as static YAML) with an autonomous, peer-to-peer system where each Stone is responsible for its own state, elections resolve through fitness scoring over UDP multicast, and DNS publication via Koi signals primary status to the entire network.

The core principle: **deploy an offering twice and get resilience for free, with zero configuration.**

---

## Table of Contents

1. [Motivation](#motivation)
2. [Design Philosophy](#design-philosophy)
3. [The Default Policy: Singleton-with-Replica](#the-default-policy-singleton-with-replica)
4. [Offering Lifecycle State Machine](#offering-lifecycle-state-machine)
5. [Election Protocol](#election-protocol)
6. [Fitness Scoring](#fitness-scoring)
7. [Pull-Based Synchronization](#pull-based-synchronization)
8. [DNS-as-Publication via Koi](#dns-as-publication-via-koi)
9. [Pinning](#pinning)
10. [Replicability](#replicability)
11. [FQN Scoping](#fqn-scoping)
12. [Degradation Detection & Graceful Handover](#degradation-detection--graceful-handover)
13. [Cross-Subnet: Lantern as Chirp Coordinator](#cross-subnet-lantern-as-chirp-coordinator)
14. [Policy Graduation](#policy-graduation)
15. [Manifest Extensions](#manifest-extensions)
16. [Observability](#observability)
17. [Implementation Phases](#implementation-phases)
18. [References](#references)

---

## Motivation

### Problems with the Original Model

The original same-offering orchestration proposal (federation/process/consistency as YAML declarations) attempted to encode coordination logic declaratively. Real-world scenarios expose its limitations:

1. **Ollama** — Routing needs to understand VRAM constraints, model loading state, and request characteristics *at runtime*. Static `federation: pool` doesn't capture this.
2. **MongoDB** — Clustering requires running actual commands during lifecycle events, monitoring cluster health continuously. Static `choreography:` YAML can't express conditional logic, retries, or rollbacks.
3. **Pi-hole** — Must be a true singleton with automatic failover, but the original model had no mechanism for standby promotion or DNS migration.
4. **User apps** — Pushing a container app twice should "just work" without requiring the user to understand federation modes.

### The Insight

The three-concern model describes *characteristics* of offerings, not *mechanisms* of coordination. Most offerings don't need complex orchestration — they need a sensible default that handles resilience automatically.

For the 20% that need more (load balancing, capability routing, database clustering), specialized orchestrator services handle the complexity. These are themselves offerings in the garden, not core Moss logic.

---

## Design Philosophy

### Autonomous Stones

Every Moss instance is responsible for its own state. No central coordinator pushes instructions. If a Stone needs to sync, its Moss queries the primary and pulls. If a Stone detects the primary is gone, it participates in an election. If a Stone comes back after being offline, it catches up on its own.

### Pull, Never Push

Replicas pull from primaries. Primaries don't track who their replicas are. Seed Banks are caches that replicas prefer over bothering the primary. The primary's only obligation is to serve its state when asked.

### DNS Is the Publication Mechanism

The `<offering>.lan` DNS entry registered via Koi is the externally visible marker of primary status. Only the primary registers DNS. When an election produces a new primary, the DNS entry moves. From the network's perspective, the offering always resolves to exactly one place.

### Sane Defaults, Explicit Upgrades

The default policy (singleton-with-replica) requires zero configuration. Users who need load balancing, capability routing, or database clustering graduate to explicit policies. The default covers 80% of use cases.

---

## The Default Policy: Singleton-with-Replica

When a user deploys an offering:

- **First deployment** → primary. Registers DNS, serves traffic.
- **Second deployment (same FQN)** → joins the federation by syncing from the primary, then goes dormant. It's a warm replica, ready to take over.
- **Third+ deployments** → same as second. More replicas, more resilience.

The user never specifies roles. They never type `--primary` or `--replica`. Moss figures it out:

```bash
garden-rake offer my-app          # primary on stone-01
garden-rake offer my-app          # replica on stone-02 (auto-syncs, goes dormant)
garden-rake offer my-app          # replica on stone-03 (auto-syncs, goes dormant)

# stone-01 dies → election → stone-02 or stone-03 promotes
# DNS moves → traffic resumes within seconds
```

---

## Offering Lifecycle State Machine

An offering instance exists in one of four states:

```
┌──────────┐     sync complete     ┌──────────┐
│ Joining  │ ───────────────────► │ Dormant  │
└──────────┘                       └────┬─────┘
     ▲                                  │
     │                     primary fails/degrades
   deploy                              │
     │                           ┌─────▼──────┐
     │                           │  Election   │
     │                           └──┬──────┬───┘
     │                    winner    │      │  losers
     │                         ┌───▼──┐   │
     │                         │Primary│   │
     │                         └───┬───┘   │
     │                             │       │
     │              degrades ──────┘       │
     │                                     │
     └─────── recovered ◄─────────────────┘
```

### States

| State | Description | DNS Registration | Serves Traffic |
|-------|-------------|------------------|----------------|
| **Joining** | Just deployed, syncing data from primary. Not eligible for election. | No | No |
| **Dormant** | Fully synced, healthy, waiting. Eligible for election. | No | No |
| **Primary** | Actively serving. Makes state available for replicas to sync from. | Yes | Yes |
| **Degraded** | Running but health checks failing or performance tanking. Triggers re-election. | Yes (until handover) | Yes (until handover) |

### Transitions

| From | To | Trigger |
|------|-----|---------|
| *(deploy)* | Joining | New offering deployment |
| Joining | Dormant | Sync complete (cursor matches primary) |
| Dormant | Primary | Won election |
| Primary | Dormant | Lost election (fitness re-election or pin override) |
| Primary | Degraded | Sustained health/performance threshold breach |
| Degraded | Dormant | Lost election (handover complete) |
| Dormant | Joining | Cursor fell behind (long offline), re-syncing |

---

## Election Protocol

### Three Election Triggers

**1. Failure Election** — Primary disappears (chirp heartbeat timeout). Urgent. Short collection window (1s quiet / 3s hard cap).

**2. Degradation Election** — Primary's own Moss transitions role to Degraded. The chirp carries `role: "degraded"` on `TopologyServiceEntry`; dormant replicas see this in the topology cache and initiate an election. The primary is still up, allowing graceful handover. See [Degradation Detection](#degradation-detection--graceful-handover).

**3. Fitness Re-Election** — No emergency. All instances are confirmed synced. Triggered periodically or by `garden-rake rebalance`. Optimizes placement (e.g., new more powerful Stone added). Only fires when all replicas' sync cursors match the primary's state cursor.

### Protocol Messages (UDP Multicast)

Elections reuse the existing announcement type constants in `announcement_types.rs` — no new message types needed:

| Message | Constant | Payload | Sender |
|---------|----------|---------|--------|
| Election request | `ELECTION_REQUEST` | `ElectionRequest { offering_fqn, election_id, score_mechanism: Fitness }` | Any Moss detecting primary absence or degradation |
| Candidacy | `ELECTION_CANDIDATE` | `ElectionCandidate { offering_fqn, election_id, stone_id, score: Option<i16>, pin_timestamp }` | Every Moss holding a replica of the offering |
| Result | `ELECTION_RESULT` | `ElectionResult { offering_fqn, election_id, winner_stone_id }` | The winner (confirmation) |

No `DEGRADATION_WARNING` announcement type needed — degradation flows through the existing chirp pipeline. When a Primary's role transitions to Degraded, the chirp carries `role: "degraded"` on `TopologyServiceEntry`. Dormant replicas see this in the topology cache and trigger an election.

### Election Mechanics

1. **Initiation**: Any Moss that detects primary absence broadcasts `ELECTION_REQUEST` with `ScoreMechanism::Fitness` and a unique election ID.
2. **Candidacy**: Every Moss holding a replica computes its fitness score (`i16` in [-1000, 1000]). If eligible, it immediately responds with `ELECTION_CANDIDATE` including the score. Ineligible Stones (constraint failure, unreachable) simply don't respond — no filtering needed on the collection side.
3. **Collection**: The election initiator collects candidates until either 1 second of quiet (no new candidate) or 3 seconds hard cap, whichever comes first.
4. **Resolution**: Highest score wins. Ties broken by most recent `pin_timestamp`, then lexicographically higher `stone_id` (deterministic). The initiator also self-bids if it holds a replica.
5. **Confirmation**: Winner broadcasts `ELECTION_RESULT`. If a Moss computed a different winner (missed a candidate due to packet loss), it defers to the announced result.

### Cold Start

When the entire garden boots:

- First Stone with a replica comes up, holds election, no opponents respond → wins by default, becomes primary.
- Second Stone comes up, discovers an existing primary → enters Joining state, syncs, becomes Dormant.
- If two Stones boot simultaneously and both win elections with zero opponents: brief dual-primary. Resolved on first mutual chirp — **the Stone with the lexicographically lower `stone_id` yields** (deterministic, no flapping). No fitness comparison needed — this is a conflict breaker, not an election. For most offerings, a few seconds of dual-primary is harmless. For stateful offerings, the yielding primary should stop accepting writes during resolution.

### Split-Brain Prevention

If two Stones both believe they're primary for the same FQN (cold boot race, healed network partition):

- First mutual chirp reveals the conflict — the `role` field on `TopologyServiceEntry` carries each Stone's claimed role.
- Lower `stone_id` Stone immediately yields (deterministic, no negotiation, no fitness comparison).
- For cross-subnet gardens: Lantern acts as arbiter. The side with Lantern connectivity is authoritative; the side without is degraded.

### Startup Reconciliation

When Moss restarts, the world may have changed while it was down:

1. Emit a chirp immediately (role carried on `TopologyServiceEntry`)
2. **Wait one full election window (3 seconds)** before asserting Primary
3. During this window, watch for chirps from other Stones
4. If another Stone is already Primary for this FQN → yield to it (become Dormant)
5. If no other Primary seen → retain Primary

This prevents a stale Primary from conflicting with a legitimately elected replacement.

---

## Fitness Scoring

Fitness is a single opaque number: **`i16` in the range [-1000, 1000]**. Higher is better. Pinned = **1001** (outside the valid range, always wins).

### Design Principles

- **Scoring is opaque.** The election protocol knows "candidates have scores, highest wins." How a Stone computes that number is a Moss-private implementation detail, not a shared type.
- **No weight tables.** No composite model with prescribed metrics and per-trigger weight profiles. Start simple, refine freely — the protocol never changes.
- **Ineligible = silence.** If a Stone fails manifest constraints (wrong architecture, insufficient memory), it simply never sends a candidacy. No filtering needed on the collection side.
- **No `FitnessInput` god struct.** No shared struct enumerating every possible metric. The scoring function uses whatever data is locally available in `AppState` and `HardwareCapabilities`.

### Scoring Guidance (Implementation, Not Protocol)

A reasonable starting heuristic (can evolve without protocol changes):

| Input | Points | Rationale |
|-------|--------|-----------|
| CPU headroom (% free) | 0–250 | Processing capacity |
| Memory headroom (% free) | 0–250 | Headroom for the offering |
| Offering count penalty | 0–250 | Fewer offerings = more resources available |
| Health status | 0 or 250 | Healthy offering = full bonus |

As sync matures, sync freshness can be factored in; as latency measurement matures, network quality can contribute. The function grows privately in Moss without touching the election wire protocol.

### Pin Override

A pinned Stone's fitness score is **1001** — outside the valid [-1000, 1000] range, guaranteeing victory. Pin status and `pin_timestamp` are included in election candidacies. See [Pinning](#pinning).

### Constraint-Based Eligibility

The offering manifest can declare typed `orchestration_constraints`:

```yaml
orchestration_constraints:
  architectures: ["x86_64"]
  cpu_features: ["avx2"]
  min_memory_mb: 4096
  min_storage_mb: 10240
```

Constraints are checked against `HardwareCapabilities` before computing a score. Failure = don't respond to the election (ineligible).

---

## Pull-Based Synchronization

### Core Principle

Each Moss instance that holds a replica is responsible for keeping itself synced. No coordinator pushes state. The replica:

1. Knows who the primary is (from Tools API stream / chirps)
2. Periodically queries the primary's state cursor
3. If behind, pulls the delta
4. If a Seed Bank exists, pulls from there instead (less load on primary)

### What Primary Exposes

The primary's obligation is minimal — two questions any replica can ask:

1. **"What's your current cursor?"** — Lightweight endpoint or metadata in chirp. Just a timestamp or sequence number.
2. **"Give me state since cursor X"** — The actual sync payload. Format depends on sync tier.

### Sync Tiers

| Tier | Applies To | Payload | Mechanism |
|------|-----------|---------|-----------|
| **Container volumes** | Managed offerings with persistent data | Tarball diff or rsync-over-HTTP of mounted volumes | Replica pulls volume snapshot from primary or Seed Bank |
| **Docker images** | All managed offerings | Container image layers | Already handled — if image is in a local registry, replicas pull normally |
| **Capability mirroring** | Adopted offerings with capability manifests | Capability list + pull commands | Replica discovers primary's capabilities, mirrors missing ones (e.g., `ollama pull <model>`) |
| **Manifest sync hooks** | Offerings with custom sync | Offering-specific | Manifest defines `sync_command` or a well-known HTTP endpoint |

### Sync Decision Tree (per Moss instance, per replicated offering)

```
Am I primary for this offering?
  Yes → Serve state when asked. Publish DNS. Do my job.
  No →
    Is there a Seed Bank in the garden?
      Yes → Pull from Seed Bank
      No → Pull directly from primary
    
    Am I current? (my cursor == primary's cursor)
      Yes → Sleep, check again on interval
      No → Pull delta, update cursor, sleep
    
    Did the primary disappear?
      Yes → Participate in election
      No → Stay dormant
```

### Seed Bank as Optimization

Seed Bank is not a "backup system" — it's a cache that replicas prefer:

- Primary periodically pushes state to Seed Bank (or Seed Bank pulls — same pattern)
- Replicas pull from Seed Bank instead of primary
- If Seed Bank absent → replicas pull from primary directly
- If both unavailable → replica stays at current cursor, waits

No offering needs to know whether a Seed Bank exists. It's discovered through the Tools API and used opportunistically.

### Cursor Tracking

Each Moss instance tracks per-offering sync state:

```json
{
  "offering_fqn": "my-app",
  "state": "dormant",
  "sync_cursor": "2026-02-16T14:30:00Z",
  "sync_method": "seed-bank",
  "last_sync_check": "2026-02-16T14:45:00Z",
  "primary_stone_id": "019c3a2b-..."
}
```

During elections, the sync cursor is locally available — no network calls needed to compute fitness. The election is pure computation over local data.

---

## DNS-as-Publication via Koi

### The Mechanism

When a Moss instance's offering transitions to Primary:

```rust
// Register DNS entry via Koi
koi_handle.dns()?.add_entry(DnsEntry {
    name: format!("{}.lan", offering_name),
    ip: stone_ip.to_string(),
    ttl: None,
})?;
```

When it transitions away from Primary:

```rust
// Remove DNS entry
koi_handle.dns()?.remove_entry(&format!("{}.lan", offering_name))?;
```

### What This Solves

- **Single resolution point**: `grafana.lan` always points to exactly one Stone — the current primary.
- **Failover visibility**: DNS moves within seconds of election completion. Every machine using Koi for resolution sees the change immediately.
- **TLS continuity**: The certmesh certificate covers the name (e.g., `grafana.lan`), not the Stone. The new primary serves under the same name with a valid cert. `https://grafana.lan:8443` works across failover.
- **No load balancer**: For the default singleton-with-replica policy, DNS *is* the routing mechanism. No proxy layer needed.

### Named Instances

Named FQN instances get namespaced DNS:

- `mongodb` → `mongodb.lan`
- `mongodb:analytics` → `mongodb-analytics.lan`

The colon-to-hyphen transformation in DNS names mirrors the FQN container naming convention.

---

## Pinning

### Concept

A user can pin an offering to a specific Stone, expressing intent that this Stone should be the primary whenever it's available.

```bash
garden-rake pin grafana stone-amber-ridge
garden-rake unpin grafana
```

### Semantics

- Pinning stores local metadata on the pinned Stone: `{ offering_fqn, pinned: true, pin_timestamp }`.
- **Pin timestamp** is the tiebreaker: if two Stones are both pinned for the same offering, the most recent pin wins. This encodes chronological user intent — the most recent pin is the most recent decision.
- **Pinned Stone online** → always wins elections, regardless of fitness (score = `1001`, outside the valid [-1000, 1000] range).
- **Pinned Stone offline** → normal election among remaining replicas. Someone else takes over.
- **Pinned Stone recovers** → immediately triggers re-election. Its pin means it wins. Orderly handover: current primary syncs state, pinned Stone catches up, promotes, registers DNS, old primary goes dormant.
- **Unpin** → removes local pin metadata. Normal fitness-based elections apply.

### No Central Registry

Pins are local metadata. `garden-rake pin` writes to the target Stone. No coordination with other Stones needed. If conflicting pins exist, they resolve at the next election via `pin_timestamp`. No explicit "unpin the old one" required.

### Election Resolution Order

1. Pinned beats unpinned (always)
2. Among pinned: most recent `pin_timestamp` wins
3. Among unpinned: highest fitness score wins
4. Tie: higher Stone ID wins (deterministic)

---

## Replicability

### Offering Opt-Out

Not all offerings can be replicated. The manifest declares this:

```yaml
# In offering manifest
replicable: false    # Default: true
```

When `replicable: false`:
- Second deployment creates an independent instance, not a replica
- No election protocol, no sync, no failover
- Each deployment is a standalone singleton

### When to Use

- **Hardware-bound** — GPU-specific workloads only one Stone can run
- **License-locked** — Node-locked licensing that can't run on multiple machines
- **External state** — Offering depends on local hardware or external systems that can't be snapshot

### Adopted Offering Gradient

Adopted offerings (native processes Moss didn't deploy) have limited replication:

| Adopted Offering Has | Replicable? | Sync Method |
|----------------------|-------------|-------------|
| Capability manifest (e.g., Ollama models) | **Yes** (capabilities only) | Capability mirroring — replicas pull same models/extensions/modules |
| Sync hooks in manifest (`sync_command`) | **Yes** (via hooks) | Manifest-defined export/import |
| Neither | **No** | Defaults to `replicable: false` |

An adopted Ollama can't have its process state replicated, but its capabilities (models) can be mirrored. If the primary fails, the replica has the same models loaded and can serve immediately.

---

## FQN Scoping

Election groups are determined by Fully-Qualified Name identity:

- `garden-rake offer mongodb` twice → same FQN (`mongodb`), same election group. Default policy applies.
- `garden-rake offer mongodb` + `garden-rake offer mongodb:analytics` → different FQNs, completely independent election groups. Each one independently gets the default policy if deployed multiple times.

The rule: **FQN identity determines what "same offering" means for election and sync purposes.** Two deployments with the same FQN are replicas of each other. Two deployments with different FQNs are unrelated.

---

## Degradation Detection & Graceful Handover

### Detection

A primary's own Moss is best positioned to detect degradation — it has direct access to local metrics. Degradation is declared after **sustained threshold breach**:

- Health check failures for N consecutive checks (e.g., 3 checks at 10-second intervals = 30 seconds)
- Resource pressure exceeding threshold for sustained period (memory > 90%, CPU > 95%, disk > 95%)
- Offering-specific health endpoint reporting unhealthy

When degradation is detected, the primary's Moss calls `transition_role(Degraded)`. This emits `OfferingEvent::RoleChanged`, which the chirp listener picks up automatically — the next chirp carries `role: "degraded"` on `TopologyServiceEntry`. Dormant replicas see this in the topology cache and trigger a Fitness-mode election.

**No special `DEGRADATION_WARNING` announcement type needed** — the existing chirp pipeline IS the notification mechanism. One event emission, all downstream systems react.

If something catastrophic happens (Stone crashes, power loss, network failure), replicas detect primary absence via heartbeat timeout (chirp staleness) and trigger a failure election directly — no degradation signaling possible or needed.

### Graceful Handover Sequence

When the primary is still alive during degradation, an orderly handover is possible:

```
1. Primary's Moss transitions role to Degraded (emits RoleChanged event)
2. Chirp broadcasts role: "degraded" on TopologyServiceEntry
3. Dormant replicas detect degraded primary in topology cache → trigger election
4. Winner identified (new primary)
5. New primary confirms sync cursor is current
6. Old primary calls cordon_service_v1() internally (drain connections, 30s timeout)
7. Old primary confirms drain complete
8. New primary registers DNS via Koi, transitions to Primary
9. Old primary removes its DNS entry, transitions to Dormant
```

The `cordon_service_v1` endpoint (already stubbed at `POST /api/v1/stone/services/{service}/cordon`) is implemented as part of this work to handle the drain step.

This minimizes the window where neither or both are serving. The DNS swap (steps 8-9) is the atomic handover point — clients following DNS resolution switch seamlessly.

### Catastrophic Failure

If the primary simply disappears (no degradation signal possible):

1. Replicas detect absence via chirp heartbeat timeout (chirp not seen for 6 seconds)
2. Failure election (Fitness-mode, short collection window)
3. Winner promotes, registers DNS
4. If old primary later recovers → startup reconciliation (3s window, sees existing primary, yields)

### Graceful Primary Removal

When `take_away_offering_v1` is called on a Primary that has replicas:

1. Trigger a Fitness-mode election (this Stone does NOT self-bid)
2. Wait for `ELECTION_RESULT` naming a new Primary (timeout: 3 seconds)
3. The new Primary transitions (gets DNS, emits events)
4. THEN proceed with removal of this offering

This prevents unplanned failover when intentionally removing a Primary.

### Last-Copy Seed-Bank Archival

When `take_away_offering_v1` is called and this is the **last instance in the garden** (no other Stone lists this FQN in the topology cache):

- If a seed-bank offering is discovered in the garden's tools cache:
  - Return a response indicating last-copy status and seed-bank availability
  - Rake prompts: *"This is the last instance of `mongodb`. Archive to seed-bank before removal? [Y/n]"*
  - If yes → Moss snapshots the offering's volume/capabilities to the seed-bank via the job system. Wait for job completion, then remove.
  - If no → remove immediately
- If no seed-bank exists: remove immediately (warn that data is permanently lost)

The API endpoint returns metadata; the interactive prompt lives in Rake. Moss never blocks on user input.

---

## Cross-Subnet: Lantern as Chirp Coordinator

### The Problem

On a single subnet, the UDP multicast election protocol works perfectly. But Stones on different subnets can't hear each other's chirps.

### The Solution

When a Lantern is present, Stones communicate election messages through Lantern instead of direct UDP multicast. Lantern becomes the chirp coordinator:

- Stones send election messages (`ELECTION_REQUEST`, `ELECTION_CANDIDATE`, `ELECTION_RESULT`) to Lantern via HTTP
- Lantern decides when and how to fan out messages to participating Stones
- Lantern's topology knowledge ensures all relevant Stones receive election messages regardless of subnet

### Partition Arbitration

Lantern also acts as the split-brain arbiter:

- The side of a network partition that can reach Lantern is authoritative
- The side without Lantern connectivity is degraded (offerings continue running but don't register DNS, don't participate in elections)
- When the partition heals, the degraded side discovers the authoritative state and reconciles

### Graceful Degradation

If Lantern is absent:
- Same-subnet elections work normally via UDP multicast
- Cross-subnet elections can't happen (Stones on different subnets are independent)
- This is fine for simple single-subnet gardens — Lantern is optional infrastructure

---

## Policy Graduation

The default policy covers most use cases. Users who need more graduate to explicit policies:

| Policy | Orchestrator | Use Case |
|--------|-------------|----------|
| **none** (default) | Singleton-with-replica, automatic election | Most offerings |
| **routed** | Capability Router offering | Ollama, heterogeneous instances |
| **clustered** | Choreographer offering | MongoDB, PostgreSQL, Redis with replication |
| **balanced** | Gateway offering (Traefik/Caddy) | HTTP apps needing throughput |
| **failover** | Sentinel offering | DNS-level singletons (Pi-hole) with VIP |
| **custom** | User-provided orchestrator | Anything else |

```bash
# Upgrade from default
garden-rake policy ollama routed
garden-rake policy mongodb clustered
garden-rake policy my-app balanced

# Return to default
garden-rake policy my-app none
```

Each non-default policy implies a specialized orchestrator offering that Moss auto-provisions (or prompts the user to install). The orchestrator is itself an offering in the garden — it subscribes to the Tools API stream, understands garden topology, and manages its domain.

See companion specs:
- [ORCH-0002: AI Capability Router (Ollama)](#)
- [ORCH-0003: Database Choreographer (MongoDB)](#)

---

## Manifest Extensions

### New Fields

```yaml
# offerings/my-app.manifest.yaml
name: my-app
category: application

# Orchestration
replicable: true                    # Default: true. Set false for hardware-bound/licensed offerings
default_policy: none                # Default: none (singleton-with-replica)

# Hardware/capability constraints for election eligibility (optional)
orchestration_constraints:
  architectures: ["x86_64"]         # Any-of match against stone capabilities
  cpu_features: ["avx2"]            # Any-of match
  min_memory_mb: 4096               # Minimum total memory
  min_storage_mb: 10240             # Minimum free storage

# Sync hooks (optional, for offerings with custom state)
sync:
  cursor_endpoint: "/api/v1/state/cursor"    # Returns current state cursor
  snapshot_endpoint: "/api/v1/state/snapshot" # Returns state since cursor
  snapshot_command: "pg_dump -Fc > /backup/dump.pgc"  # Alternative: shell command
  restore_command: "pg_restore -d $DATABASE /backup/dump.pgc"

# Degradation thresholds (optional, overrides defaults)
health:
  consecutive_failures: 3           # Default: 3
  check_interval_seconds: 10        # Default: 10
  degradation_threshold:
    memory_percent: 90              # Default: 90
    cpu_percent: 95                 # Default: 95
    disk_percent: 95                # Default: 95
```

### Adopted Offering Example

```yaml
name: ollama
category: ai
replicable: true    # Capabilities can be mirrored

# Capabilities define what can be synced for adopted instances
capabilities:
  type: models
  discover:
    command: ["ollama", "list", "--json"]
    parse: ".models[].name"
  add:
    command: ["ollama", "pull"]
  remove:
    command: ["ollama", "rm"]
```

---

## Observability

All orchestration state is visible through existing channels:

### Role on Chirps

A new `role: Option<String>` field on `TopologyServiceEntry` piggybacks the orchestration role on every chirp (~20 bytes/service, <3% overhead on a 1500–3500 byte chirp). This means every Stone in the garden automatically knows every other Stone's role for every offering — no extra messages, no polling.

### Event-Driven Fan-Out

Role transitions emit `OfferingEvent::RoleChanged` on the event bus. The existing event bus subscribers — chirp listener, presence stream, tools projector — react automatically:

- **Chirp listener** picks up `RoleChanged` and triggers a chirp (role carried on `TopologyServiceEntry`)
- **Presence stream** emits the corresponding presence event (`offering.role.promoted` / `offering.role.demoted`)
- **Tools projector** updates `OrchestrationState` on the `ToolProjection`

One event emission, all downstream systems react. No manual wiring at each role transition callsite.

### Tools API

The `/api/v1/garden/tools` response includes orchestration fields directly (reusing the domain `OrchestrationState` type, not a lossy projection wrapper):

```json
{
  "tool_fqid": "offering:my-app",
  "state": "ready",
  "orchestration": {
    "role": "primary",
    "sync_cursor": "2026-02-16T14:30:00Z",
    "sync_method": "seed-bank",
    "pinned": false
  }
}
```

`ToolProjection` participates in `ToolsBeacon` (UDP). A `stripped_for_beacon()` method (like `TopologyEntry::stripped_for_chirp()`) minimizes the orchestration field in the UDP payload.

### Presence Stream

New event constants:

- `offering.election.started` — election initiated for an FQN
- `offering.role.promoted` — Stone promoted to primary
- `offering.role.demoted` — Stone demoted to dormant
- `offering.sync.completed` — Replica finished syncing
- `offering.health.degraded` — Primary health declining

### Rake

Instead of a new top-level `observe` command, orchestration state is surfaced via the existing `garden-rake garden` command with an `--orchestration` flag:

```bash
$ garden-rake garden --orchestration

  STONE-AMBER-RIDGE    [thriving]
    my-app             primary    synced    dns: my-app.lan
    ollama:adopted     primary    synced    dns: ollama.lan    4 models

  STONE-CORAL-REEF     [thriving]
    my-app             dormant    synced    replica
    ollama:adopted     dormant    synced    replica            4 models (mirrored)

  STONE-BRONZE-CANYON  [resting]
    my-app             joining    syncing   42% complete
```

This extends existing UI rather than creating a new command surface to discover and document.

### Lantern Dashboard

Lantern's web UI visualizes replica topology, election history, and sync status across the garden. Primary/dormant/joining states are color-coded. Election events appear in the activity feed.

---

## Implementation Phases

### Phase 0: Koi Infrastructure (prerequisite — see KOI-0001)

**Effort:** ~1 week (in Koi repo + Moss wiring)  
**Spec:** [KOI-0001: Embedded HTTP & UDP Bridging](KOI-0001-phase0-prerequisite.md) (full proposal in `koi` repo)

Before any orchestration logic, containerized offerings need a path to interact with the host network. This phase activates three capabilities:

- **0a:** koi-embedded HTTP self-hosting — activate the dead `http_enabled` config. When true, `start()` spawns an axum listener on `:5641` serving the same API as standalone Koi. Domain routes already exist in each crate; this wires them together (~150 lines).
- **0b:** koi-udp crate — new Koi domain crate bridging host UDP sockets into HTTP/SSE. Containerized orchestrators subscribe to Garden mesh traffic (`stone_chirp`, `tools_beacon` on port 7184) via `GET /v1/udp/recv/{id}`. Also enables outbound sends (WoL, SSDP, etc.).
- **0c:** Moss container wiring — `extra_hosts` for `host.docker.internal`, env var injection (`KOI_ENDPOINT`, `GARDEN_STONE_ENDPOINT`, `GARDEN_OFFERING_NAME`), enable `dns_enabled(true)` and `http(true)` in Koi builder.

After this phase, any containerized offering can `curl http://host.docker.internal:5641/v1/dns/add` or subscribe to mesh UDP — no `network_mode: host` required.

### Phase 1: Shared Types, State Machine & Election (Foundation)

**Effort:** ~2-3 weeks

- Add `OrchestrationState` to runtime `Offering` struct (`role`, `primary_stone_id`, `pinned`, `pin_timestamp`)
- Add `role: Option<String>` to `TopologyServiceEntry` (piggyback on chirps)
- Add `replicable` and typed `OrchestrationConstraints` to offering manifests
- Add `OfferingEvent::RoleChanged` variant to event bus (drives chirps, presence, tools projection automatically)
- Extend existing `ElectionService` with `ScoreMechanism::Fitness` mode (opaque `i16` scoring, 1s quiet / 3s hard cap)
- Add `OfferingPrimary` to `ElectionType`, `score: Option<i16>` and `pin_timestamp` to `ElectionCandidate`
- Implement the four-state lifecycle (Joining, Dormant, Primary, Degraded)
- First-deploy-is-primary logic
- Cold start: election with zero opponents
- Dual-primary resolution on mutual chirp (lower `stone_id` yields)
- Startup reconciliation (3s window before asserting Primary)
- Fitness scoring function in Moss (private, not shared type)

### Phase 2: Pull-Based Sync

**Effort:** ~2-3 weeks

- Extend `OrchestrationState` with `sync_cursor`, `sync_method`, `last_sync_check` (backward-compatible)
- Primary exposes cursor endpoint (lightweight)
- Volume snapshot sync for managed offerings (tarball diff)
- Capability mirroring sync for adopted offerings (reuse existing mirror infrastructure)
- Seed Bank integration: prefer Seed Bank when available
- Joining → Dormant transition on sync completion
- Consider `Syncable` trait dispatched by `OfferingModeData` for clean separation

### Phase 3: DNS Publication via Koi

**Effort:** ~1 week

- Primary registers `<offering>.lan` on promotion (via `koi_handle.dns()`)
- Primary removes DNS on demotion
- Named instance DNS: `<offering>-<instance>.lan` (colon-to-hyphen)
- DNS registration/removal is a side-effect of `transition_role()`, not scattered across callsites
- TLS proxy entry follows DNS for pond-enabled gardens

### Phase 4: Pinning & Lifecycle Edge Cases

**Effort:** ~1 week

- `garden-rake pin <offering> <stone>` / `unpin` commands
- Pin metadata storage on target Stone (score = 1001)
- Pin timestamp tiebreaker in election
- Pinned Stone recovery: automatic re-election and reclaim
- Graceful Primary removal: trigger election before `take_away_offering_v1` completes
- Last-copy seed-bank archival: prompt user if removing the last instance and seed-bank available

### Phase 5: Degradation & Graceful Handover

**Effort:** ~1-2 weeks

- Sustained threshold detection in Moss (consecutive health failures, resource pressure)
- Role transition to Degraded → chirp broadcasts `role: "degraded"` naturally (no `DEGRADATION_WARNING`)
- Implement `cordon_service_v1` (already stubbed) for connection draining
- Graceful handover sequence (detect → degrade → election → drain → DNS swap → demote)
- Catastrophic failure path (chirp heartbeat timeout → failure election)

### Phase 6: Lantern Chirp Coordination

**Effort:** ~1-2 weeks

- Election messages (`ELECTION_REQUEST`, `ELECTION_CANDIDATE`, `ELECTION_RESULT`) routed through Lantern for cross-subnet Stones
- Transport-layer concern: `p2p::send_announcement()` dual-paths to UDP + Lantern. Orchestration task never knows.
- Partition arbitration: Lantern-connected side is authoritative

### Phase 7: Observability

**Effort:** ~1 week

- `OrchestrationState` on `ToolProjection` (reuse domain type, `stripped_for_beacon()` for UDP)
- Presence stream events (automatic via `RoleChanged` event bus wiring)
- `garden-rake garden --orchestration` display (extends existing command)
- Lantern dashboard visualization

---

## References

- [OFFER-0003: Offering FQN](../decisions/OFFER-0003-offering-fqn.md)
- [OFFER-0004: Intelligent Offering Placement](../decisions/OFFER-0004-intelligent-offering-placement.md)
- [Koi Embedded Integration](koi-embedded-integration.md)
- [Lantern Registry](../specs/lantern.md)
- [Sub-Capabilities Proposal](sub-capabilities.md)
- [Nurturing Proposal](nurturing.md)
- [Same-Offering Orchestration (original, superseded)](same-offering-orchestration.md)
- [Topology Caching](discovery-topology-caching.md)
