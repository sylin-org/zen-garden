# AI Orchestrator Implementation Prompt

> Use this prompt to start a fresh implementation of the AI orchestrator (ORCH-0013).
> A previous implementation was reverted because it was operationally non-functional
> despite compiling and passing tests. Read the "Implementation Lessons" section of
> the ADR before writing any code.

---

## Context

You are implementing the Zen Garden AI Orchestrator — a single binary that manages
ALL AI service types (Ollama, ComfyUI, whisper.cpp, OpenedAI Speech, Infinity,
LibreTranslate, and cloud providers) through an offering adapter pattern.

### What Already Exists (DO NOT recreate)

1. **ADR**: `docs/decisions/ORCH-0013-ai-orchestrator-promotion.md` — full architecture,
   offering specs, validation rules, implementation plan, AND failure lessons from the
   first attempt. **Read the "Implementation Lessons" section first.**

2. **Offering manifests**: `src/moss/embedded/manifests/sw/ai/` — snippet.yaml,
   frontmatter.json, compatibility.yaml, adopted.yaml, and research.md for all 7
   services. These are researched and verified. Use the research.md files as the
   authoritative API reference for each service.

3. **Ollama orchestrator** (reference implementation): `src/orchestrators/ollama/` —
   a fully functional, production-running orchestrator. This is your primary source
   of truth for:
   - How to connect to Koi from inside Docker (`host.docker.internal:5641`)
   - How to register gateways with Moss
   - How to structure the discovery task (3-phase: stone → topology → SSE)
   - How to structure the proxy (NDJSON streaming, queue depth, metrics extraction)
   - How to structure the dashboard (2,616-line embedded SPA)
   - Configuration defaults, environment variables, Docker networking
   - The exercise.ps1 black-box exerciser

4. **Orchestrator-common**: `src/orchestrators/common/` — shared infrastructure
   (Koi mDNS, Moss gateway, tools stream, topology, persistence, SSE, stone catalog).
   The Ollama orchestrator already uses all of these. Study its imports.

5. **Domain analysis**: The Ollama orchestrator's domain layer (~6,300 LOC) was
   assessed module by module. ~70% is generic (routing, demand, fitness, placement,
   metrics, tiering, lease, reconciliation, gpu_catalog, recommendation, advisor).
   ~30% is Ollama-specific (HTTP client, NDJSON parsing, model pull, benchmark prompts).

### What Needs To Be Built

A new crate at `src/orchestrators/ai/` that:
- Starts as a Docker container with correct Koi/Moss connectivity
- Discovers all AI offering instances across the garden
- Routes capability requests to the optimal instance
- Provides Ollama backward compatibility (existing clients work unchanged)
- Serves a management dashboard
- Supports cloud providers as priority -10 fallbacks

---

## Implementation Approach

**DO NOT** build types outward (domain → adapters → tasks → API).
**DO** build operational requirements inward (Docker → startup → discovery → routing → dashboard).

### Block 1: Operational Foundation

**Goal:** Container starts, connects to Koi, discovers a stone, registers gateway.

1. Create `src/orchestrators/ai/` with Cargo.toml matching the Ollama orchestrator's
   dependency pattern exactly.

2. Copy the Ollama orchestrator's `Dockerfile` and adapt it. The key configuration
   values must be harvested from the Ollama orchestrator, not guessed:
   - Koi endpoint: check `src/orchestrators/ollama/src/main.rs` for the default
   - Ports: proxy 21434, dashboard 7190
   - Data directory: `/data`
   - Log level: default `info`
   - All `--arg` defaults and `env` mappings

3. Write `main.rs` by studying the Ollama orchestrator's startup sequence:
   - How it initializes tracing
   - How it loads config
   - How it creates channels
   - How it constructs AppState
   - How it spawns tasks
   - How it binds servers
   - The exact order matters

4. Write the discovery task by harvesting `src/orchestrators/ollama/src/tasks/discovery.rs`.
   Generalize `OllamaInstance` → `ServiceInstance` but preserve all operational behavior:
   - Stone resolution cascade (explicit → cached → Koi mDNS)
   - Topology query as authoritative initial load
   - SSE stream subscription with tools_stream filtering
   - Periodic topology refresh alongside SSE
   - Profile-and-register with HW data merging
   - Error handling (probe failure → register as unhealthy, not silently skip)

5. Write the gateway announce task by harvesting
   `src/orchestrators/ollama/src/tasks/gateway_announce.rs`.
   For the AI orchestrator, register per-offering (one gateway entry per offering type).

**Verification:** Build the container. Start it. It must show in logs:
- `Starting AI Orchestrator proxy_port=21434 dashboard_port=7190`
- `Gateway: mDNS registered via Koi`
- `topology returned ... stones`
- `instance profiled and added to routing pool`

If Koi is not reachable, the logs must show retry messages with the correct Koi
endpoint (not `127.0.0.1`). If a stone is reachable, at least one instance must
appear in the registry.

### Block 2: Ollama Feature Parity

**Goal:** Replace the Ollama orchestrator with zero regressions.

1. Generalize the domain layer (routing, demand, fitness, metrics, placement, policy,
   tiering, lease, reconciliation, gpu_catalog, recommendation, advisor). The ADR has
   the classification of what's shared vs Ollama-specific.

2. Implement the OllamaOffering adapter (Offering trait). Harvest from
   `src/orchestrators/ollama/src/infra/ollama_client.rs`.

3. Wire all tasks (health_check, reconciliation, metrics_processor, metrics_flush,
   snapshot_publisher, benchmark, placement, resource_sync).

4. Wire all API routes. The ADR lists every endpoint for both proxy and dashboard ports.

5. Implement Ollama backward compatibility routes (`GET /`, `/api/tags`, `/api/ps`,
   `/api/version`, `/api/show`, `/api/pull`, `/api/delete`).

**Verification:** Run `exercise.ps1` from the Ollama orchestrator against the AI
orchestrator's proxy port. All tests must pass.

### Block 3: Multi-Offering Extension

**Goal:** Each offering type works end-to-end.

For each service (ComfyUI, whisper.cpp, OpenedAI Speech, Infinity, LibreTranslate,
Speaches):

1. Read the research.md in `src/moss/embedded/manifests/sw/ai/{service}.research.md`.
2. Implement the Offering trait adapter.
3. Verify against a running instance of that service:
   - Probe succeeds
   - Enumerate returns real models/resources
   - Proxy forwards a request and returns a response
4. Only move to the next service after the current one works end-to-end.

### Block 4: Dashboard

**Goal:** Operators can monitor and manage the orchestrator through a web UI.

1. Study the Ollama dashboard (`src/orchestrators/ollama/assets/dashboard.html`,
   2,616 lines). Understand every section, every API call, every interaction.
2. Build a React + TypeScript + Tailwind dashboard (or a single-file SPA — either
   pattern works if the result is functional).
3. The dashboard must show real data from real instances. Not hardcoded, not stubbed.

### Block 5: Cloud Providers

**Goal:** Cloud APIs as priority -10 fallbacks.

Study LiteLLM and OpenRouter patterns for provider abstraction. Key points:
- `provider/model` naming convention
- API keys via environment variables (never in config files)
- Anthropic uses `x-api-key` header and Messages API (not OpenAI-compatible)
- Cloud instances must be registered in the instance registry (not just the catalog)

---

## Critical Rules

1. **Never guess a configuration value.** Check the Ollama orchestrator first.
   If it's not there, check orchestrator-common. If it's not there, check the
   Moss codebase. If none of those have it, ask.

2. **Never declare a phase complete based on compilation + unit tests.**
   Verify against a running system. "It compiles" is not a deliverable.

3. **Never create bespoke code for a problem the existing codebase already solves.**
   Koi resolution, Moss gateway registration, Docker networking, SSE streaming,
   topology queries — all solved. Harvest, don't reinvent.

4. **Break the problem into blocks with clear interfaces.**
   Each block has a concrete operational verification criterion.
   Do not build all adapters in parallel before any single one works.

5. **The Ollama orchestrator is the ground truth for operational behavior.**
   Study it as both source code AND a running system. Match its behavior
   before extending it.

---

## Files to Read First

In this order:

1. `docs/decisions/ORCH-0013-ai-orchestrator-promotion.md` — especially the
   "Implementation Lessons" section at the end
2. `src/orchestrators/ollama/src/main.rs` — startup sequence
3. `src/orchestrators/ollama/Dockerfile` — Docker configuration
4. `src/orchestrators/ollama/src/tasks/discovery.rs` — discovery pipeline
5. `src/orchestrators/ollama/src/tasks/gateway_announce.rs` — gateway registration
6. `src/orchestrators/ollama/src/app_state.rs` — shared state
7. `src/orchestrators/ollama/src/api/proxy.rs` — proxy handler
8. `src/orchestrators/common/src/` — all shared infrastructure modules
9. `src/moss/embedded/manifests/sw/ai/*.research.md` — service API references
