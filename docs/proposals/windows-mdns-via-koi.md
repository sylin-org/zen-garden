---
audience: [contributor]
doc_type: proposal
status: draft
last_verified: 2026-02-08
---

# Windows mDNS via Koi Integration

**Author**: Claude / onose
**Date**: 2026-02-08

---

## Problem Statement

Windows Moss has no mDNS capability. All mDNS code is `#[cfg(target_os = "windows")]` stubbed to no-ops:

- `MdnsHandle` is an empty struct
- `announce_moss()` returns a dummy handle
- `start_mdns_lurk_listener()` returns an empty broadcast channel
- `reregister()` is a no-op

This means Windows stones are invisible to mDNS-based discovery. Linux stones announce themselves via `_moss._tcp.local.` and passively discover neighbors through mDNS — Windows stones cannot do either.

**Who is affected**: Any garden with Windows stones. They rely solely on UDP chirps for topology visibility, missing the fast initial discovery that mDNS provides.

## Proposed Solution

Replace the Windows no-op stubs with an HTTP client that delegates to [Koi](https://github.com/onose/koi), an mDNS proxy running on the same machine. Koi exposes mDNS registration, browsing, and lifecycle events through a REST/SSE API.

### Design Principles

1. **No hard dependency** — If Koi isn't running, Windows Moss behaves exactly as today (silent no-ops). Zero regression.
2. **Full feature parity** — If Koi is present, Windows gets the same mDNS capabilities as Linux: service registration, passive discovery via SSE, re-registration on IP changes.
3. **Resilient** — Connection loss triggers automatic reconnection with backoff. Registration re-established on recovery.

### Architecture Overview

```
Linux Moss                          Windows Moss
┌─────────────────┐                ┌─────────────────┐
│ mdns.rs         │                │ mdns.rs         │
│                 │                │                 │
│ MdnsHandle {    │                │ MdnsHandle {    │
│   daemon:       │                │   koi: Option<  │
│     ServiceDaemon│               │     KoiClient>  │
│   registered:   │                │   reg_id:       │
│     AtomicBool  │                │     RwLock<Opt>  │
│ }               │                │ }               │
│                 │                │                 │
│ announce_moss() │                │ announce_moss() │
│  → ServiceDaemon│                │  → POST /v1/    │
│    .register()  │                │    services     │
│                 │                │  + heartbeat    │
│ lurk_listener() │                │  task           │
│  → ServiceDaemon│                │                 │
│    .browse()    │                │ lurk_listener() │
│  → ServiceEvent │                │  → GET /v1/     │
│    loop         │                │    events (SSE) │
│                 │                │  → parse +      │
│ reregister()    │                │    broadcast    │
│  → daemon       │                │                 │
│    .register()  │                │ reregister()    │
│                 │                │  → DELETE old   │
└─────────────────┘                │  → POST new     │
                                   └─────────────────┘
                                          │
                                          ▼
                                   ┌─────────────────┐
                                   │ Koi daemon      │
                                   │ localhost:5641   │
                                   │                 │
                                   │ mDNS ←→ HTTP   │
                                   └─────────────────┘
```

### Koi API Mapping

| Moss Operation | Linux (mdns-sd) | Windows (Koi HTTP) |
|---|---|---|
| Register service | `ServiceDaemon::register(ServiceInfo)` | `POST /v1/services` with `ip` field |
| Unregister | `ServiceDaemon::unregister(fullname)` | `DELETE /v1/services/{id}` |
| Re-register (IP change) | `daemon.register(new_info)` | `DELETE` old + `POST` new |
| Browse/discover | `daemon.browse(type)` → `ServiceEvent` loop | `GET /v1/events?type=_moss._tcp&idle_for=0` SSE |
| Keep-alive | Built into mdns-sd daemon | `PUT /v1/services/{id}/heartbeat` every 60s |
| Health check | N/A (in-process) | `GET /healthz` |

### Registration Flow

```
announce_moss() called at boot (Phase 4):
  1. Probe GET /healthz on localhost:5641
     ├─ Timeout/error → return MdnsHandle { koi: None } (no-op, like today)
     └─ OK →
  2. POST /v1/services
     {
       "name": "{stone_name}",
       "type": "_moss._tcp",
       "port": 7185,
       "ip": "{current_lan_ip}",
       "txt": {
         "stone_id": "{stone_id}",
         "stone_name": "{stone_name}",
         "mac": "{mac_address}"
       },
       "lease_secs": 120
     }
  3. Store registration ID
  4. Spawn heartbeat task (PUT /heartbeat every 60s)
  5. Return MdnsHandle { koi: Some(client), reg_id: Some(id) }
```

### Re-registration Flow (IP Change)

```
reregister(new_ip, new_mac) called by announce_resolution_change():
  1. If koi is None → return Ok(()) (no-op)
  2. DELETE /v1/services/{old_id}  (best-effort, ignore errors)
  3. POST /v1/services with updated ip/txt
  4. Store new registration ID
  5. Heartbeat task continues with new ID
```

### Discovery Flow (Lurk-Listener)

```
start_mdns_lurk_listener() called at Phase 11.5:
  1. Probe GET /healthz
     ├─ Error → return dummy broadcast channel (like today)
     └─ OK →
  2. Create broadcast channel (same signature as Linux)
  3. Spawn background task:
     loop {
       a. Connect GET /v1/events?type=_moss._tcp&idle_for=0
       b. Parse SSE stream line by line:
          - "resolved" events → extract stone_id, stone_name, ip, port, mac
          - Skip self (stone_name == self_stone_name)
          - Skip non-LAN IPs (loopback, link-local)
          - Build MdnsDiscoveredStone
          - Send to broadcast channel
          - "removed" events → log (topology handles TTL)
       c. On stream break → backoff reconnect (1s, 2s, 4s, 8s... cap 30s)
       d. On reconnect → reset backoff to 1s
     }
  4. Return broadcast receiver
```

### Heartbeat Task

```
Spawned after successful registration:
  loop {
    sleep(60s)
    PUT /v1/services/{id}/heartbeat
    ├─ 200 OK → continue
    ├─ 404 (expired/removed) → re-register
    └─ Connection error → mark unhealthy, backoff retry
        On recovery → re-register from scratch
  }
```

### Resilience Matrix

| Scenario | Behavior |
|---|---|
| Koi not installed | `announce_moss()` returns no-op handle. Zero change from today. |
| Koi installed but stopped at boot | Same as above — health probe fails, graceful fallback. |
| Koi starts after Moss boot | Not auto-detected. Requires Moss restart (acceptable). |
| Koi crashes mid-session | Heartbeat fails → reconnect loop. SSE breaks → reconnect loop. Both re-register on recovery. |
| Koi restarts mid-session | Registration lost. Heartbeat gets 404 → re-registers. SSE reconnects → resumes discovery. |
| Network IP changes | `announce_resolution_change()` calls `reregister()` → DELETE old + POST new with pinned IP. |
| Moss shuts down | Best-effort `DELETE /v1/services/{id}`. If fails, Koi's lease expires in 120s. |

### KoiClient Module

New internal module `src/moss/src/infra/koi_client.rs`:

```rust
/// HTTP client for Koi mDNS proxy
///
/// Used on Windows to delegate mDNS operations to a local Koi daemon.
/// All methods are fail-safe — errors are logged but never propagated
/// to callers in a way that breaks Moss operation.
pub struct KoiClient {
    client: reqwest::Client,
    base_url: String,  // "http://localhost:5641"
}

impl KoiClient {
    /// Probe Koi health. Returns None if Koi is unreachable.
    pub async fn try_connect() -> Option<Self>;

    /// Register a service. Returns registration ID.
    pub async fn register(
        &self, name: &str, service_type: &str,
        port: u16, ip: &str, txt: HashMap<String, String>,
        lease_secs: u32,
    ) -> Result<String>;

    /// Unregister by ID (best-effort).
    pub async fn unregister(&self, id: &str) -> Result<()>;

    /// Send heartbeat. Returns true if renewed, false if expired.
    pub async fn heartbeat(&self, id: &str) -> Result<bool>;

    /// Open SSE events stream. Returns a reader that yields KoiEvent.
    pub async fn events_stream(
        &self, service_type: &str,
    ) -> Result<impl Stream<Item = KoiEvent>>;
}
```

### Configuration

| Source | Variable | Default | Purpose |
|---|---|---|---|
| Environment | `KOI_PORT` | `5641` | Koi HTTP port |
| Environment | `KOI_HOST` | `localhost` | Koi hostname (for remote Koi) |
| Hardcoded | — | `120s` | Registration lease |
| Hardcoded | — | `60s` | Heartbeat interval |
| Hardcoded | — | `1s → 30s` | Reconnect backoff range |

### IP Filtering (Discovery)

When processing events from Koi, filter IPs to LAN-routable addresses only:

- Accept: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`
- Reject: `127.0.0.0/8` (loopback), `169.254.0.0/16` (link-local), `172.17.0.0/16` (Docker default bridge)

With Koi's new `ip` pinning feature, registration-side multi-IP is solved. This filter is defense-in-depth for the discovery (consumer) side, in case neighbor stones registered without IP pinning.

## Implementation Plan

### Phase 1: KoiClient module
- New file: `src/moss/src/infra/koi_client.rs`
- HTTP client with health probe, register, unregister, heartbeat, events stream
- SSE parser for `data: {...}` lines
- Unit tests with mock responses

### Phase 2: Replace Windows MdnsHandle
- Modify `#[cfg(target_os = "windows")]` blocks in `src/moss/src/mdns.rs`
- `MdnsHandle` gains `koi: Option<KoiClient>`, `reg_id`, heartbeat task handle
- `announce_moss()` probes Koi, registers if available, spawns heartbeat
- `reregister()` does DELETE + POST cycle
- `is_registered()` checks actual registration state

### Phase 3: Replace Windows lurk-listener
- `start_mdns_lurk_listener()` probes Koi, connects SSE if available
- Background task with reconnection logic
- Feeds `MdnsDiscoveredStone` into existing broadcast channel
- Same topology cache integration as Linux (Phase 11.5 in run.rs unchanged)

### Phase 4: Shutdown cleanup
- Add `unregister()` call to Moss shutdown path
- Best-effort — fire and forget, lease provides backup cleanup

### Phase 5: Bundling and service management
- New `tools/koi/tool.json` manifest (external tool reference with repo link)
- `dist.json` gains `externalTools` section pointing to `tools/` directory
- `DistConfig.psm1` gains `Get-ExternalTools` + `Copy-ExternalToolToStaging`
- `build-windows.ps1` stages `koi.exe` from local Koi dist into `bin/tools/`
- `take-root` installs Koi service via `koi.exe install` before starting Moss
- Package layout: `bin/tools/koi.exe` (Windows only, not included in Linux tarball)

### Phase 6: Testing
- Integration test: register → verify in events → unregister
- Resilience test: simulate Koi restart during active session
- Cross-platform: verify Linux mDNS path unchanged (`cargo test --package garden-moss`)
- Build pipeline: verify `tools/koi/` is picked up and staged correctly

## Bundling and Service Management

Koi is bundled as an **external tool** — a pre-built binary from a separate repo, not a cargo workspace member.

### Package Layout

```
zen-garden-{version}-windows-amd64/
├── bin/
│   ├── garden-moss.exe
│   ├── garden-rake.exe
│   ├── tools/
│   │   └── koi.exe              ← external tool
│   └── companions/
│       └── ...
└── package.json                  ← includes koi in components
```

### External Tools Convention

```
tools/
  koi/
    tool.json       ← repo URL, binary name, platforms, service metadata
```

The `tool.json` manifest provides everything the build pipeline and installer need:
- Where to find the pre-built binary (`acquire.localDist`)
- Which platforms it applies to (`platforms: ["windows"]`)
- Service management verbs (`service.installVerb`, `service.uninstallVerb`)
- Health check endpoint for runtime probing

### Installation Order (take-root)

1. Copy `koi.exe` to `C:\ProgramData\ZenGarden\tools\`
2. Run `koi.exe install` — creates Windows Service, firewall rules, recovery policy
3. Copy `garden-moss.exe` to `C:\ProgramData\ZenGarden\`
4. Create `ZenGardenMoss` service, start it
5. Moss boots → probes `localhost:5641` → Koi is already running

### Lifecycle

- Koi and Moss are independent Windows Services with `AutoStart`
- No service dependency declared — Moss degrades gracefully if Koi isn't ready
- Koi has its own recovery policy (restart 5s, 10s, then stop; 24h reset)
- Upgrades: `koi.exe install` is idempotent (stops old, replaces, restarts)

## Alternatives Considered

### Direct mdns-sd on Windows

- **Pros**: No external dependency, same code as Linux
- **Cons**: `mdns-sd` crate works on Windows but relies on OS mDNS stack which is unreliable. The crate author documents Windows quirks. Services appear in browse but fail to resolve — the exact problem Koi was built to solve.
- **Why not**: Battle-tested evidence that raw mdns-sd on Windows is unreliable. Koi wraps it with proven reliability.

### Compile Koi into Moss

- **Pros**: Single binary, no external dependency
- **Cons**: Koi is a standalone daemon designed for multi-consumer use. Embedding it creates lifecycle coupling, doubles mDNS daemon instances if Koi is also running standalone.
- **Why not**: Violates separation of concerns. Koi serves other consumers beyond Moss.

### Skip mDNS, rely solely on UDP chirps

- **Pros**: No new code, already works
- **Cons**: Slower initial discovery (must wait for first chirp cycle). No mDNS visibility to non-Moss consumers (e.g., Koan framework).
- **Why not**: mDNS provides instant discovery on startup. Koan and other consumers use mDNS directly.

## Impact

- **Windows Moss**: Gains full mDNS parity with Linux when Koi is present
- **Linux Moss**: Zero changes — all new code is `#[cfg(target_os = "windows")]`
- **No breaking changes**: Koi absence = current behavior
- **New dependency**: `reqwest` (already in Cargo.toml)
- **New optional runtime dependency**: Koi daemon on Windows host

## Open Questions

- Should Moss auto-detect Koi coming online after boot? (Current plan: no, require restart. Keeps implementation simple.)
- Should the Koi endpoint be configurable via `ZG_KOI_ENDPOINT` for remote Koi scenarios? (Useful for Docker-in-Docker setups where Koi runs on the host.)

## References

- [Koi CONTAINERS.md](https://github.com/onose/koi/blob/main/CONTAINERS.md) — Full API reference
- [COMM-0001](../decisions/COMM-0001-p2p-transport-singleton.md) — P2P transport singleton (UDP chirp path)
- [TOPO-0002](../decisions/TOPO-0002-shared-topology-directory.md) — Shared topology directory
- Current stubs: `src/moss/src/mdns.rs` lines 122-147, 271-282
