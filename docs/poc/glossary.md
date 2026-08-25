# Glossary

**Purpose**: Single source of truth for all Zen Garden terminology  
**Audience**: All (visitor, operator, developer, contributor, security, AI)

---

## Contents

- [Core Components](#core-components)
- [Services & Templates](#services--templates)
- [Discovery & Networking](#discovery--networking)
- [Security](#security)
- [Operations](#operations)
- [Architectural Terms](#architectural-terms)

---

## Core Components

**Garden** - Logical collection of Stones working together as distributed infrastructure. Example: "My home lab Garden has 3 Stones running MongoDB, Redis, and MinIO."

**Stone** - Physical device running Garden-Moss daemon. Any laptop, desktop, Raspberry Pi, or thin client can be a Stone. Offers services to apps via automatic discovery.  
→ See: [guides/stone-hardware.md](guides/stone-hardware.md)

**Moss** - Daemon service running on each Stone (port 7185). Manages Docker Compose services, announces via mDNS, responds to management commands from Rake.  
→ See: [specs/moss-daemon-lifecycle.md](specs/moss-daemon-lifecycle.md)

**Rake** - CLI tool (`garden-rake`) for discovering Stones and sending management commands. Operators use Rake to install services, check health, and coordinate operations.  
→ See: [specs/rake-commands.md](specs/rake-commands.md)

**Companion** - Service running on a Stone that extends Moss capabilities. Companions communicate with Moss via HTTP command protocol and receive Stone presence events via SSE. Examples: Cricket (audio), Firefly (LEDs), OLED (display).  
→ See: [specs/Companion-COMMAND-PROTOCOL.md](specs/companion-command-protocol.md)

**Cricket** - Audio Companion providing 4-channel mixer (foreground/midground/ambient/background) and tune system for sonifying Stone presence events. Uses 180 CC0-licensed samples for event-to-audio mapping.  
→ See: [decisions/CRICKET-0001-audio-Companion-spec.md](decisions/CRICKET-0001-audio-Companion-spec.md)

**Lantern** - Optional HTTP directory service (port 7184) for cross-subnet discovery and Windows compatibility. Not required for Linux/macOS on same LAN.  
→ See: [decisions/LANTERN-0001-registry.md](decisions/LANTERN-0001-registry.md)

**Cornerstone** - First Stone in a Pond with certificate authority. Issues certificates to other Stones during admission. Only one Cornerstone per Pond.  
→ See: [security/pond-setup.md](security/pond-setup.md)

---

## Services & Templates

**Offering** - Pre-defined service template (YAML file) specifying Docker image, ports, volumes, environment variables, and healthcheck. Examples: `mongodb`, `redis`, `postgresql`.  
→ See: [reference/offerings.md](reference/offerings.md)

**Service** - Running container instance of an offering. A Stone may run multiple services simultaneously (MongoDB + Redis on same Stone).

**Template** - Service configuration blueprint (YAML spec). Operators can create custom templates for services not in the standard catalog.  
→ See: [specs/offerings.md](specs/offerings.md)

**Catalog** - The compiled offering catalog. Combines the frozen `ManifestRegistry` (offering templates) with hardware-resolved compatibility evaluations into a compiled `OfferingsIndex`. Extracted as a DDD aggregate in ARCH-0022 (Book V). See also the [Catalog bounded context](#catalog-aggregate-terms-book-v--arch-0022) section below.

**Native Service** - Database/service running on its native protocol. Examples: MongoDB on port 27017, Redis on 6379, PostgreSQL on 5432. Apps connect using standard drivers.

**Agnostic Sidecar** - HTTP REST API wrapping a native service (port 8080+). Provides protocol-agnostic access for clients that can't use native drivers.  
→ See: [specs/api-v1.md](specs/api-v1.md)

**Set** - Logical namespace for application data (maps to database/schema/prefix). Example: `zen-garden:mongodb/production` connects to MongoDB's `production` database.

---

## Discovery & Networking

**Discovery** - Protocol for finding services and Stones on the local network. Uses mDNS (multicast DNS) for automatic announcement and resolution. Services advertised as `<name>._http._tcp.local`.  
→ See: [decisions/LANTERN-0003-mdns-service-discovery.md](decisions/LANTERN-0003-mdns-service-discovery.md)

**mDNS** - Multicast DNS protocol (proven technology, 20+ years old). Zero-config service discovery on local networks. Used by AirPlay, Chromecast, Spotify Connect, and Zen Garden. Built into macOS (Bonjour) and Linux (Avahi).  
→ See: [decisions/LANTERN-0003-mdns-service-discovery.md](decisions/LANTERN-0003-mdns-service-discovery.md)

**Service Advertisement** - mDNS broadcast announcing service availability. Format: `<name>._http._tcp.local` with TXT records for metadata (garden, stone, version). Example: `MediaX._http._tcp.local` points to `stone-02.local:8080`.

**Friendly Proxy Names** - Optional reverse proxy layer on Cornerstone exposing services as `<name>.zen-garden.local`. Cornerstone discovers services via mDNS, verifies with Moss API, exposes unified naming. Example: `http://MediaX.zen-garden.local` → `stone-02:8080`.  
→ See: [decisions/LANTERN-0003-mdns-service-discovery.md](decisions/LANTERN-0003-mdns-service-discovery.md)

**Announcement** - Broadcast message where a Stone announces "I offer [service]" via mDNS. Includes service type, port, and metadata in TXT records.

**Connection String** - Format for requesting services: `zen-garden:<service-type>[/<database>]`. Example: `zen-garden:mongodb/mydb`. Resolver translates to native connection string.  
→ See: [reference/connection-strings.md](reference/connection-strings.md)

**Registry** - In-memory catalog of discovered services and Stones. Moss maintains a registry updated via UDP broadcasts (TTL 90 seconds).  
→ See: [decisions/MOSS-0001-registry.md](decisions/MOSS-0001-registry.md)

**TXT Record** - mDNS metadata field containing offering name, version, capabilities, and Stone ID. Example: `offering=mongodb version=7.0 stone_id=01936d2e-...`.

**Topology** - Map of all Stones and services in a Garden (network graph). Used for health dashboards and coordinated operations.

---

## Security

**Pond** - Optional security layer for encrypted Stone-to-Stone communication. Uses a CA-based mTLS model with ECDSA P-256 certificates, backed by koi-certmesh. Prevents network sniffing and rogue device admission. Each pond receives a water-themed display name (e.g. `pond-moonlit-basin`) that can be changed at any time.  
→ See: [security/overview.md](security/overview.md), [security/pond-setup.md](security/pond-setup.md)

**Keystone** - Encrypted CA private key stored on the cornerstone. Protected by passphrase encryption (AES-256-GCM via Argon2id KDF). Only the cornerstone (and promoted standby stones) hold the CA private key.  
→ See: [security/pond-setup.md](security/pond-setup.md), [decisions/SECURITY-0003-keystone-protection-tiers.md](decisions/SECURITY-0003-keystone-protection-tiers.md)

**Cornerstone** - Stone that initialized the pond (ran `place keystone`). Holds the CA private key and acts as the certificate authority — the trust anchor for the pond. Can promote other stones to standby CA.  
→ See: [specs/POND-0001-protocol.md](specs/POND-0001-protocol.md)

**TPM (Trusted Platform Module)** - Hardware security chip for cryptographic operations. Zen Garden auto-detects TPM 2.0 and seals Keystone in hardware when available. Provides physical tamper resistance and boot attestation.  
→ See: [decisions/SECURITY-0003-keystone-protection-tiers.md](decisions/SECURITY-0003-keystone-protection-tiers.md)

**Keystone Protection Tiers** - Automatic security capability detection:

- **Hardware-backed**: TPM 2.0 (keys sealed in physical chip)
- **Hypervisor-backed**: vTPM (VM isolation via KVM/VMware/Hyper-V)
- **Software-backed**: Passphrase encryption (AES-256-GCM fallback)  
  → See: [decisions/SECURITY-0003-keystone-protection-tiers.md](decisions/SECURITY-0003-keystone-protection-tiers.md)

**Stone Admission** - Process of joining a Stone to a Pond. Uses TOTP-based Bluetooth-style pairing (6-digit code, 30-second period, configurable enrollment window). The cornerstone generates invitations; any stone with the TOTP URI can produce valid codes.  
→ See: [specs/POND-0001-protocol.md](specs/POND-0001-protocol.md#invitation-protocol)

**Drain** - Emergency pond reset. Destroys pond credentials on all stones, reverting garden to open mode.  
→ See: [security/pond-setup.md](security/pond-setup.md)

**Pond Name** - Water-themed display name assigned when a pond is created (`pond-{adjective}-{noun}`). Generated from a dictionary of 64 adjectives × 64 nouns (4,096 combinations) evoking water, reflection, and stillness. Purely decorative — can be changed at any time without affecting certificates or membership.  
→ See: [security/pond-setup.md](security/pond-setup.md)

---

## Companions

**Companion** - Extensible service running on a Stone that adds capabilities beyond core service management. Companions receive Stone presence events via SSE and execute commands via HTTP. Port assignments managed by Moss via persistent ledger (base 7187, range 7187-7199).

**Command Manifest** - JSON document describing Companion commands, parameters, and examples. Generated via `--dump-commands` protocol during Companion registration. Format includes command names, descriptions, parameter schemas, and usage examples.  
→ See: [specs/Companion-SERVICE-REGISTRY.md](specs/companion-service-registry.md)

**Port Ledger** - Persistent JSON file (`{data_dir}/companion-ports.json`) mapping Companion IDs to assigned ports. Moss assigns ports incrementally starting from 7187, ensuring no conflicts between Companions or restarts.  
→ See: [reference/ports.md](reference/ports.md)

**Hey-Tell Command** - Rake command syntax for Companion control: `garden-rake hey tell {Companion} {command} [args]`. Examples: `hey tell cricket play stone-online`, `hey tell cricket volume 50`.  
→ See: [specs/HEY-TELL-SYNTAX.md](specs/hey-tell-syntax.md)

**Cricket Companion** - Audio Companion providing 4-channel mixer and tune system. Maps Stone presence events (stone-online, service-started, etc.) to audio samples with configurable channels, volume, and looping. Includes 180 CC0 samples from Freesound.org.  
→ See: [decisions/CRICKET-0001-audio-Companion-spec.md](decisions/CRICKET-0001-audio-Companion-spec.md)

**Tune** - YAML configuration file mapping Stone presence events to audio samples. Specifies channel assignment (foreground/midground/ambient/background), volume, looping, debounce timing. Example: `zen-tech` tune sonifies infrastructure operations.  
→ See: [guides/cricket-tune-authoring.md](guides/cricket-tune-authoring.md)

**Mixer** - 4-channel audio system in Cricket providing layered soundscapes. Channels: foreground (alerts), midground (notifications), ambient (background loops), background (continuous ambiance). Supports simultaneous playback with per-channel volume control.

---

## Operations

**Installer** - USB creation tool (`NewStone-linux-x64.ps1` on Windows) that generates bootable Debian drives with preseed configuration. Automates Stone provisioning.

**Provisioning** - Stone setup process: boot from USB → install Debian → configure networking → install Moss daemon → join Garden.  
→ See: [guides/first-stone.md](guides/first-stone.md)

**Migration** - Moving services between Stones. Example: MongoDB running on failing laptop → migrate to replacement Stone without app downtime.

**Backup** - Local A/B snapshot of an offering's volumes and container image, stored on the Stone or replicated to a seed bank. Managed via `garden-rake backup` and the `/api/v1/stone/snapshots` API.

**Update** - Applying a newer version of an offering image or firmware to a Stone. Managed via `garden-rake upgrade` and the `/api/v1/stone/updates` API. Replaces the former "nourishment" terminology.

**Snapshot** - Point-in-time capture of an offering's data (volumes + optional container image commit). Stored in A/B rotation locally, with optional replication to seed banks.

**Health** - Service/Stone status monitoring. Moss checks container healthchecks and reports: Running, Stopped, Maintenance, Degraded, or Unknown.

**Diagnostics** - Troubleshooting data collection (logs, metrics, container state). Used to debug discovery failures, connection errors, and performance issues.  
→ See: [guides/troubleshooting.md](guides/troubleshooting.md)

**Compatibility** - Hardware/architecture validation rules. Example: MongoDB requires x86_64 (not ARM) and minimum 4GB RAM for production workloads.  
→ See: [decisions/COMPAT-0001-compatibility.md](decisions/COMPAT-0001-compatibility.md)

**Version** - Timestamp-based release identifier using Natural Flow Versioning. Format: `major.minor.timestamp`. Example: `0.1.202601181256`.  
→ See: [decisions/BUILD-0001-versioning.md](decisions/BUILD-0001-versioning.md)

**E-waste** - Repurposed obsolete hardware. Zen Garden's mission is to reduce the 62M tonnes/year of electronic waste by making old devices productive again.  
→ See: [philosophy/humanist-infrastructure.md](philosophy/humanist-infrastructure.md)

**Cordon** - Mark Stone as "do not schedule new services" (existing services continue). Used when hardware is flaky (overheating, disk errors).

**Lift** - Planned Stone maintenance (need to move device to different room). Services migrated temporarily, Stone taken offline gracefully.

**Replace** - Swap failing Stone with replacement. Services migrate to new Stone, old Stone retired. Apps reconnect automatically (connection strings unchanged).

**Retire** - Responsibly end-of-life a Stone. Data wiped, hardware recycled or repurposed. Services migrated to other Stones first.

---

## Architectural Terms

Terms used across [ARCH-0017](decisions/ARCH-0017-ddd-monolith-epic.md) and its books. These define how moss is *structured* — the vocabulary every contributor should share when reading, writing, or reviewing code under `src/moss/src/domain/`.

**Moss (struct)** - The daemon's runtime dependency injection container (`src/moss/src/app_state.rs`). Holds `Arc<Aggregate>` fields for each bounded context plus cross-cutting infrastructure (shutdown token, event bus, console). Renamed from `AppState` in ARCH-0037 (Book XIX) per code standards §3 (type names name the concept, not the architectural role). The only method with logic is `emit_storage_changed` (cross-cutting coordination across event bus, broadcast channel, tool projection, and orchestration nudge).

**Bounded context** - A module with a single responsibility, private state, and an explicit contract for cross-boundary interaction. Every `src/moss/src/domain/<name>/` subdirectory is a bounded context. Contexts never reach into each other's state directly; they interact through events and typed method calls.  
→ See: [specs/domain-aggregates.md](specs/domain-aggregates.md)

**Aggregate** - The root type of a bounded context. Owns private state, enforces invariants, exposes typed reads and commands. Example: `Offerings` is the aggregate for the offerings bounded context.  
→ See: [specs/domain-aggregates.md](specs/domain-aggregates.md)

**Aggregate root** - Same as Aggregate. The single entry point for reading or mutating a bounded context's state. Callers never touch state except through the aggregate root's methods.

**Command** - A write method on an aggregate. Mutates state, persists, emits an event, and returns a typed result. Commands use imperative verbs: `upsert`, `remove`, `promote`, `complete`, `join`. Every command funnels through a private `finalize` pipeline.

**Query** - A read method on an aggregate. Pure — no side effects except lock acquisition. Queries return snapshots (`snapshot()`), single items (`find_by_id()`), or scoped closures (`with_active(|o| ...)`). Queries never return raw lock guards.

**CQRS-lite** - Command/query separation. Read methods and write methods are distinct. API handlers translate HTTP `GET` requests to queries and HTTP `POST`/`PUT`/`DELETE` requests to commands. Moss uses a lightweight form of CQRS — the same aggregate holds both sides of the API, but they don't share code paths.

**Domain event** - A broadcast message describing a state change in a bounded context. Every aggregate publishes one event type (e.g., `OfferingsChanged`) via a `broadcast::Sender`. Subscribers (typically projection tasks) react by rebuilding their derived view. Events are the lingua franca between contexts — contexts never call each other's command methods directly from inside another aggregate.

**Ubiquitous language** - The shared vocabulary of a project, used identically in code, docs, and conversation. Moss's ubiquitous language is captured in this glossary. Metaphorical terms (stone, pond, companion, ceremony, nourishment) are *the* terms — not cute aliases. A bounded context that uses different words for a concept than this glossary is a bug.

**Port** - A trait defining infrastructure a bounded context depends on. Example: `OfferingStore` is a port; `FileOfferingStore` is its adapter. Ports live inside their owning context (`domain/<context>/port.rs`); adapters live in `src/moss/src/infra/`. A domain module that imports `crate::infra::*` directly violates the port pattern.

**Adapter** - A concrete implementation of a port. Adapters translate between domain types and foreign models (Bollard, filesystem, HTTP, UDP). Anti-corruption happens inside the adapter — foreign types never cross the adapter boundary into domain code.

**Anti-corruption layer (ACL)** - The translation boundary where foreign models meet domain models. Moss has ACLs at every adapter: Bollard container types translate to `OfferingFqn` + `OfferingLocation`, Ollama's HTTP responses translate to `HealthChanged`, mDNS SRV records translate to `StoneDiscovered`. The ACL exists so foreign breaking changes are contained inside the adapter.

**Projection** - A derived view of state, maintained by reacting to domain events. Example: the tool registry is a projection of offerings (plus storage banks). When offerings change, the projection task rebuilds the tool registry. Projections are always downstream of an event stream; they never mutate the source.

**Projection task** - A `BackgroundTask` (per [ARCH-0015](decisions/ARCH-0015-task-supervisor-registry.md)) that subscribes to an aggregate's `changes()` stream and maintains a projection. Projection tasks follow three non-negotiable rules: subscribe before seed, lag-tolerant (full reconcile on `RecvError::Lagged`), shutdown-aware (select on cancellation token).

**Finalize pipeline** - The private method every aggregate command calls after mutating state. It persists through the store port, records metrics, and emits the domain event. Ordering matters: persist first, meter second, emit third. A mutation that fails persistence does not fire an event.

**Chirp** - A UDP announcement broadcast by a stone describing its current topology (stone identity, capabilities, services, health). Chirps are the mechanism by which peer stones learn about each other without central coordination. Emitted immediately on topology-changing mutations and periodically as a heartbeat.

**Topology** - A stone's self-description at a moment in time: identity, address, capabilities, offerings, health, MAC, tags. Topology is built on demand from the aggregates that own each piece (Current, Offerings, Tool, Presence) and serialized into chirps, API responses, and mDNS TXT records.

**Tool** - A generic view over a service (an offering instance) or a data source (a seed bank). Tools are published to a garden-wide registry so other stones can discover what's available without knowing about moss-specific types. The tool registry is the projection that `garden-rake list` and `garden-rake find` consume.

**Tools beacon** - A UDP broadcast of tool deltas, announced alongside chirps so peer stones update their tool registries.

**Strangler vine** - A migration technique where new code is introduced alongside old code, and old callers are migrated gradually rather than in a flag-day rewrite. [ARCH-0016](decisions/ARCH-0016-offerings-aggregate-domain.md) used a strangler vine (`ActiveGuard`) to keep 82 read sites compiling while the aggregate pattern was introduced. Strangler scaffolds are tracked in [scaffolding.md](scaffolding.md) with explicit removal triggers.

**Scaffolding** - Intermediate-state code that exists only during an in-progress refactor. Every scaffold is tracked in [scaffolding.md](scaffolding.md) with an ID, a removal trigger book, and a removal action. Untracked scaffolds (`TODO: migrate later` comments with no entry) are forbidden.

**Book** - A unit of work in the ARCH-0017 epic. Each book refactors one bounded context (or one coordinated group) and ships green to `dev` as a single reviewable PR-sized unit. Books follow a fixed six-chapter template (scope, extract, wire events, migrate call sites, delete old surface, verify). The epic has 21 books (Book 0 prologue + Books I–XX).

**Chapter** - One commit inside a book. Chapters follow a fixed template: scope & ADR, extract the aggregate, wire events & projections, migrate call sites, delete old surface, verify & document.

**Shippability rule** - The constraint that every book merges green to `dev` at its final chapter. No long-lived epic branch. No cross-book atomicity. The `dev` branch is always buildable and testable.

**Discovery mandate** - The rule (amended into [ARCH-0017](decisions/ARCH-0017-ddd-monolith-epic.md) on 2026-04-11) that every book's Chapter 1 re-evaluates the plan against the current code before writing any implementation. If the plan is wrong, the author changes the plan (logging the amendment in ARCH-0017's revision history) before writing code. Material plan changes are surfaced to the user for visibility.

### Infrastructure terms (Book XII / ARCH-0030)

**ContainerRuntime** - The sealed abstraction over the container engine (Docker/Podman). Lives at `docker::ContainerRuntime` — all Bollard types are confined inside the `docker::` module boundary. Call sites use `state.platform.container.*` with domain-type returns. No trait abstraction needed — the concrete struct already is the anti-corruption layer.

**ContainerEvent** - Domain-level lifecycle event from the container runtime. Captures container name and action string (start, stop, die, kill, destroy, health_status). Replaces raw Bollard `EventMessage` so no foreign types cross the `docker::` module boundary.

### Observability-specific terms (Book I / ARCH-0018)

**Resources** - Hardware state snapshots (CPU, memory, disk, network, GPU, uptime). Dynamic but derived from the physical stone. Renamed from "metrics" in Book I Chapter 2 to free the term "metrics" for its proper observability meaning. See `/api/v1/stone/resources`.

**Metrics** - Software observability of the stone's behavior — per-domain event counters, mutation latency histograms, per-task timing and subscriber lag, process-global totals. Distinct from Resources (which is hardware state). See `/api/v1/stone/metrics`.

**Latency histogram** - A Prometheus-compatible observation structure with fixed upper-bound buckets (1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s, +Inf), a total count, and a cumulative sum of milliseconds. Each observation falls into exactly one bucket. Used to track mutation latency for domain aggregates. Lock-free via atomic bucket counters.

**Interesting transition** - A state change worth broadcasting to subscribers, as opposed to a routine counter increment. Metrics fires `MetricsChanged` events only on interesting transitions (task state changes, lag detection, domain/task registration) — counter increments are too high-volume to stream and are observed via snapshot polling instead. Push/pull duality per ARCH-0018.

**Subscriber lag** - The condition where a broadcast consumer falls behind the producer and misses events. Detected via `tokio::sync::broadcast::error::RecvError::Lagged(skipped)`. Projection tasks handle lag by recording the skipped count through `Metrics::record_subscriber_lag` and doing a full reconcile from the producer's snapshot rather than breaking the stream.

**Register-with-kinds** - The pattern used by Metrics to achieve lock-free per-kind counter increments without a concurrent map dependency. Domains call `register_domain(name, kinds: &'static [&'static str])` at construction; the kinds populate a plain `HashMap<&'static str, AtomicU64>` that is never mutated afterward. Lookups take a read lock on the outer state map only, then atomic-increment on the looked-up counter. No `DashMap`, no `Mutex<HashMap>`.

**Observability vs lifecycle (tasks)** - Two complementary surfaces. `/api/v1/stone/tasks/{name}` returns task **lifecycle state** (Waiting/Running/Completed/Failed) from `SupervisorHandle`. `/api/v1/stone/metrics/tasks/{name}` returns **observability data** (timing, event counts, subscriber lag) from the Metrics aggregate. Consumers that want unified status join the two by task name.

### Catalog aggregate terms (Book V / ARCH-0022)

**Catalog (bounded context)** - The aggregate owning the compile-time manifest catalog: a frozen `Arc<ManifestRegistry>` (immutable after bootstrap) providing typed manifest queries, and a mutable `RwLock<CatalogState>` holding the compiled offerings index (per-offering compatibility evaluation, image resolution, port/volume/env resolution). Typed commands `load` (idempotent, cache-first) and `rebuild` (force-refresh after capabilities change) own the write path; typed queries (`get_manifest`, `get_compiled`, `compiled_snapshot`, `stats`, `is_loaded`, `manifest_count`, `find_hw_manifest`, `manifests`) return owned values. Third persistent aggregate after Offerings and Topology.

**Frozen input** - An immutable cross-crate type held by an aggregate as a struct field but not subject to mutation, persistence, dirty tracking, or event emission. Part of the aggregate's state *shape* (queryable through typed methods) but not its *identity* (cannot mutate or subscribe to changes). Example: `Catalog` holds `Arc<ManifestRegistry>` as a frozen input — the registry is built once in `bootstrap::build_state()` and never changes. No interior lock needed.

**Dual-rebuild invariant** - The constraint that the catalog must tolerate being built twice per process start with different hardware capabilities. The `catalog-builder` task calls `Catalog::load()` early with zero or partial capabilities (GPU detection takes 2-6 seconds on Windows). Later, `hardware-detection` calls `Catalog::rebuild()` with the complete capabilities snapshot to refresh compatibility decisions (e.g., "no GPU -> ollama incompatible" transitions to "CUDA detected -> ollama compatible"). Both entry points are stable typed commands on the aggregate.

**Typed errors (Catalog)** - The first domain aggregate in the ARCH-0017 epic to return `Result<T, CatalogError>` from commands instead of infallible mutations or `anyhow::Result`. `CatalogError` has four variants: `ManifestHashFailed`, `CompilationFailed`, `CacheReadFailed`, `CacheWriteFailed`. Matches code-standards section 10 "Domain errors as enums". Elevated to a first-class pattern deviation in `docs/specs/domain-aggregates.md`.

### Health aggregate terms (Book VII / ARCH-0024)

**Health (bounded context)** - A stateless command facade that orchestrates per-offering health probing, transition detection, and event emission. Unlike other aggregates, Health holds no internal state (`RwLock<State>`) — per-offering health lives on the `Offering.health: ServiceHealthStatus` field in the common crate. The aggregate delegates probe execution through the `HealthProbe` port (production: `DockerHealthProbe` wrapping Bollard container inspection) and offering mutation through the Offerings aggregate's `update` API. Three typed commands: `probe_offering` (probe→compare→mutate→emit), `apply_docker_event` (event-driven status update from Docker events), `update_notification` (set/clear degraded-offerings notification tag). Emits `HealthChanged` events on interesting transitions only.

**HealthProbe (port)** - The infrastructure trait for executing a health check against a named offering. Returns a `HealthProbeResult` with `status: OfferingStatus` and `health: ServiceHealthStatus`. Production adapter `DockerHealthProbe` wraps `docker::Client::get_service_status` + `get_service_health` (Bollard container inspection). The port is designed to be extended with HTTP/TCP probe adapters in future — the trait is generic enough — but Book VII delivers only the Docker adapter since no other probe mechanisms exist in the codebase.

**Health transition** - A change in an offering's `ServiceHealthStatus` that is classified as `Recovered` (offline/degraded→healthy), `Degraded` (any→degraded), or `Failed` (any→offline). Only transitions fire `HealthChanged` events — same-state probe results are no-ops. This "interesting transitions only" pattern matches the Topology aggregate's chirp-on-change invariant from ARCH-0020.

**Stone-level system health** - The `domain/health/system.rs` module containing pure functions (`check_disk_health`, `check_memory_health`, `build_disk_component`, `build_memory_component`, `determine_overall_status`) that compute the stone's overall health status for the `/api/health` endpoint. Distinct from per-offering health probing — this is infrastructure health (disk, memory, docker, initialization), not service health. Moved from `domain/health.rs` to `domain/health/system.rs` during the module extraction.

### Subsystems aggregate terms (Book VI / ARCH-0023)

**Subsystems (bounded context)** - The aggregate owning per-subsystem readiness state, backed by `tokio::sync::watch` channels. Subsystems are registered by name at bootstrap (`register("network")`, `register("docker")`); monitor tasks toggle readiness via `mark_ready`/`mark_unready` commands; consumer tasks and API handlers poll via `is_ready()` (synchronous, zero-cost) or await via `wait_ready()` (async). The simplest aggregate in the epic: no `RwLock` (the `HashMap` is frozen after registration and `watch::Sender::send_modify` is inherently thread-safe), no persistence, no typed errors. Replaces the prior `SubSystems` struct of `Arc<AtomicBool>` fields.

**Subsystem readiness** - A boolean gate indicating whether a prerequisite infrastructure dependency (network stack, Docker daemon) is operational. Producers (monitor tasks) set readiness; consumers (background tasks, API handlers) gate their work on it. Before ARCH-0023, readiness was an `Arc<AtomicBool>` threaded through constructors. After ARCH-0023, readiness is a named slot in the `Subsystems` aggregate with `watch` channel semantics — synchronous poll for existing sites, async wait for future sites.

**Watch channel (subsystems)** - `tokio::sync::watch` — a single-producer, multi-consumer channel where the latest value is always available via `borrow()`. Used by the Subsystems aggregate instead of `AtomicBool` because it offers both synchronous polling (`.borrow()` / `.is_ready()`) and async waiting (`.changed()` / `.wait_ready()`) with built-in change notification. No lock contention because `watch::Sender::send_modify` acquires only a brief internal lock that does not block readers.

### Jobs aggregate terms (Book IV / ARCH-0021)

**Jobs (bounded context)** - The aggregate owning the in-memory map of background job state (`Pending`, `Running`, `Completed`, `Failed`) keyed by job id. Typed commands (`submit`, `start`, `record_item_completed`, `record_item_failed`, `complete`, `fail`, `maintain`) own the write path; typed queries (`get`, `snapshot`, `list_active`, `active_count`, `find_active_by_prefix`) return owned values. The first **ephemeral aggregate with a periodic reaper** — no `JobStore` port (state is rebuilt empty on every process start), but a `JobsReaperTask` sweeps terminal jobs past the 24-hour TTL every 10 minutes.

**Terminal TTL** - The retention window for `Completed` / `Failed` jobs in the `Jobs` aggregate. Default 24 hours, defined by `DEFAULT_TERMINAL_TTL` in `domain::jobs::maintenance`. A terminal job whose `completed_at` is older than the TTL is swept by `Jobs::maintain` on the next reaper tick. Active jobs (`Pending`, `Running`) are never evicted regardless of age — a stuck job is a bug worth surfacing, not a memory leak to hide. The TTL closes the "jobs accumulate forever" memory-leak class identified in ARCH-0021 Chapter 1: production stones that completed hundreds of jobs per day previously drifted unbounded since nothing removed finished jobs from the map.

**`JobsReaperTask`** - The background task registered in `tasks::task_registry` that drives periodic eviction of terminal jobs. Fires every 10 minutes (`REAPER_INTERVAL_SECS`), calls `state.jobs.maintain()`, logs the evicted / kept counts when a sweep does any work. Leaf task (no `dependencies()` override), registered alongside `RegistryMaintenanceTask`.

**Dual event streams (Jobs)** - The `Jobs` aggregate exposes two parallel broadcast streams from the same command gateway: `changes()` carries the internal `JobsChanged` domain event (7 kinds — Submitted, Started, ItemCompleted, ItemFailed, Completed, Failed, Evicted) with rich metadata for in-process subscribers; the pre-existing wire-format `JobEvent` (Started / Progress / Completed / Failed) continues to flow through `EventBus::emit()` for rake and dashboard SSE consumers that subscribe to the pulse firehose. Every command that maps to a `JobEvent` variant emits *both* streams atomically from inside the aggregate command. Matches Book II's `ToolChanged` + `ToolDelta` precedent — the wire format cannot be collapsed without breaking public consumers.

**Infallible mutations (Jobs)** - All `Jobs` commands return `()` (or a value) and no `JobsError` type exists. A mutation addressed at a missing job id is treated as a warn-level no-op. Reuses the Book I `Metrics` deviation rationale: an ephemeral aggregate with no persistence port and no cross-context invariants has no domain-meaningful failure modes to surface — there is no save to flunk, no external port to propagate errors from, and no rule for a command to violate.

**`find_active_by_prefix`** - Typed query on the Jobs aggregate that returns the first active (`Pending` or `Running`) job whose id starts with a given prefix. Replaces the open-coded HashMap-scan loops that previously lived in `api::v1::offering_capabilities` for duplicate-add and duplicate-refresh detection. Used to return `InProgress` responses when an operator retries an operation whose job is still running — one typed method serving two handlers instead of two 15-line inline loops.

**`Job.offerings` wart** - The `offerings: Vec<String>` field on the `Job` domain type is semantically overloaded: for install jobs it holds service names; for `refresh-capabilities` and `add-capability` jobs it holds capability names so progress can be computed as `completed.len() / offerings.len()`. The field is serialized in the public `/api/v1/jobs` response shape — renaming it to `targets` is a breaking wire-format change deferred to the post-epic API realignment project via the `deferred-job-offerings-field` entry in `docs/scaffolding.md`. The aggregate's `submit` command takes the parameter as `targets: Vec<String>` internally; only the serialized field keeps the legacy name.

### Topology aggregate terms (Book III / ARCH-0020)

**Topology (bounded context)** - The aggregate owning the peer cache on a stone: a map of `stone_id → TopologyEntry` holding every stone this stone has discovered (online + offline). Typed commands (`upsert_from_chirp`, `mark_stone_offline`, `forget_stone`, `maintain`, `flush`, `build_self_entry`, `sync_services`, `sync_capabilities`, `update_stone_health`, `announce_resolution_change`, `chirp`) own the mutation path; typed queries (`all_stones`, `online_stones`, `get_by_id`, `get_by_name`, `count`, `online_count`, `is_dirty`) return owned values. Second persistent aggregate after Offerings.

**SelfEntryInputs** - Explicit input struct for `Topology::build_self_entry`. Holds stone identity, address, health, mac, capabilities, tags, services, moss version, and the `network_ready` flag. Caller composition helpers in `domain::topology::composition::*` assemble this from Moss before invoking the aggregate — the aggregate holds no back-reference to Moss per ARCH-0020's "Alternative B rejected" rationale.

**Interesting transition (Topology)** - `TopologyChanged` fires only on status changes, not on every `upsert_from_chirp`. New stones fire `StoneDiscovered`; Offline→Online transitions fire `StoneOnline`; Online→Offline transitions fire `StoneOffline` (via maintenance or goodbye); explicit operator forgets fire `StoneForgotten`; TTL evictions fire `StoneEvicted`; local self-entry chirps fire `SelfEntryChirped`. Peer refreshes of unchanged entries produce no event — too high-volume for the interesting-transition stream. Same push/pull duality as Metrics per ARCH-0018.

**ChirpTransport** - Port injected into `Topology` for publishing `STONE_CHIRP` announcements over the garden's UDP transport. Production adapter `P2pChirpTransport` wraps `crate::announcement::announce`; test adapter `NoopChirpTransport`. Removes direct `crate::announcement::*` imports from the aggregate per code-standards §15.

**TopologyStore** - Persistence port injected into `Topology` for reading and writing `garden-topology.json` per TOPO-0002. Production adapter `FileTopologyStore` writes atomically via `tmp + rename`. Second persistent aggregate's store port after `OfferingStore` (ARCH-0016). Contrast with Tool / Metrics / Resources which are ephemeral aggregates.

**Topology composition helpers** - Free functions in `domain::topology::composition::*` that take `&Moss` and call the aggregate's typed commands. They own the assembly of `SelfEntryInputs` from the seven upstream Moss sources (stone identity, address, health, mac, capabilities, presence tags, offerings, subsystems readiness) plus the mDNS re-registration side-effect in `announce_resolution_change` — mDNS stays outside the aggregate because Discovery is Book X's scope. Same shape as Book II's `domain::tool::projection::*` helpers (ARCH-0019 Ch5).

**Always-dirty invariant** - The `Topology::upsert_from_chirp` command always marks the cache dirty for persistence. Collapses the prior split into `upsert_from_chirp` (no-mark) + `upsert_from_chirp_dirty` (with-mark) — the aggregate owns the invariant that every mutation is followed by persistence.

### Storage domain terms (Book VIII-a / ARCH-0025)

**Bank** - The user-facing named storage container (FQN: "personal", "media"). A bank groups volumes across stones under a shared `replica_set_id`. In moss, Bank is a view aggregate: derived from the volume collection at query time, not separately persisted. Typed commands: `rename`, `set_roles`, `set_visibility`, `pin`, `unpin`, `release`. Typed queries: `local_banks`, `by_name`, `primary_volume`, `volumes_for_bank`. Source: `src/moss/src/domain/storage/bank_aggregate.rs`.

**Volume** - A physical storage device known to this stone (USB drive, NAS mount, local directory). Owns a state machine (Online / Degraded / Offline) driven by OS facts. A managed volume belongs to exactly one bank (via `Management::replica_set_id`). Source: `src/moss/src/domain/storage/volume.rs`.

**VolumeIngestor** - Domain bridge for physical storage events. Routes OS monitor events (appeared/vanished) into Volume state machines and forwards the returned events to the broadcast channel. Previously named `StorageBank` (renamed in ARCH-0025 to avoid conflicting with the Bank aggregate). Source: `src/moss/src/domain/storage/bank.rs`.

**BankError** - Typed error enum for bank-level operations: `NotFound`, `InvalidName`, `PinFailed`, `UnpinFailed`. Fourth pattern deviation (typed errors) following Catalog (ARCH-0022).

### Discovery aggregate terms (Book X / ARCH-0028)

**Discovery (bounded context)** - The aggregate encapsulating mDNS service registration, the Koi embedded handle, and peer discovery via lurk-listener. Ephemeral (no persistence). Typed commands: `reregister` (mDNS `_moss._tcp` + `_http._tcp`), `update_health` (mDNS TXT record), `register_certmesh` (`_certmesh._tcp` CA service). The Koi handle is accessed via `koi()` — it serves multiple domains (Security for certmesh, Storage for vault, Discovery for mDNS) and stays on Discovery as the multi-capability embedded handle's home. Source: `src/moss/src/domain/discovery/`.

**Koi handle** - `Arc<KoiHandle>` from `koi-embedded`. A multi-capability embedded service providing mDNS, DNS, certmesh, vault, proxy, and health sub-handles. Owned by the Discovery aggregate but consumed cross-domain via `state.discovery.koi()`. The handle's placement under Discovery is historical — it predates DDD extraction — but is preserved because Discovery is the closest domain to its primary function (service advertisement).

**Lurk-listener** - A passive mDNS browse task that listens for `_moss._tcp` service announcements from peer stones. Discovered peers are fed to the Topology aggregate via `topology.upsert_from_chirp()`. The lurk-listener is infrastructure (Koi browse API wrapping) — it lives in `domain/discovery/mdns.rs` as a free function, not as an aggregate command.

### Companion integration platform terms (COMPANION-0001)

**Companion integration platform** - The architectural reframing codified by [COMPANION-0001](decisions/COMPANION-0001-companion-integration-epic.md): the companion runtime is a local-process **event integration hub**, not a device-driver framework. Hardware drivers, audio sinks, observability exporters, and external-system bridges are all the same type of thing — extensions that consume garden events and produce local effects. See [companion-architecture.md](specs/companion-architecture.md) for the pattern spec.

**Companion (struct)** - The top-level runtime of a companion binary. Replaces `CompanionRuntime` per code standards §3 (drops the `Runtime` suffix — the concept is already the runtime). Built by a fluent API: `Companion::new(config).with_transport(...).with_adapter_factory(...).run()`. Owns `Pulse`, `Garden`, and `Adapters` plus the shutdown token and transport list.

**Garden context** - The bounded context within the companion SDK that owns event ingestion, canonicalization, projection, and fan-out. Holds `Pulse`, `Garden`, and the `Transport` trait. Never knows which adapters exist; adapters subscribe to it. One of two bounded contexts in the SDK (see **Adapters context**).

**Adapters context** - The bounded context within the companion SDK that owns extension lifecycle: the `Adapter` trait, `AdapterFactory` registry, device/endpoint discovery, supervisor loop, and all cross-cutting concerns (subscription filtering, delivery policy, hydration, structured logging, dependencies, grace windows, persisted state). Spawns adapters; adapters consume Garden events. One of two bounded contexts in the SDK.

**Event (companion sense)** - The uniform envelope every companion-internal communication uses: `{ id: EventId, timestamp: DateTime<Utc>, kind: &'static str, payload: Arc<dyn EventPayload> }`. Presence events from moss, HTTP commands, inter-adapter messages, and future external-source messages all share this shape. Distinct from `PulseEvent` in moss (which is moss-internal). See [companion-architecture.md §The event envelope](specs/companion-architecture.md#the-event-envelope).

**EventId** - A GUIDv7 (`uuid::Uuid` generated with time-ordered semantics). Primary key for deduplication, sort key for replay, correlation anchor for distributed tracing. Every event carries one; the generator lives in the SDK.

**EventPayload** - The trait that every event payload implements. Provides `KIND: &'static str` (the canonical kind tag matching the envelope's `kind` field) and `COALESCING: bool` (whether the orchestrator may coalesce rapid bursts into the latest value). Downcastable from `Arc<dyn EventPayload>` via `Event::payload::<T>()`.

**Kind (event)** - The namespaced string identifier for an event type. Reserved prefix `core.*` for SDK-defined events (e.g., `core.stone.health.changed`). Companions use their crate name as the namespace (`firefly.*`, `cricket.*`). Commands are kinds: `firefly.command.brightness` is an event kind like any other. Validated at ingest time by `Pulse`. See [companion-architecture.md §Kind namespace convention](specs/companion-architecture.md#kind-namespace-convention).

**Pulse (companion SDK)** - The orchestrator in the Garden context — the single fan-in point for all events. Owns deduplication (bounded LRU by `EventId`), validation (namespace + kind/payload match), coalescing (per-kind for `EventPayload::COALESCING=true`), fan-out (broadcast channel), and metrics. Named symmetrically with moss's `state.pulse` broadcast — same concept, richer implementation on the receiving side. Distinct naming scope: moss's Pulse and companion's Pulse are separate codebases with parallel roles.

**Garden (companion SDK)** - The client-side CQRS projection of moss state. Read-model aggregate exposing **properties** (synchronous queries: `garden.health()`, `garden.load()`, `garden.offerings()`, etc.) and an **event stream** (`garden.events()`). Projects raw presence events from `Pulse` into typed domain state. First event any subscriber receives is a synthetic `GardenSnapshot` — hydration without special-cased init. Shares domain types with moss via `garden-common::domain`.

**GardenSnapshot** - The synthetic event emitted to every new `Garden::events()` subscriber as its first event. Carries the current `GardenState`. Unifies adapter initialization and crash recovery under a single code path — there is no distinct "startup" code in an adapter, only event handling.

**Transport** - The trait defining event sources (and sinks, for request-response patterns). Implementations: `SseTransport` (consumes moss `/presence/stream`), `CommandTransport` (HTTP server publishing command events and correlating result events back to HTTP responses). Part of the Garden context. Adapters never see transports. New event sources (MQTT, webhook, file watch) are new `Transport` implementations without touching the rest of the architecture.

**Adapter** - The extension contract in the companion SDK. A trait with three methods: `info()` (identity), `profile()` (subscriptions + delivery policy + dependencies + state persistence opt-in), `run()` (the event loop, receiving a filtered event stream and a `Garden` handle). One instance per physical device / logical endpoint. Owned by the Adapters context.

**AdapterProfile** - Declared metadata that tells the supervisor how to dispatch events to an adapter: which event kinds it cares about (subscriptions), how often it wants them delivered (`DeliveryPolicy`: All / LatestEvery / Debounced), what system dependencies its factory requires, and whether it opts into typed state persistence. Avoids adapters implementing per-adapter filtering and throttling themselves.

**AdapterFactory** - Produces `Adapter` instances for currently-present devices or endpoints. Methods: `kind()` (the factory's adapter-kind identifier), `required_dependencies()` (system deps installed/verified before any instance spawns), `discover()` (scan and return candidate adapters). Registered with `Companion` at wire-up; the supervisor polls `discover()` periodically.

**Adapters (aggregate)** - The supervisor struct in the Adapters context. Owns the factory registry, tracks running adapter instances by `AdapterInfo::id`, runs the discovery loop, applies cross-cutting concerns (filtering, delivery, logging spans, grace windows), and reaps disconnected adapters. Plural because it's the aggregate holding many `Adapter` entities — same naming shape as moss's `Offerings`, `Storage`, `Topology`.

**DeliveryPolicy** - How the supervisor paces event delivery to a specific adapter: `All` (every event), `LatestEvery(Duration)` (coalesce to latest at interval — e.g., matrix adapter at 30fps), `Debounced(Duration)` (quiet window after each delivery — e.g., cricket's tend sparkle). Declared in `AdapterProfile`; enforced by the supervisor before the adapter sees events. Orthogonal to (and layered above) `Pulse`'s global per-kind coalescing.

**Command-as-event** - The architectural commitment that HTTP commands flow through the same `Pulse` bus as every other event. `POST /command { raw_args: [...] }` becomes an event with `kind = "<companion>.command.<action>"` and a correlation ID. Adapters subscribe to command kinds they handle and publish `core.command.result` events. `CommandTransport` correlates the results and synthesizes the HTTP response. No adapter imports HTTP; commands gain for-free properties (correlation, fan-out to multiple adapters, idempotency, timeout at transport level).

**Correlation ID** - The identifier that ties a command event to its result event(s). Generated by `CommandTransport` when a command arrives; embedded in the command event's payload; echoed by adapters in their `core.command.result` payloads. The transport's correlation map awaits matching results within a timeout window, aggregates, and returns the HTTP response. Distinct from `EventId` — correlation ID scopes a request/response pair; `EventId` scopes a single event.

**Hexagonal architecture (companion)** - The organizing principle: a pure event-driven core (Garden + Pulse + GardenState projection) surrounded by pluggable ports. Transports are input ports (and output ports, for command responses). Adapters are output ports. The core knows neither. See [companion-architecture.md §Architecture overview](specs/companion-architecture.md#architecture-overview).

**Break-and-rebuild (COMPANION-0001 tenet)** - The methodological commitment that, where existing shape prevents a clean design, we rebuild rather than migrate. Under this tenet, Book VIII replaces the firefly and cricket crates wholesale without long-lived coexistence scaffolding. Contrast with ARCH-0017, which used strangler-style migration extensively. The difference is scale: the companion segment has no external consumers of its internals, so atomic replacement is cheaper than compatibility.

**Cross-cutting-concerns-at-owning-layer (COMPANION-0001 tenet)** - The generalization of the deduplication decision that seeded COMPANION-0001: any concern that three adapters would implement the same pattern for belongs at a higher layer. The adapter trait stays small; the supervisor (or orchestrator) owns filtering, delivery shaping, hydration, logging context, dependencies, cleanup, and state persistence. See [companion-architecture.md §Cross-cutting concerns matrix](specs/companion-architecture.md#cross-cutting-concerns-matrix).

### Tool aggregate terms (Book II / ARCH-0019)

**Tool (bounded context)** - The aggregate owning the garden-wide registry of `GardenTool` entries (offerings + seed-banks + gateway registrations + remote-announced tools from peer stones) on a single stone. Typed commands (`upsert`, `register_gateway`, `deregister_gateway`, `reap_expired_gateways`, `reconcile_local`, `apply_remote_beacon`, `remove_stone`) own the write path; typed queries return owned values without leaking references across the lock boundary. See `/api/v1/stone/tools/{fqid}` and `/api/v1/garden/tools`.

**Dual event streams (Tool)** - The Tool aggregate exposes two parallel broadcast streams from the same command gateway: `changes()` carries the internal `ToolChanged` domain event (origin, cursor, batch counts) for in-process subscribers; `delta_stream()` carries the wire-format `ToolDelta` consumed by SSE and UDP beacon subscribers. Every command feeds both streams atomically. Documented deviation — `ToolDelta` is an existing consumer-facing contract that cannot be collapsed into `ToolChanged` without breaking rake, garden dashboards, and peer-stone beacon receivers.

**`ToolsBeaconTransport`** - The port injected into the Tool aggregate for publishing UDP tools beacons. Production adapter `P2pBeaconTransport` wraps `garden_common::infra::communications::p2p::send_announcement`; test adapter `NoopBeaconTransport` drops deltas. Replaces direct `crate::infra::tools::*` imports from the aggregate per code-standards §15 (domain never imports infra).

**Ephemeral aggregate** - A DDD aggregate that has no `Store` port because its state is rebuilt on every startup from other domains' state plus runtime sources (remote beacons, TTL reaping). Metrics, Resources, and Tool all fit this pattern. No `save` after mutation, no `load` on boot, no persistence invariants to maintain. Documented as the first-class pattern deviation in `docs/specs/domain-aggregates.md` (added in Book II Ch6). Contrast with persistent aggregates like Offerings, which own an `OfferingStore` port and call `store.save(...)` from `finalize` after every mutation.

**Field-level strangler** - A migration shortcut used inside Book II (ARCH-0019 Ch3 refinement of the original ActiveGuard plan). The aggregate exposes its state as a `pub(crate)` field temporarily so legacy `state.tool.registry.read().await` call sites compile unchanged while typed methods grow alongside. Ch6 migrates the 14 API/domain/task read sites to typed methods; the field remains `pub(crate)` for the 25 infra-layer `{registry: &state.tool.registry, ...}` struct-field sites (`StorageResolver`, `StorageHandle`, cloud filter adapters) that legitimately need a raw registry handle as an infrastructure dependency. Net effect: the API/domain/task layer boundary is clean — no direct registry access — while the infra layer retains the handle as documented implementation surface.

---

## Quick Reference

| Term              | Summary                                    | Category  |
| ----------------- | ------------------------------------------ | --------- |
| Stone             | Physical device running Moss               | Core      |
| Moss              | Daemon on each Stone (port 7185)           | Core      |
| Rake              | CLI tool for operators                     | Core      |
| Garden            | Collection of Stones                       | Core      |
| Offering          | Pre-defined service template               | Services  |
| mDNS              | Multicast DNS discovery protocol           | Discovery |
| Connection String | `zen-garden:<type>[/<db>]`                 | Discovery |
| Pond              | mTLS security layer                        | Security  |
| Pond Name         | Water-themed display name (decorative)     | Security  |
| Keystone          | Encrypted CA keypair file                  | Security  |
| Cornerstone       | First Stone with CA authority              | Security  |
| Lantern           | Optional HTTP directory (port 7186)        | Discovery |
| Set               | Logical namespace (database/schema/prefix) | Services  |
| Backup            | A/B snapshot of offering data              | Operations|
| Update            | Applying newer offering/firmware version    | Operations|
| Snapshot          | Point-in-time capture of offering data     | Operations|
| E-waste           | Repurposed obsolete hardware               | Mission   |

---

**Related**: [philosophy/](philosophy/), [specs/](specs/), [reference/](reference/)
