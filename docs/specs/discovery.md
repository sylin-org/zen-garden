# Discovery Protocol Specification

**Purpose:** Technical specification for mDNS service discovery, TXT records, and client resolution.  
**Audience:** Developers implementing discovery, operators troubleshooting network issues.

---

## Table of Contents

1. [Overview](#overview)
2. [mDNS Service Types](#mdns-service-types)
3. [TXT Record Schema](#txt-record-schema)
4. [Client Resolution Algorithm](#client-resolution-algorithm)
5. [Discovery Flow](#discovery-flow)
6. [UDP Broadcast Discovery](#udp-broadcast-discovery)
7. [Connection String Resolution](#connection-string-resolution)

---

## Overview

Zen Garden uses two discovery mechanisms:

- **mDNS (Linux/macOS):** Zero-config automatic discovery via Multicast DNS
- **UDP Broadcast (Windows):** Broadcast-based discovery for Windows without mDNS

Both protocols enable clients to discover Stones and services without manual configuration.

---

## mDNS Service Types

### Moss Self-Announcement

**Service Type:** `_moss._tcp.local.`  
**Instance Name:** `<stone-name>-moss._moss._tcp.local.`  
**Purpose:** Announce Moss daemon for Rake CLI discovery

**Example:**

```
stone-01-moss._moss._tcp.local.
Port: 7185
TXT: stone_name=stone-01
     version=0.1.0
     api_port=7185
     health=healthy
```

### Native Service Announcement

**Service Type:** `_koan-stone._tcp.local.`  
**Instance Name:** `<stone-name>-<offering>._koan-stone._tcp.local.`  
**Purpose:** Announce native database/service on its vendor protocol

**Example: MongoDB Native**

```
stone-01-mongodb._koan-stone._tcp.local.
Port: 27017
TXT: offering=mongodb
     port=27017
     protocol=native
     version=7.0.4
     categories=database,document-database
     health=healthy
     priority=50
```

### Agnostic Sidecar Announcement

**Service Type:** `_koan-stone._tcp.local.`  
**Instance Name:** `<stone-name>-<offering>-agnostic._koan-stone._tcp.local.`  
**Purpose:** Announce HTTP REST API wrapper for database-neutral access

**Example: MongoDB Agnostic Sidecar**

```
stone-01-mongodb-agnostic._koan-stone._tcp.local.
Port: 8080
TXT: offering=mongodb-agnostic
     port=8080
     protocol=agnostic
     version=1.0.2
     backend_service_version=7.0.4
     categories=database,document-database
     set_mode=database
     capabilities=crud,query,filter,bulk,transactions
     health=healthy
     priority=50
```

---

## TXT Record Schema

### Moss Daemon TXT Records

| Field        | Required | Example        | Description                                |
|--------------|----------|----------------|--------------------------------------------|
| `stone_name` | Yes      | `stone-01`     | Unique Stone identifier                    |
| `version`    | Yes      | `0.1.0`        | Moss daemon version                        |
| `api_port`   | Yes      | `7185`         | HTTP API port                              |
| `health`     | Yes      | `healthy`      | Health status: `healthy`, `degraded`, `offline` |

### Native Service TXT Records

| Field        | Required | Example                  | Description                                |
|--------------|----------|--------------------------|--------------------------------------------|
| `offering`   | Yes      | `mongodb`                | Offering name from template                |
| `instance`   | No       | `staging`                | Instance name for multi-instance offerings |
| `port`       | Yes      | `27017`                  | Native protocol port                       |
| `protocol`   | Yes      | `native`                 | Always `native` for native services        |
| `protocols`  | No       | `mongodb,storage`        | Comma-separated protocols supported        |
| `protocol_default` | No | `mongodb`               | Default protocol for this offering         |
| `admission`  | No       | `communal`               | Admission policy: `communal` or `dedicated`|
### Agnostic Sidecar TXT Records

| Field                      | Required | Example                  | Description                                |
|----------------------------|----------|--------------------------|--------------------------------------------|
| `offering`                 | Yes      | `mongodb-agnostic`       | Offering name with `-agnostic` suffix      |
| `port`                     | Yes      | `8080`                   | HTTP REST API port                         |
| `protocol`                 | Yes      | `agnostic`               | Always `agnostic` for sidecars             |
| `version`                  | Yes      | `1.0.2`                  | Sidecar version                            |
| `backend_service_version`  | Yes      | `7.0.4`                  | Backend service version (MongoDB 7.0.4)    |
| `categories`               | No       | `database,document-database` | Comma-separated category tokens      |
| `set_mode`                 | No       | `database`               | Isolation mode: `database` or `collection` |
| `capabilities`             | No       | `crud,query,filter,bulk` | Comma-separated API capabilities           |
| `health`                   | Yes      | `healthy`                | Health status                              |
| `priority`                 | No       | `50`                     | Priority for service selection (0-100)     |

---

## Client Resolution Algorithm

### Goal

Resolve a `zen-garden:` URI (e.g. `zen-garden:mongodb`, `zen-garden:database`, `zen-garden:?cap=s3`) to a native connection string.

### Summary

1. Parse the URI per [URI-0003](../decisions/URI-0003-zen-garden-urn-form-scheme.md) grammar
2. Build a candidate set:
   - Empty target + `cap=` → resources matching the capability
   - Explicit kind (`<kind>//<name>`) → resources of that kind by name
   - Bare name → run cascade (offering → stone → bank → service → companion → pond → garden → category)
3. Apply query constraints (`at=`, `cap=`, `tags=`, `protocol=`)
4. Apply instance qualifier if present
5. Rank by health → priority → latency
6. Apply action (`wish`, `logs`, etc.) if present
7. Build native connection string from the selected endpoint

The full algorithm with worked examples is in [§"Connection String Resolution"](#connection-string-resolution) below.

---

## Discovery Flow

### Rake CLI Discovery Flow

**Priority order:**

1. **Localhost cache query** (if Rake running on Stone)
   - Query: `GET localhost:7185/api/garden/stones`
   - Latency: <1ms (zero discovery overhead)

2. **UDP multicast/broadcast discovery** (Windows-compatible)
   - Multicast: UDP 239.255.42.99:7184 (primary)
   - Fallback: Directed broadcast on each interface
   - Response: Unicast with Stone topology
   - Latency: <100ms

3. **mDNS browse** (Linux/macOS)
   - Browse: `_moss._tcp.local.`
   - Discover all Moss daemons
   - Latency: <50ms

4. **Lantern HTTP query** (cross-subnet fallback)
   - Query: `GET <lantern-endpoint>/api/garden/stones`
   - Works across subnets
   - Latency: <200ms

5. **Manual `--at` flag** (explicit bypass)
   - Example: `garden-rake list --at stone-01`
   - Skips all discovery

### Application Client Discovery Flow

**Priority order:**

1. **Specific service (native protocol):**
   - Connection string: `zen-garden:mongodb/myapp`
   - Query mDNS: `_koan-stone._tcp.local.` → Filter `offering=mongodb` and `protocol=native`
   - Resolve to: `mongodb://10.0.1.10:27017/myapp`

2. **Category service (agnostic HTTP):**
   - Connection string: `zen-garden:database/myapp`
   - Query mDNS: `_koan-stone._tcp.local.` → Filter `categories CONTAINS database` and `protocol=agnostic`
   - Resolve to: `http://10.0.1.10:8080/v1/data/myapp`

---

## UDP Broadcast Discovery

**Purpose:** Windows-compatible discovery without mDNS support

### Protocol Flow

**1. Rake broadcasts request:**

```json
UDP 239.255.42.99:7184 (multicast) or 255.255.255.255:7184 (broadcast fallback)
{
  "discover": "moss",
  "request_id": "01933b83-1234-7abc-9000-abcdef123456",
  "requester": "rake-cli"
}
```

> **Note:** Zen Garden uses multicast-first transport. See [discovery-transport.md](discovery-transport.md) for details on multi-homed system support.

**2. All Moss daemons calculate election delay:**

```rust
// Election algorithm (prevents reply storm)
let input = format!("election:{}:{}", stone_id, request_id);
let hash = blake3::hash(input.as_bytes());
let delay_ms = (hash.as_bytes()[0] as u64) * 30; // 0-7650ms
tokio::time::sleep(Duration::from_millis(delay_ms)).await;
```

**3. First responder unicast to requester:**

```json
UDP <requester-ip>
{
  "stone_name": "stone-01",
  "stone_endpoint": "http://10.0.1.10:7185",
  "lantern_endpoint": "http://10.0.1.5:7186",
  "moss_version": "0.1.0"
}
```

**4. Rake queries Stone for full topology:**

```http
GET http://10.0.1.10:7185/api/garden/stones
```

### Benefits

- **Zero-discovery common case:** 90% of Rake invocations hit localhost cache (<1ms)
- **Windows first-class:** UDP broadcast works without mDNS daemon
- **No Lantern dependency:** Rake can discover Stones without centralized registry
- **Hot cache always available:** Moss maintains current topology via UDP broadcasts
- **Single query reveals all:** UDP response includes Lantern endpoint for full garden

### Configuration

```toml
# /etc/zen-garden/moss.toml
[discovery]
udp_broadcast_port = 7184
udp_broadcast_timeout = 3000  # ms
udp_broadcast_retry = 3
election_hash_algorithm = "blake3"
```

---

## Connection String Resolution

The full URI grammar is specified in [URI-0003](../decisions/URI-0003-zen-garden-urn-form-scheme.md). This section describes how the discovery layer resolves URI-0003 URIs to native connection strings.

### Grammar (summary)

```
zen-garden:[<target>][/<sub-path>][?<query>][#<fragment>]

<target> := <bare-name>            # cascade resolution
          | <kind>//<name>          # explicit kind
          | (empty, with cap= query)
```

**Cascade order** (when `<target>` is a bare name): offering → stone → bank → service → companion → pond → garden → category. First match wins.

**Reserved kinds**: `offering`, `stone`, `bank`, `service`, `companion`, `pond`, `garden`. Names colliding with these are rejected at resource-creation time.

**Standard query parameters**: `cap=` (capability constraint), `action=` (wish, logs, restart, etc.), `at=` (replica/stone pin), `tags=` (taxonomy filter), `protocol=` (wire-protocol hint).

### Examples

| URI | Resolved as |
|---|---|
| `zen-garden:mongodb` | Cascade hits offering "mongodb" → `mongodb://10.0.1.10:27017` |
| `zen-garden:mongodb/mydb` | Same, with database sub-path → `mongodb://10.0.1.10:27017/mydb` |
| `zen-garden:mongodb:staging` | Offering "mongodb" instance "staging" → `mongodb://10.0.1.10:27018` |
| `zen-garden:?cap=s3` | Empty target + capability → any S3-speaking endpoint |
| `zen-garden:?cap=s3&at=seed-usb-01` | Same, pinned to a specific bank |
| `zen-garden:offering//mongodb` | Explicit offering kind (forces offering cascade level) |
| `zen-garden:stone//crystal-forest` | Explicit stone reference |
| `zen-garden:database` | Cascade falls through to category index → any database offering |
| `zen-garden:mongodb?action=wish` | Find-or-provision MongoDB |

### Resolution algorithm

```
1. Parse URI per URI-0003 grammar
   - Returns: target (or empty), sub-path, query, fragment
   - Empty target requires cap= query; otherwise parse error

2. Build candidate set:
   IF target is empty:
     candidates = all offerings whose protocols ∋ cap query
   ELIF target uses explicit kind (kind//name):
     candidates = resources of that kind matching name
   ELSE (bare name):
     candidates = run_cascade(name)
     where run_cascade tries each kind in order, returning first non-empty match
     (final stage consults the category index)

3. Apply query constraints in order:
   - at=<name>: hard filter; resolution fails if no candidate matches
   - cap=<X[,Y]>: candidates must support all listed capabilities
   - tags=<X[,Y]>: candidates' taxonomy must include all listed tags
   - protocol=<X>: soft preference (does not exclude non-matching candidates)

4. Apply instance qualifier (if target has :instance):
   - candidates = candidates filtered by instance match
   - empty result is a resolution failure (not a category fallback)

5. Select best endpoint:
   - Rank by health: healthy > degraded > offline
   - Rank by priority: higher first
   - Rank by latency: faster first

6. Apply action (if action= present):
   - "wish": on candidate-set empty AND resolver has provisioning rights, request provisioning; otherwise return wish-failed error
   - "logs", "restart", etc.: kind-specific verbs; resolver returns the appropriate endpoint URL or invokes the action
   - default (no action): return native connection string

7. Build native connection string:
   - Native protocol: <protocol>://<endpoint>[<sub-path>]
     Example: mongodb://10.0.1.10:27017/mydb
   - Agnostic HTTP: http://<endpoint>[/v1/<sub-path>]
     Example: http://10.0.1.10:8080/v1/data/mydb
```

### Worked examples

**Bare name + sub-path** (`zen-garden:mongodb/myapp`):

```
1. Parse: target="mongodb", sub_path="myapp"
2. Cascade: offering "mongodb" matches → candidate set
3. Constraints: none
4. Instance: none
5. Selection: pick healthy stone-01 (priority 50)
6. Action: none
7. Result: mongodb://10.0.1.10:27017/myapp
```

**Capability-only** (`zen-garden:?cap=s3`):

```
1. Parse: target empty, cap=["s3"]
2. Candidates: all offerings whose `protocols` TXT record contains "s3"
3. Constraints: cap satisfied by candidate set construction
4. Instance: none
5. Selection: prefer offering > gateway > seed-bank
6. Action: none
7. Result: http://10.0.1.10:9000 (MinIO native S3)
```

**Wish on miss** (`zen-garden:postgres-prod?action=wish`):

```
1. Parse: target="postgres-prod", action="wish"
2. Cascade: no match across any kind
3. Action=wish: request provisioning via Moss API
   - Resolver MUST have provisioning capability; otherwise wish-failed
4. Provisioning succeeds: returns endpoint
5. Result: postgresql://10.0.1.10:5432
```

### Client library responsibilities

1. Parse URI per URI-0003 grammar (use a real URI library — `url` crate in Rust, `System.Uri` in C#)
2. Run discovery (mDNS + UDP broadcast); cache topology with 90-second TTL
3. Apply resolution algorithm above
4. Cache resolved endpoint (5-minute TTL recommended at application level)
5. Reconnect on failure: re-run resolution, do not cache failed endpoints
6. Fallback to alternate candidate when ranked-best fails connection

---

## Next Steps

- **Moss daemon specification:** [moss-daemon-lifecycle.md](moss-daemon-lifecycle.md)
- **Rake CLI specification:** [rake-commands.md](rake-commands.md)
- **Service offerings specification:** [offerings.md](offerings.md)
- **Troubleshooting discovery:** [../guides/troubleshooting.md](../guides/troubleshooting.md)
