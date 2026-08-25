# ARCH-0009: Moss-owned MOTD

**Status**: Accepted
**Date**: 2026-03-22

## Context

The stone displayed `/etc/motd` at SSH login to give operators a quick summary of the
machine they had connected to. Two separate implementations existed and neither worked:

1. **Branding system** (`installer/branding/prepared/stone-root/`): a `motd.template`
   and a `generate-stone-motd.sh` shell script deployed via the preseed `stone-root` copy.
   The script was placed in `/usr/local/bin/` but never wired to a trigger — no entry in
   `/etc/update-motd.d/`, no PAM hook, no systemd unit. It was never called.

2. **Moss first-boot** (`tty.rs:write_motd`): wrote a static text block to `/etc/motd`
   during first-boot initialization. Written once, immediately stale (baked-in IP, no
   hardware info, no pond or storage context).

Neither system delivered an accurate, useful MOTD.

## Decision

Moss owns `/etc/motd` entirely. The branding template and shell script are removed.

Moss writes the MOTD at two points in its lifecycle:

1. **Startup** (`bootstrap/run.rs`) — written immediately after `AppState` is built, with
   whatever is cached: stone identity, current IP, pond enrollment, storage banks. Hardware
   line omitted if capabilities cache is absent (first boot).

2. **After hardware detection** (`tasks/hardware_detection.rs`) — overwritten once
   detection completes (~3–5 s after startup), adding CPU cores, RAM, and GPU/VRAM.

The MOTD is regenerated on every startup, so IP changes, pond transitions, and storage
changes are always reflected on the next SSH login.

## Format

50-character width (matching `RIBBON_DIVIDER`). Two-column rows; left side truncated to
preserve the right side. Optional lines are omitted entirely when their data is absent —
no placeholders, no "N/A".

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  stone-shadowed-haven          Moss v0.2.0
  192.168.1.173:7185            pond-still-lotus
  4 cores / 15.6 GB             RTX 3060 / 12 GB
  2 storage sets
    storage  (my-seagate-4tb)   2.1 TB / 4.0 TB
    prod  (samsung-t7)          890 GB / 2.0 TB
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

Storage hierarchy: the summary line names the count of replica sets; sub-lines show
`{replica_set_name}  ({seed_bank_name})` with used/capacity right-aligned.

## Rationale

Moss already knows everything needed: stone name, IP, version, pond state, storage
domain, hardware capabilities. An external script relying on `hostname`, `curl`, and `jq`
at login time has more failure modes, introduces a delay on SSH login, and duplicates
knowledge that moss already holds.

Moss runs on every boot. Writing a file it already owns is zero-dependency and always
correct relative to the state it just loaded.

## Consequences

- `installer/branding/prepared/stone-root/etc/motd.template` removed.
- `installer/branding/prepared/stone-root/usr/local/bin/generate-stone-motd.sh` removed.
- `write_motd` call removed from `first_boot.rs` — first boot exits moss anyway; the
  MOTD is written correctly on the subsequent systemd-restarted boot.
- No PAM, `/etc/update-motd.d/`, or cron configuration required.
