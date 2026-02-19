---
audience: [developer, ai]
doc_type: decision
status: proposed
last_verified: 2026-02-18
---

# ORCH-0004: Gateway Announcement — Orchestrator Self-Registration

**Date**: 2026-02-18
**Status**: Proposed
**Applies to**: `zen-garden-ollama-orchestrator` crate, `moss` (API + chirp), `garden-common`
**Depends on**: Koi mDNS announce API (`POST /v1/mdns/announce`)

## Context

The Ollama orchestrator is a VRAM-aware routing proxy (port 21434) that sits in
front of N raw Ollama instances (port 11434 each). Today it is **invisible** to
the garden's service discovery system:

```
$ rake find ollama

  ollama:adopted (ai) on stone-azure-pool
  http://stone-azure-pool.local:11434

  ollama:adopted (ollama) on stone-quiet-lens
  http://stone-quiet-lens.local:11434

  ollama:adopted (ollama) on stone-azure-pool
  http://stone-azure-pool.local:11434

  ollama:adopted (ollama) on stone-stained-luminance
  http://stone-stained-luminance.local:11434
```

The orchestrator's proxy port is only reachable by clients that know about it
out of band. There is no way for `rake find ollama` to return the orchestrator.

### Architectural Constraint

The orchestrator **does not require a local Moss**. It only needs Koi (mDNS,
DNS, UDP). It discovers stones via Koi mDNS browse or explicit `--stone` flag,
then queries topology for Ollama instances. It may run on:

- A stone machine (Moss present → has a `.local` hostname)
- A bare machine (no Moss → no `.local` hostname, no chirps)

This means the orchestrator **cannot assume** it inherits a stone's identity or
network name. It must self-register for both name resolution (mDNS) and
service discovery (topology).

### What Exists Today

| System | Relevant API | Status |
|--------|-------------|--------|
| **Koi mDNS** | `POST /v1/mdns/announce` — register service with name, type, port, lease, TXT records | Available |
| **Koi mDNS** | `PUT /v1/mdns/heartbeat/{id}` — renew lease | Available |
| **Koi mDNS** | `DELETE /v1/mdns/unregister/{id}` — deregister + goodbye packets | Available |
| **Moss topology** | UDP chirps carry `Vec<TopologyServiceEntry>` per stone | Available |
| **Moss service discovery** | `GET /api/v1/garden/services?q=ollama` → searches local offerings + topology cache | Available |
| **Moss gateway API** | `PUT /api/v1/garden/gateway/{offering}` | **Not yet implemented** |

## Decision

### Two-Registration Model

The orchestrator performs **two independent registrations** at boot:

1. **mDNS registration via Koi** — makes the orchestrator reachable by hostname
2. **Gateway registration via Moss** — makes the orchestrator discoverable via `rake find`

Both use TTL-based leases. Both auto-expire on crash. Both are explicitly
deregistered on graceful shutdown.

### Desired Outcome

```
$ rake find ollama

  ollama:orchestrator on ollama-orchestrator
  http://ollama-orchestrator.local:21434      ← gateway (routed endpoint)

  ollama:adopted (ollama) on stone-quiet-lens
  http://stone-quiet-lens.local:11434         ← raw instance

  ollama:adopted (ollama) on stone-azure-pool
  http://stone-azure-pool.local:11434         ← raw instance

  ollama:adopted (ollama) on stone-stained-luminance
  http://stone-stained-luminance.local:11434  ← raw instance
```

---

## Design

### Registration 1: mDNS via Koi

The orchestrator calls Koi's existing mDNS announce API to register itself on
the local network. This gives it a resolvable `.local` hostname regardless of
whether a Moss instance exists on the same machine.

**Request** (`POST koi:5641/v1/mdns/announce`):

```json
{
  "name": "ollama-orchestrator",
  "type": "_http._tcp",
  "port": 21434,
  "lease_secs": 60,
  "txt": {
    "garden-offering": "ollama",
    "garden-role": "orchestrator"
  }
}
```

**Response** (`201 Created`):

```json
{
  "registered": {
    "id": "a1b2c3d4",
    "name": "ollama-orchestrator",
    "type": "_http._tcp",
    "port": 21434,
    "mode": "heartbeat",
    "lease_secs": 60
  }
}
```

After this call, `ollama-orchestrator.local` resolves to the host machine's IP,
port 21434.

**Heartbeat**: `PUT koi:5641/v1/mdns/heartbeat/{id}` every 30 seconds.

**Shutdown**: `DELETE koi:5641/v1/mdns/unregister/{id}` — Koi sends mDNS goodbye
packets, name disappears from the network immediately.

**Crash**: Lease expires after 60 seconds, Koi stops answering for the name.

### Registration 2: Gateway via Moss

The orchestrator calls a new Moss API to register as the gateway for the
"ollama" offering. This makes it visible in topology chirps so that any stone
in the garden can discover it via `rake find`.

**Request** (`PUT stone:7185/api/v1/garden/gateway/ollama`):

```json
{
  "fqn": "ollama:orchestrator",
  "hostname": "ollama-orchestrator.local",
  "ip": "192.168.1.50",
  "port": 21434,
  "handler_for": ["ollama"],
  "protocol": "http",
  "uri_template": "http://{host}:{port}",
  "source": "zen-garden.ollama.orchestrator"
}
```

**Response** (`200 OK`):

```json
{
  "lease_id": "gw-a1b2c3d4",
  "ttl_seconds": 60
}
```

**Heartbeat**: Same `PUT` call every 30 seconds (idempotent upsert).

**Shutdown**: `DELETE stone:7185/api/v1/garden/gateway/ollama`.

**Crash**: TTL expires after 60 seconds. Moss evicts the entry. Next chirp
no longer includes it. Other stones age it out of their topology cache.

### Key Detail: Self-Reported Address

The gateway registration carries the orchestrator's **own** hostname and IP,
not the host stone's. When Moss builds the `FoundService` for a gateway entry
in `find_services`, it resolves the connection using the gateway's self-reported
address:

```rust
// Gateway resolution — uses gateway's address, NOT stone's address
resolve_connection(
    &gateway.hostname,  // "ollama-orchestrator.local"
    &gateway.ip,        // "192.168.1.50"
    gateway.port,       // 21434
    &gateway.protocol,  // "http"
    gateway.uri_template.as_deref(),
)
```

This decouples the orchestrator's network identity from any stone.

---

## End-to-End Flow

```
┌────────────────────────────────────────────────────────┐
│  Orchestrator boots                                     │
│                                                         │
│  1. POST koi:5641/v1/mdns/announce                     │
│     → ollama-orchestrator.local:21434 is resolvable     │
│     → stores mdns_lease_id                              │
│                                                         │
│  2. Discover stone via Koi (existing flow)              │
│     → resolves stone-azure-pool.local:7185              │
│                                                         │
│  3. PUT stone:7185/api/v1/garden/gateway/ollama        │
│     → Moss stores GatewayRegistration in memory         │
│     → stores gateway_lease_id                           │
│                                                         │
│  4. Every 30s (heartbeat loop):                         │
│     PUT koi:5641/v1/mdns/heartbeat/{mdns_id}           │
│     PUT stone:7185/api/v1/garden/gateway/ollama        │
│                                                         │
│  5. Moss builds next chirp:                             │
│     services: [ollama:adopted, ...existing...]          │
│     gateways: [{ offering: "ollama", hostname:          │
│                  "ollama-orchestrator.local",            │
│                  port: 21434 }]                          │
│                                                         │
│  6. Other stones receive chirp, cache gateway entry     │
│                                                         │
│  7. rake find ollama                                    │
│     → Moss checks gateway entries first                 │
│     → finds gateway for "ollama"                        │
│     → resolves: http://ollama-orchestrator.local:21434  │
│     → returns alongside raw instances                   │
│                                                         │
└────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────┐
│  Graceful shutdown (SIGTERM)                            │
│                                                         │
│  1. DELETE koi:5641/v1/mdns/unregister/{mdns_id}       │
│     → Koi sends mDNS goodbye packets                   │
│                                                         │
│  2. DELETE stone:7185/api/v1/garden/gateway/ollama     │
│     → Moss removes entry, next chirp excludes it        │
│                                                         │
└────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────┐
│  Crash recovery (no shutdown hook ran)                  │
│                                                         │
│  1. Koi lease expires (60s) → stops answering mDNS      │
│  2. Moss gateway TTL expires (60s) → evicts entry       │
│  3. Next chirp omits gateway → other stones forget it   │
│  4. rake find ollama → returns raw instances only        │
│     (natural failover, zero coordination)                │
│                                                         │
└────────────────────────────────────────────────────────┘
```

---

## Data Structures

### garden-common: `GatewayRegistration`

```rust
/// A registered gateway — an orchestrator that fronts an offering.
///
/// Stored in-memory by Moss, included in chirp payloads, and used by
/// service discovery to resolve connection endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayRegistration {
    /// Offering FQN, e.g. "ollama:orchestrator"
    pub fqn: String,

    /// The offering(s) this gateway handles, e.g. ["ollama"]
    pub handler_for: Vec<String>,

    /// Self-reported hostname (registered via Koi mDNS)
    pub hostname: String,

    /// Self-reported IP address
    pub ip: String,

    /// Proxy port (e.g. 21434)
    pub port: u16,

    /// Protocol for URI construction
    pub protocol: String,

    /// URI template for connection resolution, e.g. "http://{host}:{port}"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri_template: Option<String>,

    /// Identifier of the registering process
    pub source: String,

    /// When this registration was created/last refreshed
    pub registered_at: DateTime<Utc>,
}
```

### Moss State: In-Memory Gateway Store

```rust
/// In AppState (or behind a RwLock):
pub gateways: RwLock<HashMap<String, GatewayRegistration>>,
// Key: offering name ("ollama"), Value: registration
// One gateway per offering per stone.
```

TTL eviction: During chirp building, entries older than 60 seconds since
`registered_at` are evicted. No background reaper needed — chirp runs every
~30 seconds anyway.

### Chirp Extension

The `TopologyEntry` (chirp wire format) gains an optional `gateways` field:

```rust
pub struct TopologyEntry {
    pub stone_id: String,
    pub stone_name: String,
    pub address: PeerAddress,
    pub moss_version: String,
    pub services: Vec<TopologyServiceEntry>,
    // ... existing fields ...

    /// Gateway registrations (orchestrators fronting offerings on this stone).
    /// Empty for most stones. Backward-compatible: old Moss ignores this field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gateways: Vec<GatewayRegistration>,
}
```

Old Moss versions that don't understand `gateways` will ignore it via
`#[serde(default)]`. No breaking change.

### Topology Cache Extension

Other stones' topology cache must store gateway entries from received chirps.
In the topology cache's `TopologyEntry`, the `gateways` field is already
present (same struct). Service discovery reads from it.

---

## Moss API Endpoints

### PUT /api/v1/garden/gateway/{offering}

Register or refresh a gateway for an offering.

**Handler logic**:
1. Parse `{offering}` from path and request body
2. Validate: `handler_for` must contain `{offering}`
3. Set `registered_at = Utc::now()`
4. Upsert into `state.gateways` map (key = `{offering}`)
5. Trigger a chirp (self-entry changed → auto-chirp)
6. Return `{ lease_id, ttl_seconds: 60 }`

### DELETE /api/v1/garden/gateway/{offering}

Deregister a gateway.

**Handler logic**:
1. Remove from `state.gateways` map
2. Trigger a chirp (self-entry changed → auto-chirp)
3. Return `200 OK`

### Service Discovery: Gateway-First Resolution

In `find_services()`, before the existing local + topology search:

```rust
pub async fn find_services(criteria, state, fresh) -> ServiceDiscoveryResponse {
    let mut all_services = Vec::new();

    // ── Gateway check (new) ─────────────────────────────────────
    // Check local gateway registrations
    {
        let gateways = state.gateways.read().await;
        for (offering, gw) in gateways.iter() {
            if matches_criteria(criteria, &gw.fqn, offering, "ai", &[], &[]) {
                let conn = connection::resolve_connection(
                    &gw.hostname, &gw.ip, gw.port,
                    &gw.protocol, gw.uri_template.as_deref(),
                );
                all_services.push(FoundService {
                    offering_id: String::new(),
                    name: gw.fqn.clone(),
                    offering: offering.clone(),
                    category: "ai".to_string(),
                    tags: vec!["orchestrator".to_string()],
                    status: "running".to_string(),
                    stone: StoneRef { /* host stone info */ },
                    connection: conn,
                    sub_capabilities: vec![],
                });
            }
        }
    }

    // Check topology cache for remote gateways
    {
        let stones = topology::get_online_stones(&state.topology_cache).await;
        for stone in &stones {
            for gw in &stone.gateways {
                if matches_criteria(criteria, &gw.fqn, &gw.handler_for[0], ...) {
                    // resolve_connection using gw.hostname, gw.ip, gw.port
                    all_services.push(/* ... */);
                }
            }
        }
    }

    // ── Existing logic (unchanged) ──────────────────────────────
    let local_services = find_local_services(criteria, state).await;
    all_services.extend(local_services);

    let cached_services = find_services_in_topology_cache(criteria, state).await;
    all_services.extend(cached_services);

    // ... rest unchanged
}
```

Gateways appear **first** in the results list (before raw instances), which is
the natural position for a routed endpoint. No priority field needed — ordering
is structural.

---

## Orchestrator Implementation

### New Files

#### `infra/koi_client.rs` — Koi mDNS HTTP Client

```rust
pub struct KoiClient {
    http: reqwest::Client,
    base_url: String,
}

impl KoiClient {
    /// Register an mDNS service with Koi.
    /// Returns the registration ID for heartbeat/unregister.
    pub async fn mdns_announce(
        &self, name: &str, port: u16, lease_secs: u32,
        txt: HashMap<String, String>,
    ) -> Result<String>;  // returns registration id

    /// Renew an mDNS heartbeat lease.
    pub async fn mdns_heartbeat(&self, id: &str) -> Result<()>;

    /// Unregister an mDNS service (sends goodbye packets).
    pub async fn mdns_unregister(&self, id: &str) -> Result<()>;
}
```

#### `infra/moss_gateway.rs` — Moss Gateway HTTP Client

```rust
pub struct MossGatewayClient {
    http: reqwest::Client,
}

impl MossGatewayClient {
    /// Register/refresh gateway with Moss.
    pub async fn register(
        &self, stone_endpoint: &str, offering: &str,
        registration: &GatewayRegistration,
    ) -> Result<String>;  // returns lease_id

    /// Deregister gateway from Moss.
    pub async fn deregister(
        &self, stone_endpoint: &str, offering: &str,
    ) -> Result<()>;
}
```

#### `tasks/gateway_announce.rs` — Registration + Heartbeat Loop

```rust
/// Boot sequence:
/// 1. Register with Koi mDNS (get hostname resolvable)
/// 2. Wait for stone discovery (need a Moss endpoint)
/// 3. Register gateway with Moss (get into topology)
/// 4. Heartbeat both every 30s
///
/// Shutdown: deregister both on CancellationToken.
pub async fn run(
    state: AppState,
    koi_client: KoiClient,
    moss_gw_client: MossGatewayClient,
    shutdown: CancellationToken,
) {
    // Phase 1: mDNS announce
    let mdns_id = koi_client.mdns_announce(
        "ollama-orchestrator", state.proxy_port, 60,
        btree!{ "garden-offering" => "ollama", "garden-role" => "orchestrator" },
    ).await;

    // Phase 2: Wait for tended stone
    let stone_endpoint = wait_for_stone(&state, &shutdown).await;

    // Phase 3: Detect self IP
    let self_ip = detect_self_ip();  // from network interface or Koi

    // Phase 4: Register gateway with Moss
    let registration = GatewayRegistration {
        fqn: "ollama:orchestrator".into(),
        handler_for: vec!["ollama".into()],
        hostname: "ollama-orchestrator.local".into(),
        ip: self_ip,
        port: state.proxy_port,
        protocol: "http".into(),
        uri_template: Some("http://{host}:{port}".into()),
        source: state.offering_name.clone(),
        registered_at: Utc::now(),
    };
    moss_gw_client.register(&stone_endpoint, "ollama", &registration).await;

    // Phase 5: Heartbeat loop (30s interval)
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                // Graceful deregister
                koi_client.mdns_unregister(&mdns_id).await.ok();
                moss_gw_client.deregister(&stone_endpoint, "ollama").await.ok();
                return;
            }
            _ = tokio::time::sleep(Duration::from_secs(30)) => {
                koi_client.mdns_heartbeat(&mdns_id).await.ok();
                moss_gw_client.register(&stone_endpoint, "ollama", &registration).await.ok();
            }
        }
    }
}
```

### Changes to `main.rs`

Wire up the new task alongside existing tasks:

```rust
let koi_client = KoiClient::new(&cli.koi_endpoint);
let moss_gw_client = MossGatewayClient::new();

let gateway_handle = tokio::spawn(tasks::gateway_announce::run(
    state.clone(),
    koi_client,
    moss_gw_client,
    shutdown.clone(),
));
```

### Self IP Detection

The orchestrator needs to know its own LAN IP for the gateway registration.
Options (in preference order):

1. **CLI flag / env var**: `--announce-ip` / `ANNOUNCE_IP` — explicit override
2. **From Koi**: After `mdns_announce`, query `GET koi/v1/status` which may
   report the bound interface IP
3. **Network interface scan**: Use `if_addrs` crate (already a dependency in
   the workspace) to find a non-loopback IPv4 address

---

## Backward Compatibility

| Component | Old version behavior | New version behavior |
|-----------|---------------------|---------------------|
| Old Moss receiving chirp with `gateways` | Ignores field (`#[serde(default)]`) | Stores + advertises |
| Old Rake calling `find` | Sees raw instances only | Sees gateway + raw instances |
| Old orchestrator (no announce) | Invisible to topology | Invisible (unchanged) |
| New orchestrator, old Moss | mDNS works; gateway PUT returns 404 — orchestrator logs warning, still reachable by IP | Full flow |

No breaking changes. The gateway mechanism is additive.

---

## Implementation Order

### Phase 1: Orchestrator → Koi (mDNS only)

**Scope**: Orchestrator-only changes. No Moss changes needed.

1. Add `infra/koi_client.rs` to orchestrator crate
2. Add `tasks/gateway_announce.rs` (mDNS registration + heartbeat only)
3. Wire into `main.rs`
4. **Test**: After boot, `ollama-orchestrator.local:21434` resolves and serves
   the Ollama-compatible proxy API

**Result**: Orchestrator is reachable by name. Not yet in `rake find`.

### Phase 2: Moss Gateway API

**Scope**: Moss + garden-common changes.

1. Add `GatewayRegistration` to `garden-common`
2. Add `gateways: RwLock<HashMap<String, GatewayRegistration>>` to Moss AppState
3. Implement `PUT/DELETE /api/v1/garden/gateway/{offering}` handlers
4. Add TTL eviction in chirp builder
5. Include `gateways` in `TopologyEntry` → chirp payload
6. Store gateway entries from incoming chirps in topology cache
7. Add gateway-first check in `find_services()`

**Test**: `curl -X PUT stone:7185/api/v1/garden/gateway/ollama -d '{...}'`
then `rake find ollama` shows the gateway entry.

### Phase 3: Wire Together

**Scope**: Orchestrator changes only.

1. Add `infra/moss_gateway.rs` (Moss gateway HTTP client)
2. Extend `tasks/gateway_announce.rs` with Moss registration + heartbeat
3. Add graceful deregister on shutdown

**Test**: Full end-to-end — orchestrator boots, `rake find ollama` shows
`ollama:orchestrator` on first position, `rake find ollama --format uri`
returns `http://ollama-orchestrator.local:21434`.

### Phase 4: Polish

1. Dashboard indicator: "Gateway: registered on {stone}" status line
2. `rake find ollama --raw` flag to bypass gateway (debugging)
3. Handle stone failover: if tended stone goes down, re-register gateway on
   another stone

---

## Open Questions

1. **Gateway name**: Should the mDNS name be configurable? Default
   `ollama-orchestrator` works for single-orchestrator gardens. Multi-orchestrator
   would need unique names (e.g., `ollama-orchestrator-{hash}`).

2. **Multiple gateways**: If two orchestrators register for "ollama" on
   different stones, `find_services` returns both. Caller picks. Is this
   sufficient, or should there be an election?

3. **Category for gateway entries**: Raw Ollama instances use category "ai"
   (from manifest). The gateway should use the same category so
   `rake find --category ai` finds it. Confirm: hardcode "ai" or derive from
   manifest?

4. **`--raw` flag on rake**: Should this be a general `--no-gateway` flag, or
   specific to the offering? Lean toward general: `rake find ollama --raw`
   skips all gateways, shows only direct instances.

---

## References

- **Koi mDNS API**: `POST /v1/mdns/announce`, `PUT /v1/mdns/heartbeat/{id}`,
  `DELETE /v1/mdns/unregister/{id}` — see Koi `docs/reference/http-api.md`
- **Chirp wire format**: `TopologyEntry` in `src/common/src/types/topology.rs`
- **Service entry**: `TopologyServiceEntry` in `src/common/src/types.rs:713`
- **Service discovery**: `find_services()` in `src/moss/src/domain/service_discovery.rs:421`
- **Connection resolution**: `resolve_connection()` in `src/moss/src/domain/connection.rs:289`
- **Offering manifest**: `src/moss/embedded/manifests/sw/ai/ollama.frontmatter.json`
- **Orchestrator main**: `src/orchestrators/ollama/src/main.rs`
- **Orchestrator discovery**: `src/orchestrators/ollama/src/tasks/discovery.rs`
- **ORCH-0002**: Routing safety net (fitness scores used in routing)
- **ORCH-0003**: Fitness profiler (benchmark system)
- **TOPO-0001**: Chirp protocol (UDP topology broadcasts)
