# ORCH-0007: MongoDB Orchestrator

**Status:** Draft
**Date:** 2026-02-24
**Authors:** Leo Botinelly, Claude
**Depends On:** ORCH-0001 (Offering Orchestration), ORCH-0003 (Database Choreographer), ORCH-0008 (Orchestrator Common), Tools API, Koi DNS
**Supersedes:** ORCH-0003 (absorbs its MongoDB scope; ORCH-0003 retains the generalized database choreographer pattern for future databases)

### Dependency Status

| Dependency | Status | Notes |
|---|---|---|
| KOI-0001 (all phases) | **Done** | HTTP self-hosting, koi-udp, container wiring all merged |
| Tools API | **Done** | `GET /api/v1/garden/tools/stream` live |
| ORCH-0001 Phase 1 (types, elections) | **Done** | `OfferingRole`, `OrchestrationState`, election scoring |
| ORCH-0002 (Ollama orchestrator) | **Done** | Reference implementation for orchestrator binary pattern |
| ORCH-0004 (Gateway announcement) | **Done** | mDNS + Moss gateway registration |
| ORCH-0006 (Coordination mode) | **Accepted** | `coordination: elected` in MongoDB manifest |
| ORCH-0008 (Orchestrator Common) | **Proposed** | Shared crate extracted from Ollama; prerequisite |
| Koi DNS registration | **Not started** | `.dns()` handle exists but no registration code |
| Moss exec API | **Not started** | `POST /api/v1/stone/offerings/:name/exec` not implemented |

---

## Abstract

The MongoDB Orchestrator is a standalone binary that discovers all MongoDB instances across a Zen Garden, bootstraps them into MongoDB replica sets grouped by FQN, manages membership lifecycle, monitors cluster health, and publishes connection strings. It replaces the default ORCH-0001 singleton-with-replica policy with MongoDB's native oplog replication — where MongoDB handles data consistency, failover, and read/write routing natively.

Beyond basic choreography, the orchestrator provides operational intelligence: oplog window monitoring to prevent unrecoverable replication breaks, WiredTiger cache tuning per stone's hardware profile, locality-aware connection strings for applications, automated backup orchestration to seed banks, workload-aware placement recommendations, schema/index insight from `serverStatus()`, and automated failover validation.

The orchestrator follows the pattern established by ORCH-0002 (Ollama Orchestrator) — a standalone Rust binary with domain/infra/api/tasks layering, Koi mDNS discovery, Moss gateway registration, and a management dashboard. Shared infrastructure (discovery, gateway, tools stream) is extracted into ORCH-0008 (Orchestrator Common) and consumed by both orchestrators.

---

## Table of Contents

1. [Motivation](#motivation)
2. [Architecture](#architecture)
3. [Instance Discovery](#instance-discovery)
4. [Replica Set Bootstrap](#replica-set-bootstrap)
5. [Membership Management](#membership-management)
6. [Connection String Publication](#connection-string-publication)
7. [Health Monitoring](#health-monitoring)
8. [Oplog Window Guardian](#oplog-window-guardian)
9. [WiredTiger Cache Advisor](#wiredtiger-cache-advisor)
10. [Locality-Aware Routing](#locality-aware-routing)
11. [Backup Orchestration](#backup-orchestration)
12. [Workload-Aware Placement Advisor](#workload-aware-placement-advisor)
13. [Schema & Index Insight](#schema--index-insight)
14. [Failover Validation](#failover-validation)
15. [Multi-FQN Isolation](#multi-fqn-isolation)
16. [Command Execution](#command-execution)
17. [Dashboard](#dashboard)
18. [API Surface](#api-surface)
19. [CLI Integration](#cli-integration)
20. [Configuration & Persistence](#configuration--persistence)
21. [Data Flow: Who Does What](#data-flow-who-does-what)
22. [Interaction with ORCH-0001](#interaction-with-orch-0001)
23. [Edge Cases & Failure Modes](#edge-cases--failure-modes)
24. [Implementation Phases](#implementation-phases)

---

## Motivation

### The Problem

MongoDB supports replica sets natively, but setting them up requires:

1. Starting each `mongod` with `--replSet <name>`
2. Connecting to one instance and running `rs.initiate()`
3. Adding each additional member with `rs.add()`
4. Configuring read preferences and write concerns
5. Monitoring replica set health via `rs.status()`
6. Tracking replication lag and oplog window consumption
7. Managing connection strings (multi-host format with `replicaSet=` parameter)
8. Tuning WiredTiger cache for each machine's RAM
9. Running backups from secondaries to avoid primary impact
10. Knowing when a secondary has fallen so far behind it needs a full resync

In a traditional setup, an operator does all of this manually. In Zen Garden:

```bash
garden-rake offer mongodb                # on stone-01
garden-rake offer mongodb                # on stone-02
garden-rake offer mongodb                # on stone-03

# Result: three-node replica set, fully configured, healthy,
# connection string published, backups scheduled,
# oplog monitored, cache tuned, applications auto-discover
```

### Why Not ORCH-0001 Default Policy?

The default policy provides resilience via Moss-managed snapshot sync and elections. This works, but:

- MongoDB has superior replication (oplog-based, zero RPO)
- MongoDB's driver handles failover natively (no Moss election needed)
- Replica set connection strings give clients multi-host awareness
- Read scaling is possible (read from secondaries)
- Snapshot-based sync is hours of downtime for large datasets; oplog replay is seconds

### Why an Orchestrator, Not Just a Shell Script?

The orchestrator provides value beyond initial setup:

- **Continuous monitoring** — oplog window, replication lag, WiredTiger pressure
- **Proactive prevention** — alerts before a secondary falls past the oplog window
- **Hardware awareness** — tunes cache per stone's RAM, recommends placement
- **Garden integration** — connection string discovery via Koi DNS and wish resolution
- **Self-healing** — detects and responds to membership changes automatically
- **Backup automation** — seed bank integration, secondary-only backups

---

## Architecture

```
                    ┌─────────────────────────────┐
                    │    MongoDB Orchestrator      │
                    │    (standalone binary)       │
                    │                              │
                    │  ┌─────────┐  ┌───────────┐ │
                    │  │ domain/ │  │   api/     │ │
                    │  │ pure    │  │   :7192    │ │
                    │  │ logic   │  │  dashboard │ │
                    │  └────┬────┘  └─────┬─────┘ │
                    │       │             │        │
                    │  ┌────┴─────────────┴─────┐  │
                    │  │       infra/            │  │
                    │  │  mongo_client           │  │
                    │  │  orchestrator-common    │  │
                    │  └────────────────────────┘  │
                    └──┬──────────┬──────────┬─────┘
                       │          │          │
          rs.initiate  │    rs.add│          │ rs.status
                       │          │          │ mongodump
                  ┌────▼──┐  ┌───▼──┐  ┌───▼───┐
                  │mongod │  │mongod│  │mongod │
                  │PRIMARY│  │SECND │  │SECND  │
                  └───────┘  └──────┘  └───────┘
                 stone-01   stone-02   stone-03
                       │       │         │
                       └───────┼─────────┘
                               │
                     oplog replication
                     (MongoDB-managed)
```

The orchestrator is a lightweight process (~10MB) that:

1. Discovers MongoDB instances via Tools API + topology
2. Bootstraps replica sets per FQN
3. Manages membership (add/remove)
4. Monitors health, lag, oplog, cache pressure
5. Publishes connection strings
6. Advises on placement and tuning
7. Orchestrates backups to seed banks

It does NOT handle data replication — MongoDB's oplog does that natively.

---

## Instance Discovery

### Discovery Strategy

The orchestrator uses the same three-tier discovery established by ORCH-0002:

1. **Explicit stone** (`--stone` / `GARDEN_STONE`) — skip discovery entirely
2. **Cached tending state** — resume from previous run's tended stone
3. **Koi mDNS browse** — fresh discovery of `_moss._tcp` services

All three are provided by `orchestrator-common::discovery`.

### Topology Query

After resolving the tended stone, query `GET /api/v1/garden/topology` to find all stones with running MongoDB offerings:

```rust
// Filter topology entries for MongoDB instances
let has_mongodb = entry.services.iter().any(|s|
    s.offering == "mongodb" && s.status == "running"
);
```

Each matching stone provides:
- Stone name, ID, IP, Moss port
- Hardware capabilities (RAM for cache tuning)
- Offering role (if ORCH-0001 is active, transitioning to orchestrator control)

### Tools API Stream

Subscribe to `GET /api/v1/garden/tools/stream` and filter for `offering:mongodb*` (covers `mongodb`, `mongodb:analytics`, etc.):

```rust
fn is_mongodb_tool(fqid: &str) -> bool {
    fqid == "offering:mongodb" || fqid.starts_with("offering:mongodb:")
}
```

Events trigger:
- `tool.upsert` with MongoDB FQID → new instance or status change
- `tool.remove` with MongoDB FQID → instance disappeared

### FQN Grouping

Instances are grouped by FQN into independent replica sets:

| FQN | Replica Set Name | Instances |
|-----|-----------------|-----------|
| `mongodb` | `zen-garden` | All default MongoDB offerings |
| `mongodb:analytics` | `zen-garden-analytics` | Named instance group |
| `mongodb:staging` | `zen-garden-staging` | Named instance group |

---

## Replica Set Bootstrap

### Snippet Modification

The MongoDB snippet must start `mongod` with replica set configuration. The snippet changes from:

```yaml
# Current
services:
  mongodb:
    image: mongo:7
    ports:
      - "27017:27017"
```

To:

```yaml
# Updated
services:
  mongodb:
    image: mongo:{{ version | default(value="7") }}
    command: ["mongod", "--replSet", "zen-garden", "--bind_ip_all"]
    ports:
      - "27017:27017"
```

For named instances (`mongodb:analytics`), the replica set name is `zen-garden-analytics`. The orchestrator passes the FQN-derived name via environment variable `ZG_REPLICA_SET_NAME`, and the snippet uses it:

```yaml
command: ["mongod", "--replSet", "${ZG_REPLICA_SET_NAME:-zen-garden}", "--bind_ip_all"]
```

### First Instance

When the orchestrator detects the first MongoDB instance for a given FQN:

1. **Check existing state.** Query `rs.status()`. If it returns a valid replica set, the instance was previously initialized — map the topology and resume monitoring.

2. **Initiate.** If no replica set exists:

```javascript
rs.initiate({
  _id: "zen-garden",
  members: [
    { _id: 0, host: "stone-amber-ridge.local:27017" }
  ]
})
```

3. **Verify.** Poll `rs.status()` until the member shows `PRIMARY` state (typically 5-10 seconds).

4. **Publish.** Register connection string via gateway and Koi DNS.

### Additional Instances

When a new MongoDB instance appears for the same FQN:

1. **Wait for ready.** The Tools API reports `state: ready`.
2. **Check membership.** Query the current primary's `rs.status()` — is this member already known? (idempotency)
3. **Add member.** Execute `rs.add("stone-coral-reef.local:27017")` on the primary.
4. **Wait for sync.** Poll until the new member's `stateStr` transitions from `STARTUP2` to `SECONDARY`.
5. **Update connection string.** Republish with the new member included.

---

## Membership Management

### Intentional Removal

When a MongoDB instance is intentionally removed (`garden-rake lift mongodb`):

1. Detect instance disappearance via Tools API `tool.remove` event
2. If this was the primary, wait for MongoDB to auto-elect a new primary
3. Execute `rs.remove("stone.local:27017")` on the current primary
4. Update connection string

### Unplanned Failure

When a stone running MongoDB goes offline:

1. Detect via Tools API (instance goes stale/removed)
2. MongoDB internally detects the missing member and triggers its own election if the primary was lost (typically 10-12 seconds)
3. The orchestrator monitors `rs.status()` until a new primary is elected
4. The orchestrator does **NOT** run `rs.remove()` — the member may return
5. After a configurable timeout (default: 30 minutes), the orchestrator optionally removes the persistently-absent member

### Recovery

When a previously-failed member comes back:

1. MongoDB automatically reconnects it and begins oplog replay
2. Member catches up and transitions to `SECONDARY`
3. Orchestrator detects via `rs.status()` and updates topology state
4. No orchestrator action needed — MongoDB handles recovery

---

## Connection String Publication

### The Connection String

MongoDB replica set connection strings include all members:

```
mongodb://stone-amber-ridge.local:27017,stone-coral-reef.local:27017,stone-bronze-canyon.local:27017/?replicaSet=zen-garden
```

The MongoDB driver uses this to discover the current primary, handle failover automatically, and route reads to secondaries if configured.

### Publication Channels

**1. Gateway Registration (ORCH-0004)**

Self-register with Koi mDNS + Moss gateway:

- mDNS name: `mongodb-orchestrator` → resolvable on LAN
- Moss gateway: `PUT /api/v1/garden/gateway/mongodb` with `handler_for: ["mongodb"]`
- `rake find mongodb` returns orchestrator first, then raw instances

**2. DNS via Koi**

Register `mongodb.lan` → primary stone IP. Applications using `mongodb.lan:27017` are routed to the current primary. On failover, the DNS record moves.

**3. Wish Resolution**

Register the full replica set connection string:

```
zen-garden:mongodb → mongodb://stone-01:27017,stone-02:27017/?replicaSet=zen-garden
```

**4. Tools API Orchestration Fields**

The Tools API projection includes cluster metadata:

```json
{
  "tool_fqid": "offering:mongodb",
  "orchestration": {
    "policy": "clustered",
    "cluster_type": "replica-set",
    "connection_string": "mongodb://...",
    "primary": "stone-amber-ridge",
    "members": 3,
    "healthy_members": 3
  }
}
```

### Updates

On membership changes (add/remove/failover), all four channels are updated. MongoDB drivers already monitor topology changes via the driver's server monitoring protocol, but new connections benefit from the updated string.

---

## Health Monitoring

### Polling Cycle

The orchestrator polls `rs.status()` on the primary every 15 seconds. From each response, it extracts:

| Field | Use |
|-------|-----|
| `members[].stateStr` | Member role (PRIMARY, SECONDARY, ARBITER, etc.) |
| `members[].health` | 1 = reachable, 0 = unreachable |
| `members[].optimeDate` | Last operation time (lag calculation) |
| `members[].lastHeartbeat` | MongoDB's internal heartbeat |
| `set` | Replica set name |
| `ok` | Overall cluster health |

### Lag Monitoring

Replication lag per secondary:

```
lag = primary.optimeDate - secondary.optimeDate
```

Thresholds and actions:

| Lag | Severity | Action |
|-----|----------|--------|
| < 5s | Normal | No action |
| 5–30s | Warning | Log, emit presence event |
| 30s–5m | High | Dashboard alert, presence stream |
| > 5m | Critical | Alert, consider intervention |

### Presence Events

The orchestrator emits events on the Presence stream:

- `mongodb.cluster.initialized` — replica set created
- `mongodb.member.added` — new member joined
- `mongodb.member.removed` — member removed
- `mongodb.primary.changed` — MongoDB elected a new primary
- `mongodb.member.unreachable` — member health went to 0
- `mongodb.member.recovered` — member came back
- `mongodb.lag.warning` — replication lag exceeding threshold
- `mongodb.oplog.warning` — oplog window approaching danger zone
- `mongodb.backup.completed` — backup finished successfully
- `mongodb.failover.validated` — controlled failover test passed

---

## Oplog Window Guardian

### The Problem

The oplog is a capped collection with a fixed size. If a secondary falls behind past the oplog window (the time range covered by the oldest oplog entry to the newest), it can never catch up — it needs a full initial sync, which means copying the entire dataset. For large databases, this takes hours and the secondary is unavailable during that time.

This is the single most common MongoDB operational failure, and most teams don't monitor it until it's too late.

### How It Works

Every health check cycle (15s), the orchestrator queries:

```javascript
// On the primary
db.getReplicationInfo()
// Returns: { logSizeMB, usedMB, timeDiff (seconds), tFirst, tLast }
```

The `timeDiff` field is the oplog window — how many seconds of operations the oplog retains.

The orchestrator compares this against secondary lag:

```
oplog_remaining = oplog_window - max_secondary_lag
safety_ratio = oplog_remaining / oplog_window
```

### Thresholds and Actions

| Safety Ratio | Severity | Action |
|---|---|---|
| > 0.7 | Healthy | No action |
| 0.3–0.7 | Warning | Dashboard warning, presence event |
| 0.1–0.3 | Danger | Prominent alert, presence event with `attention` tag |
| < 0.1 | Critical | Trigger emergency backup of secondary, alert |
| 0 (lag > window) | **Unrecoverable** | Secondary needs full resync; alert operator, offer `rs.remove()` + re-add |

### Proactive Intervention

When safety ratio drops below 0.3:

1. If the lagging secondary is on a stone with high CPU/memory pressure, emit a placement recommendation ("consider moving other offerings off this stone")
2. If the oplog size is small relative to write throughput, emit a sizing recommendation ("consider increasing oplog size")
3. Log the write rate trend to help the operator understand whether this is a spike or sustained pressure

---

## WiredTiger Cache Advisor

### The Problem

MongoDB's WiredTiger storage engine defaults to using `50% of (RAM - 1GB)` for its internal cache. On small stones (2GB RAM), this gives only 512MB — potentially too small for working sets. On large stones (64GB RAM), this gives 31.5GB — potentially wasteful if the dataset is small.

### How It Works

The orchestrator knows each stone's total RAM from `HardwareCapabilities` (available in topology chirps). On each health check, it also queries:

```javascript
db.serverStatus().wiredTiger.cache
// Key fields: "bytes currently in the cache", "maximum bytes configured",
// "tracked dirty bytes in the cache", "pages evicted by application threads"
```

### Recommendations

| Condition | Recommendation |
|---|---|
| Cache hit ratio < 80% and dataset > cache size | "Increase `wiredTigerCacheSizeGB` — working set exceeds cache" |
| Application-thread evictions > 0 | "Memory pressure — WiredTiger is evicting under request load" |
| Cache utilization < 30% for > 1 hour | "Cache oversized — RAM could be used by other offerings" |
| Stone RAM < 2GB | "MongoDB minimum is 512MB cache — consider more RAM" |
| Stone RAM changed (nourishment) | Recalculate optimal cache size |

### Cache Size Calculation

```rust
fn recommended_cache_mb(stone_ram_mb: u64, other_offerings_count: u32) -> u64 {
    let base = (stone_ram_mb as f64 * 0.5) - 1024.0;
    let min = 256.0;

    // Reduce if many other offerings share this stone
    let pressure_factor = match other_offerings_count {
        0..=2 => 1.0,
        3..=5 => 0.8,
        _ => 0.6,
    };

    (base * pressure_factor).max(min) as u64
}
```

Recommendations are advisory — displayed on the dashboard. The orchestrator does NOT automatically change MongoDB configuration (that would require container restart and risks data issues).

---

## Locality-Aware Routing

### The Problem

Applications typically get a single connection string and connect to the primary for all operations. But if an application runs on the same stone as a secondary, reads could be served locally — faster and with no network overhead.

### How It Works

The orchestrator knows:
- Which stones run MongoDB instances (from topology)
- Which stones run application offerings (from topology)
- The role of each MongoDB member (from `rs.status()`)

When publishing connection strings, it generates **per-stone variants**:

```
# Default (for applications not co-located with any member)
mongodb://stone-01:27017,stone-02:27017,stone-03:27017/?replicaSet=zen-garden

# For applications on stone-02 (where a SECONDARY lives)
mongodb://stone-02:27017,stone-01:27017,stone-03:27017/?replicaSet=zen-garden&readPreference=secondaryPreferred
```

The MongoDB driver's server selection algorithm naturally prefers the first host in the seed list for initial connection, and `secondaryPreferred` allows reads from the local secondary while writes still go to the primary.

### Publication

Locality-aware connection strings are served via the orchestrator's management API:

```
GET /api/connect?from=stone-coral-reef
→ Returns the optimal connection string for that stone
```

Applications using wish resolution (`zen-garden:mongodb`) receive the default connection string. Applications that query the orchestrator directly can get the locality-optimized variant.

---

## Backup Orchestration

### Integration with Seed Banks

The garden already has seed banks (portable storage on USB/external drives). The orchestrator coordinates backups:

1. **Select source.** Always back up from a secondary (never the primary). Pick the secondary with the lowest lag.
2. **Execute.** Run `mongodump --host <secondary> --oplog --gzip --out <seed_bank_path>/garden/backups/mongodb/<timestamp>/`
3. **Verify.** Check exit code and output size.
4. **Record.** Write backup manifest to seed bank with metadata (timestamp, size, secondary used, oplog position).
5. **Prune.** Apply retention policy (configurable: default 7 daily, 4 weekly).

### Execution Method

For managed offerings, backup runs via Moss exec API:

```http
POST /api/v1/stone/offerings/mongodb/exec
{
  "command": ["mongodump", "--oplog", "--gzip", "--out", "/backup/2026-02-24T12:00:00Z/"],
  "timeout_seconds": 3600
}
```

The backup directory is mounted from the seed bank into the container.

### Schedule

Default schedule (configurable):

| Backup Type | Frequency | Retention |
|---|---|---|
| Full (`mongodump --oplog`) | Daily at 02:00 local | 7 daily |
| Weekly archive | Sunday 02:00 | 4 weekly |

### Dashboard Integration

The dashboard shows:

- Last backup time, size, duration, source secondary
- Backup history (success/failure timeline)
- Restore point coverage ("you can restore to any point in the last 7 days")
- Seed bank space remaining

### Restore

Restore is manual (destructive operations require operator intent):

```bash
garden-rake cluster restore mongodb --from 2026-02-23T02:00:00Z
```

This triggers `mongorestore` from the seed bank archive. The orchestrator handles:
1. Stop writes (step down primary briefly)
2. Restore to a clean instance
3. Resync secondaries
4. Verify and resume

---

## Workload-Aware Placement Advisor

### What It Knows

The orchestrator has access to:

| Data Source | Information |
|---|---|
| Topology chirps | CPU%, memory%, disk%, per stone |
| Hardware capabilities | RAM, CPU cores, disk type, GPU presence |
| Stone offerings | What else runs on each stone |
| `rs.status()` | Which member is primary, secondary |
| `serverStatus()` | MongoDB's own resource metrics |

### Recommendations

The advisor evaluates placement and emits recommendations:

**"Move PRIMARY to stone-X"**
- Stone-X has more RAM, SSD storage, lower CPU utilization
- Primary handles all writes — it benefits most from fast storage and available memory

**"MongoDB competes with Ollama on stone-Y"**
- Stone-Y runs GPU workloads that consume RAM
- MongoDB's WiredTiger cache gets memory-pressured
- Recommendation: move MongoDB secondary off this stone, or reduce cache size

**"stone-Z has spinning disk — expect slow reads"**
- Disk I/O is the bottleneck for MongoDB on HDD
- Recommendation: use this secondary only as a backup source, not for read scaling

**"Replica set has even number of members — add an arbiter or third member"**
- Even member counts risk split-vote elections
- Recommendation: plant an arbiter on a lightweight stone

### Scoring

Each stone gets a placement fitness score:

```rust
fn placement_score(stone: &StoneProfile) -> i32 {
    let mut score = 0;

    // Storage type
    score += if stone.has_ssd { 200 } else { 0 };

    // RAM headroom (after other offerings)
    score += ((stone.available_ram_mb as f64 / 1024.0) * 50.0).min(250.0) as i32;

    // CPU headroom
    score += ((100.0 - stone.cpu_percent) * 2.5) as i32;  // 0-250

    // Penalty for co-located GPU workloads
    score -= stone.gpu_offerings_count * 100;

    // Penalty for high disk utilization
    if stone.disk_percent > 80.0 { score -= 100; }

    score
}
```

Recommendations are advisory — displayed on the dashboard with reasoning.

---

## Schema & Index Insight

### What It Collects

Every 5 minutes (configurable), the orchestrator queries the primary:

```javascript
// Collection statistics
db.getCollectionNames().forEach(c => db[c].stats())

// Index usage
db.getCollectionNames().forEach(c => db[c].aggregate([{$indexStats: {}}]))

// Current operations (sampled)
db.currentOp({ "active": true, "secs_running": { "$gt": 1 } })
```

### Insights Surfaced

| Insight | Detection | Dashboard Display |
|---|---|---|
| **Missing indexes** | Collections with `totalIndexSize == 0` (only `_id`) | "collection `orders` has no secondary indexes — queries will scan all documents" |
| **Unused indexes** | `$indexStats` shows `accesses.ops == 0` for > 7 days | "index `users.email_1` hasn't been used in 14 days — consider dropping (saves 24MB)" |
| **Large collections** | `stats().size > threshold` | Size, document count, average document size, growth rate |
| **Slow queries** | `currentOp` with `secs_running > 5` | Operation type, namespace, duration, plan summary |
| **Index hit ratio** | `serverStatus().metrics.queryExecutor` | "92% of queries use an index" (vs full collection scans) |

### Boundaries

The orchestrator does NOT:
- Create or drop indexes (destructive, requires operator intent)
- Modify schema (not its job)
- Profile individual queries in depth (use MongoDB's profiler for that)

It surfaces the "20% effort, 80% value" basics that operators forget to check.

---

## Failover Validation

### Purpose

Operators need confidence that their replica set actually fails over correctly. The orchestrator can validate this automatically or on demand.

### Automatic Validation

Configurable (default: disabled). When enabled, the orchestrator periodically (e.g., weekly):

1. Announce a controlled failover test on the dashboard
2. Execute `rs.stepDown(60)` on the primary (steps down for 60 seconds)
3. Observe the election via `rs.status()` polling
4. Record: time to new primary election, which member won, total disruption window
5. The old primary automatically becomes eligible again after 60 seconds

### On-Demand Validation

```bash
garden-rake cluster failover-test mongodb
```

Or via the dashboard management API:

```http
POST /api/management/failover-test
```

### Dashboard Display

```
FAILOVER HISTORY

  2026-02-23 02:00  ✓ Planned validation    12s  stone-02 → stone-01
  2026-02-20 14:32  ⚡ Unplanned            8s   stone-01 → stone-02  (stone-01 offline)
  2026-02-16 02:00  ✓ Planned validation    11s  stone-01 → stone-03
```

### Alerts

- Failover took > 30 seconds → "Slow failover — investigate secondary lag or network issues"
- Failover failed (no new primary within 120s) → "Failover failure — manual intervention required"
- Same member always wins → "Election bias — consider rebalancing priorities"

---

## Multi-FQN Isolation

Multiple MongoDB FQNs map to independent replica sets, each fully isolated:

```
mongodb (default)       → replica set "zen-garden"
mongodb:analytics       → replica set "zen-garden-analytics"
mongodb:staging         → replica set "zen-garden-staging"
```

Each gets:
- Independent replica set with its own membership
- Separate connection string
- Separate backup schedule and retention
- Separate health monitoring and alerts
- Separate dashboard panel
- Separate placement recommendations

The orchestrator manages all FQN groups concurrently from a single binary instance.

### FQN Detection

The Tools API provides `tool_fqid` which encodes the FQN:

```
"offering:mongodb"            → FQN "mongodb"     → RS "zen-garden"
"offering:mongodb:analytics"  → FQN "mongodb:analytics" → RS "zen-garden-analytics"
```

---

## Command Execution

The orchestrator needs to execute MongoDB shell commands (`rs.status()`, `rs.add()`, `mongodump`, etc.) on remote stones. Two paths:

### Path 1: Moss Exec API (Preferred)

```http
POST /api/v1/stone/offerings/mongodb/exec
{
  "command": ["mongosh", "--quiet", "--eval", "JSON.stringify(rs.status())"],
  "timeout_seconds": 30
}
```

Response:

```json
{
  "exit_code": 0,
  "stdout": "{ ... rs.status() output ... }",
  "stderr": ""
}
```

This is the preferred path because the orchestrator doesn't need direct network access to MongoDB — it goes through Moss, which already manages the container.

**Note:** This API does not exist yet. It must be implemented in Moss as a prerequisite.

### Path 2: Direct MongoDB Wire Protocol (Fallback)

If the exec API is unavailable, the orchestrator connects directly to `mongod` on port 27017 using the `mongodb` Rust driver crate:

```rust
let client = mongodb::Client::with_uri_str(
    "mongodb://stone-amber-ridge.local:27017/?directConnection=true"
).await?;

let admin = client.database("admin");
let result = admin.run_command(doc! { "replSetGetStatus": 1 }).await?;
```

This requires network connectivity to port 27017 on each stone. Works for both managed and adopted offerings.

### Strategy

Phase 1 uses the direct wire protocol (Rust `mongodb` driver crate) as the primary execution path. This works immediately for both managed and adopted offerings without requiring Moss changes. The Moss exec API is added later as an alternative path — useful for operations that don't map cleanly to the wire protocol (e.g., `mongodump` invocation, file system access inside containers).

---

## Dashboard

### Layout

The dashboard serves on port 7191 (management API) and provides:

#### Cluster Overview Panel

```
MONGODB "zen-garden"                                    [healthy]
═══════════════════════════════════════════════════════════════

  stone-amber-ridge    PRIMARY     lag: —      uptime: 4d 12h    SSD  16GB
  stone-coral-reef     SECONDARY   lag: 0.2s   uptime: 4d 12h    SSD   8GB
  stone-bronze-canyon  SECONDARY   lag: 1.4s   uptime: 1d  3h    HDD   4GB

  CONNECTION STRING
  mongodb://stone-amber-ridge.local:27017,stone-coral-reef.local:27017,stone-bronze-canyon.local:27017/?replicaSet=zen-garden
```

#### Oplog Health Panel

```
OPLOG HEALTH                                            [healthy]
═══════════════════════════════════════════════════════════════

  Window:     72h 14m          Used:    847 MB / 2048 MB
  Write rate: 3.2 MB/h         Safety:  ████████████░░░░  71h remaining

  Max secondary lag: 1.4s      Safety ratio: 0.98
```

#### WiredTiger Panel

```
WIREDTIGER CACHE                                        [healthy]
═══════════════════════════════════════════════════════════════

  stone-amber-ridge    4096 MB    Hit: 97.2%    Dirty: 2.1%    ✓ optimal
  stone-coral-reef     2048 MB    Hit: 94.8%    Dirty: 1.3%    ✓ optimal
  stone-bronze-canyon  1024 MB    Hit: 78.4%    Dirty: 8.7%    ⚠ pressure

  ⚠ stone-bronze-canyon: cache hit ratio below 80% — working set may exceed
    cache. Consider increasing RAM or reducing dataset on this member.
```

#### Backup Panel

```
BACKUPS                                                 [healthy]
═══════════════════════════════════════════════════════════════

  Last backup:    2026-02-24 02:00    1.2 GB    from stone-coral-reef
  Next backup:    2026-02-25 02:00    (daily schedule)
  Restore points: 7 daily, 4 weekly
  Seed bank:      garden-data-01     142 GB free
```

#### Placement Advisor Panel

```
PLACEMENT ADVISOR
═══════════════════════════════════════════════════════════════

  ✓ Primary on stone-amber-ridge (score: 450) — best fit

  ⚠ stone-bronze-canyon (score: 120):
    • HDD storage — reads will be slower than SSD peers
    • 78% cache hit ratio — working set exceeds available cache
    • Recommendation: Use as backup source only, not for read scaling

  ℹ stone-coral-reef (score: 380):
    • Co-located with Ollama (GPU workload) — monitor RAM pressure
```

#### Schema Insights Panel

```
SCHEMA INSIGHTS
═══════════════════════════════════════════════════════════════

  Collections: 12       Total size: 2.4 GB      Indexes: 28

  ⚠ UNUSED INDEXES (2)
    users.legacy_email_idx      0 ops / 14 days    saves 24 MB
    orders.created_at_-1_1      0 ops / 30 days    saves 112 MB

  ⚠ UNINDEXED COLLECTIONS (1)
    audit_logs                  450K docs, 890 MB  — no secondary indexes

  SLOW QUERIES (last hour)
    3 queries > 5s on orders collection (full scan)
```

#### Failover History Panel

```
FAILOVER HISTORY
═══════════════════════════════════════════════════════════════

  2026-02-23 02:00  ✓ Validation    12s    stone-02 → stone-01
  2026-02-20 14:32  ⚡ Unplanned     8s    stone-01 → stone-02
  2026-02-16 02:00  ✓ Validation    11s    stone-01 → stone-03

  Average failover time: 10.3s    Last test: 1 day ago
```

---

## API Surface

### Management API (Port 7191)

#### Dashboard & Status

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/` | HTML dashboard |
| GET | `/api/status` | Full cluster status (JSON) |
| GET | `/api/events` | SSE stream of cluster events |
| GET | `/health` | Health check |

#### Cluster Operations

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/cluster/status` | Replica set status |
| GET | `/api/cluster/members` | Member list with details |
| GET | `/api/cluster/connect` | Connection string |
| GET | `/api/cluster/connect?from={stone}` | Locality-aware connection string |
| POST | `/api/cluster/stepdown` | Force primary stepdown |
| POST | `/api/cluster/failover-test` | Controlled failover validation |

#### Monitoring

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/monitoring/oplog` | Oplog window health |
| GET | `/api/monitoring/cache` | WiredTiger cache status per member |
| GET | `/api/monitoring/lag` | Replication lag per secondary |
| GET | `/api/monitoring/placement` | Placement advisor recommendations |

#### Schema & Indexes

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/schema/collections` | Collection statistics |
| GET | `/api/schema/indexes` | Index usage statistics |
| GET | `/api/schema/slow-queries` | Recent slow operations |

#### Backups

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/backups` | Backup history |
| POST | `/api/backups/trigger` | Trigger immediate backup |
| GET | `/api/backups/schedule` | Current schedule |
| POST | `/api/backups/schedule` | Update schedule |

#### Settings

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/settings` | Current configuration |
| POST | `/api/settings` | Update configuration |
| GET | `/api/jobs` | Background job status |

---

## CLI Integration

### Rake Commands

```bash
# Cluster status overview
garden-rake cluster status mongodb

# View connection string (default)
garden-rake cluster connect mongodb

# View connection string (locality-optimized for current stone)
garden-rake cluster connect mongodb --local

# View replication lag
garden-rake cluster lag mongodb

# Force primary stepdown
garden-rake cluster stepdown mongodb

# Run failover validation
garden-rake cluster failover-test mongodb

# Trigger immediate backup
garden-rake cluster backup mongodb

# View backup history
garden-rake cluster backup mongodb --history
```

### Policy Management

```bash
# Enable orchestrator for MongoDB (deploys orchestrator, disables ORCH-0001)
garden-rake policy mongodb clustered

# Revert to default policy (removes orchestrator, re-enables ORCH-0001)
garden-rake policy mongodb none

# View current policy
garden-rake policy mongodb
```

---

## Configuration & Persistence

### Configuration File

`{data_dir}/config.toml`:

```toml
[general]
health_check_interval_secs = 15
member_removal_timeout_secs = 1800  # 30 minutes before removing failed member

[oplog]
warning_safety_ratio = 0.3
danger_safety_ratio = 0.1

[cache]
low_hit_ratio_threshold = 0.80
high_dirty_threshold = 0.10

[backup]
enabled = true
schedule = "daily"
time = "02:00"
retention_daily = 7
retention_weekly = 4
seed_bank = "auto"  # Use first available seed bank

[failover_validation]
enabled = false
schedule = "weekly"
day = "sunday"
time = "03:00"

[schema_insight]
enabled = true
collection_interval_secs = 300  # 5 minutes
slow_query_threshold_secs = 5

[placement]
enabled = true
evaluation_interval_secs = 300
```

### Persisted State

`{data_dir}/state.json`:

```json
{
  "replica_sets": {
    "zen-garden": {
      "initialized": true,
      "members": ["stone-amber-ridge:27017", "stone-coral-reef:27017"],
      "last_primary": "stone-amber-ridge",
      "failover_history": [...]
    }
  },
  "backups": {
    "last_backup": "2026-02-24T02:00:00Z",
    "history": [...]
  }
}
```

Note: The orchestrator rebuilds its live view from `rs.status()` on every startup. The persisted state is for backup history, failover history, and configuration — not for replica set topology (MongoDB is the source of truth for that).

---

## Data Flow: Who Does What

| Concern | Handled By | NOT By |
|---------|-----------|--------|
| Data replication | MongoDB oplog | Orchestrator |
| Failover election | MongoDB internal | Orchestrator or Moss |
| Read/write routing | MongoDB driver | Orchestrator |
| Replica set initialization | Orchestrator | Moss |
| Member add/remove | Orchestrator | Application |
| Connection string publication | Orchestrator | Moss |
| Health monitoring & alerting | Orchestrator | Application |
| Oplog window monitoring | Orchestrator | Application |
| WiredTiger cache analysis | Orchestrator | MongoDB (it just uses defaults) |
| Backup scheduling & execution | Orchestrator | Operator (manual) |
| Placement recommendations | Orchestrator | Operator (manual analysis) |
| Schema/index insight | Orchestrator | DBA (manual profiling) |
| Container lifecycle | Moss | Orchestrator |
| DNS registration | Orchestrator (via Koi) | Moss default policy |

---

## Interaction with ORCH-0001

When the `clustered` policy is applied:

1. **ORCH-0001 default policy is disabled** for this offering FQN. Moss no longer treats multiple MongoDB instances as primary/dormant with elections and sync.
2. **All instances are active.** MongoDB manages its own roles (PRIMARY/SECONDARY).
3. **DNS publication changes.** The orchestrator registers DNS, not the ORCH-0001 election winner.
4. **Sync is MongoDB's job.** No Moss-managed snapshot sync, no cursor tracking.
5. **Container lifecycle remains Moss's job.** Starting, stopping, health-checking the container is still Moss. The orchestrator just manages membership.

### Reverting to Default

```bash
garden-rake policy mongodb none
```

The orchestrator is removed. MongoDB instances revert to the ORCH-0001 default policy. The existing replica set configuration inside MongoDB is orphaned but harmless — each instance continues running independently.

---

## Edge Cases & Failure Modes

### Orchestrator Starts After MongoDB Instances

No problem. The orchestrator queries `rs.status()` on the first reachable instance. If the replica set exists, it maps the topology and resumes monitoring. If it doesn't exist yet, it bootstraps.

### All MongoDB Instances Restart Simultaneously

MongoDB handles this. Each instance starts with `--replSet zen-garden`, discovers the existing replica set config from its data files, and rejoins. The orchestrator detects the topology rebuilding and waits for a primary to be elected.

### Network Partition Between Members

MongoDB's election protocol handles this. The side with majority quorum elects a primary. The minority side's members become SECONDARY (read-only) or enter `(not reachable/healthy)` state. The orchestrator observes via `rs.status()` and reports on the dashboard.

For 3-member sets: 2-member side gets a primary, 1-member side stays secondary.
For 2-member sets: neither side gets a primary (no majority). This is why the placement advisor recommends odd member counts.

### Orchestrator Itself Fails

The orchestrator is stateless (rebuilds from `rs.status()`). If it dies:
- MongoDB continues operating normally (replication, failover, queries all work)
- No new members are added/removed until orchestrator restarts
- Backups stop until orchestrator restarts
- Dashboard and monitoring go offline

The orchestrator can use ORCH-0001 default policy for its own HA (primary/dormant orchestrator instances).

### Mixed Managed and Adopted Instances

Managed (container) and adopted (native) MongoDB instances can coexist in the same replica set. The orchestrator adjusts its command execution method per instance — `docker exec` for managed, direct `mongosh` for adopted.

### Even Member Count

The orchestrator detects even member counts and recommends adding an arbiter:

```
⚠ Replica set has 2 voting members — split-vote elections are possible.
  Consider: garden-rake offer mongodb --arbiter on a lightweight stone.
```

---

## Implementation Phases

### Phase 0: Prerequisites

**Effort:** ~1 week

| Task | Deliverable |
|---|---|
| ORCH-0008: Extract orchestrator-common crate | `src/orchestrators/common/` with discovery, gateway, tools stream, HTTP helpers |
| Refactor Ollama orchestrator to use orchestrator-common | Ollama compiles and tests pass with shared crate |
| Implement Moss exec API | `POST /api/v1/stone/offerings/:name/exec` for container command execution |
| Modify MongoDB snippet | Add `--replSet zen-garden --bind_ip_all` to `mongodb.snippet.yaml` |

### Phase 1: Discovery & Bootstrap

**Effort:** ~1 week

| Task | Deliverable |
|---|---|
| Create MongoDB orchestrator skeleton | `src/orchestrators/mongodb/` with Cargo.toml, main.rs, app_state.rs |
| Implement `infra/mongo_client.rs` | Command execution via Moss exec API + direct wire protocol fallback |
| Implement `domain/types.rs` | `MongoInstance`, `ReplicaSetState`, `MemberState`, `ReplicaSetConfig` |
| Implement `domain/bootstrap.rs` | `rs.initiate()` / `rs.add()` decision logic (pure domain) |
| Implement `tasks/discovery.rs` | Find MongoDB instances via topology + tools stream |
| Implement `tasks/bootstrap.rs` | React to new instances, bootstrap/join replica set |
| Implement connection string generation | Build and update multi-host connection strings |
| Implement gateway announce | Register with Koi mDNS + Moss gateway (from orchestrator-common) |
| Basic `api/health.rs` | Health check endpoint |
| Dockerfile + push.bat | Container build infrastructure |

### Phase 2: Health Monitoring & Oplog Guardian

**Effort:** ~1 week

| Task | Deliverable |
|---|---|
| Implement `tasks/health_monitor.rs` | Periodic `rs.status()` polling (15s) |
| Implement `domain/health.rs` | Lag calculation, threshold evaluation, presence events |
| Implement `domain/oplog.rs` | Oplog window monitoring, safety ratio calculation |
| Implement `tasks/oplog_guardian.rs` | Continuous oplog health evaluation with alerts |
| Implement membership management | Handle `tool.remove` events, planned/unplanned removal |
| Implement presence event emission | Emit `mongodb.*` events on the presence stream |
| Connection string updates | Republish on membership changes |

### Phase 3: WiredTiger Advisor & Cache Tuning

**Effort:** ~3-5 days

| Task | Deliverable |
|---|---|
| Implement `domain/cache_advisor.rs` | Cache analysis, hit ratio, eviction detection |
| Implement cache data collection | Query `serverStatus().wiredTiger.cache` per member |
| Implement `domain/placement.rs` | Placement scoring algorithm |
| Implement placement data collection | Aggregate stone profiles from topology + hardware capabilities |
| Configuration system | `config.toml` loading, defaults, runtime updates |

### Phase 4: Backup Orchestration

**Effort:** ~1 week

| Task | Deliverable |
|---|---|
| Implement `domain/backup.rs` | Backup scheduling, source selection, retention policy |
| Implement `tasks/backup_scheduler.rs` | Cron-like backup trigger |
| Implement `infra/backup_executor.rs` | `mongodump` execution via Moss exec API |
| Implement seed bank integration | Discover seed banks, write backup manifests, prune old backups |
| Implement backup history | Persist and query backup records |

### Phase 5: Schema Insight & Failover Validation

**Effort:** ~1 week

| Task | Deliverable |
|---|---|
| Implement `domain/schema_insight.rs` | Collection stats, index usage analysis, slow query detection |
| Implement `tasks/schema_collector.rs` | Periodic data collection (5 min cycle) |
| Implement `domain/failover.rs` | Controlled `rs.stepDown()`, election observation, result recording |
| Implement failover history | Persist and query failover events |
| Locality-aware connection strings | Per-stone connection string variants |

### Phase 6: Dashboard & Management API

**Effort:** ~1 week

| Task | Deliverable |
|---|---|
| Implement `api/dashboard.rs` | HTML dashboard with all panels |
| Implement `api/cluster.rs` | Cluster operations endpoints |
| Implement `api/monitoring.rs` | Oplog, cache, lag, placement endpoints |
| Implement `api/schema.rs` | Schema insight endpoints |
| Implement `api/backup.rs` | Backup management endpoints |
| Implement `api/management.rs` | Settings, jobs, failover-test endpoints |
| SSE event stream | Real-time cluster events on dashboard |
| Snapshot publisher task | Pre-build dashboard JSON every 5s |

### Phase 7: CLI Integration & Policy

**Effort:** ~3-5 days

| Task | Deliverable |
|---|---|
| Add `garden-rake cluster` subcommands | status, connect, lag, stepdown, failover-test, backup |
| Add `garden-rake policy mongodb clustered/none` | Policy toggle with orchestrator deployment/removal |
| Integration testing | End-to-end test with multi-stone MongoDB setup |

---

## References

- [ORCH-0001: Offering Orchestration](ORCH-0001-offering-orchestration.md) — default policy, elections, sync
- [ORCH-0002: AI Capability Router](ORCH-0002-ai-capability-router.md) — reference orchestrator implementation
- [ORCH-0003: Database Choreographer](ORCH-0003-database-choreographer.md) — generalized database choreography (this proposal implements the MongoDB specialization)
- [ORCH-0004: Gateway Announcement](../../decisions/ORCH-0004-gateway-announcement.md) — mDNS + Moss gateway registration
- [ORCH-0006: Coordination Mode](../../decisions/ORCH-0006-coordination-mode.md) — `coordination: elected` for stateful offerings
- [ORCH-0008: Orchestrator Common](ORCH-0008-orchestrator-common.md) — shared crate extraction
- [MongoDB Replica Set Documentation](https://www.mongodb.com/docs/manual/replication/)
- [MongoDB `serverStatus` Reference](https://www.mongodb.com/docs/manual/reference/command/serverStatus/)
