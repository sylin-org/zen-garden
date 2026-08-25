---
audience: developer
doc_type: decision
status: proposed
---

# ORCH-0027: AI Orchestrator API Surface v2

**Date**: 2026-04-07
**Status**: Proposed
**Deciders**: Leo
**Supersedes**: ORCH-0017 (schema-driven try-it), ORCH-0018 (skills + workflow API)
**Depends on**: ORCH-0011 (recommended monikers), ORCH-0015 (model directory), ORCH-0025 (skill persistence)

---

## Problem

The orchestrator's current public surface accreted from three different design eras: an OpenAI-shaped `/v1/chat/completions` family, a bespoke `/v1/{capability}/skill/{moniker}` job system, and a per-provider skill management namespace. The result is incoherent in five ways that compound:

1. **Three URL grammars in one surface.** `/v1/chat/completions` (OpenAI verb), `/v1/audio/speech` (OpenAI noun grouping), `/v1/{capability}/skill/{moniker}` (capability-first), `/v1/services/{provider}/skills/...` (provider-first), `/v1/models/{model}/form?capability=` (resource-then-query) — no rule predicts the next URL.
2. **Capability is overloaded.** It appears as a URL prefix, a modality container, a path segment, a query parameter, and is silently inferred from request payload shape in `chat/completions`. Five expressions of one concept.
3. **Sync vs async is provider-shaped, not semantically shaped.** `audio.speech` is synchronous because TTS providers stream; `image.generate` is asynchronous because ComfyUI is a workflow. Callers must memorize which capability lives in which world — a leak from implementation up through the public surface.
4. **Models and skills are the same primitive in different costumes.** Both are parameterized invocations, but they get different verbs, different forms, different listing endpoints, different dispatch paths in the provider trait. Users see two parallel inventories of "things I can invoke."
5. **Capability is a closed enum.** Adding a verb requires touching the enum, the routing scorer, the dispatch site, and adding a handler. There is no extension story.

A full discussion of these problems and their resolution is in the design conversation that produced this ADR. The remainder of this document is the resulting design — what we will build instead — and the live test suite that exercises it.

---

## Objectives

This ADR replaces the current `/v1/` surface with a new design that satisfies these objectives. Each is concrete and testable.

**O1. Coherent grammar.** A user who has seen `/v1/image/generate` can predict that `/v1/audio/transcribe` exists without reading docs. URL structure is `/v1/{modality}/{primitive}[/{skill}]` everywhere, with no exceptions.

**O2. Intent over implementation.** The minimum request is pure intent. Every selector (provider, model, skill) is optional. Consumers express what they want; operators configure how it's served.

**O3. Registry-driven capabilities.** Adding a primitive or skill is a data change, not a code change. The orchestrator boots, primitives register from code, providers register their skills from disk, the catalog hydrates. New skills appear in the URL tree without recompilation.

**O4. Self-describing endpoints.** `GET` (or `OPTIONS`) on any endpoint returns its schema, declared selectors, defaults, and child skills. The orchestrator is its own API documentation.

**O5. Single dispatch path.** Hierarchical URLs (`POST /v1/image/generate`) and the dynamic dispatcher (`POST /v1/do {action: "image.generate"}`) execute the same code. Hierarchical URLs are sugar that pre-fills the action field.

**O6. Composition as a first-class primitive.** Multi-step operations are expressible without the consumer orchestrating round-trips. Three execution modes (sync, async batch, async stream) cover bounded, batched, and continuous workloads under one envelope.

**O7. Locality is a routing input.** A consumer can express "internal only" as a constraint; the router honors it before any other selector. Cloud providers never silently enter the routing pool when the consumer has opted out.

**O8. Binary decoupled from JSON.** Media (images, audio, video, documents) is uploaded once via a binary endpoint, returned as a `media_id`, and referenced in subsequent JSON requests. Base64-in-JSON is no longer the primary path.

**O9. Full traceability.** Every response carries a `_meta` block describing what action ran, on which provider, against which model, with which constraints, in how much time. Correlation IDs propagate through every layer. Ops can answer "why did my call go there?" from a single API response.

**O10. Pristine surface.** No shims, wrappers, or compatibility layers from the previous design. The new surface is greenfield. The previous `/v1/` routes are deleted, not deprecated.

---

## Decision

### Design principles

These principles govern every decision in the rest of this document:

1. **Intent over implementation.** Consumers express what they want; operators configure how it's served. The minimum request is pure intent. Every selector beyond that is optional.
2. **User-advocate promotes verbs to primitives.** A skill becomes a primitive when user mental model demands it, not when schema purity allows it. "Should this be a peer URL?" is decided by "would a user looking at a flat list expect it there?" — never by implementation similarity.
3. **The pond is the trust boundary.** Inside the pond, everything is cooperatively shared. Outside, there is no API. We do not design for multi-tenancy, per-consumer ACLs, or cross-organization isolation.
4. **Registry over code.** Primitives are declared in code (their schemas are the contract), but skills, models, and provider instances live in the registry as data. Adding capability is provisioning, not releasing.
5. **Self-describing.** Every endpoint returns its own schema on GET. The catalog is the union of every endpoint's descriptor. Documentation is the runtime.
6. **Pristine over backward-compatible.** Greenfield design has no legacy weight. We delete the old surface entirely rather than carry shims.

### URL grammar

The `/v1/` namespace contains exactly these shapes and no others:

```
/v1/do                                       # universal dispatcher
/v1/catalog                                  # full registry snapshot
/v1/catalog/events                           # SSE stream of catalog deltas

/v1/{modality}                               # GET: modality summary, list of primitives
/v1/{modality}/{primitive}                   # GET: descriptor; POST: invoke
/v1/{modality}/{primitive}/{skill}           # GET: descriptor; POST: invoke

/v1/pipelines                                # POST: instantiate async streaming pipeline
/v1/pipelines/{id}                           # GET: status; DELETE: terminate
/v1/pipelines/{id}/stream/in                 # WS: input channel
/v1/pipelines/{id}/stream/out                # WS: output channel

/v1/jobs/{id}                                # GET: poll async batch job
/v1/jobs/{id}/events                         # SSE: progress events

/v1/media                                    # POST: upload; GET: list
/v1/media/{id}                               # GET: download; DELETE: delete
/v1/media/{id}/meta                          # GET: metadata only

/v1/idempotency/config                       # GET/PATCH: TTL configuration
/v1/idempotency/flush                        # POST: clear cache

/health                                      # liveness
```

The grammar is exactly two or three segments deep under `/v1/{modality}/`. There are no four-level URLs. There are no ad-hoc namespaces. New verbs come from the registry, not from URL invention.

### Primitive inventory (locked for v1)

Twelve primitives across four modalities. Each was promoted by the user-advocate rule, not by schema distinctness:

| Modality | Primitives |
|----------|------------|
| **text** | `text.chat`, `text.translate`, `text.embed`, `text.rerank` |
| **image** | `image.generate`, `image.edit`, `image.upscale`, `image.analyze` |
| **audio** | `audio.generate`, `audio.transcribe` |
| **video** | `video.generate`, `video.edit` |

**Notes on the inventory:**

- `text.chat` absorbs chat, think, and tool-calling as routing modes (via `model: "recommended:think"` or model selection), not as separate URLs.
- `image.analyze` covers OCR, captioning, classification, and segmentation as skills under one primitive.
- `image.edit` covers inpaint, outpaint, and img2img as skills under one primitive.
- `audio.generate` is a parent primitive whose default child is `audio.generate.speak`. Its other children include `music`, `voice`, and `clone`. Bare `POST /v1/audio/generate` resolves to `audio.generate.speak` via the default-child mechanism described below.
- `image.generate` accepts bare invocation (text → image baseline) and hosts skills like `outpaint`, `inpaint`, `cute-bunny`, etc. Bare invocation strictly accepts only the baseline schema; outpaint and friends require their child URLs.
- `text.rerank` is included even though it's adjacent to `text.embed` because reranking is a recognizable user intent.
- `video.edit` is included to reserve the URL grammar even though no provider implements it at v1 launch. The descriptor returns `available: false` until a provider registers.

### Request envelope

#### `/v1/do` — universal dispatcher

```json
POST /v1/do
Content-Type: application/json
X-Correlation-Id: 01JA7Z...    (optional; generated if absent)

{
  "action":   "image.generate.outpaint",   // required: dotted action ID
  "input":    { ... },                     // required: action-specific payload
  
  "provider": "comfyui",                   // optional: provider class override
  "model":    "comfyui|sd-xl-1.0",         // optional: model override (action-dependent)
  "skill":    "outpaint",                  // optional: skill (redundant with action's third segment)
  
  "constraints": {                          // optional: locality/budget/etc.
    "zone": "internal"
  },
  
  "execution": "sync",                     // optional: sync | async | stream (action-dependent)
  "idempotency_key": "01JA7Z..."           // optional: caller-supplied retry-safe key
}
```

#### Hierarchical sugar

`POST /v1/image/generate/outpaint` is exactly equivalent to `POST /v1/do {"action": "image.generate.outpaint", ...}`. The hierarchical handler pre-fills the `action` field from the URL path and forwards to the dispatcher. The body shape is identical except `action` is omitted (it would be redundant). Both produce identical responses, identical headers, identical telemetry.

#### Selector precedence

When multiple selectors are specified, the router applies them in this order. Conflicts are rejected with `validation_failed` and a clear message — never silently overridden.

1. **Skill** (third URL segment or `skill` field) pins target and usually model. Caller-provided `provider` or `model` must match the skill's declarations or the request is rejected.
2. **Provider + model both specified** → directory validates compatibility. If `model` resolves to a different provider than the one specified, reject.
3. **Provider only** → router picks a healthy instance of that provider class, uses the action's default model on that provider.
4. **Model only** → directory resolves the model to its provider class, router picks a healthy instance.
5. **Constraints** are intersected with the candidate set after the above. `zone: "internal"` removes external providers from consideration before scoring.
6. **Nothing specified** → router uses `recommended:{action}` resolution against fitness scores.

### Response envelope

#### Headers (always present)

```
X-Zen-Action:        image.generate.outpaint
X-Zen-Mode:          sync | stream | async
X-Zen-Provider:      comfyui
X-Correlation-Id:    01JA7Z...
X-Zen-Job-Id:        01JA7Z...        (only when X-Zen-Mode: async)
```

#### Body — sync responses

```json
{
  "result": {
    // action-specific output
  },
  "_meta": {
    "action":     "image.generate.outpaint",
    "mode":       "sync",
    "provider":   "comfyui",
    "instance":   "stone-crystal-forest",
    "model":      "comfyui|sd-xl-1.0",
    "skill":      "outpaint",
    "correlation_id": "01JA7Z...",
    "resolved_from": {
      "requested_provider": null,
      "requested_model":    null,
      "constraints_applied": ["zone:internal"],
      "resolution_path":     "recommended:image.generate → pinned:comfyui → sd-xl-1.0"
    },
    "timings": {
      "routing_ms":   2,
      "queue_ms":     45,
      "inference_ms": 3204,
      "total_ms":     3251
    },
    "usage": {
      "input_tokens":  null,
      "output_tokens": null,
      "asset_bytes":   1247361
    }
  }
}
```

#### Body — async batch (`202 Accepted`)

```json
{
  "job_id": "01JA7Z...",
  "status": "running",
  "_meta":  { /* same shape; routing already decided */ }
}
```

Subsequent `GET /v1/jobs/{id}` returns the same envelope with `status: "completed"` and `result` populated when ready. `GET /v1/jobs/{id}/events` is an SSE stream of progress events (named events: `step.started`, `step.completed`, `job.done`, `job.error`).

#### Body — error responses

```json
{
  "error": {
    "code":    "constraint_unsatisfied",
    "message": "No provider matched constraints {zone: internal} for action image.generate",
    "details": { "constraint": "zone", "value": "internal", "available_zones": ["external"] }
  },
  "_meta": { /* same shape as success — error envelopes always include _meta */ }
}
```

#### Error taxonomy (stable across providers)

| Code | Meaning | HTTP |
|------|---------|------|
| `validation_failed` | Request did not parse or violated schema | 400 |
| `constraint_unsatisfied` | No candidate matches constraints (zone, etc.) | 400 |
| `not_found` | Action, skill, model, or media id does not exist | 404 |
| `no_candidates` | No providers can serve this action right now | 503 |
| `provider_overloaded` | All candidates at capacity | 503 |
| `provider_unreachable` | Network or health issue with selected provider | 503 |
| `auth_failed` | Provider rejected credentials (cloud) | 502 |
| `rate_limited` | Caller hit a rate limit | 429 |
| `quota_exhausted` | Caller hit a configured quota | 429 |
| `timeout` | Exceeded time budget | 504 |
| `upstream_error` | Provider returned an unclassifiable error | 502 |
| `internal_error` | Orchestrator bug | 500 |

The `details` object is action- and code-specific. Clients key on `code`, never on `message`.

### Self-describing endpoints

`GET` or `OPTIONS` on any action URL returns its descriptor:

```json
GET /v1/image/generate

{
  "action": "image.generate",
  "modality": "image",
  "schema": {
    "type": "object",
    "required": ["prompt"],
    "properties": {
      "prompt":          { "type": "string" },
      "negative_prompt": { "type": "string" },
      "width":           { "type": "integer", "minimum": 256, "maximum": 2048 },
      "height":          { "type": "integer", "minimum": 256, "maximum": 2048 },
      "image":           { "$ref": "#/definitions/media_ref" },
      "mask":            { "$ref": "#/definitions/media_ref" }
    }
  },
  "selectors": {
    "provider": {
      "applicable":      true,
      "options":         ["comfyui", "openai", "google"],
      "current_default": "comfyui",
      "default_source":  "operator_pin"
    },
    "model": {
      "applicable":      true,
      "mode":            "hint",
      "current_default": "recommended:image.generate",
      "default_source":  "recommendation",
      "format":          "mfqn-or-shortname"
    },
    "skill": {
      "applicable": true,
      "options":    [
        { "name": "outpaint",    "summary": "Extend an image beyond its borders" },
        { "name": "inpaint",     "summary": "Fill a masked region" },
        { "name": "cute-bunny",  "summary": "Stylized bunny preset" }
      ]
    }
  },
  "execution_modes": ["sync", "async"],
  "default_execution": "sync",
  "constraints": {
    "zone":   { "applicable": true, "default": "any" }
  }
}
```

`GET /v1/image/generate/cute-bunny` returns the same shape but with the skill's effective schema (verb baseline + skill overrides + skill-specific parameters), and `selectors.skill.applicable` is `false` because the skill is already pinned by the URL.

`GET /v1/audio/generate` resolves through the default-child mechanism: returns the descriptor for `audio.generate.speak` (the default child) with an additional `default_child` field naming the resolution. `POST /v1/audio/generate` is then identical to `POST /v1/audio/generate/speak`.

### Catalog

`GET /v1/catalog` returns the full live registry:

```json
{
  "version":    "01JA7Z...",   // GUIDv7, monotonic
  "updated_at": "2026-04-07T15:42:11Z",
  "primitives": [ /* every primitive with full descriptor */ ],
  "skills":     [ /* every skill with parent action */ ],
  "models":     [ /* every model in directory */ ],
  "providers":  [ /* every provider class with state */ ],
  "instances":  [ /* every running provider instance with health */ ]
}
```

`GET /v1/catalog/events` is an SSE stream of catalog deltas — emits whenever a skill is registered, a provider goes up/down, a model is pulled, etc. Dashboards subscribe once and stay live. Batch consumers can poll with `If-None-Match` against the catalog `version`.

### Pipelines

Three execution modes share one envelope:

#### Sync — bounded, request/response

```json
POST /v1/do
{
  "action": "pipeline.run",
  "input": {
    "mode":  "sync",
    "steps": [
      { "as": "img",  "action": "image.generate", "input": { "prompt": "a cat" } },
      { "as": "desc", "action": "image.analyze",  "input": { "image": "$img.result" } },
      { "as": "ja",   "action": "text.translate", "input": { "text": "$desc.text", "to": "ja" } }
    ]
  }
}
→ 200 OK { "result": { "ja": "..." }, "_meta": { /* per-step breakdown */ } }
```

The pipeline runner validates the DAG, computes execution order, runs steps (in parallel where independent), and returns the final result with per-step `_meta`.

#### Async batch — long-running, polled

```json
POST /v1/do
{
  "action": "pipeline.run",
  "execution": "async",
  "input": { "mode": "sync", "steps": [...] }
}
→ 202 { "job_id": "01JA7Z...", "status": "running", "_meta": {...} }

GET /v1/jobs/{id}            # poll
GET /v1/jobs/{id}/events     # SSE progress
DELETE /v1/jobs/{id}         # cancel
```

Same envelope, delayed delivery. The job runs the same DAG; the only difference is the caller polls instead of blocking.

#### Async stream — instantiated, persistent session

```json
POST /v1/pipelines
{
  "mode":  "stream",
  "steps": [
    { "action": "audio.transcribe" },
    { "action": "text.segment" },
    { "action": "text.chat", "input": { "model": "recommended:text.chat" } },
    { "action": "audio.generate.speak" }
  ],
  "constraints": { "zone": "internal" },
  "ttl_seconds": 3600
}
→ 201 Created
{
  "pipeline_id": "01JA7Z...",
  "state":       "provisioning",
  "endpoints": {
    "input":  "/v1/pipelines/01JA.../stream/in",
    "output": "/v1/pipelines/01JA.../stream/out"
  },
  "_meta": { /* routing already decided */ }
}
```

State machine: `provisioning → ready → active → draining → closed` (with `error` reachable from any state).

Streams use **WebSocket** (binary frames for media data, text frames for control events). The client opens `/stream/out` first to wait for `pipeline.ready`, then opens `/stream/in` and starts producing. Closing `/stream/in` triggers drain; the pipeline finishes processing buffered data and closes `/stream/out`.

Output stream emits named events: `pipeline.ready`, `step.started`, `step.output` (with data frames), `step.completed`, `step.error`, `pipeline.done`, `pipeline.error`.

### Media pre-staging

Media (images, audio, video, documents) is uploaded once via a binary endpoint and referenced by `media_id` in subsequent JSON requests. Base64 in JSON is supported as a fallback for small inputs but is not the primary path.

#### Upload

```
POST /v1/media
Content-Type: image/png            (or whatever the actual type is)
X-Filename: cat.png                (optional)
[binary body]

→ 201 Created (first ingestion) or 200 OK (deduplicated)
{
  "media_id":     "01JA7Z...",
  "content_hash": "sha512:abc...",
  "size_bytes":   1247361,
  "content_type": "image/png",
  "metadata": {
    "width":      1024,
    "height":     768,
    "format":     "png",
    "color_mode": "RGBA"
  },
  "created_at":   "2026-03-22T10:14:03Z",
  "expires_at":   "2026-04-14T10:14:03Z",
  "is_new":       false
}
```

**Key invariant**: same SHA-512 → same `media_id`. Uploading the same content twice returns the existing handle. The blob is stored once in the pond's storage, the GUIDv7 represents first-ingestion time in this pond. There is no per-owner bookkeeping. There is no ACL. The pond is the trust boundary.

#### Reference in actions

```json
POST /v1/image/generate/inpaint
{
  "input": {
    "prompt": "a sleeping cat",
    "image":  { "media_id": "01JA7Z..." },
    "mask":   { "media_id": "01JA7Y..." }
  }
}
```

Action handlers fetch metadata at invocation time and validate that the referenced media's `content_type` matches what the field expects. A `media_id` pointing to audio passed to an image field is rejected with `validation_failed` before any inference runs.

#### Lifecycle

- Upload returns the entry with `expires_at` set from the configured TTL (default 7 days).
- Media referenced by an active job, pipeline, or skill is implicitly pinned for the lifetime of that reference.
- `POST /v1/media/{id}/pin` pins indefinitely; `DELETE /v1/media/{id}/pin` releases.
- Expired unpinned media is garbage-collected by a background sweep.
- `DELETE /v1/media/{id}` is hard-delete (no owner check, no soft-delete).

### Zones

Two zones for v1, declared at provider registration time:

- **internal** — anything running on stones in this pond (LAN-resident, no external network egress)
- **external** — cloud providers (Gemini, OpenAI, Anthropic, …)

Default constraint is `any` (no restriction). Consumers opt in to a ceiling:

```json
"constraints": { "zone": "internal" }
```

Means "internal zones only." The router excludes external providers from the candidate set before scoring. If no internal candidate exists, the response is `503 no_candidates` with `details.zone_constraint: "internal"`.

Operators can set a pond-wide default constraint via configuration. Consumers can never loosen the operator's constraint, only tighten it further.

### Idempotency

`Idempotency-Key` header on any non-idempotent action causes the orchestrator to cache the response under (consumer, key) for the configured TTL. Subsequent requests with the same key return the cached response without re-executing.

Configuration is pond-level:
```
GET   /v1/idempotency/config       → { "enabled": true, "default_ttl_seconds": 86400 }
PATCH /v1/idempotency/config       → updates
POST  /v1/idempotency/flush        → clears cache
```

No per-consumer slicing, no inspection/export endpoints, no privacy gates. Pond is the trust boundary.

### Correlation

Every request gets a correlation ID:

1. If the caller provides `X-Correlation-Id`, honor it.
2. If the caller provides W3C `traceparent`, honor it (and synthesize an `X-Correlation-Id` from it).
3. If neither, generate a fresh GUIDv7 for the correlation ID and a fresh `traceparent`.

Both headers propagate through to provider calls so distributed traces span boundaries. The correlation ID appears in `_meta.correlation_id` and on every `tracing::span!` in the orchestrator's logs.

### Registry model

The registry is an in-memory object graph with three layers:

1. **Primitives** — declared in code at startup. Reserved namespace; cannot be shadowed by user content.
2. **Skills** — loaded from disk by providers at startup. ComfyUI scans `{data_dir}/skills/comfyui/` per [ORCH-0025 three-tier persistence](ORCH-0025-three-tier-skill-persistence.md). Other providers scan their own directories. The orchestrator does not own skill persistence; providers do.
3. **Models** — pulled from the model directory ([ORCH-0015](ORCH-0015-model-directory-architecture.md)) and provider inventories. Updated as providers come and go.

**Naming rules** enforced at registration:
- Skill names must match `[a-z][a-z0-9-]*` (lowercase kebab-case).
- Skill names cannot collide with primitive names within the same modality.
- Skill names cannot use reserved segments: `new`, `list`, `search`, `batch`, `do`.
- On collision (different providers register the same name), the first writer wins; subsequent writers fail with `validation_failed` and a clear message naming the conflict.

**Same-content uniqueness** for skills follows the same rule as media: a skill is identified by its content hash (SHA-512 of its definition + workflow files). Re-importing identical content returns the existing record without bumping the version.

---

## Deprecated / Removed

This is greenfield. The following routes and handlers are **deleted**, not deprecated. No shims, no compatibility layers.

### Routes deleted from `src/orchestrators/ai/src/main.rs`

```
POST   /v1/chat/completions                                  [api::unified::chat_completions]
POST   /v1/embeddings                                        [api::unified::embeddings]
POST   /v1/audio/speech                                      [api::unified::speech]
POST   /v1/audio/transcriptions                              [api::unified::transcriptions]
GET    /v1/models                                            [api::unified::models]
GET    /v1/models/{model}/form                               [api::unified::model_form]

POST   /v1/{capability}/skill/{moniker}                      [api::workflows::invoke_skill]
GET    /v1/jobs/{id}                                         [api::workflows::get_job]
GET    /v1/jobs/{id}/assets/{filename}                       [api::workflows::get_job_asset]
GET    /v1/skills                                            [api::workflows::list_skills]
GET    /v1/skills/{skill}/form                               [api::workflows::skill_form]

GET    /v1/services/{provider}/skills                        [api::skill_manage::list_skills]
GET    /v1/services/{provider}/skills/new                    [api::skill_manage::new_skill]
GET    /v1/services/{provider}/skills/analyze                [api::skill_manage::analyze_skill]
GET    /v1/services/{provider}/skills/{moniker}              [api::skill_manage::get_skill]
POST   /v1/services/{provider}/skills/{moniker}              [api::skill_manage::upsert_skill]
DELETE /v1/services/{provider}/skills/{moniker}              [api::skill_manage::delete_skill]
POST   /v1/services/{provider}/skills/{moniker}/rename       [api::skill_manage::rename_skill]
GET    /v1/services/{provider}/skills/{moniker}/workflows    [api::skill_manage::list_workflows]
GET    /v1/services/{provider}/skills/{moniker}/workflows/{n} [api::skill_manage::get_workflow]
PUT    /v1/services/{provider}/skills/{moniker}/workflows/{n} [api::skill_manage::put_workflow]

GET    /v1/secrets                                           [api::secrets::list_secrets]
POST   /v1/secrets/{key}                                     [api::secrets::set_secret]
DELETE /v1/secrets/{key}                                     [api::secrets::delete_secret]
```

### Files deleted

- [src/orchestrators/ai/src/api/unified.rs](../../src/orchestrators/ai/src/api/unified.rs) — replaced by new dispatcher + hierarchical handlers
- [src/orchestrators/ai/src/api/workflows.rs](../../src/orchestrators/ai/src/api/workflows.rs) — replaced by `/v1/jobs` and `/v1/pipelines` handlers
- [src/orchestrators/ai/src/api/skill_manage.rs](../../src/orchestrators/ai/src/api/skill_manage.rs) — skill CRUD moves into the catalog model
- [src/orchestrators/ai/src/api/secrets.rs](../../src/orchestrators/ai/src/api/secrets.rs) — secrets management is dashboard concern, not v1 surface

### Files retained

- [src/orchestrators/ai/src/api/dashboard.rs](../../src/orchestrators/ai/src/api/dashboard.rs) — `/api/*` namespace, internal dashboard, untouched
- [src/orchestrators/ai/src/api/service_actions.rs](../../src/orchestrators/ai/src/api/service_actions.rs) — `/api/services/*`, dashboard infra
- [src/orchestrators/ai/src/api/health.rs](../../src/orchestrators/ai/src/api/health.rs) — `/health`
- [src/orchestrators/ai/src/api/static_files.rs](../../src/orchestrators/ai/src/api/static_files.rs) — dashboard SPA
- [src/orchestrators/ai/src/api/proxy.rs](../../src/orchestrators/ai/src/api/proxy.rs) and [generic_proxy.rs](../../src/orchestrators/ai/src/api/generic_proxy.rs) — Ollama proxy on `:21434`. **Out of scope for this ADR.** External tools that speak native Ollama still need this. A separate decision will determine its fate.

### Files added

- `src/orchestrators/ai/src/api/v2/mod.rs` — module root for the new surface
- `src/orchestrators/ai/src/api/v2/dispatch.rs` — `POST /v1/do` handler
- `src/orchestrators/ai/src/api/v2/hierarchical.rs` — sugar URLs (`/v1/{modality}/{primitive}[/{skill}]`)
- `src/orchestrators/ai/src/api/v2/catalog.rs` — `GET /v1/catalog` and `/v1/catalog/events`
- `src/orchestrators/ai/src/api/v2/media.rs` — media pre-staging endpoints
- `src/orchestrators/ai/src/api/v2/pipelines.rs` — async streaming pipeline lifecycle
- `src/orchestrators/ai/src/api/v2/jobs.rs` — async batch job polling
- `src/orchestrators/ai/src/api/v2/idempotency.rs` — idempotency configuration
- `src/orchestrators/ai/src/domain/registry/` — registry model (primitives, skills, catalog snapshot)
- `src/orchestrators/ai/src/domain/pipelines/` — pipeline DAG validator and runner
- `src/orchestrators/ai/src/domain/zones.rs` — zone constraint type and validator
- `src/orchestrators/ai/src/domain/envelope.rs` — request/response envelope types

The `Capability` enum at [domain/types.rs:202](../../src/orchestrators/ai/src/domain/types.rs#L202) is replaced by `Primitive` and `Modality` types. The `infer_chat_capability` function and silent payload-shape inference are removed entirely — capability is always declared explicitly in the URL.

---

## Live test suite

The test suite is a bash script that runs against the development garden's live AI orchestrator at `http://localhost:7190`. Each test maps to a specific objective from the Objectives section above, and is structured as `setup → request → assertions`. Tests can be run individually or as a full suite, and report pass/fail with diagnostic output for each.

The script lives at [docs/decisions/ORCH-0027/test-suite.sh](ORCH-0027/test-suite.sh) and is executable against any garden running the new orchestrator.

### Test inventory

#### Coherent grammar (O1)

| ID | Test | Method | URL |
|----|------|--------|-----|
| G01 | Catalog discoverable | `GET` | `/v1/catalog` |
| G02 | Modality summary | `GET` | `/v1/text` |
| G03 | Primitive descriptor | `GET` | `/v1/text/chat` |
| G04 | Skill descriptor | `GET` | `/v1/image/generate/{any-skill}` |
| G05 | OPTIONS returns same as GET | `OPTIONS` | `/v1/text/chat` |
| G06 | Unknown action returns `not_found` | `GET` | `/v1/nonsense/foo` |
| G07 | Catalog event stream connects | `GET` | `/v1/catalog/events` (SSE) |

#### Intent over implementation (O2)

| ID | Test | Setup | Assertion |
|----|------|-------|-----------|
| I01 | Bare chat call works with no selectors | `{"input": {"messages": [...]}}` | 200, `_meta.provider` populated |
| I02 | Bare image generate uses pinned default | `{"input": {"prompt": "..."}}` | 200, `_meta.resolution_path` shows recommendation chain |
| I03 | Default child resolves bare audio.generate | bare POST | `_meta.action = "audio.generate.speak"` |
| I04 | Provider override is honored | `{"provider": "ollama", ...}` | `_meta.provider = "ollama"` |
| I05 | Model override is honored | `{"model": "qwen3.5:9b", ...}` | `_meta.model contains "qwen3.5:9b"` |
| I06 | Provider+model conflict rejected | mismatched pair | 400 `validation_failed` |

#### Registry-driven (O3)

| ID | Test | Action | Assertion |
|----|------|--------|-----------|
| R01 | Catalog includes all 12 primitives | `GET /v1/catalog` | `primitives.length >= 12` |
| R02 | ComfyUI skills appear in catalog | inspect catalog | `skills` includes entries with `provider: "comfyui"` |
| R03 | Catalog version monotonic | two GETs | second `version >= first version` |
| R04 | Reserved name collision rejected | attempt skill named "generate" | 400 `validation_failed` |

#### Self-describing (O4)

| ID | Test | URL | Assertion |
|----|------|-----|-----------|
| D01 | Descriptor includes schema | `GET /v1/text/chat` | `schema.required` includes `messages` |
| D02 | Descriptor includes selectors | `GET /v1/image/generate` | `selectors.provider.options` non-empty |
| D03 | Descriptor includes skills | `GET /v1/image/generate` | `selectors.skill.options` lists skills |
| D04 | Skill descriptor has merged schema | `GET /v1/image/generate/{skill}` | schema includes verb baseline + skill overrides |
| D05 | Descriptor declares execution modes | `GET /v1/image/generate` | `execution_modes` includes at least one of sync/async |

#### Single dispatch path (O5)

| ID | Test | Compare | Assertion |
|----|------|---------|-----------|
| S01 | Hierarchical = dispatcher | `POST /v1/text/chat` vs `POST /v1/do {action: "text.chat"}` | identical body envelope, identical headers (except Date/correlation) |
| S02 | Action header echoes resolved | any POST | `X-Zen-Action` matches `_meta.action` |

#### Composition (O6)

| ID | Test | Pipeline | Assertion |
|----|------|----------|-----------|
| C01 | Sync pipeline 2-step | `image.generate → image.analyze` | 200, `_meta.steps.length = 2` |
| C02 | Sync pipeline data flow | output of step 1 used as input to step 2 | step 2 sees correct value |
| C03 | Async batch pipeline submit | `execution: async` | 202, `job_id` returned |
| C04 | Async batch poll | `GET /v1/jobs/{id}` | eventually `status: completed` |
| C05 | Async stream pipeline instantiate | `POST /v1/pipelines` | 201, `endpoints.input` and `endpoints.output` returned |
| C06 | Async stream pipeline state | `GET /v1/pipelines/{id}` | reports valid state |
| C07 | Async stream pipeline cancel | `DELETE /v1/pipelines/{id}` | state transitions to `closed` |

#### Locality / zones (O7)

| ID | Test | Constraint | Assertion |
|----|------|-----------|-----------|
| Z01 | Zone constraint honored | `{"constraints": {"zone": "internal"}}` | `_meta.provider` is internal (ollama, comfyui, etc.) |
| Z02 | Zone constraint forces error when no candidate | request action with `zone: internal` and only external providers | 503 `no_candidates`, `details.zone_constraint: "internal"` |
| Z03 | Default zone unrestricted | no constraint | external provider may be selected |
| Z04 | Constraint applied visible in meta | any | `_meta.resolved_from.constraints_applied` includes zone |

#### Media pre-staging (O8)

| ID | Test | Action | Assertion |
|----|------|--------|-----------|
| M01 | Upload returns media_id and hash | `POST /v1/media` with PNG bytes | 201, `media_id` and `content_hash` populated |
| M02 | Same content returns same id | upload same bytes twice | second response has `is_new: false`, same `media_id` |
| M03 | Metadata extracted at upload | upload PNG | `metadata.width` and `metadata.height` populated |
| M04 | Reference by media_id in invocation | `image.analyze` with `{"image": {"media_id": "..."}}` | 200, action sees the image |
| M05 | Wrong content type rejected | reference audio media_id in image field | 400 `validation_failed` |
| M06 | Download by media_id | `GET /v1/media/{id}` | bytes round-trip identical |
| M07 | Delete then reference | delete then use | 404 `not_found` on the action that referenced it |

#### Traceability (O9)

| ID | Test | Assertion |
|----|------|-----------|
| T01 | `_meta` present on all responses | every test above implicitly verifies |
| T02 | `_meta` present on errors | force a `validation_failed` | response has both `error` and `_meta` |
| T03 | Correlation ID echoed | send `X-Correlation-Id: foo` | response has `X-Correlation-Id: foo` and `_meta.correlation_id: foo` |
| T04 | Correlation ID synthesized when absent | no header | response has both header and `_meta.correlation_id` (matching) |
| T05 | W3C traceparent honored | send `traceparent: ...` | both headers preserved/synchronized |
| T06 | Resolution path is human-readable | any | `_meta.resolved_from.resolution_path` is a non-empty string |
| T07 | Timings populated | any sync call | `_meta.timings.total_ms > 0` |

#### Pristine surface (O10)

| ID | Test | Assertion |
|----|------|-----------|
| P01 | Old `/v1/chat/completions` is gone | `POST /v1/chat/completions` | 404 |
| P02 | Old `/v1/embeddings` is gone | `POST /v1/embeddings` | 404 |
| P03 | Old `/v1/{capability}/skill/{moniker}` is gone | any old skill URL | 404 |
| P04 | Old `/v1/services/{provider}/skills` is gone | any old service URL | 404 |
| P05 | New `/v1/do` exists | `OPTIONS /v1/do` | 200 with allowed methods |

#### Idempotency

| ID | Test | Assertion |
|----|------|-----------|
| K01 | Same idempotency key returns cached | two identical POSTs with same key | second response identical to first, `_meta.idempotent: true` |
| K02 | Different keys execute independently | two POSTs with different keys | both execute, distinct correlation IDs |
| K03 | Flush clears cache | post, flush, repost same key | second post executes again |

#### Error taxonomy

| ID | Test | Assertion |
|----|------|-----------|
| E01 | Invalid JSON returns `validation_failed` | malformed body | 400, `error.code: validation_failed` |
| E02 | Unknown action returns `not_found` | bogus action ID | 404, `error.code: not_found` |
| E03 | Missing required field returns `validation_failed` | omit `messages` from chat | 400, `error.code: validation_failed`, `details.missing_field: "messages"` |
| E04 | Provider down returns `provider_unreachable` | shut down a provider, request it explicitly | 503, `error.code: provider_unreachable` |

### Test runner conventions

- Tests are pure HTTP. No SDK, no language-specific harness. `curl` and `jq` only.
- Each test is a function in the script with the test ID as the function name.
- The runner exits non-zero if any test fails.
- Output is one line per test: `[PASS|FAIL] <id> <description>`. Failures include the request, response, and the assertion that failed.
- Tests are designed to be **non-destructive on the live garden**: they only invoke read operations and idempotent writes, and clean up any media they upload.
- Tests requiring specific providers (e.g. ComfyUI for image generation) skip with `[SKIP]` if the catalog reports the provider unhealthy.
- The runner accepts an `ORCHESTRATOR_URL` environment variable (default `http://localhost:7190`) so it can target staging or other ponds.

---

## Consequences

### What gets easier

- **Adding a new model that does something novel.** Provider declares the capability in its registration; the action becomes available in the catalog; URLs work immediately. No code change in the orchestrator.
- **Building an SDK.** One dispatcher endpoint, one envelope shape, one error taxonomy. The rest is data.
- **Debugging routing.** `_meta.resolved_from.resolution_path` answers "why did it go there?" in one API call.
- **Pipelines for agents.** Multi-step operations are first-class. An agent can submit a pipeline as one call and get one result with full per-step traceability.
- **Locality enforcement.** Privacy-sensitive workloads get an enforceable constraint, not a documentation note.
- **Binary throughput.** Mass embedding workloads upload images once and reference by ID, eliminating base64 round-trip overhead.

### What gets harder

- **Implementing the new surface.** The dispatcher, registry, descriptor system, and pipeline runner are all net-new. Previous code is deleted, not refactored — there is no migration path. This is a deliberate trade-off for design coherence.
- **WebSocket transport for stream pipelines.** Adds a new protocol surface. Local-only deployments (no reverse proxy) will be fine; future deployment behind WAN proxies will need WS-aware configuration.
- **Disciplined registry naming.** Skills must obey reserved-name and namespace rules. Imports with conflicting names will fail and require operator intervention. The trade-off is a coherent URL tree.

### What is locked

- The 12-primitive inventory is locked for v1. Adding a primitive requires a new ADR with the user-advocate justification.
- The URL grammar is exactly two or three segments under `/v1/{modality}/`. No four-level URLs.
- All requests dispatch through one path. Hierarchical URLs are sugar; they cannot diverge from the dispatcher's behavior.
- The error taxonomy is published as part of v1 and stable within the major version.
- The pond is the trust boundary. Multi-tenancy is explicitly out of scope.

### What is deferred

- **Authentication.** Punt entirely. The orchestrator runs in a trusted pond. A future ADR will integrate pond mTLS.
- **Cost / budget enforcement.** Architecture is friendly to adding it; v1 ships without it.
- **Explain mode.** The current `_meta.resolved_from.resolution_path` is enough for v1. Verbose candidate enumeration is a future debugging feature.
- **Streaming pipeline edge validation.** v1 ships with manual segmentation steps. Auto-buffering between mismatched I/O modes is v2.
- **Cancellation of in-progress sync requests.** v1 cancels async jobs and stream pipelines but not sync requests in flight.
- **Catalog SSE delta format details.** v1 ships with full catalog refresh on every change; delta encoding is a future optimization.
- **Dashboard rewrite.** The dashboard consumes the current API and must be updated to consume the new one. That work is tracked separately.
- **Ollama proxy port (`:21434`) fate.** Out of scope for this ADR.

---

## Open questions

These do not block ADR acceptance but should be answered during implementation:

1. **Where does the dispatcher's request envelope type live?** Probably `domain/envelope.rs` as a shared type used by both `api/v2/dispatch.rs` and `api/v2/hierarchical.rs`. Both handlers parse into the same `InvokeRequest` struct and call into a shared executor.

2. **How does the registry expose itself to handlers?** Probably through `FromRef<AppState>` (per ARCH-0007 conventions) so handlers declare exactly the registry slice they need.

3. **What's the catalog SSE event format?** v1 will start with full snapshots on every change (simple, debuggable). The format becomes `event: catalog.updated\ndata: { full snapshot }`. Delta format is a v2 optimization.

4. **How do skills declare their effective schema?** Skills extend their parent primitive's schema. Three options for declaring the extension: full schema replacement, JSON Schema diff, or property-by-property override. v1 should probably go with property-by-property override (skill declares only what changes), with a deep merge at descriptor-render time.

5. **Pipeline DAG validator semantics.** Cycle detection is mandatory. Type checking on edges (`$step.field` references) requires schema introspection. v1 should validate references exist; type checking is v2.

6. **Idempotency cache backend.** In-memory with TTL eviction is sufficient for v1. Persistent backing via the pond's storage is a v2 concern.

---

## References

- [ORCH-0011 Recommended Model Monikers](ORCH-0011-recommended-model-monikers.md)
- [ORCH-0015 Model Directory Architecture](ORCH-0015-model-directory-architecture.md)
- [ORCH-0017 Schema-Driven Try-It](ORCH-0017-schema-driven-tryit.md) — superseded by self-describing endpoints
- [ORCH-0018 Skills, Workflow API, and Synthetic Capabilities](ORCH-0018-skills-and-workflow-api.md) — superseded by primitive/skill registry
- [ORCH-0025 Three-Tier Skill Persistence](ORCH-0025-three-tier-skill-persistence.md)
- [ARCH-0007 Common Scope Modernization](ARCH-0007-modernization.md)
- [STORAGE-0009](STORAGE-0009-distributed-storage.md) — backing store for media pre-staging
