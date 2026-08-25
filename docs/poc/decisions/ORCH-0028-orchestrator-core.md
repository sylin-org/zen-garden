---
audience: developer
doc_type: decision
status: proposed
---

# ORCH-0028: Orchestrator Core — Pipeline, Providers, Vocabulary, Media

**Date**: 2026-04-07
**Status**: Proposed
**Deciders**: Leo
**Related ADRs (integrated into this design)**:
- ORCH-0011 (recommended model monikers) — integrated into the RecommendationEngine service
- ORCH-0015 (model directory architecture) — promoted to the Directory aggregate
- ORCH-0025 (three-tier skill persistence) — skills load from disk; this ADR consumes them
- ORCH-0026 (vision-assisted skill naming) — produces the human-friendly monikers the registry uses

This ADR defines the orchestrator core: the request pipeline, the Provider contract, the Directory aggregate, the media model, and the canonical vocabulary. It is a greenfield specification. Implementation is a break-and-rebuild: the current orchestrator's `api/`, `domain/`, and `catalog/` layers are wiped and replaced. Only concepts that survive intact — individual vendor adapters (Ollama, Anthropic, OpenAI, Google, ComfyUI, LibreTranslate, Infinity, Docling, Kokoro, WhisperCpp, Speaches, OpenedaiSpeech), their wire-format translation logic, and the on-disk skill persistence from ORCH-0025 — are carried forward, and even those are rewrapped behind the new `Provider` trait before use.

---

## Mandate

**Break and rebuild.** The orchestrator's core is rewritten from zero. No shims, no compatibility layers, no parallel operation. Every type, function, and module that does not map to a concept in this ADR is deleted in the first commit of the rebuild branch, before any new code is written.

The rule is binary and enforced by CI: **if a symbol is not referenced by this ADR, it does not exist in the new codebase.** No "we'll clean it up later." No "let's keep it in case we need it."

Concepts preserved from the existing codebase (and only these):

- **Vendor client code** — the private HTTP helpers, request/response type mappings, and authentication logic inside `offerings/{vendor}/client.rs` and `offerings/{vendor}/types.rs` for each supported vendor. These are consumed privately by the new provider wrappers.
- **Skill disk persistence** (ORCH-0025) — the `{data_dir}/skills/{provider}/{moniker}/` layout and file format. A new loader reads this layout into the new Directory aggregate at startup.
- **Vision-assisted skill naming** (ORCH-0026) — provides human-friendly monikers during skill import.
- **Recommendation monikers** (ORCH-0011) — the `recommended:{capability}` concept becomes the RecommendationEngine service.
- **Ollama fitness / benchmark / placement logic** — relocated inside the Ollama provider's private implementation as module-private code. Not promoted to any shared layer.

Everything else is removed. The complete wipe list is in the "Wipe list" section below.

---

## Guiding principle

> **User satisfaction is the ultimate value. Intent must be understood and executed upon with minimal blocks.**

Every decision in this ADR is evaluated against that standard. "Minimal blocks" means the caller sends what they mean and the system finds a way to serve it, or tells them clearly why it cannot. No stubs, no silent drops, no vendor-specific knobs leaking into the public API.

Three concrete commitments follow from the principle:

1. **Callers write one shape; the orchestrator translates to many shapes.** A `text.chat` request with `temperature: 1.0` works against Ollama, Anthropic, OpenAI, and Google without the caller knowing how each vendor spells that field.
2. **Unknown input fails loudly; unknown output passes through.** The caller gets a precise error when they send a malformed request. Providers that populate fields the orchestrator hasn't yet catalogued still have their output delivered — we don't drop data because our schema is behind.
3. **Every feature the catalog advertises, the orchestrator serves.** No stubs, no "pending implementation." A primitive in the catalog is a primitive that works end-to-end against at least one real provider.

---

## Objectives

**O1. One pipeline, composable stages.** Every request flows through the same stages: parse, contextualize, resolve media, dispatch, execute, respond. Each stage does one thing and can be tested in isolation.

**O2. One request object.** The `OrchestratorRequest` carries everything any downstream component needs. It is built once at HTTP ingress, enriched by each stage, handed to the provider, and dropped when the request completes.

**O3. Dispatcher as a pipe.** The dispatcher knows who handles what; it does not know how. It looks up a provider by name and hands over a fully-prepared request. Ten lines of code.

**O4. Providers are autonomous.** Each provider owns its own instances, its own load balancing, its own busy semantics, its own protocol translation. The orchestrator does not micromanage.

**O5. Directory is a single aggregate.** One object holds the registration of every provider, every skill, every model, and the derived lookup tables. Handlers consult it; they never consult overlapping sub-domains.

**O6. Vocabulary is the public contract.** Inputs and outputs are described by per-primitive vocabularies: lists of namespaced keys with type constraints. The vocabulary is the caller's reference and the provider's target. It lives in code, validated at startup.

**O7. Composable input and output.** Payloads are nested JSON objects keyed by modality (`text`, `image`, `audio`, `video`). Providers populate what they produce. Consumers read what they need. No per-primitive typed enums.

**O8. Media is a first-class orthogonal concern.** Media is uploaded, referenced by GUIDv7, and delivered to providers in the format each provider declared. The orchestrator handles the format negotiation; providers see what they asked for.

**O9. Three delivery modes.** The provider chooses whether to serve a request synchronously, asynchronously, or as a stream. The orchestrator supports all three with one response shape; the provider's return variant picks the delivery mode.

**O10. GUIDv7 for all mutable identities.** Every request, response, media entry, job, and pipeline instance has a GUIDv7. Primitives, provider names, skill monikers, and URLs keep their human-readable identities.

**O11. No magic strings.** Every canonical field key, every action name, every constant that crosses layer boundaries is declared as a typed constant in Rust. Magic strings in code are forbidden.

**O12. Clean cut.** The rebuild is a break-and-replace: the wipe list executes first, and the new code is written from a blank slate. No shims, no parallel operation.

---

## Core principles (the decision record)

These are the decisions the design is built on. Each one was chosen deliberately; alternatives were considered and rejected.

### Principle 1: The pipeline is linear and composable

```
HTTP ingress
    ↓
OrchestratorRequest construction
    ↓
Contextualizer (resolve action, model, provider; normalize payload)
    ↓
Media resolver (apply provider's declared delivery mode to each media reference)
    ↓
Dispatcher (lookup provider, hand off)
    ↓
Provider::onboard (the provider takes custody)
    ↓
Response (back through dispatcher, serialized by HTTP ingress)
```

Each stage has a single responsibility. Each stage can be unit-tested. Stages communicate through `OrchestratorRequest` evolving across the pipeline.

**Rejected**: a router object that second-guesses provider choices. Routing is collapsed into a single contextualizer + single dispatcher; no separate router service.

### Principle 2: The request object carries everything

A single struct, `OrchestratorRequest`, is created at HTTP ingress and flows through every stage. It contains:
- Request identity (id, correlation id, receive time)
- Caller intent (action, raw payload, selectors, constraints)
- Referenced media
- Resolution state (populated by the contextualizer)
- Execution context (media store handle, job sink, cancellation token)

By the time the provider receives it, everything is resolved. The provider needs no other argument.

**Rejected**: a separate `ExecutionContext` sidecar parameter. Treating the context as a first-class field on the request keeps the function signatures tight and makes the object the single thing to pass around.

### Principle 3: The provider trait has one entry point for work, one state bundle, one subscription

```rust
async fn onboard(&self, request: OrchestratorRequest) -> Result<ProviderOutcome, ProviderError>;
fn state(&self) -> Arc<ProviderState>;
fn subscribe(&self) -> watch::Receiver<Arc<ProviderState>>;
```

The provider takes custody of the request and returns a `ProviderOutcome` describing how the work will be delivered: sync, async, or streaming. Every primitive goes through `onboard`. There is no `infer`, `embed`, `translate`, `rerank`, `speak`, `transcribe`, or `workflow` on the trait.

Live state (registrations, models, health, performance hints) is bundled in a single `ProviderState` struct. The provider publishes a new snapshot whenever anything changes via an internal `watch::Sender<Arc<ProviderState>>`; subscribers receive the current value immediately and every subsequent change. The Directory subscribes once at registration and watches for the rest of the provider's lifetime.

**Rejected**: a multi-method Provider trait with per-primitive entry points (one method for chat, one for embed, one for speech, etc.). A trait with N methods forces the dispatcher to know which method corresponds to each primitive, which reintroduces the per-primitive coupling this design removes.

**Rejected**: separate trait methods for each piece of live state (`models`, `models_changed`, `health`, `health_changed`, `registrations`, `registrations_changed`). Bundling state into one struct and exposing one subscribe method halves the trait surface and lets providers publish atomic transitions (e.g., a provider that goes unhealthy and drops a model at the same time publishes one combined change).

### Principle 4: The dispatcher is a pipe

The dispatcher is ten lines of logic: contextualize, resolve media, look up provider, call `onboard`, return. It has no knowledge of primitives, no per-provider branching, no fallback logic. If it cannot find a provider for the request, it returns a clean error. If the provider fails, it propagates the failure.

**Rejected**: a dispatcher that picks instances, applies fitness scores, or makes provider-specific decisions. Those concerns belong inside providers.

### Principle 5: Providers are autonomous and opaque

Each provider owns its instances. The orchestrator has zero visibility into instance count, instance health, model placement, queue depth, fitness data, or any other per-instance concern. When Ollama reports "I handle `text.chat`," the orchestrator dispatches to Ollama and Ollama decides which of its three instances serves the request.

Corollary: load balancing is a provider concern, not an orchestrator concern. The orchestrator appears as a load balancer to external callers because its providers are load-balancing internally.

**Rejected**: any shared `Instance` type promoted to an orchestrator-level concept. Forcing every provider (ComfyUI, LibreTranslate, cloud providers) to pretend they have uniform instance topology is a leaky abstraction that serves none of them.

### Principle 6: Vocabulary is the contract, providers narrow it

A vocabulary per primitive declares what keys the caller may send and what keys the provider may return. The vocabulary is orchestrator-owned and lives in Rust code.

Each provider, at registration time, declares which fields of the vocabulary it honors for each primitive it registers for. A provider can narrow the vocabulary (require a field the vocabulary marks optional, constrain a numeric range) but cannot extend it without using the escape-hatch `x_` prefix.

**Rejected**: schemas declared per-provider with no shared vocabulary. That model made it impossible for callers to write portable requests; they had to know which provider they were targeting and shape the request for that provider's quirks.

### Principle 7: Composable, namespaced outputs

The output of every primitive is a nested JSON object keyed by modality. The provider populates whatever fields it produced; the orchestrator validates against the vocabulary (with a warn-and-pass policy for unknown keys), serializes, and returns. No per-primitive output type, no discriminated union on content.

A `text.chat` returns `{text: {response: "...", finish_reason: "stop"}}`. An `image.generate` returns `{image: {media_id: "...", width: 1024}}`. An `image.analyze` returns `{text: {response: "..."}}` because cross-modal primitives populate the modality of the output, not the input.

**Rejected**: `CanonicalOutput` as an enum with one variant per primitive. Rejected because adding a primitive would require editing the enum, and because providers sometimes produce field combinations the enum doesn't anticipate.

### Principle 8: Media delivery is provider-declared

Each provider declares, per media-carrying field in its registration, how it wants to receive media. Three modes:

- **ById** — provider wants the media_id and will fetch bytes itself via the media store handle.
- **Base64** — orchestrator encodes bytes and substitutes `{base64, content_type}` into the canonical payload before dispatch.
- **Transfer** — provider stages the media to one of its own instances before execution (HTTP upload, shared path, in-memory handoff) and receives a handle to reference.

The contextualizer's media resolution stage reads each provider's declaration and applies the appropriate transformation. Providers see exactly what they asked for.

**Rejected**: a universal base64-in-payload strategy. ComfyUI, Whisper, and Docling don't accept base64 — they want file uploads. A universal strategy would force those providers to decode and re-upload on every request.

**Rejected (for v1)**: signed-URL delivery where the orchestrator mints a short-lived URL the provider fetches from. This requires the orchestrator to expose a public base URL reachable from the provider's network, plus HMAC key management for the signature. Both are out of scope for a homelab-scoped solution. Providers currently needing URL-based delivery (some cloud APIs for large media) fall back to Base64 in v1. A future ADR may reintroduce `Url` delivery alongside tunneling support.

### Principle 9: Three delivery modes on one response shape

The provider's return value is a `ProviderOutcome` enum with three variants:

- **`Sync(Output)`** — the result is ready; return it inline.
- **`Async(Output)`** — the work is happening in the background; the output contains a job reference.
- **`Streaming { initial, stream }`** — deltas will flow over a stream; the initial output carries metadata (including any pre-allocated media_id).

All three variants carry `Output`, which is a namespaced map of key-value pairs. There is no separate "media response" or "error response" variant — media is a key in Output, errors are `Err` from the Result.

**Rejected**: four separate enum variants for "completed, accepted, streaming, refused." The refusal case is a `Result::Err`; the rest collapse because delivery mode and content are orthogonal concerns.

### Principle 10: No magic strings

Every canonical field key (`text.prompt.user`, `image.media_id`, `usage.tokens.input`), every action identifier (`text.chat`, `image.generate.outpaint`), every delivery mode name, every vocabulary key is declared as a typed Rust constant in a central location.

Code that needs to reference a canonical key uses the constant, not a string literal. Code that emits a key (in output construction, in logging, in serialization) uses the constant.

**Rejected**: stringly-typed keys scattered through provider code. That invites typos that silently drop fields, and makes refactoring impossible.

---

## Decision

### Domain model

The orchestrator core is a small set of types organized around the DDD pattern established by ORCH-0020 (domain-owned state):

- **Aggregates** (state owners): `Directory`, `JobStore`, `MediaStore`, `IdempotencyStore`
- **Domain services** (pure, stateless logic): `Contextualizer`, `MediaResolver`, `Dispatcher`, `RecommendationEngine`
- **Entity trait**: `Provider`
- **Value objects**: `Primitive`, `Action`, `Moniker`, `OrchestratorRequest`, `ProviderState`, `ProviderOutcome`, `Output`, `Registration`, `Vocabulary`

Each aggregate owns its mutable state behind a `Mutex` and publishes immutable snapshots via `watch::channel`. API handlers read snapshots lock-free. Every mutation has a single documented writer. See **State ownership discipline** below.

#### Primitive (value object)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Primitive {
    // Text primitives
    TextChat,
    TextTranslate,
    TextEmbed,
    TextRerank,

    // Image primitives
    ImageGenerate,
    ImageEdit,
    ImageUpscale,
    ImageAnalyze,

    // Audio primitives
    AudioGenerate,
    AudioTranscribe,
}
```

Ten primitives. Video primitives (`VideoGenerate`, `VideoEdit`) are reserved but not part of the v1 locked inventory. They enter the catalog when a video provider exists.

Primitives are a closed enum: adding a primitive is an ADR amendment, not a runtime decision. The set is small and stable on purpose.

Each primitive has a `modality()` method returning `Text | Image | Audio` and a `dotted()` method returning the canonical name (`text.chat`, `image.generate`, etc.).

#### Action (value object)

```rust
pub struct Action {
    pub primitive: Primitive,
    pub skill: Option<Moniker>,
}
```

An `Action` is what the caller wants done: either a bare primitive (`text.chat`) or a skill-scoped primitive (`image.generate.outpaint`). The skill is a human-readable moniker, unique within its primitive.

The dotted form (`image.generate.outpaint`) is the canonical string representation used in URLs, logs, `_meta.action`, and the `/v1/do` dispatcher.

#### Moniker (value object)

```rust
pub struct Moniker(String);

impl Moniker {
    pub fn new(name: impl Into<String>) -> Result<Self, MonikerError>;
}
```

A skill name: lowercase kebab-case, no reserved words, ≤64 characters, globally unique within its primitive. `"cute-bunny"`, `"outpaint"`, `"vision-tag"` are monikers. `"image-generate"` is rejected (colliding with the primitive segment). `"new"`, `"list"`, `"batch"` are reserved.

#### Provider (entity trait)

```rust
pub trait Provider: Send + Sync + 'static {
    /// Stable human-readable identity.
    fn name(&self) -> ProviderName;

    /// Current snapshot of the provider's live state. Cheap to call;
    /// equivalent to `self.subscribe().borrow().clone()`.
    fn state(&self) -> Arc<ProviderState>;

    /// Subscribe to state changes. The returned receiver yields the current
    /// value immediately and every subsequent update. The Directory calls
    /// this once at registration time and keeps the receiver for the
    /// provider's lifetime.
    fn subscribe(&self) -> watch::Receiver<Arc<ProviderState>>;

    /// Take custody of a request. The provider owns instance selection,
    /// protocol translation, and response construction. Returns a
    /// `ProviderOutcome` describing how the result will be delivered.
    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError>;

    /// Clear any artifacts cached on the provider's instances. Default
    /// impl is a no-op; providers that stage files override.
    async fn flush_caches(&self) -> Result<FlushReport, ProviderError> {
        Ok(FlushReport::empty())
    }
}

pub struct ProviderState {
    /// Current health. Providers publish transitions eagerly.
    pub health: ProviderHealth,

    /// Current registration list. Mutates when a provider adds, removes,
    /// or modifies a skill (e.g., ComfyUI imports a workflow).
    pub registrations: Vec<Registration>,

    /// Current model catalog. Providers with dynamic model inventories
    /// (Ollama pulling models at runtime) publish as they change.
    pub models: Vec<Model>,

    /// Optional performance hints per registration. Providers that measure
    /// their own performance publish verdicts; providers that don't leave
    /// this empty and the orchestrator treats them as unmeasured.
    pub performance_hints: Vec<PerformanceHint>,
}

pub enum ProviderHealth {
    /// Provider is accepting requests normally.
    Healthy,
    /// Provider can accept some requests but some functionality is impaired.
    /// Reason is a short human-readable string for display in the catalog.
    Degraded { reason: String },
    /// Provider cannot accept requests. Removed from routing candidates until
    /// it transitions back to Healthy or Degraded.
    Offline { reason: String },
}

pub struct PerformanceHint {
    pub registration_id: RegistrationId,
    pub verdict: PerformanceVerdict,
    pub sample_count: u32,
    pub measured_at: DateTime<Utc>,
}

pub enum PerformanceVerdict {
    Fast,
    Degraded,
    Vetoed,
    Blocked,
    Unmeasured,
}
```

**Five methods total**: `name`, `state`, `subscribe`, `onboard`, `flush_caches`.

`ProviderState` is a single struct bundling everything the orchestrator needs to know about a provider's live state. The provider mutates its internal state however it wants (locks, background tasks, event loops) and publishes a new `Arc<ProviderState>` via an internal `watch::Sender` whenever anything changes. External consumers (Directory, dashboard) subscribe once and receive atomic state transitions.

**`performance_hints`** is optional. Providers that measure their own performance (Ollama via its benchmark runner, ComfyUI via workflow timing, cloud providers via observed latency) publish verdicts. Providers that don't have signals leave the field empty, and the RecommendationEngine treats the registration as `Unmeasured` — it gets a neutral score, neither boosted nor penalized. This makes measurement opt-in without forcing every provider to implement a benchmarking subsystem.

**`ProviderName`** is a human-readable string (`"ollama"`, `"comfyui"`, `"anthropic"`) declared as a typed wrapper. Canonical provider names are constants in `domain/keys/providers.rs`.

#### Registration (value object)

```rust
pub struct Registration {
    pub id: RegistrationId,           // GUIDv7, orchestrator-assigned
    pub provider: ProviderName,
    pub primitive: Primitive,
    pub strategy: RegistrationStrategy,
    pub honored_fields: Vec<HonoredField>,
    pub media_inputs: Vec<MediaInputSpec>,
    pub media_outputs: Vec<MediaOutputSpec>,
}

pub enum RegistrationStrategy {
    /// Bare registration: the provider handles this primitive for any caller
    /// request matching the primitive. Used by model-oriented providers (Ollama,
    /// OpenAI, Anthropic, Google) where the caller picks a model.
    Bare,

    /// Model registration: the provider handles this primitive and publishes
    /// a list of models it offers. The model directory uses this to build
    /// the model→provider lookup.
    Models { catalog: Vec<ModelDescriptor> },

    /// Skill registration: the provider handles this primitive under a named
    /// skill. Used by task-oriented providers (ComfyUI) where the caller picks
    /// a named workflow.
    Skill {
        moniker: Moniker,
        display_name: String,
        description: Option<String>,
    },
}

pub struct HonoredField {
    pub path: FieldPath,              // vocabulary key, e.g. "text.sampling.temperature"
    pub required: bool,                // narrowing: vocabulary may say optional, provider makes required
    pub range: Option<FieldRange>,     // narrowing: e.g., Anthropic clamps temperature to [0, 1]
}

pub struct MediaInputSpec {
    pub field: FieldPath,              // vocabulary key, e.g. "image.source"
    pub delivery: MediaDelivery,
    pub accepted_types: Vec<String>,   // MIME types accepted, e.g. ["image/png", "image/jpeg"]
}

pub struct MediaOutputSpec {
    pub field: FieldPath,              // vocabulary key, e.g. "image.media_id"
    pub content_type: String,          // MIME type the provider produces
}
```

A registration is a fact: "provider P serves primitive X via strategy S, honoring these fields, expecting media in these formats." Providers emit registrations at construction time and whenever their internal state changes (e.g., ComfyUI's skill list updates on import).

#### Directory (aggregate root)

The Directory is the single aggregate for provider metadata: which providers exist, which registrations they publish, which models they host, what their health looks like. It follows the **domain-owned state** pattern established by ORCH-0020: private mutable state behind a tokio `Mutex`, immutable snapshots published via `watch::channel`, lock-free reads for every consumer.

```rust
pub struct Directory {
    /// Private mutable state. Only the directory maintenance task
    /// (see "State ownership discipline") acquires this lock.
    state: Mutex<DirectoryState>,

    /// Published snapshot. Readers acquire via `snapshot()` or subscribe
    /// via `subscribe()` and never touch the state lock.
    tx: watch::Sender<Arc<DirectorySnapshot>>,
}

struct DirectoryState {
    providers: HashMap<ProviderName, Arc<dyn Provider>>,
    subscriptions: HashMap<ProviderName, watch::Receiver<Arc<ProviderState>>>,
    dirty: bool,
}

#[derive(Clone)]
pub struct DirectorySnapshot {
    pub version: u64,
    pub updated_at: DateTime<Utc>,

    /// All providers currently registered, by name.
    pub providers: Arc<HashMap<ProviderName, ProviderView>>,

    /// All primitives with at least one registered provider.
    /// Missing primitives do not appear — the catalog reflects what
    /// the orchestrator can actually serve right now.
    pub primitives: Arc<HashMap<Primitive, PrimitiveView>>,

    /// All skills currently registered, keyed by (primitive, moniker).
    pub skills: Arc<HashMap<(Primitive, Moniker), SkillView>>,

    /// Model-to-provider lookup. Models are fully-qualified
    /// (`provider|name`) to prevent cross-provider collisions.
    pub models: Arc<HashMap<ModelFqn, ModelView>>,
}

pub struct ProviderView {
    pub name: ProviderName,
    pub health: ProviderHealth,
    pub registration_count: u32,
    pub model_count: u32,
}

pub struct PrimitiveView {
    pub primitive: Primitive,
    pub registrations: Vec<RegistrationId>,
    pub skill_monikers: Vec<Moniker>,
}

pub struct SkillView {
    pub registration: Registration,
    pub provider: ProviderName,
}

pub struct ModelView {
    pub fqn: ModelFqn,              // "ollama|deepseek-r1:8b"
    pub short_name: String,          // "deepseek-r1:8b"
    pub provider: ProviderName,
    pub registration: RegistrationId,
    pub capabilities: Vec<String>,   // primitives this model can serve
}

impl Directory {
    pub async fn register(&self, provider: Arc<dyn Provider>) -> Result<(), DirectoryError>;
    pub async fn unregister(&self, name: &ProviderName);

    /// Lock-free snapshot read.
    pub fn snapshot(&self) -> Arc<DirectorySnapshot> {
        self.tx.borrow().clone()
    }

    /// Subscribe for reactive consumers (catalog builder, dashboards).
    pub fn subscribe(&self) -> watch::Receiver<Arc<DirectorySnapshot>> {
        self.tx.subscribe()
    }

    /// Look up a provider by name.
    pub fn provider(&self, name: &ProviderName) -> Option<Arc<dyn Provider>>;
}
```

**Model names are fully-qualified** via `ModelFqn` (e.g., `"ollama|deepseek-r1:8b"`). The Directory's `models` lookup is keyed by FQN. Callers may send a short name (`"deepseek-r1:8b"`) in their request; the Contextualizer's resolution pass finds the matching FQN. If multiple providers publish the same short name (e.g., Ollama and a hypothetical cloud provider both offer `llama-3.1:8b`), disambiguation uses operator pins first, alphabetical-provider order as a tiebreaker. Callers wanting to lock in a specific provider's version pass the FQN directly.

**Moniker uniqueness** is enforced per `(Primitive, Moniker)` pair. Two providers cannot both register `image.generate.outpaint`; the second registration fails with a clear `duplicate_moniker` error. A provider can rename its skill to resolve the conflict.

**Version monotonicity**: the `version: u64` counter increments only when the rebuilt snapshot differs structurally from the previous one. Back-to-back calls that don't change anything return the same version. ETag responses on `/v1/catalog` use this version directly.

#### Directory maintenance task

The Directory has a single background writer: the `directory_maintenance` task. It is spawned at orchestrator startup and runs for the process lifetime.

Responsibilities:
- On `register(provider)`: subscribe to the provider's `state` channel; mark the directory dirty.
- On `unregister(name)`: drop the subscription; mark dirty.
- On any provider's state channel ticking (models changed, registrations changed, health changed, performance hints changed): mark dirty.
- On every iteration: if dirty, rebuild the snapshot from the current state, publish via `tx.send()`, clear the dirty flag.

The rebuild is **debounced** with a 100ms window. A burst of provider events (discovery completing on three stones within a few ms of each other) triggers one rebuild, not three. The debounce window is a minimum; a rebuild never runs more than 10 times per second in steady state.

The rebuild function is pure: given the current provider map, iterate every provider's current `ProviderState`, merge into the `DirectorySnapshot`, compare structurally against the previous snapshot, bump the version if different, and return. No I/O, no provider calls, no blocking.

**No other code mutates Directory state.** Provider registration goes through `register()`, which queues a mutation to the maintenance task's channel. Readers never wait; writers serialize through a single task. This is the ORCH-0020 discipline: one writer per mutable domain, many readers of the published snapshot.

#### State ownership discipline

Every piece of mutable state in the orchestrator has **exactly one documented writer**. Readers are unconstrained; writers are a known task or call-site. This prevents races and keeps derived state consistent.

| State | Owner | Writer | Mutation trigger |
|---|---|---|---|
| `Directory` internal map | `Directory` aggregate | `directory_maintenance` task | Provider register/unregister; provider state channels |
| `DirectorySnapshot` watch | `Directory` aggregate | `directory_maintenance` task | Debounced on dirty flag (100 ms window) |
| Catalog JSON watch | `catalog_builder` task | `catalog_builder` task | Subscribes to `DirectorySnapshot`; rebuilds on version bump |
| `/v1/do` index watch | `catalog_builder` task | `catalog_builder` task | Same as catalog JSON |
| Recommendation cache | `RecommendationEngine` | `recommendation_refresher` task | Subscribes to `DirectorySnapshot`; rebuilds on version bump |
| `MediaStore` metadata + bytes | `MediaStore` aggregate | Media upload handler; `media_gc` task | New uploads; GC sweep for expired/unreserved entries |
| Media reservations | `MediaStore` aggregate | Job creation / completion hooks | `reserve()` on job start; `release_reservation()` on job terminal state |
| `JobStore` | `JobStore` aggregate | Job creation call-sites (via `JobSink`); `job_gc` task | Job creation; job updates; sweep of terminal jobs |
| `IdempotencyStore` | `IdempotencyStore` aggregate | Dispatcher (on cache miss + completion); `idempotency_gc` task | Request completion; sweep of stale entries |
| Request counters | `DemandLedger` | Dispatcher (post-completion hook) | Every completed request |
| `ProviderState` internal | Each provider | That provider's private implementation | Whatever the provider's internal logic decides |

**Providers are opaque.** Each provider manages its own internal state however it wants. The orchestrator never peers inside; it only reads what the provider publishes through its `ProviderState` snapshot. This is what lets the Ollama provider run its own benchmark subsystem, or the ComfyUI provider maintain its own per-instance file cache, without the orchestrator knowing or caring.

**Readers never block writers.** Every snapshot is an `Arc<T>` pulled from a `watch::channel`. A reader that holds a snapshot while the writer publishes a new one simply has an older `Arc`; the writer proceeds without waiting. This is the ORCH-0020 guarantee: fast reads, never contended.

#### Pre-built snapshots for discovery endpoints

Two endpoints — `/v1/catalog` and `/v1/do` — are frequently polled by dashboards and SDKs. Serializing them on every request would mean iterating the full Directory snapshot, rendering JSON, and handing it out many times per second. That is wasteful when the underlying data changes infrequently.

Instead, the `catalog_builder` background task subscribes to the `DirectorySnapshot` watch channel and pre-renders both JSON documents whenever the version bumps:

- **Catalog JSON** — the full `GET /v1/catalog` response body, including all primitives, skills, models, providers, and vocabularies. Published via its own `watch::Sender<Arc<Value>>`.
- **`/v1/do` index JSON** — the abbreviated action list response body, including examples and setup hints. Published via a separate `watch::Sender<Arc<Value>>`.

HTTP handlers for both endpoints read their respective watch channel, clone the `Arc<Value>`, and serialize directly into the response body. No work on the hot path. Dashboards polling twice per second see no measurable load.

ETag support is straightforward: each pre-built document carries the Directory version it was built from. Responses include `ETag: "<version>"`, and incoming `If-None-Match: "<version>"` with a matching version returns `304 Not Modified`.

#### RecommendationEngine

The RecommendationEngine answers a single question: **"given a primitive, which concrete model should serve it right now?"** This is a point-in-time decision, not a long-horizon plan. Callers using `recommended:{primitive}` as their model hint get whatever the engine returns.

The engine is a **domain service** — pure logic operating on the Directory snapshot plus a few extra inputs (operator pins, a simple request counter for light demand weighting). It has no mutable state of its own; its output is computed from the inputs and cached until the inputs change.

```rust
pub struct RecommendationEngine {
    directory: Arc<Directory>,
    pins: Arc<PinRegistry>,
    demand: Arc<DemandLedger>,
    cache: watch::Sender<Arc<RecommendationCache>>,
}

pub struct RecommendationCache {
    pub version: u64,                        // matches Directory version at build time
    pub per_primitive: HashMap<Primitive, RankedRecommendations>,
}

pub struct RankedRecommendations {
    pub primitive: Primitive,
    pub selected: Option<ModelFqn>,          // rank 1 (or None if nothing is eligible)
    pub candidates: Vec<Recommendation>,
    pub reasoning: Vec<String>,              // per-primitive "why this model" breadcrumbs
}

pub struct Recommendation {
    pub model: ModelFqn,
    pub rank: u32,
    pub score: i64,
    pub pinned: bool,
    pub verdict: Option<PerformanceVerdict>, // from provider hints
    pub reasoning: Vec<String>,
}
```

##### Layered scoring

Scoring is layered and per-primitive. Each layer adds (or subtracts) from a base score. The layers are:

- **Layer 0 — Eligibility**: is this model registered for the requested primitive? Eligible models get a base score; ineligible models are filtered out.
- **Layer 1 — Operator pin**: if the operator has pinned a specific model for this primitive and the pin is eligible, it forces rank 1 regardless of other scoring. Pinned models are always selected when eligible. Pinned-but-ineligible pins are silently ignored (the pin doesn't exist anymore).
- **Layer 2 — Performance verdict**: providers that publish `PerformanceHint` entries contribute scores based on verdict. `Fast` adds a large bonus, `Degraded` adds a modest bonus, `Vetoed` subtracts, `Blocked` is filtered out. Providers without hints are treated as `Unmeasured` — neutral score, neither rewarded nor penalized.
- **Layer 3 — Capability fit**: if the caller's request implies a specific capability (presence of `text.tools.definitions` → wants tool calling, `image_url` parts in messages → wants vision), models declaring that capability get a boost.
- **Layer 4 — Fallback**: when no layer produces a winner, deterministically pick the first alphabetically-ordered eligible model. Guarantees a working default for first-time installs with no benchmarks, no pins, and no demand signal.

Weights for each layer are per-primitive. Each primitive has a weights file under `domain/recommendation/` that specifies how much each layer contributes. `text.chat` weights quality higher; `text.embed` weights speed higher; `image.generate.outpaint` has only one eligible provider anyway so weights barely matter.

##### Scope: recommender, not advisor

The RecommendationEngine answers point-in-time questions. It does **not** make configuration recommendations like "move this model to that stone" or "pull this model on a new provider" — those are the responsibility of a future **advisor** component that reads demand history over long time scales and proposes changes to an operator. The recommender is stable across requests (its output only changes when the Directory or pins change); an advisor would operate on 6-hour or 3-day horizons.

v1 ships the recommender. The advisor is explicitly deferred.

##### Demand ledger (reserved surface, passive in v1)

Every completed request updates a simple pond-wide counter in the `DemandLedger`, keyed by `(primitive, provider, model, outcome)`. The ledger accumulates data for future use but does not affect v1 routing or recommendations. It is exposed via metrics scraping (`GET /metrics`) as the raw data source for a future advisor.

The cost of maintaining the ledger is a few atomic increments per completed request. The cost of not maintaining it is losing every request's signal permanently. Accumulating the data is cheap insurance; making decisions from it is where the sophistication lives, and that's deferred.

##### HTTP surface

Operator endpoints for managing pins:

- **`GET /v1/recommendations`** — list every primitive and its current recommendation resolution.
- **`GET /v1/recommendations/{primitive}`** — current recommendation for one primitive, with reasoning breadcrumbs.
- **`PUT /v1/recommendations/{primitive}`** with body `{"model": "ollama|deepseek-r1:8b"}` — pin a model.
- **`DELETE /v1/recommendations/{primitive}`** — unpin; falls back to layered scoring.

Pins are persisted to `{data_dir}/recommendations.json` and reloaded at startup.

##### Refresh

The RecommendationEngine has its own background task — `recommendation_refresher` — that subscribes to the Directory snapshot. When the Directory version bumps, the refresher rebuilds the cache and publishes it. This matches the State Ownership discipline: one writer (`recommendation_refresher`), many readers (Contextualizer, operator endpoints).

#### Vocabulary (value object)

```rust
pub struct Vocabulary {
    pub primitive: Primitive,
    pub input: IoSchema,
    pub output: IoSchema,
}

pub struct IoSchema {
    pub required: Vec<FieldSpec>,
    pub optional: Vec<FieldSpec>,
    pub aliases: Vec<Alias>,
    pub shared_namespaces: Vec<SharedNamespace>,
}

pub struct FieldSpec {
    pub path: FieldPath,
    pub field_type: FieldType,
    pub description: &'static str,
}

pub enum FieldType {
    String,
    Integer { min: Option<i64>, max: Option<i64> },
    Number { min: Option<f64>, max: Option<f64> },
    Boolean,
    Array,
    Object,
    MediaRef,              // special: a {media_id: "..."} reference
    MessageHistory,        // special: array of {user, assistant} pairs
}

pub struct Alias {
    pub from: FieldPath,
    pub to: FieldPath,
    pub condition: AliasCondition,
}

pub enum AliasCondition {
    /// Alias fires whenever the source field is present, regardless of type.
    Always,
    /// Alias fires only when the source value is a string.
    WhenString,
    /// Alias fires only when the source value is an object.
    WhenObject,
    /// Alias fires only when the source value is an array.
    WhenArray,
    /// Special-case transformer: decomposes an OpenAI-shape `messages: [...]`
    /// array into `text.prompt.system`, `text.prompt.user`, and
    /// `text.prompt.previous`. The final user message becomes `prompt.user`;
    /// any system message becomes `prompt.system`; prior user/assistant
    /// turns become `prompt.previous`. This is the only multi-target alias
    /// in the v1 vocabulary. A trailing user message with no prior assistant
    /// reply is the normal case (that's the turn being answered); other
    /// shapes (empty messages, trailing assistant, mid-conversation system)
    /// are rejected with clear error messages.
    MessagesDecomposer,
}
```

The vocabulary lives in Rust code, one file per primitive under `domain/vocabulary/`. Vocabularies are constructed at startup and registered with the `VocabularyRegistry` (a simple `HashMap<Primitive, Vocabulary>`).

#### OrchestratorRequest (value object)

```rust
pub struct OrchestratorRequest {
    // Identity
    pub id: RequestId,                       // GUIDv7
    pub correlation_id: CorrelationId,
    pub received_at: DateTime<Utc>,

    // Intent (caller-supplied)
    pub action: Action,
    pub payload: Value,                      // nested JSON, canonicalized per vocabulary
    pub selectors: Selectors,
    pub constraints: Constraints,

    // Media
    pub media: MediaContext,

    // Resolution state (filled by contextualizer)
    pub resolved_provider: Option<ProviderName>,
    pub resolved_model: Option<ModelRef>,

    // Execution context
    pub context: ExecutionContext,
}

pub struct Selectors {
    pub provider: Option<ProviderName>,      // caller override: "ollama"
    pub model: Option<String>,               // caller override: "deepseek-r1:8b"
    pub skill: Option<Moniker>,              // caller override (normally in URL)
}

pub struct Constraints {
    pub zone: ZoneConstraint,
    pub idempotency_key: Option<String>,
}

pub struct MediaContext {
    pub referenced: Vec<MediaReference>,     // every media_id the request mentions
    pub resolutions: HashMap<MediaId, ResolvedMedia>,  // post-media-resolver
}

pub struct MediaReference {
    pub id: MediaId,
    pub field: FieldPath,                    // where in the payload this reference appears
    pub content_type: String,
    pub metadata: Value,
}

pub struct ExecutionContext {
    /// Handle to the media store for reading referenced media and
    /// staging transfers. Always present.
    pub media_store: Arc<dyn MediaStore>,

    /// Job sink for reporting progress and terminal state. Always
    /// present — the dispatcher pre-creates a job record before calling
    /// `onboard`. Providers returning `Sync` ignore the sink; the
    /// record is marked complete inline. Providers returning `Async`
    /// or `Streaming` use the sink to publish updates.
    pub job_sink: Arc<JobSink>,

    /// Cancellation signal. Fired when the caller cancels the request
    /// (via `DELETE /v1/jobs/{id}`) or the orchestrator is shutting down.
    /// Providers should check periodically and abort work when observed.
    pub cancel: CancellationToken,

    /// Tracing span for this request. Providers attach their own work
    /// spans underneath it.
    pub span: tracing::Span,
}
```

One struct, all the state. Passes through the pipeline. Read by the contextualizer, the media resolver, the dispatcher, and finally the provider.

#### ProviderOutcome (value object)

```rust
pub enum ProviderOutcome {
    /// Provider produced a complete result inline.
    Sync(Output),

    /// Provider accepted the request and will process asynchronously.
    /// The Output carries `job.id`, `job.status`, `job.eta_seconds`.
    Async(Output),

    /// Provider is producing a stream of deltas.
    /// `initial` is the pre-stream announcement (carries `job.id`, media ids, etc.).
    /// `stream` yields per-chunk Output values.
    Streaming {
        initial: Output,
        stream: BoxStream<'static, Result<Output, ProviderError>>,
    },
}
```

Three delivery modes, all carrying `Output`. The provider chooses which variant to return based on its own state and the request shape.

#### Output (value object)

```rust
pub struct Output {
    fields: BTreeMap<String, Value>,
}

impl Output {
    pub fn new() -> Self;
    pub fn set(&mut self, key: &FieldPath, value: impl Into<Value>) -> &mut Self;
    pub fn get(&self, key: &FieldPath) -> Option<&Value>;
    pub fn has(&self, key: &FieldPath) -> bool;
    pub fn keys(&self) -> impl Iterator<Item = &str>;

    /// Serialize to nested JSON (the wire format).
    pub fn to_nested(&self) -> Value;

    /// Parse from nested JSON (when deserializing for idempotency cache, etc.).
    pub fn from_nested(value: Value) -> Result<Self, OutputError>;
}
```

Output is a flat dotted-key map internally and a nested JSON object on the wire. Providers populate it with canonical constants:

```rust
let mut output = Output::new();
output.set(&keys::TEXT_RESPONSE, "Hello!");
output.set(&keys::TEXT_FINISH_REASON, keys::values::FINISH_REASON_STOP);
output.set(&keys::USAGE_TOKENS_INPUT, 12);
output.set(&keys::USAGE_TOKENS_OUTPUT, 3);
```

The serialized form is:
```json
{
  "text": {
    "response": "Hello!",
    "finish_reason": "stop"
  },
  "usage": {
    "tokens": {
      "input": 12,
      "output": 3
    }
  }
}
```

### Canonical field keys (no magic strings)

Every field key used in a vocabulary, a provider's output, a validator, or a log is declared as a constant in `domain/keys/`. Providers and vocabulary builders reference the constants.

```rust
// domain/keys/mod.rs

pub mod text {
    use super::FieldPath;

    pub const PROMPT_USER: FieldPath = FieldPath::new("text.prompt.user");
    pub const PROMPT_SYSTEM: FieldPath = FieldPath::new("text.prompt.system");
    pub const PROMPT_PREVIOUS: FieldPath = FieldPath::new("text.prompt.previous");

    pub const TOKENS_MAX: FieldPath = FieldPath::new("text.tokens.max");

    pub const SAMPLING_TEMPERATURE: FieldPath = FieldPath::new("text.sampling.temperature");
    pub const SAMPLING_TOP_P: FieldPath = FieldPath::new("text.sampling.top_p");
    pub const SAMPLING_TOP_K: FieldPath = FieldPath::new("text.sampling.top_k");
    pub const SAMPLING_SEED: FieldPath = FieldPath::new("text.sampling.seed");

    pub const STOP_SEQUENCES: FieldPath = FieldPath::new("text.stop.sequences");

    pub const TOOLS_DEFINITIONS: FieldPath = FieldPath::new("text.tools.definitions");
    pub const TOOLS_CHOICE: FieldPath = FieldPath::new("text.tools.choice");

    pub const FORMAT_RESPONSE: FieldPath = FieldPath::new("text.format.response");

    pub const STREAM: FieldPath = FieldPath::new("text.stream");

    // For text.translate
    pub const BODY: FieldPath = FieldPath::new("text.body");
    pub const LANGUAGE_SOURCE: FieldPath = FieldPath::new("text.language.source");
    pub const LANGUAGE_TARGET: FieldPath = FieldPath::new("text.language.target");

    // For text.embed
    pub const INPUT: FieldPath = FieldPath::new("text.input");
    pub const DIMENSIONS: FieldPath = FieldPath::new("text.dimensions");

    // For text.rerank
    pub const QUERY: FieldPath = FieldPath::new("text.query");
    pub const DOCUMENTS: FieldPath = FieldPath::new("text.documents");
    pub const RESULTS_TOP_K: FieldPath = FieldPath::new("text.results.top_k");

    // Output keys
    pub const RESPONSE: FieldPath = FieldPath::new("text.response");
    pub const FINISH_REASON: FieldPath = FieldPath::new("text.finish_reason");
    pub const TOOL_CALLS: FieldPath = FieldPath::new("text.tool_calls");
    pub const TRANSLATED: FieldPath = FieldPath::new("text.translated");
    pub const DETECTED_LANGUAGE: FieldPath = FieldPath::new("text.detected_language");
    pub const EMBEDDINGS: FieldPath = FieldPath::new("text.embeddings");
    pub const SEGMENTS: FieldPath = FieldPath::new("text.segments");
    pub const LANGUAGE: FieldPath = FieldPath::new("text.language");
    pub const MEDIA_ID: FieldPath = FieldPath::new("text.media_id");

    pub mod values {
        pub const FINISH_REASON_STOP: &str = "stop";
        pub const FINISH_REASON_LENGTH: &str = "length";
        pub const FINISH_REASON_TOOLS: &str = "tool_calls";
        pub const FINISH_REASON_CONTENT_FILTER: &str = "content_filter";
    }
}

pub mod image {
    use super::FieldPath;

    pub const SOURCE: FieldPath = FieldPath::new("image.source");
    pub const MASK: FieldPath = FieldPath::new("image.mask");

    pub const PROMPT_POSITIVE: FieldPath = FieldPath::new("image.prompt.positive");
    pub const PROMPT_NEGATIVE: FieldPath = FieldPath::new("image.prompt.negative");

    pub const DIMENSIONS_WIDTH: FieldPath = FieldPath::new("image.dimensions.width");
    pub const DIMENSIONS_HEIGHT: FieldPath = FieldPath::new("image.dimensions.height");
    pub const DIMENSIONS_ASPECT: FieldPath = FieldPath::new("image.dimensions.aspect");

    pub const SAMPLING_STEPS: FieldPath = FieldPath::new("image.sampling.steps");
    pub const SAMPLING_SEED: FieldPath = FieldPath::new("image.sampling.seed");
    pub const SAMPLING_GUIDANCE: FieldPath = FieldPath::new("image.sampling.guidance");

    pub const STYLE_PRESET: FieldPath = FieldPath::new("image.style.preset");
    pub const STYLE_QUALITY: FieldPath = FieldPath::new("image.style.quality");

    pub const SCALE: FieldPath = FieldPath::new("image.scale");

    // Output keys
    pub const MEDIA_ID: FieldPath = FieldPath::new("image.media_id");
    pub const WIDTH: FieldPath = FieldPath::new("image.width");
    pub const HEIGHT: FieldPath = FieldPath::new("image.height");
    pub const SEED: FieldPath = FieldPath::new("image.seed");
    pub const MODEL: FieldPath = FieldPath::new("image.model");
}

pub mod audio {
    use super::FieldPath;

    pub const SOURCE: FieldPath = FieldPath::new("audio.source");
    pub const TEXT: FieldPath = FieldPath::new("audio.text");

    pub const VOICE_ID: FieldPath = FieldPath::new("audio.voice.id");
    pub const VOICE_STYLE: FieldPath = FieldPath::new("audio.voice.style");
    pub const VOICE_SPEED: FieldPath = FieldPath::new("audio.voice.speed");

    pub const LANGUAGE_SOURCE: FieldPath = FieldPath::new("audio.language.source");

    pub const FORMAT_CODEC: FieldPath = FieldPath::new("audio.format.codec");
    pub const FORMAT_SAMPLE_RATE: FieldPath = FieldPath::new("audio.format.sample_rate");

    // Output keys
    pub const MEDIA_ID: FieldPath = FieldPath::new("audio.media_id");
    pub const DURATION_MS: FieldPath = FieldPath::new("audio.duration_ms");
    pub const FORMAT: FieldPath = FieldPath::new("audio.format");
    pub const SAMPLE_RATE: FieldPath = FieldPath::new("audio.sample_rate");
}

pub mod usage {
    use super::FieldPath;

    pub const TOKENS_INPUT: FieldPath = FieldPath::new("usage.tokens.input");
    pub const TOKENS_OUTPUT: FieldPath = FieldPath::new("usage.tokens.output");
    pub const TOKENS_TOTAL: FieldPath = FieldPath::new("usage.tokens.total");
    pub const CHARACTERS: FieldPath = FieldPath::new("usage.characters");
    pub const BYTES_INPUT: FieldPath = FieldPath::new("usage.bytes.input");
    pub const BYTES_OUTPUT: FieldPath = FieldPath::new("usage.bytes.output");
    pub const COST_USD: FieldPath = FieldPath::new("usage.cost_usd");
}

pub mod timing {
    use super::FieldPath;

    pub const ROUTING_MS: FieldPath = FieldPath::new("timing.routing_ms");
    pub const QUEUE_MS: FieldPath = FieldPath::new("timing.queue_ms");
    pub const INFERENCE_MS: FieldPath = FieldPath::new("timing.inference_ms");
    pub const TOTAL_MS: FieldPath = FieldPath::new("timing.total_ms");
}

pub mod job {
    use super::FieldPath;

    pub const ID: FieldPath = FieldPath::new("job.id");
    pub const STATUS: FieldPath = FieldPath::new("job.status");
    pub const ETA_SECONDS: FieldPath = FieldPath::new("job.eta_seconds");
    pub const PROGRESS_CURRENT: FieldPath = FieldPath::new("job.progress.current");
    pub const PROGRESS_TOTAL: FieldPath = FieldPath::new("job.progress.total");
    pub const PROGRESS_LABEL: FieldPath = FieldPath::new("job.progress.label");

    pub mod values {
        pub const STATUS_QUEUED: &str = "queued";
        pub const STATUS_RUNNING: &str = "running";
        pub const STATUS_DONE: &str = "done";
        pub const STATUS_FAILED: &str = "failed";
        pub const STATUS_CANCELLED: &str = "cancelled";
    }
}

pub mod stream {
    use super::FieldPath;

    pub const CHUNK: FieldPath = FieldPath::new("stream.chunk");
    pub const SEQUENCE: FieldPath = FieldPath::new("stream.sequence");
    pub const TOTAL_CHUNKS: FieldPath = FieldPath::new("stream.total_chunks");
}

pub mod meta {
    use super::FieldPath;

    pub const CORRELATION_ID: FieldPath = FieldPath::new("meta.correlation_id");
    pub const REQUEST_ID: FieldPath = FieldPath::new("meta.request_id");
    pub const ACTION: FieldPath = FieldPath::new("meta.action");
    pub const PROVIDER: FieldPath = FieldPath::new("meta.provider");
    pub const MODEL: FieldPath = FieldPath::new("meta.model");
    pub const MODE: FieldPath = FieldPath::new("meta.mode");
    pub const IDEMPOTENT: FieldPath = FieldPath::new("meta.idempotent");
    pub const RESOLUTION_PATH: FieldPath = FieldPath::new("meta.resolution.path");
    pub const REQUESTED_PROVIDER: FieldPath = FieldPath::new("meta.resolution.requested_provider");
    pub const REQUESTED_MODEL: FieldPath = FieldPath::new("meta.resolution.requested_model");
    pub const IGNORED_FIELDS: FieldPath = FieldPath::new("meta.ignored_fields");

    pub mod values {
        pub const MODE_SYNC: &str = "sync";
        pub const MODE_ASYNC: &str = "async";
        pub const MODE_STREAM: &str = "stream";
    }
}
```

This module is the entire vocabulary of the system. Every reference to a canonical key in code goes through these constants. Compiler-enforced, grep-able, refactor-safe.

### Vocabulary specifications

Each primitive has a vocabulary spec in `domain/vocabulary/{primitive}.rs`. Example:

```rust
// domain/vocabulary/text_chat.rs

use crate::domain::keys::{text, usage};
use crate::domain::vocabulary::{Vocabulary, IoSchema, FieldSpec, FieldType, Alias, AliasCondition};
use crate::domain::primitive::Primitive;

pub fn text_chat() -> Vocabulary {
    Vocabulary {
        primitive: Primitive::TextChat,

        input: IoSchema {
            required: vec![
                FieldSpec {
                    path: text::PROMPT_USER,
                    field_type: FieldType::String,
                    description: "The user's message or current turn in the conversation.",
                },
            ],
            optional: vec![
                FieldSpec {
                    path: text::PROMPT_SYSTEM,
                    field_type: FieldType::String,
                    description: "System prompt providing persona or instructions.",
                },
                FieldSpec {
                    path: text::PROMPT_PREVIOUS,
                    field_type: FieldType::MessageHistory,
                    description: "Prior conversation turns as an array of {user, assistant} pairs.",
                },
                FieldSpec {
                    path: text::TOKENS_MAX,
                    field_type: FieldType::Integer { min: Some(1), max: Some(200_000) },
                    description: "Maximum output length in tokens.",
                },
                FieldSpec {
                    path: text::SAMPLING_TEMPERATURE,
                    field_type: FieldType::Number { min: Some(0.0), max: Some(2.0) },
                    description: "Sampling temperature controlling randomness.",
                },
                FieldSpec {
                    path: text::SAMPLING_TOP_P,
                    field_type: FieldType::Number { min: Some(0.0), max: Some(1.0) },
                    description: "Nucleus sampling probability threshold.",
                },
                FieldSpec {
                    path: text::SAMPLING_TOP_K,
                    field_type: FieldType::Integer { min: Some(1), max: None },
                    description: "Top-K sampling — keep the K highest-probability tokens.",
                },
                FieldSpec {
                    path: text::SAMPLING_SEED,
                    field_type: FieldType::Integer { min: None, max: None },
                    description: "Random seed for deterministic sampling.",
                },
                FieldSpec {
                    path: text::STOP_SEQUENCES,
                    field_type: FieldType::Array,
                    description: "Array of strings that end generation when seen.",
                },
                FieldSpec {
                    path: text::TOOLS_DEFINITIONS,
                    field_type: FieldType::Array,
                    description: "Tool/function definitions for function calling.",
                },
                FieldSpec {
                    path: text::TOOLS_CHOICE,
                    field_type: FieldType::String,
                    description: "Tool choice strategy: 'auto', 'required', or a specific tool name.",
                },
                FieldSpec {
                    path: text::FORMAT_RESPONSE,
                    field_type: FieldType::String,
                    description: "Response format hint: 'text' or 'json'.",
                },
                FieldSpec {
                    path: text::STREAM,
                    field_type: FieldType::Boolean,
                    description: "Request streaming delivery of tokens.",
                },
            ],
            aliases: vec![
                // {"prompt": "hi"} → text.prompt.user = "hi"
                Alias {
                    from: FieldPath::new("prompt"),
                    to: text::PROMPT_USER,
                    condition: AliasCondition::WhenString,
                },
                // Flat convenience aliases
                Alias {
                    from: FieldPath::new("temperature"),
                    to: text::SAMPLING_TEMPERATURE,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("max_tokens"),
                    to: text::TOKENS_MAX,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("top_p"),
                    to: text::SAMPLING_TOP_P,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("stop"),
                    to: text::STOP_SEQUENCES,
                    condition: AliasCondition::Always,
                },
                Alias {
                    from: FieldPath::new("tools"),
                    to: text::TOOLS_DEFINITIONS,
                    condition: AliasCondition::Always,
                },
                // OpenAI-shape compatibility: decompose `messages: [...]`
                // into prompt.user + prompt.system + prompt.previous in one
                // transformation. This is the only multi-target alias in v1.
                Alias {
                    from: FieldPath::new("messages"),
                    to: text::PROMPT_USER,  // primary target; decomposer also
                                             // populates PROMPT_SYSTEM and
                                             // PROMPT_PREVIOUS from the array
                    condition: AliasCondition::MessagesDecomposer,
                },
            ],
            shared_namespaces: vec![SharedNamespace::Usage, SharedNamespace::Timing, SharedNamespace::Meta],
        },

        output: IoSchema {
            required: vec![],
            optional: vec![
                FieldSpec {
                    path: text::RESPONSE,
                    field_type: FieldType::String,
                    description: "The assistant's reply text.",
                },
                FieldSpec {
                    path: text::FINISH_REASON,
                    field_type: FieldType::String,
                    description: "Why generation stopped: 'stop', 'length', 'tool_calls', 'content_filter'.",
                },
                FieldSpec {
                    path: text::TOOL_CALLS,
                    field_type: FieldType::Array,
                    description: "Tool calls the model wants to make.",
                },
                FieldSpec {
                    path: text::MEDIA_ID,
                    field_type: FieldType::String,
                    description: "Media ID for the full response (populated for streaming or archive-mode calls).",
                },
                FieldSpec {
                    path: usage::TOKENS_INPUT,
                    field_type: FieldType::Integer { min: Some(0), max: None },
                    description: "Input token count.",
                },
                FieldSpec {
                    path: usage::TOKENS_OUTPUT,
                    field_type: FieldType::Integer { min: Some(0), max: None },
                    description: "Output token count.",
                },
            ],
            aliases: vec![],
            shared_namespaces: vec![SharedNamespace::Usage, SharedNamespace::Timing, SharedNamespace::Meta],
        },
    }
}
```

One file per primitive (10 files). Each one is a constant spec compiled into the binary. The full set is constructed at startup into a `VocabularyRegistry`.

### Shared namespaces

Cross-cutting concerns have their own namespaces declared once and referenced by primitives:

- **`usage.*`** — token counts, byte counts, cost accounting. Used by every primitive that has billable usage.
- **`timing.*`** — routing, queue, inference, total timings. Used by every primitive.
- **`meta.*`** — correlation ID, provider, model, resolution path. Used by every response.
- **`job.*`** — async job reference fields. Used in `Async` and `Streaming` outcomes.
- **`stream.*`** — streaming delta envelope fields. Used in streaming outcomes.

A primitive's vocabulary declares which shared namespaces it opts into. The validator treats keys from opted-in namespaces as known.

### The canonical payload shape (user-facing)

**Callers write nested JSON. The orchestrator normalizes and canonicalizes. Providers receive a canonical payload.**

Minimal `text.chat`:
```json
{"prompt": "Hi!"}
```
expands via aliases to:
```json
{
  "text": {
    "prompt": {"user": "Hi!"}
  }
}
```

Full `text.chat` with all the trimmings:
```json
{
  "text": {
    "prompt": {
      "user": "What color is the sky in the evening?",
      "system": "You are a helpful assistant.",
      "previous": [
        {"user": "hi", "assistant": "Hello! How can I help?"},
        {"user": "I'm curious about physics", "assistant": "Sure, what topic?"}
      ]
    },
    "sampling": {
      "temperature": 0.7,
      "top_p": 0.95,
      "seed": 42
    },
    "tokens": {
      "max": 500
    },
    "stream": false
  }
}
```

`text.translate`:
```json
{
  "text": {
    "body": "Monsieur!",
    "language": {
      "source": "fr",
      "target": "en-US"
    }
  }
}
```

`text.embed`:
```json
{
  "text": {
    "input": ["first passage", "second passage"]
  }
}
```

`text.rerank`:
```json
{
  "text": {
    "query": "which cities are in Europe?",
    "documents": ["Paris", "Tokyo", "Berlin", "Sydney"],
    "results": {"top_k": 2}
  }
}
```

`image.generate` with full controls:
```json
{
  "image": {
    "prompt": {
      "positive": "a serene mountain landscape at sunrise",
      "negative": "blurry, lowres"
    },
    "dimensions": {"width": 1024, "height": 768},
    "sampling": {"steps": 30, "seed": 12345, "guidance": 7.5},
    "style": {"quality": "high"}
  }
}
```

`image.edit` (inpaint):
```json
{
  "image": {
    "source": {"media_id": "01JA7X-..."},
    "mask": {"media_id": "01JA7Y-..."},
    "prompt": {"positive": "a small cat"},
    "sampling": {"steps": 20}
  }
}
```

`image.upscale`:
```json
{
  "image": {
    "source": {"media_id": "01JA7X-..."},
    "scale": 4
  }
}
```

`image.analyze`:
```json
{
  "image": {
    "source": {"media_id": "01JA7X-..."}
  },
  "text": {
    "prompt": {"user": "What's in this image?"}
  }
}
```

`audio.generate` (text-to-speech):
```json
{
  "audio": {
    "text": "Hello, world!",
    "voice": {"id": "en-us-female-1", "speed": 1.0},
    "format": {"codec": "mp3", "sample_rate": 24000}
  }
}
```

`audio.transcribe`:
```json
{
  "audio": {
    "source": {"media_id": "01JA7Z-..."},
    "language": {"source": "en"}
  }
}
```

### Canonical response shape (user-facing)

Outputs are nested JSON, mirroring inputs. Providers populate what they produce.

`text.chat` completion:
```json
{
  "output": {
    "text": {
      "response": "Hello! How can I help?",
      "finish_reason": "stop"
    },
    "usage": {"tokens": {"input": 12, "output": 8}},
    "timing": {"total_ms": 340}
  },
  "_meta": {
    "correlation_id": "req-abc-123",
    "request_id": "01JA7Z-...",
    "action": "text.chat",
    "provider": "ollama",
    "model": "deepseek-r1:8b",
    "mode": "sync",
    "resolution": {
      "path": "recommended:chat → deepseek-r1:8b → ollama"
    }
  }
}
```

`image.generate` completion:
```json
{
  "output": {
    "image": {
      "media_id": "01JA7Z-...",
      "width": 1024,
      "height": 768,
      "seed": 12345,
      "model": "sd-xl-1.0"
    },
    "timing": {"total_ms": 8200}
  },
  "_meta": {
    "action": "image.generate",
    "provider": "comfyui",
    "mode": "sync"
  }
}
```

`audio.generate` streaming (initial announcement):
```json
{
  "output": {
    "audio": {
      "media_id": "01JA7Y-...",
      "format": "mp3",
      "sample_rate": 24000
    },
    "job": {"id": "01JA7X-...", "status": "running"}
  },
  "_meta": {
    "action": "audio.generate",
    "provider": "kokoro",
    "mode": "stream"
  }
}
```

followed by SSE events carrying chunk deltas. Final SSE event carries the completed metadata. The pre-announced `audio.media_id` is already valid — a client that missed the stream can fetch the full audio from `/v1/media/01JA7Y-...` after the stream closes.

### Error responses

Errors use the same envelope with an `error` object instead of `output`:

```json
{
  "error": {
    "code": "validation_failed",
    "message": "Field 'text.sampling.temperature' must be between 0 and 1 when using provider 'anthropic' (got 1.8)",
    "details": {
      "field": "text.sampling.temperature",
      "provided": 1.8,
      "constraint": {"min": 0, "max": 1},
      "provider": "anthropic"
    }
  },
  "_meta": {
    "correlation_id": "req-abc-123",
    "request_id": "01JA7Z-...",
    "action": "text.chat",
    "mode": "sync"
  }
}
```

Error codes are a stable taxonomy:

- `validation_failed` — request violated input vocabulary or provider narrowing. HTTP 400.
- `constraint_unsatisfied` — zone constraint, budget, or similar filtered all candidates. HTTP 400.
- `not_found` — action, skill, model, media, job, or provider does not exist. HTTP 404.
- `no_candidates` — no provider is registered for this primitive. HTTP 503.
- `provider_unreachable` — network or health failure talking to the provider. HTTP 503.
- `provider_overloaded` — provider returned a busy signal. HTTP 503.
- `auth_failed` — cloud provider rejected credentials. HTTP 502.
- `rate_limited` — caller hit a rate limit. HTTP 429.
- `quota_exhausted` — caller hit a configured quota. HTTP 429.
- `timeout` — exceeded a time budget. HTTP 504.
- `idempotency_conflict` — same idempotency key with different request content. HTTP 422.
- `upstream_error` — provider returned an unclassifiable failure. HTTP 502.
- `internal_error` — orchestrator bug. HTTP 500.

Each code is a typed constant in `domain/errors.rs`:
```rust
pub const VALIDATION_FAILED: &str = "validation_failed";
pub const NOT_FOUND: &str = "not_found";
// ... etc
```

#### Actionable error messages

Every error response must satisfy three requirements:

1. **`error.code`** is a stable programmatic identifier (the taxonomy above). Clients key off `code`, never off `message`.
2. **`error.message`** is a human-readable English string naming at least one of: the offending field path, the provider, or the constraint that failed. A message like `"validation failed"` or `"internal error"` is a bug; messages always point at *something specific* the caller can act on.
3. **`error.details`** is a structured object with fields appropriate to the error code. The shape is stable per code — clients parsing `validation_failed` know to look for `details.field` and `details.provided`; clients parsing `provider_unreachable` know to look for `details.provider` and `details.reason`.

The principle: the user reading an error should be able to take a next action without opening the orchestrator source code. If the only way to diagnose a failure is to grep logs, the error response has failed its job.

Localization is a client concern. `error.code` is the stable token to translate against; `error.message` is English for debugging; `error.details` carries the structured data for localized rendering.

### The pipeline, stage by stage

#### Stage 1: HTTP ingress

Two entry points:
- **`POST /v1/do`** — universal dispatcher. Request body has an `action` field.
- **`POST /v1/{modality}/{primitive}[/{skill}]`** — hierarchical sugar. Action is pre-filled from the URL.

Both entry points construct the same `OrchestratorRequest` and hand it to the same executor. Hierarchical URLs are syntactic sugar over `/v1/do`.

HTTP ingress responsibilities:
1. Parse URL and method.
2. Extract headers: `X-Correlation-Id`, `Idempotency-Key`, `traceparent`.
3. Parse body as JSON.
4. Build the initial `OrchestratorRequest`:
   - Fresh `RequestId` (GUIDv7).
   - `CorrelationId` from header or synthesized.
   - `Action` parsed from URL path or body's `action` field.
   - `payload` = body value (unvalidated).
   - `selectors` extracted from top-level fields in body (provider, model, skill).
   - `constraints` extracted from body.
   - `context` populated with media store handle, job sink (if async), cancellation token.
5. Hand to the dispatcher.

No vocabulary validation yet — that happens in contextualization. Ingress is the thinnest possible layer.

#### Stage 2: Contextualizer

The Contextualizer takes the raw request and runs a series of enrichment passes, each of which is a pure function over the request and the current Directory snapshot:

```rust
impl Contextualizer {
    pub async fn resolve(
        &self,
        mut request: OrchestratorRequest,
    ) -> Result<OrchestratorRequest, ContextError> {
        let snapshot = self.directory.snapshot();

        // Pass 1: validate the action exists
        self.validate_action(&request, &snapshot)?;

        // Pass 2: normalize the payload (apply aliases, flatten shortcuts)
        request.payload = self.normalize_payload(&request, &snapshot)?;

        // Pass 3: validate the payload against the input vocabulary (Layer 1)
        self.validate_input(&request, &snapshot)?;

        // Pass 4: extract media references from the payload
        request.media = self.extract_media(&request).await?;

        // Pass 5: resolve the model hint (recommended:* → concrete)
        self.resolve_model(&mut request, &snapshot)?;

        // Pass 6: resolve the provider (via model lookup or skill lookup)
        self.resolve_provider(&mut request, &snapshot)?;

        // Pass 7: validate the provider's narrowing (Layer 2 — range clamps, required narrowings)
        self.validate_provider_narrowing(&request, &snapshot)?;

        // Pass 8: validate zone constraint against the resolved provider
        self.validate_constraints(&request, &snapshot)?;

        Ok(request)
    }
}
```

Each pass is unit-testable in isolation with a mocked directory snapshot. Each pass produces clear error messages pointing at the exact field that failed.

**Pass 2 (normalize_payload)** applies the primitive's aliases. Examples:
- `{"prompt": "Hi!"}` → `{"text": {"prompt": {"user": "Hi!"}}}`
- `{"temperature": 0.7}` → `{"text": {"sampling": {"temperature": 0.7}}}`
- `{"max_tokens": 500}` → `{"text": {"tokens": {"max": 500}}}`

Aliases compose: a caller can send `{"prompt": "Hi!", "temperature": 0.7, "max_tokens": 500}` and all three aliases fire independently. Collisions (caller sends both an alias and the canonical form with different values) are rejected with `validation_failed`.

**Pass 3 (validate_input)** walks the canonical payload, checks every key against the input vocabulary, validates types, validates ranges, and rejects unknown fields unless prefixed with `x_` (provider-specific escape hatches).

**Pass 5 (resolve_model)** handles `recommended:*` resolution via the RecommendationEngine:
- `recommended:chat` → looks up the pinned/scored model for `text.chat` → returns a concrete model identifier.
- `recommended:vision` → same for `image.analyze`.
- A concrete model name (`"deepseek-r1:8b"`) passes through unchanged.
- A bare primitive with no model → resolves to `recommended:{primitive.short_name}`.

**Pass 6 (resolve_provider)** finds the target provider:
- If `action.skill` is present → skill lookup in Directory → provider.
- Else if `selectors.model` is present → model lookup in Directory → provider.
- Else → look up all providers registered for the primitive, pick the default (operator-pinned) or the first bare-registered.
- Validates that caller-supplied `selectors.provider` matches the resolved provider; conflict = `validation_failed`.

**Pass 7 (validate_provider_narrowing)** applies the chosen provider's `honored_fields` constraints. If the caller sent `temperature: 1.8` and the chosen provider narrows temperature to `[0, 1]`, this is where the rejection happens, with a clear message naming both the provider and the constraint.

Post-contextualization, the request carries: resolved provider, resolved model, validated canonical payload, extracted media references. Ready for media resolution.

#### Stage 3: Media resolver

For each media reference in the request, apply the chosen provider's declared `MediaDelivery` mode for the corresponding field:

```rust
impl MediaResolver {
    pub async fn resolve(
        &self,
        mut request: OrchestratorRequest,
    ) -> Result<OrchestratorRequest, MediaError> {
        let snapshot = self.directory.snapshot();
        let provider_name = request.resolved_provider.as_ref().unwrap();
        let registration = snapshot
            .find_registration(provider_name, request.action.primitive, request.action.skill.as_ref())?;

        // For each media input the registration declares, find the corresponding
        // reference in the request and apply the declared delivery mode.
        for spec in &registration.media_inputs {
            if let Some(media_ref) = request.media.find_at_field(&spec.field) {
                // Validate content type against accepted types
                if !spec.accepted_types.iter().any(|t| content_type_matches(t, &media_ref.content_type)) {
                    return Err(MediaError::ContentTypeMismatch { /* ... */ });
                }

                // Apply delivery mode. Three modes in v1.
                let resolved = match spec.delivery {
                    MediaDelivery::ById => {
                        // Payload unchanged. Provider will fetch bytes itself
                        // via ctx.media_store from its own onboard method.
                        ResolvedMedia::ById
                    }
                    MediaDelivery::Base64 => {
                        // Fetch bytes now, base64-encode, rewrite the payload
                        // to replace {media_id: "..."} with {base64, ...}.
                        self.resolve_base64(&media_ref, &mut request.payload, &request.context).await?
                    }
                    MediaDelivery::Transfer => {
                        // Do nothing here. The provider handles staging
                        // inside its onboard method after picking an instance.
                        ResolvedMedia::DeferredToProvider
                    }
                };

                request.media.resolutions.insert(media_ref.id.clone(), resolved);
                request.context.media_store.touch(&media_ref.id).await?;  // refresh TTL
            }
        }

        Ok(request)
    }
}
```

The resolver uses a **per-request resolution cache** to deduplicate work: if the same `media_id` appears twice (pipeline steps, parallel references), it's resolved once per request. The cache lives on `request.media.resolutions` and is scoped to the request's lifetime.

**Touch is a global operation.** Every time the contextualizer or media resolver validates or reads a media reference, it calls `media_store.touch(id)` to refresh the TTL. The media store's `touch()` is cheap (a single timestamp update) and idempotent. Any component holding a `MediaStore` handle can touch a media_id; there is no authority restriction.

**For `Transfer` mode, the provider handles staging inside `onboard`.** This is critical: the resolver validates the media reference (content type, existence, metadata) but does not move bytes, because it doesn't know which instance will run the request. The provider's `onboard` implementation picks an instance using its own internal logic, then calls `ctx.media_store.transfer_to(id, target)` with a target bound to that specific instance. This keeps the staged file and the executing instance always in agreement — no race where the resolver stages to one instance and the provider's load balancer picks another.

#### Stage 4: Dispatcher

Ten lines:

```rust
impl Dispatcher {
    pub async fn dispatch(
        &self,
        raw: OrchestratorRequest,
    ) -> Result<DispatchResult, DispatchError> {
        // 1. Contextualize
        let request = self.contextualizer.resolve(raw).await?;

        // 2. Check idempotency cache
        if let Some(cached) = self.check_idempotency(&request).await? {
            return Ok(cached.into());
        }

        // 3. Resolve media
        let request = self.media_resolver.resolve(request).await?;

        // 4. Look up provider
        let provider_name = request.resolved_provider.clone().unwrap();
        let provider = self.directory.provider(&provider_name)
            .ok_or(DispatchError::ProviderNotFound(provider_name))?;

        // 5. Hand off
        let outcome = provider.onboard(request.clone()).await?;

        // 6. Cache for idempotency (if key was provided)
        self.store_idempotency(&request, &outcome).await?;

        Ok(DispatchResult::from(outcome, request))
    }
}
```

No per-primitive logic. No instance selection. No fallback. The dispatcher is a coordinator: it runs the pipeline, hands off, and returns the result.

#### Stage 5: Provider execution (`Provider::onboard`)

The provider takes custody of the request and does whatever it needs to do. It owns:
- Instance selection (if it has multiple instances).
- Load balancing (if it's model-oriented with many models on many instances).
- Busy semantics (sync, queue, async job, refuse).
- Protocol translation (canonical → vendor wire format).
- Response construction (vendor response → canonical `Output`).

A provider's `onboard` implementation is the only place that knows about its vendor's quirks. Example skeleton for Ollama:

```rust
impl Provider for OllamaProvider {
    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        // Pick an instance (Ollama's private logic — fitness, queue depth, model placement)
        let instance = self.select_instance(&request).await?;

        // Translate the canonical payload to Ollama's wire format
        let wire_request = self.translate_request(&request, &instance).await?;

        // Call Ollama's API
        let wire_response = self.call_ollama(&instance, wire_request).await?;

        // Translate Ollama's response back to canonical Output
        let mut output = Output::new();
        output.set(&keys::text::RESPONSE, &wire_response.message.content);
        output.set(&keys::text::FINISH_REASON, &wire_response.done_reason);
        output.set(&keys::usage::TOKENS_INPUT, wire_response.prompt_eval_count);
        output.set(&keys::usage::TOKENS_OUTPUT, wire_response.eval_count);
        output.set(&keys::timing::INFERENCE_MS, wire_response.total_duration_ms);

        Ok(ProviderOutcome::Sync(output))
    }
}
```

The provider can return `Sync`, `Async`, or `Streaming` based on its own state. A busy provider returns `Async` with a job id; a streaming-capable provider returns `Streaming` with the tee to media handled internally; a sync provider returns `Sync` with the complete output.

### Media model

#### MediaStore trait

```rust
pub trait MediaStore: Send + Sync + 'static {
    /// Store bytes atomically. Content-addressed: same SHA-512 returns
    /// the same MediaId regardless of caller.
    async fn put(
        &self,
        bytes: Bytes,
        content_type: String,
        source: MediaSource,
    ) -> Result<MediaEntry, MediaError>;

    /// Open a streaming writer. The caller receives a MediaSink that
    /// accepts chunks and allocates the MediaId at open time. Used by
    /// providers producing media progressively (streaming audio, image
    /// workflow output).
    async fn open_writer(
        &self,
        content_type: String,
        source: MediaSource,
    ) -> Result<MediaSink, MediaError>;

    async fn get_bytes(&self, id: &MediaId) -> Result<Bytes, MediaError>;
    async fn get_metadata(&self, id: &MediaId) -> Result<MediaEntry, MediaError>;
    async fn delete(&self, id: &MediaId) -> Result<(), MediaError>;

    /// Refresh the TTL for this media. Cheap; called at every validation point.
    async fn touch(&self, id: &MediaId) -> Result<(), MediaError>;

    async fn list(&self, filter: MediaFilter) -> Result<Vec<MediaEntry>, MediaError>;

    /// Transfer bytes to a provider-specified target. The target shape
    /// determines how the bytes are delivered (multipart HTTP upload,
    /// shared filesystem path, in-memory buffer).
    async fn transfer_to(
        &self,
        id: &MediaId,
        target: TransferTarget,
    ) -> Result<TransferHandle, MediaError>;

    /// Bind media to a job or pipeline. While reserved, the media is
    /// protected from GC for up to 30 days regardless of touch activity.
    /// Released automatically when the job terminates or is cancelled.
    async fn reserve(
        &self,
        id: &MediaId,
        reservation: MediaReservation,
    ) -> Result<(), MediaError>;

    async fn release_reservation(
        &self,
        id: &MediaId,
        job_id: &JobId,
    ) -> Result<(), MediaError>;

    /// Bulk delete by filter. Operator-level operation.
    async fn flush(&self, filter: MediaFilter) -> Result<FlushReport, MediaError>;
}

pub struct MediaSink {
    media_id: MediaId,  // pre-allocated at open() time
}

impl MediaSink {
    pub fn media_id(&self) -> &MediaId;
    pub async fn write(&self, chunk: Bytes) -> Result<(), MediaError>;
    pub async fn close(self) -> Result<MediaEntry, MediaError>;
    pub async fn abort(self);
}

pub struct MediaEntry {
    pub id: MediaId,                  // GUIDv7
    pub content_hash: String,         // sha512:...
    pub content_type: String,
    pub size_bytes: u64,
    pub metadata: Value,              // width, height, duration, etc.
    pub source: MediaSource,
    pub lifecycle: MediaLifecycle,
    pub created_at: DateTime<Utc>,
}

pub enum MediaLifecycle {
    /// Default state. TTL clock runs; touch refreshes.
    Active { expires_at: DateTime<Utc> },

    /// Bound to an in-flight job or pipeline. Protected from GC until
    /// the binding is released (job completes/cancels) or the reservation
    /// window elapses, whichever comes first.
    Reserved {
        expires_at: DateTime<Utc>,  // 30 days from reservation
        reservation: MediaReservation,
    },
}

pub struct MediaReservation {
    pub job_id: Option<JobId>,
    pub reason: String,
}

pub struct MediaSource {
    pub kind: MediaSourceKind,
    pub provider: Option<ProviderName>,
    pub action: Option<String>,
    pub origin_request_id: Option<RequestId>,
}

pub enum MediaSourceKind {
    Uploaded,      // caller POSTed it
    Generated,     // produced by a provider
}
```

#### Content addressing

Media is keyed by GUIDv7 but deduplicated by SHA-512: the same bytes uploaded twice return the same `media_id`. Uploads are idempotent by content — a caller retrying an upload gets the existing handle without a second store.

#### Lifecycle: Active vs Reserved

Media lives in one of two lifecycle states:

**Active** — default state for uploaded and generated media. TTL default is 24 hours from last touch. Touching the media (validation, fetch, reference in a new request, transfer) resets the expiration to `now + 24h`. Media in active use never expires; media sitting idle expires after a day.

**Reserved** — bound to an in-flight job, pipeline, or other long-running work. When a provider creates a job that references a media_id, the orchestrator transitions the media to `Reserved` with a 30-day retention window. This protects against the case where a long-running workflow (e.g., a ComfyUI batch) is still processing media that the Active TTL would have expired. Reserved media ignores touch activity; its lifetime is driven exclusively by the reservation's lifecycle.

When the owning job reaches a terminal state (done, failed, cancelled), the media store releases the reservation and returns the media to `Active` with a fresh 24h TTL. If the 30-day window elapses while the job is still running, the GC task extends the reservation as long as the job remains active — a long-running workflow implicitly keeps its inputs alive.

**Touch is a global operation.** Any component that references a media_id may call `touch()` — the contextualizer, the media resolver, HTTP handlers serving `/v1/media/{id}`, providers that fetch bytes. There is no authority restriction; touch is cheap and idempotent.

#### Delivery modes

```rust
pub enum MediaDelivery {
    /// Provider reads the media_id and calls the media store itself to fetch
    /// bytes or transfer them. Used by ComfyUI (transfer to instance upload
    /// endpoint) and Whisper (multipart upload to inference endpoint).
    ById,

    /// Orchestrator fetches bytes, base64-encodes, and substitutes
    /// {base64, content_type, size_bytes} into the canonical payload.
    /// Used by OpenAI, Anthropic, Google, Ollama vision models.
    Base64,

    /// Provider stages the media to one of its own instances before execution.
    /// The provider's internal logic calls media_store.transfer_to() and
    /// handles the resulting TransferHandle.
    Transfer,
}
```

Three modes. The resolver applies the mode based on the provider's declared preference for each media field:

- **ById**: payload unchanged; the provider uses the MediaStore handle from the ExecutionContext to fetch or transfer bytes on its own schedule. Most flexible; gives the provider full control over when the bytes are pulled.
- **Base64**: the resolver fetches bytes, encodes, and rewrites `{media_id: "..."}` to `{base64: "...", content_type: "...", size_bytes: N}` before the provider's `onboard` is called. Simplest for providers that just embed base64 in their wire format.
- **Transfer**: the provider handles staging internally inside its `onboard`. The resolver validates the media reference (content type, existence) but does not move bytes. The provider calls `media_store.transfer_to(id, target)` after picking an instance, so the target and the chosen instance always agree.

**Signed URL delivery is out of scope for v1.** Providers currently needing URL-based delivery for media (some cloud APIs for very large files) fall back to `Base64` with the tradeoff that large media hits base64 overhead. A future ADR may add `Url` delivery alongside tunneling support when the orchestrator is reachable from outside the pond.

#### Transfer API

```rust
pub enum TransferTarget {
    /// Multipart HTTP upload. Provider specifies the form field name
    /// for the file; additional form fields are constructed by the
    /// provider itself after the transfer returns.
    HttpUpload {
        endpoint: String,
        field_name: String,
    },

    /// Raw HTTP POST with the bytes as the body.
    HttpPost {
        endpoint: String,
        content_type: String,
    },

    /// Place at a filesystem path (for providers with shared volumes).
    SharedPath {
        directory: PathBuf,
        filename: Option<String>,  // default: {media_id}.{ext}
    },

    /// Return the bytes as an in-memory buffer. Used by providers that
    /// construct their own wire request and want the orchestrator to
    /// fetch without writing to disk.
    InMemory,
}

pub struct TransferHandle {
    /// Opaque provider-specific reference (filename, upload id, etc.).
    pub reference: String,
    /// Which instance the transfer is bound to. The provider MUST route
    /// the subsequent execution to the same instance.
    pub instance_fqn: String,
    /// When this handle stops being valid on the target instance.
    pub expires_at: DateTime<Utc>,
}
```

The provider calls `media_store.transfer_to(id, target)` inside its own `onboard` method, after it has picked the instance that will run the request. This ordering guarantees that the transfer target and the chosen instance agree — the media is staged to the same instance that will consume it.

**TransferTarget carries only what the media store needs to push bytes.** Complex multipart bodies with many fields (Whisper with `model`, `language`, `response_format` alongside the file) are not expressed as `extra_fields` on the transfer target. Instead, the provider fetches bytes via `media_store.get_bytes` (or uses `TransferTarget::InMemory`), constructs its own multipart body with all the fields it needs, and posts directly. `HttpUpload` is reserved for the simple case where the target accepts a single file field and nothing else.

**The handle is opaque to the orchestrator.** `reference` is whatever string the provider needs to address the staged bytes on the chosen instance — a filename for ComfyUI, an upload ID for a resumable endpoint, whatever fits the wire format. The orchestrator never parses it.

#### Flush operations

Three scopes, three endpoints:

- **`POST /v1/providers/{name}/flush`** — flush one provider's instance caches. Operator-scoped. Each provider's `flush_caches` is called; default impl is a no-op.
- **`POST /v1/providers/flush`** — flush all providers.
- **`POST /v1/media/flush?filter=...`** — flush the orchestrator's media store (dangerous; filter required to avoid accidental full wipes).

Flush is non-destructive to in-flight work: only idle cache entries are cleared. A provider mid-request with staged files completes the request; cleanup happens afterward.

#### Media endpoints

- **`POST /v1/media`** — upload. Body is raw binary. Returns metadata JSON with `media_id`.
- **`GET /v1/media/{id}`** — download raw bytes with `Content-Type` from the stored media.
- **`HEAD /v1/media/{id}`** — metadata in headers only.
- **`GET /v1/media/{id}/metadata`** — metadata JSON.
- **`DELETE /v1/media/{id}`** — delete (cascades to provider caches).
- **`GET /v1/media`** — list with filters (`source`, `provider`, `content_type_prefix`, `created_before`).

The mental model: `/v1/media/{id}` *is* the media. A browser pasting that URL sees the image. Metadata is a sub-resource.

### Job model

Jobs represent tracked units of work with observable progress. They are **ephemeral operational state**, not a permanent history. A job record exists for as long as someone observing the system has a reason to care about it. When it stops being useful, it goes away. Operators who need long-term history use metrics scraping and log aggregation, not job queries.

Jobs are not exclusively for API async requests. Any tracked unit of work in the orchestrator — async inference, a provider's background workflow import, a benchmark run, a media GC sweep — is a job. The dashboard reads the job store to show "what's happening right now" and "what happened recently," regardless of source.

```rust
pub struct Job {
    pub id: JobId,                   // GUIDv7
    pub correlation_id: CorrelationId,
    pub category: JobCategory,
    pub owner: ProviderName,         // empty for orchestrator-internal jobs
    pub action: Option<Action>,      // present for API-initiated work
    pub state: JobState,
    pub progress: Option<Progress>,
    pub eta_seconds: Option<u64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub terminal_at: Option<DateTime<Utc>>,
    pub metadata: Value,
    pub result: Option<Output>,      // populated on Done
    pub error: Option<Value>,        // populated on Failed
}

pub enum JobCategory {
    /// API-initiated async request (ProviderOutcome::Async).
    Api,
    /// Provider-initiated background work (skill import, model pull, benchmark run).
    Provider,
    /// Orchestrator-initiated maintenance (GC sweep, directory refresh).
    Background,
}

pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

pub struct Progress {
    pub current: u64,
    pub total: Option<u64>,
    pub label: Option<String>,
}
```

Endpoints:
- **`GET /v1/jobs/{id}`** — metadata + status. Cheap; safe to poll. Returns `410 Gone` if the job has been evicted from the store.
- **`GET /v1/jobs/{id}/result`** — the completed output. `409 Conflict` if the job is not yet terminal. `410 Gone` if the job has been evicted.
- **`DELETE /v1/jobs/{id}`** — cancel. The provider receives cancellation via the request's `CancellationToken`; hard termination is best-effort.
- **`GET /v1/jobs`** — list recent jobs, filterable by `category`, `state`, `owner`, `action`. Used by dashboards.

#### Job lifetime semantics

A job's lifetime is driven by its usefulness to observers, not by an arbitrary timer or ring-buffer cap:

**Running jobs** persist until they reach a terminal state (`Done`, `Failed`, `Cancelled`). They are never evicted while running, regardless of age or store pressure.

**Terminal jobs** persist until either:
- Their result has been observed by the caller (via `GET /v1/jobs/{id}/result` or equivalent), **and** a short grace period has elapsed to accommodate retries and out-of-band polling; **or**
- A reasonable total lifetime has elapsed since they became terminal, to cap leakage from callers that never fetch.

Whichever comes first. The specific grace period and maximum lifetime are tunable operational parameters — explicitly **not** part of this ADR's public contract. What the ADR commits to is the **principle**: the orchestrator retains job records long enough to be useful for their intended observers, and no longer. Callers that need permanent records rely on metrics and logs, not on querying the JobStore after the fact.

Storage is in-memory for v1. Persistence and replication are future concerns.

#### JobSink

Providers update jobs via a `JobSink` handed to them in the execution context. The sink is always present (the dispatcher pre-creates a job record before every `onboard` call, per the state ownership table). Providers returning `Sync` ignore the sink and the record is marked `Done` inline. Providers returning `Async` or `Streaming` use the sink to publish updates as work progresses.

```rust
pub struct JobSink {
    job_id: JobId,
}

impl JobSink {
    pub fn job_id(&self) -> &JobId;
    pub async fn update_state(&self, state: JobState);
    pub async fn update_progress(&self, progress: Progress);
    pub async fn update_eta(&self, eta: Duration);
    pub async fn complete(&self, result: Output);
    pub async fn fail(&self, error: ProviderError);
}
```

When a provider returns `ProviderOutcome::Async(output)`, the `output` must contain `job.id` matching the sink's job id. The dispatcher verifies this invariant — a mismatch is an internal error — and returns `202 Accepted` with the job reference in the body.

#### Idempotency interaction

When an idempotent request produces an async outcome, the idempotency cache stores **only the job reference**, not the result. A retry of the same idempotent request returns the same `job_id`; clients poll `/v1/jobs/{id}/result` for the actual output. The JobStore remains the sole source of truth for async results, avoiding the consistency trap of the same result living in two stores.

### Idempotency

- **Header**: `Idempotency-Key` on POST requests.
- **Cache key**: SHA-256 over `(header_value || action.dotted() || canonicalized_payload_json || canonicalized_selectors_json)`. The payload is canonicalized *after* the Contextualizer's normalization pass, so two semantically-equal requests sent in different alias forms produce the same key. Selectors are part of the key so `{provider: "ollama"}` and `{provider: "anthropic"}` with the same payload are distinct.
- **Conflict behavior**: a key reused with a different canonical hash is a conflict. Response is `422 Unprocessable Entity` with `code: idempotency_conflict` and details identifying the differing fields.
- **Cache hit (sync)**: return the cached `Output` with `_meta.idempotent: true`. Cached entry holds the full sync response.
- **Cache hit (async)**: the cache holds only a **job reference**, not the result. A repeated async request with the same key returns the same `job_id`; clients poll `/v1/jobs/{id}/result` as normal. The JobStore is the sole source of truth for async results.
- **Streaming bypass**: requests that result in `ProviderOutcome::Streaming` do not populate the idempotency cache. Streams are inherently one-shot; a retry opens a new stream. Clients wanting idempotent streaming behavior should use async-batch instead.
- **Scope**: pond-wide. No per-consumer partitioning; the pond is the trust boundary.
- **Lifetime**: idempotency cache entries are ephemeral operational state (see State ownership section). They live long enough to catch realistic retry windows and no longer; the specific eviction policy is an implementation detail tunable per deployment.

### Streaming

When a provider returns `ProviderOutcome::Streaming { initial, stream }`:

1. Orchestrator responds with `Content-Type: text/event-stream` and `Transfer-Encoding: chunked`.
2. First SSE event is `event: initial`, `data: {initial Output as JSON}`. This carries any pre-allocated `media_id`, the `job.id`, and any known metadata the provider has at stream-open time.
3. Subsequent SSE events are `event: delta`, `data: {Output as JSON}` for each chunk the provider yields.
4. Final SSE event is `event: done`, `data: {final Output with aggregated usage/timing}`.
5. A terminal error is delivered as `event: error`, `data: {error envelope}`, and the stream closes.

**Streaming and media.** The provider is the tee. A provider that wants the stream's bytes to also land in the media store opens a `MediaSink` via `ctx.media_store.open_writer()` at the start of its work, writes each chunk's bytes to both the sink and the outgoing stream, and closes the sink on completion. The sink's `media_id` is published in the `initial` output so clients that prefer fetching over streaming can use `/v1/media/{id}` once the stream closes. The orchestrator does not do any automatic teeing; it's the provider's responsibility and it's explicit in the provider's code.

**Stream reconnection is out of scope for v1.** Clients should treat dropped streaming connections as fatal and either re-issue the request (possibly with a fresh idempotency key since streams bypass the cache) or fetch the completed media from the store if the provider pre-allocated a `media_id`.

### Discovery endpoints

Three discovery endpoints, ordered from quickest-to-read to most detailed:

#### `GET /v1/` — sitemap

A first-time caller hitting the root of the versioned surface gets a minimal sitemap pointing at the rest:

```json
GET /v1/
{
  "actions": "/v1/do",
  "catalog": "/v1/catalog",
  "media": "/v1/media",
  "jobs": "/v1/jobs",
  "health": "/health"
}
```

One small JSON blob. Cheap to serve, cheap to parse, answers "where do I go next?" A curl-wielding user with nothing else to go on lands here and clicks through.

#### `GET /v1/do` — action list with examples and setup hints

The dispatcher endpoint is dual-purpose. `POST /v1/do` invokes an action; `GET /v1/do` returns an index of every action the orchestrator can currently serve, with minimal working examples and setup hints.

```json
GET /v1/do
{
  "actions": [
    {
      "action": "text.chat",
      "url": "/v1/text/chat",
      "summary": "Conversational text completion with optional tool calling.",
      "required": ["text.prompt.user"],
      "providers": ["ollama", "anthropic", "openai", "google"],
      "example": {
        "prompt": "Hi!"
      }
    },
    {
      "action": "text.translate",
      "url": "/v1/text/translate",
      "summary": "Translate text from one language to another.",
      "required": ["text.body", "text.language.target"],
      "providers": ["libretranslate", "google"],
      "example": {
        "text": {
          "body": "Hello",
          "language": {"target": "ja"}
        }
      }
    },
    {
      "action": "image.generate.outpaint",
      "url": "/v1/image/generate/outpaint",
      "summary": "Extend an image beyond its borders.",
      "required": ["image.source", "image.prompt.positive"],
      "providers": ["comfyui"],
      "example": {
        "image": {
          "source": {"media_id": "01JA7X-..."},
          "prompt": {"positive": "continue the landscape to the left"}
        }
      }
    }
    // ... one entry per primitive + every registered skill
  ],
  "status": {
    "providers_registered": 10,
    "providers_healthy": 9,
    "providers_degraded": 1,
    "actions_available": 10,
    "models_discovered": 38
  },
  "setup": {
    "hints": [
      "Provider 'anthropic' is degraded: missing API key. Set ANTHROPIC_API_KEY or configure via POST /v1/providers/anthropic/config."
    ]
  }
}
```

The `example` field is the killer feature. Every entry is a ready-to-run JSON body the caller can copy into a curl request and send. Examples use the canonical nested shape with convenience aliases where they improve clarity (e.g., `{"prompt": "Hi!"}` instead of `{"text": {"prompt": {"user": "Hi!"}}}` for the simplest `text.chat` case). Examples are written once in the vocabulary spec files and rendered into this response by the `catalog_builder` task.

The `status` block is a tiny health summary. It lets a first-time user see at a glance whether the orchestrator has providers and whether they're working.

The `setup.hints` block is **only present when something needs attention**. An orchestrator with everything working omits it. When providers are degraded or offline, or when no provider serves a primitive the system would normally offer, the hints point the operator at the next action. Examples:
- `"Provider 'anthropic' is degraded: missing API key. Set ANTHROPIC_API_KEY..."`
- `"No vision-capable model discovered. Pull one via: ollama pull gemma3:12b"`
- `"No provider serves text.translate. Install LibreTranslate via..."`

Hints are built by the `catalog_builder` task from the Directory snapshot. Providers in `ProviderHealth::Degraded { reason }` contribute a hint that includes their reason string. Missing primitives contribute a hint suggesting the smallest provider install that would fix it. Hint generation is a static table lookup keyed on (health state, missing primitive) — no heuristics.

#### `GET /v1/catalog` — full catalog

The catalog is the full machine-readable view of the orchestrator's current capabilities. Dashboards and SDKs that want every field use it; humans exploring the API usually stop at `GET /v1/do`.

`GET /v1/catalog` returns a live snapshot of what the orchestrator can do right now. It's built from the Directory's snapshot (which is itself a cached projection with a monotonic version).

```json
{
  "version": 42,
  "updated_at": "2026-04-07T10:14:03Z",
  "primitives": [
    {
      "action": "text.chat",
      "modality": "text",
      "summary": "Conversational text completion with optional tool calling.",
      "vocabulary": {
        "required": [
          {"path": "text.prompt.user", "type": "string", "description": "..."}
        ],
        "optional": [
          {"path": "text.prompt.system", "type": "string", "description": "..."},
          {"path": "text.sampling.temperature", "type": "number", "min": 0, "max": 2, "description": "..."}
        ],
        "aliases": [
          {"from": "prompt", "to": "text.prompt.user", "when": "string"},
          {"from": "temperature", "to": "text.sampling.temperature", "when": "always"}
        ]
      },
      "providers": [
        {
          "name": "ollama",
          "honors": ["text.prompt.user", "text.prompt.system", "text.prompt.previous",
                     "text.sampling.temperature", "text.tokens.max", "..."],
          "constraints": {}
        },
        {
          "name": "anthropic",
          "honors": ["text.prompt.user", "text.prompt.system", "text.prompt.previous",
                     "text.sampling.temperature", "text.tokens.max", "text.tools.definitions", "..."],
          "constraints": {
            "text.sampling.temperature": {"min": 0, "max": 1},
            "text.tokens.max": {"required": true, "min": 1, "max": 200000}
          }
        }
      ],
      "recommended_model": "deepseek-r1:8b"
    }
  ],
  "skills": [
    {
      "action": "image.generate.outpaint",
      "primitive": "image.generate",
      "moniker": "outpaint",
      "display_name": "Outpainting",
      "description": "...",
      "provider": "comfyui"
    }
  ],
  "providers": [
    {"name": "ollama", "zone": "internal"},
    {"name": "comfyui", "zone": "internal"},
    {"name": "anthropic", "zone": "external"}
  ],
  "models": [
    {"id": "deepseek-r1:8b", "provider": "ollama", "capabilities": ["text.chat"]}
  ]
}
```

`GET /v1/catalog/events` is an SSE stream of catalog changes. Dashboards subscribe once and stay live.

Unimplemented primitives are **not in the catalog**. If a primitive has no provider registered, it does not appear. Callers querying `/v1/catalog` see only what actually works.

### Primitive-specific vocabularies (summary)

Full specs live in `domain/vocabulary/*.rs`. Summaries:

| Primitive | Required | Notable Optional |
|---|---|---|
| `text.chat` | `text.prompt.user` | `text.prompt.system`, `text.prompt.previous`, `text.sampling.*`, `text.tokens.max`, `text.tools.*`, `text.stream`, `text.format.response` |
| `text.translate` | `text.body`, `text.language.target` | `text.language.source`, `text.format.response` |
| `text.embed` | `text.input` (string or array) | `text.dimensions`, `text.format.encoding` |
| `text.rerank` | `text.query`, `text.documents` | `text.results.top_k`, `text.results.min_score` |
| `image.generate` | `image.prompt.positive` | `image.prompt.negative`, `image.dimensions.*`, `image.sampling.*`, `image.style.*` |
| `image.edit` | `image.prompt.positive`, `image.source` | `image.mask`, `image.sampling.*` |
| `image.upscale` | `image.source` | `image.scale` |
| `image.analyze` | `image.source` | `text.prompt.user` (question to ask), `text.format.response` |
| `audio.generate` | `audio.text` | `audio.voice.*`, `audio.format.*` |
| `audio.transcribe` | `audio.source` | `audio.language.source`, `text.format.response` |

Every optional field must be recognized by the vocabulary registry. Unknown fields (outside the vocabulary and not `x_`-prefixed) are rejected with `validation_failed`.

### Provider inventory (v1)

| Provider | Primitives | Notes |
|---|---|---|
| **Ollama** | `text.chat`, `text.embed`, `image.analyze` | Model-oriented. Media: Base64 for vision. Publishes performance hints via its internal benchmark runner. |
| **Anthropic** | `text.chat` | Model-oriented. Media: Base64 for vision. Narrows `text.tokens.max` to required. Clamps `text.sampling.temperature` to `[0,1]`. |
| **OpenAI** | `text.chat`, `text.embed`, `image.analyze`, `image.generate`, `audio.generate`, `audio.transcribe` | Model-oriented. Media: Base64 for chat vision; Transfer for Whisper and DALL-E edit. |
| **Google** | `text.chat`, `text.embed`, `text.translate`, `image.generate`, `image.analyze`, `audio.generate`, `audio.transcribe` | Model-oriented. Media: Base64 for all modes in v1 (URL delivery is deferred). |
| **LibreTranslate** | `text.translate` | Bare function. No media. |
| **Infinity** | `text.embed`, `text.rerank` | Bare function. No media. |
| **Docling** | `image.analyze` (OCR skill) | Bare function. Media: Transfer (multipart). |
| **ComfyUI** | `image.generate`, `image.edit`, `image.upscale` | Skill-oriented. Media: Transfer (HttpUpload to `/upload/image`). Publishes performance hints from workflow timings. |
| **Kokoro / OpenedaiSpeech** | `audio.generate` | Model-oriented (by voice id). |
| **WhisperCpp / Speaches** | `audio.transcribe` | Bare function. Media: Transfer (multipart). |

Video primitives are deferred until a video provider exists.

### Wipe list (break-and-rebuild scope)

The rebuild begins by deleting every module, type, and function below. Nothing is kept "for reference." Nothing is left behind as scaffolding. The deletion happens in the first commit of the rebuild branch, before any new code is written.

**HTTP / API layer**:
- `src/orchestrators/ai/src/api/` — the entire directory, every handler, every router, every middleware file. Rebuilt from scratch as `src/orchestrators/ai/src/http/` following this ADR.

**Domain layer**:
- `src/orchestrators/ai/src/domain/` — the entire directory. Replaced by a new `src/orchestrators/ai/src/domain/` built to the object model in this ADR. No selective preservation: all aggregates, services, value objects, and enums go.

**Catalog / provider-registry layer**:
- `src/orchestrators/ai/src/catalog/` — the entire directory, including the current `Provider` trait, `ProviderContext`, `ProviderRegistry`, and every `inference::*` request/response type. A new `Provider` trait is defined per this ADR.

**Provider adapters — preserved content, deleted wrappers**:
- `src/orchestrators/ai/src/providers/*.rs` — each file's current `impl Provider for XProvider` block is deleted along with any references to the old `Provider` trait methods (`infer`, `infer_stream`, `embed`, `speak`, `transcribe`, `workflow`, `form_schema`, `probe`, `enumerate`, `builtin_skills`, `check_skill_readiness`, `provision_skill`, `vram_estimate`). What stays inside each file: the private vendor client code (HTTP helpers, request/response type mappings, authentication logic). Each adapter is then rewrapped with a single `impl Provider for XProvider` block implementing the new 5-method trait defined in this ADR.

**Offerings layer**:
- `src/orchestrators/ai/src/offerings/*/client.rs` and `types.rs` files are **retained** as private vendor client code consumed by the rewrapped providers. No changes to their internals.
- `src/orchestrators/ai/src/offerings/*/mod.rs` is **rebuilt** to export only what the new provider wrappers import, nothing more.

**Tasks layer**:
- `src/orchestrators/ai/src/tasks/` — audited file by file. Tasks that directly depend on deleted domain/catalog types are deleted. Tasks that implement ORCH-0025 skill persistence or ORCH-0026 vision-assisted naming are rewritten to call the new Directory aggregate.

**Skills layer**:
- `src/orchestrators/ai/src/skills/` — the on-disk persistence (ORCH-0025) and the vision-assisted naming (ORCH-0026) stay. The in-memory representation is rebuilt to match the new `Registration` / `Moniker` types. Any code referencing the old `SkillsDomain` or `SkillDefinition` types is rewritten.

**Global state**:
- Every `static OnceLock<Arc<T>>` in the current codebase is deleted. All state moves to fields on `AppState`.
- Every use of `lazy_static!` for mutable state is deleted.

**Test suite**:
- `docs/decisions/ORCH-0027/test-suite.sh` is deleted along with any other inherited test scripts. A new suite is written that exercises every primitive end-to-end, with zero skips.
- Any existing Rust unit tests under the wiped modules are deleted (they test code that no longer exists).

**Configuration**:
- Any configuration fields that reference deleted concepts (`Capability`, old routing tables, per-primitive feature flags) are removed from `OrchestratorConfig`.

**Documentation under `docs/decisions/ORCH-0027/`**:
- Deleted. The ADR itself (`docs/decisions/ORCH-0027-api-surface-v2.md`) is left as a historical record but marked superseded in its frontmatter.

### What the rebuild carries forward

Only these, and only in the forms specified:

- **Vendor HTTP client code** (`offerings/*/client.rs`, `offerings/*/types.rs`): the request-construction and response-parsing logic for each vendor's native API. Each client is consumed privately by its corresponding rewrapped provider.
- **ORCH-0025 disk persistence layout**: the `{data_dir}/skills/{provider}/{moniker}/` directory structure and file format. A new loader populates the Directory at startup; the loader is newly written.
- **ORCH-0026 vision-assisted naming**: the existing thumbnail + vision-model pipeline that produces human-friendly skill names. Called during skill import; its output feeds the new `Moniker::new` validator.
- **ORCH-0011 `recommended:{capability}` moniker concept**: reimplemented in a new `RecommendationEngine` service that lives in the new `domain/recommendation.rs`.
- **Ollama fitness / benchmark / placement logic**: moved into the Ollama provider's private implementation as module-private code. Not promoted to any shared layer.

Everything else is deleted and rewritten.

### Rebuild sequence

1. **Wipe**: delete everything in the wipe list. The orchestrator binary will not compile after this step. That is expected.
2. **Foundation**: write `domain/keys/`, `domain/primitive.rs`, `domain/moniker.rs`, `domain/vocabulary/`, `domain/directory.rs`, `domain/request.rs`, `domain/output.rs`, `domain/provider.rs`, `domain/media.rs`, `domain/errors.rs`. Unit tests for every pure function and every value-object constructor.
3. **Services**: write `domain/contextualizer.rs`, `domain/media_resolver.rs`, `domain/dispatcher.rs`, `domain/recommendation.rs`. Unit tests with mocked Directory and mocked providers.
4. **Provider rewraps**: for each vendor in the provider inventory, write a new `providers/{vendor}.rs` containing the new `impl Provider for XProvider` block. Each wrapper consumes the retained `offerings/{vendor}/client.rs` code internally. End-to-end test against the live garden for every primitive the provider registers.
5. **HTTP layer**: write `http/ingress.rs` (the `/v1/do` + hierarchical sugar handlers), `http/media.rs`, `http/jobs.rs`, `http/catalog.rs`, `http/flush.rs`. Integration tests against a mocked Directory.
6. **Wire into `main.rs`**: instantiate the Directory, register every provider, construct the `AppState`, mount the HTTP routes, start serving.
7. **Test suite**: write the live test suite that exercises every primitive against the development garden. Zero skips. Every test asserts on real output, not status codes.
8. **CI guards**: add a CI step that greps for string literals matching the pattern of canonical keys (`"text\.[a-z]`, `"image\.[a-z]`, etc.) outside `domain/keys/`. Any match fails the build.

Each step must leave the previous step's output intact. No step reintroduces deleted code. The orchestrator will not boot until step 6; that is the point at which the new surface comes alive.

---

## Consequences

### What gets easier

- **Adding a provider**: implement the `Provider` trait, emit registrations. The dispatcher handles the rest. No changes to the core.
- **Adding a primitive**: add an enum variant, write a vocabulary spec file, declare which providers will implement it. No changes to the dispatcher.
- **Debugging routing**: `_meta.resolution.path` in every response tells the caller exactly how the provider was chosen.
- **Writing a client**: one request envelope, one response envelope, one error envelope. SDK generation is a straightforward walk of the vocabulary.
- **Pipelines**: a pipeline step's output is an `Output` map; the next step reads specific keys from it. Composition is field-level, not type-level.
- **Media handling**: providers declare their preference once; the orchestrator handles negotiation. Adding a new delivery mode is an enum variant plus a match arm in the resolver.

### What gets harder

- **Rust type safety on outputs**: the `Output` is untyped (`BTreeMap<String, Value>`). Providers must use the canonical key constants; forgetting a constant means using a string literal, which fails code review. Enforced by grep at CI time.
- **Vocabulary drift**: if a provider adds a new output field without updating the vocabulary, the field passes through with a warning but isn't in the catalog. Operators must keep the vocabulary in sync with provider implementations.
- **Provider autonomy**: each provider's internal code is now more complex (instance selection, load balancing, caching). The orchestrator is smaller; the providers are bigger. Total code is roughly the same but the distribution changes.

### What is locked

- The 10-primitive inventory. Adding a primitive requires an ADR amendment.
- The URL grammar: `POST /v1/do` for dispatch, `POST /v1/{modality}/{primitive}[/{skill}]` for hierarchical sugar.
- The Provider trait's five methods (`name`, `state`, `subscribe`, `onboard`, `flush_caches`). Adding a method is a breaking change across all providers.
- `ProviderState` as the single bundled state struct. All provider-exposed live state (health, registrations, models, performance hints) travels together through one `watch::channel`.
- The `Output` as a namespaced map. No per-primitive typed outputs.
- The `ProviderOutcome` enum: Sync, Async, Streaming. Three delivery modes, no more.
- The three `MediaDelivery` modes: ById, Base64, Transfer.
- All identity types (`RequestId`, `ResponseId`, `MediaId`, `JobId`, `RegistrationId`) are GUIDv7.
- Canonical field keys are Rust constants in `domain/keys/`. Magic strings in code are forbidden.
- State ownership: every mutable domain has one documented writer. See the State Ownership table.

### What is deferred

- Authentication (pond mTLS integration is a separate ADR).
- Budget / cost enforcement.
- Explain mode (verbose routing diagnostics including candidate-by-candidate scoring breakdown).
- Streaming pipelines with WebSocket transport (sync and async-batch pipelines ship in v1; streaming pipelines are deferred).
- Pipelines as a meta-primitive (`pipeline.run`). Callers in v1 compose primitives client-side by chaining requests. A future ADR may introduce pipelines with `Output` as the substrate.
- Skill CRUD via API (skills load from disk via ORCH-0025 in v1).
- Video primitives until a video provider exists.
- SSE stream reconnection (`Last-Event-ID` resumption). Clients treat dropped streams as fatal in v1.
- Signed-URL media delivery (`MediaDelivery::Url`). Providers requiring URL-based delivery fall back to `Base64` in v1. A future ADR revisits when pond-external tunneling is in scope.
- Demand-weighted advisor (ORCH-0009-style topology advisor feeding placement optimization). The request counter is accumulated in v1 but no decisions are made from it.
- Persistent idempotency / job / media storage. All stores are in-memory in v1.

---

## Acceptance criteria

The rebuild is complete when:

1. **Every primitive in the catalog executes end-to-end against at least one real provider in the live garden.** No skips, no stubs. The live test suite enumerates the catalog at startup and fails immediately if any primitive has no corresponding test.
2. **Every primitive test asserts on real output content**, not just status codes or presence of keys. A `text.chat` test verifies the response string contains non-whitespace tokens. An `image.generate` test verifies the returned `media_id` resolves to bytes with correct image dimensions. A `text.embed` test verifies the returned vector has the declared dimensionality and non-zero values.
3. **The test suite has zero skipped tests** except for explicitly manual tests (e.g., "provider unreachable" which requires shutting down a provider out-of-band).
4. **Unit tests cover every domain service with mocked collaborators.** The Contextualizer has per-pass unit tests with mocked Directory snapshots. The MediaResolver has tests for each delivery mode with mocked providers and media store. The Dispatcher has tests with mocked Directory and mocked providers returning each `ProviderOutcome` variant. These tests run in CI without the dev garden.
5. **Every canonical field key used in production code is a constant in `domain/keys/`.** A CI check (clippy custom lint or AST-based grep) fails the build on string literals matching canonical key patterns (`"text\.[a-z]`, `"image\.[a-z]`, etc.) when they appear as arguments to `Output::set`, `FieldPath::new`, or vocabulary builders outside `domain/keys/`. Test modules, doc comments, log format strings, error message `format!` calls, and deserialized user input are explicitly exempt.
6. **The Directory snapshot has a monotonic version that bumps only on real changes.** Back-to-back catalog queries return the same version when nothing changed. Verified by a test that calls `GET /v1/catalog` twice and asserts version equality.
7. **Idempotency cache keys include the request content hash after normalization and selectors.** A test verifies that reusing a key with different content returns `422 idempotency_conflict`. A separate test verifies that the same semantic request sent in two different alias forms (e.g., `{"prompt": "hi"}` and `{"text": {"prompt": {"user": "hi"}}}`) with the same key is a cache hit, not a conflict.
8. **Alias collisions are rejected.** For each alias in the text.chat vocabulary, a test verifies that sending both the alias form and the canonical form simultaneously — even with equal values — returns `validation_failed`.
9. **Vocabulary drift is audited.** A CI report captures, per provider, the output keys produced across a suite of test requests. Keys not in the corresponding primitive's output vocabulary are listed. Operators review the report periodically; it does not fail the build, but a long-standing drift is treated as a vocabulary bug.
10. **`Provider::onboard` is the only entry point for real work.** No `infer`, `embed`, `speak`, `transcribe`, `workflow`, or similar method exists on the Provider trait.
11. **No global `OnceLock<Arc<T>>` stores anywhere in the binary.** All mutable state lives on `AppState` fields and follows the State Ownership table.
12. **Every response envelope has a `_meta` block with `correlation_id`, `request_id`, `action`, `provider`, `mode`.** Verified by a test that inspects every response across the suite.
13. **Error responses carry actionable messages.** Every error response has a non-empty `error.message` naming at least one of: the offending field path, the provider, or the constraint that failed. A test iterates every error code and verifies the message format.
14. **Error taxonomy is conformance-tested.** For each of the 13 error codes, a test constructs the minimum request that produces the error and asserts the correct `error.code` and HTTP status.
15. **Media references are resolved according to the target provider's declared `MediaDelivery`.** Tests with mocked providers cover each of the three delivery modes (ById, Base64, Transfer).
16. **Vocabulary validation runs on every input.** Tested with requests containing unknown top-level fields, unknown nested fields, and `x_`-prefixed fields (passthrough). Responses are `400 validation_failed` with explicit field paths in the first two cases; passthrough in the third.
17. **Selector precedence rules are unit-tested.** A test exists for each precedence rule: skill pins provider/model; provider+model must agree; model-only lookup via Directory; provider-only with default model; explicit `recommended:*` resolution; bare primitive with no selectors.
18. **Dispatcher and hierarchical sugar produce byte-identical responses.** A test verifies that `POST /v1/do {action: "text.chat", ...}` and `POST /v1/text/chat {...}` with the same semantic content produce identical response bodies (modulo `_meta.request_id` and timings).
19. **The wipe list is honored in the first commit of the rebuild branch.** No file listed under "Wipe list" exists in its original form after the first commit.
20. **Operational state is truly ephemeral.** Tests verify that terminal jobs are eventually evicted, that expired-and-unreserved media is GC'd, and that idempotency cache entries don't accumulate without bound.

---

## Decisions locked

Every item in this ADR is a locked decision. There are no open questions.

- **10-primitive inventory**: `text.chat`, `text.translate`, `text.embed`, `text.rerank`, `image.generate`, `image.edit`, `image.upscale`, `image.analyze`, `audio.generate`, `audio.transcribe`. Video primitives reserved; not part of v1. Adding or removing a primitive requires an ADR amendment.
- **Field key organization**: `domain/keys/{text,image,audio,usage,timing,meta,job,stream}.rs`, one module per namespace, every constant declared once and imported where used.
- **Vocabulary spec format**: one Rust file per primitive under `domain/vocabulary/`, loaded at startup into a `VocabularyRegistry`. Each vocabulary includes a canonical `example` field used to render the `GET /v1/do` index.
- **Media delivery modes**: `ById`, `Base64`, `Transfer`. Three modes.
- **`ProviderOutcome` enum**: `Sync(Output)`, `Async(Output)`, `Streaming { initial, stream }`. No refused variant; refusals are `Err(ProviderError)`.
- **Provider trait shape**: five methods (`name`, `state`, `subscribe`, `onboard`, `flush_caches`). Live state is bundled in `ProviderState` published via a single `watch::channel`.
- **Break-and-rebuild**: the wipe list executes in the first commit. No shims, no backwards compatibility, no parallel operation.
- **`_meta` structure**: all fields declared as constants in `domain/keys/meta.rs`. Every response carries `_meta.correlation_id`, `_meta.request_id`, `_meta.action`, `_meta.provider`, `_meta.mode` at minimum.
- **State ownership**: every mutable domain has exactly one documented writer (see State Ownership table). Enforced by convention and code review; CI has no automated check, but the table is the authoritative contract.
- **Jobs are ephemeral**: no fixed TTL, no fixed ring-buffer cap. Job records live as long as they are useful to observers and are evicted when they stop being useful. Tunable per deployment, not part of the public contract.
- **Idempotency**: content-hash keys, 422 conflict on mismatch, async outcomes store only job references, streaming bypasses the cache.
- **GUIDv7** for every mutable identity (`RequestId`, `ResponseId`, `MediaId`, `JobId`, `RegistrationId`, `CorrelationId` when synthesized). Human-readable names for static identities (`ProviderName`, `Primitive`, `Moniker`).

---

## References

- [ORCH-0011 Recommended Model Monikers](ORCH-0011-recommended-model-monikers.md) — concept integrated into `RecommendationEngine`
- [ORCH-0015 Model Directory Architecture](ORCH-0015-model-directory-architecture.md) — concept integrated into `Directory` aggregate
- [ORCH-0025 Three-Tier Skill Persistence](ORCH-0025-three-tier-skill-persistence.md) — disk layout preserved, in-memory representation rebuilt
- [ORCH-0026 Vision-Assisted Skill Naming](ORCH-0026-vision-assisted-skill-naming.md) — feeds human-friendly monikers to the Directory
- [ARCH-0007 Common Scope Modernization](ARCH-0007-modernization.md)
- Code standards (`docs/code-standards.md`) — §1 (namespaces), §5 (domain ownership), §10 (typed errors), §17 (.unwrap discipline), §18 (validate at boundaries)
