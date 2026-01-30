# PORTRAIT-0001: Stone Landing Page

**Status:** Accepted  
**Date:** 2026-01-27  
**Objective:** Transform the 404 at `GET /` into a living portrait of the stone.

---

## Executive Summary

When you navigate to `http://stone-crystal-forest:7185/`, you should encounter something that embodies the project's philosophy: **tangible, comprehensible, present**. Not a JSON blob. Not a redirect. A *place*.

The Stone Portrait is a single-page application (SPA) that displays the stone's identity, vitals, offerings, companions, and horizon—updated every 4 seconds to create a "breathing" effect. The page is entirely self-contained: HTML, CSS, and JavaScript embedded at compile time. No external dependencies.

**Core principle:** A Stone is a place. The landing page should feel like arriving at that place.

---

## Design Philosophy

### Specialist Team Assessment

**UX (Elena):** *"The 404 is a missed opportunity. Every Stone is a doorway into someone's garden. The first thing you see should tell you where you are and what lives here. Minimal, but with warmth."*

**DX (Marcus):** *"Developers hitting the root path are either debugging, exploring, or lost. Give them `/health`, `/api/v1/manifest`, and the stone's name in 3 seconds or less. But make it pleasant—they'll remember that."*

**Ops (Priya):** *"I need to know: Is this Stone alive? What's it running? How long has it been up? I shouldn't need to `curl /health | jq`. The root path is my first diagnostic."*

**Semantics (Theo):** *"A Stone is a place. The page should feel like arriving at a stone in a garden—solid, patient, present."*

**UI (Kai):** *"The design carries the poetry. The labels carry the information. Literal words, elegant presentation."*

### Portrait vs Dashboard

This is **not a dashboard**. It's a **portrait**.

| Dashboard | Portrait |
|-----------|----------|
| Monitor continuously | Visit, see, leave |
| Charts and graphs | Numbers and status dots |
| Demands attention | Invites a glance |
| Control room | Doorway |

The page loads as a snapshot. If you stay, it breathes—values update gently every 4 seconds. But you're not meant to watch it all day. You're meant to *check in*.

---

## Structure

The portrait has five sections:

### 1. Hero — Identity

```
STONE                                    [QR Code]
stone-crystal-forest
http://stone-crystal-forest:7185
```

- **Role label**: `STONE`, `LANTERN`, or `CORNERSTONE` (dynamic based on state)
- **Name**: The stone's name from configuration
- **Endpoint**: The stone's URL
- **QR Code**: Generated server-side, allows mobile access by scanning
- **Color bar**: 3px vertical line in the stone's unique color (derived from stone_id hash)

### 2. Foundation — Vitals

Three cards: CPU, Memory, Disk. Optional fourth: Temperature (if available).

Each card shows:
- Label (literal: "CPU", "Memory", "Disk")
- Value with unit
- Thin gauge bar (2px, animates to show percentage)

### 3. Offerings — Services

List of planted offerings (managed containers):
- Name (mongodb, redis, postgresql)
- Container ID
- Port
- Health status (dot + label)

Empty state: *"No offerings planted."*

### 4. Companions — Companions

List of Companions running on this stone:
- ID (cricket, firefly)
- Description (audio Companion, LED control)
- Port
- Status (running/stopped)

Empty state: *"No companions."*

### 5. Horizon — Visible Stones

Collapsed by default. Shows count: "▶ 5 stones visible"

Expands to show list of other stones this one can see:
- Name
- Health status
- Link to their portrait

Empty state: *"Alone in the garden."*

---

## Visual Design

### Aesthetic: Vellum

- Warm background: `#f4f2ee` (parchment/cream)
- Frosted glass cards with subtle grain texture
- Sage green for healthy status: `#84a59d`
- Clay/amber for active/warning: `#d4a373`
- Soft shadows, rounded corners (2px)
- Generous whitespace

### Typography

System fonts for zero external dependencies:
- Headers: `system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif`
- Data: `ui-monospace, 'Cascadia Code', 'Fira Code', Consolas, monospace`

### Stone Color

Each stone has a unique accent color derived from its stone_id:

```javascript
function stoneHue(stoneId) {
  let hash = 0;
  for (let i = 0; i < stoneId.length; i++) {
    hash = stoneId.charCodeAt(i) + ((hash << 5) - hash);
  }
  return Math.abs(hash) % 360;
}
// Usage: hsl(${hue}, 55%, 50%)
```

This color appears as:
- The vertical bar next to the stone name
- CSS variable `--stone-color` available for companion indicators

### Breathing

The page polls `/api/v1/stone/portrait` every 4 seconds. Changes animate via CSS transitions:
- Gauge bars: 1.5s ease transition on width
- Status dots: 2s pulse animation
- Values: 0.3s fade transition

---

## Technical Architecture

### Single File, Embedded at Build Time

```
src/moss/
├── assets/
│   └── portrait.html      ← Maintained as real HTML file
├── src/
│   └── api/v1/
│       └── portrait.rs    ← Handler includes file at compile time
```

```rust
const PORTRAIT_HTML: &str = include_str!("../../assets/portrait.html");
```

The HTML file is a complete, valid HTML document that can be opened directly in a browser for development. At compile time, it's embedded into the binary.

### Dependencies

| Asset | Size | Approach |
|-------|------|----------|
| Alpine.js | ~15KB | Inlined in `<script>` tag |
| CSS | ~5KB | Inlined in `<style>` tag |
| Fonts | 0KB | System fonts only |
| QR Code | ~1KB | Generated server-side as SVG |

**Total page size:** ~30KB, zero external requests.

### Endpoints

#### `GET /`

Returns the portrait HTML page. Content-Type: `text/html`.

#### `GET /api/v1/stone/portrait`

Returns JSON payload with all data needed for the portrait:

```json
{
  "identity": {
    "role": "STONE",
    "name": "stone-crystal-forest",
    "endpoint": "http://stone-crystal-forest:7185",
    "version": "0.1.47",
    "uptime": "3d 14h 22m",
    "color": "hsl(217, 55%, 50%)"
  },
  "foundation": {
    "cpu": { "percent": 12.4 },
    "memory": { "used_gb": 4.8, "total_gb": 64.0, "percent": 7.5 },
    "disk": { "used_gb": 120, "total_gb": 500, "percent": 24.0 },
    "temperature": null
  },
  "offerings": [
    { "name": "mongodb", "container": "zen-offering-mongodb", "port": 27017, "status": "running", "health": "healthy" }
  ],
  "companions": [
    { "id": "cricket", "description": "audio Companion", "port": 7187, "status": "running" }
  ],
  "horizon": {
    "count": 5,
    "stones": [
      { "name": "stone-morning-dew", "health": "thriving", "endpoint": "http://stone-morning-dew:7185" }
    ]
  },
  "qr_svg": "<svg>...</svg>"
}
```

### Alpine.js Data Flow

```html
<main x-data="stonePortrait()" x-init="startBreathing()">
  <!-- Bindings use x-text, x-for, :style, :class -->
</main>

<script>
function stonePortrait() {
  return {
    identity: {}, foundation: {}, offerings: [], companions: [], horizon: {},
    qr_svg: '',
    
    async refresh() {
      const res = await fetch('/api/v1/stone/portrait');
      if (res.ok) Object.assign(this, await res.json());
    },
    
    startBreathing() {
      this.refresh();
      setInterval(() => this.refresh(), 4000);
    }
  }
}
</script>
```

---

## Implementation Checklist

### Rust Side

- [x] Decision document (this file)
- [ ] Create `src/moss/assets/portrait.html`
- [ ] Create `src/moss/src/api/v1/portrait.rs`
- [ ] Add `pub mod portrait` to `src/moss/src/api/v1/mod.rs`
- [ ] Wire routes in `src/moss/src/bootstrap/router.rs`:
  - `GET /` → `portrait::get_portrait_page`
  - `GET /api/v1/stone/portrait` → `portrait::get_portrait_data`
- [ ] Add QR generation (optional: can use placeholder initially)

### HTML Side

- [ ] Structure: Hero, Foundation, Offerings, Companions, Horizon
- [ ] Literal labels (CPU, Memory, Disk)
- [ ] Alpine.js bindings
- [ ] System fonts
- [ ] Stone color variable
- [ ] Dark mode support (`prefers-color-scheme`)
- [ ] Responsive layout (mobile-friendly)

---

## Future Considerations

### Text Mode (Not in Scope)

A future enhancement could detect `Accept: text/plain` or `curl` user-agent and return an ASCII version of the portrait for terminal use.

### Companion Color Chips

Companions like Cricket could display a small indicator in the stone's color, creating visual association across the garden.

---

## References

- [Zen Garden Philosophy: Joy in Infrastructure](../philosophy/joy-in-infrastructure.md)
- [Zen Garden Philosophy: Metaphor as Architecture](../philosophy/metaphor-as-architecture.md)
- [PRESENCE-0001: Stone Presence Protocol](PRESENCE-0001-COMPLETE.md)
- [ARCHITECTURE-REFERENCE.md](../ARCHITECTURE-REFERENCE.md)
