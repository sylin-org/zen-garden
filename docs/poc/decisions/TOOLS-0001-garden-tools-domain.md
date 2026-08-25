---
audience: developer
doc_type: decision
status: current
last_verified: 2026-02-07
---

# TOOLS-0001: Unified Garden Tools Domain

**Date**: 2026-02-06
**Status**: Accepted

## Context

Offerings and seed banks each had their own discovery and status reporting. Automation clients (Rake, external tools) needed to query multiple endpoints and reconcile data manually to get a unified view of what's available in the garden.

## Decision

Introduce a `Tools` bounded context that provides a single normalized projection of all offerings and seed banks across the garden:

- **API**: `GET /api/v1/garden/tools` (snapshot), `GET /api/v1/garden/tools/stream` (SSE)
- **Propagation**: `TOOLS_BEACON` inter-stone beacon for garden-wide tool state
- **Wishful readiness**: `garden-rake find <query> wishfully` waits on the tools stream until the requested capability becomes available
- **Capability persistence**: Stone capabilities are persisted and propagated garden-wide

## Consequences

**Positive:**
- One API surface for all automation queries
- SSE stream enables event-driven UX (tools appear as they become ready)
- Wishful mode bridges the gap between "not yet ready" and "available"
- Capability-aware queries filter results by hardware features

**Negative:**
- Additional beacon adds to inter-stone traffic
- Stream semantics require careful client handling (reconnection, dedup)

## References

- API: `GET /api/v1/garden/tools`, `GET /api/v1/garden/tools/stream`
- User guide: [guides/tools-domain.md](../guides/tools-domain.md)
- Implementation: `src/moss/src/domain/tools/`
- Original spec: [archive/proposals/moss-tools-domain.md](../archive/proposals/moss-tools-domain.md)
- Implementation report: [archive/proposals/tools-domain-implementation.md](../archive/proposals/tools-domain-implementation.md)
