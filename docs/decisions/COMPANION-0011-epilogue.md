---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-14
canonical: true
---

# COMPANION-0011: Epilogue — Book X of COMPANION-0001

**Date**: 2026-04-14
**Status**: Accepted — **implementation pending**
**Book**: X (closing) of [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)
**Depends on**: [COMPANION-0009](COMPANION-0009-companion-rebuild.md), [COMPANION-0010](COMPANION-0010-integration-tests.md)

## Context

Book X closes the epic. Its purpose is to verify that everything COMPANION-0001 committed to has actually landed — **not** to introduce new architecture. Where Books 0-IX build the system, Book X is the inspection pass that confirms:

- The pattern spec matches the live code.
- No scaffolds are active.
- CI enforces the invariants.
- Docs reflect the final state.
- The success criteria from COMPANION-0001 are all green.

This mirrors ARCH-0017's Book XX (Epilogue) pattern — a dedicated closing book that does doc work and enforcement, not new code.

## Decision

Execute Book X across four chapters of pure verification + documentation work.

### Chapter 1 — Scaffolding + CI check

**Deliverables**:
- Verify `docs/scaffolding.md` shows zero active `companion-*` entries (Book VIII Ch6 should have done this).
- Verify `scripts/check-scaffolding.sh` runs green across the full set of `companion-*` scaffold entries.
- Add a CI gate enforcing the scaffolding check on every PR touching `companion-sdk` or `firefly`/`cricket` (if a CI config exists; otherwise document the manual run).
- Add a CI gate enforcing `cargo test --package garden-companion-sdk --tests` (Book IX's integration tests).

**Exit criteria**: Scaffolding check runs in CI. Any reintroduced legacy import fails the build.

### Chapter 2 — Pattern spec / live code alignment

**Deliverables**:
- Re-read `docs/specs/companion-architecture.md` top to bottom.
- Diff every type shape / method signature against the actual `companion-sdk` code.
- Fix drift in the spec (or, if the code drifted from the spec in a justified way, amend the spec with a "design evolution" note).
- Validate every worked example in the spec compiles by turning them into doc-tests or checked examples under `examples/` where practical.
- Cross-check the cross-cutting-concerns matrix (§Cross-cutting concerns matrix) against the supervisor's actual behaviour.

**Exit criteria**: `docs/specs/companion-architecture.md` contains no falsehoods relative to the live code. All examples compile.

### Chapter 3 — Glossary + final documentation pass

**Deliverables**:
- Re-read `docs/glossary.md` COMPANION-0001 section — every term defined there must exist in the live code with the documented meaning.
- Add terms that emerged during implementation but weren't predicted (if any).
- Update `docs/reference/context-map.md` (if it exists) with the two companion bounded contexts.
- Update `docs/code-standards.md` with any conventions that hardened during the epic (e.g., "companion namespaces are `core.*` or `<crate>.*`", "adapters use mpsc receivers not broadcast").
- Add a top-level `docs/guides/companion-development.md` (or amend the existing guide if present) explaining how to write a new adapter from scratch. Link to the pattern spec and the three ADRs most relevant to adapter authors (Book VI, VII, VIII).

**Exit criteria**: Glossary, context map, code standards, and development guide reflect the final state.

### Chapter 4 — Success criteria verification + epic closure

Runs last. Verifies the ten success criteria in COMPANION-0001.

**Deliverables**:
- A checklist update appended to COMPANION-0001 §Success criteria with evidence for each item:
  1. Adapter count is data, not code → grep for `AdapterFactory` impls, count them per crate; verify no `device_type` dispatch remains.
  2. Device-type dispatch sites = 0 → `rg 'device_type ==|match device_type' src/firefly/` returns 0.
  3. Shared-mutex device-port pattern = 0 → `rg 'FireflyConnection' src/firefly/` returns 0.
  4. Integration test coverage > 0 → `cargo test --tests` shows the Book IX scenarios.
  5. PresenceSnapshot in garden-common::domain → grep verifies no duplicate struct in firefly/cricket.
  6. Every adapter's event interests declared → every `impl Adapter` has an explicit `profile()` with `subscriptions: &[...]`.
  7. Every command through Pulse → no `CommandHandler` impl exists in the tree.
  8. SDK usable by a third adapter → write a throwaway `PrometheusExporter` adapter in a scratch crate; verify it compiles against the SDK with no modifications.
  9. Pattern spec matches live code → Book X Ch2 verified.
  10. Scaffolding tracker: zero active companion entries → Book X Ch1 verified.

- Amend COMPANION-0001 status from `accepted-living` to `completed` and add the final closure entry to the revision history.
- Add a brief "Postmortem" section to COMPANION-0001 summarizing what went well, what didn't, and notes for the next epic (mirroring ARCH-0017's postmortem convention).

**Exit criteria**: COMPANION-0001 is marked completed. All ten success criteria are demonstrably met. The companion-integration-platform epic is closed.

## Implementation plan

Four chapters, one commit each, all doc / config / verification — zero new production Rust code. `cargo check --all` green at every chapter boundary; Book X should never break the build.

## Out of scope (deferred — post-epic ADRs)

Items COMPANION-0001's out-of-scope list promised to revisit after the epic:

- CloudEvents envelope alignment — separate ADR if external integration pressure appears.
- Config-driven adapter composition (TOML) — small follow-up ADR.
- Prometheus / observability output adapters — any contributor can add now.
- Event recording / time-travel debugging — beyond integration-test scope.
- Cross-stone event federation — future multi-stone work.
- Multi-language SDK bindings — build when a consumer needs one.
- Moss → companion presence notifications — orthogonal capability gap.
- Tune schema versioning (Cricket) — CRICKET-* namespace follow-up.

Each is left documented in COMPANION-0001 as future work. Book X does not expand or reopen these; it confirms the epic scoped them correctly.

## References

- [COMPANION-0001 §Success criteria](COMPANION-0001-companion-integration-epic.md#success-criteria) — the ten items Book X Ch4 verifies
- [COMPANION-0001 §Out of scope](COMPANION-0001-companion-integration-epic.md#out-of-scope) — deferred items
- [ARCH-0017 Book XX Epilogue](ARCH-0017-ddd-monolith-epic.md) — playbook for epic closure
- [docs/specs/companion-architecture.md](../specs/companion-architecture.md) — pattern spec reconciled in Ch2
- [docs/scaffolding.md](../scaffolding.md) — tracker verified empty of companion entries in Ch1
