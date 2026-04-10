# AI Orchestrator — API Reference

> Complete usage guide for the Zen Garden AI Orchestrator REST API.
> Covers every endpoint, request/response shapes, patterns, and testing recipes.

---

## Base URL

```
http://<host>:7190
```

When running in Docker (default), the orchestrator listens on port 7190. All endpoints are prefixed with `/v1/` except `/health` and `/metrics`.

---

## Discovery

The orchestrator discovers the garden automatically via Koi mDNS. On startup it:

1. Queries `http://host.docker.internal:5641/v1/mdns/discover?type=_moss._tcp` to find stones.
2. Picks the first healthy stone as the tended stone.
3. Subscribes to the tended stone's `/api/v1/garden/tools/stream` SSE endpoint.
4. As tool events arrive, adapters receive instance URLs and probe them.
5. Each adapter publishes a capability announcement to the internal event bus.
6. The Directory rebuilds its view; the catalog updates.

No manual configuration is needed. The orchestrator fills in providers as the garden reports them.

---

## Endpoints at a glance

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health` | Health check |
| GET | `/v1/` | Sitemap — lists all endpoint families |
| GET | `/v1/catalog` | Navigation summary — what can I call? |
| GET | `/v1/catalog/{modality}/{leaf}[/{skill}]` | Full form schema for one registration |
| POST | `/v1/do` | Universal dispatch — single action or flow |
| POST | `/v1/{modality}/{leaf}` | Sugar — single action by primitive |
| POST | `/v1/{modality}/{leaf}/{skill}` | Sugar — invoke a named skill |
| GET | `/v1/{modality}/{leaf}` | Introspect a primitive |
| GET | `/v1/{modality}/{leaf}/{skill}` | Introspect a skill |
| GET | `/v1/events` | SSE event stream with glob-based focus |
| GET | `/v1/skills` | List all skills |
| GET | `/v1/skills/{id}` | Inspect one skill |
| DELETE | `/v1/skills/{id}` | Delete a skill |
| POST | `/v1/skills/{provider}/import` | Import a skill from URL/PNG/text |
| GET | `/v1/jobs` | List all jobs |
| GET | `/v1/jobs/{id}` | Inspect one job |
| DELETE | `/v1/jobs/{id}` | Cancel a job |
| POST | `/v1/media` | Upload media |
| GET | `/v1/media/{id}` | Download media |
| GET | `/v1/preferences` | Get global preferences |
| PUT | `/v1/preferences` | Set global preferences (merge) |
| DELETE | `/v1/preferences/{key}` | Remove a preference |
| GET | `/v1/resources` | Stone resource overview |
| GET | `/v1/resources/stones/{name}` | One stone's resources |

---

## Health

```bash
curl http://localhost:7190/health
```

```json
{
  "directory_version": 8,
  "providers_enabled": 7,
  "providers_registered": 7,
  "status": "ok"
}
```

- `providers_registered`: adapters that exist (may be probing or disabled).
- `providers_enabled`: adapters with at least one healthy instance and declared capabilities.
- `status`: always `"ok"` from boot (eventually consistent).

---

## Catalog

### Navigation summary

```bash
curl http://localhost:7190/v1/catalog
```

Returns a compact listing of every primitive and skill registered in the garden. Use this to render a sidebar, a command palette, or a picker.

```json
{
  "primitives": [
    {
      "action": "text.chat",
      "modality": "text",
      "summary": "Conversational text completion with optional tool calling.",
      "providers": [{"name": "ollama", "media_inputs": []}]
    }
  ],
  "skills": [
    {
      "action": "image.generate.flux-butterfly",
      "primitive": "image.generate",
      "id": "flux-butterfly",
      "display": {"name": "Flux Butterfly", "description": "..."},
      "provider": "comfyui",
      "parameters": [...]
    }
  ],
  "providers": [
    {"name": "ollama", "enabled": true, "capability_count": 3, "skill_count": 0}
  ]
}
```

Supports `ETag` / `If-None-Match` for conditional requests.

### Full schema (Try It form)

```bash
# Base primitive
curl http://localhost:7190/v1/catalog/text/chat

# Skill
curl http://localhost:7190/v1/catalog/image/generate/flux-butterfly
```

The URL grammar mirrors dispatch: `/v1/catalog/{modality}/{leaf}[/{skill}]`. A client that knows how to call `POST /v1/image/generate/flux-butterfly` already knows the catalog URL — change POST to GET and prefix with `/catalog`.

Returns the complete field list with types, widgets, constraints, and defaults for one registration. This is everything a client needs to render a form.

```json
{
  "path": "text.chat",
  "kind": "primitive",
  "display_name": "Conversational text completion...",
  "providers": ["ollama"],
  "fields": [
    {
      "field": "text.prompt.user",
      "label": "Message",
      "field_type": "string",
      "widget": "textarea",
      "required": true,
      "placeholder": "Ask anything..."
    },
    {
      "field": "text.sampling.temperature",
      "label": "Temperature",
      "field_type": "number",
      "widget": "slider",
      "default": 0.7,
      "min": 0.0,
      "max": 2.0,
      "step": 0.1
    },
    {
      "field": "selectors.model",
      "label": "Model",
      "field_type": "string",
      "widget": "select",
      "auto": {
        "default": "recommended:chat",
        "description": "The garden picks the best available chat model"
      },
      "options": ["llama3.1:8b", "gemma3:12b", "magistral:24b", "..."]
    }
  ]
}
```

For skills: `curl http://localhost:7190/v1/catalog/image.generate.flux-butterfly`

Widget types: `textarea`, `slider`, `number`, `select`, `toggle`, `hidden`, `file`.

---

## Dispatching requests

### The universal verb: `/v1/do`

#### Single action

```bash
curl -X POST http://localhost:7190/v1/do \
  -H "Content-Type: application/json" \
  -d '{
    "action": "text.chat",
    "text": {
      "prompt": {
        "user": "What is the capital of France?"
      }
    }
  }'
```

#### Flow (multi-step)

```bash
curl -X POST http://localhost:7190/v1/do \
  -H "Content-Type: application/json" \
  -d '{
    "actions": [
      {
        "id": "transcribe",
        "action": "audio.transcribe",
        "payload": {"audio": {"source": {"media_id": "abc123"}}}
      },
      {
        "id": "summarize",
        "action": "text.chat",
        "payload": {
          "text": {"prompt": {"user": "Summarize:\n{{transcribe.text.response}}"}}
        }
      }
    ]
  }'
```

Flows execute steps sequentially. `{{step_id.field.path}}` placeholders are resolved from completed upstream results. On failure, remaining steps are skipped and partial results are preserved.

Flow response: `200` (all succeeded) or `207 Multi-Status` (partial).

```json
{
  "job_id": "019d7459-6554-...",
  "status": "completed",
  "steps": [
    {"id": "transcribe", "action": "audio.transcribe", "status": "completed", "result": {...}},
    {"id": "summarize", "action": "text.chat", "status": "completed", "result": {...}}
  ]
}
```

### REST sugar

These are shortcuts that pre-fill the action from the URL:

```bash
# Same as /v1/do with action: "text.chat"
curl -X POST http://localhost:7190/v1/text/chat \
  -H "Content-Type: application/json" \
  -d '{"text": {"prompt": {"user": "Hello"}}}'

# Invoke a specific skill
curl -X POST http://localhost:7190/v1/image/generate/flux-butterfly \
  -H "Content-Type: application/json" \
  -d '{"image": {"prompt": {"positive": "A butterfly in a garden"}}}'
```

### Response envelope

Every successful dispatch returns:

```json
{
  "output": {
    "text": {"response": "Paris", "finish_reason": "stop"},
    "usage": {"tokens": {"input": 22, "output": 3}},
    "timing": {"total_ms": 21345}
  },
  "_meta": {
    "correlation_id": "...",
    "request_id": "...",
    "action": "text.chat",
    "provider": "ollama",
    "mode": "sync",
    "received_at": "2026-04-09T22:25:08Z",
    "completed_at": "2026-04-09T22:25:30Z"
  }
}
```

Error responses:

```json
{
  "error": {
    "code": "timeout",
    "message": "timeout: error sending request for url (...)",
    "details": {"provider": "ollama"}
  },
  "_meta": {...}
}
```

Error codes: `validation_failed`, `no_provider`, `timeout`, `upstream_error`, `pin_not_servable`, `unsupported`.

---

## Primitives reference

| Primitive | Modality | Key input fields | Provider examples |
|-----------|----------|------------------|-------------------|
| `text.chat` | text | `text.prompt.user`, `text.prompt.system`, `text.sampling.temperature` | ollama |
| `text.translate` | text | `text.body`, `text.language.target`, `text.language.source` | libretranslate |
| `text.embed` | text | `text.input` (array) | ollama, infinity |
| `text.rerank` | text | `text.query`, `text.documents` | infinity |
| `image.generate` | image | `image.prompt.positive`, `image.prompt.negative`, `image.sampling.*` | comfyui |
| `image.edit` | image | `image.source`, `image.prompt.positive` | comfyui |
| `image.upscale` | image | `image.source` | comfyui |
| `image.analyze` | image | `image.source`, `text.prompt.user` | ollama, comfyui, docling |
| `audio.generate` | audio | `text.prompt.user`, `audio.voice`, `audio.speed` | kokoro, speaches |
| `audio.transcribe` | audio | `audio.source` | whispercpp, speaches |

Use `GET /v1/catalog/{primitive}` for the authoritative field list with types and constraints.

---

## Model selection

### Default: `recommended:*`

When you omit `selectors.model`, the orchestrator auto-selects. The adapter picks the best model from its live instances based on capability, warmth (model loaded in VRAM), queue depth, and stone pressure.

```bash
# No model specified — the garden decides
curl -X POST http://localhost:7190/v1/text/chat \
  -d '{"text": {"prompt": {"user": "Hello"}}}'
```

### Pinning a model

```bash
# Explicit model — routes only to instances that have it
curl -X POST http://localhost:7190/v1/text/chat \
  -d '{"text": {"prompt": {"user": "Hello"}}, "model": "llama3.1:8b"}'
```

If the pinned model isn't available on any healthy instance, the response is:

```json
{
  "error": {
    "code": "pin_not_servable",
    "message": "model is not installed on any instance in the garden",
    "details": {"model": "llama3.1:8b"}
  }
}
```

### Selecting a provider

```bash
# Force a specific provider
curl -X POST http://localhost:7190/v1/text/chat \
  -d '{"text": {"prompt": {"user": "Hello"}}, "provider": "ollama"}'
```

---

## Events (SSE)

Subscribe to the orchestrator's event stream for real-time updates:

```bash
# All events
curl -N http://localhost:7190/v1/events

# Only skill events
curl -N "http://localhost:7190/v1/events?focus=skills.*"

# Only failures anywhere
curl -N "http://localhost:7190/v1/events?focus=*.failed"

# Multiple patterns
curl -N "http://localhost:7190/v1/events?focus=skills.*,jobs.*,directory.*"

# Resume from a known position
curl -N -H "Last-Event-ID: 42" "http://localhost:7190/v1/events?focus=skills.*"
```

Event format:
```
event: skills.flux-butterfly.state
id: 48
data: {"topic":"skills.flux-butterfly.state","state":"ready"}
```

Topic families:
- `directory.provider.{name}.*` — provider health, capability changes
- `skills.{moniker}.*` — skill lifecycle (analyzing, naming, ready, failed)
- `jobs.{id}.*` — job state, progress, result
- `catalog.version` — catalog rebuilt
- `resources.stone.{name}.*` — GPU/memory pressure
- `preferences.changed` — preferences updated

---

## Skills

### List skills

```bash
curl http://localhost:7190/v1/skills

# Filter by provider
curl "http://localhost:7190/v1/skills?provider=comfyui"

# Filter by primitive
curl "http://localhost:7190/v1/skills?primitive=image.generate"
```

### Import a skill

```bash
# From a CivitAI image URL
curl -X POST http://localhost:7190/v1/skills/comfyui/import \
  -H "Content-Type: application/json" \
  -d '{"input": "https://civitai.com/images/126242620"}'
```

Returns `202 Accepted` with `Location` header:

```json
{
  "moniker": "flux-26239",
  "draft_dir": "/data/skills/comfyui/flux-26239"
}
```

The skill goes through: `analyzing` → `naming` (async AI naming) → `ready`. Subscribe to `skills.flux-26239.*` on the event stream to watch progress.

### Delete a skill

```bash
curl -X DELETE http://localhost:7190/v1/skills/flux-26239
```

---

## Preferences

Global defaults that auto-populate form fields and pre-fill dispatch requests.

```bash
# Get all preferences
curl http://localhost:7190/v1/preferences

# Set preferences (merge)
curl -X PUT http://localhost:7190/v1/preferences \
  -H "Content-Type: application/json" \
  -d '{
    "image.width": 1024,
    "image.height": 1024,
    "text.sampling.temperature": 0.7
  }'

# Remove one preference
curl -X DELETE http://localhost:7190/v1/preferences/image.width
```

Layering order: `caller payload > preferences > field default > recommended:*`

### How preferences layer into dispatch

When a request reaches the contextualizer, preferences inject values for every dotted field path the caller did not explicitly set. The check uses JSON Pointer resolution (`image.width` → `/image/width`): if the pointer resolves to nothing in the caller's payload, the preference value is injected with intermediate objects created as needed.

Example: the operator sets `text.sampling.temperature = 0.3` as a preference. A caller sends `{"text": {"prompt": {"user": "Hello"}}}` without specifying temperature. The contextualizer injects `0.3` before dispatch. If the caller explicitly sends `temperature: 1.5`, the caller's value wins — preferences never overwrite explicit input.

Preferences persist to `{data_dir}/preferences.json` and reload on startup. Every mutation publishes `preferences.changed` on the event bus so the catalog rebuilds with updated default values.

---

## Ollama orchestration

The Ollama adapter is the reference implementation of the bottom-up capability-event architecture. It discovers instances from the garden, probes them for models, builds a scoring matrix, and publishes capabilities to the Directory.

### Startup and discovery

1. The adapter subscribes to `GardenDiscovery` for FQN `"ollama"` (matches `ollama`, `ollama::adopted`, `ollama::dev`, etc.).
2. When discovery events arrive with instance URLs, the adapter probes each in parallel:
   - `/api/tags` → list of installed models (`models_available`)
   - `/api/ps` → list of currently loaded models in VRAM (`models_loaded`)
   - `/api/show` (per unique model) → capabilities, parameter count, context length from GGUF metadata
3. The results populate the `OllamaCapabilityMatrix` — an in-memory cross-product of instances, models, and their metadata.
4. The adapter publishes a `CapabilityAnnouncement` with:
   - Base primitives derived from the union of Ollama capability tags across all healthy instances
   - Full form-schema parameters per primitive (prompt, system prompt, temperature slider, max tokens, model selector with live `options` from the installed model list)

### Capability mapping

| Ollama tag | Primitives registered | `recommended:*` capability |
|---|---|---|
| `completion` | `text.chat` | `chat`, `quick`, `synthesis`, `thinking` |
| `embedding` | `text.embed` | `embed` |
| `vision` | `image.analyze` | `vision`, `ocr` |
| `tools` | (via `text.chat`) | `tools` |
| `thinking` | (via `text.chat`) | `thinking` |

A model with tags `[completion, tools, vision]` is eligible for chat, tools, and vision requests. The adapter never infers capabilities — it reads them from Ollama's `/api/show` response.

### Model selection: `recommended:*`

When a request arrives with `selectors.model = "recommended:chat"` (or absent, which defaults to `recommended:chat` for `text.chat`), the adapter's `OllamaSelector` scores every eligible model through five layers:

**Layer 0 — Availability (floor)**
- `+50` if the model is installed on any healthy instance
- `+10` per additional stone that has it (capped at `+30` for 4+ stones)
- `+20` warmth bonus if the model is currently loaded in VRAM on at least one instance

**Layer 1 — Capability filter (hard)**
- Models that don't declare the required Ollama tag are removed entirely, not deprioritized
- `chat` requests filter to `completion` tag; `embed` to `embedding`; `vision` to `vision`
- `tools` and `thinking` require exact tag match

**Layer 2 — Context window**
- Formula: `min(context_tokens / 1000, cap)`
- Caps per capability: Synthesis=500, Thinking=300, Tools=250, Vision=200, Chat=150, Quick=0
- A 128K-context model beats an 8K model by ~120 points for synthesis

**Layer 3 — Quality (parameter count)**
- Formula: `min(params_billions * multiplier, cap)`
- Multipliers: Thinking=60/B, Tools/Vision=50/B, Chat/Synthesis=40/B, OCR=15/B
- OCR has a deliberately low multiplier: a purpose-built 1B OCR model beats a generic 13B
- Quality cap per capability: Thinking=500, Tools/Vision=450, Chat/Synthesis=400, Quick=0

**Layer 4 — Name affinity**
- If the model name contains the capability keyword, bonus applies
- Currently only OCR: `+300` for models with "ocr" in the name

After scoring, models are sorted by score descending (ties broken by name for determinism). The top model is paired with its best instance using warmth-first, then lowest queue depth, then stable by stone name.

### Model selection: pinned

When a request specifies `model: "llama3.1:8b"`, scoring is bypassed entirely. The adapter looks up which healthy instances have the model installed, picks the one with lowest queue depth (warm preferred), and dispatches. If no healthy instance has the model:

- Model exists somewhere but all instances are unhealthy → `PinNotServable { reason: "no healthy instance currently hosts this model" }`
- Model is completely unknown → `PinNotServable { reason: "model is not installed on any instance in the garden" }`

### Anti-phantom invariant

The selector's first step is to compute the **loadable model set**: the union of `models_available` across all healthy instances. Any model in the metadata catalog that doesn't appear in this set is invisible to recommendation. This prevents the phantom-model bug where the engine recommends a model no instance can actually serve.

### Live model options in the catalog

The capability announcement carries the sorted list of loadable model names as `options` on the `selectors.model` field descriptor. When a dashboard renders `GET /v1/catalog/text/chat`, the model selector dropdown shows exactly the models currently available in the garden — not a static list. When models are pulled or evicted, the adapter republishes and the catalog updates.

---

## ComfyUI skill import

The import pipeline accepts a URL, PNG, or raw text and produces a ready-to-use skill definition on disk.

### Accepted input types

| Input | Detection | Example |
|-------|-----------|---------|
| CivitAI image | `civitai.com/images/<id>` URL | `https://civitai.com/images/126242620` |
| CivitAI model | `civitai.com/models/<id>` URL | `https://civitai.com/models/12345` |
| PNG with embedded workflow | `.png` URL or raw PNG bytes | Any ComfyUI-generated PNG |
| Generic URL | Any HTTP URL (tried as JSON workflow) | `https://example.com/workflow.json` |
| Raw workflow JSON | JSON with `class_type` fields | `{"1": {"class_type": "KSampler", ...}}` |
| Generation text | Text containing `Negative prompt:`, `Steps:`, etc. | Pasted from Automatic1111 |

### Pipeline stages

```
Input → Classify → Extract → Parse → Extract params → Resolve models → Build draft
```

1. **Classify** (`input_detect`) — regex/parse determines input type, no I/O.

2. **Extract** — each input type has its own extraction path:
   - CivitAI image → fetch image metadata API → extract embedded workflow or generation parameters from the `meta` field
   - CivitAI model → fetch model page → download associated workflow file
   - PNG → download bytes → extract `tEXt`/`zTXt`/`iTXt` PNG chunks containing the workflow
   - Generation text → parse A1111-style parameters, then synthesize a txt2img workflow

3. **Parse** (`workflow_parser`) — analyze the ComfyUI node graph. Identify KSampler, model loaders, output nodes. Build a dependency tree.

4. **Extract parameters** (`param_extract`) — plant `PLACEHOLDER_*` tokens into the workflow for every user-configurable field. Emit typed `Binding` objects with field paths, defaults, labels, and `FieldConstraint` narrows (range, options, auto).
   - KSampler-driven role detection: positive/negative prompt roles determined by tracing connections from the KSampler's conditioning inputs back through CLIP text encode nodes
   - Checkpoint and LoRA loaders become model selector entries
   - Numeric parameters (steps, guidance, seed) get range constraints from the node definition

5. **Resolve models** (`model_resolve`) — 5-level cascade to find download URLs:
   1. Exact filename match in local cache
   2. Known-models database (86 common community models with CivitAI URLs)
   3. CivitAI hash lookup (SHA256 of the safetensors file)
   4. CivitAI model version search
   5. Filename-based web search
   Unresolvable checkpoints are hard failures; other models (LoRAs, upscalers) produce warnings.

6. **Build draft** (`draft_builder`) — writes three files to `{data_dir}/skills/{provider}/{moniker}/`:
   - `skill.json` — v3 schema with `draft: true`, primitive, bindings, model_selector, variants, required_models, source provenance
   - `workflow.json` — the ComfyUI API-format workflow with placeholder tokens
   - `_debug.json` — resolved model details, warnings, analysis metadata

### AI-assisted naming

After the draft is written, the import handler spawns an async naming task. The namer dispatches a `text.chat` request through the orchestrator's own `Dispatcher` with `selectors.model = "recommended:chat"` — no external HTTP call, the garden's own chat model names the skill.

The prompt describes the workflow's structure, models, and extracted parameters. The chat model returns a JSON response with `display_name` and `description`. The namer has a 30-second timeout; on failure it falls back to a heuristic name derived from the moniker.

Naming progress is visible on the event bus:
- `skills.{moniker}.state = analyzing` → import started
- `skills.{moniker}.state = naming` → draft written, AI naming in progress
- `skills.{moniker}.named` → display name assigned
- `skills.{moniker}.state = ready` → skill available for dispatch

### Importing via the API

```bash
# From a CivitAI image page
curl -X POST http://localhost:7190/v1/skills/comfyui/import \
  -H "Content-Type: application/json" \
  -d '{"input": "https://civitai.com/images/126242620"}'

# From a PNG with embedded workflow
curl -X POST http://localhost:7190/v1/skills/comfyui/import \
  -H "Content-Type: image/png" \
  --data-binary @workflow.png

# From raw workflow JSON
curl -X POST http://localhost:7190/v1/skills/comfyui/import \
  -H "Content-Type: application/json" \
  -d '{"input": "{\"1\": {\"class_type\": \"KSampler\", ...}}"}'
```

Response: `202 Accepted` with `Location: /v1/skills/{moniker}`

```json
{
  "moniker": "flux-26239",
  "draft_dir": "/data/skills/comfyui/flux-26239"
}
```

### Skill provisioning

After import, the skill may need models pushed to ComfyUI instances. The provisioning queue downloads missing models from CivitAI and streams them to instances via the Moss volume API (`PUT /api/v1/stone/offerings/comfyui/volumes/comfyui-models/...`). Progress is logged and visible in the container logs. Skills transition to `ready` only after all required models are available on at least one instance.

---

## Resources

View stone hardware and resource claims:

```bash
curl http://localhost:7190/v1/resources
curl http://localhost:7190/v1/resources/stones/stone-azure-pool
curl http://localhost:7190/v1/resources/stones/stone-azure-pool/pressure
```

---

## Jobs

```bash
# List all jobs
curl http://localhost:7190/v1/jobs

# Inspect one job
curl http://localhost:7190/v1/jobs/019d7459-6554-...

# Get job result
curl http://localhost:7190/v1/jobs/019d7459-6554-.../result

# Cancel a job
curl -X DELETE http://localhost:7190/v1/jobs/019d7459-6554-...
```

---

## Media

```bash
# Upload
curl -X POST http://localhost:7190/v1/media \
  -H "Content-Type: image/png" \
  --data-binary @photo.png

# Download
curl http://localhost:7190/v1/media/{id}

# Metadata
curl http://localhost:7190/v1/media/{id}/metadata
```

---

## Testing patterns

### Smoke test: is the garden alive?

```bash
# Health + provider count
curl -s http://localhost:7190/health | jq '.providers_enabled'

# Catalog has primitives
curl -s http://localhost:7190/v1/catalog | jq '.primitives | length'

# At least one model available for chat
curl -s http://localhost:7190/v1/catalog/text.chat | jq '.fields[] | select(.field == "selectors.model") | .options | length'
```

### End-to-end: dispatch and verify

```bash
# Chat
curl -s -X POST http://localhost:7190/v1/text/chat \
  -H "Content-Type: application/json" \
  -d '{"text": {"prompt": {"user": "Say hello"}}}' \
  | jq '.output.text.response'

# Translate
curl -s -X POST http://localhost:7190/v1/text/translate \
  -H "Content-Type: application/json" \
  -d '{"text": {"body": "Good morning", "language": {"target": "es"}}}' \
  | jq '.output.text.translated'

# Embed
curl -s -X POST http://localhost:7190/v1/text/embed \
  -H "Content-Type: application/json" \
  -d '{"text": {"input": ["hello world"]}}' \
  | jq '.output.text | keys'
```

### Parallel dispatch

```bash
# Fire 5 chat requests concurrently
for i in $(seq 1 5); do
  curl -s -X POST http://localhost:7190/v1/text/chat \
    -H "Content-Type: application/json" \
    -d "{\"text\": {\"prompt\": {\"user\": \"Count to $i\"}}}" &
done
wait
```

### Event stream testing

```bash
# In terminal 1: subscribe
curl -N "http://localhost:7190/v1/events?focus=skills.*"

# In terminal 2: import a skill
curl -X POST http://localhost:7190/v1/skills/comfyui/import \
  -H "Content-Type: application/json" \
  -d '{"input": "https://civitai.com/images/126242620"}'

# Terminal 1 shows skill lifecycle events in real-time
```

### Preference-driven dispatch

```bash
# Set a low temperature preference
curl -X PUT http://localhost:7190/v1/preferences \
  -d '{"text.sampling.temperature": 0.3}'

# Dispatch without specifying temperature — preferences fill it
curl -s -X POST http://localhost:7190/v1/text/chat \
  -d '{"text": {"prompt": {"user": "Be creative"}}}' \
  | jq '._meta'

# Override explicitly — caller wins
curl -s -X POST http://localhost:7190/v1/text/chat \
  -d '{"text": {"prompt": {"user": "Be creative"}, "sampling": {"temperature": 1.5}}}' \
  | jq '._meta'
```

### Flow testing

```bash
curl -s -X POST http://localhost:7190/v1/do \
  -H "Content-Type: application/json" \
  -d '{
    "actions": [
      {"id": "translate", "action": "text.translate", "payload": {"text": {"body": "Hello", "language": {"target": "fr"}}}},
      {"id": "chat", "action": "text.chat", "payload": {"text": {"prompt": {"user": "Expand on: {{translate.text.translated}}"}}}}
    ]
  }' | jq '.steps[] | {id, status, result}'
```

---

## Docker

### Running the orchestrator

```bash
docker run -d --name zen-garden-ai-orchestrator \
  -p 7190:7190 \
  -p 21434-21439:21434-21439 \
  -v /path/to/data:/data \
  zen-garden-ai-orchestrator:latest
```

The orchestrator discovers the garden via Koi at `http://host.docker.internal:5641`. No `ZG_STONE` override is needed unless Koi is unavailable.

### Building from source

```bash
cd zen-garden
docker build -f src/orchestrators/ai/Dockerfile -t zen-garden-ai-orchestrator:dev .
```

### Logs

```bash
docker logs -f zen-garden-ai-orchestrator
```

Key log lines to watch for:
- `mDNS discovery complete count=N` — stones found
- `tending stone stone=http://...` — primary stone selected
- `N local adapters registered` — adapters constructed
- `comfyui: inventory + readiness pass complete` — skills probed
- `listening addr=0.0.0.0:7190` — ready to serve

---

## Error reference

| Code | HTTP Status | Meaning |
|------|-------------|---------|
| `validation_failed` | 400 | Request body fails field/type validation |
| `no_provider` | 503 | No enabled provider serves this primitive |
| `pin_not_servable` | 404 | Pinned model not found on any healthy instance |
| `unsupported` | 400 | Provider cannot serve the requested primitive |
| `timeout` | 504 | Upstream provider timed out |
| `upstream_error` | 502 | Provider returned an error |
| `idempotency_conflict` | 409 | Idempotency key reused with different payload |
| `flow_reference_error` | 400 | Flow placeholder references unknown step/field |
| `registration_not_found` | 404 | Catalog path doesn't match any registration |
