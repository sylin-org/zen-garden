# Static IP Management for Network Infrastructure Offerings

**Status:** Implemented
**Author:** Infrastructure Team
**Date:** 2026-02-01
**Related:** Pi-hole, DNS offerings, DHCP lease stability

## Problem Statement

Network infrastructure offerings like Pi-hole (DNS) require predictable IP addresses. When a stone's DHCP lease renews with a different IP, all devices configured to use that DNS server lose resolution—a catastrophic failure for the network.

Currently, users must manually:
1. Configure DHCP reservations on their router
2. Set static IPs via OS-level tools
3. Enable Pi-hole's DHCP server mode

This friction undermines the "just works" experience zen-garden aims to provide.

## Design Goals

1. **Premium UX:** User runs `rake offer pihole`, accepts conditions, everything else is automatic
2. **Fail-Safe:** Machine always remains reachable; DHCP fallback on any failure
3. **Opt-In:** Static IP only when needed (offering requests it) or user enables it
4. **Observable:** Clear status visibility via API, CLI, and console events

## Design Principles

- **SoC:** Network configuration is a domain concern, platform specifics are infrastructure
- **DDD:** `StaticIpRequest` is a domain event; platform adapters handle implementation
- **YAGNI:** Phase 1 is Linux-only; Windows comes when needed
- **KISS:** ARP probe + single platform adapter per OS
- **DRY:** Reuse existing `NetworkMonitor`, config patterns, console event system

## Architecture

### Domain Layer (`src/moss/src/domain/`)

```
domain/
├── network.rs              # NEW: StaticIpRequest, NetworkMode, conflict types
└── events.rs               # EXTEND: NetworkEvent variants
```

**Domain Types:**

```rust
/// Network addressing mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkMode {
    /// OS-managed DHCP (default)
    Dhcp,
    /// Moss-managed static IP from configured pool
    Static {
        address: Ipv4Addr,
        applied_at: DateTime<Utc>,
    },
    /// Static IP desired but fell back to DHCP
    FallbackDhcp {
        desired: Ipv4Addr,
        reason: String,
        fallback_at: DateTime<Utc>,
    },
}

/// Persistent static IP state with offering-bound lifecycle
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StaticIpState {
    pub mode: NetworkMode,
    /// Offerings currently using the static IP (reference counting)
    /// When empty, system reverts to DHCP
    pub requested_by: Vec<String>,
}

/// Request for static IP assignment (domain event)
#[derive(Debug, Clone)]
pub struct StaticIpRequest {
    pub offering: String,
    pub reason: String,
    pub severity: StaticIpSeverity,
}

/// Request to release static IP (domain event, on offering removal)
#[derive(Debug, Clone)]
pub struct StaticIpRelease {
    pub offering: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StaticIpSeverity {
    /// Informational - offering works without static IP
    Info,
    /// Warning - offering works better with static IP
    Warn,
    /// Required - offering refuses to install without static IP
    Required,
}
```

### Infrastructure Layer (`src/moss/src/infra/`)

```
infra/
├── network/
│   ├── mod.rs              # NEW: Platform detection, trait definitions
│   ├── probe.rs            # NEW: ARP/ICMP conflict detection
│   ├── linux.rs            # NEW: netplan/NetworkManager adapter
│   └── state.rs            # NEW: Persistent network state file
```

**Platform Trait (SoC):**

```rust
/// Platform-specific network configuration
#[async_trait]
pub trait NetworkPlatform: Send + Sync {
    /// Detect which network stack is available
    fn detect() -> Option<Box<dyn NetworkPlatform>> where Self: Sized;

    /// Apply static IP configuration
    async fn apply_static(&self, config: &StaticIpApply) -> Result<(), NetworkError>;

    /// Revert to DHCP
    async fn apply_dhcp(&self, interface: &str) -> Result<(), NetworkError>;

    /// Get current network state
    async fn current_state(&self, interface: &str) -> Result<NetworkState, NetworkError>;
}

/// What we need to apply a static IP
pub struct StaticIpApply {
    pub interface: String,
    pub address: Ipv4Addr,
    pub prefix_length: u8,
    pub gateway: Ipv4Addr,
    pub dns: Vec<Ipv4Addr>,
}
```

### Existing Code Reuse

| Need | Existing Code | Location |
|------|---------------|----------|
| IP detection | `get_local_ip()`, `get_local_ip_and_mac()` | `common/src/infra/network.rs` |
| IP change events | `NetworkMonitor`, `NetworkEvent` | `moss/src/tasks/network_monitor.rs` |
| Console events | `ConsolePrinter`, event patterns | `common/src/console/` |
| Config loading | `MossConfig`, TOML parsing | `moss/src/infra/config.rs` |
| Subsystem flags | `SubSystems.network.ready` | `moss/src/app_state.rs` |
| Platform detection | `std::env::consts::OS` | stdlib |
| Privilege check | `nix::unistd::geteuid()` | Already in dependencies |

## Configuration Schema

### Moss TOML (`garden-moss.toml`)

```toml
# Static IP pool configuration
# Moss will select from this range when an offering requests static IP
[network.static_ip]
enabled = true
pool_start = "192.168.1.240"
pool_end = "192.168.1.250"
gateway = "192.168.1.1"
dns = ["8.8.8.8", "1.1.1.1"]
# interface = "eth0"  # Optional: auto-detect if omitted
```

**Rust Types:**

```rust
// In src/moss/src/infra/config.rs
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct NetworkConfig {
    #[serde(default)]
    pub static_ip: Option<StaticIpPoolConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StaticIpPoolConfig {
    pub enabled: bool,
    pub pool_start: Ipv4Addr,
    pub pool_end: Ipv4Addr,
    pub gateway: Ipv4Addr,
    pub dns: Vec<Ipv4Addr>,
    #[serde(default)]
    pub interface: Option<String>,
}
```

### Offering Manifest Extension

```yaml
# pihole.snippet.yaml
name: pihole
category: networking

# Network requirements (NEW)
network:
  # Static IP preference
  static_ip: preferred  # "none" | "preferred" | "required"
  static_ip_reason: "DNS servers need stable addresses to prevent network outages"
```

**Rust Types (in `common/src/manifests/sw.rs`):**

```rust
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct NetworkRequirements {
    /// Static IP preference: "none", "preferred", "required"
    #[serde(default)]
    pub static_ip: StaticIpPreference,

    /// Human-readable reason shown during installation
    #[serde(default)]
    pub static_ip_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StaticIpPreference {
    #[default]
    None,
    Preferred,
    Required,
}
```

### Network State File (`/etc/zen-garden/network-state.json`)

```json
{
  "version": 1,
  "mode": "static",
  "requested_by": ["pihole"],
  "desired": {
    "address": "192.168.1.245",
    "prefix_length": 24,
    "gateway": "192.168.1.1",
    "dns": ["8.8.8.8", "1.1.1.1"],
    "interface": "eth0"
  },
  "active": {
    "address": "192.168.1.245",
    "obtained_via": "static",
    "applied_at": "2026-02-01T22:00:00Z"
  }
}
```

## User Experience Flow

### Happy Path: `rake offer pihole`

```
$ rake offer pihole

  Pi-hole - Network-wide ad blocking

  ⚠ Network Configuration Required

  Pi-hole works best with a static IP address to prevent DNS
  outages when DHCP leases renew.

  Moss will:
  • Assign static IP 192.168.1.245 to this stone
  • Configure gateway 192.168.1.1 and DNS 8.8.8.8
  • Fall back to DHCP automatically if conflicts detected

  [1] Accept and continue (Recommended)
  [2] Skip static IP - I'll configure my router instead
  [3] Cancel installation

> 1

  [Network]  PROBING     Checking 192.168.1.245 for conflicts...
  [Network]  AVAILABLE   No conflicts detected
  [Network]  APPLYING    Configuring static IP via netplan...
  [Network]  APPLIED     Static IP 192.168.1.245 active

  [Install]  PULLING     pihole/pihole:latest
  [Install]  CREATING    Container pihole
  [Install]  STARTED     Pi-hole running on 192.168.1.245:53

  ✓ Pi-hole installed successfully

  Admin panel: http://192.168.1.245:80/admin
  DNS server:  192.168.1.245:53
```

### Conflict Detection Path

```
  [Network]  PROBING     Checking 192.168.1.245 for conflicts...
  [Network]  CONFLICT    192.168.1.245 in use (ARP reply from aa:bb:cc:dd:ee:ff)
  [Network]  PROBING     Checking 192.168.1.246 for conflicts...
  [Network]  AVAILABLE   No conflicts detected
  [Network]  APPLYING    Configuring static IP via netplan...
  [Network]  APPLIED     Static IP 192.168.1.246 active
```

### Fallback Path

```
  [Network]  PROBING     Checking pool 192.168.1.240-250...
  [Network]  EXHAUSTED   All IPs in pool have conflicts
  [Network]  FALLBACK    Continuing with DHCP (192.168.1.103)
  [Network]  WARNING     Static IP unavailable - configure DHCP reservation recommended

  Proceeding with installation...
```

## Implementation Plan

### Phase 1: Foundation (MVP)

**Scope:** Linux netplan only, ARP probe, manual config

1. **Domain types** (`domain/network.rs`)
   - `NetworkMode`, `StaticIpRequest`, `StaticIpSeverity`
   - Constants in `common/src/constants/network.rs`

2. **Config schema** (`infra/config.rs`)
   - `NetworkConfig`, `StaticIpPoolConfig`
   - TOML parsing integration

3. **ARP probe** (`infra/network/probe.rs`)
   - RFC 5227 conflict detection
   - Fallback to ICMP ping

4. **Netplan adapter** (`infra/network/linux.rs`)
   - `LinuxNetplan` implementing `NetworkPlatform`
   - YAML generation, `netplan apply`

5. **State persistence** (`infra/network/state.rs`)
   - JSON state file read/write
   - Desired vs active tracking

6. **Bootstrap integration** (`bootstrap/run.rs`)
   - Phase 1.7: Apply static IP if configured
   - Fallback to DHCP on failure

**Deliverables:**
- Static IP works via `garden-moss.toml` config
- ARP-based conflict detection
- DHCP fallback on any failure
- Console events for visibility

### Phase 2: Offering Integration

**Scope:** Manifest `network.static_ip`, installation flow

1. **Manifest extension** (`common/src/manifests/sw.rs`)
   - `NetworkRequirements` struct
   - `StaticIpPreference` enum

2. **Installation hook** (`tasks/job_executors.rs`)
   - Check offering's `network.static_ip`
   - Prompt user if `preferred` or `required`
   - Apply static IP before container creation

3. **Rake integration** (`rake/src/commands/adoption/adopt.rs`)
   - Display static IP prompt
   - Pass user choice to Moss API

**Deliverables:**
- `rake offer pihole` prompts for static IP
- Automatic IP assignment from pool
- Offering-specific messaging

### Phase 3: API & Observability

**Scope:** REST API, SSE events, status visibility

1. **API endpoints** (`api/v1/network.rs`)
   - `GET /api/v1/network/status`
   - `POST /api/v1/network/static-ip/apply`
   - `POST /api/v1/network/dhcp/revert`

2. **SSE events** (`domain/events.rs`, `common/src/presence/event_types.rs`)
   - `network.static_ip.applied`
   - `network.static_ip.conflict`
   - `network.static_ip.fallback`

3. **Portrait integration** (`api/v1/portrait.rs`)
   - Include network mode in portrait response

### Phase 4: Multi-Platform (Future)

**Scope:** Windows, NetworkManager, advanced features

1. Windows PowerShell adapter
2. Linux NetworkManager adapter
3. Garden-wide IP pool coordination via chirp
4. CLI: `rake network status`, `rake network configure`

## Conflict Detection Algorithm

### Individual IP Probing

```rust
/// Probe an IP for conflicts using ARP (primary) and ICMP (fallback)
async fn probe_ip_conflict(
    ip: Ipv4Addr,
    interface: &str,
    config: &ProbeConfig,
) -> ProbeResult {
    // 1. Check local bindings first (fast)
    if is_ip_bound_locally(ip) {
        return ProbeResult::LocalConflict;
    }

    // 2. ARP Probe (RFC 5227) - Linux only
    // Send ARP request with sender IP = 0.0.0.0
    // If we get a reply, IP is in use
    #[cfg(target_os = "linux")]
    if let Ok(Some(mac)) = arp_probe(ip, interface, config).await {
        return ProbeResult::Conflict {
            method: "arp",
            responder_mac: Some(mac),
        };
    }

    // 3. ICMP Ping (fallback for all platforms)
    if ping_probe(ip, config.ping_timeout).await {
        return ProbeResult::Conflict {
            method: "icmp",
            responder_mac: None,
        };
    }

    ProbeResult::Available
}
```

### Parallel Batched Pool Selection

IPs are probed in **parallel batches of 4** for faster discovery while avoiding network flooding:

```rust
const PROBE_BATCH_SIZE: usize = 4;

/// Select an IP from the pool using parallel batched probing
async fn select_ip_from_pool(
    config: &StaticIpPoolConfig,
    interface: &str,
) -> Result<Ipv4Addr, PoolExhausted> {
    let all_ips: Vec<Ipv4Addr> = config.iter().collect();

    // Process in batches of 4
    for batch in all_ips.chunks(PROBE_BATCH_SIZE) {
        // Spawn all probes in batch concurrently
        let probe_futures: Vec<_> = batch.iter().map(|&ip| {
            async move { (ip, probe_ip_conflict(ip, interface, &config).await) }
        }).collect();

        // Wait for all probes in batch
        let results = futures_util::future::join_all(probe_futures).await;

        // Return lowest available IP from batch
        if let Some(ip) = results.iter()
            .filter(|(_, r)| matches!(r, ProbeResult::Available))
            .map(|(ip, _)| *ip)
            .min()
        {
            return Ok(ip);
        }
    }

    Err(PoolExhausted { ... })
}
```

**Performance:** For a pool of 11 IPs (`.240-.250`):
- Sequential: Up to 11 × 2s = 22 seconds worst case
- Batched (4): Up to 3 batches × 2s = 6 seconds worst case
- Best case: First batch finds available IP in ~2 seconds

## Offering-Bound Lifecycle

Static IP assignment is **tied to offerings that request it**, not to the stone itself. This ensures the system is self-healing and doesn't leave orphaned configurations.

### State Model

```rust
/// Persistent static IP state with reference tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticIpState {
    pub mode: NetworkMode,
    /// Offerings currently using the static IP (reference counting)
    pub requested_by: Vec<String>,  // ["pihole", "bind9", ...]
    pub desired: Option<StaticIpDesired>,
    pub active: Option<StaticIpActive>,
}
```

### Lifecycle Rules

1. **First Request:** When first offering with `static_ip: required/preferred` (accepted) is installed:
   - Probe and allocate IP from pool
   - Apply static configuration
   - Add offering to `requested_by`

2. **Additional Requests:** When another offering requests static IP:
   - Reuse existing static IP (no change to network)
   - Add offering to `requested_by`

3. **Offering Removal:** When an offering is removed:
   - Remove from `requested_by`
   - If `requested_by` becomes empty → **revert to DHCP**
   - Clean up netplan config file

4. **Upgrade/Reinstall:** Offering upgrade preserves static IP (no change to `requested_by`)

### Example Flow

```
# Initial state
requested_by: []
mode: Dhcp

# Install pihole (requests static IP)
requested_by: ["pihole"]
mode: Static { address: 192.168.1.245 }

# Install bind9 (also requests static IP)
requested_by: ["pihole", "bind9"]
mode: Static { address: 192.168.1.245 }  # Same IP, no network change

# Remove pihole
requested_by: ["bind9"]
mode: Static { address: 192.168.1.245 }  # Still have a requester

# Remove bind9
requested_by: []
mode: Dhcp  # ← Automatic revert!
```

### State File Update

```json
{
  "version": 1,
  "mode": "static",
  "requested_by": ["pihole", "bind9"],
  "desired": {
    "address": "192.168.1.245",
    "prefix_length": 24,
    "gateway": "192.168.1.1",
    "dns": ["8.8.8.8", "1.1.1.1"],
    "interface": "eth0"
  },
  "active": {
    "address": "192.168.1.245",
    "obtained_via": "static",
    "applied_at": "2026-02-01T22:00:00Z"
  }
}
```

### Console Output on Revert

```
  [Network]  RELEASING   Last static IP requester (pihole) removed
  [Network]  REVERTING   Removing static IP configuration...
  [Network]  APPLIED     Reverted to DHCP (acquired 192.168.1.103)
```

## Safety Guarantees

1. **Never remove working config:** We add a new config file, never modify existing network config
2. **Atomic operations:** Use netplan's atomic apply; rollback on failure
3. **DHCP always available:** If static fails, DHCP continues working
4. **Preserve desired state:** Even on fallback, we record what was wanted for debugging
5. **Privilege check:** Verify root/CAP_NET_ADMIN before attempting changes
6. **Timeout all probes:** Never hang waiting for network responses
7. **Reference counting:** Static IP only reverts when ALL requesters are removed

## Constants & Tunables

```rust
// src/common/src/constants/network.rs

/// ARP probe timeout (RFC 5227 recommends 1s, we use 2s for reliability)
pub const ARP_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// ICMP ping timeout
pub const PING_PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// Number of ARP probes to send (RFC 5227 recommends 3)
pub const ARP_PROBE_COUNT: u32 = 3;

/// Delay between ARP probes
pub const ARP_PROBE_INTERVAL: Duration = Duration::from_millis(500);

/// Network state file path
pub const NETWORK_STATE_PATH: &str = "/etc/zen-garden/network-state.json";

/// Default static IP pool suffix (last octet range)
pub const DEFAULT_POOL_START_SUFFIX: u8 = 240;
pub const DEFAULT_POOL_END_SUFFIX: u8 = 250;
```

## File Changes Summary

| File | Change |
|------|--------|
| `src/common/src/constants/mod.rs` | Add `network` module |
| `src/common/src/constants/network.rs` | NEW: Network constants |
| `src/common/src/manifests/sw.rs` | Add `NetworkRequirements` |
| `src/moss/src/domain/mod.rs` | Add `network` module |
| `src/moss/src/domain/network.rs` | NEW: Domain types |
| `src/moss/src/infra/config.rs` | Add `NetworkConfig` |
| `src/moss/src/infra/network/mod.rs` | NEW: Platform trait, detection |
| `src/moss/src/infra/network/probe.rs` | NEW: ARP/ICMP probing |
| `src/moss/src/infra/network/linux.rs` | NEW: Netplan adapter |
| `src/moss/src/infra/network/state.rs` | NEW: State persistence |
| `src/moss/src/bootstrap/run.rs` | Add Phase 1.7 |
| `src/moss/embedded/manifests/sw/networking/pihole.snippet.yaml` | Add `network` section |

## Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Machine becomes unreachable | Low | Critical | DHCP fallback always enabled |
| IP conflict not detected | Medium | High | Multiple probe methods (ARP + ICMP) |
| Netplan not available | Medium | Medium | Detect and skip; log clearly |
| Privilege denied | Low | Medium | Check before attempting; clear error |
| Pool exhausted | Low | Medium | Clear message; suggest expanding pool |

## Success Metrics

1. **Zero network outages** from static IP operations (DHCP fallback works)
2. **< 5 seconds** to probe and apply static IP
3. **Single command** (`rake offer pihole`) completes full setup
4. **Clear console output** at each step

## Open Questions

1. Should we support IPv6 static addressing? (Recommendation: YAGNI for v1)
2. Should pool config be per-offering or global? (Recommendation: Global pool, simpler)
3. Should we auto-detect gateway/DNS from current DHCP? (Recommendation: Yes, as defaults)

## Implementation Notes

### Implemented Features

| Feature | Status | Location |
|---------|--------|----------|
| Domain types | ✅ | `src/moss/src/domain/network.rs` |
| Config schema | ✅ | `src/moss/src/infra/config.rs` |
| ARP/ICMP probing | ✅ | `src/moss/src/infra/network/probe.rs` |
| Linux netplan adapter | ✅ | `src/moss/src/infra/network/linux.rs` |
| State persistence | ✅ | `src/moss/src/infra/network/state.rs` |
| Manifest `network.static_ip` | ✅ | `src/common/src/manifests/sw.rs` |
| Install hook | ✅ | `src/moss/src/tasks/job_executors.rs` |
| Remove hook | ✅ | `src/moss/src/api/v1/services.rs` |
| Parallel batched probing | ✅ | `src/moss/src/infra/network/mod.rs` |

### Key Behaviors

1. **Install Flow:**
   - Checks `compiled.network.wants_static_ip()`
   - If pool configured → probes in parallel batches, applies first available
   - If `required` and fails → installation aborts
   - If `preferred` and fails → installation continues with warning

2. **Remove Flow:**
   - Checks if offering was in `requested_by`
   - Calls `revert_to_dhcp()` which decrements reference count
   - If last requester → reverts to DHCP automatically

3. **Hostname Resilience:**
   - mDNS (`.local`) hostnames survive IP changes automatically
   - Garden P2P chirps update topology with new IP
   - Direct IP connections break on change

### Files Changed

```
src/common/src/manifests/sw.rs          # NetworkRequirements, StaticIpPreference
src/common/src/manifests/mod.rs         # Re-exports
src/moss/src/domain/network.rs          # NetworkMode, StaticIpState, ProbeResult
src/moss/src/domain/mod.rs              # Module declaration
src/moss/src/infra/config.rs            # NetworkConfig, StaticIpPoolConfig
src/moss/src/infra/network/mod.rs       # NetworkPlatform trait, select_ip_from_pool
src/moss/src/infra/network/probe.rs     # probe_ip_conflict, ARP/ICMP
src/moss/src/infra/network/linux.rs     # LinuxNetplan adapter
src/moss/src/infra/network/state.rs     # load/save_network_state
src/moss/src/tasks/job_executors.rs     # Install hook
src/moss/src/api/v1/services.rs         # Remove hooks (delete + destroy)
src/moss/embedded/manifests/sw/networking/pihole.snippet.yaml  # static_ip: preferred
```

## Appendix: Netplan Configuration Example

```yaml
# /etc/netplan/99-zen-garden-static.yaml
# Managed by zen-garden - do not edit manually
network:
  version: 2
  renderer: networkd
  ethernets:
    eth0:
      addresses:
        - 192.168.1.245/24
      routes:
        - to: default
          via: 192.168.1.1
      nameservers:
        addresses:
          - 8.8.8.8
          - 1.1.1.1
```
