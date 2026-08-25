# Defensive Publication: Semiotic Infrastructure Presence Protocol

**Inventor**: Leonardo Milson Botinelly Soares (Leo Botinelly)
**Disclosure Date**: 2026-03-24
**Field**: Ambient computing and infrastructure monitoring through sensory output
**Keywords**: infrastructure sonification, semiotic events, companion devices, spatial audio, presence protocol, ambient awareness

---

## 1. Problem Statement

Infrastructure monitoring is a visual discipline. Operators observe dashboards (Grafana, Datadog), receive text alerts (PagerDuty, OpsGenie), and read log streams. All of these modalities require active screen attention. No existing system translates infrastructure state into ambient sensory signals — sound, light, haptic feedback — using domain-semantic events that peripheral companion devices independently interpret.

The gap is not merely "alerting via sound." Alert-based audio (sirens, beeps) triggers alarm fatigue and operates on a binary model: silent or alarming. The disclosed system instead creates a continuous ambient soundscape where infrastructure health is encoded as density, timbre, and spatial position of natural sounds. The operator perceives the state of their infrastructure at the periphery of attention, without looking at a screen.

### Prior Art Differentiation

| System | Modality | Ambient | Domain Semantics | Companion Architecture | Spatial Audio |
|--------|----------|:---:|:---:|:---:|:---:|
| Nagios/Grafana | Visual dashboards | No | Metrics only | No | No |
| PagerDuty | Text/voice alerts | No (binary: alert or silent) | Alert routing, not events | No | No |
| MQTT (IoT) | Transport protocol | No (transport, not semantics) | No (payload-agnostic) | No | No |
| Ishii's Tangible Bits | Physical objects | Yes | Research prototypes, not software infrastructure | Physical, not software | No |
| Home automation (Hue/LIFX) | Light | Partial | No infrastructure semantics | Centralized hub | No |
| **Disclosed system** | **Sound + Light + Display** | **Yes (continuous)** | **Yes (garden-metaphor vocabulary)** | **Yes (autonomous SSE consumers)** | **Yes (per-node stereo panning)** |

---

## 2. Description of the Invention

### 2.1 Architecture Overview

The disclosed system consists of three layers:

1. **Event Emitter** (Moss daemon): Emits domain-semantic events via Server-Sent Events (SSE). The emitter knows nothing about how events will be rendered. Events describe what happened in infrastructure terms, never in presentation terms.

2. **Presence Protocol**: An SSE-based streaming protocol with snapshot-on-connect, periodic heartbeat, category filtering, and connection-based presence detection. Defined as a contract in shared types; no implementation details leak across the boundary.

3. **Companion Devices**: Independent services that connect to the SSE stream and translate events into their native medium — audio (Cricket), LED matrix (Firefly), OLED display, or web dashboard. Each companion owns all rendering decisions. Companions do not coordinate with each other.

```
Infrastructure Node (Moss :7185)
    |
    | GET /api/v1/stone/presence/stream (SSE)
    |
    +---> Cricket (audio companion)    --> Speaker
    +---> Firefly (LED companion)      --> 5x5 RGB matrix
    +---> OLED companion               --> 0.96" display
    +---> Web dashboard                --> Browser
```

**Companion discovery:** Companions locate their infrastructure node through one of three mechanisms:
1. **Explicit configuration**: The node address is provided via command-line argument (`--stone http://stone-name:7185`), environment variable (`ZG_STONE`), or configuration file. This is the simplest and most reliable mechanism.
2. **mDNS/multicast discovery**: The infrastructure node announces itself via multicast (group `239.255.42.99`, port `7184`). Companions listen for announcements and connect to the first (or preferred) responding node. This enables zero-configuration deployment on a LAN.
3. **Service registry (Lantern)**: Companions query the Lantern service registry (`GET /api/v1/registry/stones`) for available nodes and connect to the one designated as tended (primary).

All three mechanisms produce the same result: a base URL for the node's HTTP API. The companion then appends `/api/v1/stone/presence/stream` to establish the SSE connection. The discovery mechanism is orthogonal to the presence protocol — any companion using any discovery method connects to the same SSE endpoint.

### 2.2 Domain-Semantic Event Vocabulary

Events use a garden metaphor vocabulary rooted in domain concepts, not presentation concepts. This is the critical semiotic property: the protocol emits signified meanings, not signifiers.

**Prohibited**: `stone.color.green` (leaks visual presentation into protocol)
**Required**: `stone.health.thriving` (domain truth; companions interpret)

The complete event vocabulary:

| Category | Event Types |
|----------|-------------|
| Lifecycle | `presence.snapshot`, `presence.heartbeat` |
| Stone | `stone.health.changed`, `stone.load.updated`, `stone.tended`, `stone.milestone` |
| Service | `service.sprouted`, `service.started`, `service.stopped`, `service.uprooted`, `service.health.changed`, `service.activity` |
| Storage | `storage.detected`, `storage.removed`, `storage.sync.started`, `storage.sync.completed` |
| Jobs | `job.started`, `job.progress`, `job.completed`, `job.failed` |
| Security | `pond.joined`, `pond.left` |

Health states use garden vocabulary: `thriving` (healthy), `withering` (degraded), `wilting` (critical), `resting` (maintenance mode).

#### Implementation Evidence

- Event type constants in `src/common/src/presence/mod.rs` — `event_types` module.
- `PresenceSnapshot` struct in `src/common/src/presence/types.rs` — `StoneState` with `health: String` field.
- `compute_health()` in `src/moss/src/api/v1/presence.rs` — maps CPU/memory thresholds to `thriving`/`withering`/`wilting`.
- ADR: `docs/decisions/PRESENCE-0001-stone-presence-protocol.md`.

### 2.3 SSE Streaming with Snapshot-on-Reconnect

**Terminology:** "Presence" in this disclosure has a single meaning: the ambient awareness of infrastructure state. The SSE endpoint (`/presence/stream`) carries presence data. The protocol defines how that data is structured and delivered. Connection-based presence detection (knowing that a companion is connected by virtue of its open SSE connection) is a derived property of using SSE as the transport — not a separate concept. The term is not overloaded; the endpoint, protocol, and detection mechanism all serve the same purpose: making infrastructure state present to companion devices.

The presence stream follows a convergence protocol:

1. **On connect**: The server generates a complete `presence.snapshot` event from current infrastructure state (node health, all offerings with status, security posture, resource metrics, GPU state, network rates). This is emitted as the first SSE event.

2. **Incremental events**: As infrastructure state changes, domain events are emitted through a pulse channel. The presence endpoint filters for domain events only (transport events are excluded), applies category filtering if requested, and translates each event to the presence vocabulary.

3. **Heartbeat**: Every 30 seconds, the server emits a `presence.heartbeat` summarizing current state.

4. **Reconnection**: On disconnect, companions reconnect using standard SSE semantics. The new connection receives a fresh snapshot, ensuring convergence regardless of missed events. The snapshot is a best-effort point-in-time view: the server reads current state from its in-memory data structures (which are updated by independent background tasks). Because the snapshot aggregates data from multiple `Arc<RwLock<T>>` fields, it is not transactionally consistent across all fields — node health may reflect a slightly more recent state than the offerings list. This is acceptable because (a) the incremental events that follow the snapshot will converge any stale fields within seconds, and (b) companions are designed for ambient awareness, not transactional consistency. The snapshot serves as a "close enough" starting point from which the incremental stream converges to exact state.

5. **Graceful shutdown**: When the server shuts down, it emits a `server.shutdown` event before closing the stream.

**Event ordering and priority:** Events are emitted in the order they are produced by the infrastructure — the SSE stream preserves causal ordering within a single node. The protocol does not reorder events by priority; all events are treated equally in the stream. Priority is a companion-local rendering concern: a companion may choose to interrupt ambient audio for a critical health event, but this decision is made by the companion's event handler, not by the protocol. This design is intentional — priority ordering in the stream would embed rendering assumptions (which events are "important") into the protocol, violating the semiotic boundary. A companion rendering audio may prioritize health events; a companion rendering a status display may prioritize service events. The protocol cannot know which is correct.

The category filter (`?categories=service,stone`) allows resource-constrained companions (ESP8266 microcontrollers) to subscribe only to relevant event types.

#### Pseudocode: Presence Stream

```
function stream_presence(categories_filter):
    token = shutdown_token.child_token()
    filter = parse_categories(categories_filter)

    snapshot = generate_snapshot_from_current_state()
    emit SSE event { type: "presence.snapshot", data: snapshot }

    subscribe to pulse_channel
    loop:
        select:
            event = receive from pulse_channel:
                if event is DomainEvent AND filter.allows(event.category):
                    sse_event = event.to_presence_event()
                    emit sse_event
                if event is TransportEvent:
                    skip  // presence is domain-only
            event = receive from pulse_channel (lagged):
                // Broadcast channel overflowed — companion missed events.
                // Log the lag count and continue. The next heartbeat (every 30s)
                // will carry summary state, converging the companion's view.
                // Do NOT disconnect — SSE reconnection would trigger a full
                // snapshot regeneration, which is more expensive than skipping
                // a few events that the heartbeat will cover.
                log_warn("presence stream lagged, skipped N events")
                continue
            token cancelled:
                emit SSE event { type: "server.shutdown", data: {} }
                break
```

#### Implementation Evidence

- `stream_stone_presence()` in `src/moss/src/api/v1/presence.rs`.
- `generate_snapshot()` function reading from `AppState` — offerings, metrics, GPU, network, storage, companions.
- `EventFilter` struct in `src/common/src/presence/types.rs` with `allows()` method.

### 2.4 Companion-Agnostic Interpretation

The same event stream drives fundamentally different sensory outputs. The protocol contains no rendering hints. Each companion independently decides how to express each event:

| Event | Cricket (Audio) | Firefly (LED) | OLED Display |
|-------|-----------------|---------------|--------------|
| `stone.health.changed` → `withering` | Cricket chirps become strained/irregular (30s crossfade) | Color transitions from green to amber | Status icon changes |
| `service.started` | Wind chime (brief, pentatonic) | Ripple animation outward from center | Brief notification text |
| `stone.tended` | Single clear chime (immediate) | Diagonal shimmer animation (400ms) | "Tending" with rake icon |
| `pond.joined` | Water sounds fade in (60s) | Transition to water-mode animation | Show pond icon |

This companion-agnostic design is not merely "different clients rendering the same data." The disclosed system establishes a formal semiotic boundary: the emitter produces domain signifieds (what happened), and each companion provides its own signifiers (how to express it). The mapping is entirely companion-local.

#### Formal Semiotic Constraint

The semiotic property is enforced by a concrete architectural constraint: **the event vocabulary contains no presentation-layer terms**. Specifically:

1. **No color references**: Events never contain `red`, `green`, `amber`, `blue`, or any color-space value. The health state `withering` carries no color implication.
2. **No audio references**: Events never contain `beep`, `chime`, `alert-sound`, `volume`, or frequency values.
3. **No layout references**: Events never contain `position`, `x`, `y`, `left`, `right`, or spatial coordinates.
4. **No temporal rendering hints**: Events never contain `duration`, `fade_ms`, `animation_speed`, or timing values.

This is not a naming convention — it is a structural constraint on the `event_types` module. Any event type that passes a presentation hint to a companion violates the architecture and must be rejected. The constraint ensures that the same event stream can drive fundamentally incompatible output modalities (audio, tactile, visual, olfactory) because no event assumes a rendering capability.

This distinguishes the disclosed system from MQTT-based IoT systems (which are payload-agnostic transports), from Grafana/PagerDuty (which embed visual presentation in their data model), and from ambient display research (Ishii et al., ambientROOM 1998; Wisneski et al., Ambient Displays 1998) which used physical artifacts with hardwired mappings between data sources and physical actuators rather than a domain-semantic event protocol that decouples emitters from interpreters.

**Modality extensibility:** The semiotic boundary enables companions for any sensory modality, not only the three reference implementations (audio, LED, display). The protocol's prohibition on presentation-layer terms means the same event stream can drive:
- **Haptic companions**: Vibration motors where event density maps to vibration intensity, health state maps to vibration pattern (smooth = thriving, staccato = withering), and node identity maps to vibration location on a wearable device.
- **Olfactory companions**: Scent diffusers where ambient health maps to scent blend (pleasant botanical = thriving, sharp/acrid = critical), and event density maps to diffusion rate.
- **Tactile surface companions**: Shape-changing surfaces (e.g., pneumatic pin arrays) where infrastructure topology maps to surface relief and load maps to pin height.
- **Environmental companions**: HVAC or lighting systems where infrastructure health subtly influences room temperature setpoint or light color temperature.

No protocol change is required for any new modality — the companion implements its own mapping from domain events to its native output. The disclosed system explicitly covers all sensory modalities reachable through the domain-semantic event vocabulary.

### 2.5 Four-Channel Spatial Audio Mixer

The Cricket audio companion implements a 4-channel mixer architecture where each channel serves a distinct perceptual layer:

```
enum Channel {
    Foreground,   // Notifications, alerts (high priority, interrupts current sound)
    Midground,    // UI feedback, confirmations
    Ambient,      // Continuous nature sounds (looping)
    Background,   // Pads, drones (continuous low-level)
}
```

Each channel has:
- An independent audio output (implemented as `rodio::Sink` in the reference implementation, but equivalently implementable via Web Audio API `GainNode` chains, CoreAudio `AUGraph`, ALSA `snd_pcm` streams, PulseAudio streams, or any audio API that supports concurrent playback with independent volume control).
- Per-channel volume control (`0.0 - 1.0`), applied as a linear gain multiplier.
- Master volume attenuation applied multiplicatively to all channels (`effective_volume = channel_volume * master_volume`).
- Independent start/stop without affecting other channels.
- Support for looping (ambient/background) and one-shot (foreground/midground) playback.

The mixer manages channel contention: playing a new sample on an occupied channel stops the existing playback on that channel before starting the new sample. This is a preemptive model (new content always wins), not a mixing model (no summing of concurrent audio on the same channel).

#### Implementation Evidence

- `Mixer` struct in `src/cricket/src/mixer.rs` — `channels: Arc<RwLock<[Option<ChannelState>; 4]>>`.
- `Channel` enum with `Foreground`, `Midground`, `Ambient`, `Background` variants.
- `play_bytes()`, `stop()`, `set_master_volume()`, `set_channel_volume()` methods.
- `rodio` crate for audio output via `OutputStream` and `Sink`.

### 2.6 Stone-Derived Voice Profiles

Each infrastructure node produces a unique audio identity derived deterministically from its name. The derivation uses a hash function:

```
voice_profile_index = hash(stone_name) % NUM_PROFILES
```

The reference implementation uses a simple string hash (sum of byte values, equivalent to Java's `String.hashCode()` approach) with `NUM_PROFILES = 5`. Any deterministic hash function produces the same result: FNV-1a, CRC32, SHA-256 truncated to u32, djb2, or similar. The choice of hash function does not affect the design — only the requirement that the function is deterministic (same input always produces same output) and reasonably distributed.

The reference implementation uses 5 pre-designed timbral profiles, each defined as a set of audio resource files (WAV/OGG) bundled with the companion binary. The profiles are distinguished by perceptual characteristics: (1) high register (~3kHz fundamental), (2) low register (~1.8kHz), (3) mid-range with amplitude modulation (tremolo at ~6Hz), (4) sine-dominant with minimal harmonics, (5) harmonically rich with strong overtones. The specific timbres are not part of the invention — the invention is the deterministic mapping from node identity to a profile index via hashing, ensuring consistent audio identity across restarts and companion instances.

**Alternative timbre generation approaches** (all covered by this disclosure):
- **Sample-based**: Pre-recorded audio files per profile, selected by index. This is the reference implementation approach.
- **Procedural synthesis**: Hash bits directly parameterize a synthesizer — e.g., bits 0-7 set fundamental frequency (200Hz-4kHz), bits 8-11 set waveform (sine/triangle/saw/square), bits 12-15 set attack/decay envelope, bits 16-23 set harmonic ratios. This produces unique timbres per node without pre-designed profiles.
- **Instrument mapping**: Hash index selects from a bank of sampled instruments (piano, marimba, bell, flute, etc.).
- **Concatenative**: Hash selects segments from a corpus of nature sounds (different bird species, different water sounds).

The inventive step is the deterministic identity-to-timbre binding, not the specific timbre design.

The voice profile is deterministic: the same node name always produces the same voice, even across restarts or when heard from different companion instances. Over time, the operator forms spatial memory associations: "that high chirp is the database node."

### 2.7 Stereo Panning for Multi-Node Spatial Awareness

In multi-node deployments, the audio companion positions each node's sounds in the stereo field. Node A is panned left, Node B panned right, Node C centered. This creates spatial awareness: the operator can locate which node produced an event by the perceived direction of the sound.

Panning positions are assigned deterministically from the node's position in the sorted node list, distributing evenly across the stereo field:

```
pan_position = (index_in_sorted_list / (total_nodes - 1)) * 2.0 - 1.0
// Result: -1.0 (full left) to +1.0 (full right)
// Single node: centered (0.0)
```

**Multi-node connection model**: A companion may connect to a single tended node's presence stream (which aggregates events from all nodes via the garden topology) or connect to multiple individual node streams. In the multi-connection case, the companion maintains a merged node list from all connections, sorted by node name. Panning positions are recalculated whenever the node list changes (node joins or leaves). Events from a newly discovered node are assigned the recalculated pan position immediately.

**Stereo panning implementation:** The pan position (-1.0 to +1.0) is applied to audio samples using constant-power panning (equal-power stereo law):

```
function apply_pan(samples_mono, pan_position):
    // pan_position: -1.0 (full left) to +1.0 (full right)
    angle = (pan_position + 1.0) * PI / 4    // 0 to PI/2
    left_gain  = cos(angle)
    right_gain = sin(angle)

    for each sample in samples_mono:
        left_channel  = sample * left_gain
        right_channel = sample * right_gain
        output stereo frame (left_channel, right_channel)
```

This is applied at the mixer level before writing to the audio output. The reference implementation converts mono audio resources to stereo with per-node panning. When using `rodio`, this is implemented as a custom `Source` adapter wrapping the mono source with gain-split stereo output. Equivalent implementations exist in Web Audio API (`StereoPannerNode`), CoreAudio (matrix mixer AU), PulseAudio (`pa_cvolume` per-channel), or any audio framework supporting per-channel gain control. The constant-power law ensures that a centered sound (pan = 0.0) has the same perceived loudness as a fully-panned sound, avoiding the 3dB dip of linear panning.

### 2.8 Event Debouncing

When multiple simultaneous events of the same type arrive (e.g., 5 services starting during a deployment), the companion debounces them into a single composite sound. The debounce mechanism tracks the last fire time per event type and suppresses duplicate events within a configurable window:

```
struct DebounceState {
    last_fired: HashMap<String, Instant>,
}

function can_fire(event_type, debounce_ms):
    if debounce_ms == 0:
        return true
    now = Instant::now()
    if last_fired[event_type] exists AND (now - last_fired[event_type]) < debounce_ms:
        return false
    last_fired[event_type] = now
    return true
```

This prevents cascading sound events during bulk operations while preserving isolated event sounds.

#### Implementation Evidence

- `DebounceState` in `src/cricket/src/events.rs` with `can_fire()` method.
- `CricketEvents::on_event()` checking debounce before playing audio.

### 2.9 Rise from Silence Design Philosophy

The disclosed system inverts the conventional alerting model. Instead of silence being the default and alerts being loud, the system starts from near-silence and increases sound density (not volume) as activity increases:

- **Idle**: Very occasional cricket chirps (1 per 60 seconds minimum heartbeat).
- **Normal activity**: Periodic chirps, gentle breeze ambient.
- **High activity**: Increased chirp frequency, wind sounds, occasional chimes.
- **Critical**: Strained/irregular chirps, rain sounds. Not louder — different quality.
- **Catastrophic failure**: Sudden silence (the most alarming signal).

Volume is configured once (default 30%) and does not change dynamically. The sound density and timbral quality communicate state, not amplitude.

**Concrete density mapping:** The audio companion maps infrastructure activity to sound density using event rate thresholds. The event rate is computed as a sliding window count of domain events received in the last 60 seconds:

| Event Rate (events/min) | Activity Level | Audio Behavior |
|--------------------------|----------------|----------------|
| 0-1 | Idle | Heartbeat chirp every 60s; ambient channel silent |
| 2-5 | Low | Chirps on events; ambient channel plays gentle loop at 50% volume |
| 6-15 | Normal | All event sounds play; ambient at 80%; midground active |
| 16-40 | High | All channels active; ambient crossfades to wind/activity loop; foreground chimes overlap |
| 41+ | Storm | Ambient switches to rain/storm loop; chirps become rapid/strained (pitch variance +/- 15%); all channels saturated |
| Drop from >15 to 0 within 10s | Catastrophic | All channels fade to silence over 3 seconds (abrupt silence = alarm signal) |

These thresholds are configurable per tune manifest. The reference implementation uses the values above as defaults. The transition between activity levels uses a 10-second smoothing window to prevent rapid oscillation at threshold boundaries. Timbral quality changes (strained chirps, pitch variance) are achieved by selecting alternative audio resources from the tune manifest — the tune defines separate resources for normal and stressed variants of each sound.

### 2.10 YAML-Based Tune System

Event-to-sound mappings are declarative, defined in YAML tune manifests:

```yaml
name: "garden-night"
version: "1.0.0"
description: "Nighttime garden soundscape"
fallback: "chirp-default.wav"
events:
  service.started:
    resource: "chime-wind.wav"
    channel: "foreground"
    debounce_ms: 5000
    looping: false
    volume: 1.0
  stone.health.changed:
    resource: "weather-transition.wav"
    channel: "ambient"
    debounce_ms: 30000
    looping: true
    volume: 0.8
```

Tunes can be embedded in the binary or loaded from the filesystem. Filesystem tunes override embedded tunes with the same name, enabling customization without recompilation. The override is whole-tune replacement, not per-event merge: if a filesystem tune named `garden-night` exists, it entirely replaces the embedded `garden-night` tune. This means a filesystem tune must be complete — it cannot selectively override a single event mapping from the embedded tune while inheriting the rest. This simplicity is intentional: partial merging creates ordering ambiguities (which field wins?) and makes it impossible to reason about the active mappings by reading a single file. Each tune defines its own event-to-resource mappings, channel assignments, debounce intervals, and looping behavior.

#### Implementation Evidence

- `TuneManifest` struct in `src/cricket/src/manifest.rs` with `events: HashMap<String, EventMapping>`.
- `EventMapping` struct with `resource`, `channel`, `debounce_ms`, `looping`, `volume` fields.
- `EmbeddedTunes` via `rust_embed` for compiled-in tune assets.
- `TuneSource` enum (`Embedded`, `Filesystem(PathBuf)`) for resolution priority.

### 2.11 Correlation IDs for Related Events

Events may include correlation identifiers that allow companions to group related events. For example, a deployment operation that starts 5 services emits 5 `service.started` events with a shared correlation ID. Companions use this to coalesce rendering (one chime for the group, not five).

**Differentiation from Alertmanager grouping:** Prometheus Alertmanager groups related alerts by label sets and applies silencing/inhibition rules to suppress duplicate notifications. This operates at the alert routing layer with static grouping rules. The disclosed correlation ID mechanism differs in three ways: (1) correlation IDs are set by the operation that produces events (the deployment job), not by a downstream routing engine — the correlation is causal, not heuristic; (2) the grouping decision is made by each companion independently (the companion decides whether to coalesce correlated events into a single rendering), not by a centralized alert router; (3) the mechanism applies to all event types (including positive events like service starts), not only to alert/error conditions. The debouncing mechanism (§2.8) is complementary — it handles temporal clustering of events of the same type, while correlation IDs handle semantic grouping of events from the same operation.

### 2.12 Client-Initiated Presence Notifications

The protocol supports bidirectional notification for specific interaction events. A CLI tool can POST a `ClientNotification` to the node, which emits it as a domain event to all connected companions:

```
POST /api/v1/stone/presence/notify
{
  "event_type": "tended",
  "client": "rake",
  "from_host": "leo-laptop",
  "message": "Tending started"
}
```

This triggers a `stone.tended` event on all companions — a clear chime on Cricket, a shimmer on Firefly — providing tactile feedback that a human is interacting with the node.

#### Implementation Evidence

- `notify_presence()` in `src/moss/src/api/v1/presence.rs`.
- `ClientNotification` struct in `src/common/src/presence/types.rs`.
- `StoneEvent::tended()` factory method.

---

## 3. Claims

1. A method for translating infrastructure state into ambient sensory output comprising: emitting domain-semantic events from infrastructure nodes using a vocabulary rooted in domain concepts rather than presentation concepts; streaming said events via Server-Sent Events to one or more companion devices; each companion device independently interpreting the domain events into its native sensory medium (audio, light, display); wherein the emitter contains no knowledge of or coupling to any companion's rendering logic.

2. A presence protocol for infrastructure state convergence comprising: emitting a complete state snapshot as the first SSE event upon companion connection; streaming incremental domain events thereafter; emitting periodic heartbeat events carrying summary state; supporting category-based event filtering via query parameters; using SSE connection state as the sole presence detection mechanism without separate registration or health-check protocols.

3. A four-channel spatial audio mixer for infrastructure sonification comprising: four named audio channels (foreground, midground, ambient, background) serving distinct perceptual layers; per-channel volume control with master volume attenuation; independent playback lifecycle per channel (looping for ambient/background, one-shot for foreground/midground); channel contention management where new playback on an occupied channel replaces existing playback.

4. A method for deriving unique audio identity per infrastructure node comprising: applying a deterministic hash function to the node's name or identifier; mapping the hash output to one of a fixed set of pre-designed timbral profiles; wherein the same node always produces the same voice across restarts and across different companion instances; enabling operators to form spatial memory associations between sounds and infrastructure nodes.

5. A stereo panning system for multi-node infrastructure awareness comprising: assigning each node a deterministic position in the stereo field based on its sorted position in the node list; spatially separating simultaneous sounds from different nodes; enabling operators to identify the source node of an event by perceived sound direction.

6. An event debouncing mechanism for infrastructure sonification comprising: tracking the last fire time per event type in a companion-local state; suppressing duplicate events within a configurable time window; allowing bulk infrastructure operations (deployments, restarts) to produce a single composite sound rather than cascading repetitions; wherein the debounce interval is configurable per event type via declarative tune manifests.

7. A rise-from-silence design for ambient infrastructure monitoring comprising: a default state of near-silence with minimal periodic sounds; increasing sound density proportional to infrastructure activity without increasing volume; encoding critical states through timbral quality changes rather than amplitude increases; using sudden silence as the most alarming signal (indicating catastrophic failure); wherein the system operates at the periphery of operator attention without demanding active monitoring.

8. A declarative tune system for event-to-sound mapping comprising: YAML manifest files defining per-event mappings to audio resources, channels, debounce intervals, looping behavior, and volume; support for embedded and filesystem-loaded tunes with filesystem override priority; per-tune fallback resources for unmapped events; enabling customization of the sonification experience without code changes.

---

## 4. Implementation Evidence

| Component | Location |
|-----------|----------|
| Presence SSE endpoint | `src/moss/src/api/v1/presence.rs` — `stream_stone_presence()` |
| Presence types | `src/common/src/presence/types.rs` — `PresenceSnapshot`, `StoneState`, `EventFilter` |
| Presence event constants | `src/common/src/presence/mod.rs` — `event_types` module |
| 4-channel mixer | `src/cricket/src/mixer.rs` — `Mixer`, `Channel` enum |
| Event handler with debounce | `src/cricket/src/events.rs` — `CricketEvents`, `DebounceState` |
| Tune manifest system | `src/cricket/src/manifest.rs` — `TuneManifest`, `EventMapping` |
| Client notification | `src/moss/src/api/v1/presence.rs` — `notify_presence()` |
| Presence protocol spec | `docs/decisions/PRESENCE-0001-stone-presence-protocol.md` |
| Cricket spec | `docs/decisions/CRICKET-0001-audio-companion-spec.md` |

---

## 5. Public Domain Dedication

This document is published as a defensive disclosure to establish prior art. The inventor(s) dedicate this disclosure to the public domain and assert no patent rights over the described inventions. All rights to use, implement, and build upon these inventions are hereby granted to the public.

---

## Antagonist Review Log

### Pass 1
**Antagonist:** (1) Reproducibility gap: stereo panning described but no audio API integration or gain calculation specified. (2) Scope hole: only 3 companion modalities described; haptic/olfactory left open. (3) Abstraction gap: "rise from silence" is philosophy without concrete density-to-sound mapping thresholds. (4) Missing edge case: snapshot consistency during generation not specified.
**Author revision:** Added constant-power stereo panning formula with pseudocode and cross-platform API equivalents. Added modality extensibility section covering haptic, olfactory, tactile surface, and environmental companions. Added concrete event-rate-to-activity-level threshold table with smoothing window. Added snapshot consistency note (best-effort point-in-time with heartbeat convergence).

### Pass 2
**Antagonist:** (1) Scope hole: no event prioritization or ordering guarantees. (2) Prior art weakness: Prometheus Alertmanager grouping/silencing not differentiated. (3) Reproducibility gap: voice profile timbres described qualitatively, not generatively.
**Author revision:** Added event ordering section explaining priority as companion-local concern (not protocol-level). Added Alertmanager differentiation for correlation IDs (causal vs. heuristic, companion-local vs. centralized, all events vs. alerts-only). Expanded voice profile section with four alternative timbre generation approaches (sample-based, procedural synthesis with bit mapping, instrument bank, concatenative) and clarified the inventive step is the deterministic binding, not the timbre design.

### Pass 3
**Antagonist:** (1) Scope hole: companion discovery mechanism not described. (2) Missing edge case: tune manifest override semantics (whole-tune vs. per-field merge) undefined.
**Author revision:** Added companion discovery section with three mechanisms (explicit config, mDNS/multicast, service registry). Clarified tune override as whole-tune replacement with rationale for rejecting per-field merge.

### Pass 4
**Antagonist:** (1) Terminology drift: "presence" overloaded across endpoint, protocol, and detection. (2) Missing edge case: broadcast channel lag behavior unspecified.
**Author revision:** Added terminology clarification establishing single meaning of "presence." Added lag handling to pseudocode (warn and continue, heartbeat converges).

### Pass 5
**Antagonist:** No further objections — this disclosure is sufficient to block patent claims on the described invention.

### Final Status
CLEARED — Antagonist found no further weaknesses. Safe to publish.
