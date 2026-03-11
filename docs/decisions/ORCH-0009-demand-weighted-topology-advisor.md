# ORCH-0009: Demand-Weighted Topology Advisor

- **Status**: Accepted
- **Date**: 2026-03-06
- **Scope**: Ollama orchestrator (applicable pattern for future orchestrators)

## Context

The Ollama orchestrator manages a distributed fleet of GPU-equipped stones, each
running an Ollama instance with N models and a single shared
`OLLAMA_NUM_PARALLEL` value. The orchestrator must decide:

1. **Placement** — which models live on which GPUs
2. **Parallelism** — what `NUM_PARALLEL` / `MAX_LOADED_MODELS` to set per stone
3. **Replication** — when to duplicate a hot model across stones
4. **Eviction** — when to suggest unloading cold models

The critical constraint is that `OLLAMA_NUM_PARALLEL` is per-Ollama-process, not
per-model. All models on a stone share one parallelism value. This means the
optimal strategy naturally separates workloads by type onto different stones when
the fleet allows it.

Prior to this decision, the advisor used a static VRAM-based algorithm (BFD
placement + water-fill parallelism) with no demand awareness and no fitness
weighting beyond VRAM capacity.

## Decision

Introduce a **Demand-Weighted Topology Advisor** built on three input axes:
**Demand** (what users ask for), **Topology** (where things are, what fits), and
**Fitness** (how fast each stone serves each model). The advisor finds the
placement + parallelism configuration that maximizes the product of all three.

### Core Concept: Demand Pressure

Every capability has a pressure score:

```
pressure(capability) = demand / capacity
```

- `demand` = request rate x concurrency factor
- `capacity` = sum over serving stones of (effective_parallel_slots x fitness)

| Pressure | Meaning |
|----------|---------|
| > 1.0 | Under-provisioned (requests queue or fail) |
| 0.7-1.0 | Healthy headroom |
| < 0.5 | Over-provisioned (resources wasted) |

The advisor equalizes pressure across capabilities by adjusting placement and
parallelism — preventing any single capability from being starved while others
sit idle.

### Capability Profiles

Each capability type has intrinsic resource characteristics:

| Capability | KV Cache / Slot | Parallelism Affinity | Latency Sensitivity |
|------------|-----------------|---------------------|---------------------|
| embedding | ~80 MB | High (8-16) | Low |
| chat | ~300 MB | Moderate (2-6) | Medium (TTFT) |
| vision | ~400 MB | Low-Moderate (2-4) | Medium |
| reasoning | ~500 MB+ | Low (1-3) | Low (long gen) |
| synthesis | ~200 MB | Moderate (3-6) | High (real-time) |
| tools | ~300 MB | Moderate (2-4) | High (agent loops) |

These profiles determine how much benefit high parallelism provides and how much
KV cache pressure it creates.

### Three Phases

**Phase 0 — Intent-Only (T=0):** No request metrics. The orchestrator already
knows: model inventory per stone (`/api/tags`), loaded models (`/api/ps`), model
capabilities and parameter count (`/api/show`), GPU name and VRAM (topology
chirp), current `NUM_PARALLEL` and `MAX_LOADED_MODELS` (Moss env API). Strategy:
uniform demand assumption weighted by pin overrides, placement via BFD with
workload affinity, parallelism from dominant capability profile. GPU name
heuristic provides projected fitness.

**Phase 1 — Accumulation (T=0 to T=N):** The proxy records per-request signals
(model, capability, timestamp, tokens generated, latency) into a Demand Ledger
using exponentially decayed counters. A confidence ramp blends observed weights
with the uniform T=0 assumption until sufficient data accumulates.

**Phase 2 — Demand-Weighted (T=N, confident):** Observed demand fully drives the
advisor. Pressure computed per capability. Recommendations target pressure
equalization: placement swaps, parallelism changes, replication of hot models,
eviction of cold models.

### Demand Ledger

Lightweight in-memory structure with exponentially decayed counters:

- Per-capability: request rate (reactive 15m, tactical 6h, strategic 3d)
- Per-model: request rate at the same three decay windows
- Per (model, stone): observed throughput (tok/s), cold-load events
- Per-stone: queue pressure, saturation metrics

Each `DecayCounter` is ~16 bytes. No time-bucketed histograms. Natural
forgetting of old data.

**Confidence ramp:**
```
effective_weight = lerp(uniform_weight, observed_weight, confidence)
confidence = min(1.0, total_requests / CONFIDENCE_THRESHOLD)
```

### Fitness Model

Three-stage bootstrap, from coarse to precise:

| Stage | Source | Precision | When |
|-------|--------|-----------|------|
| Projected | GPU name lookup table | +/-30% | Discovery (always) |
| Benchmarked | Fitness evaluation | +/-5% | On-demand |
| Observed | Live proxy tok/s | +/-2%, adaptive | After serving requests |

Fitness is per (model-family, stone), not per stone alone. A GPU that excels at
7B inference may be mediocre at 70B. Projected fitness bootstraps from GPU name
(generation, architecture, memory bandwidth) — imprecise but never absent.

Within the same VRAM tier, GPU performance varies significantly: an RTX 4070 and
RTX 3060 may both have 12 GB, but measured throughput can differ 2x. The
projected fitness lookup table maps GPU name to a relative performance estimate
(memory bandwidth, CUDA cores, architecture generation) so that T=0
recommendations are directional even without benchmarks.

Observed fitness decays with a 15-minute half-life (faster than demand) to adapt
to thermal throttling, competing workloads, or driver changes.

### Placement Scoring

```
placement_score(model, stone) =
    fitness(model, stone)
    x affinity(model.capability, stone_workload_mix)
    x (1.0 - utilization(stone))
    / vram_fraction(model, stone)
```

High-fitness, low-utilization, compatible-workload, VRAM-efficient placements
score highest.

### Recommendation Types

| Kind | Priority | Auto-applicable? |
|------|----------|-----------------|
| Parallelism change | Urgent/Suggested | Yes (with restart) |
| MAX_LOADED_MODELS change | Suggested | Yes (with restart) |
| Model placement swap | Suggested | No (requires pull/delete) |
| Model replication | Informational | No |
| Cold model eviction | Informational | No |

Each recommendation carries: reasoning, pressure_before, pressure_after
(projected), confidence level, and fitness source tag.

### Advisor Triggers

| Trigger | Source |
|---------|--------|
| Topology change | `registry.updated` event (debounced) |
| Periodic | Every 5 minutes |
| Manual | `POST /api/advisor/evaluate` or dashboard button |
| Post-benchmark | Automatic after fitness evaluation completes |

Manual evaluation is synchronous (pure computation, sub-100ms). No async job
tracking needed.

### Actuation Modes

| Mode | Behavior |
|------|----------|
| Observe (default) | Recommendations on dashboard only |
| Suggest | Dashboard + API/webhook notification |
| Auto-tune | Parallelism applied automatically; placement requires confirmation |

Parallelism changes use the MOSS-0005 `PATCH .../env` endpoint + service
restart. Placement changes (model pull/delete) are always user-confirmed.

### Pins Override Demand

Recommendation pins represent explicit user intent and always take precedence
over observed demand patterns. A pinned capability gets elevated weight
regardless of request volume.

## Existing Infrastructure (Code Audit)

Audit performed 2026-03-06 against the current orchestrator codebase.

### What Exists and Can Be Reused

**Metrics pipeline** (`api/proxy.rs`, `domain/metrics.rs`,
`tasks/metrics_processor.rs`): The proxy extracts `tokens_in`, `tokens_out`,
`duration_ns`, `eval_duration_ns` from every Ollama response and sends a
`MetricEvent` to an async channel. A background processor aggregates into
`MetricsEngine` which maintains per-model request counts, per-stone throughput
ring buffers (5-min windowed tok/s), and a `model_demand` ring buffer for
demand-share computation. Cumulative counters persist to disk every 30 seconds.

**Fitness benchmark** (`domain/fitness.rs`, `tasks/benchmark.rs`): Full
per-(model, capability, stone) benchmark framework with `Verdict` enum
(Fast/Degraded/Vetoed/Blocked), `GpuMatrix` synthesis, and disk persistence.
The benchmark runner tests Generate, Embed, and Vision workloads separately,
yields to live traffic, and handles timeouts/OOM gracefully.

**Routing** (`domain/routing.rs`): Performance-first routing with fitness
advisory (Blocked filtered, others as soft sort), demand-based reservation
(preserve high-tier for large models), and queue-depth tie-breaking.

**Recommendation scoring** (`domain/recommendation.rs`): Multi-layer scoring
(availability, fitness, context window, quality) with pin support.

**Discovery** (`tasks/discovery.rs`, `infra/stone_discovery.rs`): Fetches model
inventory, loaded state, VRAM, GPU name, `OLLAMA_NUM_PARALLEL` per stone at
profile time.

**Advisor task** (`tasks/advisor.rs`): Reactive (topology events) + periodic
(5 min) loop with debouncing. Snapshots instances + models, calls advisor
algorithm, publishes to `AppState.advisor`.

**Snapshot publisher** (`tasks/snapshot_publisher.rs`): Every 2 seconds,
serializes advisor, benchmark, metrics, config to JSON via watch channel.
Dashboard reads lock-free.

### What Must Change

**MetricEvent** (`domain/types.rs:476-497`): Add `capability: Option<String>`
field. The proxy already knows the request path (`/api/embed`, `/api/chat`,
`/api/generate`) but discards this information. Tag at proxy dispatch time.

**MetricsEngine** (`domain/metrics.rs`): The existing ring buffers
(`response_times`, `stone_throughput`, `model_demand`) are fixed-capacity
VecDeques with manual windowing. Replace with `DecayCounter`-based
`DemandLedger` that provides multi-window decay (15m/6h/3d), per-capability
counters, per-(model, stone) observed fitness, and total request tracking for
the confidence ramp. The existing cumulative counters and persistence can remain
alongside.

**Advisor algorithm** (`domain/advisor.rs`): The current Worst-Fit Decreasing +
water-fill algorithm is VRAM-only. Replace entirely with demand-pressure-driven
placement scoring that considers fitness, demand weight, workload affinity, and
utilization. The `GpuSlot`/`ModelSlot` input types need fitness and demand
fields. The output type needs typed `Recommendation` structs with kind,
priority, and actuation metadata.

**Advisor task** (`tasks/advisor.rs`): Add manual trigger support (receive from
channel or direct function call). Add post-benchmark trigger (listen for
`benchmark.completed` event).

**AppState** (`app_state.rs`): Add `DemandLedger` field (or integrate into
existing metrics). Add actuation mode setting.

### What Is New

**`DecayCounter`** — Exponentially weighted counter (~16 bytes). Pure type with
`record(now)` and `rate(now, half_life)` methods.

**`DemandLedger`** — Aggregates DecayCounters: per-capability, per-model,
per-(model, stone) fitness, cold-load tracking. Consumed by advisor.

**GPU projected fitness table** — Maps GPU name/generation to relative
performance estimate. Bootstraps fitness at T=0 without benchmarks.

**Fitness resolver** — Three-source priority: observed > benchmarked > projected.
Returns fitness value + source tag for recommendation transparency.

**Pressure engine** — Computes `demand / capacity` per capability from
DemandLedger + topology + fitness. Core input to the new advisor.

**Actuation module** — Calls `PATCH .../services/{service}/env` on Moss for
parallelism changes. Coordinates drain → patch → restart → health-wait → resume.

**`POST /api/advisor/evaluate`** — Synchronous manual evaluation endpoint.

### Implementation Phases

**Phase A — Demand Ledger + Capability Tracking:**
`DecayCounter` type, capability field on `MetricEvent`, `DemandLedger` wired
from proxy through metrics processor, exposed in snapshot.

**Phase B — Fitness Integration:**
GPU projected fitness table, observed fitness accumulator from proxy tok/s,
three-source fitness resolver, pass into advisor inputs.

**Phase C — Pressure Engine + New Advisor:**
Capability pressure computation, confidence ramp, new `advise_topology()`
producing typed `Recommendation` list, workload separation and
demotion/promotion logic.

**Phase D — Actuation:**
Manual trigger endpoint, actuation mode setting, Moss env write-back with
restart coordination, dashboard "Apply" button.

## Consequences

- The advisor becomes demand-aware, producing recommendations that reflect actual
  usage rather than static VRAM heuristics alone.
- Fitness-weighted placement ensures hot models land on the fastest eligible GPU,
  not just any GPU with enough VRAM.
- The three-stage fitness bootstrap means the system is useful from T=0 (GPU name
  heuristic) and progressively improves.
- The demand ledger adds minimal memory overhead (~16 bytes per counter, no
  histograms or time-series storage).
- Auto-tune mode for parallelism closes the loop: the orchestrator can
  autonomously optimize `NUM_PARALLEL` without operator intervention.
- Breaking change: the existing advisor algorithm (pure VRAM BFD + water-fill)
  is replaced, not extended. No backward-compatibility shims.
