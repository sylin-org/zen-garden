# Cricket Audio Companion - Executive Summary

**Date:** 2026-01-26  
**Status:** Proposal Complete, Ready for Implementation  
**Objective:** Make home lab infrastructure feel intimate, tactile, and real through spatial audio.

---

## What Is Cricket?

Cricket is an **audio presence Companion** for Zen Garden that transforms infrastructure events into ambient soundscapes. It taps into the Stone Presence Protocol (PRESENCE-0001) and creates audio feedback that makes your hardware feel **alive without being alarming**.

**Example:** When you run `garden-rake tend stone-wyse`, the Wyse 5070 plays a gentle wind chime through its speakers. When a service starts, you hear a brief chime. When the stone is under load, the soundscape intensifies subtly—like wind picking up before rain.

---

## Current State

### ✅ What Already Exists (Implemented & Functional)

**Moss Presence API:**
- `GET /api/v1/stone/presence/stream` - SSE endpoint with full event vocabulary
- Shared event constants in `garden_common::presence::event_types` (zero magic strings)
- Snapshot on connect, heartbeat every 30s
- Event filtering by category (service, stone)
- Proper DDD separation (domain events → presence translation)

**Rake Presence Client:**
- `garden-rake presence` - SSE consumer for debugging/monitoring
- Displays snapshots and real-time events
- Validates protocol compliance

**Protocol Documentation:**
- Complete spec: [PRESENCE-0001](docs/decisions/PRESENCE-0001-stone-presence-protocol.md)
- Implementation report: [PRESENCE-0001-COMPLETE](docs/PRESENCE-0001-COMPLETE.md)
- Event vocabulary: `service.started`, `stone.tended`, `stone.health.changed`, etc.

### ❌ What Needs Building

**Cricket Service** (this proposal):
- SSE client connecting to local Moss
- 4-layer audio mixer (cricket voice, weather, events, water)
- Event-to-sound mapping with debouncing
- Configuration system (TOML)
- Systemd service for background execution

---

## Specialist Team Assessment

A **6-person specialist team** reviewed the Cricket proposal against the objective: *"make home lab infrastructure feel intimate, tactile, and real."*

### Team Composition

1. **Dr. Marcus Chen** — Cognitive Psychology & Mnemonics
2. **Dr. Elena Vasquez** — Sensory Design & UX
3. **Omar Al-Rashid** — Distributed Systems Architect
4. **Dr. Yuki Tanaka** — Ambient Computing Researcher
5. **Dr. Anjali Mehta** — Semiotics & Symbolic Meaning
6. **Dr. Raj Patel** — Software Architecture (DDD/CQRS)

### Group Consensus

**Unanimous Approval.** The team agrees Cricket achieves the objective through:

1. **Ambient Awareness** — Soundscape at periphery of attention, not demanding focus
2. **Organic Palette** — Natural sounds (crickets, wind, water, chimes), no beeps/buzzers
3. **Spatial Identity** — Each stone has unique voice (pitch derived from name hash)
4. **Graceful Degradation** — Continues playing on disconnect, resyncs on reconnect
5. **Zero Coupling** — Pure SSE consumer, no knowledge of Moss internals
6. **Culturally Legible** — Sounds carry positive/neutral associations across cultures

### Key Insights

**From Dr. Tanaka (Ambient Computing):**
> "The most alarming sound is **sudden silence**. If Cricket stops chirping, something is very wrong."

**From Dr. Chen (Mnemonics):**
> "Over time, you'll recognize 'that's the database stone' without thinking—spatial memory formed through consistent audio identity."

**From Dr. Vasquez (Sensory Design):**
> "If `stone.health.changed` → wilting triggers a loud alarm, users will disable Cricket. Instead: cricket voice becomes strained/irregular—subtle but noticeable."

---

## Sound Design

### 4-Layer Audio Architecture

```
Layer 1: Cricket Voice  (per-stone, continuous)
Layer 2: Weather        (garden-wide load, continuous)
Layer 3: Events         (transient chimes, <2s)
Layer 4: Water          (pond security, continuous)

Master Volume: 0-100 (default 30%)
```

### Event-to-Sound Mappings

| Presence Event | Cricket Response | Layer |
|----------------|------------------|-------|
| `service.started` | Wind chime (brief) | Events |
| `service.stopped` | Chime fade-out | Events |
| `stone.tended` | Single clear chime | Events |
| `stone.load.updated` | Adjust weather intensity | Weather |
| `stone.health.changed` → withering | Cricket voice strained | Voice |
| `stone.health.changed` → wilting | Cricket irregular + rain | Voice + Weather |
| `pond.joined` | Fade in water sounds | Water |

### Sample Palette

**Voice Layer:** 5 cricket profiles (high, mid, low, raspy, mellow)
**Weather Layer:** Breeze → Wind → Rain (smooth crossfades)
**Events Layer:** Pentatonic wind chimes
**Water Layer:** Gentle stream/fountain

All samples: **CC0 licensed** from Freesound.org

---

## Technical Architecture

### Pure Consumer Pattern

Cricket is a **standalone service** that:
- Connects to local Moss SSE endpoint
- Maintains internal state model (rebuilt from snapshots)
- Translates events to audio
- Handles reconnection gracefully

Cricket does **NOT**:
- Import `garden_moss` crates
- Query other Moss endpoints
- Coordinate with other Companions
- Require Moss to know about audio

### Dependencies

```toml
tokio = "1"           # Async runtime
reqwest = "0.11"      # SSE client
rodio = "0.17"        # Audio playback
garden_common = { path = "../common" }  # Shared types only
```

### Deployment

- **Repository:** `zen-garden/garden-cricket` (separate during development)
- **Install:** Debian package (`.deb`) + systemd unit
- **Config:** `/etc/zen-garden/cricket.toml`
- **User:** `stone` (group: `audio`)
- **Requirements:** Linux + ALSA/PulseAudio + speakers

---

## Implementation Roadmap

### Phase 1: Minimal Viable Cricket (2 weeks)

**Goal:** Prove protocol works with simplest audio feedback.

**Deliverables:**
- SSE client with reconnection
- Single cricket voice (no spatial yet)
- Wind chime on `service.started`
- Clear chime on `stone.tended`
- Config loading + systemd service

**Success:** Chime plays within 500ms of Rake tending command.

### Phase 2: Full Layer System (2 weeks)

**Goal:** Implement 4-layer mixer with complete event vocabulary.

**Deliverables:**
- Cricket voice (5 profiles, health-reactive)
- Weather layer (load → breeze/wind/rain)
- Water layer (pond security)
- Event debouncing (5s window)
- Expanded config (layer enable/disable, volumes)

**Success:** All layers work independently, crossfades are smooth.

### Phase 3: Spatial & Polish (2 weeks)

**Goal:** Multi-stone support, production-ready.

**Deliverables:**
- Stereo panning (multi-stone gardens)
- 30s crossfades for all transitions
- Volume normalization (-23 LUFS)
- Sample library finalization (CC0 verified)
- Documentation + Debian package

**Success:** 7-day uptime test passes, no memory leaks.

**Total Timeline:** 6-8 weeks from start to v0.1.0 release.

---

## Configuration Example

**File:** `/etc/zen-garden/cricket.toml`

```toml
[moss]
endpoint = "http://localhost:7185"

[audio]
master_volume = 30  # 0-100, default 30%
voice_volume = 100
weather_volume = 80
events_volume = 90
water_volume = 70

enable_voice = true
enable_weather = true
enable_events = true
enable_water = true

[voice]
voice_profile = "auto"  # Derived from stone name hash
chirp_rate = 1.0        # Frequency multiplier

[events]
debounce_window = 5     # Seconds (prevents chime cascades)

[logging]
level = "info"
```

---

## Success Criteria

Cricket succeeds if:

1. **Feels Alive:** Users report infrastructure "breathes" without being annoying
2. **Ambient Awareness:** Users know garden state without checking dashboards
3. **Zero Coupling:** Works with any PRESENCE-0001 compliant Moss version
4. **Reliable:** 7+ days uptime without manual intervention
5. **Reconnection:** Survives Moss restarts automatically
6. **Spatial Recognition:** In multi-stone setup, users identify stones by sound

---

## Key Design Decisions

### Why Audio Instead of Visual First?

**Team Consensus:**
- Audio provides **peripheral awareness** (works without looking)
- Faster development feedback loop (easier than LED matrix)
- Wyse 5070 has built-in speakers (no external hardware needed)
- Complements visual Companions (Firefly) rather than competing

### Why Not Volume Auto-Adjustment?

**Team Consensus (Phase 1):**
- Requires microphone access (privacy concern)
- Adds complexity (ambient noise detection)
- User-controlled volume sufficient for home lab use case
- Can be Phase 6 enhancement (opt-in)

### Why Embed Samples Instead of Streaming?

**Team Consensus:**
- Curated quality control (organic, high-quality sounds)
- No network dependency (works if internet down)
- License verification (CC0 only)
- Sample library can expand with updates

---

## User Experience Flow

### Installation

```bash
# On stone with audio capability
sudo dpkg -i garden-cricket_0.1.0_amd64.deb
sudo systemctl enable garden-cricket
sudo systemctl start garden-cricket
```

### Daily Use

**Scenario 1: Tending a Stone**
```bash
# From workstation
garden-rake tend stone-wyse-5070
```
**Result:** Wyse 5070 speakers play clear wind chime (immediate feedback).

**Scenario 2: Deploying a Service**
```bash
garden-rake deploy mongodb --on stone-wyse-5070
```
**Result:** Wind chime when service starts (~10s after deploy).

**Scenario 3: High Load**
- Stone CPU reaches 85%
- Cricket gradually transitions from breeze → wind over 30s
- Cricket voice becomes more frequent/strained
- User notices without looking at dashboard

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Sounds are annoying | Beta test with 5+ people, curate carefully |
| Users disable it immediately | Conservative default volume (30%), easy config |
| Memory leak | Load testing, 7-day continuous run |
| Reconnection fails | Exponential backoff, clear error logging |
| Multi-stone voice overlap | Limit to 3 stones, distinct pitch ranges |

---

## Next Steps

1. ✅ **Proposal Complete** — This document
2. 🔲 **Create Repository** — `zen-garden/garden-cricket`
3. 🔲 **Implement Phase 1** — Minimal viable Cricket (2 weeks)
4. 🔲 **Deploy to Wyse 5070** — Internal testing
5. 🔲 **Iterate on Feedback** — Volume, sample quality
6. 🔲 **Implement Phase 2** — Full layer system (2 weeks)
7. 🔲 **Beta Testing** — 2-3 additional stones
8. 🔲 **Implement Phase 3** — Polish + multi-stone (2 weeks)
9. 🔲 **Release v0.1.0** — Production-ready

---

## Documents Created

### 1. Full Specification
**[CRICKET-0001-audio-Companion-spec.md](docs/decisions/CRICKET-0001-audio-Companion-spec.md)**
- Complete specialist team assessments (6 experts)
- Event-to-sound mappings
- Configuration schema
- Audio engine API
- Testing strategy
- Future enhancements (Phase 4-8)

### 2. Implementation Roadmap
**[CRICKET-IMPLEMENTATION-ROADMAP.md](docs/specs/CRICKET-IMPLEMENTATION-ROADMAP.md)**
- Phase-by-phase build plan (3 phases)
- Deliverables & success criteria per phase
- Sample acquisition strategy
- Deployment plan (internal → beta → production)
- Risk mitigation
- Open issues & decisions

### 3. This Summary
Quick reference for stakeholders and future contributors.

---

## Conclusion

**Cricket is feasible, aligned with Zen Garden's philosophy, and ready for implementation.**

The Stone Presence Protocol (PRESENCE-0001) is already implemented and functional in Moss. Cricket will be the first Companion to demonstrate the protocol's value—proving that infrastructure can feel **intimate, tactile, and real** without dashboards or constant monitoring.

**Recommendation:** Proceed with Phase 1 implementation.

---

**Objective Alignment:** ✅ **Achieved**  
**Protocol Compliance:** ✅ **PRESENCE-0001 (no deviations)**  
**Separation of Concerns:** ✅ **Pure consumer, zero Moss coupling**  
**Specialist Consensus:** ✅ **Unanimous approval (6/6 experts)**  
**Timeline:** **6-8 weeks to v0.1.0**

**Status:** Ready for Implementation  
**Last Updated:** 2026-01-26
