---
audience: developer
doc_type: decision
status: proposed
---

# ORCH-0029: Skill Subsystem — Vocabulary-Aligned Skills, Adapter-Owned Lifecycle

**Date**: 2026-04-07
**Status**: Proposed
**Deciders**: Leo
**Related ADRs**:
- ORCH-0011 (recommended model monikers) — `selectors.model` channel for skill model picking
- ORCH-0018 (mapping-driven skills) — original skill mapping concept; **superseded** by this ADR
- ORCH-0021 (skill instance readiness) — per-instance readiness; **integrated** into the Skills aggregate
- ORCH-0022 (skill repository + dependency provisioning) — disk repo, content-addressed cache, provisioner; **integrated**
- ORCH-0023 (ComfyUI skill management) — CRUD + import pipeline; **integrated**
- ORCH-0025 (three-tier skill persistence) — disk + Moss + instance recovery; **integrated**
- ORCH-0026 (vision-assisted skill naming) — async AI naming; **integrated** (rewired to internal Dispatcher)
- ORCH-0028 (orchestrator core) — defines the vocabulary, Directory, Provider trait, Selectors, dispatch pipeline this ADR builds on

This ADR defines the skill subsystem in the ORCH-0028 framework. It supersedes the prior skill model (ORCH-0018) by collapsing the parallel parameter type system into ORCH-0028's canonical vocabulary, splits the prior `SkillMapping` into a public-facing schema layer and a provider-private execution layer, and binds the entire lifecycle (load, register, provision, dispatch, hot-reload, import, name) to the ComfyUI adapter.

The on-disk state at `.zen-garden/ai-orchestrator/skills/comfyui/` and `.zen-garden/ai-orchestrator/cache/dependencies/comfyui/` is preserved byte-for-byte. **The 90 GB of cached models and 20 imported skill directories survive the rebuild.**

---

## Mandate

**Reuse the vocabulary, don't duplicate it.** The prior skill system invented its own `ParamType` enum (`Options`/`Range`/`Auto`/`Text`) and skill-local field names (`"steps"`, `"cfg"`, `"checkpoint"`) that competed with ORCH-0028's canonical `FieldType` and dotted vocabulary paths (`image.sampling.steps`, `image.sampling.guidance`). This duplication is gone.

A skill is now **a narrowing of a primitive's vocabulary plus a binding to a backend execution plan**. Nothing more.

The orchestrator core gains **one new aggregate** (`Skills`), **two new fields** on `HonoredField` (`default`, `constraint`), **one new field** on `Selectors` (`variant`), and **one new field** on `MediaInputSpec` (`overlay`). Everything else lives inside the ComfyUI adapter as private implementation detail.

---

## Guiding principle

> **Skills inherit the language of the primitive. Providers own the execution, the directory owns the schema.**

Three concrete commitments follow:

1. **No parallel taxonomies.** A skill's input fields are canonical vocabulary fields. The vocabulary defines the type, range, description, and validation. The skill defines the *narrowing* (defaults, restricted options, tighter ranges) and the *binding* (where the value lands in the workflow).
2. **The ComfyUI adapter owns the skill lifecycle end to end.** Loading from disk, building Registrations, publishing to the Directory, watching for filesystem changes, provisioning models on instance discovery, dispatching at execution time. The orchestrator core never sees a workflow JSON, a placeholder string, or a model file.
3. **The 90 GB on disk is sacred.** The new system reads `manifest.json` and `skill.json` files written by the prior system without modification. A one-pass schema migration translates the old field names to canonical paths at load time. No re-downloading. No re-importing.

---

## Objectives

1. **Eliminate the parallel parameter type system.** Skills bind to canonical vocabulary fields. The vocabulary is the schema.
2. **Split presentation from execution.** What the dashboard sees lives on the Directory's Registration; what the provider needs to execute lives in adapter-private state.
3. **Adapter-owned lifecycle.** The ComfyUI adapter loads, registers, provisions, hot-reloads, and dispatches. The orchestrator core hosts shared services (cache, queue, Skills aggregate) but holds no ComfyUI-specific knowledge.
4. **Two aggregates, different cadences.** The Directory carries slow-moving static schema (registrations); the Skills aggregate carries fast-moving dynamic state (provisioning progress, per-instance readiness, model cache status).
5. **Typed multi-workflow + typed model selection.** Replace `field: "workflow"` and `field: "checkpoint"` magic strings with first-class `variants` and `model_selector` typed declarations, surfaced via `selectors.variant` and `selectors.model`.
6. **Preserve every working capability of the prior system.** Content-addressed cache, streaming downloads with resume + checksum, bounded provisioning queue with backoff, Moss volume API push, three-tier persistence, full CivitAI/PNG/UI→API/synth/resolve import pipeline, async AI naming.
7. **Drop everything decorative.** Mermaid diagram generation goes away. The `Capability` enum goes away. The skill-local field naming goes away.

---

## Decision

### The two-layer model

**Vocabulary** (orchestrator-owned, defined in `domain/vocabulary/{primitive}.rs`):
- The canonical schema for a primitive — required and optional `FieldSpec` entries with `FieldType`, descriptions, validation.
- Every field has a canonical dotted path (`image.sampling.steps`, `image.prompt.positive`, …) declared as a constant in `domain/keys/`.
- This is **the language**. Any provider that serves the primitive speaks it.

**Skill** (adapter-owned on disk, pushed to the Directory via Registration):
- A typed binding of a subset of the vocabulary's fields, with skill-specific defaults and narrowing constraints.
- The skill **specializes** the vocabulary; it does not extend it (except via `x_*` passthrough for genuine provider-specific knobs).
- The dashboard renders the form for a skill by reading `vocabulary[primitive]` as the base and overlaying `registration.honored_fields[*].constraint` as the narrowing.

### `HonoredField` extended

The existing `HonoredField` from ORCH-0028 §Provider gains three optional fields. The compiler-checked invariants from ORCH-0028 §1 (no magic strings) and §6 (domain ownership) are preserved.

```rust
// domain/provider.rs

pub struct HonoredField {
    /// Canonical field path (or `x_*` for provider-specific extensions).
    pub path: FieldPath,
    /// True when the skill cannot execute without this field.
    pub required: bool,
    /// Skill-specific override for the dashboard label.
    /// `None` → fall back to the vocabulary's `FieldSpec.description`.
    pub label: Option<String>,
    /// Skill-specific default. Pre-fills the dashboard form and is used
    /// at dispatch time when the caller omits the field.
    pub default: Option<serde_json::Value>,
    /// Skill-specific narrowing of the vocabulary's type. `None` means
    /// "use the vocabulary's `FieldType` as-is."
    pub constraint: Option<FieldConstraint>,
}

/// Narrows a vocabulary `FieldType` for a specific skill. The
/// dashboard renders the form by reading the vocabulary type as the
/// base and applying these as overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldConstraint {
    /// Restrict to a finite set of values. Compatible with vocabulary
    /// types `String`, `Integer`, `Number`.
    Options { options: Vec<ParamOption> },

    /// Tighten a numeric range. Compatible with `Integer` and `Number`.
    /// `min`/`max` MUST be inside the vocabulary's declared range.
    Range {
        min: f64,
        max: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
    },

    /// Auto-generated value (e.g., random seed). The dashboard renders
    /// a "regenerate" button; the dispatcher fills in if the caller
    /// omits.
    Auto { kind: AutoKind },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamOption {
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoKind {
    RandomInt,
}
```

**Validation**: the Contextualizer's existing validation pass uses the vocabulary's `FieldType`. After this ADR, it ALSO checks the skill's `FieldConstraint` for the resolved registration:

- `Options`: caller value must be in the option set
- `Range`: caller value must be inside the narrowed `[min, max]`
- `Auto`: no validation (the dispatcher fills it)
- `None`: vocabulary validation only (current behavior)

### `MediaInputSpec` extended

```rust
// domain/provider.rs

pub struct MediaInputSpec {
    pub field: FieldPath,
    pub delivery: MediaDelivery,
    pub accepted_types: Vec<String>,
    /// When set, the dashboard renders this slot as a paint overlay
    /// on the named role's image. Used by inpaint skills to draw
    /// masks on top of the source image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay: Option<String>,
}
```

The single new field `overlay: Option<String>` carries the prior `ContentSlot.overlay` semantics. Inpaint skills set `media_inputs[0]` to `image.source` and `media_inputs[1]` to `image.mask` with `overlay: Some("source".into())`.

### `Selectors` extended

```rust
// domain/selectors.rs

pub struct Selectors {
    pub provider: Option<ProviderName>,
    pub model: Option<String>,
    pub skill: Option<Moniker>,
    /// Skill-meta selector for multi-workflow skills. The skill's
    /// catalog metadata declares the available variants; the dashboard
    /// renders a dropdown; the request carries the chosen variant
    /// here. Providers consume it during `onboard`.
    pub variant: Option<String>,
}
```

`selectors.variant` is a generic skill-meta channel. ComfyUI uses it to pick a workflow file (`upscale_2x.json` vs `upscale_8x.json`). A future Whisper adapter could use it for `tiny`/`base`/`small`/`large` model size. A future Docling adapter could use it for `fast`/`accurate`. The orchestrator validates that the value is one of the variants declared by the resolved registration.

### The `Skills` aggregate

A new aggregate on `AppState`, parallel to the Directory. Owned by the orchestrator core; consumed by every provider that adopts the skill model.

```rust
// services/skills/mod.rs
pub mod cache;
pub mod loader;
pub mod provisioner;
pub mod queue;
pub mod registry;
pub mod types;

// services/skills/registry.rs

pub struct Skills {
    /// Static skill definitions, keyed by (provider, moniker).
    /// Loaded from disk by the adapters; mutated only via
    /// `register`/`unregister`. Snapshot is published via watch.
    state: Mutex<SkillsState>,
    publisher: SkillsPublisher,
}

#[derive(Debug, Clone, Default)]
pub struct SkillsSnapshot {
    pub version: u64,
    pub skills: Arc<HashMap<SkillKey, SkillEntry>>,
    pub provisioning: Arc<HashMap<ProvisioningTarget, ProvisioningStatus>>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct SkillKey {
    pub provider: ProviderName,
    pub moniker: Moniker,
}

#[derive(Debug, Clone)]
pub struct SkillEntry {
    /// Static metadata that mirrors what the adapter declares in its
    /// Registration plus skill-meta fields the Registration doesn't
    /// carry (variants, model_selector, required_models, source).
    pub meta: SkillMeta,
    /// Per-instance readiness, keyed by instance endpoint.
    pub readiness: HashMap<String, InstanceReadiness>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillMeta {
    pub provider: ProviderName,
    pub moniker: Moniker,
    pub primitive: Primitive,
    pub display_name: String,
    pub description: String,
    pub vram_mb: u64,
    /// Multi-workflow variants. When `None`, this skill has a single
    /// workflow. When `Some`, the dashboard exposes a `variant`
    /// dropdown driving `selectors.variant`.
    pub variants: Option<Vec<ParamOption>>,
    /// Model picker. When `None`, the skill has a fixed model. When
    /// `Some`, the dashboard exposes a `model` dropdown driving
    /// `selectors.model`.
    pub model_selector: Option<ModelSelector>,
    /// Required model files (filename, type, url, sha256, license).
    /// The provisioner reads this; the dashboard shows readiness.
    pub required_models: Vec<ModelRef>,
    /// Optional import provenance.
    pub source: Option<ImportSource>,
    pub preview_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelSelector {
    pub default: String,
    pub options: Vec<ParamOption>,
}

#[derive(Debug, Clone)]
pub struct InstanceReadiness {
    pub stone_name: String,
    pub endpoint: String,
    pub ready: bool,
    pub reason: String,
    pub vram_mb: u64,
}

impl Skills {
    pub fn new() -> Arc<Self> { /* … */ }

    /// Adapter pushes a skill into the registry. Idempotent — calling
    /// twice with the same key replaces the metadata.
    pub async fn register(&self, meta: SkillMeta) {}

    /// Adapter removes a skill (e.g., file deleted on disk).
    pub async fn unregister(&self, key: &SkillKey) {}

    /// Adapter or provisioning worker updates per-instance readiness.
    pub async fn set_readiness(&self, key: &SkillKey, readiness: InstanceReadiness) {}

    pub fn snapshot(&self) -> Arc<SkillsSnapshot> { self.publisher.borrow() }

    /// Lifecycle event API (per ORCH-0028 §13).
    pub fn skill_stream(&self) -> broadcast::Receiver<SkillEvent> { /* … */ }
    pub fn on_provisioning_progress(&self) -> broadcast::Receiver<ProvisioningProgress> { /* … */ }
}

#[derive(Debug, Clone, Serialize)]
pub enum SkillEvent {
    Registered { key: SkillKey },
    Unregistered { key: SkillKey },
    ReadinessChanged { key: SkillKey, endpoint: String, ready: bool },
    Named { key: SkillKey, display_name: String, description: String },
}
```

**Why a separate aggregate**:

- The Directory's update cadence is slow (registrations change on adapter init or hot-reload, ~seconds apart at most).
- Skills' provisioning progress updates fast (every 10 seconds during a download, faster during the final push).
- Catalog requests join `directory.snapshot()` × `skills.snapshot()` at read time.
- Per ORCH-0028 §6, each domain owns its mutable state with one writer. The ComfyUI adapter writes to the Directory via `ProviderState`; the provisioning worker writes to `Skills` via `set_readiness`. Two channels, two writers, no contention.

### Catalog rendering — the unified pipeline

A skill view in `/v1/catalog` is **the join of**:

1. The vocabulary for the skill's primitive (`vocabulary[primitive]`).
2. The Directory registration with `RegistrationStrategy::Skill { moniker }` (carries `honored_fields` with constraints + `media_inputs`).
3. The Skills aggregate entry (carries `variants`, `model_selector`, `required_models`, `source`, per-instance readiness).

Rendering algorithm:

```
GET /v1/catalog/skills/{provider}/{moniker}:
  1. directory.snapshot() → find Registration with strategy=Skill{moniker}
  2. vocabulary[registration.primitive]
  3. skills.snapshot()[SkillKey{provider, moniker}]

  4. compose response:
     {
       "moniker": <moniker>,
       "primitive": <primitive>,
       "display_name": <skills.meta.display_name>,
       "description": <skills.meta.description>,
       "preview_url": <skills.meta.preview_url>,
       "source": <skills.meta.source>,
       "variants": <skills.meta.variants>,
       "model_selector": <skills.meta.model_selector>,
       "fields": [
         for each honored_field:
           {
             "path": <field.path>,
             "required": <field.required>,
             "label": <field.label OR vocab_field_spec.description>,
             "default": <field.default>,
             "type": resolve(vocab_field_spec.field_type, field.constraint),
             "description": <vocab_field_spec.description>,
           }
       ],
       "media_inputs": [...],
       "media_outputs": [...],
       "readiness": {
         <endpoint>: { ready, reason, stone_name, vram_mb },
         ...
       },
       "required_models": [
         { filename, model_type, sha256, size_bytes, status: "cached"|"missing"|"auth_required" },
         ...
       ]
     }
```

The `resolve(vocab_type, constraint)` function:

| `vocab_type` | `constraint` | Resulting renderable type |
|---|---|---|
| `Integer{min, max}` | `Range{min, max, step}` | `Slider{min, max, step}` (with min/max clamped to vocab bounds) |
| `Integer{min, max}` | `Options{options}` | `Dropdown{options: integer values}` |
| `Integer{min, max}` | `Auto{RandomInt}` | `AutoFill{kind: random_int, hint: "click to regenerate"}` |
| `Number{min, max}` | `Range{min, max, step}` | `Slider` (numeric) |
| `String` | `Options` | `Dropdown` (string values) |
| `String` | `None` | `TextInput` |
| `Boolean` | `None` | `Checkbox` |
| `Array` | `None` | `ChipInput` or JSON editor |
| `MediaRef` | `None` | (handled via `media_inputs`, not as a regular field) |
| Any | `None` | Default widget for the vocabulary type |

Constraints with mismatched types (e.g., `Range` on a `String` field) fail the loader's validation pass with a clear error citing the skill's source file.

### Disk schema (v3)

The new `skill.json` format:

```json
{
  "version": 3,
  "name": "generate",
  "display_name": "Generate",
  "primitive": "image.generate",
  "description": "Stable Diffusion 1.5 baseline txt2img",
  "vram_mb": 4096,
  "default_workflow": "generate",

  "bindings": [
    {
      "field": "image.prompt.positive",
      "placeholder": "PLACEHOLDER_PROMPT"
    },
    {
      "field": "image.prompt.negative",
      "placeholder": "PLACEHOLDER_NEGATIVE",
      "default": "blurry, watermark, low quality, deformed"
    },
    {
      "field": "image.dimensions.width",
      "node": "4",
      "input": "width",
      "default": 512,
      "narrow": { "kind": "options", "options": [
        {"value": 512}, {"value": 768}, {"value": 1024}
      ]}
    },
    {
      "field": "image.dimensions.height",
      "node": "4",
      "input": "height",
      "default": 512,
      "narrow": { "kind": "options", "options": [
        {"value": 512}, {"value": 768}, {"value": 1024}
      ]}
    },
    {
      "field": "image.sampling.steps",
      "node": "5",
      "input": "steps",
      "default": 20,
      "narrow": { "kind": "range", "min": 1, "max": 50, "step": 1 }
    },
    {
      "field": "image.sampling.seed",
      "node": "5",
      "input": "seed",
      "narrow": { "kind": "auto", "kind_inner": "random_int" }
    }
  ],

  "model_selector": {
    "placeholder": "PLACEHOLDER_CHECKPOINT",
    "default": "v1-5-pruned-emaonly.safetensors",
    "options": [
      { "value": "v1-5-pruned-emaonly.safetensors", "label": "SD 1.5 (default)" }
    ]
  },

  "variants": null,

  "required_models": [
    {
      "filename": "v1-5-pruned-emaonly.safetensors",
      "model_type": "checkpoints",
      "url": "https://huggingface.co/.../v1-5-pruned-emaonly.safetensors",
      "size_bytes": 4265380512,
      "sha256": "6ce0161689b3853acaa03779ec93eafe75a02f4ced659bee03f50797806fa2fa",
      "license": "CreativeML Open RAIL-M",
      "description": "Stable Diffusion 1.5 — versatile, runs on 4GB+ VRAM."
    }
  ]
}
```

Multi-workflow with media binding (`upscale`):

```json
{
  "version": 3,
  "name": "upscale",
  "display_name": "Upscale",
  "primitive": "image.upscale",
  "vram_mb": 1024,
  "default_workflow": "upscale_4x",

  "bindings": [
    {
      "field": "image.source",
      "placeholder": "PLACEHOLDER_IMAGE",
      "delivery": "transfer",
      "accepted_types": ["image/png", "image/jpeg", "image/webp"]
    }
  ],

  "variants": [
    { "value": "upscale_2x",  "label": "2x" },
    { "value": "upscale_4x",  "label": "4x" },
    { "value": "upscale_8x",  "label": "8x" },
    { "value": "upscale_16x", "label": "16x" }
  ],

  "model_selector": {
    "placeholder": "PLACEHOLDER_MODEL",
    "default": "RealESRGAN_x4plus.pth",
    "options": [
      { "value": "RealESRGAN_x4plus.pth",          "label": "Realistic" },
      { "value": "RealESRGAN_x4plus_anime_6B.pth", "label": "Anime" }
    ]
  },

  "required_models": [
    {
      "filename": "RealESRGAN_x4plus.pth",
      "model_type": "upscale_models",
      "url": "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.1.0/RealESRGAN_x4plus.pth",
      "size_bytes": 67040989,
      "sha256": "4fa0d38905f75ac06eb49a7951b426670021be3018265fd191d2125df9d682f1",
      "license": "BSD-3-Clause"
    },
    {
      "filename": "RealESRGAN_x4plus_anime_6B.pth",
      "model_type": "upscale_models",
      "url": "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.2.4/RealESRGAN_x4plus_anime_6B.pth",
      "size_bytes": 17938799,
      "sha256": "f872d837d3c90ed2e05227bed711af5671a6fd1c9f7d7e91c911a61f155e99da",
      "license": "BSD-3-Clause"
    }
  ]
}
```

The `bindings` array unifies what the prior schema split into `content_slots` + `mappings`. A binding whose `field` is a `MediaRef`-typed vocabulary field is a media binding (carries `delivery` and `accepted_types`); a binding whose `field` is any other type is a regular form field (may carry `default` and `narrow`).

### Schema migration (v1/v2 → v3)

The on-disk skills are v1 or v2. The loader supports both via a one-pass translation:

```rust
fn translate_legacy_to_v3(raw: serde_json::Value) -> Result<SkillDefinition, LoaderError> {
    let version = raw["version"].as_u64().unwrap_or(1);
    if version >= 3 { return parse_v3(raw); }

    // ── Map legacy `name` and `capability` to canonical primitive ──
    let legacy_name = raw["name"].as_str()?;       // "image.generate", "vision.tag", "speech.tts", "image.img2img", "image.inpaint", "image.upscale", "image.<imported-slug>"
    let primitive = match legacy_name {
        n if n.starts_with("image.upscale")  => Primitive::ImageUpscale,
        n if n.starts_with("image.inpaint")  => Primitive::ImageEdit,
        n if n.starts_with("image.img2img")  => Primitive::ImageEdit,
        n if n.starts_with("image.")          => Primitive::ImageGenerate,
        n if n.starts_with("vision.tag")     => Primitive::ImageAnalyze,
        n if n.starts_with("speech.tts")     => Primitive::AudioGenerate,
        _ => bail!("unknown legacy primitive: {legacy_name}"),
    };

    // ── Translate skill-local field names to canonical paths ──
    // The lookup table is per-primitive. For image.generate:
    let field_map: HashMap<&str, FieldPath> = match primitive {
        Primitive::ImageGenerate => [
            ("negative",       keys::image::PROMPT_NEGATIVE),
            ("width",          keys::image::DIMENSIONS_WIDTH),
            ("height",         keys::image::DIMENSIONS_HEIGHT),
            ("steps",          keys::image::SAMPLING_STEPS),
            ("seed",           keys::image::SAMPLING_SEED),
            ("cfg",            keys::image::SAMPLING_GUIDANCE),
        ].into_iter().collect(),
        Primitive::ImageUpscale => [
            // Most upscale fields are skill-meta (workflow, upscale_model)
            // and become variants/model_selector, not bindings.
        ].into_iter().collect(),
        Primitive::ImageEdit => [
            ("strength",       keys::image::STRENGTH),  // newly added vocab key
            ("steps",          keys::image::SAMPLING_STEPS),
            ("seed",           keys::image::SAMPLING_SEED),
            ("negative",       keys::image::PROMPT_NEGATIVE),
        ].into_iter().collect(),
        // ... similar tables for ImageAnalyze, AudioGenerate
        _ => HashMap::new(),
    };

    // ── Translate content_slots ──
    // role: "source"  → keys::image::SOURCE  (or keys::audio::SOURCE)
    // role: "mask"    → keys::image::MASK
    // role: "prompt"  → keys::image::PROMPT_POSITIVE  (or keys::text::PROMPT_USER for vision/speech)
    // role: "negative" → keys::image::PROMPT_NEGATIVE

    // ── Walk legacy mappings ──
    // For each Param mapping:
    //   - if field == "workflow"        → extract into top-level `variants`
    //   - if field == "checkpoint"      → extract into top-level `model_selector`
    //   - if field == "lora"|"upscale_model"|"vae"  → extract into model_selector
    //                                                   (or skip — mostly skill-meta)
    //   - else, lookup canonical field via field_map
    //     - if found → emit Binding { field: canonical, default, narrow }
    //     - if not found → emit Binding { field: x_<legacy_name>, ..., type: Self-described }
    // For each Content mapping:
    //   - lookup canonical via the role table
    //   - emit Binding { field: canonical, placeholder, delivery, accepted_types }

    // ... build SkillDefinition v3 ...
}
```

The loader translates on read. The disk file is **never modified**. The translation is deterministic and idempotent.

The CRUD API's "save" operation writes v3 directly. Operators editing imported skills migrate their files to v3 the first time they save.

### The ComfyUI adapter — owner of the lifecycle

```rust
// providers/comfyui/mod.rs

pub struct ComfyUiProvider {
    name: ProviderName,
    instances: Arc<InstancePool>,
    http: Client,
    publisher: ProviderStatePublisher,
    /// Adapter-private skill state. Maps (primitive, moniker) → loaded
    /// skill with workflow files in memory and binding plans pre-built.
    skills: Arc<RwLock<HashMap<SkillKey, LoadedSkill>>>,
    /// Shared aggregates from AppState.
    skills_aggregate: Arc<Skills>,
    cache: Arc<DependencyCache>,
    queue: Arc<ProvisioningQueue>,
    skills_dir: PathBuf,
    file_watcher: Option<JoinHandle<()>>,
}

/// Adapter-private skill state. Carries everything the executor needs;
/// nothing in this struct is published to the Directory.
struct LoadedSkill {
    moniker: Moniker,
    primitive: Primitive,
    workflows: HashMap<String, serde_json::Value>,
    default_workflow: String,
    bindings: Vec<WorkflowBinding>,
    model_selector: Option<ModelSelector>,
    output_node: String,
    required_models: Vec<ModelRef>,
}

struct WorkflowBinding {
    /// Canonical field path (or x_*).
    field: FieldPath,
    /// Where the value lands in the workflow.
    target: BindingTarget,
}

enum BindingTarget {
    /// String substitution throughout the workflow tree.
    Placeholder(String),
    /// Direct addressing: workflow[node]["inputs"][input] = value.
    NodeInput { node: String, input: String },
}
```

### Lifecycle stages

**Stage 1: Construction / load**

```rust
impl ComfyUiProvider {
    pub async fn new(
        config: ComfyUiConfig,
        skills_aggregate: Arc<Skills>,
        cache: Arc<DependencyCache>,
        queue: Arc<ProvisioningQueue>,
        discovery: Arc<GardenDiscovery>,
        shutdown: CancellationToken,
    ) -> Arc<Self> {
        // 1. Seed embedded skills if missing/outdated.
        loader::seed_embedded_skills(&config.skills_dir).await;

        // 2. Scan disk → typed SkillDefinitions.
        let definitions = loader::load_skills(&config.skills_dir).await;

        // 3. For each skill: split into Registration (public) +
        //    LoadedSkill (private), publish both.
        let mut registrations = Vec::new();
        let mut loaded = HashMap::new();
        for def in definitions {
            let (registration, loaded_skill) = split_definition(def)?;
            skills_aggregate.register(meta_from_registration(&registration)).await;
            registrations.push(registration);
            loaded.insert(SkillKey { provider, moniker }, loaded_skill);
        }

        // 4. Publish registrations to the Directory via ProviderState.
        let initial = ProviderState {
            health: ProviderHealth::Offline { ... },
            registrations,
            ...
        };

        let provider = Arc::new(Self { /* ... */ });

        // 5. Spawn the discovery subscriber (see Stage 3).
        spawn_discovery_subscriber(provider.clone(), discovery, shutdown.clone());

        // 6. Spawn the file watcher (see Stage 2).
        spawn_file_watcher(provider.clone(), config.skills_dir.clone(), shutdown);

        provider
    }
}
```

**Stage 2: Hot-reload (filesystem watcher)**

```rust
fn spawn_file_watcher(provider: Arc<ComfyUiProvider>, skills_dir: PathBuf, shutdown: CancellationToken) {
    use notify::{RecommendedWatcher, RecursiveMode, Watcher};

    tokio::spawn(async move {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(8);
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                if let Ok(_event) = res {
                    let _ = tx.try_send(());
                }
            },
            notify::Config::default(),
        ).expect("create watcher");
        watcher.watch(&skills_dir, RecursiveMode::Recursive).ok();

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                Some(_) = rx.recv() => {
                    // Debounce: drain any further events that arrive within 500ms.
                    let _ = tokio::time::timeout(Duration::from_millis(500), async {
                        while rx.recv().await.is_some() {}
                    }).await;

                    provider.reload_from_disk().await;
                }
            }
        }
    });
}

impl ComfyUiProvider {
    async fn reload_from_disk(&self) {
        let definitions = loader::load_skills(&self.skills_dir).await;

        // Build the new state.
        let mut new_loaded = HashMap::new();
        let mut new_registrations = Vec::new();
        for def in definitions {
            let (reg, loaded) = split_definition(def);
            new_registrations.push(reg);
            new_loaded.insert(loaded.key(), loaded);
        }

        // Diff against current.
        let current_keys: HashSet<_> = self.skills.read().await.keys().cloned().collect();
        let new_keys: HashSet<_> = new_loaded.keys().cloned().collect();

        // Unregister removed skills.
        for removed in current_keys.difference(&new_keys) {
            self.skills_aggregate.unregister(removed).await;
        }

        // Register new + updated skills.
        for (key, loaded) in &new_loaded {
            self.skills_aggregate.register(meta_for(loaded)).await;
        }

        // Replace adapter-private state.
        *self.skills.write().await = new_loaded;

        // Re-publish provider state with new registrations.
        self.publisher.modify(|mut state| {
            state.registrations = new_registrations;
            state
        });
    }
}
```

**Stage 3: Discovery + provisioning**

When a ComfyUI instance comes up via the existing `garden_discovery` channel, the adapter's subscriber:

```rust
fn spawn_discovery_subscriber(provider: Arc<ComfyUiProvider>, discovery: Arc<GardenDiscovery>, shutdown: CancellationToken) {
    tokio::spawn(async move {
        let mut rx = discovery.subscribe(&["comfyui", "comfyui::adopted"]).await;
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                event = rx.recv() => {
                    let Some(event) = event else { break };
                    let urls: Vec<String> = event.instances.iter().map(|i| i.url.clone()).collect();

                    // Update instance pool + provider state.
                    provider.apply_discovered(urls.clone());

                    // For each loaded skill, check readiness on each new instance.
                    let skills = provider.skills.read().await;
                    for (key, loaded) in skills.iter() {
                        for instance in &event.instances {
                            let readiness = provider.cache.check_instance_readiness(
                                &loaded.required_models,
                                &instance.endpoint,
                                "comfyui-models",
                            ).await;

                            provider.skills_aggregate.set_readiness(
                                key,
                                InstanceReadiness {
                                    stone_name: instance.stone_name.clone(),
                                    endpoint: instance.endpoint.clone(),
                                    ready: readiness.ready,
                                    reason: readiness.reason.clone(),
                                    vram_mb: instance.vram_mb,
                                },
                            ).await;

                            if !readiness.ready {
                                // Submit a provisioning job.
                                provider.queue.submit(
                                    ProvisioningTarget {
                                        skill: key.moniker.to_string(),
                                        endpoint: instance.endpoint.clone(),
                                    },
                                    Priority::Discovery,
                                    instance.stone_name.clone(),
                                    "comfyui".into(),
                                ).await;
                            }
                        }
                    }
                }
            }
        }
    });
}
```

The provisioning worker (in `services/skills/queue.rs`) consumes jobs and runs the cache + push pipeline (preserved from ORCH-0022). On completion, it calls `skills_aggregate.set_readiness(key, InstanceReadiness { ready: true, ... })` so the catalog reflects the new state.

**Stage 4: Dispatch (`onboard`)**

```rust
#[async_trait]
impl Provider for ComfyUiProvider {
    async fn onboard(&self, request: OrchestratorRequest) -> Result<ProviderOutcome, ProviderError> {
        // 1. Resolve the skill.
        let skill_moniker = request.action.skill.as_ref().ok_or_else(|| {
            ProviderError::Unsupported("comfyui requires a skill moniker".into())
        })?;
        let key = SkillKey { provider: self.name.clone(), moniker: skill_moniker.clone() };
        let skill = self.skills.read().await.get(&key).cloned()
            .ok_or_else(|| ProviderError::Unsupported(format!("skill {key:?} not loaded")))?;

        // 2. Pick the workflow file via variant selector.
        let variant = request.selectors.variant.as_deref()
            .unwrap_or(&skill.default_workflow);
        let workflow_template = skill.workflows.get(variant)
            .ok_or_else(|| ProviderError::Unsupported(format!("variant `{variant}` not found")))?;
        let mut workflow = workflow_template.clone();

        // 3. Pick an instance and pin it for the entire request.
        let instance = self.pick()?;

        // 4. Apply each binding.
        for binding in &skill.bindings {
            let value = lookup_field(&request.payload, &binding.field)
                .or_else(|| skill_default_for(&binding.field, &skill));
            let Some(value) = value else { continue };
            apply_binding(&mut workflow, binding, &value)?;
        }

        // 5. Apply model selector.
        if let Some(selector) = &skill.model_selector {
            let model_filename = request.selectors.model
                .clone()
                .unwrap_or_else(|| selector.default.clone());
            let canonical = self.cache.resolve_alias(&model_filename).await;
            substitute_placeholder(&mut workflow, &selector.placeholder, &canonical);
        }

        // 6. Upload media for image content slots.
        for binding in skill.bindings.iter().filter(|b| is_media_field(&b.field)) {
            let media_ref = request.media.find_at_field(&binding.field)
                .ok_or_else(|| ProviderError::Unsupported(format!("media missing at {}", binding.field)))?;
            let bytes = request.context.media_store.get_bytes(&media_ref.id).await?;
            let uploaded_filename = upload_image(&self.http, &instance, &bytes, &media_ref).await?;
            let placeholder = match &binding.target {
                BindingTarget::Placeholder(p) => p,
                _ => continue,
            };
            substitute_placeholder(&mut workflow, placeholder, &uploaded_filename);
        }

        // 7. Submit prompt, poll history, fetch view, store in media store.
        // (Same flow as the current ComfyUI provider.)
        let media_id = run_workflow_on_instance(&self.http, &instance, &workflow, &skill.output_node, &request).await?;

        let mut out = Output::new();
        out.set(&keys::image::MEDIA_ID, media_id.as_str());
        Ok(ProviderOutcome::Sync(out))
    }
}
```

**Zero per-skill branches.** The same `onboard` runs every loaded skill — `image.generate.generate`, `image.upscale.upscale`, `image.edit.inpaint`, `image.analyze.tag`, `audio.generate.tts`, and every imported user skill.

### Provisioning subsystem (preserved from ORCH-0022)

The cache, provisioner, and queue are ported intact. Only their location and ownership change:

| Module | Location | Ownership |
|---|---|---|
| `services/skills/cache.rs` | `DependencyCache` aggregate on `AppState` | Shared across providers |
| `services/skills/provisioner.rs` | Pure functions called by the worker | Stateless |
| `services/skills/queue.rs` | `ProvisioningQueue` aggregate on `AppState` | Shared across providers |
| `services/skills/moss_volume.rs` | Pure HTTP helpers | Stateless |

**Invariants preserved**:

- `manifest.json` schema: `{ files: { name: "sha256:hex" }, aliases: { req: canonical } }`
- Cache layout: `{data_dir}/cache/dependencies/{provider}/{model files}`
- Workspace layout: `{data_dir}/cache/dependencies/workspace/{skill}/`
- Streaming download with HTTP `Range:` resume + single-pass SHA-256
- 4-case dedup (AlreadyCached / Aliased / Added / Renamed)
- Auth detection at HEAD probe time (CivitAI Bearer + query token, HuggingFace token)
- Atomic manifest writes (write `.tmp`, rename)
- Garbage collection on skill removal
- Bounded queue with semaphore (default 2), priority (User > Discovery), exponential backoff (1m → 5m → 30m → 1h)
- Best-effort Tier-3 push of skill files to instances after each successful provision

### Import pipeline (preserved from ORCH-0023, rewired for ORCH-0028)

Ported intact under `services/skills/import/`:

- `input_detect.rs` — pure classifier (CivitAI image/model URL, PNG URL, generic URL, raw JSON, A1111 text, PNG bytes)
- `png_extract.rs` — `tEXt`/`zTXt`/`iTXt` chunk parser
- `civitai.rs` — CivitAI API client with the Bearer-header + `?token=`-query auth dance, image fetch, model page fetch, model version resolve, hash resolve, workflow zip extract, unsupported-generator detection
- `ui_to_api.rs` — UI-format → API-format converter with the 60+ widget mapping table
- `model_resolve.rs` — 5-level cascade (CivitAI version IDs → CivitAI hash → local cache → ComfyUI Manager registry → known_models.json)
- `param_extract.rs` — KSampler-driven role detection, placeholder injection, mapping generation
- `workflow_synth.rs` — txt2img / txt2img-with-LoRA / from-resources synthesis
- `gen_data_parse.rs` — A1111 generation text parser
- `known_models.rs` + `known_models.json` — 86 curated HuggingFace ecosystem entries
- `analyze.rs` — pipeline orchestrator
- `draft_builder.rs` — writes draft `skill.json` with `draft: true`
- `namer.rs` — async AI naming, **rewired to call `state.dispatcher.dispatch()` internally** with `text.chat` + `recommended:chat` instead of HTTP-ing `localhost:21434`

The output of `analyze::run` is an `AnalyzeResult` with v3 schema mappings (the import pipeline emits the new format directly; legacy loader translation is only for files already on disk).

### CRUD API

Endpoints under `/v1/skills/{provider}/...`:

```
GET    /v1/skills                                      list all skills (all providers)
GET    /v1/skills/{provider}                           list skills for one provider
GET    /v1/skills/{provider}/{moniker}                 get one skill (catalog view)
GET    /v1/skills/{provider}/new                       empty scaffold
POST   /v1/skills/{provider}/{moniker}                 upsert (clears `draft`)
DELETE /v1/skills/{provider}/{moniker}                 delete + GC unreferenced models
POST   /v1/skills/{provider}/import                    accept text/JSON/multipart, run analyze pipeline
POST   /v1/skills/{provider}/{moniker}/rename          trigger AI namer
POST   /v1/skills/{provider}/{moniker}/provision       force-submit provisioning jobs
GET    /v1/skills/{provider}/{moniker}/workflows/{name} read workflow file
PUT    /v1/skills/{provider}/{moniker}/workflows/{name} write workflow file
GET    /v1/skills/events                               SSE: skill.registered, skill.unregistered, skill.named, skill.provisioning, skill.readiness_changed
```

### State Ownership update

| Aggregate | State | Writer | Snapshot |
|---|---|---|---|
| `Directory` | Provider registrations, vocabulary | Each provider via `ProviderStatePublisher` | `DirectorySnapshot` via watch |
| `Skills` | Skill metadata, per-instance readiness, provisioning status | ComfyUI adapter (registration), provisioning worker (readiness), namer (display name update) | `SkillsSnapshot` via watch |
| `DependencyCache` | `manifest.json` per provider | Provisioning worker | None (read directly when needed) |
| `ProvisioningQueue` | Queue state (pending/running/history/backoff) | Worker via `domain.submit/take/complete/fail` | `ProvisioningSnapshot` via watch |

### Wipe list (additions to ORCH-0028's wipe list)

- The Mermaid diagram generator in the prior `parser.rs` is **not ported**. The rest of `parser.rs` (model loader detection, input detection, output detection) IS ported because the import pipeline needs it.
- The `Capability` enum is **not ported**. Use `Primitive` directly.
- The `ParamType` enum is **not ported**. Use `FieldType` (vocabulary) + `FieldConstraint` (skill).
- Skill-local field naming (`"steps"`, `"checkpoint"`, etc.) is **not ported**. The loader's legacy translation table converts old files to canonical paths at read time.
- The `field == "workflow"` magic string is **not ported**. Replaced by typed `variants`.
- The current minimal `SkillManifest` struct in `providers/comfyui.rs` is **deleted**. Replaced by the v3 `SkillDefinition` from `services/skills/types.rs`.
- The `localhost:21434` proxy hop in the prior `namer.rs` is **not ported**. Replaced by an internal `state.dispatcher.dispatch()` call.

### Migration sequence

1. **First commit on the branch**: drop the current minimal `providers/comfyui.rs` skill loader (`load_skills_from_disk`, `SkillManifest`, `LoadedSkill`). Add the new `services/skills/` module skeleton with empty modules. The build is broken at this point — that's intentional.

2. **Second commit**: port `services/skills/types.rs` (the v3 `SkillDefinition` and friends), `services/skills/loader.rs` (with legacy translation), and the embedded skill files. Adds `FieldConstraint`, `ParamOption`, `AutoKind` to `domain/provider.rs`. Adds `overlay: Option<String>` to `MediaInputSpec`. Adds `variant: Option<String>` to `Selectors`.

3. **Third commit**: port `services/skills/registry.rs` (`Skills` aggregate). Wire into `AppState`. Wire the new `Selectors.variant` through the contextualizer and dispatcher (no-op pass-through in the contextualizer; the provider reads it).

4. **Fourth commit**: rewrite `providers/comfyui.rs` to use the new model. Import skills from disk via the loader, register with the Skills aggregate, build Registrations from the loaded skills, walk bindings in `onboard`. **At the end of this commit, `cargo test` is green and all 20 on-disk skills load and register.**

5. **Fifth commit (Phase 1 validation)**: live exercise. Spin up the orchestrator against the workspace data dir, hit `/v1/catalog/skills/comfyui/upscale`, verify the response contains the variants and model selector. Hit `POST /v1/do { action: "image.upscale.upscale", selectors: { variant: "upscale_4x" }, image: { source: { media_id: <uploaded> } } }` against a live ComfyUI instance, verify the returned `image.media_id` resolves to a 4× upscaled PNG.

6. **Phases 2–4** layer on the cache, provisioner, queue, import pipeline, CRUD API, and hot-reload — each as its own commit batch.

---

## Acceptance criteria

The skill subsystem is complete when:

1. **All 20 on-disk skills load** from `.zen-garden/ai-orchestrator/skills/comfyui/` without modification. Verified by `cargo test --test live` with the workspace data dir.
2. **`/v1/catalog/skills/comfyui/upscale` returns a complete view** with variants, model_selector, and bindings whose types are correctly resolved from the vocabulary + constraint overlay.
3. **`POST /v1/do { action: "image.upscale.upscale", selectors: { variant: "upscale_4x" }, image: { source: { media_id: ... } } }` against a live ComfyUI instance produces a 4× upscaled PNG** that downloads from the media store and decodes correctly. **This is the Phase 1 validation deliverable.**
4. **The 90 GB existing cache reads without re-downloading.** Verified by spinning up against the workspace data dir and observing zero `stream_download` calls during the readiness check pass.
5. **Provisioning a fresh ComfyUI instance pushes the existing cached models** without re-downloading from the upstream URL. The bytes flow from local cache → Moss volume API → instance.
6. **Skill validation rejects out-of-narrowed-range inputs** at the contextualizer pass. Sending `image.sampling.steps: 100` to a skill that narrows to `1..50` returns `400 validation_failed` with the offending field.
7. **`/v1/catalog/skills/{provider}/{moniker}` joins Directory + Skills + Vocabulary** in one response with no inconsistency.
8. **The catalog completeness check** (the existing `live_catalog_completeness` test) passes — every primitive that has at least one skill registered exposes that skill in the catalog.
9. **Hot-reload works**: deleting a `skill.json` while the orchestrator is running causes the registration to disappear from `/v1/catalog` within 1 second.
10. **Filesystem watcher debounces correctly**: rapidly editing a `skill.json` (write + chmod + write within 100ms) triggers exactly one reload, not three.
11. **Schema migration is lossless**: every legacy v1/v2 skill on disk produces a valid v3 `SkillDefinition` after loader translation. Round-trip via the CRUD API's "save" call writes valid v3 to disk that reloads identically.
12. **Provisioning queue dedup**: submitting the same `(skill, endpoint)` target twice in quick succession results in one running job, not two.
13. **Backoff schedule is honored**: a failed download enters a 60s cooldown; resubmitting before the cooldown expires returns `false` from `submit`.
14. **Tier-3 push happens after successful provisioning**: the instance's `comfyui-models/zen-garden/skills/{moniker}/skill.json` exists and matches the local file.
15. **AI namer uses the internal dispatcher**: no HTTP request to `localhost:21434` is made. Verified by mocking the `Dispatcher` and asserting the namer calls `dispatch(text.chat, recommended:chat)`.
16. **Import pipeline produces v3 directly**: `POST /v1/skills/comfyui/import { input: "<civitai-url>" }` writes a `skill.json` with `version: 3` and no legacy fields.
17. **Per-skill validation** rejects requests where `selectors.variant` is not one of the skill's declared variants.

---

## Decisions locked

- **Two layers**: vocabulary (orchestrator) + skill (adapter). Skills inherit from vocabulary; never duplicate it.
- **Two aggregates**: Directory (slow, schema) + Skills (fast, dynamic state). Catalog joins at read time.
- **Adapter ownership**: ComfyUI adapter owns load, register, hot-reload, provision dispatch, and dispatch execution. Orchestrator core hosts shared services (`DependencyCache`, `ProvisioningQueue`, `Skills` aggregate) that any future skill-based adapter can reuse.
- **`HonoredField` extension**: `label`, `default`, `constraint` (optional). Backward-compatible (None means current behavior).
- **`MediaInputSpec.overlay`**: optional, carries the prior content-slot overlay hint.
- **`Selectors.variant`**: new typed field. Generic across providers — not ComfyUI-specific.
- **Disk schema v3** is the new format. Loader translates v1/v2 on read; CRUD API writes v3 on save. The `version` field is a `u64`; bumping it is an ADR amendment.
- **Embedded skills** are versioned and seeded with the same gate as ORCH-0022 (embedded version > existing version → overwrite).
- **Contained migration**: the on-disk state is preserved byte-for-byte. The 90 GB cache and 20 skill directories survive.
- **AI namer** calls the internal `Dispatcher` with `text.chat` + `recommended:chat`. The `localhost:21434` proxy hop is gone.
- **CRUD API namespace**: `/v1/skills/{provider}/...` (replaces the prior `/v1/services/{provider}/skills/...`).

---

## Consequences

### What gets easier

- **Adding a skill is a JSON file.** Drop a `skill.json` and a workflow file under `.zen-garden/ai-orchestrator/skills/comfyui/{moniker}/`. The file watcher picks it up. The catalog reflects it. The provisioner downloads any missing models. No Rust changes.
- **Form rendering is one pipeline.** The dashboard generates a form from `vocabulary + skill bindings` for any skill. Bare-primitive forms and skill forms share the same code path.
- **Validation is enforced earlier.** The contextualizer's existing validation gains skill-aware narrowing. Out-of-range values fail with a clear field path before the request reaches the provider.
- **Provisioning is a shared service.** The cache and queue live on `AppState`. Future skill-based adapters (Whisper variants, Docling preset switching, etc.) reuse them with zero duplication.
- **Hot-reload is real**: the filesystem watcher gives 500ms latency from disk edit to catalog update. The prior 30s ticker is gone.

### What gets harder

- **Schema migration is one-way**: the v1/v2 → v3 translation runs on read, but operators editing files in v1/v2 format are nudged toward v3 on save (the CRUD API writes v3 only). The legacy translation table needs to be maintained until every imported skill has been re-saved at least once.
- **Catalog joins must stay cheap**: the `/v1/catalog/skills/{provider}/{moniker}` response joins Directory + Skills + Vocabulary at read time. Snapshot reads are O(1) per aggregate; the join is bounded by the number of bindings (typically <10 per skill). No caching layer needed at v1.
- **Two writers update the per-skill state**: the adapter writes registrations to the Directory; the worker writes readiness to the Skills aggregate. Both writers are documented; reading clients must call `directory.snapshot()` and `skills.snapshot()` independently and join.

### What is locked

- **The disk schema is part of the contract.** Future versions remain backward-compatible at the `version` discriminator level. Removing fields requires an ADR amendment + a migration pass.
- **The cache layout is part of the contract.** `manifest.json` shape, content-addressed file storage, alias chains. The 90 GB on disk depends on this.
- **The Moss volume API endpoints are part of the contract.** `/api/v1/stone/offerings/{fqn}/volumes/comfyui-models/{path}`. Changing this breaks ComfyUI provisioning.
- **The variant + model_selector concepts are first-class.** No magic strings.

### What is deferred

- **Generic skill model for non-ComfyUI providers**: the Skills aggregate is generic, but only the ComfyUI adapter implements the lifecycle in v1. Future adapters (Whisper variant picker, Docling preset switching) plug in by implementing the same lifecycle stages.
- **Multi-provider model resolution cascade**: today the cache is per-provider (`{data_dir}/cache/dependencies/comfyui/`). Sharing models across providers (e.g., a Whisper model that several adapters use) is a future enhancement.
- **Skill versioning + audit trail**: the disk file is mutable. A future enhancement could add a `history/` directory with timestamped snapshots when the CRUD API saves.
- **Skill marketplace + pull-from-stone recovery via Tier-2**: ORCH-0025 envisioned three tiers; this ADR fully implements Tier 1 (host) + Tier 3 (instance), and leaves Tier 2 (Moss-stored backup) for future work.

---

## References

- [ORCH-0028 Orchestrator Core](ORCH-0028-orchestrator-core.md) — vocabulary, Directory, Selectors, Provider trait, dispatch
- [ORCH-0018 Mapping-driven Skills](ORCH-0018-mapping-driven-skills.md) — original skill mapping concept (superseded by §Decision)
- [ORCH-0022 Skill Repository and Dependency Provisioning](ORCH-0022-skill-repository-and-dependency-provisioning.md) — disk repo, cache, provisioner, queue (integrated)
- [ORCH-0023 ComfyUI Skill Management](ORCH-0023-comfyui-skill-management.md) — CRUD API, import pipeline (integrated)
- [ORCH-0025 Three-Tier Skill Persistence](ORCH-0025-three-tier-skill-persistence.md) — three-tier recovery (Tiers 1+3 implemented; Tier 2 deferred)
- [ORCH-0026 Vision-Assisted Skill Naming](ORCH-0026-vision-assisted-skill-naming.md) — async AI naming via internal Dispatcher (integrated)
- Code standards (`docs/code-standards.md`) — §1 (no magic strings), §6 (domain ownership), §13 (event API)
