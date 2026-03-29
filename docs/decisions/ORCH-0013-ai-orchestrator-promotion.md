---
audience: [developer, ai]
doc_type: decision
status: proposed
last_verified: 2026-03-27
---

# ORCH-0013: Promote Ollama Orchestrator to General-Purpose AI Orchestrator

**Date**: 2026-03-27
**Status**: Proposed
**Applies to**: `zen-garden-ai-orchestrator` crate (new)
**Depends on**: ORCH-0002 (Routing), ORCH-0003 (Fitness), ORCH-0009 (Demand-Weighted Advisor), ORCH-0010 (Extended Capabilities), ORCH-0011 (Recommended Monikers)
**See also**: ORCH-0012 (Cluster Adapter Extraction — similar adapter pattern for stateful database orchestrators)

---

## Context

The Ollama Orchestrator is a fully functional, VRAM-aware, fitness-scored,
demand-weighted router for Ollama instances across a Zen Garden deployment.
It handles multi-instance discovery, GPU tiering, fitness benchmarking,
exponential-decay demand tracking, capability-aware model recommendations,
model lifecycle management, NDJSON proxy streaming, and a real-time dashboard.

This is sophisticated infrastructure — but it only serves Ollama.

A Zen Garden deployment increasingly runs multiple AI service types: Ollama
for LLM inference, ComfyUI for image generation, Speaches/whisper.cpp for
speech-to-text, OpenedAI Speech for text-to-speech, Infinity for embeddings
and reranking, LibreTranslate for translation, and cloud providers as
fallbacks. Each of these services needs the same operational intelligence
the Ollama orchestrator already provides: discovery, routing, health
monitoring, fitness profiling, demand tracking, and capability-based
recommendations.

Building a separate orchestrator per AI service type would duplicate the
routing, demand, fitness, placement, and metrics infrastructure — the same
problem ORCH-0012 identified for database orchestrators. The correct
abstraction is a single AI orchestrator that manages all AI service types
through an offering adapter pattern, where each service type implements a
trait contract and the domain logic operates on generic service instances.

---

## Investigation Findings

### Ollama Orchestrator Code Assessment

The existing Ollama orchestrator (~7,900 LOC across 37 files) was assessed
module by module. Each public type, function, and struct was classified as
shared domain, shared infra, Ollama adapter, or needs generalization.

**Classification summary:**

| Category | LOC | % | Description |
|----------|-----|---|-------------|
| Shared Domain | ~2,300 | 29% | Pure business logic: routing, demand, fitness, placement, advisor, recommendation, tiering, reconciliation, metrics, lease, policy, GPU catalog |
| Shared Infra | ~3,500 | 45% | Reusable I/O: Koi/Moss discovery, gateway registration, persistence, SSE events, stone discovery, background task loops |
| Ollama Adapter | ~600 | 8% | Irreducibly Ollama-specific: HTTP client for `/api/tags`, `/api/ps`, `/api/show`, NDJSON streaming, model pull protocol, benchmark prompts |
| Needs Generalization | ~1,400 | 18% | Currently Ollama-flavored but pattern is generic: `OllamaInstance` type, proxy handler, management endpoints, benchmark task runner |

**Domain layer purity:** The domain layer (~2,300 LOC, 14 modules) is
completely free of I/O — no async, no HTTP, no filesystem. All decision
logic operates on plain data structures. This is the strongest foundation
for generalization: the algorithms (routing, placement, demand decay,
fitness scoring, recommendation ranking) are already generic in behavior,
only Ollama-specific in their type names.

**Key generalization points:**

1. `OllamaInstance` becomes `ServiceInstance` — add `offering_type`
   discriminator, keep VRAM/GPU/compute_type fields, move Ollama-specific
   fields (`ollama_version`) into offering metadata.

2. `RequestCapability` / `fitness::Capability` unify into a single
   `Capability` enum extended with new variants (Imagine, Transcribe,
   Speak, Rerank, Translate).

3. `OllamaClient` methods (probe, enumerate, proxy, benchmark) become
   the `Offering` trait — each service type implements its own client.

4. Proxy handler extracts capability inference and response streaming
   into trait-dispatched methods.

5. Benchmark task runner parameterizes test payloads and verdict
   thresholds per offering type.

### Moss Content Sync Capabilities

Moss has production-ready infrastructure for syncing content between stones:

**What exists today:**

| Capability | Status | Mechanism |
|-----------|--------|-----------|
| Storage bank replication | Production | Changelog-based Primary/Dormant sync with cursor tracking, 60s polling + event-driven, full reconciliation fallback |
| Offering snapshots | Production | `create_harvest()` commits container image + archives volumes to `.tar.gz` with SHA-256 checksums |
| Snapshot replication to seed banks | Production | Nurturing scheduler routes snapshots to seed banks (First/MostCapacity/All strategies) |
| S3 gateway with auto-replication | Production | S3 writes generate changelog entries, auto-replicated to Dormant replicas |
| Inter-stone communication | Production | StoneClient with mTLS (pond) or HTTP fallback, transparent proxying |

**What does not exist:**

| Gap | Impact |
|-----|--------|
| Offering-to-offering sync | Cannot sync model files between two stones running the same offering |
| Offering migration/relocation | Snapshots are backup-only, no automated restore-on-target |
| Resource sync for non-Ollama offerings | ComfyUI checkpoints, whisper models have no pull mechanism |

**Sync strategy for the AI orchestrator:** For offerings with native pull
mechanisms (Ollama: `POST /api/pull`), the orchestrator uses the native
protocol. For offerings without native pull (ComfyUI checkpoints, whisper
models), the orchestrator leverages Moss storage banks as transport: write
resources to a bank on the source stone, replication carries them to other
stones, the offering reads from the local replica. This avoids the
orchestrator implementing direct file transfer and reuses proven
infrastructure.

A Moss-level investigation is queued to evaluate extending the offering
snapshot/harvest system for active sync (not just backup), which would
provide a uniform transport for all offering types.

---

## Decision

Build a single AI orchestrator binary (`zen-garden-ai-orchestrator`) that
manages all AI service types through an offering adapter pattern. The
existing Ollama orchestrator's domain logic, infrastructure, and task
patterns are carefully unbundled into shared layers and an Ollama adapter.
New offerings (ComfyUI, Speaches, etc.) are additional adapters
implementing the same trait contract.

### Design Principles

1. **Single binary, single deployment unit.** One Docker image, one
   config, one dashboard. Per-service proxy ports in a dedicated range
   (21434+) — each port speaks the native protocol of its service so
   existing clients work without reconfiguration. Operators deploy one
   container that replaces multiple individual service proxies.

2. **Offering adapter pattern.** Each AI service type implements a trait.
   Adding a new service = implementing the trait + registering in the
   catalog. Zero changes to domain, tasks, or API layers.

3. **No regressions.** The Ollama orchestrator is fully functional. Every
   feature — routing, demand tracking, fitness profiling, placement,
   model sync, proxy streaming, dashboard — must work identically in the
   new orchestrator for Ollama workloads.

4. **Same depth for every offering.** Each new offering gets the same
   operational depth as Ollama: discovery, health monitoring, fitness
   benchmarking, demand tracking, model/resource enumeration, proxy
   dispatch, and model sync.

5. **Proper separation of concerns.** The offerings catalog (Moss-level)
   defines how to get a service running — deploy, adopt, configure. The
   AI orchestrator defines how to use running services — discover, route,
   benchmark, proxy, sync resources. Adopted instances are first-class
   citizens, same as deployed ones.

6. **Domain purity.** The domain layer has zero I/O, zero async. All
   algorithms operate on plain data. This is non-negotiable.

### Offering Trait

The core abstraction — the contract between the shared orchestrator and
each service type:

```rust
/// A type of AI service the orchestrator can discover, probe, and route to.
///
/// Each offering implementation encapsulates all service-specific protocol
/// knowledge: HTTP endpoints, response shapes, streaming formats, model
/// management commands. The orchestrator's domain layer never sees these
/// details — it operates on `ServiceInstance` and `Capability` exclusively.
///
/// Methods that perform I/O return `BoxFuture` rather than using `async fn`
/// so that the trait is object-safe (`dyn Offering`). The project removed
/// `async-trait` in ARCH-0007; boxed futures are the explicit replacement
/// for dyn-compatible async methods.
pub trait Offering: Send + Sync {
    /// Unique type identifier: "ollama", "comfyui", "speaches", etc.
    fn offering_type(&self) -> OfferingKind;

    /// AI capabilities this offering type can provide.
    fn capabilities(&self) -> &[Capability];

    /// How to discover instances (port probe, topology filter, configured).
    fn discovery_config(&self) -> DiscoveryConfig;

    /// Probe an endpoint for liveness. Returns service metadata if healthy.
    fn probe(&self, endpoint: &str) -> BoxFuture<'_, Result<ProbeResult>>;

    /// Enumerate available models/resources on a live instance.
    fn enumerate(&self, endpoint: &str) -> BoxFuture<'_, Result<Vec<ServiceModel>>>;

    /// Estimate VRAM consumption for a model on this offering.
    /// Static estimate from model metadata — not a live query.
    /// Real-time VRAM data comes from `probe()` and is cached in
    /// `ServiceInstance.vram.free_bytes`.
    fn vram_estimate(&self, model: &ServiceModel) -> Option<u64>;

    /// Forward a capability request to the instance's native API.
    fn proxy(
        &self,
        endpoint: &str,
        capability: Capability,
        request: ProxyRequest,
    ) -> BoxFuture<'_, Result<ProxyResponse>>;

    /// Benchmark a capability on an instance.
    fn benchmark(
        &self,
        endpoint: &str,
        capability: Capability,
    ) -> BoxFuture<'_, Result<BenchmarkSample>>;

    /// Sync a resource from one instance to another.
    /// Ollama: calls POST /api/pull on target.
    /// ComfyUI: transfers checkpoint via storage bank.
    fn sync_resource(
        &self,
        resource: &str,
        from: &ServiceInstance,
        to: &ServiceInstance,
    ) -> BoxFuture<'_, Result<SyncProgress>>;
}
```

`BoxFuture<'_, T>` is `Pin<Box<dyn Future<Output = T> + Send + '_>>`.
The `OfferingRegistry` stores `Arc<dyn Offering>` for runtime dispatch
over heterogeneous offering types.

The trait lives in `catalog/` — outside `domain/` because it performs
I/O. Implementations live in `offerings/{type}/`. The domain layer never
imports `catalog/` or `offerings/` — it operates on the data types those
layers produce.

### Trait Support Types

Types used in the `Offering` trait contract, defined in `catalog/`:

```rust
/// How the orchestrator discovers instances of this offering type.
pub enum DiscoveryConfig {
    /// Probe a well-known port on discovered stones.
    PortProbe { default_port: u16 },
    /// Filter Moss topology by offering name.
    TopologyFilter { offering_name: String },
    /// Manually configured endpoint (cloud providers, HuggingFace).
    Configured,
}

/// Result of a successful health probe.
pub struct ProbeResult {
    /// Service version string (e.g., "0.9.1" for Ollama).
    pub version: Option<String>,
    /// Capabilities confirmed by this specific instance.
    pub capabilities: Vec<Capability>,
    /// Real-time VRAM free bytes (ComfyUI provides this; Ollama does not).
    pub vram_free_bytes: Option<u64>,
    /// Offering-specific metadata (opaque to domain).
    pub metadata: serde_json::Value,
}

/// A model or resource available on a service instance.
pub struct ServiceModel {
    /// Model identifier (e.g., "llama3.2:3b", "flux-dev.safetensors").
    pub name: String,
    /// Capabilities this specific model supports.
    pub capabilities: Vec<Capability>,
    /// VRAM consumption when loaded (bytes). None if unknown.
    pub vram_bytes: Option<u64>,
    /// Offering-specific model metadata (param count, quant level, etc.).
    pub metadata: serde_json::Value,
}

/// Proxy response — the raw HTTP response from the offering instance.
/// The proxy handler forwards this directly to the client. Each offering
/// produces the correct content type (NDJSON stream for Ollama, image
/// bytes for ComfyUI, audio bytes for Speak/Transcribe, JSON for others).
pub struct ProxyResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: ProxyBody,
}

/// Proxy body — either a complete buffer or a byte stream.
pub enum ProxyBody {
    /// Complete response (JSON, image bytes, audio bytes).
    Complete(Vec<u8>),
    /// Streaming response (Ollama NDJSON, SSE progress).
    Stream(Pin<Box<dyn Stream<Item = Result<Bytes>> + Send>>),
}

/// A single benchmark measurement for one capability on one instance.
pub struct BenchmarkSample {
    /// Raw timing samples (one per test prompt/input).
    pub samples: Vec<Sample>,
    /// Capability tested.
    pub capability: Capability,
}

/// Progress/completion of a resource sync operation.
pub enum SyncProgress {
    /// Sync completed successfully.
    Completed { bytes_transferred: u64 },
    /// Sync is in progress (for streaming progress updates).
    InProgress {
        bytes_transferred: u64,
        total_bytes: Option<u64>,
    },
    /// Sync failed.
    Failed { reason: String },
}
```

### Capability Enum

The existing Ollama orchestrator has two separate capability enums with
different variant names. The AI orchestrator unifies them into a single
`Capability` enum. The migration table below maps old variants to new:

**Migration from existing enums:**

| Old enum | Old variant | New `Capability` variant | Notes |
|----------|------------|-------------------------|-------|
| `fitness::Capability` | `Generate` | `Generate` | Kept — distinct from `Chat` (fitness benchmarks raw generation, not chat completion) |
| `fitness::Capability` | `Embed` | `Embed` | Kept |
| `fitness::Capability` | `Vision` | `Vision` | Kept |
| `fitness::Capability` | `Tools` | `Tools` | Kept |
| `fitness::Capability` | `Think` | `Think` | Kept (not renamed to `Thinking` — shorter is idiomatic) |
| `demand::RequestCapability` | `Chat` | `Chat` | Kept — demand tracks chat as a request type (includes generate) |
| `demand::RequestCapability` | `Embedding` | `Embed` | **Renamed** to `Embed` for consistency with fitness enum |
| `demand::RequestCapability` | `Vision` | `Vision` | Kept |
| `demand::RequestCapability` | `Tools` | `Tools` | Kept |
| `demand::RequestCapability` | `Thinking` | `Think` | **Renamed** to `Think` for consistency with fitness enum |

**Design choice:** `Generate` and `Chat` both exist. `Generate` is the
fitness/benchmark concept (raw token generation speed). `Chat` is the
demand/routing concept (conversational request type). In fitness scoring,
a `Chat` request maps to `Generate` benchmark thresholds. This mirrors
the existing Ollama orchestrator where `demand::RequestCapability::Chat`
maps to `fitness::Capability::Generate`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    // Text/LLM — existing Ollama orchestrator variants (names preserved)
    Generate,       // raw token generation (fitness/benchmark concept)
    Chat,           // conversational request (demand/routing concept)
    Embed,          // text -> vector (renamed from demand::Embedding)
    Vision,         // image + text -> text
    Tools,          // structured tool-calling
    Think,          // sustained long-generation (renamed from demand::Thinking)

    // Generation (new)
    Imagine,        // text -> image
    Edit,           // image + instruction -> image
    Render,         // text -> video

    // Audio (new)
    Transcribe,     // audio -> text
    Speak,          // text -> audio

    // Search/Retrieval (new)
    Rerank,         // query + docs -> scored docs

    // Language (new)
    Translate,      // text + target -> text
}
```

**Fitness threshold mapping:** Existing `Verdict::compute()` branches on
`Capability::Generate`, `Capability::Think`, etc. These variant names are
preserved exactly — no threshold logic changes. New capabilities
(`Imagine`, `Transcribe`, `Speak`, `Rerank`, `Translate`) get their own
threshold definitions in each offering's benchmark implementation.

**Embed capability disambiguation:** Both Ollama and Infinity claim
`Embed`. Routing disambiguates by model name: if the requested model
exists on an Ollama instance, route there; if it exists on Infinity,
route there. If the same model name exists on both (unlikely — different
model ecosystems), priority breaks the tie (both at 0 — first discovered
wins). The `recommended:embed` moniker resolves to the highest-scored
model across all offerings, which naturally selects the best embedding
instance regardless of offering type.

### ServiceInstance (Generalized)

Replaces `OllamaInstance`. Field naming follows code standards: struct
nesting for namespaces (SS1), no type-in-name (SS2), value objects for
identity (SS7).

```rust
pub struct ServiceInstance {
    // Identity — Stone value object (code standard §7)
    pub stone: Stone,
    pub endpoint: String,
    pub kind: OfferingKind,

    // Hardware
    pub gpu: Gpu,
    pub vram: Vram,

    // Service state
    pub health: InstanceHealth,
    pub models_available: Vec<String>,
    pub models_loaded: Vec<LoadedModel>,
    pub capabilities: Vec<Capability>,
    pub queue_depth: u32,
    pub last_seen: Instant,

    // Offering-specific metadata (opaque to routing)
    pub metadata: serde_json::Value,

    // Priority
    pub priority: i32,
}

/// Offering type discriminator — enum, not String (code standard §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferingKind {
    Ollama,
    ComfyUi,
    Speaches,
    OpenedaiSpeech,
    Infinity,
    LibreTranslate,
    HuggingFace,
    // Cloud providers
    OpenAi,
    Anthropic,
    StabilityAi,
    ElevenLabs,
    Cohere,
    Deepgram,
    Google,
}

/// GPU identity (code standard §1 — namespace, not prefix).
pub struct Gpu {
    pub name: Option<String>,
    pub compute: ComputeType,
}

/// VRAM state (code standard §1 — namespace, not prefix).
pub struct Vram {
    pub total_bytes: u64,
    pub budget_bytes: u64,
    pub free_bytes: Option<u64>,
}

/// Stone identity (reuse from garden_common per code standard §7).
pub struct Stone {
    pub id: String,
    pub name: String,
}
```

### Cross-Offering VRAM Accounting

When multiple offerings share a GPU on the same stone, the routing engine
needs an aggregate VRAM view. A per-stone VRAM summary is assembled by
the tasks layer and fed to the domain routing function:

```rust
/// Aggregate VRAM state for one stone across all offerings.
pub struct StoneVramBudget {
    pub stone_id: String,
    pub total_bytes: u64,
    pub used_bytes: u64,         // sum of all offerings' loaded model VRAM
    pub free_bytes: u64,         // total - used
    pub per_offering: Vec<OfferingVramUsage>,
}

pub struct OfferingVramUsage {
    pub kind: OfferingKind,
    pub used_bytes: u64,
    pub model_count: usize,
}
```

The `StoneVramBudget` is recomputed whenever any instance on a stone
reports updated VRAM state (via `probe()` or `enumerate()`). The routing
function `select_instance` receives `&[StoneVramBudget]` alongside the
candidate list, using `free_bytes` for headroom calculations instead of
per-instance VRAM fields alone.

### Priority Model

Priority replaces the concept of locality-only tiers:

| Priority | Meaning | Source |
|----------|---------|--------|
| `+10` | Operator-pinned (explicit preference) | Pin override in dashboard |
| `0` | Default (local + garden instances) | Auto-discovered or configured |
| `-10` | Cloud fallback | Auto-registered from API key |
| `-20` | Emergency/expensive cloud | Operator-configured |

### Routing Engine (Extended)

The existing `routing::select_instance` algorithm gains three extensions:

1. **Filter by capability.** Before filtering by model availability,
   filter by offering capability. A `Transcribe` request only considers
   Speaches/whisper.cpp instances.

2. **Priority as primary sort.** Within non-degraded candidates, sort by
   priority descending before fitness. Cloud providers (priority -10)
   are only selected when no local instance serves the capability.

3. **Cross-offering VRAM awareness.** When two offerings share a GPU
   (Ollama + ComfyUI on the same stone), the routing engine considers
   combined VRAM pressure from all offerings on that stone.

Resolution flow:

```
1. Parse request -> extract capability + optional model name
2. If model is "recommended:{cap}" -> resolve via recommendation engine
3. Filter instances by:
   a. Capability match (offering supports this capability)
   b. Model match (if specified, instance has it)
   c. Health (routable instances only)
   d. Fitness (exclude Blocked verdicts)
4. Priority gate (RT-4 enforcement):
   If any candidate has priority >= 0, EXCLUDE all candidates with
   priority < 0. Cloud providers are filtered out entirely when any
   local/garden instance can serve the request — not merely sorted lower.
5. Sort remaining candidates by:
   a. Priority (descending)
   b. Idle preference (queue_depth == 0 first)
   c. Fitness score (descending)
   d. VRAM headroom from StoneVramBudget (descending)
   e. Queue depth (ascending)
6. Demand reservation check (existing)
7. Pick first candidate under max_queue
8. Return RoutingDecision with target + offering_type
```

### Cloud Providers

Cloud providers implement the `Offering` trait at priority -10:

- **Discovery:** Configured via dashboard (API key + endpoint). Never
  auto-discovered.
- **Enumeration:** Call provider's model list API. Register each model.
- **Proxy:** Forward with `Authorization: Bearer {key}`. Provider-specific
  API format.
- **Health:** Periodic API ping to verify key validity. Auto-disable after
  3 consecutive failures (same circuit breaker as local instances).
- **VRAM:** Not applicable.
- **Benchmarking:** Latency measurement only.
- **API keys:** Encrypted at rest in `{data_dir}/providers.enc`.

**Provider capability matrix:**

| Provider | Chat | Embed | Imagine | Transcribe | Speak | Rerank |
|----------|------|-------|---------|-----------|-------|--------|
| OpenAI | Y | Y | Y | Y | Y | - |
| Anthropic | Y | - | - | - | - | - |
| Stability AI | - | - | Y | - | - | - |
| ElevenLabs | - | - | - | - | Y | - |
| Cohere | Y | Y | - | - | - | Y |
| Deepgram | - | - | - | Y | Y | - |
| Google | Y | Y | Y | - | Y | - |
| HuggingFace | Y | Y | Y | Y | - | - |

### Crate Structure

```
src/orchestrators/ai/
├── Cargo.toml
├── Dockerfile
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── app_state.rs               # Thin facade (§14): re-exports domain contexts
│   │
│   ├── catalog/                   # Offering abstraction boundary
│   │   ├── mod.rs
│   │   ├── traits.rs              # Offering trait definition
│   │   ├── registry.rs            # OfferingRegistry
│   │   └── cloud.rs               # Cloud provider base
│   │
│   ├── offerings/                 # Per-offering adapters
│   │   ├── mod.rs
│   │   ├── ollama/                # Ollama adapter (~600 LOC moved)
│   │   │   ├── mod.rs
│   │   │   ├── client.rs          # Ollama HTTP client
│   │   │   ├── proxy.rs           # NDJSON streaming, /api/* dispatch
│   │   │   ├── benchmark.rs       # Ollama-specific test payloads
│   │   │   └── types.rs           # OllamaTagsResponse, etc.
│   │   ├── comfyui/               # ComfyUI adapter (new)
│   │   │   ├── mod.rs
│   │   │   ├── client.rs          # HTTP + WebSocket client
│   │   │   ├── proxy.rs           # Workflow dispatch + image fetch
│   │   │   ├── templates.rs       # Parameterized workflow templates
│   │   │   └── types.rs
│   │   ├── speaches/              # Speaches/whisper.cpp adapter (new)
│   │   │   ├── mod.rs
│   │   │   ├── client.rs
│   │   │   └── proxy.rs
│   │   ├── openedai_speech/       # TTS adapter (new)
│   │   │   ├── mod.rs
│   │   │   ├── client.rs
│   │   │   └── proxy.rs
│   │   ├── infinity/              # Embedding + reranking adapter (new)
│   │   │   ├── mod.rs
│   │   │   ├── client.rs
│   │   │   └── proxy.rs
│   │   ├── libretranslate/        # Translation adapter (new)
│   │   │   ├── mod.rs
│   │   │   ├── client.rs
│   │   │   └── proxy.rs
│   │   ├── huggingface/           # HF Inference adapter (new)
│   │   │   ├── mod.rs
│   │   │   ├── client.rs
│   │   │   └── proxy.rs
│   │   └── cloud/                 # Cloud provider adapters (new)
│   │       ├── mod.rs
│   │       ├── openai.rs
│   │       ├── anthropic.rs
│   │       └── provider.rs        # Generic OpenAI-compatible base
│   │
│   ├── domain/                    # Pure domain (zero I/O)
│   │   ├── mod.rs
│   │   ├── types.rs               # ServiceInstance, Capability, etc.
│   │   ├── routing.rs             # select_instance (capability + priority)
│   │   ├── demand.rs              # DemandLedger (extended capabilities)
│   │   ├── fitness.rs             # GpuMatrix, Verdict (extended caps)
│   │   ├── advisor.rs             # TopologyAdvice (multi-offering)
│   │   ├── recommendation.rs      # Per-capability model ranking
│   │   ├── tiering.rs             # Tier computation
│   │   ├── placement.rs           # Demand-weighted bin-packing
│   │   ├── lease.rs               # High-VRAM reservation
│   │   ├── metrics.rs             # MetricsEngine
│   │   ├── policy.rs              # Sync/delete decisions
│   │   ├── reconciliation.rs      # Drift detection
│   │   ├── gpu_catalog.rs         # GPU score lookup
│   │   └── pins.rs                # Model/capability pin overrides
│   │
│   ├── infra/                     # Shared I/O
│   │   ├── mod.rs
│   │   ├── gateway.rs             # Koi mDNS + Moss gateway
│   │   ├── persistence.rs         # TOML config + JSON metrics
│   │   ├── stone_discovery.rs     # Koi SSE + topology queries
│   │   ├── events.rs              # SSE broadcast
│   │   ├── tools_stream.rs        # Moss Tools API SSE
│   │   └── cloud_store.rs         # Encrypted API key storage (new)
│   │
│   ├── tasks/                     # Background tasks
│   │   ├── mod.rs
│   │   ├── discovery.rs           # Multi-offering discovery loop
│   │   ├── health_check.rs        # Per-offering health via probe()
│   │   ├── reconciliation.rs      # Drift detection via enumerate()
│   │   ├── resource_sync.rs       # Per-offering sync via sync_resource()
│   │   ├── placement.rs           # Cross-offering placement
│   │   ├── advisor.rs             # Multi-offering topology advisor
│   │   ├── benchmark.rs           # Per-offering fitness via benchmark()
│   │   ├── cloud_sync.rs          # Cloud provider model refresh (new)
│   │   ├── metrics_processor.rs   # Demand ledger feeder
│   │   ├── metrics_flush.rs       # Persistence flush
│   │   ├── snapshot_publisher.rs  # Dashboard snapshot
│   │   └── gateway_announce.rs    # mDNS + gateway lifecycle
│   │
│   └── api/                       # HTTP handlers
│       ├── mod.rs
│       ├── proxy.rs               # Unified proxy (dispatches to offering)
│       ├── extension.rs           # /v1/ API (capabilities, models, stones)
│       ├── management.rs          # Per-offering model management
│       ├── dashboard.rs           # Dashboard SPA + status
│       ├── health.rs              # Liveness probe
│       ├── benchmark_api.rs       # Fitness profiling API
│       └── providers_api.rs       # Cloud provider CRUD (new)
│
├── workflows/                     # ComfyUI workflow templates
│   ├── txt2img.json
│   ├── img2img.json
│   └── inpaint.json
│
├── exercise.ps1
└── docs/
```

### API Surface

**Per-service proxy ports** (each speaks the service's native protocol):

| Port  | Service | Protocol |
|-------|---------|----------|
| 21434 | Ollama | Ollama API (OpenAI-compat) |
| 21435 | ComfyUI | Custom workflow + WebSocket |
| 21436 | whisper.cpp | Multipart `/inference` |
| 21437 | Speaches | OpenAI `/v1/audio/*` |
| 21438 | OpenedAI Speech | OpenAI `/v1/audio/speech` |
| 21439 | Infinity | OpenAI `/embeddings` + `/rerank` |
| 21440 | LibreTranslate | Custom `/translate` |

Port assignments are in the orchestrator's own range (21434+) to
avoid collision with native service ports on the same stone. Exact
assignments finalized during implementation.

**Ollama proxy port** (`:21434`) — full Ollama API compatibility:

```
# Ollama-native (preserved, no regressions)
POST /api/generate            -> Ollama proxy
POST /api/chat                -> Ollama proxy
POST /api/embed               -> Ollama proxy
GET  /api/tags                -> Merged model list
GET  /api/ps                  -> Merged loaded models
POST /api/show                -> Model detail
POST /api/pull                -> Model pull
DELETE /api/delete             -> Model delete
GET  /api/version             -> Orchestrator version
GET  /                        -> "Ollama is running" (client compat)

# Extension API (shared across all ports via dashboard)
GET  /v1/models               -> All models across all offerings
GET  /v1/stones               -> All stones with offering details
GET  /v1/capabilities         -> Available capabilities + serving offerings
GET  /v1/recommendations      -> Per-capability model recommendations
PUT  /v1/recommendations/{cap}/pin
DELETE /v1/recommendations/{cap}/pin
```

**Dashboard server** (`:7190`):

```
# Dashboard (capability-centric + per-offering detail pages)
GET  /                        -> Dashboard SPA
GET  /api/status              -> Full snapshot (all offerings, all stones)
GET  /api/events              -> SSE stream
GET  /api/settings            -> Config
POST /api/settings            -> Update config
GET  /api/offerings           -> Offering catalog + per-offering instances

# Cloud provider management
GET  /api/providers           -> Registered cloud providers
POST /api/providers           -> Add provider (API key + config)
PUT  /api/providers/{id}      -> Update provider
DELETE /api/providers/{id}    -> Remove provider

# Existing (preserved)
GET  /api/jobs
POST /api/management/pull
POST /api/management/delete
GET  /api/management/feasibility
POST /api/benchmark/start
POST /api/benchmark/cancel
GET  /api/benchmark/results
GET  /api/benchmark/export
POST /api/metrics/reset
POST /api/metrics/model-counters/reset
GET  /health
```

### Dashboard Architecture

The dashboard is capability-centric as its primary view, with per-offering
detail pages for offering-specific management:

**Capability overview** (primary): What can this garden do? Shows all
capabilities (Chat, Imagine, Transcribe, etc.) with which offerings and
instances serve each one. This matches the routing model — the
orchestrator thinks in capabilities, so the dashboard does too.

**Per-offering pages** (drill-down): Offering-specific management UI.
Ollama: model pull/delete, VRAM allocation, benchmark results. ComfyUI:
workflow template management, checkpoint inventory. Cloud providers: API
key configuration, model enumeration, usage limits.

### Resource Sync Architecture

Every offering manages its own sync mechanism through the trait's
`sync_resource()` method:

**Ollama:** Uses native `POST /api/pull` on the target instance. The
orchestrator triggers the pull and monitors progress via the NDJSON
stream. This is the existing model_sync behavior, unchanged.

**ComfyUI:** No native pull mechanism. The orchestrator uses Moss storage
banks as transport:
1. Source stone: read checkpoint file, write to storage bank via S3 API
2. Moss replication: changelog carries the file to Dormant replicas
3. Target stone: offering reads checkpoint from local bank replica
4. Orchestrator configures ComfyUI's model directory to include the bank
   mount path

**Speaches/whisper.cpp:** Same pattern as ComfyUI — whisper model files
(400MB-3GB) synced via storage bank transport.

**Cloud providers:** No sync needed — the cloud manages its own resources.

A Moss-level investigation is planned to evaluate extending the offering
snapshot/harvest system for active resource sync, which would provide a
uniform transport for all non-native-pull offerings.

---

## Offering Specifications

### Ollama

| Aspect | Detail |
|--------|--------|
| **Offering type** | `ollama` |
| **Capabilities** | Chat, Embed, Vision, Tools, Thinking |
| **Discovery** | Koi mDNS (`_moss._tcp`) + topology + Tools API SSE. Filter `offering:ollama` |
| **Health probe** | `GET /` -> "Ollama is running" (200 OK) |
| **Enumeration** | `GET /api/tags` (models), `POST /api/show` (detail), `GET /api/ps` (loaded + VRAM) |
| **VRAM tracking** | Authoritative from `/api/ps` (`size_vram`). Projected from disk size x 1.1 when not loaded |
| **Proxy** | Forward `/api/generate`, `/api/chat`, `/api/embed`. NDJSON streaming with metrics extraction |
| **Model management** | Pull (`POST /api/pull`), delete, load (empty-prompt trick), unload (`keep_alive:0`) |
| **Benchmarking** | Generate (5 prompts), Vision (3 images), Tools (100-distractor correctness), Thinking (2 long-gen), Embed (3 inputs) |
| **Recommendation** | 4-layer scoring: availability, fitness, context window, quality (param count) |
| **Resource sync** | Native `POST /api/pull` + tier-peer replication |

### ComfyUI

| Aspect | Detail |
|--------|--------|
| **Offering type** | `comfyui` |
| **Capabilities** | Imagine, Edit, Render |
| **Discovery** | Port probe (`:8188`). Topology filter `offering:comfyui` |
| **Health probe** | `GET /system_stats` -> 200 with `system` and `devices` |
| **Enumeration** | `GET /models/checkpoints`, `GET /models/loras`, `GET /object_info/CheckpointLoaderSimple` |
| **VRAM tracking** | `GET /system_stats` -> `devices[*].vram_total` and `vram_free` (real-time) |
| **Proxy** | Workflow template dispatch: parameterize template, `POST /prompt`, monitor WebSocket, fetch output image |
| **WebSocket** | Per-request connect (upgrade to persistent pool if latency proves significant) |
| **Template validation** | Discovery-time: validate against `GET /object_info` (cache). Dispatch-time: safety gate from cache |
| **Benchmarking** | Reference txt2img (512x512, 20 steps). Fast < 15s, Degraded < 60s, Vetoed < 180s |
| **Resource sync** | Storage bank transport (checkpoint files 2-12GB) |

### Speaches / whisper.cpp

| Aspect | Detail |
|--------|--------|
| **Offering type** | `speaches` |
| **Capabilities** | Transcribe |
| **Discovery** | Port probe (`:8000`). Topology filter `offering:speaches` |
| **Health probe** | `GET /health` -> 200 OK |
| **Enumeration** | Model from env var `WHISPER__MODEL` or `--model` flag |
| **VRAM tracking** | Inferred from model size (tiny ~400MB, base ~500MB, small ~1GB, medium ~2.6GB, large-v3 ~2.9GB) |
| **Proxy** | OpenAI-compatible: forward `POST /v1/audio/transcriptions` (multipart) |
| **Benchmarking** | Transcribe reference 10s clip. Fast < 2s, Degraded < 10s, Vetoed < 30s |
| **Resource sync** | Storage bank transport (whisper model files 400MB-3GB) |

### OpenedAI Speech

| Aspect | Detail |
|--------|--------|
| **Offering type** | `openedai-speech` |
| **Capabilities** | Speak |
| **Discovery** | Port probe (`:8001`). Topology filter `offering:openedai-speech` |
| **Health probe** | `GET /` -> 200 |
| **Enumeration** | Voices from `voice_to_speaker.yaml` |
| **VRAM tracking** | XTTS v2: ~2-3GB estimated. Piper: CPU only |
| **Proxy** | OpenAI-compatible: forward `POST /v1/audio/speech` |
| **Benchmarking** | Generate reference phrase. Fast TTFB < 500ms, Degraded < 2s, Vetoed < 5s |

### Infinity

| Aspect | Detail |
|--------|--------|
| **Offering type** | `infinity` |
| **Capabilities** | Embed (multimodal), Rerank |
| **Discovery** | Port probe (`:7997`). Topology filter `offering:infinity` |
| **Health probe** | `GET /health` -> 200 OK |
| **Enumeration** | `GET /models` -> loaded models with type (embed/rerank/clip/clap) |
| **Proxy** | Embed: `POST /embeddings` (OpenAI-compatible). Rerank: `POST /rerank` |
| **VRAM tracking** | Not directly exposed. Inferred from model size (SigLIP-so400m ~1.5GB, BGE-reranker-large ~1GB) |
| **Benchmarking** | Embed: 100 texts throughput. Rerank: 20 queries x 50 docs throughput |
| **Resource sync** | Models loaded at startup via CLI flags. No runtime pull. Sync via storage bank if needed |

### LibreTranslate

| Aspect | Detail |
|--------|--------|
| **Offering type** | `libretranslate` |
| **Capabilities** | Translate |
| **Discovery** | Port probe (`:5000`). Topology filter `offering:libretranslate` |
| **Health probe** | `GET /languages` -> 200 |
| **Enumeration** | `GET /languages` -> available language pairs |
| **VRAM tracking** | CPU-only. N/A |
| **Proxy** | Forward `POST /translate` with `{ q, source, target, format }` |
| **Benchmarking** | Translate 10 reference sentences across 3 language pairs. Measure wall-clock time |
| **Resource sync** | Language models (~300MB each) downloaded on demand by LibreTranslate. Sync via storage bank for pre-populated deployments |

### HuggingFace

| Aspect | Detail |
|--------|--------|
| **Offering type** | `huggingface` |
| **Capabilities** | Chat, Embed, Imagine, Transcribe (task-dependent) |
| **Discovery** | Configured (API token via dashboard) |
| **Enumeration** | `GET /api/models?inference_provider=all&pipeline_tag={task}` |
| **Priority** | -10 (cloud default) |
| **Proxy** | OpenAI-compatible for Chat. Task-specific for others |
| **Rate limits** | Free tier rate-limited. `X-Wait-For-Model` for cold starts |

---

## Validation Rules

### Offering Catalog Invariants

| Rule | Validation |
|------|-----------|
| OC-1 | Every offering type has a unique `offering_type()` string. Registry rejects duplicates at startup |
| OC-2 | Every offering declares at least one capability. `capabilities()` non-empty |
| OC-3 | Boot report warns on uncovered capabilities |
| OC-4 | Cloud providers require explicit API key configuration, never auto-discovered |
| OC-5 | Offering implementations are isolated; a bug in one adapter must not crash another |

### Routing Invariants

| Rule | Validation |
|------|-----------|
| RT-1 | `recommended:X` resolves to highest-scored model for capability X, respecting priority + fitness + demand |
| RT-2 | Explicit model routes to highest-priority healthy instance that has it |
| RT-3 | Pin override always wins (with warning if instance is unhealthy) |
| RT-4 | Cloud providers (priority -10) selected only when no local instance serves the capability |
| RT-5 | Blocked fitness verdict removes instance from pool (except pinned) |
| RT-6 | Cross-offering VRAM: if Ollama uses 8GB and ComfyUI needs 12GB on a 24GB GPU, routing accounts for combined 20GB |

### Proxy Invariants

| Rule | Validation |
|------|-----------|
| PX-1 | Ollama API backward compatibility — existing clients work unchanged |
| PX-2 | Each capability endpoint returns expected format (image bytes for Imagine, audio for Speak, JSON for Chat) |
| PX-3 | Streaming responses forwarded without full buffering |
| PX-4 | Queue depth tracked per-instance, decremented on completion (success or error) |
| PX-5 | Metrics emitted for every proxied request |

### Cloud Provider Invariants

| Rule | Validation |
|------|-----------|
| CP-1 | API keys encrypted at rest |
| CP-2 | Model enumeration refreshes every 30 min and on config change |
| CP-3 | Cloud instances always at configured priority (never 0) |
| CP-4 | Local model preferred over cloud when both exist |
| CP-5 | Removing a provider cascades: all its models removed from routing pool |

### Health Invariants

| Rule | Validation |
|------|-----------|
| HL-1 | Each offering defines its own probe endpoint and expected response |
| HL-2 | Health checks: 15s for local/garden, 60s for cloud |
| HL-3 | Unhealthy after 3 consecutive failed probes (circuit breaker) |
| HL-4 | Orchestrator `/health` returns 200 if at least one instance across any offering is healthy |

### Benchmark Invariants

| Rule | Validation |
|------|-----------|
| BM-1 | Benchmarks are advisory — influence recommendations but never prevent routing (except Blocked) |
| BM-2 | Each offering defines its own benchmark strategy via `benchmark()` |
| BM-3 | Results persist across restarts in `{data_dir}/fitness.json` |
| BM-4 | Benchmark as-is (real-world conditions with colocated workloads); record colocation state when cross-offering VRAM tracking is implemented |

---

## Architecture Compliance

### Layer Rules

```
offerings/ --> catalog/ --> domain/ <-- tasks/
                 |                       |
                 v                       v
              infra/ <-------------- api/
```

| Rule | Enforcement |
|------|-------------|
| `domain/` has zero I/O — no async, no HTTP, no filesystem | No `tokio`, `reqwest`, `std::fs` in domain imports |
| `offerings/` implement the `Offering` trait from `catalog/` | All offering-specific I/O lives here |
| `catalog/` defines traits and registry — no offering-specific logic | No `if offering_type == "ollama"` in catalog code |
| `infra/` handles shared I/O (Koi, Moss, persistence, events) | No offering-specific code |
| `tasks/` orchestrate via `Offering` methods and domain algorithms | No direct HTTP calls |
| `api/` dispatches to offerings via proxy, no service-specific logic | Capability routing, not offering routing |

### Litmus Test

For a protocol-compatible offering (one that maps to existing capabilities
and needs no new API endpoints), can someone add it by:
1. Creating `offerings/{name}/` with trait implementation
2. Registering it in the catalog
3. Touching zero files in `domain/`, `tasks/`, or `api/`?

If yes, the core abstraction is right.

Offerings that introduce new capabilities (new API endpoints, new
dashboard management pages) require additive changes to `api/` and
`dashboard/`. The key constraint is that `domain/` and `tasks/` remain
untouched — the new capability variant in the `Capability` enum is the
only domain-level change, and the enum is designed to be extended.

### Configuration

All offering types are compiled in. Which ones are active is controlled by
`config.toml`. An offering with no discovered instances and no configuration
is dormant — no background tasks, no health checks, no resources consumed.

---

## Implementation Plan

### Phase 1: Foundation (Crate Skeleton + Shared Domain + Ollama Adapter)

**Goal:** New crate compiles, Ollama works identically to the current orchestrator.

**Design first:**
1. Define the multi-offering architecture: `Offering` trait, `ServiceInstance`,
   extended `Capability` enum, `OfferingRegistry`.
2. Map every module in the current orchestrator to its target location
   (shared domain / shared infra / Ollama adapter / needs generalization).

**Then build:**
3. Create `src/orchestrators/ai/` with the crate structure.
4. Generalize domain types: `OllamaInstance` -> `ServiceInstance`,
   `RequestCapability` -> `Capability`, extend routing with priority +
   capability filtering.
5. Move shared domain modules (routing, demand, fitness, advisor,
   recommendation, tiering, placement, lease, metrics, policy,
   reconciliation, gpu_catalog) — updating types but preserving algorithms.
6. Move shared infra (gateway, persistence, stone_discovery, tools_stream,
   events) — parameterizing offering filters.
7. Implement `OllamaOffering` in `offerings/ollama/` — encapsulating
   `OllamaClient`, NDJSON proxy, benchmark payloads, response types.
8. Wire tasks to dispatch through `Offering` trait.
9. Wire API to dispatch through `OfferingRegistry`.

**Validation:** All existing `exercise.ps1` tests pass against the new
orchestrator with zero behavioral regressions.

### Phase 2: ComfyUI Adapter

**Goal:** Image generation via `POST /api/imagine` and `POST /api/edit`.

1. Implement `ComfyUiOffering` in `offerings/comfyui/`.
2. ComfyUI client: HTTP + per-request WebSocket.
3. Workflow template library (txt2img, img2img, inpaint).
4. Template parameterization + validation against `GET /object_info`.
5. Discovery, health, enumeration, VRAM tracking.
6. Proxy: full workflow dispatch lifecycle.
7. Benchmark: reference txt2img.
8. Resource sync via storage bank transport.
9. Exercise script: Imagine + Edit tests.

**Validation:** `POST /api/imagine {"model":"flux-dev","prompt":"A zen garden"}`
returns PNG bytes.

### Phase 3: Audio Adapters (Speaches + OpenedAI Speech)

**Goal:** `POST /api/transcribe` and `POST /api/speak` work.

1. Implement `SpeachesOffering` (OpenAI-compatible multipart proxy).
2. Implement `OpenedAiSpeechOffering` (OpenAI-compatible JSON proxy).
3. Discovery, health, enumeration, benchmark for each.
4. Exercise script: audio round-trip (speak -> transcribe -> verify).

**Validation:** Speak text, get audio, transcribe audio, text matches.

### Phase 4: Embedding + Reranking (Infinity)

**Goal:** `POST /api/embed` (multimodal) and `POST /api/rerank`.

1. Implement `InfinityOffering`.
2. Proxy: embed (OpenAI-compatible) + rerank.
3. Model enumeration from `GET /info`.
4. Exercise script: embed text, embed image, rerank.

### Phase 5: Translation + Cloud Providers

**Goal:** `POST /api/translate` and cloud provider registration.

1. Implement `LibreTranslateOffering`.
2. Implement cloud provider framework: `CloudProviderOffering` base,
   per-provider specifics.
3. Encrypted API key management.
4. Cloud model enumeration (30-min refresh).
5. Dashboard: provider management.
6. Exercise script: translate + cloud fallback test.

### Phase 6: HuggingFace + Dashboard Polish

**Goal:** HF Inference as multi-capability offering. Capability-centric
dashboard.

1. Implement `HuggingFaceOffering`.
2. Task-based routing (pipeline_tag -> capability).
3. Cold-start handling (`X-Wait-For-Model`).
4. Dashboard: capability overview + per-offering detail pages.
5. Full exercise script covering all capabilities.

---

## Migration Path

The Ollama orchestrator continues to work as-is during development. The AI
orchestrator is a new crate with a new binary name
(`zen-garden-ai-orchestrator`), new Docker image, and same default ports
(`:21434` proxy, `:7190` dashboard).

**Transition:**
1. Both orchestrators can coexist (different offering names in Moss).
2. When the AI orchestrator passes all exercise tests for Ollama, operators
   switch.
3. The old `ollama` crate is archived (not deleted — reference for audit).
4. Docker image label: `zen-garden.ai.orchestrator`.

**Backward compatibility:**
- The AI orchestrator serves the full Ollama API surface on the proxy port.
- Existing Ollama clients see no difference.
- `GET /` returns "Ollama is running" for client compatibility.
- New capability endpoints are additive.

---

## Open Questions

1. **Moss-level resource sync:** The storage bank transport strategy for
   non-Ollama offerings depends on Moss's ability to serve as a sync
   layer for offering content. A dedicated investigation is planned to
   evaluate extending the nurturing/harvest system for active sync.

2. **ComfyUI WebSocket pool:** Starting with per-request WebSocket
   connections. If profiling shows handshake latency matters (unlikely
   for 10-60s image generation jobs), upgrade to a persistent pool.

3. **ComfyUI template versioning:** Discovery-time validation against
   `GET /object_info` plus dispatch-time safety gate. Templates marked
   incompatible per-instance when node APIs drift.

4. **Cross-offering benchmark coordination:** Benchmark as-is (real-world
   conditions). Record colocation state in fitness matrix entries when
   cross-offering VRAM tracking lands.

5. **Multi-stone ComfyUI checkpoints:** Orchestrator-managed sync via
   storage bank transport. Route to instances that have the checkpoint;
   sync asynchronously when placement demands it.

---

## Consequences

**Positive:**
- Single operational surface for all AI services in a Zen Garden.
- Koan Framework connects via one adapter (`Koan.AI.ZenGarden`) to one
  endpoint — routing, capability resolution, and cloud fallback are
  transparent.
- Existing Ollama functionality is preserved with zero regressions.
- Adding a new AI service type is a bounded, additive change.
- Cross-offering VRAM awareness prevents resource contention on shared GPUs.
- Cloud providers as priority-based fallbacks eliminate single-point-of-failure
  for AI capabilities.

**Negative:**
- Larger single binary (all offerings compiled in).
- Phase 1 is a significant effort: ~5,500 LOC of shared domain + infra to
  generalize, plus ~600 LOC of Ollama adapter to encapsulate.
- ComfyUI workflow template management adds operational complexity.
- Resource sync for non-Ollama offerings depends on Moss storage bank
  infrastructure.

**Neutral:**
- The Ollama orchestrator crate is archived after migration, not deleted.
- All offerings share the same release cadence.
- Dashboard complexity grows with each offering but is managed through the
  capability-centric + per-offering-detail architecture.

---

## Implementation Lessons (2026-03-29)

The first implementation attempt was reverted. The crate compiled, passed
63 unit tests, and cleared 8 adversarial reviews, but failed operationally.
The root causes and corrective guidance are recorded here so the next
attempt does not repeat them.

### Root Causes

1. **Bespoke code without checking existing solutions.** The Ollama
   orchestrator already solved Docker networking (`host.docker.internal`),
   Koi endpoint resolution (port 5641, not 5353), gateway registration
   patterns, and startup sequencing. The implementation wrote new code
   for these instead of harvesting the working solutions.

2. **Assumptions instead of verification.** When the container couldn't
   reach Koi at `127.0.0.1:5353`, the response was "this is expected
   behavior" instead of checking how existing orchestrators solve it.
   The answer (`http://host.docker.internal:5641`) was already in the
   Ollama orchestrator's Dockerfile and CLI defaults.

3. **Checkbox-driven instead of holistic.** Optimized for "does it
   compile" and "do tests pass" instead of "does this work as a running
   service." Built types outward (domain → adapters → tasks → API)
   instead of operational requirements inward (Docker → startup →
   discovery → routing → dashboard).

### Corrective Guidance for Re-implementation

**Approach:** Decompose the Ollama orchestrator into shared infrastructure
and Ollama-specific adapter, then build the AI orchestrator on the shared
layer. Break into blocks with clear interfaces and operational verification.

**Block 1: Harvest the Shared Orchestration Layer**
- Extract offering-agnostic logic (~70%) from the Ollama orchestrator
  into a shared layer. Define the offering adapter trait.
- Complete when generalized domain modules compile and pass tests.
  The Ollama orchestrator must remain functional (proof of extraction).

**Block 2: Operational Foundation**
- Wire the AI orchestrator binary using the shared layer. Harvest
  operational configuration (Koi endpoint, ports, Docker networking)
  from the Ollama orchestrator — do not guess values.
- Complete when the container starts, connects to Koi, discovers
  instances, and registers per-offering gateways — verified by running
  against a real garden.

**Block 3: Ollama Feature Parity**
- Implement the OllamaOffering adapter as a bounded context. Wire all
  shared tasks. Wire the Ollama proxy on its dedicated port.
- Complete when exercise.ps1 passes against the AI orchestrator.

**Block 4: Multi-Offering Extension**
- Implement each offering adapter as a bounded context, one at a time.
  Each gets its own proxy port speaking its native protocol.
- Complete when each offering can be discovered, health-checked, and
  proxy-routed — verified against a real running instance.

**Block 5: Dashboard**
- Build after the backend is operationally verified. Design from the AI
  orchestrator's own requirements (capability-centric, multi-offering).

**Block 6: Cloud Providers**
- Cloud APIs as priority -10 fallbacks. Anthropic needs dedicated
  Messages API translation, not a generic OpenAI-compat adapter.

**Anti-patterns to avoid:**
- Writing any code before exhaustively checking what exists in the
  codebase that solves the same problem.
- Defaulting configuration values from templates instead of from
  verified production behavior.
- Declaring a phase complete based on compilation + unit tests instead
  of end-to-end operational verification.
- Building all adapters in parallel before any single one works
  end-to-end.
- Treating adversarial code review as the primary quality mechanism
  instead of runtime verification.
