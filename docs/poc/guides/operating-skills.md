---
audience: [operator]
doc_type: guide
status: current
last_verified: 2026-04-08
---

# Operating Skills

How to add, inspect, and troubleshoot AI Orchestrator skills — the ComfyUI workflow templates, upscalers, inpainters, taggers, and TTS pipelines that the orchestrator dispatches on behalf of API callers.

---

## What You'll Need

- A running orchestrator pointed at a data directory (`--data-dir` flag or `AI_ORCH_DATA_DIR` env var).
- A tended stone reachable at `--stone <url>` (typically `http://localhost:7185` locally, or `http://stone-name:7185` against a real garden).
- At least one ComfyUI instance discovered in the garden. Verify with `GET /v1/catalog` — the `providers` array should include an entry with `"name": "comfyui"` and `"health": "healthy"`.
- Basic familiarity with ComfyUI API-format workflow JSON (the thing ComfyUI writes into a PNG's `prompt` tEXt chunk, not the UI-format save file).

---

## Add a Skill

Skills are directories under `{data_dir}/skills/{provider}/{moniker}/`. Each directory contains a `skill.json` plus one or more workflow JSON files.

### Step 1: Create the skill directory

```bash
mkdir -p {data_dir}/skills/comfyui/my-new-skill
cd {data_dir}/skills/comfyui/my-new-skill
```

The directory name becomes the skill's moniker. It must start with a lowercase letter, contain only letters/digits/hyphens, and not collide with reserved names (`generate`, `upscale`, `edit`, `analyze`, `chat`, `translate`, `embed`, `rerank`, `transcribe`, `text`, `image`, `audio`, `video`, and orchestrator endpoints like `catalog`, `do`, `media`, `jobs`, `events`, `health`). Colliding names get a `-skill` suffix at load time.

### Step 2: Drop the workflow JSON

Save your ComfyUI API-format workflow as a JSON file. The filename (without extension) is what you reference from `skill.json`:

```bash
cp ~/my-workflow.json ./my-workflow.json
```

### Step 3: Write `skill.json`

Minimum viable skill for `image.generate`:

```json
{
  "version": 3,
  "name": "my-new-skill",
  "display_name": "My New Skill",
  "primitive": "image.generate",
  "description": "What this skill does",
  "vram_mb": 4096,
  "default_workflow": "my-workflow",

  "bindings": [
    {
      "field": "image.prompt.positive",
      "placeholder": "PLACEHOLDER_PROMPT"
    },
    {
      "field": "image.prompt.negative",
      "placeholder": "PLACEHOLDER_NEGATIVE",
      "default": "low quality, blurry"
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
    "placeholder": "PLACEHOLDER_CHECKPOINT",
    "default": "my-checkpoint.safetensors",
    "options": [
      { "value": "my-checkpoint.safetensors", "label": "Default" }
    ]
  },

  "required_models": [
    {
      "filename": "my-checkpoint.safetensors",
      "model_type": "checkpoints",
      "url": "https://huggingface.co/.../my-checkpoint.safetensors",
      "size_bytes": 4265380512,
      "license": "CreativeML Open RAIL-M"
    }
  ]
}
```

Replace:

- `"default_workflow": "my-workflow"` with your workflow filename (without `.json`).
- `"PLACEHOLDER_PROMPT"`, `"PLACEHOLDER_NEGATIVE"`, `"PLACEHOLDER_CHECKPOINT"` — these are the exact strings the orchestrator substitutes into your workflow at dispatch time. Use whatever string your workflow already contains (or edit the workflow to use these).
- `"node": "5", "input": "steps"` — the JSON pointer target. `"5"` is a top-level key in your workflow JSON; `"steps"` is an input field on that node. Pick whichever node is your KSampler.
- `required_models[0]` — every model your workflow needs. The provisioner downloads these and pushes them to ComfyUI instances.

### Step 4: Restart the orchestrator

```bash
# Stop and restart — hot-reload lands in a later phase.
systemctl restart garden-ai-orchestrator    # systemd
# or kill + cargo run / docker restart
```

### Step 5: Verify

```bash
curl http://localhost:7190/v1/catalog | jq '.skills[] | select(.moniker == "my-new-skill")'
```

The response should show your skill's `fields`, `variants` (if any), `model_selector`, and `readiness` (per-discovered-instance). If `readiness` shows `ready: false` with `reason: "missing on instance: ..."`, the provisioning worker has queued the download and will push it to the instance as soon as it runs.

---

## Add a Multi-Workflow Skill (Variants)

Skills that expose multiple workflow files behind one moniker — like "upscale 2x vs 4x vs 8x" — use the `variants` field.

### Step 1: Drop all the workflow files

```
{data_dir}/skills/comfyui/my-upscale/
├── skill.json
├── upscale_2x.json
├── upscale_4x.json
└── upscale_8x.json
```

### Step 2: Declare them in `skill.json`

```json
{
  "version": 3,
  "name": "my-upscale",
  "primitive": "image.upscale",
  "default_workflow": "upscale_4x",

  "bindings": [
    {
      "field": "image.source",
      "placeholder": "PLACEHOLDER_IMAGE",
      "delivery": "transfer",
      "accepted_types": ["image/png", "image/jpeg", "image/webp"]
    }
  ],

  "variants": [
    { "value": "upscale_2x", "label": "2x" },
    { "value": "upscale_4x", "label": "4x" },
    { "value": "upscale_8x", "label": "8x" }
  ],

  "required_models": [
    { "filename": "RealESRGAN_x4plus.pth", "model_type": "upscale_models", "url": "..." }
  ]
}
```

Each `value` must have a matching `{value}.json` file in the skill directory. The orchestrator loads all of them at startup.

### Step 3: Dispatch with the variant selector

```bash
curl -X POST http://localhost:7190/v1/do \
  -H "Content-Type: application/json" \
  -d '{
    "action": "image.upscale.my-upscale",
    "variant": "upscale_2x",
    "image": { "source": { "media_id": "01JA..." } }
  }'
```

If the caller omits `variant`, the skill's `default_workflow` is used.

---

## Inspect Skill State

### Catalog

```bash
curl http://localhost:7190/v1/catalog | jq '.skills'
```

Each entry carries everything the dashboard needs: fields, variants, model_selector, required_models, per-instance readiness.

### Narrow to one skill

```bash
curl http://localhost:7190/v1/catalog \
  | jq '.skills[] | select(.moniker == "upscale-skill")'
```

### Check readiness across all instances

```bash
curl http://localhost:7190/v1/catalog \
  | jq '.skills[] | {moniker, readiness}'
```

Each `readiness` entry shows `endpoint`, `stone_name`, `ready` (bool), and `reason` (string). `ready: false` with `reason: "missing on instance: ..."` means the provisioning worker has queued a download.

### See what's in the cache

```bash
jq '.files | length' {data_dir}/cache/dependencies/comfyui/manifest.json
jq '.files | keys' {data_dir}/cache/dependencies/comfyui/manifest.json
du -sh {data_dir}/cache/dependencies/comfyui/
```

Files in `manifest.files` are keyed by filename. Aliases in `manifest.aliases` map alternate names to canonical entries (when two skills request the same content under different filenames, the provisioner records an alias).

---

## Troubleshooting

### Skill isn't in `/v1/catalog`

Check the orchestrator log for loader warnings. Common causes:

- **`draft: true`** in `skill.json`. The loader skips drafts. Remove the field or set it to `false`.
- **Missing workflow file**. If `default_workflow` or any `variants[].value` references a file that isn't in the directory, the skill fails to load. The log line is `skills loader: skipping broken skill`.
- **Invalid primitive**. The `primitive` field must be a dotted canonical name: `image.generate`, `image.edit`, `image.upscale`, `image.analyze`, `audio.generate`, `audio.transcribe`, `text.chat`, `text.translate`, `text.embed`, `text.rerank`.
- **Moniker collision with reserved name**. The directory name `generate` becomes `generate-skill`; `upscale` becomes `upscale-skill`. Check the loader log for the sanitized name.

### Dispatch returns `Unsupported: skill \`foo\` not loaded`

The skill loaded successfully (or it would be missing from the catalog entirely) but the caller is referencing it under the wrong primitive. The `action` must be `<primitive>.<moniker>` — e.g. `image.upscale.upscale-skill`, not `image.generate.upscale-skill`.

### Dispatch returns `Unsupported: variant \`foo\` not found`

The caller sent `variant` with a value that isn't declared in the skill's `variants` array. Check the catalog entry's `variants` list to see what's valid. If the caller omits `variant`, the default workflow is used and this error doesn't apply.

### Readiness is `ready: false` with `missing on instance: ...`

The ComfyUI instance doesn't have one of the skill's required models. The provisioning worker has either queued a job, is currently running one, or the job is in backoff after a failure.

Check queue state via the log. Lines to look for:

```
comfyui: queued provisioning job skill=my-skill endpoint=http://...
provisioner: pushing model to instance model=... endpoint=...
moss_volume: streaming PUT to instance url=... bytes=...
comfyui: provisioning complete skill=my-skill duration_secs=77
```

If the job fails, the log shows the reason and a backoff delay. Subsequent discovery events within the backoff window are deduped — the queue won't retry until the window expires. To force an immediate retry, restart the orchestrator (shutdown clears the backoff map).

### ComfyUI instance doesn't appear in the catalog

The orchestrator only discovers ComfyUI through the tended stone's topology stream. Verify:

```bash
# Does the stone know about the ComfyUI instance?
curl http://{stone}:7185/api/v1/garden/topology | jq '.data.stones[].services[] | select(.kind == "comfyui")'

# Is the orchestrator connected to the stone?
curl http://localhost:7190/health
# Should show all providers registered, health status.
```

If the stone sees ComfyUI but the orchestrator doesn't, check the orchestrator log for garden discovery errors.

### Provisioning job keeps failing with `checksum mismatch`

The downloaded file's SHA-256 doesn't match the `sha256` field in your `skill.json`'s `required_models` entry. Either the upstream URL moved to a different version of the file or the declared checksum is wrong. Update the `sha256` field, or remove it to disable checksum verification for that model.

### Dependency cache keeps growing

The cache holds one copy of every unique model content. When you remove a skill, the models it used stay in the cache (in case another skill needs them). Manual cleanup:

```bash
# Stop the orchestrator first.
# Remove the manifest entry AND the file.
jq 'del(.files["filename-to-remove.pth"])' \
  {data_dir}/cache/dependencies/comfyui/manifest.json > /tmp/manifest.json.tmp
mv /tmp/manifest.json.tmp {data_dir}/cache/dependencies/comfyui/manifest.json
rm {data_dir}/cache/dependencies/comfyui/filename-to-remove.pth
```

Alternatively, wait for the garbage collector (`cache::garbage_collect`) to run — it scans every `skill.json` on disk, finds unreferenced models, and removes them automatically. Garbage collection runs on skill removal events (once hot-reload ships in a later phase).

### Model file exists in `manifest.files` but not on disk

The cache manifest tolerates this — the provisioner will notice the file is missing and re-download it the next time a skill needs it. If you want to clean the stale entry immediately, edit `manifest.json` and remove the offending key.

---

## Move the Cache to Another Disk

The cache at `{data_dir}/cache/dependencies/` is content-addressed and safe to move wholesale.

1. Stop the orchestrator.
2. Move the `cache/` directory to the new location, preserving structure.
3. Point `--data-dir` at the new parent directory.
4. Restart.

The orchestrator reads `manifest.json` on startup and verifies the listed files exist at the expected paths. Drift is tolerated (re-downloaded on demand); corrupted manifests fall back to an empty cache.

Do **not** try to share the cache between multiple orchestrator instances running simultaneously — manifest writes are atomic but there's no cross-process locking. One orchestrator per cache directory.

---

## Next Steps

- **[Skill Subsystem Spec](../specs/skill-subsystem.md)** — how the loader, aggregates, and provisioning pipeline fit together. Read this when writing a new skill-aware adapter or changing the schema.
- **[ORCH-0029 Skill Subsystem Decision](../decisions/ORCH-0029-skill-subsystem.md)** — design rationale, wipe list, acceptance criteria.
- **[ORCH-0028 Orchestrator Core](../decisions/ORCH-0028-orchestrator-core.md)** — vocabulary, primitives, the Provider trait, the dispatch pipeline.
