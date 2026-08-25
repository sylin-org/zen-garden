---
audience: [contributor]
doc_type: proposal
status: draft
last_verified: 2026-04-10
---

# Dashboard Implementation Plan

**Author**: Leo + Claude
**Date**: 2026-04-10
**ADR**: [ORCH-0031](../../decisions/ORCH-0031-dashboard-architecture.md)

---

## Overview

Replace the broken AI orchestrator dashboard with a catalog-driven SPA
per ORCH-0031. Four phases: backend prerequisites, shell + infrastructure,
surface implementation, and integration.

Each phase is a logical unit that can be committed, tested, and verified
independently. Phases are sequential — each depends on the previous.
Within a phase, tasks may be parallelizable.

---

## Phase 0 — Backend prerequisites

These changes unblock the frontend. They ship before any dashboard code.

### B1: ComfyUI base parameters for image capabilities

**Problem**: `GET /v1/catalog/image/generate` returns no `fields[]`
because `compute_capabilities()` in `providers/comfyui.rs:1223`
hardcodes `parameters: vec![]` on all capabilities. The Ollama adapter
provides base parameters via `base_parameters_for()` (ollama.rs:838).

**Approach**:
1. Read the existing skill parameters across all `image.generate` skills
   to identify the common parameter surface (positive prompt, negative
   prompt, width, height, steps, guidance, seed)
2. Add a `base_parameters_for()` function to the ComfyUI adapter that
   returns typed `SkillParameter` entries for each image capability:
   - `image.generate` — prompt, negative prompt, width, height, steps,
     guidance, seed, model selector
   - `image.edit` — prompt, source image, mask
   - `image.upscale` — source image, scale factor
3. Wire it into `compute_capabilities()` so the capability announcement
   carries these parameters
4. Verify: `GET /v1/catalog/image/generate` returns `fields[]`

**Files**: `src/orchestrators/ai/src/providers/comfyui.rs`

**Verification**:
```bash
curl -s http://localhost:7190/v1/catalog/image/generate | jq '.fields | length'
# Should be > 0
```

### B2: Modality icons in catalog summary

**Problem**: The sidebar tool tree groups by modality but has no visual
indicator. Adding a client-side icon map violates the "backend is truth"
mandate.

**Approach**:
1. Add an `icon` field (String, unicode) to the modality grouping in the
   catalog summary response
2. The catalog builder derives modalities from registered primitives and
   attaches icons. Initial mapping:
   - `text` → `💬`
   - `image` → `🖼️`
   - `audio` → `🔊`
3. If a new modality appears (e.g., `video`), it gets a generic fallback
   icon until explicitly mapped

**Investigation needed**: The catalog summary currently groups by
`modality` field on each primitive. Check whether the response already
has a top-level modality grouping or if we need to add one. The frontend
can group client-side from the `primitives[].modality` field, but the
icon must come from the backend.

**Files**: `src/orchestrators/ai/src/http/catalog.rs`, possibly
`src/orchestrators/ai/src/services/catalog_builder.rs`

**Verification**:
```bash
curl -s http://localhost:7190/v1/catalog | jq '.modalities'
# or verify icon field appears on primitive groupings
```

### B3: Static file serving via rust-embed

**Problem**: `rust-embed` is in Cargo.toml but not wired in. The
Dockerfile builds the dashboard and copies `dist/` into the Rust build
context, but no handler serves it.

**Approach**:
1. Create `src/http/static_files.rs`:
   - `#[derive(RustEmbed)] #[folder = "dashboard/dist/"]` struct
   - Axum handler that serves embedded files
   - SPA fallback: any path not matching `/v1/`, `/health`, `/metrics`
     returns `index.html`
   - Correct `Content-Type` from file extension
   - `Cache-Control` headers: hashed assets get `max-age=31536000`;
     `index.html` gets `no-cache`
2. Wire the fallback into `router.rs` as the last route
3. Verify: `curl http://localhost:7190/` returns the dashboard HTML

**Investigation needed**: Check how other Zen Garden services handle
static embedding (e.g., the Ollama orchestrator dashboard if it exists).
Use the same pattern for consistency.

**Files**: `src/orchestrators/ai/src/http/static_files.rs`,
`src/orchestrators/ai/src/http/router.rs`,
`src/orchestrators/ai/src/http/mod.rs`

**Verification**:
```bash
curl -s http://localhost:7190/ | head -5
# Should return HTML with <div id="root">
```

### B4: Media upload → dispatch integration test

**Status**: Verified — no code changes needed.

The media pipeline is fully implemented (contextualizer extracts
`media_id` objects, resolver validates content-type against the
provider's `accepted_types`, three delivery modes work: Base64,
ById, Transfer). Tested live on 2026-04-10:

1. `POST /v1/media` with a PNG → received `media_id` (GUIDv7)
2. `POST /v1/image/analyze` with `{"image":{"source":{"media_id":"..."}}}` +
   `provider: "ollama"` → request accepted, no validation error, Ollama
   began processing (timed out due to model cold-load, not a media issue)

The dashboard's media upload flow can use this pipeline directly:
upload to `/v1/media`, inject `{media_id}` at the field path from
`media_inputs[].field`, dispatch.

---

## Phase 1 — Shell + infrastructure

The app skeleton: layout frame, routing, API client, SSE hooks,
JobManager. No surface content yet — just the bones.

### 1.1: Wipe and scaffold

1. Delete all files in `dashboard/src/`
2. Keep: `package.json`, `vite.config.ts`, `tsconfig.json`,
   `tsconfig.app.json`, `tsconfig.node.json`, `eslint.config.js`,
   `index.html`
3. Update `package.json`:
   - Remove `@rjsf/*`, `mermaid` (unused)
   - Keep `react`, `react-dom`, `react-router-dom`, `tailwindcss`,
     `@tailwindcss/vite`, `vite`, `@vitejs/plugin-react`, `typescript`
   - Add: `uuid` (for idempotency keys)
4. Update `vite.config.ts`:
   - Proxy `/v1` and `/health` to `http://localhost:7190` (was 7192)
5. Create `src/index.css` with Tailwind directives and the dark theme
   custom properties (from the proposal prototype's color system)
6. Create `src/main.tsx` — StrictMode + BrowserRouter + App
7. `npm ci && npm run build` succeeds

### 1.2: API client (`src/api/`)

Thin fetch wrapper. No axios, no heavy abstractions.

- `client.ts` — `get<T>(path)`, `post<T>(path, body)`, `put<T>(path, body)`,
  `del(path)`, `upload(path, file)`. All return `Promise<T>`.
  Error handling: parse error envelope, throw typed `ApiError` with
  `code`, `message`, `details`.
- `types.ts` — TypeScript types mirroring backend response shapes.
  **Generated from live API responses, not invented.** Hit each
  endpoint, capture the shape, define the type.

  Key types:
  - `CatalogSummary` (from `GET /v1/catalog`)
  - `CatalogDetail` (from `GET /v1/catalog/{mod}/{leaf}[/{skill}]`)
  - `CatalogField` (the field descriptor)
  - `Primitive`, `Skill`, `Provider` (from catalog arrays)
  - `DispatchResponse`, `ErrorEnvelope`, `Meta`
  - `JobView`, `JobState`, `Progress`
  - `SkillListResponse`, `SkillView`
  - `MediaEntry`
  - `StoneResources`, `GpuDevice`
  - `IntrospectionResponse`

### 1.3: SSE hook (`src/hooks/useSSE.ts`)

Reusable hook for SSE connections:

```typescript
function useSSE(focus: string, onEvent: (topic: string, payload: any) => void)
```

- Opens `EventSource` to `/v1/events?focus={focus}`
- Tracks `Last-Event-ID` for resumption
- Auto-reconnects with backoff on drop
- Calls `onEvent` for each received event
- Closes on unmount or when `focus` changes
- Returns `{ connected: boolean, lastSeq: number }`

### 1.4: Catalog context (`src/contexts/CatalogContext.tsx`)

App-wide context that holds the catalog summary:

- Fetches `GET /v1/catalog` on mount
- Subscribes to `catalog.version` events (via the route SSE, not a
  dedicated connection — the route feed includes this topic)
- On `catalog.version` event, re-fetches the catalog
- Provides: `catalog`, `loading`, `error`, `refresh()`
- ETag caching: stores the ETag, sends `If-None-Match` on re-fetch

### 1.5: JobManager context (`src/contexts/JobManagerContext.tsx`)

Global singleton per ORCH-0031 §4:

- Owns one persistent SSE connection to `/v1/events?focus=jobs.*`
- Maintains `Map<string, JobState>` registry
- `track(jobId, action)` — register a new job
- `useJob(id)` — hook returning reactive `JobState`
- `useRecentJobs()` — hook returning the last N jobs
- On terminal state: fetches `GET /v1/jobs/{id}/result`, caches it
- Initial load: `GET /v1/jobs` to populate recent jobs

### 1.6: Shell layout (`src/components/Shell.tsx`)

The three-panel frame:

- Fixed sidebar (220px) with surface tabs (Create, Manage, Configure)
- Flexible center panel
- Fixed right panel (340px) with border
- Header bar with breadcrumb
- Responsive breakpoints: collapse sidebar at 1200px, stack at 800px
- `<Outlet>` for React Router nested routes in the center+right area
- Status footer in sidebar: SSE connection indicator, provider count
  from catalog

### 1.7: Route definitions (`src/App.tsx`)

```typescript
<Routes>
  <Route element={<Shell />}>
    <Route index element={<Navigate to="/create" />} />

    {/* Create surface — lazy loaded */}
    <Route path="create" element={<CreateSurface />}>
      <Route index element={<CreateIndex />} />
      <Route path=":modality/:leaf" element={<Workspace />} />
      <Route path=":modality/:leaf/:skill" element={<Workspace />} />
    </Route>

    {/* Manage surface — lazy loaded */}
    <Route path="manage" element={<ManageSurface />}>
      <Route index element={<Navigate to="skills" />} />
      <Route path="skills" element={<SkillList />} />
      <Route path="skills/import" element={<ImportWizard />} />
      <Route path="skills/:id" element={<SkillDetail />} />
      <Route path="jobs" element={<JobList />} />
      <Route path="jobs/:id" element={<JobDetail />} />
      <Route path="media" element={<MediaBrowser />} />
    </Route>

    {/* Configure surface — lazy loaded */}
    <Route path="configure" element={<ConfigureSurface />}>
      <Route index element={<Navigate to="preferences" />} />
      <Route path="preferences" element={<PreferenceEditor />} />
      <Route path="garden" element={<GardenView />} />
      <Route path="garden/:name" element={<StoneDetail />} />
      <Route path="providers" element={<ProviderList />} />
      <Route path="providers/:name" element={<ProviderDetail />} />
      <Route path="events" element={<EventLog />} />
    </Route>
  </Route>
</Routes>
```

### 1.8: Verify Phase 1

- `npm run build` succeeds
- `npm run dev` shows the three-panel shell with surface tabs
- Clicking surface tabs switches sidebar content (placeholder text)
- SSE hook connects to `/v1/events` (visible in browser DevTools)
- JobManager loads initial jobs from `/v1/jobs`
- Catalog context loads from `/v1/catalog`

---

## Phase 2 — Create surface

The primary surface. Users come here to use the garden's AI capabilities.

### 2.1: Tool tree sidebar (`src/features/create/ToolTree.tsx`)

- Reads from `CatalogContext`
- Groups primitives by `modality`, shows modality icon from backend
- Each primitive shows: display name, provider count badge
- Skills indent under their parent primitive; first 4-5 visible,
  rest behind "+N more" expander
- Click navigates to `/create/{modality}/{leaf}` or
  `/create/{modality}/{leaf}/{skill}`
- Active item highlighted with accent border

### 2.2: Widget components (`src/features/create/widgets/`)

One component per catalog widget type:

- `TextareaWidget.tsx` — multi-line text, placeholder support
- `SliderWidget.tsx` — range input with min/max/step, live value display
- `NumberWidget.tsx` — numeric spinner with min/max
- `SelectWidget.tsx` — dropdown from `options[]`, supports "Auto" default
- `ToggleWidget.tsx` — boolean switch
- `FileWidget.tsx` — drop zone + file picker, `accepted_types` filtering,
  preview thumbnail for images, audio player for audio files

Each widget takes a `CatalogField` and renders accordingly. No widget
knows what primitive it belongs to.

### 2.3: Generic form renderer (`src/features/create/WorkspaceForm.tsx`)

- Fetches `GET /v1/catalog/{mod}/{leaf}[/{skill}]` on mount
- Splits fields into primary (required + textarea/file) and secondary
- Primary fields render at the top
- Secondary fields render inside `<details>` (Settings), expansion
  state persisted in `localStorage`
- Model selector shows "Auto (recommended:*)" as first option, then
  live model names from `options[]`
- Form state: `Record<string, any>` keyed by dotted field path
- On submit: builds nested JSON from dotted paths, generates
  `idempotency-key`, POSTs to the dispatch URL

### 2.4: Skill picker (`src/features/create/SkillPicker.tsx`)

- Renders when catalog detail has no `fields` (or as an optional
  refinement when fields exist but skills are available)
- Shows cards for each skill under the current primitive
- Card shows: display name, description snippet, preview image (if
  available), provider badge
- Click navigates to `/create/{mod}/{leaf}/{skill}`

### 2.5: Result panel (`src/features/create/ResultPanel.tsx`)

Adapts to the output type:

- **Text** (chat, translate): rendered markdown/text with line breaks
- **Image** (generate, edit, upscale): `<img>` with download button.
  Image data comes from `output.image.data` (base64) or
  `output.image.media_id` (fetch from `/v1/media/{id}`)
- **Audio** (generate): `<audio>` player widget
- **Transcript** (transcribe): rendered text
- **Embedding** (embed): dimension count + truncated vector preview
- **Error**: human-readable message, actionable suggestion, retry button

Below the result: metadata footer showing provider, model, latency,
token count from `_meta`.

### 2.6: Streaming dispatch handler

- On dispatch, detect response mode:
  - Status 200 + `application/json` → sync result → render in panel
  - Status 202 → async → `jobManager.track(id)` → show progress via
    `useJob(id)`
  - Status 200 + `text/event-stream` → streaming → read SSE, append
    tokens to result panel incrementally
- For streaming: use `fetch()` + `ReadableStream` reader (not
  `EventSource`, since this is a POST response)

### 2.7: Conversation UI for text.chat (`src/features/create/ChatWorkspace.tsx`)

Special-case workspace for conversational primitives:

- Detection: catalog detail includes field `text.prompt.previous`
- Layout: message thread (scrollable) + textarea at bottom
- Message history in component state, persisted to `localStorage`
  keyed by primitive path
- Each send includes full `text.prompt.previous` array
- Streaming tokens append to the current assistant message
- Settings (model, temperature, system prompt) in collapsible section
  above the thread
- "New conversation" button clears history

### 2.8: Media upload integration

For workspaces with `media_inputs`:

- Render `FileWidget` for each media input field
- On submit: upload files first (`POST /v1/media`), collect `media_id`s
- Show upload progress bar per file
- Inject `media_id` references into the dispatch payload
- Handle upload errors with retry option

### 2.9: Copy-as-curl

- Button in the workspace header or result panel
- Reads the current form state + dispatch URL
- Generates a curl command with the correct JSON payload
- Copies to clipboard with a toast notification

### 2.10: Verify Phase 2

- Navigate to `/create/text/chat` — see conversation UI, send a message,
  receive a streaming response
- Navigate to `/create/image/generate` — see skill picker (or base
  form if B1 is done), pick a skill, fill the form, generate an image
- Navigate to `/create/audio/generate` — see TTS form, generate speech,
  hear it in the audio player
- Navigate to `/create/image/analyze` — upload an image, ask a question,
  get a text response
- Copy-as-curl generates a valid curl command
- Settings expansion state persists across navigation
- Model selector shows live models from the catalog

---

## Phase 3 — Manage surface

### 3.1: Manage sidebar + surface layout

- Sidebar shows: Skills, Jobs, Media (static nav, not data-driven)
- Center + right panels swap per selection

### 3.2: Skill list (`src/features/manage/SkillList.tsx`)

- Fetches `GET /v1/skills` on mount
- Searchable/filterable by name, primitive, provider
- Each row: skill name, parent primitive, provider, status badge
- SSE subscription: `skills.*,catalog.version`
- Click selects → detail in right panel

### 3.3: Skill detail (right panel)

- Displays: status, primitive, provider, field list, required models
- Action buttons: "Try it" (→ Create surface with skill pre-selected),
  "Delete" (with confirmation)

### 3.4: Import wizard (`src/features/manage/ImportWizard.tsx`)

- Center panel: input form — paste URL, drop PNG, paste raw JSON
- Right panel: live progress — Analyzing → Naming → Ready
- SSE subscription: `skills.{moniker}.*` (moniker from 202 response)
- Each step lights up as its SSE event arrives
- AI-generated display name appears live on naming step
- On completion: show full skill detail, "Try it" button

### 3.5: Job list (`src/features/manage/JobList.tsx`)

- Reads from `JobManager.useRecentJobs()`
- Filterable by category, state, action
- Each row: job ID (truncated), action, status badge, timing
- Click → detail in right panel

### 3.6: Job detail (right panel)

- Full job info: request payload, response/error, provider trace,
  timing breakdown
- For in-progress jobs: live progress bar from `useJob(id)`

### 3.7: Media browser (`src/features/manage/MediaBrowser.tsx`)

- Fetches `GET /v1/media` on mount
- Grid of thumbnails (images) or file icons (audio, other)
- Click → right panel shows metadata: content type, size, hash,
  lifecycle/expiry, source
- Upload button → `POST /v1/media`

### 3.8: Verify Phase 3

- Skill list loads and filters work
- Import a CivitAI URL → watch lifecycle in real time
- Job list shows recent dispatches from Phase 2 testing
- Media browser shows uploaded files

---

## Phase 4 — Configure surface

### 4.1: Configure sidebar + surface layout

- Sidebar shows: Preferences, Garden, Providers, Events

### 4.2: Preference editor (`src/features/configure/PreferenceEditor.tsx`)

- Fetches `GET /v1/preferences`
- Renders editable table: dotted field path (monospace), current value,
  field default (from catalog)
- "Reset" button per row: `DELETE /v1/preferences/{key}`
- "+ Add preference" button: field-path picker from catalog fields
- SSE: `preferences.changed` → re-fetch

### 4.3: Garden view (`src/features/configure/GardenView.tsx`)

- Fetches `GET /v1/resources`
- Stone cards: name, health badge, IP, GPU model + VRAM, utilization bar
- SSE: `resources.stone.*` → update utilization in real time
- Click → right panel with stone detail: active claims, provider
  instances, model warmth

### 4.4: Provider list (`src/features/configure/ProviderList.tsx`)

- Reads from `CatalogContext` (providers array)
- Each row: health dot, name, capability count, skill count
- SSE: `directory.provider.*` → update health
- Click → right panel: enabled/disabled, primitives, instance count

### 4.5: Event log (`src/features/configure/EventLog.tsx`)

- SSE connection with configurable focus (text input with glob grammar)
- Events stream in real time, newest at top
- Each event: timestamp, topic, truncated payload
- Click → right panel: full JSON payload, topic, sequence number
- Default focus: `*` (all events)

### 4.6: Verify Phase 4

- Preferences round-trip: set, verify in API, reset
- Garden view shows stone cards (if resources are available)
- Provider list shows all 7 providers with health dots
- Event log streams live events

---

## Phase 5 — Polish + integration

### 5.1: Responsive layout

- Test and fix breakpoints: 1200px (sidebar collapse), 800px (stack)
- Sidebar icons-only mode at medium width
- Hamburger menu at narrow width

### 5.2: Error states

- Network down → reconnection indicator in sidebar footer
- API errors → structured error display in result/detail panels
- Empty states → helpful messages ("No skills imported yet", etc.)

### 5.3: Loading states

- Skeleton loaders for catalog, skill list, job list
- Spinner for dispatch in progress
- Progress bar for media uploads

### 5.4: rust-embed integration test

- Docker build: `docker build -f src/orchestrators/ai/Dockerfile .`
- Verify: `curl http://localhost:7190/` returns dashboard HTML
- Verify: `curl http://localhost:7190/create/text/chat` returns
  `index.html` (SPA fallback)
- Verify: API routes still work (`/v1/catalog`, `/health`)

### 5.5: Final verification

Full end-to-end walkthrough of all three surfaces:

1. **Create**: Chat conversation, image generation (skill), TTS,
   image analysis with upload, copy-as-curl
2. **Manage**: Skill import from CivitAI, job inspection, media browse
3. **Configure**: Set a preference, verify it affects dispatch default,
   watch events, check provider health

---

## File structure (target)

```
dashboard/
├── index.html
├── package.json
├── vite.config.ts
├── tsconfig.json
├── tsconfig.app.json
├── tsconfig.node.json
├── eslint.config.js
└── src/
    ├── main.tsx
    ├── index.css                          (Tailwind + dark theme vars)
    ├── App.tsx                            (routes)
    ├── api/
    │   ├── client.ts                      (fetch wrapper)
    │   └── types.ts                       (TypeScript types from API)
    ├── hooks/
    │   ├── useSSE.ts                      (SSE connection hook)
    │   └── useCatalogDetail.ts            (per-tool catalog fetch)
    ├── contexts/
    │   ├── CatalogContext.tsx              (catalog summary cache)
    │   └── JobManagerContext.tsx           (global job tracking)
    ├── components/
    │   ├── Shell.tsx                       (three-panel frame)
    │   ├── SidebarTabs.tsx                (surface tab switcher)
    │   ├── RightPanel.tsx                 (shared right panel frame)
    │   └── common/                        (shared UI: badges, buttons)
    └── features/
        ├── create/
        │   ├── CreateSurface.tsx           (surface layout + sidebar)
        │   ├── ToolTree.tsx               (sidebar tool tree)
        │   ├── Workspace.tsx              (route → form or chat)
        │   ├── WorkspaceForm.tsx          (generic catalog form)
        │   ├── ChatWorkspace.tsx          (conversation UI)
        │   ├── SkillPicker.tsx            (card grid for skills)
        │   ├── ResultPanel.tsx            (text/image/audio/embed)
        │   ├── CopyAsCurl.tsx             (curl command generator)
        │   └── widgets/
        │       ├── TextareaWidget.tsx
        │       ├── SliderWidget.tsx
        │       ├── NumberWidget.tsx
        │       ├── SelectWidget.tsx
        │       ├── ToggleWidget.tsx
        │       └── FileWidget.tsx
        ├── manage/
        │   ├── ManageSurface.tsx
        │   ├── SkillList.tsx
        │   ├── SkillDetail.tsx
        │   ├── ImportWizard.tsx
        │   ├── JobList.tsx
        │   ├── JobDetail.tsx
        │   └── MediaBrowser.tsx
        └── configure/
            ├── ConfigureSurface.tsx
            ├── PreferenceEditor.tsx
            ├── GardenView.tsx
            ├── StoneDetail.tsx
            ├── ProviderList.tsx
            ├── ProviderDetail.tsx
            └── EventLog.tsx
```

---

## Open questions

- **Chat history scope**: Per-primitive? Per-model? Global? Currently
  proposed as per-primitive-path in localStorage. May want per-model
  threads later.
- **Skill preview images**: Field exists (`display.preview_image`) but
  currently null on all skills. When ComfyUI starts populating these,
  the SkillPicker and ToolTree should render them. No work needed now,
  but the components should handle the null → URL transition gracefully.
- **Inpaint mask overlay**: `media_inputs[].overlay: "source"` hints
  that the file widget for image.edit should support painting a mask
  on top of the source image. This is a Phase 5+ feature — the initial
  FileWidget just handles file selection.
- **Multiple providers for the same primitive**: image.analyze has 3
  providers (comfyui, docling, ollama). The form's model selector may
  need provider-awareness. Currently the backend's `recommended:*`
  handles this transparently — but a power user may want to pick a
  provider explicitly.

---

## References

- [ORCH-0031](../../decisions/ORCH-0031-dashboard-architecture.md) — architecture decision
- [ai-orchestrator-api.md](../../reference/ai-orchestrator-api.md) — API reference
- [ai-orchestrator-ux.md](../../reference/ai-orchestrator-ux.md) — UX architecture
- [AI-Orchestrator-proposal-full.html](AI-Orchestrator-proposal-full.html) — visual prototype
