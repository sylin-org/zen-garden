---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-29
---

# ORCH-0014: Dashboard Routing, Navigation, and UX Architecture

**Date**: 2026-03-29
**Status**: Accepted
**Applies to**: `zen-garden-ai-orchestrator` dashboard (React SPA)

---

## Context

The AI orchestrator dashboard serves two distinct concerns: AI capability
routing (choosing models) and infrastructure management (managing services).
The first implementation conflated these — capability pages embedded
offering management cards with VRAM details, load/unload buttons, and
instance health alongside model selection. This created inconsistencies
(cloud providers showing "loaded/available" labels that don't apply to
them) and confusion about the page's purpose.

A clear separation emerged through iterative review:

- **Capability pages** are about **choosing**: "I need Chat. What models
  can serve it? Which is best?"
- **Service pages** are about **managing**: "Ollama has 22 models. Which
  stones is it on? Pull a new model. Run a benchmark."

---

## Decision

### Two-Layer Information Architecture

**Layer 1: AI Capabilities** — model selection and routing.
The user thinks in capabilities (Chat, Embed, Speak), not in services.
Each capability page shows a flat model list across all providers with
a pin button. The provider name links to the service management page.

**Layer 2: Infrastructure Management** — service operations.
Each service (local or cloud) has a dedicated management page with
full operational depth. Local services show models across all
capabilities, VRAM, benchmarking, sync. Cloud providers show API key
config, model list, priority.

### Route Structure

```
/                                   Overview (capability card grid)

/capability/:name                   Capability model list
/capability/:name?model=:id         Model expanded within capability

/infra/services                     Local services list
/infra/services/:name               Service detail (e.g., /infra/services/ollama)

/infra/cloud                        Cloud providers list
/infra/cloud/:name                  Provider detail (e.g., /infra/cloud/google)
/infra/cloud/:name/edit             Edit provider config

/infra/stones                       Stone hardware view

/settings                           Global config
```

### Route Parameters

**Service and provider names** are URL-safe monikers matching
`OfferingKind::as_str()`:

| Route | Moniker |
|-------|---------|
| `/infra/services/ollama` | `ollama` |
| `/infra/services/infinity` | `infinity` |
| `/infra/services/openedai-speech` | `openedai-speech` |
| `/infra/services/libretranslate` | `libretranslate` |
| `/infra/cloud/google` | `google` |
| `/infra/cloud/anthropic` | `anthropic` |
| `/infra/cloud/openai` | `openai` |

**Model names** use query parameters (not path segments) because model
names contain URL-unfriendly characters: colons (`qwen3.5:9b`), slashes
(`sentence-transformers/all-MiniLM-L6-v2`), dots, etc. Query params
preserve the original name without encoding:

```
/capability/chat?model=qwen3.5:9b
/capability/embed?model=sentence-transformers/all-MiniLM-L6-v2
```

The query param represents a **selection state** (model detail expanded),
not a separate page. Removing the param collapses the detail. The back
button works correctly.

### Sidebar Navigation

```
AI
  Chat          ●     (green = active)
  Embed         ●
  Vision        ●
  Tools         ●
  Think         ●
  Speak         ○     (yellow = needs setup)
  Transcribe    ·     (gray = not installed)
  Imagine       ·
  ...

Infra
  Services            (local offerings)
  Cloud               (cloud providers)
  Stones              (hardware)

Settings
```

All 13 capabilities always listed (not hidden when inactive). Capability
state shown via colored dot. Infra section has three entries.

---

## Capability Page Design

**Purpose**: Model selection for a capability. Flat list, provider as
metadata.

| Column | Description |
|--------|-------------|
| Model | Name (click to expand details) |
| Provider | Offering name (click navigates to `/infra/services/:name` or `/infra/cloud/:name`) |
| Params | Parameter size |
| Stone grid | Colored squares per stone (local only) |
| Pin | Pin button |

**Pinned model banner** at top when a model is pinned. Unpin action.

**Expanded model detail** shows: family, quantization, context window,
VRAM, disk size, capabilities, per-stone placement.

**Cloud models** show in the same flat list. Provider column shows
cloud icon. No stone grid (cloud has no stones). Size column shows
"cloud" instead of disk size.

**No infrastructure management** on capability pages. No VRAM gauges,
no pull buttons, no benchmark triggers. Those live on service pages.

---

## Service Page Design

**Purpose**: Operational management of a single AI service.

### List (`/infra/services`)

Card per installed service:
- Service name, health status, stone(s)
- Model count, loaded count, VRAM usage
- Click → service detail

### Detail (`/infra/services/ollama`)

Full operational view:
- Instance info: stone, GPU, VRAM gauge, health
- Model table: ALL models (not filtered by capability), with capabilities
  column, disk size, VRAM, loaded status, stone grid
- Actions: Pull Model, Sync Models, Run Benchmark, Refresh
- Benchmark results (when available)

Each service type shows what's relevant:
- **Ollama**: models, VRAM, pull/delete/load/unload, benchmark
- **Infinity**: loaded models, engine type, batch config
- **OpenedAI Speech**: voices, engine (Piper/XTTS), test voice
- **LibreTranslate**: language pairs, download status
- **ComfyUI**: checkpoints, workflow templates, VRAM

---

## Cloud Provider Page Design

### List (`/infra/cloud`)

All known cloud providers (configured + unconfigured):
- Provider name, capabilities, status
- Configured: masked key, priority, model count, health
- Unconfigured: "Add API Key" button

### Detail (`/infra/cloud/google`)

- Connection status, masked key, priority
- Model list (from cached enumeration)
- Capabilities this provider serves
- Link to edit

### Edit (`/infra/cloud/google/edit`)

- API key input
- Priority setting
- Test Key button (per-provider validation endpoint)
- Save / Cancel

---

## Cross-Navigation

The capability page and service pages link to each other:

- **Capability → Service**: Provider name in model list is a link.
  "Ollama" → `/infra/services/ollama`. "Google ☁" → `/infra/cloud/google`.

- **Service → Capability**: Model capabilities column links to
  capability pages. "chat, embed" → clickable to `/capability/chat`.

- **Expanded model → Service**: "View in Ollama →" link in the
  expanded detail panel.

---

## Stone Grid Visualization

Local offering model tables show stone presence as colored squares:

- **Filled square** (full color): model loaded in VRAM on this stone
- **Faded square** (30% opacity): model available but not loaded
- **Empty/dark square**: model not on this stone

Colors are deterministic per stone name (hash-based palette of 8 colors).
Legend at bottom of the table maps colors to stone names.

**Future**: when benchmarking is wired, the square's border encodes
fitness verdict (green = Fast, yellow = Degraded, red = Vetoed). The
"best stone" for a model gets a highlight indicator.

---

## Design Principles Applied

1. **Capability pages are for choosing, service pages are for managing.**
   Don't mix model selection with infrastructure operations.

2. **Everything is addressable.** A model, a service, a provider, a
   stone — each has a URL. Deep-linkable, bookmarkable, shareable.

3. **Provider name is a link, not a wrapper.** The capability page
   doesn't embed offering cards. It lists models flat and lets the
   provider name be a navigation target.

4. **Cloud and local in the same list, different rendering.** Cloud
   models don't show stone grids. Local models don't show "cloud"
   badges. But they're in the same list, sortable by the same columns.

5. **Show the full potential.** All 13 capabilities visible. All 7
   cloud providers visible (configured or not). Inactive items guide
   the user toward enabling them.

---

## Consequences

### Positive

- Clear separation of concerns eliminates UI inconsistencies
- Every entity is navigable and deep-linkable
- The capability page is clean and focused on model choice
- Service management has room for full operational depth
- Cloud providers treated as first-class citizens in the model list

### Negative

- More pages to build and maintain
- Cross-navigation requires careful link management
- Model names in query params need proper encoding in edge cases

### Neutral

- The Ollama dashboard's single-page approach is replaced by a
  multi-page architecture — more pages but each is simpler
