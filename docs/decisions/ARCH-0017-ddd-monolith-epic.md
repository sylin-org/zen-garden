---
audience: [developer, ai]
doc_type: decision
status: accepted-living
last_verified: 2026-04-11
canonical: true
---

# ARCH-0017: DDD Monolith Epic — Pattern-Enforced Bounded Contexts Across Moss

**Date**: 2026-04-11
**Status**: Accepted — Living Document
**Depends on**: [ARCH-0004](ARCH-0004-appstate-domain-context-extraction.md) (domain context extraction), [ARCH-0007](ARCH-0007-monomorphic-domain-traits.md) (monomorphic trait pattern), [ARCH-0015](ARCH-0015-task-supervisor-registry.md) (BackgroundTask registry), [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md) (first domain aggregate)

## Revision history

Unlike typical ADRs which are immutable after acceptance, ARCH-0017 is a **living plan** that evolves as the epic progresses. Every amendment is logged here with date, trigger, and scope. See [The Discovery Mandate](#the-discovery-mandate) for the rule that authorizes amendments.

| Date | Change | ADR |
|------|--------|-----|
| 2026-04-12 | **Book IX closed.** Security — single aggregate (not 3 sub-contexts), private enrollment state, `SecurityChanged` event, `PondClient` + `CeremonyPersistence` ports relocated, `PondState` absorbed, 753 tests. | [ARCH-0027](ARCH-0027-security-aggregate.md) |
| 2026-04-12 | **Book VIII-b closed.** Storage API surface — `/v1/stone/banks` and `/v1/garden/banks` routes, `BankContentOps` port + data-plane commands, 301 redirects for pin/unpin, 741 tests. | [ARCH-0026](ARCH-0026-storage-api-surface.md) |
| 2026-04-12 | **Book VIII-a closed.** Storage domain model — Bank view aggregate, VolumeIngestor rename, ports relocated, 8 API handlers migrated, `BankError` typed enum, 735 tests. | [ARCH-0025](ARCH-0025-storage-bank-aggregate.md) |
| 2026-04-12 | **Book VII closed.** Health — stateless probe facade, `HealthProbe` port, `DockerHealthProbe` adapter, 724 tests. | [ARCH-0024](ARCH-0024-health-aggregate.md) |
| 2026-04-12 | **Book VI closed.** Subsystems — `watch` channels replace `AtomicBool` flags, lock-free state deviation, 707 tests. | [ARCH-0023](ARCH-0023-subsystems-aggregate.md) |
| 2026-04-12 | **Book V closed.** Catalog — frozen `ManifestRegistry` + compiled index, first typed `CatalogError`, third persistent aggregate, 692 tests. | [ARCH-0022](ARCH-0022-catalog-aggregate.md) |
| 2026-04-11 | **Book IV closed.** Jobs — ephemeral with `JobsReaperTask` (24h TTL), dual event streams, infallible mutations, 671 tests. | [ARCH-0021](ARCH-0021-jobs-aggregate.md) |
| 2026-04-11 | **Book III closed.** Topology — persistent with `TopologyStore` + `ChirpTransport` ports, `SelfEntryInputs` composition, 649 tests. | [ARCH-0020](ARCH-0020-topology-aggregate.md) |
| 2026-04-11 | **Book II closed.** Tool — field-level strangler, dual event streams, `ToolsBeaconTransport` port, 649 tests. | [ARCH-0019](ARCH-0019-tool-aggregate.md) |
| 2026-04-11 | **Book I closed.** Metrics — register-with-kinds, lock-free hot path, 7 HTTP handlers + SSE, resources rename. | [ARCH-0018](ARCH-0018-metrics-aggregate.md) |
| 2026-04-11 | **Book 0.** Prologue — pattern spec, context map, scaffolding tracker, glossary. | — |
| 2026-04-11 | Initial acceptance. | Epic commits to 21-book arc |
| 2026-04-11 | Added **Discovery Mandate**. Plan is a living hypothesis; Ch1 re-evaluates against code. | User directive |

## Context

Moss grew from a single-binary prototype into a distributed stone daemon
through a series of increasingly-principled refactors. ARCH-0004 extracted
domain contexts from `AppState`. ARCH-0005 did a structural quality pass.
ARCH-0007 modernized to edition 2024 and monomorphized trait dispatch.
ARCH-0015 replaced the coordinator with a task supervisor registry.
ARCH-0016 made `Offerings` the first proper DDD aggregate — private state,
event-driven projections, persistence port, `ChangeKind` + broadcast
`changes()` stream.

Each of those refactors was correct and land-worthy on its own. The
composite effect is a codebase that is *partly* DDD-aligned: some domains
exist, some don't, some own their invariants, some don't, some expose
events, some expose raw `Arc<RwLock<Vec<_>>>`. Call sites reach across
contexts. Invariants live in comments. Projections are poked imperatively.
The `AppState` struct still has ~25 fields mixing domain data, cross-cutting
concerns, and raw maps.

ARCH-0016 proved — with a live bug fix on stone-azure-pool as evidence —
that the DDD aggregate pattern is the right answer for moss. The failure
mode it fixed (`promote_adopted` bypassing the "canonical mutation boundary"
because the boundary was a comment, not a type) is not specific to
offerings. It is a structural class of bug that the codebase permits
anywhere state is public and invariants are prose.

The goal of this epic is to apply the pattern uniformly. Every module
becomes a bounded context. Every context owns its state privately, exposes
typed reads, commands mutations through an API that enforces invariants in
the type system, emits events on mutation, and depends on foreign
infrastructure through ports. No shims, no wrappers, no raw access across
boundaries, no `anyhow` inside domain logic.

The epic will produce a modular monolith in which each moving part knows
exactly what it owns and communicates with other parts only through
explicit contracts.

## Decision

Moss adopts the DDD tactical playbook uniformly across all modules, phased
as an epic of 20 books plus a prologue. Each book introduces or rebuilds one
bounded context according to a fixed pattern. Each book ships independently
green to `dev` before the next begins. The pattern is codified in the
prologue and enforced by CI in the epilogue.

### The Principles

The epic commits to seven principles. Every book is judged against them.

#### 1. Bounded contexts

Each bounded context owns a slice of state and a slice of behavior. Its
types, events, errors, and ports are module-local. Cross-context
interaction is explicit: either through a subscription to another context's
event stream, or through a command/query method on another context's
aggregate. **No context reaches into another context's state directly.**

#### 2. Ubiquitous language

Moss already has a rich domain vocabulary: stone, pond, moss, offering,
nourishment, harvest, companion, presence, ceremony, caretaking, thriving,
withering. These are not cute metaphors — they are the domain language.
Code inside a bounded context uses this language consistently. Technical
suffixes (`Manager`, `Service`, `Handler`, `Context`) are forbidden per
code-standards §3.

A glossary is maintained at `docs/reference/glossary.md` (written in Book
0). Every term used in a bounded context name, aggregate name, or event
name must be defined there.

#### 3. Aggregates own their invariants

Every bounded context that holds mutable state exposes it through an
aggregate root. The aggregate:

- Keeps its state **private** behind `RwLock<State>` (or equivalent).
- Exposes **typed reads**: `snapshot()`, `find_by_*`, `with_*` (scoped
  borrow closures), `count`, `exists`.
- Exposes a **mutation API** of command methods (`upsert`, `remove`,
  `update`, context-specific verbs). Every mutation method funnels through
  a private `finalize` helper that persists, emits an event, and returns.
- Emits a **typed event** on every mutation via `broadcast::Sender`.
- Depends on infrastructure through **injected ports** (`Arc<dyn Store>`,
  `Arc<Metrics>`, `Arc<dyn ContainerRuntime>`, ...), not direct calls to
  `crate::infra::*`.

If a context has no mutable state (e.g., a probe service), the "aggregate"
is a facade struct with command methods instead of a data-holding one.

#### 4. Domain events as lingua franca

Contexts communicate with each other through domain events, not direct
method calls. The producer context emits; the consumer context subscribes.

A projection task (`BackgroundTask` per ARCH-0015) owns each subscription
lifecycle. Projections are:

- **Subscribe-before-seed**: subscribe to the event stream *first*, then
  run the initial snapshot refresh, then enter the receive loop. This
  avoids the race window that ARCH-0016 identified.
- **Lag-tolerant**: on `RecvError::Lagged`, fall back to a full reconcile
  from the producer's snapshot.
- **Shutdown-aware**: `select!` on the cancellation token in parallel with
  `feed.recv()`.

Direct cross-context method calls are permitted *only* for queries
(`other_context.find_by_id()`), never for mutations. Mutations are always
requested via event or via a command method that the target context owns.

#### 5. Ports and adapters

Every bounded context declares the infrastructure it needs as a trait
(port). Adapters implement the trait. The context is constructed with
`Arc<dyn Port>` injection. This is the pattern established by ARCH-0016's
`OfferingStore` / `FileOfferingStore`.

- **No context imports `crate::infra::*` directly.**
- Ports live inside their owning context: `domain/offerings/store.rs`,
  `domain/tool/beacon.rs`, `domain/topology/announcer.rs`.
- Adapters live in `infra/` and are wired at bootstrap.
- Infrastructure traits use the `Pin<Box<Future>>` pattern (per ARCH-0007,
  async-trait removed), matching ARCH-0015's `BackgroundTask`.

This makes every bounded context testable in isolation with in-memory
fakes.

#### 6. Anti-corruption layers

Where moss meets foreign models (Docker containers from Bollard, Ollama's
HTTP API, Rake's wire format, mDNS SRV records, systemd units), the
adapter translates foreign types into moss domain types at the boundary.
Foreign types **do not bleed inward** past the adapter.

The existing `OfferingFqn` type is a good example of this — Docker
container names are translated to/from our canonical FQN at the
container-runtime boundary. This pattern generalizes.

#### 7. Command / query separation

Read methods and write methods are distinct at every level. Read methods
are pure queries with no side effects (except lock acquisition). Write
methods are commands that mutate and emit. The API layer becomes a thin
HTTP → command/query dispatcher.

- `GET /api/v1/stone/offerings` → `state.offerings.snapshot().await`
- `POST /api/v1/stone/offerings` → `state.offerings.upsert(o).await`

The handler is a one-liner per endpoint in the ideal case. No state
reaches into the API layer except through `FromRef` extraction.

### The Scaffolding Contract

Intermediate-state scaffolding is permitted *if and only if* it is tracked
in `docs/SCAFFOLDING.md` with:

- **What**: a brief description
- **Where**: file(s) and module
- **Introduced in**: Book N
- **Removal trigger**: the book whose completion obsoletes it
- **Removal action**: the exact deletion(s) needed

A CI check verifies that for every scaffold item whose removal trigger book
is marked complete, the scaffold is actually gone. A scaffold with a
complete removal trigger that still exists is a hard error.

Examples of acceptable scaffolds:

- `Offerings::read()` + `ActiveGuard` (introduced in Book 0's precursor
  ARCH-0016; removal trigger: Book XVIII "Offerings strangler removal"
  in this epic)
- A `NoopMetricsRecorder` that no-ops until real `Metrics` lands (but only
  if Book I is genuinely unable to land without it — in practice Book I
  *is* Metrics, so this scaffold never exists)

Examples of unacceptable scaffolds:

- `TODO: migrate to X later` comments with no `docs/SCAFFOLDING.md` entry
- `#[deprecated]` markers without a removal trigger
- Dead code paths marked "keep for now"
- Any shim that outlives the book immediately after its introducer

Scaffolding is an exception, not a pattern. The goal of the epic is a
codebase with zero active scaffold entries on the day the epilogue ships.

### The Shippability Rule

**Every book, at the final chapter commit, merges green to `dev`.** No
cross-book atomicity. No long-lived epic branch. No "two books land
together." If Book III's design implies a change that Book VII will
complete, Book III does the part of that change necessary to keep `dev`
green — or Book VII is pulled forward.

CI on `dev` runs `cargo check --all`, `cargo test --package garden-moss`,
`cargo clippy -- -D warnings`, and the scaffolding-tracker check. A book
that cannot land on `dev` green is rolled back until it can.

This rule is non-negotiable. The epic takes as long as it takes, but
`dev` is always shippable.

### The Rename Mandate

Methods, files, directories, types, and modules are all up for rename.
Where a name violates the ubiquitous language or code-standards §3 (no
architectural suffixes), it changes. Examples already anticipated:

- `AppState` (architectural suffix, bag-of-everything): likely dissolved
  or renamed to `Moss` in Book XIX.
- `helpers.rs`, `utils.rs`, `common.rs` catch-alls: decomposed per
  code-standards §14 (one file per concept) throughout the epic.
- `docker.rs` / `docker/` files: renamed to match their owning context
  (e.g., `container_runtime/` in Book XII).
- `coordinator.rs` remnants: fully dissolved per ARCH-0015.

Renames happen inside the book that touches the file. Pure `git mv` with
no content changes goes in its own commit (per code-standards §14) so
`git log --follow` preserves history.

### The Discovery Mandate

The plan in this document is a **hypothesis**, not a contract. It was
written before every line of moss was re-read with clean-architecture
eyes. As each book opens, the author (human or AI) **re-evaluates the
hypothesis against the actual code** and is expected — mandated — to
change the plan when the code teaches them something the plan did not
anticipate.

**The mandate in one sentence:** If, while working on a book, you
discover that the plan is wrong — that a context should split, merge,
move, be renamed, or be approached differently — **stop, put on a clean
architecture specialist hat, ask "what would the right shape actually
look like?", and change the plan, the code, or both.**

Concrete triggers that warrant a plan change:

- **Discovered context** — a bounded context exists in the code that
  the plan does not list, or the plan lists a context that does not
  meaningfully exist in the code.
- **Missed dependency** — Book N cannot start until Book M delivers
  something the plan did not identify as a dependency.
- **Scope error** — a single book turns out to be too large to ship
  green and needs to split, or two books turn out to be entangled
  and need to merge.
- **Wrong abstraction** — a port the plan listed does not fit the
  domain it serves, or a type the plan named is not actually the
  aggregate root of its context.
- **Pre-existing work** — the plan proposes building something that
  already exists in a form close enough to keep rather than rewrite.
- **Better name** — a name in the plan turns out to violate the
  ubiquitous language, clash with an existing term, or mislead.
- **Cascading rename** — renaming a type propagates further than the
  plan anticipated and the new surface touches files the book did
  not scope.
- **Structural simplification** — two contexts could fold into one
  without loss, or one context could split into two for clarity.

### Rules for plan changes

1. **Every plan change is documented in the revision history table at
   the top of this ADR** with date, change, and trigger. An undocumented
   plan change is a bug.
2. **Material plan changes (adding/removing a book, changing sequencing,
   changing scope > 20% for a book) are surfaced to the user in the
   book's opening message** — not for approval (the user has committed
   to full scope) but for visibility. The user can redirect if the
   discovery was misjudged.
3. **Minor plan changes (renaming a context, refining a port name,
   moving a file inside a book) are documented in the book's own ADR**
   and in the context map update, but do not require surfacing beyond
   that.
4. **The scaffolding tracker still applies.** If a plan change requires
   a temporary shim, that shim is logged in
   [scaffolding.md](../scaffolding.md) with a removal trigger.
5. **Never silently deviate.** The failure mode ARCH-0017 exists to
   prevent — plans that drift from reality without documentation —
   applies equally to plan changes. A plan that quietly differs from
   the live codebase is worse than a plan with tracked amendments.
6. **Reverting a plan change is also a plan change.** If an amendment
   turns out to be wrong, revert it in the same way: a new revision
   history entry explaining what was tried and why it was reverted.

### When NOT to change the plan

- **Routine technical problems inside a chapter** — just solve them.
  Compilation errors, test failures, refactoring nits, small naming
  decisions: these are book-internal and do not touch the plan.
- **Taste preferences** — "I'd organize this differently if I were
  rewriting it" is not a plan change; the plan wins unless there is
  a concrete structural reason.
- **Scope creep disguised as discovery** — if a book tempts you to
  "while I'm here, also refactor X that's not in this book," resist.
  X gets its own book or its own chapter in a planned book.

### Authority

The author of a book (human or AI working with a human in the session)
has authority to make minor plan changes. Material plan changes require
user visibility but not approval. The user retains veto power at any
point: if they redirect, the redirection becomes a new revision history
entry reverting the change.

This mandate exists because the alternative — a rigid plan enforced
against discovered reality — is how moss got into its current state in
the first place. ARCH-0017 explicitly chooses to prefer adaptation over
rigidity.

## The Ubiquitous Language — Moss Glossary

*This section is the seed of `docs/reference/glossary.md`, finalized in
Book 0.*

### Core domain terms

| Term | Meaning |
|------|---------|
| **Stone** | A physical or virtual machine running moss. One stone = one garden-moss process. |
| **Garden** | A collection of stones discovered and coordinating via the pond. |
| **Moss** | The daemon process that runs on each stone. |
| **Rake** | The operator CLI that talks to moss over HTTP. |
| **Pond** | The trust boundary. Stones that share a pond can talk to each other under mTLS. |
| **Pond membership** | The set of stones currently trusted by a pond. |
| **Keystone** | The stone that initially created the pond (holds the CA root). |
| **Offering** | A service template (managed, borrowed, or adopted) known to moss. |
| **Managed offering** | An offering whose lifecycle moss owns (Docker container). |
| **Borrowed offering** | An offering running in an existing container moss did not deploy. |
| **Adopted offering** | A native service detected on the host (e.g., Ollama on Windows bare metal). |
| **FQN** | Fully-qualified offering name — `name::instance` canonical form. |
| **Nourishment** | An upgrade operation on an offering. |
| **Harvest** | A snapshot of an offering's state before an upgrade. |
| **Caretaking** | Periodic maintenance (sweep, health, pruning). |
| **Thriving / Withering** | Healthy / degraded stone status. |
| **Companion** | An auxiliary process that runs alongside moss (Cricket, Firefly, ...). |
| **Ceremony** | A multi-stone coordinated operation (pond join, nourishment, ...). |

### Architectural terms

| Term | Meaning |
|------|---------|
| **Bounded context** | A module with a single responsibility, private state, and an explicit contract for cross-boundary interaction. |
| **Aggregate** | The root type of a bounded context's mutable state. Owns invariants. |
| **Command** | A write method on an aggregate. Mutates state, persists, emits. |
| **Query** | A read method on an aggregate. Pure, no side effects. |
| **Domain event** | A broadcast message describing a state change in a bounded context. |
| **Port** | A trait defining infrastructure a bounded context depends on. |
| **Adapter** | A concrete implementation of a port. Lives in `infra/`. |
| **Projection** | A derived view of state maintained by a background task that subscribes to events. |
| **Anti-corruption layer** | The adapter boundary where foreign types are translated to domain types. |
| **Chirp** | A UDP announcement broadcast by a stone describing its current topology. |
| **Topology** | A stone's self-description: identity, capabilities, offerings, health. |
| **Tool** | A generic view over offerings + seed-banks published to the garden registry. |
| **Tools beacon** | A UDP broadcast of tool deltas so peer stones update their registries. |

## The Context Map

The bounded contexts moss will contain after the epic. Each entry lists
**owns** (state the context holds), **emits** (events the context
publishes), **subscribes** (events the context consumes), and **ports**
(infrastructure it depends on).

### Core stone identity

**Current** (partially exists from ARCH-0004 — deep-cleaned in Book IX of
this list indirectly)
- **Owns**: stone identity (id, name), address, health, MAC, API port,
  system/network/GPU metrics
- **Emits**: `StoneHealthChanged`, `StoneAddressChanged`
- **Ports**: `NetworkInterface` (for IP/MAC detection)

### Application services

**Offerings** (exists from ARCH-0016)
- **Owns**: active offering pool, adopted candidate pool
- **Emits**: `OfferingsChanged { kind, affected }`
- **Subscribes**: (none — offerings is the root of the service pipeline)
- **Ports**: `OfferingStore`

**Catalog** (Book V) — absorbs `manifest_registry` + `offerings_index`
- **Owns**: manifest registry, compiled offerings index with fingerprint
- **Emits**: `CatalogChanged { reason }`
- **Subscribes**: none (catalog is pure compile-time)
- **Ports**: `ManifestSource`, `CatalogCache` (disk persistence)

**Jobs** (Book IV)
- **Owns**: active jobs map, job history
- **Emits**: `JobEvent { id, kind, phase }`
- **Subscribes**: none directly (consumers query)
- **Ports**: `JobStore` (if persisted; may be in-memory only)

**Tool** (Book II)
- **Owns**: garden-wide tool registry (local + remote + gateway entries),
  tool delta stream
- **Emits**: `ToolDelta`, `ToolBeaconEmitted`
- **Subscribes**: `OfferingsChanged` → rebuild local projection;
  `StorageChanged` → rebuild seed-bank projection
- **Ports**: `ToolsBeaconTransport` (UDP adapter)

**Topology** (Book III)
- **Owns**: self-entry cache, topology projection for garden-wide queries
- **Emits**: `TopologyChanged { reason }`, `ChirpEmitted`
- **Subscribes**: `OfferingsChanged`, `StoneHealthChanged`,
  `StoneAddressChanged`, `ToolDelta`
- **Ports**: `ChirpTransport`, `MdnsTransport`

**Health** (Book VII) — probes and health state
- **Owns**: per-offering health state, probe schedule
- **Emits**: `HealthChanged { offering_id, status }`
- **Subscribes**: `OfferingsChanged` to trigger lifecycle-aligned probes
- **Ports**: `HealthProbe` (HTTP + TCP adapters)

**Subsystems** (Book VI) — readiness as events
- **Owns**: per-subsystem readiness state (network, docker, pond, ...)
- **Emits**: `SubsystemReady { name }`, `SubsystemUnready { name }`
- **Subscribes**: adapter-level events (network monitor, docker monitor,
  ...)
- **Ports**: none directly (receives from other adapters)

### Storage

**Storage** (Book VIII) — deep-clean of the existing partial context
- **Owns**: physical volumes, volume state, media, banks, replication
  state
- **Emits**: `VolumeChanged`, `BankChanged`, `ReplicationChanged`
- **Subscribes**: none (storage is the root of its own pipeline)
- **Ports**: `VolumeMonitor`, `FileSystem`, `ReplicationTransport`

Storage will likely split into sub-aggregates inside the Storage context:
`Storage::Volumes`, `Storage::Banks`, `Storage::Replication`. The book's
chapters reflect that.

### Security

**Security / Pond** (Book IX) — consolidates pond + ceremony + TLS
- **Owns**: pond state (membership, CA), ceremony registry, TLS key
  material
- **Emits**: `PondChanged`, `CeremonyChanged`, `TrustChanged`
- **Subscribes**: `StoneDiscovered` (from Discovery) for enrollment flow
- **Ports**: `CeremonyJournal`, `PondCertStore`, `MtlsAcceptor`

### Networking / Discovery

**Discovery** (Book X) — mDNS + koi integration
- **Owns**: known peer stones with their last-seen state
- **Emits**: `StoneDiscovered`, `StoneLost`
- **Subscribes**: `MdnsEvent` from adapter
- **Ports**: `MdnsTransport`, `KoiClient`

**Announcement** (Book X) — chirp scheduling + emission
- **Owns**: chirp schedule, last-chirp timestamp
- **Emits**: `ChirpEmitted`
- **Subscribes**: `TopologyChanged` (immediate chirp), periodic timer
- **Ports**: `ChirpTransport` (UDP)

**Networking** (Book X) — network interface monitoring
- **Owns**: current network interface state
- **Emits**: `NetworkStateChanged`
- **Subscribes**: OS-level interface change events
- **Ports**: `InterfaceMonitor`

### Orchestration

**Orchestration** (Book XI) — deep-clean of existing partial context
- **Owns**: orchestration tick stream, nurturing store, nudge state,
  offering primary/dormant elections
- **Emits**: `OrchestrationTick`, `ElectionResolved`
- **Subscribes**: `OfferingsChanged`, periodic timer
- **Ports**: `ElectionTransport`

### Infrastructure contexts

**ContainerRuntime** (Book XII) — Docker port + adapter
- **Owns**: no state of its own; it's a port
- **Exposes**: trait `ContainerRuntime` with methods for container
  lifecycle
- **Implementation**: `BollardAdapter` in `infra/`
- **Anti-corruption layer**: Bollard types never leave the adapter

**Persistence** (Book XIV) — unified pattern for file-backed stores
- **Owns**: no state of its own; it's a set of reusable helpers
- **Exposes**: `atomic_write`, `load_json`, `save_json` primitives
- **Consumers**: every domain `Store` adapter

**Configuration** (Book XIII)
- **Owns**: typed `EnvConfig`, runtime feature flags
- **Emits**: `ConfigChanged` (for hot-reload paths)
- **Subscribes**: file-watch signals (optional)
- **Ports**: `ConfigSource`

**Logging** (Book XV)
- **Owns**: log broadcast channel, file sink handle
- **Emits**: `LogLine` (via broadcast)
- **Subscribes**: tracing layer events
- **Ports**: `LogSink` (file + stderr adapters)

### Cross-cutting

**Metrics** (Book I)
- **Owns**: per-domain counters, per-task metrics, global metrics
- **Emits**: `MetricsChanged` (only on interesting transitions, not on
  counter increments)
- **Subscribes**: none (other contexts push to Metrics via command
  methods)
- **Ports**: none (in-memory only in Phase 1)

**EventBus / Pulse** (Book XVI)
- **Owns**: the unified cross-cutting event surface (`DomainEvent`,
  `PulseEvent`)
- **Emits**: translated `PulseEvent` stream
- **Subscribes**: every domain's `changes()` stream via
  `PulseDomainBridge`
- **Ports**: none

**Companion** (exists from ARCH-0004, internals deep-cleaned in a later
book if needed)
- **Owns**: registered companions, their ports, their commands
- **Emits**: `CompanionChanged`
- **Ports**: `CompanionSocket`, `CompanionManifest`

**Presence** (exists from ARCH-0004, internals deep-cleaned if needed)
- **Owns**: election service, notification registry
- **Emits**: `ElectionResolved`, `PresenceNotification`
- **Subscribes**: `StoneDiscovered`, `StoneLost`

### Application layer

**HttpApi** (Book XVII)
- **Owns**: the axum `Router` and handler functions
- **Subscribes**: none — handlers are stateless
- **Depends on**: every domain aggregate via `FromRef` extraction
- **Anti-corruption**: `api::dto::*` types separated from `domain::*`
  types; handlers translate at the boundary

**Bootstrap** (cross-cutting, no single book — touched by every book)
- **Owns**: startup sequence, dependency injection wiring
- **Outputs**: a fully-constructed `Moss` (the former `AppState`)

**Shutdown** (cross-cutting, touched in Book XIX)
- **Owns**: cascading shutdown lifecycle
- **Inputs**: OS signals, admin-triggered shutdown
- **Outputs**: cancellation token propagation, final flush hooks

### Root

**Moss** (Book XIX) — the former `AppState`
- **Owns**: shutdown token, start time, the dependency graph of
  aggregates
- **Fields**: `offerings`, `tool`, `topology`, `jobs`, `catalog`,
  `health`, `subsystems`, `storage`, `security`, `discovery`,
  `announcement`, `networking`, `orchestration`, `metrics`, `events`,
  `pulse`, `companion`, `presence`, `logging`, `configuration`,
  `current`
- **Methods**: none except possibly lifecycle helpers (bootstrap output,
  shutdown orchestration)
- **Role**: pure dependency container. A facade, not a god object.

## The Aggregate Pattern Spec

*This is the seed of `docs/specs/domain-aggregates.md`, finalized in Book
0. It is the canonical reference every book applies.*

### Shape

```rust
// domain/<context>/mod.rs
pub mod aggregate;   // the aggregate root type
pub mod command;     // command types (if complex; may be inlined)
pub mod event;       // event types
pub mod error;       // typed error enum
pub mod port;        // trait definitions
pub mod state;       // internal state struct (pub(super) only)

pub use aggregate::<Context>;
pub use event::{<Context>Changed, ChangeKind};
pub use error::<Context>Error;
pub use port::{<Context>Store, <Context>Transport, ...};
```

### Aggregate root

```rust
pub struct <Context> {
    state:   RwLock<<Context>State>,          // private, no pub
    store:   Arc<dyn <Context>Store>,         // persistence port
    metrics: Arc<Metrics>,                    // cross-cutting port
    changes: broadcast::Sender<<Context>Changed>,
    // plus any additional ports this context depends on
}
```

**Rules:**

- `state` field is private. No `pub`, no `pub(crate)`, no accessor that
  hands out a raw lock guard.
- `changes` field is private; subscription is exposed via a `changes()
  -> broadcast::Receiver<_>` method.
- All infrastructure dependencies are `Arc<dyn Port>` and injected at
  construction.
- All methods that mutate state go through a private `finalize` helper
  that:
  1. Acquires the write lock.
  2. Mutates.
  3. Clones a persistence snapshot before releasing the lock.
  4. Calls `store.save(snapshot).await`.
  5. Records a mutation-latency metric via `metrics.record_mutation_latency`.
  6. Records a domain-event metric via `metrics.record_domain_event`.
  7. Emits the typed event.
- No method returns `anyhow::Error`; domain errors are typed per context.
- Every public method has `#[tracing::instrument(level = "debug",
  skip(self))]` (or `info` where appropriate).

### Read API shapes

Every aggregate provides at least these read shapes where they make sense:

```rust
impl <Context> {
    pub async fn snapshot(&self) -> <State snapshot type>;
    pub async fn find_by_id(&self, id: &str) -> Option<Item>;
    pub async fn find_by_name(&self, name: &str) -> Option<Item>;
    pub async fn with_<scope><F, R>(&self, f: F) -> R
        where F: FnOnce(&[Item]) -> R;
    pub async fn count_<scope>(&self) -> usize;
}
```

### Mutation API shapes

Command-verb methods specific to the context:

```rust
impl <Context> {
    pub async fn upsert(&self, item: Item) -> Result<(), <Context>Error>;
    pub async fn remove(&self, id: &str) -> Result<bool, <Context>Error>;
    pub async fn update<F>(&self, id: &str, f: F) -> Result<bool, <Context>Error>
        where F: FnOnce(&mut Item) -> bool;
    // plus context-specific verbs (e.g. Offerings::promote/demote,
    // Pond::join, Jobs::complete, ...)
}
```

### Event API

```rust
impl <Context> {
    pub fn changes(&self) -> broadcast::Receiver<<Context>Changed>;
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct <Context>Changed {
    pub kind: ChangeKind,
    pub affected: Vec<String>,   // IDs, or whatever identifies "what"
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ChangeKind {
    // context-specific variants
}

impl ChangeKind {
    pub fn should_chirp(self) -> bool { /* context-specific */ }
}
```

### Error type

```rust
#[derive(Debug, thiserror::Error)]
pub enum <Context>Error {
    #[error("<context> not found: {id}")]
    NotFound { id: String },

    #[error("invariant violation: {0}")]
    Invariant(&'static str),

    #[error("persistence failed: {source}")]
    Persistence { #[source] source: anyhow::Error },

    // context-specific variants
}
```

`anyhow` is permitted as a wrapper for adapter errors at the persistence
boundary (`Persistence { source }`), but domain logic never produces
`anyhow::Error` directly.

### Port (persistence example)

```rust
pub trait <Context>Store: Send + Sync {
    fn load(&self) -> Pin<Box<dyn Future<Output = Result<<Snapshot>, anyhow::Error>> + Send + '_>>;
    fn save<'a>(&'a self, snapshot: &'a <Snapshot>) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send + 'a>>;
}

pub struct File<Context>Store { /* ... */ }

impl <Context>Store for File<Context>Store { /* ... */ }
```

Ports use the `Pin<Box<Future>>` pattern per ARCH-0007 (async-trait
removed).

### Test scaffold

Every aggregate has `domain/<context>/tests.rs` with the following
minimum coverage:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Fake implementation of every port the aggregate depends on.
    struct Fake<Context>Store { /* in-memory state */ }
    impl <Context>Store for Fake<Context>Store { /* ... */ }

    // Helper that constructs an aggregate with fakes wired.
    fn new_test_<context>() -> (<Context>, Fake<Context>Store, Arc<Metrics>) { ... }

    #[tokio::test]
    async fn upsert_adds_item_persists_and_emits() { ... }

    #[tokio::test]
    async fn remove_nonexistent_returns_false_without_emit() { ... }

    #[tokio::test]
    async fn update_inside_closure_changes_state() { ... }

    #[tokio::test]
    async fn changes_subscriber_receives_events_in_order() { ... }

    #[tokio::test]
    async fn round_trip_persist_reload_preserves_state() { ... }

    // plus command-specific tests per aggregate
}
```

Book 0 writes the test scaffold template as a generic reference. Each
subsequent book instantiates it for its aggregate.

### Tracing

Every public method on every aggregate has:

```rust
#[tracing::instrument(level = "debug", skip(self), fields(<context>.<id_field> = %id))]
pub async fn method(&self, id: &str, ...) -> Result<(), <Context>Error> { ... }
```

Spans are named `<context>.<method>` (e.g., `offerings.promote`,
`tool.register_local`). This gives free timing, parameter capture,
filtering by context, and OpenTelemetry compatibility.

### Projection task (for consumers of events)

```rust
// tasks/task_defs/<consumer>_projection.rs
pub struct <Consumer>ProjectionTask;

impl BackgroundTask for <Consumer>ProjectionTask {
    fn name(&self) -> &'static str { "<consumer>-projection" }
    fn dependencies(&self) -> &'static [&'static str] { &[] }

    fn run(self: Box<Self>, ctx: TaskContext) -> Pin<Box<dyn Future<Output = TaskOutcome> + Send>> {
        Box::pin(async move {
            // Subscribe BEFORE seed (ARCH-0016 lesson).
            let mut feed = ctx.state.<producer>.changes();
            // Seed.
            ctx.state.<consumer>.rebuild_from(&ctx.state.<producer>).await;
            ctx.ready.signal();

            loop {
                tokio::select! {
                    _ = ctx.token.cancelled() => return TaskOutcome::Cancelled,
                    msg = feed.recv() => match msg {
                        Ok(event) => {
                            ctx.state.<consumer>.apply(event).await;
                        }
                        Err(RecvError::Lagged(n)) => {
                            tracing::warn!(skipped = n, "<consumer> projection feed lagged");
                            ctx.state.<consumer>.rebuild_from(&ctx.state.<producer>).await;
                        }
                        Err(RecvError::Closed) => return TaskOutcome::Completed,
                    }
                }
            }
        })
    }
}
```

## The Chapter Template

Every book (except the Prologue) follows this structure. Chapters may be
combined when the scope is small (e.g., Ch2+Ch3, Ch4+Ch5).

### Chapter 0 — Architecture Discussion *(conversation, no commit)*

Required for complex domains (Books VIII+). Optional for simple extractions.

- **Survey the code**: read domain files, count call sites, map data flow.
- **Present the concept map**: how the code sees this domain today —
  types, relationships, state locations, event flow.
- **Propose the target model**: what the domain *should* look like,
  grounded in the survey.
- **Discuss**: the user reacts, redirects, refines. Converge on a
  design. This becomes the input to Ch1's ADR.

Ch0 exists because the most valuable architectural insights come from
the collision between code facts and domain thinking — not from solo
discovery. Books I–VII proved that the books requiring user redirection
(II, VIII) were the ones where Ch0 would have saved time.

### Chapter 1 — Scope & ADR

- **Re-evaluate the plan against the current code** (per [The
  Discovery Mandate](#the-discovery-mandate)). If Ch0 produced an
  agreed model, Ch1 formalizes it as an ADR. If no Ch0, Ch1 does both
  discovery and formalization.
- Write the book's own ADR as `ARCH-NNNN-<book-name>.md`.
- Define the bounded context's surface: state, events, ports, errors,
  commands, queries.
- Declare exit criteria.
- Commit: docs-only.

### Chapters 2–5 — Build, wire, migrate, delete

The exact split varies by book. The common pattern:

- **Extract**: create `domain/<context>/` module, aggregate, events,
  ports, tests. Wire into bootstrap + `FromRef<AppState>`.
- **Migrate**: move every call site to the aggregate's typed API.
  Delete old `AppState` fields and free functions.
- **Clean up**: delete scaffolds, re-exports, back-compat aliases.
  Enter any cross-book scaffolds in `docs/scaffolding.md`.

Each commit lands green on `dev`. A book may use 1–4 commits depending
on blast radius.

### Chapter 6 — Verify & document

- Run exit-criteria `rg` patterns (must return 0 matches).
- `cargo check --all && cargo test --package garden-moss --lib &&
  cargo clippy --package garden-moss --lib -- -D warnings`
- Update `docs/reference/context-map.md` with the finalized context.
- Update `docs/glossary.md` with any new terms.
- Update the book's ADR frontmatter (`completed: <date>`).
- Add a one-line entry to the ARCH-0017 revision history table.
- Commit: docs-only.

**Not in Ch6**: memory file updates (MEMORY.md, project_arch0017_ddd_epic.md).
These are AI session context, not project deliverables.

## The Book List

Twenty books plus a prologue. Ordered for correctness (dependencies) and
for pragmatism (smaller, higher-value books first).

### Book 0 — Prologue: Pattern Codification

**Scope:** Write the foundational documents that every subsequent book
references. Produces no Rust code.

**Deliverables:**
- `docs/specs/domain-aggregates.md` — the pattern spec (full version of
  the "Aggregate Pattern Spec" section above)
- `docs/reference/glossary.md` — ubiquitous language (full version of
  the "Ubiquitous Language" section above)
- `docs/reference/context-map.md` — bounded context map (live document,
  updated by each book)
- `docs/SCAFFOLDING.md` — scaffolding tracker, initially with the single
  ARCH-0016 entry for `ActiveGuard`
- CI: add a check script that validates `docs/SCAFFOLDING.md` entries
  against book status

**Exit criteria:**
- All four documents land under `docs/`
- Scaffolding-tracker CI check runs on every PR (green for the single
  known entry)
- `cargo check --all` green (trivial — no code touched)

**Commit count:** 1 (doc-only; could be 2 if CI check is its own commit)

### Book I — Metrics

> **Scope revised 2026-04-11** per Discovery Mandate. See [ARCH-0018](ARCH-0018-metrics-aggregate.md) for full detail; key points summarized below.

**Scope:** First concrete aggregate. Validates the pattern. Every
subsequent book injects `Arc<Metrics>` at construction from day one.
Also renames the existing hardware-resource endpoint (`/metrics` →
`/resources`) that was squatting on the "metrics" name.

**Bounded context:** `Metrics` (new — domain observability)
**Incidentally renamed:** existing hardware-resource surface →
`Resources` (was `MetricsSnapshot`/`metrics_collection`/`/api/v1/stone/metrics`)

**Deliverables:**
- **Rename** existing hardware-resource types, modules, paths, and
  manifest entries from "metrics" to "resources": `MetricsSnapshot` →
  `ResourcesSnapshot`, `api/v1/metrics.rs` → `api/v1/resources.rs`,
  `domain/metrics_collection.rs` → `domain/resources/collection.rs`,
  `StoneInfoApi::metrics()` → `StoneInfoApi::resources()`,
  `fetch_stone_metrics()` → `fetch_stone_resources()`,
  `/api/v1/stone/metrics` → `/api/v1/stone/resources`, manifest entry
  updated (and corrected — it falsely claimed Prometheus format).
- `domain/metrics/` module with aggregate, event, error, state, tests
- `MetricsState` holding per-domain, per-task, and global metrics as
  `Arc<DomainMetrics>` / `Arc<TaskMetrics>` with atomic internals for
  lock-free hot-path recording
- **No `Store` port** — Metrics is in-memory only; counters reset on
  restart (Prometheus-standard behavior)
- Command methods: `register_domain`, `register_task`,
  `record_domain_event`, `record_mutation_latency`,
  `record_task_transition`, `record_subscriber_lag`
- Read methods: `snapshot`, `global`, `domain(name)`, `domains()`,
  `task(name)`, `tasks()`
- Event methods: `changes()` returning only interesting transitions
  (task state changes, lag, threshold crossings — not counter
  increments)
- **URL surface** (four sibling top-level paths):
  - `/api/v1/stone/capabilities` — existing, unchanged (static hardware)
  - `/api/v1/stone/resources` — **renamed** from `/metrics` (dynamic hardware)
  - `/api/v1/stone/tasks` — existing, returns `SupervisorStatus`
  - `/api/v1/stone/tasks/{name}` — **new** (minor improvement) singular lookup
  - `/api/v1/stone/metrics` — **new** full observability snapshot
  - `/api/v1/stone/metrics/global` — **new** process-wide counters
  - `/api/v1/stone/metrics/domains` — **new** all domain observability
  - `/api/v1/stone/metrics/domains/{name}` — **new** single domain
  - `/api/v1/stone/metrics/tasks` — **new** all task observability
    (complementary to `/tasks`; metrics timings/event counts, not
    lifecycle state)
  - `/api/v1/stone/metrics/tasks/{name}` — **new** single task observability
  - `/api/v1/stone/metrics/stream` — **new** SSE of `MetricsChanged`
- **Complementary to SupervisorHandle, not duplicative.** The supervisor
  keeps owning task lifecycle state (Waiting/Running/Completed); Metrics
  owns timing + event counts. Consumers that want unified view call both
  and join by task name.
- Full test scaffold per the pattern spec
- `Arc<Metrics>` field on `AppState`, constructed at bootstrap
- `FromRef<AppState> for Arc<Metrics>`
- Retrofit ARCH-0016's `Offerings` to inject `Arc<Metrics>` and call
  `record_domain_event` / `record_mutation_latency` in `finalize` —
  no `NoopMetrics` shim exists at any point.
- **Out of scope:** Prometheus exporter. Deferred to a post-epic
  adapter (either `?format=prometheus` query param on `/metrics` or
  a separate `/prometheus` path). Book I ships JSON only.

**Dependencies:** Book 0

**Exit criteria:**
- `rg '/api/v1/stone/metrics' src/moss/src/` returns matches only in
  the NEW Metrics handlers (old `/metrics` path is gone, renamed to
  `/resources`)
- `rg 'MetricsSnapshot' src/` returns 0 matches — the old type is
  renamed to `ResourcesSnapshot`
- `rg 'record_domain_event' src/moss/src/domain/offerings/` returns at
  least 1 match (Offerings is retrofitted)
- `/api/v1/stone/metrics` returns a JSON snapshot with at least the
  `offerings` domain registered
- `/api/v1/stone/resources` returns the hardware snapshot
- `garden-moss` builds, tests pass, clippy clean
- All six chapter commits land green

**Estimated size:** ~2000 lines (up from original ~1500 due to rename scope)

### Book II — Tool

**Scope:** Extract Tool as a bounded context. Move `refresh_local_tools_projection`,
`publish_tool_deltas`, `ingest_tools_beacon`, `remove_tools_for_stone`
off `AppState`. Introduce `Tool::rebuild_from(&Offerings)` as the
canonical projection refresh path.

**Bounded context:** `Tool`

**Deliverables:**
- `domain/tool/` module (existing, deep-cleaned)
- Tool aggregate owning the registry privately
- `ToolChanged` event stream
- `ToolsBeaconTransport` port (UDP adapter in `infra/`)
- `ToolProjectionTask` subscribing to `Offerings::changes()` and
  `Storage`'s changed stream
- Replaces the existing imperative `refresh_local_tools_projection` on
  `AppState`
- Metrics integration
- Tests

**Dependencies:** Book I (Metrics), prior ARCH-0016 (Offerings)

**Exit criteria:**
- `rg 'refresh_local_tools_projection' src/moss/src/` returns 0 matches
- `rg 'state\.tool\.registry\.(read|write)' src/moss/src/` returns 0
  matches outside `domain/tool/`
- `rg 'publish_tool_deltas\|ingest_tools_beacon\|remove_tools_for_stone'
  src/moss/src/app_state.rs` returns 0 matches
- Tools beacon still fires correctly after offering mutations (manual
  verification: `garden-rake list` on a stone shows all adopted
  services)
- All six chapter commits land green

**Estimated size:** ~1800 lines

### Book III — Topology

**Scope:** Consolidate self-entry construction, chirping, health
updates, and resolution changes into a Topology context.

**Bounded context:** `Topology`

**Deliverables:**
- `domain/topology/` module
- Topology aggregate owning self-entry cache
- Moves `build_self_entry`, `sync_self_services`,
  `sync_self_capabilities`, `update_stone_health`,
  `announce_resolution_change` off `AppState`
- `TopologyChanged` event
- `TopologyProjectionTask` subscribing to `OfferingsChanged`,
  `ToolChanged`, `StoneHealthChanged`, `StoneAddressChanged`
- `ChirpTransport` port
- Metrics integration
- Tests

**Dependencies:** Book I (Metrics), Book II (Tool), prior ARCH-0016

**Exit criteria:**
- `rg 'build_self_entry\|sync_self_services\|sync_self_capabilities\|update_stone_health\|announce_resolution_change' src/moss/src/app_state.rs` returns 0 matches
- Chirp broadcasts still fire on offering mutations (manual verification)
- `OfferingsProjectionTask` from ARCH-0016 is retired; its functionality
  is subsumed by `TopologyProjectionTask` + `ToolProjectionTask`
- Scaffolding tracker: mark ARCH-0016's `OfferingsProjectionTask` as
  removed

**Estimated size:** ~1200 lines

### Book IV — Jobs

**Scope:** Extract Jobs from the raw `Arc<RwLock<HashMap<String, Job>>>`
on `AppState`.

**Bounded context:** `Jobs`

**Deliverables:**
- `domain/jobs/` module
- Jobs aggregate with command methods (`create`, `start`, `complete`,
  `fail`, `remove`)
- `JobChanged` event
- Read methods (`find_by_id`, `list_active`, `list_recent`)
- In-memory only (no persistence port needed yet — jobs are ephemeral)
- Metrics integration
- Tests

**Dependencies:** Book I

**Exit criteria:**
- `rg 'state\.jobs\.(read|write)' src/moss/src/` returns 0 matches
- `AppState.jobs` field deleted

**Estimated size:** ~700 lines

### Book V — Catalog (Manifests + offerings_index)

**Scope:** Consolidate `manifest_registry` + `offerings_index` into a
single `Catalog` context. Absorbs the compile-time catalog cache into
proper domain shape.

**Bounded context:** `Catalog`

**Deliverables:**
- `domain/catalog/` module
- `Catalog` aggregate holding `ManifestRegistry` + compiled index
- `CatalogChanged` event (on rebuild / invalidation)
- `ManifestSource` port (filesystem + embedded adapters)
- `CatalogCache` port (disk persistence for compiled index)
- Retires `AppState.manifest_registry` and `AppState.offerings_index`
  fields; replaces them with a single `AppState.catalog: Arc<Catalog>`
- Moves `ensure_offerings_index`, `get_compiled_offering`,
  `rebuild_offerings_index`, etc. from `domain/offerings/catalog.rs`
  into `domain/catalog/`
- The existing `offerings/catalog.rs` is deleted or significantly
  thinned (its remaining content merges with `domain/catalog/`)
- Metrics integration
- Tests

**Dependencies:** Book I, prior ARCH-0016

**Exit criteria:**
- `rg 'manifest_registry\|offerings_index' src/moss/src/app_state.rs`
  returns 0 matches
- `src/moss/src/domain/offerings/catalog.rs` is deleted or reduced to a
  re-export shim that scaffolding tracker records for deletion
- Offerings index rebuild still works on first-boot hardware detection

**Estimated size:** ~1400 lines

### Book VI — Subsystems / Readiness

**Scope:** Replace `AtomicBool` subsystem-ready flags with a
`Subsystems` context whose readiness is event-driven via `watch`
channels.

**Bounded context:** `Subsystems`

**Deliverables:**
- `domain/subsystems/` module
- `Subsystems` aggregate with per-subsystem `watch::Sender<Readiness>`
- Registration API: `register_subsystem(name)`
- Signal API: `mark_ready(name)`, `mark_unready(name, reason)`
- Query API: `is_ready(name)`, `wait_ready(name)`, `snapshot()`
- Event API: `changes()` emitting `SubsystemReady { name }` / `SubsystemUnready { name, reason }`
- Replaces `AppState.subsystems: SubSystems` (the struct of `AtomicBool`
  fields)
- Every `subsystems.network.ready.load(Ordering::Relaxed)` call site
  migrates to `state.subsystems.is_ready("network").await`
- Metrics integration (readiness transitions are interesting events)
- Tests

**Dependencies:** Book I

**Exit criteria:**
- `rg 'AtomicBool' src/moss/src/` outside of `domain/subsystems/` and
  metrics code returns 0 matches
- `rg 'subsystems\.\w+\.ready\.(load|store)' src/moss/src/` returns 0
  matches
- Bootstrap ordering still works (no race where a task starts before
  its subsystem is marked ready)

**Estimated size:** ~900 lines

### Book VII — Health & Probes

**Scope:** Extract health checking and HTTP/TCP probes into a `Health`
context with pluggable probe adapters.

**Bounded context:** `Health`

**Deliverables:**
- `domain/health/` module
- `Health` aggregate owning per-offering health state
- `HealthProbe` port (HTTP, TCP, and HTTP-tag adapters)
- `HealthChanged` event
- Subscribes to `OfferingsChanged` to schedule probes on promote/demote
- `HealthProjectionTask`
- Metrics integration
- Tests

**Dependencies:** Book I, Book II (Tool) — probe results surface in
tool metadata, ARCH-0016

**Exit criteria:**
- `rg 'http_probe\|tcp_probe' src/moss/src/tasks/health_monitor.rs`
  returns 0 matches (logic moved into Health context)
- Existing health-monitor task is retired or reduced to a thin shell
- Probe failures still mark offerings as Degraded

**Estimated size:** ~1300 lines

### Book VIII — Storage Deep-Clean

**Scope:** The existing `Storage` context is partial and internally
messy. Deep-clean into proper sub-aggregates.

**Bounded context:** `Storage` (with sub-aggregates `Storage::Volumes`,
`Storage::Banks`, `Storage::Replication`)

**Deliverables:**
- `domain/storage/` deep clean
- `Volumes` sub-aggregate owning per-volume state
- `Banks` sub-aggregate owning seed-bank lifecycle
- `Replication` sub-aggregate owning replication state machine
- Events: `VolumeChanged`, `BankChanged`, `ReplicationChanged`
- Ports: `VolumeMonitor`, `FileSystem`, `ReplicationTransport`
- Retires the existing `emit_storage_changed` on `AppState`
- S3 listener lifecycle subscribes to `BankChanged`
- Metrics integration
- Tests for each sub-aggregate

**Dependencies:** Book I, Book II (Tool subscribes to `BankChanged` for
seed-bank projection)

**Exit criteria:**
- `rg 'emit_storage_changed' src/moss/src/app_state.rs` returns 0
  matches
- `state.current.storage.*` raw field access outside `domain/storage/`
  returns 0 matches
- Existing replication and S3 functionality still works (manual
  verification)

**Estimated size:** ~2500 lines (largest book)

### Book IX — Security / Pond / Ceremonies

**Scope:** Consolidate the scattered `pond`, `ceremony`, and TLS
components into a coherent `Security` context.

**Bounded context:** `Security` (with sub-aggregates `Pond`, `Ceremonies`,
`Trust`)

**Deliverables:**
- `domain/security/` deep clean
- `Pond` sub-aggregate owning pond membership, CA, enrollment state
- `Ceremonies` sub-aggregate owning ceremony registry, journal, lifecycle
- `Trust` sub-aggregate owning per-stone trust and mTLS material
- Events: `PondChanged`, `CeremonyChanged`, `TrustChanged`
- Ports: `CeremonyJournal`, `PondCertStore`, `MtlsAcceptor`
- Retires scattered `security.pond.ceremony.host`, `ceremony.registry`,
  `ceremony.journal` access patterns
- Metrics integration
- Tests

**Dependencies:** Book I, Book X (Discovery — for enrollment flow)

**Estimated size:** ~1800 lines

### Book X — Discovery, Announcement, Networking

**Scope:** Consolidate mDNS, UDP chirp, koi discovery, and network
interface monitoring into three distinct but co-located bounded contexts.

**Bounded contexts:** `Discovery`, `Announcement`, `Networking`

**Deliverables:**
- `domain/discovery/` (existing, deep-cleaned)
- `domain/announcement/` (new)
- `domain/networking/` (new)
- `Discovery` aggregate: known peers, last-seen, `StoneDiscovered` /
  `StoneLost` events
- `Announcement` aggregate: chirp scheduler, periodic + triggered
  emission, `ChirpEmitted` event
- `Networking` aggregate: interface state, IP change detection,
  `NetworkStateChanged` event
- Ports: `MdnsTransport`, `KoiClient`, `ChirpTransport`, `InterfaceMonitor`
- Retires `discovery.mdns` field on Current, scattered network-monitor
  code, and existing periodic-announcer task
- Metrics integration
- Tests

**Dependencies:** Book I, Book III (Topology emits events Announcement
subscribes to)

**Estimated size:** ~1600 lines

### Book XI — Orchestration Deep-Clean

**Scope:** The existing partial `Orchestration` context has tick
aggregation, nudge, nurturing, offering election state. Deep-clean.

**Bounded context:** `Orchestration`

**Deliverables:**
- `domain/orchestration/` deep clean
- `Tick` sub-aggregate for storage tick aggregation
- `Nurturing` sub-aggregate for nurturing lifecycle
- `Election` sub-aggregate for offering primary/dormant coordination
- Events: `OrchestrationTick`, `ElectionResolved`
- Ports: `ElectionTransport`
- Metrics integration
- Tests

**Dependencies:** Book I, Book VIII (Storage tick source), ARCH-0016

**Estimated size:** ~1400 lines

### Book XII — ContainerRuntime Port

**Scope:** Extract the Bollard/Docker dependency behind a
`ContainerRuntime` port with an anti-corruption layer. Foreign Bollard
types never bleed into domain code.

**Bounded context:** `ContainerRuntime` (port + adapter; no aggregate
state)

**Deliverables:**
- `infra/container_runtime/` module with the `ContainerRuntime` trait
- `BollardAdapter` implementing the trait
- Every domain call site using `platform.docker.*` migrates to
  `container_runtime.*` with typed domain arguments
- Anti-corruption: Bollard types live only inside the adapter; domain
  code uses `domain::container::*` types
- Tests against a fake runtime

**Dependencies:** Book I, Book II (Tool), Book VII (Health), Book VIII
(Storage — all of these currently talk to Docker directly)

**Estimated size:** ~1800 lines

### Book XIII — Configuration

**Scope:** Extract typed configuration into a `Configuration` context
with event-driven hot-reload for the parts that support it.

**Bounded context:** `Configuration`

**Deliverables:**
- `domain/configuration/` module
- `Configuration` aggregate holding typed env + runtime settings
- `ConfigChanged` event for hot-reload scenarios
- `ConfigSource` port (env + file + overrides adapters)
- Retires scattered `EnvConfig` accessors
- Metrics integration
- Tests

**Dependencies:** Book I

**Estimated size:** ~800 lines

### Book XIV — Persistence Consolidation

**Scope:** Every domain has its own `Store` port by now. Unify the
file-backed adapter helpers so atomic-write invariants, directory
creation, temp-file naming, and error conversion happen in one place.

**Bounded context:** `Persistence` (not an aggregate — a set of shared
adapter helpers)

**Deliverables:**
- `infra/persistence/` module with:
  - `AtomicJsonStore<T>` helper
  - `DirectoryCache<K, V>` helper
  - Canonical error conversion
- Every per-domain `File<X>Store` adapter uses the shared helpers
- Tests for the helpers

**Dependencies:** Books I–XI (every aggregate with a Store port must
exist first)

**Estimated size:** ~700 lines

### Book XV — Logging

**Scope:** The `AppState.log: broadcast::Sender<String>` field and file
sink become a proper `Logging` context.

**Bounded context:** `Logging`

**Deliverables:**
- `domain/logging/` module
- `Logging` aggregate owning the log broadcast channel and sink handle
- `LogSink` port (file + stderr + memory adapters)
- SSE log-streaming handler migrates to the context
- Tracing integration: a custom tracing layer emits into the aggregate
- Tests

**Dependencies:** Book I

**Estimated size:** ~900 lines

### Book XVI — EventBus / Pulse Unification

**Scope:** Unify `EventBus`, `PulseEvent`, per-domain `changes()`
streams, and `PulseDomainBridge` into a coherent cross-cutting surface.

**Bounded context:** `Events` (replaces scattered EventBus + Pulse)

**Deliverables:**
- `domain/events/` module
- Clear separation between:
  - **Domain events** — each aggregate's typed `changes()` stream (Book
    I–XIII's work)
  - **Pulse events** — the firehose translation for SSE consumers
  - **Transport events** — announcements, network events, adapter events
- `PulseProjectionTask` subscribes to every domain's `changes()` and
  translates to `PulseEvent`
- Retires ad-hoc `state.event_bus.emit(...)` calls in favor of domain
  mutations that emit naturally
- Metrics integration (pulse subscriber lag is an interesting event)
- Tests

**Dependencies:** Books I–XV (every domain with events must exist first)

**Estimated size:** ~1100 lines

### Book XVII — HTTP API Thin Layer

**Scope:** Every HTTP handler becomes a thin command/query dispatcher.
API response types separated from domain types via DTO mapping.

**Bounded context:** `HttpApi` (application layer, not a domain
aggregate)

**Deliverables:**
- Every handler in `api/v1/` refactored to a single-statement dispatch:
  `Ok(Json(state.<domain>.<command_or_query>(...).await?))`
- DTOs in `api/dto/` separated from domain types
- Anti-corruption layer: handlers translate DTOs at the boundary
- `FromRef<AppState>` extraction used throughout; no handler takes
  `State<AppState>` as a whole
- Error types translate to `(StatusCode, Json<ErrorResponse>)` via a
  shared `IntoResponse` impl per domain error type
- Tests: handler-level tests using a fake `AppState` built from fakes
  for each domain

**Dependencies:** Books I–XVI (every domain the handlers talk to must be
properly extracted first)

**Estimated size:** ~2000 lines (touches every handler file)

### Book XVIII — Offerings Strangler Removal

**Scope:** Delete `Offerings::read()`, `ActiveGuard`, `CandidatesGuard`,
and the `get_offerings` / `find_offering` delegates on `AppState`. The
ARCH-0016 strangler vine is fully uprooted.

**Bounded context:** touches `Offerings` only

**Deliverables:**
- Delete `domain/offerings/guard.rs`
- Delete `Offerings::read()`, `read_candidates()`
- Delete `AppState::get_offerings`, `get_managed_offerings`,
  `get_adopted_offerings`, `get_borrowed_offerings`, `find_offering`,
  `find_offering_by_id`
- Migrate any remaining read sites to typed query methods
- Update `docs/SCAFFOLDING.md` — ARCH-0016's entry is removed

**Dependencies:** All prior books (read sites get migrated naturally as
each book touches its area)

**Exit criteria:**
- `rg 'state\.offerings\.read\(\)' src/moss/src/` returns 0 matches
- `rg 'ActiveGuard\|CandidatesGuard' src/moss/src/` returns 0 matches
- `docs/SCAFFOLDING.md` entry for ARCH-0016 is marked removed

**Estimated size:** ~300 lines (mostly deletions)

### Book XIX — AppState Dissolution

**Scope:** `AppState` is renamed (probably to `Moss`) and reduced to a
pure dependency container. Every method that does work moves into its
owning context.

**Bounded context:** root

**Deliverables:**
- `AppState` → `Moss` rename (`git mv` in a dedicated commit)
- All remaining methods on `AppState` either:
  - Move into their owning domain context, OR
  - Stay only if they represent genuine cross-cutting lifecycle concerns
    (shutdown orchestration, bootstrap output wiring)
- `AppState.log`, `AppState.subsystems`, `AppState.task_supervisor`,
  etc. — all verified to be context-owned by now
- Final `Moss` struct is a list of `Arc<Domain>` fields plus shutdown
  token, start time, event bus handle. That's it.
- Tests

**Dependencies:** All prior books

**Exit criteria:**
- `AppState` type is renamed to `Moss` (or stays, if that name turns
  out to be domain-appropriate — unlikely)
- `impl Moss { ... }` has no methods doing mutation (only construction
  helpers if any)
- Every handler uses `FromRef` extraction for narrow dependencies
- `state.<field>.<method>()` is the only call shape anywhere

**Estimated size:** ~1500 lines (rename + final cleanup)

### Book XX — Epilogue: CI Enforcement + Deferred Renames

**Scope:** Lock in the pattern with automated checks. Settle all
deferred renames. Write the epic postmortem.

**Deliverables:**
- **Resolve all deferred renames** in `docs/scaffolding.md`. Every
  entry's wire-format rename is coordinated across moss, rake, and
  consumers in a single sweep. No deferred rename survives past this
  book. Current entries:
  - `deferred-placement-metrics` (Book I)
  - `deferred-job-offerings-field` (Book IV)
  - `deferred-registry-loader-task-rename` (Book V)
- CI check: `cargo-modules` layering lint (`domain/` cannot import
  `infra/`)
- CI check: custom script forbidding:
  - `Arc<RwLock<Vec<_>>>` or `Arc<RwLock<HashMap<_,_>>>` as public
    fields on any domain type
  - `pub(crate) fn` returning `RwLockWriteGuard`
  - `anyhow::Error` in return types inside `domain/` modules
- CI check: every `BackgroundTask` that subscribes to a `changes()`
  stream uses subscribe-before-seed pattern (heuristic grep)
- Scaffolding tracker audit: `docs/SCAFFOLDING.md` must be empty of
  both active entries and deferred renames by this book's close
- Epic postmortem: `docs/history/arch-0017-epic-postmortem.md`
- Refresh `docs/reference/components.md`, `docs/specs/*`, and the
  project README to reflect the final architecture

**Dependencies:** All prior books

**Exit criteria:**
- All CI checks enabled on `main`
- `docs/SCAFFOLDING.md` has zero entries (active or deferred)
- Postmortem written and linked from `docs/history/`

**Estimated size:** ~500 lines (mostly CI + docs)

## Total Scope

- **21 documents** (1 prologue + 20 books)
- **~22,000–28,000 lines** of diff, most of it deletions of scattered
  code replaced by concentrated, tested domain code
- **~120 commits** (6 per book + prologue + epilogue)
- **~20 new `BackgroundTask` projection subscribers**, registered in the
  task registry
- **~20 new typed error enums** (one per bounded context)
- **~15 new persistence/transport ports** with adapters
- **~300 new unit tests** (minimum 10 per aggregate, more for complex
  contexts)

## Consequences

### Positive

- **Structural bug prevention.** The `promote_adopted` class of bug
  becomes impossible to write: aggregate state is private, mutation is
  commanded through typed methods, and projections subscribe. Any
  regression attempt fails at compile time.
- **Testability by construction.** Every context is testable with
  in-memory fakes. Integration with real infrastructure is confined to
  adapters, which have their own minimal tests.
- **Operational observability.** Every mutation is metered, timed,
  traced. Dashboard surfaces (`/api/v1/stone/metrics`, `/tasks`) have
  rich structured data without new infrastructure.
- **Onboarding clarity.** A new contributor reads `docs/specs/domain-aggregates.md`,
  `docs/reference/glossary.md`, and `docs/reference/context-map.md`
  and knows how to add a new bounded context or extend an existing
  one without asking.
- **Refactor velocity.** After the epic, touching a domain doesn't risk
  unrelated domains. Each aggregate is a surgical target.
- **Future migration enablement.** Swapping Bollard for containerd, or
  adding a second persistence backend, or changing the wire format —
  all become localized changes.

### Negative

- **Scope and duration.** Twenty books of coordinated refactor work is
  a multi-month commitment. The project incurs opportunity cost during
  the epic.
- **Intermediate-state complexity.** Until Book XIX, `AppState` will
  shrink gradually but not disappear. Developers must internalize the
  shippability rule ("every book green") rather than expect a clean
  state at every commit.
- **CI pressure.** Each book adds tests, lints, and scaffolding checks.
  CI run time will grow throughout the epic.
- **Trait-object overhead.** Every aggregate holds `Arc<dyn Port>`
  fields. This is one pointer indirection per infrastructure call. For
  moss's workload this is negligible, but it is non-zero.
- **`Pin<Box<Future>>` in port traits is visually heavier than
  `async-trait`.** We pay a verbosity tax in exchange for no proc-macro
  dependency and static dispatch everywhere else.

### Neutral

- **The `Offering` struct in `garden_common` is not touched.** The epic
  is moss-internal. Rake, orchestrators, and wire formats stay stable.
- **External API contracts stay stable.** REST endpoints, UDP wire
  format, JSON schemas are unchanged throughout the epic. Internal
  Rust APIs are rewritten freely.
- **Metaphorical domain language (stones, pond, moss, offerings,
  nourishment) is preserved.** It is already ubiquitous language in
  the DDD sense — just codified into `docs/reference/glossary.md`.
- **ARCH-0016's `OfferingsProjectionTask` is retired mid-epic** (in
  Book III, when Topology absorbs its responsibilities). Scaffolding
  tracker records this with a removal trigger.

## Migration Plan

The migration plan is the book list above. Each book is a self-contained
PR-sized unit landing on `dev`. The epic is complete when Book XX's CI
checks pass and `docs/SCAFFOLDING.md` has zero active entries.

No cross-book atomicity. No long-lived epic branch. No back-compat
wrappers beyond those explicitly tracked in `docs/SCAFFOLDING.md` with
concrete removal triggers.

Book 0 is a prerequisite for all subsequent books. Book I (Metrics) is a
prerequisite for Books II–XIX. Beyond those, the dependency order listed
under each book governs sequencing — books with no unsatisfied
dependencies can run in parallel if multiple contributors are available.

## References

- [ARCH-0004](ARCH-0004-appstate-domain-context-extraction.md) — first
  domain context extraction from `AppState`
- [ARCH-0005](ARCH-0005-structural-quality-pass.md) — structural quality
  pass that preceded this epic
- [ARCH-0007](ARCH-0007-monomorphic-domain-traits.md) — edition 2024,
  monomorphic traits, async-trait removal (establishes the
  `Pin<Box<Future>>` port pattern)
- [ARCH-0015](ARCH-0015-task-supervisor-registry.md) — `BackgroundTask`
  trait and task registry (every projection task in this epic is a
  `BackgroundTask`)
- [ARCH-0016](ARCH-0016-offerings-aggregate-domain.md) — first DDD
  aggregate in moss, proving the pattern this epic generalizes
- [code-standards.md](../code-standards.md) — the authoritative style
  guide, especially §3 (no architectural suffixes), §5 (domain
  ownership through struct nesting), §10 (typed domain errors), §13
  (event subscription API), §14 (one file per concept), §16–20
  (hygiene rules)
- Evans, _Domain-Driven Design: Tackling Complexity in the Heart of
  Software_ (2003) — for the tactical pattern vocabulary this epic
  uses (bounded context, aggregate, ubiquitous language,
  anti-corruption layer, port and adapter)
