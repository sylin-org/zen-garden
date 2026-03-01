# PULSE-0002: Frame Buffer Renderer and Wire Feed

- **Status:** Accepted
- **Date:** 2026-02-28
- **Supersedes:** Rendering approach in PULSE-0001 (layout only; ADR itself remains valid)

## Context

PULSE-0001 introduced `rake pulse` as a permanent terminal monitor. The initial implementation used sequential vertical rendering: header, gauges, offerings, divider, events, garden. This created layout tension — events and topology competed for vertical space, and the garden section was invisible on standard 24-row terminals.

More critically, the event feed only consumed the presence stream (domain events). For debugging, operators need to see everything the stone experiences: UDP chirps, elections, storage beacons, discovery requests. The pulse stream (`/api/v1/stone/pulse/stream`) already carries these transport events but lacked an initial snapshot, forcing the monitor to use the presence stream instead.

## Decision

### 1. Frame buffer rendering model

Replace sequential string building with a spatial allocation model. Each region (header, gauges, wire feed, topology sidebar, footer) is painted independently into a set of pre-formatted lines. A compositor assembles them into the final frame.

This enables split-screen layout without interleaving logic, and makes each painter independent of the others.

### 2. Wire-first layout

The wire feed (live event stream) is the primary content. Topology is a context sidebar, not a competing region. On wide terminals (>= 100 cols), wire gets ~65% of width with topology as a right sidebar. On medium terminals (60-99 cols), wire fills full width with a compact garden summary below. On narrow terminals (< 60 cols), wire only.

### 3. Pulse stream as single source

Add a `pulse.snapshot` event to `/api/v1/stone/pulse/stream` (same `PresenceSnapshot` payload as presence stream). This gives rake pulse a single connection that provides: initial state, ongoing domain events, and transport events. The presence stream remains unchanged for Companions.

### 4. Transport event visibility

Transport events (chirps, elections, beacons, goodbyes) render in the wire feed with dimmed styling for routine events (chirps, beacons) and highlighted styling for significant events (elections, goodbyes). The `TransportPulse.summary` field provides human-readable text.

### 5. Status footer

A persistent footer line shows connection diagnostics: connected duration, events/minute, time since last chirp, time since last health update. This enables instant diagnosis — if "last chirp" shows "5m ago" on a garden with 15 stones, transport is broken.

## Layout tiers

| Tier | Columns | Mode | Topology |
|------|---------|------|----------|
| Wide | >= 100 | Split (wire left, sidebar right) | Right column, sorted stone list |
| Medium | 60-99 | Stacked (wire, then garden) | Compact summary below wire |
| Narrow | < 60 | Wire only | Summary in separator label |

## Consequences

- Rendering is more complex (paint + composite vs sequential append) but each piece is independently testable
- Single pulse stream connection replaces presence stream — simpler client, richer data
- Transport events may be high-frequency (chirps every few seconds per stone) — the event FIFO naturally caps visible events to screen height
- The 250ms redraw throttle keeps SSH bandwidth reasonable (~2-4KB per frame)
