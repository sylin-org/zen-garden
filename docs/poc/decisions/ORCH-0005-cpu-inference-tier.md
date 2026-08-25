---
audience: [developer, ai]
doc_type: decision
status: proposed
last_verified: 2026-02-19
---

# ORCH-0005: CPU Inference Tier — Thin Client Stones via `ollama-cpu` Offering

**Date**: 2026-02-19
**Status**: Proposed
**Applies to**: `zen-garden-ollama-orchestrator`, `moss`, `garden-common`
**Depends on**: ORCH-0003 (fitness profiler), ORCH-0004 (gateway announcement)

## Context

The garden currently assumes all Ollama inference happens on GPU-equipped
stones. Tiering, routing, and profiling all use VRAM as the primary resource
dimension (`vram_total_bytes`, `vram_budget_bytes`).

Many gardens include thin client machines (e.g. Dell Wyse 5070, 4-core Celeron,
8 GB RAM, no GPU) that are unsuitable for generation but capable of running
lightweight embedding and tiny completion models via CPU-only Ollama. These
machines are currently excluded from the inference pool entirely.

### Opportunity

Small embedding models (`all-minilm:latest` at 43 MB, `nomic-embed-text:latest`
at 261 MB) run comfortably on CPU. Offloading embedding work to thin clients:

- Frees GPU queue slots for generation/vision (fewer evictions, lower latency)
- Adds parallelism for embedding-heavy applications (10 thin clients ≈ 50–150 req/s)
- Uses idle hardware that's already deployed and managed

### Constraints

- CPU thin clients report 0 GPU VRAM — the current tiering logic cannot place them
- Ollama on CPU is 10–50× slower than GPU for generation; only small models are viable
- The orchestrator must route to CPU nodes when appropriate, overflow to GPUs when not

## Decision

### 1. `ollama-cpu` Offering

Thin client stones announce a distinct offering: `ollama-cpu` (instead of
`ollama`). This is a **deployment constraint** — Moss on a GPU stone only
accepts `ollama`, Moss on a thin client only accepts `ollama-cpu`. The operator
cannot accidentally install the wrong one.

The offering tag is external identity (mDNS service type), not an internal
routing boundary.

### 2. Orchestrator Handles Both Offerings

The orchestrator discovers both `ollama` and `ollama-cpu` stones and merges
them into **one unified routing pool**. The gateway registration advertises:

```json
{
  "fqn": "ollama:orchestrator",
  "port": 21434,
  "handler_for": ["ollama", "ollama-cpu"],
  "protocol": "http",
  "uri_template": "http://{host}:{port}",
  "source": "zen-garden.ollama.orchestrator"
}
```

Any client doing `FIND ollama` or `FIND ollama-cpu` gets routed to the
orchestrator. Direct access bypasses the orchestrator as usual.

### 3. Workspace Memory (Replaces VRAM-Only Tiering)

The concept of "VRAM" is generalised to **Workspace Memory** — the memory
available for model loading, regardless of backing hardware:

| Stone type     | Workspace Memory source        | How determined             |
|---------------|-------------------------------|---------------------------|
| GPU stone      | VRAM (from GPU driver)         | Auto-detected by Ollama    |
| CPU thin client| System RAM for inference       | Operator-configured        |
| Hybrid (iGPU)  | Shared memory carved out       | Driver + config            |

#### Field renames

| Current name          | New name               | Notes                           |
|----------------------|------------------------|---------------------------------|
| `vram_total_bytes`   | `workspace_bytes`      | Total workspace available       |
| `vram_budget_bytes`  | `workspace_budget_bytes`| Usable budget (after overhead)  |
| `vram_bytes` (model) | `workspace_bytes`      | Memory needed to load model     |
| `Tier.vram_bytes`    | `Tier.workspace_bytes` | Tier capacity threshold         |

#### Precedence for workspace detection

1. If Ollama reports GPU VRAM > 0 → use that (GPU stone, auto-detected)
2. Else if `workspace_budget_mb` is set in stone config → use that (CPU stone)
3. Else → stone is not routable for inference (no workspace declared)

### 4. Tiering: Natural Separation

With Workspace Memory, CPU thin clients form a low tier automatically:

```
Tier "2G"   (workspace = 2 GiB)  → thin clients with all-minilm, nomic-embed-text
Tier "8G"   (workspace = 8 GiB)  → RTX 3050, RTX 3060 Ti
Tier "24G"  (workspace = 24 GiB) → RX 7900 XTX
```

Routing already prefers the **smallest viable tier**. Embedding models (43–261 MB)
fit in the 2G tier, so they route to thin clients first. When thin clients are
busy, they overflow to GPU tiers. Generation models (4–19 GB) don't fit in the
2G tier and route exclusively to GPU tiers.

### 5. Fitness Profiling

No special-casing needed. The profiler benchmarks whatever models are installed
on each stone:

- Thin client with `all-minilm` → profiler runs embedding benchmark → gets a
  Fast or Degraded verdict (CPU embedding is slower but functional)
- If someone installs `llama3.1:8b` on a thin client → profiler runs generate
  benchmark → 1.5 tok/s → Vetoed or Blocked → router deprioritises or blocks

The profiler is the safety net for misconfiguration.

### 6. Instance Metadata: `compute_type`

Each `OllamaInstance` gains an optional `compute_type` field:

```rust
pub enum ComputeType {
    Gpu,
    Cpu,
}
```

Derived from the offering tag: `ollama` → `Gpu`, `ollama-cpu` → `Cpu`.
Routing does **not** use this field. It's metadata for:

- Dashboard display (CPU/GPU column grouping)
- `/v1/stones` API response
- Operational visibility

## Consequences

### Positive

- **GPU queue relief**: Embedding requests served by thin clients, GPUs stay
  loaded with completion/vision models, fewer evictions
- **Linear scaling**: 10 thin clients at 5–15 embed req/s each = 50–150 req/s
  of embedding capacity without touching a GPU
- **Unified routing**: One orchestrator, one pool, overflow in both directions
- **Zero routing changes**: Tiering, fitness, blocked verdict all work as-is
  with the Workspace Memory rename
- **Offering separation**: `ollama-cpu` on mDNS means clients can optionally
  target CPU nodes directly for embedding-specific workflows
- **Existing hardware**: Uses machines already deployed in the garden

### Negative

- **Rename scope**: `vram_*` → `workspace_*` touches types, API responses,
  dashboard labels, documentation. Mechanical but broad.
- **Config obligation**: CPU stones must have `workspace_budget_mb` in their
  config (Moss/stone.toml). No auto-detection for system RAM budget.
- **Discovery complexity**: Orchestrator listens for two mDNS service types
  instead of one.

### Risks

- **Operator misconfiguration**: Wrong `workspace_budget_mb` could cause OOM on
  thin clients. Mitigated by fitness profiler (errors → Blocked).
- **CPU embedding latency**: Thin clients are slower than GPUs. If embedding
  latency is critical, the fitness score will degrade thin clients and routing
  will prefer GPUs. The system self-corrects.

## Implementation Plan

### Phase 1: Workspace Memory rename (code-only, no behaviour change)

1. Rename `vram_*` fields to `workspace_*` across types, API, dashboard
2. Update extension API (`/v1/stones`, `/v1/models`) field names
3. Update all tests

### Phase 2: Config-based workspace budget

4. Add `workspace_budget_mb` to stone config (moss.toml)
5. Moss reports configured value when Ollama reports 0 GPU VRAM
6. Orchestrator uses whichever value is available (GPU auto-detect > config)

### Phase 3: `ollama-cpu` offering support

7. Define `ollama-cpu` offering in offering taxonomy
8. Orchestrator discovery task: listen for both `ollama` and `ollama-cpu`
9. Gateway announcement: `handler_for: ["ollama", "ollama-cpu"]`
10. Tag instances with `ComputeType::Gpu` or `ComputeType::Cpu` from offering
11. Dashboard: show compute type label per stone

### Phase 4: Thin client deployment

12. Install Ollama (CPU mode) + Moss on thin clients
13. Configure `workspace_budget_mb = 2048` and offering `ollama-cpu`
14. Install embedding models (`all-minilm`, `nomic-embed-text`)
15. Orchestrator discovers, profiles, and routes automatically

## References

- ORCH-0003: Fitness profiler — benchmarks all installed models per stone
- ORCH-0004: Gateway announcement — orchestrator registers as handler for offerings
- ORCH-0002: Routing safety net — smallest viable tier, fallback sweep
- OFFER-0001: Offering taxonomy — `ollama-cpu` would be a new offering type
