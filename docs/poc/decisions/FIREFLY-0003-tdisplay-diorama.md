# FIREFLY-0003: T-Display Diorama — Presence Protocol Extensions & Device Integration

**Status:** Proposal  
**Date:** 2026-02-19  
**Deciders:** Architecture Team  
**Related:** [FIREFLY-0001](FIREFLY-0001-v0-implementation.md), [FIREFLY-0002](FIREFLY-0002-esp8266-oled-device.md), [PRESENCE-0001](PRESENCE-0001-stone-presence-protocol.md)  
**Spec:** [firefly-tdisplay-diorama-spec.md](../proposals/firefly-ESP32-ST7789/firefly-tdisplay-diorama-spec.md)  
**Simulator:** [firefly-diorama-v6.jsx](../proposals/firefly-ESP32-ST7789/firefly-diorama-v6.jsx)

---

## Context

A third Firefly hardware device has been acquired:

**TENSTAR T-Display ESP32-D0WD — 1.14" ST7789 135×240 Color TFT, CH9102 USB-UART, 16MB Flash**

This is a TTGO/LILYGO T-Display clone. Unlike the monochrome OLED (FIREFLY-0002) or the 5×5 LED matrix (FIREFLY-0001), it has a full-color display capable of rendering a pixel-art diorama — a living miniature zen garden scene that encodes stone health, service status, resource utilization, and ambient environment through visual metaphor.

A detailed rendering specification exists (see **Spec** above) with a pixel-accurate React simulator (see **Simulator**). The diorama requires richer data than the current presence protocol provides: GPU utilization, I/O load, network throughput, seed bank capacity, time of day, and capability flags.

This ADR proposes:
1. Backwards-compatible extensions to the Stone Presence Protocol (PRESENCE-0001)
2. New metrics collection (GPU utilization, I/O load)
3. A compact serial protocol for the T-Display device
4. Pre-rendered asset pipeline for the constrained ESP32 environment
5. Behavior state machine consistent with existing Firefly devices

---

## Decision

### 1. Hardware Profile

| Specification | Value |
|---|---|
| MCU | ESP32-D0WD (dual-core Xtensa LX6, 240 MHz) |
| Display | 1.14" ST7789 TFT, 135×240 pixels, RGB565 (16-bit color) |
| Flash | 16MB |
| RAM | 520KB SRAM (~110KB usable MicroPython heap) |
| USB | CH9102 (USB-UART bridge) |
| Buttons | GPIO35 (Button 1), GPIO0 (Button 2) |
| Battery | JST connector with onboard charging |
| I2C | SDA: GPIO21, SCL: GPIO22 |

**Display pin mapping:**

| Signal | GPIO |
|---|---|
| TFT_MOSI | 19 |
| TFT_SCLK | 18 |
| TFT_CS | 5 |
| TFT_DC | 16 |
| TFT_RST | 23 |
| TFT_BL | 4 |

**USB identification:** VID `0x1a86` (WCH — same vendor as CH340 on ESP8266 OLED). Differentiated from ESP8266 by device info response (see §5).

---

### 2. Presence Protocol Extensions

All extensions are **backwards-compatible** — new fields use `#[serde(default)]` so existing Companions ignore them. No breaking changes to PRESENCE-0001.

#### 2.1 Extended `StoneState` (snapshot payload)

New fields added to `garden_common::presence::StoneState`:

```rust
pub struct StoneState {
    // --- Existing (unchanged) ---
    pub name: String,
    pub health: String,           // "thriving" | "withering" | "wilting"
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub disk_percent: f64,
    pub uptime_seconds: u64,
    pub pond_active: bool,

    // --- NEW: Resource gauges ---
    #[serde(default)]
    pub io_percent: f64,              // Aggregate disk I/O utilization (0–100)
    #[serde(default)]
    pub gpu_percent: f64,             // GPU compute utilization (0–100), 0 if no GPU
    #[serde(default)]
    pub net_rx_bytes_per_sec: u64,    // Network receive rate
    #[serde(default)]
    pub net_tx_bytes_per_sec: u64,    // Network transmit rate

    // --- NEW: Capability flags ---
    #[serde(default)]
    pub has_gpu: bool,                // Any GPU hardware detected
    #[serde(default)]
    pub gpu_active: bool,             // GPU utilization above activity threshold
    #[serde(default)]
    pub is_lantern: bool,             // This stone runs the Lantern registry
    #[serde(default)]
    pub has_cricket: bool,            // Cricket audio companion connected

    // --- NEW: Environment ---
    #[serde(default)]
    pub hour: f64,                    // Local time as decimal hour (14.5 = 2:30 PM)

    // --- NEW: Seed bank summary ---
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub seed_bank: Option<SeedBankSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedBankSummary {
    pub name: String,
    pub used_gb: u64,
    pub total_gb: u64,
}
```

#### 2.2 Extended `StoneLoadUpdatedPayload` (incremental event)

The `stone.load.updated` event currently carries only CPU and memory. Extended:

```rust
pub struct StoneLoadUpdatedPayload {
    // --- Existing ---
    pub cpu_percent: f64,
    pub memory_percent: f64,

    // --- NEW (all default to 0/false for old consumers) ---
    #[serde(default)]
    pub disk_percent: f64,
    #[serde(default)]
    pub io_percent: f64,
    #[serde(default)]
    pub gpu_percent: f64,
    #[serde(default)]
    pub gpu_active: bool,
    #[serde(default)]
    pub net_rx_bytes_per_sec: u64,
    #[serde(default)]
    pub net_tx_bytes_per_sec: u64,
}
```

This is emitted every 5 seconds by `presence_monitor.rs`. The T-Display maps these to the four gauge bars (CPU, MEM, DSK, I/O) and the GPU capability icon animation.

#### 2.3 Backwards Compatibility

| Consumer | Behavior with new fields |
|---|---|
| RP2040-Matrix Firefly | Ignores unknown JSON fields (only reads `cpu_percent`, `memory_percent`) |
| ESP8266-OLED Firefly | Ignores unknown JSON fields (only reads cpu, mem, health, offerings) |
| T-Display Firefly | Reads all fields including new ones |
| Rake `presence` command | Displays new fields if present, graceful fallback |
| Cricket | Ignores load payload entirely (only listens to service/health events) |

---

### 3. New Metrics Collection

#### 3.1 GPU Utilization — Vendor-Agnostic Approach

GPU utilization is **not currently collected** — `detect_gpus()` in `metrics/system.rs` only runs at boot for hardware inventory. We need periodic utilization sampling.

**Design:** A single `get_gpu_utilization()` function that dispatches by vendor, returning a normalized 0–100 percentage. Called on the **fast interval** (5s) alongside CPU/memory.

```rust
/// Collect GPU compute utilization across all detected GPUs.
/// Returns the maximum utilization if multiple GPUs are present.
/// Returns None if no GPU is detected or query fails.
pub fn get_gpu_utilization() -> Option<f32> {
    // Try each vendor in order of likelihood for AI workloads
    if let Some(pct) = query_nvidia_utilization() { return Some(pct); }
    if let Some(pct) = query_amd_utilization()    { return Some(pct); }
    if let Some(pct) = query_intel_utilization()   { return Some(pct); }
    None
}
```

**Per-vendor mechanisms:**

| Vendor | Tool | Query | Platform |
|---|---|---|---|
| NVIDIA | `nvidia-smi` | `--query-gpu=utilization.gpu --format=csv,noheader,nounits` | Linux, Windows |
| AMD | `rocm-smi` / `radeontop` | `rocm-smi --showuse` or parse `/sys/class/drm/card*/device/gpu_busy_percent` | Linux |
| AMD (Windows) | WMI | `Win32_PerfFormattedData_GPUPerformanceCounters` | Windows |
| Intel | `intel_gpu_top` | Parse `/sys/class/drm/card*/gt/gt_cur_freq_mhz` or `xe_gt/` | Linux |
| Intel (Windows) | WMI | GPU engine utilization counters | Windows |

**Fallback chain:**
1. If the vendor-specific tool exists and succeeds → use it
2. If the tool is absent or fails → return `None`
3. `None` means `gpu_percent = 0.0` and `gpu_active = false` in the presence payload

**Activity threshold:** `gpu_active = gpu_percent > 10.0`. This distinguishes "GPU hardware present but idle" from "GPU is working." The diorama uses this to switch the GPU capability icon from blue (idle) to honey (active) with a scanning underline animation.

**Caching:** New field in `AppState`:

```rust
pub gpu_utilization: RwLock<Option<f32>>,
```

Updated on the fast metrics interval. Read by `generate_snapshot()` and the load monitor when emitting `stone.load.updated`.

**Rationale for not using sysinfo crate:** The `sysinfo` crate does not expose GPU utilization on any platform. Vendor CLIs (`nvidia-smi`, `rocm-smi`) are the standard approach used by monitoring tools.

#### 3.2 I/O Utilization

**Design:** Derive from disk I/O byte counters (delta between two samples).

```rust
/// Collect aggregate disk I/O bytes for rate calculation
pub fn get_disk_io_counters() -> (u64, u64) // (read_bytes, write_bytes)
```

| Platform | Source |
|---|---|
| Linux | `/proc/diskstats` — fields 6 (sectors read) and 10 (sectors written), × 512 |
| Windows | WMI `Win32_PerfRawData_PerfDisk_PhysicalDisk` or `sysinfo::Disks` I/O counters |

The `io_percent` is computed as a normalized utilization:

```
io_rate = (delta_read + delta_write) / elapsed_seconds
io_percent = min(100, io_rate / reference_rate * 100)
```

Where `reference_rate` is a configurable threshold (default: 200 MB/s for NVMe, tunable via `GARDEN_IO_REFERENCE_RATE_MBPS`). This gives a meaningful gauge rather than raw throughput.

**Alternative (Linux-only):** Parse disk busy time from `/proc/diskstats` field 13 (weighted time in I/O, milliseconds). Delta / elapsed_ms × 100 = I/O busy percentage. This is what `iostat`'s `%util` uses and is more intuitive. On Windows, fall back to byte-rate normalization.

#### 3.3 Network Metrics in Presence Stream

Network metrics are **already collected** (`get_network_metrics()`, `calculate_network_rate()`) and cached in `state.network_metrics_cache`. They are just not included in the presence snapshot or load events. Wire them through:

```rust
// In generate_snapshot():
let network = state.network_metrics_cache.read().await;
let (net_rx, net_tx) = network.as_ref()
    .map(|n| (n.rx_bytes_per_sec.unwrap_or(0), n.tx_bytes_per_sec.unwrap_or(0)))
    .unwrap_or((0, 0));
```

No new collection code needed.

#### 3.4 Seed Bank Summary

Seed bank lifecycle objects already track `used_bytes` / `capacity_bytes` via health ticks in the disk interval. Expose in the snapshot:

```rust
// In generate_snapshot():
let seed_bank = {
    let banks = state.seed_banks.read().await;
    banks.values().next().map(|b| SeedBankSummary {
        name: b.storage.label.clone(),
        used_gb: b.storage.used_bytes / 1_073_741_824,
        total_gb: b.storage.capacity_bytes / 1_073_741_824,
    })
};
```

Only included when a seed bank is physically connected. `None` otherwise.

#### 3.5 Capability Flags

| Flag | Source | When |
|---|---|---|
| `has_gpu` | `state.hardware_capabilities.gpus.is_empty()` | Snapshot generation |
| `is_lantern` | `state.is_lantern()` (existing method) | Snapshot generation |
| `has_cricket` | `state.companion_registry.has_companion("cricket")` | Snapshot generation |
| `hour` | `chrono::Local::now()` as decimal hour | Snapshot generation + load updates |

---

### 4. T-Display Serial Protocol

#### 4.1 Device Detection

The CH9102 shares VID `0x1a86` with the CH340 on ESP8266. Differentiation occurs after serial connection via the `I` (info) command:

| Device | Info Response |
|---|---|
| ESP8266-OLED | `OK,firefly-oled,esp8266,128x64,...` |
| T-Display | `OK,firefly-tdisplay,esp32,135x240,...` |

The Firefly companion auto-detects the device type from the info response and selects the appropriate command set.

#### 4.2 Command Set

The T-Display uses a compact text protocol optimized for the richer data model. Commands are terminated with `\n`, responses with `\r\n`.

**Full state push** — sent on connect and as periodic heartbeat (every 30s):

```
J,<compact-json>\n
```

JSON payload:

```json
{
  "n": "amber-ridge",
  "h": "thriving",
  "c": 38, "m": 62, "d": 28, "i": 12,
  "g": 45, "ga": true,
  "u": "47d 3h",
  "o": [["mongodb","h"],["redis","h"],["ollama","w"]],
  "hr": 14.5,
  "gp": true, "lnt": false, "ck": true, "pa": false,
  "sb": ["seed-quartz", 32, 64],
  "nr": 1048576, "nt": 524288
}
```

| Key | Type | Maps to |
|---|---|---|
| `n` | string | Stone name (stripped of `stone-` prefix) |
| `h` | string | Health: `"thriving"` / `"withering"` / `"wilting"` |
| `c` | int | CPU percent |
| `m` | int | Memory percent |
| `d` | int | Disk percent |
| `i` | int | I/O percent |
| `g` | int | GPU percent (0 if no GPU) |
| `ga` | bool | GPU active (inferencing) |
| `u` | string | Uptime (friendly) |
| `o` | array | Offerings: `[[name, health_char], ...]` where health_char = `"h"`/`"w"`/`"d"` |
| `hr` | float | Hour as decimal (14.5 = 2:30 PM) |
| `gp` | bool | Has GPU hardware |
| `lnt` | bool | Is Lantern |
| `ck` | bool | Has Cricket |
| `pa` | bool | Pond active |
| `sb` | array\|null | Seed bank: `[name, used_gb, total_gb]` or omitted |
| `nr` | int | Net RX bytes/sec |
| `nt` | int | Net TX bytes/sec |

**Incremental load update** — every 5s:

```
L,<cpu>,<mem>,<disk>,<io>,<gpu>,<gpu_active_flag>\n
```

Example: `L,38,62,28,12,45,1\n` (20 bytes). This is the primary heartbeat.

**Discrete event commands:**

| Command | Meaning | Example |
|---|---|---|
| `H,<health>` | Health changed | `H,withering` |
| `+,<name>,<health>` | Service started | `+,ollama,h` |
| `-,<name>` | Service stopped | `-,redis` |
| `T,<client>,<host>` | Stone tended | `T,rake,leo-laptop` |
| `SD,<name>,<used>,<total>` | Seed bank detected | `SD,seed-quartz,32,64` |
| `SR` | Seed bank removed | `SR` |
| `I` | Request device info | `I` |
| `C` | Clear display | `C` |
| `B,<0-100>` | Set backlight brightness | `B,80` |

**Bandwidth consideration:** At 115200 baud (11.5 KB/s), a full `J` push (~250 bytes) takes ~22ms. The 5-second `L` update is ~20 bytes (<2ms). Well within capacity.

#### 4.3 Why Text Protocol, Not Binary

Consistency with existing Firefly devices. All three device types (RP2040, ESP8266, ESP32) share the same text-over-serial pattern, debuggable with any terminal emulator. The bandwidth overhead is negligible at 115200 baud for this data volume.

---

### 5. Firmware Architecture

#### 5.1 Platform

**MicroPython** with the [russhughes/st7789_mpy](https://github.com/russhughes/st7789_mpy) C driver compiled into the firmware image. This driver provides hardware-accelerated `blit_buffer()`, `fill_rect()`, `pixel()`, `line()`, `hline()`, `vline()`, and text rendering via SPI DMA.

**Pre-compiled firmware:** The st7789_mpy project provides a ready-to-flash `T-DISPLAY` firmware binary (MicroPython v1.20 + ESP-IDF v4.4 + ST7789 C driver + frozen font modules).

**Display initialization:**

```python
import machine, st7789
spi = machine.SPI(2, baudrate=40000000, sck=machine.Pin(18), mosi=machine.Pin(19))
tft = st7789.ST7789(spi, 135, 240,
    reset=machine.Pin(23, machine.Pin.OUT),
    dc=machine.Pin(16, machine.Pin.OUT),
    cs=machine.Pin(5, machine.Pin.OUT),
    backlight=machine.Pin(4, machine.Pin.OUT))
tft.init()
```

#### 5.2 Pre-Rendered Asset Pipeline

The diorama spec calls for pixel-level operations (sand grain texture, concentric rake ellipses, stone shadows) that are too expensive to compute per-frame on the ESP32 in Python. These are **pre-rendered at build time on the host PC** and stored as binary assets.

**Three storage tiers:**

| Tier | Format | Storage | RAM Cost | Use Case |
|---|---|---|---|---|
| Frozen modules | `.py` with `bytes` constants | Flash (in firmware image) | **0 bytes** — memory-mapped | Sprite pixel data, palettes, LUTs |
| Filesystem files | `.rgb565` binary blobs | LittleFS partition (~12MB on 16MB flash) | Loaded into RAM on demand | Pre-rendered textures (sand, star field) |
| `.mpy` modules | Pre-compiled bytecode | LittleFS partition | Loaded into RAM on import | Animation logic, utility functions |

**Build pipeline** (`tools/build_firefly_assets.py`, runs on PC):

```
Source                          Output                       Stored As
──────────────────────────────────────────────────────────────────────
Sprite char grids + palettes  → RGB565 frozen bytes modules → Frozen in firmware
Sand texture (day/dusk/night) → .rgb565 binary files        → /assets/ filesystem
Star field (135×50)           → .rgb565 binary file          → /assets/ filesystem
Rake line overlay             → .rgb565 binary file          → /assets/ filesystem
Sin/cos lookup table          → Frozen int tuple             → Frozen in firmware
Gauge color ramp (100 entries)→ Frozen RGB565 tuple          → Frozen in firmware
```

**Compositing model:** Three independent panel framebuffers:

| Panel | Dimensions | Buffer Size | Update Frequency |
|---|---|---|---|
| Top (identity + gauges) | 130 × 96 | 24,960 bytes | On data change only |
| Scene (diorama) | 130 × 72 | 18,720 bytes | Every frame (~15 fps) |
| Bottom (offerings + icons) | 130 × 72 | 18,720 bytes | On data change only |

Scene rendering per frame:
1. Fill sky gradient (6 `fill_rect` calls)
2. `blit_buffer` pre-rendered star field (1 call, dimmed by time-of-day)
3. Draw moon (~30 pixels)
4. `blit_buffer` pre-rendered sand/pond texture (1 call)
5. `blit` background fireflies with transparency key (N calls)
6. `blit_buffer` stone sprite (1 call)
7. `blit` foreground fireflies
8. `blit_buffer` scene buffer → display (1 SPI burst)

Most per-pixel work happens in steps 5 and 7 (firefly glow), which are small radii (2–3.5px per firefly). Target: 15–20 fps.

#### 5.3 Memory Budget

| Allocation | Bytes | Notes |
|---|---|---|
| MicroPython heap baseline | ~40,000 | Interpreter + imports |
| Scene framebuffer | 18,720 | 130 × 72 × 2 |
| Top panel buffer | 24,960 | 130 × 96 × 2 (or direct-draw) |
| Bottom panel buffer | 18,720 | 130 × 72 × 2 (or direct-draw) |
| Sand texture (loaded) | 2,860 | 130 × 11 × 2 |
| Star field (loaded) | 13,000 | 130 × 50 × 2 |
| Data structures / state | ~4,000 | Offerings list, config, etc. |
| **Total** | **~122,260** | Fits in ~110–160KB usable heap |

Top and bottom panels can be direct-drawn to the display instead of buffered (saving ~43KB) since they update infrequently. Only the scene panel needs a framebuffer for animation.

---

### 6. Behavior State Machine

Maintains consistency with the existing OLED (FIREFLY-0002) behavior model: loss of serial communication returns to an ambient mode; data arrival resumes rich display.

```
                    ┌──────────┐
                    │   BOOT   │  "ZEN GARDEN / Firefly" splash
                    │  (1.5s)  │  Identity-hue gradient background
                    └────┬─────┘
                         │ timeout
                         ▼
    ┌─────────────── NO_COMM ◀──────────────────────┐
    │                (ambient)                       │ serial timeout (10s)
    │  Midnight sky + drifting fireflies             │
    │  No data panels, no stone, no gauges           │
    │  Pure ambient — looks like a tiny night scene  │
    └────┬──────────────────────────────────────────┘
         │ first 'J' command received
         ▼
    ┌──────────┐
    │ CONNECT  │  Fireflies converge to scene positions
    │  (dash)  │  Panels fade in from identity-hue
    │  ~1.5s   │  Same semantic as OLED "dash" transition
    └────┬─────┘
         │ dash animation completes
         ▼
    ┌──────────┐
    │   IDLE   │  Full diorama rendering from cached data
    │ (render) │  Scene animates at 15 fps
    │          │  Panels redraw on data change
    └──────────┘
```

#### 6.1 NO_COMM — Midnight Sky Default

When the Firefly has no serial data (device just powered on, or connection lost after timeout):

- **Sky:** Full midnight palette (`#05061a` → `#0a0d22` → `#10132a`)
- **Stars:** 26 deterministic stars twinkling
- **Moon:** Real lunar phase via Conway's algorithm
- **Fireflies:** 3 ambient fireflies drifting slowly (no service association)
- **Ground:** Not rendered (scene fills with sky)
- **Panels:** Not rendered (screen is the scene only, edge to edge)

This creates a peaceful night-sky display that works as ambient decor even without a connected stone. The identity bar is not shown (no identity without data). Same concept as the OLED's floating fireflies, but richer in color.

#### 6.2 CONNECT — Dash Transition

Triggered by receiving the first `J` (full state) command:

1. Ambient fireflies accelerate toward their assigned scene positions (same concept as OLED dash — fireflies "dash" offscreen)
2. Identity bar slides in from left edge
3. Top panel fades in (name, gauges appear)
4. Scene transitions: ground appears, stone sprite fades in, sky adjusts to current hour
5. Bottom panel fades in (offerings, icons)
6. Duration: ~1.5s, then state → IDLE

#### 6.3 IDLE — Full Rendering

Continuous diorama rendering as specified in the [diorama spec](../proposals/firefly-ESP32-ST7789/firefly-tdisplay-diorama-spec.md). Scene animates at 15 fps. Top and bottom panels redraw only when data changes.

#### 6.4 Event Overlays (in IDLE state)

These are **transient visual effects** layered on top of the normal rendering. They match the semantic events that the OLED Firefly already handles, with richer visual expression:

| SSE Event | Visual Effect | Duration |
|---|---|---|
| `stone.tended` | Stone sprite pulses bright (2× lightness), lantern flares if present | ~2s |
| `service.started` | New firefly spawns as bright flash at stone center, drifts to position | ~1.5s |
| `service.stopped` | Firefly flickers rapidly, then fades out | ~1s |
| `storage.detected` | Seed bank icon blooms in bottom panel (scale 0→1, corner brackets flash) | ~1s |
| `storage.removed` | Seed bank icon dims and fades | ~0.5s |
| `stone.health.changed` → withering | Scene cross-fades to fire mode (spec §6) over ~2s | Persistent |
| `stone.health.changed` → thriving | Fire fades, scene rebuilds (sand/stone/sky fade in) | ~3s |
| `job.started` | Subtle scanning underline appears beneath relevant capability icon | Duration of job |
| `job.completed` | Capability icon corner brackets do a bright flash | ~0.5s |

#### 6.5 Reconnect After Loss

When serial data resumes after a NO_COMM period (same pattern as OLED reconnection):

1. Companion resends cached `J` snapshot
2. T-Display transitions from NO_COMM → CONNECT → IDLE
3. All state is rebuilt from the snapshot

The Firefly companion (Rust) caches the last-known state and resends it on reconnection, exactly as it does for the OLED today.

---

### 7. Companion Integration

#### 7.1 Device Type in serial.rs

Add `TDisplay` variant to the existing device type enum:

```rust
pub enum FireflyDevice {
    Rp2040Matrix,   // 5×5 RGB LED (CircuitPython)
    Esp8266Oled,    // 128×64 monochrome OLED (MicroPython)
    Esp32TDisplay,  // 135×240 color TFT (MicroPython + st7789)
}
```

Detection: same `0x1a86` VID, differentiated by `I` command response containing `firefly-tdisplay`.

#### 7.2 Event Handler Extension

The existing `FireflyEventHandler` in `events.rs` dispatches by device type. Add a third arm for T-Display that maps SSE events to the compact serial commands (§4.2).

The new fields from the extended `StoneLoadUpdatedPayload` (§2.2) are parsed and forwarded:

```rust
// Existing: cpu_percent, memory_percent
// New: disk_percent, io_percent, gpu_percent, gpu_active, net_rx, net_tx
```

For RP2040 and OLED devices, the new fields are simply ignored (their handlers never read them).

#### 7.3 Animation Engine

The T-Display does **not** need the companion-side animation engine that the RP2040 uses. All animation runs on the ESP32 itself (firefly drift, star twinkle, breathing dot, etc.). The companion only pushes data; the device renders autonomously.

This is a deliberate architectural difference:
- **RP2040:** Dumb display, companion drives animation via rapid `P,x,y,r,g,b` commands
- **OLED:** Semi-smart, companion sends state commands, device draws static screens
- **T-Display:** Smart display, companion sends state data, device runs full animation loop

---

### 8. Installer Extension

`NewFirefly.ps1` gains a third device path:

| Step | T-Display |
|---|---|
| Detection | Scan for CH9102 VID, confirm via info response or boot-mode GPIO0 |
| Firmware | Flash `st7789_mpy` T-Display firmware via `esptool.py` |
| Assets | Upload pre-rendered `.rgb565` files to `/assets/` via `mpremote` |
| Application | Upload `main.py`, `diorama.py`, `sprites.py` (or `.mpy` equivalents) |
| Test | Send `I` command, verify `firefly-tdisplay` response |

**Cache directory:** `~/.zen-garden/firefly-cache/tdisplay/`

---

### 9. Data Flow Summary

```
                         ┌──────────────────────────────────────┐
                         │           Moss (Stone)               │
                         │                                      │
                         │  metrics_collector (5s/30s)          │
                         │  ├── get_fast_metrics() → CPU, MEM  │
                         │  ├── get_gpu_utilization() → GPU  ←── NEW
                         │  ├── get_disk_io_counters() → I/O ←── NEW
                         │  ├── get_network_metrics() → NET     │
                         │  └── get_storage_metrics() → DISK    │
                         │                                      │
                         │  presence_monitor (5s)               │
                         │  └── emit stone.load.updated         │
                         │      {cpu, mem, disk, io, gpu, net}  │
                         │                                      │
                         │  generate_snapshot()                 │
                         │  └── StoneState + offerings + flags  │
                         │      + seed_bank + hour              │
                         └──────────┬───────────────────────────┘
                                    │ SSE (presence.snapshot +
                                    │      stone.load.updated +
                                    │      service.* + storage.*)
                                    ▼
                         ┌──────────────────────────────────────┐
                         │     Firefly Companion (Rust)         │
                         │                                      │
                         │  SSE Client → FireflyEventHandler    │
                         │  ├── RP2040:  P,x,y,r,g,b commands  │
                         │  ├── OLED:    S/H/M/R commands       │
                         │  └── TDisplay: J/L/H/+/-/T commands  │← NEW
                         └──────────┬───────────────────────────┘
                                    │ Serial (115200 baud)
                                    ▼
                         ┌──────────────────────────────────────┐
                         │     ESP32 T-Display (MicroPython)    │← NEW
                         │                                      │
                         │  Parse serial → update state cache   │
                         │  Render loop (15 fps):               │
                         │  ├── Top panel (on data change)      │
                         │  ├── Scene (every frame)             │
                         │  └── Bottom panel (on data change)   │
                         │                                      │
                         │  State machine:                      │
                         │  BOOT → NO_COMM → CONNECT → IDLE    │
                         └──────────────────────────────────────┘
```

---

### 10. Implementation Phases

| Phase | Scope | Effort | Dependencies |
|---|---|---|---|
| **Phase 1** | Presence protocol extensions (§2) | Small | None — backwards compatible |
| **Phase 2** | GPU utilization collection (§3.1) | Medium | Need NVIDIA/AMD/Intel test hardware |
| **Phase 3** | I/O utilization collection (§3.2) | Medium | Platform-specific implementation |
| **Phase 4** | T-Display serial protocol + companion device type (§4, §7) | Medium | Phase 1 |
| **Phase 5** | Asset build pipeline (§5.2) | Medium | Sprite/palette finalization |
| **Phase 6** | ESP32 firmware — boot + NO_COMM + serial parser | Medium | Phase 4, Phase 5 |
| **Phase 7** | ESP32 firmware — full IDLE rendering | Large | Phase 6 |
| **Phase 8** | Installer support (§8) | Small | Phase 6 |

Phases 1–3 benefit all Companions (not just T-Display) and can proceed independently. Phase 2 can be incremental — start with NVIDIA (most common for AI workloads), add AMD and Intel as hardware becomes available for testing.

---

## Consequences

### Positive

- **Richer ambient awareness.** The diorama provides at-a-glance status from across a room — color, motion, and spatial encoding instead of text readout.
- **GPU visibility.** GPU utilization has been a blind spot. Adding it to the presence protocol benefits all consumers (Rake, Portrait, future Companions), not just the T-Display.
- **Backwards compatible.** All protocol extensions use `serde(default)`. Existing Companions, Rake commands, and Lantern endpoints continue to work without modification.
- **Consistent architecture.** Same companion-sdk → serial → device pattern as RP2040 and OLED. Same state machine semantics (BOOT → NO_COMM → CONNECT → IDLE). Same event vocabulary.

### Negative

- **GPU collection shell-outs.** Calling `nvidia-smi` every 5 seconds adds a process spawn. Mitigation: the call is fast (~10ms) and already used at boot for detection. If latency is a concern, increase to 10s interval or use NVML library binding.
- **I/O normalization is imprecise.** Converting raw I/O bytes/sec to a percentage requires a reference rate that varies by hardware. The gauge will be directionally correct but not perfectly calibrated. Acceptable for ambient display — the goal is "busy vs. idle", not precise measurement.
- **ESP32 MicroPython performance ceiling.** Per-pixel alpha blending in Python is slow. The pre-rendering pipeline and `blit_buffer` approach mitigate this, but some visual effects (firefly glow radial falloff) may need to be simplified vs. the React simulator.
- **16MB flash variant dependency.** The standard T-Display has 4MB flash. The TENSTAR variant has 16MB. The asset pipeline assumes ample filesystem space. For 4MB variants, reduce pre-rendered assets and compute more at runtime (possible but slower).

### Neutral

- **No WiFi used.** The T-Display has WiFi, but we deliberately don't use it. Communication is serial-over-USB, consistent with other Firefly devices. WiFi could enable standalone operation in the future but adds complexity (network config, security, power) with no current need.
- **Buttons unused.** GPIO35 and GPIO0 buttons are available for future interactions (e.g., cycle display modes, toggle backlight, scroll offerings). Not in scope for initial implementation.
