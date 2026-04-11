---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0031: Dashboard Architecture — Catalog-Driven SPA with SSE Spine

**Date**: 2026-04-10
**Status**: Accepted
**Deciders**: Leo
**Related ADRs**:
- ORCH-0028 (orchestrator core) — defines the vocabulary, Directory, Provider trait, dispatch pipeline the dashboard consumes
- ORCH-0029 (skill subsystem) — skill lifecycle, import pipeline, skill schema the dashboard renders
- ORCH-0030 (architecture realignment) — catalog two-view, events bus, REST sugar the dashboard targets

The existing React dashboard is architecturally misaligned: it was built for a different API contract (`/api/status`, `/api/events` with named event types) that was never implemented. The Rust backend has matured into a complete REST + SSE surface (`/v1/*`) documented in ORCH-0030. This ADR defines the replacement dashboard — a catalog-driven SPA that renders all forms from backend schema, tracks all state through SSE, and carries zero client-side data maps that could desync from the backend.

---

## Mandate

**The backend is the single source of truth for everything the UI renders.**

No client-side icon maps keyed by primitive codes. No hardcoded label tables. No widget-type assumptions. If the backend doesn't provide a field, the UI doesn't show it. If a new primitive appears tomorrow, the dashboard renders it without a code change — because the catalog told it what fields exist, what widgets to use, and what options are available.

This mandate has one architectural consequence: the frontend is a **generic renderer** of backend-provided schema, not a bespoke application with per-feature views. The catalog endpoint is the form definition. The dispatch endpoint is the form submission. The event stream is the state synchronization. Everything else is layout and interaction.

---

## Context

### What exists

The orchestrator serves a complete API surface:

| Domain | Endpoints | Shape |
|--------|-----------|-------|
| Catalog | `GET /v1/catalog`, `GET /v1/catalog/{mod}/{leaf}[/{skill}]` | Navigation summary + full field schema per registration |
| Dispatch | `POST /v1/do`, `POST /v1/{mod}/{leaf}[/{skill}]` | Universal verb + REST sugar; sync, async (202), or streaming (SSE) responses |
| Introspect | `GET /v1/{mod}/{leaf}[/{skill}]` | Routing info, invocation URL, parameters with effective defaults |
| Events | `GET /v1/events?focus=glob` | Unified SSE bus with glob filtering and `Last-Event-ID` resumption |
| Skills | `GET /v1/skills`, `POST /v1/skills/{provider}/import`, `DELETE /v1/skills/{id}` | Skill CRUD + import pipeline with lifecycle events |
| Jobs | `GET /v1/jobs`, `GET /v1/jobs/{id}`, `GET /v1/jobs/{id}/result` | Job tracking with state machine |
| Media | `POST /v1/media`, `GET /v1/media/{id}`, `GET /v1/media` | Upload, download, list |
| Preferences | `GET /v1/preferences`, `PUT /v1/preferences` | Flat key-value map, merge semantics |
| Resources | `GET /v1/resources`, `GET /v1/resources/stones/{name}` | Stone hardware, GPU devices, claims |
| Providers | via catalog `providers[]` + `directory.provider.*` events | Provider health, capability counts |

The catalog detail endpoint returns typed field descriptors:

```json
{
  "field": "text.sampling.temperature",
  "field_type": "number",
  "widget": "slider",
  "label": "Temperature",
  "min": 0.0, "max": 2.0, "step": 0.1,
  "default": 0.7,
  "required": false
}
```

Widget types: `textarea`, `slider`, `number`, `select`, `toggle`, `hidden`, `file`.

The events bus carries topic-scoped events with sequence IDs:

```
event: skills.flux-butterfly.state
id: 48
data: {"topic":"skills.flux-butterfly.state","state":"ready","seq":48,"at":"..."}
```

### What's broken

The current `dashboard/` directory contains a React + Vite + Tailwind app that:
- Expects `GET /api/status` and `GET /api/events` (named event types like `registry.updated`) — endpoints that don't exist
- Has routes for `/capability/{name}`, `/infra/services`, `/infra/cloud`, `/settings` — none matching the actual API
- Contains `types.ts` referencing a `dashboard.rs` response struct that was never written
- Has `rust-embed` in Cargo.toml but no serving handler

The entire `dashboard/src/` is replaced. The tech stack (React 19, Vite 8, Tailwind v4, TypeScript 5.9, React Router v7) is preserved.

### Backend gaps discovered during investigation

1. **ComfyUI publishes empty `parameters` on capabilities.** `compute_capabilities()` in `providers/comfyui.rs` hardcodes `parameters: vec![]` on all capabilities. Ollama populates base parameters via `base_parameters_for()`. Result: `GET /v1/catalog/image/generate` returns no `fields[]`, making the primitive unusable without selecting a skill first. Fix: ComfyUI should declare common image generation parameters (prompt, negative prompt, width, height, steps, guidance, seed) at the capability level.

2. **No modality icons in catalog.** The catalog groups primitives by `modality` string but provides no icon or visual indicator. Fix: add an `icon` field (unicode) to the modality grouping in the catalog summary.

3. **Media delivery flow untested end-to-end.** Capabilities declare `media_inputs` with `delivery: "transfer"` but the upload-then-reference flow (`POST /v1/media` → `media_id` → inject into dispatch) needs integration verification.

---

## Decision

### 1. Three-surface, three-panel layout

The application has three concerns, each a top-level surface:

| Surface | Purpose | Left sidebar | Center panel | Right panel |
|---------|---------|-------------|-------------|------------|
| **Create** | Use capabilities | Tool tree (primitives + skills grouped by modality) | Catalog-driven form or skill picker | Dispatch result (text, image, audio, embed) |
| **Manage** | Operate skills, jobs, media | Section nav (Skills, Jobs, Media) | Master list or import wizard | Selected item detail |
| **Configure** | Preferences, garden health, providers, events | Section nav | Editor or dashboard | Contextual detail |

The layout is fixed: 220px sidebar, flexible center (min 360px), 340px right panel. The sidebar frame, header bar, and panel split never change — only the content within each panel changes per route.

### 2. Catalog-driven form rendering

The center panel of Create renders forms dynamically from `GET /v1/catalog/{mod}/{leaf}[/{skill}]`:

```
field.widget → component
  textarea  → <Textarea>
  slider    → <Slider min={} max={} step={}>
  select    → <Select options={}>
  number    → <NumberInput>
  toggle    → <Toggle>
  file      → <FileUpload acceptedTypes={}>
  hidden    → (omitted from UI)
```

The primary input (`required: true`, widget `textarea` or `file`) renders prominently. All secondary fields render inside a collapsible Settings section. The expansion state persists in `localStorage`.

When a primitive has no `fields` in its catalog detail (currently `image.generate` until the ComfyUI fix lands), the workspace shows a skill picker: card grid of the skills under that primitive, sourced from the catalog summary's `skills[]` filtered by `primitive`. After the ComfyUI fix, the primitive renders a base form and the skill picker becomes an optional refinement.

### 3. Two SSE connections

**Job feed** (always on): A singleton `JobManager` holds one persistent SSE connection to `/v1/events?focus=jobs.*`. This tracks all dispatched jobs regardless of which surface the user is on. The JobManager maintains a registry of tracked jobs, updates their state from SSE events, and fetches results on terminal state.

**Route feed** (changes on navigation): Each route opens an SSE connection scoped to its concern:

| Route | Focus pattern |
|-------|--------------|
| `/create/*` | `catalog.version` |
| `/manage/skills` | `skills.*,catalog.version` |
| `/manage/skills/import` | `skills.{moniker}.*` |
| `/manage/jobs` | `jobs.*` |
| `/configure/garden` | `resources.stone.*` |
| `/configure/providers` | `directory.provider.*` |
| `/configure/events` | `*` or user-specified filter |

On navigation, the old route feed closes and a new one opens with the appropriate focus. `Last-Event-ID` ensures no events are missed during the switch. On reconnection, the route re-fetches its snapshot to avoid stale state.

### 4. JobManager — client-side job tracking

The JobManager is a singleton React context at the app root. It owns the global job lifecycle:

1. **`track(jobId, action)`**: Called after every dispatch. Registers the job in the manager's registry.
2. **SSE integration**: The `jobs.*` feed updates job state in real time — `queued → running → done|failed|cancelled`. Progress events update `{ current, total, label }`.
3. **`useJob(id) → JobState`**: Any component subscribes to a specific job. Returns reactive state: `{ status, progress, result, error, timing }`. Re-renders only when that job changes.
4. **Result fetch**: On terminal state (`done`, `failed`), the manager fetches `GET /v1/jobs/{id}/result` once and caches it.
5. **Recent jobs**: Maintains the last N jobs for the Manage/Jobs view. The initial list loads from `GET /v1/jobs`; SSE keeps it current.
6. **Garbage collection**: Terminal jobs older than the session are dropped from the in-memory registry. History comes from `GET /v1/jobs` on demand.

### 5. Conversational chat UI

`text.chat` is conversational, not one-shot. The Create workspace for `text.chat` specifically renders a conversation thread:

- Message history accumulates in component state (and `localStorage` for persistence)
- Each dispatch includes `text.prompt.previous` with all prior turns
- The center panel shows message bubbles (user / assistant alternating)
- The textarea sits at the bottom; Settings (model, temperature) collapse above
- Streaming dispatch renders assistant tokens incrementally via SSE

This is a special case of the generic form renderer: the catalog still drives the available fields, but the layout is conversation-shaped rather than form-shaped. The detection is automatic — if the catalog detail for a primitive includes a field with path `text.prompt.previous`, the workspace switches to conversation mode.

### 6. Media upload flow

For primitives with `media_inputs` (image.edit, image.upscale, image.analyze, audio.transcribe):

1. The catalog detail includes `media_inputs[]` with `field`, `accepted_types`, and `delivery`
2. The form renderer detects `delivery: "transfer"` and renders a file drop zone / picker
3. On submit: `POST /v1/media` (binary body, `Content-Type` from file) → receive `media_id`
4. Inject `{"media_id": "..."}` at the field path declared by `media_inputs[].field`
5. Dispatch with the media reference

Upload progress is visible in the UI. The drop zone validates `accepted_types` client-side before upload.

### 7. Streaming dispatch

The dispatch pipeline supports three response modes:

- **Sync** (200): JSON response in the result panel immediately
- **Async** (202): Job ID returned; JobManager tracks completion
- **Streaming** (SSE): Response headers indicate `text/event-stream`; tokens render incrementally in the result panel

The frontend detects the response mode from the HTTP status and `Content-Type` header. For streaming, a dedicated reader consumes the SSE stream and appends to the result panel in real time.

### 8. Static serving via `rust-embed`

The Vite build outputs to `dashboard/dist/`. The Rust binary embeds this directory via `rust-embed` and serves it at `/` with SPA fallback (all non-`/v1/`, non-`/health`, non-`/metrics` paths return `index.html`). The Docker multi-stage build runs `npm ci && npm run build` before `cargo build`.

Zero-config deployment: the orchestrator binary is a single artifact that serves both the API and the dashboard.

### 9. Idempotency and double-submit protection

Every dispatch generates a client-side UUID as the `idempotency-key` header. The Send button disables on click and re-enables only after the response (or timeout). The backend's idempotency cache prevents duplicate execution if the user retries.

### 10. No client-side data maps

The frontend contains zero mappings of backend identifiers to display properties:

- **Icons**: Modality icons come from the backend catalog (`icon` field). No `Record<string, icon>` on the client.
- **Labels**: All display text comes from `display_name`, `label`, `summary`, `description` fields in API responses.
- **Widget types**: Determined by the `widget` field in the catalog schema.
- **Model lists**: Populated from the `options` array on the `selectors.model` field.
- **Status colors**: Derived from semantic status strings (`ready`, `failed`, `draft`) with a small CSS class mapping (3 states, not per-entity).

If the backend adds a new primitive, modality, or skill tomorrow, the dashboard renders it without any code change.

### 11. Code splitting

Three lazy-loaded route chunks — one per surface:

```
root bundle (~15KB gzip):
  Shell         — sidebar frame, surface tabs, header, panel split
  JobManager    — global job tracking context
  useSSE        — shared SSE hook
  useCatalog    — catalog summary fetch + cache context
  api           — fetch wrapper with error handling

create/ (lazy):
  Tool tree, Workspace (generic form + conversation), ResultPanel, widget components

manage/ (lazy):
  SkillList, SkillDetail, ImportWizard, JobList, JobDetail, MediaBrowser

configure/ (lazy):
  PreferenceEditor, GardenView, StoneDetail, ProviderList, ProviderDetail, EventLog
```

The root bundle loads instantly with the three-panel frame. Surface content appears after the lazy chunk loads — typically <100ms on a local network.

### 12. Vocabulary — developer affordances

Every primitive carries a `vocabulary` block in the catalog summary with `examples.minimal`, `examples.full`, and `input.aliases`. The Create workspace exposes:

- **Copy as curl**: Generates a curl command from the current form state. Uses the `invocation.url` from introspection and maps form values to the canonical payload shape.
- **API example**: Shows the minimal and full example payloads from the vocabulary. Collapsible panel in the right sidebar.

---

## Backend changes required

These changes are prerequisites or companions to the dashboard work:

| ID | Change | Where | Why |
|----|--------|-------|-----|
| B1 | ComfyUI base parameters for image capabilities | `providers/comfyui.rs` `compute_capabilities()` | `image.generate`, `image.edit`, `image.upscale` have no fields at the primitive catalog level |
| B2 | Add `icon` (unicode) to modality in catalog summary | `http/catalog.rs` + catalog builder | Sidebar tool tree needs visual grouping without client-side icon maps |
| B3 | Static file serving via `rust-embed` | `http/router.rs` + new `http/static_files.rs` | Dashboard needs to be served from the orchestrator binary |
| B4 | Integration test: media upload → dispatch flow | Test suite | Verify `POST /v1/media` → `media_id` → dispatch with `delivery: "transfer"` works end-to-end |

---

## Consequences

### Positive

- **Zero-maintenance forms.** A new provider, primitive, or skill appears in the dashboard the moment it registers with the Directory. No frontend deploy needed.
- **Single artifact.** The orchestrator binary serves both API and dashboard. No nginx, no separate container, no CORS.
- **SSE-driven liveness.** Every view updates in real time. No polling loops, no stale state.
- **No desync risk.** Icons, labels, options, and field schemas all come from the backend. The frontend is a rendering engine for backend-provided data.

### Negative

- **Catalog endpoint becomes critical path.** If the catalog is slow or malformed, the entire UI is broken. Mitigated by ETag caching and the catalog's in-memory construction.
- **Chat conversation state in localStorage.** Browser-local, not synced across devices. Acceptable for a dashboard — this is not a chat product.
- **Two SSE connections per session.** One for jobs (always), one for the current route. Slightly more server load than a single multiplexed connection. Acceptable given the orchestrator's scale (single-digit concurrent users).

### Risks

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| ComfyUI base parameters don't cover all skill variations | Medium | The skill-level form always works; base parameters are a convenience, not a gate |
| Streaming dispatch parsing complexity | Low | The backend already formats SSE correctly; the frontend just needs an EventSource reader |
| Large catalog payloads on gardens with many skills | Low | ETag caching + the summary endpoint is compact; detail endpoints are per-tool |

---

## References

- [ai-orchestrator-api.md](../reference/ai-orchestrator-api.md) — complete API reference
- [ai-orchestrator-ux.md](../reference/ai-orchestrator-ux.md) — UX architecture spec
- [AI-Orchestrator-proposal-full.html](../proposals/ux/AI-Orchestrator-proposal-full.html) — interactive prototype (layout and feel reference only; technical details superseded by this ADR)
- [ORCH-0028](ORCH-0028-orchestrator-core.md) — orchestrator core architecture
- [ORCH-0029](ORCH-0029-skill-subsystem.md) — skill subsystem
- [ORCH-0030](ORCH-0030-orchestrator-architecture-realignment.md) — API surface and event bus
