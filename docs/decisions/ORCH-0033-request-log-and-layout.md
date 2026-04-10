---
audience: developer
doc_type: decision
status: proposed
---

# ORCH-0033: Persistent Request Log + Adaptive Dashboard Layout

**Date**: 2026-04-10
**Status**: Proposed
**Deciders**: Leo
**Amends**: ORCH-0031 (dashboard architecture), ORCH-0032 (sidebar + breadcrumb)
**Related ADRs**:
- ORCH-0028 (orchestrator core) — defines the dispatch pipeline this ADR extends
- ORCH-0030 (architecture realignment) — defines the job store pattern this ADR parallels

This ADR introduces two changes that evolved from iterative design
discussion:

1. **Persistent request log** — the existing ephemeral
   `OrchestratorRequest` gains a persisted lifecycle, turning every
   user interaction into a durable, browsable, forkable record.
2. **Adaptive dashboard layout** — the three-panel layout from
   ORCH-0031 is replaced with an icon sidebar, an adaptive center
   area, and a collapsible overview panel.

---

## Part 1: Persistent Request Log

### Context

The `OrchestratorRequest` (defined in `domain/request.rs:160`) carries
rich data through the dispatch pipeline:

| Field | Contains |
|-------|----------|
| `id` (RequestId) | GUIDv7 |
| `correlation_id` | Caller trace ID |
| `received_at` | Timestamp |
| `action` | Primitive + optional skill |
| `payload` | Full input JSON |
| `selectors` | Provider, model, skill, variant |
| `resolved_provider` | Provider chosen by contextualizer |
| `media.referenced` | Media IDs the user provided |
| `context.job_sink.job_id()` | Linked Job ID |

All of this is **ephemeral** — the object dies when the HTTP handler
returns. The Job persists but only carries `action`, `correlation_id`,
`state`, and `result`. The input payload, selectors, resolved provider,
and media references are lost.

This is a separation-of-concerns problem. The Job tracks **operational
lifecycle** (queued → running → done → GC). The user's interaction —
what they asked, what they got back, which media was involved — is a
different concern with a different lifecycle: long-lived, user-controlled
retention, browsable, forkable.

### Decision

#### The Request entity

Persist the `OrchestratorRequest` at two moments in the dispatch
pipeline: pre-dispatch (input snapshot) and post-dispatch (output +
metadata). The existing `RequestId` (already a GUIDv7) becomes the
durable identifier.

```rust
pub struct PersistedRequest {
    // ── Identity ──
    pub id:            RequestId,       // GUIDv7, already generated
    pub correlation_id: CorrelationId,
    pub created_at:    DateTime<Utc>,
    pub completed_at:  Option<DateTime<Utc>>,

    // ── Lineage ──
    pub parent_id:     Option<RequestId>,  // fork source

    // ── Intent ──
    pub action:        String,           // "text.chat" or "image.generate.animij-36771"
    pub status:        RequestStatus,    // running → success | failure
    pub input:         Value,            // full payload as sent by the user
    pub selectors:     SelectorsSnapshot, // provider, model, variant

    // ── Output ──
    pub output:        Option<RequestOutput>,
    pub error:         Option<ErrorSnapshot>,

    // ── Media ──
    pub media_inputs:  Vec<RequestMedia>,  // media the user provided
    pub media_outputs: Vec<RequestMedia>,  // media the orchestrator produced

    // ── Resolution metadata ──
    pub meta:          RequestMeta,

    // ── Retention ──
    pub pinned:        bool,

    // ── Job link (operational, not user-facing) ──
    pub job_id:        Option<JobId>,
}

pub enum RequestStatus {
    Running,
    Success,
    Failure,
}

pub struct RequestOutput {
    /// Full output payload. For text responses this IS the content.
    /// For media responses this contains metadata alongside media_id
    /// references.
    pub payload: Value,
}

pub struct ErrorSnapshot {
    pub code:    String,
    pub message: String,
    pub details: Option<Value>,
}

pub struct RequestMedia {
    pub media_id:     MediaId,
    pub field:        String,        // "image.source", "image.data", "audio.data"
    pub content_type: String,        // "image/png", "audio/wav"
}

pub struct SelectorsSnapshot {
    pub provider: Option<String>,
    pub model:    Option<String>,
    pub variant:  Option<String>,
}

pub struct RequestMeta {
    pub provider:   Option<String>,   // resolved provider (may differ from requested)
    pub model:      Option<String>,   // resolved model
    pub stone:      Option<String>,   // stone that served the request
    pub latency_ms: Option<u64>,
    pub tokens:     Option<TokenUsage>,
}

pub struct TokenUsage {
    pub input:  Option<u64>,
    pub output: Option<u64>,
}
```

#### Persistence point in the dispatch pipeline

The `RequestStore` is called at two points in `dispatcher.rs`:

```
dispatcher.dispatch():
  1. Create Job
  2. Build OrchestratorRequest
  3. Contextualizer (payload normalized, provider resolved)
  4. Media resolver (media refs resolved)
  ──── request_store.create(request) ────
       Status: Running. Input + selectors + media_inputs captured.
  5. Provider onboard (execution)
  6. Handle outcome (result or error)
  ──── request_store.complete(id, output, media_outputs, meta) ────
       Status: Success or Failure. Output + meta captured.
```

The request record exists from the moment the user hits Send (visible
in history as "running") and is completed when the provider returns.

#### Storage

`{data_dir}/requests/{request_id}.json` — one file per request,
matching the job store pattern. The `RequestStore` mirrors `JobStore`:
in-memory index for listing, file-backed for persistence.

#### Media preservation chain

When a request is **pinned**, all media referenced by
`media_inputs` and `media_outputs` are exempt from the media reaper's
TTL sweep. The chain:

```
pinned request → media_inputs[].media_id  → preserved
               → media_outputs[].media_id → preserved
```

Unpinning a request releases the media back to normal TTL rules.

#### Lineage

A request created via the fork workflow carries `parent_id` pointing
to the source request. The lineage is a singly-linked list (each node
points to its parent). Children are discovered by query:
`GET /v1/requests?parent_id={id}`.

The dashboard shows bidirectional lineage on any request:

**Ancestors** (up to 3 visible, expand for more):
```
019d7458 "a cat" (10:28)
  └─ 019d7460 "a cat in a garden" (10:32)
      └─ [this request]
```

**Descendants** (up to 3 visible, expand for more):
```
[this request]
  ├─ 019d7465 "...watercolor" (10:35)
  ├─ 019d7468 "...oil painting" (10:38)
  └─ + 2 more...
```

#### API surface

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/v1/requests` | List requests (filter: action, status, pinned, parent_id, before, after, limit) |
| GET | `/v1/requests/{id}` | Full request record |
| PATCH | `/v1/requests/{id}/pin` | Toggle pinned status |
| DELETE | `/v1/requests` | Flush unpinned requests older than threshold |
| GET | `/v1/requests/{id}/lineage` | Ancestor chain (walk parent_id) |

#### Flush semantics

`DELETE /v1/requests?before={iso_timestamp}` deletes all requests where:
- `pinned = false`
- `created_at < before`
- Media referenced only by deleted requests is released to normal TTL

Pinned requests and their media survive indefinitely.

---

## Part 2: Adaptive Dashboard Layout

### Context

ORCH-0031 specified a fixed three-panel layout (220px sidebar, flex
center, 340px right). ORCH-0032 refined the sidebar to a unified
navigation tree. During testing, two problems emerged:

1. The right panel fights itself — it wants to be both a persistent
   status view and the primary result display. A generated image
   deserves full width, not a 340px strip.
2. The 220px sidebar is wider than needed for a shallow navigation
   tree with only group headers and leaves.

### Decision

#### Icon sidebar (~52px)

The sidebar narrows to icon-only with tooltips on hover:

```
┌──────┐
│  ✦   │  ← Logo
├──────┤
│  +   │  ← CREATE (group)
│  💬  │  ← Text
│  🖼️  │  ← Image
│  🔊  │  ← Audio
│      │
│  ☰   │  ← MANAGE
│  ⚙   │  ← CONFIGURE
├──────┤
│  ●   │  ← Connection status
└──────┘
```

Each icon is a link. The active item has an accent-colored indicator
(left border or background). Tooltips show the full label on hover.
Manage and Configure sub-items (Skills, Jobs, Preferences, etc.)
appear as a tooltip flyout or are handled in the breadcrumb.

#### Adaptive center area

The center area between the sidebar and the overview panel adapts
based on what the user is doing:

| Mode | When | Layout |
|------|------|--------|
| **Directory** | `/create`, `/manage`, `/configure` | Full-width cards, discovery, overview |
| **Workspace** | `/create/text/chat` | Proportional split — form left, result right |
| **Focus** | User expands result | Result fills center (form accessible via toggle) |

For workspace mode, the split ratio adapts to content type:
- Text chat: 50/50 or 60/40
- Image generation: result panel is wider to show images at a decent
  size
- Audio: result panel is minimal (just a player)

The split is not a fixed pixel width. It's a flex ratio that
the content determines.

#### Collapsible overview panel

A right-side panel that shows persistent operational context:

```
┌──────────────────┐
│ OVERVIEW         │
│                  │
│ Primitives    9  │
│ Skills       20  │
│ Providers     7  │
│                  │
│ ── Providers ──  │
│ ● ollama         │
│ ● comfyui        │
│ ● kokoro         │
│ ...              │
│                  │
│ ── Last Req ──   │
│ { json... }      │
│                  │
│ ── History ───   │
│ 10:32 text.chat  │
│   "What is..."   │
│ 10:28 img.gen    │
│   animij-36771   │
│ 10:15 txt.trans  │
│   "Good morning" │
│ ...              │
└──────────────────┘
```

Sections:
- **Status**: primitive/skill/provider counts, provider health dots
- **Last request/response**: expandable JSON of the most recent
  interaction
- **History**: timestamped request list with action summary. Each
  entry is clickable → navigates to the workspace with the request
  loaded (`?from={id}` for fork, `?r={id}` for view).
- **Pin toggle**: visible on each history entry and on the request
  detail view.
- **Lineage**: when viewing a request with `parent_id` or children,
  the lineage tree appears (up to 3 ancestors, up to 3 descendants,
  expand for more).

Default state: open on wide screens (>1400px), collapsed on narrower.
Toggle button on the panel edge.

#### URL scheme

```
/create/text/chat                → fresh workspace
/create/text/chat?from=019d7460  → fork: pre-fill form from request, editable
/create/text/chat?r=019d7460     → view: show request + result, read-only until edited

/compare?a=019d7460&b=019d7461   → side-by-side comparison (future)
```

The `from` parameter signals a fork: the form is populated from the
parent request's input, but it's a new interaction. On submit, the
new request carries `parent_id = 019d7460`.

The `r` parameter signals a view: the form and result are populated
from the stored request. Editing any field transitions to fork mode
(the URL updates to `?from=` and the submit creates a child request).

#### Example pre-fill

The vocabulary carries `examples.minimal` and `examples.full` for
every primitive. When the workspace loads with no `from` or `r`
parameter, an "Example" button appears if the vocabulary has examples.
Clicking it pre-fills the form from `examples.minimal`. This is the
zero-documentation onboarding path.

The button only renders when example data exists. No client-side
knowledge of which primitives have examples — the catalog data
determines it.

---

## Consequences

### Positive

- **Every interaction is durable.** The user never loses work. Prompts,
  parameters, results, and media are all preserved and browsable.
- **Fork workflow enables creative iteration.** Generate → tweak →
  regenerate with full lineage tracking. The most-forked requests
  surface naturally as the user's productive seed prompts.
- **Media lifecycle is coherent.** Pinning a request pins its media.
  Flushing unpinned requests flushes orphaned media. No manual media
  management needed.
- **Layout serves the content.** A 1-pixel-wide image generation
  preview in a cramped right panel is replaced by a proportional
  split that respects the output type.
- **Separation of concerns.** Jobs track operational lifecycle (queued,
  running, progress, cancellation). Requests track user interactions
  (what was asked, what was returned, lineage, bookmarks). Each
  concept has its own lifecycle and retention rules.

### Negative

- **Storage growth.** Every dispatch creates a persistent record. For
  heavy users, this could accumulate. Mitigated by flush with
  configurable retention, and pinning for intentional preservation.
- **Two persistence writes per dispatch.** The dispatcher now writes
  to both `JobStore` and `RequestStore`. Both are sequential file I/O
  and sub-millisecond — not a hot-path concern.
- **Wider sidebar loses labels.** Icon-only sidebar requires the user
  to learn icons or hover for tooltips. Mitigated by the small number
  of items (7 icons total) and the breadcrumb that always shows the
  current context in words.

### Risks

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Large payloads in request records (e.g. long chat histories) | Medium | Cap stored input at a reasonable size; truncate with a "full payload available via API" note |
| Media reaper complexity with pin cascading | Low | Simple check: if any pinned request references a media_id, skip it |
| Icon sidebar confuses new users | Low | Tooltips + breadcrumb provides full context; icons are standard (text=chat bubble, image=frame, audio=speaker) |

---

## Backend changes required

| ID | Change | Where | Effort |
|----|--------|-------|--------|
| R1 | `PersistedRequest` struct + `RequestStore` | New `domain/persisted_request.rs` + `services/request_store.rs` | Medium |
| R2 | Persist at dispatch time (pre + post) | `services/dispatcher.rs` | Small — two insertion points |
| R3 | HTTP endpoints (list, get, pin, flush, lineage) | New `http/requests.rs` + router entry | Medium |
| R4 | Media reaper respects pinned request refs | `services/media_store.rs` reaper logic | Small |
| R5 | Wire `RequestStore` into `AppState` | `app_state.rs` + `main.rs` | Small |

---

## References

- [ORCH-0031](ORCH-0031-dashboard-architecture.md) — parent dashboard ADR
- [ORCH-0032](ORCH-0032-sidebar-and-breadcrumb-navigation.md) — sidebar + breadcrumb
- [ORCH-0028](ORCH-0028-orchestrator-core.md) — dispatch pipeline
- `domain/request.rs:160` — existing `OrchestratorRequest` struct
- `services/dispatcher.rs:107` — dispatch pipeline where persistence is inserted
- `services/job_store.rs` — parallel pattern for file-backed persistence
