# Presence Protocol Implementation Spec

**Date:** January 2026  
**Target:** Moss 0.2.x release  
**Hardware Testbed:** Wyse 5070 (internal speakers), ESP8266 OLED, RP2040 5×5 Matrix

---

## Executive Summary

This spec defines the implementation plan for the Stone Presence Protocol (PRESENCE-0001). We'll build the Moss endpoints first, create a test harness, then incrementally add Companion support.

**Key insight:** Start with what we can test immediately (Wyse 5070 speakers), prove the protocol works, then expand to external hardware.

---

## Specialist Team Assessment

### Omar Al-Rashid — Distributed Systems

*"Build the foundation first. Moss endpoints are the protocol. Everything else is consumers."*

**Recommendation:**
1. Implement `/api/v1/presence/stream` with full event vocabulary
2. Create a CLI test consumer (`garden-rake presence watch`)
3. Validate with real events before touching hardware

### Dr. Raj Patel — Software Architecture

*"Separation of concerns is paramount. Moss emits events. Companions are separate binaries."*

**Recommendation:**
- Companions should be standalone executables, not Moss plugins
- Installation can be Moss-assisted but execution is independent
- Each Companion type gets its own repository/package

### Elena Vasquez — Sensory Design

*"Start with audio. It's the fastest feedback loop for developers."*

**Recommendation:**
- Cricket on Wyse 5070 is the first testbed
- Audio feedback during development is more intuitive than LEDs
- Build a minimal "cricket-dev" tool that just beeps on events

### Marcus Chen — Mnemonics

*"The rake command should feel like the protocol. Same vocabulary."*

**Recommendation:**
- `garden-rake presence watch` — see events flow
- `garden-rake presence test` — emit synthetic events
- `garden-rake presence subscribers` — who's listening

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│  MOSS (Stone)                                                   │
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌────────────────┐  │
│  │ Domain Events   │──│ Presence Bridge │──│ SSE Endpoint   │  │
│  │ (existing)      │  │ (new)           │  │ /presence/     │  │
│  └─────────────────┘  └─────────────────┘  └────────────────┘  │
│                                                   │             │
└───────────────────────────────────────────────────┼─────────────┘
                                                    │ SSE
                    ┌───────────────────────────────┼───────────────┐
                    │                               │               │
              ┌─────▼─────┐  ┌───────────┐  ┌──────▼──────┐       │
              │  Rake     │  │  Cricket  │  │  Firefly    │       │
              │  (debug)  │  │  (audio)  │  │  (LED)      │       │
              └───────────┘  └───────────┘  └─────────────┘       │
                                                                   │
              ┌───────────────────────────────────────────────────┘
              │  Companions (separate processes)
              └───────────────────────────────────────────────────
```

---

## Milestone Plan

### Phase 1: Moss Presence Endpoints (Week 1-2)

**Goal:** Emit presence events from Moss. Validate with Rake.

#### 1.1 Presence Stream Endpoint

**File:** `src/moss/src/api/v1/presence.rs`

```
GET /api/v1/presence/stream?Companion=<name>&version=<ver>
Accept: text/event-stream
```

Implementation:
- Create new API module `presence.rs`
- Add route to router
- Emit `presence.snapshot` on connect
- Bridge existing domain events to presence events
- Send `presence.heartbeat` every 30 seconds
- Track subscribers in `AppState`

**Deliverables:**
- [ ] `GET /api/v1/presence/stream` returns SSE
- [ ] `presence.snapshot` sent on connect
- [ ] `presence.heartbeat` sent every 30s
- [ ] Companion query params parsed and tracked
- [ ] Existing `ServiceEvent` mapped to presence events

#### 1.2 Subscribers Endpoint

**File:** `src/moss/src/api/v1/presence.rs`

```
GET /api/v1/presence/subscribers
```

Implementation:
- Track SSE connections in `Arc<RwLock<Vec<Subscriber>>>`
- Clean up on disconnect
- Return current list

**Deliverables:**
- [ ] `GET /api/v1/presence/subscribers` returns JSON
- [ ] Subscribers appear on connect
- [ ] Subscribers disappear on disconnect

#### 1.3 Presence Bridge

Map existing domain events to presence vocabulary:

| Domain Event | Presence Event |
|--------------|----------------|
| `ServiceEvent::Started` | `service.started` |
| `ServiceEvent::Stopped` | `service.stopped` |
| `ServiceEvent::InstallCompleted` | `service.sprouted` |
| `ServiceEvent::Removed` | `service.uprooted` |
| `ServiceEvent::HealthChanged` | `service.health.changed` |

New events to add:
- `stone.load.updated` — Periodic (every 5s)
- `stone.health.changed` — Computed from load thresholds
- `stone.tended` — Emit when Rake connects with `?intent=tend`

**Deliverables:**
- [ ] Presence event structs in `garden_common`
- [ ] Bridge translates domain events → presence events
- [ ] Load monitoring task emits `stone.load.updated`
- [ ] Health computation (CPU >80% = withering, >95% = wilting)

#### 1.4 Rake Presence Commands

**File:** `src/rake/src/commands/presence/`

```bash
# Watch presence stream (debug tool)
garden-rake presence watch
garden-rake presence watch --stone stone-crystal-forest

# Show current subscribers
garden-rake presence subscribers
garden-rake presence subscribers --stone stone-crystal-forest

# Emit test event (for Companion development)
garden-rake presence test stone.tended
garden-rake presence test service.started --service mongodb
```

**Deliverables:**
- [ ] `presence watch` streams events to terminal
- [ ] `presence subscribers` shows connected Companions
- [ ] `presence test` emits synthetic events (dev mode only)

---

### Phase 2: Test Harness (Week 2-3)

**Goal:** Comprehensive testing without hardware.

#### 2.1 Mock Event Generator

Script or Rake command to generate event sequences:

```bash
# Simulate typical stone lifecycle
garden-rake presence simulate lifecycle

# Simulate load spike
garden-rake presence simulate load-spike

# Simulate service failure
garden-rake presence simulate failure --service mongodb
```

Sequences:
- `lifecycle`: snapshot → services start → activity → idle
- `load-spike`: normal → high load → withering → recovery → thriving
- `failure`: healthy → unhealthy → service stopped → restarted → healthy

#### 2.2 Protocol Compliance Tests

**File:** `tests/presence-protocol.ps1` or Rust integration tests

Tests:
- [ ] Snapshot sent on connect
- [ ] Heartbeat every 30s (±5s tolerance)
- [ ] Subscriber appears in list
- [ ] Subscriber disappears on disconnect
- [ ] Events include required fields
- [ ] Timestamps are ISO 8601
- [ ] Reconnection gets fresh snapshot

#### 2.3 Load Testing

- Connect 10 Companions simultaneously
- Disconnect/reconnect rapidly
- Emit 100 events/second
- Verify no memory leaks over 1 hour

---

### Phase 3: Audio Companion (Cricket-Dev) (Week 3-4)

**Goal:** Prove the protocol with audio on Wyse 5070.

#### 3.1 Minimal Cricket Implementation

**Repository:** `zen-garden/garden-cricket` (or in-tree `src/cricket/`)

Start minimal:
- Connect to presence stream
- On `service.started` → play beep
- On `stone.tended` → play chime
- On `stone.health.changed` to `withering` → play warning tone

No soundscape yet. Just proof of concept.

**Tech stack:**
- Rust (consistency with Moss/Rake)
- `rodio` crate for audio playback
- Embed a few simple WAV files

#### 3.2 Audio Files

Minimal set:
- `beep.wav` — Service started
- `chime.wav` — Stone tended
- `warning.wav` — Health degraded
- `chirp.wav` — Activity

License: CC0 from Freesound.org

#### 3.3 Installation on Wyse 5070

Manual for Phase 3:
```bash
# On the Wyse 5070 stone
sudo apt install garden-cricket  # or copy binary

# Configure
echo 'moss_url = "http://localhost:7185"' > /etc/zen-garden/cricket.toml

# Start
sudo systemctl enable garden-cricket
sudo systemctl start garden-cricket
```

Verify:
```bash
# From workstation
garden-rake tend to stone-wyse-5070
# Should hear chime from Wyse speakers
```

---

### Phase 4: LED Matrix Companion (Firefly) (Week 5-6)

**Goal:** Visual presence on RP2040 5×5 matrix.

#### 4.1 Firmware (MicroPython or Rust)

**Repository:** `zen-garden/garden-firefly`

Two components:
1. **Firmware** — Runs on RP2040, receives serial commands, drives LEDs
2. **Service** — Runs on Stone, connects to presence stream, sends serial

#### 4.2 Service Implementation

Similar to Cricket:
- Connect to presence stream
- Translate events to LED commands
- Send over USB serial to RP2040

Serial protocol (simple):
```
[0xAA] [length] [25 × RGB bytes]
```

#### 4.3 Installation

```bash
# Service installation
sudo apt install garden-firefly

# Firmware: flash via Thonny or mpremote
mpremote connect /dev/ttyACM0 fs cp main.py :main.py
```

---

### Phase 5: OLED Companion (Week 6-7)

**Goal:** Text display on ESP8266 OLED.

#### 5.1 ESP8266 Firmware

**Tech:** MicroPython or Arduino

The ESP8266 connects directly to Moss WiFi network:
- Connects to `http://<stone-ip>:7185/api/v1/presence/stream`
- Parses SSE events
- Updates OLED display

No host service needed — the ESP8266 IS the Companion.

#### 5.2 Configuration

How does ESP8266 know the stone IP?

Options:
1. **Hardcode** — Simple, inflexible
2. **mDNS discovery** — ESP8266 finds `stone-name._garden._tcp.local`
3. **Config portal** — AP mode for initial setup

Recommendation: mDNS first, config portal fallback.

#### 5.3 Display Layout

```
┌──────────────────────────────────┐
│  stone-crystal-forest            │
│  ━━━━━━━━━━━━━━━━━━━━━           │
│  ◉ thriving     2d 4h            │
│  ▓▓▓▓▓░░░░░ 45%                  │
│  mongodb ● redis ●               │
└──────────────────────────────────┘
```

---

### Phase 6: Automated Detection & Installation (Week 8+)

**Goal:** Moss detects presence hardware and assists installation.

#### 6.1 USB Device Detection

Moss periodically scans for known USB devices:

| Device | USB VID:PID | Action |
|--------|-------------|--------|
| RP2040-Matrix | `2e8a:0005` | Suggest Firefly |
| ESP8266 (serial) | `1a86:7523` | Suggest OLED Companion |
| USB Speaker | (various) | Suggest Cricket |

#### 6.2 Detection API

```
GET /api/v1/stone/presence/devices

{
  "detected": [
    {
      "type": "led-matrix",
      "device": "/dev/ttyACM0",
      "vendor": "Raspberry Pi",
      "product": "RP2040",
      "suggested_Companion": "firefly",
      "installed": false
    }
  ]
}
```

#### 6.3 Moss-Assisted Installation

```
POST /api/v1/stone/presence/Companions

{
  "Companion": "firefly",
  "device": "/dev/ttyACM0"
}
```

Moss:
1. Downloads Companion package (from Lantern registry or GitHub)
2. Places in `/opt/zen-garden/companions/firefly/`
3. Configures with detected device
4. Starts service
5. Verifies connection via `/api/v1/presence/subscribers`

#### 6.4 Rake Integration

```bash
# List detected presence hardware
garden-rake presence devices

# Install Companion for detected device
garden-rake presence install firefly --device /dev/ttyACM0

# Or auto-install all detected
garden-rake presence install --auto
```

---

## Development Workflow

### For Moss Endpoint Development

```bash
# Terminal 1: Run Moss locally
cd dist/windows
./garden-moss.exe

# Terminal 2: Watch presence stream
curl -N http://localhost:7185/api/v1/presence/stream

# Terminal 3: Trigger events
garden-rake plant mongodb
garden-rake tend
garden-rake uproot mongodb
```

### For Companion Development

```bash
# Use the test event emitter
garden-rake presence test stone.tended
garden-rake presence test service.started --service mongodb
garden-rake presence test stone.health.changed --health withering

# Or run simulation
garden-rake presence simulate load-spike
```

### For ESP8266/RP2040 Development

```bash
# Stream events to file for offline testing
curl -N http://localhost:7185/api/v1/presence/stream > events.log

# Replay for firmware testing
cat events.log | nc -l 8080  # Simple mock server
```

---

## File Structure

### In Moss

```
src/moss/src/
├── api/
│   └── v1/
│       ├── presence.rs          # NEW: SSE endpoint, subscribers
│       └── mod.rs               # Add presence routes
├── domain/
│   └── presence.rs              # NEW: Presence event types, bridge
└── tasks/
    └── presence_monitor.rs      # NEW: Load monitoring, heartbeat
```

### In garden_common

```
src/common/src/
├── presence/                    # NEW
│   ├── mod.rs
│   ├── events.rs               # PresenceEvent enum
│   └── types.rs                # Subscriber, Snapshot, etc.
└── lib.rs                      # Export presence module
```

### Companions (Separate Repos/Packages)

```
garden-cricket/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── audio.rs
│   └── events.rs
├── sounds/
│   ├── chirp.wav
│   └── chime.wav
└── README.md

garden-firefly/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── serial.rs
│   └── renderer.rs
├── firmware/
│   └── main.py
└── README.md
```

---

## Configuration

### Cricket Configuration

```toml
# /etc/zen-garden/cricket.toml

[connection]
moss_url = "http://localhost:7185"
Companion_name = "cricket"
Companion_version = "0.1.0"

[audio]
device = "default"
volume = 0.4

[sounds]
service_started = "chime"
service_stopped = "fade"
stone_tended = "gentle"
health_warning = "warning"
```

### Firefly Configuration

```toml
# /etc/zen-garden/firefly.toml

[connection]
moss_url = "http://localhost:7185"
Companion_name = "firefly"
Companion_version = "0.1.0"

[device]
serial_port = "/dev/ttyACM0"
baud_rate = 115200

[display]
brightness_max = 0.6
```

---

## Testing Strategy

### Unit Tests (Rust)

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_presence_snapshot_contains_required_fields() { ... }
    
    #[test]
    fn test_domain_event_maps_to_presence_event() { ... }
    
    #[test]
    fn test_subscriber_tracking_on_connect_disconnect() { ... }
}
```

### Integration Tests (PowerShell/Bash)

```powershell
# tests/presence-integration.ps1

# Test: Snapshot on connect
$response = Invoke-WebRequest -Uri "http://localhost:7185/api/v1/presence/stream" -TimeoutSec 2
Assert-Contains $response.Content "presence.snapshot"

# Test: Subscriber appears
$subscribers = Invoke-RestMethod "http://localhost:7185/api/v1/presence/subscribers"
Assert-GreaterThan $subscribers.count 0
```

### End-to-End Tests

```bash
# Start Moss
# Connect Cricket
# Plant MongoDB
# Verify: Cricket played chime
# Tend stone
# Verify: Cricket played gentle tone
# Check subscribers shows Cricket
```

---

## Rollout Plan

### Alpha (Internal Testing)

- Moss presence endpoints deployed to all stones
- Cricket running on Wyse 5070
- Team validates event flow

### Beta (Hardware Expansion)

- Firefly on RP2040 for 2-3 stones
- OLED Companion on ESP8266
- Document installation process

### GA (General Availability)

- Companions packaged for apt/cargo
- Moss auto-detection enabled
- Documentation complete

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| SSE connection instability | Heartbeat + reconnection logic |
| Event storms (100 events/sec) | Companions debounce; Moss rate-limits `service.activity` |
| USB device permissions | Udev rules in package |
| ESP8266 WiFi reliability | Reconnection with exponential backoff |
| Audio device contention | Exclusive mode or warn on conflict |

---

## Success Criteria

### Phase 1 Complete When:

- [ ] `curl -N http://localhost:7185/api/v1/presence/stream` shows events
- [ ] `garden-rake presence watch` works
- [ ] `garden-rake presence subscribers` shows connected clients
- [ ] Planting a service emits `service.sprouted` and `service.started`

### Phase 3 Complete When:

- [ ] Wyse 5070 plays chime when `garden-rake tend` runs
- [ ] Wyse 5070 plays warning when CPU > 80%
- [ ] Cricket appears in subscribers list

### Phase 4 Complete When:

- [ ] RP2040 LED matrix glows green when stone is thriving
- [ ] Matrix sparkles on service activity
- [ ] Firefly appears in subscribers list

### Phase 5 Complete When:

- [ ] ESP8266 OLED shows stone name and status
- [ ] Display updates when services start/stop
- [ ] Works without any host-side service (standalone)

---

## Open Questions

1. **In-tree or separate repos?**
   - *Proposal:* Start in-tree (`src/cricket/`, `src/firefly/`), split later if needed

2. **Companion package format?**
   - *Proposal:* Debian packages for Linux, standalone binaries for Windows

3. **Firmware flashing UX?**
   - *Proposal:* Document manual process; Moss-assisted flashing is Phase 7+

4. **Lantern garden-wide stream priority?**
   - *Proposal:* Implement after single-stone Companions proven

---

## Timeline Summary

| Phase | Duration | Deliverable |
|-------|----------|-------------|
| 1. Moss Endpoints | 2 weeks | `/presence/stream`, `/presence/subscribers`, Rake commands |
| 2. Test Harness | 1 week | Simulations, protocol tests, load tests |
| 3. Cricket (Audio) | 1-2 weeks | Working audio on Wyse 5070 |
| 4. Firefly (LED) | 2 weeks | Working LED matrix on RP2040 |
| 5. OLED Companion | 1 week | Working OLED on ESP8266 |
| 6. Auto-Detection | 2 weeks | Moss detects hardware, assists install |

**Total:** 9-11 weeks to full presence ecosystem

---

## Decision

**Proceed with Phase 1 immediately.**

- Moss endpoints are foundation
- Can test with Rake before hardware arrives
- Cricket on Wyse 5070 is first hardware validation

---

**Document Status:** Planning → In Progress  
**Authors:** Specialist team consensus  
