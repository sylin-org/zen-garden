# External Patterns Research

> Research artifact for ORCH-0013. Documents external systems studied,
> patterns extracted, and how they apply to the AI orchestrator.

---

## 1. Adapter/Plugin Trait Design

### Systems Studied

**Envoy Proxy** — Network filter architecture
- Trait surface: `onData(bytes, end_stream) -> FilterStatus` (2-3 required methods per filter direction)
- Protocol dispatch is configuration, not code branching — listener config selects filter chains
- Optional capabilities via default method implementations
- Graceful shutdown is a separate composable trait (`DrainableFilterChain`)

**Containerd** — Runtime plugin interface
- `PlatformRuntime` interface: 4 methods (Create, Get, Tasks, Delete) + ID
- Process boundary as the adapter layer — the shim binary IS the adapter
- `google.protobuf.Any` escape hatch for runtime-specific config (opaque passthrough)
- Synthesized events on failure — if adapter dies, manager fakes a clean exit

**Dapr** — Component abstraction for heterogeneous backends
- Mandatory surface: Init + CRUD + Features + Close (5-6 methods)
- `Features()` returns string constants declaring capabilities at registration
- Optional interfaces via Go type assertion (Rust equivalent: trait objects + downcast)
- `DefaultBulkStore` wraps base trait to synthesize missing bulk operations
- `Operations()` for open-ended capability sets (bindings declare their verbs)

### Convergent Pattern

All three systems converge on:
1. **Tiny mandatory surface** (2-6 methods)
2. **Capability advertisement** at init/registration time
3. **Optional capabilities** via separate traits/interfaces
4. **Default implementations** that synthesize advanced behavior from base methods
5. **Opaque config passthrough** for adapter-specific settings

### Application to AI Orchestrator

The `Offering` trait in ORCH-0013 has 8 methods — slightly larger than the
ideal 5-6, but justified because AI orchestration has more dimensions than
CRUD (probe + enumerate + proxy + benchmark + sync + vram_estimate +
capabilities + discovery_config). Comparing:

| Concern | Envoy | Containerd | Dapr | AI Orchestrator |
|---------|-------|------------|------|-----------------|
| Identity | — | `ID()` | `Features()` | `offering_type()` + `capabilities()` |
| Lifecycle | `onNewConnection()` | `Create()` | `Init()` | `discovery_config()` |
| Health | (filter status) | `State()` | (health.Pinger optional) | `probe()` |
| Data | `onData()` | `Tasks()` | `Get()`/`Set()` | `enumerate()` + `proxy()` |
| Cleanup | `onClose()` | `Delete()` | `Close()` | (shutdown via CancellationToken) |
| Advanced | default impls | `Any` passthrough | optional interfaces | `benchmark()` + `sync_resource()` + `vram_estimate()` |

**Decision**: Keep 8 methods. `benchmark()` and `sync_resource()` could be
optional (not all offerings need benchmarking or cross-instance sync), but
making them required with default no-op implementations is simpler than
runtime type assertion. Follow Envoy's pattern: default implementations
over optional interfaces.

---

## 2. Inference Routing

### Systems Studied

**vLLM** — Token-level GPU-aware scheduling
- PagedAttention for KV cache management (O(1) block allocation)
- FCFS scheduling with chunked prefill (decode priority over prefill)
- Preemption when KV cache exhausted (recompute strategy)
- Single-model per instance — multi-model via external orchestrator (llm-d)
- llm-d adds prefix-aware routing: 60% TTFT reduction by routing to pods with warm KV cache

**HuggingFace TGI** — Continuous batching
- `WAITING_SERVED_RATIO` heuristic: pause running batch for new prefills when ratio exceeded
- `MAX_WAITING_TOKENS` hard cap on decode tokens before forced batch admission
- Simpler than vLLM — ratio-based heuristics vs explicit page management
- Pre-computes KV cache budget at startup from available GPU memory

**NVIDIA Triton** — Multi-model serving
- Instance groups: explicit model-to-GPU pinning
- Rate limiter with abstract resource types (GPU_MEMORY as configurable budget)
- No automatic VRAM budgeting — relies on offline Model Analyzer
- Designed for ML pipelines, not LLM inference (no PagedAttention)

**Ray Serve** — Distributed inference
- Pluggable `RequestRouter` with `choose_replicas()` override
- Composite scoring: cache hit → locality → throughput → queue depth
- Model multiplexing with LRU eviction in replica cache
- Prefix-aware routing (same concept as llm-d)

### Comparison with Ollama Orchestrator

| Dimension | Best External | Ollama Orchestrator | Assessment |
|-----------|--------------|---------------------|------------|
| Demand tracking | None comparable | 3-window exponential decay (15min/6h/3d) | **State of the art** — unique across all systems |
| Fitness profiling | Triton Model Analyzer (offline) | Empirical benchmark matrix (online) | **State of the art** — live scoring per (model, capability, stone) |
| VRAM awareness | vLLM PagedAttention (intra-instance) | Auto-tiered from hardware discovery | **Good** — appropriate for inter-instance routing |
| Reservation | None comparable | Demand-weighted reservation mode | **Novel** — anticipates future load patterns |
| KV cache routing | llm-d prefix-aware | Not available | **Gap** — contingent on Ollama exposing cache metadata |
| Queue management | vLLM preemption | Queue depth + idle-first sort | **Adequate** — preemption is intra-instance concern |

### Application to AI Orchestrator

The Ollama orchestrator's routing engine is already more sophisticated than
external alternatives for the multi-instance, multi-model use case. The key
extensions for multi-offering are:

1. **Capability filter** (new): Before model matching, filter by offering capability
2. **Priority gate** (new): Cloud providers at -10 excluded when local serves capability
3. **Cross-offering VRAM** (new): `StoneVramBudget` aggregates VRAM across all offerings on a stone
4. **Abstract resource budgets** (from Triton): Consider if VRAM-only tiering becomes insufficient for non-LLM workloads — but start with VRAM and generalize only if needed

The demand-weighted reservation, fitness scoring, and tiering patterns
should be **copied directly** from the Ollama orchestrator with only type
signature changes.

---

## 3. Cloud Provider Abstraction

### Systems Studied

**LiteLLM** — 50+ providers
- Provider interface: `validate_environment()`, `get_complete_url()`, `transform_request()`, `transform_response()`, `get_supported_openai_params()`
- `provider/model` naming convention (universal)
- `os.environ/VAR_NAME` config syntax (keys never in config files)
- Router: 3-layer (fallback → retry → call), configurable strategies (shuffle, least-busy, latency-based)
- Anthropic handler: extracts system messages, maps tools (`parameters` → `input_schema`), injects required `max_tokens`

**OpenRouter** — 300+ models
- Single unified OpenAI-compatible endpoint
- Per-request `provider` object with `order`, `allow_fallbacks`, `sort`, `only`, `ignore`
- Model variants: `:free`, `:nitro`, `:floor`, `:exacto`, `:online`
- Server-side translation — callers never see provider-specific formats

**Portkey Gateway** — 70+ providers
- Static TypeScript provider registry with declarative parameter mapping
- `ProviderConfig` maps OpenAI params to provider params declaratively (`stop` → `stop_sequences`, `user` → `metadata.user_id`)
- Routing modes: `fallback` (sequential), `loadbalance` (weighted random), `conditional`
- Composable: fallback targets can be nested load balancers

### Anthropic API Differences (Critical)

| Aspect | OpenAI | Anthropic |
|--------|--------|-----------|
| Auth header | `Authorization: Bearer sk-...` | `x-api-key: sk-ant-...` |
| Version header | None | `anthropic-version: 2023-06-01` (required) |
| Endpoint | `POST /v1/chat/completions` | `POST /v1/messages` |
| System prompt | In messages array `{role: "system"}` | Top-level `system` parameter |
| `max_tokens` | Optional | **Required** |
| Temperature | 0-2.0 | 0-1.0 |
| Stop sequences | `stop: [...]` | `stop_sequences: [...]` |
| Tool definition wrapper | `{type: "function", function: {parameters: ...}}` | `{name: ..., input_schema: ...}` (no wrapper) |
| Response content | `choices[0].message.content` (string) | `content[]` (typed blocks: text, tool_use, thinking) |
| Finish reason | `finish_reason` | `stop_reason` |
| Streaming | `data:` lines, `data: [DONE]` terminator | Named SSE events (`message_start`, `content_block_delta`, etc.) |
| Message alternation | Allows consecutive same-role | **Strict user/assistant alternation** |

### Application to AI Orchestrator

1. **`provider/model` naming** — adopt universally (matches all three systems)
2. **API keys via environment** — never in config files; encrypted at rest in `{data_dir}/providers.enc`
3. **OpenAI as lingua franca** — translate to/from for each provider
4. **Anthropic needs dedicated handler** — too many structural differences for generic OpenAI-compat adapter
5. **Cloud instances register with priority -10** — routing engine's priority gate (RT-4) handles fallback
6. **Fallback is handled by the routing engine**, not by the cloud adapter — no need for LiteLLM-style fallback chains within the cloud layer

---

## 4. Multi-Protocol Proxy Dispatch

### Systems Studied

**Envoy** — Validates per-listener model. Cannot mix `tcp_proxy` and
`http_connection_manager` in the same filter chain. Multiple listeners on
different ports is the clean path for protocol diversity.

**Traefik** — EntryPoints (one per port). TCP routers evaluated before
HTTP routers on shared EntryPoints. True protocol diversity requires
separate EntryPoints.

**Kong** — Two WebSocket modes: transparent pass-through (HTTP services)
and message-aware (dedicated `ws`/`wss` services with WebSocket PDK).

**Axum** — The actual framework in use.

### Axum-Specific Patterns

**Multiple listeners**: `tokio::select!` with separate `axum::serve(listener, router).with_graceful_shutdown(token.cancelled_owned())` per port. Already used by the Ollama orchestrator (proxy + dashboard).

**WebSocket proxy** (for ComfyUI):
- Accept upgrade on axum side via `WebSocketUpgrade`
- Connect to backend via `tokio-tungstenite::connect_async()`
- Relay bidirectionally with `split()` + two `tokio::spawn` tasks
- Manual message type conversion between `axum::ws::Message` and `tungstenite::Message`
- WebSocket handlers need independent CancellationToken awareness (axum#3003)

**Multipart forwarding** (for whisper.cpp):
- Stream body directly: `Body::wrap_stream(req.into_body().into_data_stream())`
- 87.5% less memory vs buffering (benchmarked)
- The existing 50MB buffer in `proxy.rs` works for JSON but must NOT be used for audio uploads

**Binary streaming** (for TTS):
- `Body::from_stream(resp.bytes_stream())` with `transfer-encoding: chunked`
- Already used by the Ollama proxy for NDJSON — same pattern for audio/image bytes

**Graceful shutdown caveat** (axum#3326): CancellationToken + axum graceful
shutdown can hang with lost wakers. Workaround: use `signal::ctrl_c()`
directly or add a timeout on the graceful shutdown period.

### Application to AI Orchestrator

**Per-port dispatch is the right architecture.** Each service type gets its
own listener speaking its native protocol:

| Port | Protocol Handler | Key Pattern |
|------|-----------------|-------------|
| 21434 | Ollama NDJSON streaming | Existing tee-and-inspect pattern from Ollama proxy |
| 21435 | ComfyUI WebSocket relay | Bidirectional WS proxy with `tokio-tungstenite` |
| 21436 | whisper.cpp multipart | Streaming body forwarding (no 50MB buffer) |
| 21437 | Speaches OpenAI-compat | Standard JSON proxy |
| 21438 | OpenedAI Speech | Streaming audio response |
| 21439 | Infinity OpenAI-compat | Standard JSON proxy |
| 21440 | LibreTranslate | Standard JSON proxy |
| 7190 | Dashboard | SPA + SSE + REST |

Each router is built independently and bound to its port. The
`CancellationToken` propagates shutdown to all listeners.

---

## Sources

### Adapter Patterns
- Envoy `envoy/network/filter.h` — filter interface hierarchy
- Containerd `core/runtime/runtime.go` — PlatformRuntime interface
- Containerd `api/runtime/task/v3/shim.proto` — shim protocol
- Dapr `state/store.go` — state store interface
- Dapr `pubsub/pubsub.go` — pubsub interface

### Inference Routing
- vLLM V1 Architecture Blog (2025-01-27)
- llm-d KV-Cache Architecture (github.com/llm-d)
- HuggingFace TGI Architecture docs
- NVIDIA Triton Rate Limiter docs
- Ray Serve Custom Request Routing docs
- Ray Serve LLM Architecture Overview

### Cloud Providers
- LiteLLM Provider Registration docs
- LiteLLM Router Architecture docs
- OpenRouter Provider Selection docs
- Portkey Gateway source (github.com/Portkey-AI/gateway)
- Anthropic Streaming API docs

### Multi-Protocol Proxy
- Envoy Life of a Request docs
- Traefik EntryPoints docs
- Kong WebSocket PDK docs
- Axum examples (websockets, reverse-proxy, graceful-shutdown)
- Axum issues #3003 (WebSocket + shutdown), #3326 (CancellationToken + shutdown)
- Adam Chalmers streaming proxy benchmarks
