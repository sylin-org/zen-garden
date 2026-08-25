# Ollama CPU-Only Research

## Overview

| Property | Value |
|----------|-------|
| **Offering Name** | ollama-cpu |
| **Category** | CPU-only LLM/Embedding Inference |
| **Primary Use** | Embedding models on thin client stones |
| **License** | MIT |
| **Docker Image** | `ollama/ollama:0.5` (same as GPU offering) |
| **Docker Hub** | https://hub.docker.com/r/ollama/ollama |
| **GitHub** | https://github.com/ollama/ollama |
| **Runtime** | Go + llama.cpp (CPU backend) |
| **Relates to** | ORCH-0005 CPU Inference Tier ADR |

## Design Decision: Ollama vs llama.cpp

### Candidates Evaluated

| Candidate | Docker Image | Image Size |
|-----------|-------------|------------|
| **Ollama** (selected) | `ollama/ollama:0.5` | ~3.1 GB |
| llama.cpp server | `ghcr.io/ggml-org/llama.cpp:server` | ~50–100 MB |

### Why Ollama Was Chosen Over llama.cpp

**1. API Protocol Compatibility (decisive factor)**

The Zen Garden orchestrator communicates with stones via Ollama's proprietary
API endpoints:

| Endpoint | Purpose | llama.cpp equivalent |
|----------|---------|---------------------|
| `GET /api/tags` | List models | `GET /v1/models` (different JSON schema) |
| `POST /api/show` | Model metadata | **None** |
| `POST /api/pull` | Download model | **None** (must pre-download GGUF files) |
| `POST /api/generate` | Text generation | `POST /v1/completions` (different format) |
| `POST /api/embed` | Embeddings | `POST /v1/embeddings` (different format) |
| `POST /api/chat` | Chat | `POST /v1/chat/completions` (different format) |

Using llama.cpp would require a second API client in the orchestrator —
different URL paths, different JSON schemas, no model management API. This is a
large engineering cost for the thin client use case.

**2. Same Inference Engine**

Ollama embeds llama.cpp as its inference backend. For CPU workloads, the compute
path is identical. The only difference is Ollama's Go runtime overhead (~50–80
MB RAM), which is negligible on a 4–8 GB thin client with a 2 GB workspace
budget.

**3. Operational Simplicity**

| Operation | Ollama | llama.cpp |
|-----------|--------|-----------|
| Install model | `ollama pull all-minilm` | Download GGUF manually, mount volume |
| List models | `ollama list` or API | Read filesystem or API |
| Update model | `ollama pull model` | Re-download GGUF file |
| Multi-model | Built-in hot-swap | Router mode (newer, less tested) |

**4. Unified Orchestrator Code**

ORCH-0005 designed `ollama-cpu` so the orchestrator requires **zero protocol
changes**: same discovery (/api/tags), same fitness profiler (/api/generate),
same proxy (all 16 Ollama endpoints). The only difference is the mDNS offering
tag.

### When llama.cpp Would Be the Right Choice

- If building a **dedicated embedding microservice** that doesn't participate in
  the garden's Ollama routing pool
- If RAM is extremely constrained (<2 GB) and the 50–80 MB Go overhead matters
- If serving a **single static model** that never changes (no pull/update needed)
- As a **separate offering** (`llamacpp`) with its own orchestrator

## Docker Image Analysis

### Image Selection

**Selected**: `ollama/ollama:0.5` — same image as GPU offering.

The Ollama Docker image is a universal binary that auto-detects GPU availability.
When no GPU is present (or when `CUDA_VISIBLE_DEVICES=""` is set), it
automatically falls back to CPU-only inference. No separate CPU image exists or
is needed.

### CPU-Only Configuration

GPU offloading is disabled via environment variables:

| Variable | Value | Purpose |
|----------|-------|---------|
| `CUDA_VISIBLE_DEVICES` | `""` (empty) | Hide NVIDIA GPUs from Ollama |
| `HIP_VISIBLE_DEVICES` | `""` (empty) | Hide AMD GPUs from Ollama |
| `ROCR_VISIBLE_DEVICES` | `""` (empty) | Hide AMD GPUs (ROCm path) |
| `OLLAMA_MAX_LOADED_MODELS` | `1` | Limit RAM usage on thin clients |
| `OLLAMA_NUM_PARALLEL` | `1` | Single inference slot |
| `OLLAMA_KEEP_ALIVE` | `10m` | Reduce reload churn |
| `OLLAMA_FLASH_ATTENTION` | `1` | Enable flash attention for CPU path |

### Memory Limit

The Docker snippet sets `deploy.resources.limits.memory: 2g` as a default.
Operators should adjust this based on their thin client's RAM and the
`workspace_budget_mb` value in their stone config.

## Target Hardware

### Dell Wyse 5070 (reference thin client)

| Spec | Value |
|------|-------|
| CPU | Intel Celeron J4105 (4C/4T, 1.5–2.5 GHz) |
| RAM | 4–8 GB DDR4 |
| Storage | 16–64 GB eMMC or M.2 SSD |
| GPU | Intel UHD 600 (no CUDA/ROCm) |
| AVX | ❌ No AVX (SSE4.2 only) |
| TDP | 10W |
| Price | $30–60 used |

### Other Viable Thin Clients

| Model | CPU | RAM | AVX | Notes |
|-------|-----|-----|-----|-------|
| HP t630 | AMD GX-420GI | 4–8 GB | AVX | Better than J4105 |
| HP t740 | AMD Ryzen V1756B | 8–32 GB | AVX2 | Excellent for CPU inference |
| Dell Wyse 5060 | AMD GX-424CC | 4–8 GB | AVX | Quad core, decent |
| Lenovo M710q Tiny | Intel i3/i5 | 8–32 GB | AVX2 | Not strictly thin client, overkill |

### CPU Feature Impact

| CPU Feature | Impact on Inference |
|-------------|-------------------|
| AVX2 | ~2× faster matrix ops vs SSE4.2 |
| AVX | ~1.5× faster than SSE4.2 only |
| SSE4.2 only | Functional but slowest path |

Ollama/llama.cpp will use the best available SIMD instructions automatically.
No configuration needed.

## Recommended Models

### Embedding Models (Primary Use Case)

| Model | Parameters | Size | RAM Needed | Use Case |
|-------|-----------|------|------------|----------|
| `all-minilm` | 23M | 43 MB | ~200 MB | Fast sentence embeddings |
| `nomic-embed-text` | 137M | 261 MB | ~500 MB | Higher quality embeddings |
| `mxbai-embed-large` | 335M | 665 MB | ~1 GB | Best embedding quality |
| `snowflake-arctic-embed` | 109M | 217 MB | ~400 MB | Retrieval-focused |

### Tiny Completion Models (Use with Caution)

| Model | Parameters | Size | RAM Needed | Speed (J4105) | Notes |
|-------|-----------|------|------------|---------------|-------|
| `tinyllama` | 1.1B | 637 MB | ~1.5 GB | ~2–5 tok/s | Marginal quality |
| `qwen2.5:0.5b` | 0.5B | 394 MB | ~800 MB | ~5–10 tok/s | Better quality/size |
| `phi-3-mini` | 3.8B | 2.3 GB | ~3.5 GB | ~0.5–1 tok/s | Likely too large |

**Recommendation**: Stick to embedding models on thin clients. The fitness
profiler will naturally Veto or Block generation models that perform below
thresholds.

## Workspace Memory

Per ORCH-0005, CPU thin clients use a configured `workspace_budget_mb` instead
of auto-detected GPU VRAM:

| Configuration | Workspace | Suitable Models |
|--------------|-----------|----------------|
| `workspace_budget_mb = 1024` | 1 GiB | all-minilm, nomic-embed-text |
| `workspace_budget_mb = 2048` | 2 GiB | Above + mxbai-embed-large, tinyllama |
| `workspace_budget_mb = 4096` | 4 GiB | Above + qwen2.5:0.5b, phi |

The workspace budget should be **less than total system RAM** to leave room for
the OS, Docker, and Ollama's Go runtime (~500 MB total overhead).

## Network Configuration

| Port | Protocol | Purpose |
|------|----------|---------|
| 11434 | HTTP | Ollama REST API (same as GPU offering) |

Same port as the GPU offering. On a thin client, only one Ollama instance runs.

## Health Check Strategy

Identical to the GPU offering:

```yaml
healthcheck:
  test: ["CMD", "curl", "-f", "http://localhost:11434/api/tags"]
  interval: 30s
  timeout: 10s
  retries: 5
  start_period: 30s
```

## Differences From GPU Offering

| Aspect | `ollama` (GPU) | `ollama-cpu` (this) |
|--------|---------------|-------------------|
| Docker image | `ollama/ollama:0.5` | Same |
| GPU access | `--gpus all` / deploy.devices | None |
| Volume name | `ollama-data` | `ollama-cpu-data` |
| Memory limit | None (GPU manages VRAM) | `2g` default |
| `CUDA_VISIBLE_DEVICES` | (not set) | `""` (empty) |
| `OLLAMA_MAX_LOADED_MODELS` | (not set, default) | `1` |
| `OLLAMA_NUM_PARALLEL` | (not set, default) | `1` |
| Target models | Generation (7B–70B) | Embedding (23M–335M) |
| `gpu_recommended` | `true` | `false` |
| Compatibility | Warns if no GPU | Warns if GPU present |
| mDNS offering | `ollama` | `ollama-cpu` |

## Security Considerations

Same as GPU offering:

| Concern | Mitigation |
|---------|------------|
| API exposed | Internal network (zen-garden) |
| Model downloads | Models from ollama.com verified |
| Resource exhaustion | Memory limit in Docker + workspace budget |

## Validation Checklist

- [x] Same Docker image as GPU offering (ollama/ollama:0.5)
- [x] GPU disabled via CUDA_VISIBLE_DEVICES=""
- [x] Memory limit set for thin client safety
- [x] Embedding model sizing documented
- [x] AVX/non-AVX implications documented
- [x] Health check identical to GPU offering
- [x] Compatibility rules warn if GPU is present
- [x] llama.cpp alternative evaluated and documented
- [x] ORCH-0005 ADR alignment verified
- [x] All manifest files created (8/8)

## Files

| File | Status |
|------|--------|
| `ollama-cpu.frontmatter.json` | ✅ Created |
| `ollama-cpu.snippet.yaml` | ✅ Created |
| `ollama-cpu.compatibility.yaml` | ✅ Created |
| `ollama-cpu.capabilities.yaml` | ✅ Created |
| `ollama-cpu.adopted.yaml` | ✅ Created |
| `ollama-cpu.adopted.guidance.md` | ✅ Created |
| `ollama-cpu.adopted.example.yaml` | ✅ Created |
| `ollama-cpu.research.md` | ✅ Created |

## References

1. [Ollama GitHub](https://github.com/ollama/ollama)
2. [Ollama Docker Hub](https://hub.docker.com/r/ollama/ollama)
3. [Ollama Environment Variables (envconfig/config.go)](https://github.com/ollama/ollama/blob/main/envconfig/config.go)
4. [llama.cpp Server](https://github.com/ggml-org/llama.cpp/tree/master/tools/server)
5. [llama.cpp Docker Images](https://github.com/ggml-org/llama.cpp/blob/master/docs/docker.md)
6. [ORCH-0005: CPU Inference Tier ADR](../../docs/decisions/ORCH-0005-cpu-inference-tier.md)
7. [Dell Wyse 5070 Specs](https://www.dell.com/support/kbdoc/en-us/000131134/wyse-5070-thin-client)
