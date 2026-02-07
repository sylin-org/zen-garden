---
audience: developer
doc_type: spec
status: current
last_verified: 2026-02-07
---

# Cricket Implementation Roadmap

**Target:** Zen Garden 0.2.x
**Hardware Testbed:** Wyse 5070 (internal speakers)

---

## Overview

Cricket is the first presence Companion for Zen Garden. This roadmap defines the incremental build plan, starting with a minimal viable implementation and expanding to full soundscape capability.

**Protocol:** Cricket consumes PRESENCE-0001 (implemented in Moss)
**Repository:** `zen-garden/garden-cricket` (separate during development)
**Language:** Rust (consistent with Moss/Rake)

---

## Phase 1: Minimal Viable Cricket

**Objective:** Prove the protocol works with simplest possible audio feedback.

### SSE Client

- Connect to `http://localhost:7185/api/v1/stone/presence/stream`
- Parse SSE protocol (event/data/blank line)
- Handle reconnection with exponential backoff (max 30s)
- Rebuild state from `presence.snapshot`

### Basic Audio Playback

- Use `rodio` crate for audio
- Embed 3 WAV files:
  - `chime.wav` — Wind chime sound
  - `chime_clear.wav` — Single clear chime
  - `beep.wav` — Fallback test sound

### Event Handling

- `service.started` → Play `chime.wav`
- `service.stopped` → Play `chime.wav` (same for now)
- `stone.tended` → Play `chime_clear.wav`
- All other events → Log but ignore

### Configuration

Load `/etc/zen-garden/cricket.toml`:

- `moss.endpoint` (required)
- `audio.master_volume` (0-100, default 30)
- `logging.level` (default "info")

### Systemd Integration

- `garden-cricket.service` unit file
- Auto-restart on failure
- Reload config on SIGHUP
- User: `stone`, Group: `audio`

### Testing

- Unit tests: Config parsing, state model
- Manual test: Deploy service on Wyse 5070, hear chime
- Reconnection test: Kill Moss, verify Cricket reconnects

### Success Criteria

- Cricket runs for 1 hour without crash
- Chime plays within 500ms of `service.started` event
- Reconnection works after Moss restart
- Volume at 30% is audible but not loud

### Sample Acquisition

All samples from Freesound.org (CC0 license):

- Wind chime: Search "wind chime pentatonic"
- Clear chime: Search "meditation bell"
- Beep: Generate with Audacity (440Hz sine, 100ms)

---

## Phase 2: Full Layer System

**Objective:** Implement 4-layer audio mixer with full event vocabulary.

### Layer Architecture

```rust
pub struct AudioEngine {
    voice_layer: VoiceLayer,      // Cricket chirps
    weather_layer: WeatherLayer,  // Breeze/wind/rain
    events_layer: EventsLayer,    // Chimes (transient)
    water_layer: WaterLayer,      // Pond security
    mixer: LayerMixer,
}
```

### Voice Layer (Cricket Chirps)

- 5 cricket voice profiles (high, mid, low, raspy, mellow)
- Derive from `hash(stone_name) % 5`
- Health status affects chirp pattern:
  - Thriving: Occasional, regular chirps
  - Withering: Strained, more frequent
  - Wilting: Irregular, distressed
- Config: `voice.voice_profile` ("auto" or manual selection)

### Weather Layer

- Map CPU load → weather intensity:
  - 0-50%: Breeze (subtle wind)
  - 50-80%: Wind (moderate)
  - 80-100%: Rain (heavy)
- Smooth crossfade (30s) between states
- Config: `audio.enable_weather`

### Events Layer

- Wind chime on `service.sprouted`, `service.started`
- Chime fade-out on `service.uprooted`, `service.stopped`
- Clear chime on `stone.tended`
- Debouncing: 5s window (5 services start → 1 chime)

### Water Layer

- Gentle stream sound when `pond_active = true`
- 60s fade-in on `pond.joined`
- 60s fade-out on `pond.left`
- Config: `audio.enable_water`

### Event Debouncing

```rust
pub struct Debouncer {
    events: HashMap<String, VecDeque<Instant>>,
}

impl Debouncer {
    /// Add event to window
    pub fn add_event(&mut self, event_type: &str);

    /// Check if should trigger (returns true once per window)
    pub fn should_trigger(&mut self, event_type: &str, window: Duration) -> bool;
}
```

### State Synchronization

- On `presence.snapshot`: Initialize all layers
- On reconnect: Crossfade from current state to new state (no jarring reset)
- On disconnect: Continue playing last known state for 2 minutes, then mute

### Configuration

```toml
[audio]
master_volume = 30
voice_volume = 100
weather_volume = 80
events_volume = 90
water_volume = 70
enable_voice = true
enable_weather = true
enable_events = true
enable_water = true

[voice]
voice_profile = "auto"  # auto, high, mid, low, raspy, mellow
chirp_rate = 1.0        # Multiplier for chirp frequency

[events]
debounce_window = 5     # Seconds

[logging]
level = "info"
```

### Sample Library

**Voice Layer:**
- `chirp_high_01.wav` through `chirp_high_05.wav` (variations)
- `chirp_mid_01.wav` through `chirp_mid_05.wav`
- `chirp_low_01.wav` through `chirp_low_05.wav`
- `chirp_raspy_01.wav` through `chirp_raspy_05.wav`
- `chirp_mellow_01.wav` through `chirp_mellow_05.wav`

**Weather Layer:**
- `breeze_loop.wav` (seamless loop, 60s)
- `wind_loop.wav` (seamless loop, 60s)
- `rain_loop.wav` (seamless loop, 60s)

**Events Layer:**
- `chime_01.wav` through `chime_05.wav` (wind chime variations)
- `chime_clear.wav` (single bell)

**Water Layer:**
- `water_stream_loop.wav` (seamless loop, 60s)

### Testing

**Layer Isolation:**
- Disable all layers except voice → Verify chirps play
- Disable all layers except weather → Verify weather changes with simulated load
- Disable all layers except events → Verify chime on service start

**Crossfade Quality:**
- Simulate `stone.health.changed` (thriving → withering)
- Verify 30s crossfade (no clicks/pops)
- Verify cricket voice changes smoothly

**Debouncing:**
- Deploy 5 services rapidly
- Verify only 1 chime plays
- Log shows "Debounced 4 service.started events"

### Success Criteria

- All 4 layers work independently
- Crossfades are smooth (no audio artifacts)
- Debouncing prevents chime cascades
- Soundscape feels coherent (not chaotic)
- Volume levels are balanced

---

## Phase 3: Spatial & Polish

**Objective:** Multi-stone support, stereo panning, production-ready.

### Stereo Panning (Multi-Stone)

- If connecting to Lantern (`/api/v1/garden/presence/stream`):
  - Stone 1: Pan left (70% L / 30% R)
  - Stone 2: Pan center (50% L / 50% R)
  - Stone 3: Pan right (30% L / 70% R)
- Config: `stones.panning` (auto, left, center, right)

### Smooth Crossfades

- All state transitions use 30s crossfade minimum
- Weather layer: 30s
- Voice health: 30s
- Water layer: 60s
- No sudden audio changes

### Volume Normalization

- Analyze all samples with `ffmpeg-normalize`
- Target: -23 LUFS (broadcast standard)
- Ensure no clipping at 100% volume

### Sample Library Finalization

- Curate all sounds (organic, high-quality)
- License verification (CC0 only)
- Attribution file (`SAMPLES.md`)

### Error Handling

- Audio device unplugged → Log error, retry init every 30s
- Corrupted config → Use defaults, log warning
- Network timeout → Exponential backoff, max 30s
- Sample missing → Use fallback beep, log error

### Performance Targets

- Memory usage: <20MB RSS
- CPU usage: <1% (idle), <5% (active)
- Startup time: <2s
- Sample preloading (avoid disk I/O during playback)

### Packaging

- Build script (`build.sh`)
- Debian package (`.deb`)
- Install script for Wyse 5070
- Version number: `0.1.0`

### Testing

**Load Test:**
- Run Cricket for 24 hours
- Simulate 100 events/hour
- Verify: No memory leaks, CPU usage stable

**Multi-Stone Test:**
- Connect to Lantern with 3 stones
- Verify: Each stone has distinct voice + panning
- Verify: No voice overlap confusion

**Failure Recovery:**
- Unplug USB speakers during playback
- Verify: Cricket logs error, doesn't crash
- Plug speakers back in
- Verify: Audio resumes within 30s

**Integration Test (Full Stack):**
1. Start Moss on Wyse 5070
2. Start Cricket
3. Run Rake commands:
   - `garden-rake tend stone-wyse` → Hear clear chime
   - `garden-rake deploy mongodb` → Hear wind chime
   - `garden-rake stop mongodb` → Hear fade-out
4. Simulate high load (stress CPU to 85%)
   - Verify: Weather transitions to wind over 30s
   - Verify: Cricket voice becomes strained
5. Stop Moss
   - Verify: Cricket reconnects, audio continues

### Success Criteria

- All integration tests pass
- Cricket runs 7 days without manual intervention
- Users report soundscape feels "natural"
- No complaints about volume/annoyance
- Reconnection works 100% of time
- Memory/CPU usage within targets

---

## Deployment Plan

### Phase 1 Deployment (Internal Testing)

**Target:** Wyse 5070 (development stone)

1. Build binary: `cargo build --release`
2. SSH to Wyse: `ssh stone@wyse-5070`
3. Copy binary: `scp target/release/garden-cricket stone@wyse-5070:/tmp/`
4. Install:
   ```bash
   sudo mv /tmp/garden-cricket /usr/local/bin/
   sudo chmod +x /usr/local/bin/garden-cricket
   sudo cp cricket.toml.example /etc/zen-garden/cricket.toml
   sudo cp cricket.service /etc/systemd/system/garden-cricket.service
   sudo systemctl daemon-reload
   sudo systemctl enable garden-cricket
   sudo systemctl start garden-cricket
   ```
5. Verify: `sudo journalctl -u garden-cricket -f`

### Phase 2 Deployment (Beta Testing)

**Target:** 2-3 additional stones (volunteers)

1. Create Debian package: `build-deb.sh`
2. Push to stone: `scp garden-cricket_0.1.0_amd64.deb stone@target:/tmp/`
3. Install: `sudo dpkg -i /tmp/garden-cricket_0.1.0_amd64.deb`
4. Configure: Edit `/etc/zen-garden/cricket.toml` if needed
5. Start: `sudo systemctl start garden-cricket`
6. Collect feedback: Volume, sound quality, annoyance factor

### Phase 3 Deployment (Production)

**Target:** All stones with audio capability

1. Integrate into Zen Garden installer
2. Auto-detect audio capability: `aplay -l` (Linux) or `Get-AudioDevice` (Windows)
3. Prompt user: "Enable Cricket audio feedback? [Y/n]"
4. If yes:
   - Install `garden-cricket` package
   - Generate config with auto-detected Moss endpoint
   - Start service
   - Play test chime
5. Include in `garden-rake observe` output:
   ```
   Stone: stone-wyse-5070
   Presence Companions: Cricket v0.1.0
   ```

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Audio device conflicts (other apps using speakers) | Cricket can't play sounds | Document: Disable other audio apps, use dedicated speakers |
| Samples are annoying/grating | Users disable Cricket | Curate carefully, beta test with 5+ people before release |
| Multi-stone voice overlap confusing | Can't distinguish stones by sound | Limit to 3 stones max, use distinct pitch ranges |
| Memory leak in rodio | Cricket crashes after 24h | Load testing, monitor RSS, report upstream bug if found |
| SSE reconnection fails | Cricket goes silent | Exponential backoff, log clear error messages |
| Volume too loud/quiet | User complaints | Conservative default (30%), document adjustment in README |

---

## Open Issues

### Issue 1: Windows Support

**Question:** Should Cricket support Windows stones?

Cricket targets Linux first (Wyse 5070 testbed). Windows support (WASAPI vs ALSA) is deferred to a later phase, after the Linux release stabilizes.

### Issue 2: Docker Deployment

**Question:** Should Cricket run in Docker container?

No. Cricket runs bare metal for direct audio access. Audio device passthrough in Docker adds complexity and latency. Moss manages containers; Cricket is a companion service.

### Issue 3: Sample Copyright

CC0 samples from Freesound.org are embedded directly in the binary. An attribution file (`SAMPLES.md`) documents sources as a courtesy (not legally required for CC0).

---

## Success Metrics

**Adoption:**
- 10+ stones running Cricket in production
- Zero uninstalls due to annoyance (sound quality acceptable)

**Reliability:**
- 99% uptime (excluding Moss outages)
- Zero crashes over 7-day period
- Reconnection success rate >99%

**Performance:**
- Memory: <20MB RSS
- CPU: <1% average
- Startup time: <2s

**User Feedback:**
- "Makes infrastructure feel alive" (qualitative)
- No complaints about volume/sound quality
- Users can identify stone by chirp sound (multi-stone gardens)

---

## References

- [PRESENCE-0001](../decisions/PRESENCE-0001-stone-presence-protocol.md) — Protocol specification
- [CRICKET-0001](../decisions/CRICKET-0001-audio-companion-spec.md) — Design and specialist assessment
- [Moss Presence API](../../src/moss/src/api/v1/presence.rs) — Existing implementation
- [Rake Presence Client](../../src/rake/src/commands/presence.rs) — Reference consumer
