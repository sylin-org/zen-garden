---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0035: Capability and Skill Examples

**Date**: 2026-04-10
**Status**: Accepted
**Deciders**: Leo
**Related ADRs**:
- ORCH-0028 (orchestrator core) — vocabulary and field paths
- ORCH-0033 (request log) — stored payloads use the same canonical keys
- ORCH-0034 (provider metadata) — adapters own domain knowledge

---

## Context

The dashboard renders catalog-driven forms, but new users face a blank
textarea with no idea what to type. The vocabulary carries generic
`examples.minimal` per primitive, but these are wire-format reference
payloads — not user-facing scenarios.

Each adapter knows what makes a good example for its specific
capability. LibreTranslate knows that a Rilke poem translated to
English demonstrates the quality of the engine. The ComfyUI adapter
knows which prompts produce striking results with each skill's
checkpoint. Ollama knows which questions showcase a model's reasoning.

---

## Decision

### Example struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Example {
    /// Short label shown on the card, action-oriented.
    /// E.g. "German poem to English", "Anime portrait"
    pub label: String,

    /// Optional one-liner expanding on what the example does.
    /// E.g. "Rilke's Duino Elegies opening stanza"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// The payload that fills the form. Uses canonical vocabulary
    /// field paths as keys — identical structure to a dispatch
    /// payload. The dashboard reads each key, matches it to a
    /// catalog field, and populates the corresponding widget.
    pub payload: Value,
}
```

### Placement

Added as an optional `Vec<Example>` on both `Capability` and
`SkillDeclaration` in the capability announcement:

```rust
pub struct Capability {
    pub primitive: Primitive,
    pub media_inputs: Vec<CapabilityMediaInput>,
    pub parameters: Vec<SkillParameter>,
    pub examples: Vec<Example>,  // NEW
}

pub struct SkillDeclaration {
    pub id: String,
    pub primitive: Primitive,
    pub display: SkillDisplay,
    pub parameters: Vec<SkillParameter>,
    pub examples: Vec<Example>,  // NEW
}
```

### Adapter responsibilities

Each adapter populates examples for every capability and skill it
serves. Examples use canonical vocabulary field paths (same paths
as the form fields and the stored request input):

| Adapter | Capability | Example label | Payload sketch |
|---------|-----------|---------------|----------------|
| libretranslate | text.translate | "German poem to English" | `{text.body: "Wer, wenn ich...", text.language.target: "en"}` |
| ollama | text.chat | "Ask about geography" | `{text.prompt.user: "What is the capital of France?"}` |
| ollama | text.embed | "Embed a sentence" | `{text.input: "The quick brown fox..."}` |
| ollama | image.analyze | "Describe an image" | `{text.prompt.user: "What do you see?"}` |
| kokoro | audio.generate | "Read a greeting" | `{audio.text: "Good evening...", audio.voice.id: "af_heart"}` |
| whispercpp | audio.transcribe | *(media-based — no payload example)* | — |
| comfyui | image.generate (per skill) | "Anime portrait" | `{image.prompt.positive: "1girl, garden...", ...}` |

Skills that require media input (audio.transcribe, image.analyze,
image.edit) may omit examples or provide text-only partial examples
that fill the non-media fields.

### Catalog surface

The catalog detail endpoint includes examples from the capability
(for primitives) and from the skill declaration (for skills):

```json
GET /v1/catalog/text/translate
{
  "path": "text.translate",
  "fields": [...],
  "examples": [
    {
      "label": "German poem to English",
      "description": "Rilke's Duino Elegies opening stanza",
      "payload": {
        "text": {
          "body": "Wer, wenn ich schriee, hörte mich denn aus der Engel Ordnungen?",
          "language": { "target": "en" }
        }
      }
    }
  ]
}
```

### Dashboard UX

Cards rendered above the form (or below the primary field when
empty). Each card shows label + optional description. Click fills
the form. Cards dim or collapse once the form has user input.
Clearing the form restores them.

Horizontally laid out, compact. No more than 3 visible; "+N more"
if the adapter provides more.

---

## Consequences

### Positive

- **Zero-documentation onboarding.** A new user clicks a card and
  sees a real scenario fill in — they learn the input shape by
  example.
- **Adapter-owned quality.** Each adapter provides examples that
  actually work well with its engine. A Rilke poem demonstrates
  literary translation quality. A carefully crafted prompt shows
  off a checkpoint's style.
- **Same canonical paths.** Example payloads use the same field keys
  as the form, the dispatch, and the stored request — no translation
  needed.

### Negative

- **Maintenance.** Each adapter now carries example content that could
  become stale if the vocabulary changes. Mitigated: vocabulary
  changes are rare and mechanical (rename a field path → update
  examples to match).

---

## References

- [ORCH-0028](ORCH-0028-orchestrator-core.md) — vocabulary
- [ORCH-0033](ORCH-0033-request-log-and-layout.md) — request payloads
- [ORCH-0034](ORCH-0034-provider-resolution-metadata.md) — adapter domain knowledge
