# DNS-0001: Serve `.local` Zone from mDNS Cache in Koi DNS

**Status**: Accepted
**Date**: 2026-02-26
**Deciders**: System architect

## Context

Containers running on a stone cannot always resolve `.local` mDNS hostnames. On Linux with systemd-resolved listening on the Docker bridge gateway, `.local` queries are answered via `MulticastDNS=resolve` and forwarded to avahi — this works. On Windows with Docker Desktop, the Hyper-V VM's embedded DNS resolver cannot resolve the **host machine's own** `.local` name from inside containers, though remote stones' `.local` names resolve fine.

This creates a platform-specific failure: orchestrator containers (MongoDB, Ollama) discover their tended stone via mDNS, construct `http://stone-name.local:7185` endpoints, and immediately fail to connect on Windows stones.

Koi DNS already maintains a live mDNS cache (via `MdnsCache` browsing `_dns-sd._udp` meta-queries). This cache contains every discovered service's hostname and IP address. However, Koi DNS only serves its single configured zone (e.g., `.zengarden`) — queries for `.local` names are forwarded to the upstream system resolver, which hits the same platform limitation.

## Decision

Add `.local` as a secondary zone in Koi DNS, served directly from the mDNS hostname-to-IP cache. This makes `.local` resolution available to any container using Koi DNS as its resolver, regardless of platform.

### Design

**Two-zone model:**

| Zone | Source | Purpose |
|------|--------|---------|
| Primary (`.zengarden`) | Static entries, certmesh SANs, mDNS aliases | Application-level DNS names |
| Local (`.local`) | mDNS hostname→IP cache (direct lookup) | Platform-agnostic `.local` resolution |

**Query dispatch (in order):**

1. If query matches primary zone → resolve from snapshot (static + certmesh + mDNS aliases)
2. If query matches `.local` zone → resolve hostname directly from mDNS cache
3. Forward to upstream system resolver

The `.local` zone uses a **direct hostname lookup** against the mDNS cache — no alias transformation, no certmesh integration, no static entries. A query for `stone-azure-pool.local` extracts the hostname `stone-azure-pool`, finds it in the mDNS host map, and returns the IP.

**Configuration:**

```rust
// Enabled by default when mDNS capability is active.
// Can be disabled via KOI_DNS_NO_LOCAL=1 or builder option.
.dns(|cfg| cfg
    .zone("zengarden")
    .local_zone(true)   // default: true when mDNS is available
    .port(5642)
)
```

**Not a full mDNS responder.** Koi DNS answers unicast DNS queries for `.local` names using cached mDNS data. It does not participate in multicast DNS on port 5353 — that remains avahi's (Linux) or Bonjour's (Windows/macOS) responsibility.

## Consequences

### Positive

- Platform-agnostic `.local` resolution for all containers using Koi DNS
- No per-consumer workarounds — orchestrators, Koan apps, and custom containers all benefit
- No new infrastructure — reuses the mDNS cache Koi already maintains
- Graceful degradation — if hostname isn't in cache, query falls through to upstream

### Negative

- Koi DNS becomes a dual-zone server (~150 lines of change)
- `.local` answers depend on mDNS browse latency — a stone must be discovered before it can be resolved
- Serving `.local` from unicast DNS is unconventional (RFC 6762 reserves `.local` for multicast), though this is strictly a local-network optimization for containers

### Neutral

- On Linux with systemd-resolved, `.local` queries may be answered by either resolved (via avahi) or Koi DNS, depending on container DNS configuration. Both return the same data. No conflict.
- The upstream forwarder remains the fallback for `.local` queries not in the mDNS cache.

## Scope

**Koi changes** (koi-dns crate):
- `DnsConfig`: add `local_zone: bool` field
- `DnsCore`: add `local_zone: Option<DnsZone>` field + `resolve_mdns_local()` method
- `DnsHandler`: insert `.local` zone check between primary zone and upstream forwarding
- `records.rs`: expose `mdns_host_ips()` as `pub(crate)`
- `DnsConfigBuilder` (koi-embedded): add `local_zone(bool)` builder method

**Zen Garden changes**:
- Moss bootstrap (`run.rs`): enable `.local_zone(true)` in Koi builder (default)
