# AI Orchestrator — UX Architecture

> Design philosophy, interaction patterns, and information architecture
> for the Zen Garden AI Orchestrator frontend. Companion to
> [ai-orchestrator-api.md](ai-orchestrator-api.md).

---

## Design principle

**Select left, work center, detail right.**

One layout pattern governs the entire application. The user's eye
moves left to right through three fixed panels:

1. **Left sidebar** — navigation and selection. Always visible. Shows
   what's available and where you are.
2. **Center panel** — the workspace. Where input happens: prompts,
   forms, lists, editors.
3. **Right panel** — the result or detail. Shows output, inspection,
   or context for whatever is selected in the center.

This pattern repeats identically across every surface. A user who
learns it in one place knows it everywhere.

---

## Three surfaces

The application has three concerns, each mapped to a top-level
surface accessible via tabs at the top of the sidebar:

| Surface | Purpose | Left shows | Center shows | Right shows |
|---------|---------|------------|--------------|-------------|
| **Create** | Use the garden's capabilities | Tool tree (primitives + skills) | Prompt / input form | Response / result |
| **Manage** | Operate skills, jobs, media | Content types (Skills, Jobs, Media, Import) | Master list or wizard | Selected item detail |
| **Configure** | Set preferences, view garden health | Settings sections (Preferences, Garden, Providers, Events) | Settings editor or dashboard | Contextual detail |

Switching surfaces swaps the sidebar content and resets the center
and right panels. The layout frame (sidebar width, panel split,
header bar) never changes.

---

## Create surface

### The tool tree

The left sidebar under Create shows every capability the garden
currently offers, grouped by modality:

```
Text
  Chat                    22 models
  Translate
  Embed

Image
  Generate                19 styles
    Flux Butterfly
    Z-Image Turbo
    Flat Vector
    Animij
    + 15 more
  Edit
  Upscale
  Analyze                 3 providers

Audio
  Speak
  Transcribe
```

**Primitives** are top-level items (Chat, Generate, Translate).
**Skills** are indented children under their parent primitive
(Flux Butterfly under Generate). The sidebar shows 4-5 skill
names directly; the rest are behind a "+N more" expander that
reveals the full list inline.

This structure is generated from `GET /v1/catalog`. Primitives
come from the `primitives` array; skills from the `skills` array,
nested under their parent primitive. When the catalog changes (a
skill is imported, a provider comes up), the sidebar updates live
via the SSE event stream (`catalog.version` events).

Badge counts (22 models, 19 styles, 3 providers) give the user a
sense of depth without requiring a click.

### The workspace

Clicking any item in the tool tree loads its workspace into the
center panel. The workspace is a **generic form renderer** driven
entirely by the catalog schema from
`GET /v1/catalog/{modality}/{leaf}[/{skill}]`.

For every field in the schema:

| `widget` value | Renders as |
|----------------|------------|
| `textarea` | Multi-line text input |
| `slider` | Range slider with `min`, `max`, `step` |
| `number` | Numeric spinner |
| `select` | Dropdown with `options` |
| `toggle` | On/off switch |
| `hidden` | Not rendered (adapter-internal) |
| `file` | File upload button / drop zone |

The primary input (usually a prompt textarea) is always visible.
Secondary fields (temperature, model selector, dimensions) live
inside a collapsible "Settings" section beneath the primary input.
This progressive disclosure serves both personas: the casual user
sees only the prompt and a "Send" button; the power user expands
Settings for full control.

The model selector defaults to "Auto (recommended:*)" and shows
a dropdown of live model names populated from the catalog's
`options` array. These options come from the adapter's real-time
probe of its instances — they are not a static list.

### The result panel

The right panel shows the response from the most recent dispatch.
Its content adapts to the output type:

- **Text** (chat, translate): rendered text with line breaks.
- **Image** (generate, edit, upscale): rendered image with
  download button.
- **Audio** (speak): audio player widget.
- **Transcript** (transcribe): rendered text.
- **Embedding** (embed): dimension count and a truncated vector
  preview.

Below the result, a metadata footer shows: provider name, model
used, latency, token count. This comes from the response's `_meta`
object. The user always knows *what answered* and *how long it took*.

Errors render in the same panel with a human-readable message and
an actionable suggestion (retry button, alternative model dropdown,
link to the Garden health view).

### URL routing

Every tool selection maps to a URL:

```
/create/text/chat
/create/text/translate
/create/image/generate
/create/image/generate/flux-butterfly
/create/audio/transcribe
```

The URL grammar mirrors the API: `/create/{modality}/{leaf}[/{skill}]`.
Sharing a URL shares the exact tool state. The browser back button
navigates between tools. Refreshing restores the same tool.

---

## Manage surface

### Skills

**Left**: "Skills" selected in the sidebar.
**Center**: A searchable master list of all skills. Each row shows:
skill name, parent primitive, provider, and status badge (ready,
provisioning, draft, failed). Click a row to inspect it.
**Right**: Detail panel for the selected skill — status, primitive,
provider, moniker, field list with widget types and constraints,
required models with filenames and types, action buttons (Try it,
Delete).

"Try it" switches to the Create surface with the skill pre-selected.
This is a one-click path from inspection to use.

### Import

**Left**: "Import skill" selected.
**Center**: A simple form — paste a CivitAI URL, drop a PNG, or
paste raw workflow JSON. Click Import.
**Right**: Live progress panel showing the import lifecycle:
Analyzing → Naming → Ready. Each step lights up as its SSE event
arrives (`skills.{moniker}.*`). The AI-generated display name
appears live when the naming step completes. On completion, the
detail panel shows the imported skill's full schema.

### Jobs

**Left**: "Jobs" selected.
**Center**: Job list with status, action, timing.
**Right**: Selected job detail — request payload, response, provider
trace, timing breakdown.

### Media

**Left**: "Media" selected.
**Center**: Uploaded media browser with thumbnails.
**Right**: Selected media metadata — dimensions, MIME type, size,
expiry.

### URL routing

```
/manage/skills
/manage/skills/flux-butterfly        (detail selected)
/manage/skills/import
/manage/jobs
/manage/jobs/{id}
/manage/media
```

---

## Configure surface

### Preferences

**Left**: "Preferences" selected.
**Center**: The global preference map as an editable table. Each
row shows the dotted field path (monospace), current value, and the
field's static default. A "Reset" button on each row removes the
preference and falls back to the default. An "+ Add preference"
button opens a field-path picker.
**Right**: Layering explanation — how preferences interact with
caller payloads, field defaults, and `recommended:*` selectors.
When a specific preference row is selected, the right panel shows
which catalog entries are affected by that preference.

### Garden

**Left**: "Garden" selected.
**Center**: Stone cards showing: stone name, health badge
(healthy/wilting/unreachable), IP address, GPU model and VRAM,
utilization bar. The utilization bar is driven by
`resources.stone.{name}.gpu.{idx}.pressure` events from the SSE
stream — it animates in real time as claims are placed and released.
**Right**: Selected stone detail — active claims, provider instances
running on that stone, model warmth status.

### Providers

**Left**: "Providers" selected.
**Center**: Provider list with health status dot, name, and
capabilities. Each row shows which primitives the provider declared
in its most recent capability announcement.
**Right**: Selected provider detail — enabled/disabled, capability
count, skill count, declared primitives, instance count (if exposed
by the adapter).

### Events

**Left**: "Events" selected.
**Center**: Live event log. A topic filter input at the top accepts
the same glob grammar as `/v1/events?focus=...`. Events stream in
real time, newest at top. Each event shows: timestamp, topic,
truncated payload.
**Right**: Selected event detail — full JSON payload, topic, sequence
number.

### URL routing

```
/configure/preferences
/configure/garden
/configure/garden/{stone-name}
/configure/providers
/configure/providers/{name}
/configure/events
```

---

## The SSE spine

Every view holds an SSE connection to `/v1/events` with a focus
pattern matching what the user is looking at:

| View | Focus |
|------|-------|
| Create (any tool) | `jobs.*` |
| Manage / Skills | `skills.*,catalog.version` |
| Manage / Import | `skills.{moniker}.*` |
| Manage / Jobs | `jobs.*` |
| Configure / Garden | `resources.stone.*,directory.provider.*` |
| Configure / Providers | `directory.provider.*` |
| Configure / Events | `*` (or user-specified filter) |

When the user navigates between views, the frontend closes the
current SSE connection and opens a new one with the appropriate
focus. `Last-Event-ID` ensures no events are missed during the
switch. The transition is seamless — the user never sees stale
state after navigation.

The SSE connection drives three behaviors:

1. **Live results**: a dispatch response appears in the right panel
   as soon as `jobs.{id}.result` fires, without polling.
2. **Live updates**: the sidebar tool tree, skill list, and garden
   dashboard update in place when the catalog, skills, or resources
   change.
3. **Progress indicators**: the import wizard steps highlight as
   `skills.{moniker}.state` events arrive.

---

## Catalog-driven rendering

The frontend hardcodes no forms, no field lists, no model dropdowns.
Every form is generated at runtime from the catalog schema:

1. User clicks a tool in the sidebar.
2. Frontend fetches `GET /v1/catalog/{modality}/{leaf}[/{skill}]`.
3. The response contains `fields[]` — each with `key`, `label`,
   `field_type`, `widget`, `required`, `default`, `min`, `max`,
   `step`, `options`, `placeholder`, `auto`.
4. The frontend renders the appropriate widget for each field.
5. User fills the form and clicks Send.
6. Frontend posts to `POST /v1/{modality}/{leaf}[/{skill}]` with
   the field values mapped to the canonical key paths.

If the garden gains a new capability (a provider comes up, a skill
is imported), the sidebar updates from the catalog and the new
tool's form renders without any frontend code change.

If an adapter updates its model list (a model is pulled or evicted),
the catalog's `options` array for the model selector field changes,
and the dropdown updates on the next catalog fetch.

This is the zero-maintenance property: the frontend is a generic
renderer, and the API is the single source of truth for what the
garden can do and how to invoke it.

---

## Progressive disclosure

Every workspace has two levels of detail:

**Level 1 (default)**: The primary input (prompt, file drop zone)
and a single action button (Send, Generate, Translate). This is
what the casual user sees. No configuration, no model selection,
no advanced parameters. The garden picks sensible defaults via
`recommended:*` and preferences.

**Level 2 (expanded)**: A collapsible "Settings" section showing
every field the catalog declares for this tool. Model selector,
temperature slider, dimension picker, system prompt — all present
but tucked away. The power user expands this on first use; the
expansion state persists in localStorage.

This means the same workspace serves both personas without
compromise. The casual user is never overwhelmed; the power user
is never constrained.

---

## Error handling

Errors are first-class UI, not browser-level failures. The API
returns structured errors with `code`, `message`, and `details`.
The frontend renders them in the right panel (the same place as
successful results) with:

- A human-readable message explaining what happened.
- An actionable suggestion specific to the error type.
- A button or link to resolve the issue.

| Error code | UX treatment |
|---|---|
| `timeout` | "The garden took too long. [Retry] or pick a faster model." |
| `pin_not_servable` | "Model X isn't available. Here's what is:" + model dropdown. |
| `no_provider` | "No provider can handle this right now. [View Garden]" |
| `validation_failed` | Inline field highlight + "Did you mean X?" suggestion from the error's `details`. |
| `upstream_error` | "The provider returned an error: {message}. [Retry]" |

The user is never left at a dead end. Every error guides toward a
resolution.

---

## Responsive behavior

The three-panel layout adapts to screen width:

- **Wide (>1200px)**: All three panels visible simultaneously.
- **Medium (800-1200px)**: Sidebar collapses to icons only (tooltip
  on hover). Center and right panels share the remaining width.
- **Narrow (<800px)**: Single-panel mode. The sidebar becomes a
  hamburger menu. Center and right panels stack vertically (input
  above, result below).

The panel proportions are:
- Sidebar: 220px fixed.
- Center: flexible, minimum 360px.
- Right: 340px fixed, collapses on medium screens.

---

## Personas and their paths

### The Creative

Arrives at `/create`. Sees "Generate" in the sidebar with "19 styles."
Clicks a style name → sees a prompt textarea. Types a description.
Clicks Generate. Watches the result appear in the right panel. Never
opens Settings. Never sees a model name.

**Click count**: 2 (style + Generate).

### The Developer

Doesn't use the frontend — hits the API directly. But when debugging
or exploring, opens `/create/text/chat`. Types a prompt. Opens
Settings to pin a specific model. Sends. Reads the `_meta` in the
result panel for provider, latency, and token counts. Copies the
equivalent curl command from the API docs.

**Click count**: 1 (Send) + Settings toggle.

### The Operator

Starts at `/manage/skills`. Reviews the skill list. Clicks Import.
Pastes a CivitAI URL. Watches the import lifecycle in the right
panel. Switches to `/configure/garden` to check stone health. Sees
a GPU utilization bar spiking on stone-01. Clicks the stone card
to see active claims. Switches to `/configure/providers` to verify
all 7 providers are healthy.

**Click count**: 1 per navigation.

### The Tinkerer

Starts at `/create/text/chat` with Settings expanded. Tries
temperature 2.0. Switches to `/create/image/generate` and picks
a style. Opens `/manage/skills/import` and pastes a random
workflow JSON. Watches it fail, reads the error detail in the right
panel, adjusts, retries. Opens `/configure/events` with
`focus=*` to watch everything happening in real time.

**Click count**: 1 per action. Everything is one click away.

---

## Implementation notes

### API mapping

| Frontend need | API call | Cache strategy |
|---|---|---|
| Tool tree (sidebar) | `GET /v1/catalog` | Cache until `catalog.version` event |
| Workspace form | `GET /v1/catalog/{mod}/{leaf}[/{skill}]` | Cache per-path, bust on catalog event |
| Dispatch | `POST /v1/{mod}/{leaf}[/{skill}]` | No cache |
| Live results | `GET /v1/events?focus=...` | SSE stream |
| Skill list | `GET /v1/skills` | Cache until `skills.list.changed` event |
| Skill import | `POST /v1/skills/{provider}/import` | No cache |
| Import progress | SSE `skills.{moniker}.*` | Stream |
| Preferences | `GET/PUT /v1/preferences` | Cache until `preferences.changed` event |
| Garden health | `GET /v1/resources` | Cache until `resources.stone.*` event |
| Provider list | From `GET /v1/catalog` `providers` array | Same as tool tree |
| Jobs | `GET /v1/jobs` | Poll or SSE `jobs.*` |

Nine distinct API calls cover the entire frontend. Every cache is
invalidated by a specific event topic. No polling except as a
fallback.

### Local state

Stored in `localStorage`, never on the server:

- **Recent tools**: frequency-sorted list of `/create/...` paths.
  Rendered as quick-access pills or sidebar highlights.
- **Settings expansion**: whether the Settings section is collapsed
  or expanded per tool. Persists across sessions.
- **Chat history**: conversation turns for `text.chat`, replayed
  via `text.prompt.previous` on each send.
- **Event filter**: last-used glob pattern for the Events view.
- **Panel widths**: user-resized panel proportions (if draggable
  dividers are implemented).

### Technology-agnostic

This document describes the UX architecture, not the implementation
stack. The patterns work with any SPA framework (React, Vue, Svelte,
Solid) or with vanilla JS. The prototype at
`docs/proposals/ux/proposal-c-full.html` demonstrates the layout
and interactions with zero dependencies.

The critical technical requirement is SSE support with dynamic
reconnection — the frontend must handle `Last-Event-ID` resumption
across focus changes.

---

## Prototype

An interactive HTML prototype demonstrating the full three-surface
layout is available at:

```
docs/proposals/ux/proposal-c-full.html
```

Open it in a browser to interact with all three surfaces. The
prototype simulates API responses and demonstrates the select-left,
work-center, detail-right pattern across Create, Manage, and
Configure.
