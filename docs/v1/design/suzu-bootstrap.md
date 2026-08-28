# Suzu — project bootstrap

**What:** the companion ecosystem for Zen Garden. The small bells that ring when the garden has something to say.
**Repo:** `sylin-org/suzu` (to be created; currently bootstrapping from `zen-garden/src/poc/`)
**Contract:** ADR-0006 (`zen-garden/docs/v1/decisions/ADR-0006-suzu-contract.md`)
**Status:** ready for a dedicated agent to pick up

---

## What Suzu is

The garden says what happened. Suzu makes the household feel it.

A firefly blooms green when the backup commits. A cricket plays the water-can tune when a capture finishes. A phone buzzes when a stone goes silent. These aren't features — they're the garden's *presence* in the home, the difference between infrastructure you maintain and a living thing you notice.

## The brief for the Suzu agent

The garden (Zen Garden, `sylin-org/zen-garden`) produces events when things happen — stones join and leave, offerings are planted and rest, checkpoints are committed, offerings are replanted. The event envelope is defined in ADR-0006 (JSON, versioned, published on SSE).

Your job: make those events *felt*. Build the companions — small, standalone programs and devices that connect to the garden's event stream and respond with light, sound, and motion. Own the serial/USB layer that talks to the hardware. Design the CLI that makes every companion testable without the garden running.

You do NOT need to understand the garden's discovery envelope, the offer lifecycle, the capture pipeline, or the storage data plane. You need the event envelope (ADR-0006) and the delight brief above. That's the whole context.

## Project structure

```
suzu/
├── contract/              ← the event envelope + command manifest types
│   └── (JSON schemas, fixture tests)
├── sdk/                   ← the Rust convenience layer (spawn, SSE client, port pool)
│   └── src/
├── usb/                   ← serial/USB device layer (udev + PollMonitor, from PoC companion-usb)
│   └── src/
├── companions/
│   ├── firefly/           ← visual companions (RP2040-Matrix, OLED, T-Display)
│   │   ├── src/
│   │   └── assets/
│   └── cricket/           ← audio companion (YAML tunes, CC0 samples)
│       ├── src/
│       └── assets/tunes/
├── firmware/              ← RP2040 device side (from PoC scripts/)
├── cli/                   ← the `vesper` CLI (list, status, control, enroll)
│   └── src/
└── docs/
```

## The five interfaces

Every companion behavior is reachable through five doors:

| Transport | What it's for |
|---|---|
| SSE | receiving garden events (the one interface pointing *toward* the garden) |
| CLI (`vesper`) | operators, developers, testing without the garden |
| Web API | portals, integrations, anything HTTP |
| MCP | agents (Claude, future home-automation agents) — tools derive from the command manifest |
| stdio | embedding, scripting, line-delimited JSON |

The CLI is the rake pattern applied to devices: `vesper firefly status`, `vesper cricket play backup-done`, `vesper list`.

## What ports from the PoC

| PoC crate | Suzu crate | Notes |
|---|---|---|
| `companion-sdk/` | `sdk/` | SSE client, spawn/lifecycle, port pool. Replace `garden-common` imports with contract types. Fix the two-enabled-formats drift. |
| `companion-usb/` | `usb/` | udev + PollMonitor, per-device reader. Nearly verbatim (L14 cfg split preserved). |
| `firefly/` | `companions/firefly/` | Three device families. The identity handshake (write `I`, expect JSON in 4s) and its compile-time latency assert port whole. |
| `cricket/` | `companions/cricket/` | YAML tune format, CC0 samples, offline `test` subcommand. Port whole. |
| `scripts/` (firmware) | `firmware/` | RP2040 device side. Already a separate artifact. |

## What's genuinely new

- **The heal-moment vocabulary** — `health-degraded`, `health-healed`, `capture-committed`, `replanted`. These events emerge from the garden's converge loop and capture runner. They're the design work that makes companions *delightful* rather than merely functional: the garden's personality expressed as transitions, not just states.
- **The `vesper` CLI** — the rake pattern applied to devices. Manifest-driven, five transports, one semantic core.
- **Multi-protocol companions** — MCP + web API + stdio in addition to SSE. The companion declares its commands once; every interface derives from that declaration.

## What's NOT Suzu's job

- Managing offerings, storage, discovery, capture, or replant. Those are the garden's.
- Being always-on. A companion that isn't running is a companion that's resting. The garden doesn't care; it announces to whoever listens.
- Network security. The pond (ADR / M2) handles trust between stones; Suzu trusts the local moss and communicates over loopback.

## Constitution

Suzu follows the same laws as the garden (R0–R5, adapted for its scale):

- One envelope, one shape (B1 — the contract is the shape)
- Guests at the edge (B7 — companions own their devices and processes)
- Events inside, polling at the edge (L18 — SSE from the garden, never poll each other)
- Delight is load-bearing (B11 — this project IS the delight budget)
- Calm honesty (the garden whispers when it heals; Suzu makes the whisper audible)

## First slices

| Slice | What | Gate |
|---|---|---|
| S1 | Contract: event envelope JSON schemas + command manifest shape, fixture-tested | Schemas stable, tests green |
| S2 | SDK: spawn + SSE client + port pool (ported from PoC companion-sdk) | A companion receives garden events |
| S3 | Cricket: YAML tunes + audio playback (ported whole) | Plays a tune on a garden event, live |
| S4 | USB: companion-usb port (udev + PollMonitor) + firefly identity handshake | A firefly is recognized by plugging in |
| S5 | Firefly: pixel/fill/clear/brightness/animate on garden events | The garden's state is visible in light |
| S6 | CLI (`vesper`): list/status/control/enroll | Every companion behavior testable standalone |
| S7 | Web API + MCP server | Multi-protocol: every behavior reachable from every interface |
