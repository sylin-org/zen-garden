---
audience: developer
doc_type: decision
status: current
last_verified: 2026-02-07
---

# OFFER-0004: Intelligent Offering Placement

**Date**: 2026-01-23
**Status**: Accepted

## Context

When installing an offering, users had to manually choose which stone to target. In a multi-stone garden, this requires knowing each stone's architecture, available resources, and existing workload — information that Moss already has.

## Decision

Implement multi-factor scoring for automatic stone selection via `garden-rake offer <name> somewhere`.

The scoring algorithm evaluates three dimensions:

| Factor | Weight | What it measures |
|--------|--------|------------------|
| Compatibility | Pass/fail gate | Architecture match, manifest requirements |
| Resources | Weighted score | Available memory, CPU, storage headroom |
| Distribution | Weighted score | Spread workload across stones |

The tended stone aggregates metrics from all peers, scores them, and returns ranked recommendations.

## Consequences

**Positive:**
- Users get intelligent placement with a single word (`somewhere`)
- Quiet mode (`somewhere quietly`) enables fully automated pipelines
- Exclusion summaries explain why stones were filtered out

**Negative:**
- Requires tended stone to have topology visibility (depends on discovery)
- Real-time metrics accuracy depends on peer responsiveness

## References

- API: `POST /api/v1/garden/recommend`
- CLI: `garden-rake offer <name> somewhere [quietly]`
- Implementation: `src/moss/src/domain/placement.rs`
- Original proposal: [archive/proposals/intelligent-offering-placement.md](../archive/proposals/intelligent-offering-placement.md)
