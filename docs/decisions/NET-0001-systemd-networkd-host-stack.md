---
audience: [developer, ai]
doc_type: decision
status: accepted
date: 2026-05-10
depends_on: [DNS-0002]
---

# NET-0001: Use systemd-networkd as the Stone Host Network Stack

**Date**: 2026-05-10
**Status**: Accepted
**Depends on**: [DNS-0002](DNS-0002-remove-zengarden-zone.md)

## Context

DNS-0002 removed Koi DNS and the moss-managed host DNS chain. With moss out of the picture, the host's DNS-via-resolved chain depends on whatever populates per-link DNS in systemd-resolved.

The current Debian preseed at [installer/templates/debian-preseed.template](../../installer/templates/debian-preseed.template) uses the Debian default of `ifupdown` (the `networking.service` unit) driving an external DHCP client (`dhcpcd` on the field stones we have, sometimes `dhclient`). **Neither dhcpcd nor dhclient natively integrates with systemd-resolved.** They write DHCP-discovered DNS into `/etc/resolv.conf` directly, which on a systemd-resolved system is a symlink to the resolved stub at `127.0.0.53` — and resolved itself never learns about those nameservers. The stub then fails every public lookup.

We hit this exact pathology on `stone-golden-summit` during the DNS-0002 work. The stone's host had a working IP and default route from DHCP, but `getent hosts deb.debian.org` failed, `apt update` failed, and Docker image pulls failed — all because the DHCP-provided DNS server (`192.168.1.1`) was visible to `dhcpcd -U` but never made it into `resolvectl status`.

Before DNS-0002, moss compensated for this by installing its own DNS server (Koi DNS) and routing the host's resolved through it. That moved the gap inside moss instead of fixing it. DNS-0002 explicitly stops moss from playing that role, which makes the gap visible.

### The three options we considered

**(A) `openresolv` package.** Standard Debian-managed package that provides `resolvconf` and a set of "subscribers" that push DHCP DNS into various consumers including systemd-resolved. Looks like a one-line preseed addition.

In practice, `apt install openresolv` triggers a package conflict and **removes systemd-resolved** (both packages claim ownership of `/etc/resolv.conf`). To make them coexist requires explicit configuration of openresolv's `systemd-resolved` subscriber via `/etc/resolvconf.conf`, which on Debian 13 (trixie) ships in a non-standard location. We hit all of these in the live debug session — what looked like a one-line fix needed install ordering, custom config files, and unit overrides.

**(B) Custom dhcpcd hook.** A ~30-line shell script at `/usr/lib/dhcpcd/dhcpcd-hooks/99-systemd-resolved` that translates dhcpcd's `BOUND`/`RENEW` events into `resolvectl dns/default-route` calls. We wrote and tested this in the session — it works, no package conflicts. But it's tied to dhcpcd specifically; switching the stone to NetworkManager or networkd later would silently bypass it, and the stone's DNS would break the way it did before.

**(C) Switch the host stack to `systemd-networkd`.** systemd-networkd is the systemd-native network manager. It integrates with systemd-resolved by design: every DHCP-provided DNS server is automatically registered with resolved on the right link with `Default Route: yes`. **No glue layer.**

## Decision

Switch the Debian preseed to use `systemd-networkd` for stone host networking. Disable `networking.service` (ifupdown) and any `dhcpcd*` units that might be present. Drop a single permissive `.network` file that matches every Ethernet interface and runs DHCPv4+v6.

### Preseed changes

The new `late_command` lines disable the legacy stack, enable the systemd one, and write the network config. The exact shell form is in [installer/templates/debian-preseed.template](../../installer/templates/debian-preseed.template) (uses `printf "%s\n"` instead of a heredoc because preseed `late_command` strings don't interpret `\n` inside single-quoted `sh -c '...'`). The resulting `/etc/systemd/network/10-wired.network` file:

```ini
[Match]
Name=en* eth*
Type=ether

[Network]
DHCP=yes
IPv6AcceptRA=yes
LLMNR=no
MulticastDNS=no
```

Notes on the `.network` config:

- `Name=en* eth*` matches the modern (`enp1s0`, `eno1`) and legacy (`eth0`) interface name conventions Debian assigns.
- `Type=ether` keeps wireless out of scope (intentional — wireless on a stone is an explicit configuration).
- `DHCP=yes` enables both v4 and v6.
- `LLMNR=no` and `MulticastDNS=no` on the link itself disable systemd-networkd's own LLMNR/mDNS — `avahi-daemon` is the mDNS responder per the stack as a whole, and we don't want two daemons answering the same multicast.

### What this gives us

- DHCP-provided DNS appears in `resolvectl status enp1s0` automatically with `Default Route: yes`. Host `getent` works.
- Containers reach this via `bridge_gw → resolved → enp1s0 link DNS`.
- `MulticastDNS=resolve` in moss's resolved drop-in (DNS-0002) still drives `.local` lookups through avahi.
- The host's DNS chain self-configures at boot with no glue layer.

### What stays the same

- `avahi-daemon` is still the mDNS responder.
- moss still drops `MulticastDNS=resolve` + `DNSStubListenerExtra=<bridge_gw>` for container reach.
- Docker, fwupd, sudo, the Zen Garden binaries — unchanged.
- `/etc/network/interfaces` is left in place but ignored (since `networking.service` is disabled). Not deleted — leaving it untouched means a future operator running `systemctl enable networking.service` gets a working fallback.

## Consequences

### Positive

- Eliminates the entire class of "DHCP client doesn't talk to systemd-resolved" failures by removing the gap rather than bridging it.
- Zero glue code in the install payload — no `resolvconf.conf`, no dhcpcd hook script, no openresolv subscriber config.
- Same DNS path that every modern systemd distribution uses; documented and well-supported upstream.
- Works identically across wired interfaces of any naming convention.
- If a stone operator later wants NetworkManager (e.g., for wireless setup), NM also speaks to resolved natively — same outcome.
- Removes a hidden install-order dependency: openresolv would have required careful sequencing to avoid removing systemd-resolved.

### Negative

- Behavioral change for any operator who reaches for `ifup`/`ifdown` or edits `/etc/network/interfaces` by hand. On a Zen Garden stone, no human is doing that, but it's a difference from a stock Debian install.
- `dhcpcd` users on existing stones (those installed pre-NET-0001) still need either an in-place migration or a re-image to get the new behaviour. Documented as a migration path below.

### Neutral

- The two extra `late_command` lines (mask `dhcpcd.service`, write the `.network` file) add complexity to the preseed but are mechanical and idempotent.

## Migration

### New stones

The next stone built from an updated installer image gets systemd-networkd from first boot. No additional steps.

### Existing stones (already installed pre-NET-0001)

Two options, in order of preference:

**Re-image (preferred)**: rebuild the installer USB and reinstall the stone. Cleanest, removes any drift from prior debugging.

**In-place migration**: must be done with a planned reboot, not a live SSH cutover. Attempting to cut over the active interface (e.g., `dhcpcd -k enp1s0` followed by `networkctl reconfigure`) creates a race window where the DHCP lease is released before networkd takes over. Observed in development: the stone either fails to re-acquire (interface ends up with no IP) or acquires a different lease while the router still has ARP cached for the old address — in either case the SSH session and the stone's reachability are lost without console access.

The safe sequence is to *stage* the changes, then reboot:

```bash
# Stage: write config and unit changes only — do not stop the running stack.
sudo tee /etc/systemd/network/10-wired.network <<'EOF'
[Match]
Name=en* eth*
Type=ether

[Network]
DHCP=yes
IPv6AcceptRA=yes
LLMNR=no
MulticastDNS=no
EOF
sudo systemctl disable networking.service
sudo systemctl disable dhcpcd.service 2>/dev/null || true
sudo systemctl mask dhcpcd.service 2>/dev/null || true
sudo systemctl enable systemd-networkd.service
sudo systemctl enable systemd-resolved.service
sudo ln -sf /run/systemd/resolve/stub-resolv.conf /etc/resolv.conf

# Apply: reboot from a clean state. The stone will come up under networkd.
sudo reboot
```

After the reboot, the stone re-DHCPs under networkd. mDNS advertises the hostname again. SSH-via-`.local` works normally.

Stones that previously had any workarounds from DNS-0002 development (custom dhcpcd hook, openresolv install, `resolvectl docker0` runtime state) are cleared by the reboot.

## Scope

- [installer/templates/debian-preseed.template](../../installer/templates/debian-preseed.template): add the `late_command` lines that switch the host stack.

No moss code changes. NET-0001 is purely a provisioning decision.

## References

- [DNS-0002](DNS-0002-remove-zengarden-zone.md) — created the operational gap NET-0001 closes.
- systemd.network(5), systemd-networkd(8), systemd-resolved.service(8) — upstream documentation for the stack chosen.
