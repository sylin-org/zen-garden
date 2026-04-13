# The Moss Refactor: A Technical Narrative

**ARCH-0017 — DDD Monolith Epic**
**April 2026**

---

## The starting point

Moss is the daemon that runs on every stone in a Zen Garden — a distributed platform for self-hosted infrastructure. It manages Docker containers, discovers peer stones, replicates storage, handles mTLS enrollment, and serves a REST/S3/WebDAV API surface. One binary, one process, one codebase.

Moss grew the way production software grows: feature by feature, urgency by urgency. A `struct AppState` started as a thin dependency container and accumulated 25 fields mixing domain data, cross-cutting infrastructure, and raw concurrent maps. Offerings were a `Vec<Offering>` behind an `Arc<RwLock<>>`. Jobs were a `HashMap<String, Job>` with no TTL — they accumulated in memory forever, bounded only by uptime. Health checks, topology caching, subsystem readiness flags — all wired through public fields that any module could read or mutate without ceremony.

The code worked. It shipped. Stones ran it in production. But every new feature touched more files than it should have, because there were no boundaries. A bug in ARCH-0016 proved the structural risk: `promote_adopted` bypassed the offerings mutation gateway because the gateway was a code comment, not a type. The fix landed a proper DDD aggregate for offerings — private state, typed commands, event-driven projections. It worked so well that it raised an obvious question: what if every module had this shape?

## The plan

ARCH-0017 committed to applying the DDD aggregate pattern uniformly across every module in moss. Twenty books plus a prologue, each extracting one bounded context:

- **Private state** behind `RwLock` or equivalent — no public fields crossing domain boundaries.
- **Typed commands** for every mutation — `submit`, `start`, `complete`, not `map.write().await.get_mut(id).status = Running`.
- **Domain events** via `broadcast::Sender` — subscribers react to state changes, not imperative pokes.
- **Ports and adapters** for infrastructure — the domain never imports `crate::infra::*`.
- **Typed queries** returning owned values — no lock guards leaking across boundaries.

The plan estimated ~22,000 lines across ~120 commits. Every book would ship green to `dev` — no long-lived branches, no cross-book atomicity, no "it'll work once everything lands."

A **Discovery Mandate** acknowledged that the plan was a hypothesis: as each book opened, Chapter 1 would re-evaluate the plan against the actual code. The author was *mandated* — not permitted, mandated — to change the plan when reality disagreed.

## What actually happened

### The aggregates

Seven bounded contexts received full DDD aggregate treatment:

**Metrics** (Book I) established the pattern. A `Metrics` aggregate with `register_domain` / `record_domain_event` / `record_mutation_latency` — lock-free after registration via the "register-with-kinds" pattern. Every subsequent aggregate injected `Arc<Metrics>` from day one, so observability was baked in from the start, not bolted on later.

**Tool** (Book II) was the first test of the migration pattern at scale. Fifty direct `registry.read().await` sites across 20+ files. The "field-level strangler" pattern emerged here — the aggregate exposes its state as a `pub(crate)` field temporarily so legacy call sites compile unchanged while typed methods grow alongside. The strangler was retired inside the same book. Two event streams coexisted: `ToolChanged` (internal, rich metadata) and `ToolDelta` (wire format for SSE and UDP beacons). This "dual event streams" pattern was documented and reused by Jobs (Book IV).

**Topology** (Book III) was the second persistent aggregate. A `TopologyStore` port for disk persistence, a `ChirpTransport` port for UDP announcements, and a `SelfEntryInputs` composition struct that assembled a stone's self-description from seven upstream sources without ever holding a back-reference to `AppState`. The aggregate owned the invariant that every cache mutation marks dirty for persistence — collapsing a prior split into two functions (`upsert_from_chirp` and `upsert_from_chirp_dirty`) into one.

**Jobs** (Book IV) was the first ephemeral aggregate with a reaper. The Discovery Mandate found that jobs accumulated forever in memory — a production memory leak bounded only by uptime. The aggregate added a `maintain` command with a 24-hour TTL on terminal jobs, and a `JobsReaperTask` that called it every 10 minutes. A 5-second completion-poll loop in bootstrap was deleted entirely once typed commands made `install_batch_task().await` terminal-state-guaranteed.

**Catalog** (Book V) introduced the first typed error enum in the epic. `CatalogError` with four variants (`ManifestHashFailed`, `CompilationFailed`, `CacheReadFailed`, `CacheWriteFailed`) — commands returned `Result<T, CatalogError>` instead of the infallible mutations that prior books used. The aggregate held a frozen `Arc<ManifestRegistry>` (a cross-crate type from `garden_common`) as an immutable input — the first aggregate to explicitly model a "frozen input" that is part of the state shape but not subject to mutation or events.

**Subsystems** (Book VI) replaced `AtomicBool` readiness flags with `watch` channels. The aggregate's state map was frozen after a single-threaded registration phase — no `RwLock` needed. Mutations flowed through `watch::Sender::send_modify`, which is inherently thread-safe. This "lock-free state" pattern was the fifth design dimension in the pattern spec.

**Health** (Book VII) was a stateless command facade — it orchestrated a probe-compare-mutate-emit pipeline but held no mutable state of its own. The `HealthProbe` port wrapped Docker health checking. The plan anticipated HTTP/TCP probe adapters; the code had none. The Discovery Mandate caught this, and the book shipped the actual shape instead of the anticipated one.

### The storage conversation

Book VIII was where the epic nearly went wrong — and then went very right.

The original plan called for three sub-aggregates: `Volumes`, `Banks`, `Replication`. A background agent started the Discovery Mandate survey. Then the human stopped it.

"Let's try to understand how the code sees these concepts."

What followed was a domain modeling conversation that reshaped the entire storage architecture. The code used "replica set" for the grouping concept — the set of physical volumes replicating the same content. But "replica set" is a mechanism name. It describes *how* data gets copied, not *what* the user cares about. The user cares about their named storage: "personal," "media," "backups."

The right domain term was **Bank**. A Bank is the user-facing named storage container. It groups Volumes across stones. Users interact with banks by name; they never see individual volumes. S3, WebDAV, and the REST file API are just different protocol adapters sitting in front of the same Bank commands. Internal operations — offering snapshots, harvest backups, seed-bank restore — are just another caller of `Bank::write()`. Every write path is the same path.

This insight — that the FQN is a grouping concept of possibly asymmetric storage units — produced a cleaner architecture in 20 minutes of conversation than 20 hours of mechanical extraction would have. It led to a process improvement: **Chapter 0**, an architecture discussion before code, required for complex domains.

Book VIII shipped in two sub-books: VIII-a (domain model — Bank as aggregate root, `StorageBank` renamed to `VolumeIngestor`) and VIII-b (API surface — new `/v1/garden/banks` endpoints, `BankContentOps` data-plane port, backward-compat redirects).

### The dissolutions

After the storage conversation, the remaining books met a leaner codebase. Seven of them dissolved:

**Orchestration** (Book XI) — the `Orchestration` struct was a 110-line coordination bag holding raw channels. No domain state, no invariants. The storage coordination primitives moved to `Current::Storage::Coordination`; nurturing and nourishment promoted to direct `AppState` fields. The struct was deleted.

**ContainerRuntime** (Book XII) — the plan called for a `ContainerRuntime` trait abstracting Bollard. Discovery found that `docker::Client` already *was* the anti-corruption layer — all 30 methods accepted and returned domain types, with Bollard confined to method bodies. The book renamed `Client` to `ContainerRuntime`, sealed the one Bollard type leak (`EventMessage` became `ContainerEvent`), and deleted dead code.

**Configuration** (Book XIII) — `MossConfig` is loaded once at boot from TOML and frozen. No hot-reload, no runtime mutation. Pure value objects don't warrant aggregates.

**Persistence** (Book XIV) — the consolidation target (`AtomicJsonStore<T>`) already existed in `garden_common::persistence`. Store port adapters were already 10-20 lines each.

**Logging** (Book XV) — a single `broadcast::Sender<String>` with one SSE consumer. `tracing-subscriber` handles everything else. Infrastructure, not domain.

**EventBus/Pulse** (Book XVI) — three event channels (per-aggregate `changes()`, EventBus for user-facing domain events, Pulse for SSE firehose) serve different populations correctly. Unifying them would scatter translation logic for no benefit.

**HTTP API** (Book XVII) — after 16 books of aggregate extraction, handlers were already thin dispatchers. 85 aggregate method calls across 27 handler files. The "thin layer" refactor had already happened organically.

Each dissolution produced a one-page ADR arguing for *not* doing something. Together, they validated a meta-principle: **the plan is a hypothesis, and the code is the evidence.**

### The final acts

**Offerings Strangler Removal** (Book XVIII) was the book with the most concrete deliverable: removing the `ActiveGuard` scaffold from ARCH-0016 that had been tracked in `docs/scaffolding.md` since the very first aggregate. Eighty-one `.read().await` sites migrated to typed queries (`snapshot`, `find_by_id`, `find_by_name`, `with_active`, `count_active`). The guard file was deleted. Active scaffolds: zero.

**AppState Dissolution** (Book XIX) renamed `AppState` to `Moss`. The struct that started as a 25-field bag of mixed concerns was now a clean dependency container — every field either an `Arc<Aggregate>` or cross-cutting infrastructure. Seven delegate methods were inlined at call sites. 555 occurrences renamed across 97 files in one mechanical commit.

**The Epilogue** (Book XX) resolved all three deferred renames that had accumulated across the epic: `Job.offerings` became `Job.targets` (with `#[serde(rename)]` for wire compatibility), `registry-loader` became `offerings-reconciler`, and `PlacementMetrics` was closed without rename (the name was accurate). The scaffolding tracker was emptied. ARCH-0017's status changed from `accepted-living` to `completed`.

## What we accomplished

### The numbers

- **21 books**, ~120 commits, every one shipping green to `dev`
- **11 DDD aggregates** with typed commands, queries, events, and ports
- **7 dissolutions** that proved the existing structure was already correct
- **764 tests** (from 649 at the start)
- **81 strangler sites** migrated and the scaffold deleted
- **3 deferred renames** resolved
- **0 active scaffolds** remaining
- **0 errors** on integration test and production deploy

### The architecture

Before the epic, moss was a monolith with implicit boundaries. Domain logic leaked across module lines. State was public. Mutations happened through raw map access. Events were imperative pokes. Infrastructure types bled into domain code.

After the epic, moss is a **modular monolith** — a single binary where every moving part is a bounded context with an explicit contract. Aggregates own their state privately. Mutations flow through typed commands that enforce invariants in the type system, not in comments. Events broadcast on mutation, and downstream consumers subscribe. Infrastructure dependencies are injected through ports, not imported directly.

The `Moss` struct (formerly `AppState`) is a dependency container and nothing more. It holds `Arc<Aggregate>` fields and cross-cutting infrastructure. It has one coordination method (`emit_storage_changed`) that bridges four aggregates for a single cross-cutting concern — documented, intentional, and visible.

### The benefits

**Structural bug prevention.** The class of bug that motivated the epic — "a mutation bypassed the canonical gateway because the gateway was a code comment" — is now unrepresentable. The gateway is a type. You can't bypass `Offerings::upsert()` by accident because there's no public field to write to.

**Operational hygiene.** The jobs memory leak (accumulate forever, bounded only by uptime) was found and fixed in Book IV. The 5-second completion-poll loop was deleted. The `ActiveGuard` strangler survived 18 books and was cleanly retired. These aren't glamorous wins, but they're the kind of thing that prevents 3 AM pages.

**Developer ergonomics.** `state.jobs.submit(id, "install", targets).await` is self-documenting. `state.jobs.write().await.insert(id, Job { status: Pending, ... })` is not. The aggregate's typed surface tells you what operations are possible; the raw map told you nothing. New contributors can read the aggregate API and understand the domain without reading the implementation.

**Testability.** Every aggregate is constructible in isolation with in-memory fakes. The `NoopBeaconTransport`, `NoopChirpTransport`, `FileCatalogCache`, `DockerHealthProbe` — these are test doubles that exist because the ports exist. Before the epic, testing required constructing a full `AppState` with a real Docker connection.

**Evolvability.** Adding a new storage protocol (e.g., NFS) means implementing `BankContentOps` and registering a new route. Adding a new health probe type means implementing `HealthProbe`. The aggregate doesn't know or care what adapter is behind the port. This is the promise of ports and adapters — and now it's real, not theoretical.

### The process

The epic refined its own process as it went:

- **The Discovery Mandate** prevented building the wrong shape. Every Chapter 1 that surveyed real code changed the plan.
- **Chapter 0** (architecture discussion before code) was introduced after Book VIII and caught domain model issues before they became migration debt.
- **The dissolution pattern** proved that honest evaluation of existing code is more valuable than mechanical pattern application. Seven books said "the code is already right" — and that's a valid, valuable finding.
- **Background agents** handled mechanical books (VI, VII, IX-XX) while the main conversation drove architectural decisions (VIII, process cleanup). The division of labor emerged naturally: agents for execution, conversation for design.
- **The pattern spec** evolved from "canonical form + 5 exceptions" to a decision tree with five dimensions. After 11 aggregates, the "deviations" *were* the pattern.

## The insight that mattered most

The most important moment in the entire epic was not a code change. It was a question:

> "Aren't S3, WebDAV and the custom file APIs just different expressions of the same storage access concept?"

That question, asked by a human looking at code they'd written, reframed the entire storage domain from "three protocol handlers with duplicated logic" to "one domain concept with three wire-format adapters." It led to Bank as the aggregate root, which led to the unified write path, which led to the principle that internal operations (harvest, seed-bank, replication) are just another caller of `Bank::write()`.

No amount of mechanical pattern application would have found that. It took a person who understood the domain — not the code, the *domain* — asking the right question at the right moment.

The epic was a collaboration. The code is the artifact. The architecture is the outcome. But the insight was the thing that made it worth doing.

---

*ARCH-0017: 21 books, 11 aggregates, 7 dissolutions, 764 tests, zero errors on deploy.*
*April 11-12, 2026.*
