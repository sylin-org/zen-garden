# ORCH-0030 R2 — M3 Trait-Switch Contract Spec

> **Status:** Working contract for M3 (the load-bearing milestone of M1).
> **Audience:** Adapter rewrite agents (M3.13–M3.21) and the
> contextualizer/dispatcher/media_resolver rewriters (M3.9–M3.12).
> **Authority:** This document is normative for M3. If anything in
> here conflicts with [`MILESTONE-1-PLAN.md`](./MILESTONE-1-PLAN.md),
> the plan wins; flag the conflict back to the integrator.

This spec is the contract every adapter and service must satisfy
after the M3 atomic commit lands. Adapter rewrite agents read this
document plus their adapter's existing source and produce a new
adapter file that implements the lean trait. The integrator
(main thread) wires everything together at the end.

---

## 1. The lean `Provider` trait

After M3, `domain/provider.rs` defines this and nothing else:

```rust
#[async_trait]
pub trait Provider: Send + Sync + 'static {
    /// Stable, compile-time provider identity.
    fn name(&self) -> ProviderName;

    /// Take custody of a request. The provider owns instance
    /// selection, model resolution, protocol translation, and
    /// response construction.
    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError>;

    /// Clear any artifacts cached on the provider's instances.
    /// Default is a no-op; providers that stage files override.
    async fn flush_caches(&self) -> Result<FlushReport, ProviderError> {
        Ok(FlushReport::empty())
    }
}
```

**Three methods. That is the entire trait.**

`ProviderState`, `state()`, `subscribe()`, `Registration`,
`HonoredField`, `MediaInputSpec`, `MediaOutputSpec`, `Model`,
`ModelDescriptor`, `ProviderHealth`, `PerformanceHint`,
`PerformanceVerdict`, `FieldRange`, `RegistrationStrategy`, and
`ProviderStatePublisher` are **all deleted** from `domain/provider.rs`.
Adapters that previously held a `ProviderStatePublisher` field
delete it.

The types that survive in `domain/provider.rs` after M3:

- `Provider` trait (above)
- `ProviderOutcome` enum (`Sync`, `Async`, `Streaming`)
- `ProviderError` enum (every existing variant **plus**
  `PinNotServable { model, reason }` — already added pre-M3)
- `FlushReport` struct

That's it. The file shrinks from 539 lines to roughly 130.

---

## 2. Capability publication: every adapter publishes events

Every adapter publishes a [`CapabilityAnnouncement`] to the
`EventBus` whenever its capability set changes.

**Topic:** `directory.provider.{name}.capabilities`

**Helper:** [`crate::services::directory_subscriber::publish_capability_announcement`]
takes `(events: &EventBus, announcement: &CapabilityAnnouncement)`
and writes to the correct topic. **Never** call `events.publish`
directly with a hand-formatted topic.

**Shape (already defined in `domain/capability_announcement.rs`):**

```rust
pub struct CapabilityAnnouncement {
    pub provider: ProviderName,
    pub enabled: bool,            // false ⇒ drop out of routing
    pub capabilities: Vec<Capability>,
    pub skills: Vec<SkillDeclaration>,
}

pub struct Capability {
    pub primitive: Primitive,
    pub media_inputs: Vec<CapabilityMediaInput>,
}

pub struct CapabilityMediaInput {
    pub field: String,                 // "image.source", "audio.source", ...
    pub delivery: MediaDelivery,       // ById | Base64 | Transfer
    pub accepted_types: Vec<String>,
    pub overlay: Option<String>,
}
```

**When to publish:**

1. At construction time, **after** the discovery subscriber has
   given the adapter its first instance list (or immediately for
   cloud adapters that have no discovery).
2. Whenever the adapter's instance pool gains or loses a healthy
   instance (publish a fresh full snapshot — no deltas).
3. Whenever the adapter's loaded skill set changes (only ComfyUI
   does this in M1).
4. **Idempotent re-publishing is allowed.** The subscriber emits a
   coarse `directory.provider.*.updated` event on every accepted
   announcement regardless of diff content; it does **not** echo
   redundant fine-grained derived events when nothing changed.

**`enabled` field semantics:**

- `enabled: true` ⇔ at least one healthy instance is currently
  serving traffic. The adapter is in the dispatcher's routing pool.
- `enabled: false` ⇔ no healthy instances. The adapter is not in
  the routing pool. The dispatcher will return `ProviderUnreachable`
  if the caller targets it explicitly, and skip it during automatic
  routing.

For cloud adapters with no discovery loop, `enabled: true` is
constant — the adapter assumes the cloud endpoint is reachable
until proven otherwise inside `onboard`, where it returns
`ProviderError::Unreachable` on failure.

---

## 3. CapabilityDirectory: the query API

`CapabilityDirectory` (in `services/directory_subscriber.rs`) is
the **single source of truth** for routing decisions after M3.
The legacy `Directory` aggregate is deleted in M3.6.

The existing query methods stay and one new method is added
(commit point in M3.9-M3.11):

```rust
impl CapabilityDirectory {
    // ── Existing (already shipped in commit 6) ────────────────
    pub async fn providers(&self) -> HashMap<ProviderName, ProviderCapabilities>;
    pub async fn provider(&self, name: &ProviderName) -> Option<ProviderCapabilities>;
    pub async fn providers_for_primitive(&self, primitive: Primitive) -> Vec<ProviderName>;
    pub async fn providers_for_skill(&self, primitive: Primitive, skill_id: &str) -> Vec<ProviderName>;
    pub async fn skill(&self, provider: &ProviderName, skill_id: &str) -> Option<SkillDeclaration>;
    pub async fn all_skills(&self) -> Vec<(ProviderName, SkillDeclaration)>;
    pub async fn provider_count(&self) -> usize;
    pub async fn enabled_provider_count(&self) -> usize;

    // ── NEW in M3 ────────────────────────────────────────────
    /// Look up the capability declaration (with media_inputs) that
    /// `provider` exposes for `primitive`. Returns `None` if the
    /// provider is not registered, is disabled, or does not declare
    /// the primitive.
    ///
    /// This is the primary lookup for the dispatcher and the
    /// media_resolver after M3 — the dispatcher uses it to confirm
    /// the chosen provider serves the requested primitive, and the
    /// media_resolver reads the returned `media_inputs` list to
    /// resolve every media reference in the request.
    pub async fn capability(
        &self,
        provider: &ProviderName,
        primitive: Primitive,
    ) -> Option<Capability>;
}
```

The new method is a thin wrapper:

```rust
pub async fn capability(
    &self,
    provider: &ProviderName,
    primitive: Primitive,
) -> Option<Capability> {
    let state = self.providers.read().await;
    state
        .get(provider)
        .filter(|p| p.enabled)
        .and_then(|p| {
            p.announcement
                .capabilities
                .iter()
                .find(|c| c.primitive == primitive)
                .cloned()
        })
}
```

---

## 4. Service rewrites (M3.9–M3.12)

### 4.1 Contextualizer (M3.9)

`services/contextualizer.rs` reads **only** `CapabilityDirectory`
after M3. The legacy `Directory` parameter is deleted from the
constructor and from the `contextualize` method signature.

**Old signature:**
```rust
pub async fn contextualize(
    &self,
    request: OrchestratorRequest,
    directory: &Arc<Directory>,
) -> Result<OrchestratorRequest, OrchestratorError>;
```

**New signature:**
```rust
pub async fn contextualize(
    &self,
    request: OrchestratorRequest,
    directory: &Arc<CapabilityDirectory>,
) -> Result<OrchestratorRequest, OrchestratorError>;
```

**Behavioral changes:**

- Provider resolution: `directory.providers_for_primitive(primitive)`
  (or `providers_for_skill` when `request.action.skill` is set)
  returns a `Vec<ProviderName>`. The contextualizer picks the
  first one (M1 has no preferences/locality routing — that's
  R2.5 commit 12, deferred per the M0 plan).
- The contextualizer does **not** resolve models. Model
  resolution is now adapter-local — every adapter reads
  `request.selectors.model` inside `onboard`.
- Vocabulary validation against the chosen provider's honored
  fields **stays** in the contextualizer, but it validates against
  the vocabulary's full field type rather than provider-specific
  narrowings (provider narrowings are deleted with `HonoredField`).
  In other words: every honored field is validated against its
  vocabulary `FieldType`, full stop.
- If `directory.providers_for_*` returns an empty `Vec`, the
  contextualizer returns `ErrorCode::NotFound` with
  "no provider serves `{primitive}`" / "no provider serves skill
  `{skill_id}` for `{primitive}`".

### 4.2 MediaResolver (M3.10)

`services/media_resolver.rs` reads `CapabilityDirectory.capability()`
after M3, replacing the legacy `snapshot.find_registration` path.

**New implementation pattern:**

```rust
pub async fn resolve(
    &self,
    mut request: OrchestratorRequest,
    directory: &Arc<CapabilityDirectory>,
) -> Result<OrchestratorRequest, OrchestratorError> {
    let Some(provider_name) = request.resolved_provider.as_ref().cloned() else {
        return Ok(request);
    };
    let Some(capability) = directory
        .capability(&provider_name, request.action.primitive)
        .await
    else {
        return Ok(request);
    };

    // Build the field → CapabilityMediaInput map.
    let spec_for_field: HashMap<String, &CapabilityMediaInput> = capability
        .media_inputs
        .iter()
        .map(|spec| (spec.field.clone(), spec))
        .collect();

    // ... existing resolution loop, but reading `delivery`,
    //     `accepted_types`, `overlay` from CapabilityMediaInput
    //     instead of MediaInputSpec.
}
```

The `inline_base64_into_payload` helper and `content_type_matches`
helper stay unchanged — they don't depend on the registration
type.

### 4.3 Dispatcher (M3.11)

`services/dispatcher.rs` is the heart of the request flow. The
new constructor takes `CapabilityDirectory` + `ProviderRegistry`
instead of `Directory`.

**Old constructor:**
```rust
pub fn new(
    directory: Arc<Directory>,
    contextualizer: Arc<Contextualizer>,
    media_resolver: Arc<MediaResolver>,
    idempotency_store: Arc<dyn IdempotencyStore>,
    demand: Arc<DemandLedger>,
    job_store: Arc<dyn JobStore>,
    media_store: SharedMediaStore,
) -> Self;
```

**New constructor:**
```rust
pub fn new(
    capability_directory: Arc<CapabilityDirectory>,
    provider_registry: Arc<ProviderRegistry>,
    contextualizer: Arc<Contextualizer>,
    media_resolver: Arc<MediaResolver>,
    idempotency_store: Arc<dyn IdempotencyStore>,
    job_store: Arc<dyn JobStore>,
    media_store: SharedMediaStore,
) -> Self;
```

**`DemandLedger` is deleted** as part of M3.6 (it was an internal
detail of the recommendation engine). If the dispatcher referenced
it for demand recording, that code is removed wholesale — there
is no demand ledger after M3.

**`do_dispatch` flow (post-M3):**

1. `contextualizer.contextualize(request, &capability_directory).await?`
2. Idempotency cache lookup (unchanged)
3. `media_resolver.resolve(request, &capability_directory).await?`
4. Look up the provider handle:
   `provider_registry.get(&request.resolved_provider).await`
   → returns `Option<Arc<dyn Provider>>`. None ⇒ `ProviderUnreachable`.
5. `provider.onboard(request).await`
6. Same outcome handling as today (Sync / Async / Streaming).

The dispatcher no longer touches `provider.state()` — that method
no longer exists. All routing decisions are made via
`CapabilityDirectory` queries; all dispatch is `provider_registry.get`
followed by `onboard`.

### 4.4 Catalog builder (M3.12)

`services/catalog_builder.rs` rebuilds when the
`CapabilityDirectory` version bumps. The trigger source switches
from "`Directory::on_snapshot()` watch channel" to "EventBus
subscription on `directory.provider.*.updated`".

**Implementation pattern:**

```rust
pub async fn run(self: Arc<Self>, shutdown: CancellationToken) {
    let mut events_rx = self.events.raw_subscribe();
    // Render once at startup so /v1/catalog has something to
    // serve before the first event arrives.
    self.rebuild().await;
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            result = events_rx.recv() => {
                match result {
                    Ok(event) => {
                        if event.topic.starts_with("directory.provider.")
                            && event.topic.ends_with(".updated")
                        {
                            self.rebuild().await;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "catalog_builder lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        }
    }
}
```

`rebuild()` walks `capability_directory.providers()` and
`capability_directory.all_skills()`, joining them with the
vocabulary registry to produce the JSON documents `/v1/catalog`
and `/v1/do` serve. The Skills aggregate (`Skills` in
`services/skills/registry.rs`) is **deleted** in M3.6 — ComfyUI
owns its skill state internally and announces it via the bus.

---

## 5. Per-adapter rewrite checklist

Every M1 adapter (Ollama, ComfyUI, WhisperCpp, Speaches, Kokoro,
Docling, LibreTranslate, Google/Gemini, OpenedaiSpeech) must
satisfy this checklist after M3.

### 5.1 What to delete from existing adapters

- The `state(&self) -> Arc<ProviderState>` method.
- The `subscribe(&self) -> watch::Receiver<Arc<ProviderState>>` method.
- The `publisher: ProviderStatePublisher` field (and its
  construction).
- Any `ProviderHealth`, `Registration`, `HonoredField`,
  `MediaInputSpec`, `MediaOutputSpec`, `Model`, `ModelDescriptor`
  imports — all deleted from `domain/provider.rs`.
- Calls to `self.publisher.publish(...)` and
  `self.publisher.modify(...)` — replaced by
  `publish_capability_announcement(&self.events, &announcement).await`.

### 5.2 What to add

- A `events: Arc<EventBus>` field (already present on adapters
  that ship in commit 6+, like Ollama).
- An `async fn publish_capabilities(&self)` method that builds a
  `CapabilityAnnouncement` from the adapter's current state and
  calls `publish_capability_announcement`.
- Calls to `self.publish_capabilities().await` at every state
  transition where the legacy adapter called `self.publisher.modify`.

### 5.3 The `Provider` impl shrinks to:

```rust
#[async_trait]
impl Provider for MyProvider {
    fn name(&self) -> ProviderName {
        self.name.clone()
    }

    async fn onboard(
        &self,
        request: OrchestratorRequest,
    ) -> Result<ProviderOutcome, ProviderError> {
        // Existing dispatch logic, with model resolution moved
        // INSIDE this method (read request.selectors.model).
        // ...
    }

    // flush_caches: keep the existing override if the adapter
    // had one; otherwise omit (uses the trait default).
}
```

### 5.4 Constructor signature

Every adapter constructor takes:

```rust
pub fn new(
    config: MyConfig,
    discovery: Arc<GardenDiscovery>,    // omit for cloud adapters
    events: Arc<EventBus>,
    shutdown: CancellationToken,
) -> Arc<Self>;
```

Cloud adapters (Google in M1) skip `discovery` and publish their
fixed capability list once at construction.

### 5.5 Model resolution (adapter-local)

Adapters that resolve models read `request.selectors.model`
inside `onboard`. The two cases:

1. **`recommended:*` moniker** — adapter consults its own
   capability matrix / static list to pick the best fit. For
   Ollama this is the existing `OllamaSelector::pick_recommended`
   path. For static-list cloud adapters this is the
   `cloud_common::resolve_cloud_model` helper (added in M3.8).
2. **Concrete model name** — adapter validates the model is
   currently servable (for instance-pool adapters: at least one
   healthy instance has it; for cloud adapters: the name appears
   in the static list). On failure, return
   `ProviderError::PinNotServable { model, reason }`.

If `selectors.model` is absent:

- Static-list cloud adapters fall back to their default model
  for the requested primitive.
- Instance-pool adapters with a recommendation matrix
  (Ollama) compute the default capability for the primitive and
  call `pick_recommended`.
- Adapters with no concept of "model" (ComfyUI, OpenedaiSpeech,
  LibreTranslate) ignore the field entirely.

### 5.6 ComfyUI-specific (M3.14)

ComfyUI is the most complex rewrite because it owns the skills
subsystem after M3 (the `Skills` aggregate is deleted).

- Drop `skills_aggregate: Arc<Skills>` field; the adapter holds
  its `LoadedSkill` map directly.
- The provisioning queue + cache + moss_volume modules stay; they
  are still consumed by the discovery subscriber's
  `readiness_pass`.
- `publish_capabilities` constructs a `CapabilityAnnouncement`
  whose `skills: Vec<SkillDeclaration>` field comes from
  `compute_skill_declarations(&self.skills.read().await)` (the
  M2 helper added in commit `d6a21951`).
- The `image-understanding` cross-provider skill (and any future
  Ollama skill) does **not** apply to ComfyUI — ComfyUI publishes
  only its workflow-backed skills.

### 5.7 WhisperCpp / Speaches / Kokoro / OpenedaiSpeech (M3.15–M3.17, M3.21)

These four adapters were 50–60-line shells around
`openai_compat_stt` and `openai_compat_tts`. Those compat helpers
are **deleted** in M3.7. Each of the four adapters becomes
self-contained — it carries its own HTTP client, its own request
translation, its own response parsing, its own multipart builder
(STT) or audio body parser (TTS).

The four adapters remain functionally equivalent to today, just
without sharing code through the compat helpers. Cross-adapter
duplication is acceptable in M1 — the user explicitly requested
adapters be self-contained. Future deduplication (if any) can
revisit in M6.

### 5.8 Google/Gemini (M3.20)

Google/Gemini in M1 is the only cloud adapter we keep. The new
`providers/cloud_common.rs` (added in M3.8) provides
`resolve_cloud_model(model_input, default_model, supported_models)`
which centralizes the static-list lookup logic. Gemini calls it
inside `onboard` to resolve `selectors.model`.

The supported model list lives as a const inside `google.rs`.

---

## 6. Files deleted in M3.6 / M3.7

### M3.6 — legacy aggregates

- `src/domain/directory.rs`
- `src/domain/recommendation_types.rs`
- `src/services/recommendation.rs`
- `src/services/directory_maintenance.rs`
- `src/http/recommendations.rs`
- `src/services/skills/registry.rs`

### M3.7 — dropped adapters

- `src/providers/anthropic.rs`
- `src/providers/openai.rs`
- `src/providers/infinity.rs`
- `src/providers/openai_compat_stt.rs`
- `src/providers/openai_compat_tts.rs`

---

## 7. Files added in M3

### M3.8 — `src/providers/cloud_common.rs`

```rust
//! Shared helpers for cloud adapters that resolve models against
//! a static supported list (Google/Gemini in M1; Anthropic and
//! OpenAI when they return in M2).

use crate::domain::provider::ProviderError;

/// Resolve `selectors.model` against a static supported list.
///
/// - `None` → returns `default_model`.
/// - `Some("recommended:*")` → returns `default_model`. Cloud
///   adapters with static catalogs treat every `recommended:*`
///   moniker as "give me your default" because the recommendation
///   engine that mapped capabilities to specific cloud models is
///   gone in M3.
/// - `Some(concrete)` where `concrete` is in `supported_models` →
///   returns the matched name.
/// - `Some(concrete)` where `concrete` is **not** in
///   `supported_models` → returns
///   `Err(ProviderError::PinNotServable { model: concrete, reason })`.
pub fn resolve_cloud_model(
    model_input: Option<&str>,
    default_model: &'static str,
    supported_models: &[&'static str],
) -> Result<String, ProviderError> {
    let Some(input) = model_input else {
        return Ok(default_model.to_string());
    };
    if input.starts_with("recommended:") {
        return Ok(default_model.to_string());
    }
    if supported_models.iter().any(|m| *m == input) {
        return Ok(input.to_string());
    }
    Err(ProviderError::PinNotServable {
        model: input.to_string(),
        reason: format!(
            "model not in supported list (supported: {})",
            supported_models.join(", ")
        ),
    })
}
```

---

## 8. AppState shape after M3

```rust
#[derive(Clone)]
pub struct AppState {
    pub vocabularies: VocabularyRegistry,
    pub media_store: SharedMediaStore,
    pub job_store: Arc<dyn JobStore>,
    pub idempotency_store: Arc<dyn IdempotencyStore>,
    pub dispatcher: Arc<Dispatcher>,
    pub catalog: Arc<CatalogBuilder>,
    /// Provisioning queue for ComfyUI (Phase 2 of ORCH-0029).
    pub provisioning: Arc<ProvisioningQueue>,
    pub data_dir: PathBuf,
    pub events: Arc<EventBus>,
    pub resources: Arc<Resources>,
    pub capability_directory: Arc<CapabilityDirectory>,
    pub provider_registry: Arc<ProviderRegistry>,
}
```

**Removed fields:** `directory` (legacy aggregate), `recommendation`
(deleted with the engine), `skills` (deleted Skills aggregate).

---

## 9. main.rs construction order after M3

```text
1. Stores: media_store, job_store, idempotency_store
2. EventBus, Resources, CapabilityDirectory, DirectorySubscriber,
   ProviderRegistry
3. Adapters (in any order — each is constructed with events +
   discovery + shutdown, then registered into provider_registry)
4. Contextualizer, MediaResolver, Dispatcher (which takes
   capability_directory + provider_registry)
5. CatalogBuilder (subscribes to directory.provider.*.updated)
6. Background task spawns: catalog.run, directory_subscriber.run,
   terminal reaper, etc.
```

The legacy `directory_maintenance::run` task is deleted.
The legacy `recommendation.run` task is deleted.

---

## 10. Test surface

Tests under `tests/` that referenced `Directory`, `Registration`,
`HonoredField`, `RecommendationEngine`, `DemandLedger`, `Skills`
aggregate, or model resolution will need updating or deletion.
The integrator handles this in M3.26 after the adapter rewrites
land. Adapter rewrite agents do **not** touch `tests/`.

---

## 11. Subagent rules for adapter rewrites (M3.13–M3.21)

When invoked to rewrite a single adapter, an agent must:

1. **Read this contract spec in full first.** It is the
   normative source of truth.
2. **Read the existing adapter file in full** to understand the
   current state (instance pool, model resolution, wire-format
   translation). Preserve every existing wire translation and
   instance management primitive — only the trait surface and
   capability publication path change.
3. **Read the existing Ollama adapter** (`src/providers/ollama.rs`)
   as the reference for the new pattern. Ollama is the only
   adapter that already publishes capability events; the new
   shape mirrors it.
4. **Write the rewritten adapter** to its existing file path,
   replacing the contents wholesale. Do not leave back-compat
   shims or `// removed` comments.
5. **Do NOT run `cargo check` or `cargo build`.** The codebase
   is mid-refactor; many other adapters and services are broken
   simultaneously. The integrator runs the final build.
6. **Do NOT touch any file outside the assigned adapter file**
   unless the contract spec explicitly directs it (e.g., the
   ComfyUI rewrite removes the `Skills` aggregate dependency,
   which only affects `comfyui.rs` itself — `skills/registry.rs`
   is deleted by the integrator in M3.6).
7. **Report back as a structured summary** of what was changed,
   any uncertainty about the contract, and any line ranges in
   the existing code the agent could not preserve confidently.

---

## 12. Open questions for the integrator

These are decisions the contract intentionally defers — the
integrator resolves them as the rewrite progresses:

- **Skill announcement for cross-provider skills** (e.g.
  `image-understanding`). M1's plan declares the rerank decision
  for M6; cross-provider skills are similarly out of scope here.
  Each adapter publishes only its native skills in M1.
- **Per-instance metadata in `enabled` semantics.** The current
  `enabled` field is a single bool. A future enhancement could
  carry per-instance health to give the dashboard more detail —
  but the existing `directory.provider.*.updated` event already
  includes a version bump, which is enough for M1.
- **Locality / preference routing** in the contextualizer. M1
  picks the first provider returned by `providers_for_*`. R2.5
  commit 12 will introduce preferences and locality. The
  contextualizer's `pick_provider` helper (or whatever it's
  called after M3.9) is intentionally trivial in M1 so the
  routing logic has a clear seam to extend.
