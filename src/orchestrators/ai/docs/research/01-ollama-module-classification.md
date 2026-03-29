# Ollama Orchestrator Module Classification

> Research artifact for ORCH-0013. Classifies every module in the Ollama
> orchestrator as shared, Ollama-specific, or needs-generalization.
> Produced during Phase 0 research before any code was written.

---

## Domain Layer (`src/orchestrators/ollama/src/domain/`)

### Fully Shared (Zero OllamaInstance References)

These modules operate on generic concepts (strings, numbers, durations)
and can be copied directly into the AI orchestrator's domain layer.

| Module | LOC | Key Types | Notes |
|--------|-----|-----------|-------|
| `demand.rs` | ~591 | `RequestCapability`, `DemandLedger`, `DecayCounter`, `DecayAverage` | Pure decay math on string model/capability names. `RequestCapability` needs extending with new variants (Imagine, Transcribe, etc.) |
| `fitness.rs` | ~696 | `Verdict`, `Capability`, `GpuMatrix`, `BenchmarkRun`, `Sample`, `TestSuite` | Scoring thresholds are generic. `Capability` enum needs extending. `Verdict::compute()` threshold constants are reusable. |
| `lease.rs` | ~139 | `LeaseManager`, `Lease` | Pure endpoint+model string management with expiry. Zero coupling. |
| `metrics.rs` | ~351 | `MetricsEngine`, `MetricsSnapshot` | Ring-buffer metrics on string stone/model names. Fully generic. |
| `gpu_catalog.rs` | ~300 | `FitnessSource`, `ResolvedFitness` | Static GPU score table + `resolve_fitness()` priority chain. No offering awareness. |
| `reconciliation.rs` | ~177 | `RegistryDrift` | Pure diff logic comparing model lists. Operates on `Vec<String>` and `Vec<LoadedModel>`. |

### Needs Generalization (Algorithm is Generic, Types are Ollama-Specific)

These modules contain offering-agnostic algorithms but reference
`OllamaInstance` directly. The algorithm survives extraction; only the
type signatures change.

| Module | LOC | OllamaInstance Fields Used | Generalization Strategy |
|--------|-----|---------------------------|------------------------|
| `routing.rs` | ~710 | `endpoint`, `stone_name`, `health`, `models_available`, `queue_depth`, `vram_budget_bytes` | Replace `HashMap<String, OllamaInstance>` with `HashMap<String, ServiceInstance>`. All accessed fields exist on `ServiceInstance` (ORCH-0013 design). Algorithm unchanged. |
| `tiering.rs` | ~121 | `health.is_routable()`, `vram_budget_bytes`, `endpoint` | Replace `&[OllamaInstance]` with `&[ServiceInstance]`. Algorithm: group by VRAM tier, unchanged. |
| `placement.rs` | ~256 | `health.is_routable()`, `endpoint`, `vram_budget_bytes`, `models_loaded` | Replace instance type. Greedy bin-pack algorithm unchanged. |
| `advisor.rs` | ~877 | `endpoint`, `stone_name`, `health`, `vram_budget_bytes`, `gpu_name`, `num_parallel`, `models_available` | `advise_topology()` is pure shared. `gpu_slots_from_instances()` (lines 590-655) is the only Ollama-coupled function — needs adapter or generic mapping. |
| `policy.rs` | ~114 | `health`, `vram_budget_bytes`, `vram_total_bytes`, `models_available`, `endpoint` | `models_needing_sync()` algorithm is generic (group by VRAM tier, compute model union, find gaps). Replace instance type. |
| `recommendation.rs` | ~1469 | `health`, `stone_name`, `endpoint`, `models_available`, `models_loaded` | 4-layer scoring algorithm is fully generic. Capability mapping (lines 59-82) needs extending. Replace instance type. |

### Ollama-Specific (Cannot Be Generalized)

| Module | LOC | What Makes It Specific |
|--------|-----|----------------------|
| `types.rs` (partial) | ~172 | `OllamaTagsResponse`, `OllamaModelTag`, `OllamaModelDetails`, `OllamaPsResponse`, `OllamaRunningModel`, `OllamaShowResponse`, `OllamaVersionResponse`, `OllamaInferenceFinal`, `OllamaPullProgress`, `OllamaEmbedResponse` — all Ollama HTTP response shapes |

### Shared Types from `types.rs` (Reusable)

| Type | Lines | Notes |
|------|-------|-------|
| `AutoPullMode` | 17 | Generic concept |
| `JobKind`, `JobStatus`, `OrchestratorJob` | 43-115 | Generic job tracking. `JobKind` variants need extending (not just ModelPull/ModelDelete/Benchmark) |
| `ComputeType` | 118 | CUDA/ROCm/Metal/CPU — universal |
| `InstanceHealth` | 159 | Profiling/Healthy/Unhealthy — universal |
| `LoadedModel` | 176 | name + size_vram + expires_at — universal |
| `ModelInfo` | 188 | Model metadata — universal (capabilities, VRAM, context_length) |
| `Tier` | 214 | VRAM grouping — universal |
| `Lease` | 227 | Endpoint + model reservation — universal |
| `RoutingDecision`, `RoutingError` | 254-290 | Generic routing result |
| `RouterConfig`, `FeatureConfig`, `StoneConfig` | 295-335 | Configuration — needs extending |
| `StoneMetrics`, `MetricsSnapshot` | 340-365 | Generic |
| `MetricEvent` | 500 | Request/Error metrics — universal |
| `PlacementPlan` | 532 | Generic |

---

## Tasks Layer (`src/orchestrators/ollama/src/tasks/`)

| Task | Classification | Rationale |
|------|---------------|-----------|
| `advisor.rs` | **Shared** | Computes topology advice from abstract GPU/model slots. No Ollama calls. |
| `benchmark.rs` | **Ollama-specific** | Hardcoded Ollama prompts, vision images, tool schemas. Calls `OllamaClient` methods directly. |
| `discovery.rs` | **Ollama-specific** | Queries Ollama topology, subscribes to Tools SSE filtered for `offering:ollama`, profiles via `OllamaClient.full_profile()`. |
| `gateway_announce.rs` | **Shared** | Generic mDNS + Moss gateway registration. Only the `OFFERING` string constant is specific. |
| `health_check.rs` | **Ollama-specific** | Uses `OllamaClient.health_check()` (GET /api/tags). |
| `metrics_flush.rs` | **Shared** | Pure persistence, no Ollama knowledge. |
| `metrics_processor.rs` | **Shared** | Generic event aggregation from channel. |
| `model_sync.rs` | **Ollama-specific** | Uses `OllamaClient.pull_model()` and `policy::models_needing_sync()`. |
| `placement.rs` | **Mostly shared** | Generic bin-pack algorithm + hysteresis. Only `OllamaClient.load_model()` is specific. |
| `reconciliation.rs` | **Ollama-specific** | Queries `OllamaClient.get_tags()`, `.get_ps()`, `.show_model()`. |
| `snapshot_publisher.rs` | **Shared** | Pure data projection to JSON via `watch` channel. |

### Generalization Pattern for Tasks

Shared tasks call through the `Offering` trait instead of `OllamaClient`:
- `health_check` → `offering.probe(endpoint)`
- `benchmark` → `offering.benchmark(endpoint, model, capability)`
- `discovery` → `offering.enumerate(endpoint)` + shared topology infrastructure
- `model_sync` → `offering.sync_resource(resource, from, to)`
- `reconciliation` → `offering.enumerate(endpoint)` + domain diff
- `placement` → domain compute + `offering.sync_resource()` for pre-warming

---

## API Layer (`src/orchestrators/ollama/src/api/`)

| Handler | Classification | Rationale |
|---------|---------------|-----------|
| `health.rs` | **Shared** | Generic health aggregation. |
| `dashboard.rs` | **Shared** | Generic SPA serving, SSE, settings, jobs. |
| `extension.rs` | **Shared** | `/v1/models`, `/v1/stones`, `/v1/recommendations` — reads from cached registries. |
| `proxy.rs` | **Ollama-specific** | Ollama NDJSON streaming, metrics extraction from Ollama response shapes, moniker resolution, auto-pull. Core routing logic (`select_instance`) is shared but proxy dispatch is protocol-specific. |
| `management.rs` | **Ollama-specific** | Ollama model pull/delete/feasibility. |
| `benchmark_api.rs` | **Ollama-specific** | Triggers Ollama fitness profiler. |

---

## Infrastructure Layer (`src/orchestrators/ollama/src/infra/`)

| Module | Classification | Rationale |
|--------|---------------|-----------|
| `ollama_client.rs` | **Ollama-specific** | HTTP client for all Ollama endpoints. |
| `persistence.rs` | **Shared** | Config (TOML) + metrics (JSON) persistence. |
| `events.rs` | **Shared** | SSE broadcast re-export from orchestrator-common. |
| `gateway.rs` | **Shared** | Koi + Moss gateway clients. |
| `stone_discovery.rs` | **Mostly shared** | Koi mDNS, topology queries. `query_topology_ollama()` is specific; rest is generic. |
| `tools_stream.rs` | **Ollama-specific** | Filters SSE events for `offering:ollama`. |

---

## Summary

| Category | Module Count | LOC (approx) | % |
|----------|-------------|-------------|---|
| Fully shared | 15 | ~3,800 | 48% |
| Needs generalization | 7 | ~3,550 | 45% |
| Ollama-specific | 9 | ~600 | 8% |

The 8% Ollama-specific code becomes the `offerings/ollama/` bounded context.
The 48% shared code copies directly (with new Capability variants).
The 45% needing generalization requires replacing `OllamaInstance` → `ServiceInstance`
in type signatures — the algorithms themselves are unchanged.
