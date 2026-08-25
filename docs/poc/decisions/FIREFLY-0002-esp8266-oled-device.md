# FIREFLY-0002: ESP8266 OLED Device Support

**Status**: Research Complete / Pending Decision
**Date**: 2026-02-03
**Deciders**: Architecture Team
**Related**: [FIREFLY-0001](FIREFLY-0001-v0-implementation.md), Companion SDK

---

## Context

A new hardware device has been acquired for potential Firefly integration:

**NodeMCU ESP8266 V3 Development Board with 0.96" OLED Display**

This device differs significantly from the current RP2040-Matrix target. This document evaluates its capabilities, viability, and integration requirements.

---

## Hardware Specifications

### NodeMCU ESP8266 V3 + OLED

| Specification | Value |
|---------------|-------|
| **MCU** | Tensilica 32-bit RISC Xtensa LX106 |
| **Clock** | 80-160 MHz (adjustable) |
| **Memory** | 128KB RAM, 4MB Flash |
| **WiFi** | 802.11 b/g/n (2.4 GHz) built-in |
| **USB Chip** | CH340G (USB-to-Serial) |
| **Display** | 0.96" SSD1306 OLED, 128×64 pixels |
| **Display Interface** | I2C (SDA: GPIO12, SCL: GPIO14) |
| **Display Colors** | Monochrome (Yellow-Blue zones) |
| **I/O** | 16 GPIO, 1 ADC, 1 UART, 1 SPI, 1 I2C |
| **Power** | 3.3V operating, 7-12V input |
| **USB Connector** | Type-C |
| **Board Size** | 59mm × 31mm |
| **I2C Address** | Typically 0x3C |

### Comparison with RP2040-Matrix

| Aspect | RP2040-Matrix (Current) | ESP8266 + OLED (New) |
|--------|-------------------------|----------------------|
| **Display Type** | 5×5 RGB LED Matrix | 128×64 Monochrome OLED |
| **Pixels** | 25 (full color) | 8,192 (monochrome) |
| **Output Paradigm** | Color status indicators | Text, icons, graphics |
| **Connectivity** | USB serial only | USB serial + WiFi |
| **USB VID** | 0x2e8a / 0x239a | 0x1a86 (CH340) |
| **Firmware Platform** | CircuitPython | MicroPython (no CircuitPython) |
| **Glanceability** | High (color-coded) | Medium (requires reading) |
| **Information Density** | Low | High |

---

## USB Device Identification

### Current Detection (RP2040)

```rust
// src/firefly/src/serial.rs:196-200
const RP2040_VIDS: [u16; 2] = [0x2e8a, 0x239a];
```

| Device | Vendor ID | Product ID |
|--------|-----------|------------|
| Raspberry Pi RP2040 | `0x2e8a` | varies |
| Adafruit CircuitPython | `0x239a` | varies |

### New Device (CH340)

| Device | Vendor ID | Product ID |
|--------|-----------|------------|
| CH340G/CH340C | `0x1a86` | `0x7523` |
| CH340B (EEPROM) | `0x1a86` | `0x5523` |

### Platform Detection

| Platform | CH340 Device Path |
|----------|-------------------|
| Linux | `/dev/ttyUSB0` (native driver) |
| Windows | `COM3` etc. (requires driver) |
| macOS | `/dev/cu.usbserial-*` |

---

## Firmware Platform Analysis

### CircuitPython Status

**CircuitPython does NOT support ESP8266.**

- Support was dropped in CircuitPython version 4
- Reasons: Memory limitations, limited GPIO, RTOS constraints
- The ESP8266 has only 128KB RAM vs 264KB on RP2040

### MicroPython Status

**MicroPython fully supports ESP8266.**

- Native SSD1306 driver included
- Framebuffer support for graphics operations
- Active community and documentation
- WiFi stack integrated

### Arduino/PlatformIO

Also viable with extensive library support:
- ThingPulse SSD1306 library (framebuffer, transitions)
- LovyanGFX (optimized rendering)
- arduinoWebSockets (WiFi communication)

---

## Display Capabilities

### SSD1306 OLED Characteristics

| Feature | Value |
|---------|-------|
| Resolution | 128×64 pixels |
| Color Depth | 1-bit (on/off) |
| Color Zones | Top 16 rows: Yellow, Bottom 48 rows: Blue |
| Viewing Angle | >160° |
| Interface | I2C @ 400kHz |
| Driver IC | SSD1306 |
| Operating Temp | -30°C to 70°C |

### Hardware Color Zones (Important)

The dual-color SSD1306 displays have **physically different colored OLEDs** on the panel:

```
┌────────────────────────────────────┐
│  YELLOW ZONE (128×16 pixels)       │  ← Top 16 rows
│  Hardware yellow LEDs              │
├────────────────────────────────────┤
│                                    │
│  BLUE ZONE (128×48 pixels)         │  ← Bottom 48 rows
│  Hardware blue LEDs                │
│                                    │
│                                    │
└────────────────────────────────────┘
```

This is **not software-selectable** — the colors are determined by the physical LED placement. This naturally suggests a UI layout with the stone name as a yellow header and status information in the blue body.

### Available Graphics Operations (MicroPython FrameBuffer)

```python
# Basic operations
display.fill(0)              # Clear
display.fill(1)              # Fill white
display.pixel(x, y, color)   # Single pixel

# Shapes
display.rect(x, y, w, h, c)       # Rectangle outline
display.fill_rect(x, y, w, h, c)  # Filled rectangle
display.hline(x, y, w, c)         # Horizontal line
display.vline(x, y, h, c)         # Vertical line
display.line(x1, y1, x2, y2, c)   # Line

# Text
display.text("Hello", x, y, c)    # 8px font

# Scrolling
display.scroll(dx, dy)            # Scroll buffer
display.blit(fbuf, x, y)          # Copy framebuffer
```

---

## WiFi Capabilities

The ESP8266's integrated WiFi opens possibilities not available with USB-only devices:

### Potential Communication Modes

| Mode | Description |
|------|-------------|
| **USB Serial** | Same as RP2040, backward compatible |
| **WiFi Client** | Connect to Stone's network, receive HTTP/WebSocket |
| **WiFi AP** | Device creates hotspot for configuration |
| **mDNS** | Auto-discovery via `firefly-oled.local` |

### Two Operating Modes

The ESP8266's WiFi capability enables two distinct operating modes:

#### Mode 1: USB Serial (via Firefly Companion)

Same architecture as RP2040 — Firefly detects the device over USB serial (CH340) and sends display commands:

```
┌─────────────────────────────────────────────────────────────────┐
│  STONE                                                          │
│                                                                 │
│  ┌─────────┐      ┌─────────────────┐      ┌────────────────┐  │
│  │         │ HTTP │                 │ USB  │                │  │
│  │  Moss   │─────▶│ Firefly Companion│─────▶│ ESP8266 OLED   │  │
│  │ :7185   │      │ (Rust, :718x)   │Serial│ (MicroPython)  │  │
│  │         │      │                 │      │                │  │
│  └─────────┘      └─────────────────┘      └────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Use when**: WiFi not available, centralized control desired, debugging.

#### Mode 2: WiFi Standalone (Direct to Moss)

The ESP8266 connects directly to Moss's SSE stream over WiFi — no Firefly Companion needed:

```
┌─────────────────────────────────────────────────────────────────┐
│  STONE                                                          │
│                                                                 │
│  ┌─────────┐                                                   │
│  │         │                                                   │
│  │  Moss   │ ◄─── SSE /api/v1/stone/presence/stream           │
│  │ :7185   │                                                   │
│  │         │                                                   │
│  └────┬────┘                                                   │
│       │                                                         │
└───────┼─────────────────────────────────────────────────────────┘
        │ WiFi (mDNS discovery)
        │
┌───────▼────────┐
│                │
│  ESP8266 OLED  │  (Standalone, wireless)
│  (MicroPython) │
│                │
└────────────────┘
```

**Use when**: Clean desk setup, display mounted away from Stone, multiple displays.

#### Boot Decision Flow

```
ESP8266 boots
    │
    ▼
Check for WiFi config (ssid/password stored in flash)
    │
    ├─ WiFi configured ──▶ Connect to network
    │                          │
    │                          ▼
    │                     Discover Moss via mDNS
    │                          │
    │                          ▼
    │                     Subscribe to SSE stream
    │                          │
    │                          ▼
    │                     STANDALONE MODE
    │
    └─ No WiFi config ──▶ Wait for USB serial commands
                              │
                              ▼
                         USB SERIAL MODE
```

---

## Proposed Integration Architecture

### Multi-Device Support

To support both RP2040-Matrix and ESP8266-OLED, Firefly needs a device abstraction:

```
┌─────────────────────────────────────────────────────────────────┐
│  Firefly Companion                                              │
│                                                                 │
│  ┌──────────────────┐                                          │
│  │  DeviceManager   │  ◄── Detects devices by VID/PID          │
│  │                  │      or WiFi discovery                   │
│  └────────┬─────────┘                                          │
│           │                                                     │
│  ┌────────┴─────────┬─────────────────┐                        │
│  ▼                  ▼                 ▼                        │
│ ┌──────────────┐ ┌───────────────┐ ┌──────────────┐            │
│ │ RP2040 Driver│ │ ESP8266 Driver│ │ Future...    │            │
│ │              │ │               │ │              │            │
│ │ VID: 2e8a/   │ │ VID: 1a86     │ │              │            │
│ │      239a    │ │ PID: 7523     │ │              │            │
│ │              │ │               │ │              │            │
│ │ Protocol:    │ │ Protocol:     │ │              │            │
│ │ P,x,y,r,g,b  │ │ TEXT,x,y,msg  │ │              │            │
│ │ F,r,g,b      │ │ ICON,name     │ │              │            │
│ │ T,status     │ │ BAR,percent   │ │              │            │
│ └──────────────┘ └───────────────┘ └──────────────┘            │
└─────────────────────────────────────────────────────────────────┘
```

### Device Discovery Flow

```
1. Enumerate serial ports
2. For each port with USB info:
   a. Check VID/PID against known devices
   b. If RP2040 VID → use LED protocol
   c. If CH340 VID → probe with "I" command
      - Response "OK,firefly-v0,rp2040-matrix,*" → LED protocol
      - Response "OK,firefly-oled,esp8266,*" → OLED protocol
3. Optionally scan WiFi for firefly-oled.local
```

---

## Proposed OLED Protocol

### Serial Commands (Text-Based)

Extending the existing V0 protocol pattern:

```
# Information
I                           → OK,firefly-oled,esp8266,128x64
?                           → Command listing

# Display control
CLEAR                       → Clear display
INVERT,0|1                  → Normal/inverted display
CONTRAST,0-255              → Set contrast level
ROTATE,0|180                → Screen orientation

# Text rendering
TEXT,x,y,size,message       → Draw text (size: 1=8px, 2=16px)
TEXTC,y,size,message        → Draw centered text

# Graphics primitives
PIXEL,x,y,0|1               → Set/clear pixel
LINE,x1,y1,x2,y2            → Draw line
RECT,x,y,w,h,fill           → Rectangle (fill: 0=outline, 1=filled)
CIRCLE,x,y,r,fill           → Circle

# Status indicators (mapped to visual)
T,healthy                   → Show checkmark icon + "Healthy"
T,warning                   → Show warning icon + "Warning"
T,error                     → Show X icon + "Error"
T,offline                   → Show disconnected icon

# Predefined icons
ICON,x,y,name               → Draw icon (heart, cloud, gear, etc.)

# Progress/metrics
BAR,x,y,w,h,percent         → Progress bar
METER,x,y,r,percent         → Arc meter

# Animations
A,scroll                    → Scrolling text
A,blink                     → Blinking content
A,fade                      → Fade in/out
S                           → Stop animation

# WiFi configuration (optional)
WIFI,ssid,password          → Connect to network
WIFISTATUS                  → Report connection status
```

### Response Format

```
OK                          → Success
OK,data                     → Success with data
ERR,message                 → Error with description
```

---

## MicroPython Firmware Structure

### File Layout

```
firmware/firefly/micropython/
├── main.py                 # Entry point, serial loop
├── boot.py                 # WiFi configuration (optional)
├── oled.py                 # SSD1306 wrapper with protocol
├── icons.py                # Icon bitmaps
├── fonts/                  # Custom fonts
│   ├── minecraft_8.py
│   └── minecraft_16.py
└── README.md
```

### Sample Implementation Skeleton (Dual-Mode)

```python
# main.py - ESP8266 OLED Firefly Firmware (Dual-Mode)
import machine
import ssd1306
import network
import sys
import time

# Non-standard I2C pins for integrated NodeMCU+OLED board
i2c = machine.I2C(scl=machine.Pin(14), sda=machine.Pin(12))
display = ssd1306.SSD1306_I2C(128, 64, i2c)

# Import custom fonts
from minecraft_8 import draw_text as draw_text_8
from minecraft_16 import draw_text as draw_text_16

def show_splash():
    """Boot splash screen."""
    display.fill(0)
    draw_text_16(display, "Firefly", 20, 0)  # Yellow zone
    draw_text_8(display, "Zen Garden", 30, 28)  # Blue zone
    display.show()

def try_wifi_connect():
    """Attempt WiFi connection from stored config."""
    try:
        with open('wifi.cfg', 'r') as f:
            ssid, password = f.read().strip().split('\n')

        wlan = network.WLAN(network.STA_IF)
        wlan.active(True)
        wlan.connect(ssid, password)

        # Wait up to 10 seconds
        for _ in range(20):
            if wlan.isconnected():
                return wlan.ifconfig()[0]
            time.sleep(0.5)
    except:
        pass
    return None

def run_wifi_mode(ip):
    """Standalone WiFi mode - subscribe to Moss SSE."""
    import urequests
    # Discover Moss via mDNS or configured endpoint
    # Subscribe to /api/v1/stone/presence/stream
    # Parse SSE events and update display
    pass  # Implementation details...

def run_serial_mode():
    """USB serial mode - wait for Firefly Companion commands."""
    while True:
        line = sys.stdin.readline()
        if line:
            response = process_command(line)
            print(response)

def process_command(line):
    """Process serial command."""
    parts = line.strip().split(',')
    cmd = parts[0].upper()
    args = parts[1:] if len(parts) > 1 else []

    if cmd == 'I':
        return 'OK,firefly-oled,esp8266,128x64'
    elif cmd == 'CLEAR':
        display.fill(0)
        display.show()
        return 'OK'
    elif cmd == 'TEXT' and len(args) >= 4:
        x, y, size = int(args[0]), int(args[1]), int(args[2])
        msg = ','.join(args[3:])
        if size == 1:
            draw_text_8(display, msg, x, y)
        else:
            draw_text_16(display, msg, x, y)
        display.show()
        return 'OK'
    elif cmd == 'T' and len(args) >= 1:
        show_status(args[0])
        return 'OK'
    elif cmd == 'WIFI' and len(args) >= 2:
        # Configure WiFi for next boot
        with open('wifi.cfg', 'w') as f:
            f.write(f"{args[0]}\n{args[1]}")
        return 'OK,reboot to connect'
    return 'ERR,unknown command'

def show_status(status):
    """Render status screen."""
    display.fill(0)
    # Yellow zone: stone name
    draw_text_16(display, "stone-name", 0, 0)
    # Blue zone: status
    icon = {'healthy': '✓', 'warning': '!', 'error': 'X'}.get(status, '?')
    draw_text_8(display, f"{icon} {status}", 0, 20)
    display.show()

# Main entry point
print('Firefly OLED - Zen Garden')
show_splash()
time.sleep(1)

ip = try_wifi_connect()
if ip:
    print(f'WiFi connected: {ip}')
    run_wifi_mode(ip)
else:
    print('USB serial mode')
    run_serial_mode()
```

---

## Viability Assessment

### Strengths

| Strength | Impact |
|----------|--------|
| **Rich Information Display** | Can show service names, metrics, logs |
| **WiFi Connectivity** | Wireless status displays, no USB tether |
| **Cost Effective** | ~$5-10 per unit |
| **Integrated Design** | No wiring between MCU and display |
| **MicroPython Support** | Good tooling, familiar syntax |
| **Community** | Extensive tutorials and libraries |

### Challenges

| Challenge | Mitigation |
|-----------|------------|
| **Different Display Paradigm** | Abstract protocol behind device drivers |
| **No CircuitPython** | Use MicroPython (similar API) |
| **Monochrome Only** | Design for contrast, not color |
| **Device Detection** | Extend VID list, add probe logic |
| **I2C Pin Non-Standard** | Document in firmware README |

### Limitations

| Limitation | Severity |
|------------|----------|
| Single ADC (0-1V) | Low (not needed for display) |
| No hardware USB CDC | Low (CH340 is reliable) |
| 160MHz max | Medium (sufficient for OLED) |
| 128KB RAM | Medium (tight for complex animations) |

---

## UI Layout Design

### Recommended Layout

Based on the hardware color zones, the recommended layout:

```
┌─ YELLOW (128×16) ────────────────────┐
│  stone-crystal-forest                │  ← Stone name (16px font)
├─ BLUE (128×48) ──────────────────────┤
│  ◉ thriving          2d 4h       ▲  │  ← Health + uptime (8px)
│  ▓▓▓▓▓▓░░░░░░ 45%                │  │  ← Load bar
│  mongo● redis● ollama●           ▼  │  ← Services with health dots
└──────────────────────────────────────┘
```

### Multi-Page Views

The OLED can cycle through different views (timer or button-triggered):

**Screen 1 — Status (default)**
- Health state with icon
- Uptime
- CPU/memory load bar
- Top services with health indicators

**Screen 2 — Activity**
- Scrolling event log
- "planted mongodb", "tended by leon", etc.

**Screen 3 — Network**
- Pond membership
- Connected stones
- Network topology glyph

### Font Strategy

Two font sizes using the **Minecraft pixel font** (already converted):

| Zone | Font Size | Characters/Line | File |
|------|-----------|-----------------|------|
| Yellow header | 16px | ~9-10 chars | `minecraft_16.py` |
| Blue body | 8px | ~18 chars | `minecraft_8.py` |

The Minecraft font:
- Proportional widths ('i' = 2px, 'm' = 8px at 8pt)
- Clean pixel aesthetic matching reclaimed hardware ethos
- Integer scaling (no anti-aliasing artifacts)

### Status Indicators

Since the font may not include special glyphs, status indicators use geometric primitives:

| Indicator | Rendering |
|-----------|-----------|
| Health dot (●) | `fill_rect(x, y, 4, 4, 1)` |
| Progress bar | Filled vs outlined rectangles |
| Icons | Custom bitmap glyphs (8×8 or 16×16) |

---

## Use Cases

### Primary: Detailed Status Display

Unlike the RP2040-Matrix which provides color-coded ambient status, the OLED can display:

```
┌────────────────────────────┐
│ stone-crystal-forest       │  ← Stone name
│ ────────────────────────── │
│ ✓ Healthy                  │  ← Status with icon
│                            │
│ Services: 3                │  ← Service count
│ CPU: ████████░░ 78%        │  ← Load meter
│ Mem: █████░░░░░ 52%        │
│                            │
│ mongodb redis postgres     │  ← Running services
└────────────────────────────┘
```

### Secondary: Event Log

```
┌────────────────────────────┐
│ Recent Events              │
│ ────────────────────────── │
│ 14:32 mongodb started      │
│ 14:30 redis restarted      │
│ 14:28 health: warning      │
│ 14:15 stone discovered     │
│ 14:00 boot complete        │
└────────────────────────────┘
```

### Tertiary: WiFi Status Board

Mounted away from Stone, receiving updates over WiFi:

```
┌────────────────────────────┐
│    ZEN GARDEN STATUS       │
│ ────────────────────────── │
│ Stones: 3 online           │
│ Services: 12 running       │
│ Storage: 2.4 TB free       │
│                            │
│ Last update: 14:32:15      │
└────────────────────────────┘
```

---

## Implementation Phases

### Phase 1: USB Serial Mode (V0.1)

**Goal**: ESP8266 OLED responds to serial commands, same pattern as RP2040

**Deliverables**:
1. MicroPython firmware with OLED protocol
2. Basic text, icon, status commands
3. Device detection in Firefly Companion (CH340 VID)
4. Protocol negotiation via `I` command response

### Phase 2: Dual-Device Support (V0.2)

**Goal**: Firefly Companion handles both RP2040 and ESP8266 devices

**Deliverables**:
1. Device abstraction layer in Rust
2. Protocol router based on device type
3. Command translation for common operations
4. Documentation for both device types

### Phase 3: WiFi Mode (V1.0)

**Goal**: ESP8266 receives updates over WiFi, no USB required

**Deliverables**:
1. WiFi configuration flow (AP mode for setup)
2. WebSocket client in MicroPython
3. mDNS discovery (`firefly-oled.local`)
4. Companion routes to WiFi or serial based on connection

---

## Decision Options

### Option A: Add as Second Device Type

- Implement OLED support alongside RP2040
- Each serves different use case
- Requires device abstraction layer
- **Recommended**

### Option B: Replace RP2040 Target

- ESP8266 OLED becomes the only Firefly device
- Loses color-coded ambient status
- Simpler implementation
- **Not recommended** (different strengths)

### Option C: Defer

- Focus on V0 RP2040 stability first
- Revisit ESP8266 later
- Risk: Hardware may be forgotten
- **Acceptable** if resources constrained

---

## Files Already Present

The following ESP8266-related files exist in the repository:

```
firmware/firefly/etc/esp8266/
├── font_to_py.py       # Font converter (TTF → MicroPython module)
├── minecraft_8.py      # 8px Minecraft font (760 bytes glyph data)
└── minecraft_16.py     # 16px Minecraft font (2,592 bytes glyph data)
```

### Font Module Details

**minecraft_8.py** (Blue zone text):
- Height: 8px, Max width: 8px
- 95 ASCII characters (printable range)
- Proportional widths: 'i' = 2px, 'A' = 5px, 'W' = 6px
- Stone names use ~50-67px of 128px width

**minecraft_16.py** (Yellow zone header):
- Height: 16px, Max width: 15px
- 95 ASCII characters
- Even "stone-crystal-forest" fits (114px with room to spare)

**font_to_py.py** (Converter tool):
- Uses `freetype-py` to render TTF glyphs
- Outputs MicroPython modules compatible with `framebuf`
- Supports character subsetting to save memory
- Usage: `python font_to_py.py font.ttf 16 output.py`

Total font memory footprint: ~27KB source, compresses to ~10KB as `.mpy` bytecode.

---

## References

### Hardware Documentation
- [NodeMCU ESP8266 User Manual](https://manuals.plus/ae/1005005242283189)
- [ESP8266 Pinout Reference](https://randomnerdtutorials.com/esp8266-pinout-reference-gpios/)
- [CH340 Driver Documentation](https://learn.sparkfun.com/tutorials/how-to-install-ch340-drivers/all)

### Software Documentation
- [MicroPython SSD1306 Tutorial](https://docs.micropython.org/en/latest/esp8266/tutorial/ssd1306.html)
- [MicroPython FrameBuffer](https://docs.micropython.org/en/latest/library/framebuf.html)
- [CircuitPython ESP8266 Deprecation Notice](https://learn.adafruit.com/welcome-to-circuitpython/circuitpython-for-esp8266)

### Libraries
- [ThingPulse SSD1306 Library](https://github.com/ThingPulse/esp8266-oled-ssd1306) (Arduino)
- [LovyanGFX](https://github.com/lovyan03/LovyanGFX) (Optimized graphics)
- [arduinoWebSockets](https://github.com/Links2004/arduinoWebSockets) (WiFi communication)

### Tutorials
- [Random Nerd Tutorials - ESP8266 OLED](https://randomnerdtutorials.com/esp8266-0-96-inch-oled-display-with-arduino-ide/)
- [MicroPython OLED Display](https://randomnerdtutorials.com/micropython-oled-display-esp32-esp8266/)
- [ESP8266 WebSocket Guide](https://newbiely.com/tutorials/esp8266/esp8266-websocket)

---

**Document Status**: Research Complete
**Next Steps**: Architecture decision on Option A/B/C
**Last Updated**: 2026-02-03
