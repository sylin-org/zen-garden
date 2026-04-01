---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0018: Skills, Workflow API, and Synthetic Capabilities

**Date**: 2026-03-31
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
    Skills: upscale, generate, img2img, inpaint, remove_bg

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

#### Skill definition

```rust
pub struct SkillDefinition {
    /// Unique name: "image.upscale", "image.generate", "speech.clone_voice"
    pub name: String,
    /// Parent capability for routing
    pub capability: Capability,
    /// Human-readable description
    pub description: String,
    /// What inputs the skill requires
    pub content_schema: Vec<ContentSlot>,
    /// Tuning parameters (JSON Schema + RJSF UI Schema)
    pub parameter_schema: FormSchema,
    /// Optional Mermaid diagram of the pipeline
    pub diagram: Option<String>,
    /// Models that must be installed for this skill to work
    pub required_models: Vec<ModelRef>,
    /// Provider-specific implementation data (e.g., ComfyUI workflow JSON)
    pub implementation: serde_json::Value,
}

pub struct ContentSlot {
    /// Role name: "source", "mask", "prompt", "negative"
    pub role: String,
    /// Content type: Image, Text
    pub content_type: ContentType,
    /// Whether this input is required
    pub required: bool,
}
```

Skills are **provider-agnostic**. ComfyUI implements them as workflow templates. Another
provider implements them as direct API calls. The orchestrator doesn't care — it dispatches
to the provider, which handles the implementation.

#### Built-in vs imported skills

**Built-in skills** are pre-registered with curated workflow templates, known-good
parameter schemas, and validated model requirements. They ship with the orchestrator.

**Imported skills** come from user-uploaded ComfyUI workflows (PNG with embedded metadata
or exported JSON). They go through an import flow before becoming available.

### 2. Workflow API — OpenAI-shaped request envelope

The API uses a familiar structure inspired by the OpenAI API design language, adapted
for pipeline operations.

#### Submit

```
POST /v1/workflows/run
```

```json
{
  "capability": "image",
  "skill": "upscale",
  "content": [
    { "type": "image", "url": "https://example.com/photo.png" }
  ],
  "parameters": {
    "scale": 4,
    "upscale_model": "4x-UltraSharp"
  }
}
```

**Content blocks** carry the inputs. Each block has:

| Field | Purpose |
|-------|---------|
| `type` | `image` or `text` |
| `role` | Optional disambiguator: `source`, `mask`, `prompt`, `negative` |
| `data` | Inline base64 (caller's choice) |
| `url` | URL reference — orchestrator fetches and caches (caller's choice) |

Content supports both inline (`data`) and URL (`url`) modes. Output is always a URL
to a cached asset.

#### Response (immediate)

```json
{
  "id": "job-019d45dc",
  "skill": "image.upscale",
  "status": "queued"
}
```

#### Poll status

```
GET /v1/workflows/jobs/{id}
```

```json
{
  "id": "job-019d45dc",
  "skill": "image.upscale",
  "status": "running",
  "progress": 0.65
}
```

#### Completed

```json
{
  "id": "job-019d45dc",
  "skill": "image.upscale",
  "status": "completed",
  "content": [
    { "type": "image", "format": "png", "url": "/v1/workflows/assets/019d45dc-result.png" }
  ],
  "usage": {
    "duration_ms": 3200
  }
}
```

#### Failed

```json
{
  "id": "job-019d45dc",
  "skill": "image.upscale",
  "status": "failed",
  "error": {
    "code": "model_not_found",
    "message": "Upscale model '4x-UltraSharp.pth' not installed"
  }
}
```

#### Statuses

`queued` → `running` → `completed` | `failed`

#### Streaming progress

```
GET /v1/workflows/jobs/{id}/stream
```

SSE events with progress updates. Same pattern as Moss install job streams.

#### Asset retrieval

```
GET /v1/workflows/assets/{id}
```

Binary response with proper `Content-Type`. Cached with TTL, caller downloads if they
need permanent storage.

#### Additional endpoints

```
GET  /v1/skills                     — list all registered skills
GET  /v1/skills/{skill}/form        — schema + diagram for TryIt UI
POST /v1/skills/import              — import a community workflow
GET  /v1/workflows/jobs             — list recent jobs
```

### 3. Skill TryIt — dashboard integration

The skill form endpoint returns everything the dashboard needs:

```
GET /v1/skills/image.upscale/form
```

```json
{
  "schema": {
    "type": "object",
    "properties": {
      "scale": { "type": "integer", "enum": [2, 4], "default": 4 },
      "upscale_model": { "type": "string", "enum": ["4x-UltraSharp", "RealESRGAN_x4plus"] }
    }
  },
  "ui_schema": {
    "scale": { "ui:widget": "radio" },
    "upscale_model": { "ui:widget": "select" }
  },
  "content": [
    { "role": "source", "type": "image", "required": true }
  ],
  "diagram": "graph LR\n    A[Load Image] --> B[Upscale 4x]\n    B --> C[Save Image]"
}
```

The dashboard renders:
- **Mermaid diagram** (optional — only when the provider returns one)
- **RJSF form** from schema + ui_schema
- **Content upload zones** from content slots
- **Submit → progress bar → result display**

The same `SkillTryIt` component works for every skill. No per-skill frontend code.
The `diagram` field is `Option` — when absent, nothing renders. Simple skills
(text in → text out) show just the form. Complex pipelines (ComfyUI) show the graph.

The existing model TryIt (`GET /v1/models/{model}/form`) returns the same shape.
Both use the same dashboard component, fed different data.

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
  "import_id": "imp-abc123",
  "name": "anime_style_transfer",
  "capability": "image",
  "required_models": [
    { "name": "animagine-xl-3.1.safetensors", "size_gb": 6.4, "license": "Fair AI Public License 1.0", "source": "civitai", "installed": false },
    { "name": "anime-style-lora.safetensors", "size_gb": 0.14, "license": "CC-BY-NC-4.0", "source": "civitai", "installed": false },
    { "name": "4x-UltraSharp.pth", "size_gb": 0.065, "license": "MIT", "source": "github", "installed": true }
  ],
  "detected_parameters": [
    { "name": "prompt", "type": "text", "required": true, "expose": true },
    { "name": "negative", "type": "text", "required": false, "expose": true, "default": "blurry, watermark" },
    { "name": "strength", "type": "number", "required": false, "expose": true, "default": 0.7, "min": 0.0, "max": 1.0 },
    { "name": "steps", "type": "number", "required": false, "expose": false, "default": 30 },
    { "name": "cfg", "type": "number", "required": false, "expose": false, "default": 7.5 },
    { "name": "sampler", "type": "string", "required": false, "expose": false, "default": "dpmpp_2m" }
  ],
  "total_download_gb": 6.54,
  "diagram": "graph LR\n    A[Prompt] --> B[SDXL Checkpoint]\n    B --> C[LoRA]\n    C --> D[KSampler]\n    D --> E[VAE Decode]\n    E --> F[Upscale 4x]\n    F --> G[Output]"
}
```

The user reviews:
- Which models to download (with sizes and licenses shown)
- Which parameters to expose vs hide (pre-selected by the orchestrator)
- The pipeline diagram

#### Step 3 — Confirm and install

```
POST /v1/skills/import/{import_id}/confirm
```

```json
{
  "exposed_parameters": ["prompt", "negative", "strength"],
  "accept_licenses": true
}
```

The orchestrator creates a multi-step job:
1. Download missing models to ComfyUI instances
2. Push workflow template to each instance
3. Register the skill in the skill registry
4. Publish to the dashboard

The skill is available once at least one instance has all required models.

#### License visibility

The dashboard model inventory permanently shows license information:

| Model | Size | License | Used by |
|-------|------|---------|---------|
| SDXL Base 1.0 | 6.4 GB | CreativeML Open RAIL-M | generate, img2img |
| 4x-UltraSharp | 65 MB | MIT | upscale |
| anime-style LoRA | 144 MB | CC-BY-NC-4.0 | anime_style_transfer |

Models used by zero skills are flagged for cleanup.

### 5. Skill sync across stones

When a new ComfyUI instance joins the garden, the orchestrator replicates skills to it:

1. Compare installed models/workflows against the skill registry
2. Transfer missing models (from peer stones or original sources)
3. Push workflow templates
4. Instance starts advertising the skills

This uses the existing `Provider::sync_resource()` trait method. The pattern is the same
as Ollama model sync — just models + workflow templates instead of just models.

Sync is automatic on instance join and can be triggered manually via the dashboard.

### 6. Provider trait extension

The Provider trait gains one method:

```rust
fn workflow(
    &self,
    ctx: &ProviderContext,
    skill: &str,
    content: Vec<ContentBlock>,
    parameters: serde_json::Value,
) -> BoxFuture<'_, Result<WorkflowJob>>;
```

Default implementation returns "not supported." Only providers with pipeline capabilities
(ComfyUI initially) override it. The existing `infer`, `embed`, `speak`, `transcribe`
methods are unchanged.

### 7. Routing

Skill requests route through the existing `select_instance()` algorithm:

1. `capability: image` + `skill: upscale` → filter instances that advertise this skill
2. Standard routing: health check, VRAM tier, queue depth, fitness score
3. Dispatch to the selected instance's provider

The skill name is an additional filter on the existing capability-based routing. An
instance that has `Capability::Image` but lacks the `upscale` skill is not a candidate.

---

## Example skills across providers

| Provider | Capability | Skills |
|----------|-----------|--------|
| ComfyUI | Image | upscale, generate, img2img, inpaint, remove_bg |
| Ollama | Chat, Vision, Tools, Embed | (model-level, no skills needed) |
| Speaches | Speech, Transcribe | synthesize, clone_voice, transcribe |
| Docling | Ocr | extract, convert |
| Kokoro | Speech | synthesize (high-quality voices) |
| Infinity | Embed, Rerank | embed, rerank |

Not every provider needs skills. Ollama's chat completions are fully described by
the model + capability — no skill layer required. Skills add value when a provider
supports multiple distinct operations within a single capability.

---

## Consequences

- ComfyUI becomes a first-class citizen in the orchestrator, not just a proxied service.
- Complex image operations (upscale, inpaint, generate) are exposed as clean API
  endpoints without leaking ComfyUI internals (node graphs, workflow JSON).
- Community workflows can be imported and published as new skills without code changes.
- The dashboard TryIt UI works for skills out of the box via schema-driven rendering.
- Model licensing is visible and tracked from import through the lifetime of the skill.
- Skill sync ensures new stones get the full skill set automatically.
- The async job pattern (submit → poll → retrieve) handles long-running operations
  that the synchronous inference API cannot.
- The Provider trait extension is minimal (one method) and backward-compatible.
- Mermaid diagrams give users visibility into pipeline internals without exposing
  ComfyUI's node editor complexity.
