# ADR-0014 — The channels law: MCP, CLI, API are mouths, not brains

**Status:** Accepted (2026-08-29)
**Trigger:** D5 (MCP surface, RC0-gated) + operator direction: "MCP, CLI,
API — they're all just channels that enter the same command pipeline.
If needed, break and rebuild to realign."

## Context

The PoC's drift diseases (three error shapes, parallel registries,
manifest fiction) were all the same disease: surfaces that thought.
v1's faces already delegate every operation to the application
services — the pipeline exists; nothing needed breaking.

## Decision

1. **One pipeline.** Every surface — HTTP faces, `rake` verbs, MCP
   tools, future portals — calls the same application services
   (`OfferingService`, storage, topology, jobs, the pulse bus). A
   surface renders and relays; it never decides.
2. **The MCP surface** (`crates/moss/src/mcp.rs`, POST `/mcp`) is
   Streamable HTTP's legal minimum: JSON-RPC `initialize`,
   `tools/list`, `tools/call` over POST; 405 for GET; no session
   state; notifications accepted with 202. Tools are the garden's
   verbs (observe, offerings, plant, rest, wake, uproot, capabilities,
   grow, jobs) with plain-English descriptions written for an AI.
3. **Tool outputs are the same envelopes the faces speak** (B1) —
   an AI that reads `observe` reads the room exactly as the wall does.
4. No SDK: the subset is small and stable, and owning the framing
   keeps the surface on the Face table like every other mouth.

## Consequences

- New operations gain MCP presence by adding ONE tool def that calls
  the pipeline — the same test the HTTP face already passes.
- Assistant-delight is now first-class: "plant redis and tell me when
  it's running" is a tool call, not screen-scraping.
- Mutations via MCP are journaled and audited exactly like HTTP ones
  (same pipeline, same ledger).
