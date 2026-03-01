# Proposal: `rake pulse` — Terminal Monitor

## Summary

A permanent, unattended terminal display for stone observability. Designed for dedicated Linux screens (tty1 on a Pi, an OLED sidecar, a wall-mounted monitor) where an operator glances at it like a clock on the wall. No interaction expected during normal operation — just Ctrl+C to exit.

## The use case, precisely

A stone has a small screen attached. Maybe a 7" HDMI display on a Raspberry Pi. Maybe a repurposed laptop with the lid open. The stone boots, autologins to tty1, and runs `rake pulse` in `.bashrc`. The screen shows live vitals, offering status, and recent events. Nobody touches it. An operator walks by, glances, sees everything is green, keeps walking.

This is not a debugger. This is not an interactive TUI with tabs and modals. This is a **wall monitor** — the terminal equivalent of a status board in a server room.

**Why not the HTML pulse page?** No browser. No GUI. No X11. A Linux framebuffer console with a monospace font is the entire rendering surface. The HTML page exists for operators who have a desktop. This serves the ones who don't.

## Data sources

All infrastructure exists (Pulse consolidation):

| Source | Endpoint | Provides |
|--------|----------|----------|
| Presence stream | `GET /api/v1/stone/presence/stream` | Initial snapshot + domain events |
| Topology | `GET /api/v1/garden/topology` | Garden-wide stone list (polled every 10s) |

The presence stream is the right choice over the full pulse firehose. A wall monitor doesn't need raw UDP transport announcements — those are debugging artifacts. Domain events (service started, health changed, load updated) are what an operator glances at.

## Design

### Invocation

```
rake pulse                    # Monitor tended stone
rake pulse --at stone-name    # Monitor specific stone
```

No flags for tabs, modes, or filtering. This is a single-purpose display.

### Layout: Fluid, not breakpointed

Instead of discrete breakpoints, each region measures its available space and renders what fits. The layout is a vertical stack — regions are never side-by-side unless both fit. This matters because the target screens vary wildly:

- 80x24 standard terminal
- 40x30 vertical OLED
- 120x40 laptop display
- 20x4 I2C character LCD (extreme edge — but why not degrade to it?)

**Regions in priority order** (first region that doesn't fit gets cut):

```
1. Header       — stone name + health (1 line, always shown)
2. Gauges       — CPU, MEM, DISK, optional GPU/NET (1-2 lines)
3. Offerings    — status dots (1 line)
4. Divider      — (1 line)
5. Events       — scrolling feed (remaining rows)
6. Garden       — peer stones, only if rows > 30
```

### Rendering with characters

No box-drawing Unicode required. Plain ASCII works on every framebuffer console, every serial terminal, every SSH session. Unicode is used only when detected.

**Wide (80+ cols):**
```
 stone-crystal-forest                              thriving  up 14d 3h
 CPU [=========---------] 47%   MEM [=============-] 87%   DSK [====----] 52%
 mongodb:ok  ollama:ok  weaviate:ok  redis:degraded
 ──────────────────────────────────────────────────────────────────────────
 14:32:01  load updated                cpu 47% mem 87%
 14:32:04  ollama                      health ok
 14:31:58  mongodb                     backup completed
 14:31:55  health changed              thriving
 14:31:42  weaviate                    started
 14:31:30  redis                       health degraded
```

**Narrow (40 cols):**
```
 crystal-forest        thriving
 CPU [====----] 47%  MEM 87%
 mongodb:ok ollama:ok +2
 ────────────────────────────────
 14:32 load updated
 14:32 ollama health ok
 14:31 mongodb backup done
 14:31 health: thriving
```

**Tiny (20 cols, edge case):**
```
crystal-forest
CPU 47 MEM 87 DSK 52
──────────────────
14:32 load upd
14:32 ollama ok
14:31 mongodb ok
```

### Gauge rendering

Gauges adapt to available width. The `[===---]` style works on every terminal, no special characters needed:

```
>= 30 chars:  CPU [===============-----------] 52%
>= 16 chars:  CPU [=====---] 52%
<  16 chars:  CPU 52%
```

Color thresholds match Firefly: green < 60%, yellow 60-85%, red > 85%. On no-color terminals, gauges still read fine — the `=` fill is self-descriptive.

GPU and NET gauges appear only when the stone reports them (`has_gpu: true`, nonzero network rates). On a Pi with no GPU, that line doesn't exist — no wasted vertical space.

### Event feed

The event feed is the soul of the display. It occupies all remaining vertical space after the header regions. Events scroll up, newest at bottom.

Each line: `HH:MM  entity  message` — padded to align columns. Entity is the offering name, or "stone" for system events. Messages are truncated to fit, never wrapped.

Event colors: green for positive (started, ok), yellow for warnings (degraded, lagged), red for errors (failed, stopped), dim for routine (load updated). On no-color terminals, the text alone carries meaning.

The feed holds a ring buffer (200 events max). Old events fall off the top. No scrollback — this is a live display, not a log viewer.

### Garden peers (optional region)

When vertical space allows (rows > 30), a "garden" section appears below the divider showing peer stones from topology:

```
 garden ──────────────────────────
 mossy-brook     ok  3 svc  2m ago
 quiet-pond      ok  1 svc  5m ago
 ochre-alcove    --  0 svc  offline
```

On smaller screens this section is simply absent. The local stone's vitals are always more important than peer status.

### Reconnection

The display is permanent. If the stone restarts, the SSE connection drops. The monitor shows a connection status line and reconnects with exponential backoff (1s, 2s, 4s, max 30s). When it reconnects, it gets a fresh snapshot and resumes.

```
 stone-crystal-forest                              reconnecting...  3s
 CPU [------------------] --%   MEM [------------------] --%
```

This is critical for the "runs in .bashrc on tty1" use case — the display must survive stone restarts without operator intervention.

### Exit

Ctrl+C only. No `q`, no `Esc`, no key bindings. Raw mode is unnecessary. The display runs in cooked mode with a simple `tokio::signal::ctrl_c()` handler. This eliminates the entire class of "terminal left in broken state" bugs.

Wait — can we avoid raw mode entirely?

**Yes.** The display doesn't need keyboard input. It doesn't need to intercept individual keypresses. It just prints lines. The render loop:

1. On SSE event or topology poll: clear screen (`\x1b[2J\x1b[H`), redraw all regions.
2. Between events: do nothing.
3. On Ctrl+C (SIGINT): exit cleanly.

This is `println!`-level simplicity. No crossterm raw mode. No terminal event polling. No panic hooks. The only cursor control needed is "move to top-left and clear" — two ANSI escapes that work on every terminal since VT100.

**Flicker concern:** Full-screen clear + redraw flickers on slow terminals. Mitigation: write the entire frame to a `String` buffer, then `write_all` in one syscall. On a local tty this is imperceptible. Over SSH, it's a single TCP segment. Good enough for a 1-2 Hz update rate.

## Feasibility

### What already exists

| Concern | Status |
|---------|--------|
| Terminal size | `TerminalInfo::detect()` gets `(width, height)` |
| Color detection | `supports-color` + `NO_COLOR` env |
| Unicode detection | `TerminalInfo::supports_unicode` |
| SSE parsing | `watch.rs` proven pattern |
| Topology fetching | `observe.rs` proven pattern |
| Presence snapshot | `PresenceSnapshot` with all gauge fields |
| Stone resolution | Tending + discovery fallback |
| Command trait | Standard `Command` impl |

### What needs building

| Component | Effort | Notes |
|-----------|--------|-------|
| Frame renderer | Medium | Measure regions, build `String` buffer, flush |
| Gauge formatter | Small | `format_gauge(value, width) -> String` |
| Event ring buffer | Small | `VecDeque` with max 200 |
| SSE consumer task | Small | Adapt from `watch.rs` |
| Topology poll task | Small | `tokio::time::interval(10s)` + HTTP GET |
| Reconnection logic | Small | Backoff loop around SSE connect |
| ANSI clear + cursor | Trivial | Two escape sequences |

### Dependencies

**None new.** No crossterm. No ratatui. The display uses `println!` and two ANSI escapes. All formatting uses `colored` (already a dependency) and `terminal_size` (already a dependency). The SSE consumer uses `reqwest::stream` (already a dependency).

This is the strongest argument for this design: **zero new crates**.

## Critique

### What's strong

1. **Zero dependencies.** No crossterm, no ratatui, no raw mode, no panic hooks. The entire rendering surface is `print!()` + ANSI escapes.
2. **Unattended by design.** No keyboard handling means no "terminal left in broken state" failure mode. The process can be killed, SIGTERMed, or OOM-killed and the terminal recovers naturally.
3. **Framebuffer-friendly.** Works on tty1 without X11, Wayland, or any display server. Works on serial consoles. Works over SSH.
4. **Data infrastructure is done.** Presence stream provides snapshot + live updates. Topology API provides peer state. Both proven.

### What deserves skepticism

1. **Full-screen clear may flicker over slow links.** A stone connected via serial console or high-latency VPN will see a visible flash on each redraw. Mitigation: batch writes (one syscall per frame) and throttle to max 1 redraw/second. For truly slow links, an `--events-only` mode that just appends lines (no clear, no gauges) would be better — but that's just `rake watch` with extra context.

2. **No interactivity means no filtering.** If an operator wants to see only storage events or only errors, they can't. They'd switch to `rake watch --categories=storage` instead. This is acceptable — the wall monitor shows everything, the CLI tools let you slice.

3. **Topology polling adds HTTP overhead.** Every 10 seconds, Rake GETs `/api/v1/garden/topology` from the tended stone. On a garden with 15 stones this is ~5KB of JSON. Negligible, but it's a persistent background load. If the stone is under pressure, topology is the first thing to skip (it's the lowest-priority region anyway).

4. **Single-stone focus.** The display monitors one stone's presence stream. Garden-wide awareness comes only from topology polling, which is the tended stone's cached view. If the tended stone is partitioned, the garden section shows stale data. This is inherent to the architecture and acceptable for a wall monitor.

5. **Dual rendering concern is weaker now.** Since the terminal version is deliberately simpler (no transport events, no interactivity, no tabs), it won't track the HTML pulse page feature-for-feature. They serve different audiences with different fidelity expectations. The overlap is smaller than I initially feared.

## Scope

Single phase. No follow-up phases needed.

- `rake pulse` command
- Fluid layout (header, gauges, offerings, events, optional garden)
- Presence stream consumer with snapshot + live updates
- Topology polling (10s interval)
- Reconnection with backoff
- Gauge formatting (adaptive width)
- Event ring buffer (200 max)
- Color + Unicode detection (existing infrastructure)
- No raw mode, no keyboard input, no new dependencies

## File plan

| File | Purpose |
|------|---------|
| `src/rake/src/commands/pulse.rs` | Command implementation: SSE consumer, topology poller, frame renderer |
| `src/common/src/ui/gauge.rs` | `format_gauge(value, width) -> String` reusable gauge formatter |
| `src/common/src/ui/mod.rs` | Add `pub mod gauge;` |

Modified:

| File | Change |
|------|--------|
| `src/rake/src/commands/mod.rs` | Add `pub mod pulse;` |
| `src/rake/src/cli_build.rs` | Register `pulse` subcommand |
| `src/rake/src/route.rs` | Route `pulse` to command |

## Verification

```bash
cargo check --all
cargo clippy -- -D warnings
cargo test --package garden-rake
```

Manual:
- `rake pulse` on 80x24 terminal — gauges, offerings, event feed all visible
- `rake pulse` on 40x20 terminal — graceful degradation, no garden section
- Restart target stone — monitor shows "reconnecting...", resumes on restart
- Ctrl+C — clean exit
- `NO_COLOR=1 rake pulse` — gauges and events readable without color
- Run for 10+ minutes — no memory growth (ring buffer bounded)
