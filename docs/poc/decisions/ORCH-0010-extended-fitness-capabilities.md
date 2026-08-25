# ORCH-0010: Extended Fitness Capabilities — Tools and Think

**Date**: 2026-03-06
**Status**: Accepted
**Applies to**: `zen-garden-ollama-orchestrator` crate
**Depends on**: [ORCH-0003](ORCH-0003-fitness-profiler.md) (Fitness Profiler)

## Context

The fitness profiler (ORCH-0003) benchmarks three capabilities: Generate, Embed,
and Vision. Each maps to a distinct GPU compute profile, justifying separate
measurement. However, two capability categories used by the recommendation engine
lack empirical fitness data:

1. **Tools** (`?capability=tools`) — models producing structured JSON function
   calls. The recommendation engine filters on the `tools` tag and scores by
   Generate fitness, which measures throughput but not **correctness** of
   structured output.

2. **Thinking** (`?capability=thinking`) — models producing extended
   chain-of-thought reasoning (2,000-10,000+ tokens). The recommendation engine
   scores by Generate fitness, which measures short-burst throughput (~80 tokens)
   and does not capture **sustained throughput under KV cache pressure**.

### Why These Belong in the Fitness Benchmark

Both produce genuinely different `(model, capability, stone)` signals — not just
per-model signals:

**Tools vary by stone because:**
- Different quantizations across stones (Q4_K_M on 8 GB vs Q8_0 on 24 GB)
  directly affect structured output reliability. Lower-precision quantization
  pushes token probabilities closer to decision boundaries where JSON syntax
  tokens compete with natural language tokens.
- VRAM pressure on constrained GPUs can degrade generation quality when KV cache
  is tight, causing the model to lose track of the tool schema mid-generation.
- FP16 rounding differences across GPU architectures (Turing vs Ampere vs Ada)
  can flip token probabilities at structured output decision boundaries.
- Different Ollama versions across stones may have different sampling
  implementations affecting structured output consistency.

**Thinking varies by stone because:**
- Short-burst tok/s (80 tokens, current Generate benchmark) does not predict
  sustained tok/s over 2,000-10,000 tokens. KV cache growth is linear with
  sequence length; GPUs with tight VRAM headroom degrade as the thinking chain
  extends.
- Thermal throttling manifests over 30-60 second sustained generation but not
  during 2-5 second bursts.
- VRAM swapping behavior (Ollama's partial offload to CPU) triggers at long
  sequences on constrained GPUs, causing dramatic throughput cliffs that the
  short Generate benchmark cannot detect.

The existing `Verdict` enum already blends correctness with performance:
`Blocked` means "produced zero output" — a correctness signal. Tools and Think
extend this to more granular correctness/sustainability signals using the same
`(model, capability, stone) -> Verdict` matrix.

## Decision

Add two new variants to the `Capability` enum in `domain/fitness.rs`:

```rust
pub enum Capability {
    Generate,
    Embed,
    Vision,
    Tools,   // NEW
    Think,   // NEW
}
```

Both produce entries in the existing `GpuMatrix` with standard `Verdict` values
(Fast/Degraded/Vetoed/Blocked). No new data structures needed.

### Tools Benchmark

**What it tests**: Can this model, on this stone, reliably produce valid
structured tool calls at acceptable speed?

**API call**: `POST /api/chat` with `tools` parameter — the same endpoint
real tool-use requests hit.

**Test harness** (5 prompts):

Each prompt provides a tool schema and a user message that should trigger a
specific tool call. The benchmark validates the response structurally:

| Prompt | Tool Schema | Expected Behavior |
|--------|-------------|-------------------|
| "What's the weather in Tokyo?" | `get_weather(city: string)` | Single tool call, correct function name, string argument |
| "Calculate 15% tip on $84.50" | `calculate(expression: string)` | Single tool call, valid expression argument |
| "Find recent papers on transformers, limit 3" | `search(query: string, limit: integer)` | Correct types: string + integer |
| "What time is it in London and Tokyo?" | `get_time(city: string)` | Multiple tool calls (same function, different args) |
| "Search for 'rust async' and get weather in Berlin" | Both `search` and `get_weather` | Multiple tool calls (different functions) |

**Validation per response:**
1. Response contains `tool_calls` (or `message.tool_calls`) array
2. Each tool call has a valid `function.name` matching a provided tool
3. Each tool call has `function.arguments` parseable as JSON
4. Argument keys match the schema (extra keys tolerated, missing required keys fail)
5. Argument types match schema (string/integer/boolean)

**Scoring:**
- 5/5 valid: **pass** (tok/s and cold start determine Fast vs Degraded)
- 3-4/5 valid: **flaky** (maps to Degraded regardless of speed)
- 0-2/5 valid: **fail** (maps to Vetoed if some output, Blocked if no tool calls at all)

**Verdict computation:**

```rust
Capability::Tools => {
    if valid_count == 0 {
        Verdict::Blocked
    } else if valid_count < 3 {
        Verdict::Vetoed
    } else if valid_count < total_prompts {
        Verdict::Degraded  // flaky — works but unreliable
    } else if cold_start_ms < 30_000 && tokens_per_second > 5.0 {
        Verdict::Fast
    } else if cold_start_ms < 90_000 && tokens_per_second > 1.0 {
        Verdict::Degraded
    } else {
        Verdict::Vetoed
    }
}
```

**OllamaClient addition**: `benchmark_chat_tools()` — sends a chat request with
`tools` array and `stream: false`, returns the response body for validation.

### Think Benchmark

**What it tests**: Can this model, on this stone, sustain generation throughput
over extended output sequences (2,000+ tokens)?

**API call**: `POST /api/generate` with `num_predict: 2000` — same as Generate
but measuring sustained performance over 25x the token count.

**Test prompts** (3 prompts, designed to elicit long reasoning):

| Prompt | Purpose |
|--------|---------|
| "Solve step by step: A farmer has 3 fields..." (multi-step math) | Triggers extended reasoning chain |
| "Compare and contrast 5 sorting algorithms..." (structured analysis) | Triggers organized long-form output |
| "Write a detailed plan for building a..." (planning task) | Triggers sequential planning output |

The prompts do not require `<think>` block support — any model tagged with
`thinking` capability benefits from sustained throughput measurement. The key
signal is tok/s at token 1500-2000 vs token 0-100, not whether the model uses a
specific reasoning format.

**Verdict computation:**

```rust
Capability::Think => {
    // Thinking users expect slower responses — lower tok/s bar
    // but sustainability is critical
    if tokens_per_second <= 0.0 {
        Verdict::Blocked
    } else if cold_start_ms < 60_000 && tokens_per_second > 3.0 {
        Verdict::Fast
    } else if cold_start_ms < 120_000 && tokens_per_second > 0.5 {
        Verdict::Degraded
    } else {
        Verdict::Vetoed
    }
}
```

The thresholds are more relaxed than Generate:
- Cold start allowance: 60s (vs 30s) — thinking models are often larger
- tok/s floor for Fast: 3.0 (vs 5.0) — sustained throughput is harder
- tok/s floor for Degraded: 0.5 (vs 1.0) — some sustained output is better than none

**OllamaClient**: Reuses existing `benchmark_generate()` with higher
`num_predict` (2000 vs 80).

### Capability Detection

The `capabilities_to_test()` function adds:

```rust
if model.capabilities.iter().any(|c| c == "tools") {
    modes.push(Capability::Tools);
}
if model.capabilities.iter().any(|c| c == "thinking") {
    modes.push(Capability::Think);
}
```

Models without the `tools` or `thinking` tag skip these benchmarks entirely.
No wasted benchmark time on models that don't support these capabilities.

### Recommendation Engine Integration

The `fitness_capability()` mapping in `recommendation.rs` currently maps both
`tools` and `thinking` to `Capability::Generate` as a fallback:

```rust
// Before (ORCH-0003):
"tools" | "thinking" => Some(Capability::Generate)

// After (ORCH-0010):
"tools" => Some(Capability::Tools),
"thinking" => Some(Capability::Think),
// Falls back to Generate when no Tools/Think entry exists in the matrix
```

When the matrix has a Tools or Think entry for a model on a stone, the
recommendation engine uses it. When no entry exists (benchmark not yet run),
it falls back to the Generate verdict — preserving current behavior.

### Sample Data

```
GpuMatrix after ORCH-0010:

                        stone-quiet-lens (8G, Q4_K_M)   stone-azure-pool (24G, Q8_0)
llama3.1:8b
  generate              Fast (28 tok/s)                  Fast (45 tok/s)
  tools                 Degraded (flaky: 3/5 valid)      Fast (5/5 valid, 42 tok/s)
qwen2.5:7b
  generate              Fast (32 tok/s)                  Fast (50 tok/s)
  tools                 Fast (5/5 valid, 30 tok/s)       Fast (5/5 valid, 48 tok/s)
deepseek-r1:14b
  generate              Degraded (8 tok/s)               Fast (35 tok/s)
  think                 Vetoed (1.2 tok/s @ 2K tokens)   Fast (28 tok/s sustained)
qwen3:8b
  generate              Fast (30 tok/s)                  Fast (48 tok/s)
  tools                 Fast (5/5, 28 tok/s)             Fast (5/5, 45 tok/s)
  think                 Degraded (2.1 tok/s sustained)   Fast (38 tok/s sustained)
```

This shows the signals current Generate-only fitness cannot provide:
- llama3.1 is **flaky** at tools on 8 GB Q4 but reliable on 24 GB Q8
- deepseek-r1 is **Fast** for short generation on 8 GB but **Vetoed** for
  sustained thinking (KV cache pressure at 2K tokens)

### Benchmark Time Impact

Current benchmark per model per stone: ~3 capabilities x 5-10 prompts = ~30
requests. Adding Tools (5 prompts) and Think (3 prompts) for tagged models
adds ~8 requests per applicable model. Most models in a typical garden have
2-4 capabilities tagged, so the benchmark grows by ~15-25% for a full run.

Think prompts with `num_predict: 2000` take longer per-prompt (~30-60s vs ~5s
for Generate), adding ~2-3 minutes per thinking-capable model per stone. This
is acceptable for a manually-triggered benchmark.

## Consequences

### Positive

- `?capability=tools` recommendations backed by empirical correctness data
  instead of Generate speed proxy
- `?capability=thinking` recommendations reflect sustained throughput, not burst
- Router can deprioritize stones where a model is flaky at tools (Degraded)
  in favor of stones where it's reliable (Fast) — same model, different verdict
- Quantization impact on structured output becomes visible in the matrix
- No new data structures — same GpuMatrix, same Verdict enum, same persistence

### Negative

- Think benchmark adds ~2-3 minutes per thinking-capable model per stone
  (mitigated: manual trigger, yield-to-traffic)
- Tools validation logic adds complexity to the benchmark runner (~60 lines
  of JSON schema validation)
- False negatives possible: a model might produce valid tool calls on retry
  that it failed on first attempt (mitigated: 5 prompts provides statistical
  signal)

### Risks

**Risk:** Tool schema validation is too strict/lenient, producing incorrect
verdicts.
**Mitigation:** Conservative validation — require function name and parseable
JSON arguments, tolerate extra keys. Err toward Degraded rather than Blocked
for edge cases.

**Risk:** Think benchmark prompts don't reliably elicit 2000-token output from
all thinking-tagged models.
**Mitigation:** Use `num_predict: 2000` to force output length. The prompt
encourages verbosity but the cap ensures measurement regardless of model
cooperation.

**Risk:** Non-deterministic generation causes different verdicts across
benchmark runs.
**Mitigation:** 5 prompts for Tools provides robustness. Think measures
aggregate tok/s across the full output, smoothing variance. Verdicts use
conservative thresholds with buffer zones.

## Implementation Plan

### Modified Files

| File | Change |
|------|--------|
| `domain/fitness.rs` | Add `Capability::Tools` and `Capability::Think` variants, verdict computation |
| `tasks/benchmark.rs` | Add `bench_tools()` and `bench_think()` functions, tool call validation, capability detection |
| `infra/ollama_client.rs` | Add `benchmark_chat_tools()` method |
| `domain/recommendation.rs` | Update `fitness_capability()` mapping |
| `assets/dashboard.html` | Show Tools/Think columns in fitness matrix table |

### New Constants

| Constant | Value | Purpose |
|----------|-------|---------|
| `TOOLS_PROMPTS` | 5 prompt+schema pairs | Tool-calling test suite |
| `THINK_PROMPTS` | 3 long-reasoning prompts | Sustained generation test suite |
| `THINK_NUM_PREDICT` | 2000 | Token cap for thinking benchmark |

### Estimated Scope

- Domain types: ~20 lines (2 enum variants + verdict arms)
- Tool validation: ~80 lines
- Benchmark functions: ~120 lines (bench_tools + bench_think)
- Client method: ~30 lines (benchmark_chat_tools)
- Recommendation mapping: ~5 lines
- Dashboard: ~20 lines (column additions)
- Total: ~275 lines of new/modified code

## References

- [ORCH-0003](ORCH-0003-fitness-profiler.md) — Fitness Profiler (base system)
- [ORCH-0009](ORCH-0009-demand-weighted-topology-advisor.md) — Demand-Weighted Topology Advisor (consumes fitness data)
- Ollama `/api/chat` — `tools` parameter for function calling
- Ollama `/api/show` — `capabilities` field includes `tools` and `thinking` tags
