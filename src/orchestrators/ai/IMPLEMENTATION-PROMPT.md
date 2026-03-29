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

This is NOT another Ollama orchestrator. It is a **capability router across
heterogeneous AI services**, of which Ollama is one managed part. The Ollama
orchestrator is a source of infrastructure patterns and harvestable domain logic —
not the architectural template.

### Architecture: Monolith of Bounded Contexts

The AI orchestrator is a monolith binary, but its internal composition is
**microservice-like bounded contexts** — one per offering type. Think of each
offering adapter (Ollama adapter, ComfyUI adapter, Infinity adapter, HuggingFace
adapter, etc.) as a self-contained microservice within the main service:

- Each adapter owns its own HTTP client, proxy protocol, health semantics,
  model enumeration, benchmark payloads, and error handling.
- Adapters share infrastructure (routing, demand, fitness, metrics, discovery)
  but never reach into each other's internals.
- Domain logic that's offering-agnostic lives in the shared orchestration layer.
  Domain logic that's offering-specific lives inside the adapter.

**What "bounded context" means here:** Each adapter is a self-contained module
(`offerings/{service}/`) that encapsulates all protocol-specific knowledge.
Nothing outside that directory knows about the service's API shapes, auth
mechanism, streaming format, or model management protocol.

**What it does NOT mean:** Adapters are NOT isolated processes, do NOT have
their own AppState, channels, or task supervisors. They share the single
`AppState`, the single set of broadcast channels, the single task supervisor.
The isolation is at the code/module boundary, not at the runtime boundary.
The offering adapter trait is the interface between the shared infrastructure
and the service-specific code.

### Port Forwarding: Per-Service Proxy Ports

The AI orchestrator **replaces** the Ollama orchestrator. It offers a
dedicated proxy port per service type in its own port range (21434+), so
it never conflicts with the actual service instances running on the same
stone or elsewhere in the garden:

| Port  | Service | Protocol | Clients Connect Here Instead Of |
|-------|---------|----------|-------------------------------|
| 21434 | Ollama | Ollama API (OpenAI-compat) | `:11434` (native Ollama) |
| 21435 | ComfyUI | Custom workflow + WebSocket | `:8188` (native ComfyUI) |
| 21436 | whisper.cpp | Multipart `/inference` | `:8080` (native whisper.cpp) |
| 21437 | Speaches | OpenAI `/v1/audio/*` | `:8000` (native Speaches) |
| 21438 | OpenedAI Speech | OpenAI `/v1/audio/speech` | `:8001` (native OpenedAI Speech) |
| 21439 | Infinity | OpenAI `/embeddings` + `/rerank` | `:7997` (native Infinity) |
| 21440 | LibreTranslate | Custom `/translate` | `:5000` (native LibreTranslate) |
| 7190  | Dashboard | HTTP | Operator browser |

Each proxy port speaks the native protocol of its service. An Ollama client
connecting to `:21434` sees a standard Ollama API. A ComfyUI workflow tool
connecting to `:21435` sees a standard ComfyUI API. The orchestrator routes
internally based on which port received the request, dispatching to the
correct adapter's proxy logic.

The port numbering (21434+) mirrors the Ollama orchestrator's existing
convention — 21434 was chosen to not conflict with Ollama's native port
(11434). The same principle applies to all services.

**Note:** The exact port assignments should be finalized during implementation
based on what works operationally. The key principle is: orchestrator proxy
ports live in a dedicated range and never collide with native service ports.

---

## What Already Exists (DO NOT recreate)

1. **ADR**: `docs/decisions/ORCH-0013-ai-orchestrator-promotion.md` — full architecture,
   offering specs, validation rules, implementation plan, AND failure lessons from the
   first attempt. **Read the "Implementation Lessons" section first.**

2. **ORCH-0012**: `docs/decisions/ORCH-0012-cluster-adapter-extraction.md` — a
   **pattern precedent**, not a direct dependency. ORCH-0012 extracted shared cluster
   management primitives from the MongoDB orchestrator into `orchestrator-common::cluster`.
   The AI orchestrator applies the **same decomposition approach** to the Ollama
   orchestrator's AI-agnostic logic — but this is a separate extraction effort.
   ORCH-0012's cluster module is for stateful database orchestrators (MongoDB, PostgreSQL).
   The AI orchestrator extracts a different set of primitives (routing, demand, fitness,
   placement) that are specific to AI inference orchestration. Read ORCH-0012 for the
   methodology, not for the types.

3. **Offering manifests**: `src/moss/embedded/manifests/sw/ai/` — snippet.yaml,
   frontmatter.json, compatibility.yaml, adopted.yaml, and research.md for all 7
   services. These are researched and verified. Use the research.md files as the
   authoritative API reference for each service.

4. **Ollama orchestrator**: `src/orchestrators/ollama/` — a fully functional,
   production-running orchestrator. Contains two kinds of code:
   - **Harvestable (~70%)**: routing, demand, fitness, placement, recommendation,
     metrics, tiering, lease, reconciliation, gpu_catalog, advisor, proxy pattern,
     dashboard pattern, gateway lifecycle, discovery pipeline
   - **Ollama-specific (~30%)**: HTTP client for Ollama API, NDJSON streaming,
     model pull protocol, benchmark prompts, `/api/tags` response shapes

5. **Orchestrator-common**: `src/orchestrators/common/` — shared infrastructure
   (Koi mDNS, Moss gateway, tools stream, topology, persistence, SSE, stone catalog,
   cluster primitives). Both the Ollama and MongoDB orchestrators depend on this.

---

## The Engineering Task

**Decompose the Ollama orchestrator into shared orchestration infrastructure and an
Ollama-specific adapter. Then build the AI orchestrator on top of the shared layer.**

This is not "build a new thing and copy patterns." It is:

1. **Extract** the offering-agnostic orchestration logic from the Ollama orchestrator
   into a shared layer (either extending `orchestrator-common` or creating a new
   `orchestrator-ai-core` module within the AI orchestrator crate).
2. **Prove** the extraction by keeping the Ollama orchestrator functional on top of it
   (or by the AI orchestrator achieving Ollama feature parity).
3. **Extend** the shared layer with multi-offering support: the capability enum,
   the offering adapter trait, per-offering discovery filtering, and the
   capability-routing extensions.
4. **Build** offering adapters for each service on top of the shared + extended layer.

This way the AI orchestrator inherits correct operational behavior **by construction**
— it uses the same infrastructure code that makes the Ollama orchestrator work,
not a hand-written approximation.

---

## Implementation Approach

### Block 1: Harvest the Shared Orchestration Layer

**Goal:** The Ollama orchestrator's offering-agnostic logic lives in a shared location.

1. **Analyze** each module in `src/orchestrators/ollama/src/domain/`:
   - What types reference `OllamaInstance` specifically?
   - What types are already generic (operate on endpoint strings, VRAM numbers, etc.)?
   - What functions take `&HashMap<String, OllamaInstance>` that could take a trait?

2. **Extract** the generic parts. The ADR's module assessment provides the classification:
   | Shared Domain | routing, demand, fitness, tiering, placement, lease, metrics, policy, reconciliation, gpu_catalog, recommendation, advisor |
   | Shared Infra | gateway lifecycle, discovery pipeline, persistence, SSE events |
   | Ollama-Specific | ollama_client, NDJSON proxy, benchmark prompts, Ollama API types |

3. **Define the offering adapter trait** (the equivalent of ORCH-0012's `ClusterAdapter`):
   - `probe()` — health check
   - `enumerate()` — list models/resources
   - `proxy()` — forward requests
   - `benchmark()` — fitness profiling
   - `sync_resource()` — model sync

4. **Generalize types**: `OllamaInstance` → `ServiceInstance` with offering-kind
   discriminator. `RequestCapability` / `fitness::Capability` → unified `Capability`
   enum extended for non-LLM capabilities.

**Verification:** The generalized domain modules compile, pass their existing tests
(adapted for the new types), and the Ollama orchestrator can be wired to use them
(either directly or via the new AI orchestrator achieving Ollama parity).

### Block 2: Operational Foundation

**Goal:** The AI orchestrator binary starts, connects to infrastructure, and discovers
instances — using the harvested shared layer, not bespoke reimplementations.

1. **Harvest operational configuration** from the Ollama orchestrator. Do not invent
   values — read them from the source:
   - Koi endpoint default: open `src/orchestrators/ollama/src/main.rs`, find the
     `--koi` CLI arg, read its `default_value` and `env` attribute. Do NOT hardcode
     a value from this document — read the actual source. The Ollama orchestrator
     uses `host.docker.internal` (not `localhost` or `127.0.0.1`) because it runs
     inside a Docker container that needs to reach Koi on the host. Verify the
     port from the Koi crate's configuration, not from memory.
   - Ports, data dir, log level: read every `#[arg(...)]` in the Ollama
     orchestrator's `Cli` struct and replicate the defaults and env var names.
   - Docker networking: study the Ollama `Dockerfile` for EXPOSE, ENV defaults,
     and how it connects to host services.

2. **Wire the startup sequence.** The AI orchestrator's `main.rs` binds **multiple
   listener ports** — one per offering type's native protocol (see port table above)
   plus the dashboard port. Each port dispatches to the correct offering adapter's
   proxy method. The infrastructure plumbing (tracing, config loading, channel
   creation) follows the Ollama orchestrator's patterns because it uses the same
   shared infrastructure crates.

3. **Wire the discovery task** using the shared pipeline. The AI orchestrator
   extends it to filter for ALL AI offering types (not just `offering:ollama`),
   and dispatches profiling through the offering adapter trait.

4. **Wire the gateway announce task.** The AI orchestrator registers a gateway entry
   per managed offering type (one `PUT /api/v1/garden/gateway/{offering}` per type),
   each pointing to the corresponding native proxy port.

**Verification:** Build the container. Start it against a real garden with Koi running.
Logs must show:
- Correct Koi endpoint (not `127.0.0.1`)
- Successful mDNS registration
- Topology discovery with instance profiling
- Gateway registration for each offering type
- Each per-service port listening and accepting connections

### Block 3: Ollama Feature Parity

**Goal:** The AI orchestrator can replace the Ollama orchestrator with zero regressions.

1. **Implement the OllamaOffering adapter as a bounded context.** This is the first
   "microservice within the monolith." It owns:
   - `offerings/ollama/client.rs` — HTTP client for Ollama API (harvest from
     `ollama_client.rs`)
   - `offerings/ollama/proxy.rs` — NDJSON streaming proxy logic (harvest from
     `api/proxy.rs`)
   - `offerings/ollama/benchmark.rs` — Ollama-specific test payloads
   - `offerings/ollama/types.rs` — Ollama API response shapes
   - The Offering trait implementation that ties it together

   Nothing outside the `offerings/ollama/` directory should know about Ollama's
   API shapes, NDJSON format, or model pull protocol. The shared layer sees only
   `ServiceInstance`, `Capability`, and the trait interface.

2. **Wire all shared tasks** through the offering adapter trait: health_check,
   reconciliation, metrics_processor, metrics_flush, snapshot_publisher, benchmark,
   placement, resource_sync.

3. **Wire the Ollama proxy on port 21434.** This port speaks native Ollama protocol:
   `GET /`, `/api/tags`, `/api/ps`, `/api/version`, `/api/show`, `/api/pull`,
   `/api/delete`, `/api/generate`, `/api/chat`, `/api/embed`. An Ollama client
   connecting to this port must see exactly the same behavior as connecting to
   the current Ollama orchestrator.

4. **Wire the extension API**: `/v1/models`, `/v1/stones`, `/v1/capabilities`,
   `/v1/recommendations`.

**Verification:** Run the Ollama orchestrator's `exercise.ps1` against the AI
orchestrator's proxy port. All tests must pass.

### Block 4: Multi-Offering Extension

**Goal:** Each offering type works end-to-end as its own bounded context.

Each adapter is a "microservice within the monolith" — self-contained, owning its
own client, proxy protocol, health model, and error handling. The pattern:

```
offerings/{service}/
├── mod.rs          — Offering trait implementation
├── client.rs       — HTTP client specific to this service's API
├── proxy.rs        — Protocol-specific proxy logic (if complex)
├── types.rs        — API response types (serde shapes)
└── benchmark.rs    — Service-specific benchmark payloads
```

For each service (ComfyUI, whisper.cpp, OpenedAI Speech, Infinity, LibreTranslate,
Speaches):

1. Read `src/moss/embedded/manifests/sw/ai/{service}.research.md` for the exact API.
2. Implement the adapter as a bounded context: its own client, types, proxy logic.
3. Wire its native proxy port (see port table above) — clients connect using the
   same protocol they'd use with the real service.
4. Verify against a running instance of that service:
   - Probe succeeds (health check via the service's native endpoint)
   - Enumerate returns real models/resources
   - A native client connecting to the proxy port can use the service normally
5. **Only move to the next service after the current one works end-to-end.**

Nothing outside `offerings/{service}/` should know about that service's API shapes,
authentication mechanism, or streaming format. The shared layer sees only the trait
interface.

### Block 5: Dashboard

**Goal:** Operators can monitor and manage the orchestrator through a web UI.

Build a React + TypeScript + Tailwind dashboard (Grafana-inspired, dark-first).
Study the Ollama dashboard (`src/orchestrators/ollama/assets/dashboard.html`,
2,616 lines) to understand what operators need, then design the AI orchestrator's
dashboard from its own requirements — capability-centric primary view, per-offering
detail, multi-stone VRAM visualization, cross-offering fitness matrix.

The dashboard must show real data from real instances. Not hardcoded, not stubbed.
Use a single SSE connection (`/api/events`) with the snapshot publisher emitting
full state every 3 seconds — no polling.

### Block 6: Cloud Providers

**Goal:** Cloud APIs as priority -10 fallbacks.

Study LiteLLM and OpenRouter patterns for provider abstraction. Key points:
- `provider/model` naming convention
- API keys via environment variables (never in config files)
- Anthropic uses `x-api-key` header and Messages API (not OpenAI-compatible) —
  needs a dedicated translation layer, not a generic OpenAI-compat adapter
- Cloud instances must be registered in the instance registry with `priority: -10`
  so the routing engine's priority gate (RT-4) handles them correctly

---

## Critical Rules

1. **Harvest, don't reinvent.** If the Ollama orchestrator already solves a problem,
   extract that solution into the shared layer and use it. Do not write a new version.

2. **Never guess a configuration value.** Read it from the existing orchestrators.
   If the Ollama orchestrator uses `host.docker.internal:5641` for Koi, that's what
   the AI orchestrator uses. If you don't know a value, find it in the code. If it's
   not in the code, ask.

3. **Never declare a phase complete based on compilation + unit tests.** Verify against
   a running system. "It compiles" is not a deliverable. "The container starts,
   connects to Koi, discovers instances, and routes a request" is a deliverable.

4. **Break the problem into blocks with clear interfaces.** Each block has a concrete
   operational verification criterion. Do not build all adapters in parallel before
   any single one works end-to-end.

5. **Never create bespoke code for a problem the existing codebase already solves.**
   Koi resolution, Moss gateway registration, Docker networking, SSE streaming,
   topology queries, stone discovery — all solved in `orchestrator-common` and
   proven by the Ollama and MongoDB orchestrators. Use them.

6. **The extraction must not break the Ollama orchestrator.** If you move domain
   logic to a shared location, the Ollama orchestrator must still work. This is
   the proof that the extraction is correct.

---

## Files to Read First

In this order:

1. `docs/decisions/ORCH-0013-ai-orchestrator-promotion.md` — especially the
   "Implementation Lessons" section at the end
2. `docs/code-standards.md` — the project's Rust code standards (20 rules).
   Naming conventions (§1–§3), domain purity (§6), channel conventions (§4),
   error handling (§17), shared resources (§19), and memory budgets (§20)
   are all enforced. Read before writing any Rust.
3. `docs/decisions/ORCH-0012-cluster-adapter-extraction.md` — pattern precedent
   for extracting shared primitives from a working orchestrator (methodology,
   not types — see note in "What Already Exists" above)
4. `src/orchestrators/common/src/` — all shared infrastructure modules (understand
   what's already shared before deciding what to extract)
5. `src/orchestrators/ollama/src/domain/` — every module, classifying each as
   shared vs Ollama-specific
6. `src/orchestrators/ollama/src/main.rs` — startup sequence and configuration
   (read every `#[arg]` default and env mapping)
7. `src/orchestrators/ollama/Dockerfile` — Docker configuration and networking
8. `src/orchestrators/ollama/src/tasks/` — all background tasks, identifying
   the offering-agnostic orchestration patterns
9. `src/orchestrators/ollama/src/api/` — API handlers and proxy pattern
10. `src/moss/embedded/manifests/sw/ai/*.research.md` — service API references

---

## Anti-patterns from the Previous Attempt

These are not theoretical — they caused the revert:

- **Writing `main.rs` by "studying" the Ollama orchestrator** without reading the
  actual default values. Result: wrong Koi endpoint, wrong port, no log output.
- **Building all 7 offering adapters in parallel** before any single one worked
  end-to-end. Result: the proxy rejected all multipart requests, health labels
  were wrong, cloud instances were never registered.
- **Using adversarial code review as the quality mechanism** instead of running the
  service. Result: 8 reviews found 29 issues, but the fundamental operational
  failures weren't caught because no one ran the container against a real garden.
- **Treating the dashboard as a checkbox** instead of a product. Result: two rewrites
  (vanilla HTML → React) with critical rendering bugs in both.
- **Assuming runtime behavior** instead of testing it. When the container couldn't
  reach Koi, the response was "this is expected" instead of checking how the
  existing orchestrators handle it.
