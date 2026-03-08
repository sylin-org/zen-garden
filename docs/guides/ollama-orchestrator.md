---
audience: [operator, developer, ai]
doc_type: guide
status: current
last_verified: 2026-03-06
---

# Ollama Orchestrator Usage Guide

The Ollama orchestrator is a transparent proxy that sits between your applications
and one or more Ollama instances running across garden stones. It provides
intelligent routing, model recommendations, fitness benchmarking, and a unified
API surface.

---

## Quick Start

Point any Ollama client at the orchestrator's proxy port instead of a local
Ollama instance:

```bash
# Default proxy port: 21434
export OLLAMA_HOST=http://orchestrator-ip:21434

# Works with ollama CLI
ollama run llama3.1:8b "Hello!"

# Works with curl
curl http://orchestrator-ip:21434/api/generate -d '{
  "model": "llama3.1:8b",
  "prompt": "Hello!"
}'

# Works with any Ollama SDK (Python, JS, etc.)
```

The orchestrator discovers all Ollama instances across garden stones, merges
their model catalogs, and routes each request to the optimal stone based on
VRAM availability, model placement, and fitness benchmarks.

---

## Proxy Endpoints (Port 21434)

The proxy emulates the full Ollama API. All standard endpoints work transparently.

### Inference

| Method | Endpoint | Purpose |
|--------|----------|---------|
| POST | `/api/generate` | Text generation (completion) |
| POST | `/api/chat` | Chat conversation |
| POST | `/api/embed` | Embedding generation |
| POST | `/api/embeddings` | Embedding generation (legacy) |

Requests are routed to the best available stone based on:
- Model availability and VRAM residency (loaded models preferred)
- Queue depth (least-loaded stone wins)
- Fitness tier (benchmarked throughput)
- Demand reservation (high-traffic models get dedicated capacity)

### Discovery

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/tags` | Merged model list across all stones |
| GET | `/api/ps` | Currently loaded models across all stones |
| POST | `/api/show` | Model details (with catalog fallback) |
| GET | `/api/version` | Orchestrator version |

`/api/tags` returns a unified view — if the same model exists on three stones,
it appears once. `/api/ps` includes a `stone` field identifying where each
model is loaded.

`/api/show` tries the upstream instance first, then falls back to the
orchestrator's cached catalog. This means model details are available even
when the model is not currently loaded in VRAM.

### Model Management

| Method | Endpoint | Purpose |
|--------|----------|---------|
| POST | `/api/pull` | Pull model to a stone |
| DELETE | `/api/delete` | Delete model from a stone |
| POST | `/api/copy` | Copy/rename a model |
| POST | `/api/create` | Create a model from a Modelfile |

Management requests are routed to a healthy instance that has the model, or
any healthy instance for pull operations.

---

## Recommended Model Monikers

Instead of specifying a model by name, use the `recommended:` prefix to let the
orchestrator select the best model for a given capability.

```bash
# Best chat model
curl http://orchestrator-ip:21434/api/generate -d '{
  "model": "recommended:chat",
  "prompt": "Explain quantum computing in simple terms"
}'

# Best vision model
curl http://orchestrator-ip:21434/api/chat -d '{
  "model": "recommended:vision",
  "messages": [{"role": "user", "content": "Describe this image", "images": ["base64..."]}]
}'

# Best embedding model
curl http://orchestrator-ip:21434/api/embed -d '{
  "model": "recommended:embedding",
  "input": "search query"
}'

# Best tool-calling model
curl http://orchestrator-ip:21434/api/chat -d '{
  "model": "recommended:tools",
  "messages": [{"role": "user", "content": "What is the weather in Tokyo?"}],
  "tools": [{"type": "function", "function": {"name": "get_weather", "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}}}]
}'
```

### Available Capabilities

| Moniker | Selects for |
|---------|-------------|
| `recommended:quick` | Fastest usable response (autocomplete, extraction) |
| `recommended:chat` | Best conversational quality |
| `recommended:completion` | Alias for `chat` |
| `recommended:synthesis` | Long-context distillation and extraction |
| `recommended:vision` | Image understanding |
| `recommended:ocr` | OCR and document reading |
| `recommended:tools` | Function calling / agent workflows |
| `recommended:thinking` | Extended reasoning and analysis |
| `recommended:embedding` | Semantic search, RAG |

### How Resolution Works

The orchestrator resolves the moniker using the same recommendation engine
behind `GET /v1/recommendations`. The model ranked #1 for the requested
capability is substituted into the request body before routing.

- **Pin interaction**: If you pin a model for a capability (via the dashboard
  or API), the moniker resolves to the pinned model.
- **Transparency**: The response includes an `X-Zen-Resolved-Model` header
  showing which model was selected.
- **Errors**: Unknown capability returns 400. No model available returns 404.

```bash
# Check which model was selected
curl -s -D- http://orchestrator-ip:21434/api/generate -d '{
  "model": "recommended:chat",
  "prompt": "Hi",
  "stream": false
}' 2>&1 | grep -i x-zen-resolved
# X-Zen-Resolved-Model: qwen3:8b
```

> Design rationale: [ORCH-0011](../decisions/ORCH-0011-recommended-model-monikers.md)

---

## Extension API (Port 21434, /v1/ prefix)

The orchestrator exposes additional endpoints under `/v1/` on the same proxy
port. These provide capabilities beyond the standard Ollama API.

### Model Inventory

```bash
curl http://orchestrator-ip:21434/v1/models
```

Returns all known models with placement details, VRAM usage, fitness verdicts,
and capabilities. Richer than `/api/tags` — includes which stones have the model,
whether it is loaded, and benchmark results.

### Stone Inventory

```bash
curl http://orchestrator-ip:21434/v1/stones
```

Returns all discovered stones with GPU details, total/free VRAM, loaded models,
and health status.

### Recommendations

```bash
# All capabilities
curl http://orchestrator-ip:21434/v1/recommendations

# Single capability
curl http://orchestrator-ip:21434/v1/recommendations?capability=chat
```

Returns ranked model recommendations per capability, with scores and reasoning.
Each recommendation includes fitness verdict, parameter size, context length,
and a breakdown of why it scored the way it did.

Capabilities: `quick`, `chat`, `completion`, `synthesis`, `vision`, `ocr`,
`tools`, `thinking`, `embedding`.

### Pinning a Model

```bash
# Pin qwen3:8b as the recommended chat model
curl -X PUT http://orchestrator-ip:21434/v1/recommendations/chat/pin \
  -d '{"model": "qwen3:8b"}'

# Unpin (return to score-based selection)
curl -X DELETE http://orchestrator-ip:21434/v1/recommendations/chat/pin
```

Pins affect both the `/v1/recommendations` response and `recommended:` moniker
resolution.

---

## Dashboard (Port 7190)

The orchestrator runs a web dashboard on port 7190.

```
http://orchestrator-ip:7190
```

### Dashboard API

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/status` | Full snapshot (stones, models, benchmark, metrics) |
| GET | `/api/events` | SSE event stream (live updates) |
| GET | `/api/settings` | Current orchestrator settings |
| POST | `/api/settings` | Update settings |
| GET | `/api/jobs` | Active/recent orchestrator jobs |
| POST | `/api/metrics/reset` | Reset all metrics |
| POST | `/api/metrics/model-counters/reset` | Reset per-model request counters |

### Model Management via Dashboard

```bash
# Pull a model to specific stones
curl -X POST http://orchestrator-ip:7190/api/management/pull \
  -d '{"model": "llama3.1:8b", "stones": ["stone-quiet-lens"]}'

# Delete a model
curl -X POST http://orchestrator-ip:7190/api/management/delete \
  -d '{"model": "llama3.1:8b", "stones": ["stone-quiet-lens"]}'

# Check if a model fits on a stone
curl "http://orchestrator-ip:7190/api/management/feasibility?model=llama3.1:70b&stone=stone-quiet-lens"
```

### Fitness Benchmarks

The orchestrator benchmarks each model on each stone to measure real-world
performance. Results feed into routing decisions and recommendations.

```bash
# Start a benchmark
curl -X POST http://orchestrator-ip:7190/api/benchmark/start

# Check results
curl http://orchestrator-ip:7190/api/benchmark/results

# Export the GPU fitness matrix
curl http://orchestrator-ip:7190/api/benchmark/export
```

The fitness matrix measures five capabilities per model per stone:

| Capability | Metric | What it reveals |
|------------|--------|-----------------|
| Generate | tok/s + cold start | Short-burst text generation speed |
| Embed | cold start | Embedding latency |
| Vision | tok/s + cold start | Image understanding speed |
| Tools | % valid + tok/s | Structured output reliability |
| Think | sustained tok/s | Long-generation throughput under KV pressure |

Each cell produces a verdict: **Fast**, **Degraded**, **Vetoed**, or **Blocked**.

---

## Usage with SDKs

### Python (ollama package)

```python
import ollama

client = ollama.Client(host="http://orchestrator-ip:21434")

# Direct model
response = client.chat(model="llama3.1:8b", messages=[
    {"role": "user", "content": "Hello!"}
])

# Recommended model
response = client.chat(model="recommended:chat", messages=[
    {"role": "user", "content": "Hello!"}
])
```

### Python (OpenAI-compatible)

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://orchestrator-ip:21434/v1",
    api_key="unused"
)

response = client.chat.completions.create(
    model="recommended:chat",
    messages=[{"role": "user", "content": "Hello!"}]
)
```

### JavaScript

```javascript
import { Ollama } from "ollama";

const ollama = new Ollama({ host: "http://orchestrator-ip:21434" });

const response = await ollama.chat({
  model: "recommended:chat",
  messages: [{ role: "user", content: "Hello!" }],
});
```

### LangChain

```python
from langchain_ollama import ChatOllama

llm = ChatOllama(
    base_url="http://orchestrator-ip:21434",
    model="recommended:chat",
)
```

---

## Configuration

### Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `OLLAMA_PROXY_PORT` | `21434` | Proxy listen port |
| `DASHBOARD_PORT` | `7190` | Dashboard listen port |

### Settings (via API or Dashboard)

| Setting | Description |
|---------|-------------|
| `auto_pull_mode` | `off`, `on_demand` — auto-pull unknown models on first request |
| `delete_on_idle` | Remove unused models to free VRAM |
| `metrics_enabled` | Enable request/performance metrics collection |
| `pins` | Per-capability model pins (overrides recommendation ranking) |

---

## Related

- [Ollama detection states](ollama-detection-states.md) — adopted mode on Windows
- [ORCH-0007](../decisions/ORCH-0007-capability-recommendation-engine.md) — Recommendation engine design
- [ORCH-0009](../decisions/ORCH-0009-demand-weighted-topology-advisor.md) — Demand-weighted topology
- [ORCH-0010](../decisions/ORCH-0010-extended-fitness-capabilities.md) — Extended fitness capabilities
- [ORCH-0011](../decisions/ORCH-0011-recommended-model-monikers.md) — Recommended model monikers
