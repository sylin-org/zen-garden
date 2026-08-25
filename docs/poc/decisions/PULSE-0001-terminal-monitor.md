---
audience: [contributor, maintainer, ai]
doc_type: adr
status: accepted
last_verified: 2026-02-28
canonical: true
---

# PULSE-0001: Terminal Monitor (`rake pulse`)

**Status**: Accepted
**Date**: 2026-02-28
**Deciders**: Leo, Claude
**Tags**: [pulse, rake, terminal, observability, monitoring]

---

## Context

Zen Garden stones are headless Linux machines. The primary interaction mode is
SSH + Rake. The Pulse consolidation (2026-02) unified streaming infrastructure
into a single `pulse_tx` channel and added an HTML instrument panel at `/pulse`.

The HTML page requires a browser and a GUI environment. Stones often have
small dedicated screens (7" HDMI on a Pi, repurposed laptop, OLED sidecar)
running a bare Linux framebuffer console — no X11, no Wayland, no browser.
These screens are meant to show live status permanently: the stone boots,
autologins to tty1, and runs `rake pulse` from `.bashrc`. An operator walks
by, glances, sees green, keeps walking.

No existing Rake command serves this use case:
- `rake observe` is a one-shot snapshot (stale immediately).
- `rake watch` is an unstructured event stream (no gauges, no context).
- Neither adapts to different screen geometries.

---

## Decision

Implement `rake pulse` as a permanent, unattended terminal monitor.

### Design principles

1. **Wall monitor, not debugger.** No tabs, no modals, no interactive features.
   A single unified display that shows everything important in priority order.

2. **No raw mode.** The display does not intercept keypresses. It runs in
   cooked terminal mode with standard SIGINT handling. Ctrl+C exits. This
   eliminates the entire class of "terminal left in broken state" failures
   and removes the need for crossterm raw mode or panic hooks.

3. **Zero new dependencies.** Rendering uses `print!()` and two ANSI escape
   sequences (clear screen `\x1b[2J`, cursor home `\x1b[H`). Color via
   `colored` (existing). Terminal size via `terminal_size` (existing). SSE
   parsing via `reqwest::stream` (existing). No crossterm, no ratatui.

4. **Fluid layout, not breakpoints.** Each region measures available space
   and renders what fits. Regions are never side-by-side unless both fit.
   The display adapts to any geometry: 80x24 standard, 40x30 vertical OLED,
   120x40 laptop, 20x10 character LCD.

5. **Presence stream, not pulse firehose.** The monitor consumes
   `/api/v1/stone/presence/stream` (domain events only). Transport-level UDP
   announcements are debugging artifacts — not useful on a wall display.
   Topology comes from periodic polling of `/api/v1/garden/topology`.

### Layout regions (vertical stack, priority order)

| Priority | Region | Rows | Content |
|----------|--------|------|---------|
| 1 | Header | 1 | Stone name, health, uptime |
| 2 | Gauges | 1-2 | CPU, MEM, DSK bars; GPU/NET only if present |
| 3 | Offerings | 1 | Service status dots |
| 4 | Divider | 1 | Horizontal line |
| 5 | Events | remaining | Scrolling feed, newest at bottom, ring buffer (200 max) |
| 6 | Garden | 3-N | Peer stones from topology (only if rows > 30) |

Regions below the cutoff are simply absent. On a 10-row screen, only header,
one gauge line, and a few event lines appear. On a 40-row screen, the garden
section appears.

### Gauge rendering

ASCII bar gauges that work on every terminal:

```
>= 30 chars:  CPU [===============-----------] 52%
>= 16 chars:  CPU [=====---] 52%
<  16 chars:  CPU 52%
```

Color thresholds: green < 60%, yellow 60-85%, red > 85% (matches Firefly).
GPU and NET lines only appear when the stone reports them (`has_gpu: true`,
nonzero network rates).

### Reconnection

The display is permanent. On SSE disconnect, it shows "reconnecting..." with
a countdown and uses exponential backoff (1s, 2s, 4s, ... max 30s). On
reconnect, the presence stream delivers a fresh snapshot. No operator
intervention required.

### Invocation

```
rake pulse                    # Monitor tended stone
rake pulse --at stone-name    # Monitor specific stone
```

No flags for modes, tabs, or filtering. Single-purpose display.

---

## Consequences

### Positive

- Stones with dedicated screens get a native, permanent monitoring display.
- Zero new dependencies — no compile time or binary size impact.
- No raw mode — no "broken terminal" failure mode, trivially killable.
- Fluid layout degrades gracefully to any screen geometry.
- All data infrastructure already exists (presence stream, topology API).

### Negative

- Two rendering codebases for pulse data (HTML + terminal). Mitigated by
  keeping the terminal version deliberately simpler — it is a monitor, not
  a debugger. Feature parity with the HTML page is not a goal.
- Full-screen clear may flicker over high-latency SSH or serial consoles.
  Mitigated by writing the entire frame in one `write_all` syscall and
  throttling to max 1 Hz.
- No interactivity means no filtering. Operators who need to slice events
  use `rake watch --categories=...` instead.

### Neutral

- Topology polling adds one HTTP GET every 10 seconds. ~5 KB of JSON for
  a 15-stone garden. Negligible overhead.
- Single-stone focus: garden awareness comes from the tended stone's cached
  topology. Same limitation as `rake observe`.

---

## File Plan

New files:

| File | Purpose |
|------|---------|
| `src/rake/src/commands/pulse.rs` | Command: SSE consumer, topology poller, frame renderer |
| `src/common/src/ui/gauge.rs` | Reusable gauge formatter |

Modified files:

| File | Change |
|------|--------|
| `src/common/src/ui/mod.rs` | Add `pub mod gauge;` |
| `src/rake/src/commands/mod.rs` | Add `pub mod pulse;` |
| `src/rake/src/cli_build.rs` | Register `pulse` subcommand |
| `src/rake/src/route.rs` | Route to command |
