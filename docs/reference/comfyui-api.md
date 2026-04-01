# ComfyUI API Reference — Observed Behavior

> Research notes from live instance at `192.168.1.119:8188` (stone-quiet-lens).
> ComfyUI v0.18.2, PyTorch 2.11.0+cu130, NVIDIA RTX 3060 Ti (8GB VRAM).
> Used for building the ComfyUI provider (ORCH-0018).

---

## Health / System Info

### `GET /system_stats`

Returns system info, device list, VRAM state. Use for probe.

```json
{
  "system": {
    "os": "linux",
    "ram_total": 16292962304,
    "ram_free": 14670872576,
    "comfyui_version": "0.18.2",
    "python_version": "3.13.12 (...)",
    "pytorch_version": "2.11.0+cu130",
    "embedded_python": false,
    "argv": ["./ComfyUI/main.py", "--listen", "--port", "8188", ...]
  },
  "devices": [
    {
      "name": "cuda:0 NVIDIA GeForce RTX 3060 Ti : cudaMallocAsync",
      "type": "cuda",
      "index": 0,
      "vram_total": 8589410304,
      "vram_free": 7472152576,
      "torch_vram_total": 0,
      "torch_vram_free": 0
    }
  ]
}
```

**Key fields for provider:**
- `system.comfyui_version` → ProbeResult.version
- `devices[].type` → capability detection (cuda, cpu, mps)
- `devices[].vram_total` / `vram_free` → VRAM tracking

---

## Model Listing

### `GET /models/{type}`

Returns an array of filenames (strings) for each model type. Empty array if none installed.

**Observed model types:**
- `/models/checkpoints` → `[]` (empty — fresh install)
- `/models/upscale_models` → `[]`
- `/models/loras` → `[]`
- `/models/vae` → `[]`

Other types (not tested but documented): `clip`, `clip_vision`, `controlnet`,
`diffusers`, `embeddings`, `gligen`, `hypernetworks`, `style_models`,
`text_encoders`, `unet`.

**Format:** plain JSON array of filename strings (no metadata):
```json
["4x-UltraSharp.pth", "RealESRGAN_x4plus.pth"]
```

No size, no hash, no metadata. Just filenames relative to the model directory.

---

## Node Info

### `GET /object_info`

Returns ALL registered node types with their input/output schemas.
Very large response (~hundreds of nodes). Use for capability discovery.

### `GET /object_info/{node_name}`

Returns schema for a single node. Key nodes for skills:

#### `UpscaleModelLoader`
```json
{
  "input": {
    "required": {
      "model_name": ["COMBO", {"multiselect": false, "options": []}]
    }
  },
  "output": ["UPSCALE_MODEL"],
  "display_name": "Load Upscale Model",
  "category": "loaders"
}
```
The `options` array is empty when no upscale models are installed. It populates
dynamically with filenames from `models/upscale_models/`.

#### `ImageUpscaleWithModel`
```json
{
  "input": {
    "required": {
      "upscale_model": ["UPSCALE_MODEL", {}],
      "image": ["IMAGE", {}]
    }
  },
  "output": ["IMAGE"],
  "display_name": "Upscale Image (using Model)",
  "category": "image/upscaling"
}
```
Takes an UPSCALE_MODEL and IMAGE, outputs IMAGE.

#### `LoadImage`
```json
{
  "input": {
    "required": {
      "image": [["example.png"], {"image_upload": true}]
    }
  },
  "output": ["IMAGE", "MASK"],
  "display_name": "Load Image",
  "category": "image"
}
```
The `image_upload: true` flag indicates this node accepts uploaded images.
The options array lists previously uploaded filenames.

#### `SaveImage`
```json
{
  "input": {
    "required": {
      "images": ["IMAGE", {}],
      "filename_prefix": ["STRING", {"default": "ComfyUI"}]
    },
    "hidden": {
      "prompt": "PROMPT",
      "extra_pnginfo": "EXTRA_PNGINFO"
    }
  },
  "output": [],
  "output_node": true,
  "display_name": "Save Image",
  "category": "image"
}
```
`output_node: true` marks this as a terminal node.

---

## Queue Management

### `GET /queue`

```json
{
  "queue_running": [],
  "queue_pending": []
}
```

Each entry contains the prompt ID and workflow details.

### `GET /history`

Returns completed prompts. Empty object `{}` when no history.

### `GET /history/{prompt_id}`

Returns a single prompt's execution history and outputs.

---

## Workflow Submission

### `POST /prompt`

Submit a workflow for execution. Returns a `prompt_id` for tracking.

**Request:**
```json
{
  "prompt": {
    "1": {"class_type": "LoadImage", "inputs": {"image": "uploaded.png"}},
    "2": {"class_type": "UpscaleModelLoader", "inputs": {"model_name": "4x-UltraSharp.pth"}},
    "3": {"class_type": "ImageUpscaleWithModel", "inputs": {"upscale_model": ["2", 0], "image": ["1", 0]}},
    "4": {"class_type": "SaveImage", "inputs": {"images": ["3", 0], "filename_prefix": "upscaled"}}
  },
  "client_id": "<uuid>"
}
```

**Response:**
```json
{
  "prompt_id": "<uuid>",
  "number": 1
}
```

**Node references:** `["2", 0]` means "output 0 from node 2". This is how
edges are encoded in the flat JSON graph.

**client_id:** Optional UUID. If provided, WebSocket progress events are
sent to clients connected with this ID.

---

## Image Upload

### `POST /upload/image`

Multipart form upload. Required before referencing in LoadImage node.

**Request:** `multipart/form-data` with:
- `image`: file binary
- `overwrite`: "true"/"false" (optional)
- `subfolder`: directory within input/ (optional)
- `type`: "input" (default), "temp", or "output"

**Response:**
```json
{
  "name": "uploaded_filename.png",
  "subfolder": "",
  "type": "input"
}
```

The returned `name` is what goes into `LoadImage.inputs.image`.

---

## Output Retrieval

### `GET /view?filename={name}&type=output&subfolder={sub}`

Returns the binary image file. Parameters:
- `filename`: output filename (from history)
- `type`: "output" (generated), "input" (uploaded), "temp"
- `subfolder`: optional directory

Response: binary image with `Content-Type: image/png` (or jpeg, webp, etc.)

---

## WebSocket Progress

### `WS /ws?clientId={uuid}`

Real-time progress events. Key event types:

```json
{"type": "status", "data": {"status": {"exec_info": {"queue_remaining": 0}}}}
{"type": "execution_start", "data": {"prompt_id": "..."}}
{"type": "executing", "data": {"node": "3", "prompt_id": "..."}}
{"type": "progress", "data": {"value": 5, "max": 20, "prompt_id": "...", "node": "3"}}
{"type": "executed", "data": {"node": "4", "output": {"images": [{"filename": "...", "subfolder": "", "type": "output"}]}, "prompt_id": "..."}}
{"type": "execution_complete", "data": {"prompt_id": "..."}}
```

**Key events for job tracking:**
- `execution_start` → job status = Running
- `progress` → job progress = value/max
- `executed` on output nodes → capture output filenames
- `execution_complete` → job status = Completed
- `execution_error` → job status = Failed

---

## Existing Orchestrator Integration Points

### Provider Trait Pattern

From `catalog/traits.rs`. Every provider implements:
- `kind()` → `OfferingKind` enum variant
- `capabilities()` → static capability list
- `discovery()` → `TopologyFilter { offering_name }` for local providers
- `probe()` → health check, returns version + capabilities + VRAM
- `enumerate()` → list models/resources as `Vec<ServiceModel>`
- Inference methods with default "not supported"

### Registration

In `main.rs` line 79-90: `providers.register(Arc::new(XxxProvider::new()))`.
ComfyUI needs to be added here.

### Discovery

`tasks/discovery.rs` filters topology by offering name. ComfyUI instances
appear as `offering_type: "comfyui"` in the Moss topology.

### Proxy

ComfyUI is already in the `generic_proxy_kinds` list (main.rs:252) with
proxy port 21435.

### Job System

`OrchestratorJob` in `domain/types.rs` has `JobKind` enum. Workflow jobs
would add a new variant: `WorkflowRun { skill, prompt_id }`.

`JobStatus`: Queued, Running, Completed, Failed — matches our design exactly.

### Existing Provider Count

11 providers registered. ComfyUI would be #12.

---

## Development Notes

### What ComfyUI does NOT have:
- No model download API (models must be pre-placed in volumes)
- No workflow validation endpoint (fails at runtime)
- No model metadata (just filenames, no sizes/hashes)
- No authentication

### What ComfyUI DOES have:
- Real-time WebSocket progress with per-node granularity
- Full node type introspection via `/object_info`
- Queue management (pending + running)
- History with full output references
- PNG metadata embedding (workflows travel with output images)
- ComfyUI-Manager plugin for node/extension management

### Upscale workflow — minimal viable API format

```json
{
  "prompt": {
    "load_image":   {"class_type": "LoadImage",              "inputs": {"image": "PLACEHOLDER"}},
    "load_model":   {"class_type": "UpscaleModelLoader",     "inputs": {"model_name": "PLACEHOLDER"}},
    "upscale":      {"class_type": "ImageUpscaleWithModel",  "inputs": {"upscale_model": ["load_model", 0], "image": ["load_image", 0]}},
    "save":         {"class_type": "SaveImage",              "inputs": {"images": ["upscale", 0], "filename_prefix": "zen-upscale"}}
  }
}
```

Placeholders to fill at runtime:
- `load_image.inputs.image` → uploaded filename (from POST /upload/image)
- `load_model.inputs.model_name` → selected upscale model filename

Mermaid diagram:
```
graph LR
    A[Load Image] --> C[Upscale]
    B[Load Upscale Model] --> C
    C --> D[Save Image]
```
