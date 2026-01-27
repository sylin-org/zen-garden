# CRICKET-0001: Audio Presence Adapter Specification

**Status:** Proposal  
**Date:** 2026-01-26  
**Objective:** Make home lab infrastructure feel intimate, tactile, and real through spatial audio feedback.

---

## Executive Summary

**Cricket** is an audio presence adapter that transforms infrastructure events into spatial soundscapes. It consumes the Stone Presence Protocol (PRESENCE-0001) and creates ambient audio feedback that makes hardware feel **alive without being alarming**.

Cricket runs as a standalone service on stones with audio capability (Wyse 5070, desktop PCs, Raspberry Pi with speakers) and connects to the local Moss instance via SSE. The adapter is **completely autonomous**—Moss knows nothing about audio, Cricket knows nothing about Docker.

**Core principle:** Infrastructure is not silent. It breathes, chirps, and occasionally chimes. You hear your garden without watching dashboards.

---

## Context & Existing Implementation

### What Already Exists

✅ **Moss Presence API** (IMPLEMENTED):
- `GET /api/v1/stone/presence/stream` - SSE endpoint with full event vocabulary
- Shared event constants in `garden_common::presence::event_types`
- Snapshot on connect, heartbeat every 30s
- Event filtering by category (service, stone)
- Zero magic strings, proper DDD separation

✅ **Rake Presence Client** (IMPLEMENTED):
- `garden-rake presence` - SSE consumer for debugging
- Displays snapshots and real-time events
- Validates protocol compliance

✅ **Presence Protocol Spec** (COMPLETE):
- Event vocabulary: `service.started`, `stone.tended`, `stone.health.changed`, etc.
- Garden-native semantics: thriving, withering, wilting, sprouted, uprooted
- Connection-based presence detection
- No adapter authentication required

### What Needs Building

❌ **Cricket Service** (THIS PROPOSAL):
- SSE client that connects to local Moss
- Audio synthesis engine (spatial soundscape layers)
- Event-to-sound mapping logic
- Systemd service for background execution
- Configuration file (`/etc/zen-garden/cricket.toml`)

---

## Specialist Team Assessment

### 1. Dr. Marcus Chen — Cognitive Psychology & Mnemonics

*"The goal is **ambient awareness**, not information overload."*

#### Assessment

Cricket must operate at the **periphery of attention**. You don't listen to it—you *hear* it. The soundscape communicates state without demanding focus.

**Principles:**

1. **Rise from Silence**: Default state is near-silence (very occasional cricket chirps). Activity increases sound density, not volume.

2. **Spatial Memory Formation**: Each stone should have a **unique voice** derived from its name/ID:
   - Stone A: Higher-pitched chirp (3kHz base)
   - Stone B: Lower-pitched chirp (1.8kHz base)
   - Stone C: Mid-range with slight rasp
   
   Over time, you recognize "that's the database stone" without thinking.

3. **Cognitive Load Limits**: 
   - Max 3-4 distinct stone voices before spatial confusion
   - Wind chimes limited to 1 per 5 seconds (no cascades)
   - Weather layer changes slowly (30+ second crossfade)

**Recommendation:**

- Derive cricket voice from `hash(stone_name) % 5` → select from 5 pre-designed cricket profiles
- Use **stereo panning** if multi-stone garden (Stone A left, Stone B right, Stone C center)
- Implement **event debouncing** (5 services start simultaneously → 1 wind chime, not 5)

---

### 2. Dr. Elena Vasquez — Sensory Design & UX

*"Audio feedback must feel **organic**, not robotic."*

#### Assessment

Cricket's sound palette should evoke a **natural environment**, not a data center. The metaphor is a garden at night—alive but peaceful.

**Sound Palette Design:**

| Layer | Purpose | Sonic Character |
|-------|---------|-----------------|
| **Cricket Voice** | Per-stone presence | Organic chirps, slight pitch variation, occasional pauses |
| **Weather** | Garden-wide load | Breeze (low load) → Wind (high load) → Rain (critical) |
| **Wind Chimes** | Events | Pentatonic scale, gentle onset, natural decay |
| **Water** | Security (Pond) | Gentle stream/fountain, continuous when pond active |
| **Milestones** | Achievements | Dawn chorus (7-day uptime), owl hoot (backup complete) |

**Concerns:**

1. **Alarm Fatigue**: If `stone.health.changed` → "withering" triggers a loud alarm, users will disable Cricket. Instead: cricket voice becomes strained/irregular (subtle but noticeable).

2. **Sensory Coherence**: If user also has Firefly (LED adapter), both should reinforce the same message:
   - Cricket: Strained chirp
   - Firefly: Amber glow
   - User perception: "Something needs attention" (converged signal, not conflict)

**Recommendation:**

- Use **sample libraries** (not synthesis) for organic quality (Freesound.org CC0)
- All event sounds have **soft onset** (50-200ms fade-in, no clicks)
- Crossfades between weather states: **minimum 30 seconds**
- No sounds above 85dB SPL (measured at 1m from typical PC speaker)

---

### 3. Omar Al-Rashid — Distributed Systems Architect

*"Cricket is a stateful consumer. Handle reconnection properly."*

#### Assessment

Cricket must maintain internal state to create coherent soundscape across reconnections.

**Architecture:**

```
┌─────────────────────────────────────────────────┐
│  CRICKET SERVICE                                │
│                                                 │
│  ┌──────────────┐      ┌──────────────┐        │
│  │ SSE Client   │──────│ State Model  │        │
│  │ (axum/reqwest│      │ (stone health│        │
│  │  reconnect)  │      │  services)   │        │
│  └──────┬───────┘      └──────┬───────┘        │
│         │                     │                │
│         │ events              │ state updates  │
│         ▼                     ▼                │
│  ┌──────────────────────────────────────┐      │
│  │   Event Processor                    │      │
│  │   - Debouncing (5s window)           │      │
│  │   - Event correlation                │      │
│  │   - State transition detection       │      │
│  └──────────────┬───────────────────────┘      │
│                 │ sound commands                │
│                 ▼                               │
│  ┌──────────────────────────────────────┐      │
│  │   Audio Engine (rodio)               │      │
│  │   - Layer mixer (4 tracks)           │      │
│  │   - Spatial positioning              │      │
│  │   - Volume control (config)          │      │
│  └──────────────┬───────────────────────┘      │
│                 │                               │
│                 ▼                               │
│              [Speakers]                         │
└─────────────────────────────────────────────────┘
```

**Reconnection Handling:**

1. **On connect**: Parse `presence.snapshot` to rebuild state model
2. **On disconnect**: Audio engine continues with last known state (graceful degradation)
3. **On reconnect**: Receive new snapshot, **crossfade** from current soundscape to new state (no jarring resets)

**Failure Modes:**

| Scenario | Cricket Behavior |
|----------|------------------|
| Moss crashes | Continue playing last state, mute after 2 minutes (no misleading sounds) |
| Network hiccup | Auto-reconnect, resume with snapshot resync |
| Audio device unplugged | Log error, retry audio init every 30s |
| Config reload (SIGHUP) | Reload settings, no playback interruption |

**Recommendation:**

- Use `tokio::spawn` for audio engine (separate thread, no blocking SSE client)
- SSE client: 5-second timeout, exponential backoff (max 30s)
- State model: `Arc<RwLock<GardenState>>` shared between SSE and audio threads

---

### 4. Dr. Yuki Tanaka — Ambient Computing Researcher

*"Calm technology enchants without alarming."*

#### Assessment

Cricket embodies Weiser's **calm technology principles**:

1. **Peripheral Awareness**: Soundscape exists at edge of attention
2. **Amplifies Good, Attenuates Bad**: Normal operations are peaceful; problems are noticeable but not panic-inducing
3. **Information Without Asking**: You know garden state without checking dashboards

**Taxonomy of Urgency:**

| Event Severity | Audio Response | Attention Demand |
|----------------|----------------|------------------|
| **Normal** | Occasional chirps, breeze | Peripheral (can ignore) |
| **Notable** | Wind chime, increased chirp rate | Marginal (might notice) |
| **Concerning** | Strained cricket voice, wind gusts | Mild (probably notice) |
| **Critical** | Irregular chirps, rain sounds | Moderate (will notice) |
| **Emergency** | *Silence* (sudden absence is alarming) | High (investigates) |

**Key Insight:** The most alarming sound is **sudden silence**. If Cricket stops chirping, something is very wrong (service crashed, audio failure, stone down).

**Recommendation:**

- Implement **heartbeat chirp**: Even with zero activity, minimum 1 chirp per 60 seconds
- On critical failure (`stone.health` = "wilting"), don't scream—go quiet for 10s, then resume with distressed pattern
- Include `--volume` config option (0-100), default 30 (conservative for workplace/bedroom)

---

### 5. Dr. Anjali Mehta — Semiotics & Symbolic Meaning

*"Cricket's vocabulary must be **culturally legible**."*

#### Assessment

Sound design carries cultural baggage. Cricket's palette must avoid universally negative associations.

**Cultural Considerations:**

| Sound | Western Interpretation | Eastern Interpretation | Universal Safety |
|-------|------------------------|------------------------|------------------|
| Crickets | Peaceful night, countryside | Similar (Japan: insect appreciation) | ✅ Safe |
| Wind chimes | Zen, meditation, gardens | Strong cultural presence (Buddhism) | ✅ Safe |
| Running water | Calm, nature, spa | Feng shui (flow, prosperity) | ✅ Safe |
| Crows/ravens | Ominous (Western folklore) | Mixed (Japan: messenger) | ⚠️ Avoid |
| Bells/chimes (church) | Religious connotation | Buddhist temple bells | ⚠️ Use simple chimes only |

**Garden Metaphor Coherence:**

Cricket must respect Zen Garden's vocabulary:

| Garden Concept | Audio Equivalent |
|----------------|------------------|
| **Thriving** | Healthy cricket chirps, gentle breeze |
| **Withering** | Strained chirps, wind picks up |
| **Wilting** | Irregular/distressed chirps, rain |
| **Sprouted** | Wind chime (something new appears) |
| **Uprooted** | Chime fades (something disappears) |
| **Tended** | Single clear chime (acknowledgment) |
| **Pond** (security) | Water sounds (boundary marker) |

**Recommendation:**

- Avoid animal sounds beyond crickets (no crows, owls should be minimal/optional)
- Chimes must be **tuned** (pentatonic/just intonation) to avoid dissonance
- All sounds should feel **intentional**, not random (garden is curated, not wilderness)

---

### 6. Dr. Raj Patel — Software Architecture (DDD/CQRS)

*"Cricket is a **pure consumer**. Zero coupling to Moss internals."*

#### Assessment

Cricket's architecture must demonstrate perfect **separation of concerns**:

| Layer | Cricket Implementation | What It Knows | What It Doesn't Know |
|-------|------------------------|---------------|----------------------|
| **Domain** | `GardenState` model (stones, services, health) | Event semantics | Docker, HTTP, Moss internals |
| **Infra** | SSE client, audio engine (rodio) | How to connect, play sounds | Moss event bus, domain events |
| **Application** | Event processor, sound mapper | Mapping events to sounds | How Moss generates events |

**Contracts:**

Cricket depends **only** on:
- `garden_common::presence::*` (event types, snapshot schema)
- SSE protocol (text/event-stream)
- Audio device (OS-level, not Moss-specific)

Cricket must **never**:
- Import `garden_moss` crates
- Query Moss HTTP endpoints beyond presence stream
- Assume Moss implementation details
- Coordinate with other adapters (Firefly, OLED)

**Recommendation:**

- Repository: `zen-garden/garden-cricket` (separate from main monorepo during development)
- Dependencies: `tokio`, `reqwest`, `rodio`, `serde`, `garden_common` (shared types only)
- Configuration: TOML file (`/etc/zen-garden/cricket.toml`), no ENV vars
- Deployment: Standalone binary + systemd unit, no Docker (runs bare metal for audio access)

---

## Group Consensus

After reviewing individual assessments, the team converged on these principles:

### ✅ Agreed: Core Design Principles

1. **Ambient, Not Alarming**: Soundscape operates at periphery of attention
2. **Rise from Silence**: Default is near-silence, activity increases density (not volume)
3. **Organic Palette**: Natural sounds (crickets, wind, water, chimes), no beeps/buzzers
4. **Spatial Identity**: Each stone has unique voice (pitch/timbre derived from name hash)
5. **Graceful Degradation**: Continues playing on disconnect, resyncs on reconnect
6. **Culturally Legible**: Sounds carry positive/neutral associations across cultures
7. **Zero Coupling**: Pure SSE consumer, no knowledge of Moss internals
8. **User Control**: Volume, layer enable/disable, voice selection in config

### ✅ Agreed: Event-to-Sound Mappings

| Presence Event | Cricket Response | Layer | Debounce |
|----------------|------------------|-------|----------|
| `presence.snapshot` | Initialize all layers based on state | All | N/A |
| `service.started` | Wind chime (brief) | Events | 5s window |
| `service.stopped` | Chime fade-out | Events | None |
| `service.sprouted` | Wind chime (new) | Events | 5s window |
| `service.uprooted` | Chime fade-out | Events | None |
| `stone.load.updated` | Adjust weather intensity | Weather | 30s smooth |
| `stone.health.changed` → withering | Cricket voice becomes strained | Voice | 30s crossfade |
| `stone.health.changed` → wilting | Cricket voice irregular + rain | Voice + Weather | 30s crossfade |
| `stone.tended` | Single clear chime | Events | None (immediate) |
| `pond.joined` | Fade in water sounds | Water | 60s fade |
| `pond.left` | Fade out water sounds | Water | 60s fade |

### ✅ Agreed: Audio Layer Architecture

**4-Layer Mixer:**

```
┌────────────────────────────────────────────────────┐
│  AUDIO ENGINE (rodio)                              │
│                                                    │
│  Layer 1: Cricket Voice  (per-stone, continuous)  │
│  Layer 2: Weather        (garden-wide, continuous) │
│  Layer 3: Events         (transient, <2s)          │
│  Layer 4: Water          (security, continuous)    │
│                                                    │
│  Master Volume: 0-100 (default 30)                 │
└────────────────────────────────────────────────────┘
```

Each layer can be independently:
- Enabled/disabled (config)
- Volume-adjusted (config)
- Crossfaded (smooth transitions)

### ✅ Agreed: Configuration Schema

**File:** `/etc/zen-garden/cricket.toml`

```toml
[moss]
# Required: Local Moss endpoint
endpoint = "http://localhost:7185"

[audio]
# Master volume (0-100, default 30)
master_volume = 30

# Layer volume overrides (0-100, relative to master)
voice_volume = 100
weather_volume = 80
events_volume = 90
water_volume = 70

# Layer enable/disable
enable_voice = true
enable_weather = true
enable_events = true
enable_water = true

[voice]
# Cricket voice selection (auto, high, mid, low, raspy, mellow)
# "auto" derives from stone name hash
voice_profile = "auto"

# Chirp frequency multiplier (0.5-2.0, affects activity feel)
chirp_rate = 1.0

[events]
# Debounce window for cascading events (seconds)
debounce_window = 5

# Wind chime scale (pentatonic, major, minor)
chime_scale = "pentatonic"

[logging]
# Log level (error, warn, info, debug, trace)
level = "info"

# Log to file (optional, defaults to journald)
# log_file = "/var/log/zen-garden/cricket.log"
```

### ✅ Agreed: Implementation Phases

**Phase 1: Minimal Viable Cricket** (Week 1-2)
- SSE client with reconnection
- Single cricket voice (no spatial yet)
- Wind chime on `service.started`
- Single chime on `stone.tended`
- Config loading
- Systemd service

**Phase 2: Full Layer System** (Week 3-4)
- 4-layer mixer architecture
- Weather layer (load → sound mapping)
- Water layer (pond status)
- Voice variation (5 cricket profiles)
- Event debouncing

**Phase 3: Spatial & Polish** (Week 5-6)
- Stereo panning for multi-stone gardens
- Smooth crossfades (30s transitions)
- Volume normalization
- Sample library curation (CC0 sources)
- Integration testing with real Moss

---

## Technical Specification

### Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.11", features = ["stream"] }
futures-util = "0.3"
rodio = "0.17"  # Audio playback
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
chrono = "0.4"

# Shared types
garden_common = { path = "../common" }
```

### Repository Structure

```
garden-cricket/
├── Cargo.toml
├── src/
│   ├── main.rs               # Service entry point
│   ├── config.rs             # TOML config parsing
│   ├── sse_client.rs         # SSE connection & reconnection
│   ├── state.rs              # GardenState model
│   ├── processor.rs          # Event processing & debouncing
│   ├── audio/
│   │   ├── mod.rs
│   │   ├── engine.rs         # rodio mixer
│   │   ├── layers.rs         # Layer management
│   │   ├── voice.rs          # Cricket voice synthesis
│   │   ├── weather.rs        # Weather soundscape
│   │   ├── events.rs         # Event sounds (chimes)
│   │   └── water.rs          # Water layer
│   └── samples/              # Embedded sound files
│       ├── chirp_high.wav
│       ├── chirp_mid.wav
│       ├── chirp_low.wav
│       ├── chime_01.wav
│       ├── breeze.wav
│       ├── wind.wav
│       ├── rain.wav
│       └── water_stream.wav
├── cricket.toml.example      # Sample config
├── cricket.service           # Systemd unit
└── README.md
```

### State Model

```rust
/// Cricket's internal state model (rebuilt from presence.snapshot)
pub struct GardenState {
    pub stone: StoneHealth,
    pub services: HashMap<String, ServiceInfo>,
    pub pond_active: bool,
    pub last_update: Instant,
}

pub struct StoneHealth {
    pub name: String,
    pub status: HealthStatus,  // Thriving, Withering, Wilting
    pub cpu_percent: f64,
    pub memory_percent: f64,
}

pub enum HealthStatus {
    Thriving,
    Withering,
    Wilting,
}

pub struct ServiceInfo {
    pub name: String,
    pub running: bool,
}
```

### Event Processing Pipeline

```rust
// Simplified flow
async fn process_presence_event(event: PresenceEvent, state: Arc<RwLock<GardenState>>) {
    match event.event_type.as_str() {
        event_types::PRESENCE_SNAPSHOT => {
            let snapshot: PresenceSnapshot = serde_json::from_str(&event.data)?;
            let mut state = state.write().await;
            *state = GardenState::from_snapshot(snapshot);
            
            // Initialize all audio layers based on state
            audio_engine.sync_to_state(&*state).await;
        }
        
        event_types::SERVICE_STARTED => {
            // Debounce: If multiple services start within 5s, play chime once
            debouncer.add_event("service_start").await;
            if debouncer.should_trigger("service_start", Duration::from_secs(5)).await {
                audio_engine.play_chime().await;
            }
        }
        
        event_types::STONE_HEALTH_CHANGED => {
            let data: serde_json::Value = serde_json::from_str(&event.data)?;
            let new_health = data["new"].as_str()?;
            
            // Crossfade cricket voice and weather over 30s
            audio_engine.transition_to_health(new_health, Duration::from_secs(30)).await;
        }
        
        event_types::STONE_TENDED => {
            // Immediate clear chime (no debounce)
            audio_engine.play_chime_clear().await;
        }
        
        _ => {}
    }
}
```

### Audio Engine API

```rust
pub struct AudioEngine {
    // Internal rodio mixer state
}

impl AudioEngine {
    /// Initialize from snapshot
    pub async fn sync_to_state(&self, state: &GardenState) {
        self.set_voice_health(state.stone.status);
        self.set_weather_intensity(state.stone.cpu_percent);
        self.set_water_active(state.pond_active);
    }
    
    /// Smooth transition to new health status
    pub async fn transition_to_health(&self, new_health: &str, duration: Duration) {
        // Crossfade cricket voice + weather layer
    }
    
    /// Play event sound (wind chime)
    pub async fn play_chime(&self) {
        // Trigger one-shot sample
    }
    
    /// Set weather intensity based on load (0.0-1.0)
    pub async fn set_weather_intensity(&self, load: f64) {
        // Crossfade between breeze/wind/rain samples
    }
}
```

---

## Installation & Deployment

### System Requirements

- **OS:** Linux (Ubuntu 22.04+, Debian 12+)
- **Audio:** ALSA or PulseAudio
- **Hardware:** Any stone with audio output (Wyse 5070, desktop, RPi)
- **Memory:** ~20MB RSS
- **CPU:** <1% (audio playback is efficient)

### Installation Steps

```bash
# 1. Copy binary to stone
scp garden-cricket stone@target-stone:/tmp/

# 2. Install binary
sudo mv /tmp/garden-cricket /usr/local/bin/
sudo chmod +x /usr/local/bin/garden-cricket

# 3. Create config directory
sudo mkdir -p /etc/zen-garden

# 4. Copy config template
sudo cp cricket.toml.example /etc/zen-garden/cricket.toml

# 5. Install systemd service
sudo cp cricket.service /etc/systemd/system/garden-cricket.service
sudo systemctl daemon-reload

# 6. Enable and start
sudo systemctl enable garden-cricket
sudo systemctl start garden-cricket

# 7. Verify
sudo journalctl -u garden-cricket -f
```

### Systemd Unit

**File:** `/etc/systemd/system/garden-cricket.service`

```ini
[Unit]
Description=Cricket - Zen Garden Audio Presence Adapter
Documentation=https://github.com/zen-garden/garden-cricket
After=network.target garden-moss.service
Requires=garden-moss.service

[Service]
Type=simple
User=stone
Group=audio
ExecStart=/usr/local/bin/garden-cricket --config /etc/zen-garden/cricket.toml
Restart=always
RestartSec=10

# Graceful shutdown (fade out audio)
TimeoutStopSec=5

# Reload config without restart
ExecReload=/bin/kill -HUP $MAINPID

[Install]
WantedBy=multi-user.target
```

### Verifying Cricket Works

```bash
# From workstation, trigger "tended" event
garden-rake tend stone-target

# Should hear: Clear wind chime from stone's speakers

# Check Cricket is receiving events
sudo journalctl -u garden-cricket --since "1 minute ago"

# Expected logs:
# INFO Connected to presence stream: http://localhost:7185/api/v1/stone/presence/stream
# INFO Received presence.snapshot: thriving, 2 services
# INFO Event: stone.tended from rake - playing clear chime
```

---

## Testing Strategy

### Unit Tests

- Config parsing (valid/invalid TOML)
- State model updates (snapshot → internal state)
- Debouncer logic (5s window, multiple events)
- Health status mapping (cpu/memory → thriving/withering/wilting)

### Integration Tests

**With Real Moss:**

1. Start Moss on test stone
2. Start Cricket
3. Trigger events via Rake:
   - `garden-rake tend stone-test` → Hear chime
   - Deploy service → Hear chime
   - Stop service → Hear fade-out
4. Verify logs show correct event reception

**Reconnection Test:**

1. Start Cricket + Moss
2. Kill Moss (`systemctl stop garden-moss`)
3. Verify: Cricket logs reconnection attempts
4. Verify: Audio continues with last known state (no crash)
5. Restart Moss (`systemctl start garden-moss`)
6. Verify: Cricket reconnects, receives snapshot, resyncs state

**Load Test:**

1. Deploy 5 services rapidly
2. Verify: Only 1 wind chime plays (debouncing works)
3. Stop all 5 services rapidly
4. Verify: Only 1 fade-out (debouncing works)

### Audio Quality Test

**Manual Validation:**

- Chirps sound organic (not robotic)
- Crossfades are smooth (no clicks/pops)
- Volume at 30% is comfortable (not too loud)
- Chimes are tuned (no dissonance)
- Water sounds are pleasant (not annoying)

---

## Open Questions

### 1. Multi-Stone Support

**Question:** Should one Cricket instance listen to multiple stones (garden-wide)?

**Options:**
- **Option A:** Cricket runs per-stone (1:1 with Moss), connects to local Moss only
- **Option B:** Cricket can connect to Lantern's aggregated stream (`/api/v1/garden/presence/stream`)

**Team Consensus:** Start with **Option A** (per-stone). Multi-stone Cricket can be Phase 4.

**Rationale:**
- Simpler deployment (Cricket runs where audio device is)
- Clearer spatial mapping (Cricket on Stone A plays Stone A's voice)
- Avoid complexity of multi-stone voice mixing in Phase 1

### 2. Voice Customization

**Question:** Should users be able to upload custom sound samples?

**Team Consensus:** **Not Phase 1**. Use curated CC0 library. Custom samples can be Phase 5.

**Rationale:**
- Avoids quality control issues (badly-recorded samples)
- Maintains design coherence (all Crickets sound intentionally garden-like)
- Sample library can expand over time with community contributions

### 3. Volume Auto-Adjustment

**Question:** Should Cricket detect ambient room noise and adjust volume?

**Team Consensus:** **Not Phase 1**. Fixed volume from config. Can be Phase 6.

**Rationale:**
- Requires microphone access (privacy concern)
- Adds complexity (noise detection algorithms)
- User-controlled volume is sufficient for home lab use case

---

## Success Criteria

Cricket succeeds if:

1. **It Feels Alive:** Users report infrastructure "breathes" without being annoying
2. **Ambient Awareness:** Users know garden state without checking dashboards
3. **Cultural Acceptance:** No reports of sounds being alarming/offensive
4. **Zero Coupling:** Cricket works with any Presence-compliant Moss version
5. **Reliable Operation:** Runs for 30+ days without manual intervention
6. **Reconnection Works:** Survives Moss restarts without requiring manual restart
7. **Spatial Recognition:** In multi-stone setup, users can identify which stone by sound

---

## Future Enhancements (Beyond Phase 3)

**Phase 4: Multi-Stone Garden Support**
- Connect to Lantern's aggregated stream
- Stereo panning (Stone A left, Stone B right, Stone C center)
- Voice mixing (up to 4 stones simultaneously)

**Phase 5: Custom Sound Packs**
- User-uploadable sample libraries
- "Themes": Forest, Zen Garden, Ocean, Cave, Space
- Community-contributed packs

**Phase 6: Adaptive Volume**
- Time-of-day volume adjustment (quieter at night)
- Ambient noise detection (optional, opt-in)

**Phase 7: Mobile Client**
- Android/iOS app that connects to Lantern
- Headphone-optimized spatial audio
- Bedtime mode (ultra-quiet)

**Phase 8: Haptic Extension**
- USB-connected vibration motor
- Subtle vibration on critical events
- Desktop "presence object" (physical device that hums/vibrates)

---

## Decision

**Accepted.** Cricket audio adapter will be implemented as specified.

**Next Steps:**

1. Create `garden-cricket` repository
2. Implement Phase 1 (Minimal Viable Cricket)
3. Test on Wyse 5070 with real Moss
4. Document sound palette rationale
5. Curate CC0 sample library
6. Release v0.1.0 with basic functionality

---

**Document Status:** Proposal → Pending Implementation  
**Authors:** Specialist team consensus (6 experts)  
**Objective Alignment:** ✅ "Make home lab infrastructure feel intimate, tactile, and real."  
**Protocol Compliance:** ✅ PRESENCE-0001 (no deviations)  
**Separation of Concerns:** ✅ Pure consumer, zero Moss coupling  
**Last Updated:** 2026-01-26
