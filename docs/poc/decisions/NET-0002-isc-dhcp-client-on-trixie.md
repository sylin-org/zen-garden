---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-05-10
supersedes: [NET-0001]
---

# NET-0002: Pin `isc-dhcp-client` on Debian 13, Keep ifupdown

**Date**: 2026-05-10
**Status**: Accepted
**Supersedes**: [NET-0001](NET-0001-systemd-networkd-host-stack.md)

## Context

DNS-0002 took moss out of the host's DNS chain. The host now needs a working DHCP-client → systemd-resolved bridge to function. NET-0001 chose to switch the host stack to `systemd-networkd`, which integrates with resolved natively.

Field experience with NET-0001 showed:

1. **Live in-place migration is unsafe.** The dhcpcd → networkd cutover loses SSH connectivity if performed against the active interface; even with a planned reboot, the stone often comes up with a different DHCP-assigned IP because networkd's DHCP ClientID differs from dhcpcd's, and many routers key leases by `(MAC, ClientID)`. Stones renumbered repeatedly during validation.

2. **The preseed change wasn't testable in practice.** It was committed but never verified via a fresh stone provision in this work cycle. Burning a USB and reinstalling each stone takes ~10–15 min apiece.

3. **The actual root cause is much smaller than NET-0001 addresses.** On Debian 13 (trixie), `ifupdown` recommends `dhcpcd-base | isc-dhcp-client | dhcpcd5`, and apt picks `dhcpcd-base` first by default. That's a regression from Debian 12, where `isc-dhcp-client` (`dhclient`) was the default. The Debian-shipped integration script `/etc/network/if-up.d/resolved` (from the `systemd-resolved` package) only knows how to read `dhclient`'s state files in `/run/network/isc-dhcp-v4-*`. Against a `dhcpcd`-managed interface it sees no state, calls `resolvectl revert`, and silently wipes any DNS that was pushed — including DNS pushed by a custom dhcpcd hook.

   In short: **ifupdown + dhclient + systemd-resolved works out of the box on Debian. ifupdown + dhcpcd-base + systemd-resolved is broken integration.** The fix is choosing the DHCP client whose state files the existing integration script understands.

## Decision

Pin `isc-dhcp-client` in the Debian preseed and purge `dhcpcd-base` after install. Keep `ifupdown` as the host network manager. Drop all NET-0001 customizations (no `systemd-networkd` switch, no custom `.network` file, no manual unit masking).

### Resulting host network stack

```
┌─────────────────────────────────────────────────────┐
│  app process                                        │
│       ↓ libc / NSS                                  │
│  /etc/resolv.conf  →  127.0.0.53 (resolved stub)    │
│       ↓                                             │
│  systemd-resolved daemon                            │
│       ↑   per-link DNS table                        │
│       │                                             │
│       │ DBus: SetLinkDNSEx (push)                   │
│       │                                             │
│  /etc/network/if-up.d/resolved   ← Debian-stock     │
│       ↑   reads /run/network/isc-dhcp-v4-<iface>    │
│       │                                             │
│  ifupdown                                           │
│       ↑                                             │
│  dhclient (isc-dhcp-client)                         │
│       ↑                                             │
│  DHCP server on the LAN                             │
└─────────────────────────────────────────────────────┘

Avahi handles `.local` mDNS — separate path:
  app → nss-mdns → avahi → multicast
```

Every component is Debian-stock. Zero custom files written by moss or by the preseed beyond a one-line package selection and a one-line purge.

### Preseed delta

```
- d-i pkgsel/include string sudo docker.io docker-compose avahi-daemon systemd-resolved fwupd
+ d-i pkgsel/include string sudo docker.io docker-compose avahi-daemon systemd-resolved isc-dhcp-client fwupd
```

In `late_command`, replace the eight-line NET-0001 block with one line:

```
+ in-target apt-get -y purge dhcpcd-base 2>/dev/null || true;
```

Total preseed delta vs the pre-NET-0001 baseline: **+1 word in `pkgsel/include`, +1 line in `late_command`**.

## Consequences

### Positive

- Zero custom files, hooks, drop-ins, masked units, or `.network` files in the install payload.
- Host DNS chain matches the canonical Debian-12-style stack that worked everywhere for years and is well-understood.
- No SSH-cutover risk during in-place migration of existing stones — the migration is `apt install isc-dhcp-client && apt purge dhcpcd-base && reboot`, all changes staged before reboot.
- DHCP-assigned IPs stay stable across the migration because the client identity (dhclient's RFC-2132 ClientID = MAC by default) is what most routers already key on for the existing leases that dhcpcd inherited.
- `if-up.d/resolved` (from Debian's systemd-resolved package) is an actively maintained integration we don't have to own.

### Negative

- We're depending on a Debian package `isc-dhcp-client` whose upstream (ISC) declared end-of-life. Debian continues to maintain it for trixie's lifetime but it may eventually be removed in a future Debian release. When that happens, NET-0002 will need revisiting (likely toward systemd-networkd at that point — NET-0001's design will be the right reference then).
- IPv6 RA-provided RDNSS is not directly used by ifupdown+dhclient (dhclient is IPv4-only). For IPv6 DNS, kernel RA handling fills in the IPv6 nameservers, or systemd-resolved gets them via netlink. In practice on dual-stack LANs this just works because the IPv4 nameservers from dhclient resolve everything anyway.

### Neutral

- `dhcpcd-base` removal happens via `apt-get purge` in late_command. If the purge fails (e.g., another package depends on it), the install still completes — `|| true` swallows the error, and the system would then have both DHCP clients with `ifupdown` preferring the first one apt-listed. Unlikely but possible.

## Migration

### New stones

The next stone built from the updated preseed gets `isc-dhcp-client` from first boot, no `dhcpcd-base`. No additional steps.

### Existing stones (already installed pre-NET-0002, including the 12 with the workaround hook from this session)

Do this on each stone, in any order, with console access ready:

```bash
# 1. Remove anything we added this session
sudo rm -f /usr/lib/dhcpcd/dhcpcd-hooks/99-systemd-resolved
sudo chmod +x /etc/network/if-up.d/resolved /etc/network/if-down.d/resolved

# 2. Install the right DHCP client and remove the wrong one
sudo apt-get update
sudo apt-get install -y isc-dhcp-client
sudo apt-get -y purge dhcpcd-base

# 3. Reboot to pick up the new client cleanly
sudo reboot
```

After reboot, `dhclient` runs at boot, ifupdown's `if-up.d/resolved` hook reads its state file and pushes DNS to resolved, host DNS works, image pulls work.

If a stone has `systemd-networkd` enabled from a previous attempt at NET-0001 in-place migration (likely only stone-golden-summit), also disable that and remove the `.network` file before the reboot:

```bash
sudo systemctl disable --now systemd-networkd.service
sudo systemctl unmask systemd-networkd.socket 2>/dev/null || true
sudo rm -f /etc/systemd/network/10-wired.network
```

## Scope

- [installer/templates/debian-preseed.template](../../installer/templates/debian-preseed.template) — pin `isc-dhcp-client` in `pkgsel/include`, replace NET-0001's late_command block with a single `apt-get purge dhcpcd-base` line.

No moss code changes.

## References

- [DNS-0002](DNS-0002-remove-zengarden-zone.md) — the decision that took moss out of the host DNS chain and made this gap visible.
- [NET-0001](NET-0001-systemd-networkd-host-stack.md) — superseded; the systemd-networkd path remains a valid future option if isc-dhcp-client is eventually unavailable.
- Debian bug discussions on the dhcpcd-base default in trixie are the upstream context for the regression we hit.
