# Defensive Publication: Hierarchical Capability-Aware AI Model Resolution with Declarative Recipes

**Inventor**: Leonardo Milson Botinelly Soares (Leo Botinelly)
**Disclosure Date**: 2026-03-24
**Field of Invention**: AI inference routing, model selection, and capability-based service resolution
**Keywords**: AI model routing, capability resolution, recipe configuration, fitness profiling, demand-weighted placement, virtual model monikers, multi-persona resolution chain

---

## 1. Problem Statement

Contemporary AI application frameworks require developers to specify concrete model names when invoking inference services. This tight coupling between application code and model identity creates several interrelated problems:

**Brittleness.** When an AI model is renamed, deprecated, or superseded by a newer version, every call site referencing that model by name breaks. Applications must be redeployed for what is operationally a model swap.

**Monolithic control.** A single persona -- typically the application developer -- controls model selection at every granularity. In practice, model selection is a cross-cutting concern involving at least four distinct roles: (a) application developers who know what their code needs, (b) ML engineers who know which models perform best for each task, (c) infrastructure operators who know what hardware is available and how it is loaded, and (d) DevOps engineers who need environment-specific overrides (staging vs. production). No existing system allows each of these personas to author model selection policy at their own layer without interfering with the others.

**No graceful degradation.** When a preferred model is unavailable, existing systems either fail or require explicit fallback logic in application code. There is no automatic scoring mechanism that selects the next-best model based on empirically measured performance on the specific hardware available.

**Static selection ignores runtime conditions.** Model performance varies dramatically across GPU architectures. A model that achieves 80 tokens/second on an RTX 4090 may produce only 12 tokens/second on an RTX 3060. Existing systems treat all instances as interchangeable, ignoring hardware-specific fitness. Demand patterns further complicate placement: a model that is optimal when demand is low may starve other workloads under load.

**No capability abstraction.** Applications must know not just *what* they need (text generation, embedding, vision understanding) but *which specific model* satisfies that need. This conflates the abstract capability with the concrete implementation, violating the principle of separation of concerns.

---

## 2. Prior Art Summary

### 2.1 OpenAI API / Direct Model Specification

The OpenAI API and compatible services (Azure OpenAI, Anthropic, etc.) require the caller to specify a model name in every request (`"model": "gpt-4"`). The model name is the only addressing mechanism. There is no resolution chain, no capability abstraction, and no fallback. When a model is deprecated, all callers must update. There is no per-hardware fitness scoring because the infrastructure is opaque (cloud-hosted).

### 2.2 LiteLLM

LiteLLM provides a unified interface across multiple LLM providers and supports model aliasing -- a mapping from an alias string to a provider-specific model name. However, this aliasing is single-level (one alias resolves to one concrete model) with no hierarchical chain, no capability abstraction (aliases are still model names, not capability identifiers), no fitness scoring, and no demand-aware topology optimization. The alias is a flat lookup table, not a scored, multi-persona resolution pipeline.

### 2.3 vLLM

vLLM is an inference server optimized for throughput via PagedAttention and continuous batching. It serves a single model per instance and provides no model selection intelligence, no multi-model routing, no capability categorization, and no fitness profiling. Model selection is entirely external to vLLM.

### 2.4 Ollama Native

Ollama provides a local inference runtime that manages model lifecycle (pull, load, unload). It runs as a single instance on a single host. It has no multi-node routing, no capability-based model scoring, no demand tracking, and no topology optimization. Model selection is explicit in every API call.

### 2.5 LangChain Model Abstraction

LangChain provides wrapper classes for different LLM providers, enabling code that is structurally agnostic to the provider. However, model resolution is still explicit -- the developer specifies the model name when constructing the wrapper. There is no layered resolution, no capability-to-model mapping, no fitness scoring, and no runtime adaptation. The abstraction is at the API layer, not the selection layer.

### 2.6 Kubernetes GPU Scheduling

Kubernetes supports GPU scheduling via device plugins and resource requests (`nvidia.com/gpu: 1`). This is resource-based scheduling, not capability-aware model ranking. Kubernetes does not understand AI model capabilities, does not score models against hardware, and does not provide multi-level resolution chains. It can place a pod on a node with a GPU, but cannot determine which model should run on which GPU for optimal throughput.

### 2.7 NVIDIA Triton Inference Server / KServe

Triton Inference Server supports model ensembles (directed acyclic graphs of model stages) and model version policies (latest, specific version, all). KServe provides Kubernetes-native model serving with canary rollouts, autoscaling, and multi-model serving. Both systems address model *serving* — how to execute inference once a model is selected — but neither provides capability-based model *selection*. In Triton, the client specifies the model name (or ensemble name) in every request; there is no resolution chain, no capability abstraction, and no fitness scoring across heterogeneous hardware. KServe's InferenceService routes to a specific model deployment; it does not score multiple candidate models against hardware-specific fitness data to select the best one for a given capability. The disclosed system operates at the layer above serving: it determines *which* model to invoke, then delegates execution to the serving infrastructure (Ollama, Triton, vLLM, or any other runtime).

### 2.8 Ensemble / Multi-Model Inference

Some systems route a single request to multiple models and merge responses (ensemble inference, majority voting, speculative decoding with verification). The disclosed system is explicitly single-model-per-request: the resolution chain selects one model, and that model handles the full request. Ensemble routing is an orthogonal concern — it could be layered atop this resolution chain (resolve N models for the same capability, dispatch to all, merge) but is not part of the disclosed mechanism. The disclosed contribution is the multi-persona, capability-aware selection of a single optimal model, not the dispatch pattern after selection.

---

## 3. Detailed Description of the Invention

### 3.1 The 7-Level Resolution Chain

The invention introduces a hierarchical resolution chain with seven ordered levels. When an application requests inference for a capability (e.g., "Chat"), each level is consulted in order. Each level either returns a concrete model name or returns null to defer to the next level. The first non-null response is used. This design separates the concerns of model selection across multiple personas, each authoring policy at their own level.

The seven levels, in descending priority:

| Level | Name | Persona | Mechanism | Scope |
|-------|------|---------|-----------|-------|
| 1 | Explicit model on call | Developer | `ChatOptions.Model = "qwen3.5:9b"` | Single request |
| 2 | Ambient scoped context | Developer | `AiCategoryScope` via `AsyncLocal<ImmutableStack>` | Code block (async-safe) |
| 3 | Recipe binding | ML Engineer / DevOps | `IAiRecipeProvider.GetModel(category)` from config | Deployment-wide |
| 4 | Orchestrator advisor | System (automated) | `IAiModelAdvisor.GetRecommendedModel(category)` from fitness + demand scoring | Infrastructure-wide |
| 5 | Category configuration | Operator | `appsettings.json` per-category defaults (`Koan:Ai:{Category}:Model`) | Deployment-wide |
| 6 | Source/member default model | Framework | Source definition's declared model per capability | Framework default |
| 7 | Hardcoded fallback | Framework | Category definition's `DefaultModel` field | Compile-time |

**Resolution pseudocode:**

```
function resolve(category, sourceHint, modelHint):
    // Level 1: Explicit model on the call
    if modelHint is not null:
        return modelHint

    // Level 2: Ambient scoped context (AsyncLocal stack)
    scopeModel = AiCategoryScope.ResolveModel(category)
    if scopeModel is not null:
        return scopeModel

    // Level 3: Recipe binding
    recipeModel = recipeProvider.GetModel(category)
    if recipeModel is not null:
        return recipeModel

    // Level 4: Orchestrator advisor (fitness + demand scoring)
    advisorModel = modelAdvisor.GetRecommendedModel(category)
    if advisorModel is not null:
        return advisorModel

    // Level 5: Category configuration
    categoryConfig = options[category]
    if categoryConfig.Model is not null:
        return categoryConfig.Model

    // Level 6: Source/member default model
    capabilities = source.GetEffectiveCapabilities(member)
    if capabilities[category].Model is not null:
        return capabilities[category].Model

    // Level 7: Hardcoded fallback
    if categoryDefinition.DefaultModel is not null:
        return categoryDefinition.DefaultModel

    // All levels exhausted — no model resolved
    return error("No model available for capability '{category}': all 7 resolution levels returned null. Configure a recipe binding, pin a model, or set a category default.")
```

**Chain exhaustion:** If all seven levels return null (no explicit model, no ambient scope, no recipe binding, no orchestrator recommendation, no category config, no source default, and no hardcoded fallback), the system returns a structured error. This is a configuration error, not a runtime failure — it means no persona at any level has expressed an opinion about which model to use for this capability, and no default exists. The error message identifies the capability and suggests corrective actions.

**Critical design properties:**

- **Each level returns null to defer.** A level that has no opinion produces null, and the chain continues. This means each persona only needs to specify bindings for the capabilities they care about.
- **Higher levels always override lower levels.** A developer's explicit model (Level 1) always takes precedence over the orchestrator's automated recommendation (Level 4).
- **Levels are independently deployable.** Changing a recipe (Level 3) does not require code changes. Changing the orchestrator's scoring weights (Level 4) does not require configuration changes.
- **The chain is intrinsically resilient.** If Level 4 (orchestrator) is unavailable, Level 5-7 still provide a usable model. If Level 3 (recipe) is empty, Level 4 provides automated selection.
- **Resolved model availability validation.** After the chain produces a concrete model name, the system validates that the model is actually available in the current infrastructure. If the resolved model is not present on any healthy instance, the system has two options depending on the resolution level that produced the name: (a) for Levels 1-3 (human-authored overrides), the system returns an error indicating the specified model is unavailable, preserving the operator's explicit intent rather than silently substituting; (b) for Levels 4-7 (automated/default), the system skips the unavailable model and continues the chain from the next level. This validation-with-fallback mechanism ensures that automated levels self-heal around unavailable models while human-specified models fail loudly. The validation applies regardless of the storage format of the configuration — the chain is format-agnostic.

### 3.2 Recipe Provider

Recipes are named, versioned, declarative configurations that bind capabilities to concrete model names. They occupy Level 3 in the resolution chain, sitting between developer overrides (Levels 1-2) and automated infrastructure scoring (Level 4).

**Configuration structure:**

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
        },
        "benchmark-large": {
          "Chat": "llama3:70b",
          "Thinking": "qwq:32b",
          "Embed": "mxbai-embed-large"
        }
      }
    }
  }
}
```

**Key properties:**

- **Sparse.** A recipe binds only the capabilities the author has opinions about. The `dev-fast` recipe above only binds Chat; all other capabilities (Embed, Thinking, Quick, Vision, etc.) fall through to Level 4+ for automated selection.
- **Named and diffable.** Recipes are standard JSON configuration. They can be committed to version control, diffed between environments, and reviewed in pull requests.
- **Environment-scoped.** Via the standard `appsettings.{Environment}.json` override mechanism, different environments can activate different recipes: `ActiveRecipe: "production-balanced"` in production, `ActiveRecipe: "dev-fast"` in development.
- **A/B testable.** Switching between recipes requires changing a single string (`ActiveRecipe`), enabling rapid experimentation with different model combinations.
- **Format-agnostic.** The recipe concept is independent of its serialization format. The reference implementation uses JSON (`appsettings.json`), but the identical semantics apply to YAML, TOML, database-backed stores, API-served configurations, environment variable overlays, or any key-value store capable of representing the `{recipe_name: {capability: model_name}}` structure. The interface contract is the abstraction; the storage backend is an implementation detail.
- **Atomic read semantics.** The recipe provider reads the active recipe name and its bindings at construction time (or at configuration reload) and caches the result as an immutable snapshot. Resolution calls (`GetModel`) read from this snapshot without locking. When the configuration source changes (file modification, database update, API call), the provider replaces the entire snapshot atomically — there is no window where a resolution sees a partially-updated recipe. This applies regardless of the backing store: file-based providers use file-change notification + atomic replacement; database-backed providers use transactional reads; API-backed providers use versioned responses. The snapshot-and-replace pattern ensures that concurrent resolutions during a recipe change all see either the old recipe or the new recipe, never a mix.
- **Interface:** `IAiRecipeProvider` with `ActiveRecipeName: string?` and `GetModel(category: string): string?`. The implementation reads from `IConfiguration` at construction time. A null return defers to Level 4.

### 3.3 Virtual Model Monikers (recommended:capability)

The invention introduces virtual model monikers -- synthetic model names of the form `recommended:{capability}` -- that can be used in any Ollama-compatible API request. The proxy intercepts these monikers, resolves them through the pre-computed recommendation cache, and rewrites the request transparently.

**Protocol:**

1. Client sends standard Ollama API request with `"model": "recommended:chat"`.
2. Proxy extracts the `recommended:` prefix and the capability string (`chat`).
3. Proxy looks up the capability in the pre-computed recommendation cache (`recommended_models` map).
4. If found: proxy rewrites the `model` field in the JSON body to the resolved concrete model name (e.g., `"qwen3.5:9b"`).
5. Proxy routes the rewritten request to the optimal stone/instance using the standard routing algorithm.
6. Proxy adds `X-Zen-Resolved-Model: qwen3.5:9b` response header so the client can observe the resolution.
7. If not found: proxy returns a structured error (400 for unknown capability, 404 for no model available for a valid capability).

**Valid capabilities:** `quick`, `chat`, `completion`, `synthesis`, `vision`, `ocr`, `tools`, `thinking`, `embedding`.

**Compatibility:** This mechanism operates entirely within the proxy layer. The upstream Ollama instance receives a standard request with a concrete model name. This means the moniker system works with:
- Unmodified `ollama` CLI
- Python `ollama` SDK
- LangChain's `ChatOllama`
- Any HTTP client that speaks the Ollama API

The recommendation cache is refreshed whenever models, instances, benchmark results, or user pins change. The cache maps capability string to concrete model name.

**Cache synchronization mechanism:**

The recommendation cache exists in two locations and synchronizes through a pull-based pattern:

1. **Infrastructure side (Rust proxy):** The proxy's `AppState` holds a `recommended_models: HashMap<String, String>` computed by the `recommend()` function. This cache is recomputed whenever the proxy detects a change in: the model inventory, the set of active instances, benchmark results, or user pin state. Recomputation is triggered by the advisor background task (debounced 5s after topology events, periodic every 300s). The proxy exposes the cache via `GET /v1/recommendations`, which returns the full map of capability-to-model-name.

2. **Client side (.NET application):** `ZenGardenModelAdvisor` polls the proxy's `/v1/recommendations` endpoint on a configurable interval (default: 30 seconds) and caches the result in a `RecommendationSnapshot` object. The snapshot is an immutable record replaced atomically on each refresh. Between refreshes, `GetRecommendedModel(category)` performs a dictionary lookup against the cached snapshot — no network call. If the proxy is unreachable, the advisor returns the last-known snapshot (stale but available), or null if no snapshot has ever been obtained (causing the resolution chain to fall through to Level 5+).

### 3.4 Capability-Specific Model Scoring

**Terminology note:** This disclosure uses three distinct concepts related to model performance evaluation:
- **GPU fitness matrix** (§3.4, Layer 1): A data structure storing per-model, per-GPU benchmark results — verdicts (Fast/Degraded/Vetoed/Blocked) and measured throughput. This is empirical measurement data.
- **Model scoring algorithm** (§3.4): The 5-layer algorithm that computes a composite score for each model for a given capability, using the GPU fitness matrix as one input among several (availability, context window, parameter count, name affinity).
- **Fitness bootstrap** (§3.5): The three-stage process by which the GPU fitness matrix is populated over time — from heuristic estimates at first boot, to formal benchmarks, to observed production metrics.

These are distinct mechanisms that compose: the bootstrap populates the matrix, and the scoring algorithm reads the matrix.

The recommendation engine scores every eligible model for a given capability using a 5-layer scoring algorithm. Each layer has capability-specific weights, reflecting the fact that different capabilities value different model properties.

**Layer 0: Availability (binary presence + redundancy)**

```
score += 50                                    // model is available on at least 1 stone
score += min(10 * (num_stones - 1), 30)        // redundancy bonus, capped at 30
score += 20                                    // if model is currently loaded in VRAM (hot)
```

**Layer 1: Fitness (best-stone-only verdict + throughput)**

For each stone that has the model, look up the benchmark verdict in the GPU fitness matrix. Use only the *best* stone's verdict (not averaged across all stones, which would dilute a fast GPU's score with a slow one).

Verdict scores:
- `Fast`: +300 (cold start < 30s AND tok/s > 5)
- `Degraded`: +150 (cold start < 90s AND tok/s > 1)
- `Vetoed`: +30 (below thresholds, deprioritized but routable)
- `Blocked`: -500 (model errors on this GPU, hard block)

Throughput bonus (capped per capability):

| Capability | Throughput Metric | TPS Bonus Cap | Rationale |
|------------|-------------------|---------------|-----------|
| Quick | Tokens/second (generation) | 200 | Speed is primary differentiator |
| Chat / Completion | Tokens/second (generation) | 50 | Moderate speed importance |
| Tools / Thinking | Tokens/second (generation) | 30 | Quality matters more |
| Embedding | Embeddings/second (batch throughput) | 100 | Batch vectorization speed |
| Synthesis / OCR | 0 (not measured) | 0 | Batch workloads -- speed irrelevant |

**Capability-specific throughput metrics:** The throughput bonus uses the metric natural to each capability type. For generative capabilities (Quick, Chat, Completion, Tools, Thinking), throughput is measured in tokens per second of output generation. For Embedding, throughput is measured in embeddings per second (vectors produced per unit time from the benchmark's embedding test suite). The benchmark stores both metrics per model per GPU. If the capability-specific metric is unavailable (e.g., no embedding benchmark has been run), the system falls back to the generative tok/s metric as a rough proxy, with a 0.5x scaling factor to avoid overestimating embedding throughput from generation benchmarks.

Cold start penalty: `min(cold_start_ms / 1000, 50)` subtracted from score.

**Thinking capability** uses relaxed verdict thresholds (users expect slower responses for extended reasoning):
- Fast: cold start < 60s AND tok/s > 3
- Degraded: cold start < 120s AND tok/s > 0.5

**Tools capability** adds a correctness gate via `valid_ratio`:
- All prompts valid (5/5) + fast: Fast
- All valid + slow: Degraded
- Flaky (3-4/5 valid): Degraded regardless of speed
- Low correctness (< 50% valid): Vetoed
- Zero valid: Blocked

**Layer 2: Context Window Bonus**

```
bonus = min(context_length / 1000, context_bonus_cap[capability])
```

Context bonus caps per capability:

| Capability | Context Bonus Cap | Rationale |
|------------|-------------------|-----------|
| Synthesis | 500 | Primary differentiator for long-context work |
| Thinking | 300 | Extended reasoning benefits from large context |
| Tools | 250 | Complex tool chains need context |
| Vision | 200 | Multi-image, complex scenes |
| Chat / Completion | 150 | Moderate context importance |
| OCR | 150 | Document processing |
| Quick | 0 | Speed, not context |

**Layer 3: Model Quality Bonus (parameter count)**

```
params_b = parameter_count / 1,000,000,000
bonus = min(params_b * quality_multiplier[capability], quality_bonus_cap[capability])
```

Quality multipliers and caps per capability:

| Capability | Multiplier (per B params) | Quality Cap | Rationale |
|------------|---------------------------|-------------|-----------|
| Thinking | 60 | 500 | Reasoning scales with parameters |
| Tools / Vision | 50 | 450 | Structured output / scene understanding |
| Chat / Completion / Synthesis | 40 | 400 | General quality |
| OCR | 15 | 400 | Specialization matters more than size |
| Quick | 0 | 0 | Speed only |

**Layer 4: Name Affinity Bonus**

For certain capabilities, models whose names contain the capability keyword receive a specialization bonus:

| Capability | Name Affinity Bonus |
|------------|---------------------|
| OCR | +300 (models named `*ocr*` are purpose-built) |
| All others | 0 |

**Scoring pseudocode (complete):**

```
function score_model(model, capability, instances, gpu_matrix):
    score = 0
    reasoning = []

    // Layer 0: Availability
    stones = instances_with_model(model)
    if len(stones) > 0:
        score += SCORE_AVAILABLE                            // 50
        score += min((len(stones) - 1) * 10, 30)           // redundancy
    if any_loaded(model, stones):
        score += SCORE_LOADED                               // 20

    // Layer 1: Fitness (best stone only)
    best_verdict_score = -infinity
    best_tps = 0
    best_cold = 0
    for stone in stones:
        entry = gpu_matrix.lookup(model, capability, stone.endpoint)
        if entry is null AND capability in {Tools, Think}:
            entry = gpu_matrix.lookup(model, Generate, stone.endpoint)  // fallback
        if entry is not null:
            vs = verdict_to_score(entry.verdict)  // Fast=300, Degraded=150, Vetoed=30, Blocked=-500
            if vs > best_verdict_score OR (vs == best_verdict_score AND entry.tps > best_tps):
                best_verdict_score = vs
                best_tps = entry.median_tps
                best_cold = entry.cold_start_ms

    if has_fitness_data:
        score += best_verdict_score
        score += min(best_tps, tps_bonus_cap[capability])
        score -= min(best_cold / 1000, 50)

    // Layer 2: Context window
    if model.context_length is not null:
        score += min(model.context_length / 1000, context_bonus_cap[capability])

    // Layer 3: Quality
    params_b = model.parameter_count / 1e9
    score += min(params_b * quality_multiplier[capability], quality_bonus_cap[capability])

    // Layer 4: Name affinity
    if model.name contains capability_keyword:
        score += name_affinity_bonus[capability]

    return (score, reasoning)
```

**Pin override:** A user can pin a model for a capability. When a pin is set, the pinned model is forced to rank 1 regardless of its score, provided it is eligible (correct capability tag, available on at least one stone). If the pinned model is not eligible, the pin is silently ignored and scoring proceeds normally.

**Model name resolution and versioning:** Throughout the resolution chain, model names are treated as opaque strings that may encode version information in any format native to the serving runtime (e.g., `qwen3.5:9b`, `llama3:70b-q4_0`, `gpt-4-0125-preview`). The system does not parse version semantics from model names — it matches them exactly against the model inventory. However, the recipe and category configuration layers support version-flexible bindings through two mechanisms: (a) **wildcard suffixes** — a recipe binding of `qwen3.5:*` matches any installed model whose name starts with `qwen3.5:`, selecting the largest (by parameter count) among matches; (b) **tag aliases** — the model inventory may contain alias entries (e.g., `latest` pointing to `qwen3.5:9b`) which are resolved before scoring. These mechanisms are intentionally simple — the resolution chain resolves *which capability binding to use*, and the model inventory resolves *which concrete artifact a name refers to*. Semantic versioning constraints (>=, ~=, ^) are a possible extension at the inventory layer but are not part of the disclosed resolution chain, which operates on resolved names.

### 3.5 Demand-Weighted Topology Optimization

The system tracks per-capability and per-model request demand using exponentially-decayed counters, then uses this demand signal to optimize model placement and parallelism across the GPU topology.

**Three decay windows:**

| Window | Half-Life | Purpose |
|--------|-----------|---------|
| Reactive | 15 minutes | Parallelism adjustment, queue pressure response |
| Tactical | 6 hours | Placement optimization, workload separation |
| Strategic | 3 days | Replication suggestions, eviction candidates |

**Decay formula:** For a counter with accumulated value `V` and elapsed time `t`:

```
effective_count = V * 2^(-t / half_life)
rate_per_hour = effective_count * ln(2) / (half_life / 3600)
```

Each `record()` call decays the existing total to the current time, then adds 1.0. This means old events are naturally forgotten without explicit bucket expiry. A single `DecayCounter` is 24 bytes (f64 value + Instant timestamp).

**DecayAverage** (used for observed fitness): Tracks a decaying weighted average of a continuous metric (e.g., tokens per second). It maintains two values: a decayed sum and a decayed count. Each observation `record(value)` decays both to the current time, then adds `value` to the sum and `1.0` to the count:

```
function record(value):
    elapsed = now - last_update
    decay = 2^(-elapsed / half_life)
    sum = sum * decay + value
    count = count * decay + 1.0
    last_update = now

function average() -> Option<f64>:
    elapsed = now - last_update
    decay = 2^(-elapsed / half_life)
    decayed_count = count * decay
    if decayed_count < 0.1:
        return None  // insufficient recent data
    return (sum * decay) / decayed_count
```

A `DecayAverage` is 32 bytes (f64 sum + f64 count + Instant). It uses the same exponential decay as `DecayCounter` but preserves magnitude information, not just event count.

**Confidence ramp:**

```
confidence = min(total_requests / 50, 1.0)
```

Below 50 total requests, demand data is blended with a uniform distribution:

```
blended_weight = uniform * (1 - confidence) + observed * confidence
```

This prevents the system from making extreme placement decisions based on a handful of early requests.

**Demand ledger contents:**

- `by_capability: HashMap<RequestCapability, DecayCounter>` -- per-capability request counts
- `by_model: HashMap<String, DecayCounter>` -- per-model request counts
- `observed_fitness: HashMap<(model, stone), DecayAverage>` -- live tok/s observations
- `cold_loads: HashMap<(model, stone), DecayCounter>` -- model cold-load events
- `total_requests: u64` -- for confidence ramp

**Topology advisor algorithm:**

The advisor runs as a background task, triggered reactively (on topology changes with 5-second debounce) or periodically (every 300 seconds). It computes optimal model placement and parallelism recommendations.

Phase 1 -- Fitness-weighted placement:
1. Sort models by demand-weighted VRAM (hot models first if demand data available, then largest-first for tie-breaking -- best-fit-decreasing).
2. For each model, score all eligible GPUs using: `fitness * (0.5 + 0.5 * headroom_fraction)`. This prefers fast GPUs with room to spare.
3. Place model on the highest-scoring GPU that has sufficient VRAM.

Phase 2 -- Demand-weighted parallelism:
1. Compute VRAM headroom after placement.
2. Water-fill: `max_slots = (free_vram - 256MB_headroom) / largest_kv_cache_on_gpu`.
3. Apply demand-aware cap:
   - All-embedding GPU: use full water-fill (high parallelism safe, cap 16)
   - Thinking demand > 40%: clamp to 2 (sustained generation holds KV cache)
   - Embedding demand > 60% on mixed GPU: clamp to 6
   - Otherwise: clamp to 4 (chat parallel cap)

Phase 3 -- Typed recommendations with priority, confidence, and auto-applicability flags for: parallelism changes, max loaded models, placement swaps, replication suggestions, and eviction candidates.

**Three-stage fitness bootstrap:**

| Stage | Source | When Used | Accuracy |
|-------|--------|-----------|----------|
| 1. GPU name heuristic | Static lookup table of ~60 GPU models | T=0 (no data) | Directional (relative ordering correct) |
| 2. Benchmark | Formal per-model per-GPU test suite | After benchmark run | High (measured median tok/s, cold start) |
| 3. Observed | Live proxy request throughput (DecayAverage) | Steady state | Highest (actual production performance) |

Resolution priority: observed > benchmarked > projected. The system starts with GPU name heuristics, upgrades to benchmark data when available, and converges on observed production metrics.

**CPU-only inference nodes:** When a stone has no GPU, the system treats it as a single compute tier with `CPU_SCORE = 10` in the GPU catalog. VRAM-based tier sweeps use system RAM as the capacity metric for CPU-only nodes (since CPU inference uses system memory for model weights). CPU-only nodes receive fitness verdicts using the same threshold framework, but with significantly lower expected tok/s. The routing safety net (§3.6) still guarantees that a model installed on a CPU-only node is routable — the two-phase tier sweep treats CPU nodes as the lowest tier, used as degraded fallback when no GPU-equipped node has the model. Demand-based reservation never reserves CPU nodes for large-model traffic because CPU inference throughput is always below the reservation activation threshold.

### 3.6 Routing Safety Net

The routing algorithm ensures that an installed model is always routable -- the system never refuses to serve a model that the user explicitly installed.

**Two-phase tier sweep:**

Phase 1 (preferred): Collect candidates from tiers whose VRAM budget is greater than or equal to the model's VRAM requirement. These candidates are marked as non-degraded.

Phase 2 (fallback): If Phase 1 produces no candidates, collect candidates from all tiers including those with insufficient VRAM. These candidates are marked as `is_degraded = true`.

**Invariant: installed model ALWAYS routes.** If a model is available on any healthy instance, the router will find it. The degraded label allows consumers to observe that the model is running on suboptimal hardware, but it never blocks the request.

**Demand-based reservation:** When recent traffic includes requests for large models that exclusively need higher-tier GPUs (their VRAM exceeds the lowest tier's capacity), and the current request is for a small model that fits on lower tiers, the router activates reservation mode. In reservation mode, small-model requests prefer lower-tier candidates, keeping the high-tier GPU available for large models that actually need it. Reservation is based on actual request demand, not on model catalog presence.

**Fitness-blocked filtering:** Candidates where all GPU matrix entries have a `Blocked` verdict are removed from consideration. If removing blocked candidates empties the candidate set, the blocked candidates are restored (the model was installed intentionally, so it should still route).

### 3.7 Vision-Aware Auto-Routing

When a chat request contains image content (detected by the presence of `images` arrays in message parts or at the top level of the request body) and no explicit model is specified by the developer, the router automatically queries the model advisor for the `Vision` capability instead of `Chat`.

```
function resolve_chat(request):
    modelHint = request.Model

    if modelHint is null AND advisor is not null:
        hasImage = any message in request.Messages has parts with type "image"
        if hasImage:
            visionModel = advisor.GetRecommendedModel("Vision")
            if visionModel is not null:
                modelHint = visionModel

    return resolve("Chat", request.Route.AdapterId, modelHint)
```

If no vision-capable model is available, the system falls back to the standard Chat resolution chain. This means applications with mixed text/image workloads automatically get vision-capable models for image requests and text-optimized models for text requests, without any developer intervention.

**Capability inference from request content** also occurs on the proxy side (Rust). The proxy inspects the request body to infer `RequestCapability`:

1. Embedding endpoints (`/api/embed`, `/api/embeddings`) -> Embedding
2. `tools` array present -> Tools
3. Image content in messages -> Vision
4. Model has `thinking` tag -> Thinking
5. Fallback -> Chat

This classification feeds into the demand ledger for per-capability tracking.

---

## 4. Claims-Style Disclosure

The following statements describe the inventive mechanisms disclosed herein, each independently sufficient to distinguish this system from prior art:

1. **A method for resolving AI model selection through an ordered sequence of seven resolution levels**, each authored by a distinct persona (developer, ML engineer, operator, infrastructure, framework), where each level either returns a concrete model name or returns null to defer to the next level, and where a capability abstraction (Chat, Embed, Vision, Thinking, Tools, Quick, Synthesis, OCR) allows consumers to declare what kind of inference they need rather than which model to use.

2. **A declarative recipe provider that binds capabilities to model names via named, versioned configuration artifacts**, where recipes are sparse (omitted capabilities defer to lower resolution levels), environment-scoped (different recipes per deployment target via standard configuration override), and independently authored by ML engineers or DevOps specialists without requiring application code changes.

3. **A virtual model moniker system using a "recommended:{capability}" prefix** that is intercepted by a proxy layer, resolved through a pre-computed recommendation cache, and transparently rewritten to a concrete model name in the request body, with the resolved model name returned in a response header, operating without modification to any upstream client library.

4. **A capability-specific fitness scoring algorithm with five layers** (Availability, Fitness verdict, Context window, Model quality, Name affinity), where each scoring layer uses distinct weights per capability type, and where the fitness layer uses best-stone-only scoring to prevent a fast GPU's score from being diluted by averaging with slower hardware.

5. **A fitness verdict system with four levels** (Fast, Degraded, Vetoed, Blocked) with capability-specific thresholds (relaxed for Thinking: cold start < 60s, tok/s > 3; strict for Generate: cold start < 30s, tok/s > 5), and a correctness gate for the Tools capability based on the ratio of valid tool-call outputs to total test prompts.

6. **A demand tracking system using exponentially-decayed counters at three time windows** (15-minute reactive, 6-hour tactical, 3-day strategic), with a confidence ramp that blends observed demand with a uniform distribution until 50 requests have been recorded, preventing premature optimization on sparse data.

7. **A three-stage fitness bootstrap** that resolves GPU performance as: observed production throughput (highest fidelity) > formal benchmark results > GPU name heuristic lookup (directional ordering from a static catalog of ~60 GPU models), ensuring the system provides reasonable model placement from first boot while converging on measured performance.

8. **A routing safety net that guarantees an installed model is always routable**, using a two-phase tier sweep where Phase 1 selects candidates with sufficient VRAM and Phase 2 falls back to degraded candidates on undersized tiers, with demand-based reservation that dynamically reserves high-tier GPUs for large models when recent traffic demonstrates demand for them.

9. **An ambient scoped context mechanism for AI model selection** using `AsyncLocal<ImmutableStack>` (or any async-aware thread-local storage mechanism: Rust's `tokio::task_local!`, Go's `context.Context`, Python's `contextvars.ContextVar`) that flows per-category source and model overrides across async call boundaries, enabling developers to override model selection for a code block without modifying the call sites within that block, and where the stack supports nesting (inner scopes override outer scopes). The concrete integration with the resolution chain is: Level 2 of the 7-level chain queries the ambient scope, which returns the most-recently-pushed model override for the requested capability, or null if no scope is active. This allows library code to set model preferences (e.g., "use a fast model for all AI calls in this background job") without threading model names through every function signature. The scope is per-async-context, not per-thread and not global — concurrent requests maintain independent scope stacks.

10. **A content-driven capability inference system** that automatically detects image content in chat requests and routes to a vision-capable model without developer intervention, falling back to text-only chat models when no vision model is available, and that classifies proxy requests into capability categories (Chat, Embedding, Vision, Tools, Thinking) based on request path, body content, and model capability tags for demand tracking purposes.

11. **A topology advisor that computes demand-weighted model placement and parallelism recommendations**, using fitness-weighted GPU scoring, demand-proportional workload separation, and water-fill parallelism computation with capability-aware caps (thinking-heavy workloads capped at 2 parallel slots, embedding workloads uncapped up to 16), producing typed, prioritized, confidence-scored recommendations.

12. **A pin override mechanism** that forces a specific model to rank 1 for a given capability regardless of its fitness score, provided the model is eligible (has the correct capability tag and is available on at least one healthy stone), and that silently ignores the pin if the model becomes ineligible, preserving system resilience.

---

## 5. Implementation Evidence

### 5.1 Koan Framework (.NET) -- Client-Side Resolution Chain

| Component | File | Key Methods/Types |
|-----------|------|-------------------|
| Resolution chain orchestrator | `Koan.AI/Pipeline/AiCategoryRouter.cs` | `Resolve(category, sourceHint, modelHint)`, `ResolveChat(request)` |
| Recipe provider interface | `Koan.Core/AI/IAiRecipeProvider.cs` | `ActiveRecipeName`, `GetModel(category)` |
| Recipe provider implementation | `Koan.AI/Pipeline/AiRecipeProvider.cs` | Reads `Koan:Ai:ActiveRecipe` and `Koan:Ai:Recipes:{name}` from `IConfiguration` |
| Model advisor interface | `Koan.Core/AI/IAiModelAdvisor.cs` | `GetRecommendedModel(category): string?` |
| Zen Garden advisor implementation | `Koan.ZenGarden/AI/ZenGardenModelAdvisor.cs` | `GetRecommendedModel()`, `RefreshInBackground()`, `RecommendationSnapshot` cache |
| Ambient scoped context | `Koan.AI/Context/AiCategoryScope.cs` | `AsyncLocal<ImmutableStack<AiCategoryScope>>`, `ResolveSource()`, `ResolveModel()`, `ResolveMerged()` |
| Capability constants | `Koan.Core/AI/AiCapability.cs` | `Chat`, `Embed`, `Ocr`, `Vision`, `Quick`, `Synthesis`, `Thinking`, `Tools` |
| Category configuration | `Koan.AI.Contracts/Categories/AiCategoryOptions.cs` | `Source`, `Model`, `Via`, `Fallback` |

### 5.2 Zen Garden (Rust) -- Infrastructure-Side Scoring and Routing

| Component | File | Key Functions/Types |
|-----------|------|---------------------|
| Recommendation scoring | `src/orchestrators/ollama/src/domain/recommendation.rs` | `recommend()`, `score_model()`, `RecommendationResponse`, `Recommendation` |
| Demand ledger | `src/orchestrators/ollama/src/domain/demand.rs` | `DemandLedger`, `DecayCounter`, `DecayAverage`, `RequestCapability` |
| Topology advisor | `src/orchestrators/ollama/src/domain/advisor.rs` | `advise_topology()`, `GpuSlot`, `ModelSlot`, `DemandContext`, `TopologyAdvice`, `Recommendation`, `RecommendationKind` |
| Fitness profiler | `src/orchestrators/ollama/src/domain/fitness.rs` | `Verdict`, `Capability`, `GpuMatrix`, `GpuMatrixEntry`, `BenchmarkRun`, `TestSuite` |
| GPU fitness catalog | `src/orchestrators/ollama/src/domain/gpu_catalog.rs` | `projected_score()`, `resolve_fitness()`, `ResolvedFitness`, `FitnessSource`, `GPU_CATALOG` (static table) |
| Proxy with moniker resolution | `src/orchestrators/ollama/src/api/proxy.rs` | `proxy_handler()`, `proxy_inference()`, `X-Zen-Resolved-Model` header |
| Routing algorithm | `src/orchestrators/ollama/src/domain/routing.rs` | `select_instance()`, two-phase tier sweep, demand-based reservation |
| Advisor background task | `src/orchestrators/ollama/src/tasks/advisor.rs` | `run()`, 5-second debounce, 300-second periodic recomputation |

### 5.3 Specific Constants as Evidence of Implementation Specificity

**Scoring constants (recommendation.rs):**
- `SCORE_AVAILABLE = 50`
- `SCORE_REDUNDANCY_PER_STONE = 10`
- `SCORE_REDUNDANCY_CAP = 30`
- `SCORE_LOADED = 20`
- `SCORE_VERDICT_FAST = 300`
- `SCORE_VERDICT_DEGRADED = 150`
- `SCORE_VERDICT_VETOED = 30`
- `SCORE_VERDICT_BLOCKED = -500`
- `COLD_PENALTY_CAP = 50`
- `MAX_RECOMMENDATIONS = 5`

**Demand constants (demand.rs):**
- `REACTIVE_HALF_LIFE_SECS = 900` (15 minutes)
- `TACTICAL_HALF_LIFE_SECS = 21600` (6 hours)
- `STRATEGIC_HALF_LIFE_SECS = 259200` (3 days)
- `CONFIDENCE_THRESHOLD = 50` (requests before demand overrides uniform)

**Advisor constants (advisor.rs):**
- `DEFAULT_KV_CACHE_CHAT = 300 MB`
- `DEFAULT_KV_CACHE_EMBED = 80 MB`
- `MIN_HEADROOM = 256 MB`
- `MAX_PARALLEL = 16`
- `CHAT_PARALLEL_CAP = 4`
- `PARALLEL_CHANGE_THRESHOLD = 2`
- `DISK_TO_VRAM_FACTOR = 1.1`

**Fitness verdict thresholds (fitness.rs):**
- Generate Fast: cold < 30s, tok/s > 5
- Generate Degraded: cold < 90s, tok/s > 1
- Embed Fast: cold < 5s
- Embed Degraded: cold < 30s
- Think Fast: cold < 60s, tok/s > 3
- Think Degraded: cold < 120s, tok/s > 0.5
- Tools correctness gate: valid_ratio < 0.5 -> Vetoed, < 1.0 -> Degraded, 1.0 -> speed determines

**Advisor task timing (tasks/advisor.rs):**
- `ADVISOR_INTERVAL = 300s` (periodic)
- `DEBOUNCE = 5s` (after topology event)
- `STARTUP_DELAY = 15s` (initial wait for discovery)

**GPU catalog (gpu_catalog.rs):**
- `UNKNOWN_GPU_SCORE = 35`
- `CPU_SCORE = 10`
- 60+ GPU entries from H100 (100) to GTX 1650 (22), including NVIDIA, AMD, Apple Silicon, and Intel Arc

---

## 6. Publication Notice

This document constitutes a defensive publication under the doctrine of statutory invention registration and prior art publication. The inventions described herein are hereby dedicated to the public domain for the purpose of preventing future patent claims on these specific mechanisms by any party, including the inventor.

The full implementation is available in the Zen Garden (Rust) and Koan Framework (.NET) source repositories. The disclosure date of 2026-03-24 establishes the earliest priority date for prior art purposes.

This publication does not constitute a waiver of any rights to file patent applications on these inventions. It serves as prior art to prevent others from obtaining patents on the described mechanisms.

---

## Antagonist Review Log

### Pass 1
**Antagonist:** (1) Abstraction gap: no specification of what happens when a resolved model is unavailable in infrastructure. (2) Scope hole: recipe provider described as JSON-only, leaving other formats patentable. (3) Reproducibility gap: DecayAverage referenced but formula never provided. (4) Missing edge case: CPU-only inference nodes not addressed in VRAM-based scoring/routing.
**Author revision:** Added model availability validation with level-aware behavior (Levels 1-3 fail loudly, Levels 4-7 continue chain). Generalized recipe format to be format-agnostic. Added full DecayAverage formula with pseudocode. Added CPU-only node handling in fitness bootstrap section.

### Pass 2
**Antagonist:** (1) Scope hole: embedding throughput uses tokens/second metric but embeddings are measured differently. (2) Terminology drift: "fitness" used for three distinct concepts without disambiguation. (3) Missing edge case: concurrent recipe changes during resolution lack atomicity guarantees.
**Author revision:** Added capability-specific throughput metrics table with embedding-specific metric (embeddings/second). Added terminology note disambiguating GPU fitness matrix, model scoring algorithm, and fitness bootstrap. Added atomic read semantics for recipe provider with snapshot-and-replace pattern.

### Pass 3
**Antagonist:** (1) Scope hole: no discussion of ensemble/multi-model routing. (2) Prior art weakness: NVIDIA Triton and KServe not addressed. (3) Missing edge case: all 7 levels returning null has no defined behavior.
**Author revision:** Added Triton/KServe prior art section with differentiation. Added ensemble routing section explicitly scoping the disclosure as single-model-per-request. Added chain exhaustion error handling with structured error message.

### Pass 4
**Antagonist:** (1) Scope hole: model versioning (wildcards, constraints) not covered. (2) Section 101 exposure: ambient scoped context claim too abstract.
**Author revision:** Added model name resolution and versioning section covering wildcard suffixes, tag aliases, and explicit scoping of semantic versioning as an extension. Expanded ambient scope claim with concrete integration points, cross-language equivalents, and per-async-context isolation semantics.

### Pass 5
**Antagonist:** No further objections — this disclosure is sufficient to block patent claims on the described invention.

### Final Status
CLEARED — Antagonist found no further weaknesses. Safe to publish.
