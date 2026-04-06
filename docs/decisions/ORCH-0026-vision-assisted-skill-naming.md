# ORCH-0026: Vision-Assisted Skill Naming

- **Status**: Proposed
- **Date**: 2026-04-05
- **Deciders**: Leo
- **Depends on**: ORCH-0018 (skills), ORCH-0023 (import pipeline)

## Context

Imported skills currently derive their names and descriptions from CivitAI
metadata heuristics: the model name, a truncated prompt, or a generated
moniker like "twinlight-34290". These are uninformative and inconsistent.

Meanwhile, CivitAI imports often provide:
- A preview image (the generated output)
- The positive prompt
- The negative prompt
- Model/LoRA names
- Generation parameters (steps, CFG, sampler)

The garden already has vision-capable models running on Ollama (e.g.,
`llava`, `llama-3.2-vision`). We can use them to understand what the
skill produces and generate a meaningful name and description.

## Decision

During skill import, when a preview image is available, submit a
thumbnail + generation context to a vision/chat model to generate
a human-quality skill name and description.

### Pipeline

```
Import → CivitAI metadata + preview image
       → Resize preview to 256×256 thumbnail
       → Build context prompt:
           - Thumbnail (base64)
           - Positive prompt
           - Negative prompt
           - Model names (checkpoint, LoRAs)
           - Style hints (sampler, CFG, steps)
       → Send to vision model (recommended:{vision} or recommended:{chat})
       → Parse response: { name: "...", description: "..." }
       → Use as draft display_name + description
```

### Prompt Template

```
You are naming an AI image generation skill for a dashboard.

Given this preview image and generation parameters, provide:
1. A concise skill name (3-5 words, title case, no technical jargon)
2. A one-sentence description of what this skill produces

The name should describe the visual style or subject, not the model name.

Generation context:
- Prompt: {positive_prompt}
- Negative: {negative_prompt}
- Models: {checkpoint}, {loras}
- Parameters: {steps} steps, CFG {cfg}, {sampler}

Respond in JSON: {"name": "...", "description": "..."}
```

### Fallback Chain

1. Vision model available + preview image → full analysis
2. Chat model available, no image → text-only analysis from prompt
3. No model available → current heuristic (model name + moniker)

### Image Handling

- Resize to 256×256 max (longest edge, preserve aspect ratio)
- JPEG compression at quality 80 (minimize token cost)
- Base64 encode for Ollama API
- No external image processing dependencies — use the `image` crate

### Model Selection

Use the orchestrator's own recommendation system:
- `recommended:vision` for thumbnail analysis
- Fall back to `recommended:chat` for text-only naming
- Timeout: 10 seconds (naming is not blocking — use heuristic on timeout)

## Consequences

**Positive**:
- Skills have meaningful, human-readable names ("Cinematic Portrait",
  "Anime Landscape", "Hyper-Detailed Realism")
- Descriptions accurately reflect the visual output
- Consistent quality regardless of CivitAI metadata completeness
- Uses existing garden infrastructure (Ollama, vision models)

**Negative**:
- Requires a running Ollama instance with a vision model
- Adds ~2-5 seconds to import time (async, non-blocking)
- Token cost per import (~500 tokens for vision analysis)
- Model hallucination risk (mitigated by JSON schema + validation)

## Implementation

### Files

| File | Purpose |
|------|---------|
| `skills/import/namer.rs` | Thumbnail prep, prompt template, model call, response parse |
| `skills/import/analyze.rs` | Call namer after workflow extraction, before draft creation |
| `Cargo.toml` | Add `image` crate for resizing |

### Integration Point

In `analyze.rs`, after step 6 (metadata) and before returning `AnalyzeResult`:

```rust
// Step 7: Vision-assisted naming (ORCH-0026)
if let Some(ref preview) = preview_url {
    if let Ok(naming) = namer::generate_name(
        &state.http, &state.ollama_proxy, preview, &generation, &models
    ).await {
        display_name = naming.name;
        description = naming.description;
    }
}
```

### Not In Scope

- Batch renaming of existing skills
- User override UI (already exists in SkillEdit page)
- Multi-language naming
