---
audience: developer
doc_type: decision
status: current
last_verified: 2026-02-07
---

# ARCH-0001: SoC/DDD Architecture for Moss

**Date**: 2026-01-20
**Status**: Accepted

## Context

Moss started as a single `main.rs` file that grew to 3,976 lines. All concerns — HTTP routing, Docker integration, configuration, background tasks, domain logic — were interleaved. This made the codebase difficult to navigate, test, and extend.

## Decision

Adopt Separation of Concerns (SoC) and Domain-Driven Design (DDD) as the primary architectural principles. Restructure moss into clearly separated layers:

| Layer | Directory | Responsibility |
|-------|-----------|----------------|
| Domain | `domain/` | Pure business logic, no external deps |
| Infrastructure | `infra/` | External integrations (Docker, filesystem, network) |
| API | `api/` | Thin HTTP handlers, request/response mapping |
| Bootstrap | `bootstrap/` | Initialization, configuration, wiring |
| Tasks | `tasks/` | Background operations, schedulers |

**Key rule**: Domain never imports infrastructure. Infrastructure implements domain traits.

## Consequences

**Positive:**
- `main.rs` reduced to 45 lines (99% reduction)
- Each module has a single reason to change
- Domain logic is testable without mocks or external services
- New features follow clear placement rules

**Negative:**
- More files to navigate (74 focused modules vs 1 monolith)
- Cross-cutting concerns require careful trait design

## References

- Original proposal: [archive/proposals/rust-refactoring-proposal.md](../archive/proposals/rust-refactoring-proposal.md)
- Extraction plans: [archive/proposals/main-rs-extraction-plan.md](../archive/proposals/main-rs-extraction-plan.md), [archive/proposals/main-rs-final-extraction.md](../archive/proposals/main-rs-final-extraction.md)
