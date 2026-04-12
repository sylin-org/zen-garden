---
audience: [developer, ai]
doc_type: reference
status: canonical
last_verified: 2026-04-12
---

# Bounded Context Map

**Purpose:** Live inventory of every bounded context in moss — what it owns, what events it emits, what it subscribes to, and what infrastructure ports it depends on.
**Audience:** Developers navigating the codebase, planning a refactor, or reviewing a book in [ARCH-0017](../decisions/ARCH-0017-ddd-monolith-epic.md).

> This is a **live** document. Every book's Chapter 6 updates the "Current" section for contexts it has finished; the "Target" section is stable and reflects the epic's end state.

---

## Contents

- [How to read this map](#how-to-read-this-map)
- [Current state](#current-state)
- [Target state](#target-state)
- [Event flow graph](#event-flow-graph)
- [Port inventory](#port-inventory)
- [Maintenance](#maintenance)

---

## How to read this map

Each context entry lists:

- **Status** — one of:
  - **Full** — conforms to the [domain aggregate pattern](../specs/domain-aggregates.md)
  - **Partial** — exists as a struct or module but does not yet enforce the pattern (private state, events, ports, tests)
  - **Absent** — does not exist yet; scattered across other contexts or as raw `Moss` fields
  - **Retired** — no longer exists; its responsibilities absorbed by another context
- **Owns** — what state the context holds
- **Emits** — what domain events it publishes via `changes()`
- **Subscribes** — what other contexts' events it reacts to (usually through a projection task)
- **Ports** — what infrastructure traits it depends on
- **Source** — where in the repo the context lives
- **Book** — which book in the epic owns this context's extraction / cleanup

---

## Current state

As of 2026-04-12, after [ARCH-0031](../decisions/ARCH-0031-configuration-dissolution.md).

### Full contexts

#### Offerings

- **Status:** Full. ARCH-0016 introduced the aggregate; Book I Chapter 4 retrofitted it to inject `Arc<Metrics>` and record mutation latency + per-kind event counts through the `finalize` pipeline.
- **Owns:** active offerings pool, adopted-candidates pool
- **Emits:** `OfferingsChanged { kind, affected, timestamp }` with 8 `ChangeKind` variants (Upserted, Removed, Updated, Promoted, Demoted, Replaced, Coalesced, BatchUpdated). Each variant has a stable `name()` for Metrics per-kind counter lookup and a `ChangeKind::ALL_NAMES` constant for registration.
- **Subscribes:** none (root of the service pipeline)
- **Ports:** `OfferingStore` → `FileOfferingStore`
- **Cross-cutting:** `Arc<Metrics>` injected at construction (ARCH-0018)
- **Source:** `src/moss/src/domain/offerings/`
- **Remaining gap:** still one — typed errors. Mutation methods return `bool` / `()` rather than `Result<_, OfferingsError>`. A later cleanup book will audit this. Tracked implicitly in the pattern spec checklist.
- **Book:** ARCH-0016 (pre-epic); Book I retrofitted for Metrics injection; Book XVIII closes the strangler vine; typed errors are outstanding debt.

#### Metrics

- **Status:** Full. ARCH-0018 Book I Chapters 3–5 introduced the aggregate, wired it into Offerings + task supervisor + projection task, and exposed the HTTP read surface.
- **Owns:** per-domain counters + per-kind events, per-task observability (timing, event counts, subscriber lag), global process-wide counters, latency histogram with 9 fixed buckets (1ms–5s + `+Inf`)
- **Emits:** `MetricsChanged` enum with 5 transition variants (DomainRegistered, TaskRegistered, TaskReady, TaskStateChanged, SubscriberLagDetected). Counter increments deliberately do NOT fire events — only interesting transitions. Consumers poll `/api/v1/stone/metrics` for counter values.
- **Subscribes:** none (Metrics is a pure observer; other contexts push data into it via the mutation API)
- **Ports:** none (in-memory only; counters reset on process restart per ARCH-0018 documented deviation — standard Prometheus-style behavior)
- **Source:** `src/moss/src/domain/metrics/`
- **Three documented deviations from the pattern spec** (justified in ARCH-0018): no `Store` port, infallible mutations (return `()` not `Result`), no `affected` field on events
- **HTTP surface:** `/api/v1/stone/metrics` (snapshot), `/metrics/global`, `/metrics/domains`, `/metrics/domains/{name}`, `/metrics/tasks`, `/metrics/tasks/{name}`, `/metrics/stream` (SSE)
- **Book:** ARCH-0018 (Book I of ARCH-0017) — **COMPLETE**

#### Topology

- **Status:** Full. ARCH-0020 (Book III of ARCH-0017) — completed 2026-04-11.
- **Owns:** peer cache (discovered + offline stones indexed by stone_id), persistence dirty flag, self-entry assembly from upstream domains
- **Commands:** `upsert_from_chirp` (always-dirty invariant), `mark_stone_offline`, `forget_stone`, `maintain` (periodic eviction + persist), `flush` (graceful shutdown), `build_self_entry` (query-style), `sync_services` / `sync_capabilities` / `update_stone_health` / `announce_resolution_change` (self-entry commands with `auto_chirp` gate), `chirp` (transport passthrough)
- **Queries:** `all_stones`, `online_stones`, `get_by_id`, `get_by_name`, `count`, `online_count`, `is_dirty`
- **Emits:** `TopologyChanged` with six `ChangeKind` variants — `Discovered`, `Online`, `Offline`, `Forgotten`, `Evicted`, `Chirped`. Fires only on interesting transitions — peer refreshes of unchanged entries do NOT fire events.
- **Subscribes:** composition helpers pull `OfferingsChanged` into the self-entry reassembly path (`sync_services`); no internal subscription on the aggregate itself.
- **Ports:** `ChirpTransport` → `P2pChirpTransport` (UDP `STONE_CHIRP` announcements), `TopologyStore` → `FileTopologyStore` (TOPO-0002 atomic JSON writes)
- **Cross-cutting:** `Arc<Metrics>` injected at construction; mutation latency + per-kind event counters recorded on every command.
- **Composition layer:** `domain::topology::composition::*` holds the `Moss`-bound helpers (`self_entry_inputs`, `build_self_entry`, `sync_services`, `sync_capabilities`, `update_stone_health`, `announce_resolution_change`) so the aggregate holds no back-reference to `Moss`.
- **Source:** `src/moss/src/domain/topology/` (mod.rs + aggregate.rs + event.rs + error.rs + transport.rs + store.rs + composition.rs)
- **Book:** III (ARCH-0020) — **COMPLETE**

#### Resources (incidentally extracted by Book I Chapter 2)

- **Status:** Full-but-thin. Book I Chapter 2 renamed the existing hardware-resource surface from "metrics" to "resources" across `garden-common`, `garden-moss`, and the typed client. Not a new aggregate — Resources is a simple facade over `Current::Resources` (the runtime hardware snapshot updated by the `resources-collector` background task).
- **Owns:** CPU/memory/disk/network snapshot behind `Current::Resources` (system/network/GPU fields each under their own `Arc<RwLock<Option<...>>>`)
- **Emits:** none (pure dynamic snapshot, polled via `/api/v1/stone/resources`)
- **Subscribes:** OS events through `resources-collector` task
- **Ports:** none (direct `garden_common::resources::system` calls; a future Book could abstract this but it's low priority)
- **Source:** `src/common/src/resources/system.rs`, `src/moss/src/api/v1/resources.rs`, `src/moss/src/domain/resources_collection.rs`, `src/moss/src/tasks/resources_collector.rs`
- **Book:** I (as a rename, not a new aggregate)

#### Security

- **Status:** Full. ARCH-0027 (Book IX of ARCH-0017) -- completed 2026-04-12.
- **Owns:** pond enrollment state (enrolled, cornerstone, pond name), HTTPS listener flag, ceremony infrastructure (host, registry, journal)
- **Commands (write):** `mark_enrolled`, `mark_unenrolled`, `set_pond_name`, `seed_state` (boot-time), `refresh_active`, `try_set_https_started`, `set_https_started`, `clear_https_started`, `recover_ceremonies`
- **Queries (read):** `enrolled`, `pond_active`, `cornerstone`, `pond_name`, `https_started`, `stone_client`, `ceremony_host`, `ceremony_registry`, `ceremony_journal`, `active_arc`, `changes`
- **Emits:** `SecurityChanged` with 3 kinds (`Enrolled`, `Unenrolled`, `PondRenamed`) via `changes()` broadcast. Dual stream: `PondEvent::EnrollmentChanged` on `EventBus` preserved for the pond enrollment listener — intentional design, not deferred debt (ARCH-0034).
- **Subscribes:** none (Security is a root; consumers subscribe to `SecurityChanged`)
- **Ports:** `PondClient` -> `StoneClient` (inter-stone HTTP), `CeremonyPersistence` -> `CeremonyJournal` (crash recovery persistence). Both relocated from `domain/traits/` into the context.
- **Cross-cutting:** `Arc<Metrics>` injected at construction; per-kind event counters via `register_domain`.
- **Mutations:** infallible -- commands return `bool` (changed-or-not), matching Book I Metrics and Book IV Jobs.
- **Source:** `src/moss/src/domain/security/` (mod.rs + aggregate.rs + event.rs + ceremony_persistence.rs + pond_client.rs + pond_lifecycle.rs + tests.rs)
- **Note:** `domain/ceremony/` (nourishment ceremonies) is a separate bounded context, NOT part of Security.
- **Book:** IX (ARCH-0027) -- **COMPLETE**

### Partial contexts (exist but do not enforce the pattern)

These contexts were extracted by earlier refactors (ARCH-0004 and after) but retain imperative methods, scattered state, and direct infra imports.

#### Current (stone identity and local metrics)

- **Status:** Partial
- **Owns:** stone identity (`id`, `name`), address, health, MAC, API port, per-system metrics, local storage volumes, topology cache
- **Emits:** none
- **Subscribes:** none
- **Ports:** none (direct infra access)
- **Source:** `src/moss/src/domain/current/`
- **Book:** cleaned up implicitly across Books III (Topology), VI (Subsystems), VIII (Storage), XIX (Moss rename)

#### Storage (Bank domain model + API surface landed, VIII-a/b)

- **Status:** Partial — Bank view aggregate added (ARCH-0025), API surface unified (ARCH-0026), coordination primitives absorbed (ARCH-0029). Volume state machine clean, but `Current::Storage` still holds raw `Volumes` and `Media` maps. Ports relocated into context (`StoragePlatform`, `ManagementStoreOps`, `BankContentOps`).
- **Owns:** physical volumes (state machine), bank view aggregate (queries + commands + data-plane), media, storage event channel
- **Commands (Bank):** `rename`, `set_roles`, `set_visibility`, `pin`, `unpin`, `release`, `read`, `write`, `delete` (data-plane)
- **Queries (Bank):** `local_banks`, `by_name`, `primary_volume`, `volumes_for_bank`, `bank_infos`
- **Emits:** `StorageChanged` via `Current::Storage::changed` (direct `broadcast::Sender` field)
- **Subscribes:** storage event handlers in various tasks
- **Ports:** `StoragePlatform` → `OsPlatform`, `ManagementStoreOps` → `ContentStore`, `BankContentOps` → `ContentStore` (relocated from `domain/traits/` to `domain/storage/ports.rs`)
- **VolumeIngestor:** renamed from `StorageBank` (ARCH-0025) — routes OS monitor events into Volume state machines
- **BankError:** typed error enum (NotFound, InvalidName, PinFailed, UnpinFailed, IoFailed)
- **HTTP surface (ARCH-0026):** `/api/v1/stone/banks` (list, get), `/api/v1/stone/banks/{moniker}/pin`, `/unpin`; `/api/v1/garden/banks` (list, get, volumes). Legacy `/storage/banks/` pin/unpin redirected 301 to new paths.
- **Source:** `src/moss/src/domain/storage/` (bank_aggregate.rs, bank.rs, volume.rs, collection.rs, routing.rs, ports.rs, etc.), `src/moss/src/api/v1/banks.rs`
- **Book:** VIII-a (domain model) — **COMPLETE**; VIII-b (API surface) — **COMPLETE**

#### Discovery

- **Status:** Full. ARCH-0028 (Book X of ARCH-0017) — completed 2026-04-12.
- **Owns:** Koi embedded handle (mDNS, DNS, certmesh, vault sub-handles), mDNS service registration handle, lurk-listener broadcast source
- **Commands (write):** `reregister` (mDNS `_moss._tcp` + `_http._tcp`), `update_health` (mDNS TXT record), `register_certmesh` (`_certmesh._tcp` CA service)
- **Queries (read):** `koi()` (shared Koi handle), `mdns_registered()`, `has_mdns()`, `lurk_stream()`, `changes()`
- **Emits:** `DiscoveryChanged` with 3 kinds (`Registered`, `HealthUpdated`, `CertmeshRegistered`)
- **Subscribes:** none (Discovery is called imperatively by IP-change handler, health listener, and pond lifecycle)
- **Ports:** none — `MdnsHandle` wraps Koi mDNS directly; a `MdnsTransport` trait is not warranted (single implementation)
- **Cross-cutting:** `Arc<Metrics>` injected at construction; per-kind event counters via `register_domain`
- **Mutations:** infallible — commands return `()` or `bool`; mDNS errors are logged and swallowed (non-fatal)
- **Plan change:** ARCH-0017 planned 3 contexts (Discovery, Announcement, Networking); Book X landed 1 aggregate. Announcement (pure functions) and Networking (infrastructure feeding Subsystems) are correctly placed and need no aggregate.
- **Source:** `src/moss/src/domain/discovery/` (mod.rs + aggregate.rs + event.rs + mdns.rs + tests.rs)
- **Book:** X (ARCH-0028) — **COMPLETE**

#### Presence

- **Status:** Partial — holds `elections` service and `notifications` registry
- **Owns:** election service, cross-stone awareness tag registry
- **Emits:** notifications (not a typed context event)
- **Subscribes:** discovery events through ad-hoc wiring
- **Ports:** none
- **Source:** `src/moss/src/domain/presence/`
- **Book:** implicitly cleaned up across X and XVI

#### Companion

- **Status:** Partial — holds companion registry as a direct field
- **Owns:** registered companions, their ports, their command manifests
- **Emits:** companion events via ad-hoc mechanism
- **Subscribes:** none
- **Ports:** `CompanionSocket`, `CompanionManifest` exist as concrete types
- **Source:** `src/moss/src/domain/companion/`
- **Book:** not explicitly owned by a book; audited in XVII if API surface is affected

#### Platform

- **Status:** Partial — a facade over `container`, `runtime`, `network`, `handlers` infrastructure references
- **Owns:** nothing it enforces; it is a facade over four infra concerns
- **Emits:** none
- **Subscribes:** none
- **Ports:** none (container runtime is sealed behind `docker::ContainerRuntime`; Bollard types do not leak)
- **Source:** `src/moss/src/domain/platform/`
- **Book:** Book XII renamed `platform.docker` → `platform.container`, sealed Bollard leak, deleted dead `ServiceRuntime` trait + `infra/container.rs`. Networking stays as infrastructure on Platform (Book X determined no aggregate needed). Platform is reduced to a vestigial type.

#### Orchestration

- **Status:** Retired. ARCH-0029 (Book XI of ARCH-0017) — dissolved 2026-04-12.
- **Plan change:** ARCH-0017 anticipated 3 sub-aggregates (Tick, Nurturing, Election); Book XI found 0 aggregates warranted — Orchestration was a 110-line coordination bag with no domain state, no invariants, no business logic.
- **Dissolved into:**
  - Storage coordination (tick, nudge, rescan, S3 listeners) → `current.storage.coordination` sub-struct
  - Nurturing infrastructure (harvest_ops, store) → direct `Moss.nurturing` field
  - Nourishment SSE channels (jobs) → direct `Moss.nourishment` field
  - Election was never in Orchestration (lives in `Presence.elections`)
- **Thin namespace retained:** `domain/orchestration/` still holds `NurturingOrchestration` and `NourishmentOrchestration` as thin infrastructure structs — no aggregate, no events, no ports.
- **Book:** XI (ARCH-0029) — **COMPLETE**

#### Jobs

- **Status:** Full — DDD aggregate with typed commands, typed queries, dual event streams, Metrics injection, and a periodic `JobsReaperTask`. ARCH-0021 (Book IV of ARCH-0017) — completed 2026-04-11.
- **Owns:** active + recently-terminal jobs keyed by job id
- **Commands (write):** `submit`, `start`, `record_item_completed`, `record_item_failed`, `complete`, `fail`, `maintain` (+ `maintain_with` for tests)
- **Queries (read):** `get`, `snapshot`, `list_active`, `active_count`, `find_active_by_prefix`
- **Emits:** `JobsChanged` domain event via `changes()`; parallel wire-format `JobEvent` via `EventBus::emit()` — dual streams (the wire format is an existing SSE-consumer contract — rake, dashboards — that cannot be collapsed)
- **Subscribes:** none (consumers query; no upstream context feeds Jobs)
- **Ports:** none — ephemeral aggregate, state is rebuilt empty on every process start
- **Reaper:** `JobsReaperTask` runs every 10 minutes and calls `Jobs::maintain()`, which evicts terminal jobs (`Completed` / `Failed`) whose `completed_at` is older than 24 hours. Active jobs (`Pending` / `Running`) are never evicted — a stuck job is a bug worth surfacing, not a memory leak to hide.
- **Metrics:** domain `jobs` registered with seven kinds (`submitted`, `started`, `item_completed`, `item_failed`, `completed`, `failed`, `evicted`) using the register-with-kinds pattern for a lock-free hot path
- **Mutations:** infallible — commands return `()` (or a value) and no `JobsError` type exists; missing-id calls are warn-level no-ops, matching Book I `Metrics`
- **Source:** `src/moss/src/domain/jobs/` (aggregate, state, entry, event, maintenance, tests — one concept per file per code-standards §14)
- **Book:** IV — ARCH-0021 closed 2026-04-11

#### Catalog

- **Status:** Full. ARCH-0022 (Book V of ARCH-0017) — completed 2026-04-12.
- **Owns:** frozen manifest registry (`Arc<ManifestRegistry>`, immutable after bootstrap), compiled offerings index (`RwLock<CatalogState>` wrapping `Option<OfferingsIndex>`)
- **Commands (write):** `load` (idempotent, cache-first startup path), `rebuild` (force-recompile after capabilities change)
- **Queries (read):** `get_manifest`, `find_hw_manifest`, `manifest_count`, `get_compiled`, `compiled_snapshot`, `stats`, `is_loaded`, `manifests` (raw registry access)
- **Emits:** `CatalogChanged` with 2 kinds (`Loaded`, `Rebuilt`) — minimal, since the catalog is mostly inert after startup
- **Subscribes:** none (Catalog is a root: it reads manifests and capabilities, emits events for downstream consumers)
- **Ports:** `CatalogCache` -> `FileCatalogCache` (persistent — third persistent aggregate after Offerings and Topology)
- **Cross-cutting:** `Arc<Metrics>` injected at construction; mutation latency + per-kind event counters recorded
- **Typed errors:** First aggregate with `CatalogError` enum (ManifestHashFailed, CompilationFailed, CacheReadFailed, CacheWriteFailed) — commands return `Result<(), CatalogError>` instead of `anyhow::Result`
- **Source:** `src/moss/src/domain/catalog/` (mod.rs + aggregate.rs + state.rs + entry.rs + index.rs + fingerprint.rs + event.rs + error.rs + cache.rs + tests.rs)
- **Book:** V (ARCH-0022) — **COMPLETE**

#### Tool

- **Status:** Full — DDD aggregate with typed commands, typed queries, dual event streams, Metrics injection, and a `ToolsBeaconTransport` port. ARCH-0019 (Book II of ARCH-0017) — completed 2026-04-11.
- **Owns:** garden-wide tool registry (local projections + gateway registrations + remote-announced tools), cursor sequence, delta history
- **Commands (write):** `upsert`, `register_gateway`, `deregister_gateway`, `reap_expired_gateways`, `reconcile_local`, `apply_remote_beacon`, `remove_stone`
- **Queries (read):** `snapshot`, `deltas_since`, `get`, `current_cursor`, `cursor_for_event_id`, `storage_entries`, `storage_by_name`, `storage_primary`, `storage_by_id`, `storage_grouped_by_stone`, `storage_count`, `storage_stone_count`, `stone_endpoint`, `find_s3_gateways`, `route_to_primary`, `handles_offering`, `handled_offerings`, `local_snapshot_for_beacon`
- **Emits:** `ToolChanged` domain event via `changes()`, wire `ToolDelta` via `delta_stream()` — dual streams documented as a deviation (the wire format is an existing consumer-facing contract that cannot be collapsed)
- **Subscribes:** `OfferingsChanged` (via `offerings-projection` task) for local reconciliation
- **Ports:** `ToolsBeaconTransport` — UDP beacon publishing. Production adapter: `infra::tools::P2pBeaconTransport`. Test adapter: `NoopBeaconTransport`.
- **Metrics:** domain `tool` registered with five kinds (upserted, removed, reaped, beacon-applied, stone-removed) using the register-with-kinds pattern for a lock-free hot path
- **Persistence:** none — ephemeral aggregate (rebuilt on startup from offerings + storage + remote beacons + TTL)
- **Singular endpoint:** `GET /api/v1/stone/tools/{fqid}` added in Ch6
- **Source:** `src/moss/src/domain/tool/` (aggregate, registry, event, error, transport, projection, capability, sse, tests — one concept per file per code-standards §14)
- **Book:** II — ARCH-0019 closed 2026-04-11

### Formerly absent contexts (now extracted)

These contexts were originally scattered across `Moss` (formerly `AppState`) or free-function modules. All have been extracted into proper aggregates.

#### Subsystems

- **Status:** Full. ARCH-0023 (Book VI of ARCH-0017) — completed 2026-04-12.
- **Owns:** per-subsystem readiness state (network, docker) via `tokio::sync::watch` channels
- **Commands (write):** `register` (bootstrap-time, panics on duplicate), `mark_ready`, `mark_unready`
- **Queries (read):** `is_ready` (synchronous poll, zero-cost), `wait_ready` (async), `snapshot`
- **Emits:** `SubsystemsChanged` with 2 kinds (`Ready`, `Unready`) — interesting transitions only
- **Subscribes:** none (Subsystems is a pure readiness authority; producer tasks push into it)
- **Ports:** none — ephemeral aggregate, readiness is runtime-only
- **Cross-cutting:** `Arc<Metrics>` injected at construction; readiness transitions recorded as domain events
- **Mutations:** infallible — commands return `()` and no `SubsystemsError` type exists; unknown-name calls are warn-level no-ops, matching Book I `Metrics` and Book IV `Jobs`
- **No internal `RwLock`:** `watch::Sender` is inherently thread-safe; the `HashMap` is frozen after registration. Simplification over the standard `RwLock<State>` pattern.
- **Source:** `src/moss/src/domain/subsystems/` (mod.rs + aggregate.rs + event.rs + tests.rs)
- **Book:** VI (ARCH-0023) — **COMPLETE**

#### Health

- **Status:** Full. ARCH-0024 (Book VII of ARCH-0017) — completed 2026-04-12.
- **Owns:** probe scheduling/execution (via `HealthProbe` port), transition detection, event emission, notification projection. Does NOT own per-offering health state (stays on `Offering.health` field).
- **Commands (write):** `probe_offering` (probe via port, compare, mutate offering, emit event), `apply_docker_event` (apply real-time Docker event status/health), `update_notification` (set/clear degraded-offerings notification tag)
- **Queries (read):** `changes()` — subscribe to `HealthChanged` events
- **Emits:** `HealthChanged` with 3 transition kinds (`Recovered`, `Degraded`, `Failed`) — fires only on interesting transitions, not on every probe cycle
- **Subscribes:** none (Health is called imperatively by the health monitor task and docker events task)
- **Ports:** `HealthProbe` -> `DockerHealthProbe` (wraps Docker container inspection via Bollard)
- **Cross-cutting:** `Arc<Metrics>` injected at construction; mutation latency + per-kind event counters recorded using the register-with-kinds pattern
- **Mutations:** infallible — commands return `ProbeOutcome` or `bool`, no `HealthError` type (ephemeral aggregate pattern deviation, matching Book I Metrics)
- **Source:** `src/moss/src/domain/health/` (mod.rs + aggregate.rs + event.rs + probe.rs + system.rs + tests.rs)
- **Book:** VII (ARCH-0024) — **COMPLETE**

#### Announcement

- **Status:** Not warranted — Book X (ARCH-0028) determined no aggregate needed. `domain/announcement.rs` contains pure decision functions (no state). Periodic announcer task is a timer using Topology's `chirp()` command. Chirp transport is already owned by Topology (Book III).
- **Source:** `src/moss/src/domain/announcement.rs` (pure functions), `src/moss/src/tasks/announcer.rs` (timer task)
- **Book:** X — evaluated and rejected as aggregate

#### Networking

- **Status:** Not warranted — Book X (ARCH-0028) determined no aggregate needed. `Network` monitor (`tasks/network_monitor.rs`) is infrastructure that feeds Subsystems readiness. `domain/network.rs` contains value objects for static IP management. No domain state to encapsulate.
- **Source:** `src/moss/src/tasks/network_monitor.rs` (infrastructure), `src/moss/src/domain/network.rs` (value objects)
- **Book:** X — evaluated and rejected as aggregate

#### ContainerRuntime

- **Status:** Sealed infrastructure. ARCH-0030 (Book XII of ARCH-0017) — completed 2026-04-12.
- **Plan change:** ARCH-0017 anticipated a full `ContainerRuntime` port trait + `BollardAdapter`; Book XII found that `docker::Client` already IS the anti-corruption layer (all Bollard types confined to method bodies, domain-type returns). Revised to rename + seal + delete: `docker::Client` → `docker::ContainerRuntime`, `Platform.docker` → `Platform.container`, domain-level `ContainerEvent` replaces the one Bollard type that leaked (`EventMessage`), dead `ServiceRuntime` trait + `infra/container.rs` deleted.
- **Source:** `src/moss/src/docker/` (sealed module — zero Bollard types cross its boundary)
- **Book:** XII

#### Configuration

- **Status:** Dissolved. ARCH-0031 (Book XIII of ARCH-0017) — completed 2026-04-12.
- **Plan change:** ARCH-0017 anticipated a `Configuration` aggregate with `ConfigChanged` events and `ConfigSource` port; Book XIII found that config is loaded once at boot and frozen — no mutable state, no invariants, no events. `MossConfig` (infra value object) + `DaemonConfig` (bootstrap merge facade) are the correct architecture. `EnvConfig` stays in `garden-common` (cross-crate). 6 dead timeout fields/accessors deleted.
- **Book:** XIII

#### Persistence

- **Status:** Dissolved. ARCH-0032 (Book XIV of ARCH-0017) — completed 2026-04-12.
- **Plan change:** ARCH-0017 anticipated `AtomicJsonStore<T>`, `DirectoryCache<K, V>`, and canonical error conversion as shared infra helpers; Book XIV found that `garden_common::persistence` already provides `atomic_write_file()`, `JsonStorage<T>`, and `PersistenceProvider<T>` trait — the consolidation target already exists. Store port adapters are 10-20 line thin delegates. `DirectoryCache<K, V>` has exactly one consumer (`CeremonyJournal`). Remaining inline atomic write duplicates in moss infra are mechanical cleanup, not domain architecture.
- **Book:** XIV

#### Logging

- **Status:** Dissolved. ARCH-0033 (Book XV of ARCH-0017) — completed 2026-04-12.
- **Plan change:** ARCH-0017 anticipated a `Logging` aggregate with `LogLineEmitted` event and `LogSink` port; Book XV found that logging is pure infrastructure — a single `broadcast::Sender<String>` with 1 consumer (SSE stream), `LogBroadcastLayer` correctly in `infra/`, file sink managed by `tracing-appender`. No domain state, no invariants, no events beyond raw tracing output.
- **Book:** XV

#### Events (unified)

- **Status:** Dissolved. ARCH-0034 (Book XVI of ARCH-0017) — completed 2026-04-12.
- **Plan change:** ARCH-0017 anticipated a unified `Events` aggregate with `PulseProjectionTask` subscribing to all aggregate `changes()` channels. Book XVI found that EventBus (user-facing domain events with 3 listeners: chirp, pulse bridge, timer), Pulse (SSE firehose merging domain + transport), and per-aggregate `changes()` (internal state-transition notifications) serve different event populations with different consumers. No domain state, no invariants. Existing architecture is correct.
- **Book:** XVI

#### HttpApi

- **Status:** Dissolved. ARCH-0035 (Book XVII of ARCH-0017) — completed 2026-04-12.
- **Plan change:** ARCH-0017 anticipated handlers containing business logic that needed extraction into a thin dispatcher layer with separate DTOs. After 16 books of aggregate extraction, handlers already dispatch to typed commands/queries (85 aggregate method calls across 27 files). `FromRef` migration (161 handlers) is cosmetic — handlers access their correct aggregates regardless of extraction style. DTOs inline with handlers is higher-cohesion than a separate `api/dto/` directory. Remaining `offerings.read()` sites are Book XVIII scope.
- **Book:** XVII

---

## Target state

After [ARCH-0017](../decisions/ARCH-0017-ddd-monolith-epic.md) completes. Every context below is Full.

### Domain contexts (hold state, emit events)

| Context | Owns | Emits | Ports | Book |
|---------|------|-------|-------|------|
| **Offerings** ✅ | active + adopted-candidate pools | `OfferingsChanged` | `OfferingStore` | ARCH-0016 + Book I (Metrics injection); Book XVIII (strangler removal) |
| **Metrics** ✅ | per-domain counters, per-task metrics (timing, event counts, lag), global metrics, 9-bucket latency histogram | `MetricsChanged` (interesting transitions only — not counter increments) | none (in-memory only, counters reset on restart) | I (ARCH-0018) — **COMPLETE** |
| **Resources** ✅ *(renamed in Book I Chapter 2)* | hardware resource snapshot facade over `Current::Resources::system/network/gpu` | none | none | I (as rename only) |
| **Tool** ✅ | garden-wide tool registry (Local + Gateway + Announced origins), typed commands, dual event streams | `ToolChanged` (internal), `ToolDelta` (wire format, existing contract) | `ToolsBeaconTransport` | II (ARCH-0019) — **COMPLETE** |
| **Topology** ✅ | peer cache (discovered + offline stones), persistence dirty flag, self-entry assembly | `TopologyChanged` (6 kinds: Discovered/Online/Offline/Forgotten/Evicted/Chirped) | `ChirpTransport`, `TopologyStore` | III (ARCH-0020) — **COMPLETE** |
| **Jobs** ✅ | active + recently-terminal jobs (HashMap keyed by id) | `JobsChanged` (7 kinds: Submitted/Started/ItemCompleted/ItemFailed/Completed/Failed/Evicted) + wire `JobEvent` via `EventBus` | none (ephemeral; `JobsReaperTask` sweeps terminal jobs past 24h TTL) | IV (ARCH-0021) — **COMPLETE** |
| **Catalog** ✅ | frozen manifest registry, compiled offerings index | `CatalogChanged` (2 kinds: Loaded, Rebuilt) | `CatalogCache` | V (ARCH-0022) — **COMPLETE** |
| **Subsystems** ✅ | per-subsystem readiness state (watch channels) | `SubsystemsChanged` (2 kinds: Ready, Unready) | none (ephemeral) | VI (ARCH-0023) — **COMPLETE** |
| **Health** ✅ | probe execution, transition detection, notification projection (no per-offering state — delegates to Offerings) | `HealthChanged` | `HealthProbe` | VII (ARCH-0024) — **COMPLETE** |
| **Storage::Volumes** | physical volume state | `VolumeChanged` | `VolumeMonitor`, `FileSystem` | VIII |
| **Storage::Banks** | seed-bank lifecycle | `BankChanged` | `FileSystem` | VIII |
| **Storage::Replication** | replication state machine | `ReplicationChanged` | `ReplicationTransport` | VIII |
| **Security::Pond** | pond membership, CA, enrollment | `PondChanged` | `PondCertStore`, `MtlsAcceptor` | IX |
| **Security::Ceremonies** | ceremony registry, journal, lifecycle | `CeremonyChanged` | `CeremonyJournal` | IX |
| **Security::Trust** | per-stone trust, mTLS material | `TrustChanged` | `MtlsAcceptor` | IX |
| **Discovery** ✅ | Koi handle, mDNS registration, lurk-listener | `DiscoveryChanged` (3 kinds) | none | X (ARCH-0028) — **COMPLETE** |
| **Orchestration::Tick** | storage tick aggregation | `OrchestrationTick` | none | XI |
| **Orchestration::Nurturing** | nurturing lifecycle state | `NurturingChanged` | `NurturingStore` | XI |
| **Orchestration::Election** | offering primary/dormant election | `ElectionResolved` | `ElectionTransport` | XI |
| ~~**Configuration**~~ | ~~typed env + runtime settings~~ | ~~`ConfigChanged`~~ | ~~`ConfigSource`~~ | XIII (dissolved) |
| ~~**Logging**~~ | ~~log broadcast channel, file sink handle~~ | ~~`LogLineEmitted`~~ | ~~`LogSink`~~ | XV (dissolved) |
| ~~**Events**~~ | ~~unified cross-cutting event surface~~ | ~~(bridges all domain events to pulse)~~ | ~~none~~ | XVI (dissolved) |

### Infrastructure contexts (ports + adapters, no state)

| Context | Exposes | Adapter | Book |
|---------|---------|---------|------|
| **ContainerRuntime** | `docker::ContainerRuntime` (sealed concrete) | Bollard internalized | XII |
| ~~**Persistence helpers**~~ | ~~`AtomicJsonStore<T>`, `DirectoryCache<K, V>`, canonical error conversion~~ | ~~(no trait; reusable helpers)~~ | XIV (dissolved) |

### Application contexts

| Context | Responsibility | Book |
|---------|----------------|------|
| ~~**HttpApi**~~ | ~~Axum router, handlers as thin command/query dispatchers, DTOs in `api/dto/` separated from domain types~~ | XVII (dissolved) |
| **Bootstrap** | startup sequence, dependency injection wiring | touched by every book |
| **Shutdown** | cascading shutdown lifecycle, final flush hooks | XIX |

### Retired contexts

| Context | Status | Absorbed into |
|---------|--------|---------------|
| **Platform** | Retired by Book XII | `ContainerRuntime` (Book XII) + `Networking` (Book X) + remaining bits moved to their owning contexts |
| **AppState** | Retired by Book XIX (ARCH-0037) | Renamed to **Moss** — pure dependency container with one cross-cutting method (`emit_storage_changed`). 7 delegate methods inlined, re-exports relocated to `lib.rs`. |

---

## Event flow graph

The target-state event subscription topology. Producer `→` Consumer.

```
Offerings ────────────┬─→ Tool (via ToolProjectionTask)
                      ├─→ Topology (via TopologyProjectionTask)
                      └─→ Health (via HealthProjectionTask)

Storage::Volumes ─────┬─→ Storage::Banks
                      └─→ Storage::Replication

Storage::Banks ──────────→ Tool (seed-bank projection)

Catalog ─────────────────→ Offerings (compatibility checks)

Networking ──────────────→ Subsystems (network readiness)
ContainerRuntime ────────→ Subsystems (docker readiness) [via adapter events]
Pond ────────────────────→ Subsystems (pond readiness)

Tool ────────────────────→ Topology (tool deltas reflected in self-entry)
Topology ────────────────→ Announcement (topology changes trigger chirps)

Subsystems ──────────────→ Announcement (only chirp once network ready)
                          └─→ Discovery (only lurk once network ready)

Discovery ───────────────→ Pond (enrollment flow)
                          └─→ Topology (known peers for aggregation)

Health ──────────────────→ Offerings (mark degraded on probe failure)
                          └─→ Metrics (probe latency, failure counts)

[every context] ─────────→ Metrics (via record_domain_event)
[every context] ─────────→ EventBus (cross-cutting infra; PulseDomainBridge → Pulse SSE firehose)
```

The graph is **acyclic** by construction — if Book I proposes a subscription that would introduce a cycle, the cycle is broken by moving the coordination to a third context (typically Metrics or a cross-cutting EventBus listener).

---

## Port inventory

Complete list of infrastructure ports the epic produces. Each port lives in its owning context's `port.rs` file; each adapter lives in `src/moss/src/infra/`.

| Port | Purpose | Primary adapter | Owning context |
|------|---------|-----------------|----------------|
| `OfferingStore` | load/save `Vec<Offering>` | `FileOfferingStore` | Offerings |
| `ToolsBeaconTransport` | broadcast tool deltas over UDP | `UdpBeaconAdapter` | Tool |
| `ChirpTransport` | send topology chirps over UDP | `UdpChirpAdapter` | Topology, Announcement |
| `MdnsTransport` | register mDNS services | `KoiMdnsAdapter` | Topology, Discovery |
| `KoiClient` | embedded koi discovery | `KoiEmbeddedAdapter` | Discovery |
| `InterfaceMonitor` | observe network interface state | `NetlinkAdapter` (Linux) / `IpHelperAdapter` (Windows) | Networking |
| `ContainerRuntime` | container lifecycle operations | Bollard internalized (sealed in `docker::`) | Platform |
| `FileSystem` | filesystem operations abstracted for tests | `OsFileSystem` | Storage |
| `VolumeMonitor` | observe physical volume events | `UdevAdapter` / `WmiAdapter` | Storage::Volumes |
| `ReplicationTransport` | peer-to-peer replication protocol | `HttpReplicationAdapter` | Storage::Replication |
| `ManifestSource` | load manifest files | `EmbeddedManifestSource` + `FileSystemManifestSource` | Catalog |
| `CatalogCache` | persist compiled catalog | `FileCatalogCache` | Catalog |
| `HealthProbe` | HTTP/TCP probe execution | `HttpProbeAdapter`, `TcpProbeAdapter` | Health |
| `CeremonyJournal` | append-only ceremony log | `FileCeremonyJournal` | Security::Ceremonies |
| `PondCertStore` | CA cert + key material | `FilePondCertStore` | Security::Pond |
| `MtlsAcceptor` | inbound TLS acceptor | `RustlsMtlsAcceptor` | Security::Trust |
| `ElectionTransport` | election message transport | `UdpElectionAdapter` | Orchestration::Election |
| `NurturingStore` | persist nurturing state | `FileNurturingStore` | Orchestration::Nurturing |
| ~~`ConfigSource`~~ | ~~load typed configuration~~ | ~~`EnvConfigSource` + `FileConfigSource`~~ | ~~Configuration~~ (dissolved — Book XIII) |
| ~~`LogSink`~~ | ~~write log lines~~ | ~~`FileLogSink`, `StderrLogSink`, `MemoryLogSink`~~ | ~~Logging~~ (dissolved — Book XV) |
| `CompanionSocket` | command forwarding to companion processes | `HttpCompanionSocket` | Companion |
| `CompanionManifest` | load companion manifests | `FileCompanionManifest` | Companion |

Total: ~22 ports, each with at least one concrete adapter and an in-memory fake for tests.

---

## Maintenance

This document is updated by each book's Chapter 6. When a book completes:

1. **Promote its contexts** from "Current state / Partial" or "Current state / Absent" into "Current state / Full".
2. **Add or refine entries** in the Target state tables if the book surfaces details the epic planning did not anticipate.
3. **Update the event flow graph** if the book's projections add new subscriptions.
4. **Update the port inventory** if the book introduces new ports.
5. **Commit the update** as part of the book's final commit.

A book that does not update this map is incomplete per the epic's shippability rule.

---

## References

- [ARCH-0017](../decisions/ARCH-0017-ddd-monolith-epic.md) — the epic this map serves
- [domain-aggregates.md](../specs/domain-aggregates.md) — the pattern every Full context follows
- [glossary.md](../glossary.md) — terms used in this map
- [scaffolding.md](../scaffolding.md) — temporary code tracked during the epic
