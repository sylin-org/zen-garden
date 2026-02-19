---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-02-18
---

# ORCH-0003: Fitness Profiler — Model Benchmark System

**Date**: 2026-02-18
**Status**: Accepted — Design complete, implementation pending
**Applies to**: `zen-garden-ollama-orchestrator` crate
**Depends on**: [ORCH-0002](ORCH-0002-routing-safety-net.md) (Safety Net — never block available models)

## Context

The orchestrator routes requests to stones based on VRAM tiers and queue depth,
but has **no empirical data** about how well each model actually performs on each
stone's hardware. A model that fits in VRAM on a small GPU may still produce
unacceptably slow inference (0.5 tok/s) while performing well on a larger card.

### Problems

1. **Blind routing**: A 7B model routes to the cheapest 8 GB tier, even if that
   card produces 2 tok/s while a 24 GB card produces 40 tok/s.
2. **No visibility**: Operators have no dashboard data on model-per-stone
   performance characteristics.
3. **Capabilities untested**: Ollama reports capabilities (vision, tools,
   embedding, thinking) via `/api/show`, but the orchestrator never verifies
   they actually work on each stone.

### Constraints

- ORCH-0002 safety net: benchmarks inform routing but **never block** it.
  A model on a stone is always routable regardless of fitness verdict.
- Benchmarks must not interfere with live traffic — yield to real requests.
- Results persist across restarts (no re-benchmark on every boot).
- Manual trigger only (no automatic benchmarking on discovery).

## Decision

Implement a **fitness profiler** that benchmarks every installed model on every
stone, producing a fitness matrix used for advisory routing scores.

### Domain Model

```rust
/// Per-capability performance verdict for one model on one stone.
enum FitnessVerdict {
    /// Cold start < 30s AND tok/s > 5 — suitable for interactive use.
    Fast,
    /// Cold start < 90s AND tok/s > 1 — functional but slow.
    Degraded,
    /// Exceeds thresholds — model works but performance is poor.
    Vetoed,
    /// Not yet benchmarked.
    Unknown,
}

struct BenchmarkResult {
    model: String,
    stone_endpoint: String,
    stone_name: String,
    capability: BenchmarkMode,
    verdict: FitnessVerdict,
    cold_start_ms: u64,         // Time from request to first token
    tokens_per_second: f64,     // eval_count / eval_duration
    total_duration_ms: u64,
    timestamp: DateTime<Utc>,
}

enum BenchmarkMode {
    Generate,   // Text completion
    Embed,      // Embedding generation
    Vision,     // Image + text prompt
}
```

### Fitness Matrix

The fitness matrix is a `HashMap<(model, stone_endpoint), Vec<BenchmarkResult>>`
keyed by (model name, stone endpoint) with one entry per capability tested.

```
                    stone-quiet-lens (8G)   stone-azure-pool (24G)
llama3.2:latest     Fast (generate)         Fast (generate)
                    Fast (tools)            Fast (tools)
deepseek-r1:32b     Degraded (generate)     Fast (generate)
                    Degraded (thinking)     Fast (thinking)
qwen2.5vl:latest    Vetoed (generate)       Fast (generate)
                    Vetoed (vision)         Fast (vision)
all-minilm          Fast (embed)            Fast (embed)
```

### Benchmark Payloads

#### Generate (10 prompts, graduated complexity)

Prompts range from single-sentence to multi-paragraph, all with `num_predict: 80`
to cap output length. Example set:

1. "What is 2 + 2?"
2. "Explain why the sky is blue in two sentences."
3. "Write a haiku about a mountain."
4. ...through to...
10. "Compare and contrast REST and GraphQL APIs, covering authentication,
     versioning, and caching strategies."

#### Embed (5 texts, graduated length)

1. Single word: "photosynthesis"
2. One sentence: "The quick brown fox jumps over the lazy dog."
3. Short paragraph (~50 words)
4. Medium paragraph (~200 words)
5. Long passage (~500 words)

#### Vision (5 images, up to 1 MB each)

Static images bundled via `include_bytes!()` in `assets/benchmark/`:

| File | Content | Purpose |
|------|---------|---------|
| `01-simple-object.jpg` | Single everyday object | Baseline object recognition |
| `02-outdoor-scene.jpg` | Landscape / street scene | Scene description |
| `03-text-in-image.jpg` | Sign or document with text | OCR capability |
| `04-chart-or-diagram.jpg` | Simple chart or diagram | Data interpretation |
| `05-technical-diagram.jpg` | Circuit/architecture diagram | Complex visual reasoning |

Images are user-provided (JPEG, 100 KB – 1 MB each). The prompt for each:
"Describe what you see in this image in detail."

### Verdict Thresholds

| Verdict | Cold Start | Tokens/sec | Logic |
|---------|-----------|------------|-------|
| **Fast** | < 30 s | > 5 tok/s | Both conditions met |
| **Degraded** | < 90 s | > 1 tok/s | Both conditions met |
| **Vetoed** | ≥ 90 s | — | Either condition fails |

For **embed** mode: no tok/s metric. Verdict based on cold start and total
duration only (Fast < 5s, Degraded < 30s, Vetoed ≥ 30s).

### Execution Strategy

1. **Per-stone queues**: One benchmark task per stone, running in parallel
   across stones but serial within each stone.
2. **Smallest-first ordering**: Models sorted by `size_disk` ascending — fast
   models first for early results.
3. **Per-capability cutout**: Each model tested only for its declared
   capabilities (from `/api/show`). A text-only model skips vision payloads.
4. **Yield to traffic**: Before each benchmark request, check the stone's
   queue depth. If > 0, pause and retry after a delay.
5. **Incremental persistence**: After each model completes on a stone, write
   results to `{data_dir}/fitness.json`.

### Persistence

Results stored as JSON at `{data_dir}/fitness.json`:

```json
{
  "version": 1,
  "generated_at": "2026-02-18T14:30:00Z",
  "results": [
    {
      "model": "llama3.2:latest",
      "stone_endpoint": "http://10.0.0.5:11434",
      "stone_name": "stone-quiet-lens",
      "capability": "generate",
      "verdict": "Fast",
      "cold_start_ms": 2400,
      "tokens_per_second": 28.5,
      "total_duration_ms": 5200,
      "timestamp": "2026-02-18T14:25:12Z"
    }
  ]
}
```

Loaded on startup into `AppState.fitness_matrix`. Survives restarts.

### API Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/api/benchmark/start` | Start benchmark run |
| `POST` | `/api/benchmark/cancel` | Cancel running benchmark |
| `GET` | `/api/benchmark/results` | Current fitness matrix (JSON) |
| `GET` | `/api/benchmark/export` | Download fitness.json |

#### POST /api/benchmark/start

```json
{
  "scope": "full",
  "sync": false,
  "wipe": null
}
```

| Field | Values | Default | Description |
|-------|--------|---------|-------------|
| `scope` | `"full"`, `"stone:<name>"` | `"full"` | Benchmark all stones or one |
| `sync` | `true`, `false` | `false` | If true, pull missing models to each stone first |
| `wipe` | `null`, `"all"`, `"stone:<name>"` | `null` | Clear existing results before starting |

- `sync: false` = "test installed" — only benchmark models already on each stone.
- `sync: true` = "sync and test" — pull models to stones that don't have them,
  then benchmark. *This is a heavy operation.*

### Routing Integration

Fitness data is **advisory only** — it adjusts scoring within the tier sweep
but never eliminates candidates:

```rust
// Inside select_instance(), after finding candidates in a tier:
candidates.sort_by(|a, b| {
    let fa = fitness_score(model, &a.endpoint, &fitness_matrix);
    let fb = fitness_score(model, &b.endpoint, &fitness_matrix);
    // Primary: fitness (Fast > Degraded > Vetoed > Unknown)
    // Secondary: queue depth (ascending)
    fb.cmp(&fa).then(a.queue_depth.cmp(&b.queue_depth))
});
```

Fitness scoring (within a tier, not across tiers):

| Verdict | Score |
|---------|-------|
| Fast | 100 |
| Degraded | 50 |
| Unknown | 25 |
| Vetoed | 10 |

A `Vetoed` instance still receives traffic if it's the only one with the model
(ORCH-0002 safety net).

### Dashboard Integration

New "Fitness" section in the dashboard:

- **Run Benchmark** / **Re-run** / **Cancel** buttons
- Per-stone progress bar during benchmark execution
- Fitness matrix table: model × stone grid with colour-coded verdicts
  (green = Fast, yellow = Degraded, red = Vetoed, grey = Unknown)
- **Export** button to download `fitness.json`
- Scope selector: "Test installed" vs "Sync and test"
- Wipe selector: per-stone or all data

## Consequences

### Positive

- Operators get empirical performance data for every model/stone combination
- Routing becomes performance-aware without breaking the safety-net guarantee
- Dashboard provides clear visibility into which stones handle which models well
- Benchmarks are incremental — new stones or models can be tested individually
- Results persist — no redundant benchmarking across restarts

### Negative

- Benchmark runs consume GPU time (mitigated by yield-to-traffic)
- Image assets add ~5 MB to the binary (acceptable for `include_bytes!`)
- Manual trigger means fresh deployments start with Unknown fitness until run

### Risks

**Risk:** Benchmark results become stale after model updates or GPU driver changes.
**Mitigation:** Manual re-run with optional wipe. Dashboard shows timestamp
of each result.

**Risk:** "Sync and test" mode pulls many models, consuming disk on small stones.
**Mitigation:** Explicit opt-in via `sync: true`. Default is "test installed" only.

**Risk:** Benchmark payloads don't represent real workload characteristics.
**Mitigation:** Graduated complexity in prompts; 80-token cap prevents runaway
generation; real-world images for vision testing.

## Implementation Plan

### New Files

| File | Purpose |
|------|---------|
| `domain/fitness.rs` | `FitnessVerdict`, `BenchmarkResult`, `BenchmarkMode`, `FitnessMatrix` |
| `tasks/benchmark.rs` | Benchmark runner — per-stone parallel queues, yield-to-traffic |
| `api/benchmark_api.rs` | HTTP handlers for `/api/benchmark/*` |
| `assets/benchmark/` | 5 static JPEG images (user-provided) |

### Modified Files

| File | Change |
|------|--------|
| `app_state.rs` | Add `fitness_matrix: Arc<RwLock<FitnessMatrix>>`, load from disk on startup |
| `domain/routing.rs` | ~30 lines: fitness-aware candidate sorting within tiers |
| `tasks/snapshot_publisher.rs` | Include fitness data in dashboard JSON |
| `assets/dashboard.html` | Fitness matrix section, benchmark controls |
| `main.rs` | Register benchmark API routes, load fitness on startup |

### Estimated Scope

- Domain types: ~80 lines
- Benchmark runner: ~250 lines
- API handlers: ~120 lines
- Routing integration: ~30 lines
- Dashboard: ~150 lines HTML/CSS/JS
- Total: ~630 lines of new code

## References

- [ORCH-0002](ORCH-0002-routing-safety-net.md) — Routing Safety Net (prerequisite)
- Ollama `/api/show` — capabilities field
- Ollama `/api/generate` — `load_duration`, `eval_count`, `eval_duration` in response
