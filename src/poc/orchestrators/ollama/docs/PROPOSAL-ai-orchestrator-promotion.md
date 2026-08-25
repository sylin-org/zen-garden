# Proposal: Promote Ollama Orchestrator → Zen Garden AI Orchestrator

**Author:** AI Architecture Session (2026-03-27)
**Status:** Draft — for Zen Garden Claude Code agent
**Scope:** Greenfield rebuild of `src/orchestrators/ollama/` into `src/orchestrators/ai/`

---

## 1. Executive Summary

The current Ollama Orchestrator is a VRAM-aware, fitness-scored, demand-weighted router for Ollama instances across a Zen Garden deployment. This proposal promotes it to a **general-purpose AI Orchestrator** that manages ALL AI service types — Ollama, ComfyUI, HuggingFace, whisper.cpp/Speaches, Coqui/OpenedAI Speech, Infinity, LibreTranslate, and cloud providers — as adoptable offerings.

The AI Orchestrator becomes the **single routing brain** for all AI capabilities in a Zen Garden. Koan Framework applications connect via a single adapter (`Koan.AI.ZenGarden`) to one endpoint, and the orchestrator handles service discovery, capability routing, fitness optimization, demand tracking, and native API dispatch to each service.

**Approach:** Greenfield crate at `src/orchestrators/ai/`, harvesting proven domain logic (routing, demand, fitness, tiering, placement, metrics) from the current Ollama orchestrator. Ollama becomes one offering type among many in an **Offering Catalog** architecture.

---

## 2. Motivation

### 2.1 Current State

The Ollama Orchestrator handles:
- Multi-instance Ollama discovery via Koi/Moss
- VRAM-aware tiering and routing (performance-first + demand reservation)
- Fitness benchmarking with GPU matrix (Fast/Degraded/Vetoed/Blocked verdicts)
- Exponentially decayed demand tracking (reactive/tactical/strategic windows)
- Capability-aware model recommendations (`recommended:chat`, `recommended:vision`, etc.)
- Model lifecycle: sync, on-demand pull, placement, pre-warming
- Proxy with NDJSON metrics extraction
- Dashboard with real-time SSE updates

This is sophisticated infrastructure — but it only serves Ollama.

### 2.2 Target State

The same routing intelligence applied to:
- **Ollama** — Chat, Embed, Vision, Tools, Thinking (existing)
- **ComfyUI** — Imagine, Edit, Render (image/video generation)
- **Speaches / whisper.cpp** — Transcribe (speech-to-text)
- **OpenedAI Speech / Coqui XTTS / Piper** — Speak (text-to-speech)
- **Infinity / TEI** — Embed (multimodal), Rerank
- **LibreTranslate** — Translate
- **HuggingFace Inference** — Any task via serverless or dedicated endpoints
- **Cloud Providers** — OpenAI, Anthropic, Stability AI, ElevenLabs, Cohere, Deepgram (API key + model enumeration, default priority -10)

All managed through a unified offering catalog, routed by a single engine, proxied through native API dispatch.

### 2.3 Why Greenfield

The current orchestrator has ~3500 lines of Rust across domain, infra, tasks, and API layers. Analysis shows:

| Layer | Ollama-Specific | Generic/Reusable |
|-------|----------------|-----------------|
| Domain (types, routing, demand, fitness, advisor, placement, tiering, metrics, lease, policy, reconciliation, gpu_catalog, recommendation) | `OllamaInstance`, `LoadedModel`, `OllamaTagsResponse`, `OllamaPsResponse`, `OllamaShowResponse`, `OllamaInferenceFinal`, `OllamaEmbedResponse`, benchmark payloads | Routing algorithm, demand ledger, fitness verdicts/matrix, tiering, placement, advisor, metrics engine, lease manager, GPU catalog, recommendation scoring |
| Infra (ollama_client, gateway, persistence, stone_discovery, tools_stream, events) | `OllamaClient` (all API calls), offering filter (`offering:ollama`), endpoint construction (`:11434`) | Koi/Moss integration, mDNS/gateway registration, persistence (TOML config, JSON metrics), SSE stream parsing, stone discovery |
| Tasks (discovery, health_check, reconciliation, model_sync, placement, advisor, benchmark, metrics_processor, metrics_flush, snapshot_publisher, gateway_announce) | Ollama health probe (`/api/tags`), model pull/show/load/unload, benchmark prompts | Discovery lifecycle, health state machine, reconciliation pattern, placement loop with hysteresis, advisor debouncing, metrics processing, snapshot publishing, gateway announcement lifecycle |
| API (proxy, extension, management, dashboard, health, benchmark_api) | NDJSON streaming extraction, Ollama API path routing, `OllamaInferenceFinal` parsing | Proxy forwarding pattern, queue depth tracking, error-based health degradation, extension API contract, dashboard snapshot pattern, background job lifecycle |

**Conclusion:** The domain algorithms are largely generic. The Ollama-specific code is concentrated in infra (HTTP client) and parts of the proxy/tasks layers. A greenfield crate that harvests the generic patterns and introduces an offering abstraction is cleaner than refactoring the existing crate in-place.

---

## 3. Architecture

### 3.1 Offering Catalog

The core abstraction is the **Offering** — a type of AI service that can be discovered, probed, enumerated, and proxied.

```
trait Offering {
    /// Unique type identifier: "ollama", "comfyui", "speaches", etc.
    fn offering_type(&self) -> &str;

    /// AI capabilities this offering type can provide.
    fn capabilities(&self) -> &[Capability];

    /// How to discover instances (port probe, topology filter, configured).
    fn discovery_config(&self) -> DiscoveryConfig;

    /// Probe an endpoint for liveness. Return service metadata if healthy.
    async fn probe(&self, endpoint: &str) -> Result<ProbeResult>;

    /// Enumerate available models/configurations on a live instance.
    async fn enumerate(&self, endpoint: &str) -> Result<Vec<ServiceModel>>;

    /// Estimate VRAM consumption for a model on this offering.
    fn vram_estimate(&self, model: &ServiceModel) -> Option<u64>;

    /// Forward a capability request to the instance's native API.
    /// The orchestrator calls this after routing decides the target.
    async fn proxy(
        &self,
        endpoint: &str,
        capability: Capability,
        request: ProxyRequest,
    ) -> Result<ProxyResponse>;

    /// Optional: benchmark a capability on an instance.
    async fn benchmark(
        &self,
        endpoint: &str,
        capability: Capability,
    ) -> Result<BenchmarkSample>;
}
```

The offering catalog is a registry of `Box<dyn Offering>` instances, each handling one service type. The orchestrator iterates the catalog for discovery, health, enumeration, and proxy dispatch.

### 3.2 Capability Enum

Extend the current `RequestCapability` / `fitness::Capability` enums into a unified capability set:

```
enum Capability {
    // Text/LLM (from Ollama orchestrator)
    Chat,
    Embed,
    Vision,
    Tools,
    Thinking,

    // Generation (new)
    Imagine,        // text → image
    Edit,           // image + instruction → image
    Render,         // text → video

    // Audio (new)
    Transcribe,     // audio → text
    Speak,          // text → audio

    // Search/Retrieval (new)
    Rerank,         // query + docs → scored docs

    // Language (new)
    Translate,      // text + target → text
}
```

Each offering declares which capabilities it provides. The routing engine resolves capabilities to offerings, then to instances.

### 3.3 Instance Model (Generalized)

Replace `OllamaInstance` with a generic `ServiceInstance`:

```
struct ServiceInstance {
    // Identity
    stone_id: String,
    stone_name: String,
    endpoint: String,
    offering_type: String,          // "ollama", "comfyui", "speaches", etc.

    // Hardware (when available)
    gpu_name: Option<String>,
    compute_type: ComputeType,      // Gpu, Cpu (existing)
    vram_total_bytes: u64,
    vram_budget_bytes: u64,
    vram_free_bytes: Option<u64>,   // Real-time for ComfyUI (GET /system_stats)

    // Service state
    health: InstanceHealth,         // Profiling, Healthy, Unhealthy (existing)
    models_available: Vec<String>,  // Model/config names
    models_loaded: Vec<LoadedModel>,// Currently in VRAM (Ollama: /api/ps, ComfyUI: inferred)
    capabilities: Vec<Capability>,  // What this instance can do
    queue_depth: u32,               // Inflight requests
    last_seen: Instant,

    // Offering-specific metadata (opaque to routing, used by offering impl)
    metadata: serde_json::Value,

    // Priority
    priority: i32,                  // 0 = default, -10 = cloud, +10 = pinned
}
```

### 3.4 Priority Model

Priority replaces the concept of "locality tiers":

| Priority | Meaning | Source |
|----------|---------|-------|
| `+10` | Operator-pinned (explicit preference) | Pin override in UI |
| `0` | Default (local + garden instances) | Auto-discovered or configured |
| `-10` | Cloud fallback | Auto-registered from API key + model enumeration |
| `-20` | Emergency/expensive cloud | Operator-configured |

The routing algorithm already sorts by priority implicitly (higher priority → preferred). Cloud providers are just instances at lower priority. If a model exists both locally (priority 0) and on a cloud provider (priority -10), local wins automatically unless the operator pins to the cloud instance.

### 3.5 Routing Engine (Harvested + Extended)

The existing `routing::select_instance` algorithm is nearly generic. Changes:

1. **Filter by capability** (new): Before filtering by model availability, filter by offering capability. A `Transcribe` request only considers Speaches/whisper.cpp instances.
2. **Priority as primary sort** (new): Within non-degraded candidates, sort by priority desc before fitness.
3. **Cross-offering VRAM awareness** (new): When two offerings share a GPU (Ollama + ComfyUI on the same stone), the routing engine considers combined VRAM pressure.
4. **Model-less capabilities** (new): Some capabilities don't have model names (Translate, Rerank). Route by capability + offering type + fitness.

```
Resolution flow:

1. Parse request: extract capability + optional model name
2. If model is a moniker ("recommended:chat"):
   → Resolve via recommendation engine (existing, extended with new capabilities)
3. Filter instances by:
   a. Capability match (offering supports this capability)
   b. Model match (if model specified, instance has it)
   c. Health (routable instances only)
   d. Fitness (exclude Blocked verdicts)
4. Sort candidates by:
   a. Priority (descending) — pinned > default > cloud
   b. Idle preference (queue_depth == 0 first)
   c. Fitness score (descending)
   d. VRAM headroom (descending)
   e. Queue depth (ascending)
5. Demand reservation check (existing):
   If large models have recent demand → reserve high-VRAM instances
6. Pick first candidate under max_queue
7. Return RoutingDecision with target endpoint + offering type
```

### 3.6 Cloud Providers

Cloud providers implement the `Offering` trait:

```
struct CloudProviderOffering {
    provider_name: String,      // "openai", "anthropic", "stability"
    api_key: EncryptedString,
    base_endpoint: String,      // "https://api.anthropic.com"
    capabilities: Vec<Capability>,
    default_priority: i32,      // -10
}
```

**Discovery:** Configured via UI, not auto-discovered.
**Enumeration:** Call provider's model list API (e.g., `GET /v1/models`). Register each model as available on a virtual instance at `default_priority`.
**Proxy:** Forward request with `Authorization: Bearer {api_key}`. Each cloud provider has its own API format (OpenAI-compatible for most; Anthropic Messages API; Stability REST).
**Health:** Periodic API ping (e.g., `GET /v1/models` with key) to verify key validity.
**VRAM:** Not applicable — cloud manages its own resources.
**Benchmarking:** Latency-based fitness (measure request round-trip time). No VRAM/throughput benchmarks.

### 3.7 API Surface

The orchestrator exposes two servers (same as current):

**Proxy Server** (`:21434` default, Ollama-compatible + extensions):

```
# Ollama-compatible (existing, preserved for backward compatibility)
POST /api/generate         → Ollama proxy
POST /api/chat             → Ollama proxy
POST /api/embed            → Ollama or Infinity proxy
GET  /api/tags             → Merged model list across all offerings
GET  /api/ps               → Merged loaded model list
POST /api/show             → Model detail (Ollama)
POST /api/pull             → Model pull (Ollama)
DELETE /api/delete          → Model delete (Ollama)
GET  /api/version          → Orchestrator version
GET  /                     → "Ollama is running" (Ollama client compat)

# New capability endpoints
POST /api/imagine          → ComfyUI proxy (workflow dispatch + image return)
POST /api/edit             → ComfyUI proxy (inpaint workflow)
POST /api/render           → ComfyUI proxy (AnimateDiff workflow)
POST /api/transcribe       → Speaches/whisper.cpp proxy
POST /api/speak            → OpenedAI Speech proxy
POST /api/rerank           → Infinity/TEI proxy
POST /api/translate        → LibreTranslate proxy (or Ollama via Chat)

# Extension API (existing, extended)
GET  /v1/models            → All models across all offerings
GET  /v1/stones            → All stones with offering details
GET  /v1/capabilities      → Available capabilities and serving offerings
GET  /v1/recommendations   → Per-capability model recommendations
PUT  /v1/recommendations/{cap}/pin → Pin a model for a capability
DELETE /v1/recommendations/{cap}/pin → Remove pin
```

**Dashboard Server** (`:7190` default):

```
# Dashboard (existing, extended)
GET  /                     → Dashboard SPA
GET  /api/status           → Snapshot (all offerings, all stones)
GET  /api/events           → SSE stream
GET  /api/settings         → Config
POST /api/settings         → Update config

# Cloud provider management (new)
GET  /api/providers        → Registered cloud providers
POST /api/providers        → Add cloud provider (API key + config)
PUT  /api/providers/{id}   → Update provider
DELETE /api/providers/{id} → Remove provider

# Offering catalog (new)
GET  /api/offerings        → All registered offering types + instances

# Existing
GET  /api/jobs             → Background jobs
POST /api/management/pull  → Pull model
POST /api/management/delete → Delete model
GET  /api/management/feasibility → Check if model fits
POST /api/benchmark/start  → Start fitness run
POST /api/benchmark/cancel → Cancel
GET  /api/benchmark/results → Results
GET  /api/benchmark/export → Export fitness data
POST /api/metrics/reset    → Reset metrics
GET  /health               → Liveness probe
```

---

## 4. Offering Specifications

### 4.1 Ollama Offering

**Harvested from:** Current orchestrator (nearly complete).

| Aspect | Detail |
|--------|--------|
| **Offering type** | `ollama` |
| **Capabilities** | Chat, Embed, Vision, Tools, Thinking |
| **Discovery** | Koi mDNS (`_moss._tcp`) + topology query + Tools API SSE stream. Filter by `offering:ollama` |
| **Health probe** | `GET /` → "Ollama is running" (200 OK) |
| **Enumeration** | `GET /api/tags` → model list. `POST /api/show` → model detail (capabilities, params, context length). `GET /api/ps` → loaded models with VRAM |
| **VRAM tracking** | Authoritative from `/api/ps` (`size_vram`). Projected from disk size × 1.1 when not loaded |
| **Proxy** | Forward Ollama API requests (`/api/generate`, `/api/chat`, `/api/embed`). NDJSON streaming passthrough with metrics extraction from final `done:true` object |
| **Model management** | Pull (`POST /api/pull`), delete (`DELETE /api/delete`), load (empty-prompt trick), unload (`keep_alive:0`) |
| **Benchmarking** | Generate (5 prompts), Vision (3 images), Tools (5 function-call prompts), Thinking (2 long-gen prompts), Embed (3 inputs). Verdict thresholds per capability |
| **Recommendation** | Layered scoring: availability → fitness → context window → quality (param count). Per-capability bonus caps |
| **Model sync** | Tier-peer replication in Sync/OnDemand modes |
| **Placement** | Demand-weighted bin-packing with hysteresis |

**Harvest scope:** Domain logic (routing, demand, fitness, tiering, placement, advisor, recommendation, metrics, lease, policy, reconciliation, gpu_catalog) is generic. `OllamaClient` becomes the offering-specific infra.

### 4.2 ComfyUI Offering

| Aspect | Detail |
|--------|--------|
| **Offering type** | `comfyui` |
| **Capabilities** | Imagine, Edit, Render |
| **Discovery** | Port probe (`:8188`). Koi mDNS / topology query with `offering:comfyui` filter |
| **Health probe** | `GET /system_stats` → 200 with `system` and `devices` fields |
| **Enumeration** | `GET /models/checkpoints` → checkpoint list. `GET /models/loras` → LoRA list. `GET /object_info/CheckpointLoaderSimple` → installed checkpoint names from enum values. `GET /models/controlnet`, `GET /models/vae` for additional model types |
| **VRAM tracking** | `GET /system_stats` → `devices[*].vram_total` and `devices[*].vram_free` (bytes, real-time). `POST /free {"unload_models": true, "free_memory": true}` to force VRAM release |
| **Queue tracking** | `GET /prompt` → `exec_info.queue_remaining`. `GET /queue` → full queue state (running + pending) |

#### ComfyUI Proxy: Workflow Template Architecture

The orchestrator does NOT send ComfyUI workflow JSON directly. Instead, it maintains a **workflow template library** — parameterized API-format JSON workflows for each capability:

**Template: txt2img (Imagine)**
```json
{
  "1": { "class_type": "CheckpointLoaderSimple", "inputs": { "ckpt_name": "{{model}}" } },
  "2": { "class_type": "CLIPTextEncode", "inputs": { "text": "{{prompt}}", "clip": ["1", 1] } },
  "3": { "class_type": "CLIPTextEncode", "inputs": { "text": "{{negative}}", "clip": ["1", 1] } },
  "4": { "class_type": "EmptyLatentImage", "inputs": { "width": "{{width}}", "height": "{{height}}", "batch_size": 1 } },
  "5": { "class_type": "KSampler", "inputs": { "model": ["1", 0], "seed": "{{seed}}", "steps": "{{steps}}", "cfg": "{{guidance}}", "sampler_name": "euler", "scheduler": "normal", "positive": ["2", 0], "negative": ["3", 0], "latent_image": ["4", 0], "denoise": 1.0 } },
  "6": { "class_type": "VAEDecode", "inputs": { "samples": ["5", 0], "vae": ["1", 2] } },
  "7": { "class_type": "SaveImage", "inputs": { "filename_prefix": "orch", "images": ["6", 0] } }
}
```

**Template: img2img (Edit)**
```json
{
  "1": { "class_type": "CheckpointLoaderSimple", "inputs": { "ckpt_name": "{{model}}" } },
  "2": { "class_type": "LoadImage", "inputs": { "image": "{{input_image}}" } },
  "3": { "class_type": "VAEEncode", "inputs": { "pixels": ["2", 0], "vae": ["1", 2] } },
  "4": { "class_type": "CLIPTextEncode", "inputs": { "text": "{{prompt}}", "clip": ["1", 1] } },
  "5": { "class_type": "CLIPTextEncode", "inputs": { "text": "{{negative}}", "clip": ["1", 1] } },
  "6": { "class_type": "KSampler", "inputs": { "model": ["1", 0], "seed": "{{seed}}", "steps": "{{steps}}", "cfg": "{{guidance}}", "sampler_name": "euler", "scheduler": "normal", "positive": ["4", 0], "negative": ["5", 0], "latent_image": ["3", 0], "denoise": "{{strength}}" } },
  "7": { "class_type": "VAEDecode", "inputs": { "samples": ["6", 0], "vae": ["1", 2] } },
  "8": { "class_type": "SaveImage", "inputs": { "filename_prefix": "orch_edit", "images": ["7", 0] } }
}
```

**Proxy flow for `POST /api/imagine`:**

1. Parse request body: `{ model, prompt, negative, width, height, seed, steps, guidance }`
2. Select workflow template (txt2img for Imagine, img2img for Edit)
3. Validate model exists on target instance (`GET /models/checkpoints` cached)
4. If request includes input image: `POST /upload/image` to ComfyUI first
5. Patch template with request parameters (string substitution on `{{placeholders}}`)
6. `POST /prompt` with patched workflow → receive `prompt_id`
7. Connect WebSocket `/ws?clientId={uuid}` (or reuse persistent connection)
8. Monitor WebSocket events:
   - `execution_start` → log
   - `progress { value, max }` → forward progress if client supports it
   - `executed { output: { images: [...] } }` → note output filenames
   - `execution_success` → fetch images
   - `execution_error` → return error with details
9. `GET /view?filename={output}&type=output` → fetch image bytes
10. Return image bytes to client with `Content-Type: image/png`

**Benchmarking:** Submit a standard txt2img workflow (512×512, 20 steps, known seed) and measure wall-clock time. Verdict thresholds:
- Fast: < 15s
- Degraded: < 60s
- Vetoed: < 180s
- Blocked: Error or > 180s

**Model naming convention:** Checkpoint filenames serve as model names: `flux-dev.safetensors`, `sd_xl_base_1.0.safetensors`. The orchestrator strips extensions for display: `flux-dev`, `sd_xl_base_1.0`.

**Workflow template management:**
- Templates stored in `{data_dir}/workflows/` as JSON files
- Built-in defaults for txt2img, img2img, inpaint, upscale, animatediff
- Operator can add custom workflow templates via dashboard
- Templates validated at load time against `GET /object_info` (check that referenced node classes exist on the instance)

### 4.3 Speaches / whisper.cpp Offering

| Aspect | Detail |
|--------|--------|
| **Offering type** | `speaches` |
| **Capabilities** | Transcribe |
| **Discovery** | Port probe (`:8000`). Topology filter `offering:speaches` |
| **Health probe** | `GET /health` → 200 OK |
| **Enumeration** | Speaches: configured via `WHISPER__MODEL` env var. whisper.cpp: `--model` flag at startup. Orchestrator stores model name from probe/config |
| **VRAM tracking** | Not exposed by Speaches/whisper.cpp API. Inferred from model size (tiny ~400MB, base ~500MB, small ~1GB, medium ~2.6GB, large-v3 ~2.9GB) |
| **Proxy** | OpenAI-compatible: forward `POST /v1/audio/transcriptions` with multipart form data (file, model, language, response_format). Stream passthrough for real-time transcription |
| **Benchmarking** | Transcribe a reference audio clip (~10s). Measure wall-clock time. Verdict: Fast if < 2s (5x+ real-time), Degraded if < 10s, Vetoed if < 30s |

### 4.4 OpenedAI Speech Offering

| Aspect | Detail |
|--------|--------|
| **Offering type** | `openedai-speech` |
| **Capabilities** | Speak |
| **Discovery** | Port probe (`:8001` or configured). Topology filter `offering:openedai-speech` |
| **Health probe** | `GET /` → any 200 response |
| **Enumeration** | Voices configured in `voice_to_speaker.yaml`. Orchestrator queries at discovery time or stores known voice list |
| **VRAM tracking** | XTTS v2 model: ~2-3GB estimated. Piper backend: CPU only, no VRAM |
| **Proxy** | OpenAI-compatible: forward `POST /v1/audio/speech` with `{ model, input, voice, speed, response_format }`. Return audio bytes |
| **Benchmarking** | Generate a reference phrase (~20 words). Measure wall-clock time + latency to first byte. Verdict: Fast if TTFB < 500ms, Degraded if < 2s, Vetoed if < 5s |

### 4.5 Infinity Offering

| Aspect | Detail |
|--------|--------|
| **Offering type** | `infinity` |
| **Capabilities** | Embed (multimodal), Rerank |
| **Discovery** | Port probe (`:7997`). Topology filter `offering:infinity` |
| **Health probe** | `GET /health` → 200 OK |
| **Enumeration** | `GET /models` → loaded model list with embedding dimensions, model type (embed/rerank/clip/clap) |
| **VRAM tracking** | Not directly exposed. Inferred from model size (SigLIP-so400m ~1.5GB, BGE-reranker-large ~1GB) |
| **Proxy** | For Embed: forward to `POST /embeddings` (OpenAI-compatible). For Rerank: forward to `POST /rerank` with `{ query, texts, raw_scores }`. Return ranked results |
| **Benchmarking** | Embed: 100 texts, measure throughput (texts/sec). Rerank: 20 queries × 50 docs, measure throughput. Verdict thresholds TBD |

### 4.6 LibreTranslate Offering

| Aspect | Detail |
|--------|--------|
| **Offering type** | `libretranslate` |
| **Capabilities** | Translate |
| **Discovery** | Port probe (`:5000`). Topology filter `offering:libretranslate` |
| **Health probe** | Healthcheck script or `GET /languages` → 200 |
| **Enumeration** | `GET /languages` → available language pairs with language codes and names |
| **VRAM tracking** | CPU-only service. No VRAM |
| **Proxy** | Forward `POST /translate` with `{ q, source, target, format }`. Return `{ translatedText }` |
| **Benchmarking** | Translate 10 reference sentences across 3 language pairs. Measure wall-clock time. Mostly CPU-bound |

### 4.7 HuggingFace Offering

HuggingFace is a special case — it can be both a **cloud provider** (serverless inference) and a **self-hosted offering** (TEI, Inference Endpoints).

| Aspect | Detail |
|--------|--------|
| **Offering type** | `huggingface` |
| **Capabilities** | Chat, Embed, Imagine, Transcribe, Classify (task-dependent) |
| **Discovery** | **Configured** (not auto-discovered). Operator adds HF token via UI |
| **Enumeration** | `GET https://huggingface.co/api/models?inference_provider=all&pipeline_tag={task}` → models available per task. Filter by `inference` status = `warm` |
| **Priority** | `-10` (cloud default) |
| **Proxy** | OpenAI-compatible for Chat: `POST https://router.huggingface.co/v1/chat/completions`. Task-specific for others (text-to-image returns raw bytes, ASR accepts audio bytes) |
| **Rate limits** | Free tier is rate-limited. PRO tier gets higher limits. `X-Wait-For-Model` header for cold starts |

For **self-hosted TEI** instances: discovery via topology, health via `GET /health`, enumeration via `GET /info`. Treated as a separate offering type `tei` with capabilities Embed + Rerank.

### 4.8 Cloud Provider Offerings

Generic cloud provider template:

| Aspect | Detail |
|--------|--------|
| **Offering type** | Provider name: `openai`, `anthropic`, `stability`, `elevenlabs`, `cohere`, `deepgram` |
| **Capabilities** | Per-provider (see table below) |
| **Discovery** | Configured via UI (API key + endpoint) |
| **Enumeration** | Call provider's model list API. Register each model at priority -10 |
| **Health probe** | Lightweight API call with key (e.g., `GET /v1/models`) to verify key validity |
| **Proxy** | Forward with `Authorization: Bearer {key}`. Provider-specific API format |
| **Benchmarking** | Latency measurement only. No VRAM/throughput benchmarks |

**Cloud provider capability matrix:**

| Provider | Chat | Embed | Imagine | Transcribe | Speak | Rerank | Moderate |
|----------|------|-------|---------|-----------|-------|--------|----------|
| OpenAI | ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ (free) |
| Anthropic | ✓ | — | — | — | — | — | — |
| Stability AI | — | — | ✓ | — | — | — | — |
| ElevenLabs | — | — | — | — | ✓ | — | — |
| Cohere | ✓ | ✓ | — | — | — | ✓ | — |
| Deepgram | — | — | — | ✓ | ✓ | — | — |
| Google | ✓ | ✓ | ✓ | — | ✓ | — | — |

---

## 5. Crate Structure

```
src/orchestrators/ai/
├── Cargo.toml
├── Dockerfile
├── src/
│   ├── main.rs                    # Bootstrap (CLI, servers, task spawning)
│   ├── lib.rs                     # Module declarations
│   ├── app_state.rs               # Shared state (generalized from current)
│   │
│   ├── catalog/                   # Offering Catalog (NEW)
│   │   ├── mod.rs
│   │   ├── traits.rs              # Offering trait definition
│   │   ├── registry.rs            # OfferingRegistry (Vec<Box<dyn Offering>>)
│   │   └── cloud.rs               # Cloud provider base implementation
│   │
│   ├── offerings/                 # Per-offering implementations (NEW)
│   │   ├── mod.rs
│   │   ├── ollama/
│   │   │   ├── mod.rs
│   │   │   ├── client.rs          # Ollama HTTP client (harvested from infra/ollama_client.rs)
│   │   │   ├── proxy.rs           # Ollama NDJSON proxy logic (harvested from api/proxy.rs)
│   │   │   ├── benchmark.rs       # Ollama benchmark payloads (harvested from tasks/benchmark.rs)
│   │   │   └── types.rs           # Ollama API response types (harvested from domain/types.rs)
│   │   ├── comfyui/
│   │   │   ├── mod.rs
│   │   │   ├── client.rs          # ComfyUI HTTP + WebSocket client
│   │   │   ├── proxy.rs           # Workflow template dispatch + image fetch
│   │   │   ├── templates.rs       # Workflow template loading + parameterization
│   │   │   └── types.rs           # ComfyUI API types
│   │   ├── speaches/
│   │   │   ├── mod.rs
│   │   │   ├── client.rs          # OpenAI-compatible audio client
│   │   │   └── proxy.rs           # Multipart form forward
│   │   ├── openedai_speech/
│   │   │   ├── mod.rs
│   │   │   ├── client.rs
│   │   │   └── proxy.rs
│   │   ├── infinity/
│   │   │   ├── mod.rs
│   │   │   ├── client.rs
│   │   │   └── proxy.rs
│   │   ├── libretranslate/
│   │   │   ├── mod.rs
│   │   │   ├── client.rs
│   │   │   └── proxy.rs
│   │   ├── huggingface/
│   │   │   ├── mod.rs
│   │   │   ├── client.rs          # HF Inference API client
│   │   │   └── proxy.rs
│   │   └── cloud/                 # Generic cloud providers
│   │       ├── mod.rs
│   │       ├── openai.rs
│   │       ├── anthropic.rs
│   │       ├── stability.rs
│   │       └── provider.rs        # Generic OpenAI-compatible cloud offering
│   │
│   ├── domain/                    # Pure domain logic (HARVESTED, generalized)
│   │   ├── mod.rs
│   │   ├── types.rs               # ServiceInstance, ServiceModel, Capability, RoutingDecision, etc.
│   │   ├── routing.rs             # select_instance (generalized: capability + priority aware)
│   │   ├── demand.rs              # DemandLedger (HARVESTED as-is, extend RequestCapability)
│   │   ├── fitness.rs             # GpuMatrix, Verdict, BenchmarkRun (HARVESTED, extend Capability)
│   │   ├── advisor.rs             # TopologyAdvice, advise_topology (HARVESTED, extend for multi-offering)
│   │   ├── recommendation.rs      # Model recommendations (HARVESTED, extend capabilities)
│   │   ├── tiering.rs             # compute_tiers (HARVESTED as-is)
│   │   ├── placement.rs           # compute_placement (HARVESTED, extend for multi-offering VRAM sharing)
│   │   ├── lease.rs               # LeaseManager (HARVESTED as-is)
│   │   ├── metrics.rs             # MetricsEngine (HARVESTED, extend per-offering tracking)
│   │   ├── policy.rs              # Auto-pull, sync, idle-delete (HARVESTED, generalize beyond Ollama)
│   │   ├── reconciliation.rs      # Drift detection (HARVESTED, generalize diff types)
│   │   ├── gpu_catalog.rs         # GPU name → score lookup (HARVESTED as-is)
│   │   └── pins.rs                # Pin overrides (NEW — model/capability → instance pinning)
│   │
│   ├── infra/                     # I/O layer (HARVESTED gateway/persistence, NEW per-offering)
│   │   ├── mod.rs
│   │   ├── gateway.rs             # Koi mDNS + Moss gateway (HARVESTED, parameterize offering name)
│   │   ├── persistence.rs         # Config (TOML) + metrics (JSON) persistence (HARVESTED as-is)
│   │   ├── stone_discovery.rs     # Koi/Moss discovery (HARVESTED, generalize offering filter)
│   │   ├── events.rs              # SSE event stream (HARVESTED as-is)
│   │   ├── tools_stream.rs        # Tools API SSE (HARVESTED, generalize offering filter)
│   │   └── cloud_store.rs         # Encrypted cloud API key persistence (NEW)
│   │
│   ├── tasks/                     # Background tasks (HARVESTED + NEW)
│   │   ├── mod.rs
│   │   ├── discovery.rs           # Multi-offering discovery (HARVESTED, iterate catalog)
│   │   ├── health_check.rs        # Per-offering health (HARVESTED, dispatch via Offering::probe)
│   │   ├── reconciliation.rs      # Drift detection per offering (HARVESTED, dispatch via Offering::enumerate)
│   │   ├── model_sync.rs          # Ollama-specific model sync (HARVESTED, offering-scoped)
│   │   ├── placement.rs           # Cross-offering placement (HARVESTED, extend VRAM sharing)
│   │   ├── advisor.rs             # Topology advisor (HARVESTED, extend for multi-offering)
│   │   ├── benchmark.rs           # Per-offering benchmarking (HARVESTED framework, dispatch via Offering::benchmark)
│   │   ├── cloud_sync.rs          # Cloud provider model enumeration refresh (NEW)
│   │   ├── metrics_processor.rs   # Metrics event processing (HARVESTED as-is)
│   │   ├── metrics_flush.rs       # Metrics persistence (HARVESTED as-is)
│   │   ├── snapshot_publisher.rs  # Dashboard snapshot (HARVESTED, extend for multi-offering)
│   │   └── gateway_announce.rs    # mDNS + gateway lifecycle (HARVESTED, parameterize offering name)
│   │
│   └── api/                       # HTTP layer (HARVESTED + NEW)
│       ├── mod.rs
│       ├── proxy.rs               # Unified proxy handler (dispatches to offering-specific proxy)
│       ├── extension.rs           # /v1/ API (HARVESTED, extend with capabilities, offerings)
│       ├── management.rs          # Model management (HARVESTED, offering-scoped)
│       ├── dashboard.rs           # Dashboard SPA + status (HARVESTED, extend for multi-offering)
│       ├── health.rs              # Liveness probe (HARVESTED as-is)
│       ├── benchmark_api.rs       # Benchmark API (HARVESTED, extend for multi-offering)
│       └── providers_api.rs       # Cloud provider CRUD (NEW)
│
├── workflows/                     # ComfyUI workflow templates (NEW)
│   ├── txt2img.json
│   ├── img2img.json
│   ├── inpaint.json
│   ├── upscale.json
│   └── animatediff.json
│
├── exercise.ps1                   # Black-box exerciser (extended for all capabilities)
└── docs/
    └── PROPOSAL-ai-orchestrator-promotion.md   # This document
```

---

## 6. Validation Rules

### 6.1 Offering Catalog Invariants

| Rule | Validation |
|------|-----------|
| **OC-1**: Every offering type has a unique `offering_type()` string | Registry rejects duplicates at startup |
| **OC-2**: Every offering declares at least one capability | `capabilities()` must be non-empty |
| **OC-3**: Every capability is served by at least one offering in a healthy deployment | Boot report warns on uncovered capabilities |
| **OC-4**: Cloud providers are never auto-discovered; they require explicit API key configuration | Discovery filter excludes cloud offerings |
| **OC-5**: Offering implementations are isolated; a bug in ComfyUI proxy must not affect Ollama routing | Separate `offerings/` modules, separate `proxy()` implementations, no shared mutable state beyond AppState |

### 6.2 Routing Invariants

| Rule | Validation |
|------|-----------|
| **RT-1**: `recommended:X` moniker always resolves to the highest-scored model for capability X, respecting priority + fitness + demand | Unit test: given known fitness matrix and demand, verify recommendation matches expected model |
| **RT-2**: Explicit model name routes to the highest-priority healthy instance that has it | Unit test: given two instances with different priorities, verify correct selection |
| **RT-3**: Pin override always wins, regardless of priority, fitness, or demand | Unit test: pinned instance selected even when unhealthy (with warning) |
| **RT-4**: Cloud providers (priority -10) are only selected when no local/garden instance serves the capability or model | Unit test: cloud instance not selected when local instance is healthy |
| **RT-5**: Blocked fitness verdict removes instance from candidate pool (except with pin) | Unit test: blocked instance excluded, pinned+blocked still selected |
| **RT-6**: Cross-offering VRAM awareness: if Ollama uses 8GB on a 24GB GPU and ComfyUI needs 12GB, the routing engine accounts for the combined 20GB | Integration test with shared-GPU stone |

### 6.3 Proxy Invariants

| Rule | Validation |
|------|-----------|
| **PX-1**: Ollama API backward compatibility — existing Ollama clients must work unchanged against the new orchestrator | Exercise script: all Ollama API paths return expected shapes |
| **PX-2**: Each capability endpoint returns the expected format (image bytes for Imagine, audio bytes for Speak, JSON for Chat/Rerank/Translate) | Per-endpoint integration tests |
| **PX-3**: Streaming responses (Ollama NDJSON, ComfyUI WebSocket progress) are forwarded without buffering the entire response | Verify first byte arrives before generation completes |
| **PX-4**: Queue depth is tracked per-instance and decremented on response completion (success or error) | Counter never goes negative; always decremented in finally/drop |
| **PX-5**: Metrics events are emitted for every proxied request (success and error) with stone, model, capability, and timing | Metrics engine receives events for all proxied requests |

### 6.4 Cloud Provider Invariants

| Rule | Validation |
|------|-----------|
| **CP-1**: API keys are encrypted at rest in `{data_dir}/providers.enc` | Never stored in plaintext in config or logs |
| **CP-2**: Cloud model enumeration refreshes periodically (every 30 min) and on provider config change | Models appear/disappear as provider updates model list |
| **CP-3**: Cloud instances have priority = `default_priority` (typically -10), never 0 | Priority is set from provider config, not auto-detected |
| **CP-4**: A model existing on both local (priority 0) and cloud (priority -10) routes to local unless pinned | Routing unit test |
| **CP-5**: Removing a cloud provider removes all its registered models from the routing pool | Cascading cleanup |

### 6.5 Health Invariants

| Rule | Validation |
|------|-----------|
| **HL-1**: Each offering type defines its own health probe endpoint and expected response | Offering trait enforces this |
| **HL-2**: Health checks run every 15s for local/garden, every 60s for cloud | Different intervals by priority tier |
| **HL-3**: An instance transitions to Unhealthy after 3 consecutive failed probes | Circuit breaker pattern (harvested) |
| **HL-4**: `GET /health` on the orchestrator returns 200 if at least one instance across any offering is healthy | Aggregated health |

### 6.6 Benchmark Invariants

| Rule | Validation |
|------|-----------|
| **BM-1**: Benchmarks are advisory — they influence recommendations but never prevent routing | Vetoed instances are deprioritized, not excluded. Only Blocked is filtered |
| **BM-2**: Each offering defines its own benchmark strategy (prompts for Ollama, workflows for ComfyUI, audio clips for Speaches) | Offering trait `benchmark()` method |
| **BM-3**: Benchmark results persist across restarts in `{data_dir}/fitness.json` | Load on startup (harvested) |
| **BM-4**: User can always override routing regardless of benchmark results | Pin mechanism + explicit model selection |

---

## 7. Exercise Script (Extended)

The current `exercise.ps1` discovers models and capabilities dynamically, then exercises them in three phases: Warm+Test, Chaos, Summary. The extended version adds:

**Phase 0: Capability Discovery**
```
GET /v1/capabilities → [chat, embed, vision, imagine, transcribe, speak, rerank, translate]
```

**Phase 1: Per-Capability Warm+Test**
- Chat: generate + chat endpoints (existing)
- Embed: embed endpoint (existing)
- Vision: chat with image (existing)
- Tools: chat with tools array (existing)
- Imagine: POST /api/imagine with test prompt → verify image bytes returned
- Transcribe: POST /api/transcribe with test audio → verify text returned
- Speak: POST /api/speak with test text → verify audio bytes returned
- Rerank: POST /api/rerank with test query + docs → verify scored results
- Translate: POST /api/translate with test text → verify translated text

**Phase 2: Cross-Capability Chaos**
- Random mixed requests across all capabilities in parallel bursts
- Exercises cross-offering VRAM contention (Imagine while Chat is running)

**Phase 3: Summary**
- Per-capability latency stats
- Per-offering health status
- Per-stone resource usage (VRAM, queue depth)

---

## 8. Implementation Phases

### Phase 1: Foundation (Greenfield Crate + Ollama Offering)

**Goal:** New crate compiles and runs, Ollama works exactly as before.

1. Create `src/orchestrators/ai/` with crate structure
2. Define `Offering` trait in `catalog/traits.rs`
3. Harvest domain layer (types, routing, demand, fitness, advisor, recommendation, tiering, placement, lease, metrics, policy, reconciliation, gpu_catalog) — generalize types (`OllamaInstance` → `ServiceInstance`, extend `Capability` enum)
4. Harvest infra layer (gateway, persistence, stone_discovery, tools_stream, events) — parameterize offering filter
5. Implement `OllamaOffering` in `offerings/ollama/` — harvest `OllamaClient`, proxy logic, benchmark payloads
6. Harvest tasks layer — dispatch via `Offering` trait instead of direct Ollama calls
7. Harvest API layer — unified proxy handler that dispatches to `offering.proxy()`
8. Implement `OfferingRegistry` that loads `OllamaOffering` at startup
9. Exercise script validates full Ollama compatibility

**Validation:** All existing exercise.ps1 tests pass against the new orchestrator.

### Phase 2: ComfyUI Offering

**Goal:** Image generation via `POST /api/imagine` and `POST /api/edit`.

1. Implement `ComfyUiOffering` in `offerings/comfyui/`
2. ComfyUI client: HTTP + WebSocket (workflow submit, progress monitoring, image fetch)
3. Workflow template library: txt2img, img2img, inpaint
4. Template parameterization engine (placeholder substitution)
5. Discovery: port probe + topology filter
6. Health: `GET /system_stats`
7. Enumeration: `GET /models/checkpoints`
8. VRAM tracking: `GET /system_stats` → devices[*].vram_free
9. Proxy: full workflow dispatch lifecycle
10. Benchmark: reference txt2img workflow
11. Exercise script: Imagine + Edit tests

**Validation:** `POST /api/imagine {"model":"flux-dev","prompt":"A zen garden"}` returns PNG bytes.

### Phase 3: Audio Offerings (Speaches + OpenedAI Speech)

**Goal:** `POST /api/transcribe` and `POST /api/speak` work.

1. Implement `SpeachesOffering` — OpenAI-compatible multipart proxy
2. Implement `OpenedAiSpeechOffering` — OpenAI-compatible JSON proxy
3. Discovery, health, enumeration for each
4. Exercise script: Transcribe + Speak tests

**Validation:** Audio round-trip: speak text → get audio → transcribe audio → verify text matches.

### Phase 4: Embedding + Rerank (Infinity)

**Goal:** Multimodal embeddings and reranking via `POST /api/embed` (extended) and `POST /api/rerank`.

1. Implement `InfinityOffering`
2. Proxy: embed (OpenAI-compatible) + rerank (custom endpoint)
3. Model enumeration from `GET /info`
4. Exercise script: embed text, embed image, rerank

### Phase 5: Translation + Cloud Providers

**Goal:** `POST /api/translate` and cloud provider registration.

1. Implement `LibreTranslateOffering`
2. Implement cloud provider framework: `CloudProviderOffering` base, per-provider specifics (OpenAI, Anthropic, Stability)
3. API key management with encrypted persistence
4. Cloud model enumeration (periodic refresh)
5. Dashboard: provider management UI
6. Exercise script: translate + cloud fallback test

### Phase 6: HuggingFace + Polish

**Goal:** HF Inference as a multi-capability offering. Dashboard generalization.

1. Implement `HuggingFaceOffering`
2. Task-based routing (pipeline_tag → capability mapping)
3. Cold-start handling (X-Wait-For-Model)
4. Dashboard: multi-offering views, capability grid
5. Full exercise script covering all capabilities

---

## 9. Clean Architecture Mandate

### 9.1 Layer Rules

```
offerings/ ──→ catalog/ ──→ domain/ ←── tasks/
                  │                       │
                  ▼                       ▼
               infra/ ←────────────── api/
```

| Rule | Enforcement |
|------|-------------|
| `domain/` has ZERO I/O — no async, no HTTP, no filesystem | No `tokio`, `reqwest`, `std::fs` in domain imports. Only `std`, `serde`, `chrono` |
| `offerings/` implement the `Offering` trait from `catalog/` | All offering-specific I/O (HTTP clients, WebSocket) lives here |
| `catalog/` defines traits and the registry — no offering-specific logic | No `if offering_type == "ollama"` in catalog code |
| `infra/` handles shared I/O (Koi, Moss, persistence, events) | No offering-specific code |
| `tasks/` orchestrate background loops by calling `Offering` methods and `domain` algorithms | No direct HTTP calls — always through offerings or infra |
| `api/` handles HTTP requests by calling `domain` algorithms and dispatching to offerings | Proxy handler routes by capability, dispatches to `offering.proxy()` |

### 9.2 Testing Rules

| Rule | Detail |
|------|--------|
| Domain logic has comprehensive unit tests with no I/O mocks | All domain types are pure — test with `assert_eq!` and constructed data |
| Each offering has integration tests against a real or mocked service | Use containers for CI, mock HTTP for unit tests |
| Exercise script is the acceptance test gate | Must pass before merge |
| Cross-offering scenarios have dedicated integration tests | Shared-GPU VRAM contention, capability fallback chains |

### 9.3 Harvest Rules

When harvesting code from the Ollama orchestrator:

| Rule | Detail |
|------|--------|
| **Extract, don't copy** | Understand the pattern, rewrite for generality. Don't paste Ollama code and add `if`/`match` branches |
| **Generalize types first** | `OllamaInstance` → `ServiceInstance`, `RequestCapability` → `Capability` with extended variants |
| **Keep tests** | Every harvested domain module brings its tests, updated for new types |
| **Measure coverage** | Harvested domain modules must maintain or exceed current test coverage |
| **Mark provenance** | Comment `// Harvested from ollama-orchestrator domain/routing.rs` at module level for audit |

---

## 10. Data Directory Layout

```
{data_dir}/
├── config.toml              # Router config (features, pins, stone overrides)
├── providers.enc            # Encrypted cloud provider API keys
├── fitness.json             # Benchmark results (GPU matrix)
├── placement.json           # Current placement plan
├── metrics/
│   ├── {stone_name}.json    # Per-stone cumulative metrics
│   └── summary.json         # Global metrics snapshot
└── workflows/               # ComfyUI workflow templates
    ├── txt2img.json
    ├── img2img.json
    ├── inpaint.json
    └── custom/              # Operator-added templates
```

---

## 11. Migration Path

The Ollama orchestrator continues to work as-is during development. The AI orchestrator is a new crate with a new binary name (`zen-garden-ai-orchestrator`), new Docker image, and new default ports (`:21434` proxy, `:7190` dashboard — same as current).

**Transition:**
1. Both orchestrators can coexist during migration (different offering names in Moss)
2. When the AI orchestrator passes all exercise tests for Ollama, operators switch
3. The old `ollama` crate is archived (not deleted — reference for harvest audit)
4. Docker image label changes: `zen-garden.ai.orchestrator`

**Backward compatibility:**
- The AI orchestrator's proxy port serves the full Ollama API surface
- Existing Ollama clients see no difference
- `GET /` returns "Ollama is running" for Ollama client compatibility
- New capability endpoints (`/api/imagine`, `/api/transcribe`, etc.) are additive

---

## 12. Open Questions

1. **Shared WebSocket pool for ComfyUI:** Should the orchestrator maintain persistent WebSocket connections to ComfyUI instances, or connect per-request? Persistent is faster but adds connection management complexity.

2. **Workflow template versioning:** When a ComfyUI instance updates and node APIs change, templates may break. How to detect and handle template/instance version drift?

3. **Cloud provider API key rotation:** Should the orchestrator detect expired keys and disable the provider automatically, or just log errors and let the operator fix it?

4. **Cross-offering benchmark coordination:** When benchmarking ComfyUI on a stone that also runs Ollama, should the benchmark unload Ollama models first to get clean VRAM measurements?

5. **Multi-stone ComfyUI:** ComfyUI doesn't have a model sync/pull mechanism like Ollama. If two stones run ComfyUI with different checkpoints, how does the orchestrator handle model availability across the fleet?
