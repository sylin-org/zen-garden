# Phone Stone — LAN / discovery live-test runbook

> Working runbook for validating acceptance #3 (discovered by `garden-rake` with
> correct specs) and #5 (appears in garden inspection) on the Pixel 3 XL Stone,
> once a USB-C Ethernet adapter is attached. The adapter takes the single USB-C
> port that `adb` uses, so `adb` must move to TCP/IP first.

Device: `89TY0BAV9` · Wi-Fi is ABI-broken on this build, so the adapter is the only
LAN path. Moss runs natively on `:7185`; the native CLI is `/data/garden-rake`.

## Sequence

1. **Deploy + confirm moss is up (over USB adb), still on USB:**
   ```powershell
   adb -s 89TY0BAV9 shell "su -c 'pidof garden-moss && (wget -qO- http://127.0.0.1:7185/health | head -c 120)'"
   ```

2. **Switch adb to TCP so it survives losing the USB-C port:**
   ```powershell
   adb -s 89TY0BAV9 tcpip 5555
   ```
   (adbd restarts listening on `:5555`. USB adb drops — expected.)

3. **Operator: attach the USB-C Ethernet adapter** (use a passthrough/charging dock
   so the phone keeps power). The phone gets a DHCP lease on the LAN.

4. **Learn the phone's LAN IP** (any one):
   - Operator reads it on-device (Settings → About → Status → IP), or
   - From a peer on the same LAN: `garden-rake discover` (the phone should appear), or
   - ARP scan from a host on the LAN.

5. **Reconnect adb over the LAN** (only if this host shares the LAN):
   ```powershell
   adb connect <phone-ip>:5555
   ```

## What to verify

- **Moss sees the interface** — the no-LAN WARNs stop:
  ```powershell
  adb -s <phone-ip>:5555 shell "su -c 'tail -n 30 /data/garden-moss.log'"
  ```
  Expect: no more `No eligible network interfaces` / `using 127.0.0.1`; an mDNS
  registration line with the phone's real IP (not loopback).
- **Announced address is the LAN IP, not 127.0.0.1:**
  ```powershell
  adb -s <phone-ip>:5555 shell "su -c 'wget -qO- http://127.0.0.1:7185/api/v1/stone/capabilities | head -c 200'"
  ```
- **#3 discovery** — from another stone (or this host with `garden-rake`) on the LAN:
  `garden-rake discover` lists the phone with correct specs (8 cores, SDM845, 3.5 GB, aarch64).
- **#5 garden inspection** — the phone appears in `garden-rake observe all` / the
  tended-Moss garden topology, and `garden-rake services` shows its offerings (mongodb).

## If discovery does not work

- moss interface selection is route-table-based (HOST-0001) — it should pick the
  adapter by any name. Check `/proc/net/route` has a default route on the adapter.
- p2p discovery uses multicast `239.255.42.99:7184` TTL 1 — confirm the LAN passes
  multicast (some managed switches drop it; try a dumb switch / direct link).
- Local-IP detection feeding the announcement: if it still reports `127.0.0.1` with
  a live adapter, that is a `garden_common::infra::network` detection gap to fix.

## Restore USB adb afterwards

```powershell
adb -s <phone-ip>:5555 usb     # or just replug USB-C (adapter removed)
```
