---
audience: [contributor, ai]
doc_type: decision
status: accepted
last_verified: 2026-02-07
canonical: true
---

# COMM-0004: Multicast-First Discovery Transport

**Status**: Accepted
**Date**: 2026-01-26
**Tags**: discovery, networking, p2p, multicast

---

## Context

Zen Garden used UDP limited broadcast (`255.255.255.255:7184`) for stone discovery. This worked on single-NIC Linux systems but failed on multi-homed Windows 11 hosts with Hyper-V or WSL virtual adapters.

The failure mode: when sending to `255.255.255.255`, the OS routes the packet through its default interface. On Windows with virtual adapters, the default interface is often `vEthernet (WSL)` or `vEthernet (Default Switch)` rather than the physical NIC. The broadcast packet egresses through the virtual adapter and never reaches the physical LAN.

This made stone discovery unreliable on common developer workstations.

---

## Decision

We adopted a multicast-first transport strategy with two fallback tiers:

1. **Primary**: IPv4 multicast to `239.255.42.99:7184` (TTL=1)
2. **Secondary**: Directed broadcast per-interface (compute subnet broadcast from IP + prefix)
3. **Tertiary**: Limited broadcast to `255.255.255.255` (disabled by default)

Senders create one socket per eligible interface, bound to that interface's IP address. This guarantees the packet egresses through the correct NIC regardless of the OS default route.

---

## Rationale

- **Explicit interface control**: Binding to a specific IP eliminates OS routing ambiguity
- **Router-friendly**: Multicast has well-understood semantics, less likely to be blocked than broadcast
- **Cross-platform**: Works reliably on Windows, Linux, macOS including multi-homed configurations
- **No subnet assumptions**: Directed broadcast computed from actual prefix length (supports /16, /20, /24, etc.)
- **TTL=1 security**: Multicast packets cannot leave the local subnet

---

## Consequences

### Positive

- Discovery works reliably on multi-homed Windows (WSL, Hyper-V, VPN)
- Works on any network size (no /24 assumption)
- Configurable via environment variables for edge cases

### Negative

- More packets per announcement: 2N (N interfaces x 2 strategies) vs 1
- Per-interface sockets use slightly more memory (~20KB per interface)
- Some networks block multicast (mitigated by directed broadcast fallback)

### Neutral

- Wire protocol (`UdpAnnouncement` envelope) unchanged
- API (`send_announcement`, `subscribe_to_announcement`) unchanged
- Old stones using limited broadcast can still be discovered if `DISCOVERY_ENABLE_LIMITED_BCAST=true`

---

## Alternatives Considered

### Keep Limited Broadcast + Fix Interface Selection

- **Description**: Try to force `255.255.255.255` packets through the correct NIC using socket options
- **Pros**: Minimal code change
- **Cons**: `SO_BINDTODEVICE` not available on Windows; `IP_MULTICAST_IF` doesn't apply to broadcast
- **Rejected because**: No reliable cross-platform way to control broadcast egress interface

### mDNS Only

- **Description**: Replace custom UDP discovery with pure mDNS (via `mdns-sd` crate)
- **Pros**: Standard protocol, built-in multicast
- **Cons**: Adds dependency, less control over announcement types, heavier protocol
- **Rejected because**: Our announcement types (chirps, elections, beacons) don't map well to mDNS service records

### Unicast Discovery with Known Peers

- **Description**: Maintain a peer list and send discovery unicast to known IPs
- **Pros**: Works through firewalls, no broadcast/multicast needed
- **Cons**: Requires bootstrapping (how to find the first peer?), not zero-configuration
- **Rejected because**: Violates the "discovery over configuration" philosophy

---

## References

- [Discovery Transport spec](../specs/discovery-transport.md) — current-state technical details
- [COMM-0001: P2P Transport Singleton](COMM-0001-p2p-transport-singleton.md) — singleton pattern this builds on
- RFC 2365: Administratively Scoped IP Multicast (`239.255.0.0/16`)
