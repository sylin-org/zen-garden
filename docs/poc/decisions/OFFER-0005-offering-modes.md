---
audience: developer
doc_type: decision
status: current
last_verified: 2026-02-07
---

# OFFER-0005: Three Offering Modes (Managed, Adopted, Borrowed)

**Date**: 2026-01-21
**Status**: Accepted

## Context

Zen Garden started as a Docker-only orchestrator. But containers aren't always the right answer: GPU workloads suffer 10-20% overhead, existing native installations get duplicated, and network devices (NAS, printers) can't be containerized at all. Users needed a way to bring all three types of services into their garden.

## Decision

Define three offering modes, each with different levels of Moss control:

| Mode | What it is | Moss controls | Example |
|------|-----------|---------------|---------|
| **Managed** | Container deployed by Moss | Full lifecycle (start, stop, update, remove) | `garden-rake offer redis` |
| **Adopted** | Existing service (native or container) | Monitoring, optional restart | `garden-rake adopt ollama` |
| **Borrowed** | External network service | Announcement only | `garden-rake borrow nas://192.168.1.10` |

All offerings are manifest-driven with minimal required fields (4-6 lines). Detection uses a pluggable engine (command, HTTP, container probes).

**Terminology note**: The original proposal used "Planted" for the container mode. This was changed to "Managed" for alignment with industry terminology.

## Consequences

**Positive:**
- Garden can represent the full home infrastructure, not just containers
- Zero hardcoded service names — entirely manifest-driven
- Minimal manifests keep the common case simple
- Consistent API surface regardless of mode

**Negative:**
- Adopted/Borrowed modes have weaker lifecycle guarantees
- Detection engine adds complexity for edge cases
- Three modes means more states to handle in domain logic

## References

- Implementation: `src/moss/src/domain/offerings/`, `src/moss/src/infra/manifests/`
- Original proposal: [archive/proposals/offering-modes.md](../archive/proposals/offering-modes.md)
- Refactoring plan: [archive/proposals/offering-modes-refactoring-plan.md](../archive/proposals/offering-modes-refactoring-plan.md)
