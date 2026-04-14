# Firefly OLED v2 Firmware

Dense icon-based dashboard for the ESP8266 + SSD1306 OLED (128x64, yellow/blue dual-zone).

## Display Layout

```
┌────────────────────────────────────────────┐
│ STONE-AZURE-POOL                      [♥] │  yellow 16px
├─────────────────────────┬──────────────────┤
│ [CPU] │█████░░░░░░│ 42  │  [⚙]  7         │  blue row 1
│ [MEM] │██████████░│ 82  │  [🌐] 1.2K       │  blue row 2
│ [DSK] [⚡] │███░░│  30  │  [◷]  3h         │  blue row 3
└─────────────────────────┴──────────────────┘
```

- **Yellow zone**: stone name (scrolls if long) + health icon (♥ thriving / ⚠ withering / ✕ wilting)
- **Blue zone left**: 3 resource meters (CPU / MEM / DSK) with 8×8 icons, proportional bar, and percentage. A lightning-bolt icon appears next to DSK when a seed-bank is plugged in.
- **Blue zone right**: contextual info (offerings count, network throughput, uptime)

## Files

| File | Purpose |
|------|---------|
| `boot.py` | Boot script (gc cleanup) |
| `main.py` | Serial protocol state machine + command dispatch |
| `firefly_oled_v2.py` | Display driver (dashboard renderer) |
| `icons.py` | 8×8 icon bitmaps (auto-generated from Open Iconic) |
| `OPEN-ICONIC-LICENSE` | MIT license attribution for icon artwork |

## Serial Protocol

Baud: 115200, newline-terminated.

| Command | Purpose |
|---------|---------|
| `I` | Device info (returns `OK,firefly-oled-v2,esp8266,...`) |
| `C` | Clear display |
| `S,<name>` | Set stone name |
| `H,<health>` | Set health (`thriving` / `withering` / `wilting`) |
| `M,<cpu>,<mem>,<disk>,<uptime>` | Update resource metrics |
| `G,<offerings>,<stones>,<net_bps>,<seed_bank>` | Update garden context |
| `D,<cpu>,<mem>,<disk>,<uptime>,<offerings>,<stones>,<net_bps>,<seed_bank>` | All-in-one dashboard update |
| `WIPE-IN,<line1>,<line2>` | Wipe-in animation (event interrupt) |
| `WIPE-OUT,<line1>,<line2>` | Wipe-out animation |
| `BLINK,<count>` | Blink blue zone |
| `PULSE,<count>` | Breathing pulse |
| `R` | Force redraw |

## Installation

Same flow as v1 — upload files via `mpremote`:

```bash
mpremote connect COM28 cp boot.py          :boot.py
mpremote connect COM28 cp main.py          :main.py
mpremote connect COM28 cp firefly_oled_v2.py :firefly_oled_v2.py
mpremote connect COM28 cp icons.py         :icons.py
mpremote connect COM28 cp ../../etc/esp8266/profont_10.mpy :profont_10.mpy
mpremote connect COM28 reset
```

The ESP8266 must already have MicroPython and the `ssd1306` driver installed.

## Regenerating Icons

```bash
# Clone Open Iconic somewhere
git clone --depth 1 https://github.com/iconic/open-iconic.git /tmp/open-iconic

# Regenerate icons.py
python firmware/firefly/tools/convert_icons.py /tmp/open-iconic/png firmware/firefly/micropython/v2/icons.py
```

The converter writes one MSB-left 8-byte bitmap per icon, with ASCII art comments.
To change which Open Iconic icon maps to which concept, edit `ICON_MAP` in the converter.

## Attribution

Icons: [Open Iconic](https://github.com/iconic/open-iconic) by Waybury — MIT License (see `OPEN-ICONIC-LICENSE`).
Font: `profont_10` — bundled separately in `firmware/firefly/etc/esp8266/`.
