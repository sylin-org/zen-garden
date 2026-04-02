---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0023: ComfyUI Skill Management and Import

**Date**: 2026-04-02
**Status**: Accepted
**Depends on**: ORCH-0018 (skill API), ORCH-0022 (skill repository and provisioning)

---

## Problem

The AI orchestrator's skill repository (ORCH-0022) provides the infrastructure:
skills as JSON on disk, dependency caching, model provisioning, hot-reload. But
there is no way for users to create, edit, import, or manage skills through the
dashboard.

Each adapter/service has its own skill ecosystem. ComfyUI skills are built from
workflow templates and model dependencies. Ollama skills (future) might be built
from model configurations. The management interface must be **adapter-scoped** —
each service manages its own skills in its own way.

This ADR defines the skill management layer for **ComfyUI** specifically — the
first adapter to support user-managed skills. The patterns established here
inform future adapters, but the implementation is ComfyUI-specific.

---

## Decision

### 1. Provider-scoped skill CRUD API

Skills are managed under their provider's service namespace. The API follows
REST conventions with a hybrid action endpoint for import.

```
GET    /v1/services/comfyui/skills                          → list all skills
GET    /v1/services/comfyui/skills/new                      → empty scaffold
GET    /v1/services/comfyui/skills/analyze?t={input}        → smart import (URL/text)
POST   /v1/services/comfyui/skills/analyze                  → smart import (file upload)
GET    /v1/services/comfyui/skills/{moniker}                → get skill data
POST   /v1/services/comfyui/skills/{moniker}                → upsert (create or update)
DELETE /v1/services/comfyui/skills/{moniker}                → delete skill

GET    /v1/services/comfyui/skills/{moniker}/models         → list required models
POST   /v1/services/comfyui/skills/{moniker}/models         → add model entry
DELETE /v1/services/comfyui/skills/{moniker}/models/{file}  → remove model entry
POST   /v1/services/comfyui/skills/{moniker}/models/resolve → resolve filename → URL

GET    /v1/services/comfyui/skills/{moniker}/workflows      → list workflow files
POST   /v1/services/comfyui/skills/{moniker}/workflows      → upload workflow file
DELETE /v1/services/comfyui/skills/{moniker}/workflows/{name} → remove workflow file
```

**`/skills/new`** returns an empty skill scaffold — same JSON shape as a real
skill, all fields blank. The dashboard renders it in edit mode.

**`/skills/{moniker}`** POST is an upsert — creates if the moniker doesn't exist,
updates if it does. This means create and edit are the same API call.

### 2. Smart import — the analyze endpoint

The analyze endpoint accepts any input a user can provide and extracts a
ComfyUI workflow from it:

| Input | Detection | Action |
|-------|-----------|--------|
| CivitAI image URL (`civitai.com/images/123`) | Regex on URL | Fetch CivitAI API → download PNG → extract workflow |
| Direct PNG URL (`*.png`) | URL extension | Download PNG → extract tEXt/zTXt chunks |
| PNG file upload | POST binary, check magic bytes | Read tEXt/zTXt chunks directly |
| Raw workflow JSON | Try parse, check for `class_type` keys | Direct ingest |
| Skill zip (future) | Detect zip magic bytes | Extract skill.json + workflows |

The analyze pipeline:

1. Detect input type
2. Fetch/read content
3. Extract ComfyUI API-format workflow from PNG `prompt` tEXt chunk
4. Feed to `parser.rs` → identify models, inputs, outputs, generate Mermaid diagram
5. Run model resolution cascade (section 3)
6. Auto-generate skill metadata (name from source, capability from node types)
7. Create a **draft** skill on disk with `"draft": true` in `skill.json`
8. If CivitAI source: download preview image, record source tracking
9. Return the draft moniker

The dashboard redirects to the edit form for that moniker. The user reviews,
adjusts, saves. **Save clears the draft flag** — the skill becomes published,
provisioning starts (ORCH-0022).

### 3. Model resolution cascade

Workflow templates reference models by filename only — no URLs, no hashes.
The system resolves filenames to download URLs through a priority chain:

| Priority | Source | Method | Reliability |
|----------|--------|--------|-------------|
| 1 | **Local dependency cache** | Exact filename match in ORCH-0022 manifest | 100% when cached |
| 2 | **ComfyUI Manager registry** | Exact filename in `model-list.json` (527+ curated entries) | 100% when present |
| 3 | **CivitAI hash lookup** | `GET /api/v1/model-versions/by-hash/{hash}` | 100% when hash available |
| 4 | **HuggingFace Hub** | Search repo name → scan file siblings | High for official models |
| 5 | **CivitAI name search** | `GET /api/v1/models?query={stem}` | Fuzzy, needs confirmation |
| 6 | **User provides** | Paste URL or model page link | Manual fallback |

The ComfyUI Manager `model-list.json` is fetched from GitHub and cached locally
on startup. It maps exact filenames to download URLs for essential infrastructure
models (CLIP, ControlNet, VAE, upscalers, base checkpoints).

Resolution status per model:

| Status | Meaning | UI |
|--------|---------|-----|
| Green | Resolved — URL known or already cached | No action needed |
| Yellow | Fuzzy match — needs user confirmation | "Is this the right model?" |
| Red | Unresolved — no match found | User pastes URL |

A skill can be saved with unresolved models. It publishes but provisioning skips
models without URLs. The user can resolve them later via the edit form.

### 4. Draft lifecycle

Draft skills are real skill entries with `"draft": true` in `skill.json`:

- Live in the same `skills/comfyui/{moniker}/` directory as published skills
- Ignored by the skill loader — not registered, not provisioned, not visible
  to the execution engine
- Cleaned up by GC after a TTL (30 minutes without modification)

**Save clears the draft flag.** One action: the user clicks Save → backend
validates → clears `draft` → hot-reload registers the skill → provisioning
starts.

No separate "publish" step. No separate directory. No special state management.

### 5. Validation on save

Before clearing the draft flag, the backend validates:

1. `name`, `display_name`, `capability`, `provider_kind` are non-empty
2. `default_workflow` references a workflow file that exists in the skill directory
3. All workflow option values in mappings reference existing workflow files
4. All content mappings reference roles present in `content_slots`
5. All param mappings with `node` + `input` reference valid node IDs in
   the referenced workflow
6. No moniker collision with an existing published skill (unless overwriting)

Failures return specific, actionable errors:

```json
{
  "errors": [
    { "field": "default_workflow", "message": "Workflow 'generate' not found — no generate.json in skill directory" },
    { "field": "mappings[3].node", "message": "Node '99' not found in workflow 'generate'" }
  ]
}
```

The dashboard shows these inline. The user fixes and re-saves.

### 6. Source tracking

Imported skills record their origin:

```json
{
  "source": {
    "type": "civitai",
    "image_id": 125682754,
    "url": "https://civitai.com/images/125682754",
    "imported_at": "2026-04-02T01:38:00Z"
  }
}
```

Enables dedup (don't re-import the same source), provenance (visible in
dashboard), and future re-import capability.

### 7. Preview image

CivitAI imports: the original generated image is downloaded and stored as
`preview.png` in the skill directory. Shows the user what the workflow produces.

Manual skills: the user can upload a preview image.

Displayed in the edit form, the view page, and as a thumbnail on the list page.

### 8. Skill cloning

Available in View mode. "Clone" creates a draft copy:

1. Copy `skill.json` + all workflow files to `{moniker}-copy/`
2. Set `"draft": true`
3. Redirect to edit mode

The clone is independent — editing it doesn't affect the original.

### 9. Custom node detection

During analyze, the parser checks each `class_type` in the workflow against
ComfyUI's `/object_info` endpoint (lists all installed node types). Unknown
types are flagged:

```json
{
  "warnings": [
    { "type": "missing_node", "class_type": "FaceDetailer",
      "suggested_pack": "ComfyUI-Impact-Pack" }
  ]
}
```

The mapping from `class_type` to node pack name comes from ComfyUI Manager's
`extension-node-map.json`. This is a warning only — future work may automate
node pack installation.

### 10. Skill export

"Export" in View mode packages the skill as a downloadable zip:

```
{moniker}.zip
  skill.json
  {workflow}.json  (1+)
  preview.png      (if present)
```

Import of a zip via the analyze endpoint: detect zip, extract, create draft.

---

## Dashboard pages

```
/infra/services/comfyui/skills                    → List view
/infra/services/comfyui/skills/new                → Create (empty form)
/infra/services/comfyui/skills/{moniker}          → View (read-only)
/infra/services/comfyui/skills/{moniker}/edit     → Edit (form)
```

### List view

- Smart input at top: "Paste a URL, drop an image, or create from scratch"
- Skill rows: preview thumbnail, name, status dot (green/amber/gray/red),
  instance count, model resolution status
- Status: green = available, amber = preparing (with progress %), gray = draft,
  red = failed

### Edit view (Create and Edit share this)

Split panel:
- **Left**: skill configuration
  - Metadata: name, display name, description, capability, VRAM
  - Workflow panel: Mermaid diagram, file list, upload/duplicate/delete
  - Parameters: toggleable rows — field, label, type, node target, default.
    User toggles which to expose in the end-user form.
  - Workflow selector: for multi-workflow skills (like Zoom), user creates
    a `field: "workflow"` mapping with named options
  - Models: list with resolution status, resolve button, manual URL input
  - Content slots: detected inputs with role names
  - Preview image
- **Right**: live preview of the end-user form (SkillTryIt component rendered
  from current mappings, pure client-side, no backend calls)

Save validates → clears draft → publishes. Provisioning starts.

### View mode

Read-only: diagram, parameters, models, instance readiness, preview image.

Actions: Clone, Delete, Export.

### Toast notifications

After save, models download in the background (ORCH-0022). When the skill
becomes available, a toast appears: "Inpaint is now ready!" Delivered via
the existing SSE dashboard event stream.

### Soft delete

Delete → "deleted" state → "Undo" toast for 60 seconds. After timeout,
directory removed and GC runs. Prevents accidents.

---

## Implementation plan

### Phase 1: Backend — analyze + CRUD API

*Goal: a working backend that can be tested via curl, no UI yet.*

| Step | Work | Depends on |
|------|------|------------|
| 1a | PNG tEXt/zTXt chunk extractor (Rust, `png` crate) | — |
| 1b | CivitAI API client (fetch image metadata, download PNG) | — |
| 1c | ComfyUI Manager `model-list.json` fetcher + cache | — |
| 1d | Model resolution cascade (cache → Manager → CivitAI hash → manual) | 1c |
| 1e | Analyze endpoint: detect input → extract workflow → resolve models → create draft | 1a, 1b, 1d |
| 1f | CRUD endpoints: new, get, upsert, delete | — |
| 1g | Sub-resource endpoints: models, workflows | 1f |
| 1h | Validation on save | 1f |
| 1i | Draft flag: loader skips drafts, GC sweeps stale drafts | 1f |
| 1j | Source tracking + preview image download | 1e |

Deliverable: `curl` can analyze a CivitAI URL, get a draft, edit it, save it,
and the skill appears in the registry.

### Phase 2: Dashboard — list + edit + view

*Goal: full UI for skill management.*

| Step | Work | Depends on |
|------|------|------------|
| 2a | Skills list page with smart import input | Phase 1 |
| 2b | Edit form: metadata, workflows, parameters, models, content slots | Phase 1 |
| 2c | Live form preview (SkillTryIt from draft mappings) | 2b |
| 2d | View page: read-only details + Clone/Delete/Export buttons | Phase 1 |
| 2e | Status dots + progress on list page | Phase 1 |
| 2f | Toast notifications for provisioning completion | Phase 1 |

### Phase 3: Polish

| Step | Work |
|------|------|
| 3a | Skill cloning |
| 3b | Export as zip |
| 3c | Import from zip |
| 3d | Soft delete with undo |
| 3e | Custom node detection + warnings |
| 3f | HuggingFace search fallback |
| 3g | CivitAI fuzzy search fallback |

---

## Consequences

- Users create skills through the dashboard — no file editing required.
- Smart import handles any input — URL, PNG, JSON, zip. The system figures it out.
- One code path for create, import, and edit — a draft is just a skill in edit mode.
- Draft flag keeps unpublished skills invisible to the execution engine.
- Validation on save catches broken skills before they go live.
- Model resolution cascade resolves most dependencies automatically.
- Source tracking enables dedup and provenance for imported skills.
- Live form preview gives immediate visual feedback during editing.
- The CRUD API is provider-scoped — each adapter manages its own skills.
- The patterns established for ComfyUI inform future adapter implementations.
- Export enables skill sharing between users and gardens.
