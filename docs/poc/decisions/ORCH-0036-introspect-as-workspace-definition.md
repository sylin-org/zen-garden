---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0036: Introspect-as-Workspace — Payload Template, Field Map, Unified Contract

**Date**: 2026-04-10
**Status**: Accepted
**Deciders**: Leo
**Supersedes**: Catalog detail endpoints (`GET /v1/catalog/{mod}/{leaf}[/{skill}]`)
**Amends**: ORCH-0031 (dashboard architecture), ORCH-0033 (request log),
ORCH-0034 (provider metadata), ORCH-0035 (examples)
**Amended by**: [ORCH-0038](ORCH-0038-context-aware-workspace-resolution.md) —
live field resolution moved to `Provider::describe_workspace`.
`Capability.parameters` is a startup hint, not the live source; the
introspect handler no longer reads it directly.

---

## Context

The dashboard consumes two overlapping endpoints to render a workspace:

- `GET /v1/catalog/{mod}/{leaf}[/{skill}]` — returns field schema
  (parameters as an array, defaults, widget hints) but no routing,
  no payload structure, no invocation URL.
- `GET /v1/{mod}/{leaf}[/{skill}]` — returns routing info, display
  metadata, invocation URL, but no field schema.

Neither is complete. The frontend fetches one, then the other, then
reconstructs a dispatch payload from field defaults using
`flattenToDotted` and `setNested` — error-prone machinery that
produced bugs (selectors nested inside the vocabulary payload,
field paths not matching between form and dispatch, etc.).

Additionally, `selectors.model` is declared as a `SkillParameter`
alongside vocabulary fields like `text.prompt.user`, despite being a
routing directive, not a vocabulary field. The contextualizer
validates vocabulary fields and rejects selectors that appear in the
payload body.

Lineage tracking (`parent_id`) is passed via URL query parameters and
frontend navigation state — a sideband channel disconnected from the
dispatch payload.

---

## Decision

### 1. One endpoint: `GET /v1/{mod}/{leaf}[/{skill}]`

The introspect endpoint becomes the single workspace definition. It
returns everything the dashboard needs to render a form and dispatch:

```json
GET /v1/text/chat

{
  "display": {
    "name": "Text Chat",
    "description": "Conversational text completion..."
  },
  "routing": {
    "providers": ["ollama"],
    "will_run_on": "ollama",
    "status": "healthy"
  },
  "invocation": {
    "method": "POST",
    "url": "/v1/text/chat"
  },

  "payload": {
    "text": {
      "prompt": { "user": "", "system": "", "history": [] },
      "sampling": { "temperature": 0.7 },
      "tokens": { "max": 2048 }
    },
    "model": "recommended:chat"
  },

  "fields": {
    "text.prompt.user": {
      "label": "Message",
      "type": "string",
      "widget": "textarea",
      "required": true,
      "placeholder": "Ask anything..."
    },
    "text.prompt.system": {
      "label": "System Prompt",
      "type": "string",
      "widget": "textarea"
    },
    "text.prompt.history": {
      "label": "Conversation",
      "type": "dialogue",
      "widget": "dialogue"
    },
    "text.sampling.temperature": {
      "label": "Temperature",
      "type": "number",
      "widget": "slider",
      "min": 0.0,
      "max": 2.0,
      "step": 0.1
    },
    "text.tokens.max": {
      "label": "Max Tokens",
      "type": "integer",
      "widget": "number",
      "min": 1,
      "max": 131072
    },
    "model": {
      "label": "Model",
      "type": "string",
      "widget": "select",
      "options": ["llama3.1:8b", "gemma3:12b", "..."],
      "auto": {
        "default": "recommended:chat",
        "description": "The garden picks the best available chat model"
      }
    }
  },

  "examples": [
    {
      "label": "Ask about geography",
      "description": "A factual question to test knowledge",
      "payload": {
        "text": {
          "prompt": {
            "user": "What are the three largest countries by area?"
          }
        }
      }
    }
  ]
}
```

### 2. Payload template

The `payload` field is a pre-assembled dispatch body. It contains:

- Every mandatory field with an empty value (e.g. `"user": ""`)
- Every optional field that has a default, with the default applied
  (including preference overrides)
- Routing directives at the root level (`model`, `provider`, etc.)

The user can POST the payload as-is for a valid (if empty-prompt)
dispatch. The dashboard uses it as the initial form state.

The payload is the API contract made visible. What the user sees is
what gets sent. Copy-as-curl is `JSON.stringify(payload)`.

### 3. Fields as a map

The `fields` object is keyed by dotted path into the payload. Each
value is a widget descriptor: label, type, widget, constraints,
options. The field key IS the path into the payload — the frontend
uses it to locate the value to render and to write back edits.

Fields are rendering instructions only. They do not carry values or
defaults — those live in the payload.

### 4. Selectors at the payload root

`model`, `provider`, `variant` are top-level keys in the payload,
not nested under `selectors.*`. They appear in the `fields` map as
`"model"`, `"provider"`, `"variant"`. No prefix. No special-casing.

Adapters no longer declare `selectors.model` as a `SkillParameter`.
The introspect handler builds the model selector field from the
capability's live options and the adapter's auto descriptor.

### 5. Lineage in the payload

Fork lineage is carried in the payload:

```json
{
  "text": { ... },
  "model": "recommended:chat",
  "lineage": { "parent": "019d7460-722a-..." }
}
```

The contextualizer extracts `lineage.parent` and sets
`PersistedRequest.parent_id`. If absent, the request is a root.
The `lineage` key is not in the vocabulary — the contextualizer
skips it during validation (same treatment as `model`, `provider`).

The `?from=` URL parameter is removed from the frontend. Lineage
is in the data, not the URL.

### 6. Catalog detail deprecated

`GET /v1/catalog/{mod}/{leaf}` and
`GET /v1/catalog/{mod}/{leaf}/{skill}` are removed from the router.
The introspect endpoint covers their function.

`GET /v1/catalog` (lean summary) remains unchanged — it drives the
sidebar and the Create index page.

### 7. Request rehydration

The stored `PersistedRequest.input` IS a payload. Loading it from
the request log provides the exact form state — no flattening, no
path reconstruction. The dashboard loads the payload and renders.

If the user edits and submits, the form injects
`lineage.parent` pointing to the source request ID. A new request
is created with the modified payload.

---

## Backend changes

| Change | Where |
|--------|-------|
| Rebuild introspect handler: assemble `payload` template from capability parameters + defaults + preferences; build `fields` map; include examples | `http/introspect.rs` |
| Move model selector out of adapter `parameters` | All adapters (remove `selectors.model` SkillParameter) |
| Introspect handler builds model selector field from capability's live options | `http/introspect.rs` |
| Remove catalog detail routes | `http/catalog.rs`, `http/router.rs` |
| Contextualizer: extract `lineage.parent` from payload, skip vocabulary validation for `lineage`, `model`, `provider` | `services/contextualizer.rs` |
| Request store: read `parent_id` from `lineage.parent` in payload | `services/dispatcher.rs` |
| Clean up `SkillParameter`: remove `pinnable` (no longer needed) | `domain/capability_announcement.rs` |

## Frontend changes

| Change | Where |
|--------|-------|
| Fetch `GET /v1/{mod}/{leaf}[/{skill}]` instead of catalog detail | `Workspace.tsx` |
| Use `payload` as initial form state, edit in place, POST as-is | `WorkspaceForm.tsx` |
| Fields map for rendering only — keyed lookup by dotted path | `WorkspaceForm.tsx`, `FieldRenderer.tsx` |
| Delete `flattenToDotted`, `setNested`, field-default merging | `WorkspaceForm.tsx`, `Workspace.tsx` |
| Fork injects `lineage.parent` into payload, removes `?from=` | `WorkspaceForm.tsx` |
| Copy-as-curl: `JSON.stringify(payload)` | `CopyAsCurl.tsx` |
| Update TypeScript types | `api/types.ts` |

---

## Consequences

### Positive

- **One source of truth.** `GET` and `POST` on the same URL use the
  same payload shape. The form, the dispatch, the stored request, and
  the curl command are all the same object.
- **No impedance mismatch.** Selectors are at the root, vocabulary
  fields are nested, lineage is in the data. The frontend doesn't
  need to know which is which — it reads the payload, renders
  widgets from the fields map, and POSTs the payload.
- **Self-documenting API.** The introspect response shows the user
  exactly what to send. A developer can read the GET response and
  write a curl command without consulting docs.
- **Simplified frontend.** The entire flattenToDotted / setNested /
  field-default-merging / selector-prefix-detection machinery is
  replaced by "use the payload."

### Negative

- **Breaking change for catalog detail consumers.** Any external
  client using `GET /v1/catalog/text/chat` needs to switch to
  `GET /v1/text/chat`. Mitigated: the catalog detail endpoint was
  undocumented externally and the introspect endpoint existed from
  day one.

---

## References

- [ORCH-0031](ORCH-0031-dashboard-architecture.md) — dashboard architecture
- [ORCH-0033](ORCH-0033-request-log-and-layout.md) — request log
- [ORCH-0034](ORCH-0034-provider-resolution-metadata.md) — provider metadata
- [ORCH-0035](ORCH-0035-capability-and-skill-examples.md) — examples
- `http/introspect.rs` — current introspect handler
- `http/catalog.rs` — catalog detail handler (to be deprecated)
- `services/contextualizer.rs` — payload validation pipeline
