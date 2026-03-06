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
