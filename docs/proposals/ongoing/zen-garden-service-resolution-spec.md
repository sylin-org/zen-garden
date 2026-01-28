# Service Resolution Specification

**Status:** Proposal  
**Date:** January 2026  
**Authors:** Design session (Leon + Claude)  
**Supersedes:** Previous connection string and discovery specs

---

## Executive Summary

Zen Garden resolves **capabilities**, not locations. Applications ask for what they need. The environment declares context. The garden figures out the rest.

This specification defines how services are addressed, discovered, and resolved—establishing Zen Garden as a layer of meaningful indirection above traditional DNS/service addressing, not a replacement for it.

---

## Table of Contents

1. [Design Philosophy](#design-philosophy)
2. [Connection String Grammar](#connection-string-grammar)
3. [Resolution Semantics](#resolution-semantics)
4. [Admission Policy](#admission-policy)
5. [Environment-Informed Resolution](#environment-informed-resolution)
6. [Presence and Plurality](#presence-and-plurality)
7. [Federation, Process, and Consistency](#federation-process-and-consistency)
8. [Capabilities and Built-in Providers](#capabilities-and-built-in-providers)
9. [Resolution Algorithm](#resolution-algorithm)
10. [Manifest Extensions](#manifest-extensions)
11. [Graduation to Lantern](#graduation-to-lantern)
12. [Examples](#examples)
13. [Appendix A: Grammar Summary](#appendix-a-grammar-summary)
14. [Appendix B: Environment Variables](#appendix-b-environment-variables)
15. [Appendix C: Three Concerns Reference](#appendix-c-three-concerns-reference)
16. [Appendix D: Choreography Reference](#appendix-d-choreography-reference)
17. [Appendix E: Garden Process Metrics](#appendix-e-garden-process-metrics)
18. [Appendix F: Capability Providers](#appendix-f-capability-providers)

---

## Design Philosophy

### The Problem with Traditional Addressing

Traditional service addressing requires explicit knowledge:

```
mongodb://db3.internal:27017/analytics_prod
```

This encodes: protocol, hostname, port, and partition. The application must know where the service lives, manage connection strings per environment, and update configurations when infrastructure changes.

### The Zen Garden Approach

```
zen-garden:mongodb
```

This encodes: **intent**. The application declares what capability it needs. Everything else—location, instance selection, partition routing, failover—is resolved by the platform.

### Value Proposition

Zen Garden provides meaningful simplification when:

- You need **capability presence**, not instance management
- You want **environment-driven routing**, not configuration-driven
- You value **automatic resilience** over manual failover
- Your scale is **homelab/small team**, not hyperscale

When you need to distinguish between specific instances with specific configurations on specific machines, you've graduated to explicit addressing—and that's fine. The classic model is always underneath, available.

---

## Connection String Grammar

### Format

```
zen-garden:[<protocol>//]<offering>[:<instance>][/<partition>]
           ──────────    ────────  ──────────   ───────────
           wire format   service   logical ID   internal org
           (optional)    (required) (optional)  (optional)
```

### Components

| Component | Required | Description |
|-----------|----------|-------------|
| `protocol` | No | Wire protocol to use (e.g., `s3`, `mongodb`, `redis`). If omitted, uses offering's default protocol. |
| `offering` | Yes | The service requested (e.g., `mongodb`, `redis`, `minio`) |
| `instance` | No | Logical instance name, distinct from other instances of same offering |
| `partition` | No | Internal namespace within the instance (database, schema, bucket prefix) |

### Protocol vs Offering

**Protocol** is the wire format (how you talk to the service):  
**Offering** is the software (what you're talking to):

| Connection String | Protocol | Offering | Meaning |
|-------------------|----------|----------|---------|  
| `zen-garden:mongodb` | mongodb (implicit) | mongodb | MongoDB using native protocol |
| `zen-garden:s3//minio` | s3 | minio | MinIO using S3 protocol |
| `zen-garden:s3//` | s3 | (any s3 provider) | Any S3-compatible service |
| `zen-garden:storage//` | storage (agnostic) | (any storage) | Zen Garden agnostic storage API |

When protocol is omitted, the offering's default protocol is used.

### Examples

| Connection String | Meaning |
|-------------------|---------|
| `zen-garden:mongodb` | "I need mongodb (native protocol)" |
| `zen-garden:mongodb:analytics` | "I need the mongodb instance named 'analytics'" |
| `zen-garden:mongodb:analytics/events` | "I need the 'events' database within 'analytics'" |
| `zen-garden:mongodb/myapp` | "I need mongodb, partition 'myapp'" (unnamed instance) |
| `zen-garden:s3//minio` | "I need MinIO using S3 protocol" |
| `zen-garden:s3//minio:backup` | "I need the MinIO instance 'backup' via S3" |
| `zen-garden:s3//minio/myapp` | "I need MinIO with prefix 'myapp' via S3" |
| `zen-garden:s3//` | "I need any S3-compatible storage" |
| `zen-garden:storage//` | "I need storage (agnostic API)" |

### What Is NOT in the Grammar

- **Location**: No hostnames, IPs, or stone names
- **Port**: Derived from offering manifest
- **Credentials**: Managed separately (secrets layer)

The `@stone-name` pattern is explicitly rejected. Naming a location defeats the purpose of capability-based resolution. If you need a specific service on a specific machine, use direct addressing outside Zen Garden.

---

## Resolution Semantics

### Unnamed vs Named Instances

**Unnamed instance** (`zen-garden:mongodb`):
- Represents the **capability** itself
- Garden may have one or many physical deployments
- All unnamed deployments are logically **the same instance**
- Adding another deployment increases **presence**, not count

**Named instance** (`zen-garden:mongodb:analytics`):
- Represents a **distinct logical entity**
- Isolated from other instances of the same offering
- Has its own data, lifecycle, and identity

### Resolution Priority

When resolving `zen-garden:mongodb`:

1. Find all `mongodb` offerings with **communal** admission
2. Prefer **unnamed** over named (unnamed matches intent exactly)
3. If multiple unnamed exist, they're the same—return any healthy endpoint
4. If only named communal exist, return the first available

When resolving `zen-garden:mongodb:analytics`:

1. Find exactly `mongodb:analytics`
2. Admission policy doesn't matter (explicit request)
3. If multiple deployments exist, return any healthy endpoint

### Instance Identity

Instance names are **identities**, not locations. The name travels with the data:

```
mongodb:analytics on stone-01 (today)
mongodb:analytics on stone-03 (after ceremony)
```

Same instance. Same data. Different physical location. Applications never know the difference.

---

## Admission Policy

Each offering deployment declares an admission policy:

| Policy | Meaning |
|--------|---------|
| **Communal** | "I serve anonymous requests for this capability" |
| **Dedicated** | "I serve only those who ask for me by name" |

### Behavior

```
Garden state:
  mongodb          (communal)    ← serves "zen-garden:mongodb"
  mongodb:violet   (communal)    ← serves "zen-garden:mongodb" (secondary)
  mongodb:orchid   (dedicated)   ← serves only "zen-garden:mongodb:orchid"
```

Resolution of `zen-garden:mongodb`:
1. Returns `mongodb` (unnamed, communal, exact intent match)
2. Falls back to `mongodb:violet` if `mongodb` unhealthy

Resolution of `zen-garden:mongodb:violet`:
1. Returns only `mongodb:violet` (explicit request)

Resolution of `zen-garden:mongodb:orchid`:
1. Returns only `mongodb:orchid` (explicit request)

### Default Policy

- **Unnamed offerings**: Communal by default
- **Named offerings**: Dedicated by default (explicit naming implies intentional separation)

### CLI Commands

**Install with admission policy:**

```bash
# Communal (public) - default for unnamed
garden-rake offer mongodb:staging

# Dedicated (private) - explicit
garden-rake offer mongodb:staging privately

# Override default to communal
garden-rake offer mongodb:staging publicly
```

**Change admission policy:**

```bash
# Make an existing offering private
garden-rake offering mongodb:staging set-admission dedicated

# Make an existing offering public
garden-rake offering mongodb:staging set-admission communal
```

**Rename an offering instance:**

```bash
# Rename locally
garden-rake offering mongodb:old-name rename-to mongodb:new-name

# Rename on specific stone
garden-rake offering mongodb:old-name@stone-01 rename-to mongodb:new-name
```

---

## Environment-Informed Resolution

### The Problem

Applications often need to connect to different data based on context (dev/prod, tenant, feature branch) without code changes.

### The Solution

Environment variables inform resolution defaults:

```bash
# On dev machine
export ZG_PARTITION=dev

# On prod machine  
export ZG_PARTITION=prod
```

Application code (identical everywhere):

```python
db = connect("zen-garden:mongodb")
```

Resolution:
- Dev machine → `zen-garden:mongodb/dev`
- Prod machine → `zen-garden:mongodb/prod`

**Same code. Same image. Same garden. Different data.**

### Environment Variables

#### Global Defaults

| Variable | Effect |
|----------|--------|
| `ZG_PARTITION` | Default partition for all offerings |
| `ZG_INSTANCE` | Default instance for all offerings |

#### Per-Offering Overrides

| Variable | Effect |
|----------|--------|
| `ZG_<OFFERING>_PARTITION` | Default partition for specific offering |
| `ZG_<OFFERING>_INSTANCE` | Default instance for specific offering |

Offering name is uppercased: `ZG_MONGODB_PARTITION`

#### Precedence

```
1. Explicit in connection string    zen-garden:mongodb:analytics/prod
                                              ↓ (if not specified)
2. Per-offering environment         ZG_MONGODB_PARTITION=staging
                                              ↓ (if not specified)
3. Global environment               ZG_PARTITION=dev
                                              ↓ (if not specified)
4. No partition                     Service default behavior
```

### Use Cases

| Scenario | Configuration |
|----------|---------------|
| Dev/prod separation | `ZG_PARTITION=dev` vs `=prod` |
| Multi-tenant | `ZG_PARTITION=tenant_xyz` |
| Blue/green deployment | `ZG_INSTANCE=blue` vs `=green` |
| Feature branches | `ZG_PARTITION=feature_123` |
| Personal sandbox | `ZG_PARTITION=leon` |

---

## Presence and Plurality

### The Core Insight

Adding another deployment of an unnamed offering doesn't create a second instance. It increases the **presence** of that capability in the garden.

```bash
garden-rake offer mongodb          # mongodb is now in the garden (stone-01)
garden-rake offer mongodb          # mongodb presence extended (stone-02)
```

You don't have two databases. You have one database that exists in two places.

### Implications

| Action | Result |
|--------|--------|
| `offer mongodb` (first) | Capability appears in garden |
| `offer mongodb` (second) | Capability becomes more resilient |
| `offer mongodb:analytics` | Different capability (distinct instance) |
| Stone dies | Capability continues (if presence > 1) |

### Plurality Modes

How multiple deployments coordinate depends on the offering's federation capability:

| Offering declares | Multiple deployments behave as |
|-------------------|-------------------------------|
| `federation: replica-set` | Active replicas (load balanced, synchronized) |
| `federation: cluster` | Distributed cluster (sharded) |
| `federation: none` | Primary + helpers (garden-managed failover) |

---

## Federation, Process, and Consistency

### Three Orthogonal Concerns

Resilience is not one axis. It's three independent concerns that compose:

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│  FEDERATION                                                     │
│  "Who is part of this logical entity?"                          │
│                                                                 │
│    singleton   - only one active at a time                      │
│    pool        - independent instances, same capability         │
│    cluster     - coordinated instances, aware of each other     │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  PROCESS                                                        │
│  "How do we distribute work?"                                   │
│                                                                 │
│    direct      - single endpoint, no distribution               │
│    client      - client driver routes (multi-host string)       │
│    garden      - garden selects endpoint (performance, etc.)    │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  CONSISTENCY                                                    │
│  "How do we keep data consistent?"                              │
│                                                                 │
│    none        - stateless, nothing to sync                     │
│    lazy        - pull on demand, eventual consistency           │
│    replicated  - active sync between instances (service-managed)│
│    backup      - garden copies data periodically                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### How They Compose

| Offering | Federation | Process | Consistency | Result |
|----------|------------|---------|-------------|--------|
| MongoDB | cluster | client | replicated | Replica set, driver routes, service syncs |
| Ollama | pool | garden | lazy | Load-balanced, garden routes, models pull on demand |
| Legacy DB | singleton | direct | backup | One active, garden failover, backup/restore |
| Redis cache | pool | garden | none | Load-balanced, stateless |
| PostgreSQL | cluster | client | replicated | Streaming replication, driver routes |
| SQLite app | singleton | direct | backup | One active, garden failover |

Each concern is configured independently. The garden combines them.

---

### Federation

**Federation** defines the relationship between instances of the same offering.

#### singleton

Only one instance is active at a time. Others are dormant helpers.

```yaml
federation:
  mode: singleton
```

- First deployment: active
- Additional deployments: helpers (waiting)
- Active dies: helper promoted
- Active returns: becomes helper

Use for: Services that can't share load or coordinate.

#### pool

Instances are independent. Same capability, no coordination.

```yaml
federation:
  mode: pool
```

- All deployments are active
- No awareness of each other
- Any can serve any request
- Adding/removing has no coordination cost

Use for: Stateless services, services with lazy data sync.

#### cluster

Instances coordinate with each other. Aware of membership.

```yaml
federation:
  mode: cluster
  choreography:
    startup_args: ["--replSet", "zen-garden"]
    initiate:
      on: first_instance
      command: "mongosh --eval \"rs.initiate({...})\""
    add:
      on: new_instance
      target: primary
      command: "mongosh --eval \"rs.add('{{new}}')\""
    remove:
      on: instance_removed
      target: primary
      command: "mongosh --eval \"rs.remove('{{removed}}')\""
```

- Garden runs choreography at lifecycle events
- Service maintains internal membership
- Instances know about each other

Use for: Databases with replication, distributed systems.

---

### Process

**Process** defines how work is distributed across instances.

#### direct

Single endpoint. No distribution.

```yaml
process:
  mode: direct
```

- Client receives one host
- No load balancing
- Used with `federation: singleton`

Connection string: `mongodb://stone-01.local:27017`

#### client

Client driver handles routing.

```yaml
process:
  mode: client
  connection:
    template: "mongodb://{{hosts}}/?replicaSet=zen-garden"
```

- Client receives multi-host connection string
- Driver discovers topology, routes reads/writes
- Driver handles failover

Connection string: `mongodb://stone-01:27017,stone-02:27017,stone-03:27017/?replicaSet=zen-garden`

Use for: Services with smart client drivers (MongoDB, PostgreSQL, Redis Cluster).

#### garden

Garden selects the endpoint.

```yaml
process:
  mode: garden
  strategy: performance-weighted
  metrics:
    - name: tokens_per_second
      weight: 1.0
    - name: queue_depth
      weight: -0.5
    - name: time_to_first_token
      weight: -0.3
  model_aware: true  # optional: prefer instances with requested model
```

- Garden tracks performance metrics per instance
- Resolution returns best current endpoint
- Can factor in: performance, queue depth, affinity, locality

Connection string: `http://stone-02.local:11434` (best right now)

Use for: Stateless services, services without smart drivers, performance-sensitive routing.

##### Garden Process Strategies

| Strategy | Selection Logic |
|----------|-----------------|
| `round-robin` | Rotate through instances |
| `least-connections` | Prefer instances with fewer active connections |
| `performance-weighted` | Weight by metrics (throughput, latency, queue) |
| `affinity` | Prefer instances that have cached resources (models, data) |
| `locality` | Prefer instances on same subnet/rack |

---

### Consistency

**Consistency** defines how data stays synchronized.

#### none

Stateless. Nothing to sync.

```yaml
consistency:
  mode: none
```

- No data persists between requests
- Instances are interchangeable
- No sync mechanism needed

Use for: Caches, proxies, stateless APIs.

#### lazy

Pull on demand. Eventual consistency.

```yaml
consistency:
  mode: lazy
```

- Data fetched when needed
- Instances may have different data at any moment
- Eventually converge as requests come in

Use for: Ollama (models pull on demand), CDN-like patterns.

#### replicated

Active sync between instances. Service-managed.

```yaml
consistency:
  mode: replicated
  # Service handles replication internally
  # Garden just runs choreography to set it up
```

- Service maintains consistency
- Garden orchestrates membership via choreography
- Strong or eventual consistency (service decides)

Use for: Databases with built-in replication.

#### backup

Garden copies data periodically.

```yaml
consistency:
  mode: backup
  command: "/app/backup.sh"
  restore: "/app/restore.sh"
  interval: 15m
  retention: 7d
  quiesce: false
```

- Garden runs backup command periodically
- Helpers receive backup copies
- On failover: restore then promote
- RPO = backup interval

Use for: Services that can't replicate but need resilience.

---

### Combining the Concerns

#### Example: MongoDB

```yaml
name: mongodb

federation:
  mode: cluster
  choreography:
    startup_args: ["--replSet", "zen-garden"]
    initiate:
      on: first_instance
      command: "mongosh --eval \"rs.initiate({_id:'zen-garden', members:[{_id:0, host:'{{self}}'}]})\""
    add:
      on: new_instance
      target: primary
      command: "mongosh --eval \"rs.add('{{new}}')\""
    remove:
      on: instance_removed
      target: primary
      command: "mongosh --eval \"rs.remove('{{removed}}')\""

process:
  mode: client
  connection:
    template: "mongodb://{{hosts}}/?replicaSet=zen-garden"

consistency:
  mode: replicated
```

Result: Instances form a replica set. Client gets multi-host string. Driver routes. Service replicates.

#### Example: Ollama

```yaml
name: ollama

federation:
  mode: pool

process:
  mode: garden
  strategy: performance-weighted
  metrics:
    - name: tokens_per_second
      weight: 1.0
    - name: queue_depth
      weight: -0.5
  model_aware: true

consistency:
  mode: lazy
```

Result: Instances are independent. Garden routes to best performer. Models pull on demand.

#### Example: Legacy Database

```yaml
name: legacy-db

federation:
  mode: singleton

process:
  mode: direct

consistency:
  mode: backup
  command: "pg_dump -Fc > /backup/dump.pgc"
  restore: "pg_restore /backup/dump.pgc"
  interval: 15m
```

Result: One active instance. Direct connection. Garden backs up to helpers. Failover restores from backup.

#### Example: Redis Cache

```yaml
name: redis-cache

federation:
  mode: pool

process:
  mode: garden
  strategy: least-connections

consistency:
  mode: none
```

Result: Independent instances. Garden balances load. Stateless (cache misses are fine).

---

### The Sidecar Escape Hatch

When the three concerns can't be expressed through configuration:

```yaml
federation:
  mode: cluster
  sidecar:
    image: zen-garden/complex-thing-coordinator
    tag: "1.0"

process:
  mode: sidecar
  sidecar:
    port: 7200

consistency:
  mode: sidecar
```

The sidecar handles what configuration can't express. This should be rare.

---

## Capabilities and Built-in Providers

### The Capability Ladder

Some capabilities can be provided by multiple sources: dedicated offerings OR built-in Moss functionality. When an app requests a capability, the garden resolves to the best available provider.

```
┌─────────────────────────────────────────────────────────────────┐
│  App: connect("zen-garden:s3//myapp")                           │
│                                                                 │
│  Resolution:                                                    │
│                                                                 │
│    1. Is there an offering that provides 's3'? (MinIO, etc.)    │
│       → YES: Resolve to offering (uses three-concern model)     │
│                                                                 │
│    2. Does any stone have built-in s3 capability?               │
│       → YES: Resolve to Moss gateway endpoint                   │
│                                                                 │
│    3. Neither?                                                  │
│       → Error: no s3 capability in garden                       │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

Offerings take precedence because they're more capable. Built-ins are fallbacks that ensure basic functionality always exists.

### Capability vs Offering

| Connection String | Meaning |
|-------------------|---------|
| `zen-garden:s3//myapp` | "I need S3 capability" (best available) |
| `zen-garden:minio` | "I need MinIO specifically" |
| `zen-garden:minio:production` | "I need the MinIO instance named 'production'" |

`s3` is a **capability**. `minio` is an **offering** that provides that capability.

This mirrors the database pattern:
| Connection String | Meaning |
|-------------------|---------|
| `zen-garden:database` | "I need a database" (agnostic) |
| `zen-garden:mongodb` | "I need MongoDB specifically" |

### Built-in S3: Infrastructure vs Gateways

Moss provides built-in S3 capability backed by seed bank infrastructure. The key insight: **storage is singular, but access is distributed**.

```
┌─────────────────────────────────────────────────────────────────┐
│  STORAGE (Infrastructure)                                       │
│                                                                 │
│    USB drive, NAS, local disk                                   │
│    Physically SINGULAR - one location for the data              │
│    Configured in moss.toml as seed_banks                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ "I can reach it"
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  GATEWAYS (Moss S3 endpoints)                                   │
│                                                                 │
│    Any stone that can reach storage announces s3 capability     │
│    Multiple gateways possible (NAS reachable from N stones)     │
│    Gateways are STATELESS - just proxy to storage               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ "Route me to s3"
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  RESOLUTION                                                     │
│                                                                 │
│    App asks for zen-garden:s3//myapp                            │
│    Garden finds stones with s3 capability                       │
│    Garden routes to any healthy gateway                         │
│    Gateway proxies to underlying storage                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Gateway Scenarios

**Scenario 1: USB in stone-01**

```
stone-01: USB mounted at /mnt/usb
          announces: capability=s3, access=direct
          
stone-02: no direct storage access
          
stone-03: no direct storage access

App on stone-03: zen-garden:s3//myapp
  → Garden: stone-01 has s3 capability
  → Return: http://stone-01.local:7180/api/v1/storage
```

Single gateway. Apps anywhere in garden can reach it.

**Scenario 2: NAS reachable from multiple stones**

```
stone-01: NAS mounted at /mnt/nas
          announces: capability=s3, access=direct
          
stone-02: NAS mounted at /mnt/nas
          announces: capability=s3, access=direct
          
stone-03: cannot reach NAS

App on stone-03: zen-garden:s3//myapp
  → Garden: stone-01 and stone-02 have s3
  → Select best: stone-02 (less loaded)
  → Return: http://stone-02.local:7180/api/v1/storage
```

Multiple gateways to same storage. Garden load-balances.

**Scenario 3: Proxy gateway**

```
stone-01: NAS mounted, announces s3 (direct)
stone-02: NAS mounted, announces s3 (direct)
stone-03: no NAS access, but can reach stone-02
          announces: capability=s3, access=proxy, via=stone-02

App on stone-03: zen-garden:s3//myapp
  → Garden: stone-01, stone-02, stone-03 have s3
  → Prefer direct over proxy
  → Return: http://stone-02.local:7180/api/v1/storage
  
  OR if stone-01 and stone-02 are overloaded:
  → Return: http://stone-03.local:7180/api/v1/storage
  → stone-03 forwards to stone-02 transparently
```

Proxy gateways extend reach. Direct access preferred but proxies available.

### Built-in S3: Three Concerns

From the **app's perspective**, built-in S3 gateways behave as:

| Concern | Value | Meaning |
|---------|-------|---------|
| Federation | `pool` | Multiple gateways, independent, any can serve |
| Process | `garden` | Garden selects best gateway |
| Consistency | `none` | Gateways are stateless proxies |

The underlying storage being singular is an **infrastructure detail**, not the app's concern.

### Gateway Capability Announcement

Stones announce S3 capability via mDNS:

```
stone-01 (USB mounted):
  TXT: capability=s3
       storage_access=direct
       storage_id=seed-usb-backup
       
stone-02 (NAS mounted):
  TXT: capability=s3
       storage_access=direct
       storage_id=seed-nas-main

stone-03 (proxy):
  TXT: capability=s3
       storage_access=proxy
       storage_id=seed-nas-main
       proxy_via=stone-02
```

### Gateway Selection Logic

```
1. Find all gateways announcing s3 capability
2. Filter by storage_id if specified in connection string
3. Prefer direct access over proxy
4. Select by: health > load > latency
5. Return endpoint
```

### MinIO vs Built-in

When MinIO is deployed, it supersedes built-in gateways:

| Phase | Provider | Capabilities |
|-------|----------|--------------|
| Day 1: Fresh garden | Moss built-in | Basic S3, single storage, gateway pool |
| Day 2: MinIO deployed | MinIO offering | Full S3, clusterable, erasure-coded |
| Day 3: MinIO scaled | MinIO cluster | Distributed, replicated, highly available |

**Same connection string throughout**: `zen-garden:s3//myapp`

The app doesn't change. The capability grows.

### MinIO Manifest (provides s3)

```yaml
# minio.manifest.yaml
name: minio
category: storage
tags: [s3, object-storage]
provides: [s3]  # ← declares capability

federation:
  mode: cluster
  choreography:
    initiate:
      on: first_instance
      command: "minio server /data --console-address :9001"
    add:
      on: new_instance
      # MinIO cluster expansion commands

process:
  mode: client
  connection:
    template: "http://{{hosts}}:9000"

consistency:
  mode: replicated
```

The `provides: [s3]` declares that this offering satisfies the S3 capability request.

### Resolution Priority

When resolving `zen-garden:s3//myapp`:

```
1. Offerings with provides: [s3]
   → Ranked by: health > communal admission > capability
   → Uses offering's three-concern configuration
   
2. Moss built-in gateways (if seed bank configured)
   → Fallback when no s3 offering exists
   → Uses gateway pool model
   
3. Neither available
   → Error: "No s3 capability in garden"
```

### Seed Bank Configuration (Infrastructure)

Seed banks remain **infrastructure**, configured per-stone:

```toml
# moss.toml
[cultivation]
enabled = true
schedule = "0 3 * * *"

[[cultivation.seed_banks]]
type = "path"
path = "/mnt/usb-backup"
announce_s3 = true  # Expose as s3 gateway

[[cultivation.seed_banks]]
type = "network"
protocol = "smb"
host = "nas.local"
share = "zen-garden"
announce_s3 = true
```

Seed banks serve dual purposes:
1. **Cultivation**: Where backups are written
2. **S3 Gateway**: App storage (if `announce_s3 = true`)

---

## Resolution Algorithm

### Client SDK Flow

```
Input: connection_string = "zen-garden:mongodb"
       environment = { ZG_PARTITION: "dev" }

1. PARSE connection string
   → offering = "mongodb"
   → instance = None
   → partition = None

2. APPLY environment defaults
   → partition = "dev" (from ZG_PARTITION)

3. DISCOVER offerings
   → Query mDNS: _koan-stone._tcp.local.
   → Filter: offering = "mongodb"
   → Filter: admission = "communal" (since instance is None)

4. SELECT instance
   → Prefer unnamed (exact intent match)
   → Rank by: health > priority > latency

5. BUILD connection string based on process mode
   → direct: Single-host string (one active instance)
   → client: Multi-host string from template (client driver routes)
   → garden: Best-performing host (garden selects)

Output varies by process mode:
  direct: "mongodb://stone-01.local:27017/dev"
  client: "mongodb://stone-01:27017,stone-02:27017,stone-03:27017/dev?replicaSet=zen-garden"
  garden: "http://stone-02.local:11434" (best performer right now)
```

### Connection String by Process Mode

| Process Mode | Connection String Form | Routing Handled By |
|--------------|------------------------|-------------------|
| `direct` | `protocol://single-host:port` | Garden (failover via singleton promotion) |
| `client` | `protocol://host1,host2,host3?params` | Client driver |
| `garden` | `protocol://best-host:port` | Garden (performance-weighted selection) |

### How Three Concerns Affect Resolution

| Concern | Affects |
|---------|---------|
| **Federation** | Which instances are candidates (active vs helper) |
| **Process** | Connection string format and who routes |
| **Consistency** | Nothing directly (it's about data, not routing) |

### mDNS TXT Records

Extended to include instance, admission, and protocols:

| Field | Example | Description |
|-------|---------|-------------|
| `offering` | `mongodb` | Offering name |
| `instance` | `analytics` | Instance name (empty if unnamed) |
| `admission` | `communal` | Admission policy: `communal` or `dedicated` |
| `protocols` | `mongodb,storage` | Comma-separated list of supported protocols |
| `protocol_default` | `mongodb` | Default protocol for this offering |
| `federation_role` | `primary` | Role in federation (primary/secondary/helper) |
| `health` | `healthy` | Health status: `healthy`, `degraded`, `offline` |
| `priority` | `50` | Selection priority (0-100) |
| `port` | `27017` | Primary service port |
| `version` | `7.0.4` | Service version |

### Resolution API

Moss exposes a resolution endpoint for clients that prefer HTTP over mDNS:

```http
GET /api/v1/resolve?offering=mongodb&instance=analytics&protocol=mongodb

Response 200:
{
  "offering": "mongodb",
  "instance": "analytics",
  "protocol": "mongodb",
  "endpoint": "mongodb://stone-01.local:27017",
  "federation_role": "primary",
  "health": "healthy"
}

Response 404:
{
  "error": "OFFERING_NOT_FOUND",
  "message": "No mongodb offering found in garden",
  "hint": "Run 'garden-rake find mongodb' to check availability"
}
```

**Query Parameters:**

| Parameter | Required | Description |
|-----------|----------|-------------|
| `offering` | Yes | Offering name (e.g., `mongodb`) |
| `instance` | No | Instance name (defaults to unnamed) |
| `protocol` | No | Desired protocol (defaults to offering's default) |
| `partition` | No | Partition/database name |

### Ambiguity Handling

When resolution would be ambiguous:

```
Garden has:
  mongodb:dev    (communal)
  mongodb:prod   (communal)
  
Request: zen-garden:mongodb (no unnamed exists)
```

**Soft error with guidance:**

> Multiple mongodb instances exist (dev, prod) but no unnamed default. Please specify: `zen-garden:mongodb:dev` or `zen-garden:mongodb:prod`

Alternatively, operator can mark one as primary:

```bash
garden-rake offering mongodb:dev --primary
```

---

## Manifest Extensions

### Complete Example: MongoDB

```yaml
# offerings/mongodb.manifest.yaml
name: mongodb
category: data
tags: [database, document, nosql]
protocols:
  - name: mongodb
    port: 27017
    default: true
  - name: storage
    port: 8080
    sidecar: true

admission:
  default: communal
  allow_override: true

federation:
  mode: cluster
  choreography:
    startup_args: ["--replSet", "zen-garden"]
    initiate:
      on: first_instance
      command: >
        mongosh --eval "rs.initiate({
          _id: 'zen-garden',
          members: [{ _id: 0, host: '{{self}}' }]
        })"
    add:
      on: new_instance
      target: primary
      command: "mongosh --eval \"rs.add('{{new}}')\""
    remove:
      on: instance_removed
      target: primary
      command: "mongosh --eval \"rs.remove('{{removed}}')\""
    status:
      command: "mongosh --eval \"rs.status()\""
      healthy_when: "members.some(m => m.stateStr === 'PRIMARY')"

process:
  mode: client
  connection:
    template: "mongodb://{{hosts}}/?replicaSet=zen-garden"

consistency:
  mode: replicated
```

### Complete Example: Ollama

```yaml
# offerings/ollama.manifest.yaml
name: ollama
category: ai
tags: [inference, llm, chat, embeddings, vision]
protocols:
  - name: http
    port: 11434
    default: true

admission:
  default: communal

federation:
  mode: pool

process:
  mode: garden
  strategy: performance-weighted
  metrics:
    - name: tokens_per_second
      source: "/api/stats"
      weight: 1.0
    - name: queue_depth
      source: "/api/stats"
      weight: -0.5
    - name: time_to_first_token
      source: "/api/stats"
      weight: -0.3
    - name: available_vram
      source: "/api/stats"
      weight: 0.3
  model_aware: true
  connection:
    template: "http://{{best_host}}:11434"

consistency:
  mode: lazy
```

### Complete Example: Legacy Database

```yaml
# offerings/legacy-db.manifest.yaml
name: legacy-db
category: data
tags: [database, sql]
protocols:
  - name: postgresql
    port: 5432
    default: true

admission:
  default: communal

federation:
  mode: singleton

process:
  mode: direct
  connection:
    template: "postgresql://{{host}}:5432"

consistency:
  mode: backup
  command: "pg_dump -Fc > /backup/dump.pgc"
  restore: "pg_restore -d $DATABASE /backup/dump.pgc"
  interval: 15m
  retention: 7d
  quiesce: false
```

### Complete Example: Redis Cache

```yaml
# offerings/redis-cache.manifest.yaml
name: redis-cache
category: cache
tags: [cache, key-value]
protocols:
  - name: redis
    port: 6379
    default: true

admission:
  default: communal

federation:
  mode: pool

process:
  mode: garden
  strategy: least-connections
  connection:
    template: "redis://{{best_host}}:6379"

consistency:
  mode: none
```

### Complete Example: PostgreSQL (Replicated)

```yaml
# offerings/postgresql.manifest.yaml
name: postgresql
category: data
tags: [database, sql, relational]
protocols:
  - name: postgresql
    port: 5432
    default: true
  - name: storage
    port: 8080
    sidecar: true

admission:
  default: communal

federation:
  mode: cluster
  choreography:
    initiate:
      on: first_instance
      commands:
        - "echo 'wal_level = replica' >> $PGDATA/postgresql.conf"
        - "echo 'max_wal_senders = 10' >> $PGDATA/postgresql.conf"
        - "pg_ctl reload"
    add:
      on: new_instance
      target: self
      commands:
        - "pg_basebackup -h {{primary_host}} -D $PGDATA -U replicator -Fp -Xs -P"
        - "echo \"primary_conninfo = 'host={{primary_host}} user=replicator'\" >> $PGDATA/postgresql.auto.conf"
        - "touch $PGDATA/standby.signal"

process:
  mode: client
  connection:
    template: "postgresql://{{hosts}}:5432"
    primary: "postgresql://{{primary}}:5432"
    read_replicas: "postgresql://{{secondaries}}:5432"

consistency:
  mode: replicated
```

### Complete Example: Sidecar (Escape Hatch)

```yaml
# offerings/complex-thing.manifest.yaml
name: complex-thing
category: application
protocols:
  - name: http
    port: 7200
    default: true

admission:
  default: dedicated

federation:
  mode: cluster
  sidecar:
    image: zen-garden/complex-thing-coordinator
    tag: "1.0"

process:
  mode: sidecar
  sidecar:
    image: zen-garden/complex-thing-proxy
    tag: "1.0"
    port: 7200

consistency:
  mode: sidecar
  sidecar:
    image: zen-garden/complex-thing-sync
    tag: "1.0"
```

### Admission Configuration

```yaml
# Any offering can include admission policy
admission:
  default: communal              # communal | dedicated
  allow_override: true           # Operator can change at deploy time
```

---

## Graduation to Lantern

### When Garden-Only Works

- Single subnet (mDNS reaches all stones)
- 2-5 stones
- Simple topology
- Rare network partitions

### When Lantern Is Needed

- Multiple subnets (mDNS doesn't cross routers)
- Many stones (discovery overhead)
- Complex topology
- Network partitions possible

### Lantern's Role

```
┌─────────────────────────────────────────────────────────────────┐
│  Without Lantern                                                │
│                                                                 │
│    stone-01 ◄──mDNS──► stone-02 ◄──mDNS──► stone-03            │
│                                                                 │
│  Network partition:                                             │
│    [stone-01, stone-02]  │  [stone-03]                         │
│    Both sides think they're the whole garden                    │
│    Split brain possible                                         │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│  With Lantern                                                   │
│                                                                 │
│                    ┌──────────┐                                 │
│                    │ Lantern  │                                 │
│                    │ (arbiter)│                                 │
│                    └────┬─────┘                                 │
│           ┌─────────────┼─────────────┐                         │
│           ▼             ▼             ▼                         │
│       stone-01      stone-02      stone-03                      │
│                                                                 │
│  Network partition:                                             │
│    Side with Lantern connectivity = authoritative               │
│    Side without = degraded (read-only or offline)               │
│    No split brain                                               │
└─────────────────────────────────────────────────────────────────┘
```

Lantern provides:
- Persistent topology map
- Authoritative service registry
- Partition arbitration
- Cross-subnet discovery

---

## Examples

### Example 1: Simple Single-Instance

```bash
# Operator
garden-rake offer mongodb

# App (any machine in garden)
db = connect("zen-garden:mongodb")
# → mongodb://stone-01.local:27017
```

### Example 2: Dev/Prod Separation

```bash
# Operator
garden-rake offer mongodb:dev
garden-rake offer mongodb:prod

# Dev machine (.env)
ZEN_GARDEN_INSTANCE=dev

# Prod machine (.env)
ZEN_GARDEN_INSTANCE=prod

# App code (identical)
db = connect("zen-garden:mongodb")
# Dev  → mongodb://stone-01.local:27017 (dev instance)
# Prod → mongodb://stone-02.local:27017 (prod instance)
```

### Example 3: Cluster + Client + Replicated (MongoDB)

```bash
# Operator
garden-rake offer mongodb
garden-rake offer mongodb   # On another stone

# Garden:
#   1. Sees federation: cluster
#   2. Runs choreography: rs.initiate() on first, rs.add() for second
#   3. Process: client → builds multi-host connection string
#   4. Consistency: replicated → service handles sync

# App
db = connect("zen-garden:mongodb")
# → mongodb://stone-01.local:27017,stone-02.local:27017/?replicaSet=zen-garden
# Client driver handles routing reads/writes and failover

# stone-01 dies
# MongoDB elects stone-02 as primary (service handles it)
# Client driver reconnects automatically (driver handles it)
# Consistency maintained by service replication
```

### Example 4: Singleton + Direct + Backup (Legacy DB)

```bash
# Operator
garden-rake offer legacy-db
garden-rake offer legacy-db   # On another stone

# Garden:
#   1. Sees federation: singleton → only one active
#   2. Process: direct → single-host connection
#   3. Consistency: backup → garden copies data every 15m

# Garden state:
#   legacy-db on stone-01 (active)
#   legacy-db on stone-02 (helper, receiving backups)

# App
db = connect("zen-garden:legacy-db")
# → legacy-db://stone-01.local:5432

# stone-01 dies
# Garden:
#   1. Detects stone-01 unreachable
#   2. Restores latest backup on stone-02 (consistency: backup)
#   3. Promotes stone-02 to active (federation: singleton)
#   4. Updates discovery

# App reconnects
# → legacy-db://stone-02.local:5432
```

### Example 5: Pool + Garden + Lazy (Ollama)

```bash
# Operator
garden-rake offer ollama            # RTX 4090
garden-rake offer ollama            # RTX 3060
garden-rake offer ollama            # CPU only

# Garden:
#   1. Sees federation: pool → all instances independent
#   2. Process: garden → garden routes based on metrics
#   3. Consistency: lazy → models pull on demand

# Garden tracks performance ledger:
#   stone-01: 85 tok/s, queue=2, vram=18GB
#   stone-02: 32 tok/s, queue=0, vram=8GB
#   stone-03:  4 tok/s, queue=0, vram=0

# App
ai = connect("zen-garden:ollama")
# → http://stone-01.local:11434 (best performer with capacity)

# Next request, stone-01 busy:
# → http://stone-02.local:11434 (garden re-evaluates)

# App requests model "codellama":
# → Garden checks who has it cached
# → Routes to stone with model (avoids pull delay)

# stone-01 goes offline (gaming time)
# → Removed from ledger
# → Traffic shifts to stone-02, stone-03
# → No data loss (consistency: lazy, nothing to lose)
```

### Example 6: Pool + Garden + None (Redis Cache)

```bash
# Operator
garden-rake offer redis-cache
garden-rake offer redis-cache

# Garden:
#   1. Sees federation: pool → independent instances
#   2. Process: garden → least-connections routing
#   3. Consistency: none → stateless cache

# App
cache = connect("zen-garden:redis-cache")
# → redis://stone-02.local:6379 (fewer connections right now)

# Cache miss? Fine. It's a cache.
# Instance dies? Fine. Other instances serve. Data refills naturally.
```

### Example 7: S3 Capability Ladder

```bash
# Day 1: Fresh garden, USB backup drive

garden-rake tend /mnt/usb as seed-bank

# stone-01 now announces: capability=s3, access=direct
# Built-in S3 gateway available

# App
storage = connect("zen-garden:s3//myapp")
# → http://stone-01.local:7180/api/v1/storage
# Basic S3, single gateway, works.

# Day 2: NAS added, reachable from stone-01 and stone-02

garden-rake tend nas.local:/zen-garden as seed-bank

# stone-01 announces: capability=s3, access=direct, storage=nas
# stone-02 announces: capability=s3, access=direct, storage=nas
# Two gateways to same storage

# App (same code)
storage = connect("zen-garden:s3//myapp")
# → http://stone-02.local:7180/api/v1/storage (less loaded)
# Still built-in, but now load-balanced across gateways

# Day 3: MinIO deployed for production storage

garden-rake offer minio

# MinIO provides: [s3]
# Takes precedence over built-in

# App (same code)
storage = connect("zen-garden:s3//myapp")
# → http://stone-01.local:9000 (MinIO endpoint)
# Real S3 implementation, full features

# Day 4: MinIO scaled

garden-rake offer minio  # on stone-02
garden-rake offer minio  # on stone-03

# MinIO cluster: erasure-coded, distributed

# App (same code)
storage = connect("zen-garden:s3//myapp")
# → http://stone-01:9000,stone-02:9000,stone-03:9000
# Client-mode routing to MinIO cluster

# Throughout: same connection string
# Capability grew. App didn't change.
```

### Example 8: Direct Offering vs Capability Request

```bash
# These are different:

# Capability request (abstract)
connect("zen-garden:s3//myapp")
# → Best available S3 provider (MinIO if present, else built-in)

# Offering request (specific)
connect("zen-garden:minio")
# → MinIO specifically, error if not present

# Named instance
connect("zen-garden:minio:production")
# → The MinIO instance named "production"
```

### Example 5: Mixed Admission

```bash
# Operator
garden-rake offer mongodb                        # Unnamed, communal (default)
garden-rake offer mongodb:analytics --admission communal
garden-rake offer mongodb:secrets --admission dedicated

# Anonymous request
connect("zen-garden:mongodb")
# → Returns unnamed mongodb (first choice)
# → Falls back to mongodb:analytics (also communal)
# → Never returns mongodb:secrets (dedicated)

# Explicit request
connect("zen-garden:mongodb:secrets")
# → Returns mongodb:secrets (explicit naming works regardless of admission)
```

### Example 6: Environment Layering

```bash
# Global default
export ZG_PARTITION=dev

# Override for specific offering
export ZG_MONGODB_PARTITION=analytics_dev

# App code
connect("zen-garden:mongodb")        # → mongodb://.../analytics_dev
connect("zen-garden:redis")          # → redis://.../dev
connect("zen-garden:mongodb/prod")   # → mongodb://.../prod (explicit wins)
```

---

## Summary

### The Model

1. **Connection strings express intent**, not location
2. **Unnamed offerings are capabilities**; adding presence increases resilience
3. **Named instances are distinct entities** with separate data and lifecycle
4. **Admission policy controls** who can use an offering anonymously
5. **Environment informs resolution** for dev/prod/tenant separation
6. **Three orthogonal concerns compose**: federation (who), process (how work flows), consistency (how data syncs)
7. **Protocol vs Offering**: `s3` is a protocol (wire format), `minio` is an offering (software)
8. **Protocol-first requests** (`zen-garden:s3//`) resolve to any compatible provider
9. **Configuration over sidecars**—express behavior declaratively when possible
10. **Lantern graduates the model** to complex topologies

### The Three Concerns

| Concern | Question | Modes |
|---------|----------|-------|
| **Federation** | Who is part of this? | singleton, pool, cluster |
| **Process** | How is work distributed? | direct, client, garden |
| **Consistency** | How is data synchronized? | none, lazy, replicated, backup |

### Request Types

| Request Type | Example | Resolves To |
|--------------|---------|-------------|
| Protocol-any | `zen-garden:s3//` | Any offering supporting S3 protocol |
| Protocol-specific | `zen-garden:s3//minio` | MinIO using S3 protocol |
| Offering | `zen-garden:minio` | MinIO using its default protocol |
| Instance | `zen-garden:minio:prod` | Named instance of MinIO |

### The Promise

> Ask for what you need. The garden figures out the rest.

---

## Appendix A: Grammar Summary

```
zen-garden:[<protocol>//]<offering>[:<instance>][/<partition>]

protocol   = [a-z][a-z0-9]*            # s3, mongodb, redis, storage, http
offering   = [a-z][a-z0-9-]*           # mongodb, redis, minio
instance   = [a-z][a-z0-9-]*           # analytics, dev, prod
partition  = [a-zA-Z][a-zA-Z0-9_-]*    # mydb, events, tenant_123
```

### Request Types

| Pattern | Type | Example |
|---------|------|---------|
| `zen-garden:<protocol>//<offering>` | Protocol-specific | `zen-garden:s3//minio` |
| `zen-garden:<protocol>//` | Protocol-any-provider | `zen-garden:s3//` |
| `zen-garden:<offering>` | Offering (default protocol) | `zen-garden:mongodb` |
| `zen-garden:<offering>:<instance>` | Instance request | `zen-garden:mongodb:analytics` |
| `zen-garden:<offering>/<partition>` | Partitioned request | `zen-garden:mongodb/mydb` |
| `zen-garden:<offering>:<instance>/<partition>` | Full path | `zen-garden:mongodb:analytics/events` |

### Protocol vs Offering

**Protocol** specifies the wire format (how you communicate):  
**Offering** specifies the software (what you're talking to):

| Connection String | Protocol | Offering | Meaning |
|-------------------|----------|----------|---------|  
| `zen-garden:mongodb` | mongodb (implicit) | mongodb | MongoDB using native protocol |
| `zen-garden:s3//minio` | s3 | minio | MinIO using S3 protocol |
| `zen-garden:s3//` | s3 | (any) | Any S3-compatible service |
| `zen-garden:storage//minio` | storage | minio | MinIO using agnostic storage API |

## Appendix B: Environment Variables

```bash
# Global
ZG_PARTITION=<partition>
ZG_INSTANCE=<instance>

# Per-offering (offering name uppercased)
ZG_<OFFERING>_PARTITION=<partition>
ZG_<OFFERING>_INSTANCE=<instance>

# Examples
ZG_PARTITION=dev
ZG_MONGODB_INSTANCE=analytics
ZG_REDIS_PARTITION=cache_dev
```

### Path and Configuration Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ZG_DATA_DIR` | `/var/lib/zen-garden` (Linux), `.zen-garden` (Windows) | Data directory |
| `ZG_CONFIG_DIR` | `/etc/zen-garden` (Linux), `.zen-garden` (Windows) | Config directory |
| `ZG_STONE_NAME` | auto-generated | Stone name |
| `ZG_STONE` | (discovery) | Skip discovery, use this endpoint |
| `ZG_QUIET` | (unset) | Suppress suggestions |

## Appendix C: Three Concerns Reference

### Federation Modes

| Mode | Instances Active | Coordination | Use Case |
|------|------------------|--------------|----------|
| `singleton` | One | None | Services that can't share load |
| `pool` | All | None | Stateless services, lazy-sync |
| `cluster` | All | Choreography | Databases, distributed systems |

### Process Modes

| Mode | Connection String | Routing By | Use Case |
|------|-------------------|------------|----------|
| `direct` | Single host | N/A | Singleton federation |
| `client` | Multi-host | Client driver | Smart drivers (MongoDB, PostgreSQL) |
| `garden` | Best host | Garden (metrics) | Stateless, performance-sensitive |

### Consistency Modes

| Mode | Sync Mechanism | RPO | Use Case |
|------|----------------|-----|----------|
| `none` | None | N/A | Stateless services |
| `lazy` | Pull on demand | Eventual | Ollama (models), CDN patterns |
| `replicated` | Service-managed | Zero | Databases with replication |
| `backup` | Garden-managed | Backup interval | Legacy services |

### Common Combinations

| Pattern | Federation | Process | Consistency |
|---------|------------|---------|-------------|
| Replicated database | cluster | client | replicated |
| Load-balanced AI | pool | garden | lazy |
| Single with backup | singleton | direct | backup |
| Stateless cache | pool | garden | none |
| Stateful singleton | singleton | direct | backup |

## Appendix D: Choreography Reference

### Template Variables

| Variable | Scope | Description |
|----------|-------|-------------|
| `{{self}}` | Current instance | hostname:port of this instance |
| `{{self_host}}` | Current instance | hostname only |
| `{{self_port}}` | Current instance | port only |
| `{{new}}` | add event | hostname:port of newly added instance |
| `{{removed}}` | remove event | hostname:port of removed instance |
| `{{primary}}` | Any | hostname:port of current primary |
| `{{primary_host}}` | Any | hostname of current primary |
| `{{primary_port}}` | Any | port of current primary |
| `{{secondaries}}` | Any | comma-separated hostname:port of secondaries |
| `{{hosts}}` | Any | comma-separated hostname:port of all instances |
| `{{best_host}}` | garden process | hostname of best-performing instance |
| `{{instance_id}}` | Any | numeric ID for this instance (0, 1, 2...) |

### Lifecycle Events

| Event | When Fired | Typical Use |
|-------|------------|-------------|
| `first_instance` | First deployment of offering | Initialize cluster |
| `new_instance` | Additional deployment starts | Add to cluster |
| `instance_removed` | Deployment intentionally removed | Remove from cluster |
| `instance_failed` | Deployment unreachable | Usually no-op (service handles) |
| `instance_recovered` | Failed deployment returns | Re-add if necessary |
| `primary_failed` | Primary instance unreachable | Trigger election/promotion |

### Command Execution

Commands run inside the container via `docker exec` or equivalent:

```yaml
# Single command
command: "mongosh --eval \"rs.add('{{new}}')\""

# Multiple commands
commands:
  - "first command"
  - "second command"
  - "third command"

# Multi-line command
command: >
  mongosh --eval "rs.initiate({
    _id: 'zen-garden',
    members: [{ _id: 0, host: '{{self}}' }]
  })"
```

## Appendix E: Garden Process Metrics

When `process.mode: garden`, the garden can track these metrics:

| Metric | Source | Use |
|--------|--------|-----|
| `queue_depth` | API probe | Avoid overloaded instances |
| `tokens_per_second` | API probe | Prefer faster instances (AI) |
| `time_to_first_token` | API probe | Latency-sensitive routing |
| `available_vram` | API probe | Model fitting |
| `connections` | Connection count | Least-connections strategy |
| `response_time` | Probe timing | Performance weighting |
| `model_cache` | API probe | Model affinity routing |

### Strategy Configuration

```yaml
process:
  mode: garden
  strategy: performance-weighted  # or: round-robin, least-connections, affinity
  
  # For performance-weighted:
  metrics:
    - name: tokens_per_second
      source: "/api/stats"        # Endpoint to probe
      weight: 1.0                 # Positive = higher is better
    - name: queue_depth
      source: "/api/stats"
      weight: -0.5                # Negative = lower is better
  
  # For affinity:
  model_aware: true               # Prefer instances with requested model cached
  
  # Probe configuration
  probe:
    interval: 5s
    timeout: 2s
```

## Appendix F: Capability Providers

### Built-in Capabilities

Moss provides built-in implementations for certain capabilities, backed by infrastructure:

| Capability | Protocol | Backed By | Three Concerns |
|------------|----------|-----------|----------------|
| `s3` | S3 API | Seed bank | pool / garden / none |

Built-ins are **fallbacks**. When an offering provides the same capability, the offering takes precedence.

### S3 Capability

**Built-in provider:**
- Backed by seed bank storage (USB, NAS, local disk)
- Any stone with storage access becomes a gateway
- Gateways are stateless proxies
- Garden selects best gateway

**Offering providers:**
- MinIO, SeaweedFS, etc.
- Full three-concern configuration
- Takes precedence over built-in

### Capability Declaration in Manifests

Offerings declare capabilities they provide:

```yaml
name: minio
provides: [s3]    # This offering satisfies "s3" capability requests

name: seaweedfs
provides: [s3]    # This also satisfies "s3"

name: mongodb
provides: []      # Does not satisfy any abstract capability
                  # (must be requested by name)
```

### Resolution with Capabilities

```
zen-garden:s3//myapp
         │
         ▼
┌────────────────────────────────────────┐
│ 1. Find offerings with provides: [s3]  │
│    → MinIO exists? Use MinIO           │
│                                        │
│ 2. Else, find built-in s3 gateways     │
│    → Seed bank configured? Use gateway │
│                                        │
│ 3. Else, error                         │
│    → "No s3 capability in garden"      │
└────────────────────────────────────────┘
```

### Gateway Announcement

Stones with seed bank access announce built-in capabilities:

```
_koan-stone._tcp.local
TXT:
  capability=s3
  storage_access=direct|proxy
  storage_id=seed-bank-identifier
  proxy_via=stone-name  (if access=proxy)
```

### Gateway Selection

```
1. Discover all s3 gateways
2. Prefer direct > proxy
3. Select by: health > load > latency
4. Return endpoint
```

### Infrastructure Configuration

Seed banks (infrastructure) enable built-in S3:

```toml
# moss.toml
[[cultivation.seed_banks]]
type = "path"
path = "/mnt/usb"
announce_s3 = true  # Enable S3 gateway

[[cultivation.seed_banks]]
type = "network"
protocol = "nfs"
host = "nas.local"
path = "/volume1/zen-garden"
announce_s3 = true
```

---

**End of Specification**

---

**End of Specification**
