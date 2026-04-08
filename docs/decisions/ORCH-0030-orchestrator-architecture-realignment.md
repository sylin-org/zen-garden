---
audience: developer
doc_type: decision
status: proposed
---

# ORCH-0030: Orchestrator Architecture Realignment — Event Bus, Resources Domain, Provider-Level Directory

**Date**: 2026-04-08
**Status**: Proposed
**Deciders**: Leo
**Supersedes in part**:
- ORCH-0028 (orchestrator core) — the pipeline, `Provider` trait, and vocabulary survive unchanged. The `Directory` aggregate's unit of registration shifts from instance to provider; the `Skills` aggregate is reframed as an adapter-internal helper.
- ORCH-0029 (skill subsystem) — disk layout, loader, and import pipeline survive unchanged. The two-aggregate split (`Directory` + `Skills`) is retired in favor of a single `Directory` fed by multiple registration sources.

**Related ADRs**:
- ORCH-0011 (recommended model monikers) — elevated from convenience sugar to **default interpretation** of every unfilled selector.
- ORCH-0013 (AI orchestrator promotion).

---

## Context

The orchestrator today is a dispatcher with recommendations, surfaced through three action endpoints (`/v1/do`, `/v1/{modality}/{leaf}`, `/v1/{modality}/{leaf}/{skill}`), a catalog, a job store, a media store, and a per-domain patchwork of notification mechanisms (`catalog::watch::channel`, `jobs::broadcast` for terminal reaping, `garden_discovery::SSE` consumer, silent fire-and-forget `tokio::spawn` for skill naming).

This arrangement has six architectural scars that this ADR addresses as one coherent rewrite of the orchestrator's *coordination surface*. The request pipeline, provider contract, and vocabulary (ORCH-0028 core) are untouched.

### Scar 1 — Five half-buses pretending to be a nervous system

Catalog changes publish on `watch::channel`. Job terminals publish on `broadcast::Sender`. Garden discovery runs its own SSE consumer. Skill naming updates the aggregate silently with no notification at all. The HTTP surface exposes `/v1/catalog/events` for catalog changes only; everything else is either polled or invisible to clients.

Consumers that want to react to "anything interesting in the garden" have to subscribe to four different mechanisms and reconcile them manually.

### Scar 2 — The Directory/Skills split hard-codes a ComfyUI assumption

The `Directory` tracks static, config-driven registrations. The `Skills` aggregate tracks dynamic, skill-driven state. This split made sense when skills were a ComfyUI-only novelty, but every other adapter eventually wants the same thing: a way to contribute *specialized* registrations that aren't known at compile time. An `image.tag` skill served by Ollama via a crafted vision-chat prompt is architecturally identical to a ComfyUI workflow skill — both are (primitive, honored fields, adapter-private execution). The two-aggregate split forces consumers to join the two halves every time they want a coherent view, and embeds the assumption that "skill = ComfyUI" into the data model.

### Scar 3 — The Directory tracks instances, which is the wrong altitude

Each instance of each provider currently registers separately. "Ollama on stone-01 serves `text.chat`" and "Ollama on stone-02 serves `text.chat`" are two distinct Directory entries. This bleeds instance-level volatility (health flaps, pool expansions) into the Directory's schema-change events, and forces the dispatcher to make instance-level decisions it doesn't have the right information for.

The honest shape is: the `Directory` tracks **providers** (one registration per provider per action-shape), and each adapter owns its own pool of instances as an internal concern. The airport/airline metaphor: the traveler picks an airport (provider); the airline (adapter) picks the plane (instance). Dispatchers don't choose planes.

### Scar 4 — Shared hardware is invisible

A ComfyUI instance and an Ollama instance can coexist on the same stone, competing for the same GPU. Neither adapter can make correct load-balancing decisions without knowing what the other is using. There is no place in the current architecture to track "stone-01 GPU 0 has 6GB committed right now by comfyui, 2GB free."

The result: either adapters over-provision and leave hardware idle, or they oversubscribe and OOM. Neither is acceptable.

### Scar 5 — `recommended:*` is a feature, not a default

Today a caller writes `selectors.model = "recommended:chat"` to get the auto-pick behavior. If they omit the selector, behavior is undefined or vendor-dependent. The intended UX is "describe what you want and let the garden decide" — which means auto-pick is the *default*, not an opt-in.

This elevation also exposes a layering mistake: today the recommendation engine resolves `recommended:*` centrally at dispatch time. The adapter, which is the thing that actually knows which of its instances have which models warm, has no say. Resolution belongs inside the adapter.

### Scar 6 — Composition is a client-side problem

"Transcribe this recording and summarize it" is two primitives and a data flow between them. Today that's two HTTP calls plus client-side state management and string templating. Composition should be expressible as one request — not because workflows are a new noun, but because the universal verb (`/v1/do`) should accept either a single action or a set of them with inter-step references.

### Scar 7 — Preferences don't exist

A caller who prefers 1024×1024 images, `temperature=0.3`, and local-only routing has nowhere to say so. Every request repeats the same parameter soup. There is no mechanism for operator-level defaults to populate form fields in the catalog or pre-fill the dispatcher's selector resolution.

---

## Mandate

This ADR adds and restructures; **it does not wipe ORCH-0028 core**. The request pipeline, `Provider` trait, `Contextualizer`, `MediaResolver`, vocabulary, and primitive enum are preserved. What changes is the surrounding coordination surface: notification, registration granularity, resource accounting, selector defaulting, and composition.

Concepts preserved from ORCH-0028/0029 without change:
- `Primitive` enum and `Action` grammar
- `Provider` trait and `ProviderOutcome`
- `Contextualizer` (gains one new responsibility, below)
- `MediaResolver`, `MediaStore`, idempotency store, job store
- Vocabulary registry and field types
- Skill disk layout (`{data_dir}/skills/{provider}/{moniker}/`), loader, and import pipeline
- Vendor adapter client code for all twelve providers
- The request pipeline: contextualize → resolve media → dispatch → translate → execute

Concepts reshaped:
- `Directory` now tracks **providers, not instances**; registration churn at the instance level becomes an adapter concern.
- `Skills` aggregate collapses into the Directory via adapter-emitted registrations.
- Notification collapses into a single `EventBus` with topic-based fanout.
- `recommended:*` becomes the default interpretation of every unfilled selector field.
- `/v1/do` accepts either a single action or a flow (DAG of actions with inter-step references).

Concepts added:
- `EventBus` — the orchestrator's single nervous system, exposed at `GET /v1/events?focus=...`.
- `Resources` domain — physical stone resources (GPU VRAM, system memory) with claim-based accounting.
- `InstanceManager` — a shared component consumed by adapters that manage local instance pools.
- `Preferences` — a flat, global field-path-to-value map, loaded into the Contextualizer and catalog rendering.

---

## Guiding principle (carried forward from ORCH-0028)

> **User satisfaction is the ultimate value. Intent must be understood and executed upon with minimal blocks.**

This ADR restates the principle because it is the lens through which every decision below was made. Three concrete commitments specific to this realignment:

1. **The default behavior is to recommend, not to require.** A caller who sends the minimum payload gets a sensible dispatch, because every unfilled selector is resolved against the current state of the garden and the caller's preferences.
2. **Every state transition the orchestrator cares about is observable through one stream.** A client never polls for "did it finish yet?" — they subscribe once and watch the garden breathe.
3. **Physical reality is modeled honestly.** Shared GPUs produce shared accounting; unknowable sizes produce exclusive claims; the system never silently overcommits hardware it doesn't understand.

---

## Decision

### 1. Event bus — the single nervous system

A new domain type `EventBus` replaces every ad-hoc notification mechanism in the orchestrator. Exactly one stream, accessible at `GET /v1/events?focus=<globs>&since=<seq>`, carries every state transition that matters.

#### 1.1 Topic grammar

Every event carries a dotted topic that **mirrors the URL path of the resource it concerns**. This is a load-bearing invariant: the URL grammar and the topic grammar are the same thing, so a client that knows how to ask for a resource already knows how to subscribe to its events.

```
skills.{moniker}.state              — draft | analyzing | naming | ready | published | failed
skills.{moniker}.named
skills.{moniker}.models.progress
skills.{moniker}.published
skills.{moniker}.failed
skills.list.changed

jobs.{id}.state                     — accepted | contextualizing | dispatched | routed | running | streaming | completed | failed
jobs.{id}.progress
jobs.{id}.result                    — terminal payload, async jobs only
jobs.{id}.step.{step_id}.state      — flow (multi-step) jobs only
jobs.{id}.step.{step_id}.routed

dispatch.{id}.started
dispatch.{id}.routed                — which provider, which instance, which model
dispatch.{id}.completed             — provider, instance, latency, tokens, cost estimate

directory.version
directory.provider.{name}.health
directory.provider.{name}.registration.added
directory.provider.{name}.registration.removed

catalog.version

recommendations.{primitive}.changed

resources.stone.{name}.topology.changed
resources.stone.{name}.gpu.{idx}.pressure
resources.stone.{name}.memory.pressure
resources.stone.{name}.claim.granted
resources.stone.{name}.claim.released
resources.stone.{name}.claim.rejected
resources.stone.{name}.snapshot     — synthetic, on-subscribe only (see §1.5)

media.{id}.created
media.{id}.expired
media.{id}.evicted

provisioning.{job}.started
provisioning.{job}.progress
provisioning.{job}.completed
provisioning.{job}.failed

preferences.changed
```

**Publishers always write fully-resolved topics.** Wildcards are a subscriber-side concern only.

#### 1.2 Focus — server-side fanout via globs

Subscribers declare their interest as a comma-separated set of glob patterns via `?focus=...`. The server compiles one `GlobSet` per connection and runs every published event through every connection's matcher before writing to the SSE stream.

Examples:
```
?focus=skills.*                                 — all skill events
?focus=skills.flux-25592                        — one skill in detail
?focus=skills.*.named                           — every naming event across skills
?focus=jobs.*,dispatch.*                        — job lifecycle plus dispatch traces
?focus=*.failed                                 — every failure anywhere in the garden
?focus=resources.stone.stone-01.*               — one stone's resource state
```

The client's current view is its focus. When the user navigates from the skill list to a skill detail, the dashboard **reconnects** with a narrower focus. SSE reconnection is cheap; `Last-Event-ID` handles gap-free resume.

The choice of glob over prefix is deliberate: glob accommodates the "show me every failure" and "show me every naming event" cases without forcing clients to either subscribe to too much or open multiple connections.

#### 1.3 Sequence-based resume

The bus assigns a monotonic `seq` to every event. The SSE `id` field carries `seq` so `Last-Event-ID` on reconnect tells the server where to pick up. The bus maintains a bounded ring buffer of recent events; on reconnect, the server replays matching entries from `seq+1`, then tails live.

If the client's requested `seq` is older than the oldest history entry, the server emits a `resume.gap { requested, oldest }` event and the client re-reads authoritative state via REST before trusting the live tail. This mechanism makes laptop-lid-close and reconnect work without client-side cleverness.

#### 1.4 Transitions, not state — with one named exception

The bus carries **transitions**, not state snapshots. Late subscribers see the next change, not the current value. This is the clean architectural separation:

> **REST is the state contract; the bus is the transition contract.**

Clients `GET /v1/skills` for authoritative list state, then subscribe to `skills.*` for deltas. They `GET /v1/catalog` for the catalog, then subscribe to `catalog.version` for rebuild pings. They never try to reconstruct state by replaying events from the beginning of time.

**The single exception**: `resources.stone.{name}.snapshot`. When a subscriber's focus matches a stone's resources topic, the bus immediately emits a synthetic snapshot event carrying the current committed/available state for that stone. This exception exists because live resource gauges are the canonical dashboard use case, and forcing every dashboard to pair every subscribe with a matching REST call for the initial state is a real ergonomic tax. The snapshot suffix is part of the topic grammar; no other topic family gets this treatment.

#### 1.5 Internal consumers use the same bus

Background tasks inside the orchestrator subscribe to the same bus as external clients, with the same focus grammar. The terminal reaper becomes "subscribe to `jobs.*.state` where state is terminal, release reservations." The catalog builder becomes "subscribe to `directory.*.registration.*`, rebuild on change." Garden discovery remains a *source*, but its output is republished onto the bus as `directory.provider.*.registration.*`, so internal consumers don't know or care whether the trigger came from a remote stone.

This eliminates every ad-hoc `watch::channel` and domain-specific `broadcast::Sender` in the codebase. There is one bus.

#### 1.6 `/v1/catalog/events` is retired

Clients that today subscribe to `/v1/catalog/events` migrate to `GET /v1/events?focus=catalog.*,directory.*`. The retirement happens in the same commit that introduces `/v1/events` — no parallel operation, no compatibility shim.

### 2. Resources domain — shared hardware is first-class

A new domain `Resources` (at `src/domain/resources/`) owns physical stone resources — GPU devices, system memory, and any other contention-worthy hardware. Adapters place **claims** against resources when they dispatch work and release them when the work completes.

#### 2.1 Claim sides and the hybrid model

Two axes of "known / unknown" compose into a four-quadrant admission rule:

|                      | Device total known       | Device total unknown       |
|----------------------|--------------------------|----------------------------|
| **Sized claim**      | Shared accounting: `committed + requested + headroom ≤ total` | Degrades to exclusive |
| **Unsized claim**    | Exclusive hold          | Exclusive hold             |

**Sized claim on a known device** is the happy path. Multiple adapters can share a GPU honestly: ComfyUI claims 6GB for a flux inference while Ollama claims 2GB for an embedding, both land on stone-01 GPU 0, both run concurrently. This is the scenario that motivates the entire domain.

**Unsized claim** ("I want the GPU, I don't know how much") collapses to exclusive. Any unsized claim locks its device until released. Any attempt to place a new claim (sized or not) on a device with an active unsized claim is rejected with `DeviceExclusivelyHeld`.

**Sized claim on an unknown-total device** has nothing to account against, so it also degrades to exclusive — but the claim still carries its size for observability, and the event stream records it. When topology eventually reports the total, subsequent claims can compose normally.

**Invariant**: sized and unsized claims never coexist on the same device. A device in "shared" mode rejects unsized claims; a device in "exclusive" mode rejects sized claims. Mode is per-device and per-moment, not per-adapter.

#### 2.2 Adapters should prefer sized claims

An adapter that can *sometimes* estimate a workload's footprint should prefer conservative estimation over unsized claims. A conservative overestimate still participates in shared accounting; an unsized claim forces exclusivity and blocks other adapters unnecessarily.

The fallback pattern is:
```rust
let vram_mb = workflow.estimate_peak_vram()
    .unwrap_or(device.total_vram_mb);  // worst-case: claim the whole thing, but sized
```

Claiming `total_vram_mb` has the *effect* of exclusivity but remains participative: the event stream still shows "6GB claimed" (accurate within the estimate), the rejection reasons are still informative, and when a better estimate becomes available (a benchmark lands) the adapter can upgrade.

Pure unsized claims are the last resort for cases where even `total_vram_mb` isn't known (device total not reported by topology).

#### 2.3 Soft claims for queued work

Adapters that queue work place **soft claims** against resources to reserve capacity for work that hasn't started yet. A soft claim:
- Contributes to pressure calculations for scheduling decisions
- Does **not** block hard claims from landing
- Is promoted to a hard claim when the queued work actually starts
- Expires via TTL if never promoted

Without soft claims, N queued requests all look "free" to the next incoming request, then all start at once and overcommit. With soft claims, the pressure signal reflects committed-plus-reserved, and the Instance Manager correctly throttles at the right altitude.

Soft claims only exist on shared-mode devices. An unsized (exclusive) claim is inherently unschedulable-behind-others, so soft-claiming an exclusive device is nonsensical and rejected.

#### 2.4 Claim lifecycle and safety

Claims are returned as RAII guards (`ClaimGuard`) whose `Drop` impl calls `release`. Normal-path releases are automatic. Three additional safety mechanisms handle abnormal paths:

1. **Holder liveness watch** — the Resources domain subscribes to `directory.provider.{name}.health`. When a provider goes offline, all claims held by that provider are evicted.
2. **Optional TTL** — claims may carry `expires_at` for one-shot inferences; a background sweeper releases expired claims.
3. **Eviction API** — `resources.evict_holder(adapter, instance)` for operator-triggered cleanup.

#### 2.5 Domain contract

```rust
pub struct Resources {
    stones: RwLock<HashMap<StoneName, StoneResources>>,
    events: Arc<EventBus>,
}

pub struct StoneResources {
    pub name: StoneName,
    pub gpus: Vec<GpuDevice>,
    pub memory: MemoryResource,
    pub claims: HashMap<ClaimId, Claim>,
}

pub struct GpuDevice {
    pub index: u32,
    pub name: String,
    pub vendor: GpuVendor,            // NVIDIA | AMD | Intel | Apple | Unknown
    pub compute_stack: Vec<ComputeStack>,   // CUDA | ROCm | OneAPI | Metal | Vulkan
    pub total_vram_mb: Option<u64>,   // None → unknown; forces exclusive mode
    pub headroom_mb: u64,             // safety margin, default 512MB
    pub committed_mb: u64,            // derived: sum of active sized claims
    pub mode: DeviceMode,             // Shared | Exclusive | Opaque
}

pub enum GpuVendor { Nvidia, Amd, Intel, Apple, Unknown }
pub enum ComputeStack { Cuda, Rocm, OneApi, Metal, Vulkan, Cpu }

pub enum DeviceMode {
    Shared,      // sized claims compose
    Exclusive,   // one unsized claim holds the device
    Opaque,      // total unknown, always behaves exclusive
}

pub struct Claim {
    pub id: ClaimId,
    pub holder: ClaimHolder,          // { adapter, instance }
    pub request: ResourceRequest,
    pub kind: ClaimKind,              // Hard | Soft
    pub granted_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

pub enum ResourceRequest {
    Gpu {
        stone,
        device,
        vram_mb: Option<u64>,               // None = unsized
        required_stack: ComputeStack,        // e.g., Cuda for ComfyUI today
    },
    Memory { stone, mb: Option<u64> },
}

pub enum ClaimError {
    InsufficientVram { stone, device, requested: u64, available: u64 },
    InsufficientMemory { stone, requested: u64, available: u64 },
    DeviceExclusivelyHeld { stone, device, holder: ClaimHolder },
    UnsupportedComputeStack {
        stone,
        device,
        required: ComputeStack,
        available: Vec<ComputeStack>,
    },
    SizeMismatchMode { stone, device, current_mode: DeviceMode, requested_kind: &'static str },
    UnknownStone(StoneName),
    UnknownDevice { stone, device },
}

impl Resources {
    pub async fn snapshot(&self, stone: &StoneName) -> Option<StoneResources>;
    pub async fn pressure(&self, stone: &StoneName) -> Option<PressureSnapshot>;
    pub async fn claim(&self, holder: ClaimHolder, request: ResourceRequest, kind: ClaimKind)
        -> Result<ClaimGuard, ClaimError>;
    pub async fn promote_soft_to_hard(&self, claim_id: ClaimId) -> Result<(), ClaimError>;
    pub async fn update_topology(&self, stone: StoneName, topology: StoneTopology);
    pub async fn evict_holder(&self, adapter: &str, instance: Option<&str>);
}
```

Each `ClaimError` variant carries enough context for the caller's next decision without a round-trip: `InsufficientVram` carries `available`, so the Instance Manager knows immediately whether a smaller claim would fit; `DeviceExclusivelyHeld` tells it to try a different device rather than retrying with a smaller size.

#### 2.6 Events and their semantics

```
resources.stone.{name}.topology.changed     — new hardware discovered or updated
resources.stone.{name}.gpu.{idx}.pressure   — committed_mb changed (rate-limited, 500ms debounce)
resources.stone.{name}.memory.pressure      — same for system memory
resources.stone.{name}.claim.granted        — observability
resources.stone.{name}.claim.released
resources.stone.{name}.claim.rejected       — observability, drives recommendation learning
resources.stone.{name}.snapshot             — synthetic on-subscribe event (§1.4)
```

The `rejected` event is a **demand signal** the recommendation engine learns from. A stone that rejects claims frequently is overcommitted; the engine deprioritizes it in future rankings even when it *looks* available.

Pressure events carry a `mode` field so dashboards can render the three visual states correctly:

```json
{
  "topic": "resources.stone.stone-01.gpu.0.pressure",
  "mode": "shared",
  "total_mb": 24576,
  "committed_mb": 8192,
  "available_mb": 16384,
  "headroom_mb": 512,
  "hard_claims": 2,
  "soft_claims": 1
}
```

- `shared`: multi-claim accounting, render as utilization bar
- `exclusive`: one holder, render as "in use by {holder}"
- `opaque`: total unknown, render as "in use (unmeasured)"

#### 2.7 Topology ingestion

The Resources domain does not discover hardware on its own. Garden discovery (which already subscribes to Moss topology streams) publishes `directory.stone.{name}.topology.changed` events; the Resources domain subscribes and updates its totals accordingly. This is the one place where Resources is downstream of another domain.

#### 2.8 Compute-stack capability filtering

Not every adapter can run on every GPU. ComfyUI today requires a CUDA stack; Ollama runs on CUDA, ROCm, and Metal; Whisper runs on CPU, CUDA, and ROCm. A garden with mixed-vendor hardware — the reference test garden has one AMD and two NVIDIA GPUs — must refuse to dispatch an adapter's work to a device whose compute stack the adapter does not support.

The filter is expressed at claim time, not at selection time, so the Resources domain is the single authority:

- Each `GpuDevice` advertises its supported `ComputeStack`s (from topology).
- Each `ResourceRequest::Gpu` carries a `required_stack` from the adapter.
- The domain rejects any claim where `required_stack ∉ device.compute_stack` with `UnsupportedComputeStack`.

The Instance Manager's selection logic consults this before placing a claim: it pre-filters its candidate instances by `stone.gpus.any(|g| g.compute_stack.contains(required))` to avoid trying and failing. But the domain's check is the source of truth — adapters cannot bypass it by lying about their requirements.

**The reference test garden** (see §11) exercises this path specifically. ComfyUI instances on stones with only AMD GPUs must fail to claim; Ollama instances on the same stones must succeed. Dispatch of `image.generate` to ComfyUI on such a stone must cause the Instance Manager to pick a different stone or return `SelectError::NoHealthyInstances` with a trace explaining why.

### 3. Directory tracks providers, not instances

The `Directory` aggregate's unit of registration changes from `(provider, instance, primitive)` to `(provider, primitive, specialization)`. Instance multiplicity is invisible to the Directory.

#### 3.1 Registration shape

```rust
pub struct Registration {
    pub id: RegistrationId,
    pub provider: ProviderName,
    pub primitive: Primitive,
    pub specialization: Option<Specialization>,   // e.g., "tron" for image.generate.tron
    pub strategy: RegistrationStrategy,
    pub honored_fields: Vec<HonoredField>,
    pub media_inputs: Vec<MediaInputSpec>,
    pub media_outputs: Vec<MediaOutputSpec>,
    pub metadata: RegistrationMetadata,            // display name, description, tags
}
```

A provider that serves one primitive has one registration. A provider that serves multiple primitives has multiple registrations. A provider that serves specialized variants (skills) has one registration per variant. Instance count does not appear.

#### 3.2 The Directory is fed by multiple sources

Registrations flow into the Directory from three sources, all through the same `register` API:

1. **Provider startup** — each adapter registers its static (non-specialized) primitives when it comes up. `text.chat` from Ollama, `image.analyze` from Anthropic, `audio.transcribe` from Whisper, etc.
2. **Skill loading** — the ComfyUI adapter (and any future skill-capable adapter) loads skill definitions from disk and registers each as a specialized variant. `image.generate.tron` is registered by the ComfyUI adapter at skill-load time.
3. **Skill import** — a new skill imported via `POST /v1/skills/{provider}/import` causes the adapter to register the newly-loaded skill through the same API.

The Directory does not care which source a registration came from. Consumers see a flat set of registrations; the catalog joins them into primitive/action entries for the HTTP surface.

#### 3.3 The `Skills` aggregate is retired as a peer domain

The `Skills` aggregate is reframed as an **adapter-internal helper**. Adapters that need to track dynamic per-skill state (provisioning status, model readiness, naming progress) keep a local `SkillCatalog` or similar, but it is no longer a top-level aggregate held in `AppState` parallel to the Directory.

Consumers of the old `Skills` aggregate migrate as follows:
- **Skill inventory queries** → `Directory::registrations_by_source(Source::Skill)` or a `GET /v1/skills` endpoint that filters the Directory.
- **Skill state transitions** → `skills.{moniker}.*` events on the bus.
- **Skill rename** → the adapter updates its internal state and re-registers with updated metadata; the Directory emits `directory.provider.{name}.registration.updated`.

This reframing collapses two state stores into one and eliminates the "which aggregate owns skill metadata?" confusion.

#### 3.4 Skill discoverability is strict-named

A caller who sends `POST /v1/image/generate` with a Tron-ish prompt is **not** auto-routed to the `image.generate.tron` skill. Skills are opt-in specializations: the caller names them explicitly in the URL (`/v1/image/generate/tron`) or they are not invoked.

The recommendation engine ranks **providers**, not specializations-across-providers. Skills are discoverable through the catalog (a user browses and picks) and are a first-class routing target when named, but the generic primitive path never silently substitutes a skill for a raw provider call.

This preserves predictability: a caller who says "generate an image of butterflies" knows what kind of image they'll get. Prompt-aware specialization ranking is an entire research direction that is explicitly out of scope for this ADR.

### 4. Instance Manager — the adapter's mini-orchestrator

A new shared component `InstanceManager<I: InstanceRuntime>` lives at `src/services/instances/` and is consumed by adapters that manage pools of local instances (ComfyUI, Ollama, Kokoro, WhisperCpp, Infinity, Docling, Speaches, OpenedaiSpeech, LibreTranslate).

Cloud adapters (Anthropic, OpenAI, Google) do not use it — they have no local instances.

#### 4.1 Responsibilities

- **Instance registry** — the pool of live instances belonging to one adapter, keyed by `InstanceId`.
- **Health gating** — instances in poor health are skipped during selection.
- **Queue depth tracking** — per-instance in-flight bound via `Semaphore`.
- **Scheduling policy** — pluggable; default is "least-loaded with pressure penalty" (below).
- **Selection** — given a `ProviderRequest`, pick the best instance from the healthy set.
- **Resource claim coordination** — place a hard claim against the Resources domain before dispatching, hold it for the duration of the work, release on completion.
- **Topology ingestion** — consume garden discovery events, add/remove instances as stones come and go.

#### 4.2 Default scheduling policy

**Least-loaded with pressure penalty.** The Instance Manager ranks its healthy instances by:

```
score = queue_depth * QUEUE_WEIGHT + stone_pressure * PRESSURE_WEIGHT
```

where `stone_pressure` is read from `Resources::pressure(stone)`. The instance with the lowest score wins. Ties are broken by stone name (deterministic).

This is the honest baseline. It covers the shared-GPU scenario correctly: two empty queues, one stone under resource pressure from another adapter, the Instance Manager picks the stone with more headroom. Priority, deadline, and affinity scheduling are future knobs on the same `SchedulingPolicy` trait.

#### 4.3 Selection contract

```rust
pub trait InstanceRuntime: Send + Sync + 'static {
    async fn dispatch(&self, request: ProviderRequest) -> Result<ProviderOutcome, AdapterError>;
    fn estimate_vram_mb(&self, request: &ProviderRequest) -> Option<u64>;
}

pub struct InstanceManager<I: InstanceRuntime> { ... }

impl<I: InstanceRuntime> InstanceManager<I> {
    pub async fn select(&self, request: &ProviderRequest) -> Result<Selection<I>, SelectError>;
    pub async fn dispatch(&self, selection: Selection<I>, request: ProviderRequest) -> ProviderOutcome;
}

pub enum SelectError {
    NoHealthyInstances,
    AllInstancesSaturated,
    ResourceClaimFailed(ClaimError),
}
```

An adapter's `Provider::onboard` implementation typically looks like:

```rust
async fn onboard(&self, request: OrchestratorRequest) -> Result<ProviderOutcome, ProviderError> {
    let provider_request = self.translate(request)?;
    let selection = self.instances.select(&provider_request).await?;
    Ok(self.instances.dispatch(selection, provider_request).await)
}
```

The adapter no longer writes its own pool management. It inherits correct behavior from the shared component.

#### 4.4 Migration

The existing per-adapter pool code (ComfyUI's `InstancePool`, Ollama's `LoadBalancer`, etc.) is deleted and replaced with the shared `InstanceManager`. The adapter-specific logic that survives is exclusively wire-format translation and `InstanceRuntime` implementation (HTTP calls, auth headers, response parsing).

### 5. Dispatcher picks providers; adapters pick instances and models

This is the architectural line that follows from §3 and §4.

**Dispatcher responsibilities** (unchanged from ORCH-0028 except at the instance-level altitude):
- Contextualize the request (vocabulary resolution, media resolution)
- **Pick the provider** via Directory lookup + recommendation ranking + preference filtering
- Hand the `ProviderRequest` to the selected provider's `onboard` method
- Emit `dispatch.{id}.*` lifecycle events
- Handle provider-level failures and fall back to the next-best provider

**Adapter responsibilities**:
- Translate the canonical `ProviderRequest` to its wire format
- Resolve `recommended:*` selectors locally against its own state (§6)
- Pick the instance via `InstanceManager::select` (§4)
- Place resource claims via the Resources domain (§2)
- Dispatch to the chosen instance
- Emit provider-internal events as needed
- Return a typed error on failure so the dispatcher can decide whether to retry elsewhere

This is the airport/airline split made concrete: the dispatcher is the travel agent, the adapter is the airline. A caller who says "I want to fly to Paris" gets routed to "Air France" by the travel agent; Air France picks which of its planes and which fare class to assign. The traveler never chose a plane.

### 6. `recommended:*` is the default for every unfilled selector

This is an inversion of the current semantics. Today `recommended:*` is opt-in sugar; from this ADR forward, **it is the default interpretation of any closed-set selector field the caller did not explicitly fill**.

#### 6.1 The Contextualizer materializes defaults

The Contextualizer (already responsible for vocabulary resolution) gains one new responsibility: **selector defaulting**. For every closed-set selector field defined in the primitive's registration, if the caller's payload does not include an explicit value, the Contextualizer injects `recommended:{capability}` before the request reaches the dispatcher.

The dispatcher sees only fully-resolved selector fields. It does not know about defaulting — that's a pre-dispatch pipeline concern.

#### 6.2 Adapters resolve `recommended:*` locally

The recommendation engine today resolves `recommended:chat` → a concrete model centrally at dispatch time. Under this ADR, resolution moves **inside the adapter**:

- The dispatcher picks the provider (`Ollama`) and hands off a request with `selectors.model = "recommended:chat"` still in the payload.
- The Ollama adapter resolves `recommended:chat` against its own state: which of its instances have chat-capable models currently warm, which are under pressure, which match the user's preferences, which have the best benchmark scores for `text.chat`.
- The adapter returns a decision: `{ instance: stone-02, model: llama3:8b }`.

The adapter has information the central engine doesn't have: instance-local model warmth, instance-local queue depths, instance-local pressure. Central ranking at the model level is the wrong altitude; adapter-local ranking is the right one.

**The recommendation engine becomes a provider-level advisor.** It answers "which provider is best for `text.chat` given the user's preferences?" and exposes a `RecommendationContext` that adapters consume when making their own instance+model decisions. Adapters pull facts from the context (benchmark scores, demand history, user preferences) and reason locally.

#### 6.3 The catalog advertises which fields are auto-defaulted

Catalog field entries gain an `auto` descriptor:

```json
{
  "field": "selectors.model",
  "type": "string",
  "auto": {
    "default": "recommended:chat",
    "description": "The garden picks a chat-capable model based on your preferences and current conditions."
  },
  "pinnable": true,
  "pin_values": ["llama3:8b", "gemma2:9b", "claude-sonnet-4", "..."]
}
```

The dashboard renders `auto.description` as helper text under the field, shows `recommended:chat` as a ghost placeholder, and surfaces `pin_values` as a dropdown if the user wants to override. Clients that omit the field get the auto-default; clients that send `null` also get the auto-default; clients that send a concrete value get that value.

#### 6.4 Error responses carry resolution traces

When `recommended:*` resolution fails, the error response explains what the adapter tried:

```json
{
  "error": {
    "code": "no_suitable_model",
    "class": "model_unavailable",
    "message": "Ollama adapter could not satisfy 'recommended:chat' — no chat model currently loaded on any instance.",
    "resolution_trace": {
      "provider": "ollama",
      "selector": "recommended:chat",
      "candidates_considered": ["llama3:8b", "gemma2:9b"],
      "reason": "both models evicted due to pressure on stone-01"
    }
  }
}
```

Successful dispatches leave the resolution in the `dispatch.{id}.routed` event on the bus. Every decision is observable.

### 7. `/v1/do` accepts flows as well as single actions

The universal verb grows a DAG form. The body is either a single-action shape (today's) or a multi-action shape with inter-step references:

**Single action** (unchanged):
```json
{
  "action": "image.generate",
  "payload": { "image.prompt.positive": "butterflies" }
}
```

**Flow**:
```json
{
  "actions": [
    {
      "id": "transcribe",
      "action": "audio.transcribe",
      "payload": { "audio.source": "@upload:abc123" }
    },
    {
      "id": "summarize",
      "action": "text.chat",
      "payload": {
        "text.prompt.user": "Summarize this meeting transcript:\n{{transcribe.text.response}}"
      }
    }
  ]
}
```

#### 7.1 Parsing and classification

The `/v1/do` handler inspects the body and classifies:
- Presence of `action` + `payload` → single-action dispatch, current code path.
- Presence of `actions: [...]` → flow, new code path.

Flows are never mixed with single-action fields in the same body; the handler rejects ambiguous shapes with `400`.

#### 7.2 Inter-step references

Placeholders of the form `{{step_id.field.path}}` are resolved at step execution time, not at parse time. The engine walks the DAG in topological order; each step's payload is rendered by substituting completed upstream results into placeholders. A placeholder referencing an un-completed step is a parse error; a placeholder referencing a field that doesn't exist in the upstream output is a runtime error with `class: flow_reference_error`.

#### 7.3 Job identity

A flow is one job. `POST /v1/do` with a flow returns one `job_id`. The bus publishes:
- `jobs.{id}.state` for the flow as a whole (accepted → running → completed)
- `jobs.{id}.step.{step_id}.state` for each step's lifecycle
- `jobs.{id}.step.{step_id}.routed` for each step's provider/instance
- `jobs.{id}.result` for the terminal payload of the whole flow

Dashboards subscribed to `jobs.{id}.*` see both altitudes in one stream. Single-action jobs simply don't publish any `step.*` events.

#### 7.4 Failure semantics

A flow fails the first time any step fails terminally. The failure event is `jobs.{id}.step.{step_id}.failed` followed by `jobs.{id}.failed`. Partial results from completed upstream steps are preserved in the job record and retrievable via `GET /v1/jobs/{id}/result`.

Retry, resume, and partial-flow re-execution are out of scope for this ADR. They are future capabilities that this architecture accommodates but does not implement.

#### 7.5 The REST sugar shapes only accept single actions

`POST /v1/{modality}/{leaf}` and `POST /v1/{modality}/{leaf}/{skill}` are single-action only. Flows go through `/v1/do`. This preserves the sugar shapes as "the common case" and the universal verb as "the composition point."

### 8. Preferences as globals

A new domain `Preferences` (at `src/domain/preferences/`) holds a flat map of dotted field paths to values. Preferences are **global to the orchestrator instance**, not per-caller, because this orchestrator has no identity layer.

#### 8.1 Shape

```json
{
  "image.width": 1024,
  "image.height": 1024,
  "image.sampling.guidance": 7.0,
  "text.sampling.temperature": 0.7,
  "selectors.locality": "local",
  "selectors.cost_class": "free_preferred"
}
```

Flat, dotted, typed by the vocabulary registry. No nesting.

#### 8.2 Two layering points

Preferences are consulted at two well-defined points in the pipeline:

1. **Catalog rendering** — when `GET /v1/catalog` renders a primitive's field list, preferences are layered *over* the field's static default. Clients that render forms from the catalog see the operator's preferred defaults pre-filled.

2. **Dispatcher contextualization** — when a request reaches the Contextualizer, preferences are layered *under* the caller's explicit payload. Fields the caller sent explicitly always win; fields the caller omitted are filled from preferences before `recommended:*` defaulting kicks in for selectors.

The layering order is:
```
caller payload  >  preferences  >  field static default  >  recommended:* (selectors only)
```

#### 8.3 Endpoint

```
GET  /v1/preferences              → the full flat map
PUT  /v1/preferences               → merge semantics (partial update)
DELETE /v1/preferences/{path}     → remove a specific key
```

Changes publish `preferences.changed` on the bus, which the catalog builder consumes to rebuild and republish `catalog.version`.

#### 8.4 Why not per-caller

This orchestrator has no identity layer. Adding per-caller preferences would require designing authentication, identity storage, and scoping semantics first. This ADR sidesteps that entirely by making preferences a property of the orchestrator instance — one operator, one garden, one set of defaults. The day identity lands, preferences become per-identity and the layering rules stay the same.

### 9. Lineage: trace IDs on every response

Every dispatch produces a trace. The HTTP response carries `X-Zen-Trace-Id` in its headers. The bus publishes `dispatch.{id}.completed` with `{ provider, instance, model, latency_ms, tokens_in, tokens_out, cost_estimate, claimed_resources }`.

Callers can correlate the header to the bus event for post-hoc analysis of any request. This is the "identity and cost" observability demand, landed without a billing system: it's pure lineage, not accounting. A later ADR can grow this into per-identity cost attribution when identity exists.

---

## Consequences

### Positive

- **One nervous system.** Every state transition in the orchestrator is observable through `/v1/events` with a single subscription and a glob-based focus grammar. Dashboards, CLIs, and Koan consumers share one contract.
- **Shared hardware is honest.** Two adapters on the same stone compose correctly when sizes are known; degrade to exclusive when they aren't; never silently overcommit.
- **The default experience is intent-level.** A caller who sends the minimum payload gets a sensible dispatch because every selector defaults to `recommended:*` and preferences pre-fill the rest.
- **Composition is one request.** Transcribe-and-summarize is one `POST /v1/do` with two actions. No new noun, no new URL, no new concept to learn.
- **Layering is clean.** Dispatcher picks providers, adapters pick instances, resources track hardware, the Directory is a pure schema catalog. Every question has exactly one owner.
- **Skill integration stops being special.** Skills become one of three registration sources; consumers see a unified Directory. The ComfyUI assumption is erased from the data model.
- **Topic grammar = URL grammar.** A client that knows how to ask for a resource knows how to subscribe to its events. The same invariant enables future cross-orchestrator federation via topic namespacing.

### Negative

- **Large surface area to migrate.** Ten commits, each touching multiple files. The migration is staged so every commit leaves the system runnable, but the total diff is substantial.
- **Directory-at-provider-granularity changes benchmark ownership.** Today benchmarks live in the recommendation engine's per-instance view. Under this ADR they move into the Instance Manager's per-adapter view. Dashboards that query benchmark data migrate to a new endpoint shape.
- **Resources domain is new code with correctness obligations.** Claim accounting bugs can manifest as under-utilization (safe but wasteful) or OOM (unsafe). The hybrid sized/unsized model reduces the blast radius but does not eliminate it.
- **`/v1/do` flow parser is new complexity.** DAG validation, placeholder resolution, step-level event emission, and partial-failure semantics are all new code paths. Single-action requests are unaffected; flow requests carry the complexity.
- **Bus snapshot exception is an asymmetry.** The "transitions only, except resources" rule is a named exception, which is a small architectural debt. The justification is ergonomic (dashboard gauges need initial state); a later ADR could generalize or remove it.
- **`recommended:*` as default inverts client expectations.** Callers who previously omitted selectors because they didn't know about them will suddenly get auto-picked behavior. The catalog's `auto` descriptor documents this, but existing clients may need updating.

### Neutral

- **Skill disk layout is unchanged.** ORCH-0025's three-tier persistence, ORCH-0029's loader and import pipeline, and the v3 `skill.json` schema all survive. The change is where loaded skills *go* (Directory, not Skills aggregate).
- **Provider trait is unchanged.** `Provider::onboard` keeps its signature. Adapters gain new internal collaborators (Instance Manager, Resources domain) but the trait boundary is stable.
- **Vocabulary is unchanged.** Field types, key constants, and vocabulary registration survive as-is.

---

## Implementation plan

Ten commits, sequenced so each one leaves the system runnable and independently testable.

1. **Event bus skeleton + `/v1/events`.** `EventBus` type, ring buffer, glob-based subscription, SSE handler honoring `?focus`/`?since`/`Last-Event-ID`. Migrate `catalog.version` and `directory.provider.{name}.health` onto the bus. Retire `/v1/catalog/events`. *Proves the spine.*

2. **Skill noun surface.** `GET /v1/skills`, `GET /v1/skills/{moniker}`, `DELETE /v1/skills/{moniker}`. Read-only consumers of the existing Skills aggregate. Sitemap registration. *Unblocks a skill list view.*

3. **Skill events on the bus.** `post_import` and the skill lifecycle publish `skills.{moniker}.*`. Import response shrinks to `202 + Location + { moniker, topic }`. `AnalyzeResult` lives on `GET /v1/skills/{moniker}`. *First delight arc lands.*

4. **Resources domain.** Full implementation from day one: claim accounting, RAII guards, hybrid sized/unsized model, device mode tracking, soft claims, topology ingestion, rate-limited pressure events, snapshot-on-subscribe. *Shared hardware is first-class.*

5. **Instance Manager shared component.** Extract ComfyUI and Ollama pool code into `InstanceManager<I>`. Default scheduling policy. Adapters consume the shared component. Behavior unchanged at this step (pressure penalty is zero because Resources has no real claims yet from the adapters). *Structural dedupe.*

6. **Contextualizer defaults `recommended:*` for all unfilled selectors.** Catalog gains the `auto` descriptor for every closed-set selector field. Dispatcher sees only resolved selectors. *Defaults become the default.*

7. **Adapters resolve `recommended:*` locally.** Resolution moves from central recommendation engine into each adapter, consuming a new `RecommendationContext`. Instance Managers actually claim Resources at dispatch time; the pressure penalty starts influencing selection. The shared-GPU scenario behaves correctly end-to-end. *Architectural line honored.*

8. **Directory tracks providers, not instances.** Registration unit becomes `(provider, primitive, specialization)`. The `Skills` aggregate collapses into the Directory via adapter-emitted registrations. Benchmark data moves from the recommendation engine into the Instance Manager. Catalog rendering updated. *The biggest refactor; enabled by the preceding commits.*

9. **`/v1/do` accepts flows.** DAG parser, placeholder resolver, step-level event emission, partial-failure semantics. Single-action path unchanged. *Composition headline.*

10. **Preferences as globals.** `Preferences` domain, `GET/PUT /v1/preferences`, layering at catalog render and dispatcher contextualization. `preferences.changed` events. *Form-field autopopulate.*

Each commit has two acceptance criteria:
- The system runs end-to-end after the commit (verified by the existing integration smoke tests).
- The new capability is demonstrable with a single curl or `rake` invocation (documented in the commit message).

---

## Out of scope

Explicitly excluded from this ADR, to be addressed in future work:

- **Per-caller identity and authentication.** Preferences are global; there is no caller identity layer.
- **Billing / cost attribution.** Dispatch events carry cost estimates; no aggregation, no quotas, no budget enforcement.
- **Prompt-aware specialization ranking.** Skills are strict-named opt-in specializations; the recommendation engine does not auto-substitute skills based on prompt content.
- **Cross-orchestrator federation.** Topic grammar is designed to be federation-friendly (topics mirror URL paths) but no federation mechanism is built.
- **Priority, deadline, and affinity scheduling.** The Instance Manager's default policy is least-loaded-with-pressure-penalty; priority/deadline/affinity are future knobs on the same trait.
- **Flow retry, resume, and partial-flow re-execution.** A failed flow fails terminally; upstream results are preserved but not re-used.
- **Generalization of `recommended:*` to non-model selectors.** `recommended:sampler`, `recommended:resolution-for-my-hw`, and similar are architecturally accommodated but not implemented.
- **Persistent event history.** The bus's ring buffer is in-memory and bounded; there is no durable event log.

---

## Open questions resolved during drafting

The following design questions were raised during the specialist-team discussion that produced this ADR. They are recorded here so the reasoning is not lost.

**Q: Prefix-only or glob focus patterns?** → Glob. `skills.*.named`, `*.failed`, and `resources.stone.stone-01.*` are real use cases that prefix-only cannot express without over-subscribing or multi-connecting.

**Q: Does the bus carry sync dispatch results?** → No. Sync results stay on the HTTP response. The bus carries transitions and async terminal results only. `dispatch.{id}.*` lifecycle events fire for both sync and async, but the result payload only lives on the bus for async jobs.

**Q: Where does the event bus live?** → Its own domain (`src/domain/events/`), constructed at startup and held in `AppState`. Built and proven in its own commit before any skill-specific work.

**Q: Retire `/v1/do` as a public endpoint?** → No. `/v1/do` is the universal verb; REST sugars are URL-level conveniences pre-filling the action. Retiring `/v1/do` would eliminate the composition point.

**Q: Are presets/invocations/flows new top-level nouns?** → No. They are internal vocabulary. The HTTP surface exposes six meaningful verbs (run an action, list skills, import a skill, browse the catalog, watch events, manage media, manage preferences). Primitives, invocations, and presets are implementation details the caller never names.

**Q: Is the Stone Resource Broker a separate concept from the Resources domain?** → No. There is one Resources domain. "Broker" was an earlier framing; DDD names the domain by its invariants, not its mechanism.

**Q: Should `/v1/do` with a flow produce one job ID or many?** → One parent job ID with nested step IDs in topic names (`jobs.{id}.step.{step_id}.*`). Subscribers can focus at either altitude.

**Q: Skills aggregate refactor — aggressive or staged?** → Staged. Commit 8 collapses `Skills` into the Directory via adapter-emitted registrations; commits 2–3 retain the existing aggregate as a read-only source during the transition. After commit 8, the aggregate is removed from `AppState`.

**Q: When does the Resources domain become real?** → Immediately, in commit 4. There is no no-op version; the hybrid sized/unsized model is simple enough to implement correctly from the start, and the claim-based accounting invariant is too important to stub.

**Q: Soft claims — in scope or follow-up?** → In scope (commit 4). The "N queued requests all look free" failure mode is a real bug, and soft claims are the right fix.

**Q: Catalog `auto` descriptor — which commit?** → Commit 6, alongside the Contextualizer defaulting pass. The catalog and the contextualizer both need to understand `auto` at the same moment.

**Q: Skill discoverability — strict naming or prompt-aware?** → Strict naming. Callers invoke skills by URL (`/v1/image/generate/tron`); the generic primitive path never silently substitutes a skill. Prompt-aware ranking is explicitly out of scope.

---

## Test suite — the reference garden

The ADR is accompanied by a full end-to-end test suite that exercises the reference local test garden. The suite is the **acceptance gate** for every commit in the implementation plan: no commit lands without the tests for its phase passing green, and every commit adds its own tests to the suite.

### 11.1 The reference garden

The test garden models a realistic heterogeneous setup:

```
stone-cuda-01       1× NVIDIA RTX 4090  (24 GB, CUDA)
stone-cuda-02       1× NVIDIA RTX 3080  (10 GB, CUDA)
stone-rocm-01       1× AMD Radeon VII   (16 GB, ROCm)
```

Three stones, two GPU vendors, three GPUs. Each stone runs Moss and is connected to the garden via the existing discovery path. The AI orchestrator tends one of them (doesn't matter which; tended stone selection is orthogonal).

**Provider coverage per stone**:

| Provider      | stone-cuda-01 | stone-cuda-02 | stone-rocm-01 |
|---------------|:-------------:|:-------------:|:-------------:|
| Ollama        | ✓             | ✓             | ✓             |
| ComfyUI       | ✓             | ✓             | ✗ (no CUDA)   |
| Whisper       | ✓             | ✓             | ✓             |
| Infinity      | ✓             | ✓             | ✓             |
| LibreTranslate| ✓             | ✓             | ✓             |
| Kokoro        | ✓             | ✓             | ✓             |

The `✗` on ComfyUI for `stone-rocm-01` is the load-bearing asymmetry. ComfyUI cannot be deployed to the AMD stone (ComfyUI's community workflow ecosystem is effectively CUDA-only today), which means:

- ComfyUI has **two** instances across the garden, not three.
- Any test that asks ComfyUI to serve a request must route to `stone-cuda-01` or `stone-cuda-02`.
- A test that saturates both CUDA stones with ComfyUI work must verify that additional requests queue or get rejected — they do **not** spill onto `stone-rocm-01`.
- Ollama, by contrast, uses all three stones and can absorb spillover when ComfyUI is saturated.

### 11.2 Suite organization

The suite lives at `src/orchestrators/ai/tests/garden/` and is organized by the ADR's phases:

```
tests/garden/
├── common/
│   ├── fixture.rs              — wires AppState against the real garden
│   ├── stones.rs               — declarative garden topology for assertions
│   ├── sse_client.rs           — SSE subscriber with focus + since semantics
│   ├── assertions.rs           — topic matchers, event ordering assertions
│   └── workloads.rs            — canned prompts, canned PNGs, canned audio
│
├── phase01_event_bus.rs        — §1
├── phase02_skill_nouns.rs      — §3 inventory surface
├── phase03_skill_events.rs     — §3 lifecycle events
├── phase04_resources.rs        — §2 domain, including ROCm/CUDA filtering
├── phase05_instance_manager.rs — §4
├── phase06_recommended_default.rs  — §6
├── phase07_adapter_resolution.rs   — §6 adapter-local resolution
├── phase08_provider_directory.rs   — §3
├── phase09_do_flows.rs         — §7
├── phase10_preferences.rs      — §8
│
└── garden_integration.rs       — cross-phase scenarios (§11.5)
```

Each `phaseNN_*.rs` file is an independent `#[tokio::test]` binary that:
1. Brings the orchestrator up against the reference garden (or declines to run if the garden is unreachable).
2. Asserts that the capabilities introduced by its phase behave correctly end-to-end.
3. Cleans up its own claims, jobs, and imported skills before exiting.

### 11.3 Garden availability gate

The suite guards against running in a CI environment that doesn't have the real garden. A `GardenHandle::probe()` helper checks that the three reference stones are reachable and advertising the expected hardware; if any check fails, tests `#[ignore]` themselves with a reason rather than crashing.

```rust
let garden = match GardenHandle::probe().await {
    Ok(g) => g,
    Err(e) => {
        eprintln!("skipping: reference garden unreachable: {e}");
        return;
    }
};
```

This means the suite runs on a developer's local machine against the real garden and is opted-out-of on generic CI. A separate "mock garden" mode (§11.7) provides coverage for the logic that doesn't require real hardware.

### 11.4 Per-phase test outlines

The following are acceptance criteria, not implementation sketches. Each bullet is one `#[tokio::test]` function.

#### Phase 1 — Event bus and `/v1/events`

- **bus_emits_seq_monotonically** — publish 100 events, subscribe fresh, verify `seq` is strictly monotonic.
- **focus_prefix_matches** — subscribe to `skills.*`, publish events across multiple topics, assert only `skills.*` events are received.
- **focus_glob_matches** — subscribe to `*.failed`, publish events including `jobs.abc.failed`, `skills.xyz.failed`, `dispatch.def.completed`; assert exactly the first two are received.
- **focus_multi_pattern** — subscribe to `skills.*,jobs.*`, assert union is received.
- **last_event_id_resume** — subscribe, receive events 1–10, reconnect with `Last-Event-ID: 5`, assert events 6–10 are replayed before live tail.
- **resume_gap_when_history_exceeded** — advance the bus beyond its ring capacity, reconnect with a stale `Last-Event-ID`, assert a `resume.gap` synthetic event is emitted.
- **catalog_version_migrated** — trigger a catalog rebuild, assert `catalog.version` publishes on the unified bus, assert the retired `/v1/catalog/events` returns 404.
- **keepalive_during_idle** — connect, wait longer than keepalive interval, assert the connection stays open and receives keepalive comments.

#### Phase 2 — Skill inventory surface

- **list_empty_garden** — fresh garden, `GET /v1/skills` returns `[]`.
- **list_after_static_skills** — with two skill files on disk under ComfyUI, `GET /v1/skills` returns both with `provider: comfyui`.
- **get_skill_returns_analyze_result** — `GET /v1/skills/{moniker}` includes the full field set (bindings, models, variants).
- **get_unknown_skill_404** — `GET /v1/skills/nonexistent` returns 404 with an error envelope.
- **delete_skill_removes_from_disk** — `DELETE /v1/skills/{moniker}` removes the directory and the subsequent `GET` returns 404.
- **sitemap_lists_skills** — `GET /v1/` includes `"skills": "/v1/skills"`.

#### Phase 3 — Skill lifecycle events

- **import_202_and_location_header** — `POST /v1/skills/comfyui/import` returns 202 with `Location: /v1/skills/{moniker}` and a minimal body.
- **analyzing_event_on_accept** — subscribe to `skills.*.state`, POST import, assert `skills.{moniker}.state` with `state: "analyzing"` is received before the POST returns.
- **naming_event_fires** — subscribe to `skills.*.named`, POST import, wait up to 60s, assert a `skills.{moniker}.named` event with non-empty `display_name` fires.
- **models_progress_during_provisioning** — subscribe to `skills.*.models.progress`, POST import of a skill with uncached models, assert at least one progress event with monotonically increasing `ready` count.
- **ready_state_when_models_complete** — end of the previous test, assert the terminal `skills.{moniker}.state = ready` event.
- **failed_state_on_bad_input** — POST garbage to import, assert `skills.{moniker}.state = failed` with a `reason` in the payload.
- **idempotent_import_same_moniker** — POST the same CivitAI URL twice, assert the second POST returns the same moniker without writing a second draft directory.

#### Phase 4 — Resources domain (the garden's load-bearing phase)

- **topology_ingested_on_startup** — after boot, `GET /v1/resources/stones/stone-cuda-01/snapshot` shows one NVIDIA GPU with 24 GB total.
- **topology_ingested_for_amd** — same for `stone-rocm-01`, asserts `vendor: amd`, `compute_stack: [rocm]`.
- **claim_sized_cuda_succeeds** — place a 6 GB claim on stone-cuda-01 GPU 0 with `required_stack: cuda`, assert granted.
- **claim_cuda_on_amd_rejected** — place a CUDA claim on stone-rocm-01 GPU 0, assert `UnsupportedComputeStack { required: cuda, available: [rocm] }`.
- **claim_rocm_on_amd_succeeds** — place a ROCm claim on stone-rocm-01 GPU 0, assert granted.
- **two_sized_claims_compose** — place a 6 GB claim then a 2 GB claim on the same 24 GB GPU, assert both granted and pressure event shows `committed_mb: 8192`.
- **oversubscribe_rejected_with_available** — with 6+2 GB claimed, attempt 20 GB claim, assert `InsufficientVram { available: ~16000 }`.
- **unsized_claim_forces_exclusive** — place an unsized claim on a free GPU, then attempt a sized claim, assert `DeviceExclusivelyHeld`.
- **sized_rejects_unsized_on_busy_device** — with a sized claim active, attempt an unsized claim, assert `DeviceExclusivelyHeld`.
- **claim_guard_drop_releases** — place a claim in a scope, drop the scope, assert the released event fires and a fresh claim for the same capacity succeeds.
- **evict_holder_clears_claims** — place claims under holder `{comfyui, instance: stone-cuda-01}`, call `evict_holder("comfyui", Some("stone-cuda-01"))`, assert all cleared.
- **provider_offline_triggers_eviction** — mark ComfyUI offline via its health stream, assert all ComfyUI claims are evicted within one event bus tick.
- **soft_claim_does_not_block_hard** — place a soft claim for 6 GB, then a hard claim for 6 GB on the same device, assert both granted.
- **soft_claim_promotes** — place a soft claim, promote it, assert the pressure event transitions from `soft_claims: 1, hard_claims: 0` to `soft_claims: 0, hard_claims: 1`.
- **soft_claim_expires** — place a soft claim with 1s TTL, wait 2s, assert it is automatically released.
- **pressure_events_rate_limited** — place and release 20 claims in under 500ms, assert no more than 2 pressure events are emitted for that device.
- **snapshot_on_subscribe** — connect with `?focus=resources.stone.stone-cuda-01.*`, assert a `resources.stone.stone-cuda-01.snapshot` event is received immediately with current state.

#### Phase 5 — Instance Manager

- **selection_prefers_empty_queue** — two instances, one with 3 in-flight, one empty; assert the empty one is picked.
- **selection_tiebreaks_by_pressure** — two instances with empty queues, one on a stone with active claims by another adapter, assert the less-pressured stone is picked.
- **selection_skips_unhealthy** — mark one instance unhealthy via its health watch, assert it is excluded from selection.
- **selection_respects_compute_stack** — ComfyUI instance manager selection excludes `stone-rocm-01` by construction; assert for a ComfyUI request that the AMD stone is never considered.
- **saturated_returns_no_healthy** — fill every instance's semaphore, assert the next `select()` returns `AllInstancesSaturated` rather than blocking forever.
- **topology_change_updates_pool** — publish a topology change adding a new stone, assert the Instance Manager incorporates it within one bus tick.

#### Phase 6 — Recommended-by-default

- **unfilled_model_selector_injected** — POST `/v1/text/chat` without `selectors.model`, assert the resulting `dispatch.{id}.routed` event shows a concrete model was chosen.
- **explicit_pin_honored** — POST with `selectors.model: "llama3:8b"`, assert the routed event shows exactly that model.
- **catalog_auto_descriptor_present** — `GET /v1/catalog`, assert every closed-set selector field has an `auto` descriptor with `default: "recommended:*"`.
- **preferences_layered_between_auto_and_explicit** — set a preference, POST without the field, assert the preference wins over auto default; POST with an explicit value, assert explicit wins over preference.

#### Phase 7 — Adapter-local resolution

- **ollama_resolution_trace_on_dispatch** — POST chat with recommended, subscribe to `dispatch.*.routed`, assert the routed event payload includes `{ provider: ollama, instance, model, reason }`.
- **ollama_resolution_trace_on_failure** — force all Ollama instances into an unrecoverable state (no models loaded), POST chat, assert the response error body includes a `resolution_trace` explaining what was tried.
- **adapter_respects_locality_preference** — set `preferences.selectors.locality = "local"`, POST chat, assert the dispatcher never routes to a cloud provider even if its ranking would otherwise win.

#### Phase 8 — Provider-level Directory

- **single_registration_per_primitive_per_provider** — Ollama has three instances; `GET /v1/catalog` shows one `text.chat` registration from Ollama, not three.
- **skill_registrations_merged** — with two ComfyUI skills loaded, the catalog shows three image-generate entries (base + two specializations) from one provider.
- **registration_events_fire** — subscribe to `directory.provider.comfyui.registration.*`, import a new skill, assert an `added` event with the new specialization fires.
- **specialization_opt_in_only** — POST `/v1/image/generate` (no skill), assert the base registration is used even if a specialization exists with a higher recommendation score.
- **specialization_explicit_route** — POST `/v1/image/generate/tron`, assert the Tron specialization is used.

#### Phase 9 — `/v1/do` flows

- **single_action_unchanged** — POST a single-action body, assert behavior matches the pre-refactor code path.
- **flow_transcribe_and_summarize** — POST a two-step flow (`audio.transcribe` → `text.chat` with a placeholder referencing the transcript), subscribe to `jobs.{id}.step.*`, assert both step events fire in order and the terminal `jobs.{id}.result` contains the summary.
- **flow_placeholder_unresolved_rejects** — POST a flow where a placeholder references a non-existent step, assert 400 with `code: flow_reference_error`.
- **flow_step_failure_terminates** — craft a flow where step 1 fails, assert `jobs.{id}.step.1.failed` fires, then `jobs.{id}.failed`, and no `step.2.*` events are emitted.
- **flow_partial_results_preserved** — in the above test, `GET /v1/jobs/{id}/result` returns the completed step 1's output plus the failure context.
- **single_job_id_for_flow** — POST a flow, assert exactly one top-level `jobs.{id}` appears in the response.

#### Phase 10 — Preferences

- **get_preferences_empty** — fresh garden, `GET /v1/preferences` returns `{}`.
- **put_preferences_merges** — PUT `{ "image.width": 1024 }`, then PUT `{ "image.height": 1024 }`, assert both persist.
- **preferences_change_publishes** — subscribe to `preferences.changed`, PUT a new key, assert the event fires.
- **preferences_rebuild_catalog** — PUT `image.width = 1024`, `GET /v1/catalog`, assert the `image.generate` field's rendered default is now 1024.
- **preferences_inject_at_dispatch** — set `image.sampling.temperature = 0.3`, POST image.generate without the field, assert the routed event shows the temperature used was 0.3.

### 11.5 Cross-phase integration scenarios

The `garden_integration.rs` binary exercises scenarios that span multiple phases and represent real user journeys.

- **butterfly_import_and_run** — Import the reference CivitAI butterfly skill, wait for `skills.{moniker}.state = ready`, dispatch an `image.generate/{moniker}` request, assert the image comes back. This exercises phases 3, 4, 5, 6, 7, 8.

- **saturate_comfyui_spill_is_rejected_not_amd** — Saturate both CUDA ComfyUI instances with long-running requests, POST a third request, assert it queues or returns `AllInstancesSaturated` — and critically, assert the bus shows no attempt to claim resources on `stone-rocm-01`. This is the negative test for compute-stack filtering.

- **ollama_absorbs_spillover** — Same setup but for Ollama: saturate both CUDA Ollama instances with chat requests, POST a third, assert it routes to the ROCm stone successfully. This is the positive test for heterogeneous routing.

- **shared_gpu_composition** — On `stone-cuda-01`, place an Ollama claim for 4 GB (embedding workload), then POST an `image.generate` request to ComfyUI that estimates 6 GB. Assert ComfyUI's Instance Manager picks `stone-cuda-01` (still has 14+ GB free after headroom) and the claim succeeds. Assert both complete concurrently and the pressure events show the composition.

- **shared_gpu_rejection** — Same setup but with a ComfyUI workflow estimated at 20 GB. Assert ComfyUI's Instance Manager prefers `stone-cuda-02` (free) over `stone-cuda-01` (4 GB already claimed by Ollama) purely via the pressure penalty.

- **transcribe_summarize_flow** — POST a two-step flow that transcribes a canned audio file via Whisper and summarizes via Ollama. Assert the job completes with a single `job_id` and both steps show in the event stream in topological order.

- **reconnect_across_long_job** — Start a 30-second image generation, subscribe to its events, forcibly close the SSE connection after 2s, reconnect with `Last-Event-ID`, assert the missed progress events are replayed and the terminal event is eventually observed.

- **preferences_drive_recommendation** — Set `preferences.selectors.locality = "local"`, POST `/v1/text/chat`, assert the routed provider is local (Ollama) even though Anthropic would otherwise rank higher for the specific prompt.

- **skill_failure_recovery** — Import a skill whose primary model has been deleted from all caches. Assert `skills.{moniker}.state = failed` fires with `reason: models_unresolvable` and the draft directory is marked as invalid (not deleted — the operator can edit and retry).

- **crash_cleanup** — Place several claims, kill the orchestrator process mid-flight, restart. Assert that on restart the Resources domain starts with a clean slate (claims are transient) and subsequent dispatches succeed without interference.

### 11.6 Assertions the suite reuses

`common/assertions.rs` provides high-level assertions that the per-phase files invoke repeatedly:

```rust
/// Subscribe to a focus, wait up to `timeout` for an event whose topic matches
/// `pattern` and whose payload satisfies `predicate`. Fail with a diagnostic
/// showing every event that was seen.
async fn expect_event_matching(
    sse: &mut SseClient,
    pattern: &str,
    predicate: impl Fn(&Value) -> bool,
    timeout: Duration,
);

/// Assert that a given sequence of topic patterns appears in order, with other
/// events possibly interleaved.
async fn expect_ordered_topics(sse: &mut SseClient, patterns: &[&str], timeout: Duration);

/// Assert that during the given closure's execution, no event matching the
/// forbidden pattern is ever published.
async fn assert_no_event<F, Fut>(sse: &mut SseClient, forbidden: &str, body: F) -> Fut::Output
where F: FnOnce() -> Fut, Fut: Future;

/// Assert that a pressure snapshot for a given device currently reports the
/// expected committed/available/mode values (with tolerance for debounce).
async fn assert_device_pressure(
    state: &AppState,
    stone: &StoneName,
    device: u32,
    expected: PressureExpectation,
);
```

These helpers encode the ADR's invariants once so the per-phase tests stay focused on the specific behavior they exercise.

### 11.7 Mock garden mode

For CI environments without the real hardware, the suite offers a `MOCK_GARDEN=1` environment variable that substitutes:

- Topology ingestion with a hand-written `StoneTopology` matching the reference garden's shape.
- `InstanceRuntime` implementations with scripted outcomes (echoing prompts, returning canned PNGs).
- Moss API calls with in-process stubs.

The mock garden exercises every code path that is not specifically about real hardware — claim accounting, event fanout, selection logic, flow execution, preference layering. It cannot cover GPU warmth, real benchmark latencies, or true concurrent GPU contention. The real garden is required for the shared-GPU composition test and anything that depends on wall-clock dispatch timing.

CI runs the mock garden on every PR; the real garden tests run nightly on the operator's test rig and block releases.

### 11.8 Suite invocation

```
# Full suite against real garden
cargo test --package zen-garden-ai-orchestrator --test '*' -- --ignored

# Mock garden only
MOCK_GARDEN=1 cargo test --package zen-garden-ai-orchestrator --test '*'

# Single phase
cargo test --package zen-garden-ai-orchestrator --test phase04_resources

# Single integration scenario
cargo test --package zen-garden-ai-orchestrator --test garden_integration -- shared_gpu_composition
```

The suite produces human-readable failure output: when `expect_event_matching` fails, the diagnostic shows every event received on the focus, their topics and payloads, and the pattern that was sought. Debugging a failing test should never require re-running with `RUST_LOG=trace`.

---

## Related ADRs

- **ORCH-0011** — recommended model monikers. Elevated from opt-in to default interpretation of unfilled selectors.
- **ORCH-0013** — AI orchestrator promotion. Unchanged.
- **ORCH-0025** — three-tier skill persistence. Disk layout unchanged.
- **ORCH-0026** — vision-assisted skill naming. Lifecycle now publishes `skills.{moniker}.named` on the bus.
- **ORCH-0028** — orchestrator core. Pipeline, `Provider` trait, vocabulary, media model all survive; `Directory` unit shifts from instance to provider; coordination surface rewritten.
- **ORCH-0029** — skill subsystem. Two-aggregate split retired; disk layout, loader, and import pipeline preserved.
