# Skill Authoring Guide

How to create skills for the Zen Garden AI Orchestrator.

A skill is a **JSON file** — not code. It declares what the user sees (form fields),
what content the user provides (images, text), and how those inputs map to workflow
templates. The orchestrator renders the form; the provider executes the workflow.

---

## Anatomy of a Skill

```json
{
  "name": "image.upscale",
  "display_name": "Upscale",
  "capability": "image",
  "description": "Enhance image resolution using AI super-resolution",
  "provider_kind": "comfyui",
  "vram_mb": 1024,
  "default_workflow": "upscale_4x",
  "content_slots": [ ... ],
  "mappings": [ ... ],
  "required_models": [ ... ]
}
```

| Field | Purpose |
|-------|---------|
| `name` | Unique identifier: `{capability}.{moniker}`. Used in API paths. |
| `display_name` | What the user sees in the dashboard. |
| `capability` | Parent capability: `image`, `speech`, `ocr`, etc. |
| `description` | One-line subtitle shown below the skill name. |
| `provider_kind` | Which provider executes this skill: `comfyui`, `speaches`, etc. |
| `vram_mb` | Minimum GPU VRAM required (MB). |
| `default_workflow` | Name of the workflow template to use when no override is specified. |
| `content_slots` | What the user provides (images, text). |
| `mappings` | How user inputs connect to the workflow. The form AND the execution engine. |
| `required_models` | Models that must be installed for the skill to work. |

---

## Content Slots

Content slots declare what the user provides. Each slot has a role, type, and
whether it's required.

```json
"content_slots": [
  { "role": "source", "content_type": "image", "required": true },
  { "role": "prompt", "content_type": "text", "required": true }
]
```

| Content type | Dashboard rendering |
|-------------|---------------------|
| `image` | Drag-and-drop dropzone |
| `text` | Textarea |

The `role` field connects the slot to a content mapping (see below).

---

## Mappings

Mappings are the core of a skill. They serve two purposes simultaneously:
1. **Form rendering** — the dashboard reads mappings to build the UI
2. **Execution** — the provider reads mappings to fill the workflow template

### Content Mapping

Maps user-provided content to a placeholder in the workflow template.

```json
{
  "type": "content",
  "role": "source",
  "content_type": "image",
  "placeholder": "PLACEHOLDER_IMAGE"
}
```

- `role` matches a `content_slots` entry
- `content_type` determines handling:
  - `image` → uploaded to the provider first, placeholder replaced with the uploaded filename
  - `text` → placeholder replaced with the text value directly
- `placeholder` is the literal string in the workflow JSON that gets replaced

### Param Mapping

Maps a form field to a value in the workflow.

```json
{
  "type": "param",
  "field": "upscale_model",
  "label": "Style",
  "param_type": "options",
  "placeholder": "PLACEHOLDER_MODEL",
  "default": "RealESRGAN_x4plus.pth",
  "options": [
    { "value": "RealESRGAN_x4plus.pth", "label": "Realistic" },
    { "value": "RealESRGAN_x4plus_anime_6B.pth", "label": "Anime" }
  ]
}
```

A param mapping can target the workflow in two ways:

| Method | Fields | When to use |
|--------|--------|-------------|
| Placeholder substitution | `placeholder` | String values that appear literally in the JSON template |
| Node input | `node` + `input` | Numeric/typed values on a specific node |

### Param Types

#### `options` — Selection from a list

```json
{
  "param_type": "options",
  "options": [
    { "value": "RealESRGAN_x4plus.pth", "label": "Realistic" },
    { "value": "RealESRGAN_x4plus_anime_6B.pth", "label": "Anime" }
  ]
}
```

- ≤4 options → rendered as radio buttons
- \>4 options → rendered as a dropdown
- When `label` is absent, the value itself is displayed (e.g., `512`, `768`, `1024`)

#### `range` — Numeric slider

```json
{
  "param_type": "range",
  "min": 0.0,
  "max": 1.0,
  "step": 0.05
}
```

#### `auto` — Auto-generated, user-editable

```json
{
  "param_type": "auto",
  "kind": "random_int"
}
```

Pre-filled with a generated value. The user can edit it (e.g., to reproduce a
specific result with a known seed). A re-roll button generates a new value.

#### `text` — Free text input

```json
{
  "param_type": "text"
}
```

Rendered as a textarea. Common for negative prompts.

---

## The `workflow` Parameter (Template Selection)

A skill has a `default_workflow` — the template used for most invocations.

To let the user choose between workflow variants, add a param mapping with
`"field": "workflow"`:

```json
{
  "type": "param",
  "field": "workflow",
  "label": "Zoom",
  "param_type": "options",
  "default": "upscale_4x",
  "options": [
    { "value": "upscale_2x",  "label": "2x" },
    { "value": "upscale_4x",  "label": "4x" },
    { "value": "upscale_8x",  "label": "8x" },
    { "value": "upscale_16x", "label": "16x" }
  ]
}
```

The user sees "Zoom: 2x / 4x / 8x / 16x". The provider receives a template name
and loads it. All other mappings (content, params) apply identically to whichever
template was selected.

The `workflow` field name is reserved — the provider always checks for it before
loading the default template.

---

## Workflow Templates

Workflow templates are provider-specific. For ComfyUI, they are the standard
ComfyUI API-format JSON (node graph with numbered node IDs).

Placeholders are literal strings in the JSON that get substituted at execution time:

```json
{
  "1": {
    "class_type": "LoadImage",
    "inputs": { "image": "PLACEHOLDER_IMAGE" }
  },
  "2": {
    "class_type": "UpscaleModelLoader",
    "inputs": { "model_name": "PLACEHOLDER_MODEL" }
  }
}
```

Rules:
- Use `PLACEHOLDER_` prefix for all placeholders (convention, not enforced)
- Placeholders are replaced by string substitution throughout the entire JSON tree
- Node-targeted params use `node` + `input` to set a specific value by node ID
- Templates must be valid ComfyUI API-format JSON

---

## Required Models

List every model the skill needs. The orchestrator provisions them automatically.

```json
"required_models": [
  {
    "filename": "RealESRGAN_x4plus.pth",
    "model_type": "upscale_models",
    "description": "General-purpose 4x upscaler"
  }
]
```

- `filename` — exact filename as ComfyUI expects it
- `model_type` — ComfyUI model subdirectory: `checkpoints`, `upscale_models`, `loras`, `vae`, etc.
- `description` — shown in the dashboard model inventory

The orchestrator downloads models from known sources and pushes them to instances
via the Moss volume API. A skill becomes available once at least one instance has
all required models installed.

---

## Complete Example: Upscale

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

Four workflow templates (`upscale_2x.json`, `upscale_4x.json`, `upscale_8x.json`,
`upscale_16x.json`) live alongside the skill definition. Each is a valid ComfyUI
node graph with `PLACEHOLDER_IMAGE` and `PLACEHOLDER_MODEL` placeholders.

---

## Complete Example: Generate (text-to-image)

```json
{
  "name": "image.generate",
  "display_name": "Generate",
  "capability": "image",
  "description": "Create an image from a text description",
  "provider_kind": "comfyui",
  "vram_mb": 4096,
  "default_workflow": "generate",

  "content_slots": [
    { "role": "prompt", "content_type": "text", "required": true }
  ],

  "mappings": [
    { "type": "content", "role": "prompt", "content_type": "text",
      "placeholder": "PLACEHOLDER_PROMPT" },

    { "type": "param", "field": "negative", "label": "Negative Prompt",
      "param_type": "text",
      "default": "blurry, watermark, low quality, deformed" },

    { "type": "param", "field": "checkpoint", "label": "Model",
      "placeholder": "PLACEHOLDER_CHECKPOINT", "param_type": "options",
      "options": [
        { "value": "v1-5-pruned-emaonly.safetensors" }
      ]},

    { "type": "param", "field": "width", "label": "Width",
      "node": "4", "input": "width", "param_type": "options",
      "default": 512,
      "options": [{ "value": 512 }, { "value": 768 }, { "value": 1024 }] },

    { "type": "param", "field": "height", "label": "Height",
      "node": "4", "input": "height", "param_type": "options",
      "default": 512,
      "options": [{ "value": 512 }, { "value": 768 }, { "value": 1024 }] },

    { "type": "param", "field": "steps", "label": "Steps",
      "node": "5", "input": "steps", "param_type": "range",
      "min": 1, "max": 50, "step": 1, "default": 20 },

    { "type": "param", "field": "seed", "label": "Seed",
      "node": "5", "input": "seed", "param_type": "auto",
      "kind": "random_int" }
  ],

  "required_models": [
    { "filename": "v1-5-pruned-emaonly.safetensors", "model_type": "checkpoints" }
  ]
}
```

---

## Execution Flow

1. User submits `POST /v1/image/skill/upscale` with content + parameters
2. Orchestrator looks up the `image.upscale` skill definition
3. Provider reads `parameters.workflow` → selects template (or uses `default_workflow`)
4. Provider iterates mappings:
   - Content mappings: upload images, substitute text placeholders
   - Param mappings: substitute placeholders or set node inputs
5. Provider submits the filled template to ComfyUI
6. Provider polls for completion, extracts output
7. Orchestrator returns result with proxied asset URLs

---

## Importing Community Workflows

Community ComfyUI workflows (PNG with embedded metadata or exported JSON) can be
imported as new skills:

1. Upload the workflow via `POST /v1/skills/import`
2. The orchestrator parses the node graph, identifies models and input nodes
3. User reviews the detected parameters, chooses which to expose
4. Orchestrator generates the skill JSON + stores the workflow template
5. Models are downloaded and pushed to instances
6. Skill becomes available once an instance is ready

The import process generates the same JSON structure described above. No code
changes needed — the skill is data, the execution engine is generic.
