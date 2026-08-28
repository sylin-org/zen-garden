# ADR-0006 — The Suzu contract: how the garden speaks to its companions

**Status:** Accepted · 2026-08-28
**Depends on:** ADR-0004 (discovery envelope), ADR-0005 §4 (sink banks), B7 (companions as guests at the edge), B11 (delight is load-bearing)
**Companion repo:** `sylin-org/suzu`
**Referenced by:** J4 (calm honest surfaces), the heal-moment vocabulary, the visibility epic

---

## Decision

Suzu is the garden's companion ecosystem — the small, standalone software
and hardware that makes the household *feel* the garden's presence. It
lives in its own public repo (`sylin-org/suzu`), is released independently,
and communicates with the garden through a small, versioned contract
defined here.

### The three types

#### 1. Event envelope — what the garden says

JSON, versioned, published on SSE. Every event carries a kind, a
timestamp, and the payload relevant to that kind.

```json
{
  "kind": "stone-seen",
  "proto": "zg/1",
  "ts": "2026-08-28T00:30:00Z",
  "stone": "stone-tranquil-pass",
  "health": "thriving"
}
```

| Kind | Meaning | Payload fields |
|---|---|---|
| `stone-seen` | a stone chirped | `stone`, `health` |
| `stone-goodbye` | graceful shutdown | `stone` |
| `stone-expired` | silence past threshold | `stone` |
| `offering-planted` | new offering running | `fqn`, `stem`, `stone` |
| `offering-rested` | desired-state stopped | `fqn`, `stone` |
| `offering-woke` | running again | `fqn`, `stone` |
| `offering-uprooted` | removed entirely | `fqn`, `stone` |
| `capture-committed` | checkpoint written | `fqn`, `run_id`, `final_hash` |
| `replanted` | incarnated from a checkpoint | `fqn`, `predecessor_id`, `final_hash` |
| `health-degraded` | reconcile exhausted patience | `fqn`, `stone` |
| `health-healed` | recovered from degraded | `fqn`, `stone` |

The last two kinds are the **heal-moment vocabulary** — they don't exist
as events yet; they emerge from the converge loop's transitions and are
the design work that makes companions delightful rather than merely
functional.

#### 2. Command manifest — what the companion says it can do

The companion declares its commands at startup via a `--dump-commands`
flag. Shape:

```json
{
  "companion": "firefly",
  "version": "1.0.0",
  "commands": [
    { "name": "status", "description": "Current device state" },
    { "name": "pixel", "description": "Set one pixel", "args": ["x", "y", "color"] },
    { "name": "fill", "description": "Fill all pixels", "args": ["color"] },
    { "name": "brightness", "description": "Set brightness", "args": ["level"] }
  ]
}
```

Moss reads this at companion registration. `rake hey firefly status`
proxies to the companion's HTTP server via this manifest.

#### 3. Identity handshake — how a USB/serial device proves it's a Suzu companion

Write the byte `I` to the serial port. Expect a JSON identity response
within 4 seconds:

```json
{
  "companion": "firefly",
  "family": "rp2040-matrix",
  "device_id": "GUIDv7",
  "firmware": "1.0.0",
  "pixels": 25
}
```

The 4-second deadline is a compile-time assert (the PoC's paranoia,
carried forward). A device that cannot answer in 4 seconds is not a
Suzu companion — it is a serial port that happens to be attached.

### The transports

Companions expose multiple interfaces from one semantic core:

| Transport | Audience | How |
|---|---|---|
| **SSE** | the garden (event push) | moss streams the event envelope; companion connects with backoff 1→32s |
| **CLI** | operators, developers, testing | `vesper` binary — the companion's own rake |
| **Web API** | portals, integrations | REST endpoints on the companion's loopback port |
| **MCP** | agents (the future) | companion as an MCP server; tools derive from the command manifest |
| **stdio** | embedding, testing | line-delimited JSON on stdin/stdout |

All transports speak the same semantic protocol. The transport is the
wire format; the contract is the meaning.

### The ownership model

Each companion process owns its own USB/serial devices. Moss does not
reach into companions — it sends events, assigns ports, proxies
commands, and tracks liveness. This is B7 (guests at the edge) applied
to the development model as well as the runtime model.

### The port pool

**7286–7295** (10 loopback ports) reserved for companion HTTP servers.
Declared in `contract/src/consts.rs`. Pool exhaustion = loud posture
degradation, never a crash.

---

## Consequences

### Positive
- Companions can be written in any language by anyone; the contract is
  JSON over HTTP/SSE, not a Rust trait.
- The public repo (`sylin-org/suzu`) opens the community feedback loop
  before RC.
- The garden's contract crate is the single source of truth for the
  envelope shape; companions parse documented JSON, not inferred structs.
- The heal-moment vocabulary gives the household *stories*, not just
  status lines.

### Negative
- Version coordination between the garden's contract crate and the
  published `companion-contract` crate (mitigated: the contract is small
  and stable; the envelope is versioned).
- No compile-time guarantee about companion behavior (mitigated: the
  command manifest is declared at registration, and the contract is
  fixture-tested).

### Neutral
- The SDK (spawn, SSE client, USB layer) is a convenience, not a
  requirement. A companion written in Python that speaks the contract
  is a valid companion.
- The firefly/cricket adapters are the first two companions; they prove
  the contract but don't limit it.

## References
- PoC: `src/poc/companion-sdk/`, `src/poc/companion-usb/`, `src/poc/cricket/`, `src/poc/firefly/`
- Inventory: `docs/v1/inventory/clients-companions.yaml` (all live-proven)
- ADR-0005 §4 (sink banks), §8 (storage rides discovery)
- Charter B7 (small kernel), B11 (delight budget), J4 (calm honest surfaces)
- `docs/v1/inventory/live-poc-harvest-2026-08-28/` (companion surfaces exercised live)
