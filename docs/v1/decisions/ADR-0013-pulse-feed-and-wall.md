# ADR-0013 — The pulse: one seq'd feed, one wall

**Status:** Accepted (2026-08-29)
**Walk:** [../design/pulse-wall.md](../design/pulse-wall.md) (mandate walk,
PoC homework, delight research, operator feedback)

## Context

The PoC's pulse made the room *feel* alive — a full-screen wall with
gauges, a wire feed, a garden sidebar, proven at 32 evt/min — but its
mechanism had defects: it was a window onto ONE stone with a 10s
topology poll; a TransportTap put a private ear on the wire (R2.9);
events were stringly typed with no sequence numbers (reconnect =
blind spot); three SSE consumers filtered one firehose by string
prefix; and the wall showed life but never WORK. v1 already routes
`PulsePage`/`PulseStream` faces over two narrow broadcast channels
(topology + offerings).

## Decision

1. **One envelope.** `contract::pulse::PulseEvent` —
   `{ seq, ts, kind, category, level, stone?, offering?, summary,
   data? }` — sectioned, typed, sequence-numbered per bus. Every
   consumer (wall, web page, a future Cricket/phone bridge) reads the
   same news; the vocabulary is notification-ready by construction.
2. **One bus, fed by adapters from existing sources.** Registry
   changes, topology seen/goodbye/expired, job transitions (including
   `interrupted`), storage news. NO transport tap (R2.9): the wire's
   story is a low-rate dispatcher counter-delta. A slow load event
   from the Factsheet sampler feeds the gauges.
3. **Snapshot-first streaming.** Connecting to `PulseStream` delivers
   the world as the stone sees it (stones, offerings, jobs), then
   deltas. `?categories=` filters. Lag is announced honestly.
4. **The wall is a renderer, not a collector.** `rake pulse` ports the
   PoC's visual craft — geometry ladder (wide/stacked/tall/narrow/
   tiny; portrait case screens are first-class), gauges, the wire
   ring, plain garden English — on a frame buffer with pure paint
   functions, diff-based redraws, graceful exit, and a
   **geometry gallery test**: the frame rendered at canonical sizes
   (53x120 portrait to 26x12 OLED) with structural invariants.
5. **Goodbye and expired are different stories.** Goodbye = immediate
   removal (the law); expired = the soft-honesty hold. The wall
   renders both distinctly.

## Consequences

- The PoC's TransportTap and 10s topology poll stay dead.
- Non-TTY stdout renders sequential plain frames — kiosk-loggable and
  witnessable without a human terminal.
- The event vocabulary is deliberately notification-ready; actual
  notification surfaces (Cricket, phone bridge) are later slices that
  subscribe to the same bus.
- Wire change to `PulseStream` payloads is v1-owned (R0.5); the web
  pulse page upgrade rides a later slice.
