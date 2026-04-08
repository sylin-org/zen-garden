---
audience: [developer, contributor]
doc_type: spec
status: current
last_verified: 2026-04-08
---

# Skill Subsystem Specification

How the AI Orchestrator turns declarative `skill.json` files on disk into live, dispatchable operations against remote ComfyUI instances — including the schema, the aggregates, the provisioning pipeline, and the catalog rendering join.

---

## Overview

A **skill** is a named narrowing of a primitive's canonical vocabulary plus a binding to a backend execution plan. Skills let operators expose new operations (`image.generate.flux-quick`, `image.upscale.anime-4x`) without writing Rust code — drop a `skill.json` and a workflow file under `{data_dir}/skills/{provider}/{moniker}/` and the orchestrator picks it up.

The subsystem has four responsibilities:

1. **Load** skill definitions from disk into typed in-memory state.
2. **Register** each skill with the Directory (public schema) and the Skills aggregate (dynamic state).
3. **Provision** each skill's required models: download into a local content-addressed cache, push to discovered ComfyUI instances via the Moss volume API.
4. **Dispatch** caller requests to the right skill, variant, and instance; walk the binding plan to populate the workflow template; return the result.

The subsystem is **provider-generic** — the ComfyUI adapter is the first consumer, but future adapters (Whisper model-size variants, Docling OCR presets) plug into the same `Skills` aggregate, cache, and provisioning queue.

---

## Architecture

### Two-layer model: vocabulary + skill

The orchestrator's canonical vocabulary (one file per primitive under `domain/vocabulary/`) defines **the language**: for `image.generate`, the field `image.sampling.steps` is an `Integer { min: 1, max: 150 }`, optional, described as "Number of sampling steps". Every provider that serves `image.generate` speaks this language.

A **skill** specializes that language by:

- Binding a subset of the vocabulary's fields to a specific workflow template.
- Narrowing the vocabulary's `FieldType` with a `FieldConstraint` (tighter range, restricted options, auto-generated seeds).
- Adding skill-specific defaults (pre-filled dashboard values).
- Declaring its required models and optional variants (multi-workflow skills) and model_selector (runtime checkpoint picking).

The vocabulary says *what is possible*. The skill says *what is exposed and where it lands*. Neither duplicates the other.

### Two aggregates on `AppState`

The orchestrator carries two parallel aggregates for skill state, owned by different update cadences:

| Aggregate | What it holds | Writer | Cadence |
|---|---|---|---|
| `Directory` | Provider registrations (static schema: honored fields, media inputs, vocabulary references) | Adapters via `ProviderStatePublisher` | Slow (registration changes on adapter init or hot-reload) |
| `Skills` | Per-skill metadata (variants, model_selector, required_models, preview_url, source) and per-instance readiness | Adapters (metadata) + provisioning worker (readiness) + AI namer (rename) | Fast (provisioning progress, discovery events) |

The catalog rendering joins both at read time. The directory's update cadence is slow enough that the dirty-counter debouncer and watch snapshot keep the wire cost minimal; the Skills aggregate's fast updates bump a separate version counter so readers know to re-render.

### Shared services

The Skills aggregate sits alongside two other aggregates that any skill-aware provider can share:

- **`DependencyCache`** (`services/skills/cache.rs`) — content-addressed manifest + streaming downloads + 4-case dedup + garbage collection. Per-provider cache directory: `{data_dir}/cache/dependencies/{provider}/`.
- **`ProvisioningQueue`** (`services/skills/queue.rs`) — bounded-concurrency worker with priority, exponential backoff, and `(skill, endpoint)` dedup. Default concurrency 2.

---

### Component diagram

```
                      ┌───────────────────────────────┐
  skill.json files ──►│       skills::loader          │── hot-reload (future)
                      │  v3 parser + legacy v1/v2     │
                      │  translation table            │
                      └──────────────┬────────────────┘
                                     │ SkillDefinition
                                     ▼
                      ┌───────────────────────────────┐
                      │    ComfyUI adapter            │
                      │  split_definition()           │
                      │      │                        │
                      │      ├── Registration ─────────►  Directory
                      │      │      (public schema)       (Arc<Directory>)
                      │      │
                      │      ├── LoadedSkill (private)
                      │      │      (workflows, bindings,
                      │      │       required_models)
                      │      │
                      │      └── SkillMeta ────────────►  Skills aggregate
                      │             (variants,             (Arc<Skills>)
                      │              model_selector,
                      │              preview_url)
                      └──────────────┬────────────────┘
                                     │
    discovery event  ────────────────┤
    (new ComfyUI                     ▼
     instance)          ┌───────────────────────────────┐
                        │  adapter::readiness_pass      │
                        │  1. load manifest             │
                        │  2. for each skill × instance │
                        │     - check_instance_readiness│   HEAD Moss volume
                        │     - set_readiness() ────────┼──►  Skills aggregate
                        │     - if missing, submit() ───┼──►  ProvisioningQueue
                        └───────────────────────────────┘           │
                                                                    │
                        ┌───────────────────────────────┐            │
                        │  provisioning worker          │◄───────────┘
                        │  (semaphore = max_concurrency)│
                        │                               │
                        │  for each job:                │
                        │    1. ensure_cached ──────────┼──►  DependencyCache
                        │         (stream download +    │    (manifest + files)
                        │          SHA-256 + ingest)    │
                        │                               │
                        │    2. push_to_instance ───────┼──►  Moss volume API
                        │         (HEAD + stream PUT)   │    (PUT to volume)
                        │                               │
                        │    3. complete/fail ──────────┼──►  ProvisioningQueue
                        │         set_readiness ────────┼──►  Skills aggregate
                        └───────────────────────────────┘

  POST /v1/do ──────────┐
   action:              │
    image.upscale.X     ▼
   variant: ...       ┌───────────────────────────────┐
   model: ...         │  Dispatcher                   │
   image.source: ...  │  1. Contextualizer            │
                      │  2. MediaResolver             │
                      │  3. Registration lookup       │──►  Directory
                      │  4. Provider.onboard() ───────┼──►  ComfyUI adapter
                      │                               │          │
                      │                               │          ▼
                      │                               │   walk bindings,
                      │                               │   apply variant,
                      │                               │   upload media,
                      │                               │   queue workflow,
                      │                               │   poll + fetch,
                      │                               │   return Output
                      └───────────────────────────────┘
```

---

## Protocol / API / Behavior

### On-disk layout

```
{data_dir}/
├── skills/
│   └── comfyui/
│       ├── generate/
│       │   ├── skill.json
│       │   └── generate.json
│       ├── upscale/
│       │   ├── skill.json
│       │   ├── upscale_2x.json
│       │   ├── upscale_4x.json
│       │   ├── upscale_8x.json
│       │   └── upscale_16x.json
│       ├── inpaint/
│       │   ├── skill.json
│       │   └── inpaint.json
│       └── {imported-moniker}/
│           ├── skill.json
│           ├── workflow.json
│           └── _debug.json   (optional, from import pipeline)
└── cache/
    └── dependencies/
        ├── comfyui/
        │   ├── manifest.json   { files: {name: "sha256:hex"}, aliases: {} }
        │   ├── RealESRGAN_x4plus.pth
        │   ├── sd-v1-5-inpainting.ckpt
        │   ├── flux_dev.safetensors
        │   └── …
        └── workspace/
            └── {skill-moniker}/   (ephemeral, cleaned after ingest)
```

### `skill.json` — schema v3

```json
{
  "version": 3,
  "draft": false,
  "name": "upscale",
  "display_name": "Upscale",
  "primitive": "image.upscale",
  "description": "Enhance image resolution using AI super-resolution",
  "vram_mb": 1024,
  "default_workflow": "upscale_4x",

  "bindings": [
    {
      "field": "image.source",
      "placeholder": "PLACEHOLDER_IMAGE",
      "delivery": "transfer",
      "accepted_types": ["image/png", "image/jpeg", "image/webp"]
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
      "narrow": { "kind": "auto", "auto": "random_int" }
    }
  ],

  "model_selector": {
    "placeholder": "PLACEHOLDER_MODEL",
    "default": "RealESRGAN_x4plus.pth",
    "options": [
      { "value": "RealESRGAN_x4plus.pth",           "label": "Realistic" },
      { "value": "RealESRGAN_x4plus_anime_6B.pth",  "label": "Anime" }
    ]
  },

  "variants": [
    { "value": "upscale_2x",  "label": "2x" },
    { "value": "upscale_4x",  "label": "4x" },
    { "value": "upscale_8x",  "label": "8x" },
    { "value": "upscale_16x", "label": "16x" }
  ],

  "required_models": [
    {
      "filename": "RealESRGAN_x4plus.pth",
      "model_type": "upscale_models",
      "url": "https://github.com/xinntao/Real-ESRGAN/releases/download/v0.1.0/RealESRGAN_x4plus.pth",
      "size_bytes": 67040989,
      "sha256": "4fa0d38905f75ac06eb49a7951b426670021be3018265fd191d2125df9d682f1",
      "license": "BSD-3-Clause",
      "description": "General-purpose 4x upscaler. Commercial-friendly."
    }
  ],

  "source": {
    "type": "civitai",
    "url": "https://civitai.com/images/126242620",
    "image_id": 126242620,
    "username": "Lady_Luminous"
  },
  "preview_url": "https://image.civitai.com/.../preview.jpeg"
}
```

Field reference:

| Field | Type | Purpose |
|---|---|---|
| `version` | `u64` | Schema discriminator. `3` is current. Versions `1` and `2` are recognized and translated on read. |
| `draft` | `bool` | When `true`, the loader skips the file. Used by the import pipeline for review-before-publish. |
| `name` | `string` | Internal identifier. Typically equal to the moniker. |
| `display_name` | `string` | Dashboard label. |
| `primitive` | `string` | Dotted canonical primitive (`"image.generate"`, `"image.upscale"`, …). Parsed into `Primitive` at load time. |
| `description` | `string` | Dashboard subtitle. |
| `vram_mb` | `u64` | Minimum GPU VRAM the skill needs to run. Informational. |
| `default_workflow` | `string` | Which sibling `.json` file to load as the starting workflow. Always loaded; additional files are loaded via `variants`. |
| `bindings` | `array` | One entry per vocabulary field the skill binds. See binding shape below. |
| `model_selector` | `object?` | Optional typed model picker. When present, the dashboard renders a dropdown; `selectors.model` drives it at dispatch. |
| `variants` | `array?` | Optional workflow-file selector. Each `value` must have a corresponding `{value}.json` file in the skill directory. |
| `required_models` | `array` | Models that must be on a ComfyUI instance for the skill to execute. Read by the provisioner. |
| `source` | `object?` | Import provenance (CivitAI image/model URL). |
| `preview_url` | `string?` | Optional preview image URL for the dashboard. |

### Binding shape

```json
{
  "field": "image.sampling.steps",
  "placeholder": "PLACEHOLDER_STEPS",   // OR "node" + "input"
  "node": "5",
  "input": "steps",
  "default": 20,
  "narrow": { "kind": "range", "min": 1, "max": 50, "step": 1 },
  "label": "Steps",
  "required": false,
  "delivery": "transfer",                // media bindings only
  "accepted_types": ["image/png"],       // media bindings only
  "overlay": "source"                     // mask bindings only
}
```

A binding MUST set exactly one target:

- **`placeholder`** — a string sentinel that appears in the workflow JSON. The provider's executor walks the workflow tree and replaces every occurrence with the user-supplied value.
- **`node` + `input`** — the JSON-pointer-like address `workflow[node]["inputs"][input]`. Used when placeholder substitution would be ambiguous or unsafe.

Optional overlays:

- **`default`** — pre-fill value when the caller omits the field. Applied by the dispatcher.
- **`narrow`** — a `FieldConstraint` that tightens the vocabulary's `FieldType`. Three kinds:
  - `{ "kind": "range", "min": <f64>, "max": <f64>, "step": <f64>? }`
  - `{ "kind": "options", "options": [{ "value": <any>, "label": <string>? }, …] }`
  - `{ "kind": "auto", "auto": "random_int" }`
- **`label`** — dashboard label override. Falls back to the vocabulary's `FieldSpec.description`.
- **`required`** — skill-level required flag. May tighten the vocabulary (vocabulary optional → skill required) but MUST NOT loosen it.
- **`delivery`**, **`accepted_types`**, **`overlay`** — media-binding metadata. Only meaningful when `field` is a vocabulary `MediaRef`-typed entry.

### `selectors.variant` — multi-workflow skills

Multi-workflow skills (upscale 2x/4x/8x/16x, TTS ChatterBox/F5) load every workflow file declared in `variants` into memory at construction time. At dispatch, the caller picks via the canonical top-level selector:

```json
POST /v1/do
{
  "action": "image.upscale.upscale-skill",
  "variant": "upscale_4x",
  "image": { "source": { "media_id": "01JA..." } }
}
```

The contextualizer passes `variant` through as a reserved top-level field. The provider validates the chosen variant against the skill's declared list (rejects unknown variants with `ProviderError::Unsupported`), loads the corresponding workflow JSON, and dispatches.

If `selectors.variant` is omitted, the provider falls back to `default_workflow`.

### `selectors.model` — model_selector resolution

When a skill declares `model_selector`, the caller may pick a model via `selectors.model`. The provider:

1. Reads `selectors.model` or falls back to `model_selector.default`.
2. Validates the chosen filename against `model_selector.options` (if any are declared). Unknown values return `ProviderError::Unsupported`.
3. Substitutes the chosen filename into every occurrence of `model_selector.placeholder` in the workflow JSON.
4. The provisioner has already ensured the file is present via the `required_models` list.

### Moniker rules

Skill monikers are validated by `domain::moniker::Moniker::new()`. Constraints:

- Lowercase ASCII + digits + hyphens.
- Must start with a letter.
- May not start or end with a hyphen; no consecutive hyphens.
- Maximum 64 characters.
- May not collide with reserved names: primitive leaves (`generate`, `upscale`, `edit`, `analyze`, `chat`, `translate`, `embed`, `rerank`, `transcribe`), modality names (`text`, `image`, `audio`, `video`), or orchestrator endpoints (`catalog`, `do`, `media`, `jobs`, `events`, `health`, `recommendations`, `providers`, `flush`, `schema`, `run`).

The loader sanitizes directory names that violate these rules **without modifying disk**:

- Reserved names get a `-skill` suffix: `generate` → `generate-skill`, `upscale` → `upscale-skill`.
- Names that start with a digit or non-letter get a `skill-` prefix: `554a4380-…` → `skill-554a4380-…`.
- Non-ASCII characters are replaced with hyphens and consecutive hyphens are collapsed.

The sanitization is deterministic and idempotent; the same disk state always produces the same moniker.

### Legacy schema translation (v1 / v2 → v3)

On-disk skill files written by the prior system use a flatter `mappings` array with skill-local field names (`"steps"`, `"cfg"`, `"checkpoint"`) instead of canonical vocabulary paths. The loader translates them at read time:

1. **Primitive resolution**: the legacy `name` field's `<modality>.<leaf>` prefix maps to a canonical primitive. `vision.tag` → `image.analyze`, `speech.tts` → `audio.generate`, `image.upscale` → `image.upscale`, `image.img2img` and `image.inpaint` → `image.edit`, `image.*` (anything else including imported skills) → `image.generate`.
2. **Content mappings** become media bindings (`image.source`, `image.mask`, `audio.source`) or text bindings (`image.prompt.positive`, `image.prompt.negative`, `text.prompt.user`, `audio.text`) based on the `role` + `content_type` pair.
3. **Param mappings** are translated through a per-primitive field table: for `image.generate`, `"steps"` → `image.sampling.steps`, `"cfg"` → `image.sampling.guidance`, `"width"` → `image.dimensions.width`, and so on. Fields not in the table become `x_*` passthroughs with self-described types.
4. **`field: "workflow"`** mappings are hoisted into the top-level `variants` array.
5. **`field: "checkpoint"` / `"upscale_model"`** mappings are hoisted into `model_selector`.
6. **`param_type`** values become `FieldConstraint` overlays: `"options"` → `Options`, `"range"` → `Range`, `"auto"` → `Auto { kind: RandomInt }`.

The translation is pure — the loader reads the file but never writes back. v1/v2 files continue to work indefinitely; v3 is the canonical format for new skills written by the import pipeline or the CRUD API (Phase 4).

### Dependency cache invariants

The content-addressed cache at `{data_dir}/cache/dependencies/{provider}/` holds two things:

1. **Model files** — arbitrary binary blobs keyed by filename.
2. **`manifest.json`** — a JSON object with two maps:

```json
{
  "files":   { "<filename>": "sha256:<hex>" },
  "aliases": { "<requested_name>": "<canonical_name>" }
}
```

Invariants:

- Every entry in `files` SHOULD have a matching file on disk. (The provisioner tolerates drift — missing files are re-downloaded on demand — but consumers expect the happy case.)
- Every entry in `aliases` MUST have its target in `files`. The loader's smoke test verifies this.
- Checksums are hex-encoded SHA-256 with a `sha256:` prefix. Case-insensitive comparison.
- Manifest writes are atomic: write `manifest.json.tmp`, then rename.

`DependencyManifest::resolve(filename)` follows the alias chain: if `requested_name` is in `aliases`, return its target; otherwise return the input unchanged. `is_cached(filename)` checks the resolved name against `files`.

### Streaming download

`cache::stream_download(http, url, dest, total_bytes, on_progress)` computes SHA-256 in the same pass as the write:

- If `dest` exists with non-zero size, the existing bytes are hashed first and `Range: bytes={n}-` is sent to resume. If the server returns `200` instead of `206`, the transfer restarts cleanly from byte 0 with a fresh hasher.
- `401`/`403` responses bail with a clear error. The URL is logged with query parameters stripped (CivitAI and HuggingFace carry tokens there).
- Progress callbacks fire at most once every 10 seconds for small files, 30 seconds for files larger than 1 GB.
- The underlying TCP keepalive detects dead connections. There is no global wall-clock timeout — multi-gigabyte checkpoints need throughput, not a deadline.

### 4-case dedup on ingest

After a successful download, `cache::ingest_to_cache(manifest, cache_dir, workspace_file, requested_name, checksum)` moves the workspace file into the cache with one of four outcomes:

| Case | `checksum` in manifest? | `name` in manifest? | Result |
|---|---|---|---|
| **A** | Yes (matching checksum) | Yes (same name) | Drop workspace copy. Return `AlreadyCached`. |
| **B** | Yes (matching checksum) | Different name | Record `aliases[requested] → canonical`. Drop workspace copy. Return `Aliased { canonical_name, alias_from }`. |
| **C** | No | No | Move workspace file to cache. Record `files[name] → checksum`. Return `Added { canonical_name }`. |
| **D** | No | Yes (collision) | Generate `name(2).ext` via `next_available_name`. Move with the new name. Return `Renamed { canonical_name, original_name }`. |

The manifest is not saved to disk inside `ingest_to_cache`; the caller (`provisioner::ensure_cached`) handles the save after the full batch to avoid fsync storms.

### Moss volume API

The provisioner talks to a remote ComfyUI instance via the Moss volume API:

```
HEAD  {moss}/api/v1/stone/offerings/{fqn}/volumes/{volume}/{path}
PUT   {moss}/api/v1/stone/offerings/{fqn}/volumes/{volume}/{path}
```

For ComfyUI: `fqn = "comfyui"`, `volume = "comfyui-models"`. Paths use `{model_type}/{filename}` — e.g. `checkpoints/v1-5-pruned-emaonly.safetensors`, `upscale_models/RealESRGAN_x4plus.pth`.

`moss_volume::derive_moss_endpoint(service_endpoint)` turns a discovered ComfyUI URL (`http://stone-crystal:8188`) into the corresponding Moss URL (`http://stone-crystal:7185`) by replacing the port with `MOSS_HTTP`.

`moss_volume::push_file_streaming` opens the local cache file, wraps it in a `ReaderStream`, sets `Content-Length` from `metadata()`, and PUTs it. No global timeout.

`moss_volume::file_exists` issues a HEAD with a 5-second timeout and returns `false` on any non-success. The provisioner's readiness check uses this for the fast path.

### Provisioning queue

`ProvisioningQueue` is a `tokio::sync::Mutex`-backed aggregate with private state:

- **`pending: VecDeque<ProvisioningJob>`** — jobs waiting to start. Sorted by priority (User before Discovery) then FIFO within a priority level.
- **`running: HashMap<ProvisioningTarget, ProvisioningJob>`** — currently executing jobs, dedup-keyed by `(skill_moniker, endpoint)`.
- **`history: VecDeque<ProvisioningJob>`** — ring buffer of the last 50 completed or failed jobs.
- **`backoff: HashMap<ProvisioningTarget, (Instant, u32)>`** — retry-after timestamps and attempt counts for failed targets.

Public API:

| Method | Purpose |
|---|---|
| `submit(target, priority, stone_name, provider) -> bool` | Enqueue a job. Returns `false` if the target is already running, queued, or in the backoff window. |
| `take_next() -> Option<Job>` | Worker pulls the next pending job and marks it Running. |
| `update_progress(target, progress)` | Worker reports download progress during a running job. |
| `complete(target, duration)` | Worker marks a job successful. Clears any backoff for the target. |
| `fail(target, reason)` | Worker marks a job failed. Installs a backoff entry. |
| `clear_backoff(target)` | Bypass the backoff window (used by user-triggered retries). |
| `drain()` | Clear pending queue. Shutdown path. |
| `snapshot() -> Arc<ProvisioningSnapshot>` | Read-only view of all jobs + counts. |
| `subscribe() -> watch::Receiver<Arc<ProvisioningSnapshot>>` | Watch for snapshot changes. |
| `event_stream() -> broadcast::Receiver<QueueEvent>` | Lifecycle events (Submitted, Started, Progress, Completed, Failed). |

**Backoff schedule**: exponential, capped at 1 hour. `1m → 5m → 30m → 1h`. Attempts ≥ 4 all use the 1-hour delay.

**Concurrency**: the worker in `ComfyUiProvider::spawn_provisioning_worker` holds a `Semaphore::new(max_concurrency)` and spawns one task per permit, each running `ensure_cached` + `push_to_instance` for one job. Default is 2.

### Discovery integration

Every time `garden_discovery` emits a `DiscoveryEvent` for the `comfyui` FQN, the adapter's subscriber runs `readiness_pass(instances)`:

1. Load the current `DependencyManifest` from disk.
2. Clone the loaded skills map.
3. For each `(skill, instance)` pair:
   1. Call `provisioner::check_instance_readiness` (HEAD every required model via `moss_volume::file_exists`).
   2. Push the result to `Skills::set_readiness(key, InstanceReadiness { ready, reason, … })`.
   3. If `ready == false`, submit a `ProvisioningTarget { skill, endpoint }` to the queue with `Priority::Discovery`.

The queue's dedup ensures a rapid burst of discovery events (one per refresh tick) doesn't enqueue the same target repeatedly.

### Dispatch flow (`onboard`)

The ComfyUI adapter's `Provider::onboard` walks the binding plan in a fixed order:

1. **Resolve the skill** — `request.action.skill` is the moniker; look it up in the adapter's private `skills` map. Mismatched primitives return `ProviderError::Unsupported`.
2. **Pin an instance** — `InstancePool::pick()` returns one URL. All subsequent calls in this request (upload, queue, history, view) use the same instance because the uploaded filename only exists on the instance that accepted it.
3. **Pick the workflow variant** — `request.selectors.variant` or fall back to `default_workflow`. Unknown variants return `ProviderError::Unsupported`.
4. **Walk bindings** — for each non-media binding, look up the canonical field in `request.payload` by converting the dotted path to a JSON pointer. Fall back to `binding.default`. Apply the value via the binding target (placeholder substitution or node+input address).
5. **Resolve `model_selector`** — pick the model filename from `request.selectors.model` or `selector.default`. Validate against the options list. Substitute into `selector.placeholder`.
6. **Upload deferred media** — for each media binding, fetch bytes from the media store, upload to ComfyUI's `/upload/image`, and substitute the returned filename into the binding's placeholder. Required media that's absent returns `ProviderError::Unsupported`.
7. **Queue the prompt** — POST `/prompt` with the populated workflow. Receive `{ prompt_id }`.
8. **Poll `/history/{prompt_id}`** every 500 ms for up to 10 minutes. The first output node whose `images` array is non-empty is the result. If the skill declared `output_node`, that one is preferred; otherwise any SaveImage-shaped node works.
9. **Fetch** the output bytes from `/view?filename=…&subfolder=…&type=…`.
10. **Store** the bytes in the media store and return `ProviderOutcome::Sync(Output)` with `image.media_id` (or `audio.media_id` for TTS skills, `text.response` for vision.tag skills) populated.

The same `onboard` runs every loaded skill — `image.generate.generate-skill`, `image.upscale.upscale-skill`, `image.edit.inpaint`, `audio.generate.tts`, and every imported skill. Zero per-skill branches.

### Catalog rendering

The catalog builder subscribes to both `Directory::subscribe()` and `Skills::subscribe()`. On every version bump of either aggregate, it re-renders the `/v1/catalog` document:

- **`primitives`** — one entry per primitive in use. Each carries its vocabulary view and the list of providers that serve it.
- **`skills`** — one entry per `(provider, moniker)` pair. The entry is a join of:
  - **From the Directory's registration**: `honored_fields` (with overlays), `media_inputs`.
  - **From the vocabulary**: the `FieldType` base for every honored field.
  - **From the Skills aggregate**: `variants`, `model_selector`, `required_models`, `source`, `preview_url`, and the `readiness` map (per-instance state).

Each binding is rendered as:

```json
{
  "path": "image.sampling.steps",
  "required": false,
  "label": "Steps",
  "default": 20,
  "type":       { "kind": "integer", "min": 1, "max": 150 },
  "constraint": { "kind": "range",   "min": 1, "max": 50, "step": 1 }
}
```

Dashboard code renders the appropriate widget by looking at `type` (base vocabulary type) overlaid with `constraint` (skill narrowing).

---

## Configuration

| Variable | Default | Purpose |
|---|---|---|
| `AI_ORCH_DATA_DIR` | `/data` | Root of `skills/`, `cache/`, media store, job store, recommendation pins. |
| `AI_ORCH_PORT` | `7190` | HTTP port for `/v1/*`, `/health`, `/metrics`. |
| `ZG_STONE` | *(none)* | Tended stone URL. Overrides Koi mDNS discovery when set. |

Command-line flags of interest to skill operators:

| Flag | Default | Purpose |
|---|---|---|
| `--data-dir {path}` | `/data` | Matches `AI_ORCH_DATA_DIR`. |
| `--stone {url}` | *(none)* | Tended stone. Must use an absolute URL (`http://…:7185`). |

### Provisioning queue defaults

| Setting | Default | Where to change |
|---|---|---|
| `max_concurrency` | `2` | `ProvisioningQueue::new(n)` in `main.rs` |
| Backoff schedule | `1m → 5m → 30m → 1h` | `queue::Backoff::SCHEDULE` (constant) |
| History cap | `50` | `queue::HISTORY_CAP` (constant) |
| HEAD timeout | `5s` | `moss_volume::HEAD_TIMEOUT` (constant) |
| Progress interval | `10s` (or `30s` for >1GB) | `cache::stream_download` |

---

## Examples

### Dispatch via the variant selector

```http
POST /v1/do
Content-Type: application/json

{
  "action": "image.upscale.upscale-skill",
  "variant": "upscale_2x",
  "image": { "source": { "media_id": "01JA..." } }
}
```

Response:

```json
{
  "output": {
    "image": { "media_id": "01JB..." }
  },
  "_meta": {
    "correlation_id": "...",
    "request_id": "...",
    "action": "image.upscale.upscale-skill",
    "provider": "comfyui",
    "mode": "sync",
    "received_at": "...",
    "completed_at": "..."
  }
}
```

Fetch the result:

```http
GET /v1/media/01JB...
```

### Catalog entry for a multi-variant skill

`GET /v1/catalog` returns (excerpted):

```json
{
  "skills": [
    {
      "action": "image.upscale.upscale-skill",
      "primitive": "image.upscale",
      "moniker": "upscale-skill",
      "display_name": "Upscale",
      "description": "Enhance image resolution using AI super-resolution",
      "provider": "comfyui",
      "fields": [
        {
          "path": "image.source",
          "required": true,
          "type": { "kind": "media_ref" },
          "constraint": null
        }
      ],
      "media_inputs": [
        {
          "field": "image.source",
          "delivery": "transfer",
          "accepted_types": ["image/png", "image/jpeg", "image/webp"],
          "overlay": null
        }
      ],
      "variants": [
        { "value": "upscale_2x",  "label": "2x" },
        { "value": "upscale_4x",  "label": "4x" },
        { "value": "upscale_8x",  "label": "8x" },
        { "value": "upscale_16x", "label": "16x" }
      ],
      "model_selector": {
        "default": "RealESRGAN_x4plus.pth",
        "placeholder": "PLACEHOLDER_MODEL",
        "options": [
          { "value": "RealESRGAN_x4plus.pth",          "label": "Realistic" },
          { "value": "RealESRGAN_x4plus_anime_6B.pth", "label": "Anime" }
        ]
      },
      "required_models": [
        { "filename": "RealESRGAN_x4plus.pth",          "model_type": "upscale_models", "size_bytes": 67040989 },
        { "filename": "RealESRGAN_x4plus_anime_6B.pth", "model_type": "upscale_models", "size_bytes": 17938799 }
      ],
      "readiness": [
        {
          "endpoint": "http://192.168.1.145:8188",
          "stone_name": "stone-crystal",
          "ready": true,
          "reason": "all required models present"
        },
        {
          "endpoint": "http://192.168.1.138:8188",
          "stone_name": "stone-mossy",
          "ready": false,
          "reason": "missing on instance: upscale_models/RealESRGAN_x4plus.pth"
        }
      ],
      "source": null,
      "preview_url": null
    }
  ]
}
```

---

## References

- [ORCH-0028 Orchestrator Core](../decisions/ORCH-0028-orchestrator-core.md) — vocabulary, Directory, Selectors, Provider trait, dispatch pipeline
- [ORCH-0029 Skill Subsystem](../decisions/ORCH-0029-skill-subsystem.md) — design rationale, wipe list, acceptance criteria
- [Operating Skills Guide](../guides/operating-skills.md) — operator how-to for adding, inspecting, and troubleshooting skills
- [Code Standards](../code-standards.md) — §1 no magic strings, §6 domain ownership, §13 event API
