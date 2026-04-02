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
