---
audience: [developer, ai]
doc_type: reference
status: canonical
last_verified: 2026-04-11
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

As of 2026-04-11, after [ARCH-0016](../decisions/ARCH-0016-offerings-aggregate-domain.md).

### Full contexts

#### Offerings

- **Status:** Full (ARCH-0016, with two known gaps closed later in the epic: typed errors, `Arc<Metrics>` injection)
- **Owns:** active offerings pool, adopted-candidates pool
- **Emits:** `OfferingsChanged { kind, affected, timestamp }` with 8 `ChangeKind` variants (Upserted, Removed, Updated, Promoted, Demoted, Replaced, Coalesced, BatchUpdated)
- **Subscribes:** none (root of the service pipeline)
- **Ports:** `OfferingStore` → `FileOfferingStore`
- **Source:** `src/moss/src/domain/offerings/`
- **Book:** ARCH-0016 (pre-epic); audited in Book I (typed errors + metrics) and Book XVIII (strangler removal)

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

#### Tool

- **Status:** Partial — holds `registry: GardenRegistry` and `delta: broadcast::Sender<ToolDelta>` as direct fields; no aggregate methods
- **Owns:** garden-wide tool registry, tool delta stream
- **Emits:** `ToolDelta` via direct `broadcast::Sender` (raw, not wrapped in an event type)
- **Subscribes:** Tool projection is refreshed imperatively by AppState methods today
- **Ports:** none (direct `infra::tools::beacon` imports)
- **Source:** `src/moss/src/domain/tool/`
- **Book:** II (primary extraction)

### Absent contexts (scattered across AppState or other modules)

These contexts do not exist as modules. Their state lives as raw fields on `AppState` or as free-function modules.

#### Metrics

- **Status:** Absent — no metrics aggregate; per-task and per-domain observability does not exist. The existing `/api/v1/stone/metrics` endpoint is **misnamed** — it returns hardware resources (CPU, memory, disk, network, uptime), not domain observability. Book I renames the existing surface to `Resources` and introduces a new `Metrics` aggregate for actual observability.
- **Target source:** `src/moss/src/domain/metrics/`
- **Book:** I (per [ARCH-0018](../decisions/ARCH-0018-metrics-aggregate.md))
- **Note on naming collision:** Book I's first code chapter renames `MetricsSnapshot` → `ResourcesSnapshot`, `api/v1/metrics.rs` → `api/v1/resources.rs`, `domain/metrics_collection.rs` → `domain/resources/collection.rs`, `/api/v1/stone/metrics` → `/api/v1/stone/resources`. This frees "metrics" for the new aggregate.

#### Resources (incidentally extracted by Book I)

- **Status:** Partial — exists today scattered across `api/v1/metrics.rs`, `domain/metrics_collection.rs`, `garden_common::MetricsSnapshot`, and the `/api/v1/stone/metrics` path. Book I consolidates and renames.
- **Target source:** `src/moss/src/domain/resources/`
- **Book:** I (as a rename, not a new aggregate — Resources is a simple facade over `Current::Metrics` today, not a candidate for full DDD treatment in this epic)

#### Topology

- **Status:** Absent — responsibilities scattered across `AppState::build_self_entry`, `AppState::sync_self_services`, `AppState::sync_self_capabilities`, `AppState::update_stone_health`, `AppState::announce_resolution_change`
- **Target source:** `src/moss/src/domain/topology/`
- **Book:** III

#### Jobs

- **Status:** Absent — raw `AppState::jobs: Arc<RwLock<HashMap<String, Job>>>`
- **Target source:** `src/moss/src/domain/jobs/`
- **Book:** IV

#### Catalog (Manifests + offerings_index)

- **Status:** Absent as a unified context — `manifest_registry` is a bare `Arc<ManifestRegistry>` on AppState; `offerings_index` is a bare `Arc<RwLock<Option<OfferingsIndex>>>`; catalog-building logic lives in `domain/offerings/catalog.rs` (a file misplaced under Offerings — it is the compile-time catalog, not the runtime aggregate)
- **Target source:** `src/moss/src/domain/catalog/`
- **Book:** V

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
| **Offerings** | active + adopted-candidate pools | `OfferingsChanged` | `OfferingStore` | ARCH-0016 + audited in Books I, XVIII |
| **Metrics** | per-domain counters, per-task metrics (timing, event counts, lag), global metrics | `MetricsChanged` (interesting transitions only — not counter increments) | none (in-memory only, counters reset on restart) | I |
| **Resources** *(incidental rename in Book I)* | hardware resource snapshot facade over `Current::Metrics::system/network/gpu` | none | none | I |
| **Tool** | garden-wide tool registry, delta stream | `ToolChanged`, `ToolBeaconEmitted` | `ToolsBeaconTransport` | II |
| **Topology** | self-entry cache, chirp schedule | `TopologyChanged`, `ChirpEmitted` | `ChirpTransport`, `MdnsTransport` | III |
| **Jobs** | active jobs, job history | `JobsChanged` | (in-memory only) | IV |
| **Catalog** | manifest registry, compiled offerings index | `CatalogChanged` | `ManifestSource`, `CatalogCache` | V |
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
| `JobsStore` | (optional) persist job history | `FileJobsStore` or `NoopJobsStore` | Jobs |
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
