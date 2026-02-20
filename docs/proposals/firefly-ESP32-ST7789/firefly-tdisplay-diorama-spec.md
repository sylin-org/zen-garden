# Firefly T-Display Diorama — Implementation Specification

**Target hardware:** LILYGO T-Display ESP32 (ST7789 135×240 color TFT)
**Reference simulator:** `firefly-diorama-v6.jsx` (React/Canvas — for visual reference only, not a codebase to port)

This document is a framework-agnostic specification. Implement in whatever is appropriate for the ESP32 T-Display (Arduino/TFT_eSPI, ESP-IDF, MicroPython, Rust+embassy, etc). The reference `.jsx` file is a pixel-accurate simulator — use it to visually verify your output, but don't try to translate its JavaScript idioms into firmware. Think in framebuffers, not DOM.

---

## 1. Overview

Each Zen Garden "Stone" (a physical compute node) can have an ESP32 T-Display companion attached. The display renders a living diorama — a miniature zen garden scene with pixel-art sprites, animated fireflies representing running services, real astronomy (moon phase, day/night cycle), and ambient status encoding.

The display has three visual panels stacked vertically, plus a persistent identity bar on the left edge.

**Key principle:** The scene should look alive and peaceful when healthy, visually disturbing when degraded, and feel like an actual fire when withering. Glanceability from across a room is paramount.

---

## 2. Display Layout

```
135px wide × 240px tall, landscape-oriented vertically (portrait mode)

┌─────────────────────────────────────┐
│B│           TOP PANEL               │  96px — identity, health, gauges
│A│                                   │
│R├───────────────────────────────────┤
│ │           SCENE                   │  72px — diorama (sky, stone, ground)
│5│                                   │
│p├───────────────────────────────────┤
│x│           BOTTOM PANEL            │  72px — offerings list, capability icons
│ │                                   │
└─────────────────────────────────────┘
```

### Constants

| Name | Value | Notes |
|------|-------|-------|
| W | 135 | Display width |
| H | 240 | Display height |
| BAR | 5 | Identity color bar width (left edge) |
| HEAD_H | 96 | Top panel height |
| FOOT_H | 72 | Bottom panel height |
| SCENE_H | 72 | Scene height (H - HEAD_H - FOOT_H) |
| SCENE_T | 96 | Scene top Y coordinate |
| SCENE_B | 168 | Scene bottom Y coordinate |

---

## 3. Color System

### Named Colors

| Name | Hex | Meaning |
|------|-----|---------|
| SAGE | `#84a59d` | Healthy/thriving |
| HONEY | `#c4b060` | Warning/busy |
| CLAY | `#d4a373` | Degraded/withering |

### Identity Hue

Each stone has a deterministic hue derived from its name via a hash function:

```
hue = abs(hash(stone_name)) % 360
```

This hue tints the identity bar, stone sprite, and subtle UI accents. The hash is: iterate over name characters, for each char: `h = charCode + ((h << 5) - h)`.

The identity bar color is `hsl(hue, 40%, 60%)`. The panel background is `hsl(hue, 6%, 8%)`.

### Gauge Bar Color Ramp

Gauges shift color based on value (0–100):

| Range | Color |
|-------|-------|
| 0–14 | Mid gray `rgb(80,80,80)` |
| 15–29 | Gray → teal |
| 30–49 | Teal → green |
| 50–69 | Green → yellow |
| 70–84 | Yellow → orange |
| 85–100 | Orange → hot bright orange `rgb(255,40,5)` |

Interpolate linearly within each range. This creates a "cold to hot" feel — low values are boring gray, high values glow urgently.

---

## 4. Top Panel (y: 0–95)

### 4.1 Identity Color Bar

Full-height vertical bar at x=0, 5px wide, filled with `hsl(hue, 40%, 60%)`. Apply a subtle gradient overlay: brighter at top (18% white), darker at bottom (22% black). This bar is always visible and is the fastest way to identify which stone you're looking at.

### 4.2 Stone Name

- Label "STONE" at (12, 5), bold 7px monospace, `hsla(hue, 25%, 50%, 0.45)` — barely visible category label
- Stone name at (12, 15), bold 13px monospace, `#ece8e0` — strip the "stone-" prefix (e.g., "stone-amber-ridge" → "amber-ridge")

### 4.3 Health Status

At y=34:
- Breathing health dot: 2.5px radius circle, color = SAGE/HONEY/CLAY based on health state
  - Alpha oscillates: `0.55 + sin(tick * 0.05) * 0.35`
  - Soft radial glow behind it (radius 7px, 13% alpha)
- Health text: bold 8px, same color as dot, e.g. "THRIVING" / "WITHERING"
- Uptime: 7px monospace, right-aligned, dim `#4a4a4a`

### 4.4 Separator

1px horizontal line at y=50, `hsla(hue, 15%, 25%, 0.2)`

### 4.5 Gauge Bars

Four bars starting at y=57, spaced 9px apart:

| Label | Data |
|-------|------|
| CPU | CPU utilization % |
| MEM | Memory utilization % |
| DSK | Disk utilization % |
| I/O | I/O utilization % |

Each bar:
- Label: bold 6px monospace, `#4a4a4a`, at x=12
- Track: 3px tall rectangle, `rgba(255,255,255,0.035)` background, starts at x=34
- Fill: width proportional to value, color from gauge ramp (§3), 80% alpha
- Value text: 6px monospace, right-aligned. Color = CLAY if >80%, else `#555`

---

## 5. Scene (y: 96–167) — Healthy State

The scene is a living diorama. When health is "thriving" or "settled", it renders the full zen garden. When "withering", it switches to the fire mode (§6).

### 5.1 Sky

Three-stop vertical gradient from SCENE_T to groundY (85% of scene height):

**Night** (sunP < 0.15):
- Top: `#05061a`, Mid: `#0a0d22`, Bottom: `#10132a`

**Dusk** (sunP 0.15–0.35):
- Interpolate from night colors toward: Top `#281838`, Mid `#4a2a40`, Bottom `#b06030`

**Day** (sunP > 0.35):
- Interpolate toward: Top `#3a5878`, Mid `#6a8aa8`, Bottom `#98b0c0`

**Sun position** is calculated from the hour: `sunP = max(0, sin(((hour - 6) / 12) * π))`. This creates sunrise at 6, noon peak at 12, sunset at 18.

### 5.2 Stars

26 deterministic stars (seeded RNG, seed=42). Each has position, brightness, twinkle speed, and twinkle phase offset. Only visible when sunP < 0.5, fading proportionally. Color: `#e8e4d8`.

### 5.3 Moon

Real lunar phase via Conway's algorithm (see reference code). Drawn in upper-right area. 5px radius. Surface texture via `(px*7 + py*13) % 5 === 0` stipple. Only visible when sunP < 0.45.

### 5.4 Ground Line

At 85% of scene height (`groundY = SCENE_T + round(SCENE_H * 0.85)`). The bottom 15% of the scene is the ground surface.

### 5.5 Ground — Pond Mode

When the stone is part of a pond (orchestration group), show water:
- Water gradient: dark blue tones, day/dusk/night variants
- Shore line: 1px, subtle light blue
- Two concentric ripple arcs centered on stone position, gently oscillating radius

### 5.6 Ground — Sand Mode (No Pond)

Karesansui (Japanese dry landscape garden):
- Base sand fill: day `#d0cbbe`, dusk `#3a362e`, night `#1c1b18`
- Sand grain texture: pseudo-random light/dark pixels via `(x*17 + y*31) % 7`
- 10 concentric elliptical rake rings radiating from stone center
  - Ellipse compression: 1.8× wider, 0.55× tall (foreshortened perspective)
  - Alternating light/dark rings
  - Gap around stone footprint (±11px horizontal)
- Subtle oval shadow beneath stone

### 5.7 Stone Sprite

Three variants selected by `floor(hue / 120) % 3`:

**Variant 0 — Rounded dome:** Smooth arc top, flat base. ~28×14px.
**Variant 1 — Craggy peak:** Asymmetric jagged top rising to the right, flat base. ~28×14px.
**Variant 2 — Wide shelf:** Very flat, low profile, wide, flat base. ~28×14px.

All sprites have FLAT BOTTOMS (solid edge line at the base). All use the same palette system:
- Edge `.`: `hsl(hue, 25%, 24%)` (darkest)
- Body `S`: `hsl(hue, 25%, 38%)` (mid)
- Highlight `H`: `hsl(hue, 20%, 46%)` (lighter patch)
- Shadow `D`: `hsl(hue, 28%, 30%)` (dark crevice)

Night dimming multiplies lightness by 0.45. Dusk by 0.7.

Position: centered horizontally at 42% of scene width, bottom edge 2px into ground.

See the reference `.jsx` for exact sprite bitmaps.

### 5.8 Cricket (Optional — Audio Companion Present)

21×16 pixel side-profile cricket sitting on top of the stone. Hand-drawn pixel art with dark 1px border (`#1a1a1a`). Six-color green palette for head, thorax, wings, legs. Alternates between rest and chirp frames every 90 ticks (chirp frame shows `~` sound waves in HONEY color near antenna).

At night, all cricket colors dim by 50%.

Position: `x = stoneX + round(stoneW * 0.25)`, `y = stoneY - spriteHeight + 6` (legs overlap onto stone top).

See the reference `.jsx` for exact sprite bitmaps.

### 5.9 Tōrō Lantern (Optional — Lantern Registry Role)

17×30 pixel Japanese stone lantern with traditional anatomy: hōju finial, kasa roof, hibukuro fire chamber with window openings, chūdai platform, sao tapered pillar, kiso wide base.

Three palettes (day/dusk/night) shifting stone color. Fire chamber `F` color flickers via dual sine waves: `0.7 + sin(tick * 0.18) * 0.15 + sin(tick * 0.31) * 0.1`.

Warm glow: radial gradient from fire chamber center. Night: 16px radius, 20% intensity. Dusk: 12px, 12%. Day: 8px, 7%. Glow pulses with additional sine oscillation. Lantern body redrawn on top of glow to stay crisp.

Ground light pool: horizontal spread ±6px beneath lantern, 3 rows deep, fading alpha.

Position: `x = stoneX + stoneW + 4`, feet on ground.

See the reference `.jsx` for exact sprite bitmaps and palette tables.

### 5.10 Fireflies (Service Indicators)

One firefly per running service/offering. Each has deterministic base position, drift radius, speed, and phase (seeded from hue). They drift in gentle Lissajous-like paths.

Movement: `x = baseX + sin(elapsed + phaseX) * driftRadius`, `y = baseY + cos(elapsed * 0.7 + phaseY) * (driftRadius * 0.5)`. Fireflies stay above groundY.

Color by health: healthy = warm yellow (hue 42), warning = orange (hue 30), degraded = red (hue 0).

Glow: radial falloff around each firefly. Night: radius 3.5px. Day: radius 2px. Brightness pulses.

Depth layering: each firefly has a random depth (0–1). Those with depth < 0.5 are drawn behind the stone; depth ≥ 0.5 in front.

Water reflection: if pond mode, a dim reflection appears below groundY.

---

## 6. Scene — Withering State (Fire Mode)

When `health === "withering"`, the entire ground scene is replaced. **No stone, no cricket, no lantern, no sand, no pond.** Just fire.

### 6.1 Reddened Sky

Sky gradient extends to fill the ENTIRE scene (not just to groundY). Normal sky colors at top transition into deep reds:
- 60% stop: `#3a1a10`
- 100% stop: `#1a0800`

### 6.2 Dimmed Astronomy

Stars: 30% opacity, tinted warm `#e8b8a0`, only visible in upper 35% of scene.
Moon: 20% opacity — nearly obscured by haze.

### 6.3 Fleeing Fireflies

Fireflies compress into the top 35% of the scene. Their Y positions are multiplied by 0.3 (squashed upward) with additional jitter: `sin(tick * 0.04 + i * 3.7) * 3`. They look panicked.

### 6.4 Fire Haze

Scanline gradient covering bottom 75% of scene. For each scanline from top to bottom:
- Color shifts: transparent amber → dense orange → deep red at base
- Alpha: `t² * 0.45 + 0.02` where t=0 at top, t=1 at bottom — very dense near base
- Dual shimmer waves: `sin(tick * 0.1 + y * 0.4)` and `sin(tick * 0.07 + y * 0.25 + 1.5)`

### 6.5 Rising Ember Particles

6 ember particles drifting upward from the fire line. Each has a phase cycling 0→1:
- X: pseudo-random horizontal position, slowly drifting
- Y: rises from SCENE_B to ~30% up the scene
- Color: orange `#ff8020` → red `#ff5010` → dark red `#aa3010` as it rises
- Alpha: fades out as it rises `(1 - phase) * 0.7`
- Trail: single pixel above at 40% alpha when young

---

## 7. Bottom Panel (y: 168–239)

### 7.1 Offerings List

Header "OFFERINGS" at (12, SCENE_B+5), bold 6px, dim hue-tinted color.

Up to 4 offerings listed, 9px apart starting at y = SCENE_B+16:
- Health dot: 1.5px radius, SAGE/HONEY/CLAY
- Service name: 7px monospace, `#9a9690`

If >4 offerings: show "+N more" in dim text.

The number of fireflies in the scene should equal the number of offerings listed here.

### 7.2 Capability Icons

Right-aligned row at y = H-16 (y=224). Separated from offerings by a dim 1px line.

Each icon is 8×8 pixels, spaced 10px apart from the right edge.

**Available icons:**

| Icon | Condition | Palette |
|------|-----------|---------|
| Seed Bank | stone has seed bank | Green tones |
| GPU/AI | GPU hardware present | Blue (idle) or Honey (active/inferencing) |
| GPU (off) | Ollama service but no GPU hardware | Dark gray, no animation |
| Lantern | Lantern registry role | Gold tones |

**Animation — Breathing Corner Brackets** (all active icons):
Four L-shaped corner marks (2px each arm) frame each icon. Alpha: `0.2 + sin(tick * 0.04 + idx * 2.3) * 0.15`. Staggered per-icon phase creates independent breathing.

**Animation — Scanning Underline** (busy icons only, e.g., GPU inferencing):
Bright pixel sweeps left↔right beneath icon (ping-pong at per-icon speed). 3-pixel fading trail behind the head. Subtle glow pixel above scan line.

**GPU Inferencing Extra:** Soft radial glow (radius 3px, honey color, pulsing) behind the AI icon.

---

## 8. Scene Frame Lines

Two 1px horizontal lines at SCENE_T and SCENE_B-1 in `hsla(hue, 20%, 25%, 0.35)`. These create a "window into world" framing effect for the diorama.

---

## 9. Data Model

The display needs this data from the Stone (via Moss SSE stream or serial companion):

```
stone:
  name: string          # e.g. "stone-amber-ridge"
  health: enum          # "thriving" | "settled" | "withering"
  uptime: string        # e.g. "47d 3h"
  cpu: 0-100
  mem: 0-100
  disk: 0-100
  io: 0-100
  offerings:            # array of running services
    - name: string      # e.g. "mongodb"
      health: enum      # "healthy" | "warning" | "degraded"
  seed_bank:            # null if none
    name: string
    used: number
    total: number

capabilities:
  has_pond: bool        # is stone in an orchestration group?
  has_cricket: bool     # is garden-cricket audio companion connected?
  has_gpu: bool         # is GPU hardware available?
  gpu_active: bool      # is GPU currently inferencing?
  is_lantern: bool      # does this stone run the lantern registry?

environment:
  hour: 0-23            # current hour (for day/night cycle)
  tick: number          # monotonic animation counter, increments each frame
```

---

## 10. Animation Timing

Target frame rate: whatever is comfortable for the ESP32 — 15-30fps is fine. The tick counter drives all animations.

Key animation speeds:
- Health dot breathing: `sin(tick * 0.05)`
- Star twinkle: per-star speed 0.008–0.038
- Firefly drift: per-firefly speed 0.007–0.019
- Firefly pulse: `sin(tick * 0.055)`
- Cricket chirp: 14 ticks on, 76 ticks off (90 tick cycle)
- Lantern flicker: dual sine at 0.18 and 0.31 speed
- Corner bracket breathing: `sin(tick * 0.04)`
- Scan underline sweep: per-icon speed 0.025–0.06
- Withering shimmer: `sin(tick * 0.1)` and `sin(tick * 0.07)`

---

## 11. Implementation Notes

### Performance

The scene panel is the most expensive area (per-pixel operations for sand texture, rake lines, firefly glow). Consider:
- Pre-rendering the static sand/rake texture into a framebuffer once, then blitting
- Only redrawing the scene when data changes or every Nth frame for animations
- The top and bottom panels change infrequently — redraw only on data updates
- Fireflies and star twinkle are the main per-frame costs

### Sprite Storage

Sprites are defined as character grids with palette lookup tables. On ESP32, store them as `const` arrays in PROGMEM/flash. The reference `.jsx` has exact bitmaps for every sprite.

### Color on ST7789

ST7789 uses RGB565 (16-bit color). Pre-convert palette colors to RGB565 at compile time. Alpha blending requires manual implementation — read-back pixel, blend, write. For performance, consider approximating alpha with dithering or using pre-blended palettes for common alpha levels.

### Memory

At 135×240 RGB565, the framebuffer is ~63KB. ESP32 has plenty of RAM for this. Double-buffering is recommended to avoid tearing.

### The Reference Simulator

The `.jsx` file is designed to be run as a React artifact in claude.ai. It renders at the exact pixel dimensions of the T-Display, and includes interactive controls for switching between stones, changing time of day, and toggling capabilities. Use it as your visual reference — the firmware output should match it pixel-for-pixel where practical.

---

## 12. File Reference

| File | Purpose |
|------|---------|
| `firefly-diorama-v6.jsx` | Pixel-accurate React simulator — visual reference |
| This document | Framework-agnostic implementation specification |

The simulator contains the exact sprite data, palette tables, animation formulas, and layout coordinates. When in doubt, the simulator is the source of truth for visual appearance.
