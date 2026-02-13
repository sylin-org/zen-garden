# Lantern Dashboard — Implementation Brief

> **Purpose:** This document provides everything needed to build the Lantern dashboard for Zen Garden. It captures months of design decisions, vocabulary, data models, visual language, interaction patterns, and architectural context. Six working React prototypes are included as reference implementations in the accompanying ZIP.
>
> **For:** An agentic coding assistant (Claude Code or similar) tasked with building Lantern as a real web application served by the `garden-lantern` Rust binary.

---

## 1. What Is Lantern?

Lantern is the dashboard service for Zen Garden, a distributed computing platform built on reclaimed hardware ("Stones"). Lantern aggregates presence data from all Stones in a garden and provides a visual dashboard for monitoring, managing, and understanding the topology.

**Lantern is just another Offering.** It runs on any Stone via `garden-rake plant lantern`. It's not architecturally special — when running, Stones report to it (hub-and-spoke). When it goes down, Stones resume peer-to-peer gossip (mDNS). This means Lantern is an optimization for visibility, not a dependency.

**Data flow:** Stones run the `garden-moss` daemon, which exposes SSE streams and REST APIs. Lantern connects to each Stone's Moss instance, aggregates the data, provides persistence for historical trends, and serves the dashboard.

---

## 2. Vocabulary

Every term in Zen Garden is intentional. The dashboard must use this vocabulary consistently.

| Term | Meaning | Dashboard Context |
|------|---------|-------------------|
| **Stone** | A physical device (reclaimed thin client, workstation, laptop, etc.) | Primary entity — nodes on the overview, cards in the garden view |
| **Garden** | The logical collection of all Stones | The whole system being observed |
| **Moss** | Daemon running on each Stone | Data source — Lantern pulls from Moss APIs |
| **Offering** | A curated service template (mongodb, redis, ollama, etc.) | Shown as deployable/deployed services |
| **Seed Bank** | A named storage volume attached to a Stone | First-class view, same identity/replica semantics as offerings |
| **Pond** | Security layer (trust circle) | Shown in Pond view — keystone, members |
| **Keystone** | The Stone holding the CA keypair | Highlighted in Pond view |
| **Lantern** | This dashboard service | Self-referential — "served by Lantern" |
| **Cricket** | Audio companion (ambient soundscapes) | Shown as companion in Stone detail |
| **Firefly** | LED matrix companion (visual status) | Shown as companion in Stone detail |
| **Rake** | CLI tool (`garden-rake`) | Referenced in guidance text |

### Health States (Vitality Language)

Never use "OK", "ERROR", "stopped". Use:

| State | Color | Meaning |
|-------|-------|---------|
| **Thriving** | Sage green `#84a59d` | Healthy, responsive |
| **Withering** | Clay `#d4a373` | Degraded, under pressure |
| **Resting** | Stone gray `#78716c` | Intentionally sleeping |
| **Dormant** | (dim) | Service not running |
| **Needs attention** | Clay/red | Something requires action |

### Service Identity Model

This is the core conceptual innovation. Understand it deeply:

```
offering + instanceName = identity

mongodb (unnamed)     → THE mongodb (communal, shared)
mongodb:analytics     → THE analytics mongodb (dedicated, distinct)
```

- **Unnamed instances** sharing the same offering on different Stones are the **same logical entity** — they form a **replica set** automatically.
- **Named instances** (with `instanceName`) are distinct identities — they exist independently even if the offering is the same.
- **Naming an unnamed instance** forks it from the replica set (data wipe, new identity).
- **Clearing a name** returns to communal mode (data wipe, joins unnamed set).

The identity key is: `svcKey = instanceName ? "${offering}:${instanceName}" : offering`

This applies identically to **Seed Banks**: same-named seed banks across stones enter replica mode automatically.

---

## 3. Architecture

### Backend (Rust — `garden-lantern` binary)

```
garden-lantern
├── SSE aggregator — connects to each Stone's Moss SSE endpoint
├── REST API — serves aggregated data to the dashboard
│   ├── GET /api/v1/garden/stones — all stones with resources, services, health
│   ├── GET /api/v1/garden/stones/:id — single stone detail
│   ├── GET /api/v1/garden/offerings — catalog + deployment state
│   ├── GET /api/v1/garden/seeds — seed bank topology
│   ├── GET /api/v1/garden/pond — trust circle members
│   ├── GET /api/v1/garden/activity — event stream
│   ├── GET /api/v1/garden/presence/stream — SSE endpoint for real-time updates
│   ├── POST /api/v1/garden/stones/:id/services/:svc/rest — rest a service
│   ├── POST /api/v1/garden/stones/:id/services/:svc/wake — wake a service
│   └── POST /api/v1/garden/stones/:id/services/:svc/name — set instance name
├── Static file server — serves the dashboard SPA
└── Persistence — historical data, trends (SQLite or similar)
```

### Frontend (SPA served by Lantern)

The dashboard is a single-page application. Technology choice is flexible — React is used in the prototypes but the implementation could use any framework. The key requirement is that it connects to the Lantern REST API and SSE stream.

---

## 4. Views

### 4.1 Overview (Default Landing Page)

**The crown jewel.** A full-canvas spatial topology visualization. Not a widget — the entire viewport is the canvas.

**Layout:** SVG canvas (or canvas/WebGL for larger gardens). Stones are positioned in a pleasing spatial arrangement. For small gardens (2-8 stones), manual layout works. For larger gardens, consider force-directed layout with pan/zoom.

**Stone Nodes (circular, ~56px radius):**
- **Outer health arcs:** 3 segments at 120° each (CPU / MEM / DSK) with 5° gaps between them
  - Track outline shows the full arc in subtle gray
  - Fill is proportional to the resource percentage
  - Color transitions: sage (healthy) → clay (pressure) → red (>85%, danger)
  - Segments pulse/breathe at a rate driven by health state:
    - Thriving: slow, calm breathing (0.035 rate)
    - Withering: faster, anxious breathing (0.07 rate)
    - Resting: static, dim (no animation)
- **Inner circle:** Stone's unique color as subtle radial gradient fill
- **Center LED:** Small breathing dot in health color with glow filter
- **Service dots:** One dot per service below the name (filled sage = running, hollow = stopped)
- **Stone name:** Centered text (strip "stone-" prefix for readability)
- **Hardware hint:** Below name in monospace (e.g., "4c · 8GB")
- **Color pip:** Small rectangle at top of circle in stone's unique color

**Edges (set-based topology):**
- Cubic bezier curves connecting stones that share set identities (same svcKey)
- If two stones share multiple sets, parallel curves with offset (bow parameter)
- Each edge has a midpoint label pill showing the set identity (e.g., "mongodb", "ollama")
- Edge color: sage with reduced opacity
- **Activity sparkles:** Luminous dots travel along edges via `animateMotion` (1.2s duration), spawned periodically (~3s). These visualize replication/sync activity.

**Interactions:**
- **Hover stone:** Everything else dims to ~18% opacity. Hovered stone's edges and connected stones remain visible. Arc labels appear (e.g., "CPU 23%", "MEM 62%", "DSK 41%").
- **Click stone (Bloom):**
  - Stone scales up to 1.1×
  - All other stones dim to ~18%
  - Stone's services radiate outward as **satellite circles** (~28px radius, positioned at equal angular intervals ~128px from center)
  - Dashed spokes connect satellites to parent stone
  - Each satellite shows: offering name, instance name (gold if named), status dot, replica peer count
- **Click satellite → Action Panel:**
  - HTML overlay positioned near the satellite
  - Shows: service description, image, port, status, stone reference, identity reference, capabilities (for ollama etc.), replica peer list (clickable to bloom that stone)
  - Actions: Rest/Wake, Config, Detail
- **Click background or re-click bloomed stone:** Collapse bloom, return to scan state

**Summary strip:** Fixed at bottom center. Shows stone count, online count, running service count, replica group count.

**Design goal:** Read garden health as a gestalt — like reading a face. A thriving garden is all sage-green circles with gentle breathing and occasional sparkles. A stressed garden shows clay/red segments with faster breathing. A resting stone sits quiet and dim at the periphery.

### 4.2 Garden (List View)

Alternative representation — stone cards in a grid.

**Summary bar:** Stone count, online, services, replica groups, attention count.

**Stone cards (grid, responsive):**
- Color bar (left edge, stone's unique color)
- Name, endpoint
- Health dot (breathing animation, health-tempo-driven)
- Resource mini-bars (CPU/MEM/DSK) with percentage labels and color-coded fills
- Service chips: monospace tags with status dot, offering name, instance name (gold), ⟐ badge if replicated
- Footer: hardware summary (cores/RAM/OS), tags (⚠ attention, ✦ opportunity, 🌱 seeds, 🔊 companions)

**Replica groups section:** Cards showing set identity, member count, sync status, and member list with stone color pips, ports, and status dots. Clickable to navigate to stone detail.

**Recent activity preview:** Last 5 events.

### 4.3 Stone Detail

Deep dive into a single stone. Accessed by clicking a stone card, clicking a stone in the sidebar, or navigating from Overview bloom.

**Header:** Back button (← Garden), color bar, stone name, health dot (breathing), endpoint + hardware + OS info.

**Resource gauges:** CPU/MEM/DSK with large percentage value, color-coded gauge fill bar. Uptime display. Network RX/TX. Pond role.

**Offerings section:**
Each service is a card with consistent row layout — **content on left, actions on right:**
- Status indicator (dot + label), name (with instance name in gold if named), replica badge
- Description, image, port
- **Right side:** Rest/Wake button, Config button
- Sub-sections (indented):
  - **Capabilities** (if applicable): tag badges (e.g., llama3.2, phi3, gemma2)
  - **Replica peers** (if replicated): clickable chips with stone color pip, name, port, status dot → clicking hops to that stone
  - **Instance naming row:** "Instance" label + current name (or "unnamed" italic), with **Name/Rename/Clear buttons on the right**
  - **Contextual warning text:** "Naming will wipe data and fork from this N-member replica set" / "Renaming or clearing will wipe data and migrate identity"

**Seed Banks section:**
Same card layout as offerings. Each seed bank shows:
- 🌱 icon, name, replica badge if replicated
- Filesystem, mountpoint
- Usage bar (used/total with percentage)
- Status (mounted/ejected)
- **Right side:** Eject, Release buttons
- Sub-sections: replica peers (clickable), identity row with Rename button
- Contextual hint about replication semantics

**Companions section:**
Status, name, detail (tune, mode), port. Commands button on right.

**Stone Administration:**
Right-aligned button row: Portrait ↗, Nourish, Reconcile, Stir (Reboot), Slumber/Rouse.

**Resting state:** When stone is resting, show centered message "This stone is slumbering" with service count and Rouse button. No resource gauges.

### 4.4 Offerings (Set-Centric Topology)

**Not** a per-stone view. This shows the offering catalog and deployment topology across the entire garden.

**Filter bar:** All + category filters (database, cache, storage, ai, monitoring, messaging, application).

**Catalog grid:** One card per offering from the catalog.
- Offering name, category label, description, image
- Green LED if deployed anywhere
- **If deployed:** Identity groups section showing all deployed identities:
  - Each identity (unnamed or named) gets its own sub-heading with replica badge if multi-instance
  - Member list: stone pip, name, port, status dot (all clickable to Stone Detail)
- **If not deployed:** "+ Deploy" button

### 4.5 Seed Banks (Set-Centric Topology)

Mirrors the Offerings view pattern for storage.

**Summary bar:** Total banks, stones with seeds, replica groups, distinct identities, unseeded stone count.

**Identity group cards:** One per distinct seed bank name.
- 🌱 icon, name, replica badge
- **Aggregate usage bar** spanning all members (total used / total capacity)
- Per-member rows: stone pip, name, filesystem, mountpoint, individual usage bar, mount status, Eject/Release buttons (right-aligned)
- Identity section with replication hint
- Rename button (right-aligned)

**Unseeded Stones section:** Lists stones without seed banks, with "Attach" button.

**How it works panel:** Reference grid explaining identity, replication, renaming, filesystem agnosticism, capacity adaptation, ejection behavior.

### 4.6 Activity

Real-time event stream.

**Stats row:** Total events, success count, warning count, active stones.

**Filter bar:** All, info, success, warning (with counts).

**Event list:** Grid rows with:
- Timestamp
- Stone color pip
- Stone name
- Event type with colored dot (sage=success, clay=warning, gray=info)
- Event detail

**Live updates:** New events slide in at the top with animation. SSE streaming indicator at bottom with breathing green dot.

### 4.7 Pond

Trust circle visualization.

**Stats:** Member count, online, keystones, key type (Ed25519).

**Member cards:** Stone pip, name, endpoint, role badge (keystone highlighted in gold), health dot.

**Security properties panel:** Grid showing authentication method, transport, discovery, keystone election, invitation flow, revocation behavior.

---

## 5. Navigation & Shell

### Sidebar (fixed, 200px)

```
┌──────────────────────┐
│ LANTERN              │  ← monospace, uppercase, subtle
│ Garden Name          │  ← larger, bold
│ ● 3/4 stones · pond │  ← breathing green pip + status
├──────────────────────┤
│ VIEWS                │
│  ◎ Overview          │  ← default landing page
│  ◉ Garden            │
│  ✦ Offerings         │
│  🌱 Seed Banks       │
│  ↯ Activity          │
│  🔒 Pond             │
├──────────────────────┤
│ STONES               │
│  ▪ crystal-forest  ● │  ← color pip + name + health dot
│  ▪ quiet-stream    ● │
│  ▪ amber-ridge   ⚠ ● │  ← attention warning
│  ▪ ivy-terrace     ◌ │  ← dim for resting
├──────────────────────┤
│ v0.1.0       ⏱ 2.3s │  ← version + last sync
└──────────────────────┘
```

- Clicking a stone name navigates to Stone Detail
- Active view has highlighted border-left + background
- Overview uses full viewport (no padding). Other views have standard padding.

### Cross-View Navigation

| From | Action | To |
|------|--------|----|
| Overview | Click stone node or satellite "Detail" | Stone Detail |
| Overview | Click edge or satellite identity | Offerings |
| Garden | Click stone card | Stone Detail |
| Garden | Click replica group | Offerings |
| Offerings | Click deployment instance | Stone Detail |
| Seed Banks | Click member row | Stone Detail |
| Stone Detail | Back button | Garden |
| Stone Detail | Click service name | Offerings |
| Stone Detail | Click replica peer chip | Stone Detail (other stone) |
| Any view | Sidebar stone | Stone Detail |
| Any view | Sidebar view | That view |

---

## 6. Design Tokens

### Colors

```css
--bg:   #1a1a1a     /* Main background */
--bg2:  #222220     /* Sidebar / secondary background */
--s9:   #fafaf9     /* Primary text */
--s7:   #d6d3d1     /* Secondary text */
--s6:   #a8a29e     /* Tertiary text */
--s5:   #8a8580     /* Muted text */
--s4:   #78716c     /* Labels, hints */
--s3:   #57534e     /* Very muted */
--vb:   rgba(255,255,255,0.08)  /* Borders (vellum) */
--vh:   rgba(255,255,255,0.04)  /* Hover backgrounds */

--sage: #84a59d     /* Healthy / success / primary accent */
--clay: #d4a373     /* Warning / attention */
--gold: #c4b060     /* Named instances / keystone */
--red:  #c45050     /* Critical / danger (>85% resources) */
```

### Typography

```css
--sans: 'IBM Plex Sans', system-ui, sans-serif   /* UI text */
--mono: 'IBM Plex Mono', ui-monospace, monospace  /* Data, labels, code */
```

- Labels: mono, 0.45-0.55rem, uppercase, letter-spacing 0.1-0.2em, `--s4` color
- Body: sans, 0.72-0.88rem
- Headings: sans, 1.0-1.3rem, font-weight 500-600, letter-spacing -0.02em
- Data values: sans, 1.3-1.4rem, font-weight 600

### Stone Colors

Each stone has a unique muted color. These are generated at stone creation and persist. They serve as the stone's visual identity throughout the UI — sidebar pips, color bars, edge pips, arc inner fills. Never use stone colors for semantic meaning (that's what sage/clay/red are for).

Examples from mock data: `#84a59d`, `#d4a373`, `#c4b060`, `#a8a29e`

### Cards

```css
background: rgba(40, 40, 40, 0.65);
backdrop-filter: blur(14px);
border: 1px solid var(--vb);
border-radius: 4px;
padding: 0.9rem;
```

### Buttons

```css
background: transparent;
border: 1px solid var(--vb);
border-radius: 2px;
padding: 0.25rem 0.55rem;
font-family: var(--mono);
font-size: 0.55rem;
text-transform: uppercase;
color: var(--s5);
/* Hover: fill with accent color, white text */
```

Destructive buttons (`.warn`): hover fills with `--clay` instead of `--sage`.

### Animations

```css
--ease: cubic-bezier(0.22, 1, 0.36, 1);  /* All transitions */

/* Fade in with stagger */
animation: fadeIn 0.45s var(--ease) forwards;
opacity: 0;
/* .fi1 delay: 0.06s, .fi2: 0.12s, .fi3: 0.18s */

/* Breathing (health dots, sidebar pip) */
@keyframes br {
  0%, 100% { opacity: 0.6; box-shadow: 0 0 4px accent; }
  50% { opacity: 1; box-shadow: 0 0 10px accent; }
}
/* Duration varies: 3s thriving, 1.5s withering */
```

### Layout Principle: Content Left, Actions Right

**Every row** follows the pattern:
```
[status] [name + description + metadata] ........... [data] [BUTTONS]
```

Actions (buttons) are always right-aligned using `margin-left: auto` on a flex container. This applies to:
- Service rows (Rest/Wake, Config on right)
- Instance naming rows (Name/Rename/Clear on right)
- Seed bank rows (Eject, Release on right)
- Companion rows (Commands on right)
- Stone administration (all buttons right-aligned)
- Replica group member rows (actions on right)

---

## 7. Data Model (Mock Data for Development)

```javascript
const STONES = [
  {
    id: "stone-crystal-forest",
    name: "crystal-forest",     // Display without "stone-" prefix
    color: "#84a59d",            // Unique persistent color
    health: "thriving",          // thriving | withering | resting
    endpoint: "http://192.168.1.42:7185",
    os: "Ubuntu 24.04",
    hardware: {
      manufacturer: "Dell",
      model: "Wyse 5070",
      cpu_cores: 4,
      memory_gb: 8,
      architecture: "x86_64"
    },
    resources: {                 // Live metrics (0-100)
      cpu: 23,
      memory: 62,
      disk: 41
    },
    network: { rx_mb: 142.3, tx_mb: 38.7 },
    uptime: "14d 7h",
    services: [
      {
        offering: "mongodb",
        instanceName: null,      // null = unnamed = communal identity
        status: "running",       // running | stopped
        image: "mongo:7",
        port: 27017,
        description: "Document database",
        capabilities: null       // or string[] for ollama models etc.
      },
      // ... more services
    ],
    seeds: [
      {
        name: "garden-primary",   // Same name on other stones = replica
        filesystem: "btrfs",
        size: "32GB",
        used: "12.4GB",
        mountpoint: "/mnt/seeds/primary",
        status: "mounted"         // mounted | ejected
      }
    ],
    companions: [
      { name: "cricket", status: "running", port: 7187, detail: "tune: mr-robot · vol: 65" },
      { name: "firefly", status: "running", port: 7188, detail: "mode: presence" }
    ],
    pondRole: "keystone",         // keystone | member
    tags: []                      // ["attention", "opportunity"]
  },
  // ... more stones
];
```

### Computed Data

**Replica groups** are computed by grouping all services (or seed banks) across all stones by identity key:

```javascript
function buildReplicaGroups(stones) {
  const groups = {};
  stones.forEach(stone =>
    stone.services.forEach(svc => {
      const key = svc.instanceName
        ? `${svc.offering}:${svc.instanceName}`
        : svc.offering;
      if (!groups[key]) groups[key] = [];
      groups[key].push({ stoneId: stone.id, stone, service: svc });
    })
  );
  return groups; // Groups with >1 member are replica sets
}
```

**Topology edges** are computed by finding stones that share set identities:

```javascript
function computeEdges(stones) {
  const edges = [];
  for (let i = 0; i < stones.length; i++)
    for (let j = i + 1; j < stones.length; j++) {
      const shared = new Set();
      stones[i].services.forEach(a =>
        stones[j].services.forEach(b => {
          if (svcKey(a) === svcKey(b)) shared.add(svcKey(a));
        })
      );
      if (shared.size > 0)
        edges.push({ from: stones[i].id, to: stones[j].id, sets: [...shared] });
    }
  return edges;
}
```

---

## 8. Key Design Principles

1. **Progressive density:** Show less by default, reveal on interaction. Overview is glanceable. Bloom reveals services. Click reveals actions. Detail view shows everything.

2. **Read health as gestalt:** The Overview should communicate garden health the way you read a face — instantly, without analyzing individual metrics. A thriving garden *looks* alive (sage green, gentle breathing). A stressed garden *looks* urgent (clay/red, fast breathing).

3. **The metaphor is the architecture:** The zen garden vocabulary isn't decorative. It drives technical decisions. Stones breathe, offerings grow, seed banks replicate by name, ponds create trust boundaries.

4. **Physicality over theater:** Infrastructure should feel tangible. Stone nodes have weight (color pips, hardware hints). Edges have activity (sparkles traveling). Health segments pulse organically, not with mechanical precision.

5. **Content left, actions right:** Every row follows this pattern. Never scatter buttons. The eye scans left-to-right: understand the thing, then act on it.

6. **Amber-ridge is the canary:** In the mock data, amber-ridge (2 cores, 4GB) is always withering — CPU 89%, MEM 91%. This is the visual stress test. If the design communicates its distress clearly at a glance, the design works.

7. **Joy in infrastructure:** This is not a generic monitoring dashboard. It should feel like tending a garden, not operating a machine. Language matters (thriving not OK, offering not container, seed bank not volume).

---

## 9. Prototype Reference Files

The accompanying ZIP contains six self-contained React (.jsx) prototypes:

| File | View | Key Features |
|------|------|-------------|
| `lantern-overview.jsx` | Overview | SVG spatial canvas, breathing arcs, bloom interaction, sparkles, action panels |
| `lantern-garden.jsx` | Garden | Stone cards grid, resource bars, service chips, replica groups, activity preview |
| `lantern-stone-detail.jsx` | Stone Detail | Resource gauges, service management, seed banks with replicas, instance naming, companions, administration |
| `lantern-offerings.jsx` | Offerings | Set-centric topology, filter bar, identity groups, deployment state |
| `lantern-seed-banks.jsx` | Seed Banks | Storage topology, aggregate usage bars, replica sets, unseeded stones |
| `lantern-activity-pond.jsx` | Activity + Pond | Live event stream with simulated SSE, trust circle members, security properties |

**These prototypes are the source of truth for visual design.** They contain working CSS, animation code, layout patterns, color values, and interaction flows. When in doubt about a design decision, render the prototype and match it.

**They are NOT production code.** They use inline styles, duplicated CSS, mock data, and simplified state management. The real implementation should use a proper component library, shared design tokens, API integration, and proper state management.

---

## 10. Implementation Notes

### Technology Recommendations

- **Frontend:** React + TypeScript (matches prototypes), or Svelte/Solid if preferred
- **Styling:** CSS modules or Tailwind with custom design tokens. The prototypes use CSS custom properties — extract these as the token system.
- **State:** React Query or SWR for API data. SSE via EventSource for real-time updates. Local state for UI interactions (bloom, hover, selected view).
- **SVG:** The Overview uses SVG for the spatial canvas. This works well up to ~20 stones. For larger gardens, consider canvas/WebGL with a library like Pixi.js.
- **Routing:** Client-side routing for view navigation. URL should reflect current view and selected stone.

### API Integration Points

1. **Initial load:** Fetch all stones, offerings catalog, seed banks, pond members
2. **SSE stream:** Connect to `/api/v1/garden/presence/stream` for real-time updates
3. **Actions:** POST to stone-specific endpoints for rest/wake/name operations
4. **Polling fallback:** If SSE disconnects, poll every 5-10s until reconnected

### Responsive Strategy (Deferred)

The prototypes are desktop-first (sidebar + main content). Mobile/tablet layouts are deferred but should be considered:
- Sidebar collapses to hamburger on mobile
- Overview canvas gets touch zoom/pan
- Cards stack vertically
- Action panels become bottom sheets

### Accessibility (Important)

- Keyboard navigation for all interactive elements
- Screen reader support: aria-labels on stone nodes, live regions for SSE events
- Color is never the only indicator — always paired with text labels or patterns
- Focus management when bloom expands/collapses

---

## 11. Open Questions

These are design decisions that were intentionally deferred during prototyping:

1. **3 vs 6 health arc segments:** Current design uses 3 (CPU/MEM/DSK at 120° each). Could expand to 6 (adding NET/IO/GPU at 60° each) for richer feedback. Kept at 3 for clarity.

2. **Dual-bloom for replica comparison:** Could allow blooming two stones simultaneously to visually compare their replica peers. Kept single-bloom for simplicity.

3. **Force layout for large gardens:** Manual positioning works for 4-8 stones. 20+ stones need d3-force or similar with pan/zoom.

4. **Network activity visualization:** Currently simulated with sparkles. Real implementation could use edge glow intensity or animated halos based on actual traffic metrics.

5. **Offering deployment wizard:** The "+ Deploy" button in the Offerings view needs a deployment flow. This could be a modal or slide-out panel with stone selection, configuration, and confirmation.

6. **Historical trends:** The Activity view shows real-time events. A time-series view (resource usage over hours/days) would be valuable but requires Lantern persistence.

7. **Multi-garden support:** Current design assumes one garden. If a user runs multiple gardens, Lantern would need garden selection.
