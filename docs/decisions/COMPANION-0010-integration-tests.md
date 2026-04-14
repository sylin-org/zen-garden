---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-14
canonical: true
---

# COMPANION-0010: Integration Testing Foundation — Book IX of COMPANION-0001

**Date**: 2026-04-14
**Status**: Accepted — **implementation pending**
**Book**: IX of [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)
**Depends on**: [COMPANION-0008](COMPANION-0008-companion.md), [COMPANION-0007](COMPANION-0007-adapters.md), [COMPANION-0009](COMPANION-0009-companion-rebuild.md)

## Context

Per COMPANION-0001 §Exit criteria:

> 9. Pattern spec matches live code with no drift.
> 10. Scaffolding tracker: zero active companion entries.

Book IX addresses a related success metric: **zero integration tests** is where the epic started; the epic should leave the companion segment with a maintainable integration testing foundation. Individual books have been disciplined about unit tests (144 companion-sdk tests as of Book VII close), but nothing yet exercises the full SSE → Pulse → Garden → Adapter pipeline in a single scenario.

The architecture is uniquely well-suited to integration testing because every layer was designed to be driven from a test harness:

- **Events are a uniform envelope** — easy to synthesize in a test.
- **Pulse is independent** of transports — tests can publish directly.
- **Garden projects from events** — deterministic, no I/O.
- **Adapters receive mpsc filtered by profile** — the supervisor's filter task is the only thing a test needs to exercise.
- **Transports implement a single trait** — `MockTransport` is a trivial impl.

Book VI already ships a workable `MockAdapter` inside its test module (the `RecordingAdapter` used by the supervisor tests); Book VII ships `EchoAdapter` (used by the end-to-end Companion test). Book IX **extracts and publishes** these as first-class test utilities in the SDK, then **scripts realistic scenarios** end-to-end.

## Decision

Introduce an `integration_tests` module / features / fixtures in `companion-sdk` plus `garden-common`, and add a `tests/` directory at the SDK crate root for end-to-end scenarios.

### Deliverables

1. **`MockTransport`** (in `companion-sdk::garden::testing` or a `testing` sub-module behind a test-only feature flag) — a `Transport` impl that publishes pre-canned events to Pulse on a schedule. Used to simulate moss without standing up an HTTP server. Emitted kinds configurable at construction.

2. **`RecordingAdapter`** — extracted from Book VI's test fixture. A stable public type that records every event it receives. `AdapterProfile::subscriptions` parameterised at construction. Exposes `received() -> Vec<Event>` for test assertions.

3. **`FakeFactory<A>`** — a trivial factory that returns a single pre-configured adapter on discover. Parameterised by the adapter it produces.

4. **`TestHarness`** — a lightweight struct that bundles a `Companion`, a handle to publish events into its Pulse, a handle to query the Garden state, and a shutdown control. Simplifies scenario setup to a few lines.

5. **Integration test scenarios** at `src/companion-sdk/tests/`:
   - `full_pipeline_snapshot_to_render.rs` — publish a `PresenceSnapshot` via MockTransport; verify Garden state updates; verify a `RecordingAdapter` receives the matching `GardenSnapshot` synthetic event.
   - `command_round_trip.rs` — end-to-end POST to CommandTransport, adapter publishes CommandResult, HTTP response returned.
   - `coalescing_load_updates.rs` — publish 100 `LoadUpdated` events in rapid succession; verify only one is delivered after the flush interval.
   - `adapter_lifecycle.rs` — factory toggle reveals / hides an adapter; supervisor spawns / reaps on discovery tick; state persistence survives respawn.
   - `subscription_filtering.rs` — adapter with limited subscriptions receives only matching kinds; other kinds are filtered out by the supervisor's filter task.
   - `shutdown_completeness.rs` — all tasks (pulse flush timer, Garden projection, Adapters supervisor, transports) exit cleanly within the bounded join window.

6. **Test harness docs** in `docs/guides/companion-integration-testing.md` — a short guide for future contributors writing scenarios.

### Organisation

- `companion-sdk` gains a `testing` submodule. Compiled only under `cfg(test)` + a dev-only `testing` feature so it's available to integration tests in `tests/` but does not bloat the production binary.
- Real integration tests live at `src/companion-sdk/tests/` (Rust's standard crate-level integration-test location). Each file is a self-contained scenario.
- `garden-firefly` and `garden-cricket` (after Book VIII) get their own narrow integration tests against mock Pulse / MockAdapter to validate their real adapters.

## Implementation plan

**Chapter 1** (this ADR) — land this document.

**Chapter 2** — `companion-sdk::testing` module:
- `MockTransport`, `RecordingAdapter`, `FakeFactory`, `TestHarness`
- Unit tests for each fixture (the fixtures themselves need tests)

**Chapter 3** — integration scenarios:
- Six scenario files at `src/companion-sdk/tests/`
- Each demonstrates one architectural property (full pipeline, command round-trip, coalescing, lifecycle, filtering, shutdown)

**Chapter 4** — docs + book close:
- Integration testing guide
- Update `docs/code-standards.md` if needed
- Amend COMPANION-0001 revision history

Ships green to `dev` per chapter.

## Exit criteria

1. `cargo test --package garden-companion-sdk --tests` runs all six integration scenarios.
2. `companion-sdk::testing::{MockTransport, RecordingAdapter, FakeFactory, TestHarness}` are usable from `garden-firefly` and `garden-cricket`'s own test suites.
3. `docs/guides/companion-integration-testing.md` exists with examples.
4. `cargo check --all`, `cargo clippy --all -- -D warnings` green.
5. COMPANION-0001 revision history amended.

## Out of scope (deferred)

| Item | Deferred |
|------|---------|
| Property-based fuzzing of the Pulse orchestrator | Post-epic |
| Load / stress tests for the supervisor under 100+ concurrent adapters | Post-epic |
| Snapshot-based UI tests for OLED / matrix rendering | Follow-up — requires hardware-in-the-loop fixture |
| Integration tests against a real moss via SSE | Manual validation in Book VIII Ch5 and later; automated requires an embedded moss fixture |

## References

- [COMPANION-0001 §Success criteria](COMPANION-0001-companion-integration-epic.md#success-criteria) — item 4 (integration test coverage > 0)
- [COMPANION-0008 integration tests](COMPANION-0008-companion.md) — the `end_to_end_command_dispatches_through_companion` test is the template for Book IX scenarios
- [COMPANION-0007 supervisor tests](COMPANION-0007-adapters.md) — the `RecordingAdapter` fixture is extracted from here
