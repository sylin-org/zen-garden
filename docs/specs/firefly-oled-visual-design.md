# Firefly OLED Visual Design Specification

**Version**: 1.0.0
**Date**: 2026-02-03
**Status**: Design Complete
**Related**: [FIREFLY-0002](../decisions/FIREFLY-0002-esp8266-oled-device.md)

---

## Executive Summary

This specification defines the complete visual language, information architecture, and interaction design for the Firefly OLED display (ESP8266 + 128×64 SSD1306). It consolidates input from five specialist perspectives: Semiotics, Semantics, UX, DX, and DevOps.

---

## 1. Display Hardware Constraints

### Physical Layout

```
┌────────────────────────────────────────┐
│  YELLOW ZONE (128×16 pixels)           │  ← Hardware yellow LEDs
│  Stone name / Screen title             │     Rows 0-15
├────────────────────────────────────────┤
│                                        │
│  BLUE ZONE (128×48 pixels)             │  ← Hardware blue LEDs
│  Status, metrics, content              │     Rows 16-63
│                                        │
│                                        │
└────────────────────────────────────────┘
```

### Typography

| Zone | Font | Size | Max Chars | Use |
|------|------|------|-----------|-----|
| Yellow header | Minecraft | 16px | 9-10 | Stone name, screen titles |
| Blue body | Minecraft | 8px | ~18-20 | Metrics, labels, content |

### Design Principles

1. **Glanceability**: Health status visible in <1 second from 2-3 meters
2. **Truthfulness**: Visual metaphors match actual system state
3. **Pixel Efficiency**: Every pixel serves a purpose
4. **Joy**: Infrastructure can be delightful

---

## 2. Visual Language & Icon System

### 2.1 Health Status Indicators

The most critical visual element—must be instantly recognizable.

| State | Icon | Pattern | Meaning |
|-------|------|---------|---------|
| **Thriving** | ● (solid circle) | Filled, confident | System healthy |
| **Withering** | ◐ (half circle) | Sparse, hollow | Warnings present |
| **Wilting** | ✗ (X mark) | Broken, contrasting | Critical failure |
| **Resting** | ○ (hollow circle) | Outline only | Dormant/paused |
| **Offline** | ? (question) | Uncertain | Unreachable |

**8×8 Pixel Art:**

```
THRIVING (●):     WITHERING (◐):    WILTING (✗):      RESTING (○):
  ░██░              ░██░              █░░█              ░██░
 ████              ██░░              ░██░              █░░█
 ████              ██░░              ░██░              █░░█
  ░██░              ░██░              █░░█              ░██░
```

### 2.2 Garden Metaphor Mappings

| Zen Concept | Visual Symbol | Glyph Inspiration |
|-------------|---------------|-------------------|
| **Stone** | ■ cube/block | Foundation, solid |
| **Offering** (service) | ⚙ gear | Mechanical, working |
| **Pond** (security) | ≈ ripple | Water, boundary |
| **Seed Bank** (storage) | ⬢ hexagon | Container, archive |
| **Tended** | ✦ sparkle | Celebration, activity |
| **Connected** | ⛓ chain | Networked |
| **Disconnected** | ⊘ broken | Isolated |

### 2.3 Service Status Icons

| Status | Icon | Visual Treatment |
|--------|------|------------------|
| Running | ● | Solid dot (healthy) |
| Stopped | ○ | Hollow dot |
| Installing | ◴ | Spinner animation |
| Maintenance | ⚙ | Gear icon |
| Degraded | ◐ | Half-filled |
| Unknown | ? | Question mark |

### 2.4 Monochrome Semantic Rules

Health state is conveyed through **icons and sparklines**, not pattern density:

| Element | Role |
|---------|------|
| Health icon (●/◐/✗) | Instant status at a glance |
| Sparkline graphs | Historical trend visualization |
| Progress bars | Current percentage (solid fill) |

**Rationale**: Pattern density (solid/sparse/broken fills) adds cognitive load on a 128×64 display. Icons provide instant status; sparklines show trends through their shape. This separation is cleaner and more delightful.

---

## 3. Information Architecture

### 3.1 Data Priority Hierarchy

**Tier 1 - Critical (Always Visible)**
- Stone health indicator (●/◐/✗)
- Stone name
- Active alert (if any)

**Tier 2 - Primary (Status Screen)**
- CPU usage (%)
- Memory usage (%)
- Service count (running/total)
- Uptime

**Tier 3 - Secondary (Detailed Screens)**
- Individual service status
- Disk usage
- Network metrics
- Event log

**Tier 4 - Contextual (On-Demand)**
- Pond membership
- Connected stones
- Hardware capabilities

### 3.2 API Data Sources

| Data | Endpoint | Update Interval |
|------|----------|-----------------|
| Health, metrics | `/api/v1/stone/portrait` | 5 seconds |
| Events | `/api/v1/stone/presence/stream` (SSE) | Real-time |
| Topology | Portrait → horizon | 30 seconds |

### 3.3 Label Guidelines

Constraint: ~18 characters max per line.

```
Health States:
  "thriving"  → "● THRIVING"
  "withering" → "◐ WITHERING"
  "wilting"   → "✗ WILTING"

Metrics:
  "CPU: 45%"     (9 chars)
  "RAM: 62%"     (9 chars)
  "DSK: 49%"     (9 chars)
  "5 services"   (11 chars)

Status:
  "▶ running"    (9 chars)
  "⏹ stopped"   (9 chars)
  "⧗ updating"  (10 chars)
```

---

## 4. Screen Layouts

### 4.1 Screen 1: Status Overview (Primary)

**Purpose**: Quick health check at a glance

```
┌─ YELLOW (128×16) ──────────────────────┐
│ stone-crystal-for                      │
├─ BLUE (128×48) ────────────────────────┤
│ ● THRIVING          2d 4h         [+]  │
│ CPU:  ████████░░░░░░░░░░ 48%           │
│ Mem:  ████████░░░░░░░░░░ 44%           │
│ Svc: 5 running                         │
│                                        │
│ ● mongo  ● redis  ● postgres           │
└────────────────────────────────────────┘
```

**Elements**:
- Stone name (16px, truncate with "…" if needed)
- Health dot + status text + uptime
- CPU/Memory load bars (12 segments)
- Service count
- Top services with health dots
- [+] page indicator

### 4.2 Screen 2: Activity Log

**Purpose**: Recent events timeline

```
┌─ YELLOW (128×16) ──────────────────────┐
│ RECENT EVENTS                          │
├─ BLUE (128×48) ────────────────────────┤
│ 14:32 ✓ mongodb started                │
│ 14:28 ✓ redis restarted                │
│ 14:15 ✓ health: warning→thriving       │
│ 14:00 ✓ boot complete                  │
│ 13:45 ● connection from rake       [+] │
└────────────────────────────────────────┘
```

**Elements**:
- Screen title in yellow header
- Timestamp (HH:MM format)
- Event icon (✓ success, ● activity, ✗ error)
- Event description (truncate to ~30 chars)
- 5 events visible, scroll if more

### 4.3 Screen 3: Network Topology

**Purpose**: Pond and peer stone status

```
┌─ YELLOW (128×16) ──────────────────────┐
│ NETWORK                                │
├─ BLUE (128×48) ────────────────────────┤
│ Pond: garden-cascade    [SECURED]      │
│ ─────────────────────────────────────  │
│ Stones: 3 online                       │
│                                        │
│ ● stone-mossy-brook     (2 svc)        │
│ ● stone-quiet-pond      (5 svc)    [+] │
└────────────────────────────────────────┘
```

**Elements**:
- Pond name and status
- Connected stone count
- Stone list with service counts
- Health indicator per stone

### 4.4 Screen 4: System Metrics

**Purpose**: Detailed resource information

```
┌─ YELLOW (128×16) ──────────────────────┐
│ SYSTEM METRICS                         │
├─ BLUE (128×48) ────────────────────────┤
│ CPU:    ████░░░░░░░░░░░░░░░░ 32%       │
│ Memory: ████████████░░░░░░░░ 51%       │
│ Disk:   ███████░░░░░░░░░░░░░ 27% (2T)  │
│ Uptime: 12d 3h 47m                     │
│ Load:   0.45 0.38 0.32                 │
└────────────────────────────────────────┘
```

### 4.5 Alert Screen (Critical)

**Purpose**: Full-screen alert for critical conditions

```
┌─ YELLOW (128×16) ──────────────────────┐
│ ⚠ CRITICAL ALERT                       │
├─ BLUE (128×48) ────────────────────────┤
│                                        │
│ DISK: ████████████████░ 99%            │
│       !!! OUT OF SPACE !!!             │
│                                        │
│ Services down: 2/5                     │
│                                        │
│ ACTION: Delete old backups             │
└────────────────────────────────────────┘
```

---

## 5. Screen Cycling & Navigation

### 5.1 Auto-Cycle Timer

| Screen | Duration |
|--------|----------|
| Status (1) | 5 seconds |
| Activity (2) | 3 seconds |
| Network (3) | 3 seconds |
| Metrics (4) | 3 seconds |

Total cycle: 14 seconds, then loop.

### 5.2 Button Control (Optional)

If FLASH button is wired:
- **Single press**: Next screen
- **Long press (2s)**: Return to Status
- **Double press**: Pause auto-cycling

### 5.3 Page Indicator

Show `[+]` in bottom-right when more screens exist.

---

## 6. Animation & Transitions

### 6.1 Screen Transitions

Fade transition between screens (200ms):
- Frame 1 (0ms): Screen A at 100%
- Frame 2 (100ms): Crossfade
- Frame 3 (200ms): Screen B at 100%

### 6.2 Status Animations

| State | Animation | Duration |
|-------|-----------|----------|
| Healthy | Static | - |
| Warning | 2Hz pulse | Continuous |
| Critical | 1Hz flash | Continuous |
| Loading | Spinner rotation | Until complete |

### 6.3 Event Highlights

When a metric changes, briefly invert for 500ms to draw attention.

### 6.4 Boot Sequence

```
Frame 1 (0-500ms):    "FIREFLY" fade in (yellow)
Frame 2 (500-1000ms): Stone name appears
Frame 3 (1000-1500ms): "ZEN GARDEN" splash
Frame 4 (1500-2000ms): Fade to main UI
```

---

## 7. Load Bar Rendering

### 7.1 Segment System

Width: 12 segments (96px + 24px for label)
Pattern: `█` = filled, `░` = empty

```
45% = 5.4 segments ≈ 5 filled
████░░░░░░░░ 45%

87% = 10.4 segments ≈ 10 filled
██████████░░ 87%
```

### 7.2 Thresholds

| Metric | Normal | Warning | Critical |
|--------|--------|---------|----------|
| CPU | <80% | 80-95% | >95% |
| Memory | <85% | 85-95% | >95% |
| Disk | <90% | 90-95% | >95% |

Warning: Add `◐` indicator
Critical: Add `✗` indicator and invert bar

---

## 8. Operational Scenarios

### 8.1 Scenario Matrix

| Scenario | Health | Color | Animation | Display |
|----------|--------|-------|-----------|---------|
| Normal operation | THRIVING | Green | Static | Full status |
| High memory | WITHERING | Amber | 2Hz pulse | Highlight metric |
| Service down | WITHERING | Amber | 2Hz pulse | Show offline service |
| Disk full | WILTING | Red | 1Hz flash | Critical alert |
| Docker unavailable | WILTING | Red | 1Hz flash | Critical alert |
| Booting | INITIALIZING | Blue | Spinner | Progress |
| Updating | NOURISHING | Blue | Pulse | Progress bar |
| Offline | OFFLINE | Gray | Static | Last known state |

### 8.2 Alert Priority Levels

| Priority | Visual | Animation | Escalation |
|----------|--------|-----------|------------|
| Info | Cyan | Static | Transient (5s) |
| Notice | Blue | Static | Status bar |
| Attention | Amber | 2Hz pulse | Persistent |
| Warning | Orange | 1Hz pulse | Expand detail |
| Critical | Red | Flash | Full screen |

### 8.3 Refresh Rates

| Data Type | Interval | Rationale |
|-----------|----------|-----------|
| CPU/Memory | 5 seconds | Fast metrics |
| Disk | 10 seconds | Slower change |
| Services | 5 seconds | Status important |
| Topology | 30 seconds | Network overhead |
| Events | Real-time | SSE stream |

---

## 9. Serial Protocol

### 9.1 Command Categories

**Display Control**
```
CLEAR                       # Clear display
INVERT,0|1                  # Normal/inverted
CONTRAST,0-255              # Brightness
```

**Text Rendering**
```
TEXT,x,y,size,"message"     # Draw text (size: 1=8px, 2=16px)
TEXTC,y,size,"message"      # Centered text
```

**Graphics**
```
PIXEL,x,y,0|1               # Set pixel
RECT,x,y,w,h,fill,value     # Rectangle
LINE,x1,y1,x2,y2,value      # Line
```

**High-Level**
```
STATUS,healthy|warning|error|offline    # Full status screen
METER,x,y,w,h,percent,"label"           # Progress bar
ICON,x,y,name                           # Draw icon
```

**Animations**
```
ANIM,name,speed,iterations   # Start animation
STOP                         # Stop animation
```

**Device Info**
```
INFO                         # Returns device capabilities
```

### 9.2 Response Format

```
OK                  # Success
OK,data             # Success with data
ERR,message         # Error
```

### 9.3 Device Identification

```
INFO
→ OK,firefly-oled,esp8266,128x64,dual-zone:yellow:16:blue:48,text|graphics|icons,0.1.0
```

---

## 10. Icon Reference

### 10.1 Status Icons (8×8)

```
CHECKMARK (✓):       WARNING (⚠):         ERROR (✗):
░░█░░░░░             ░░█░░░░░             █░░░█░░░
░█░█░░░░             ░█░█░░░░             ░█░█░░░░
█░░░█░░░             ░███░░░░             ░░█░░░░░
░░░░░█░░             █░░░█░░░             ░█░█░░░░
░░░░░░█░             █░█░█░░░             █░░░█░░░
░░░░░░░░             ░███░░░░             ░░░░░░░░
```

### 10.2 Service Icons (8×8)

```
GEAR (⚙):            SPINNER (◴):         STORAGE (⬢):
░░█░█░░░             ░░██░░░░             ░███░░░░
░█░░░█░░             ░██░░░░░             █░░░█░░░
█░███░█░             ██░░░░░░             █░░░█░░░
█░░░░░█░             ░░░░██░░             █░░░█░░░
░█░░░█░░             ░░░░░██░             ░███░░░░
░░█░█░░░             ░░░░██░░             ░░░░░░░░
```

### 10.3 Network Icons (8×8)

```
CONNECTED (⛓):       DISCONNECTED:        WIFI (≈):
█░░░░█░░             █░░░░█░░             ░░░█░░░░
░█░░█░░░             ░█░░█░░░             ░░███░░░
░░██░░░░             ░░░░░░░░             ░█████░░
░░██░░░░             ░░░░░░░░             ████████
░█░░█░░░             ░█░░█░░░             ░░░░░░░░
█░░░░█░░             █░░░░█░░             ░░░█░░░░
```

---

## 11. Implementation Checklist

### Phase 1: Core Display
- [ ] Establish 128×64 coordinate system
- [ ] Implement text rendering (8px, 16px)
- [ ] Create load bar renderer
- [ ] Build health status icons

### Phase 2: Screens
- [ ] Status Overview screen
- [ ] Activity Log with scrolling
- [ ] Network Topology view
- [ ] System Metrics detail

### Phase 3: Navigation
- [ ] Screen cycling timer
- [ ] Fade transitions
- [ ] Page indicators
- [ ] Optional button control

### Phase 4: Integration
- [ ] SSE event parsing
- [ ] Health state mapping
- [ ] Service status updates
- [ ] Metric polling

### Phase 5: Polish
- [ ] Boot splash sequence
- [ ] Error state handling
- [ ] Event highlighting
- [ ] Performance optimization

---

## 12. Files & Resources

### Existing Files

```
firmware/firefly/etc/esp8266/
├── font_to_py.py       # TTF → MicroPython converter
├── minecraft_8.py      # 8px font (760 bytes)
└── minecraft_16.py     # 16px font (2,592 bytes)
```

### Files to Create

```
firmware/firefly/micropython/
├── main.py             # Entry point, boot logic
├── display.py          # SSD1306 wrapper
├── screens.py          # Screen layouts
├── icons.py            # Icon bitmaps (8×8, 16×16)
├── protocol.py         # Serial command parser
└── wifi.py             # WiFi connectivity (optional)
```

---

## 13. Visual Dictionary

### Health Indication (Simplified)

| Indicator | Role | Example |
|-----------|------|---------|
| Icon | Instant status | ● THRIVING, ◐ WITHERING, ✗ WILTING |
| Sparkline | Trend over time | ╭─╮╭─ (stable = healthy, erratic = concerning) |
| Progress bar | Current value | ████████░░ 78% (solid fill, no patterns) |

**Note**: Pattern density (sparse/alternating fills) removed in favor of icon + sparkline approach. This reduces cognitive load and increases delight.

### Layout Zones

| Zone | Y Range | Purpose |
|------|---------|---------|
| Yellow Header | 0-15 | Stone name, titles |
| Blue Title | 16-23 | Section headers |
| Blue Content | 24-55 | Metrics, lists |
| Blue Footer | 56-63 | Page indicators |

### Spacing Rules

| Element | Margin |
|---------|--------|
| Screen edge | 2px |
| Between lines | 1px |
| Icon to text | 2px |
| Section divider | 4px total |

---

**Document Status**: Design Complete
**Next Steps**: Implementation per Phase 1-5 checklist
**Review Date**: Post-Phase 1 completion
