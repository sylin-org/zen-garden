# The pulse wall — mandate walk (2026-08-29, PROPOSAL for discussion)

*Status: PROPOSAL — walked per THE SLICE MANDATE, awaiting discussion.
Nothing implemented. The PoC's mechanism had defects; its visual and
delight were right. This harvests the intent.*

---

## Gate 1 — Prior art

- **htop / btop / k9s**: full-screen terminal monitors; btop is the
  aesthetic benchmark. Lesson: the feed (sampling) and the presentation
  (paint) are separate crafts; the good ones diff frames instead of
  repainting.
- **Grafana**: the wall-dashboard standard — panels are data-driven,
  the feed is a query/stream layer, presentation never talks to
  collectors. Lesson: one event envelope, many renderers.
- **Home Assistant dashboards**: household-grade tiles + websocket
  state diffs. Lesson: "found/alive/dead" states beat numbers for
  non-technical eyes.
- **SSE**: the substrate — standard, curl-able, browser EventSource
  auto-reconnects. Right choice, kept.

## Gate 1b — Delight research: what homelabbers actually love (2026-08-29)

From the r/homelab and r/selfhosted communities, 2025 sentiment:

- **The sweet spot is LIGHTWEIGHT.** *"Beszel + Uptime Kuma is honestly
  the sweet spot for most homelabs"* — the community is actively
  DITCHING Grafana/Prometheus stacks for zero-config, low-resource
  tools. The garden's leanness is not a compromise here; it IS the
  delight. `rake pulse` must work with zero setup: run it, and the
  garden is on the screen.
- **State-change visibility beats metric precision.** Uptime Kuma's
  love is up/down at a glance and "your phone buzzes when something
  breaks" (90+ notification channels). The garden's equivalents are
  richer: seen / **said goodbye** / expired, running / stopped /
  degraded / interrupted. The wall leads with STATES, not graphs.
- **Notifications are the real delight surface** — and the wall is
  passive. Consequence for design: the pulse feed's event vocabulary
  (kind + level) must be notification-READY — a future Cricket
  (audio) or phone bridge subscribes to the same bus the wall reads.
  Not this slice; the envelope must not preclude it.
- **btop's TUI craft** — color-coded, real-time, dense but legible —
  is the terminal aesthetic benchmark. The PoC's pulse belonged in
  that lineage; port the craft.
- **The wall kiosk is a beloved physical artifact**: Raspberry Pi +
  touchscreen running an always-on dashboard is a whole genre
  (Grafana kiosks, HA panels). The PoC designed pulse for exactly
  this (tty1, OLED sidecar). Unattended auto-reconnect + low noise
  is what makes a kiosk real; the frame-buffer wall inherits it.
- **Shareable status** ("a status page others can look at") — the
  moss's pulse page is the seed of this; later slice.

## Gate 2 — PoC homework: the delight, and the mechanism's defects

**The delight (harvest whole)** — `poc/rake/src/commands/pulse.rs`
(1,760 lines) + `poc/moss/src/infra/listeners/pulse.rs` (929) +
`assets/pulse.html` (575):

- A full-screen, UNATTENDED wall view: header (stone, health, uptime,
  connection state, evt/min), gauges (CPU/MEM/DISK/GPU/NET with
  warn/crit thresholds shared with the Firefly LED companion — one
  visual vocabulary across surfaces), an offerings strip, THE WIRE
  (newest-first event feed, entity column, detail items fitted to the
  remaining character budget), a garden sidebar, a diagnostics footer.
- Geometry-adaptive: split (wire + sidebar) / stacked / narrow.
- Unattended: reconnect with 1s→30s backoff, `server.shutdown`
  sentinel exits cleanly, 250ms redraw throttle, 200-event ring.
- Live-proven at 32 evt/min. It made the room FEEL alive.

**The mechanism defects (harvest the lesson, not the thing):**

1. **A window onto ONE stone.** The wall watched a single moss's
   firehose; "garden awareness" was a 10s HTTP poll of that stone's
   topology cache — a stale sidebar, and polling where events exist
   (L18).
2. **TransportTap — a private ear on the wire.** Raw UDP datagrams
   were tapped into the firehose as `transport.*` events: a parallel
   event path beside the domain bus — R2.9's "no private tap" violated
   by the observability layer itself. And the 32 evt/min was mostly
   chirp heartbeat noise; the wire feed's signal drowned in its own
   room's heartbeat.
3. **No sequence, no resume.** Reconnect = a blind spot; no seq
   numbers, no Last-Event-ID, no "you missed N".
4. **Stringly events.** `event_type`/`category`/`message` free strings
   per consumer, three SSE consumers filtering by prefix; the presence
   snapshot rebuilt Date math per call with no ETag (inventory gaps
   339–370).
5. **The wall showed LIFE, not WORK.** The state machine applied
   started/stopped/health — nothing for capture runs, replants, or
   jobs; the garden's most interesting moments never reached the wall.
6. **1,760 lines of hand-ANSI with no test seam**, `ctrl_c` →
   `process::exit(0)` (no graceful terminal restore beyond a cursor
   move).

## Gate 3 — House law

- L18: the feed is events; a poll is a floor, not the mechanism.
- R2.9: no private taps — a wire story must come FROM the dispatcher's
  counters (posture already counts ingest/dispatch), never a new ear.
- L21: rake may poll as a floor; the wall should live on events.
- R4.8: the feed is an answer (data); the wall is a rendering.
- R3.9/B1: ONE sectioned event envelope, not stringly tuples.
- R2.6: event names speak glossary nouns.
- Already in v1: the `PulseStream` face (topology + offering events,
  JSON lines, lagged notices, keep-alive) and a minimal `PulsePage`;
  jobs.changes() broadcast (W10 gave it `interrupted`); capture run
  announcements; storage news (bank mount/eject/roles); kernel
  ingest/dispatch counters; the Factsheet sampler for load.

## Gate 4 — Design: two halves, one envelope

**Half 1 — the FEED (moss, ~small):**

1. `PulseEvent` in `contract`: `{ seq, ts, kind, category, level,
   stone?, offering?, summary, data? }` — kind/category from glossary
   nouns. Sequence numbers per stream.
2. ONE broadcast bus in moss, fed by adapters from existing sources:
   registry (offerings), topology (seen/goodbye/expired), jobs
   (started/done/failed/**interrupted**), capture runs (phases),
   storage news (mount/eject/roles). NO transport tap — a low-rate
   wire-delta event derived from dispatcher counters (datagrams
   in/dispatched per interval): the wire's FEEL, none of the noise.
3. A slow load event (cpu/mem/disk/net) from the Factsheet sampler —
   the gauges' food (5–10s cadence).
4. `PulseStream` face upgraded in place: snapshot-first (the world as
   the stone sees it, topology included), then events, each `seq`'d;
   optional `?categories=` filter. On connect-after-gap the client
   knows exactly what it missed.

**Half 2 — the WALL (rake, the delight port):**

5. `rake pulse` — the full-screen monitor, visuals ported whole:
   layout detection, header, gauges, offerings strip, the wire ring,
   garden sidebar, footer. Re-hosted on a frame buffer (the PoC's own
   "v2" intention): paint functions become pure `state → lines`
   (testable), redraw = diff, ctrl_c restores the terminal gracefully.
6. The sidebar comes from the feed's SNAPSHOT (topology rides in it)
   and updates by events — **no polling**. Labeled honestly: "as seen
   by ⟨connected stone⟩".
7. WORK on the wall: grow/capture/replant jobs appear as events — the
   garden's working moments visible, which the PoC never showed.
8. **Goodbye is a moment, not a fade.** The law (topology.rs
   on_goodbye): a goodbye chirp removes the stone from the room's map
   IMMEDIATELY — no threshold wait — its offerings and banks vanish
   from resolution that instant (spoken three times for delivery).
   The wall renders the distinction the PoC blurred: `goodbye` (the
   stone SAID farewell — wire line + sidebar row leaves at once) vs
   `expired` (silence past the threshold — the soft-honesty hold, row
   dims with "expired"). Two different trust stories; two treatments.

## A register ruling for the wall (operator feedback, 2026-08-29)

The delight lives in the VISUALS — states at a glance, motion, color,
glyphs, the heartbeat. The WORDS stay plain garden English: states and
facts, no weather metaphors, no almanac poetry. Operator surface =
plain register (R3.1); poetry is the Household register's job, later,
on the moss page.

    BEFORE (over-poetic)                     AFTER (plain garden English)
    ☀ the garden is calm — 5 stones...   →   5 stones reachable · 7 offerings running
    ☁ overcast on the east bed —         →   crystalline-dune unreachable —
      crystalline-dune expired (90s)         expired after 90s of silence
    🍂 tranquil-pass said goodbye        →   tranquil-pass said goodbye —
      (farewell poeticism)                   removed from the room
    today in the garden: 3 plantings ·   →   today: 3 planted · 1 goodbye ·
      1 goodbye · 0 interventions            2 models grown · 1 checkpoint

Glyphs stay (a goodbye marker, a dormancy glyph on rested offerings) —
a glyph is visual language, not vocabulary. The heartbeat sparkline
stays. The weather LINE becomes a plain status line; the "weather"
concept survives only as the at-a-glance quality of that line.

## Gate 5 — Verdicts

| PoC element | Verdict |
|---|---|
| Unified channel + event envelope | brought reshaped (typed, seq'd, in contract) |
| TransportTap (raw UDP tap) | LEFT DEAD (R2.9) — dispatcher counter-deltas instead |
| Presence snapshot + vocabulary | brought reshaped (snapshot-first feed, categories filter) |
| Load events → gauges | brought (new emitter, Factsheet sampler) |
| Three SSE consumers (firehose/presence/metrics) | brought reshaped (one face, `?categories=`) |
| `server.shutdown` sentinel | brought |
| Wall visuals (layout, gauges, wire, sidebar) | brought whole — re-hosted on a frame buffer |
| Hand-ANSI 1,760-line renderer | brought reshaped (pure paint fns + diff) |
| 10s topology poll | LEFT DEAD (events + snapshot) |
| Wishes → the wall | new (PoC never showed work) |

## Delight / audiences

- **Gardener**: the garden on a screen — alive, honest, working.
- **Household** (M5, later): the same feed renders tiles.
- **Skeptic demo**: wall running → kill a stone → goodbye line, sidebar
  dims, offerings go silent → stone returns → goodday. One minute of
  trust, on a wall.
- **Agent**: the feed face is machine-readable (R4.8); the wall is
  just one renderer.

## Open questions for the discussion

1. **Scope**: single-feed wall first (recommended) vs room-merged
   multi-stone feeds now?
2. **Gauges**: include the load emitter (recommended — the gauges are
   the wall's signature) or land the wall gauge-less first?
3. **Wire feed**: dispatcher counter-deltas (recommended, R2.9-clean)
   vs nothing at all?
4. **Rendering**: hand-rolled frame buffer (recommended — ports the
   craft, no new dependency) vs adopt a TUI crate (ratatui)?
