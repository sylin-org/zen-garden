# Zen Garden & Koan Framework — Patent Analysis

**Date**: 2026-03-24
**Scope**: Zen Garden (Rust platform) + Koan Framework (.NET application framework)
**Status**: Analysis — not filed

---

## Patent Family 1: Intent-Based Infrastructure Resolution with Pluggable Provider Layers

**The core invention.** A system where applications declare *what infrastructure they need* using intent URIs, and a layered pipeline of pluggable providers resolves, provisions, connects, and maintains the connection — without the application knowing where anything runs.

### 1.1 Intent Handler URI Scheme

The invention introduces an **intent handler** — a URI scheme that routes to a resolution pipeline instead of a network endpoint. The scheme name is the dispatch key; any system can register its own handler.

```
{handler}://{offering}[:{instance}][?cap={capability},...]

zen-garden://mongodb                        → "give me MongoDB"
zen-garden://mongodb:prod                   → "give me the prod instance"
zen-garden://ollama?cap=model:llama2        → "give me Ollama with llama2 loaded"
```

`zen-garden://` is one implementation of the pattern. The scheme identifies which resolution pipeline to invoke — not a host, not a protocol. A connection string says *where* and *how*. An intent handler URI says *what* and *who resolves it*.

The URI grammar is intentionally minimal:
- **Offering**: the capability being requested (not the product name — `mongodb` means "a MongoDB-compatible service", not "connect to MongoDB Inc.")
- **Instance** (optional): a named deployment variant (`:prod`, `:dev`)
- **Capabilities** (optional): sub-requirements the offering must satisfy (`?cap=model:llama2`)

Any framework can register its own handler scheme and plug in its own provider chain. The dispatch mechanism, not the scheme name, is the invention.

**Implementation**: `Koan.ZenGarden.Core/ZenGardenConnectionIntent.cs` — `TryParse()`, `ForOffering()`, `ToOfferingSelector()`. Scheme constant: `ZenGardenConnectionIntent.Scheme = "zen-garden"`.

### 1.2 Reference-As-Intent

Adding a library package to a project automatically registers an infrastructure intent binding. No configuration required.

```csharp
// Adding this NuGet package:
<PackageReference Include="Koan.Data.Connector.Mongo" />

// Auto-registers via IKoanAutoRegistrar:
services.TryAddEnumerable(
    ServiceDescriptor.Singleton<IZenGardenOfferingBinding, MongoZenGardenOfferingBinding>());
// AdapterId = "mongo" → Offering = "mongodb"
```

The act of referencing a library IS the intent declaration.

**Implementation**: `IZenGardenOfferingBinding` interface; `MongoZenGardenOfferingBinding`, `OllamaZenGardenOfferingBinding`, `WeaviateZenGardenOfferingBinding`.

### 1.3 Multi-Strategy Autonomous Discovery

Each infrastructure domain implements `IServiceDiscoveryAdapter` with a domain-specific discovery strategy. The base class orchestrates a fallback chain:

1. Explicit configuration (connection strings, env vars)
2. Container host detection (Docker networking)
3. Aspire service discovery
4. Zen Garden topology (SSE tool stream → unified registry)
5. File-based topology cache (cold container startup without HTTP)
6. Multicast/broadcast LAN discovery
7. Localhost fallback

Each candidate is health-checked with domain-specific probes (MongoDB ping, HTTP health, etc.) before acceptance.

**Implementation**: `ServiceDiscoveryAdapterBase`, `MongoDiscoveryAdapter`, `WeaviateDiscoveryAdapter`. Ollama uses custom contributor-based discovery with priority short-circuit.

### 1.4 Wishful Capability Provisioning

When no provider can fully resolve an intent, the system provisions the requested capability:

```csharp
var wish = await ZenGarden.Capability.Wish("ollama", ["model:llama2", "model:nomic-embed-text"]);
// Status: Requested → InProgress → PartiallyFulfilled → Fulfilled
```

The infrastructure adapts to the application's declared needs — pulling models, starting containers, deploying offerings. Progress is reported incrementally via SSE.

**Implementation**: `ZenGardenClient.WishAsync()`, `ZenGardenCapabilityWish`, `ZenGardenCapabilityProgressEvent`.

### 1.5 Live Connection Maintenance via Circuit Breaker

`GardenAwareEndpointManager<TConnection>` subscribes to SSE availability events and maintains connections transparently:

- **Online/Changed**: creates new connection via factory, enters HalfOpen
- **Offline**: opens circuit, blocks operations
- **Successful probe after HalfOpen**: closes circuit
- Endpoint hot-swap when infrastructure moves between nodes

The application never knows connections were migrated.

**Implementation**: `GardenAwareEndpointManager<TConnection>` with `CircuitState` enum (Closed/Open/HalfOpen).

### 1.6 Proven Across Domains

| Domain | Intent | Timing | Discovery Strategy |
|---|---|---|---|
| MongoDB | `zen-garden://mongodb` | Eager (boot) | Autonomous: env → config → container → Aspire → Zen Garden → localhost |
| Weaviate | `zen-garden://weaviate` | Eager (boot) | Autonomous: env → config → container → Aspire → Zen Garden → localhost |
| Ollama | `zen-garden://ollama?cap=model:llama2` | Mixed | Priority: explicit → Zen Garden → parallel local probing |
| S3 Storage | (implicit via garden) | Lazy (runtime) | Zen Garden topology → per-bank port catalog |
| AI Models | `recommended:chat` | Per-request | Recipe → Pin → Scoring (fitness × demand × quality) |

### Claims

**Independent Claim 1 — Intent Handler Resolution Pipeline**: A method for resolving infrastructure service connections comprising: receiving a URI whose scheme identifies an intent handler rather than a network protocol; the URI encoding a desired capability, optional instance selector, and optional sub-capability requirements; dispatching the URI to a resolution pipeline associated with the scheme; passing the parsed intent through an ordered sequence of pluggable resolution providers; each provider independently resolving, enriching, or passing the intent; returning a resolved connection with endpoint, protocol, and reported capabilities; wherein higher-priority providers that cannot resolve cause the intent to pass to the next provider.

**Independent Claim 2 — Reference-As-Intent**: A method for automatic infrastructure intent declaration comprising: a software library that, upon inclusion in a project, registers a binding between an adapter identifier and an infrastructure offering; wherein no explicit configuration is required for the intent to be resolvable; wherein the resolution pipeline automatically discovers and connects through a sequence of providers.

**Independent Claim 3 — Wishful Provisioning**: A method for capability-driven infrastructure provisioning comprising: receiving a capability wish specifying required sub-capabilities; evaluating which are satisfied against current infrastructure state; issuing provisioning requests for missing capabilities; reporting progress incrementally via server-sent events; resolving the endpoint immediately while provisioning proceeds asynchronously.

**Independent Claim 4 — Live Connection Maintenance**: A method for maintaining connections across infrastructure changes comprising: a circuit breaker monitoring health via server-sent events; automatic endpoint migration when infrastructure moves; connection factory re-invocation with new endpoint; three-state circuit driven by availability events; transparent to the consumer application.

### Prior Art Differentiation

| System | Addressing | Intent Handler | Providers | Capabilities | Live Maint. | Ref=Intent | Wishful |
|---|---|---|---|---|---|---|---|
| DNS-SD | Service type | No (network protocol) | Single | No | No | No | No |
| Consul | Service name | No (lookup key) | Single | Tags (static) | Health checks | No | No |
| Kubernetes | Service name | No (DNS alias) | Single | No | Endpoint watch | No | No |
| Aspire | Service name | No (config key) | Single | No | No | Partial | No |
| Dapr | Component | No (sidecar route) | Single | No | No | No | No |
| **This system** | **Handler URI** | **Yes (scheme → pipeline)** | **Layered, pluggable** | **Dynamic, streamed** | **Circuit breaker + SSE** | **Yes** | **Yes** |

---

## Patent Family 2: Hierarchical Capability-Aware AI Model Resolution with Recipes

**The evolved invention.** A 7-level resolution chain where consumers declare *capabilities* (chat, embed, vision), providers layer in model selections, and infrastructure provides automated scoring — with each level authored by a different persona.

### 2.1 The Resolution Chain

```
Level 1: Explicit model on the call         (developer, per-request)
Level 2: Ambient scoped context              (developer, per-code-block)
Level 3: Recipe binding                      (ML engineer / DevOps, from config)
Level 4: Orchestrator advisor                (system, from fitness + demand scoring)
Level 5: Category configuration              (ops, from appsettings.json)
Level 6: Source/member default model         (framework)
Level 7: Hardcoded fallback                  (framework)
```

Each level is authored by a different persona with different knowledge:
- **Developers** (1-2): know their code's needs
- **ML engineers / DevOps** (3): know which models work best for each capability
- **Infrastructure** (4): knows what's available, fast, and under-utilized
- **Operators** (5): know deployment constraints
- **Framework** (6-7): provides safe defaults

### 2.2 Recipes as Versionable Configuration Artifacts

```json
{
  "Koan": {
    "Ai": {
      "ActiveRecipe": "production-balanced",
      "Recipes": {
        "production-balanced": {
          "Chat": "qwen3.5:9b",
          "Embed": "nomic-embed-text",
          "Thinking": "qwq:32b",
          "Quick": "qwen3.5:1.7b"
        },
        "dev-fast": {
          "Chat": "qwen3.5:1.7b"
        }
      }
    }
  }
}
```

Recipes are sparse (omitted capabilities fall through), named (diffable, A/B testable), environment-scoped (via `appsettings.Production.json`), and static (a human's curated selection, not an algorithm).

**Implementation**: `IAiRecipeProvider`, `AiRecipeProvider`, integrated into `AiCategoryRouter.Resolve()`. ADR: AI-0032.

### 2.3 Virtual Model Monikers with Transparent Resolution

Clients specify `"model": "recommended:chat"` in standard Ollama API requests. The proxy:

1. Intercepts the `recommended:` prefix
2. Resolves through pre-computed recommendation cache (which respects pins and recipes)
3. Rewrites the request body with the resolved model name
4. Routes to the optimal stone/instance
5. Adds `X-Zen-Resolved-Model` response header

Works with unmodified clients (ollama CLI, Python SDK, LangChain).

**Implementation**: `proxy.rs::proxy_inference()`, ORCH-0011.

### 2.4 Capability-Specific Fitness Scoring

The recommendation engine scores models per capability with distinct weights:

| Capability | Values Speed | Values Params | Values Context | Name Affinity |
|---|---|---|---|---|
| Quick | High (200 TPS cap) | None | None | None |
| Chat | Medium (50) | Medium (40×B) | Medium (150) | None |
| Thinking | Low (30) | High (60×B) | High (300) | None |
| OCR | Low | Low (15×B) | Medium (150) | +300 for "ocr" in name |
| Synthesis | None | Medium (40×B) | Very High (500) | None |

Scoring uses best-stone-only fitness (not averaged across mediocre hardware).

**Implementation**: `domain/recommendation.rs::recommend()`, ORCH-0003.

### 2.5 Demand-Weighted Topology Optimization

Exponentially-decayed counters (15m/6h/3d windows) track per-capability demand. Demand pressure = demand / capacity. The advisor reshapes model placement to equalize pressure across capabilities, preventing starvation.

Three-stage fitness bootstrap: GPU name heuristic → benchmark → observed.

**Implementation**: `domain/demand.rs::DemandLedger`, `domain/advisor.rs::advise_topology()`, ORCH-0009.

### Claims

**Independent Claim 5 — Hierarchical Multi-Persona Resolution**: A method for resolving AI model selection comprising: a plurality of resolution layers ordered by priority; each layer associated with a distinct authoring persona; graceful degradation from higher to lower priority when a layer cannot satisfy; a capability abstraction where consumers declare what (chat, embed, vision) not which model.

**Independent Claim 6 — Declarative Recipe Provider**: A resolution provider comprising: named, versioned capability-to-model binding configurations; environment scoping for deployment targets; explicit fallback to lower-priority automated providers when bindings are absent; wherein recipe authors need not be the application developers.

**Independent Claim 7 — Virtual Moniker Proxy**: A proxy method for transparent AI model resolution comprising: intercepting requests containing virtual model names; resolving through the hierarchical chain; rewriting request body with resolved model; annotating response with resolution metadata; operating transparently to unmodified client libraries.

**Independent Claim 8 — Capability-Weighted Fitness Scoring**: A method for ranking AI models per capability comprising: distinct scoring weights per capability type; empirical performance data from per-model per-GPU benchmarks; demand pressure equalization using exponentially-decayed counters; best-stone-only scoring preventing false equivalence across mediocre hardware.

---

## Patent Family 3: Unified Tool Registry with Origin-Tracked Write-Through Cache

A single registry holding all infrastructure tools (offerings, gateways, storage) from all sources with origin tracking, TTL-based eviction, cursor-based SSE streaming, and capability-aware queries.

### Key Innovations

- **Origin tracking**: Local (this stone), Announced (peer chirps), Gateway (orchestrator self-registration)
- **TTL-based eviction**: Gateway entries auto-expire on crash (60s lease)
- **Cursor-based SSE**: Delta replay on reconnect via `since` parameter
- **Capability queries**: `?capability=model:llama2,model:mistral` with AND semantics
- **Single beacon type**: Replaces per-domain beacons

**Implementation**: `GardenRegistry` (Moss), `ZenGardenClient` (Koan) with `ConcurrentDictionary<string, ZenGardenToolSnapshot>` mirror.

### Claims

**Independent Claim 9**: A unified service registry comprising: entries from heterogeneous sources (local, announced, registered) with tracked origin; TTL-based eviction for lease-registered entries; cursor-monotonic event stream with delta replay; capability-predicate queries with AND semantics across capability types.

---

## Patent Family 4: Semiotic Infrastructure Presence Protocol

Infrastructure emits domain-semantic events using garden metaphors. Peripheral devices independently interpret events into sensory output.

### Key Innovations

- **Domain-anchored semantics**: `stone.health.thriving`, `service.started`, `storage.degraded` — not metrics
- **Companion-agnostic interpretation**: Same events drive LEDs (Firefly) and audio (Cricket)
- **4-channel spatial audio mixer**: Foreground (alerts), midground (notifications), ambient, background
- **Stone-derived voice profiles**: `hash(stone_name)` → unique timbre per node
- **Stereo panning**: Multi-node spatial awareness
- **Event debouncing**: N simultaneous events → 1 composite sound
- **Rise from silence**: Default state near-silence; activity increases density, not volume

### Claims

**Independent Claim 10**: A method for infrastructure sonification comprising: emitting domain-semantic events from infrastructure nodes; routing events to peripheral companion devices via SSE; companions independently interpreting events into sensory output; per-node voice derivation from deterministic hashing of node identity; spatial audio panning for multi-node awareness.

---

## Patent Family 5: Distributed Storage with Replica Set Identity and Per-Mount S3 Gateway

### Key Innovations

- **Two-level identity**: Physical device ID (immutable) separated from logical replica group ID, with offline rename propagation via timestamps
- **Port-per-storage S3**: Each mount gets a dedicated S3 listener on deterministic port (base 23400 + index)
- **Unified namespace**: S3 objects live at mount root, visible via WebDAV/REST/replication simultaneously
- **503 graceful degradation**: Port stays armed when storage removed
- **Moss-native HMAC presigned tokens**: Not SigV4

### Claims

**Independent Claim 11**: A distributed storage system comprising: two-level identity separating physical device from logical replica group; offline rename propagation with timestamp-based catch-up; per-mount S3 gateway with deterministic port assignment; unified namespace visible across S3, WebDAV, and REST interfaces.

---

## Patent Family 6: Multicast-First LAN Discovery with Fallback Tiers

### Key Innovations

- **Per-interface socket binding**: One socket per eligible NIC, solving multi-homed Windows (WSL/Hyper-V)
- **Systematic fallback**: Multicast → directed broadcast (computed from IP+prefix) → limited broadcast
- **Shared topology directory**: Dual-file ownership (`garden-topology.json` Moss-writes, `garden-stones.json` clients-write)
- **Container cold-start**: File-based topology enables container startup without HTTP dependency

### Claims

**Independent Claim 12**: A method for LAN service discovery on multi-homed hosts comprising: creating per-interface UDP sockets bound to specific NIC addresses; primary multicast with configured TTL; fallback to directed broadcast computed from interface IP and prefix length; final fallback to limited broadcast; file-based topology cache enabling container cold-start without network dependency.

---

## Filing Strategy

### Recommended Filing Order

| Priority | Family | Novelty | Defensibility | Prior Art Gap |
|---|---|---|---|---|
| 1 | **Intent-Based Resolution** (F1) | Extremely high | Very strong | No comparable system exists |
| 2 | **Hierarchical AI Model Resolution with Recipes** (F2) | Very high | Strong | No multi-persona layered resolution |
| 3 | **Semiotic Presence Protocol** (F4) | Very high | Moderate | No known prior art in infra monitoring |
| 4 | **Unified Tool Registry** (F3) | High | Moderate | Origin tracking + cursor SSE novel |
| 5 | **Distributed Storage / S3 Gateway** (F5) | High | Strong | Per-mount S3 + unified namespace novel |
| 6 | **Multicast-First Discovery** (F6) | Moderate | Moderate | Per-interface binding is novel |

### Strongest Combined Filing

Families 1 and 2 should be filed together as a single application with continuation claims. The intent-resolution pipeline (F1) is the general system; the AI model resolution with recipes (F2) is the most sophisticated instantiation. Together they demonstrate both breadth (any infrastructure type) and depth (7-level chain with fitness scoring, demand weighting, and virtual monikers).

Family 4 (Semiotic Presence) is sufficiently distinct for an independent filing in the ambient/peripheral computing space.

---

## Implementation References

| Concept | Zen Garden (Rust) | Koan Framework (.NET) |
|---|---|---|
| Intent URI | — | `ZenGardenConnectionIntent.cs` |
| Offering binding | — | `IZenGardenOfferingBinding` |
| Discovery adapter | — | `IServiceDiscoveryAdapter`, `ServiceDiscoveryAdapterBase` |
| Unified registry | `garden_registry.rs` | `ZenGardenClient.cs` (mirror) |
| Tool stream | `api/v1/tools.rs` | `ZenGardenClient` SSE consumer |
| Model monikers | `api/proxy.rs` | `ZenGardenModelAdvisor.cs` |
| Fitness scoring | `domain/recommendation.rs` | — (consumed via `/v1/recommendations`) |
| Demand tracking | `domain/demand.rs` | — (infrastructure-side) |
| Recipe provider | — | `IAiRecipeProvider`, `AiRecipeProvider` |
| Resolution chain | — | `AiCategoryRouter.Resolve()` |
| Wishful provisioning | `api/v1/tools.rs` | `ZenGardenClient.WishAsync()` |
| Circuit breaker | — | `GardenAwareEndpointManager<T>` |
| Presence protocol | `api/v1/presence.rs` | — (companion-side) |
| Audio companion | `cricket/src/mixer.rs` | — |
| Storage identity | `domain/storage.rs` | — |
| S3 gateway | `domain/s3.rs` | `Koan.Storage.S3` connector |
| Multicast discovery | `infra/communications/p2p.rs` | `KoiHandler.cs` (mDNS bridge) |
