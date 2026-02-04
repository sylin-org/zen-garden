# Firefly OLED - Delight Addendum

**Version**: 1.1.0
**Date**: 2026-02-03
**Status**: Design Proposal
**Parent**: [firefly-oled-visual-design.md](firefly-oled-visual-design.md)

---

## Overview

This addendum addresses user feedback to bring **delight** to the Firefly OLED display through:

1. **Scrolling stone name banner** (long names scroll smoothly)
2. **Sparkline graphs** (historical metrics visualization)
3. **Event animations** (4-second animations for infrastructure events)

**Connectivity**: USB serial only (WiFi deferred to future phase)

---

## 1. Scrolling Stone Name Banner

### Objective

Stone names can exceed the 9-10 character display limit. Instead of truncating, implement a smooth scrolling animation that reveals the full name.

### Behavior

```
┌─ YELLOW (128×16) ─────────────────────────┐
│ stone-crystal-forest-meditation-grove     │ ← Scrolls
└───────────────────────────────────────────┘

Timeline:
0.0s    Text starts at position 0 (left-aligned)
0.0-2.0s  PAUSE - Allow reading of visible portion
2.0-3.2s  SCROLL LEFT - Ease-in-out, 300px/sec
3.2-5.2s  PAUSE - Allow reading of end portion
5.2-6.4s  SCROLL RIGHT - Return to start
6.4s      Loop
```

### State Machine

```
┌─────────────┐
│ PAUSE_START │ ← Hold 2 seconds
└──────┬──────┘
       │
       ↓
┌─────────────┐
│ SCROLL_LEFT │ ← Ease-in-out cubic, 300px/sec
└──────┬──────┘
       │
       ↓
┌─────────────┐
│ PAUSE_END   │ ← Hold 2 seconds
└──────┬──────┘
       │
       ↓
┌──────────────┐
│ SCROLL_RIGHT │ ← Return to start
└──────┬───────┘
       │
       └──────→ Loop to PAUSE_START
```

### Easing Function

Cubic ease-in-out for natural, satisfying motion:

```python
def ease_in_out(t):
    """t ∈ [0, 1] → smooth acceleration/deceleration"""
    if t < 0.5:
        return 4 * t * t * t
    else:
        return 1 - (-2*t + 2)**3 / 2
```

### Timing Calculations

| Name Length | Width (px) | Fits? | Scroll Duration | Total Cycle |
|-------------|------------|-------|-----------------|-------------|
| "stone" (5) | 32 | Yes | Static | - |
| "stone-forest" (12) | 78 | Yes | Static | - |
| "stone-crystal-forest" (20) | 130 | No | 0.86s | 5.72s |
| "stone-meditation-grove" (22) | 143 | No | 0.90s | 5.80s |
| "stone-crystal-forest-meditation" (31) | 200 | No | 1.09s | 6.18s |

### Integration with Screen Cycling

- Banner animation **freezes** during screen fade transitions
- Resumes from freeze point after transition completes
- Prevents visual clash between two simultaneous animations

---

## 2. Sparkline Graphs

### Objective

Transform static percentage bars into **living visualizations** that show historical trends, bringing delight through seeing infrastructure "breathe."

### Data Model

```python
class MetricsHistory:
    SAMPLE_COUNT = 30  # 2.5 minutes at 5s intervals

    cpu_samples: deque[u8]     # 0-100%
    mem_samples: deque[u8]     # 0-100%
    net_samples: deque[u8]     # 0-100% (normalized)
```

**Memory footprint**: ~100 bytes total (fits easily in ESP8266's 128KB RAM)

### Layout Design

```
┌─ BLUE ZONE (128×48) ────────────────────────────────┐
│                                                      │
│  CPU: ████████░░ 78%   ╭─╮                          │
│                        │ ╰─╮ ╭─╮                    │
│                        ╯   ╰─╯ ╰                    │
│                        └──────────                  │
│  Mem: █████░░░░░ 52%   ╭───────╮                   │
│                        │       ╰──                  │
│                        ╯          ╰                 │
│                        └──────────                  │
│  Net: ██░░░░░░░░ 15%   (optional graph)            │
│                                                      │
└──────────────────────────────────────────────────────┘

Layout dimensions:
- Progress bar: 10 segments (80px)
- Percentage: 4 chars (32px)
- Sparkline: 35×8 pixels (right side)
```

### Auto-Scaling Algorithm

Graphs auto-scale to show meaningful variation:

```python
def auto_scale(samples):
    """Calculate display max, with floor at 50%"""
    sample_max = max(samples) if samples else 50
    return max(sample_max, 50)  # Never scale below 50%
```

**Why 50% floor?** Prevents visual "flattening" when all values are low. A CPU at 5-8% still shows meaningful variation.

### Rendering Algorithm

Line graph with Bresenham's algorithm (no floating point):

```python
def draw_sparkline(display, x, y, width, height, samples):
    if len(samples) < 2:
        return

    max_val = auto_scale(samples)
    x_step = width / (len(samples) - 1)

    prev_x, prev_y = None, None
    for i, sample in enumerate(samples):
        # Invert Y (higher value = lower position)
        y_scaled = 1.0 - (sample / max_val)
        px = x + int(i * x_step)
        py = y + int(y_scaled * height)

        if prev_x is not None:
            display.line(prev_x, prev_y, px, py, 1)
        prev_x, prev_y = px, py
```

### Visual Style

**Line graph** (not bar chart) because:
- Curves convey trends at a glance (↗ rising, ↘ falling)
- More "alive" feeling - infrastructure breathing
- Better use of limited pixels

**Update animation**: New points slide in from right, old points scroll left (smooth, not jumpy)

---

## 3. Event Animations

### Objective

Infrastructure events should feel **alive and joyful**. Each animation is 4 seconds (120 frames @ 30fps).

### Animation 1: Seed-Bank Connected

**Metaphor**: Storage "collecting" like water pooling

```
PHASE 1: SPAWNING (0-0.7s)
Pixels appear at random edge positions

    *     *                   *


                            *

PHASE 2: CONVERGING (0.7-3.0s)
Gravity pulls toward bottom-right



                    *  *
                  * * * *

PHASE 3: SETTLING (3.0-4.0s)
Cluster pulses gently, satisfied



                    * *
                    * * * *
                    (pulsing glow)
```

**Easing**: `ease_in_cubic` - hesitant start, accelerating finish (like droplets)

### Animation 2: Seed-Bank Disconnected

**Metaphor**: Release/dispersal, like air escaping

```
PHASE 1: COMPRESSION (0-0.5s)
Brief squeeze before burst

                    * *
                    * *

PHASE 2: EXPLOSION (0.5-2.0s)
Radial dispersal in 12 rays

*               *               *
        *               *

        *               *
*               *               *

PHASE 3: FADE (2.0-4.0s)
Particles drift to edges and dim

*                               *




```

**Easing**: `ease_out_cubic` - explosive start, smooth settle

### Animation 3: Offering Downloading

**Metaphor**: Data flowing like rain/waterfall

```
PHASE 1: INITIATION (0-0.7s)
Raindrops appear at top

    *       *       *


PHASE 2: CASCADE (0.7-3.0s)
Vertical streams fill from top to bottom

* * * * * * * * * * * *
* * * * * * * * * * * *
* * *   * *     * * * *
* * * * * * * * * * * *

PHASE 3: COMPLETE (3.0-4.0s)
Full coverage, gentle pulse

████████████████████████
████████████████████████
████████████████████████
████████████████████████
(satisfaction glow)
```

**Easing**: `ease_in_out` - natural cascade rhythm

### Animation 4: Offering Installation Complete

**Metaphor**: Celebration sparkle, fireworks

```
PHASE 1: SPARKLE BURST (0-1.0s)
16-point star radiates from center

        *
    *       *
  *     *     *
    *       *
        *

PHASE 2: CONTRACTION (1.0-2.0s)
Sparkles return to center



      * * *
      * * *


PHASE 3: GLOW (2.0-4.0s)
Satisfied pulsing at center


      * * *
      * * *

(gentle pulse)
```

**Easing**: `ease_out_cubic` for burst, `ease_in_cubic` for contraction

### Animation Layering

Animations **overlay** the normal display:
- Status screen remains visible
- Animation pixels XOR or ADD with existing content
- Critical information not obscured (animations in corners/edges)

### Event Queue

If an event fires while another is running:
1. Queue the new event (FIFO, max 5)
2. Current animation completes
3. Next queued animation starts
4. After all complete, return to baseline

---

## 4. Protocol Extensions

### New Commands

#### Scrolling Text

```
SCROLL,text,speed,pause
  text   - String to scroll (max 64 chars)
  speed  - Pixels/second (100-500, default 300)
  pause  - Pause at ends in ms (0-5000, default 2000)

Examples:
  SCROLL,stone-crystal-forest,300,2000
  → OK

  SCROLL_STOP
  → OK
```

#### Sparkline Graphs

```
GRAPH_CREATE,id,max_value
  id        - Graph identifier (cpu, mem, net)
  max_value - Auto-scale ceiling (50-100)

GRAPH_PUSH,id,value
  id    - Graph identifier
  value - New data point (0-100)

GRAPH_SHOW,id
  id - Graph to display

Examples:
  GRAPH_CREATE,cpu,100
  → OK

  GRAPH_PUSH,cpu,78
  → OK

  GRAPH_SHOW,cpu
  → OK
```

#### Event Animations

```
ANIM_EVENT,type
  type - Event animation:
         seed-connect
         seed-disconnect
         offering-download
         offering-complete

Examples:
  ANIM_EVENT,seed-connect
  → OK

  ANIM_EVENT,offering-complete
  → OK
```

#### Device Capabilities (Updated)

```
INFO
→ OK,firefly-oled,esp8266,128x64,scroll|graphs|events,v1.1
```

---

## 5. Visual Mockup: Full Status Screen

With all delight features enabled:

```
┌─ YELLOW ──────────────────────────────────────────┐
│ stone-crystal-forest-meditat...  ← Scrolling     │
├─ BLUE ────────────────────────────────────────────┤
│ ● THRIVING              2d 4h                    │
│                                                   │
│ CPU: ████████░░ 78%    ╭─╮                       │
│                        │ ╰─╮╭                    │
│                        ╯   ╰╯                    │
│                                                   │
│ Mem: █████░░░░░ 52%    ╭───╮                     │
│                        │   ╰──                   │
│                        ╯      ╰                  │
│                                                   │
│ Svc: 5 running                           * * *   │
│                                          * * * * │
│                                          (seed)  │
└───────────────────────────────────────────────────┘
```

---

## 6. Implementation Phases

### Phase 1: Scrolling Banner
- [ ] Implement text measurement function
- [ ] Build state machine (Pause → Scroll → Pause → Scroll)
- [ ] Add easing function
- [ ] Integrate with screen cycling (freeze during transitions)

### Phase 2: Sparkline Graphs
- [ ] Create metrics history buffer (30 samples × 3 metrics)
- [ ] Implement auto-scaling algorithm
- [ ] Build line graph renderer (Bresenham)
- [ ] Add update animation (smooth point entry)

### Phase 3: Event Animations
- [ ] Implement particle system (position, velocity, brightness)
- [ ] Build 4 animation sequences (120 frames each)
- [ ] Add event queue with FIFO processing
- [ ] Integrate with serial protocol

### Phase 4: Polish
- [ ] Tune easing curves for maximum delight
- [ ] Optimize memory usage
- [ ] Add animation layering (XOR mode)
- [ ] Performance testing on ESP8266

---

## 7. Delight Philosophy

These features transform the display from **utilitarian** to **joyful**:

| Before | After |
|--------|-------|
| Truncated names | Smooth scrolling reveals full identity |
| Static numbers | Living graphs show infrastructure "breathing" |
| Instant state changes | Animations convey meaning through motion |
| Cold data | Warm, organic, garden-like presence |

**The goal**: Make operators smile when they glance at their Stone's status. Infrastructure monitoring should bring **joy**, not anxiety.

---

## 8. Architecture Clarification

### Separation of Concerns

**Firmware (ESP8266)**: Pure display buffer management
- Receives graphic primitives (text, rect, line, pixel, fill)
- Manages OLED display buffer
- No knowledge of health states, events, or business logic

**Firefly Companion (Rust)**: Intelligence layer
- Subscribes to Moss SSE event stream
- Maintains metrics history (sparkline data)
- Decides what/when to render
- Translates events to display commands
- Detects connected device type and adapts protocol

### Event Flow

```
Moss SSE Stream → Firefly Companion → Serial Commands → ESP8266 → Display
     │                    │
     │              ┌─────┴─────┐
     │              │ Decides:  │
     │              │ - Screens │
     │              │ - Graphs  │
     │              │ - Anims   │
     │              └───────────┘
```

### Resolution: Pattern Density

**Decision**: Simplified to icons only (Option C)

- Health icons (●/◐/✗) provide instant status
- Sparklines show trends through their shape
- Progress bars use solid fill (no patterns)
- Pattern density removed to reduce cognitive load

---

**Document Status**: Design Accepted
**Last Updated**: 2026-02-03
