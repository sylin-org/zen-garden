---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-02-18
---

# ORCH-0002: Routing Safety Net — Never Refuse an Installed Model

**Date**: 2026-02-18
**Status**: Accepted — Implemented
**Applies to**: `zen-garden-ollama-orchestrator` crate, `domain::routing`

## Context

The Ollama orchestrator routes inference requests to the best available stone
based on VRAM tier matching. The original algorithm filtered tiers where
`tier.vram_bytes >= model.vram_bytes`, and returned a `NoViableTier` error
when no tier had sufficient VRAM.

This caused a critical failure mode: **a model explicitly installed by the user
on a stone would be refused if its VRAM requirement exceeded every available
tier's capacity**. For example:

- A 48 GB model is installed on a stone with a 24 GB GPU.
- Ollama loads it successfully (partial offload to RAM).
- The orchestrator refuses to route because `24 GiB < 48 GiB`.

The user's intent is unambiguous — they installed the model on that stone,
knowing its hardware. The orchestrator must honour that decision.

### Prior State

```rust
pub enum RoutingError {
    ModelNotFound(String),
    NoViableTier { model: String, vram_needed: u64 },  // ← the problem
    AllInstancesBusy { model: String },
    NoHealthyInstances,
}
```

When `vram_needed > max(tier.vram_bytes)`, the router returned `NoViableTier`
and the proxy responded with `503 Service Unavailable`.

## Decision

**A model that exists on any healthy stone is always routable.**

### Routing Cascade

The algorithm now uses a two-phase tier sweep:

1. **Preferred tiers** (VRAM ≥ model requirement): tried lowest-first
   (unchanged behaviour — route to cheapest adequate hardware).
2. **Fallback tiers** (VRAM < model requirement): tried highest-first
   (degraded mode — route to the largest available hardware).

Within each tier, the instance with the model available and the lowest queue
depth is selected (unchanged).

```
Preferred (lowest-first):  8G → 24G → 48G   (if model needs ≤ 48G)
Fallback  (highest-first): 24G → 8G          (if no tier ≥ model VRAM)
```

### Error Variants

`NoViableTier` is **removed**. The remaining errors:

| Error | Condition |
|-------|-----------|
| `ModelNotFound` | Model does not appear in any stone's `models_available` |
| `AllInstancesBusy` | Every instance with the model is at max queue depth |
| `NoHealthyInstances` | No healthy Ollama instances exist |

### Degraded Label

When a request is served by a fallback tier, the routing decision's
`tier_label` is suffixed with `(degraded)`:

```
tier_label: "24G(degraded)"
```

This allows logging, metrics, and the dashboard to distinguish normal routing
from safety-net routing without blocking the request.

### Invariant

> If `model ∈ instance.models_available` for any healthy instance,
> then `select_instance(model)` MUST return `Ok(RoutingDecision)` —
> never an error.

The only way a request fails is if the model is genuinely absent from
all stones, all stones are busy, or no stones are healthy.

## Consequences

### Positive

- Models installed by the user are always honoured, even on undersized hardware
- Ollama's own partial-offload (GPU + RAM) is respected rather than second-guessed
- Eliminates a class of false rejections that frustrated operators
- Future fitness profiler can mark these routes as `Degraded` without blocking them

### Negative

- Performance may be poor when a large model runs on small VRAM (expected — user chose this)
- Queue depth may spike on degraded-tier stones serving oversized models

### Risks

**Risk:** User installs huge model on tiny hardware, expects fast inference.
**Mitigation:** Fitness profiler (ORCH-0003) will benchmark and surface
per-model performance data; dashboard will show degraded status clearly.

## Implementation

### Files Changed

- `domain/types.rs`: Removed `RoutingError::NoViableTier` variant and its `Display` arm
- `domain/routing.rs`: Rewrote tier selection to preferred + fallback sweep;
  degraded label suffix; updated module and function documentation
- New test: `safety_net_routes_oversized_model` — 48 GB model on 8G + 24G tiers
  routes to 24G with `(degraded)` label
- 17/17 tests pass

### Test Coverage

| Test | Scenario |
|------|----------|
| `routes_to_lowest_tier` | 7B model → 8G tier (preferred) |
| `overflow_to_higher_tier` | 70B model → 24G tier (only viable) |
| `picks_least_loaded` | Equal tier, different queue depths |
| `safety_net_routes_oversized_model` | 48G model on 8G + 24G → 24G (degraded) |

## References

- Future: [ORCH-0003](ORCH-0003-fitness-profiler.md) — Fitness Profiler (advisory scoring, not blocking)
