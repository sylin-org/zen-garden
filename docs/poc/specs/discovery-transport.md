---
audience: [developer, contributor]
doc_type: spec
status: current
last_verified: 2026-02-07
---

# Discovery Transport Specification

Zen Garden uses a multicast-first UDP transport for stone discovery, with directed broadcast and limited broadcast as fallbacks.

---

## Transport Strategy

Discovery sends announcements through a tiered strategy:

1. **Primary — IPv4 Multicast**
   - Group: `239.255.42.99` (Organization-Local Scope, RFC 2365)
   - Port: `7184`
   - TTL: `1` (LAN-only, cannot route beyond subnet)
   - Receiver joins the multicast group on all eligible interfaces

2. **Secondary — Directed Broadcast (per interface)**
   - Computes subnet broadcast from interface IP + prefix length
   - Example: `192.168.32.10/20` produces broadcast `192.168.47.255`
   - Sends to each eligible interface's broadcast address
   - Never assumes `/24` networks

3. **Tertiary — Limited Broadcast (disabled by default)**
   - Sends to `255.255.255.255`
   - Unreliable on multi-homed systems (OS chooses the egress interface)
   - Controlled by `DISCOVERY_ENABLE_LIMITED_BCAST=true`

---

## Multicast Group

| Property | Value | Notes |
|----------|-------|-------|
| Address | `239.255.42.99` | Organization-Local Scope (RFC 2365) |
| Port | `7184` | Shared with all discovery transport methods |
| TTL | `1` | Prevents multicast from leaving local subnet |

The `239.255.0.0/16` range is reserved for organization-local use and does not require IANA registration.

---

## Interface Selection

### Eligibility Criteria

An interface is eligible for discovery if all conditions pass:

```rust
fn is_eligible_interface(iface: &Interface) -> bool {
    iface.is_up()
    && !iface.is_loopback()
    && iface.has_ipv4()
    && !is_likely_virtual(iface)
}
```

### Virtual Adapter Detection

Virtual adapters are deprioritized (not excluded) using:

- **MAC OUI prefixes**: Hyper-V (`00:15:5D`), VMware (`00:50:56`), VirtualBox (`08:00:27`), Docker (`02:42`), QEMU/KVM (`52:54:00`), Xen (`00:16:3E`)
- **Name patterns**: `vEthernet`, `WSL`, `VirtualBox`, `VMware`, `Hyper-V`, `tun`, `tap`, `veth`, `docker`, `br-`
- **IP ranges**: Docker bridge `172.17.x.x`

Physical NICs are preferred, but virtual interfaces still participate if no physical interface is available.

### Default Gateway Preference

Interfaces with a default route are preferred but not required, supporting multi-subnet LANs (e.g., home network + IoT VLAN).

### Broadcast Computation

Directed broadcast is computed per interface using: `broadcast = ip | ~netmask`

| Network | IP | Netmask | Broadcast |
|---------|-----|---------|-----------|
| /24 | 192.168.1.10 | 255.255.255.0 | 192.168.1.255 |
| /20 | 192.168.32.10 | 255.255.240.0 | 192.168.47.255 |
| /16 | 10.0.5.100 | 255.255.0.0 | 10.0.255.255 |

---

## Sender Architecture

Each announcement is sent through per-interface sockets:

```rust
for interface in eligible_interfaces() {
    let socket = bind_to_interface(interface.ip);

    // 1. Multicast
    socket.set_multicast_ttl_v4(1)?;
    socket.send_to(&packet, (MCAST_GROUP, PORT)).await?;

    // 2. Directed broadcast (if enabled)
    if ENABLE_BCAST_FALLBACK {
        let bcast_addr = compute_broadcast(interface.ip, interface.prefix);
        socket.set_broadcast(true)?;
        socket.send_to(&packet, (bcast_addr, PORT)).await?;
    }
}
```

Each socket binds to a specific interface IP (not `0.0.0.0`), guaranteeing the packet egresses through the correct NIC.

## Receiver Architecture

A single receiver socket handles all transport methods:

```rust
let socket = UdpSocket::bind("0.0.0.0:7184").await?;
socket.set_reuse_address(true)?;

for interface in eligible_interfaces() {
    socket.join_multicast_v4(MCAST_GROUP, interface.ip)?;
}
```

The receiver joins the multicast group on every eligible interface. It receives multicast packets, directed broadcast, and unicast replies through the same socket (singleton pattern per [COMM-0001](../decisions/COMM-0001-p2p-transport-singleton.md)).

---

## Configuration

| Variable | Default | Purpose |
|----------|---------|---------|
| `DISCOVERY_PORT` | `7184` | UDP port for discovery |
| `DISCOVERY_MCAST_GROUP` | `239.255.42.99` | IPv4 multicast group address |
| `DISCOVERY_ENABLE_BCAST_FALLBACK` | `true` | Enable directed broadcast per interface |
| `DISCOVERY_ENABLE_LIMITED_BCAST` | `false` | Enable legacy `255.255.255.255` (not recommended) |

### Configuration Profiles

**Default (recommended)** — no configuration needed:
```bash
# Multicast + directed broadcast. Works on most networks.
```

**Multicast-only (strict)** — disable all broadcast:
```bash
DISCOVERY_ENABLE_BCAST_FALLBACK=false
```

**Legacy compatibility (last resort)** — re-enable limited broadcast:
```bash
DISCOVERY_ENABLE_LIMITED_BCAST=true
# Warning: unreliable on multi-homed systems
```

---

## Security Considerations

### Threat Model

| Threat | Mitigation | Status |
|--------|-----------|--------|
| Malicious announcements on LAN | Envelope validation, size limits (<4KB), type filtering | Implemented |
| Replay attacks | Monotonic sequence/timestamp, reject >30s stale | Planned |
| Packet injection | Pond mTLS (optional layer) | Available |
| DoS via flood | Per-source rate limiting, deduplication by `(stone_id, type, epoch)` | Partial |
| Cross-subnet leakage | TTL=1 prevents multicast routing beyond subnet | Enforced |

### Physical Security

- Multicast TTL=1 prevents discovery from leaking across VPN, WAN, or Internet
- Respects VLAN boundaries
- AP "client isolation" blocks peer-to-peer (requires proper network configuration)

---

## Troubleshooting

### No stones discovered

**Multicast blocked?** Test with:
```bash
# Sender (stone A)
echo "test" | nc -u 239.255.42.99 7184

# Receiver (stone B)
sudo tcpdump -i eth0 'host 239.255.42.99'
```

If blocked, ensure `DISCOVERY_ENABLE_BCAST_FALLBACK=true`.

**Firewall rules?**

Windows:
```powershell
New-NetFirewallRule -DisplayName "Zen Garden Discovery" `
    -Direction Inbound -Action Allow -Protocol UDP -LocalPort 7184
```

Linux:
```bash
iptables -A INPUT -p udp --dport 7184 -j ACCEPT
```

**Wrong interface selected?** Check which interfaces Moss detected:
```bash
garden-moss 2>&1 | grep "detected interfaces"
```

**Network topology?**
- AP client isolation blocks peer-to-peer (common on guest networks)
- Stones on different VLANs cannot discover each other (by design)
- Cross-subnet requires router with multicast forwarding or Lantern registry

### Discovery works but slow

Likely cause: multicast blocked, falling back to directed broadcast. Check logs for "multicast send failed" warnings.

### Works on Linux, fails on Windows

- **Hyper-V/WSL interference**: Verify Moss is sending on physical NIC (check logs). Disable unused virtual adapters if needed.
- **Windows Firewall**: Ensure UDP 7184 is allowed in both "Private" and "Public" profiles.
- **WSL2 NAT mode**: WSL2 uses NAT, not bridged networking. Stones inside WSL2 require port forwarding or bridged mode.

---

## References

- [COMM-0001: P2P Transport Singleton](../decisions/COMM-0001-p2p-transport-singleton.md)
- [COMM-0004: Multicast-First Discovery](../decisions/COMM-0004-multicast-first-discovery.md) — design rationale
- RFC 2365: Administratively Scoped IP Multicast
- RFC 3171: IANA Guidelines for IPv4 Multicast Address Assignments
