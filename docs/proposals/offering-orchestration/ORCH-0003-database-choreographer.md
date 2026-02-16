# ORCH-0003: Database Choreographer (MongoDB)

**Status:** Draft  
**Date:** 2026-02-16  
**Authors:** Leo Botinelly, Claude  
**Depends On:** ORCH-0001 (Offering Orchestration), Tools API, Koi DNS  
**Policy Trigger:** `garden-rake policy mongodb clustered`

---

## Abstract

The Database Choreographer is a specialized orchestrator offering that discovers all instances of a MongoDB offering, bootstraps them into a replica set, manages membership changes, monitors cluster health, and publishes the appropriate multi-host connection string. It replaces the default singleton-with-replica policy with MongoDB's native replication, where MongoDB itself handles data consistency, failover, and read/write routing.

The choreographer's role is *lifecycle management* — it sets up the cluster and keeps it healthy. MongoDB does the actual replication. The application gets a standard MongoDB replica set connection string and the driver handles the rest.

---

## Table of Contents

1. [Motivation](#motivation)
2. [Architecture](#architecture)
3. [Instance Discovery & Topology Mapping](#instance-discovery--topology-mapping)
4. [Replica Set Bootstrap](#replica-set-bootstrap)
5. [Membership Management](#membership-management)
6. [Connection String Publication](#connection-string-publication)
7. [Health Monitoring](#health-monitoring)
8. [The Choreographer Offering](#the-choreographer-offering)
9. [Data Flow: Who Does What](#data-flow-who-does-what)
10. [Interaction with ORCH-0001 Default Policy](#interaction-with-orch-0001-default-policy)
11. [CLI Integration](#cli-integration)
12. [Manifest](#manifest)
13. [Edge Cases & Failure Modes](#edge-cases--failure-modes)
14. [Generalization to Other Databases](#generalization-to-other-databases)
15. [Implementation Phases](#implementation-phases)

---

## Motivation

### The Problem

MongoDB supports replica sets natively, but setting them up requires:

1. Starting each mongod instance with `--replSet <name>`
2. Connecting to one instance and running `rs.initiate()`
3. Adding each additional member with `rs.add()`
4. Configuring read preferences and write concerns
5. Monitoring replica set health (`rs.status()`)
6. Handling member failures (MongoDB does this internally, but operator needs visibility)
7. Managing connection strings (multi-host format with `replicaSet=` parameter)

In a traditional setup, an operator does all of this manually. In Zen Garden, the experience should be:

```bash
garden-rake offer mongodb
garden-rake offer mongodb        # on another stone
garden-rake offer mongodb        # on a third stone

# Result: three-node replica set, fully configured, healthy, 
# connection string published, applications auto-discover
```

### Why Not Default Policy?

The ORCH-0001 default policy (singleton-with-replica) provides resilience via Moss-managed snapshot sync and elections. This works, but:

- MongoDB has its own superior replication (oplog-based, zero RPO)
- MongoDB's driver handles failover natively (no Moss election needed)
- Replica set connection strings give clients multi-host awareness
- Read scaling is possible (read from secondaries)

Using MongoDB's native replication is strictly better than Moss-managed snapshots — but it requires choreography that Moss shouldn't embed in its core.

---

## Architecture

```
                    ┌────────────────────┐
                    │  DB Choreographer  │ ◄── watches topology, manages lifecycle
                    │  (offering)        │
                    └──┬──────┬──────┬───┘
                       │      │      │
            rs.initiate│  rs.add     │ rs.add
                       │      │      │
                  ┌────▼──┐ ┌─▼───┐ ┌▼─────┐
                  │mongod │ │mongod│ │mongod│
                  │PRIMARY│ │SECND │ │SECND │
                  └───────┘ └──────┘ └──────┘
                 stone-01   stone-02  stone-03
                       │      │      │
                       └──────┼──────┘
                              │
                    oplog replication
                    (MongoDB-managed)
```

The choreographer is a lightweight service that:
1. Discovers MongoDB instances via Tools API
2. Bootstraps the replica set
3. Manages membership (add/remove)
4. Monitors health
5. Publishes connection string

It does NOT handle data replication — MongoDB does that natively via oplog.

---

## Instance Discovery & Topology Mapping

### Discovery

The choreographer subscribes to the Tools API stream:

```http
GET /api/v1/garden/tools/stream?tool_type=offering&tool_fqid=offering:mongodb
```

Each MongoDB tool entry provides:

```json
{
  "tool_fqid": "offering:mongodb",
  "stone_name": "stone-amber-ridge",
  "connection": {
    "hostname": "stone-amber-ridge.local",
    "ip": "192.168.1.42",
    "port": 27017
  },
  "state": "ready"
}
```

### Topology State

The choreographer maintains internal state:

```json
{
  "replica_set_name": "zen-garden",
  "initialized": true,
  "members": [
    {
      "stone_name": "stone-amber-ridge",
      "endpoint": "stone-amber-ridge.local:27017",
      "role": "PRIMARY",
      "state_str": "PRIMARY",
      "health": 1,
      "uptime": 86400,
      "optime": { "ts": "Timestamp(1739721600, 1)" },
      "last_heartbeat": "2026-02-16T15:00:00Z"
    },
    {
      "stone_name": "stone-coral-reef",
      "endpoint": "stone-coral-reef.local:27017",
      "role": "SECONDARY",
      "state_str": "SECONDARY",
      "health": 1,
      "lag_seconds": 0,
      "last_heartbeat": "2026-02-16T15:00:00Z"
    }
  ]
}
```

This state is rebuilt from MongoDB's `rs.status()` on every health check cycle — it's not persisted, because MongoDB itself is the source of truth.

---

## Replica Set Bootstrap

### First Instance

When the choreographer detects the first MongoDB instance:

1. **Check if already a replica set.** Query `rs.status()`. If it returns a valid status, the instance is already in a replica set (possibly from a previous choreographer run or manual setup). Skip initialization.

2. **Verify configuration.** The MongoDB container must have been started with `--replSet zen-garden`. This is handled by the offering template's `snippet_yaml`:

```yaml
# In mongodb.snippet.yaml
services:
  mongodb:
    image: mongo:{{ version | default(value="7") }}
    command: ["mongod", "--replSet", "zen-garden", "--bind_ip_all"]
    ports:
      - "{{ port | default(value=27017) }}:27017"
    volumes:
      - zen-mongodb-data:/data/db
```

3. **Initiate.** Run `rs.initiate()` with the single member:

```javascript
rs.initiate({
  _id: "zen-garden",
  members: [
    { _id: 0, host: "stone-amber-ridge.local:27017" }
  ]
})
```

The choreographer executes this via `docker exec` on the container (for managed offerings) or `mongosh` connection (for adopted offerings).

4. **Verify.** Poll `rs.status()` until the member shows `PRIMARY` state (typically 5-10 seconds).

### Additional Instances

When a new MongoDB instance appears:

1. **Wait for ready.** The new instance must be healthy (Tools API reports `state: ready`).

2. **Check replica set state.** Query the current primary's `rs.status()`. Verify the new instance isn't already a member (idempotency).

3. **Add member.** Execute on the primary:

```javascript
rs.add("stone-coral-reef.local:27017")
```

4. **Wait for sync.** Poll `rs.status()` until the new member shows `SECONDARY` and `stateStr != "STARTUP2"`. This can take minutes for large databases (initial sync copies all data).

5. **Update connection string.** Republish with the new member included.

### Execution Method

The choreographer runs MongoDB commands via two possible paths:

| Offering Mode | Execution Method |
|--------------|-----------------|
| Managed (container) | `docker exec zen-offering-mongodb mongosh --eval '<command>'` via Moss API |
| Adopted (native) | `mongosh --host <endpoint> --eval '<command>'` via SSH or local exec |

For managed offerings, the choreographer sends the command to the Stone's Moss instance, which runs `docker exec`. This avoids the choreographer needing direct Docker access to remote Stones.

### Moss API for Command Execution

The choreographer uses the existing Moss offerings API to execute commands:

```http
POST /api/v1/stone/offerings/mongodb/exec
{
  "command": ["mongosh", "--eval", "rs.status()"],
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

---

## Membership Management

### Member Removal (Planned)

When a MongoDB instance is intentionally removed (`garden-rake lift mongodb` on a Stone):

1. Choreographer detects instance disappearance via Tools API `tool.remove` event
2. Check if this was the primary (if so, MongoDB will auto-elect a new primary)
3. Wait for new primary if needed
4. Execute `rs.remove("stone.local:27017")` on the current primary
5. Update connection string

### Member Failure (Unplanned)

When a Stone running MongoDB goes offline:

1. Choreographer detects via Tools API (instance goes stale/removed)
2. MongoDB internally detects the missing member and triggers its own election if the primary was lost
3. Choreographer monitors `rs.status()` until a new primary is elected (MongoDB handles this, typically 10-12 seconds)
4. Choreographer does NOT run `rs.remove()` — the member may come back
5. If the member is gone for a configurable period (e.g., 30 minutes), choreographer optionally removes it

### Member Recovery

When a previously-failed member comes back:

1. MongoDB automatically reconnects it and begins oplog replay
2. Member catches up and transitions to SECONDARY
3. Choreographer detects via `rs.status()` and updates its topology state
4. No choreographer action needed — MongoDB handles recovery

This is the key advantage over the ORCH-0001 default policy: MongoDB's replication handles all the hard problems (data sync, conflict resolution, catchup). The choreographer just manages membership.

---

## Connection String Publication

### The Connection String

MongoDB replica set connection strings include all members:

```
mongodb://stone-amber-ridge.local:27017,stone-coral-reef.local:27017,stone-bronze-canyon.local:27017/?replicaSet=zen-garden
```

The MongoDB driver uses this to:
- Discover the current primary
- Handle failover automatically
- Route reads to secondaries (if configured)

### Publication Mechanism

The choreographer publishes the connection string through three channels:

**1. DNS via Koi**

Register a DNS entry for the MongoDB offering:

```
mongodb.lan → <primary-stone-ip>
```

Note: This gives basic access. Applications using the replica set connection string get full multi-host awareness. Applications using just `mongodb.lan` get routed to the current primary.

**2. Wish Resolution**

The choreographer registers the connection string with the garden's resolution system:

```
zen-garden:mongodb → mongodb://stone-01:27017,stone-02:27017,stone-03:27017/?replicaSet=zen-garden
```

Applications using wishful resolution get the full replica set string.

**3. Tools API**

The orchestration state in the Tools API includes the connection string:

```json
{
  "tool_fqid": "offering:mongodb",
  "orchestration": {
    "policy": "clustered",
    "cluster_type": "replica-set",
    "connection_string": "mongodb://stone-01:27017,stone-02:27017,stone-03:27017/?replicaSet=zen-garden",
    "primary": "stone-amber-ridge",
    "members": 3,
    "healthy_members": 3
  }
}
```

### Connection String Updates

When membership changes (add/remove), the choreographer republishes on all three channels. Applications using the MongoDB driver will discover topology changes via the driver's monitoring — they don't need to re-resolve the connection string. But new applications connecting after the change will get the updated string.

---

## Health Monitoring

### Polling Cycle

The choreographer polls `rs.status()` on the primary every 15 seconds:

```javascript
rs.status()
```

From this response, it extracts:

| Field | Use |
|-------|-----|
| `members[].stateStr` | Member role (PRIMARY, SECONDARY, ARBITER, etc.) |
| `members[].health` | 1 = reachable, 0 = unreachable |
| `members[].optimeDate` | Last operation time (for lag calculation) |
| `members[].lastHeartbeat` | MongoDB's internal heartbeat |
| `set` | Replica set name |
| `ok` | Overall cluster health |

### Lag Monitoring

Replication lag per secondary:

```
lag = primary.optimeDate - secondary.optimeDate
```

The choreographer tracks lag and emits warnings if it exceeds thresholds:

| Lag | Severity | Action |
|-----|----------|--------|
| < 5s | Normal | No action |
| 5-30s | Warning | Log, emit presence event |
| 30s-5m | High | Alert via presence stream |
| > 5m | Critical | Alert, consider intervention |

### Presence Events

The choreographer emits events on the Presence stream:

- `mongodb.cluster_initialized` — Replica set created
- `mongodb.member_added` — New member joined
- `mongodb.member_removed` — Member removed
- `mongodb.primary_changed` — MongoDB elected a new primary
- `mongodb.member_unreachable` — Member health went to 0
- `mongodb.member_recovered` — Member came back
- `mongodb.lag_warning` — Replication lag exceeding threshold

---

## The Choreographer Offering

```yaml
name: db-choreographer
category: infrastructure
tags: [orchestrator, database, replication]
replicable: true    # Can have standby for HA

image: zen-garden/db-choreographer:latest
ports:
  - 7191:7191       # Management API

environment:
  - CHOREOGRAPHER_TARGET_OFFERING=mongodb
  - CHOREOGRAPHER_REPLICA_SET_NAME=zen-garden
  - CHOREOGRAPHER_TOOLS_ENDPOINT=http://localhost:7185
  - CHOREOGRAPHER_HEALTH_INTERVAL=15
  - CHOREOGRAPHER_REMOVAL_TIMEOUT=1800  # 30 minutes before removing failed member
```

### Multi-Offering Support

The choreographer can manage multiple database offerings, each in its own replica set:

```
mongodb (default) → replica set "zen-garden"
mongodb:analytics → replica set "zen-garden-analytics"
```

The replica set name is derived from the FQN: `zen-garden` for default instance, `zen-garden-<instance>` for named instances.

### Choreographer Itself

The choreographer is stateless — it rebuilds its view from MongoDB's `rs.status()` on startup. This means:

- If the choreographer dies and restarts, it polls the existing replica set and resumes monitoring
- If the choreographer moves to a different Stone, it reconnects to the same replica set
- No persistent state to sync (the choreographer can use the default ORCH-0001 policy for its own HA)

---

## Data Flow: Who Does What

This is critical to understand — the choreographer *orchestrates*, it doesn't *replicate*:

| Concern | Handled By | Not By |
|---------|-----------|--------|
| Data replication | MongoDB oplog | Choreographer |
| Failover election | MongoDB internal | Choreographer or Moss |
| Read/write routing | MongoDB driver | Choreographer |
| Replica set initialization | Choreographer | Moss |
| Member add/remove | Choreographer | Application |
| Connection string publication | Choreographer | Moss |
| Health monitoring & alerting | Choreographer | Application |
| Container lifecycle | Moss | Choreographer |
| DNS registration | Choreographer (via Koi) | Moss default policy |

The choreographer is a thin layer between Moss (which manages containers) and MongoDB (which manages data). It bridges the gap by translating garden topology events into MongoDB membership commands.

---

## Interaction with ORCH-0001 Default Policy

When the `clustered` policy is applied:

1. **ORCH-0001 default policy is disabled** for this offering FQN. Moss no longer treats multiple MongoDB instances as primary/replica with elections and sync.
2. **All instances are active.** MongoDB manages its own roles (PRIMARY/SECONDARY).
3. **DNS publication changes.** The choreographer registers DNS, not the ORCH-0001 election winner.
4. **Sync is MongoDB's job.** No Moss-managed snapshot sync, no cursor tracking.
5. **Container lifecycle remains Moss's job.** Starting, stopping, health-checking the container is still Moss. The choreographer just tells MongoDB to adjust its membership.

### Reverting to Default

```bash
garden-rake policy mongodb none
```

This removes the choreographer. MongoDB instances revert to the ORCH-0001 default policy (Moss elections, snapshot sync). The existing replica set configuration inside MongoDB is orphaned but harmless — each instance continues running as an independent standalone (or you'd need to step down the replica set manually).

---

## CLI Integration

### Status

```bash
$ garden-rake cluster status mongodb

  MONGODB REPLICA SET "zen-garden"    [healthy]

  MEMBERS (3)

    stone-amber-ridge    PRIMARY     lag: 0s     uptime: 4d 12h
    stone-coral-reef     SECONDARY   lag: 0s     uptime: 4d 12h
    stone-bronze-canyon  SECONDARY   lag: 2s     uptime: 1d 3h

  CONNECTION STRING

    mongodb://stone-amber-ridge.local:27017,stone-coral-reef.local:27017,stone-bronze-canyon.local:27017/?replicaSet=zen-garden

  HEALTH

    Last check: 3s ago
    Oplog window: 72h
    Data size: 2.4 GB
```

### Commands

```bash
# View cluster status
garden-rake cluster status mongodb

# Force step-down (trigger MongoDB election)
garden-rake cluster stepdown mongodb

# View replication lag details
garden-rake cluster lag mongodb

# View connection string
garden-rake cluster connect mongodb
```

---

## Manifest

```yaml
# offerings/db-choreographer.manifest.yaml
name: db-choreographer
category: infrastructure
tags: [orchestrator, database]
protocols:
  - name: http
    port: 7191
    default: true

replicable: true

# Choreographer-specific config
orchestration:
  target_offerings:
    - name: mongodb
      cluster_type: replica-set
      init_command: "rs.initiate({_id: '{{replica_set_name}}', members: [{_id: 0, host: '{{self}}'}]})"
      add_command: "rs.add('{{new}}')"
      remove_command: "rs.remove('{{removed}}')"
      status_command: "rs.status()"
      health_check: "members.some(m => m.stateStr === 'PRIMARY')"
      connection_template: "mongodb://{{hosts}}/?replicaSet={{replica_set_name}}"
```

This manifest structure is generalizable. Adding support for PostgreSQL or Redis would mean adding entries to `target_offerings` with different commands.

---

## Edge Cases & Failure Modes

### Choreographer Starts After MongoDB Instances

No problem. The choreographer queries `rs.status()` on the first instance it finds. If the replica set already exists, it maps the topology and resumes monitoring. If it doesn't exist yet, it bootstraps.

### All MongoDB Instances Restart Simultaneously

MongoDB handles this. Each instance starts with `--replSet zen-garden`, discovers the existing replica set config from its data files, and rejoins. The choreographer detects the topology rebuilding and waits for a primary to be elected.

### Network Partition Between MongoDB Members

MongoDB's election protocol handles this. The side with majority quorum elects a primary. The minority side's members become SECONDARY (read-only) or enter `(not reachable/healthy)` state. The choreographer observes this via `rs.status()` and reports it on the Presence stream.

For a 3-member replica set: 2-member side gets a primary, 1-member side stays secondary. For a 2-member replica set: neither side gets a primary (no majority). This is why 3 members is the minimum recommended.

### Choreographer and MongoDB on Same Stone

Valid configuration. The choreographer is lightweight (HTTP proxy + periodic polls). It can share a Stone with a MongoDB instance without impact.

### Adopted MongoDB

An adopted MongoDB (native installation) works the same way, but command execution uses `mongosh` directly instead of `docker exec`. The choreographer detects the offering mode from the Tools API and adjusts its execution method.

### Mixed Managed and Adopted

If some MongoDB instances are managed (containers) and some are adopted (native), the choreographer handles both. Execution method varies per instance, but the replica set doesn't care — it's just `mongod` instances talking to each other.

---

## Generalization to Other Databases

The choreographer architecture is designed to support multiple databases:

### PostgreSQL

```yaml
orchestration:
  target_offerings:
    - name: postgresql
      cluster_type: streaming-replication
      init_command: |
        echo "wal_level = replica" >> $PGDATA/postgresql.conf
        echo "max_wal_senders = 10" >> $PGDATA/postgresql.conf
        pg_ctl reload
      add_command: |
        pg_basebackup -h {{primary_host}} -D $PGDATA -U replicator -Fp -Xs -P
        echo "primary_conninfo = 'host={{primary_host}} user=replicator'" >> $PGDATA/postgresql.auto.conf
        touch $PGDATA/standby.signal
      status_command: "SELECT * FROM pg_stat_replication;"
      connection_template: "postgresql://{{primary}}:5432"
```

### Redis Sentinel

```yaml
orchestration:
  target_offerings:
    - name: redis
      cluster_type: sentinel
      # Sentinel manages failover, choreographer manages sentinel topology
      init_command: "redis-cli CONFIG SET slaveof no one"
      add_command: "redis-cli -h {{new}} SLAVEOF {{primary_host}} {{primary_port}}"
      status_command: "redis-cli INFO replication"
      connection_template: "redis://{{primary}}:6379"
```

Each database has different replication semantics, but the choreographer pattern is the same: discover instances, bootstrap cluster, manage membership, monitor health, publish connection string.

---

## Implementation Phases

### Phase 1: Discovery & Bootstrap

**Effort:** ~1 week

- Subscribe to Tools API for MongoDB offerings
- Detect existing replica set state (`rs.status()`)
- Bootstrap single-node replica set on first instance
- Command execution via Moss API (`docker exec` path)

### Phase 2: Membership Management

**Effort:** ~1 week

- Add new members when instances appear
- Remove members when instances are intentionally lifted
- Handle member failure (detect, wait, optional removal after timeout)
- Handle member recovery (detect, verify sync)

### Phase 3: Connection String Publication

**Effort:** ~3-5 days

- Publish via Koi DNS
- Publish via wish resolution
- Publish via Tools API orchestration fields
- Update on membership changes

### Phase 4: Health Monitoring

**Effort:** ~3-5 days

- Periodic `rs.status()` polling
- Lag calculation and thresholds
- Presence stream events
- Integration with `garden-rake cluster status`

### Phase 5: CLI & Management API

**Effort:** ~3-5 days

- `garden-rake cluster status|stepdown|lag|connect` commands
- Management API endpoints
- Integration with `garden-rake observe`

### Phase 6: Policy Integration

**Effort:** ~2-3 days

- `garden-rake policy mongodb clustered` triggers choreographer deployment
- Disable ORCH-0001 default policy for this FQN
- DNS takeover

---

## References

- [ORCH-0001: Offering Orchestration](ORCH-0001-offering-orchestration.md)
- [MongoDB Replica Set Documentation](https://www.mongodb.com/docs/manual/replication/)
- [Koi Embedded Integration](../proposals/koi-embedded-integration.md)
- [Sub-Capabilities Proposal](../proposals/sub-capabilities.md)
- [Tools API Guide](../guides/tools-api-guide.md)
