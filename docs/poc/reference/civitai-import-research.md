# CivitAI Import Research

Research findings from analyzing CivitAI image URLs to understand what data
is available and how reliable each extraction path is.

**Date**: 2026-04-02
**Sample size**: 7 images across different generators, users, and model types

---

## API Endpoint

```
GET https://civitai.com/api/v1/images?imageId={id}&limit=1
```

No authentication required for public images. Returns a single-item array.

---

## Data Available from the API Response

### Top-level fields (always present)

| Field | Type | Description | Reliability |
|-------|------|-------------|-------------|
| `id` | int | Image ID | Always |
| `url` | string | Image URL (serves JPEG thumbnail, but redirects to original when followed with `original=true`) | Always |
| `width`, `height` | int | Image dimensions | Always |
| `type` | string | "image" or "video" | Always |
| `nsfw` | bool | NSFW flag | Always |
| `browsingLevel` | int | Content rating level | Always |
| `createdAt` | ISO string | Creation timestamp | Always |
| `postId` | int | Parent post ID | Always |
| `username` | string | Creator username | Always |
| `baseModel` | string or null | Model family: "Illustrious", "OpenAI", "ZImageTurbo", "None", etc. | Always, but may be "None" or null |
| `modelVersionIds` | int[] | CivitAI model version IDs for ALL resources used | **Key field** — present even when meta is null |
| `stats` | object | `likeCount`, `heartCount`, `laughCount`, `cryCount`, `commentCount`, `dislikeCount` | Always |

### Meta fields (nested: `meta.meta`)

The API wraps metadata doubly: `response.items[0].meta.meta`. The inner `meta`
object contains the generation parameters. **This object is null for many images.**

| Field | Description | Present when |
|-------|-------------|-------------|
| `prompt` | Positive prompt text | Generator supports it AND user didn't strip it |
| `negativePrompt` | Negative prompt text | Same |
| `seed` | Generation seed (int) | SD/ComfyUI generators |
| `steps` | Inference steps | SD/ComfyUI generators |
| `cfgScale` | CFG scale | SD/ComfyUI generators |
| `sampler` | Sampler name (e.g., "Euler a") | SD/ComfyUI generators |
| `clipSkip` | CLIP skip layers | When used |
| `width`, `height` | Generation dimensions | SD/ComfyUI generators |
| `Model` | Checkpoint filename stem | SD/ComfyUI generators |
| `Model hash` | AutoV2 hash of checkpoint | Sometimes |
| `Version` | Generator name: "ComfyUI", "v1.10.1" (A1111), etc. | SD/ComfyUI generators |
| `hashes` | Map of `"type:filename" → "AutoV2 hash"` | When LoRAs/models used |
| `resources` | Array of `{ name, type, hash, weight, unmatched }` | Inconsistent |
| `civitaiResources` | Array of `{ type, modelVersionId }` | When CivitAI resolved them |

### Observed `meta.meta` states

| State | Meaning | Frequency |
|-------|---------|-----------|
| Full object with all fields | ComfyUI or A1111 with metadata preserved | Common |
| Object with only `prompt` | User censored other fields ("I won't give you a prompt") | Occasional |
| `null` | No generation data available (stripped, API-generated, or unsupported tool) | Common |

---

## Model Resolution Paths

### Path 1: `modelVersionIds` → Direct API lookup (BEST)

```
GET https://civitai.com/api/v1/model-versions/{versionId}
```

Returns: model name, version name, base model, type (Checkpoint/LORA/Upscaler),
files with filenames + SHA256 + AutoV2 hashes + size, download URL.

**Download URL**: `https://civitai.com/api/download/models/{versionId}`

**Reliability**: 100% when `modelVersionIds` is populated. This is the most
reliable resolution path — it's a direct lookup, not a search.

**Key finding**: `modelVersionIds` is present even when `meta` is null. It captures
ALL resources (checkpoints, LoRAs, upscalers) regardless of whether the user
preserved generation metadata.

### Path 2: `meta.hashes` → Hash-based lookup

```
GET https://civitai.com/api/v1/model-versions/by-hash/{hash}
```

The `hashes` field maps `"type:filename" → "AutoV2 hash"`. The hash can be
looked up to get the full model version details.

**Supported hash types**: AutoV1, AutoV2, SHA256, CRC32, BLAKE3, AutoV3.

**Reliability**: 100% when hash is available. But `hashes` is not always populated.

### Path 3: `meta.civitaiResources` → Direct version IDs

Same as Path 1 — `civitaiResources[].modelVersionId` gives direct version IDs.
Often a subset of `modelVersionIds` (only includes resources CivitAI could match).

### Path 4: `meta.resources` with `unmatched: true`

Sometimes resources appear in the `resources` array with `unmatched: true` —
meaning CivitAI couldn't resolve them to a model version. These have a `hash`
field that can be used with Path 2.

---

## Original Image Access

The API `url` field points to CivitAI's CDN. The URL contains `original=true`
but serves an optimized version. Following the redirect chain reaches the original:

```
{api_url}
  → 301 → https://image-b2.civitai.com/file/civitai-media-cache/{uuid}/original
  → 200  (original file)
```

| Original format | Determined by | Has workflow chunks? |
|-----------------|---------------|---------------------|
| PNG | `Content-Type: image/png` | May have (depends on generator) |
| JPEG | `Content-Type: image/jpeg` | Never (JPEG has no tEXt chunks) |

**Key finding**: CivitAI preserves the original file format. If the creator
uploaded a PNG, the original is a PNG with all metadata intact.

### PNG tEXt chunks found in ComfyUI images

| Chunk keyword | Content | Found in samples |
|---------------|---------|-----------------|
| `prompt` | ComfyUI API-format workflow JSON | Yes (images 125682754, 124935009, 125683233) |
| `workflow` | ComfyUI editor-format graph JSON | Expected (not scanned beyond 64KB) |
| `parameters` | A1111-compatible parameter string | Yes (for compatibility) |

**Both** the `prompt` (for execution) and `parameters` (human-readable) chunks
are present in ComfyUI-generated PNGs.

---

## Image Categories (from observations)

| Category | baseModel | meta | modelVersionIds | PNG workflow | Importable? |
|----------|-----------|------|-----------------|--------------|-------------|
| ComfyUI with full data | "Illustrious", etc. | Full params | Populated | Yes | **Fully automatic** |
| ComfyUI, censored prompt | Model family | Partial (prompt removed) | Populated | Maybe | Partial — workflow from PNG if available |
| A1111 / SD WebUI | Model family | Full params | Populated | No (JPEG or no chunks) | **Synthesize workflow** from params |
| External API (OpenAI, etc.) | "OpenAI", etc. | Minimal or null | May have 1 entry | No | **Not importable** — inform user |
| No metadata at all | "None" or null | null | Empty | Sometimes | **Nothing to extract** |
| JPEG original | Model family | Varies | Varies | Never | Must rely on API meta only |

---

## Generation Data Text Format

CivitAI's "Copy All" button produces A1111-compatible text:

```
{positive prompt}
Negative prompt: {negative prompt}
Steps: {N}, CFG scale: {N}, Sampler: {name}, Seed: {N}, Model: {name}, ...
```

This is the same data as `meta.meta` fields, formatted as a string. Users paste
this in forums, Discord, etc. The analyzer should parse this format as a fallback
input type.

---

## Recommendations for the Analyzer

### Resolution priority (updated from research)

1. **`modelVersionIds`** — always check first. Present even when meta is null.
   Direct API lookup → filename, SHA256, download URL. No ambiguity.

2. **Original PNG `prompt` chunk** — if the original is PNG, download and extract.
   Gives the complete ComfyUI workflow (all nodes, all connections, all parameters).
   The actual workflow the creator used, not a reconstruction.

3. **`meta.hashes`** → hash-based CivitAI lookup. Reliable when present.

4. **`meta` generation parameters** → synthesize a standard workflow.
   When the PNG has no workflow but meta has full params, build a
   CheckpointLoader → CLIP → KSampler → VAEDecode → SaveImage graph.

5. **ComfyUI Manager `model-list.json`** — for infrastructure models.

6. **CivitAI name search** — last resort, fuzzy, unreliable.

### Early exits

- `baseModel == "OpenAI"` → "This image was generated by OpenAI, not a local model."
- `baseModel == "Midjourney"` → same
- `meta == null && modelVersionIds == []` → "No generation data available."
- `url` resolves to JPEG, no meta → "No workflow data. Provide a workflow manually."

### Cross-referencing strategy

For maximum model coverage, cross-reference all sources:

```
modelVersionIds: [1915059, 2811751]       ← direct resolution (always try)
meta.civitaiResources: [{versionId: X}]   ← subset of above
meta.hashes: {"LORA:file.safetensors": "hash"}  ← maps filenames to hashes
meta.resources: [{name, hash}]            ← sometimes has unmatched resources
```

The `modelVersionIds` array is the superset. Resolve each one to get full details,
then cross-reference with `hashes` and `resources` to map filenames to the
resolved models.

---

## Caveats

1. **Rate limiting**: not formally documented. Use reasonable intervals (100ms between calls).
2. **NSFW content**: some images may be flagged. The API returns them regardless.
3. **Model downloads may require API key**: creator-restricted models need `?token={key}`.
4. **Large models**: checkpoints can be 6-12GB. Stream downloads, never buffer.
5. **`meta.meta` double nesting**: the API wraps meta inside meta. Handle both levels.
6. **Image 125920719 returned NOT FOUND**: some image IDs may be deleted or private.
7. **`resources[].unmatched: true`**: means CivitAI couldn't resolve the model.
   The hash may still work with the by-hash API.
