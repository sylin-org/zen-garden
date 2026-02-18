# ORCH-0002: AI Capability Router (Ollama)

**Status:** Draft  
**Date:** 2026-02-16  
**Revised:** 2026-02-18  
**Authors:** Leo Botinelly, Claude  
**Depends On:** ORCH-0001 (Offering Orchestration), KOI-0001 (Embedded HTTP & UDP Bridging), Tools API  
**Policy Trigger:** `garden-rake policy ollama routed`

### Dependency Status (verified 2026-02-17)

| Dependency | Status | Notes |
|---|---|---|
| KOI-0001 (all phases) | **Done** | HTTP self-hosting, koi-udp, container wiring all merged |
| Tools API | **Done** | `GET /api/v1/garden/tools/stream` live (moss-tools-domain implemented) |
| ORCH-0001 Phase 1 (types, elections) | **Done** | `OfferingRole`, `OrchestrationState`, election scoring |
| Sub-Capabilities Proposal | **Draft** | Router can query Ollama directly (`/api/tags`, `/api/ps`) as interim |

---

## Abstract

The AI Capability Router is a containerized orchestrator offering that sits in front of multiple Ollama instances. It auto-discovers stones and their GPUs, builds VRAM-based tiers automatically, and routes every request to the **smallest GPU that can handle the job** — escalating to larger hardware only under load. A lease-on-demand mechanism temporarily reserves high-VRAM stones for large-model work without permanently wasting their capacity on small requests.

The router exposes an Ollama-compatible HTTP endpoint (transparent to applications), a management dashboard (htmx + askama, served from the same axum instance), per-stone metrics with token tracking, and a bulk model management UI with pre-flight VRAM feasibility checks. All routing algorithms are self-tuning from observed traffic — the system exposes only five user-facing settings, all of which concern **intent** (privacy, destructive actions) rather than tuning.

Running as a container offering validates the full Koi Phase 0 infrastructure (Tools API SSE, UDP mesh, container env var injection) and serves as the reference implementation for all future containerized orchestrators (ORCH-0003+).

---

## Table of Contents

1. [Motivation](#motivation)
2. [Architecture](#architecture)
3. [Design Principles](#design-principles)
4. [Instance Discovery & Auto-Tiering](#instance-discovery--auto-tiering)
5. [Lease-on-Demand Reservation](#lease-on-demand-reservation)
6. [Request Routing](#request-routing)
7. [Metrics & Observability](#metrics--observability)
8. [Management Dashboard](#management-dashboard)
9. [Model Management](#model-management)
10. [Model Distribution Policy](#model-distribution-policy)
11. [Configuration & Persistence](#configuration--persistence)
12. [The Router Offering](#the-router-offering)
13. [Connection String Resolution](#connection-string-resolution)
14. [Health Monitoring & Failover](#health-monitoring--failover)
15. [CLI Integration](#cli-integration)
16. [API Surface](#api-surface)
17. [Implementation Phases](#implementation-phases)
18. [Future Considerations](#future-considerations)
19. [Appendix A: Ollama API Reference](#appendix-a-ollama-api-reference)

---

## Motivation

### The Problem

A garden with four GPU Stones:

| Stone | GPU | VRAM |
|-------|-----|------|
| stone-01 | RTX 3060 | 12 GB |
| stone-02 | RTX 3060 | 12 GB |
| stone-03 | RTX 4090 | 24 GB |
| stone-04 | RTX 4090 | 24 GB |

The default singleton-with-replica policy makes one primary and the others dormant — wasting GPU compute. What we actually want:

- All four instances actively serving
- Small models (≤12 GB) spread across all stones — prefer the 12 GB stones
- Large models route to the 24 GB stones only when needed
- 24 GB stones **also serve small requests** when no large models are in demand
- Incoming requests routed to the right instance based on model size + load
- The system figures out everything automatically from the hardware

### Why Not Just `balanced`?

A standard HTTP load balancer doesn't understand:
- Which instance has which model loaded
- Which instance has enough VRAM for a given model
- Model loading time (routing to an instance that needs to load a model first is slow)
- Inference performance differences between GPUs
- Queue depth (a request that's 5th in queue on a fast GPU may be slower than 1st in queue on a slow one)
- The difference between a 7B model and a 70B model

The router is a domain-specific load balancer that understands the AI inference problem space.

---

## Architecture

```
                           ┌──────────────────────┐
    Applications ────────► │  AI Router (axum)    │ ◄── :11434 Ollama-compatible proxy
                           │                      │ ◄── :7190  Management dashboard + API
    ┌──────────────────────┤  htmx dashboard      │
    │ Management UI        │  metrics engine       │
    │ (same process)       │  lease scheduler      │
    │                      │  policy engine         │
    └──────────────────────┴──┬──────┬──────┬──────┘
                              │      │      │
                           ┌──▼───┐┌─▼───┐┌─▼────────┐
                           │12G   ││12G  ││24G        │
                           │Tier  ││Tier ││Tier       │
                           │      ││     ││           │
                           │small ││small││small +    │
                           │models││     ││large (on  │
                           │      ││     ││lease)     │
                           └──────┘└─────┘└───────────┘
                          stone-01  stone-02  stone-03, stone-04
```

The router runs as a **container offering** (not Moss-native). This is a strategic choice:

- **Validates Koi Phase 0**: first real consumer of `KOI_ENDPOINT`, `GARDEN_STONE_ENDPOINT`, container env var injection
- **Reference implementation**: establishes the pattern for ORCH-0003 (database choreographer) and future containerized orchestrators
- **Matures the bridge**: exercises Tools API SSE subscription, UDP mesh participation, and DNS registration from inside a container
- **Deployment simplicity**: `docker pull` + standard offering lifecycle, no Moss binary changes

---

## Design Principles

### 1. If the system has the data, it's not a configuration

The router has complete visibility into VRAM, queue depth, latency, request patterns, and model sizes. Every routing, leasing, and distribution decision can be derived from this data. Exposing a tuning knob means admitting the algorithm isn't good enough.

### 2. Only five user-facing settings

| Setting | Type | Why it exists |
|---------|------|---------------|
| Auto-pull models to optimal stones | on/off | User may not want automatic bandwidth/disk use |
| Remove idle models after inactivity | on/off | Destructive action — needs explicit opt-in |
| Usage tracking | on/off | Privacy choice — only a human can decide |
| Reset metrics | action | User intent |
| Per-stone VRAM budget cap | number | Hardware-specific (thermal, shared machine) — unknowable by the system |

Everything else is automatic. No tier definitions, no lease durations, no queue thresholds, no routing strategy selectors.

### 3. Tiers emerge from hardware

There are no predefined "small/medium/large" bins. Tiers are the set of distinct VRAM capacities discovered across stones. A garden with 8G, 12G, 12G, 24G hardware produces three tiers: 8G, 12G, 24G.

### 4. Route to the smallest capable GPU

Every request goes to the lowest VRAM tier that can serve it. Large GPUs are preserved for large work. They serve small work only when all smaller stones are busy — and even then, only temporarily.

### 5. Overflow goes up, never down

A 7B model can overflow to an A100 under load. A 70B model can never overflow to a 3060 — it won't fit. This is a hard constraint, not a policy.

---

## Instance Discovery & Auto-Tiering

### Discovery

The router subscribes to the Tools API stream filtered for Ollama offerings:

```http
GET /api/v1/garden/tools/stream?tool_type=offering&tool_fqid=offering:ollama
```

Each Ollama tool entry includes connection info and capabilities. The router also queries each instance directly for hardware details.

### Hardware Profiling

On discovery of a new Ollama instance, the router queries three endpoints:

```http
GET  http://<ollama-instance>:11434/api/ps       # Running models (includes size_vram!)
GET  http://<ollama-instance>:11434/api/tags      # All available models (includes size, parameter_size, quantization)
POST http://<ollama-instance>:11434/api/show      # Detailed model info (includes model_info.general.parameter_count)
```

The **critical field** is `size_vram` from `/api/ps` — it reports the exact VRAM consumption per loaded model in bytes, not an estimate. The router also uses `details.parameter_size` and `details.quantization_level` from `/api/tags` to estimate VRAM for models not currently loaded.

Combined with Stone metrics from the Tools API, the router builds a hardware profile:

```json
{
  "stone_id": "019c3a2b-...",
  "stone_name": "stone-03",
  "endpoint": "http://192.168.1.50:11434",
  "gpu": {
    "name": "NVIDIA GeForce RTX 4090",
    "vram_total_mb": 24576,
    "vram_budget_mb": 24576
  },
  "models_loaded": [
    { "name": "llama3.1:8b", "size_vram": 4915724288, "expires_at": "2026-02-18T14:38:31Z" },
    { "name": "nomic-embed-text", "size_vram": 274726912, "expires_at": "2026-02-18T14:40:00Z" }
  ],
  "models_available": ["llama3.1:8b", "deepseek-r1:32b", "nomic-embed-text", "mistral:7b"]
}
```

The `vram_budget_mb` field defaults to `vram_total_mb` but can be capped by the user's per-stone VRAM budget setting (e.g., thermal limits on a shared machine). The `expires_at` field from `/api/ps` tells the router when Ollama will auto-unload each model (based on `keep_alive`), enabling proactive routing decisions.

### Auto-Tiering

Tiers are computed, never configured. The router collects every stone's VRAM budget and groups them:

```
Discovered hardware:
  stone-01: 12G    stone-02: 12G    stone-03: 24G    stone-04: 24G

Tiers (auto-generated):
  Tier 12G:  [stone-01, stone-02]
  Tier 24G:  [stone-03, stone-04]
```

A garden with different hardware:
```
  stone-01: 8G    stone-02: 12G    stone-03: 12G    stone-04: 24G    stone-05: 160G

Tiers:
  Tier 8G:   [stone-01]
  Tier 12G:  [stone-02, stone-03]
  Tier 24G:  [stone-04]
  Tier 160G: [stone-05]
```

Tiers are recalculated when stones join or leave, or when a VRAM budget changes. No manual tier assignment exists — if a user wants to constrain a stone below its hardware capacity, they set a VRAM budget cap on that stone, and auto-tiering places it correctly.

### Model Registry

The router maintains a model registry assembled from all Ollama instances:

| Model | VRAM Required | Min Tier | Present On |
|-------|--------------|----------|-----------|
| nomic-embed-text | 274 MB | 8G | all |
| mistral:7b | 4.1 GB | 8G | all |
| llama3.1:8b | 4.7 GB | 8G | all |
| deepseek-r1:32b | 18.5 GB | 24G | stone-03, stone-04 |

Model VRAM requirement is determined from:
- **`size_vram`** from `/api/ps` — exact VRAM for loaded models (authoritative)
- **`model_info.general.parameter_count`** + **`details.quantization_level`** from `/api/show` — computed estimate for unloaded models
- **`size`** from `/api/tags` — disk size, used as fallback approximation

---

## Lease-on-Demand Reservation

### The Problem with Static Tiers

Permanently reserving 24G stones for large-model work wastes capacity. In a typical homelab, large-model requests are bursty — 5 in a row, then nothing for an hour. Static reservation leaves expensive GPUs idle most of the time.

### The Lease Model

Every stone starts in **GLOBAL** mode — accepting any request. When a large-model request arrives, the router temporarily **leases** a capable stone, reserving it for large-model work. The lease expires when demand subsides.

```
                    large request arrives
    ┌─────────┐  ──────────────────────────►  ┌──────────┐
    │  GLOBAL  │                               │  LEASED  │
    │  (any    │  ◄────────────────────────── │  (large   │
    │  request)│     lease expires (no renewal) │  only)   │
    └─────────┘                                └──────────┘
                                                    │ ▲
                                                    │ │
                                          new large  │ │ each request
                                          request    └─┘ renews lease
```

### Demand-Driven Escalation

```
T=0     All 4 stones GLOBAL. Small requests spread across all 4.

T=10s   Large request arrives for llama3:70b.
        Router: stone-03 (24G) has capacity. Lease it.
        stone-03 → LEASED (timer starts). Serves the request.
        Remaining pool: stone-01, stone-02, stone-04 handle small requests.

T=45s   Another large request arrives.
        Router: stone-03 is LEASED but busy. Lease stone-04.
        stone-04 → LEASED (timer starts).
        Remaining pool: stone-01, stone-02 handle small requests.

T=2min  stone-03 finishes. Timer still active. Stays LEASED.
        Next large request → goes to stone-03 (idle, already leased).

T=7min  No large requests since T=45s. Both leases expired.
        All 4 stones → GLOBAL. Back to full capacity for small work.
```

### Adaptive Lease Duration

The lease duration is **not configurable**. It's computed from observed traffic:

```
lease_duration = 2 × avg_interval_between_large_requests
clamped to [30 seconds, 15 minutes]
```

The system learns this from its own traffic. Bursty workloads get short leases (quick reclaim). Steady large-model traffic gets longer leases (fewer mode transitions). The formula recalculates on a rolling window.

### Self-Calibrating Pressure Valve

If small-request latency spikes because too many stones are leased, the system **self-corrects**:

```
If estimated_wait_time(small_requests) > 2 × avg_response_time(small_requests):
    break the oldest lease → stone returns to GLOBAL
```

No threshold to configure. The system defines "normal" from its own metrics and reacts when small-request service degrades. This ensures leasing never starves the majority workload.

### Lease Selection Priority

When a large request arrives and a lease is needed:

1. **Already-leased + idle stone** — reuse existing lease (no mode transition)
2. **GLOBAL stone with model already warm** — prefer model affinity
3. **GLOBAL stone in the lowest viable tier** — don't lease a 160G stone for a 20G model
4. **GLOBAL stone in the next tier up** — escalate only if necessary

---

## Request Routing

### Core Algorithm

```
1. Extract model name from request body
   (POST /api/generate, /api/chat, /api/embed all include "model" field)

2. Determine model's VRAM requirement from registry

3. Compute minimum tier (smallest VRAM ≥ model requirement)

4. Collect candidates:
   a. Tier-appropriate stones in GLOBAL mode
   b. LEASED stones (if model qualifies as "large" for that lease)
   c. Exclude stones where model won't fit

5. Score candidates:
   a. Model already loaded (warm) — strong preference
   b. Lowest queue depth
   c. Lowest tier (don't waste large GPUs on small work)

6. If all tier-appropriate stones are busy:
   Overflow to next tier UP. Never down.

7. If this is a large model and no stone is currently leased:
   Lease the selected stone. Timer starts.

8. Route. Increment queue depth counter.
```

### Lowest-Viable-Tier Routing

This is the only routing strategy. There is no selector.

A 7B model (~4G VRAM) goes to a 12G stone. Not the 24G stone. Not the 160G stone. The smallest GPU that can handle it. Large GPUs are preserved for work that actually needs them.

Only when all 12G stones are busy does the router overflow a small request to the 24G tier — and only to a GLOBAL-mode stone (not a leased one, unless the pressure valve triggers).

### Model Loading Awareness

- **Model loaded (warm):** Route immediately. Fast path.
- **Model available but not loaded (cold):** Will need to load into VRAM, potentially evicting another model. Route only if no warm instances exist or all are saturated.
- **Model not available:** Cannot route to this instance.

The router strongly prefers warm instances. Loading a model takes seconds to minutes.

### Streaming Passthrough

Ollama uses **newline-delimited JSON (NDJSON)** streaming — each response chunk is a complete JSON object separated by newlines (not Server-Sent Events). The final object in the stream includes `"done": true` plus performance statistics. The router proxies the connection, not the response body:

- No additional latency per token
- No memory accumulation in the router
- Client sees the same streaming behavior as direct connection
- Router reads the final `done: true` object to extract metrics (`eval_count`, `total_duration`, etc.)

### Request Types

| Endpoint | Model Field | Routing Behavior |
|----------|-------------|------------------|
| `POST /api/generate` | `model` | Route by model + tier + load |
| `POST /api/chat` | `model` | Route by model + tier + load |
| `POST /api/embed` | `model` | Route by model + tier (new embeddings endpoint) |
| `POST /api/embeddings` | `model` | Route by model + tier (deprecated, forwards to `/api/embed`) |
| `GET /api/tags` | *(none)* | Aggregate from all instances, deduplicate |
| `GET /api/ps` | *(none)* | Aggregate from all instances (includes `size_vram`) |
| `POST /api/pull` | `model` | Via model management (see below) |
| `POST /api/show` | `model` | Route to any instance that has the model |
| `POST /api/create` | `model` | Route to target instance |
| `POST /api/copy` | `source` | Route to instance that has the source model |
| `DELETE /api/delete` | `model` | Via model management (see below) |
| `GET /api/version` | *(none)* | Route to any instance |

---

## Metrics & Observability

### What's Tracked

The proxy sees every request and response. Metrics are captured at near-zero cost (counter increments + field extraction from Ollama's response JSON).

**Per-stone counters:**

| Metric | Source |
|--------|--------|
| Request count | Proxy passthrough |
| Token count (input + output) | Ollama response: `prompt_eval_count`, `eval_count` |
| Total inference time | Ollama response: `total_duration` (nanoseconds) |
| Model load time | Ollama response: `load_duration` (nanoseconds) |
| Prompt evaluation time | Ollama response: `prompt_eval_duration` (nanoseconds) |
| Average response time (wall clock) | Proxy timing |
| Queue depth over time | Internal routing state |

**Aggregate metrics:**

| Metric | What it shows |
|--------|--------------|
| Total requests routed | Overall throughput |
| Model popularity | Request count per model — informs auto-pull decisions |
| Busiest stone | Hotspot detection |
| Time-weighted VRAM utilization | Capacity planning |
| Routing decisions log | "Sent to stone-02: lowest queue in tier-12G" |

### Token Tracking

Ollama's `/api/generate` and `/api/chat` final response objects include (all durations in **nanoseconds**):

```json
{
  "eval_count": 284,
  "prompt_eval_count": 52,
  "total_duration": 10706818083,
  "load_duration": 6338219291,
  "prompt_eval_duration": 130079000,
  "eval_duration": 4232710000
}
```

Tokens/sec can be computed as: `eval_count / eval_duration × 10⁹`.

The proxy already reads the final `done: true` object to decrement queue depth. Extracting these fields is a one-liner. This gives accurate token counts and timing without estimation.

### Storage

**In-memory ring buffer + periodic async flush** to `metrics.json` on the container's data volume. Not a database.

- Volume: small (a few KB per hour under heavy load)
- No query engine needed — dashboard reads current snapshot
- Persists across container restarts
- **Reset** = truncate file + zero in-memory counters
- **Disable** = stop recording (routing still works, counters frozen)

### User Controls

- **Tracking on/off**: privacy toggle. When off, no metrics are recorded or persisted.
- **Reset**: zeros all counters and clears history. Immediate.

---

## Management Dashboard

### Technology

**htmx + askama templates**, served from the same axum instance on port `:7190`. No JavaScript build pipeline, no Node dependency, no second container. Templates compile into the binary — the container stays small.

The UI is a convenience layer. Every setting the UI can change is also expressible in the configuration file. Headless operation works for scripted/CI deployments.

### Dashboard View (read-only, SSE-fed live updates)

```
┌──────────────────────────────────────────────────────────────┐
│  Stone Pool                                                  │
├──────────┬────────┬────────┬──────────┬───────────┬─────────┤
│  Tier    │ Stones │ Status │ Queue    │ Leased    │ Models  │
├──────────┼────────┼────────┼──────────┼───────────┼─────────┤
│  12G     │ 2      │ 2/2 ●  │ 0        │ —         │ 3       │
│  24G     │ 2      │ 2/2 ●  │ 0        │ 1 (3:42)  │ 5       │
└──────────┴────────┴────────┴──────────┴───────────┴─────────┘

▼ Tier 24G
  stone-03 (RTX 4090)  LEASED  deepseek-r1:32b  expires 3:42  queue: 0
  stone-04 (RTX 4090)  GLOBAL  idle                            queue: 0
```

**Per-stone detail (expandable):**

```
┌─────────────────────────────────────────────────────────────┐
│  stone-01 (RTX 3060, 12G)     ● online                     │
│  Models: llama3:8b, mistral:7b, nomic-embed-text            │
│  VRAM: 9.2G / 12G                                          │
│  Requests: 1,247  │  Tokens: 842K                           │
│  Avg response: 2.3s  │  Queue: 0                            │
└─────────────────────────────────────────────────────────────┘
```

### Settings View

```
┌─────────────────────────────────────────────────┐
│  Router Settings                                │
│                                                 │
│  Auto-pull models to optimal stones:  [● On]    │
│  Remove idle models after inactivity: [○ Off]   │
│  Usage tracking:                      [● On]    │
│                                                 │
│  Per-stone limits:                              │
│  stone-06: VRAM budget [120G] (of 160G)         │
│                                                 │
│  [Reset Metrics]                                │
│                                                 │
│  Everything else is automatic.                  │
└─────────────────────────────────────────────────┘
```

Five settings. That's the entire settings page.

---

## Model Management

### Model Inventory View

A model-centric view across all stones:

```
┌──────────────────────────────────────────────────────────────┐
│  Models across garden                            [Refresh]   │
├──────────────────────────────────────────────────────────────┤
│  ☑ llama3:8b        stone-01 ● stone-02 ● stone-03 ● s-04 ●│
│  ☑ mistral:7b       stone-01 ● stone-02 ● stone-03 ○ s-04 ○│
│  ☐ deepseek-r1:32b  stone-01 ○ stone-02 ○ stone-03 ● s-04 ●│
│  ☑ codellama:13b    stone-01 ○ stone-02 ○ stone-03 ● s-04 ○│
├──────────────────────────────────────────────────────────────┤
│  Selected: 3 models                                          │
│  [Pull to all ▾]  [Remove from all ▾]  [Update selected]    │
│                                                              │
│  Pull new model: [________________] [Pull to... ▾] [Go]     │
└──────────────────────────────────────────────────────────────┘
```

- **● / ○** shows presence per stone at a glance
- Checkboxes for bulk selection
- Dropdowns target "all stones", "a specific stone", or "stones with enough VRAM"

### Pre-Flight Feasibility

When a user types a model name in the pull field, the router performs a **feasibility check before showing options**:

1. Query Ollama's registry manifest (or `/api/show` on any instance that has it) for model metadata: parameter count, quantization, size on disk, approximate VRAM requirement
2. Compare against every stone's VRAM budget and current utilization
3. Return a feasibility matrix — only viable options become clickable

```
┌─────────────────────────────────────────────────────────────┐
│  deepseek-coder-v2:236b                                     │
│  Size: ~130G  │  Quantization: Q4_K_M  │  Params: 236B     │
│                                                             │
│  Feasibility:                                               │
│  stone-01 (RTX 3060, 12G)    ✗ Insufficient VRAM           │
│  stone-02 (RTX 3060, 12G)    ✗ Insufficient VRAM           │
│  stone-03 (RTX 4090, 24G)    ✗ Insufficient VRAM           │
│  stone-04 (2× A100, 160G)    ✓ Available (130G / 160G)     │
│                                                             │
│  [Pull to stone-04]                                         │
└─────────────────────────────────────────────────────────────┘
```

If a model technically fits but requires evicting other models:

```
│  stone-03 (RTX 4090, 24G)    ⚠ Fits, but will unload mistral:7b │
```

Transparency. No guessing, no wasted time pulling a model that won't fit.

### Pull Operations

- Pulls are long-running. The router fires `POST /api/pull` to the target Ollama instance and streams the pull progress events back to the UI via SSE. No polling.
- Multiple simultaneous pulls (one per target stone) run in parallel — the UI shows per-stone progress bars.
- "Update" = pull same tag again. Ollama handles digest comparison internally.

### Delete Operations

- Delete is synchronous (`DELETE /api/delete`). The UI optimistically removes the dot.
- Bulk delete fans out to all instances that have the model.

---

## Model Distribution Policy

### Auto-Pull (When Enabled)

The router does **not** use a request-count threshold for auto-pulling. Instead, it detects **routing frustration**:

```
If a model is repeatedly routed to a HIGHER tier than its VRAM requirement demands
(because lower-tier stones don't have it, but COULD fit it):
    → auto-pull it to the lowest viable tier.
```

The system detects its own inefficiency and fixes it. No "after N requests" knob to configure.

Example: `codellama:13b` (8G VRAM) keeps routing to stone-03 (24G tier) because stone-01 and stone-02 (12G tier) don't have it — but they could fit it. After observing this pattern, the router auto-pulls to stone-01 and stone-02.

### Delete-on-Idle (When Enabled)

If a model hasn't been requested for a configurable period (derived from usage patterns, not a fixed duration), the router removes it from stones where it's redundant — keeping at least one copy on the lowest viable tier.

### Distribution Triggers

| Event | Router Action |
|-------|--------------|
| New Ollama instance joins | Profile hardware, add to tier, sync via auto-pull if enabled |
| Routing frustration detected | Auto-pull model to lower tier (if auto-pull enabled) |
| Model removed from an instance | No action (operator intentional unless delete-on-idle) |
| Request for model not present anywhere | Log warning, return 404 |
| Request for model present but not on any available instance | Queue briefly, then 503 with retry hint |

---

## Configuration & Persistence

### Storage

The container gets a data directory via the volume mount contract in docker.rs. Two files:

| File | Contents | Format |
|------|----------|--------|
| `router-config.toml` | User settings (5 knobs + per-stone VRAM budgets) | TOML — human-readable, diff-able, editable outside the UI |
| `metrics.json` | Counters, token totals, response times | JSON — flushed periodically from in-memory ring buffer |

Both persist across container restarts. Both live on the mounted volume.

### Configuration Flow

```
UI edit → update in-memory state → flush to router-config.toml
Startup → read router-config.toml → populate in-memory state
```

No ambiguity about what's "saved" — every UI change is immediately durable.

### Default Configuration

```toml
# router-config.toml — written by the router, editable by hand

[features]
auto_pull = true
delete_on_idle = false
metrics_enabled = true

# Per-stone VRAM budget overrides (uncomment to cap below hardware maximum)
# [stones.stone-06]
# vram_budget_mb = 122880   # 120G of 160G total
```

That's it. No routing strategies, no tier definitions, no lease durations, no queue thresholds. The system handles all of that from observed data.

---

## The Router Offering

The AI Router is itself a garden offering:

```yaml
name: ai-router
category: infrastructure
tags: [router, ai, load-balancer, orchestrator]

image: zen-garden/ai-router:latest
ports:
  - 11434:11434      # Ollama-compatible proxy endpoint
  - 7190:7190        # Management dashboard + API

environment:
  - KOI_ENDPOINT              # Injected by Moss (Koi bridge)
  - GARDEN_STONE_ENDPOINT     # Injected by Moss (local Stone API)
  - GARDEN_OFFERING_NAME      # Injected by Moss (offering identity)
```

### Deployment

When the user applies the `routed` policy:

```bash
garden-rake policy ollama routed
```

Moss checks if `ai-router` is running. If not, it prompts:

```
Policy 'routed' requires the AI Router offering.
Install ai-router? [Y/n]
```

Auto-provisions on the most suitable Stone (low resource usage — the router is lightweight HTTP proxying).

### DNS

When the router is active, it takes over the `ollama.lan` DNS entry via Koi's `/v1/dns/entries`. Individual Ollama instances remain reachable by Stone hostname, but the canonical name resolves to the router.

Wish resolution (`zen-garden:ollama`) also resolves to the router endpoint.

---

## Connection String Resolution

### Before Router (Default Policy)

```
zen-garden:ollama → http://stone-03.local:11434
                    (whichever Stone is primary)
```

### After Router (Routed Policy)

```
zen-garden:ollama → http://<router-stone>.local:11434
                    (router forwards to appropriate instance)
```

Applications don't change. The connection string resolves to the router, which is Ollama-compatible. Routing is transparent.

---

## Health Monitoring & Failover

### Instance Health Checks

The router periodically checks each Ollama instance:

```http
GET http://<instance>:11434/api/tags
```

Healthy response within timeout = healthy. Timeout or error = mark unhealthy, stop routing.

### Router Failover

The router is stateless — it rebuilds its model registry and tier map from the Tools API on startup. Metrics persist on the volume. If the router dies:

- Moss detects the offering is down, can restart it automatically
- Direct access to individual instances still works (by Stone hostname)
- On restart, the router re-discovers all stones and resumes routing within seconds

### Instance Failure

1. Router detects via health check failure
2. Remove stone from tier and routing pool
3. If the stone had unique models (only instance at that tier), log a warning
4. When the instance recovers, router re-profiles and resumes routing

---

## CLI Integration

### Status

```bash
$ garden-rake router status

  AI ROUTER                      http://stone-01.local:11434
  Dashboard                      http://stone-01.local:7190

  STONES (4)                                        LEASED

    stone-01    RTX 3060    12G VRAM   [healthy]     —       queue: 0
    stone-02    RTX 3060    12G VRAM   [healthy]     —       queue: 1
    stone-03    RTX 4090    24G VRAM   [healthy]   3:42      queue: 0
    stone-04    RTX 4090    24G VRAM   [healthy]     —       queue: 0

  TIERS (auto)

    12G:  stone-01, stone-02
    24G:  stone-03, stone-04

  METRICS (last 5 min)

    Requests: 47   Tokens: 31K in / 142K out   Errors: 0
    Avg response: 2.1s
    Top models: llama3.1:8b (31), nomic-embed-text (12), deepseek-r1:32b (4)
```

### Model Management

```bash
# View model distribution across all stones
garden-rake router models

# Force pull a model (runs feasibility check first)
garden-rake router pull deepseek-r1:32b
# → Feasibility: stone-03 ✓, stone-04 ✓, stone-01 ✗, stone-02 ✗
# → Pull to stone-03, stone-04? [Y/n]
```

---

## API Surface

### Management API (port 7190)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | GET | Management dashboard (HTML) |
| `/api/v1/router/stones` | GET | All stones with hardware profiles and lease state |
| `/api/v1/router/tiers` | GET | Current auto-tier assignments |
| `/api/v1/router/models` | GET | Model registry with distribution and feasibility |
| `/api/v1/router/models/:name/feasibility` | GET | Pre-flight VRAM check for a model |
| `/api/v1/router/models/:name/pull` | POST | Pull model to specified stones |
| `/api/v1/router/models/:name/delete` | DELETE | Remove model from specified stones |
| `/api/v1/router/metrics` | GET | All metrics (per-stone + aggregate) |
| `/api/v1/router/metrics` | DELETE | Reset all metrics |
| `/api/v1/router/config` | GET | Current configuration |
| `/api/v1/router/config` | PUT | Update configuration |
| `/api/v1/router/health` | GET | Router health and stone status summary |
| `/api/v1/router/events` | GET (SSE) | Live dashboard updates (stone state, leases, metrics) |

### Ollama-Compatible Proxy (port 11434)

All standard Ollama API endpoints, proxied transparently with routing logic applied.

---

## Implementation Phases

### Phase 0: Koi Infrastructure ✅

**Status:** Complete (KOI-0001 all phases merged)

- **Phase 0a** — `koi-embedded` HTTP self-hosting on `:5641`
- **Phase 0b** — `koi-udp` crate (bind/send/recv-SSE for UDP datagrams over HTTP)
- **Phase 0c** — Moss container wiring (`extra_hosts`, `KOI_ENDPOINT` env var, DNS resolver injection)

### Phase 1: Discovery & Auto-Tiering

**Effort:** ~1 week

- Subscribe to Tools API stream for Ollama offerings
- Hardware profiling via Ollama API (`/api/tags`, `/api/ps`)
- Auto-tier computation from discovered VRAM capacities
- Model registry construction (model → VRAM requirement → minimum tier)
- Per-stone VRAM budget cap support

### Phase 2: Request Routing

**Effort:** ~1-2 weeks

- Axum-based HTTP proxy on `:11434`
- Model extraction from request JSON body
- Lowest-viable-tier routing algorithm
- Lease-on-demand scheduler (GLOBAL/LEASED state machine, adaptive timer)
- Upward-only overflow logic
- Streaming passthrough for Ollama NDJSON responses
- Queue depth tracking per stone
- Self-calibrating pressure valve

### Phase 2.5: Dashboard & Metrics

**Effort:** ~1 week

- Axum management server on `:7190`
- htmx + askama read-only dashboard (stone pool, tiers, leases, queue depths)
- SSE endpoint for live dashboard updates
- Metrics engine: per-request counter increments + token extraction from Ollama responses
- In-memory ring buffer with async flush to `metrics.json`
- Metrics reset + tracking on/off controls

### Phase 3: Policy Engine & Settings UI

**Effort:** ~3-5 days

- Configuration read/write to `router-config.toml` on data volume
- Settings editor UI (5 knobs)
- Routing-frustration detection for auto-pull triggering
- Delete-on-idle logic (when enabled)

### Phase 4: Model Management

**Effort:** ~1 week

- Model inventory view (model-centric, per-stone presence dots)
- Pre-flight feasibility checks (`/api/show` + VRAM comparison)
- Bulk pull/update/remove with multi-stone targeting
- SSE-fed pull progress bars (parallel pulls, one per target stone)
- Integration with auto-pull engine (same underlying pull/delete machinery)

### Phase 5: CLI & Policy Integration

**Effort:** ~3-5 days

- `garden-rake router status`, `garden-rake router models`, `garden-rake router pull`
- `garden-rake policy ollama routed` triggers router deployment
- DNS takeover (router gets `ollama.lan` via Koi)
- Wish resolution through router

---

## Future Considerations

- **Multi-offering routing**: Route for multiple AI offerings (e.g., Ollama + vLLM + llama.cpp)
- **Model affinity**: Prefer routing repeated requests for the same model to the same instance (cache warmth)
- **Predictive loading**: Anticipate model needs based on request patterns and pre-load
- **Cost-aware routing**: Factor in power consumption (iGPU vs discrete GPU) for energy-conscious routing
- **Embedding-specific optimization**: Batch embedding requests for throughput vs latency tradeoff
- **Multi-GPU split inference**: Route to stone pairs that can cooperatively serve a model too large for any single GPU

---

## Appendix A: Ollama API Reference

Verified against the [official Ollama API documentation](https://github.com/ollama/ollama/blob/main/docs/api.md) on 2026-02-18. All durations are in **nanoseconds**. Streaming uses **newline-delimited JSON (NDJSON)**, not Server-Sent Events.

### Endpoints Used by the Router

#### Inference (proxied with routing logic)

| Endpoint | Method | Model Field | Stream | Notes |
|----------|--------|-------------|--------|-------|
| `/api/generate` | POST | `model` | Yes (NDJSON) | Completion. Final object has `done: true` + stats |
| `/api/chat` | POST | `model` | Yes (NDJSON) | Chat completion. Supports `tools`, `messages` |
| `/api/embed` | POST | `model` | No | New embeddings endpoint. Field: `input` (string or array) |
| `/api/embeddings` | POST | `model` | No | **Deprecated**, superseded by `/api/embed`. Field: `prompt` |

#### Model Management (used by model management UI)

| Endpoint | Method | Key Fields | Stream | Notes |
|----------|--------|------------|--------|-------|
| `/api/pull` | POST | `model` | Yes | Pull progress: `{status, digest, total, completed}` |
| `/api/delete` | DELETE | `model` | No | Returns 200 OK or 404 |
| `/api/create` | POST | `model`, `from` | Yes | Create from existing model, GGUF, or safetensors |
| `/api/copy` | POST | `source`, `destination` | No | Copy/rename. Returns 200 or 404 |
| `/api/show` | POST | `model` | No | Model info including `model_info`, `capabilities` |

#### Discovery (polled for state)

| Endpoint | Method | Notes |
|----------|--------|-------|
| `/api/tags` | GET | List local models. Returns `models[]` with `name`, `size`, `details` |
| `/api/ps` | GET | List running models. Returns `models[]` with `size_vram`, `expires_at` |
| `/api/version` | GET | Returns `{"version": "0.5.1"}` |

### Critical Response Fields

#### `GET /api/ps` — Running Models

```json
{
  "models": [
    {
      "name": "mistral:latest",
      "model": "mistral:latest",
      "size": 5137025024,
      "digest": "2ae6f6dd7a3d...",
      "details": {
        "parent_model": "",
        "format": "gguf",
        "family": "llama",
        "families": ["llama"],
        "parameter_size": "7.2B",
        "quantization_level": "Q4_0"
      },
      "expires_at": "2024-06-04T14:38:31.83753-07:00",
      "size_vram": 5137025024
    }
  ]
}
```

**Key fields for routing:**
- `size_vram` — Exact VRAM consumption in bytes. **This is the authoritative source for VRAM-aware tiering.**
- `expires_at` — When Ollama will auto-unload (based on `keep_alive`). Enables proactive routing.
- `details.parameter_size` — Human-readable param count ("7.2B").
- `details.quantization_level` — Quantization type ("Q4_0", "Q4_K_M", etc.).

#### `GET /api/tags` — Local Models

```json
{
  "models": [
    {
      "name": "deepseek-r1:latest",
      "model": "deepseek-r1:latest",
      "modified_at": "2025-05-10T08:06:48.639712648-07:00",
      "size": 4683075271,
      "digest": "0a8c26691023...",
      "details": {
        "parent_model": "",
        "format": "gguf",
        "family": "qwen2",
        "families": ["qwen2"],
        "parameter_size": "7.6B",
        "quantization_level": "Q4_K_M"
      }
    }
  ]
}
```

**Key fields:** `size` (disk size, not VRAM — use as fallback), `details.parameter_size`, `details.quantization_level`.

#### `POST /api/show` — Model Information

```json
{
  "details": {
    "format": "gguf",
    "family": "llama",
    "families": ["llama"],
    "parameter_size": "8.0B",
    "quantization_level": "Q4_0"
  },
  "model_info": {
    "general.architecture": "llama",
    "general.parameter_count": 8030261248,
    "llama.context_length": 8192,
    "llama.embedding_length": 4096
  },
  "capabilities": ["completion", "vision"]
}
```

**Key fields:** `model_info.general.parameter_count` (exact param count for VRAM estimation), `capabilities` (informs routing — vision models, tool-capable models).

#### Inference Response — Final Object (both `/api/generate` and `/api/chat`)

```json
{
  "model": "llama3.2",
  "created_at": "2023-08-04T19:22:45.499127Z",
  "done": true,
  "done_reason": "stop",
  "total_duration": 10706818083,
  "load_duration": 6338219291,
  "prompt_eval_count": 26,
  "prompt_eval_duration": 130079000,
  "eval_count": 259,
  "eval_duration": 4232710000
}
```

All durations in nanoseconds. Tokens/sec = `eval_count / eval_duration × 10⁹`.

#### Pull Progress Stream

```json
{"status": "pulling manifest"}
{"status": "pulling digestname", "digest": "digestname", "total": 2142590208, "completed": 241970}
{"status": "verifying sha256 digest"}
{"status": "writing manifest"}
{"status": "removing any unused layers"}
{"status": "success"}
```

Progress percentage = `completed / total`. The `completed` field may be absent before download starts.

### Load/Unload Model (via generate or chat)

- **Load**: `POST /api/generate` with `{"model": "llama3.2"}` (empty prompt)
- **Unload**: `POST /api/generate` with `{"model": "llama3.2", "keep_alive": 0}`
- Same pattern works with `/api/chat` using empty `messages` array
- Response includes `done_reason: "load"` or `done_reason: "unload"`
