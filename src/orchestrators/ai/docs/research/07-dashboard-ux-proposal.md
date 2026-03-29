# Dashboard UX Proposal

> Design document for the AI orchestrator dashboard. Reviewed before
> implementation. The dashboard serves two concerns — AI capability
> routing (primary) and service management (supporting).

---

## Design Principles

1. **Routing is the star.** The orchestrator's job is to route capability
   requests to the best model/instance. The dashboard makes routing
   decisions visible, measurable, and configurable.

2. **Capability-first, offering-agnostic.** The user thinks "Embed this
   text" not "Use Infinity." Models from different offerings appear in
   the same ranking table. The offering is metadata, not the organizer.

3. **Depth where it matters.** Every capability gets benchmarking, fitness
   scoring, demand tracking, and recommendation. Offering-specific
   operations (model pull, voice config) are accessible in context, not
   in a separate silo.

4. **Real-time, not polling.** Full snapshot on page load. SSE stream for
   incremental updates. No refresh button needed.

5. **Dark-first, information-dense.** Grafana-inspired. Operators want
   density, not whitespace. Every pixel earns its place.

---

## Navigation

```
┌─────────────────────────────────────────────────────┐
│  🪨 Zen Garden AI Orchestrator          [⚙ Settings]│
├─────────┬───────────────────────────────────────────┤
│         │                                           │
│ Overview│  (main content area)                      │
│         │                                           │
│ ── AI ──│                                           │
│ Chat    │                                           │
│ Embed   │                                           │
│ Vision  │                                           │
│ Tools   │                                           │
│ Think   │                                           │
│ Speak   │                                           │
│ Transcr.│                                           │
│ Imagine │                                           │
│ Translate                                           │
│ Rerank  │                                           │
│         │                                           │
│ ── Infra│                                           │
│ Stones  │                                           │
│         │                                           │
└─────────┴───────────────────────────────────────────┘
```

Left sidebar: fixed navigation. ALL capabilities listed under "AI"
regardless of status — active ones have a colored indicator, inactive
ones are dimmed with a subtle "enable" affordance. The operator should
always see the full potential of the garden.

"Infra" section for hardware/operational views.

---

## Page: Overview (`/`)

The landing page answers: "What can my garden do right now — and what
could it do if I take action?"

Three information bands:

### Band 1: Capability Grid (dominant, top 60%)

Card grid — **ALL capabilities**, not just active ones. Each card has
one of four states:

**Active (green border)** — serving requests:
```
┌─ Chat ──────────────────────────── ● ───┐
│ Recommended: qwen3.5:9b                 │
│ Fitness: ██████████░░ Fast (47 tok/s)   │
│ Serving: Ollama (1 stone, 22 models)    │
│          + Anthropic cloud (fallback)   │
│ Traffic: ▄▆█▇▅▃▂▁ 142 req / 5m         │
└─────────────────────────────────────────┘
```

**Available but no models (yellow border)** — service installed, needs
model to serve this capability:
```
┌─ Embed ─────────────────────── needs setup┐
│ Ollama is installed but has no embedding  │
│ models. Suggested:                        │
│   nomic-embed-text (274MB, 768 dims)     │
│   all-minilm (46MB, 384 dims, fastest)   │
│                                           │
│ Infinity is installed with 1 model.       │
│                                           │
│ [Pull nomic-embed-text] [Go to Embed →]  │
└───────────────────────────────────────────┘
```

**Service not installed (gray border)** — capability available if
the operator installs a service:
```
┌─ Imagine ───────────────── not installed ─┐
│ No image generation service detected.     │
│                                           │
│ Install ComfyUI to enable Imagine.        │
│ Requires: GPU with 8GB+ VRAM             │
│                                           │
│ [How to install →]                        │
└───────────────────────────────────────────┘
```

**Degraded (red border)** — was working, now broken:
```
┌─ Translate ──────────────────── ◐ ────────┐
│ LibreTranslate is loading language models │
│ First-run download in progress...         │
│                                           │
│ Last seen: 2m ago                         │
│ Expected: ready in ~5 minutes             │
└───────────────────────────────────────────┘
```

The grid always shows ALL 13 capabilities (or a curated subset of
the most relevant). This means the operator immediately sees the
full potential and knows exactly what action to take for each.

**Proactive suggestions** are the key differentiator. The dashboard
doesn't just report — it recommends. When Ollama is installed with
chat models but no embedding model, the Embed card says so and
offers a one-click pull. When no TTS service exists, the Speak card
explains what to install.

Clicking any card navigates to the capability detail page (even for
inactive capabilities — the detail page shows how to enable them).

### Band 2: Stone VRAM (middle, 20%)

Horizontal gauge bars per stone. Shows cross-offering VRAM allocation:

```
stone-azure-pool (RX 7900 XTX, 24GB)
[████████████░░░░░░░░░░░░] 11.2 / 24.0 GB
 ■ Ollama 8.4GB  ■ ComfyUI 2.8GB  □ Free 12.8GB
```

Clicking a stone navigates to the stone detail page.

### Band 3: Activity Feed (bottom, 20%)

Live SSE-driven event stream. Recent events with timestamps:

```
07:31:55  ✓ infinity profiled: 1 model (all-MiniLM-L6-v2)
07:31:54  ✓ openedai-speech profiled: 2 models (tts-1, tts-1-hd)
07:31:53  ✓ ollama profiled: 22 models on stone-azure-pool
07:31:52  → gateway registered: gw-ollama (ttl=60s)
07:31:52  ● discovery: tending to local Moss
```

---

## Page: Capability Detail (`/capability/:name`)

The management interface for a capability. Adapts to the capability's
current state:

**If not enabled**: shows what's needed to enable it. Which services
could provide it, what models to install, hardware requirements. One-
click actions where possible (e.g., "Pull nomic-embed-text to Ollama").

**If enabled**: three sections (below).

When enabled, three sections:

### Section 1: Model Ranking Table

All models that can serve this capability, across all offerings.
Sorted by recommendation score (4-layer: availability, fitness,
context, quality).

```
┌──────────────────────────────────────────────────────────────────┐
│  Embed Models                              [📌 Pin] [▶ Benchmark]│
├──┬──────────────────────┬──────────┬────────┬───────┬───────────┤
│  │ Model                │ Offering │ Fitness│ Dims  │ Throughput│
├──┼──────────────────────┼──────────┼────────┼───────┼───────────┤
│⭐│ all-MiniLM-L6-v2     │ Infinity │ Fast   │ 384   │ 3012/s    │
│  │ qwen3-embedding:8b   │ Ollama   │ —      │ —     │ —         │
│  │ bge-m3               │ Ollama   │ —      │ —     │ —         │
│  │ mxbai-embed-large    │ Ollama   │ —      │ —     │ —         │
│  │ nomic-embed-text     │ Ollama   │ —      │ —     │ —         │
│  │ all-minilm           │ Ollama   │ —      │ —     │ —         │
│  │ text-embedding-3-sm  │ OpenAI ☁ │ —      │ 1536  │ —         │
└──┴──────────────────────┴──────────┴────────┴───────┴───────────┘
  ⭐ = recommended   ☁ = cloud (priority -10)   📌 = pinned
```

**Pin control:** Click 📌 on any model to override the recommendation.
A pinned model always wins routing (with warning if unhealthy).

**Benchmark button:** Triggers fitness benchmarking for ALL models in
this table, across all offerings. Each offering's adapter provides its
own benchmark payload. Results fill the Fitness column.

The fitness column uses the verdict system:
- **Fast** (green): meets latency/throughput thresholds
- **Degraded** (yellow): functional but slow
- **Vetoed** (red): too slow for production use
- **Blocked** (gray): errors during benchmark

### Section 2: Routing & Demand

Two panels side by side:

**Left — Routing Configuration:**
- Current recommendation + rationale (why this model was chosen)
- Priority gate status: "Local instances available — cloud excluded"
  or "No local instances — routing to cloud fallback"
- Per-model routing stats (requests served, avg latency)

**Right — Demand Curve:**
- 3-window decay visualization (reactive 15m / tactical 6h / strategic 3d)
- Shows demand trend for this capability
- Per-model demand breakdown

### Section 3: Offering Operations (contextual)

Collapsible panels per offering that serves this capability.
Only shows offerings relevant to THIS capability.

**For Ollama (when serving Chat/Embed/Vision/etc.):**
```
┌─ Ollama ──────────────────────────────────────────┐
│ Instances: stone-azure-pool (RX 7900 XTX, 24GB)  │
│ Loaded: qwen3.5:9b (4.2GB), nomic-embed-text     │
│ VRAM: ████████░░░░ 8.4 / 24.0 GB                 │
│                                                    │
│ [Pull Model] [Refresh] [Load/Unload]              │
│                                                    │
│ Models on this instance:                           │
│   qwen3.5:9b       9.7B Q4_K_M  ✓ loaded (4.2GB)│
│   llama3.1:8b      8.0B Q4_K_M  ○ available      │
│   deepseek-r1:8b   8.2B Q4_K_M  ○ available      │
│   ...                                              │
└────────────────────────────────────────────────────┘
```

**For Infinity (when serving Embed):**
```
┌─ Infinity ────────────────────────────────────────┐
│ Instance: stone-azure-pool:7997                    │
│ Engine: torch                                      │
│ Model: sentence-transformers/all-MiniLM-L6-v2     │
│ Status: healthy                                    │
└────────────────────────────────────────────────────┘
```

**For OpenedAI Speech (when serving Speak):**
```
┌─ OpenedAI Speech ─────────────────────────────────┐
│ Instance: stone-azure-pool:8001                    │
│ Engine: Piper (CPU)                                │
│ Voices: alloy, echo, fable, onyx, nova, shimmer   │
│ Format: wav, mp3, opus, flac                       │
│                                                    │
│ [▶ Test Voice]  Voice: [alloy ▼]                  │
│   "The quick brown fox jumps over the lazy dog"    │
│   [ Generate & Play ]                              │
└────────────────────────────────────────────────────┘
```

**For Cloud Provider (when serving as fallback):**
```
┌─ Anthropic (cloud, priority -10) ─────────────────┐
│ Models: claude-sonnet-4, claude-haiku-4            │
│ API Key: ****...k7Qm (valid)                      │
│ Status: healthy (last check 30s ago)               │
│ Usage: 1,240 requests this month                   │
│                                                    │
│ [Edit Key] [Disable] [Set Priority]               │
└────────────────────────────────────────────────────┘
```

The key: these panels are **contextual to the capability**. The Ollama
panel on the Chat page shows only chat-capable models. The Ollama panel
on the Embed page shows only embedding models. Same offering, different
context.

---

## Page: Stones (`/stones`)

Hardware-centric view. One card per stone:

```
┌─ stone-azure-pool ────────────────────────────────┐
│ 12th Gen i7-12700KF · AMD RX 7900 XTX · 64GB RAM │
│                                                    │
│ VRAM: [████████████░░░░░░░░░░░░] 11.2 / 24.0 GB  │
│   Ollama:         8.4 GB (3 models loaded)         │
│   ComfyUI:        2.8 GB (1 checkpoint)            │
│   Free:          12.8 GB                           │
│                                                    │
│ Offerings:                                         │
│   ✓ Ollama        22 models   :11434   healthy     │
│   ✓ Infinity       1 model    :7997    healthy     │
│   ✓ OpenedAI Sp.   2 voices   :8001    healthy     │
│   ✓ whisper.cpp    1 model    :8000    healthy     │
│   ◐ LibreTranslate loading    :5000    starting    │
│                                                    │
│ Queue: 0 active requests                           │
│ Uptime: 4d 12h                                     │
└────────────────────────────────────────────────────┘
```

---

## Page: Settings (`/settings`)

### General
- Auto-pull mode: Off / Sync / On-Demand
- Delete idle models: toggle
- Metrics collection: toggle

### Cloud Providers
Table of configured providers with add/edit/remove:

```
┌──────────┬──────────┬───────────┬──────────┬─────────┐
│ Provider │ Status   │ Models    │ Priority │ Actions │
├──────────┼──────────┼───────────┼──────────┼─────────┤
│ Anthropic│ ✓ valid  │ 4 models  │ -10      │ [Edit]  │
│ OpenAI   │ ✓ valid  │ 12 models │ -10      │ [Edit]  │
│ Cohere   │ ✗ expired│ 0         │ -10      │ [Edit]  │
└──────────┴──────────┴───────────┴──────────┴─────────┘
[+ Add Provider]
```

### Proxy Ports
Toggle per offering — enable/disable proxy listeners:

```
Ollama         :21434  [████ ON ]
Infinity       :21438  [████ ON ]
OpenedAI Speech:21437  [████ ON ]
LibreTranslate :21439  [░░░░ OFF]
whisper.cpp    :21436  [░░░░ OFF]
ComfyUI        :21435  [░░░░ OFF]
```

---

## Data Flow

```
Page Load:
  GET /api/status → full snapshot (all capabilities, instances, models,
                     fitness, demand, config, stones)
  → Render everything from snapshot

After load:
  GET /api/events (SSE) → incremental updates:
    registry.updated    → refresh instance/model tables
    config.updated      → refresh settings/pins
    job.created/done    → update activity feed + job status
    benchmark.sample    → update fitness cell in real-time
    tending.changed     → update stone connection status
```

No periodic polling. Dashboard is event-driven.

---

## Benchmark UX Flow

1. Operator navigates to `/capability/chat`
2. Sees model ranking table with empty Fitness columns
3. Clicks **[▶ Benchmark]**
4. Dialog: "Benchmark all Chat models across all instances?"
   - Scope: [All stones ▼] or specific stone
   - Options: [✓] Sync missing models before benchmarking
5. Benchmark starts. Progress appears inline:
   - Each model row shows a progress indicator
   - Samples stream in via SSE
   - Fitness cells update as verdicts are computed
6. When complete, the recommendation may change (a new model
   might score higher)

The benchmark button on the Embed page benchmarks embedding models.
On the Speak page, it benchmarks TTS latency. Each capability has
its own benchmark strategy (defined by the offering adapter's
`benchmark()` method), but the UX is identical: click button, watch
progress, see results.

---

## Responsive Behavior

The dashboard is designed for desktop/laptop screens (operators
monitoring infrastructure). Minimum width: 1024px. No mobile
optimization — this is an operations tool, not a consumer app.

At narrow widths: sidebar collapses to icons. At wide widths:
capability cards arrange in a 2-3 column grid.

---

## Color Language

| Color | Meaning |
|-------|---------|
| Green (#22c55e) | Healthy, Fast, available |
| Yellow (#eab308) | Degraded, warning |
| Red (#ef4444) | Down, Vetoed, error |
| Gray (#6b7280) | Dormant, not benchmarked, Blocked |
| Blue (#3b82f6) | Active, in-progress, selected |
| Purple (#8b5cf6) | Cloud provider |

---

## Technology

- React 19 + TypeScript
- Tailwind CSS (dark mode default)
- Vite for build
- Embedded in Rust binary via `rust-embed` or `include_dir!`
- Single SSE connection (`EventSource`)
- Client-side routing (React Router)
- No external dependencies beyond npm packages
