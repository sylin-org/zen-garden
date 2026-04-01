---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0018: Skills, Workflow API, and Synthetic Capabilities

**Date**: 2026-03-31
**Amended**: 2026-04-01 — API endpoints, mapping-driven definitions, job IDs
**Status**: Accepted

---

## Problem

The AI orchestrator routes inference requests to providers — chat completions, embeddings,
speech, transcription. These are all **simple request-response operations**: text in, text
out (or audio, or vectors).

ComfyUI introduces a fundamentally different pattern: **parameterized pipelines**. An image
upscale isn't a single model call — it's a directed acyclic graph of nodes (load image →
load upscale model → run upscale → save image). The operation takes seconds to minutes,
requires pre-installed models, and produces binary output (images, video).

The current API has no way to:
- Express pipeline-style operations
- Submit async jobs with progress tracking
- Handle binary content (images) as first-class inputs/outputs
- Let providers dynamically advertise what operations they support
- Let users import community workflows and expose them as API endpoints

---

## Decision

### 1. Skills — Dynamic capability publishing

A **skill** is a named operation that a provider publishes. Skills are the bridge between
the orchestrator's capability model and a provider's concrete implementation.

```
Offering: ComfyUI
  Capability: Image
    Skills: upscale, generate, transform

Offering: Speaches
  Capability: Speech
    Skills: synthesize, clone_voice

Offering: Docling
  Capability: Ocr
    Skills: extract, convert
```

Skills are dynamic — they come from what's installed (models + workflow templates), not
from a hardcoded list. If a ComfyUI instance has upscale models installed, it advertises
`image.upscale`. If it doesn't, it doesn't.

Not every provider needs skills. Ollama's chat completions are fully described by
the model + capability — no skill layer required. Skills add value when a provider
supports multiple distinct operations within a single capability.

#### Skill definition — mapping-driven, data-only

A skill is a **JSON file**, not code. Skills are pure data — mappings, metadata, and
references to workflow templates. The orchestrator renders the form, the provider
executes the workflow. Adding a new skill requires zero code changes.

```json
{
  "name": "image.upscale",
  "display_name": "Upscale",
  "capability": "image",
  "description": "Enhance image resolution using AI super-resolution",
  "provider_kind": "comfyui",
  "vram_mb": 1024,
  "default_workflow": "upscale_4x",
  "content_slots": [
    { "role": "source", "content_type": "image", "required": true }
  ],
  "mappings": [
    { "type": "content", "role": "source", "content_type": "image",
      "placeholder": "PLACEHOLDER_IMAGE" },
    { "type": "param", "field": "workflow", "label": "Zoom",
      "param_type": "options", "default": "upscale_4x",
      "options": [
        { "value": "upscale_2x",  "label": "2x" },
        { "value": "upscale_4x",  "label": "4x" },
        { "value": "upscale_8x",  "label": "8x" },
        { "value": "upscale_16x", "label": "16x" }
      ]},
    { "type": "param", "field": "upscale_model", "label": "Style",
      "placeholder": "PLACEHOLDER_MODEL", "param_type": "options",
      "default": "RealESRGAN_x4plus.pth",
      "options": [
        { "value": "RealESRGAN_x4plus.pth", "label": "Realistic" },
        { "value": "RealESRGAN_x4plus_anime_6B.pth", "label": "Anime" }
      ]}
  ],
  "required_models": [
    { "filename": "RealESRGAN_x4plus.pth", "model_type": "upscale_models" },
    { "filename": "RealESRGAN_x4plus_anime_6B.pth", "model_type": "upscale_models" }
  ]
}
```

**Mappings** are the single source of truth for the form UI and the execution engine:

| Mapping type | Purpose | Handling |
|-------------|---------|----------|
| `content` | User-provided input (image, text) | `content_type` determines: image = upload first then substitute placeholder; text = substitute placeholder directly |
| `param` | Form parameter → workflow value | If `placeholder` is set: string substitution throughout the workflow. If `node`+`input` is set: set `workflow[node]["inputs"][input] = value` |

**ParamType** variants:

| Type | Rendering | Example |
|------|-----------|---------|
| `options` | Radio (≤4) or select (>4). Each option has `value` (wire) and optional `label` (display). | Style: "Realistic", "Anime" |
| `range` | Slider with min/max/step | Steps: 1–50 |
| `auto` | Pre-filled editable field (e.g., random seed) | Seed: 498423072 |
| `text` | Textarea | Negative prompt |

When options carry no `label`, display = wire value (e.g., width: 512, 768, 1024).
When options carry a `label`, the user sees the label but the wire sends the value.

#### Workflow selection

Every skill has a `default_workflow` — the template used when no override is present.

If a parameter has `field: "workflow"`, its value overrides the default. The provider
loads the named template instead. The user sees a friendly label (e.g., "Zoom: 4x"),
the wire value is a template name (e.g., `"upscale_4x"`).

The execution engine:
1. Read `parameters.workflow` — if present, use it; otherwise use `default_workflow`
2. Load the named template from the provider's template registry
3. Iterate mappings: substitute content placeholders, set param values
4. Submit to the provider

This means a single skill can fan out to multiple workflow templates without any
code changes. The template selection is data-driven.

#### Built-in vs imported skills

**Built-in skills** are pre-registered with curated mappings, validated model
requirements, and embedded workflow templates. They ship with the orchestrator.

**Imported skills** come from user-uploaded ComfyUI workflows (PNG with embedded
metadata or exported JSON). They go through an import flow before becoming available.

### 2. Skill API — capability-namespaced endpoints

Skills are invoked through capability-namespaced endpoints. The pattern is consistent
across all capabilities:

```
POST /v1/{capability}/skill/{skill-moniker}
```

This coexists with the existing capability defaults (`/v1/chat/completions`,
`/v1/audio/speech`, etc.). Skills are the named, parameterized extensions.

#### Invoke a skill

```
POST /v1/image/skill/upscale
```

```json
{
  "content": [
    { "type": "image", "role": "source", "data": "<base64>" }
  ],
  "parameters": {
    "zoom": "4x",
    "style": "realistic"
  }
}
```

The capability is in the URL path (routing). The skill moniker is in the URL path
(operation selection). The body is pure data — content blocks + parameters. No routing
metadata in the payload.

#### Response (immediate — 202 Accepted)

```json
{
  "id": "019d45dc-8a3b-7def-9012-3456789abcde",
  "skill": "upscale",
  "status": "queued"
}
```

Job IDs are GUIDv7s (time-sortable, globally unique).

#### Poll status

```
GET /v1/jobs/{id}
```

Job IDs are globally unique — no capability namespace needed for polling.

```json
{
  "id": "019d45dc-8a3b-7def-9012-3456789abcde",
  "skill": "upscale",
  "status": "running",
  "progress": 0.65
}
```

#### Completed

```json
{
  "id": "019d45dc-8a3b-7def-9012-3456789abcde",
  "skill": "upscale",
  "status": "completed",
  "content": [
    { "type": "image", "format": "png", "url": "/v1/jobs/019d45dc/assets/result.png" }
  ],
  "usage": {
    "duration_ms": 3200
  }
}
```

#### Failed

```json
{
  "id": "019d45dc-8a3b-7def-9012-3456789abcde",
  "skill": "upscale",
  "status": "failed",
  "error": {
    "code": "model_not_found",
    "message": "Upscale model '4x-UltraSharp.pth' not installed"
  }
}
```

#### Statuses

`queued` → `running` → `completed` | `failed`

#### Asset retrieval

```
GET /v1/jobs/{id}/assets/{filename}
```

Binary response with proper `Content-Type`. Cached with TTL, caller downloads if they
need permanent storage.

#### Discovery and management

```
GET  /v1/skills                          — list all registered skills
GET  /v1/skills/{capability}.{skill}/form — mappings + diagram for TryIt UI
POST /v1/skills/import                   — import a community workflow
GET  /v1/jobs                            — list recent jobs
```

### 3. Skill TryIt — dashboard integration

The skill form endpoint returns mappings directly — no JSON Schema translation:

```
GET /v1/skills/image.upscale/form
```

```json
{
  "display_name": "Upscale",
  "description": "Enhance image resolution using AI super-resolution",
  "content_slots": [
    { "role": "source", "content_type": "image", "required": true }
  ],
  "mappings": [
    { "type": "content", "role": "source", "content_type": "image", "placeholder": "PLACEHOLDER_IMAGE" },
    { "type": "param", "field": "zoom", "label": "Zoom", "param_type": "options",
      "options": [
        { "value": "RealESRGAN_x4plus.pth", "label": "4x" },
        { "value": "RealESRGAN_x4plus_anime_6B.pth", "label": "4x Anime" }
      ],
      "default": "RealESRGAN_x4plus.pth" }
  ],
  "diagram": "graph LR\n    A[Load Image] --> B[Upscale 4x]\n    B --> C[Save Image]"
}
```

The dashboard renders controls directly from mappings:
- `Content(image)` → image dropzone
- `Content(text)` → textarea
- `Param(options)` → radio (≤4) or select (>4), labels displayed, wire values sent
- `Param(range)` → slider
- `Param(auto)` → pre-filled editable number with re-roll button
- `Param(text)` → textarea

The same `SkillTryIt` component works for every skill. No per-skill frontend code.

### 4. Workflow import flow

Users can import community ComfyUI workflows shared as PNG images (with embedded
workflow metadata) or exported JSON files.

#### Step 1 — Upload and analyze

```
POST /v1/skills/import
Content-Type: multipart/form-data
  file: workflow.png
  name: "anime_style_transfer"
  capability: "image"
```

The orchestrator:
- Extracts workflow JSON (from PNG tEXt chunk or directly)
- Parses the node graph to identify required models and input nodes
- Looks up model metadata from known registries (CivitAI, HuggingFace)
- Returns a preview for user confirmation

#### Step 2 — Preview and confirm

The import endpoint returns a preview:

```json
{
  "import_id": "019d45dc-1234-7abc-8def-567890abcdef",
  "name": "anime_style_transfer",
  "capability": "image",
  "required_models": [
    { "name": "animagine-xl-3.1.safetensors", "size_gb": 6.4, "license": "Fair AI Public License 1.0", "source": "civitai", "installed": false },
    { "name": "4x-UltraSharp.pth", "size_gb": 0.065, "license": "MIT", "source": "github", "installed": true }
  ],
  "detected_parameters": [
    { "name": "prompt", "type": "text", "required": true, "expose": true },
    { "name": "strength", "type": "number", "required": false, "expose": true, "default": 0.7, "min": 0.0, "max": 1.0 }
  ],
  "total_download_gb": 6.54,
  "diagram": "graph LR\n    A[Prompt] --> B[SDXL]\n    B --> C[LoRA]\n    C --> D[KSampler]\n    D --> E[Output]"
}
```

#### Step 3 — Confirm and install

```
POST /v1/skills/import/{import_id}/confirm
```

```json
{
  "exposed_parameters": ["prompt", "strength"],
  "accept_licenses": true
}
```

The orchestrator creates a multi-step job:
1. Download missing models to ComfyUI instances
2. Push workflow template to each instance
3. Register the skill in the skill registry
4. Publish to the dashboard

The skill is available once at least one instance has all required models.

### 5. Skill sync across stones

When a new ComfyUI instance joins the garden, the orchestrator replicates skills to it:

1. Compare installed models/workflows against the skill registry
2. Transfer missing models (from peer stones or original sources)
3. Push workflow templates
4. Instance starts advertising the skills

Sync is automatic on instance join and can be triggered manually via the dashboard.

### 6. Provider trait extension

The Provider trait gains skill-related methods:

```rust
/// Declare built-in skills this provider supports.
fn builtin_skills(&self) -> Vec<SkillDefinition> { Vec::new() }

/// Check if a specific instance can serve a skill.
fn check_skill_readiness(&self, ctx, skill) -> Result<SkillReadiness>;

/// Make an instance ready for a skill (download models, push workflows).
fn provision_skill(&self, ctx, skill, cache_dir, moss_endpoint, fqn) -> Result<()>;

/// Execute a skill on a ready instance.
fn workflow(&self, ctx, req, skill_def) -> Result<WorkflowJob>;
```

Default implementations return "not supported." Only providers with pipeline capabilities
(ComfyUI initially) override them.

### 7. Routing

Skill requests route through the existing `select_instance()` algorithm:

1. Capability from URL path → filter instances by capability
2. Skill moniker → filter instances that advertise this skill
3. Standard routing: health check, VRAM tier, queue depth, fitness score
4. Dispatch to the selected instance's provider

The skill name is an additional filter on the existing capability-based routing.

---

## Consequences

- ComfyUI becomes a first-class citizen in the orchestrator, not just a proxied service.
- Complex image operations (upscale, generate, transform) are exposed as clean API
  endpoints without leaking ComfyUI internals (node graphs, workflow JSON).
- The skill endpoint pattern (`/v1/{capability}/skill/{moniker}`) is consistent across
  all capabilities and coexists with existing default endpoints.
- Community workflows can be imported and published as new skills without code changes.
- The dashboard TryIt UI renders from mappings — no per-skill frontend code.
- Mapping-driven definitions mean adding a new skill is pure data, no Rust code.
- Job IDs are GUIDv7s — time-sortable and globally unique.
- Model licensing is visible and tracked from import through the lifetime of the skill.
- Skill sync ensures new stones get the full skill set automatically.
- The async job pattern (submit → poll → retrieve) handles long-running operations.
- Mermaid diagrams give users visibility into pipeline internals without exposing
  ComfyUI's node editor complexity.
