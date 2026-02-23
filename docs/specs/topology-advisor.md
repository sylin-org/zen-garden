# Topology Advisor Specification

**Purpose:** How the Ollama Orchestrator evaluates GPU topology and recommends optimal model placement and parallelism settings.  
**Audience:** Developers working on the orchestrator, operators tuning stone configurations.  
**Source:** `src/orchestrators/ollama/src/domain/advisor.rs` (pure computation) and `src/orchestrators/ollama/src/tasks/advisor.rs` (background task).

---

## Table of Contents

1. [Overview](#overview)
2. [Evaluation Modes](#evaluation-modes)
3. [Data Collection](#data-collection)
4. [Algorithm](#algorithm)
   - [Phase 1: Worst-Fit Decreasing Placement](#phase-1-worst-fit-decreasing-placement)
   - [Phase 2: Water-Fill Parallelism](#phase-2-water-fill-parallelism)
   - [Phase 3: Workload Weighting](#phase-3-workload-weighting)
5. [KV Cache Estimation](#kv-cache-estimation)
6. [VRAM Projection](#vram-projection)
7. [Constants](#constants)
8. [Trigger Schedule](#trigger-schedule)
9. [Recommendation Threshold](#recommendation-threshold)
10. [Worked Examples](#worked-examples)
11. [Future: Hot Evaluation](#future-hot-evaluation)

---

## Overview

The topology advisor answers: *"Given the GPUs we have and the models we serve, what is the best way to distribute models across GPUs and what `OLLAMA_NUM_PARALLEL` should each stone use?"*

It runs as a pure-computation function — no I/O, no async, no locks. The background task feeds it snapshots of the current state and publishes results to the dashboard.

```
GpuSlot[]  ×  ModelSlot[]  →  advise_topology()  →  TopologyAdvice
                                                        ├── per-GPU: models, recommended_parallel, VRAM breakdown
                                                        └── reasoning[]  (human-readable explanation)
```

---

## Evaluation Modes

### Cold (T=0) — Currently Active

Uses **projected** VRAM for all models. No usage history required.

- VRAM source: `size_disk × 1.1` (always available from Ollama's `/api/tags`)
- Model set: every model in the global registry
- Goal: sensible defaults from the moment stones are discovered

### Hot (Future)

Uses **measured** VRAM from `/api/ps` plus demand history (EMA request rates).

- VRAM source: live `size_vram` observations
- Model set: only models actively being requested
- Goal: runtime-optimised placement weighted by actual traffic patterns

---

## Data Collection

### GPU Slots

Built from the orchestrator's instance registry (`gpu_slots_from_instances`):

| Field | Source |
|-------|--------|
| `id` | Instance endpoint (e.g. `http://192.168.1.50:11434`) |
| `label` | `"{stone_name} / {gpu_name}"` |
| `vram_bytes` | `vram_budget_bytes` — usable VRAM after system reservation |
| `current_parallel` | `OLLAMA_NUM_PARALLEL` detected from Moss service env |

Only **routable** (healthy) instances are included.

### Model Slots (Cold)

Built from the model registry (`model_slots_projected`):

| Field | Source |
|-------|--------|
| `name` | Model name (e.g. `llama3.2:3b`) |
| `vram_bytes` | `size_disk × 1.1` (projected) |
| `is_embedding` | `"embedding" ∈ capabilities` |
| `kv_cache_per_slot` | `estimate_kv_cache()` heuristic (see below) |
| `vram_source` | `Projected` or `Measured` |

Models with `size_disk == 0` are skipped (no data available).

---

## Algorithm

### Phase 1: Worst-Fit Decreasing Placement

**Goal:** Spread models across GPUs to maximise per-GPU headroom for parallelism.

```
1. Sort models by vram_bytes descending (largest first)
2. For each model:
   a. Find GPU with the MOST remaining VRAM where:
      remaining ≥ model.vram_bytes + MIN_HEADROOM (256 MB)
   b. If found → place model on that GPU, decrement remaining VRAM
   c. If not → mark model as "unplaced" (flagged in reasoning)
```

**Why Worst-Fit (not Best-Fit)?** Best-Fit packs models tightly, minimising waste per GPU — but that leaves no room for parallelism slots. Worst-Fit spreads models out so each GPU retains maximum headroom for KV cache allocation.

### Phase 2: Water-Fill Parallelism

**Goal:** Allocate concurrent request slots from the VRAM headroom left after placement.

For each GPU:

```
free = vram_bytes − Σ model.vram_bytes    (VRAM remaining after model weights)
vram_for_kv = free − MIN_HEADROOM         (reserve 256 MB for Ollama overhead)
max_kv = max(model.kv_cache_per_slot)     (bottleneck: largest KV cost on this GPU)
max_slots = floor(vram_for_kv / max_kv)   (how many slots fit)
max_slots = min(max_slots, MAX_PARALLEL)   (cap at 16)
```

The name "water-fill" comes from the analogy: imagine pouring water (VRAM) into a container whose floor height is set by model weights. The depth of water above the floor determines how many parallel slots can be carved out, partitioned by the tallest KV cache barrier.

### Phase 3: Workload Weighting

Not all parallelism is equal. The advisor applies workload-specific caps:

| GPU Workload | Rule | Rationale |
|-------------|------|-----------|
| **All embedding** | Use full water-fill (up to 16) | Embedding inference is stateless, context-free — high parallelism has no quality cost |
| **Chat/generate only** | `clamp(max_slots, 1, 4)` | Chat models use long KV caches per active request; too many slots starves context length and increases latency |
| **Mixed** | Same cap as chat | The chat model is the bottleneck |
| **No models placed** | Default to 1 | GPU is idle |

---

## KV Cache Estimation

Each parallel slot requires VRAM for its KV cache (key-value attention state). The estimate comes from `estimate_kv_cache()`:

### When parameter count is known

| Parameter Range | KV Estimate | Example Models |
|----------------|-------------|----------------|
| > 30B | 1,600 MB | `llama3:70b`, `qwen2:72b` |
| > 10B | 600 MB | `llama3:13b`, `deepseek-r1:14b` |
| > 3B | 300 MB | `llama3:8b`, `granite3.3:8b` |
| ≤ 3B | 150 MB | `llama3.2:3b`, `tinyllama` |

### When parameter count is unknown

Falls back to a percentage of the VRAM footprint:

```
estimate = vram_bytes / 25    (~4% of model VRAM)
clamped to [100 MB, 2,000 MB]
```

Rationale: for a 7B model (~4.5 GB VRAM), 4% gives ~180 MB, which aligns with observed Ollama KV allocations.

### Embedding models

Always returns **80 MB** regardless of size. Embedding models don't maintain conversational KV state — their attention is computed in a single forward pass over the input.

---

## VRAM Projection

For cold evaluation, model VRAM is projected from the on-disk size:

```
projected_vram = size_disk × 1.1
```

| Factor | Explanation |
|--------|-------------|
| `size_disk` | GGUF file size from Ollama's `/api/tags` — always available |
| `× 1.1` | 10% overhead for runtime structures: activation buffers, scratch space, Ollama bookkeeping |

This works well because quantised GGUF models are memory-mapped directly into VRAM — the file *is* the weight tensor layout. The 10% margin covers non-weight allocations.

---

## Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `DEFAULT_KV_CACHE_CHAT` | 300 MB | Fallback KV per slot for chat models |
| `DEFAULT_KV_CACHE_EMBED` | 80 MB | Fallback KV per slot for embedding models |
| `MIN_HEADROOM` | 256 MB | Reserved VRAM (Ollama runtime + safety margin) |
| `MAX_PARALLEL` | 16 | Hard cap on recommended parallelism |
| `DISK_TO_VRAM_FACTOR` | 1.1 | Disk-to-VRAM projection multiplier |

---

## Trigger Schedule

The advisor task (`tasks/advisor.rs`) recomputes on three triggers:

| Trigger | Timing | Purpose |
|---------|--------|---------|
| **Initial** | 15 s after startup | First T=0 evaluation once discovery populates the registry |
| **Reactive** | 5 s debounce after `registry.updated`, `models.updated`, or `tiers.updated` events | Respond to stone discovery, model pulls/deletes, health changes |
| **Periodic** | Every 5 minutes | Safety net — ensures dashboard stays fresh even without events |

The debounce prevents thrashing during rapid discovery bursts (e.g. 3 stones coming online simultaneously fire many events within seconds).

---

## Recommendation Threshold

The advisor only flags `has_recommendations = true` when:

1. **Any model is unplaced** — doesn't fit on any GPU, or
2. **Any GPU's recommended parallelism differs from current by ≥ 2** — small differences (±1) are within noise and not worth flagging

This avoids noisy flip-flopping between adjacent values.

---

## Worked Examples

### Example 1: Three Stones, 18 Models

Setup:
- **stone-alpha**: RTX 3060 Ti, 8 GB VRAM budget
- **stone-beta**: RX 7900 XTX, 24 GB VRAM budget
- **stone-gamma**: RTX 3050, 8 GB VRAM budget
- 18 models total: mix of 3B–8B chat, embedding, vision

Phase 1 (placement): Models sorted largest-first. The big models (8B chat, ~5 GB projected) go to stone-beta first (most headroom). Smaller embedding models (~300–600 MB) spread across all three GPUs via worst-fit.

Phase 2 (water-fill): stone-beta has ~10 GB free after placing several large models. With a 300 MB KV bottleneck: `floor((10 GB − 256 MB) / 300 MB) = 33`, capped to 16. But Phase 3 clamps to 4 (chat workload).

Phase 3 (workload): If stone-alpha ended up with only embedding models, it gets full water-fill. stone-beta and stone-gamma with chat models are capped at 4.

### Example 2: Single GPU, Embedding-Only

Setup:
- **stone-embed**: RTX 3090, 24 GB budget
- 3 embedding models: 512 MB, 800 MB, 256 MB (total: 1.6 GB)

Phase 1: All three fit easily.  
Phase 2: `free = 24 GB − 1.6 GB = 22.4 GB`, `vram_for_kv = 22.4 − 0.256 = 22.1 GB`, `max_kv = 100 MB`. `floor(22.1 GB / 100 MB) = 221`, capped to 16.  
Phase 3: All embedding → use full water-fill → **recommended_parallel = 16**.

### Example 3: Model That Doesn't Fit

Setup:
- **stone-tiny**: GTX 1650, 4 GB budget
- Model: `llama3:70b` (projected ~38.5 GB)

Phase 1: `4 GB < 38.5 GB + 256 MB` → model is unplaced.  
Reasoning: `"⚠ llama3:70b (38500 MB) cannot fit on any GPU — needs larger VRAM or fewer models"`.  
`has_recommendations = true`.

---

## Future: Hot Evaluation

The architecture supports a second evaluation path using measured data:

- **`model_slots_measured()`** — only models with live `/api/ps` VRAM observations
- **Demand weighting** — EMA accumulator per (model, request_type) from MetricEvent stream
- **Request-type affinity** — `RequestKind` enum (Embed/Generate/Chat) for parallelism scoring in routing

This would enable recommendations like: *"stone-beta handles 80% of chat traffic — increase its parallelism from 2 → 4"* or *"nomic-embed-text gets 60% of requests — move it to the fastest GPU"*.

The pure-computation design of `advise_topology()` means the hot path would call the same algorithm with different inputs — measured VRAM and demand-weighted model priorities — without changing the placement/water-fill logic.
