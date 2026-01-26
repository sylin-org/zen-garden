# Discovery Transport Architecture

**Status**: Active (as of 2026-01-26)  
**Replaces**: Limited broadcast (`255.255.255.255`) discovery

---

## Problem Statement

### Original Behavior

Zen Garden originally used UDP **limited broadcast** to `255.255.255.255:7184` for stone discovery:

```rust
socket.send_to(&packet, "255.255.255.255:7184").await?;
```

Listeners bound to `0.0.0.0:7184` with `SO_REUSEADDR` enabled.

### Observed Failure Mode

**Multi-homed Windows 11 systems** (Hyper-V, WSL, VPN adapters) experienced unreliable discovery:

- **Symptom**: Stones on same LAN couldn't discover each other
- **Root cause**: Limited broadcast (`255.255.255.255`) egresses via the system's **default route interface**
- **Impact**: With WSL/Hyper-V vEthernet adapters active, Windows may route broadcasts through virtual interfaces instead of physical NICs

**Example topology where discovery fails:**

```
Windows 11 Host (192.168.1.100)
├─ Physical NIC: Ethernet (192.168.1.100/24) ← should use this
├─ WSL vEthernet: 172.24.32.1/20            ← Windows picks this!
└─ Hyper-V vEthernet: 172.20.16.1/20

Other Stone (192.168.1.135) ← never receives broadcast
```

The socket sends to `255.255.255.255`, but the OS routes it through `172.24.32.1` (WSL adapter), which never reaches the physical LAN.

---

## New Behavior: Multicast-First + Directed Broadcast Fallback

### Transport Strategy

**Primary: IPv4 Multicast**
- Group: `239.255.42.99` (configurable via `DISCOVERY_MCAST_GROUP`)
- Port: `7184` (configurable via `DISCOVERY_PORT`)
- TTL: `1` (stay on LAN, don't route)
- Receiver joins multicast group on **all eligible interfaces**

**Secondary: Directed Broadcast (per interface)**
- Compute subnet broadcast from interface IP + prefix
  - Example: `192.168.32.10/20` → `192.168.47.255`
- Send to **each eligible interface's broadcast address**
- Never assumes `/24` networks

**Tertiary: Limited Broadcast (legacy, disabled by default)**
- `255.255.255.255` send as last resort
- Controlled by `DISCOVERY_ENABLE_LIMITED_BCAST=true`

### Why Multicast?

1. **Explicit interface targeting**: Join operations specify which NIC to listen on
2. **Router-friendly**: Well-understood semantics, less likely to be blocked than broadcast
3. **Cross-platform**: Works reliably on Windows, Linux, macOS
4. **No subnet assumptions**: Works on any network size

### Why Directed Broadcast as Fallback?

1. **Multicast filtering**: Some networks block multicast (corporate firewalls, guest networks)
2. **Explicit interface binding**: Sender binds to `<iface_ip>:0`, ensuring packet egresses correct NIC
3. **Subnet-aware**: Computes correct broadcast even on `/20`, `/16`, etc.

### Why NOT 255.255.255.255?

1. **Routing ambiguity**: OS decides interface; unpredictable on multi-homed systems
2. **Disabled on many networks**: Enterprise firewalls, VLANs, cloud providers
3. **Unnecessary**: Directed broadcast achieves same result with explicit control

---

## Multicast Group Selection

**Chosen**: `239.255.42.99`

**Rationale**:
- Range `239.255.0.0/16` is **Organization-Local Scope** (RFC 2365)
- Not used by any IANA-registered services
- Chosen arbitrarily within local-admin space
- `.42.99` has no special meaning (avoids collision with common choices like `.1.1`, `.255.255`)

**Port**: `7184`
- Already established in Zen Garden ecosystem
- No conflicts with IANA-registered services

**TTL**: `1`
- Prevents multicast from leaving local subnet
- Security: limits discovery to LAN (can't leak across VPN/Internet)
- Performance: reduces unnecessary traffic

---

## Interface Selection Heuristics

**Eligibility criteria** (all must pass):

```rust
fn is_eligible_interface(iface: &Interface) -> bool {
    iface.is_up()                    // Interface running
    && !iface.is_loopback()          // Not 127.0.0.1
    && iface.has_ipv4()              // Has IPv4 address
    && !is_likely_virtual(iface)     // Deprioritize VPN/VM adapters
}
```

**Virtual adapter heuristics** (deprioritize, don't exclude):

- Name contains: `vEthernet`, `WSL`, `VirtualBox`, `VMware`, `Hyper-V`, `tun`, `tap`
- MAC prefix: `00:05:69`, `00:0c:29`, `00:50:56` (VMware), `00:15:5d` (Hyper-V)
- Purpose: Prefer physical NICs, but still send on virtual if no physical available

**Default gateway preference**:
- Interfaces with default route are **preferred** but not required
- Handles multi-subnet LANs (e.g., home network + IoT VLAN)

**Non-/24 network support**:
- Interface reports IP + prefix length (e.g., `192.168.32.10/20`)
- Computes broadcast: `(ip & mask) | ~mask`
- Example: `192.168.32.10/20` → `192.168.47.255` (not `.32.255`!)

---

## Implementation Details

### Sender Strategy (per announcement)

```rust
for interface in eligible_interfaces() {
    let socket = bind_to_interface(interface.ip);
    
    // 1. Multicast send
    socket.set_multicast_ttl_v4(1)?;
    socket.send_to(&packet, (MCAST_GROUP, PORT)).await?;
    
    // 2. Directed broadcast send (if fallback enabled)
    if ENABLE_BCAST_FALLBACK {
        let bcast_addr = compute_broadcast(interface.ip, interface.prefix);
        socket.set_broadcast(true)?;
        socket.send_to(&packet, (bcast_addr, PORT)).await?;
    }
}
```

**Key points**:
- Each interface gets its own socket bound to `<iface_ip>:0`
- OS guarantees packet egresses through that interface
- Multicast TTL=1 prevents routing
- Directed broadcast computed per interface (no `/24` assumption)

### Receiver Strategy (on startup)

```rust
let socket = UdpSocket::bind("0.0.0.0:7184").await?;
socket.set_reuse_address(true)?;

for interface in eligible_interfaces() {
    socket.join_multicast_v4(
        MCAST_GROUP.parse()?,
        interface.ip.parse()?
    )?;
}

// Now receives:
// - Multicast packets to 239.255.42.99:7184 on all joined interfaces
// - Directed broadcast to <subnet>.255:7184 on all interfaces
// - Unicast replies to specific IPs
```

**Key points**:
- Single receiver socket (singleton pattern preserved)
- Joins multicast group **per interface** (not just one)
- Receives both multicast and broadcast packets

---

## Configuration

**Environment Variables**:

| Variable | Default | Purpose |
|----------|---------|---------|
| `DISCOVERY_PORT` | `7184` | UDP port for discovery |
| `DISCOVERY_MCAST_GROUP` | `239.255.42.99` | IPv4 multicast group address |
| `DISCOVERY_ENABLE_BCAST_FALLBACK` | `true` | Enable directed broadcast per interface |
| `DISCOVERY_ENABLE_LIMITED_BCAST` | `false` | Enable legacy 255.255.255.255 (not recommended) |

**Usage examples**:

```bash
# Use different multicast group (avoid conflicts)
export DISCOVERY_MCAST_GROUP=239.255.100.1

# Disable broadcast fallback (multicast-only)
export DISCOVERY_ENABLE_BCAST_FALLBACK=false

# Enable legacy limited broadcast (if network blocks multicast AND directed)
export DISCOVERY_ENABLE_LIMITED_BCAST=true
```

---

## Security & Abuse Considerations

### Threat Model

**In-scope threats**:
1. Malicious announcements on LAN
2. Replay attacks
3. Packet injection (man-in-middle)
4. Denial of service (flood)

**Out-of-scope** (handled elsewhere or accepted risk):
- Authentication (no secrets in v0.1.0)
- Encryption (plaintext UDP is design choice for observability)
- Cross-subnet attacks (TTL=1 prevents)

### Mitigations

**Rate limiting** (future):
- Receiver deduplicates by `(stone_id, announcement_type, epoch)`
- Implement per-source rate limit (e.g., max 100 packets/second per IP)

**Validation**:
- Envelope validation: must be valid `UdpAnnouncement` JSON
- Type filtering: subscribers only receive registered types
- Size limits: reject packets >4KB (current buffer size)

**Replay protection** (future):
- Add monotonic sequence number or timestamp to announcements
- Reject stale announcements (>30s old)

**Reply storm prevention**:
- Discovery responses are unicast (not broadcast)
- Chirps are periodic (30s interval), not triggered by requests
- Debouncing batches rapid status changes (prevents flood)

### Physical Security

**TTL=1 enforcement**:
- Multicast packets cannot leave local subnet
- Prevents discovery leaking across VPN, WAN, Internet
- Attackers must be on same physical/virtual LAN

**Broadcast domain isolation**:
- Respects VLAN boundaries
- AP "client isolation" still blocks peer-to-peer
- Requires proper network configuration by operator

---

## Troubleshooting

### Symptom: No stones discovered

**Check 1: Multicast blocked?**

Test multicast:
```bash
# Sender (stone A)
echo "test" | nc -u 239.255.42.99 7184

# Receiver (stone B)
sudo tcpdump -i eth0 'host 239.255.42.99'
```

If blocked, ensure `DISCOVERY_ENABLE_BCAST_FALLBACK=true`.

**Check 2: Firewall rules?**

Windows:
```powershell
New-NetFirewallRule -DisplayName "Zen Garden Discovery" `
    -Direction Inbound -Action Allow -Protocol UDP -LocalPort 7184
```

Linux (iptables):
```bash
iptables -A INPUT -p udp --dport 7184 -j ACCEPT
```

**Check 3: Interface selection**

Check which interfaces Moss detected:
```bash
# Look for "P2P transport: detected interfaces" in logs
garden-moss 2>&1 | grep "detected interfaces"
```

Verify physical NIC is included (not just WSL/Hyper-V).

**Check 4: Network topology**

- **AP client isolation**: Wireless Access Point blocking peer-to-peer (common on guest networks)
- **VLAN boundaries**: Stones on different VLANs can't discover each other (by design)
- **Subnet mismatch**: Stone A on `192.168.1.0/24`, Stone B on `192.168.2.0/24` → need router with multicast forwarding

### Symptom: Discovery works but slow

**Likely cause**: Multicast blocked, falling back to directed broadcast

Check logs for "multicast send failed, trying broadcast" warnings.

**Solution**: Configure network to allow multicast `239.255.42.99` or accept slower broadcast-only discovery.

### Symptom: Works on Linux, fails on Windows

**Check 1: Windows Hyper-V/WSL interference**

Disable unused adapters:
```powershell
Get-NetAdapter | Where-Object {$_.InterfaceDescription -match "vEthernet|WSL"} | Disable-NetAdapter
```

Or verify Moss is sending on physical NIC (check logs).

**Check 2: Windows Firewall**

Ensure UDP 7184 allowed in **both** "Private" and "Public" network profiles.

**Check 3: WSL2 NAT mode**

WSL2 uses NAT, not bridged networking. Stones inside WSL2 require special configuration (port forwarding or bridged mode).

---

## Migration from Legacy Broadcast

**No action required** - discovery transport is internal implementation detail.

**Backwards compatibility**:
- Announcement payloads unchanged
- Envelope format (`UdpAnnouncement`) unchanged
- API (`send_announcement`, `subscribe_to_announcement`) unchanged
- Debouncing behavior unchanged

**Coexistence**:
- Old stones (255.255.255.255 only) can discover new stones (multicast)
- New stones (multicast) CANNOT discover old stones (unless `DISCOVERY_ENABLE_LIMITED_BCAST=true`)

**Recommendation**: Upgrade all stones simultaneously during maintenance window for best results.

---

## References

- **RFC 2365**: Administratively Scoped IP Multicast (239.255.0.0/16 organization-local scope)
- **RFC 3171**: IANA Guidelines for IPv4 Multicast Address Assignments
- **COMM-0001**: P2P Transport Singleton (Zen Garden ADR)
- **Windows Sockets 2**: UDP broadcast behavior on multi-homed systems

---

## Changelog

| Date | Change | Rationale |
|------|--------|-----------|
| 2026-01-26 | Initial multicast-first design | Solve Windows 11 multi-homing discovery failures |
| 2026-01-26 | Add directed broadcast fallback | Handle networks that block multicast |
| 2026-01-26 | Deprecate limited broadcast | Unreliable on multi-homed systems |
