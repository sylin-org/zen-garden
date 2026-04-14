---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-14
canonical: true
---

# COMPANION-0009: Companion Rebuild — Book VIII of COMPANION-0001

**Date**: 2026-04-14
**Status**: Accepted — **implemented 2026-04-14**
**Book**: VIII of [COMPANION-0001](COMPANION-0001-companion-integration-epic.md)
**Depends on**: [COMPANION-0008](COMPANION-0008-companion.md) (Companion runtime), [COMPANION-0007](COMPANION-0007-adapters.md), [COMPANION-0006](COMPANION-0006-garden-aggregate.md), [COMPANION-0005](COMPANION-0005-domain-types.md), [COMPANION-0004](COMPANION-0004-transport.md)

## Context

Book VIII is the largest book in COMPANION-0001: it replaces the firefly and cricket crates wholesale, migrating both companion binaries from the legacy `CompanionRuntime` + `SseClient` + `CommandHandler` machinery to the new `Companion` + `Transport` + `Adapter` architecture shipped in Books II–VII.

Per the Discovery Mandate, Ch0 re-evaluated against the live code. Findings:

### What the re-evaluation found

1. **Scale is real.** Firefly is ~3,539 LOC today (main / events / handler / serial / animation / oled / tdisplay); cricket is ~1,760 LOC (main / events / handler / mixer / manifest / test_mode). A faithful rewrite is on the order of 3,000–5,000 LOC of new adapter code + deletion of the old. This is on par with a full ARCH-0017 book in moss, and benefits from careful incremental landing rather than a single all-at-once commit.

2. **Per-crate dependencies already in place.** Cricket already has `rodio` for audio, `notify` for tune reload, `rust-embed` for baked-in tune assets. Firefly has `serialport`, `rand`. Book VIII keeps these — adapters are thin wrappers around the existing device-handling primitives.

3. **Parallel chapters are independent.** Ch1-4 rewrite firefly's four adapters; Ch5 rewrites cricket's single adapter. They touch different files. Within firefly, the adapters share `serial.rs`'s low-level port primitives (salvaged from the existing `FireflySerial`) but are otherwise independent. Any chapter order is valid; the book closes when all five adapters exist + Ch6 removes the old machinery.

4. **Scaffolding entries are additive.** The three scaffolds introduced by Book III (`companion-old-sse-client`, `companion-old-command-handler`, `companion-old-companion-runtime`) all target Book VIII Ch6 for removal. Firefly + cricket continue to import from the legacy paths until Ch6 swings them over — which requires all five adapter chapters to be complete. Therefore Ch6 is strictly last within the book.

5. **Harvest catalogue is well-understood.** [COMPANION-0001's harvest audit](COMPANION-0001-companion-integration-epic.md#the-harvest-audit) itemises every existing type: keep (rename), salvage (extract logic, discard shape), delete (root causes). Book VIII follows that catalogue chapter by chapter.

6. **Shippability under break-and-rebuild.** Intermediate chapters can produce a firefly binary that runs only the subset of adapters that have been migrated (the others simply have no factory registered yet). The binary still compiles, still runs, still serves HTTP commands; the unmigrated devices get "no adapter factory matches" at discovery — acceptable `dev` state mid-book per COMPANION-0001 §Shippability Rule.

No plan change vs COMPANION-0001. Scope holds.

### Process refinement

Given the scale, Book VIII is the first book in the epic where **each chapter lands as a self-contained commit with its own green-to-`dev` gate**, rather than a single book-closing commit at Ch6. This keeps bisect-friendly history even if a chapter introduces a regression. Ch6 is the only chapter that must strictly wait for its predecessors (all adapters must exist before the legacy paths can be removed).

## Decision

Execute Book VIII across six chapters. Each chapter is a complete green-to-`dev` landing.

### Chapter 1 — Firefly `main.rs` + `RpMatrixAdapter`

**Deliverables**:
- `src/firefly/src/main.rs` rewritten as a ~25-line `Companion::new("firefly").with_transport(...).with_adapter_factory(RpMatrixFactory).run()` builder. Legacy `CompanionRuntime::new(...).command_handler(FireflyCommands).run()` deleted.
- `src/firefly/src/adapters/matrix.rs` implementing `RpMatrixAdapter` — owns the serial port, animation engine (salvaged from `animation.rs`), override cycling (salvaged from Book 1 investigation fixes), duo-color logic.
- `src/firefly/src/adapters/mod.rs` registering `RpMatrixFactory`.
- Thin `src/firefly/src/serial.rs` (renamed to `FireflyPort`) with the hot-unplug read-loop fix preserved.
- Legacy `FireflyConnection` + `with_device` pattern deleted within this crate.
- Matrix tests ported / added: animation FSM unit tests, cycling health override, duo-color probability.

**Exit criteria**:
- `garden-firefly` binary builds; `cargo clippy -- -D warnings` green.
- With only `RpMatrixFactory` registered, a companion run against a live RP2040-Matrix device renders the baseline firefly animation and responds to SSE events that trigger matrix-visible effects.
- Other firefly device types (OLED v1/v2, T-Display) have no adapter yet; plugging one in is a no-op at the framework level (discovery finds the device, no factory claims it).

### Chapter 2 — `OledV1Adapter`

**Deliverables**:
- `src/firefly/src/adapters/oled_v1.rs` implementing `OledV1Adapter` against the ESP8266 v1 firmware's S / H / M / WIPE-IN protocol.
- Salvages the existing serial commands and state-replay-on-reconnect logic; discards the shared-mutex `FireflyConnection` pattern.
- Registered in `src/firefly/src/main.rs` alongside `RpMatrixFactory`.
- Tests: snapshot rendering, health transitions, offering-count updates, reconnect cached-state replay.

**Exit criteria**: OLED v1 device renders correctly; green to `dev`.

### Chapter 3 — `OledV2Adapter`

**Deliverables**:
- `src/firefly/src/adapters/oled_v2.rs` implementing the dense icon dashboard protocol (D / G / M commands, activity spinner, seed-bank icon logic from Books 0-1 UX work).
- Registered in main.
- Tests: dashboard updates via `D` command, activity spinner advance on event, seed-bank icon toggling.

**Exit criteria**: OLED v2 device renders correctly; green to `dev`.

### Chapter 4 — `TDisplayAdapter`

**Deliverables**:
- `src/firefly/src/adapters/tdisplay.rs` implementing the T-Display JSON-push + incremental-load protocol (`J,{json}`, `L,cpu,mem,disk,...`).
- Registered in main.
- Tests: full JSON snapshot push, incremental `L,...` updates, reconnect cache restoration.

**Exit criteria**: T-Display device renders correctly; green to `dev`.

### Chapter 5 — Cricket `main.rs` + `AudioAdapter`

**Deliverables**:
- `src/cricket/src/main.rs` rewritten as a ~20-line `Companion::new("cricket").with_transport(...).with_adapter_factory(AudioFactory).run()` builder.
- `src/cricket/src/adapters/audio.rs` implementing `AudioAdapter` — owns the mixer + tune manifest, subscribes to event kinds defined by the active tune, plays audio via `rodio::Sink`, honors debounce via `AdapterProfile::delivery = DeliveryPolicy::Debounced(...)` (types only in V1 — adapter internally implements debounce until supervisor enforcement lands).
- Tune manifest schema validated at load via `serde`; silent-breakage risk closed by typed domain events (subscribe by `core.*` kind, not string).
- `AudioFactory::required_dependencies` declares `libasound` for Linux.
- Legacy `cricket/src/events.rs`, `cricket/src/handler.rs` deleted.
- Tests: tune load + validate, event → mapping resolution, debounce timing, mixer interaction.

**Exit criteria**: Cricket binary produces audio for garden events; green to `dev`. Validates that the architecture works for non-hardware adapters.

### Chapter 6 — Legacy deletion + scaffold removal

Runs only after Chapters 1-5 are complete.

**Deliverables**:
- Delete `src/companion-sdk/src/sse.rs`, `src/companion-sdk/src/handler.rs`, `src/companion-sdk/src/server.rs`, `src/companion-sdk/src/runtime.rs`.
- Remove the corresponding `pub mod` declarations + re-exports from `src/companion-sdk/src/lib.rs` and prelude.
- Mark the three Book III scaffolds as `status: removed` in `docs/scaffolding.md` with the cleanup commit hash.
- Run `scripts/check-scaffolding.sh` — all check patterns return zero matches.
- Update COMPANION-0007 (scaffolding notes) to reflect the now-removed state.

**Exit criteria**: `cargo clippy -- -D warnings` green with no references to `SseClient`, `CommandHandler`, `CompanionRuntime`. Scaffolding tracker shows zero active entries in the companion namespace. COMPANION-0001 revision history amended with Book VIII closure.

## Implementation sequencing (recommended)

Although chapters 1-5 may land in any order, the recommended sequence for risk management is:

1. **Ch5 (Cricket)** first — smallest, simplest, validates the pattern end-to-end on a non-hardware adapter. Locks in muscle memory for the remaining chapters.
2. **Ch1 (Matrix)** — most complex device logic; do it with fresh eyes after cricket primes the approach.
3. **Ch2-4 (OLED v1, v2, T-Display)** — similar shape to matrix; can proceed in any order.
4. **Ch6 (cleanup)** — last.

This is a recommendation, not a constraint. The actual order depends on which adapter is most critical to user testing and oversight availability.

## Exit criteria

1. `src/firefly/src/main.rs` is a ~25-line `Companion` builder.
2. `src/cricket/src/main.rs` is a ~20-line `Companion` builder.
3. Every firefly device type has a corresponding adapter under `src/firefly/src/adapters/`.
4. `AudioAdapter` exists at `src/cricket/src/adapters/audio.rs`.
5. `SseClient`, `CommandHandler`, `CompanionRuntime`, and legacy `server.rs` are deleted from `companion-sdk`.
6. `docs/scaffolding.md` shows zero active `companion-*` entries.
7. `cargo check --all`, `cargo test`, `cargo clippy -- -D warnings` all green.
8. A manual sanity run: plug each firefly device type into a stone; verify the new adapter renders correctly. Run cricket with a stone and verify audio plays for service events.
9. COMPANION-0001 revision history amended with Book VIII closure.

## Out of scope (deferred)

| Item | Deferred |
|------|---------|
| `DeliveryPolicy::LatestEvery` + `Debounced` supervisor enforcement (adapters implement internally for V1) | COMPANION-0007 carry-over; still post-epic |
| Typed state persistence I/O | COMPANION-0007 carry-over |
| `/status` HTTP endpoint exposing `Adapters::status()` | Follow-up ADR (small) |
| Firefly installer updates reflecting new binary shape | Nice-to-have; installer already abstracts over the binary |

## References

- [COMPANION-0001 §The Harvest Audit](COMPANION-0001-companion-integration-epic.md#the-harvest-audit) — keep/salvage/delete catalogue
- [COMPANION-0001 §The Book List](COMPANION-0001-companion-integration-epic.md#the-book-list) — book scope
- [scaffolding.md §Active scaffolds](../scaffolding.md#active-scaffolds) — three `companion-old-*` entries cleared by Ch6
- [COMPANION-0007](COMPANION-0007-adapters.md) — Adapter / AdapterFactory / Adapters supervisor contract
- [COMPANION-0008](COMPANION-0008-companion.md) — Companion runtime
