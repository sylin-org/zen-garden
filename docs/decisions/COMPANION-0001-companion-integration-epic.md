---
audience: [developer, ai]
doc_type: decision
status: accepted-living
last_verified: 2026-04-13
canonical: true
---

# COMPANION-0001: Companion Integration Platform — Event-Driven Extension Architecture

**Date**: 2026-04-13
**Status**: Accepted (living)
**Depends on**: [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md) (DDD aggregate pattern), [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) (moss epic playbook this inherits from), [ARCH-0037](ARCH-0037-appstate-dissolution.md) (Moss struct naming precedent)

## Revision history

Unlike typical ADRs which are immutable after acceptance, COMPANION-0001 is a **living plan** that evolves as the epic progresses. Every amendment is logged here with date, trigger, and scope. See [The Discovery Mandate](#the-discovery-mandate) for the rule that authorizes amendments.

| Date | Change | ADR |
|------|--------|-----|
| 2026-04-13 | Initial acceptance. Epic commits to 11-artifact arc (Prologue + Books I–IX + Epilogue). | — |

## Context

The companion segment of Zen Garden — the SDK and consumer crates (`companion-sdk`, `firefly`, `cricket`) — grew organically from the needs of the first two companions: a visual LED/OLED indicator (Firefly) and an audio event sonifier (Cricket). Each companion wired its own SSE consumer directly to its own rendering loop; the SDK provided runtime scaffolding but no event bus, no extension contract, no shared domain model.

The composite effect is a codebase where:

- **`PresenceSnapshot` is defined privately inside firefly** (`src/firefly/src/events.rs`), not in `garden-common`. Cricket doesn't deserialize snapshots at all — it matches events on raw string keys. Schema drift on the moss side is a silent-breakage risk for cricket and a compile-break for firefly with no shared canonical type.
- **Device-type dispatch is scattered across firefly** — ~33 sites (`match device_type`, `if device_type == X`, `matches!(device_type, X | Y)`) across `events.rs`, `main.rs`, `animation.rs`, `serial.rs`. Adding a new device variant requires surgery across four files with no compiler help on missed sites.
- **No internal event bus exists.** Both firefly and cricket wire SSE events straight through to device I/O. There is no deduplication, no coalescing, no validation, no backpressure, no fan-out discipline. The replug deadlock fixed just before this epic (`src/firefly/src/serial.rs` read-loop) existed in part because one I/O call could wedge the entire event pipeline via a shared `Mutex`.
- **Zero integration tests.** The full pipeline — SSE reception through event handling through device commands — has no automated coverage in either companion. Hardware-dependent code paths are structurally untestable.
- **Device connection is a shared mutex.** `FireflyConnection` wraps a `Mutex<Option<FireflySerial>>`; every consumer (event handler, HTTP command server, reconnect task) goes through `with_device` holding the outer lock during the serial I/O. One slow call blocks everyone.
- **Commands flow through a parallel pathway** (`CommandHandler` trait, `CompanionRuntime` HTTP server) that is conceptually symmetric with event handling but structurally unrelated. Every companion reimplements arg parsing, validation, and device dispatch from raw strings.

Each of these problems was addressable in isolation. The composite problem is that the companion segment **lacks a coherent architecture** — the abstractions a second and third companion would want already have to exist, but don't. This epic builds them, deliberately and from first principles.

ARCH-0017 proved the DDD aggregate pattern is the right answer for moss. COMPANION-0001 applies the same discipline to the companion segment, scaled appropriately: two bounded contexts instead of eight, and a hexagonal architecture where transports and adapters are ports around a pure event-driven core.

The goal of this epic is a **companion runtime that is a first-class extension platform**, not a device-driver framework. Hardware adapters (Firefly's four device types, Cricket's audio sink) are the first consumers. Observability exporters, external-system bridges, and non-hardware integrations are future consumers that this architecture makes possible without further SDK work.

## Decision

The companion segment adopts an event-driven, hexagonal architecture codified through this epic across 11 artifacts (Prologue + Books I–IX + Epilogue). The pattern is codified in [companion-architecture.md](../specs/companion-architecture.md) and enforced by CI in the epilogue.

### The Principles

The epic commits to seven principles. Every book is judged against them.

#### 1. Companions are event-consuming extensions

A companion is a local-process extension of a Stone that observes the garden's event stream and produces local effects. "Local effects" includes device I/O (hardware adapters), system I/O (audio sinks, file watchers), and external I/O (webhooks, metric exporters). The companion runtime is **not** a device driver framework; it is an event integration hub whose first consumers happen to be device drivers.

This reframes every downstream design decision: adapters are composable extensions, not integral components. Adding a fourth firefly device type and adding a Prometheus exporter are the **same operation** (register an `AdapterFactory`) under this model.

#### 2. Two bounded contexts

The SDK consists of two bounded contexts that collaborate only through explicit contracts:

- **Garden** — event ingestion, canonicalization, projection to read-model state, and fan-out to subscribers. Owns the transport layer, the event orchestrator (`Pulse`), and the client-side `Garden` aggregate.
- **Adapters** — extension lifecycle management. Owns the `Adapter` trait, `AdapterFactory` registry, device discovery, supervisor loop, and per-adapter cross-cutting concerns (subscription filtering, delivery policy, dependency installation, structured logging, state persistence, grace windows).

Adapters consume Garden-context events through subscription; the Adapters context never reaches into Garden state directly. Garden never knows which adapters exist. The SDK wires them together at the `Companion` top-level struct.

#### 3. Uniform event envelope

Every event in the system has one shape:

```rust
pub struct Event {
    pub id: EventId,                          // GUIDv7 — time-ordered, globally unique
    pub timestamp: DateTime<Utc>,
    pub kind: &'static str,                   // namespaced identifier (see §4)
    pub payload: Arc<dyn EventPayload>,       // type-erased, downcastable
}
```

Presence events from moss, commands from HTTP, inter-adapter events, and future external-source events all conform. Extensions (including third-party adapters) emit events with the same shape. This uniformity is what makes recording, replay, metrics, distributed tracing, and cross-language bindings tractable without special cases.

#### 4. Namespaced kinds

Event kinds are strings with a strict namespace convention:

- `core.<domain>.<event>` — SDK-defined events (e.g., `core.stone.health.changed`, `core.service.started`)
- `<companion>.<subject>.<event>` — companion-scoped events (e.g., `firefly.matrix.override.started`, `cricket.tune.selected`)
- `<companion>.command.<action>` — commands translated from HTTP (e.g., `firefly.command.brightness`)

Core reserves `core.*`. Adapters emit under their companion's namespace. Collisions are architecturally impossible if the convention is followed; enforced at CI in the epilogue.

#### 5. Pulse is the single fan-in point

The `Pulse` orchestrator is the **only** path an event takes from transport to subscribers. It owns:

- **Deduplication** by `EventId` (bounded LRU cache)
- **Validation** of kind namespace and payload shape
- **Coalescing** for state-delta events declared via `EventPayload::COALESCING`
- **Fan-out** to subscribers
- **Backpressure policy** on subscriber lag
- **Metrics** (ingested, deduped, coalesced, delivered, dropped)

Adapters receive a **canonical, deduplicated, validated, correctly-ordered stream** and can treat it as a source of truth. This guarantee is the foundation all adapter-side simplifications depend on.

#### 6. Garden is a read-only CQRS projection

The `Garden` aggregate is the client-side projection of moss's published state. It exposes two surfaces:

- **Properties** — typed queries for current state: `garden.health()`, `garden.offerings()`, `garden.seed_bank()`, `garden.load()`. These are always up-to-date synchronously.
- **Event stream** — `garden.events()` returns a `broadcast::Receiver<Event>` for reacting to changes.

Domain types (`Stone`, `Offering`, `Health`, `Load`, `SeedBank`, `Pond`) live in `garden-common::domain` and are shared with moss. Companions never see JSON or SSE; they see the same typed aggregates moss operates on.

#### 7. Transport is an implementation detail

Adapters never depend on transport specifics. Whether an event arrived via moss's SSE stream, an HTTP command request, a sibling adapter publishing into the bus, or a future MQTT subscription is invisible at the adapter layer. The `Transport` trait is a port; `SseTransport` and `CommandTransport` are the initial implementations. Adding a new event source means implementing `Transport` and registering the implementation — no other code changes.

### The Tenets

Two methodological commitments that bind every book in the epic.

#### Tenet: Break-and-rebuild over migrate-in-place

Where existing shape prevents a clean design, we rebuild. Harvest proven logic; discard the shape that constrains it. The companion segment is ~6,571 LOC total — small enough that replacing a crate is cheaper than maintaining long-lived coexistence scaffolding.

In practice this means: Book VIII replaces the firefly and cricket crates wholesale, not by strangler migration. Intermediate chapters produce a binary that runs with a subset of adapters (acceptable on `dev` mid-book); the final chapter removes the old code in one commit. No permanent scaffolding, no compatibility shims, no parallel code paths.

This differs from ARCH-0017 (which used strangler-style migration extensively). The difference is scale: moss had consumers across the tree that couldn't all be flipped atomically. The companion segment has one binary per companion and no external consumers of its internals.

#### Tenet: Cross-cutting concerns live at the layer that owns them

If three adapters would implement the same pattern, it belongs at a higher layer. The adapter trait stays small — adapters own only what is device-specific. Generalizing the insight from our prior deduplication decision: any concern that could be solved differently (and inconsistently) by each adapter is a design smell pointing at a missing abstraction.

The epic commits to lifting these concerns into the supervisor / orchestrator layers:

- Event subscription filtering (adapters declare interests)
- Per-adapter delivery policy (coalesce / debounce / all)
- State hydration on spawn (synthetic `GardenSnapshot` event)
- Command-response correlation (at `CommandTransport`, not adapter)
- Structured logging context (tracing spans injected by supervisor)
- RAII cleanup (Drop, not shutdown hooks)
- Dependency declaration (at `AdapterFactory`, not adapter code)

Full list and rationale in [companion-architecture.md](../specs/companion-architecture.md).

### The Scaffolding Contract

Inherited from ARCH-0017. Under the break-and-rebuild tenet, scaffolds should be rare. Any intermediate-state code introduced during the epic is tracked in [scaffolding.md](../scaffolding.md) with an ID under the `companion-*` namespace, a removal trigger book, and a concrete removal action. The `scripts/check-scaffolding.sh` validator (written for ARCH-0017) applies to companion-epic scaffolds without modification.

The goal, consistent with ARCH-0017: **zero active scaffold entries on the day the epilogue ships.**

### The Shippability Rule

Every book, at the final chapter commit, merges green to `dev`. No cross-book atomicity. No long-lived epic branch. No "two books land together." CI on `dev` runs `cargo check --all`, `cargo test --package garden-companion-sdk --package garden-firefly --package garden-cricket`, `cargo clippy -- -D warnings`, and the scaffolding-tracker check. A book that cannot land on `dev` green is rolled back until it can.

Book VIII (Companion Rebuild) is the only book where chapter-level green-to-dev is weaker: intermediate chapters may produce a firefly or cricket binary that operates with a subset of adapters migrated. This is acceptable because the binaries still compile, still run, still serve the migrated subset, and the final chapter restores full capability.

### The Rename Mandate

Methods, files, directories, types, and modules are all up for rename. Where a name violates the ubiquitous language or code-standards §3 (no architectural suffixes), it changes. Anticipated renames (full list in [harvest audit](#the-harvest-audit)):

- `CompanionRuntime` → `Companion` (drops `Runtime` suffix per §3)
- `SseClient` → `SseTransport` (reflects role as `Transport` impl)
- `FireflyConnection` — **dissolved** (shared-mutex pattern is the bug; adapters own their own port)
- `FireflySerial` → `FireflyPort` (shorter, role-accurate)
- `FireflyDeviceType` — **dissolved** (each variant becomes its own `AdapterFactory`)
- `EventHandler` trait — **dissolved** (replaced by `Adapter::run` receiving events directly)

Renames happen inside the book that touches the file. Pure `git mv` goes in its own commit per code-standards §14.

### The Discovery Mandate

The plan in this document is a **hypothesis**, not a contract. It was written after a structured discovery pass of the companion segment, but not every line of every adapter has been re-read with clean-architecture eyes. As each book opens, the author (human or AI) **re-evaluates the hypothesis against the actual code** and is expected — mandated — to change the plan when the code teaches them something the plan did not anticipate.

**The mandate in one sentence:** If, while working on a book, you discover that the plan is wrong, **stop, put on a clean-architecture specialist hat, ask "what would the right shape actually look like?", and change the plan, the code, or both.**

Concrete triggers that warrant a plan change, rules for plan changes, and guidance for when NOT to change the plan follow the same conventions as ARCH-0017 §"The Discovery Mandate" — not restated here to keep this ADR focused.

### The Harvest Audit

Under break-and-rebuild, every existing artifact gets classified. The result of the initial discovery pass:

#### ✅ Keep (proven; rename where the new home suggests a better name)

| Old identity | New home | Rename |
|---|---|---|
| `CompanionRuntime` | `Companion` (SDK top-level) | ✓ drops `Runtime` suffix |
| `SseClient` (reconnect + backoff + parser, recent multi-line fix) | `SseTransport` (impl of `Transport`) | ✓ reflects role |
| SSE multi-line parser | `SseTransport::parse_event` | Keep verbatim |
| `CompanionState` (on/off persistence) | Field on `Companion` | Absorb |
| `CompanionConfig` + `validate_daemon` | Keep as-is | None |
| `CommandDef`, `CommandArg`, `CommandManifest`, `CommandResponse` | Keep in `garden-common::command_manifest` | None |
| `check_dump_commands` | Keep | None |
| `SystemDependency`, `ensure_dependencies` | Called from supervisor via `AdapterFactory::required_dependencies` | Relocate invocation |
| `FireflySerial` (low-level port I/O) | `firefly::port::FireflyPort` | ✓ shorter |
| Animation engine (Matrix FSM, override cycling, duo-color) | Internal to `RpMatrixAdapter` | Keep, relocate |
| Open Iconic bitmaps, v2 firmware | Unchanged — firmware is out of scope | None |
| Cricket mixer + tune manifest loader | Internal to `AudioAdapter` | Keep, relocate |
| HTTP command server | `CommandTransport` | ✓ role change |
| Presence event type constants | Keep in `garden-common::presence::event_types` | None |

#### 🔄 Salvage (extract logic, discard shape)

| Old shape | Salvage | Drop |
|---|---|---|
| `FireflyConnection` + `with_device` | Hot-unplug escape logic (read deadline, zero-read detection) | Shared-mutex pattern (root cause of deadlock) |
| `FireflyEvents` event handler | Per-event-type dispatch logic (mapping SSE kind → rendering action) — redistributes one method per adapter | Struct itself, `Arc<RwLock<Animation>>` coupling, `Arc<CompanionState>` coupling |
| Firefly's scattered reconnect logic in `main.rs` | "Cache current state → replay to freshly-connected device" pattern | Ad-hoc structure; consolidated into `Adapter::run` with `GardenSnapshot` hydration |
| Cycling health override | 10s/5s cycle timing | Current home in shared `Animation`; moves into `RpMatrixAdapter` |

#### ❌ Delete outright

| Artifact | Reason |
|---|---|
| `FireflyConnection` struct | Shared mutex is the architectural bug |
| `with_device` closure pattern | Lifetime model is the problem |
| Background reconnect task in firefly `main.rs` | Adapter lifecycle handles this |
| `EventHandler` trait (SDK) | Replaced by `Adapter::run` receiving events directly |
| `SseEvent` public struct | Becomes internal to `SseTransport` |
| Firefly's private `PresenceSnapshot`, `StoneState`, `OfferingState`, `StorageSummary` | Replaced by shared `garden-common::domain` types |
| Firefly's private `Health` enum | Replaced by shared `garden_common::domain::Health` |
| ~33 `if device_type == X` / `match device_type` dispatch sites | Adapters own their own type |
| `firefly::oled`, `firefly::tdisplay` helper modules | Absorbed into respective adapter modules |
| `firefly::events` module | Each adapter has its own event handling |
| `cricket::events` | Absorbed into `AudioAdapter` |

### The Book List

Books are numbered in dependency order along the critical path. After Book VII, remaining books parallelize freely.

**Critical path**: Prologue → I → II → III → IV → V → VI → VII

#### Book 0 — Prologue: Pattern Codification

**Scope**: Write the foundational documents every subsequent book references. Produces no Rust code.

**Deliverables**:
- `docs/decisions/COMPANION-0001-companion-integration-epic.md` (this document)
- `docs/specs/companion-architecture.md` — the pattern spec
- `docs/glossary.md` — updated with companion-epic vocabulary
- `docs/scaffolding.md` — section placeholder for `companion-*` scaffolds

**Exit criteria**: All four documents land under `docs/`. `cargo check --all` green (trivial — no code touched).

**Commit count**: 1.

#### Book I — Event Envelope

**Scope**: The foundational type everything stacks on.

**Bounded context**: Garden (begins construction)

**Deliverables**:
- `garden-companion-sdk::event` module — `Event`, `EventId`, `EventPayload` trait with `KIND` and `COALESCING` consts
- `EventId` as `uuid::Uuid` with GUIDv7 generation helpers (time-ordered)
- Kind namespace convention enforced at validation time
- Unit tests: envelope construction, GUIDv7 uniqueness + ordering, kind validation, typed downcast via `Event::payload::<T>()`

**Dependencies**: Book 0

**Exit criteria**: `Event` type can be constructed and downcast. Kind validation rejects non-namespaced strings.

#### Book II — Pulse

**Scope**: The canonicalizing orchestrator.

**Bounded context**: Garden

**Deliverables**:
- `garden-companion-sdk::pulse::Pulse` aggregate with `ingest()`, `subscribe()`, `metrics()` methods
- Dedup (bounded LRU by `EventId`)
- Validation (kind namespace, payload-kind match)
- Coalescing (per-kind map for events with `COALESCING=true`, flushed on timer)
- `PulseMetrics` with ingested / deduped / coalesced / dropped counters
- Subscriber lag policy (warn-and-continue, per ARCH-0017 §13 pattern)
- Unit tests: dedup, coalesce, fan-out, lag recovery

**Dependencies**: Book I

**Exit criteria**: `Pulse::ingest(event)` followed by `Pulse::subscribe().recv()` delivers unique, validated, optionally-coalesced events.

#### Book III — Transport

**Scope**: Event sources as pluggable ports. SSE and HTTP commands are the initial implementations.

**Bounded context**: Garden

**Deliverables**:
- `Transport` trait: `fn run(self: Box<Self>, pulse: Arc<Pulse>, shutdown: CancellationToken) -> BoxFuture<'static, ()>`
- `SseTransport` — receives moss `/presence/stream`, deserializes raw events into typed `PresenceEvent` payloads, publishes to `Pulse` (salvages reconnection/backoff/parser from `SseClient`)
- `CommandTransport` — HTTP server with `/command`, `/shutdown`, `/health` endpoints. Publishes command events to `Pulse`. Maintains correlation map: awaits matching `core.command.result` events to synthesize HTTP responses.
- Command result aggregation: timeout, multi-adapter fan-out
- Unit tests for each transport

**Dependencies**: Book II

**Exit criteria**: A companion wired with `SseTransport` + `CommandTransport` routes events through `Pulse`. HTTP `POST /command` round-trips through the event bus and returns an aggregated response.

#### Book IV — Domain Types

**Scope**: Shared domain model in `garden-common::domain`.

**Deliverables**:
- `garden-common::domain` module
- `Stone`, `Offering`, `Health` (enum), `Load`, `SeedBank`, `Pond` types
- Migration: moss uses these types where currently using wire types; firefly's private structs are staged for deletion (actual deletion in Book VIII)
- Conversion helpers from `garden-common::presence` wire types to domain types (used by `SseTransport`)

**Dependencies**: Book I

**Exit criteria**: `use garden_common::domain::Health` compiles everywhere. Moss internals use domain types at their boundaries. `Health` is no longer a string anywhere in the companion SDK.

#### Book V — Garden Aggregate

**Scope**: Client-side CQRS projection.

**Bounded context**: Garden

**Deliverables**:
- `garden-companion-sdk::garden::Garden` aggregate with private `GardenState` and `Arc<Pulse>`
- Projection task that subscribes to `Pulse`, applies `PresenceEvent` payloads to state, emits domain-level events back to `Pulse`
- Properties: `stone()`, `health()`, `load()`, `offerings()`, `seed_bank()`, `pond()`, `is_ready()`
- Synthetic `GardenSnapshot` event emitted when a new subscriber attaches (for adapter hydration — Book VI consumes this)
- Unit tests: projection correctness, snapshot-on-subscribe, coalesced state deltas

**Dependencies**: Books II, IV

**Exit criteria**: Given a `Pulse` with test events, `Garden::health()` returns the correct derived state. Subscribing to `garden.events()` delivers a `GardenSnapshot` as the first event.

#### Book VI — Adapters

**Scope**: The extension contract and lifecycle.

**Bounded context**: Adapters (new)

**Ch0 gate**: Write prototype adapter implementations for both domains — `RpMatrixAdapter` (hardware, complex) and `AudioAdapter` (audio, simple) — against the proposed trait surface. If either prototype needs trait extensions or contortions, redesign the trait before freezing. Prototypes are thrown away at book close; production implementations are written fresh in Book VIII.

**Deliverables**:
- `Adapter` trait: `info()`, `profile()`, `run()`
- `AdapterProfile` with subscriptions (event kinds), delivery policy (All / LatestEvery / Debounced), required dependencies
- `AdapterFactory` trait: `kind()`, `required_dependencies()`, `discover()`
- `Adapters` supervisor aggregate: factory registry, periodic discovery, spawn/reap lifecycle, instance replacement on disconnect
- Supervisor instruments each adapter task with a `tracing::Span` (kind, id)
- Grace window for device bounce (configurable, default 2s)
- Per-adapter typed state persistence (`{state_dir}/adapters/{kind}/{id}.json`)
- `AdapterStatus` for health telemetry exposed via `CommandTransport`'s `/status`
- Unit tests: supervisor spawn/reap, subscription filtering, delivery coalescing, grace window

**Dependencies**: Books II, III, V

**Exit criteria**: Ch0 prototypes pass. Production `Adapter` trait, `Adapters` supervisor, and their tests land green.

#### Book VII — Companion

**Scope**: Top-level wiring.

**Deliverables**:
- `Companion` struct: builder API (`.with_transport(...)`, `.with_adapter_factory(...)`, `.run()`)
- Owns `Pulse`, `Garden`, `Adapters`, shutdown coordination
- Replaces `CompanionRuntime`
- Absorbs `CompanionState` (on/off flag is a `Companion` field)

**Dependencies**: Books II, V, VI

**Exit criteria**: A 20-line `main.rs` can construct a fully-functional companion that receives events, projects state, runs adapters, and serves HTTP commands. No production consumer yet — validated via integration tests.

#### Book VIII — Companion Rebuild (firefly + cricket)

**Scope**: Replace both consumer crates wholesale using the new architecture.

**Ch0**: Cross-crate architectural discussion, device-adapter mapping, serial-helper extraction plan.

**Chapter structure** (parallel — any order):
- **Ch1**: Firefly `main.rs` (builder API) + `RpMatrixAdapter` (animation engine internal to adapter; override cycling; duo-color; matrix pixel protocol)
- **Ch2**: `OledV1Adapter` (classic status screen)
- **Ch3**: `OledV2Adapter` (icon dashboard + activity spinner; absorbs `firefly::oled` helpers)
- **Ch4**: `TDisplayAdapter` (JSON snapshot + incremental load; absorbs `firefly::tdisplay`)
- **Ch5**: Cricket `main.rs` + `AudioAdapter` (mixer + tune manifest internal to adapter; debounce policy via `AdapterProfile::Debounced`)
- **Ch6**: Old code deletion — `FireflyConnection`, `FireflyEvents`, `firefly::oled`, `firefly::tdisplay`, `firefly::events`, `cricket::events`, device-type dispatch sites

**Dependencies**: Book VII

**Exit criteria**: Both binaries use the new architecture exclusively. Old modules deleted. `cargo check --all` green. `cargo test` green. `cargo clippy -- -D warnings` green. A real ESP8266 or RP2040 run-through confirms device rendering matches pre-epic behavior.

#### Book IX — Integration Tests

**Scope**: End-to-end test infrastructure.

**Deliverables**:
- `MockTransport` (publishes scripted events into a `Pulse`)
- `MockAdapter` (records received events; assertable in tests)
- Test harness: construct a `Companion` with mock transport + mock adapter, feed events, verify outcomes
- Golden-file test fixtures for scenario replay
- At least one integration test per real adapter (exercising the full event path with device I/O mocked)

**Dependencies**: Book VIII

**Exit criteria**: `cargo test --package garden-firefly --package garden-cricket` includes integration tests that exercise the SSE → Pulse → Garden → Adapter pipeline end-to-end.

#### Book X — Epilogue: CI Enforcement & Pattern Validation

**Scope**: Close the epic.

**Deliverables**:
- Scaffolding tracker shows zero active `companion-*` entries
- `scripts/check-scaffolding.sh` covers companion scaffolds (already does; verify)
- CI enforces event-kind namespace convention (script that scans `EventPayload::KIND` consts for prefix conformance)
- `docs/reference/context-map.md` updated with companion contexts
- `docs/glossary.md` final pass
- `docs/code-standards.md` updated with any new conventions from the epic
- Pattern spec `companion-architecture.md` validated against final code (no drift)

**Dependencies**: Book IX

**Exit criteria**: Pattern spec matches live code. No scaffold entries active. CI green.

### Out of scope

Items deliberately deferred to later targeted ADRs, with reason:

| Deferred | Reason |
|---|---|
| CloudEvents envelope alignment | No external integration pressure today. Clean addition later if/when MQTT/NATS/webhook consumers appear. |
| Config-driven adapter composition (TOML) | Nice-to-have; doesn't change architecture. Targeted follow-up ADR once pattern is in place. |
| Prometheus / observability output adapters | No metrics pressure today. Any contributor can add one once the Adapter trait exists. |
| Event recording / time-travel debugging | Subsumed by integration testing (Book IX) for the 80% case. Full replay infrastructure is future work. |
| Cross-stone event federation | Architectural groundwork is free; actual multi-stone adapters are future work. |
| Multi-language SDK bindings | Not required until someone needs a Python/Node companion. Envelope stability keeps the door open. |
| Moss → companion presence notifications (tending acks) | Orthogonal capability gap; separate targeted ADR. |
| Tune schema versioning (Cricket) | Book VIII fixes silent-breakage via typed domain events; full schema versioning is a CRICKET-* follow-up. |

### Success criteria

At epilogue close, the following are true:

1. **Adapter count is data, not code.** Adding a new adapter (device variant or otherwise) is a new `AdapterFactory` implementation. No other code changes required in the SDK.
2. **Device-type dispatch sites: 0** (was ~33).
3. **Shared-mutex device-port pattern: 0** (was 1, `FireflyConnection`).
4. **Integration test coverage of event pipeline: > 0** (was 0).
5. **`PresenceSnapshot` in `garden-common::domain`** — not duplicated in firefly.
6. **Every adapter's event interests are declared, not implicit.**
7. **Every command round-trips through `Pulse`.** No adapter owns HTTP plumbing.
8. **`companion-sdk` can be used by a third adapter** (a Prometheus exporter, a webhook bridge, whatever) without any SDK modifications.
9. **Pattern spec matches live code** with no drift.
10. **Scaffolding tracker: zero active companion entries.**

## References

- [ARCH-0017](ARCH-0017-ddd-monolith-epic.md) — moss epic, playbook precedent
- [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md) — first DDD aggregate in moss
- [domain-aggregates.md](../specs/domain-aggregates.md) — the aggregate pattern this mirrors
- [companion-architecture.md](../specs/companion-architecture.md) — the pattern spec this epic produces
- [scaffolding.md](../scaffolding.md) — scaffolding tracker (shared with ARCH-0017)
- [glossary.md](../glossary.md) — ubiquitous language
