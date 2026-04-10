---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0037: Composed Introspect — Vocabulary Base + Provider Overlay

**Date**: 2026-04-10
**Status**: Accepted
**Deciders**: Leo
**Amends**: ORCH-0036 (introspect-as-workspace)

---

## Context

The introspect endpoint (`GET /v1/{mod}/{leaf}`) returns a workspace
definition with a payload template and fields. Currently it reads
fields from the winning provider's capability `parameters` — but
these are a mix of vocabulary-standard fields (prompt, width, height)
and provider-specific extensions (steps, guidance, negative prompt).

This creates two problems:

1. **Bare-primitive dispatch fails for skill-based providers.**
   `POST /v1/image/generate` without a skill moniker routes to
   ComfyUI, which rejects it because it requires a skill. The
   orchestrator shouldn't know this — it should work for any
   provider that serves the primitive.

2. **Switching providers changes the field surface.** ComfyUI's
   `image.generate` has steps, guidance, seed. Google's
   `image.generate` has none of those. The UI must recompose when
   the user switches providers.

---

## Decision

### Two-layer field composition

The introspect endpoint composes the workspace from two layers:

**Layer 1 — Vocabulary (common contract)**

The primitive's vocabulary defines the agreed-upon fields. Every
provider that serves `image.generate` honors these:

```
image.prompt.positive    (required, textarea)
image.dimensions.width   (optional, select)
image.dimensions.height  (optional, select)
```

These come from the vocabulary registry — they are primitive-level,
provider-agnostic.

**Layer 2 — Provider overlay (extensions)**

The winning provider's capability `parameters` declare additional
fields specific to that provider's implementation:

```
ComfyUI overlay:
  image.prompt.negative    (optional, textarea)
  image.sampling.steps     (optional, slider, 1-50)
  image.sampling.guidance  (optional, slider, 1-30)
  image.sampling.seed      (optional, hidden)

Google overlay:
  (none)
```

The introspect handler merges them: vocabulary fields form the base
payload + fields map, then provider overlay fields are added on top.
If a provider field has the same path as a vocabulary field, the
provider's version wins (it may narrow constraints or change the
widget hint).

### Provider selection

The introspect endpoint accepts an optional `?provider=` query
parameter:

```
GET /v1/image/generate           → winning provider by priority
GET /v1/image/generate?provider=gemini  → forced provider
```

When the user switches providers in the UI, the frontend re-fetches
the introspect endpoint with the new provider. The payload and fields
recompose — provider-specific controls appear or disappear.

### Provider priority

Each provider declares a `priority` on its capability:

- `0` — default for local providers (ComfyUI, Ollama, etc.)
- `-10` — cloud/external providers (Google, OpenAI)
- Higher values win in the default `recommended:` selection

The introspect handler picks the highest-priority provider when
`?provider=` is not specified.

The priority is declared on the `Capability` struct:

```rust
pub struct Capability {
    pub primitive: Primitive,
    pub priority: i32,           // NEW
    pub media_inputs: Vec<...>,
    pub parameters: Vec<...>,
    pub examples: Vec<...>,
}
```

### Bare-primitive dispatch

When a caller sends `POST /v1/image/generate` without a skill, the
contextualizer resolves the provider (by priority). The provider's
`onboard` method receives the request. If the provider uses skills
internally (ComfyUI), it selects its own default skill — the
orchestrator doesn't broker this.

ComfyUI's `onboard` changes:
- If `request.action.skill` is `None` and skills exist for this
  primitive, auto-select a default skill (e.g., the first
  alphabetically, or one marked as default in the skill config)
- Error only if no skills exist at all for the primitive

This keeps the provider responsible for its own skill routing —
the orchestrator just passes the bare-primitive dispatch through.

### Introspect response shape

```json
GET /v1/image/generate

{
  "display": { ... },
  "routing": {
    "providers": ["comfyui", "gemini"],
    "will_run_on": "comfyui",
    "status": "healthy"
  },
  "invocation": { "method": "POST", "url": "/v1/image/generate" },

  "payload": {
    "image": {
      "prompt": { "positive": "" },
      "dimensions": { "width": 1024, "height": 1024 },
      "sampling": { "steps": 20, "guidance": 7 }
    },
    "model": "recommended:generate"
  },

  "fields": {
    "image.prompt.positive":      { ... from vocabulary ... },
    "image.dimensions.width":     { ... from vocabulary ... },
    "image.dimensions.height":    { ... from vocabulary ... },
    "image.prompt.negative":      { ... from comfyui overlay ... },
    "image.sampling.steps":       { ... from comfyui overlay ... },
    "image.sampling.guidance":    { ... from comfyui overlay ... },
    "model": { ... }
  },

  "skills_available": [ ... ],
  "examples": [ ... ]
}
```

Switching to Gemini:

```json
GET /v1/image/generate?provider=gemini

{
  "routing": {
    "providers": ["comfyui", "gemini"],
    "will_run_on": "gemini",
    ...
  },
  "payload": {
    "image": {
      "prompt": { "positive": "" },
      "dimensions": { "width": 1024, "height": 1024 }
    }
  },
  "fields": {
    "image.prompt.positive":      { ... from vocabulary ... },
    "image.dimensions.width":     { ... from vocabulary ... },
    "image.dimensions.height":    { ... from vocabulary ... }
  }
}
```

No steps, no guidance, no negative prompt — Gemini doesn't declare
them.

---

## Backend changes

| Change | Where |
|--------|-------|
| Add `priority: i32` to `Capability` | `domain/capability_announcement.rs` |
| Update `Capability::new()` with `priority: 0` default | Same |
| Update all adapter capability constructions | All adapters |
| Introspect handler: compose vocabulary base + provider overlay | `http/introspect.rs` |
| Introspect handler: accept `?provider=` query param | Same |
| Introspect handler: select provider by priority | Same |
| ComfyUI: auto-select default skill for bare-primitive dispatch | `providers/comfyui.rs` |
| Google adapter: set `priority: -10` on capabilities | `providers/google.rs` |

## Frontend changes

| Change | Where |
|--------|-------|
| Provider selector field triggers re-fetch of introspect endpoint | `WorkspaceForm.tsx`, `Workspace.tsx` |

---

## Consequences

### Positive

- **Bare-primitive dispatch works for all providers.** The user sends
  `POST /v1/image/generate`, it works — regardless of whether the
  provider uses skills internally.
- **Provider switching is dynamic.** The UI shows the right fields
  for the active provider. No hardcoded field lists.
- **Clean SoC.** The vocabulary owns the common contract. Providers
  own their extensions. The introspect handler composes. The
  provider's onboard handles internal routing.

### Negative

- **UI re-fetches on provider switch.** A small network round-trip.
  Mitigated: the introspect endpoint is fast (in-memory composition).

---

## References

- [ORCH-0036](ORCH-0036-introspect-as-workspace-definition.md) — introspect as workspace
- [ORCH-0028](ORCH-0028-orchestrator-core.md) — vocabulary and primitives
