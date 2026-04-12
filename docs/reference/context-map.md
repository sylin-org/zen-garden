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
  - **Absent** — does not exist yet; scattered across other contexts or as raw AppState fields
  - **Retired** — no longer exists; its responsibilities absorbed by another context
- **Owns** — what state the context holds
- **Emits** — what domain events it publishes via `changes()`
- **Subscribes** — what other contexts' events it reacts to (usually through a projection task)
- **Ports** — what infrastructure traits it depends on
- **Source** — where in the repo the context lives
- **Book** — which book in the epic owns this context's extraction / cleanup

---

## Current state

As of 2026-04-12, after [ARCH-0022](../decisions/ARCH-0022-catalog-aggregate.md).

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
- **Composition layer:** `domain::topology::composition::*` holds the AppState-bound helpers (`self_entry_inputs`, `build_self_entry`, `sync_services`, `sync_capabilities`, `update_stone_health`, `announce_resolution_change`) so the aggregate holds no back-reference to `AppState`.
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

### Partial contexts (exist but do not enforce the pattern)

These contexts were extracted by earlier refactors (ARCH-0004 and after) but retain imperative methods, scattered state, and direct infra imports.

#### Current (stone identity and local metrics)

- **Status:** Partial
- **Owns:** stone identity (`id`, `name`), address, health, MAC, API port, per-system metrics, local storage volumes, topology cache
- **Emits:** none
- **Subscribes:** none
- **Ports:** none (direct infra access)
- **Source:** `src/moss/src/domain/current/`
- **Book:** cleaned up implicitly across Books III (Topology), VI (Subsystems), VIII (Storage), XIX (AppState dissolution)

#### Security / Pond / Ceremony (scattered)

- **Status:** Partial — Pond and Ceremony are separate structs under `security` with tangled ownership
- **Owns:** pond state, ceremony registry, ceremony host, ceremony journal, TLS key material
- **Emits:** ad-hoc via `event_bus`, not a typed context event
- **Subscribes:** ad-hoc
- **Ports:** `CeremonyJournal` exists as a concrete type, not an injected port
- **Source:** `src/moss/src/domain/security/`, `src/moss/src/domain/ceremony/`, `src/moss/src/domain/pond/`
- **Book:** IX (consolidation)

#### Storage

- **Status:** Partial — exists as `Current::Storage` with `volumes`, `media`, and a `changed` broadcast but no enforced aggregate
- **Owns:** physical volumes, volume state, media, bank lifecycle (scattered)
- **Emits:** `StorageChanged` via `Current::Storage::changed` (direct `broadcast::Sender` field)
- **Subscribes:** storage event handlers in various tasks
- **Ports:** none (direct `crate::infra::storage::*` imports)
- **Source:** `src/moss/src/domain/storage/`, `src/moss/src/current/storage.rs`
- **Book:** VIII (deep clean into sub-aggregates: Volumes, Banks, Replication)

#### Discovery

- **Status:** Partial — holds `mdns` handle and `koi` embedded handle as direct fields
- **Owns:** mDNS re-registration handle, koi discovery state
- **Emits:** none (mDNS events flow through koi callbacks)
- **Subscribes:** adapter callbacks
- **Ports:** none
- **Source:** `src/moss/src/domain/discovery/`
- **Book:** X (with Announcement and Networking)

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

- **Status:** Partial — a bag of `docker`, `runtime`, `network`, `handlers` infrastructure references
- **Owns:** nothing it enforces; it is a facade over four infra concerns
- **Emits:** none
- **Subscribes:** none
- **Ports:** direct Bollard imports, no abstraction
- **Source:** `src/moss/src/domain/platform/`
- **Book:** Book XII extracts `ContainerRuntime` from `platform.docker`; Book X extracts `Networking` from `platform.network`; Platform is retired or reduced to a vestigial type

#### Orchestration

- **Status:** Partial — holds tick stream, nudge, nurturing store, rescan as direct fields
- **Owns:** storage tick broadcast, nurturing store, nudge notifier, offering election state
- **Emits:** ticks via direct `broadcast::Sender`
- **Subscribes:** ad-hoc
- **Ports:** none
- **Source:** `src/moss/src/domain/orchestration/`
- **Book:** XI (deep clean into Tick, Nurturing, Election sub-aggregates)

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

### Absent contexts (scattered across AppState or other modules)

These contexts do not exist as modules. Their state lives as raw fields on `AppState` or as free-function modules.

#### Subsystems / Readiness

- **Status:** Absent as an aggregate — `AppState::subsystems: SubSystems` is a struct of `AtomicBool` flags (`network.ready`, `docker.ready`) checked imperatively
- **Target source:** `src/moss/src/domain/subsystems/`
- **Book:** VI

#### Health

- **Status:** Absent — HTTP/TCP probes live in `tasks/health_monitor.rs`, probe logic mixed with task orchestration
- **Target source:** `src/moss/src/domain/health/`
- **Book:** VII

#### Announcement

- **Status:** Absent — chirp scheduling lives in `tasks/periodic_announcer.rs` and `announcement.rs` free functions
- **Target source:** `src/moss/src/domain/announcement/`
- **Book:** X

#### Networking

- **Status:** Absent — network interface monitoring is in `tasks/ip_change_handler.rs` and scattered `infra::network` free functions
- **Target source:** `src/moss/src/domain/networking/`
- **Book:** X

#### ContainerRuntime

- **Status:** Absent as a port — `platform.docker: DockerClient` is used directly; Bollard types bleed into domain code
- **Target source:** `src/moss/src/domain/container_runtime/` (trait) + `src/moss/src/infra/container_runtime/` (adapter)
- **Book:** XII

#### Configuration

- **Status:** Absent — `EnvConfig` is a free-function module in `garden-common`; runtime feature flags are scattered
- **Target source:** `src/moss/src/domain/configuration/`
- **Book:** XIII

#### Persistence

- **Status:** Absent as a shared concept — every aggregate will have its own `Store` port, but the file-backed adapter helpers are not yet unified
- **Target source:** `src/moss/src/infra/persistence/`
- **Book:** XIV

#### Logging

- **Status:** Absent — `AppState::log: broadcast::Sender<String>` is a raw field; tracing layer wiring is in `bootstrap/run.rs`
- **Target source:** `src/moss/src/domain/logging/`
- **Book:** XV

#### Events (unified)

- **Status:** Absent as a unified surface — `EventBus` exists alongside per-domain `changes()` streams and `Pulse` events with `PulseDomainBridge` translating between them; no coherent API
- **Target source:** `src/moss/src/domain/events/` (expanded from the current partial module)
- **Book:** XVI

#### HttpApi

- **Status:** Absent as a named context — handlers exist under `api/v1/` but reach into `state.X.read().await` directly; DTO separation is inconsistent
- **Target source:** `src/moss/src/api/v1/` (handlers become thin dispatchers; DTOs under `api/dto/`)
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
| **Subsystems** | per-subsystem readiness state | `SubsystemReady`, `SubsystemUnready` | none | VI |
| **Health** | per-offering health state, probe schedule | `HealthChanged` | `HealthProbe` | VII |
| **Storage::Volumes** | physical volume state | `VolumeChanged` | `VolumeMonitor`, `FileSystem` | VIII |
| **Storage::Banks** | seed-bank lifecycle | `BankChanged` | `FileSystem` | VIII |
| **Storage::Replication** | replication state machine | `ReplicationChanged` | `ReplicationTransport` | VIII |
| **Security::Pond** | pond membership, CA, enrollment | `PondChanged` | `PondCertStore`, `MtlsAcceptor` | IX |
| **Security::Ceremonies** | ceremony registry, journal, lifecycle | `CeremonyChanged` | `CeremonyJournal` | IX |
| **Security::Trust** | per-stone trust, mTLS material | `TrustChanged` | `MtlsAcceptor` | IX |
| **Discovery** | known peer stones | `StoneDiscovered`, `StoneLost` | `MdnsTransport`, `KoiClient` | X |
| **Announcement** | chirp schedule, last-chirp timestamp | `ChirpEmitted` | `ChirpTransport` | X |
| **Networking** | network interface state | `NetworkStateChanged` | `InterfaceMonitor` | X |
| **Orchestration::Tick** | storage tick aggregation | `OrchestrationTick` | none | XI |
| **Orchestration::Nurturing** | nurturing lifecycle state | `NurturingChanged` | `NurturingStore` | XI |
| **Orchestration::Election** | offering primary/dormant election | `ElectionResolved` | `ElectionTransport` | XI |
| **Configuration** | typed env + runtime settings | `ConfigChanged` | `ConfigSource` | XIII |
| **Logging** | log broadcast channel, file sink handle | `LogLineEmitted` | `LogSink` | XV |
| **Events** | unified cross-cutting event surface | (bridges all domain events to pulse) | none | XVI |

### Infrastructure contexts (ports + adapters, no state)

| Context | Exposes | Adapter | Book |
|---------|---------|---------|------|
| **ContainerRuntime** | `ContainerRuntime` trait | `BollardAdapter` | XII |
| **Persistence helpers** | `AtomicJsonStore<T>`, `DirectoryCache<K, V>`, canonical error conversion | (no trait; reusable helpers) | XIV |

### Application contexts

| Context | Responsibility | Book |
|---------|----------------|------|
| **HttpApi** | Axum router, handlers as thin command/query dispatchers, DTOs in `api/dto/` separated from domain types | XVII |
| **Bootstrap** | startup sequence, dependency injection wiring | touched by every book |
| **Shutdown** | cascading shutdown lifecycle, final flush hooks | XIX |

### Retired contexts

| Context | Status | Absorbed into |
|---------|--------|---------------|
| **Platform** | Retired by Book XII | `ContainerRuntime` (Book XII) + `Networking` (Book X) + remaining bits moved to their owning contexts |
| **AppState** | Retired by Book XIX | Renamed to **Moss** (a pure dependency container with no methods doing work) |

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
[every context] ─────────→ Events (via PulseDomainBridge for SSE firehose)
```

The graph is **acyclic** by construction — if Book I proposes a subscription that would introduce a cycle, the cycle is broken by moving the coordination to a third context (typically Metrics or Events).

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
| `ContainerRuntime` | container lifecycle operations | `BollardAdapter` | ContainerRuntime |
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
| `ConfigSource` | load typed configuration | `EnvConfigSource` + `FileConfigSource` | Configuration |
| `LogSink` | write log lines | `FileLogSink`, `StderrLogSink`, `MemoryLogSink` (for tests) | Logging |
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
