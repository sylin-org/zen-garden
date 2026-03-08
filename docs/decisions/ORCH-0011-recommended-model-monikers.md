# ORCH-0011: Recommended Model Monikers

**Date**: 2026-03-06
**Status**: Accepted
**Applies to**: `zen-garden-ollama-orchestrator` crate
**Depends on**: [ORCH-0007](ORCH-0007-capability-recommendation-engine.md) (Recommendation Engine)

## Context

The recommendation engine (ORCH-0007) ranks models per capability and exposes
the rankings via `GET /v1/recommendations?capability=chat`. However, consuming
applications had to query the recommendation API first, extract the top model
name, then issue a second request to generate/chat/embed. This two-step dance
coupled every client to the extension API and made it impossible to use the
orchestrator's intelligence from standard Ollama client libraries.

The goal was to let any Ollama-compatible client — including `ollama run`,
Python's `ollama` package, and LangChain — benefit from orchestrator
recommendations with zero code changes beyond specifying a virtual model name.

## Decision

Introduced a `recommended:{capability}` model name prefix that the proxy
resolves transparently before routing.

### Moniker Format

```
recommended:{capability}
```

Where `{capability}` is any value accepted by the recommendation engine:
`quick`, `chat`, `completion`, `synthesis`, `vision`, `ocr`, `tools`,
`thinking`, `embedding`.

### Resolution Flow

1. Proxy receives a request with `"model": "recommended:chat"`.
2. Extracts the capability suffix (`chat`).
3. Calls `recommend(capability, ...)` — the same function backing
   `GET /v1/recommendations`.
4. Uses the `selected` model from the response (rank 1, respecting pins).
5. Rewrites the `"model"` field in the JSON body to the resolved model name.
6. Routes the rewritten request through the standard inference path.
7. Adds `X-Zen-Resolved-Model` response header for transparency.

### Pin Interaction

Monikers respect the existing pin system. If a user pins `qwen3:8b` for
`chat`, then `recommended:chat` resolves to `qwen3:8b`. Unpinning returns
to score-based selection.

### Capability Override

The capability extracted from the moniker overrides body-based inference for
demand tracking. A request to `recommended:tools` records
`RequestCapability::Tools` in the demand ledger regardless of whether the
body contains a `tools` array.

### Error Handling

- Unknown capability suffix (e.g., `recommended:invalid`) returns 400.
- No model available for the capability returns 404 with a descriptive error.
- Resolution happens before routing, so routing errors surface normally after
  model substitution.

### Examples

```bash
# Chat — resolves to the highest-ranked chat model
curl http://localhost:21434/api/generate -d '{
  "model": "recommended:chat",
  "prompt": "Hello!"
}'

# Vision — resolves to the best vision model
curl http://localhost:21434/api/chat -d '{
  "model": "recommended:vision",
  "messages": [{"role": "user", "content": "Describe this image", "images": ["..."]}]
}'

# Embedding — resolves to the best embedding model
curl http://localhost:21434/api/embed -d '{
  "model": "recommended:embedding",
  "input": "semantic search query"
}'

# Tools — resolves to the most reliable tool-calling model
curl http://localhost:21434/api/chat -d '{
  "model": "recommended:tools",
  "messages": [{"role": "user", "content": "What is the weather?"}],
  "tools": [{"type": "function", "function": {"name": "get_weather", "parameters": {}}}]
}'
```

## Consequences

### Positive

- Any Ollama-compatible client gains orchestrator intelligence via model name
  alone — no SDK changes, no extension API dependency
- Pin changes take effect immediately for all moniker users without client
  restarts or config changes
- Demand tracking accurately reflects capability intent (moniker-derived),
  improving topology advisor decisions
- The `X-Zen-Resolved-Model` header enables debugging and observability

### Negative

- Clients cannot predict which model will handle their request (mitigated:
  `X-Zen-Resolved-Model` header, `/v1/recommendations` preview endpoint)
- Model-specific parameters (e.g. `num_predict` tuned for a specific model)
  may not be optimal for the resolved model (mitigated: recommendation engine
  selects models with compatible characteristics)

### Risks

**Risk:** Resolution adds latency to the inference hot path.
**Mitigation:** A pre-computed `recommended_models` cache in `AppState` maps
capability → selected model name. The proxy resolves a moniker with a single
`RwLock` read + `HashMap::get` — no scoring computation on the hot path.
The cache is refreshed on every model/instance/benchmark/pin mutation.

**Risk:** Moniker prefix collides with a real model name.
**Mitigation:** `recommended:` contains a colon, which is the Ollama tag
separator. A model named `recommended` with tag `chat` would be
`recommended:chat` — same syntax. However, no model in the Ollama registry
uses `recommended` as a model name. If a collision ever occurs, the moniker
check runs first, so the virtual name takes precedence. Users can still
access the literal model via its full digest.

## Implementation

### Modified Files

| File | Change |
|------|--------|
| `app_state.rs` | Added `recommended_models` cache + `refresh_recommendations()` method |
| `api/proxy.rs` | Moniker detection, cache lookup, body rewrite |
| `api/dashboard.rs` | `refresh_recommendations()` call after pin mutations |
| `api/extension.rs` | `refresh_recommendations()` call after pin mutations |
| `tasks/benchmark.rs` | `refresh_recommendations()` call after benchmark completion |
| `domain/demand.rs` | Added `RequestCapability::from_moniker()` |

### Cache Invalidation

The `recommended_models` cache is refreshed after:
- `upsert_instance`, `remove_instance`, `update_instance_models`
- `upsert_model`, `remove_model`
- Benchmark completion (matrix synthesized)
- Pin create/delete (dashboard and extension API)

## References

- [ORCH-0007](ORCH-0007-capability-recommendation-engine.md) — Recommendation Engine
- [ORCH-0009](ORCH-0009-demand-weighted-topology-advisor.md) — Demand-Weighted Topology Advisor
- [ORCH-0010](ORCH-0010-extended-fitness-capabilities.md) — Extended Fitness Capabilities
