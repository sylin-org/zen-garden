# Zen Garden: A Codebase History

**Codebase**: [github.com/sylin-org/zen-garden](https://github.com/sylin-org/zen-garden)
**Period analyzed**: January 24 -- March 6, 2026 (42 days)
**Total commits**: 522
**Primary language**: Rust
**Author**: Leo Botinelly
**AI co-authorship**: 241 of 522 commits (46%) co-authored with Claude

---

## 1. Project Orientation

Zen Garden is a service discovery and orchestration system for self-hosted infrastructure running on repurposed hardware. It turns old laptops, thin clients, and single-board computers into a self-healing network of services -- framed explicitly as an answer to 62 million tonnes of annual e-waste.

The project is organized as a Rust workspace with six crates (`moss` daemon, `rake` CLI, `lantern` registry, `cricket` audio companion, `firefly` LED companion, and `garden_common` shared library), plus two standalone orchestrator crates (`ollama`, `mongodb`). It has an unusually rich documentation layer: 71 Architecture Decision Records, 36+ proposals, 12 philosophy essays, 26 narrative journey documents, 29 specs, and a 710-line changelog -- all produced in 42 days.

The metaphor is structural, not decorative. Devices are **Stones**. The daemon is **Moss** (grows on stones). The CLI is a **Rake** (tends gardens). The registry is a **Lantern** (see further). Security is a **Pond** (water creates boundaries). This vocabulary has been stable since the very first commit and actively constrains design decisions. As one of the project's own philosophy essays puts it: "The name you choose shapes what you build. Call your servers 'resources' and you will build systems that treat them as interchangeable. Call them 'stones' and you will build systems that respect their particularity."

The project explicitly describes itself as "post-refactoring greenfield" -- there was a prior implementation that was scrapped. The 129,284-line initial commit is not a typical genesis; it is an artifact compressed from earlier work. The code was restarted. The concepts survived.

---

## 2. Timeline of Major Events

### Week 1: The Big Bang (Jan 24--25)

| Date | Type | Event | Evidence |
|------|------|-------|----------|
| Jan 24 | FOUNDATION | Initial commit: 484 files, 129K lines. Moss daemon, Rake CLI, Lantern, Docker Compose orchestration, 30+ manifest templates, mDNS discovery, health monitoring, auto-adoption. The vocabulary (Stone/Moss/Rake/Lantern/Pond) already complete. | `106dbe3` |
| Jan 24 | CRYSTALLIZATION | README rewritten in "manifesto style" -- the e-waste framing, `zen-garden:mongodb` connection string, and entire conceptual model appear fully formed. "Turn old laptops into database servers. Swap failed hardware without updating configs. Understand what you're running." | `a5e77fc` |
| Jan 24 | SPLIT | Manifest system restructured into hw/sw separation. YAGNI applied: removed `variants`, `firmware.fallback`, `bios.default_password` from hardware manifests. First evidence of "kill what doesn't survive contact with reality." | `2123221` |
| Jan 25 | FOUNDATION | **The busiest day**: 43 commits. SoC/DDD architecture established in a single sprint. Common crate extracted in 5 sequential batches. P2P transport singleton designed and implemented. Election module built. Build system modernized. Multicast discovery implemented. The monolithic `main.rs` (3,976 lines) was reduced to 45 lines across 74 focused modules. | Multiple: `90106fc` through `387d97e` |
| Jan 25 | REALIZATION | "UDP discovery broken -- enable broadcast on receiver socket." First collision with real hardware. The protocol worked in theory; the socket configuration didn't. Fixed same day. | Commit message in sequence |
| Jan 25 | CONSOLIDATION | All bespoke UDP sockets consolidated into p2p transport singleton. The commit message says it plainly: "remove all bespoke UDP sockets, route through p2p transport." A rule was created from a pattern failure. | `7a6952e` |

**What happened on January 25th**: This single day tells the story of the entire project in miniature. The morning began with architectural refactoring -- extracting shared code into a common crate using SoC/DDD principles (ARCH-0001). By mid-day, the election module was being built with its own UDP socket. Then multicast discovery was added with yet another socket. Then broadcast broke. The response was not a patch but a consolidation: every UDP socket in the codebase was replaced with a single transport singleton. The ADR (COMM-0001) was written the same day. By evening, the build system had been modernized and the entire extraction was complete. 43 commits. One architectural principle established, tested, broken, and resolved.

### Week 2: Naming Things (Jan 26--Feb 2)

| Date | Type | Event | Evidence |
|------|------|-------|----------|
| Jan 26 | FOUNDATION | Stone Presence Protocol specced and implemented. Election protocol specced (ELECTION-0001). CHANGELOG begins. | `0dac337`, `3055ec3` |
| Jan 26 | FOUNDATION | Adapter framework for external processes (Cricket audio, Firefly LED). Port ledger with range 7187--7199 (13 companions max). | `6354218`, `e5cd381` |
| Jan 27 | FOUNDATION | Stone Portrait -- embedded HTML landing page per stone. The first web UI. Every stone gets a face. | `b6e00dd` |
| Jan 28-29 | FOUNDATION | Seed bank storage system: onboarding spec, API structure, beacon protocol. Three ADRs (STORAGE-0001/0002/0003) in two days -- the fastest spec-to-decision pipeline in the project. | `b7910e5`, `66748ad` |
| Jan 29 | FOUNDATION | **Second busiest day**: 43 commits. Firefly LED companion, storage infrastructure, Cricket audio companion, offering lifecycle events, nurturing scheduler. | Multiple |
| Jan 30 | RENAME | **"adapter" --> "companion"** throughout entire codebase. Commit message: "provides clearer terminology that better reflects the relationship between Moss and its helper processes." Breaking change, no backward compatibility. 43-file rename. The garden-adapter-sdk package became garden-companion-sdk. `AdapterRegistry` became `CompanionRegistry`. `adapter-ports.json` became `companion-ports.json`. | `0a12eb8` |
| Feb 1 | FOUNDATION | Guidance template system (GUIDANCE-0001): Mustache-like variable substitution for post-install instructions. Context-aware help that adapts to the stone's actual configuration. | `GUIDANCE-0001` |
| Feb 2 | RENAME | **"services" --> "offerings"** in presence protocol. `PresenceSnapshot.services` --> `PresenceSnapshot.offerings`, `ServiceState` --> `OfferingState`. A stone doesn't *have* services; it *offers* capabilities. | `680b237` |

### Week 3: The Offering Model (Feb 2--7)

| Date | Type | Event | Evidence |
|------|------|-------|----------|
| Feb 2 | CONSOLIDATION | Unified offering model created. Before this, there were separate `AdoptedOfferingInfo` and `BorrowedOfferingInfo` types -- different structs for containers, native processes, and external services. Now: one `UnifiedOffering` type with a mode field. | `313e269`, `13b4a35` |
| Feb 3 | RENAME | **UnifiedOffering --> Offering**. The migration artifact name removed. Legacy shims deleted. Commit message: "domain concept, not migration artifact." The prefix "Unified" was an implementation detail that leaked into the domain vocabulary. It survived exactly one day. | `c62f8fb` |
| Feb 3 | DELETION | Backward compatibility code explicitly removed: "Remove all migration and backward compatibility code." `to_adopted_offering_info`, `to_borrowed_offering_info`, `persist_registry`, `upsert_service` -- all gone. | `ed20e64` |
| Feb 3 | FOUNDATION | `.agentic/` context structure introduced -- tool-agnostic AI instructions replacing Claude-specific CLAUDE.md as the primary bootstrap. The project becomes self-aware about its agentic development process and creates infrastructure for it. | `3f8db0d` |
| Feb 4 | REALIZATION | Virtual adapter detection via MAC OUI lookup (COMM-0003). The previous approach used IP-range blocklisting to detect virtual network adapters (Hyper-V, VMware, Docker). This failed on non-standard configurations. The fix: look up the IEEE vendor prefix of each adapter's MAC address. Hardware truth over heuristic. | `COMM-0003` |
| Feb 6 | FOUNDATION | Tools domain created: a unified projection of offerings and seed-banks as "garden tools" with wishful readiness queries. `GET /api/v1/garden/tools` and `tools/stream` (SSE). Three ADRs would eventually cover this subsystem (TOOLS-0001/0002/0003). | `e848d8a` |
| Feb 7 | CONSOLIDATION | Comprehensive documentation revision: naming, structure, voice, navigation. 29 files renamed to lowercase-kebab-case. ~40 broken links fixed. Docs reorganized into guides/specs/reference/decisions/proposals/archive. The documentation system itself was refactored. | Multiple commits |

### Week 4: Security and Orchestration (Feb 9--18)

| Date | Type | Event | Evidence |
|------|------|-------|----------|
| Feb 9 | FOUNDATION | Caretaking sweep pipeline: automated maintenance for staging directories, Docker images, and stale binaries. The garden tends itself. | `d3841d9` |
| Feb 14 | PIVOT | **Pond security goes live.** All pond API handlers rewired from `NOT_IMPLEMENTED` stubs to live koi-certmesh operations. CA creation via in-process Tower service. TOTP enrollment. mTLS. Certificate handling. This was the longest-incubating feature -- stubs existed since Jan 24. Three weeks from placeholder to real. The original spec (POND-0001, describing Ed25519/XChaCha20-Poly1305 P2P shared-secret) was **never implemented**. The actual system uses ECDSA P-256 certificates via koi-certmesh. POND-0001 was explicitly marked "Superseded." | `e69aca3` |
| Feb 14 | CRYSTALLIZATION | Philosophy essays written. 12 documents in `docs/philosophy/`, from "Humanist Infrastructure" to "Empirical Specification" to "Joy in Infrastructure." Written the same day Pond went live -- three weeks into the project. These read as extracted understanding, not aspirational design. | Commits around `3e4ad8d` |
| Feb 15 | FOUNDATION | Phased cooperative shutdown (MOSS-0004): four phases with timeouts. Signal (CancellationToken) --> Cooperate (3s) --> Drain (8s) --> Exit (hard 15s). Integration with systemd sd_notify. | `MOSS-0004` |
| Feb 16 | FOUNDATION | Offering orchestration (ORCH-0001): fitness scoring, election-based primary selection, role state machine (Joining/Primary/Dormant/Degraded). First time Stones compete to run services based on hardware capability. UDP payload hygiene (COMM-0005) strips 50% of chirp size. | `e509db4` |
| Feb 18 | RENAME | **"router" --> "orchestrators/ollama"**. The entire `src/router/` directory renamed. 29 files moved. A specific tool (Ollama AI router) became a general concept (orchestrators). The rename happened when the author realized that what worked for routing AI models would work for coordinating database replicas. | Commits around `refactor(router)` |
| Feb 18 | FOUNDATION | Ollama orchestrator fully operational: jobs system, AutoPullMode, Koi discovery, model sync, demand-weighted placement, tokens/sec tracking, metrics persistence. 9 commits in one day -- a burst of implementation after the conceptual rename. | `f12e38f` through multiple |

### Week 5: Multi-Orchestrator and Stabilization (Feb 19--28)

| Date | Type | Event | Evidence |
|------|------|-------|----------|
| Feb 18-19 | CRYSTALLIZATION | Ollama orchestrator: fitness profiler with Fast/Degraded/Vetoed/Blocked verdicts (ORCH-0003). Gateway self-registration via Koi mDNS (ORCH-0004). Complete Ollama API coverage (16/16 endpoints). Extension API for models and stones inventory. | Multiple around `orch-0003/0004` |
| Feb 19 | FOUNDATION | Routing safety net (ORCH-0002): "Never refuse an installed model." Remove `NoViableTier` error. Always route to a fallback tier. Degraded label is advisory, not blocking. The system degrades gracefully rather than refusing. | `ORCH-0002` |
| Feb 22-23 | FOUNDATION | Topology advisor with water-fill placement algorithm. Dashboard UX: per-model request counters, remove buttons, advisor fallbacks. Benchmark hardening. Error diagnostics enriched into MetricEvents. | Multiple around `58c5163` |
| Feb 24 | SPLIT | `orchestrator-common` crate extracted -- shared infrastructure (discovery, topology, gateway, tools stream, persistence, events, HTTP helpers) pulled out of the Ollama orchestrator into a reusable foundation. The pattern generalized six days after it was born. | `15593bc` |
| Feb 24 | FOUNDATION | MongoDB orchestrator (ORCH-0007): replica set management, auto-discovery via Koi/topology, `rs.initiate()`/`rs.add()` bootstrap, health monitoring with oplog/cache/lag advisors, placement scoring. 6,259 new lines. Port 7191. Embedded HTML dashboard. | `715b27b` |
| Feb 26 | REALIZATION | Stabilization wave. Real hardware teaching real lessons: "Fixed 2-minute boot delay on all Linux stones" (systemd-networkd-wait-online masked). "Fixed duplicate offerings in `garden-rake list`" (FQN-blind upsert). Wake auto-reinstalls missing containers. Rake commands display API error details instead of bare HTTP status codes. | `feadb83`, `e61b153` |
| Feb 28 | FOUNDATION | `rake pulse` -- live terminal monitor (PULSE-0001). Frame-buffer renderer. Wire feed with budget-based detail enrichment. SSE transport events. The garden gets a heartbeat monitor. | `bcd6697` |

### Week 6: Type Safety and Convergence (Mar 1--6)

| Date | Type | Event | Evidence |
|------|------|-------|----------|
| Mar 2 | CRYSTALLIZATION | **FQN v2**: separator changed from `:` to `::`. Legacy formats auto-normalize on parse/deserialize. `OfferingFqn` becomes a typed value object with source awareness, builder methods, custom serde, and container encoding -- replacing all raw string FQN passing throughout the codebase. Source scheme grammar added (`image:`, `repo:`, `oci:`). OFFER-0003 explicitly superseded by OFFER-0006. | `2d489bb`, `9d67705` |
| Mar 2 | FOUNDATION | Image-direct deployment: deploy any Docker image without a curated manifest via `garden-rake offer image nginx:latest`. Resolution pipeline pulls, inspects OCI config, assigns ports/volumes, deploys through the standard container pipeline. The system decouples from its own offering catalog. | `9f6b1a1` |
| Mar 3 | CRYSTALLIZATION | Greenhouse -- offering management web UI. Portrait was per-stone overview; Greenhouse is the operational center for offering lifecycle. Three phases of UX iteration in 4 days. | `11893dc` through `ea5b39f` |
| Mar 4-5 | CONSOLIDATION | **TOOLS-0003**: Unified garden registry replaces separate `StorageCache` and `readiness.rs`. Twelve implementation steps executed in sequence. Dead code deleted. Single source of truth for tools, storage, and gateway state. | `2fa7d8e` through `0186736` |
| Mar 5 | FOUNDATION | `StoneBag` for lazy-cached stone metadata collapses 5 pre-flight HTTP calls into 1--2. Hot-path commands make zero pre-flight calls. Moss injects `X-Stone-Name`/`X-Stone-Id` response headers on every HTTP response. | `f5e65d4` |
| Mar 5-6 | CRYSTALLIZATION | Demand-weighted topology advisor (ORCH-0009). Three-axis optimization: Demand x Topology x Fitness. `DemandLedger` with exponentially decayed counters at three time horizons (15m reactive, 6h tactical, 3d strategic). GPU projected fitness catalog with 100+ GPU entries. Recommendation pinning. Recommended model monikers (`recommended:chat`, `recommended:vision`). The orchestrator becomes intelligent. | `73e9dfb`, `d1fdf66`, ORCH-0009/0010/0011 |

---

## 3. Conceptual Drift Map

### "Adapter" --> "Companion"
- **Started as**: "Adapter" -- generic integration layer for external processes
- **Became**: "Companion" -- a relationship metaphor (Cricket and Firefly *accompany* Moss)
- **Key moment**: Jan 30, single commit (`0a12eb8`), no backward compatibility. The commit message says "better reflects the relationship." A naming decision that enforced the garden metaphor's coherence. Every type, path, config file, and documentation reference changed in one pass.
- **Why it matters**: "Adapter" is engineering jargon. "Companion" is a relationship. The rename made Cricket (an audio companion that plays ambient sounds when infrastructure events occur) make intuitive sense. Cricket is deliberately undocumented -- an easter egg that rewards exploration.

### "Service" --> "Offering" --> "Offering Modes"
- **Started as**: "Service" -- generic cloud vocabulary. `PresenceSnapshot.services`, `ServiceState`.
- **Became**: "Offering" -- something a Stone *offers* to the garden (active, not passive)
- **Intermediate form**: `UnifiedOffering` existed for exactly one day before being renamed to plain `Offering`. The prefix "Unified" was an implementation detail that leaked into the domain.
- **Deepened into**: Three offering modes (OFFER-0005): Managed (containers, full lifecycle), Adopted (native processes, monitoring + configurable control), Borrowed (external services, announcement only). The original proposal used "Planted" for managed offerings; the final term was "Managed."
- **Key moment**: Feb 2-3, three commits. First the presence protocol renamed, then the unified model, then the migration artifact name removed.
- **Why it matters**: The question "are different deployment modes fundamentally different things, or the same thing with different configuration?" was resolved as "the same thing." This required deleting types, removing backward compatibility, and accepting that the first model was wrong.

### "Router" --> "Orchestrator"
- **Started as**: `src/router/` -- an AI model router (Ollama proxy with smart routing)
- **Became**: `src/orchestrators/ollama/` -- one of potentially many orchestrators
- **Key moment**: Feb 18. The concept generalized when the author realized that what worked for Ollama would work for MongoDB and others. Six days later, `orchestrator-common` was extracted and the MongoDB orchestrator was born.
- **Why it matters**: "Router" implies traffic direction. "Orchestrator" implies lifecycle management, placement decisions, health monitoring, and intelligent coordination. The Ollama orchestrator doesn't just route requests -- it benchmarks GPUs, advises on model placement, manages VRAM budgets, and provides demand-weighted topology optimization.

### FQN: String --> Typed Value Object
- **Started as**: Plain strings for offering names (`"ollama"`, `"mongodb"`)
- **V1** (Feb 6): FQN with instance separator `:` (`"ollama:dev"`). ADR: OFFER-0003.
- **V2** (Mar 2): Separator changed to `::`, source schemes added (`image:`, `repo:`, `oci:`), typed `OfferingFqn` struct. ADR: OFFER-0006, explicitly superseding OFFER-0003.
- **Key moment**: The V1 colon separator collided with Docker image syntax (`image:nginx:latest`). The colon meant three different things. V2 introduced `::` for instances and `image:` as a source scheme prefix.
- **Evidence of pain**: "Fixed projector FQID bug: `instance_name_from_fqid("mongodb::prod")` returned `":prod"` (V2 breakage)." The cascade of breakage from a single character change touched Moss, Rake, Lantern, both orchestrators, and all persistence layers.
- **Why it matters**: A textbook case of early convenience (colons are natural separators) creating later pain (when the namespace expands). The resolution -- a typed value object with custom serde -- ensures this class of error cannot recur.

### Discovery: Bespoke Sockets --> P2P Singleton --> Multicast-First
- **Started as**: Each module binding its own UDP sockets (`tokio::net::UdpSocket` scattered across the codebase)
- **Phase 1** (Jan 25): P2P transport singleton (COMM-0001). One socket, filtered subscriptions, no domain coupling.
- **Phase 2** (Jan 25): Multicast-first with broadcast fallback (COMM-0004). IPv4 multicast `239.255.42.99:7184` (TTL=1, LAN-only).
- **Phase 3** (Feb 4): Virtual adapter detection via MAC OUI (COMM-0003). Replace IP-range heuristics with IEEE vendor lookup.
- **Phase 4** (Feb 16): UDP payload hygiene (COMM-0005). Strip unused fields for 50% chirp size reduction.
- **Key moment**: Jan 25, when broadcast broke on real hardware. The decision rule became absolute: "NEVER import `tokio::net::UdpSocket` in domain/tasks modules."

### Pond Security: Stubs --> Abandoned Spec --> Certmesh
- **Started as**: `NOT_IMPLEMENTED` API stubs (present from Jan 24) + POND-0001 spec describing Ed25519/XChaCha20-Poly1305 P2P shared-secret model
- **Became**: Full CA-based mTLS via koi-certmesh with ECDSA P-256 certificates (Feb 14)
- **The abandoned spec**: POND-0001 was **never implemented**. The P2P shared-secret model described in the original specification was replaced entirely by a CA-based approach using an external library (koi-certmesh). POND-0001 is explicitly marked "Superseded" with a note: "This specification described a P2P shared-secret model...that was never implemented. The actual Pond implementation uses koi-certmesh."
- **Incubation**: Three weeks of stubs. The security layer was the last major subsystem to go live, consistent with the "Empirical Specification" philosophy: "You cannot specify what you haven't yet discovered."
- **Why it matters**: This is the clearest example of the project's cultivate-then-specify philosophy. The spec was written first, reality revealed a better approach, and the spec was honestly superseded rather than silently ignored.

### Storage: Scattered State --> Unified Registry
- **Started as**: `StorageCache` domain module + separate `readiness.rs` (Jan 29), plus 6 scattered AppState collections for different storage aspects
- **Intermediate** (Feb 17): Storage lifecycle objects (STORAGE-0007) unified the composition model with self-healing mount verification
- **Became**: Unified `GardenRegistry` (TOOLS-0003, Mar 4-5) merging tool/storage/gateway state into a single queryable source of truth
- **Key moment**: TOOLS-0003, twelve steps executed in sequence, ending with "delete dead cache.rs, readiness.rs, dead test code." Consolidation ends with deletion.

### Firefly: Three Hardware Tiers
- **Tier 1** (Jan 29): RP2040-Matrix LED grid, CircuitPython firmware, text serial protocol (FIREFLY-0001)
- **Tier 2** (Feb 3): ESP8266 OLED, monochrome with hardware color zones, dual-mode USB serial + WiFi standalone (FIREFLY-0002)
- **Tier 3** (Feb 19): T-Display Diorama, full-color pixel-art garden scene on ESP32 ST7789 display, presence protocol extensions for GPU util and I/O (FIREFLY-0003)
- **Why it matters**: The serial protocol was reused across all three tiers. Each tier added capability without breaking the abstraction. This is the companion philosophy in hardware form: ambient awareness that escalates in sophistication without increasing complexity for the operator.

---

## 4. Decision Record Archaeology

### The ADR Landscape

71 Architecture Decision Records organized by subsystem prefix. The project averages 1.7 ADRs per day of active development. The densest clusters reveal where the most exploration was required.

| Subsystem | ADR Count | Focus |
|-----------|-----------|-------|
| ORCH (Orchestration) | 11 | Replant ceremonies, routing, fitness profiling, placement, coordination, demand modeling |
| STORAGE | 8 | Seed bank onboarding, API structure, beacon protocol, resilience, replication, lifecycle, API split |
| OFFER (Offerings) | 6 | Taxonomy, namespace collisions, FQN, placement, modes, image-direct deployment |
| COMM (Communications) | 5 | P2P singleton, pipeline spec, virtual adapters, multicast, payload hygiene |
| SECURITY | 4 | Tiers, keystone rename, protection tiers, tier-2 deferral |
| MOSS (Daemon) | 5 | Registry, infrastructure handlers, Docker resilience, shutdown, env vars |
| FIREFLY | 3 | Three hardware tiers |
| TOOLS | 3 | Domain model, unified contract, garden registry |
| API | 2 | Dual-layer design, admin hierarchy |
| BUILD | 2 | Versioning, deployment packages |

### Eight Decision Chains

**Chain 1: P2P Communication and Discovery** (5 ADRs)

COMM-0001 (P2P singleton) --> COMM-0002 (pipeline spec) --> COMM-0003 (virtual adapter detection) --> COMM-0004 (multicast-first) --> COMM-0005 (payload hygiene)

The discovery transport required the most iterative refinement of any subsystem. Each ADR addressed a failure mode discovered in the previous iteration: port conflicts led to the singleton, text serialization needed a spec, Windows multi-NIC broke virtual adapter detection, multicast replaced broadcast for reliability, and payload size caused UDP fragmentation. The chain traces a path from "it works on my machine" to "it works on every machine we've tested."

**Chain 2: Offering Identity** (6 ADRs)

OFFER-0001 (taxonomy) --> OFFER-0002 (container namespace collision) --> OFFER-0003 (FQN v1) --> OFFER-0004 (intelligent placement) --> OFFER-0005 (offering modes) --> OFFER-0006 (image-direct, FQN v2; supersedes OFFER-0003)

The offering identity problem required six decisions over six weeks. The core question evolved: What *is* an offering? How do you name it? How do you distinguish instances? How do you place it on hardware? How do you deploy it if you don't have a manifest for it? Each answer revealed the next question.

**Chain 3: Orchestration** (11 ADRs)

ORCH-0001 (replant ceremony) --> ORCH-0002 (routing safety net) --> ORCH-0003 (fitness profiler) --> ORCH-0004 (gateway announcement) --> ORCH-0005 (CPU inference tier) --> ORCH-0006 (coordination mode) --> ORCH-0007 (managed logical sets / MongoDB) --> ORCH-0008 (handler election suppression / orchestrator-common) --> ORCH-0009 (demand-weighted topology) --> ORCH-0010 (extended fitness) --> ORCH-0011 (recommended model monikers)

The densest chain. Orchestration began as "move an offering from one stone to another" (replant ceremony) and evolved into a sophisticated demand-weighted topology advisor with GPU profiling, three-axis optimization, and capability-based model recommendations. This chain documents the journey from "how do we coordinate?" to "how do we coordinate intelligently?"

**Chain 4: Storage** (8 ADRs)

STORAGE-0001 (seed bank onboarding) --> STORAGE-0002 (API structure) --> STORAGE-0003 (beacon protocol) --> STORAGE-0004 (resilience) --> STORAGE-0005 (manifest-first discovery) --> STORAGE-0006 (replication with ChaCha20-Poly1305 encryption) --> STORAGE-0007 (lifecycle objects) --> STORAGE-0008 (garden/stone API split)

Storage was designed spec-first (three ADRs in two days) but required five more ADRs as implementation revealed complexity. The lifecycle objects ADR (STORAGE-0007) replaced 6 scattered AppState collections with a unified composition model. The API split (STORAGE-0008) separated garden-tier (name-based, cross-stone) from stone-tier (path-based, local) operations.

**Chain 5: Security** (4 ADRs)

SECURITY-0001 (pond tiers) --> SECURITY-0002 (keystone rename) --> SECURITY-0003 (protection tiers) --> SECURITY-0004 (tier-2 deferral)

The deferral ADR is the most interesting: it documents the explicit decision to *not* implement Tier 2 security yet. The project's philosophy ("Staying Focused") says: "Add features when real users ask for them." The deferral is a decision, not an absence.

**Chain 6: Moss Daemon Architecture** (5 ADRs)

ARCH-0001 (SoC/DDD) --> MOSS-0001 (persistent registry) --> MOSS-0002 (infrastructure handlers) --> MOSS-0003 (Docker runtime resilience) --> MOSS-0004 (phased cooperative shutdown)

The daemon's evolution from monolith to modular architecture. ARCH-0001 records the 3,976-line main.rs being reduced to 45 lines. Each subsequent ADR added a resilience pattern: persistent state survives restarts (MOSS-0001), trait-based handlers enable garden-wide effects (MOSS-0002), Docker health mirrors the network monitor pattern (MOSS-0003), and shutdown proceeds in four timed phases (MOSS-0004).

**Chain 7: Firefly Companion Progression** (3 ADRs)

FIREFLY-0001 (RP2040-Matrix LED) --> FIREFLY-0002 (ESP8266 OLED) --> FIREFLY-0003 (T-Display Diorama)

Hardware escalation: LED grid --> monochrome OLED --> full-color pixel-art garden scene. The serial protocol reuses across all tiers. Each tier adds visual sophistication without increasing operator complexity.

**Chain 8: Service Discovery and Registry** (4 ADRs)

LANTERN-0001 (registry architecture) --> LANTERN-0003 (mDNS service discovery) --> MDNS-0001 (single service type) --> DNS-0001 (Koi DNS local zone)

The discovery stack: HTTP registry for cross-subnet (Lantern), mDNS for same-subnet, single `_koan-stone._tcp` service type with TXT record differentiation, and finally DNS integration serving `.local` zone from the mDNS cache.

### Architectural Patterns in the ADRs

Several patterns recur across chains:

- **Monitor Pattern** (MOSS-0003): Atomic readiness flags with state-aware polling intervals (5s when unhealthy, 30s when healthy). First used for network monitoring, then replicated for Docker health.
- **Trait-Based Handlers** (MOSS-0002): Self-contained, locally-autonomous infrastructure effects. Each handler declares what it cares about and acts independently.
- **Never-Refuse Degradation** (ORCH-0002, COMM-0004): Always serve something, even if degraded. Advisory labels over hard blocks.
- **Phase-Based Ceremonies** (ORCH-0001, MOSS-0004): Long-running operations proceed through journaled, crash-recoverable phases with explicit timeouts.
- **Dual-Layer APIs** (API-0001, STORAGE-0008): Different interfaces for different audiences -- simple for beginners, detailed for power users.

---

## 5. Proposals: Paths Taken and Not Taken

### Proposals That Became Decisions

| Proposal | Became | Key Transformation |
|----------|--------|-------------------|
| `intelligent-offering-placement.md` | OFFER-0004 | Implemented cleanly: multi-factor scoring, parallel metrics, interactive CLI |
| `offering-modes.md` | OFFER-0005 | Terminology changed: "Planted" --> "Managed" between proposal and ADR |
| `unified-deployment-packages.md` | BUILD-0002 | Implemented with platform-specific finalization (Linux: ExecStartPre, Windows: flag-based) |
| `tools-domain-implementation.md` | TOOLS-0001, TOOLS-0002, TOOLS-0003 | Grew from 1 ADR to 3 as the domain's complexity became apparent |
| `rust-refactoring-proposal.md` | ARCH-0001 | The original plan was superseded by the more principled SoC/DDD approach |
| `windows-mdns-via-koi.md` | Archived | Partially implemented; Windows mDNS solved via Koi HTTP proxy |

### Proposals Still Active

| Proposal | Status | What It Reveals |
|----------|--------|-----------------|
| Orchestration suite (ORCH-0001 through ORCH-0008) | Phases 1-3 done, 4+ pending | Orchestration requires more exploration than any other subsystem |
| Nourishment safe updates | V0 detection implemented, ceremony engine designed but not built | Update execution is deliberately deferred -- detection is sufficient for now |
| Pond ceremony engine | Server-driven ceremony model designed, not deployed | Ceremonies as "constraint satisfaction over a bag of key-value pairs, not linear pipelines" |
| Rake taxonomy design | 60-70% implemented | Zen verbs (offer, tend, rest, wake) partially done; dual normative syntax pending |
| Stone hardware ecosystem | Vision complete, no physical implementation | "Shards" (purpose-built boards with RGB, OLED, rail mounts) exist only as design |
| Stone phone repurposing | Design document | Turn smartphones into Stones via PostmarketOS -- speculative but documented |

### Paths Not Taken

**Declarative orchestration --> Autonomous P2P**: The early proposals considered static YAML for federation and consistency. The project instead chose autonomous stones with pull-based sync and peer-to-peer election via UDP fitness scoring. The decision: distributed systems should discover their own topology, not have it dictated.

**Docker-only --> Multi-mode orchestration**: The initial assumption was containers everywhere. Reality (GPU workloads, existing services, external devices) forced the three-mode model. The philosophy of *shakkei* (borrowed scenery) -- using external capabilities as part of the garden's composition -- emerged from this collision.

**Linear ceremony pipelines --> Constraint satisfaction**: The pond ceremony engine was initially designed as a sequential wizard. The final design treats ceremonies as constraint satisfaction: the server returns prompts, clients are "dumb render loops," and the ceremony resolves when all constraints are met regardless of order.

**Enterprise security features --> "Stay focused"**: Multiple-invitation modes, authenticator app integration, multi-admin approval, MAC-based blocking -- all explicitly rejected. "The test: If it requires explanation beyond one sentence, it's probably wrong for our users."

---

## 6. Moments of Highest Uncertainty

### 6.1 The Offering Identity Crisis (Feb 2--3)

The project couldn't decide what a running service *was*. The initial commit had separate types for different modes: adopted native services, borrowed containers, managed deployments. Each had different fields, different lifecycle, different persistence.

The evidence of churn is vivid: `AdoptedOfferingInfo`, `BorrowedOfferingInfo` --> `UnifiedOffering` --> `Offering`. Legacy shims were added, then explicitly removed the next day ("remove all migration and backward compatibility code"). The `from_unified_offering` helper existed for less than 24 hours before becoming `from_offering`.

This wasn't a simple rename. It was the resolution of a conceptual uncertainty: *are different deployment modes fundamentally different things, or the same thing with different configuration?* The answer -- the same thing -- required deleting types, removing backward compatibility, and accepting that the first model was wrong.

### 6.2 The P2P Transport Consolidation (Jan 25)

On the busiest day of the project (43 commits), a crisis played out in miniature. The election module was built using its own UDP socket. The announcement system used another. Discovery used a third. When multicast was added, the fragmentation became untenable.

The evidence: `"UDP discovery broken -- enable broadcast on receiver socket"` followed by `"remove all bespoke UDP sockets, route through p2p transport"` followed by `"consolidate ALL UDP to common infra, proper SoC/DDD"`. Three commits that trace the arc from "it broke" to "why it broke" to "never again."

The resulting COMM-0001 ADR became one of the project's strictest rules: "NEVER import `tokio::net::UdpSocket` in domain/tasks modules." The rule exists because the violation was experienced.

### 6.3 The FQN Separator Collision (Mar 2)

FQN v1 used a colon (`:`) as the instance separator: `ollama:dev`. This worked until image-direct deployment introduced Docker image references: `image:nginx:latest`. The colon now meant three different things.

The fix was V2: double-colon (`::`) for instances, `image:` as a source scheme prefix. But the migration required touching every file that handled FQNs -- Moss, Rake, Lantern, both orchestrators, all persistence layers. The projector bug (`instance_name_from_fqid("mongodb::prod")` returning `":prod"`) shows the cascade of breakage from a single character change.

This is a textbook case of early convenience (colons are natural separators) creating later pain (when the namespace expands). The resolution -- a typed `OfferingFqn` value object absorbing all string manipulation -- ensures this class of error cannot recur.

### 6.4 The MongoDB Membership Authority (Feb 24--Mar 4)

The MongoDB orchestrator had two subsystems that both tried to manage replica set membership: the bootstrap module and the health monitor. They oscillated -- one would add a member, the other would remove it, and vice versa.

The evidence: `"fix(mongodb): single membership authority -- eliminate bootstrap/health-monitor oscillation"` followed by `"fix(mongodb): defer RS member add until config patch is applied"` followed by `"fix(mongodb): re-activate Offline/Down instances on re-discovery"` followed by `"fix(mongodb): flush stale IPs on offline stones"`.

Four sequential fix commits addressing the same conceptual error: distributed state management is hard when two modules claim authority. Resolution: single membership authority with explicit lifecycle states (Offline/Down health states, topology-driven lifecycle).

### 6.5 The Security Incubation (Jan 24 -- Feb 14)

Pond security was present as concept and API stubs from the very first commit. It appeared in the vocabulary, the README, the philosophy docs. But for three weeks, every endpoint returned `NOT_IMPLEMENTED`.

This is the most deliberate uncertainty in the project. The "Staying Focused" philosophy doc explicitly lists features not to add and says "Add features when **real users ask for them**." The "Empirical Specification" doc says "You cannot specify what you haven't yet discovered."

Meanwhile, the original POND-0001 specification -- describing a P2P shared-secret model using Ed25519 keys and XChaCha20-Poly1305 symmetric encryption -- was quietly set aside. When Pond finally went live (Feb 14), it used an entirely different approach: CA-based mTLS with ECDSA P-256 certificates via koi-certmesh. The three-week gap wasn't indecision; it was the specification meeting reality and reality winning.

---

## 7. The Stable Core

These elements appear in the first commit and remain essentially unchanged:

**The vocabulary**: Stone, Moss, Rake, Lantern, Pond. Not a single core term was renamed. The metaphor was correct from the start -- or more precisely, the metaphor was established *before* the initial commit, in an earlier iteration whose code was discarded but whose concepts survived. The project's own philosophy essay explains why: "The metaphor is not decoration applied after the fact. The metaphor is the blueprint you didn't know you were following."

**The port assignments**: Moss HTTP 7185, Lantern 7186, Companion range 7187--7199, Discovery UDP 7184, Moss HTTPS 7183. All present from day one. Never changed.

**The mission**: E-waste reclamation, service discovery over machine configuration, `zen-garden:mongodb` connection strings. The README has been rewritten three times (manifesto style, then formatted, then feature-table style) but the core framing has never changed. The opening line of the second-ever commit: "Every year, humanity generates 62 million tonnes of electronic waste."

**The DDD architecture**: Domain/infra/API separation. Present from the first refactoring day (Jan 25) and enforced increasingly strictly. The rule "Domain NEVER imports infra" appears in both CLAUDE.md and .agentic/CONTEXT.md. The ARCH-0001 ADR documents the initial extraction. The architecture was challenged by every new feature and was never compromised.

**The manifest system**: YAML frontmatter for offering templates. The format evolved (hw/sw split, embedded+overlay pattern, FQN metadata, manageable env vars) but the concept of curated templates per offering has been constant. 31 templates ship across 17 categories.

**Axum + Tokio + Bollard**: The HTTP framework (Axum), async runtime (Tokio), and Docker client (Bollard) have been the implementation substrate since day one. These were never reconsidered.

**The mDNS service type**: `_koan-stone._tcp.local` with TXT record differentiation. Established in MDNS-0001 (one of the earliest ADRs, Jan 15 -- predating the repository's first commit). The decision to use a single service type rather than per-offering types has never been revisited.

---

## 8. The Philosophy Layer

Twelve philosophy essays exist in `docs/philosophy/`, ordered for sequential reading. They were written on February 14 -- three weeks into the project, the same day Pond security went live. They read as extracted understanding, not aspirational design.

**"Empirical Specification"**: "We are not writing a specification and then implementing it. We are not hacking without direction and calling it agile. We are doing something in between... Test. Try something. Actually build it. Put it on real hardware, in real network conditions, with real constraints. See if it fits... The specification grows from the accumulated *yeses* -- the things that survived contact with reality."

**"The Metaphor Is the Architecture"**: "When you call a server a 'node,' you have already decided what it is. A node is a point in a graph... But when you call a server a Stone, something different happens. A stone has weight. It sits somewhere specific. You could trip over it."

**"Staying Focused"**: "The adversary is not the NSA. The adversary is: accidentally exposing MongoDB to the internet (HIGH), neighbor's kid on your WiFi (MEDIUM), your own typos and mistakes (HIGH)... If you need defense against nation-states, you need a security team, not a garden."

**"Failure as Weather"**: Storm/rain/frost/drought as operational vocabulary. "When someone says 'we're seeing some rain on stone-03,' you understand the severity without a rubric."

**"Discovery Over Configuration"**: Documents the `zen-garden:mongodb` connection string pattern and the five-layer discovery cascade (localhost cache, UDP broadcast, mDNS browse, Lantern query, manual override). Each layer degrades independently. "The lightness isn't a feature. It's the foundation that makes every other feature possible."

These are not design documents. They are retrospective crystallizations -- the project explaining itself to itself. The documentation system's own style guide (DOCUMENTATION.md) codifies this: guides use present instructional voice, specs use present declarative voice, ADRs use past historical voice. The litmus test: "If I deleted all ADRs, would every guide still make sense?" The answer must be yes.

---

## 9. Spec Fidelity: Where Documentation Meets Reality

An audit of specs against source code reveals that the documentation is approximately 80% extracted from implementation, 15% exploratory or draft, and 5% aspirational. Notable findings:

**High fidelity** (spec matches code):
- `moss-daemon-lifecycle.md` -- accurately describes domain/infra/api layering
- `discovery.md` -- mDNS service types, TXT records, UDP broadcast all match implementation
- `offering-fqn.md` -- FQN encoding rules match the Rust `OfferingFqn` type
- `companion-command-protocol.md` -- synchronous proxy with 5s timeout matches code
- `rake-commands.md` -- hot cache discovery, localhost-first pattern confirmed in implementation

**Stale spec** (code evolved past the spec):
- `topology-cache.md` specifies a 45-second offline threshold. The code (`topology.rs:44`) uses 90 seconds, with a comment explaining: "Stones chirp every 30s. At 90s (3 chirp cycles) we tolerate 2 missed chirps." The spec was written for an earlier, more aggressive threshold. The code is more mature.

**Superseded spec** (honestly documented):
- POND-0001 describes a P2P shared-secret model that was never implemented. Marked "Superseded" with full explanation. Preserved as historical artifact.

**Known gaps** (documented but unresolved):
- Windows self-update mechanism is non-functional for API staging (no systemd equivalent). Identified in archived planning docs. Not yet fixed.
- Moss topology persistence may still be in-memory only in some code paths (TOPO-0002 specifies files, implementation status uncertain).

---

## 10. The Co-Authorship Pattern

241 of 522 commits (46%) are co-authored with Claude. The earliest co-authored commits use `Claude Opus 4.5`; later ones use `Claude Opus 4.6`. This is not incidental tool use. The project has built infrastructure for this collaboration:

- **`.agentic/CONTEXT.md`**: Tool-agnostic rules shared across all AI assistants. Critical rules (check existing utilities, use shared models, respect architecture layers), verification commands, module structure, environment variables.
- **`.agentic/rules/`**: Domain-specific rule files that activate based on file globs: `api-handlers.md` for API code, `docker-ops.md` for container operations, `networking.md` for P2P transport, `companions.md` for the companion framework, `stone-ssh.md` for remote operations.
- **`.agentic/reference/`**: Lookup tables for utilities, constants, API endpoints -- preventing the AI from reinventing existing code.
- **`CLAUDE.md`**: Bootstrap file pointing to the agnostic context.

The 26 narrative journey documents (`docs/journeys/01-the-first-stone.md` through `26-the-reconciliation.md`) with their own `WRITING-GUIDE.md` suggest the project was being simultaneously built and narrated. The documentation is not afterthought but parallel track. The journey titles trace the user experience from setup through failure recovery: "The Night the Drive Died," "The Stone That Vanished," "The Failed Update," "The Stray Container."

---

## 11. Structural Observations

### The Rhythm of 522 Commits in 42 Days

- **192 commits in January** (8 days): Foundation sprint. The architecture was established, challenged, and stabilized.
- **265 commits in February** (28 days): Feature expansion. Orchestrators, storage, security, monitoring, companions.
- **65 commits in March** (6 days): Convergence. Type safety, consolidation, intelligence.

The cadence is not uniform. Jan 25 and Jan 29 had 43 commits each -- intense implementation sprints. Some days had zero commits. The pattern suggests "think deeply, then execute rapidly" rather than steady incremental progress.

### What the Churn Reveals

The most-touched file is `docs/CHANGELOG.md` (69 touches) -- a project that documents obsessively. Next: `bootstrap/run.rs` (67 touches) and `app_state.rs` (48 touches) -- the startup sequence and shared state were the most contested code. `Cargo.lock` (46 touches) reflects rapid dependency evolution. `router.rs` (43 touches) -- the HTTP routing table changed constantly as the API surface expanded.

### The Deletion Pattern

Deleted files cluster in three areas: (1) manifest templates that were culled or reorganized, (2) infrastructure modules that were consolidated (separate `p2p.rs`, `network.rs`, `registry.rs` all absorbed), and (3) domain modules replaced during consolidation (`storage_cache.rs`, `sub_capability.rs`, `readiness.rs`).

The project deletes confidently. `ed20e64` ("remove all migration and backward compatibility code") is characteristic -- once a migration is done, the scaffolding is torn down immediately. No `_deprecated` suffixes, no `// removed` comments, no backward compatibility shims retained "just in case."

### Source Code Sediment

Despite the confident deletion pattern, some geological layers remain visible:

- **`TEMPLATE_NOT_FOUND` and `TEMPLATE_LOAD_FAILED`** persist in `common/src/constants/mod.rs` even though the system now calls them "offerings." The error codes preserved the older vocabulary.
- **`src/moss/src/tasks/discovery.rs`** still contains foundation-era TODOs: "Build actual service list from running containers," "Add health status to registration," "Handle Lantern unavailability gracefully."
- **`src/moss/src/discovery.rs`** was deleted after consolidation into P2P transport. A `.backup` variant also appeared and was deleted -- someone kept a copy "just in case" before letting go.
- **The `ceremony/` directory** exists in the domain layer with TODOs for features like quiesceable harvesting -- future capability sketched in code structure.

---

## 12. Closing Observations

This is a project that knows what it is. The vocabulary was stable from day zero. The mission never wavered. The architecture found its shape within the first 48 hours and has been refined, not replaced, ever since.

What changed was *understanding* -- of what an offering is, of how to route UDP, of when security should go live, of how to name the separator in an identifier, of whether deployment modes are different types or the same type with different configuration. These are not pivots in direction. They are deepenings of comprehension. The project moved from "approximately right" to "precisely right" on each concept, usually through the cycle described in its own philosophy: test, see if it fits, incorporate.

The most striking feature is the project's self-awareness. It wrote philosophy essays explaining its own methodology. It created `.agentic/` infrastructure documenting its AI-assisted development process. It maintained narrative journey documents alongside the code. It wrote a documentation style guide and then audited its own documentation against it. It keeps an honest record of superseded specifications (POND-0001 was never implemented, and the spec says so).

The 71 ADRs in 42 days are not bureaucracy. They are the memory of a cultivation process. Each one records a moment where reality taught something the theory didn't predict. The chains connecting them -- 5 for communications, 6 for offerings, 11 for orchestration -- trace the project's learning paths through its most complex domains.

The archaeology suggests a development philosophy that the project itself articulates: "This is slower than writing a specification upfront. It is faster than writing a specification, implementing it, discovering it's wrong, and rewriting both."

The specification emerged from cultivation. The archaeology confirms it.
