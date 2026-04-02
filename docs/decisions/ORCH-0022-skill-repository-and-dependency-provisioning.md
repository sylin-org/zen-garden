---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0022: Skill Repository and Dependency Provisioning

**Date**: 2026-04-01
**Status**: Accepted

---

## Problem

Skills are currently defined in Rust code (`skills/builtin.rs`). Adding a new skill
requires code changes, recompilation, and redeployment. Model provisioning is ad-hoc —
the ComfyUI provider hardcodes recommended model lists, and dependencies for new skills
(like the inpainting checkpoint) aren't discovered or downloaded automatically.

There is no unified mechanism for:
- Storing skill definitions on disk as data files
- Seeding built-in skills from the binary without hardcoding them in Rust
- Resolving and caching model dependencies
- Deduplicating models across skills (same content, different names)
- Letting each provider handle its own dependency provisioning strategy

---

## Decision

### 1. Skill Repository — disk-based, data-only

Skills live on disk in a structured directory under the orchestrator's data folder:

```
{data_dir}/skills/{provider}/{skill-moniker}/
  skill.json              — skill definition (mappings, content slots, metadata)
  {workflow-name}.json    — one or more workflow templates
```

Example layout:

```
/data/skills/comfyui/upscale/
  skill.json
  upscale_2x.json
  upscale_4x.json
  upscale_8x.json
  upscale_16x.json

/data/skills/comfyui/inpaint/
  skill.json
  inpaint.json

/data/skills/comfyui/generate/
  skill.json
  generate.json

/data/skills/comfyui/transform/
  skill.json
  img2img.json
```

The `skill.json` file is the complete skill definition — the same structure described
in ORCH-0018. The orchestrator reads it, not Rust structs. Workflow template files sit
alongside it. The `default_workflow` and workflow option values in mappings reference
filenames in the same directory (without the `.json` extension).

### 2. Embedded defaults — binary seeds disk

Built-in skill definitions and workflow templates are compiled into the binary via
`include_str!()`. On startup, the orchestrator writes them to the skill repository
if not already present (or if the embedded version is newer).

```
Startup:
  for each embedded skill:
    if {data_dir}/skills/{provider}/{moniker}/skill.json does not exist:
      write embedded skill.json + workflow files to disk
    else if embedded.version > disk.version:
      update skill.json + workflow files
    else:
      skip (user may have modified)
```

After seeding, the binary never reads from its embedded copies again. The disk is
the sole source of truth.

User-imported skills (from CivitAI PNGs, community workflows) land in the same
directory structure. The loader treats them identically — no distinction between
built-in and imported.

### 3. Deterministic loading

On startup, the orchestrator scans `{data_dir}/skills/`:

```
for each {provider}/{moniker}/skill.json:
  1. Parse skill.json
  2. Resolve workflow templates from the same directory
     - default_workflow → {moniker}/{default_workflow}.json
     - workflow option values → {moniker}/{value}.json
  3. Validate: all referenced workflows exist on disk
  4. Register the skill in the SkillRegistry
  5. Log: skill loaded, or skill skipped (with reason)
```

No hardcoded skill lists. The loader doesn't know about "upscale" or "inpaint" — it
reads whatever's on disk. Adding a new skill is: drop files in the directory, restart.

If a `skill.json` references a workflow file that doesn't exist, the skill is skipped
with a warning. Partial skills don't load.

### 4. Dependency provisioning — provider-owned

The orchestrator knows WHAT is needed (from `skill.json`). The provider knows HOW to
get it.

#### Orchestrator's role (coordination):

```
for each registered skill:
  mark skill as "preparing"
  for each required_model in skill.required_models:
    ask the provider: "provision this model for this skill"
  once all models are cached locally:
    mark skill as "prepared"
  for each healthy instance:
    ask the provider: "ensure this instance has all models"
    once confirmed:
      mark instance as ready for this skill
  once at least one instance is ready:
    mark skill as "available"
```

#### Provider's role (execution):

The `Provider` trait gains a method for dependency provisioning:

```rust
/// Download a model to the local dependency cache.
/// The provider knows where to get it (HuggingFace, GitHub, ollama pull, etc.).
fn provision_dependency(
    &self,
    model: &ModelRef,
    workspace_dir: &Path,
) -> BoxFuture<'_, Result<PathBuf>>;

/// Push a cached model to a specific instance.
/// The provider knows the instance's volume layout.
fn push_dependency(
    &self,
    model: &ModelRef,
    cached_path: &Path,
    instance_endpoint: &str,
    moss_endpoint: &str,
    offering_fqn: &str,
) -> BoxFuture<'_, Result<()>>;
```

Different providers, different strategies:
- **ComfyUI**: stream download from HuggingFace/GitHub → local cache → push via
  Moss volume API to `comfyui-models/{model_type}/`
- **Ollama**: `POST /api/pull` on the target instance (model goes directly to the
  instance, no local cache needed)
- **Cloud providers**: no provisioning needed (models are hosted)

### 5. Dependency cache — content-addressed, deduplicated

Models are cached in a shared directory per provider:

```
{data_dir}/cache/dependencies/{provider}/
  manifest.json
  sd-v1-5-inpainting.ckpt
  v1-5-pruned-emaonly.safetensors
  RealESRGAN_x4plus.pth
  RealESRGAN_x4plus_anime_6B.pth
```

#### Manifest

```json
{
  "files": {
    "sd-v1-5-inpainting.ckpt": "sha256:a1b2c3d4...",
    "v1-5-pruned-emaonly.safetensors": "sha256:e5f6g7h8...",
    "RealESRGAN_x4plus.pth": "sha256:i9j0k1l2..."
  },
  "aliases": {
    "runway-inpainting-v1.ckpt": "sd-v1-5-inpainting.ckpt",
    "sd15-inpaint.safetensors": "sd-v1-5-inpainting.ckpt"
  }
}
```

`files` maps filename → SHA-256 checksum. `aliases` maps requested names that resolved
to an existing file with the same content.

#### Download + dedup flow

When a skill requires a model:

```
1. Download to workspace: cache/dependencies/workspace/{skill}/{filename}
2. Compute SHA-256 of the downloaded file
3. Check manifest:

   Case A — checksum exists, same filename:
     File already cached. Drop workspace copy. Done.

   Case B — checksum exists, different filename:
     Same content, different name. Record alias:
       aliases[requested_name] = existing_filename
     Rewrite workflow references (requested_name → existing_filename).
     Drop workspace copy.

   Case C — checksum is new, filename available:
     Move from workspace to cache/{provider}/{filename}.
     Add to manifest: files[filename] = checksum.

   Case D — checksum is new, filename already taken (different content):
     Increment: {name}(2), {name}(3), ... until available.
     Move to cache/{provider}/{name}(N).ext.
     Add to manifest: files[name(N).ext] = checksum.
     Rewrite workflow references (original_name → name(N).ext).
```

This ensures:
- Same content is never stored twice (Cases A, B)
- Same name with different content gets a numbered variant (Case D)
- Filenames stay human-readable (no hashes as names)
- Workflow references are always valid after rewriting

#### Workspace cleanup

The workspace directory (`cache/dependencies/workspace/`) is ephemeral. After
provisioning completes (success or failure), the workspace is cleaned up. No
partial downloads persist across restarts.

### 6. Instance provisioning — push cached models

Once a model is in the local cache, the provider pushes it to instances:

```
for each instance serving this skill's provider_kind:
  for each required model:
    resolve filename (apply aliases if needed)
    HEAD check: does the instance already have this file?
    if not: stream push from local cache to instance
```

The push uses the Moss offering volume API (ORCH-0019). Files are streamed — never
buffered in memory. The volume path uses the provider's internal directory structure
(e.g., `comfyui-models/checkpoints/` for ComfyUI checkpoints).

### 7. Skill status lifecycle

```
Loading → Preparing → Prepared → Available
                   ↘ Failed
```

| Status | Meaning |
|--------|---------|
| Loading | Skill definition read from disk, being validated |
| Preparing | Downloading dependencies to local cache |
| Prepared | All dependencies cached locally; pushing to instances |
| Available | At least one instance is ready |
| Failed | Dependency download or validation failed |

The dashboard shows:
- **Available** (green): ready to use
- **Preparing** (amber): downloading models or pushing to instances
- **Failed** (red): dependency error, with details

### 8. Hot-reload — no restart required

The orchestrator rescans `{data_dir}/skills/` on each discovery cycle (same interval
as instance discovery, typically 30 seconds). New skill directories are loaded,
deleted directories are unregistered. Changes to `skill.json` are picked up
automatically.

```
Each discovery cycle:
  scan {data_dir}/skills/
  for each skill.json found:
    if not registered: load + register + start provisioning
    if registered and skill.json modified: reload definition
  for each registered skill not on disk:
    unregister
```

No restart needed to add, update, or remove skills. Drop files → next cycle picks
them up. Same mechanism Moss uses for manifest hot-reload.

### 9. Download progress and resume

#### Progress reporting

Provisioning progress is reported through the dashboard SSE event stream. The skill
status carries per-model progress:

```json
{
  "skill": "image.inpaint",
  "status": "preparing",
  "progress": {
    "model": "sd-v1-5-inpainting.ckpt",
    "downloaded_bytes": 2147483648,
    "total_bytes": 4265380512
  }
}
```

The dashboard shows a progress bar for each model being downloaded. Multiple models
provision concurrently — each reports independently.

#### Download resume

If a download is interrupted (crash, network failure), the workspace file persists.
On the next provisioning attempt:

```
1. Check workspace for existing partial file
2. If exists: stat its size, send HTTP Range: bytes={size}-
3. Server supports Range: append to existing file, continue
4. Server doesn't support Range: delete partial, restart
```

This avoids re-downloading 3.5GB of a 4GB file after a transient failure.

### 10. Garbage collection

When skills are deleted, their model dependencies may become orphaned in the cache.
A periodic sweep removes unreferenced models:

```
GC sweep (runs on startup and periodically):
  1. Collect all model filenames referenced by any skill.json
  2. Include alias targets (resolve through aliases map)
  3. For each file in cache/{provider}/:
     if not referenced by any skill: delete, remove from manifest
  4. Clean stale alias entries
```

The sweep is conservative — it only removes files with zero references. Models
shared across multiple skills are safe as long as any referencing skill exists.

The sweep runs:
- On startup (clean up after unclean shutdown)
- After a skill is unregistered (immediate cleanup opportunity)
- Periodically (catch manual file deletions)

### 11. License display

Each model in `skill.json` carries a `license` field. The dashboard skill panel
displays license information for all models used by the skill. The user is
responsible for proper use — no blocking gates or acceptance dialogs.

The dashboard shows:
- License name next to each model in the skill detail panel
- A license summary icon on the skill card (e.g., commercial-friendly vs restrictive)
- Full license text available on hover or click

License types for quick visual scanning:

| Icon | Meaning | Examples |
|------|---------|---------|
| Green | Permissive / commercial OK | MIT, BSD, Apache-2.0 |
| Yellow | Open with conditions | CreativeML Open RAIL-M |
| Red | Restrictive / non-commercial | CC-BY-NC, research-only |

### 12. Future extensions (acknowledged, deferred)

#### Dependency integrity verification

On startup, verify cached model files against manifest checksums. Full SHA-256
verification is expensive (4GB files), so the initial implementation checks file
existence and size only. Full checksum verification runs as a background task or
on first use of a model.

#### Custom node dependencies

ComfyUI workflows may require custom node packs (Impact Pack, ControlNet Aux, etc.).
These are Python packages installed inside the container — a different provisioning
mechanism than model files. `skill.json` can declare:

```json
{
  "required_nodes": [
    { "name": "ComfyUI-Impact-Pack", "url": "https://github.com/ltdrdata/ComfyUI-Impact-Pack" }
  ]
}
```

The ComfyUI adapter would handle installation via the ComfyUI Manager API or
direct git clone into the custom_nodes volume. This is an extension point — the
`skill.json` schema supports it, but the provisioning logic is deferred.

### 13. Model download metadata in skill.json

Model download URLs live in `skill.json`, not in Rust code. Each required model
entry carries everything the provider needs:

```json
{
  "required_models": [
    {
      "filename": "sd-v1-5-inpainting.ckpt",
      "model_type": "checkpoints",
      "url": "https://huggingface.co/runwayml/stable-diffusion-inpainting/resolve/main/sd-v1-5-inpainting.ckpt",
      "size_bytes": 4265380512,
      "sha256": "c6bbc15e...",
      "license": "CreativeML Open RAIL-M",
      "description": "SD 1.5 inpainting — dedicated inpainting checkpoint"
    }
  ]
}
```

The `sha256` field is optional for built-in skills (computed on first download)
but required for imported skills (verified after download).

---

## File Layout Summary

```
{data_dir}/
  skills/
    {provider}/
      {skill-moniker}/
        skill.json                    — skill definition
        {workflow-name}.json          — workflow templates (1+)

  cache/
    dependencies/
      {provider}/
        manifest.json                 — checksum + alias registry
        {model-files}                 — cached model files
      workspace/
        {skill}/
          {downloading-files}         — ephemeral workspace, cleaned after use
```

---

## Addendum: Skill Management, Import, and CRUD (2026-04-02)

### 14. Skill CRUD — provider-scoped REST API

Skills are managed through a provider-scoped REST API. Each provider owns its
skills — DDD boundaries are respected.

```
GET    /v1/services                                         → list services
GET    /v1/services/comfyui                                 → adapter info
POST   /v1/services/comfyui                                 → update adapter config

GET    /v1/services/comfyui/skills                          → list skills
GET    /v1/services/comfyui/skills/new                      → empty scaffold
GET    /v1/services/comfyui/skills/analyze?t={url_or_input} → smart import (GET)
POST   /v1/services/comfyui/skills/analyze                  → smart import (binary upload)
GET    /v1/services/comfyui/skills/{moniker}                → skill data
POST   /v1/services/comfyui/skills/{moniker}                → upsert skill
DELETE /v1/services/comfyui/skills/{moniker}                 → delete skill

GET    /v1/services/comfyui/skills/{moniker}/models         → list required models
POST   /v1/services/comfyui/skills/{moniker}/models         → add model
DELETE /v1/services/comfyui/skills/{moniker}/models/{file}   → remove model
POST   /v1/services/comfyui/skills/{moniker}/models/resolve → resolve filename → URL

GET    /v1/services/comfyui/skills/{moniker}/workflows      → list workflow files
POST   /v1/services/comfyui/skills/{moniker}/workflows      → upload workflow
DELETE /v1/services/comfyui/skills/{moniker}/workflows/{name} → remove workflow
```

The `/skills/new` endpoint returns an empty scaffold — same shape as a real skill,
all fields blank. The dashboard renders it in edit mode.

### 15. Smart import — analyze endpoint

The analyze endpoint accepts any input the user can throw at it:

| Input | Detection | Action |
|-------|-----------|--------|
| CivitAI image URL | Regex: `civitai.com/images/(\d+)` | Fetch API → download PNG → extract workflow |
| Direct PNG URL | URL ending in `.png` | Download → extract tEXt/zTXt chunks |
| PNG file upload | POST with binary | Read tEXt/zTXt chunks directly |
| Raw workflow JSON | `JSON.parse`, check for `class_type` keys | Direct ingest |

The analyze endpoint:
1. Detects the input type
2. Fetches/reads the content
3. Extracts the ComfyUI API-format workflow from PNG metadata
4. Feeds it to the workflow parser (inputs, models, outputs, Mermaid diagram)
5. Runs the model resolution cascade (section 16)
6. Creates a **draft skill** on disk with `"draft": true` in `skill.json`
7. Returns the draft's moniker

The dashboard redirects to the edit form for that moniker. The user reviews,
adjusts, and saves. **Save clears the draft flag** — the skill becomes published,
provisioning starts.

Draft skills:
- Live in the same `skills/{provider}/{moniker}/` directory (no separate structure)
- Flagged with `"draft": true` in `skill.json`
- Ignored by the skill loader (not registered, not provisioned)
- Cleaned up by GC after a TTL (e.g., 30 minutes without save)

### 16. Model resolution cascade

When a workflow references a model by filename, the system resolves it to a
download URL through a priority chain:

| Priority | Source | Method | Reliability |
|----------|--------|--------|-------------|
| 1 | Local dependency cache | Exact filename match in manifest | 100% when cached |
| 2 | ComfyUI Manager model-list.json | Exact filename match (527+ curated entries) | 100% when present |
| 3 | CivitAI hash lookup | `/api/v1/model-versions/by-hash/{hash}` | 100% when hash available |
| 4 | HuggingFace Hub | Search repo name, scan files for match | High for official models |
| 5 | CivitAI name search | `/api/v1/models?query={stem}` | Fuzzy, needs confirmation |
| 6 | User provides | Paste URL or CivitAI model link | Manual fallback |

The ComfyUI Manager `model-list.json` is fetched and cached on startup. It maps
exact filenames to download URLs for essential models (CLIP, ControlNet, VAE,
upscalers, base checkpoints).

Each model in the edit form shows a resolution status:
- **Green**: resolved (URL known, or already cached)
- **Yellow**: fuzzy match found, needs user confirmation
- **Red**: unresolved, user must provide URL

A skill can be saved with unresolved models — it publishes but won't provision
until all URLs are provided.

### 17. Dashboard pages

```
/infra/services/comfyui/skills                         → List view
/infra/services/comfyui/skills/new                     → Create (empty form)
/infra/services/comfyui/skills/{moniker}               → View (read-only)
/infra/services/comfyui/skills/{moniker}/edit          → Edit (form)
```

**List view**: all skills for this provider. Name, status (draft/published),
instance count, model resolution status. "New Skill" button and smart import input.

**Create/Edit view**: the skill form.
- Skill metadata: name, display name, description, capability, VRAM
- Smart input box (URL/file/JSON) — triggers analyze, populates the form
- Workflow panel: Mermaid diagram, workflow file list, upload/duplicate/delete
- Parameters panel: detected parameters as toggleable rows. Each row shows:
  field name, label, type (options/range/auto/text), node target, default value.
  User toggles which parameters to expose in the skill form.
- Workflow selector: if multiple workflows, user creates a `field: "workflow"`
  mapping with named options (like upscale Zoom)
- Models panel: required models with resolution status (green/yellow/red),
  resolve button, manual URL input
- Content slots: detected inputs (image dropzones, text areas) with role names
- Preview image (from CivitAI import or user upload)
- Save button → validates, clears draft flag, publishes

**View mode**: read-only skill details.
- Diagram, parameters, models, instance readiness
- Clone button → creates a draft copy, redirects to edit
- Delete button → removes skill directory, triggers GC
- Export button → downloads zip of skill.json + workflows + preview

### 18. Validation on save

Before clearing the draft flag, the backend validates:

1. All workflow files referenced by `default_workflow` and workflow option values
   exist in the skill directory
2. All content mappings reference roles that exist in `content_slots`
3. All param mappings with `node` + `input` reference valid node IDs in the
   corresponding workflow templates
4. `name`, `display_name`, `capability`, `provider_kind` are non-empty
5. No moniker collision with an existing published skill (unless overwriting)

Validation failures return specific errors:
```json
{
  "errors": [
    { "field": "workflows", "message": "Workflow 'upscale_8x' referenced but file not found" },
    { "field": "mappings[2]", "message": "Node '99' not found in workflow 'generate'" }
  ]
}
```

The dashboard displays these inline on the form. The user fixes and re-saves.

### 19. Source tracking

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

This enables:
- **Dedup**: don't import the same image twice (check existing skills for matching source)
- **Provenance**: "where did this skill come from?" visible in the dashboard
- **Re-import**: future feature — check if the source has been updated

### 20. Skill cloning

Available in View mode. "Clone" button creates a draft copy:

1. Copies `skill.json` + all workflow files to a new moniker (e.g., `{original}-copy`)
2. Sets `"draft": true`
3. Redirects to edit mode

The user renames, adjusts, saves. The clone is independent — editing it doesn't
affect the original.

### 21. Preview image

When importing from CivitAI, the original generated image is stored as
`preview.png` in the skill directory. Displayed in the edit form and the view
page. Gives the user confidence that the workflow produces good results.

For manually created skills, the user can upload a preview image.

### 22. Skill export

"Export" button in View mode packages the skill directory into a downloadable
`.zip`:

```
{moniker}.zip
  skill.json
  {workflow-name}.json  (1+)
  preview.png           (if present)
```

Another user imports this zip via the analyze endpoint (POST with file upload).
The system detects it as a zip, extracts, creates a draft skill.

### 23. Custom node detection

#### Tier 1 (import-time warning)

During analyze, the parser checks each `class_type` in the workflow against
ComfyUI's `/object_info` endpoint (which lists all installed nodes). Unknown
class types are flagged:

```json
{
  "warnings": [
    { "type": "missing_node", "class_type": "FaceDetailer",
      "suggested_pack": "ComfyUI-Impact-Pack" }
  ]
}
```

The mapping from class_type to node pack comes from ComfyUI Manager's
`extension-node-map.json` (maps node class names to GitHub repos).

#### Tier 2 (future — automated installation)

`skill.json` declares required node packs:

```json
{
  "required_nodes": [
    { "name": "ComfyUI-Impact-Pack",
      "url": "https://github.com/ltdrdata/ComfyUI-Impact-Pack" }
  ]
}
```

The ComfyUI adapter installs them via ComfyUI Manager's API or direct git clone
into the `comfyui-custom-nodes` volume. Deferred to a future implementation.

#### Tier 3 (future — node pack provisioning across instances)

When a new ComfyUI instance joins the garden, sync required node packs from
the skill registry — same pattern as model provisioning. Deferred.

### 24. UX principles — "just works"

#### Import box on the list page

The skills list page has a text input at the top — always visible, always inviting:
"Paste a URL, drop an image, or create from scratch." Not buried behind a button
or a modal. One input, one action. The system detects the input type and handles it.

#### Live form preview in the editor

Split panel in edit mode:
- **Left**: configuration (mappings, parameters, labels, workflows)
- **Right**: live preview of the resulting skill form

The preview renders the `SkillTryIt` component with the current draft mappings.
Updates in real-time as the user toggles parameters, changes labels, adjusts
options. Pure client-side — no backend calls. The user sees exactly what the
end-user form will look like.

#### Status visibility on the list page

Each skill row shows:
- Colored dot: green (available), amber (preparing + progress %), gray (draft),
  red (failed)
- Instance count
- Preview thumbnail (if available)

At a glance, the user sees the state of everything.

#### Toast notifications for provisioning

After save, models download in the background. When the skill becomes available,
a toast notification appears: "Inpaint is now ready!" The user doesn't need to
poll or refresh. Delivered via the existing SSE dashboard event stream.

#### Soft delete with undo

Delete click → skill enters "deleted" state → "Undo" toast shown for 60 seconds.
After 60 seconds, directory is actually removed and GC runs. Prevents accidents.
The skill disappears from the list immediately (optimistic UI) but can be
recovered within the grace period.

### Implementation tiers (updated)

| Tier | Features | Priority |
|------|----------|----------|
| 1 | CRUD API, analyze endpoint (URL/PNG/JSON), draft flag, model resolution cascade, validation, save → publish, source tracking, preview image | Must have |
| 2 | Edit form with live preview, status dots on list, toast notifications, skill cloning, ComfyUI Manager model-list.json cache, custom node detection (warning) | Should have |
| 3 | Export/import zip, soft delete with undo, CivitAI/HF fuzzy search, custom node auto-install, re-import from source, node pack sync | Nice to have |

---

## File Layout Summary (updated)

```
{data_dir}/
  skills/
    {provider}/
      {skill-moniker}/
        skill.json                    — skill definition (draft: true if unpublished)
        {workflow-name}.json          — workflow templates (1+)
        preview.png                   — optional preview image

  cache/
    dependencies/
      {provider}/
        manifest.json                 — checksum + alias registry
        {model-files}                 — cached model files
        model-list.json               — cached ComfyUI Manager model registry
      workspace/
        {skill}/
          {downloading-files}         — ephemeral workspace, cleaned after use
```

---

## Consequences

- Adding a new skill is dropping JSON files in a directory — no Rust code changes.
- Hot-reload picks up new skills within one discovery cycle — no restart needed.
- Built-in skills seed the repository on first run; updates are version-gated.
- Imported skills (from CivitAI PNGs, community workflows) use the same structure.
- Model deduplication prevents multi-GB waste when skills share models.
- Content-addressed caching catches renamed models and genuine conflicts.
- Each provider owns its provisioning strategy — the orchestrator only coordinates.
- Streaming throughout — no in-memory buffering of multi-GB model files.
- Download resume avoids re-downloading gigabytes after transient failures.
- Progress reporting gives users visibility into multi-GB model downloads.
- The workspace pattern isolates in-flight downloads from the stable cache.
- Garbage collection reclaims disk when skills are removed.
- License information is visible on every skill — the user is responsible for proper use.
- The manifest is human-readable — an operator can inspect and manage the cache.
- Skill definitions carry download URLs — the Rust binary has no hardcoded URLs.
- Custom node dependencies are an acknowledged extension point for future work.
- Smart import handles any input (URL, PNG, JSON, zip) — automagic workflow extraction.
- Draft flag keeps unpublished skills invisible to the execution engine.
- Validation on save catches broken skills before they go live.
- Source tracking enables dedup and provenance for imported skills.
- Cloning enables skill variants without manual recreation.
- Export enables skill sharing between users and gardens.
- The CRUD API is provider-scoped — DDD boundaries respected.
- One code path for create, import, and edit — the draft is just a skill in edit mode.
