---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-30
---

# ORCH-0017: Schema-Driven TryIt with JSON Schema Forms

**Date**: 2026-03-30
**Status**: Accepted
**Applies to**: `zen-garden-ai-orchestrator`, dashboard

---

## Context

The dashboard's TryIt panels are hardcoded per capability. Each panel
assumes a fixed set of parameters (temperature, voice, etc.) regardless
of which provider or model serves the request. This creates problems:

- **Protocol mismatch**: The Speech panel hardcodes OpenedAI Speech
  voices (`alloy`, `nova`), but Google TTS uses different voices
  (`Kore`, `Aoede`). Selecting a Google model in the Speech panel
  produces a broken request.

- **Parameter blindness**: Different providers accept different
  parameters. Ollama has `num_predict`, Google has `thinkingConfig`,
  Anthropic clamps temperature to 0-1. The dashboard can't know this
  without hardcoding per-provider logic.

- **SoC violation**: Provider-specific knowledge (voice lists, parameter
  ranges, format options) leaks into the dashboard. Adding a new
  provider or updating a parameter range requires frontend changes.

- **Maintenance burden**: 12 capability panels × N providers = growing
  matrix of special cases.

---

## Decision

### Provider-emitted JSON Schema

Each provider declares the parameters it accepts for a given model and
capability as a **JSON Schema**. The dashboard renders the form
dynamically using **react-jsonschema-form (RJSF)**.

```
User expands model row
  → GET /v1/models/{model}/form?capability=chat
  → Provider returns { schema, uiSchema }
  → RJSF renders form with Tailwind styling
  → User fills params, clicks Send
  → POST /v1/chat/completions with form data
  → Response rendered inline (capability-specific)
```

### Provider trait addition

```rust
/// Return a JSON Schema describing the parameters this provider
/// accepts for the given model and capability.
///
/// The dashboard renders this schema as a form. Default: empty
/// schema (no configurable parameters).
fn form_schema(
    &self,
    model: &str,
    capability: Capability,
) -> FormSchema {
    FormSchema::default()
}
```

Where:

```rust
pub struct FormSchema {
    /// JSON Schema (draft-07) describing the form fields.
    pub schema: serde_json::Value,
    /// UI Schema for RJSF — layout hints, widget overrides, help text.
    pub ui_schema: serde_json::Value,
}
```

### API endpoint

```
GET /v1/models/{model}/form?capability={cap}
```

Response:

```json
{
  "model": "gemini-2.5-flash-preview-tts",
  "provider": "google",
  "capability": "speech",
  "schema": {
    "type": "object",
    "properties": {
      "input": {
        "type": "string",
        "title": "Text",
        "minLength": 1
      },
      "voice": {
        "type": "string",
        "title": "Voice",
        "enum": ["Kore", "Aoede", "Charon", "Fenrir", "Orus", "Leda",
                 "Puck", "Zephyr", "Sage", "River"],
        "default": "Kore"
      },
      "speed": {
        "type": "number",
        "title": "Speed",
        "minimum": 0.25,
        "maximum": 4.0,
        "default": 1.0
      }
    },
    "required": ["input"]
  },
  "uiSchema": {
    "input": {
      "ui:widget": "textarea",
      "ui:options": { "rows": 3 }
    },
    "speed": {
      "ui:widget": "range"
    }
  }
}
```

### Dashboard rendering

Install `@rjsf/core` + `@rjsf/validator-ajv8`. Use RJSF with no
built-in theme — create a thin Tailwind theme that maps RJSF's HTML
elements to our dark theme classes.

The CapabilityDetail page changes:
- **Remove** the global TryIt panel at the top
- **Add** an inline TryIt inside each model's expanded row
- The inline TryIt fetches the form schema, renders with RJSF, and
  displays results (streaming text, audio player, image, etc.)

The **result display** is capability-specific (not schema-driven):
- Chat → streaming text
- Speech → audio player
- Transcribe → transcribed text
- Image → rendered image
- Embed → dimension count + vector preview

This is correct: the form (input) is provider-specific, the result
(output) is capability-specific.

### Provider form schemas

Each provider implements `form_schema()` for the capabilities it serves:

**OllamaProvider (Chat)**:
```json
{
  "message": { "type": "string" },
  "temperature": { "type": "number", "minimum": 0, "maximum": 2, "default": 0.7 },
  "max_tokens": { "type": "integer", "minimum": 1, "default": 4096 },
  "system": { "type": "string", "title": "System Prompt" }
}
```

**GoogleProvider (Speech)**:
```json
{
  "input": { "type": "string" },
  "voice": { "type": "string", "enum": ["Kore", "Aoede", ...], "default": "Kore" }
}
```

**OpenedaiSpeechProvider (Speech)**:
```json
{
  "input": { "type": "string" },
  "voice": { "type": "string", "enum": ["alloy", "echo", "fable", "onyx", "nova", "shimmer"], "default": "alloy" },
  "speed": { "type": "number", "minimum": 0.25, "maximum": 4.0, "default": 1.0 },
  "response_format": { "type": "string", "enum": ["mp3", "wav", "opus"], "default": "mp3" }
}
```

**GoogleProvider (Image)**:
```json
{
  "prompt": { "type": "string" },
  "aspect_ratio": { "type": "string", "enum": ["1:1", "16:9", "9:16", "4:3", "3:4"], "default": "1:1" }
}
```

### Why RJSF

- 920K weekly downloads, 15.7K GitHub stars
- Updated 2 days before this decision
- ~29KB gzipped (core only)
- Takes JSON Schema in, renders React forms out
- No-theme mode renders plain HTML — style with Tailwind
- Full widget coverage: range sliders, enum dropdowns, textareas,
  file uploads, toggles
- TypeScript first-class
- Apache-2.0 license

---

## Implementation Plan

### Phase 1: Backend
- Add `FormSchema` type to `catalog/traits.rs`
- Add `form_schema()` method to `Provider` trait (default: empty)
- Add `GET /v1/models/{model}/form` endpoint to `api/unified.rs`
- Implement `form_schema()` for all 9 providers

### Phase 2: Dashboard
- Install `@rjsf/core`, `@rjsf/utils`, `@rjsf/validator-ajv8`
- Create Tailwind theme for RJSF (dark mode, monospace inputs)
- Build `ModelTryIt` component: fetch schema → render form → display result
- Rewrite `CapabilityDetail`: remove global TryIt, add inline per-model TryIt
- Remove old `TryIt.tsx` component

### Phase 3: Polish
- Add result display per capability (streaming text, audio, image, etc.)
- Add loading/error states
- Persist last-used form values per model in localStorage

---

## Consequences

### Positive

- Zero frontend changes when a provider adds parameters
- Each provider owns its parameter knowledge (proper SoC)
- Dashboard is a generic form renderer — no per-provider UI code
- New providers get TryIt support automatically
- Users see exactly the parameters their selected model accepts

### Negative

- New dependency: RJSF (~80-100KB with validator)
- Form schema must be maintained per provider (but it's Rust, not JS)
- Result display is still capability-specific (can't schema-drive
  streaming text or audio players)

### Neutral

- The form schema is a contract between provider and dashboard
- Providers can evolve schemas independently
- Old TryIt component deleted — no migration, clean replacement
