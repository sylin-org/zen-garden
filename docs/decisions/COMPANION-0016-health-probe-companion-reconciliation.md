---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-15
canonical: true
---

# COMPANION-0016: Health-Probe Companion Reconciliation — Retire the PID Ledger

**Date**: 2026-04-15
**Status**: Accepted
**Supersedes (in part)**: the `RuntimeLedger` component of [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)
**Related**: [COMPANION-0015](COMPANION-0015-adapter-self-exit-on-stale-connection.md)

## Context

Moss spawns companion processes (firefly, cricket) as long-lived children with `kill_on_drop(false)` so they survive moss restarts. After a moss restart, moss has no `Child` handle for those companions, so it must "adopt" them: figure out which companions are still alive and skip respawning them.

The original adoption mechanism persisted PIDs in `Companion-runtime.json`. On startup, moss read the file and called `is_process_alive(pid)` — implemented as `Path::new("/proc/{pid}").exists()`. If alive, "adopt"; if dead, spawn fresh.

This is unreliable in two distinct ways, both observed in production:

1. **Across reboots, PIDs are reused.** PID 1373 from yesterday's session is some unrelated process today (mongod, sshd, a kernel thread). `/proc/1373` exists; moss "adopts" the corpse; the firefly companion is never spawned. Symptom: stone-golden-summit booted at 21:03, the firefly process never started, and the OLED stayed in disconnected mode.

2. **Within a session, PIDs can be reused.** A companion crashes; before reconcile runs, the kernel hands its PID to an unrelated short-lived process. Same liar-PID problem at smaller scale.

A spotfix layering boot_id and exe-name verification onto the PID check would catch both cases I have *evidence* for — but it papers over the architectural mistake. PID-as-identity-for-service-liveness is the wrong primitive. Companions are HTTP services on assigned ports; that is the correct primitive, and moss already uses it for every other interaction (commands, shutdown).

## Decision

Companion liveness is determined by HTTP `/health` probe on the assigned port, not by PID-file lookup.

### The new reconcile

```
on reconcile (called from scan_and_autostart):
  for each registered companion with an assigned port:
    if GET http://127.0.0.1:{port}/health succeeds within 500ms:
      mark as alive (adopted)
    else:
      leave as not running — auto-start will spawn fresh
```

The companion's `/health` endpoint already exists ([command_transport.rs:213](../../src/companion-sdk/src/garden/command_transport.rs#L213)) and is required of every companion by the protocol. No companion-side change is needed.

### State model

`RegisteredCompanion` carries an `alive: bool` flag. It is set to true on:
- Successful spawn (`start`).
- Successful adoption probe.

It is set to false on:
- Explicit stop.

`is_running()` returns `self.alive`. There is no `is_process_alive(pid)` call anywhere in the liveness path.

### `RuntimeLedger` removed

`Companion-runtime.json` and the `RuntimeLedger` struct are deleted. The `PortLedger` (`companion-ports.json`) remains — it's the only state we need to persist, and ports are stable assignments across runs.

The on-disk `Companion-runtime.json` from prior installs becomes a harmless leftover; it can be deleted or ignored.

### Shutdown targeting

Most stop paths already use HTTP `/shutdown` first ([companions.rs:878](../../src/moss/src/infra/companions.rs#L878)). The fallback to OS-level kill currently relies on a stored PID. For freshly-spawned companions we still have the `Child` handle and use it. For adopted companions (no Child handle) the fallback is `find_pid_on_port(port)` — a best-effort lookup via `ss -ltnp` (Linux) or `netstat -ano` (Windows). The fallback only fires when `/shutdown` fails, which should be rare; the lookup not finding a PID is also acceptable (the companion is wedged but at least we tried).

## Consequences

### Positive

- Reboot-safe by construction — no stale PIDs to reason about.
- PID-reuse-safe by construction — we never trust a PID for liveness.
- Single source of truth (the port) instead of two (port + PID).
- Same mechanism moss already uses for every other companion interaction.
- Less code: ~50 lines of RuntimeLedger persistence, load, save, reconcile-by-PID disappear.

### Negative

- 500ms timeout per registered companion at startup (worst case). With current scale (≤ 13 companions per stone) that's ≤ 6.5s in the worst case where every companion is dead. In practice all probes complete in single-digit milliseconds because they're loopback.
- An adopted companion's PID is unknown until/unless we do a port→PID lookup. Acceptable — we only need it for the SIGKILL fallback path.

### Migration

`Companion-runtime.json` files on existing stones become unread and harmless. A separate sweep can delete them; not blocking.

## Alternatives considered

- **Spotfix: boot_id + exe-name verification on the PID ledger.** Catches the two known failure modes but doesn't address the wrong abstraction. Next failure (a same-named process recycling the PID) bypasses both checks.
- **systemd-managed companions.** Move lifecycle to systemd units. Cleaner in some ways but couples moss to systemd — we run on Windows too, and want one mechanism.
- **Discover PID via `ss -ltnp` at adopt time.** Acquires a real PID for adopted companions. Adds Linux-only code at startup and conflates two concerns (liveness vs. shutdown targeting). Deferred — only do this in the fallback path.

## References

- [COMPANION-0001](COMPANION-0001-companion-integration-epic.md) — the epic that introduced the PID ledger
- [COMPANION-0015](COMPANION-0015-adapter-self-exit-on-stale-connection.md) — sibling fix on the firefly side
- `src/moss/src/infra/companions.rs` — the registry being refactored
- `src/companion-sdk/src/garden/command_transport.rs` — companion-side `/health` endpoint
