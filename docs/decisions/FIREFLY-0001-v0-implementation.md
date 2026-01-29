# FIREFLY-0001: V0 Implementation Strategy

**Status**: Accepted
**Date**: 2026-01-29
**Deciders**: Architecture Team
**Related**: [Firefly Specification](../proposals/firefly.md), Adapter SDK

---

## Context

The Firefly adapter requires two components:

1. **Firefly Adapter** (Rust) - Runs on Stone, implements Moss adapter protocol
2. **RP2040 Firmware** - Runs on Waveshare RP2040-Matrix, controls 5×5 RGB LED matrix

The full specification describes sophisticated visual modes (Firefly, Pond, Normative), complex animations, and a binary serial protocol. Implementing everything at once is high-risk and slow to iterate.

### Hardware

**Target Device**: Waveshare RP2040-Matrix

| Specification | Value |
|---------------|-------|
| MCU | RP2040 (dual-core ARM Cortex-M0+ @ 133MHz) |
| LED Matrix | 5×5 WS2812B RGB LEDs |
| LED GPIO | GPIO16 (directly connected) |
| Connection | USB-C (CDC-ACM serial) |
| Memory | 264KB SRAM, 2MB Flash |
| Size | 23.5mm × 18mm |

## Decision

**Implement V0 using CircuitPython for firmware, with a text-based serial protocol.**

### V0 Scope

| Component | V0 Implementation | Future (V1+) |
|-----------|-------------------|--------------|
| **Firmware Language** | CircuitPython | Rust (embassy-rs) |
| **Serial Protocol** | Text (CSV-like) | Binary (per spec) |
| **Visual Modes** | Normative only | Firefly, Pond, Normative |
| **Animations** | Basic (fill, pixel, blink) | Full spec (breathing, sparkles, waves) |
| **SSE Events** | Manual trigger via commands | Auto-react to Moss events |
| **Configuration** | Hardcoded defaults | TOML config file |

### V0 Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  STONE                                                               │
│                                                                      │
│  ┌─────────┐      ┌─────────────────┐      ┌────────────────────┐   │
│  │         │ HTTP │                 │ Text │                    │   │
│  │  Moss   │─────▶│ Firefly Adapter │─────▶│ RP2040-Matrix      │   │
│  │ :7185   │      │ (Rust, :718x)   │Serial│ (CircuitPython)    │   │
│  │         │      │                 │      │                    │   │
│  └─────────┘      └─────────────────┘      └────────────────────┘   │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### V0 Serial Protocol (Text-Based)

Simple text commands, one per line, terminated with `\n`:

```
# Set single pixel (x, y, r, g, b)
P,2,2,255,0,0

# Fill all pixels (r, g, b)
F,0,255,0

# Clear (all off)
C

# Set brightness (0-100)
B,50

# Play named animation
A,rainbow

# Stop animation
S

# Show status indicator (healthy|warning|error|offline)
T,healthy

# Response from firmware (optional)
OK
ERR,message
```

**Rationale for text protocol:**
- Debuggable with serial monitor (`screen /dev/ttyACM0`)
- No byte-alignment issues
- Easy to implement in CircuitPython
- Can be replaced with binary protocol later (adapter handles translation)

### V0 Adapter Commands

```bash
# Direct LED control
garden-rake hey tell firefly pixel 2 2 ff0000
garden-rake hey tell firefly fill 00ff00
garden-rake hey tell firefly clear
garden-rake hey tell firefly brightness 50

# Status indicators
garden-rake hey tell firefly status healthy
garden-rake hey tell firefly status warning
garden-rake hey tell firefly status error

# Animations
garden-rake hey tell firefly animate rainbow
garden-rake hey tell firefly animate pulse
garden-rake hey tell firefly stop

# Info
garden-rake hey tell firefly info
```

### V0 Status Mapping (Normative Mode)

| Status | LED Display |
|--------|-------------|
| `healthy` | Solid green, all 25 LEDs |
| `warning` | Solid yellow, all 25 LEDs |
| `error` | Blinking red, all 25 LEDs |
| `offline` | All LEDs off |

### V0 Animations

| Animation | Description |
|-----------|-------------|
| `rainbow` | Cycle hue across all LEDs |
| `pulse` | Breathe brightness up/down |
| `chase` | Single LED circles the perimeter |
| `sparkle` | Random LEDs flash white |

---

## Implementation Plan

### Phase 1: CircuitPython Firmware (V0.1)

**Goal**: LED matrix responds to serial commands

**Files**:
- `firmware/circuitpython/code.py` - Main firmware
- `firmware/circuitpython/boot.py` - USB CDC configuration

**Deliverables**:
1. Pixel, fill, clear commands working
2. Brightness control
3. Basic animations (rainbow, pulse)
4. Status indicator modes

**Testing**:
```bash
# Direct serial test (no adapter needed)
screen /dev/ttyACM0 115200
F,0,255,0   # Should turn all LEDs green
P,2,2,255,0,0   # Red pixel in center
C   # Clear
```

### Phase 2: Firefly Adapter (V0.2)

**Goal**: Adapter integrates with Moss, controls firmware via serial

**Files**:
- `src/firefly/Cargo.toml`
- `src/firefly/src/main.rs`
- `src/firefly/src/handler.rs`
- `src/firefly/src/serial.rs`

**Deliverables**:
1. Adapter implements Moss protocol (`--dump-commands`, `/command`, `/shutdown`)
2. Serial port auto-detection (finds RP2040-Matrix)
3. Command translation (Moss commands → serial protocol)
4. Health endpoint

### Phase 3: SSE Integration (V0.3)

**Goal**: Firefly reacts to Moss events automatically

**Deliverables**:
1. Subscribe to Moss SSE endpoint
2. Map events to visual feedback:
   - `stone-online` → green flash
   - `service-started` → brief green pulse
   - `health-degraded` → transition to yellow
3. Configurable event mappings

### Future: Rust Firmware (V1.0)

**When**: After V0 is stable and command set is finalized

**Migration path**:
1. Serial protocol remains the same (adapter unchanged)
2. Rewrite firmware in Rust using embassy-rs
3. Implement full spec animations (breathing, sparkles, waves)
4. Add Firefly/Pond visual modes
5. Optionally switch to binary protocol for efficiency

---

## Consequences

### Positive

- ✅ **Fast Iteration**: CircuitPython allows live editing (save file → firmware restarts)
- ✅ **Debuggable**: Text protocol visible in serial monitor
- ✅ **Low Risk**: Simple V0 validates hardware and command structure
- ✅ **Decoupled**: Firmware can be replaced without changing adapter
- ✅ **Testable**: Can test firmware without adapter, adapter without firmware

### Negative

- ⚠️ **Performance**: CircuitPython is slower than Rust/C for animations
  - *Acceptable*: 25 LEDs at 30fps is trivial even for MicroPython
- ⚠️ **Rewrite Required**: V1 firmware is a full rewrite, not incremental
  - *Acceptable*: Firmware is small (~200 lines), protocol stays same
- ⚠️ **No Complex Animations**: V0 won't have breathing, sparkles, waves
  - *Acceptable*: Basic status indication is the MVP

### Neutral

- ℹ️ **Two Codebases**: CircuitPython firmware + Rust adapter
  - *Rationale*: Different concerns, different optimal languages

---

## Firmware Language Comparison

| Criterion | CircuitPython | Rust (embassy) | C (Pico SDK) |
|-----------|---------------|----------------|--------------|
| **Dev Speed** | ⚡ Fastest | Slower | Medium |
| **Performance** | Adequate | Excellent | Excellent |
| **Debugging** | Easy (REPL) | Harder | Medium |
| **NeoPixel Support** | Built-in | pio-ws2812 crate | Manual |
| **Compile Step** | None | Required | Required |
| **Hot Reload** | Yes | No | No |
| **Code Size** | Larger | Smallest | Small |
| **Consistency w/ Project** | Different | Same as Moss | Different |

**V0 Choice**: CircuitPython (speed of iteration)
**V1 Choice**: Rust (consistency, performance, type safety)

---

## File Locations

```
zen-garden/
├── firmware/
│   └── firefly/
│       ├── circuitpython/      # V0 firmware
│       │   ├── code.py
│       │   ├── boot.py
│       │   └── README.md
│       └── rust/               # V1 firmware (future)
│           ├── Cargo.toml
│           └── src/
└── src/
    └── firefly/                # Moss adapter (Rust)
        ├── Cargo.toml
        ├── src/
        │   ├── main.rs
        │   ├── handler.rs
        │   └── serial.rs
        └── README.md
```

---

## Validation Criteria

V0 is complete when:

1. [ ] `garden-rake hey list` shows firefly adapter
2. [ ] `garden-rake hey tell firefly status healthy` turns LEDs green
3. [ ] `garden-rake hey tell firefly status error` blinks LEDs red
4. [ ] `garden-rake hey tell firefly animate rainbow` shows rainbow animation
5. [ ] Adapter auto-detects RP2040-Matrix serial port
6. [ ] Adapter handles device disconnect/reconnect gracefully

---

## References

- [Firefly Specification](../proposals/firefly.md) - Full visual design spec
- [Adapter Development Guide](../guides/adapter-development.md) - Moss adapter protocol
- [Waveshare RP2040-Matrix Wiki](https://www.waveshare.com/wiki/RP2040-Matrix)
- [CircuitPython NeoPixel Guide](https://learn.adafruit.com/circuitpython-essentials/circuitpython-neopixel)
- [Embassy-rs](https://embassy.dev/) - Rust embedded async framework

---

**Document Status**: Accepted
**Last Updated**: 2026-01-29
