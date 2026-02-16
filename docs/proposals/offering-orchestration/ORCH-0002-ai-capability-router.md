# ORCH-0002: AI Capability Router (Ollama)

**Status:** Draft  
**Date:** 2026-02-16  
**Authors:** Leo Botinelly, Claude  
**Depends On:** ORCH-0001 (Offering Orchestration), KOI-0001 (Embedded HTTP & UDP Bridging), Sub-Capabilities Proposal, Tools API  
**Policy Trigger:** `garden-rake policy ollama routed`

---

## Abstract

The AI Capability Router is a specialized orchestrator offering that sits in front of multiple Ollama instances and routes requests based on model requirements, VRAM constraints, and real-time performance metrics. It replaces the default singleton-with-replica policy with an active-active topology where all instances serve traffic according to their hardware capabilities.

When the `routed` policy is applied to an Ollama offering, the router auto-discovers all instances, bins them by hardware capability, manages model distribution, and exposes a single Ollama-compatible HTTP endpoint. Applications connect to the router and are unaware of the underlying topology.

---

## Table of Contents

1. [Motivation](#motivation)
2. [Architecture](#architecture)
3. [Instance Discovery & Capability Binning](#instance-discovery--capability-binning)
4. [Model Distribution Policy](#model-distribution-policy)
5. [Request Routing](#request-routing)
6. [The Router Offering](#the-router-offering)
7. [Connection String Resolution](#connection-string-resolution)
8. [Health Monitoring & Failover](#health-monitoring--failover)
9. [CLI Integration](#cli-integration)
10. [Manifest](#manifest)
11. [API Surface](#api-surface)
12. [Implementation Phases](#implementation-phases)

---

## Motivation

### The Problem

A garden with three GPU Stones:

| Stone | GPU | VRAM |
|-------|-----|------|
| stone-amber-ridge | RTX 3060 Ti | 8 GB |
| stone-coral-reef | RTX 3060 | 8 GB (hypothetical) |
| stone-bronze-canyon | RTX 3090 | 24 GB |

The default singleton-with-replica policy would make one primary and the others dormant — wasting GPU compute. What we actually want:

- All three instances actively serving
- Models up to ~5 GB deployed to all three (they all fit in 8 GB VRAM)
- Large models (13B+ quantized, 32B, etc.) only deployed to stone-bronze-canyon (needs 24 GB VRAM)
- Incoming requests routed to the right instance based on the model requested
- If an instance is busy, route to the next capable instance

### Why Not Just `balanced`?

A standard HTTP load balancer doesn't understand:
- Which instance has which model loaded
- Which instance has enough VRAM for a given model
- Model loading time (routing to an instance that needs to load a model first is slow)
- Inference performance differences between GPUs
- Queue depth (a request that's 5th in queue on a fast GPU may be slower than 1st in queue on a slow one)

The router is a domain-specific load balancer that understands the AI inference problem space.

---

## Architecture

```
                           ┌──────────────────┐
    Applications ────────► │   AI Router      │ ◄── single Ollama-compatible endpoint
                           │   (offering)     │
                           └──┬─────┬─────┬───┘
                              │     │     │
                         ┌────▼──┐ ┌▼────┐ ┌▼─────────┐
                         │8GB    │ │8GB  │ │24GB       │
                         │Bin A  │ │Bin A│ │Bin A + B  │
                         │       │ │     │ │           │
                         │small  │ │small│ │small +    │
                         │models │ │models││large      │
                         │       │ │     │ │models     │
                         └───────┘ └─────┘ └───────────┘
                        stone-amber  stone-coral  stone-bronze
```

The router is itself an offering deployed to a Stone (ideally a low-resource Stone — it's just HTTP proxying). It subscribes to the Tools API stream to discover all Ollama instances and their capabilities.

---

## Instance Discovery & Capability Binning

### Discovery

The router subscribes to the Tools API stream filtered for Ollama offerings:

```http
GET /api/v1/garden/tools/stream?tool_type=offering&tool_fqid=offering:ollama
```

(Or, if adopted: `offering:ollama:adopted`)

Each Ollama tool entry includes connection info and capabilities (models). The router also queries each instance for hardware details.

### Hardware Profiling

On discovery of a new Ollama instance, the router queries its hardware profile:

```http
GET http://<ollama-instance>:11434/api/ps
GET http://<ollama-instance>:11434/api/tags
```

Combined with Stone metrics from the Tools API, the router builds a hardware profile:

```json
{
  "stone_id": "019c3a2b-...",
  "stone_name": "stone-bronze-canyon",
  "endpoint": "http://192.168.1.50:11434",
  "gpu": {
    "name": "NVIDIA GeForce RTX 3090",
    "vram_total_mb": 24576,
    "vram_available_mb": 22000
  },
  "models_loaded": ["llama3.1:8b", "nomic-embed-text"],
  "models_available": ["llama3.1:8b", "deepseek-r1:32b", "nomic-embed-text", "mistral:7b"]
}
```

### Capability Bins

The router organizes instances into VRAM-based capability bins:

```
Bin A (≤8 GB models):     [stone-amber, stone-coral, stone-bronze]
Bin B (8-16 GB models):   [stone-bronze]
Bin C (16-24 GB models):  [stone-bronze]
```

Bin thresholds are derived from available VRAM, not total VRAM. The router accounts for VRAM already consumed by loaded models.

Each model in the garden is assigned to the lowest bin it fits in. Bins are recalculated when:
- A new instance joins or leaves
- VRAM availability changes significantly (model loaded/unloaded)
- Models are added or removed from instances

### Model-to-Bin Assignment

The router maintains a model registry:

| Model | Size (approx) | Bin | Available On |
|-------|--------------|-----|-------------|
| nomic-embed-text | 274 MB | A | all three |
| mistral:7b | 4.1 GB | A | all three |
| llama3.1:8b | 4.7 GB | A | all three |
| deepseek-r1:32b | 18.5 GB | C | stone-bronze only |

Model size is determined from Ollama's `/api/tags` response (`size` field) or from the model manifest.

---

## Model Distribution Policy

### Automatic Distribution

The router actively manages model presence across instances:

**Rule 1: Universal small models.** Any model that fits in the smallest bin is synced to all instances. This maximizes availability and allows load distribution.

```
Model fits in Bin A? → Ensure all Bin A instances have it
```

**Rule 2: Large models stay where they fit.** Models requiring more VRAM are only pulled to instances that can accommodate them.

```
Model requires Bin B? → Only pull to instances with ≥ Bin B VRAM
```

**Rule 3: Demand-driven pulling.** The router doesn't speculatively pull every model everywhere. It watches request patterns. If a model is requested frequently and an instance *could* hold it but doesn't, the router initiates a pull.

### Sync Mechanism

The router uses the existing capabilities mirroring infrastructure:

```http
POST /api/v1/stone/offerings/ollama/capabilities/mirror
{
  "source_stone": "stone-bronze-canyon",
  "target_stone": "stone-amber-ridge",
  "capabilities": ["mistral:7b"]
}
```

Or, for instances on Stones where the router can reach Moss:

```http
POST /api/v1/stone/offerings/ollama/capabilities
{
  "action": "add",
  "name": "mistral:7b"
}
```

This triggers `ollama pull mistral:7b` on the target Stone.

### Distribution Triggers

| Event | Router Action |
|-------|--------------|
| New Ollama instance joins | Profile hardware, assign to bins, sync Bin A models |
| New model pulled on any instance | Evaluate distribution: sync to all eligible instances |
| Model removed from an instance | No action (operator intentional) |
| Request for model not present anywhere | Log warning, return 404 |
| Request for model present but not on any available instance | Queue or return 503 with retry hint |

---

## Request Routing

### Routing Algorithm

When a request arrives at the router:

```
1. Extract model name from request body
   (POST /api/generate, /api/chat, /api/embeddings all include "model" field)

2. Look up model in registry → which bin? which instances have it?

3. Filter to instances that:
   a. Have the model loaded (warm) — prefer these
   b. Have the model available (cold but pullable) — fallback
   c. Are healthy (health check passing)

4. Among warm instances, score by:
   a. Current queue depth (lower is better, weight: 0.4)
   b. Recent tokens/sec throughput (higher is better, weight: 0.3)
   c. VRAM headroom (more free VRAM = less eviction risk, weight: 0.2)
   d. Network latency to router (lower is better, weight: 0.1)

5. Route to highest-scoring instance
```

### Model Loading Awareness

A critical distinction: a model being "available" (downloaded) vs "loaded" (in VRAM). Loading a model takes seconds to minutes depending on size. The router strongly prefers instances where the model is already loaded:

- **Model loaded (warm):** Route immediately. This is the fast path.
- **Model available but not loaded (cold):** The instance will need to load it, potentially evicting another model. Route only if no warm instances exist or all warm instances are saturated.
- **Model not available:** Cannot route to this instance for this model.

### Streaming Passthrough

Ollama uses streaming responses (Server-Sent Events). The router passes the stream through without buffering — it proxies the connection, not the response body. This means:

- No additional latency per token
- No memory accumulation in the router
- Client sees the same streaming behavior as direct connection

### Request Types

| Endpoint | Model Field | Routing Behavior |
|----------|-------------|------------------|
| `POST /api/generate` | `model` | Route by model + performance |
| `POST /api/chat` | `model` | Route by model + performance |
| `POST /api/embeddings` | `model` | Route by model, prefer instances with embedding-optimized config |
| `GET /api/tags` | *(none)* | Aggregate from all instances, deduplicate |
| `GET /api/ps` | *(none)* | Aggregate from all instances |
| `POST /api/pull` | `name` | Route to appropriate bin instances |
| `DELETE /api/delete` | `name` | Fan out to all instances that have it |

### Queue Management

The router maintains a per-instance queue depth estimate:

- Incremented when routing a request to an instance
- Decremented when the response completes (stream ends or error)
- Used as the primary routing signal — keeps instances evenly loaded

For long-running inference requests, queue depth is more informative than connection count because a single long generation can monopolize a GPU.

---

## The Router Offering

The AI Router is itself a garden offering:

```yaml
name: ai-router
category: infrastructure
tags: [router, ai, load-balancer, orchestrator]
replicable: false    # Singleton — only one router needed

image: zen-garden/ai-router:latest
ports:
  - 11434:11434      # Ollama-compatible endpoint

environment:
  - ROUTER_TARGET_OFFERING=ollama        # Which offering to route for
  - ROUTER_TOOLS_ENDPOINT=http://localhost:7185  # Local Moss Tools API
  - ROUTER_HEALTH_INTERVAL=10            # Health check interval (seconds)
  - ROUTER_SYNC_INTERVAL=300             # Model distribution check (seconds)
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

Or auto-provisions it on the most suitable Stone (low resource usage — the router is lightweight).

### DNS

When the router is active, it takes over the `ollama.lan` DNS entry. The individual Ollama instances are reachable by their Stone hostname but the canonical name resolves to the router.

Wish resolution (`zen-garden:ollama`) also resolves to the router endpoint.

---

## Connection String Resolution

### Before Router (Default Policy)

```
zen-garden:ollama → http://stone-bronze-canyon.local:11434
                    (whichever Stone is primary)
```

### After Router (Routed Policy)

```
zen-garden:ollama → http://<router-stone>.local:11434
                    (router forwards to appropriate instance)
```

Applications don't change. The connection string resolves to the router, which is Ollama-compatible. The routing is transparent.

---

## Health Monitoring & Failover

### Instance Health Checks

The router periodically checks each Ollama instance:

```http
GET http://<instance>:11434/api/tags
```

A healthy response within timeout = healthy. Timeout or error = mark unhealthy, stop routing to it.

### Router Failover

The router itself is a singleton (not load-balanced — that would be meta-turtles). If the router dies:

- Applications lose the `ollama.lan` endpoint
- Direct access to individual instances still works (by Stone hostname)
- Moss detects the router offering is down, can restart it or prompt the user

For high-availability of the router itself, it can be deployed with the default singleton-with-replica policy (a router with a standby). Since the router is stateless (it rebuilds its model registry from the Tools API on startup), failover is clean.

### Instance Failure

When an Ollama instance fails:

1. Router detects via health check failure
2. Requests for models only on that instance get 503 with retry hint
3. If the instance had unique models (only instance in its bin), router logs a warning
4. When the instance recovers, router re-profiles it and resumes routing

---

## CLI Integration

### Status

```bash
$ garden-rake router status

  AI ROUTER                      http://stone-01.local:11434

  INSTANCES (3)

    stone-amber-ridge    8 GB VRAM     [healthy]   3 models   queue: 0
    stone-coral-reef     8 GB VRAM     [healthy]   3 models   queue: 1
    stone-bronze-canyon  24 GB VRAM    [healthy]   5 models   queue: 0

  CAPABILITY BINS

    Bin A (≤8 GB):    nomic-embed-text, mistral:7b, llama3.1:8b
                      → all 3 instances

    Bin C (16-24 GB): deepseek-r1:32b
                      → stone-bronze-canyon only

  ROUTING (last 5 min)

    Requests: 47   Avg latency: 1.2s   Errors: 0
    Top models: llama3.1:8b (31), nomic-embed-text (12), deepseek-r1:32b (4)
```

### Model Distribution

```bash
# View current distribution
garden-rake router models

# Force sync a model to all eligible instances
garden-rake router sync mistral:7b

# View routing decision for a model
garden-rake router route deepseek-r1:32b
# → stone-bronze-canyon (only instance with sufficient VRAM)
```

---

## Manifest

```yaml
# offerings/ai-router.manifest.yaml
name: ai-router
category: infrastructure
tags: [router, ai, orchestrator]
protocols:
  - name: http
    port: 11434
    default: true

replicable: true    # Can have standby for HA

capabilities:
  type: routed-offerings
  discover:
    source: tools-api
    filter: "tool_type=offering&capability=model:*"
```

---

## API Surface

The router exposes its management API alongside the Ollama-compatible proxy:

### Management Endpoints (port 7190 or similar)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/v1/router/instances` | GET | List all discovered Ollama instances with hardware profiles |
| `/api/v1/router/bins` | GET | Current capability bin assignments |
| `/api/v1/router/models` | GET | Model registry with distribution status |
| `/api/v1/router/models/:name/distribute` | POST | Trigger distribution of a model to eligible instances |
| `/api/v1/router/metrics` | GET | Routing metrics (requests, latency, queue depths, errors) |
| `/api/v1/router/health` | GET | Router health and instance status summary |

### Ollama-Compatible Proxy (port 11434)

All standard Ollama API endpoints, proxied transparently with routing logic applied.

---

## Implementation Phases

### Phase 0: Koi Infrastructure (KOI-0001 prerequisite)

**Effort:** ~1 week (shared with ORCH-0001 Phase 0)

The containerized router cannot discover peers, register DNS names, or participate in the UDP mesh without Koi bridging. This phase is defined in KOI-0001 and shared with ORCH-0001:

- **Phase 0a** — `koi-embedded` HTTP self-hosting on `:5641` (activate dead `http_enabled`, spawn listener in `start()`)
- **Phase 0b** — `koi-udp` crate (bind/send/recv-SSE for UDP datagrams over HTTP)
- **Phase 0c** — Moss container wiring (`extra_hosts`, `KOI_HTTP_URL` env var, DNS resolver injection)

The router specifically needs:
- `/v1/udp/bind` + `/v1/udp/recv` SSE — to listen for ORCH election messages and Stone chirps from inside the container
- `/v1/udp/send` — to emit election candidates and results
- `/v1/dns/entries` — to register `ollama.lan` DNS takeover when becoming active router
- `/v1/mdns/register` — to advertise the router instance on the local network

### Phase 1: Discovery & Binning

**Effort:** ~1 week

- Subscribe to Tools API stream for Ollama offerings
- Hardware profiling via Ollama API (`/api/tags`, `/api/ps`) and Stone metrics
- VRAM-based capability binning
- Model registry construction

### Phase 2: Request Routing

**Effort:** ~1-2 weeks

- Ollama-compatible HTTP proxy (axum or similar)
- Model extraction from request body
- Routing algorithm (queue depth, throughput, VRAM headroom)
- Streaming passthrough for SSE responses
- Queue depth tracking

### Phase 3: Model Distribution

**Effort:** ~1 week

- Automatic sync of Bin A models to all instances
- Demand-driven pulling for frequently requested models
- Integration with capabilities mirroring infrastructure

### Phase 4: CLI & Management API

**Effort:** ~3-5 days

- Router status, models, route commands in Rake
- Management API endpoints
- Integration with `garden-rake observe`

### Phase 5: Policy Integration

**Effort:** ~3 days

- `garden-rake policy ollama routed` triggers router deployment
- DNS takeover (router gets `ollama.lan`)
- Wish resolution through router

---

## Future Considerations

- **Multi-offering routing**: Route for multiple AI offerings (e.g., Ollama + vLLM + llama.cpp)
- **Model affinity**: Prefer routing repeated requests for the same model to the same instance (cache warmth)
- **Predictive loading**: Anticipate model needs based on request patterns and pre-load
- **Cost-aware routing**: Factor in power consumption (iGPU vs discrete GPU) for energy-conscious routing
- **Embedding-specific optimization**: Batch embedding requests for throughput vs latency tradeoff
