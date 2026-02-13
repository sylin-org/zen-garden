# GardenSphere Integration Guide

3D rotatable sphere visualization for the Lantern dashboard. Renders garden
stones as nodes on a sphere, connected by great-circle arcs when they share
service offerings. Framework-agnostic Three.js library with a reactive API.

## Files

| File | Purpose |
|------|---------|
| `garden-sphere.js` | **Library** — `GardenSphere` class, zero framework dependencies (needs Three.js) |
| `lantern-sphere-demo.jsx` | **Reference** — Full working React demo with simulation panel, card transitions, CSS |

## Quick Start

```bash
npm install three
```

```js
import { GardenSphere } from "./garden-sphere.js";

const container = document.getElementById("sphere");
const gs = new GardenSphere(container, {
  onHover: (id) => console.log("hover", id),
  onTrack: ({ selected, departing, hovered, progress }) => { /* 60fps updates */ },
  onTransition: ({ selectedId, departingId }) => { /* selection changed */ },
  onDataChange: (stones) => { /* stones array mutated */ },
});

gs.setData(stones);
```

The library appends a `<canvas>` to `container`, handles its own resize, and
runs its own animation loop. Call `gs.destroy()` on unmount.

---

## Stone Data Shape

Every stone must conform to this shape. All fields are required on initial load;
`updateStone` accepts any subset as a patch.

```ts
interface Stone {
  id: string;          // Unique identifier (e.g. "cf", "quiet-stream")
  name: string;        // Display name shown on the sphere
  color: string;       // Hex color for the node body ("#84a59d")
  health: "thriving" | "withering" | "resting";
  hw: {
    cores: number;     // CPU cores
    mem: number;       // RAM in GB
  };
  res: {
    cpu: number;       // 0-100 percentage
    mem: number;       // 0-100 percentage
    dsk: number;       // 0-100 percentage
  };
  svcs: Service[];     // Offerings running on this stone
  pond: "keystone" | "member";
}

interface Service {
  o: string;           // Offering name ("mongodb", "redis", "ollama")
  i: string | null;    // Instance name (null for default, "snapvault" for named)
  s: "running" | "stopped";
}
```

### How connections work

Connections (great-circle arcs) form automatically between any two stones that
share a service identity. The identity key is:

- `"mongodb"` — when instance is null
- `"postgres:snapvault"` — when instance is set

So `{ o: "mongodb", i: null }` on stone A connects to `{ o: "mongodb", i: null }`
on stone B. But `{ o: "postgres", i: "snapvault" }` does NOT connect to
`{ o: "postgres", i: "analytics" }` — different instances are different identities.

Connection labels show the shared service names along the arc. The tube thickness
increases with more shared services. Animated sparkles travel along the arc.

---

## API Reference

### `new GardenSphere(container, options?)`

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `radius` | `number` | `10` | Sphere radius in world units |
| `onHover` | `(id\|null) => void` | noop | Fires when hovered stone changes |
| `onTrack` | `(data) => void` | noop | Fires every frame (60fps) — see below |
| `onTransition` | `(data) => void` | noop | Fires on click — selection changed |
| `onDataChange` | `(stones[]) => void` | noop | Fires after any data mutation |

### `onTrack` payload (every frame)

```ts
{
  selected:  { id: string, pos: { x, y } } | null,  // screen coords of selected node
  departing: { id: string, pos: { x, y } } | null,  // screen coords of previous node
  hovered:   { id: string, pos: { x, y } } | null,  // screen coords of hovered node
  progress:  number  // 0→1 slerp progress (1 = idle)
}
```

Use `pos` to position HTML overlays (info cards, tooltips) that track the nodes.
The `progress` field drives card transition animations — see the demo's dual-card
system for the pattern.

### Methods

#### `gs.setData(stones: Stone[])`
Initial load. Clears everything and rebuilds from scratch. Call once on startup,
then use incremental methods for live updates.

#### `gs.updateStone(id: string, patch: Partial<Stone>)`
Update a stone in-place. **Does not trigger re-layout** — nodes stay in their
positions. The canvas texture re-renders immediately. If `patch.svcs` is present,
connections are rebuilt to reflect new service topology.

```js
// Live metrics update (e.g. from polling)
gs.updateStone("cf", { res: { cpu: 45, mem: 72, dsk: 41 } });

// Health change
gs.updateStone("ar", { health: "thriving", res: { cpu: 30, mem: 40, dsk: 35 } });

// Service added
gs.updateStone("qs", {
  svcs: [...currentSvcs, { o: "prometheus", i: null, s: "running" }]
});
```

#### `gs.addStone(stone: Stone)`
Add a new stone to the sphere. Triggers animated Fibonacci re-layout — all
existing nodes smoothly migrate to new positions, and the new node scales in
from zero. Connections rebuild automatically.

```js
gs.addStone({
  id: "gs", name: "golden-summit", color: "#b8a088", health: "thriving",
  hw: { cores: 4, mem: 8 }, res: { cpu: 12, mem: 45, dsk: 22 },
  svcs: [{ o: "redis", i: null, s: "running" }], pond: "member"
});
```

#### `gs.removeStone(id: string)`
Fade the stone out (scale + opacity → 0 over 500ms), then remove it and trigger
re-layout for remaining stones. If the removed stone was selected, selection
clears automatically.

#### `gs.offlineStone(id: string)`
Mark a stone as offline. The node stays in its sphere position but:
- Body turns gray (#444)
- Canvas re-renders with "OFFLINE" label and muted colors
- All connections to this stone are removed
- Breathing animation stops

#### `gs.onlineStone(id: string, patch?: Partial<Stone>)`
Restore a stone from offline. Optionally merge new data in the same call.
Colors restore, connections rebuild, breathing resumes.

```js
gs.onlineStone("ar", { health: "thriving", res: { cpu: 20, mem: 30, dsk: 35 } });
```

#### `gs.resetView()`
Reset sphere rotation and camera to default position.

#### `gs.destroy()`
Full cleanup: cancel animation loop, unbind all event listeners, dispose all
GPU resources (geometries, materials, textures), remove canvas from DOM.
**Always call on unmount.**

---

## Wiring to Lantern Data

### From garden-lantern API responses

The Lantern service provides stone data via its HTTP API. Map the response:

```js
function mapStone(raw) {
  return {
    id: raw.id,
    name: raw.name,
    color: raw.color || "#84a59d",
    health: raw.health,          // "thriving" | "withering" | "resting"
    hw: { cores: raw.cores, mem: raw.memoryGb },
    res: { cpu: raw.cpuPercent, mem: raw.memPercent, dsk: raw.diskPercent },
    svcs: raw.services.map(s => ({
      o: s.offering,
      i: s.instance || null,
      s: s.status                // "running" | "stopped"
    })),
    pond: raw.isKeystone ? "keystone" : "member",
  };
}
```

### Polling pattern

```js
async function poll(gs) {
  const res = await fetch("/api/stones");
  const data = await res.json();

  const currentIds = new Set(gs.stones.map(s => s.id));
  const newIds = new Set(data.map(s => s.id));

  // New stones
  data.filter(s => !currentIds.has(s.id)).forEach(s => gs.addStone(mapStone(s)));

  // Removed stones
  gs.stones.filter(s => !newIds.has(s.id)).forEach(s => gs.removeStone(s.id));

  // Updated stones
  data.filter(s => currentIds.has(s.id)).forEach(s => gs.updateStone(s.id, mapStone(s)));
}

setInterval(() => poll(gs), 5000);
```

### WebSocket pattern (recommended)

```js
const ws = new WebSocket("ws://garden:9090/ws");
ws.onmessage = (e) => {
  const msg = JSON.parse(e.data);
  switch (msg.type) {
    case "stone.joined":     gs.addStone(mapStone(msg.stone)); break;
    case "stone.left":       gs.removeStone(msg.id); break;
    case "stone.offline":    gs.offlineStone(msg.id); break;
    case "stone.online":     gs.onlineStone(msg.id, mapStone(msg.stone)); break;
    case "stone.metrics":    gs.updateStone(msg.id, { res: msg.res, health: msg.health }); break;
    case "stone.services":   gs.updateStone(msg.id, { svcs: msg.svcs.map(mapSvc) }); break;
  }
};
```

---

## React Integration Pattern

The demo file (`lantern-sphere-demo.jsx`) shows the full pattern, but here's the
minimal wiring for a React component:

```jsx
import { useState, useEffect, useRef } from "react";
import { GardenSphere } from "./garden-sphere.js";

function SphereView({ stones }) {
  const containerRef = useRef(null);
  const gsRef = useRef(null);
  const [tracked, setTracked] = useState(null);

  // Mount
  useEffect(() => {
    const gs = new GardenSphere(containerRef.current, {
      onHover: (id) => { /* update hover UI */ },
      onTrack: setTracked,
      onTransition: ({ selectedId }) => { /* update selection state */ },
    });
    gs.setData(stones);
    gsRef.current = gs;
    return () => gs.destroy();
  }, []);

  // React to stone changes
  useEffect(() => {
    if (!gsRef.current) return;
    // Diff and apply — see polling pattern above
  }, [stones]);

  return (
    <div style={{ width: "100%", height: "100vh" }}>
      <div ref={containerRef} style={{ width: "100%", height: "100%" }} />
      {/* Position HTML cards using tracked.selected?.pos, tracked.hovered?.pos */}
    </div>
  );
}
```

### Card transition pattern (dual-card)

The demo implements a cinematic card transition where:
1. Old card stays attached to its stone as it rotates away, fading to gray
2. New card appears small at the clicked stone and grows as it swings to center

Key state from `onTrack`:
- `selected.pos` — screen position of arriving node (every frame)
- `departing.pos` — screen position of departing node (every frame)
- `progress` — 0→1 slerp completion

Arriving card: `opacity = 0.5 + 0.5 * progress`, `scale = 0.8 + 0.2 * progress`
Departing card: `opacity = max(0, 1 - progress * 1.5)`, `grayscale(progress * 2)`

See `lantern-sphere-demo.jsx` lines 360-394 for the complete implementation.

---

## Visual Behaviors

### Breathing
- **Thriving**: 3s cycle (0.5 Hz), emissive pulses 0.25→0.5
- **Withering**: 1.5s cycle (1.3 Hz), same range but anxious tempo
- **Resting**: static dim 0.08 emissive

### Depth fading
Front nodes: 100% opacity, 100% scale.
Back nodes: 8% opacity, 55% scale.
Smooth depth-based interpolation creates strong 3D presence.

### Interactions
- **Hover**: mouse presence smoothly decelerates auto-rotation
- **Left-click**: slerp rotation to bring clicked node to front + select
- **Right-drag**: quaternion-based free rotation with inertia (0.96 decay)
- **Scroll**: zoom 16–48 units
- **Auto-rotate**: resumes after 3.5s idle

### Design tokens
- Sage: `#84a59d` (thriving, connections, UI accents)
- Clay: `#d4a373` (withering, warnings)
- Gold: `#c4b060` (keystone badge, instances)
- Stone grays: `#fafaf9` → `#57534e` → `#111110`
- Fonts: IBM Plex Sans (UI), IBM Plex Mono (data)

---

## Performance Notes

- Tested with 4–6 stones. Scales comfortably to ~20 before needing optimization.
- Canvas textures (512×512) render once per stone, not per-frame. Re-rendered only on `updateStone`, `offlineStone`, `onlineStone`.
- Sparkle count limited to 2–3 per connection.
- Stars rendered as single Points geometry (250 points).
- For >20 stones: consider LOD (reduce canvas resolution for back-facing), instanced meshes for sparkles, throttle `onTrack` to 30fps.
