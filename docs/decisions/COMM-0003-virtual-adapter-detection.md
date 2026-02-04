# COMM-0003: MAC OUI-Based Virtual Adapter Detection

**Status**: Accepted
**Date**: 2026-02-04
**Deciders**: Architecture Team
**Related**: COMM-0001 (P2P Transport Singleton), COMM-0002 (P2P Pipeline Spec)

---

## Context

### Problem

The P2P discovery mechanism was failing to discover stones on the network. Investigation revealed that the interface selection logic was binding to a Hyper-V virtual adapter (`192.168.224.1`) instead of the physical LAN interface (`192.168.1.x`).

**Symptom**:
```
🔍 Discovering stones on network (timeout: 5s)...
   Binding to LAN interface: 192.168.224.1     ← Wrong! This is Hyper-V
   Sent discovery: multicast 130 bytes + broadcast 130 bytes
   Discovery complete: Found 0 stone(s)
```

### Root Cause

The original `is_virtual_interface()` function used two approaches:

1. **Interface name pattern matching** - Checked for patterns like "docker", "veth", "hyperv"
2. **IP range blocklist** - Hardcoded ranges like `172.17.x.x`, `192.168.224.x`

Both approaches are **brittle**:

| Approach | Problem |
|----------|---------|
| Name patterns | Windows Hyper-V adapters named `vEthernet (Default Switch)` don't match "hyperv" |
| IP ranges | Virtual adapter IP ranges vary by product version and user configuration |

### Affected Components

| Component | File | Issue |
|-----------|------|-------|
| Rust P2P transport | `common/src/infra/communications/p2p.rs` | Selected wrong interface |
| PowerShell push2all | `installer/push2all.ps1` | Selected wrong interface |

## Decision

**Replace IP-range blocklisting with MAC OUI (Organizationally Unique Identifier) detection.**

MAC addresses have a 3-byte prefix (OUI) assigned by IEEE to each vendor. Virtual adapter vendors have registered OUIs that don't change across software versions:

| OUI Prefix | Vendor |
|------------|--------|
| `00:15:5D` | Microsoft Hyper-V |
| `00:50:56` | VMware (VMs) |
| `00:0C:29` | VMware (VMs alternate) |
| `00:05:69` | VMware (legacy) |
| `08:00:27` | VirtualBox |
| `00:1C:42` | Parallels |
| `52:54:00` | QEMU/KVM |
| `00:16:3E` | Xen |
| `00:03:FF` | Microsoft Virtual PC |
| `02:42:xx` | Docker containers (locally administered) |
| `12:xx:xx` | Windows Hyper-V derived addresses |

### Detection Hierarchy

```
┌─────────────────────────────────────────────────────────┐
│ 1. MAC OUI Detection (PRIMARY - most reliable)          │
│    Check first 3 bytes against known virtual OUIs       │
├─────────────────────────────────────────────────────────┤
│ 2. Interface Name Patterns (SECONDARY - fallback)       │
│    Match against "veth", "docker", "vethernet", etc.    │
├─────────────────────────────────────────────────────────┤
│ 3. Docker Bridge IP (TERTIARY - legacy compat)          │
│    Only 172.17.x.x retained (stable Docker default)     │
└─────────────────────────────────────────────────────────┘
```

### Implementation

#### Rust (garden-common)

```rust
/// Known virtual adapter MAC OUI prefixes (first 3 bytes)
const VIRTUAL_MAC_OUIS: &[[u8; 3]] = &[
    [0x00, 0x15, 0x5D], // Microsoft Hyper-V
    [0x00, 0x50, 0x56], // VMware
    [0x08, 0x00, 0x27], // VirtualBox
    // ... etc
];

fn is_virtual_mac(mac: &str) -> bool {
    let bytes: Vec<u8> = mac.split([':', '-'])
        .filter_map(|s| u8::from_str_radix(s, 16).ok())
        .collect();

    if bytes.len() < 3 { return false; }

    let oui = [bytes[0], bytes[1], bytes[2]];
    VIRTUAL_MAC_OUIS.contains(&oui)
}
```

#### Dependencies

Switched from `if-addrs` to `network-interface` crate:

| Crate | MAC Address | Interface Type |
|-------|-------------|----------------|
| `if-addrs` (old) | No | No |
| `network-interface` (new) | Yes | No |

#### PowerShell (push2all.ps1)

PowerShell doesn't have easy access to MAC addresses via `Get-NetIPAddress`. Retained IP-range filtering as practical fallback, but improved with priority tiers:

```powershell
# Priority tiers (lower = better)
# 192.168.0-15.x   → Priority 1 (typical home/office LAN)
# 192.168.16-47.x  → Priority 2
# 10.x.x.x         → Priority 2
# 192.168.65-223.x → Priority 3
# Virtual ranges   → Filtered out entirely
```

## Consequences

### Positive

- **Stable detection** - MAC OUIs are IEEE-assigned and don't change
- **Self-documenting** - Each OUI maps to a known vendor
- **Future-proof** - New virtual products need new OUIs (detectable)
- **Cross-platform** - Works on Linux and Windows identically

### Negative

- **New dependency** - Added `network-interface` crate (~25KB)
- **PowerShell limitation** - Can't easily use MAC detection, kept IP filtering

### Neutral

- **Maintainability** - OUI list may need updates for new virtualization products
  - *Mitigation*: Rare, list is comprehensive for current products

## Validation

```bash
# Test on machine with Hyper-V
cargo run --package garden-common --example test_interface_detection

# Expected: Physical LAN selected, Hyper-V filtered
# Enumerated eligible interfaces: ["Ethernet(192.168.1.100)"]
# Skipping virtual interface: vEthernet (Default Switch)(192.168.224.1) mac=00:15:5D:xx:xx:xx
```

## References

- [IEEE OUI Registry](https://standards-oui.ieee.org/)
- [MAC Address Lookup](https://dnschecker.org/mac-lookup.php)
- [network-interface crate](https://crates.io/crates/network-interface)
- [Microsoft Hyper-V MAC Address Assignment](https://learn.microsoft.com/en-us/answers/questions/762444/how-are-virtual-adapter-mac-addresses-assigned)
