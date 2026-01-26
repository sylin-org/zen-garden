# PRESENCE-0001: Stone Presence Protocol

**Status:** Proposal  
**Date:** January 2026  
**Objective:** Make home lab infrastructure feel intimate, tactile, and real.

---

## Executive Summary

The **Stone Presence Protocol** defines a simple, agnostic SSE-based mechanism for Moss to broadcast domain events to local presence adapters. Adapters translate events into sensory feedback—light, sound, text—without Moss knowing or caring about the consumer type.

**Core principle:** Moss emits *what happened*. Adapters decide *how to express it*.

---

## Context

Zen Garden repurposes old hardware. But hardware isn't just useful—it can be *alive*. A Dell Wyse thin client running MongoDB can have a voice (Cricket), a glow (Firefly), a display (OLED), even a scent (future). These are not dashboards. They are *presences*.

Existing proposals:
- **Firefly** (5×5 LED matrix): Visual breathing, sparkles, pond ripples
- **Cricket** (audio): Spatial soundscape, crickets chirping per-stone

Both connect via SSE to Moss and translate events into their medium. This document formalizes the shared protocol.

---

## Specialist Team Assessment

### 1. Dr. Helena Voss — Semiotics Expert

*"Signs, symbols, and the meaning-making process"*

#### Assessment

The proposal correctly separates **signifier** (how it's shown) from **signified** (what it means). Moss emits semantic events (`stone.health.changed`, `service.started`). Adapters perform semiotic translation—green glow means "healthy," rapid chirping means "active."

**Concern:** Event naming must be *domain-anchored*, not *presentation-anchored*. 

❌ `stone.color.green` — This leaks visual semantics into the protocol  
✅ `stone.health.thriving` — This is domain truth; adapters interpret

**Recommendation:** Define an event vocabulary rooted in garden metaphors (thriving, withering, wilting, resting) rather than technical metrics or visual properties.

---

### 2. Marcus Chen — Mnemonics Specialist

*"Memory, pattern recognition, and cognitive load"*

#### Assessment

The goal of *ambient awareness* requires low cognitive load. Users should recognize patterns without conscious parsing.

**Strengths:**
- Firefly: Color-health mapping exploits existing associations (green=good)
- Cricket: Spatial sound creates locational memory ("that high chirp is the database")
- Both: Identity-derived personality (stone name → unique voice/pattern)

**Concern:** Event bursts can overwhelm. If 5 services restart simultaneously, 5 events fire. Adapters must coalesce—but Moss shouldn't.

**Recommendation:** 
- Moss emits every event (completeness)
- Adapters implement debouncing/coalescence (their concern)
- Include `correlation_id` for related event grouping

---

### 3. Dr. Yuki Tanaka — Ambient Computing Researcher

*"Calm technology and peripheral awareness"*

#### Assessment

This proposal aligns with Weiser's calm technology principles:
1. **Periphery to center:** Information lives at the edge of attention until needed
2. **Enchantment over alarm:** Changes are noticed, not alerted

**Strengths:**
- "Rise from silence" philosophy (Cricket)
- Breathing animations vs blinking (Firefly)
- No beeps, no sirens

**Concern:** The line between *noticeable* and *alarming* is thin. A wilting state (critical) must be detectable without triggering fight-or-flight.

**Recommendation:** Protocol should include `severity` alongside `state`, allowing adapters to calibrate their urgency expression. Also: define explicit "nothing requires attention" heartbeats.

---

### 4. Omar Al-Rashid — Distributed Systems Architect

*"Protocol design, failure modes, and resilience"*

#### Assessment

SSE is a reasonable choice for this use case:
- One-way push (Moss → adapters)
- Built-in reconnection
- Text-based, debuggable
- Works through proxies

**Concerns:**

1. **Reconnection state:** When adapter reconnects after network hiccup, it missed events. Protocol must include:
   - `snapshot` event on connect (current state)
   - Periodic `heartbeat` with summary state
   
2. **Event ordering:** Events may arrive out of order. Include `sequence_id` or let adapters be idempotent.

3. **Backpressure:** Cheap ESP8266 can't handle 100 events/second. But this is adapter concern, not protocol concern. Moss emits; adapter filters.

**Recommendation:** Every SSE connection should start with a `presence.snapshot` event containing complete current state.

---

### 5. Elena Vasquez — UX/Sensory Design Lead

*"Multi-modal experience and sensory coherence"*

#### Assessment

The proposal enables *sensory coherence*—the same underlying state can be expressed through sight, sound, and touch simultaneously without conflict.

**Strengths:**
- Same events feed all adapters
- Each adapter owns its expression vocabulary
- Security state (Pond) has consistent tells across media (water sounds, water visuals)

**Concern:** How do multiple adapters coordinate? If Firefly and Cricket both respond to `service.started`, the user experiences both flash and chime. Is this redundant or reinforcing?

**Recommendation:** This is not a protocol concern—it's a deployment choice. Document the principle: *sensory stacking is intentional*. Multiple presences reinforce without conflicting.

---

### 6. Dr. Raj Patel — Software Architecture (DDD/CQRS)

*"Domain-Driven Design, bounded contexts, and separation of concerns"*

#### Assessment

The proposal correctly applies DDD principles:

| Principle | Application |
|-----------|-------------|
| **Ubiquitous Language** | Events use garden vocabulary (thriving, tending, withering) |
| **Bounded Contexts** | Moss (domain) knows nothing about adapters (presentation) |
| **Domain Events** | Protocol emits facts about what happened, not commands |
| **Anti-corruption Layer** | Adapters translate domain events into their native models |

**The key insight:** Moss doesn't emit "turn on green LED" or "play chirp sound." It emits "this stone is thriving." The translation is 100% adapter responsibility.

**Recommendation:** Document that adapters MUST NOT influence Moss behavior. This is a one-way broadcast. Adapters observe; they don't participate.

---

## Group Consensus

After reviewing individual assessments, the team converged on these principles:

### ✅ Agreed: Core Protocol Principles

1. **One-way broadcast**: Moss → Adapters only. Events flow one direction.
2. **Domain semantics**: Events describe *what happened* in domain terms, never presentation terms.
3. **Snapshot on connect**: New connections receive complete current state before incremental events.
4. **Heartbeat for liveness**: Regular heartbeats confirm connection and carry summary state.
5. **Adapter autonomy**: All presentation decisions (color, sound, display) are adapter-local.
6. **Graceful degradation**: If Moss restarts, adapter reconnects.
7. **Optional identification**: Adapters may identify themselves for observability, but it's not required.
8. **Connection-based presence**: SSE connection state = adapter presence. No separate registration.

### ✅ Agreed: Vocabulary Must Be Garden-Native

Events should feel like they belong in Zen Garden:

| Concept | Vocabulary |
|---------|------------|
| Health states | `thriving`, `withering`, `wilting`, `resting` |
| Security | `pond.active`, `pond.inactive` |
| Interaction | `tended`, `observed` |
| Lifecycle | `sprouted`, `transplanted`, `uprooted` |

Technical terms (`cpu_percent`, `container_id`) may appear in payloads but not event names.

### ✅ Agreed: Endpoint Design

```
GET /api/v1/presence/stream
Accept: text/event-stream
```

Why `/presence/`?
- Distinct from `/events/` (internal job events)
- Clearly scoped to external consumer use
- Firefly, Cricket, OLED, web dashboards all connect here

---

## Protocol Specification

### Endpoint

```
GET /api/v1/presence/stream
Accept: text/event-stream
```

### Adapter Identification (Optional)

Adapters may identify themselves via query parameters:

```
GET /api/v1/presence/stream?adapter=cricket&version=0.1.0
GET /api/v1/presence/stream?adapter=firefly&version=0.1.0
GET /api/v1/presence/stream?adapter=oled-esp8266&version=1.0.0
GET /api/v1/presence/stream              # Anonymous — also valid
```

| Parameter | Required | Description |
|-----------|----------|-------------|
| `adapter` | No | Adapter type name (e.g., "cricket", "firefly", "lantern") |
| `version` | No | Adapter version (e.g., "0.1.0") |

Identification enables:
- Dashboard display of attached presence devices
- Debugging ("which adapters are connected?")
- Future: adapter-specific event filtering (opt-in)

Moss does NOT:
- Require identification
- Change behavior based on adapter type
- Validate adapter names or versions

### Connection Lifecycle

```
1. Adapter connects to SSE endpoint (with optional ?adapter=&version=)
2. Moss adds adapter to in-memory subscriber list
3. Moss sends `presence.snapshot` (complete current state)
4. Moss sends incremental events as they occur
5. Moss sends `presence.heartbeat` every 30 seconds
6. On disconnect, Moss removes adapter from subscriber list
7. Adapter reconnects and repeats from step 1
```

### Connection-Based Presence Detection

The SSE connection **is** the presence signal:

| Adapter State | Detection |
|---------------|-----------|
| Connected | SSE connection open |
| Disconnected | SSE connection closed (TCP FIN/RST) |
| Crashed | Connection drops (TCP timeout) |
| Reconnected | New SSE connection (may re-identify) |

No additional protocol needed:
- ❌ No "adapter heartbeat" from adapter → Moss
- ❌ No "are you alive?" pings from Moss → adapter
- ❌ No registration/deregistration endpoints
- ✅ Just track SSE connection state

### Event Format

```
event: <event_type>
data: <JSON payload>

```

All events are JSON. All include `timestamp` (ISO 8601).

### Event Categories

#### Snapshot (Connection Start)

Sent once when adapter connects. Contains complete current state.

```
event: presence.snapshot
data: {
  "stone": {
    "name": "stone-crystal-forest",
    "health": "thriving",
    "load": { "cpu_percent": 12.5, "memory_percent": 45.2 },
    "uptime_seconds": 86400,
    "pond_active": true
  },
  "services": [
    {
      "name": "mongodb",
      "state": "running",
      "health": "healthy",
      "activity": "idle"
    }
  ],
  "timestamp": "2026-01-26T14:30:00Z"
}
```

#### Heartbeat (Liveness)

Sent every 30 seconds. Contains summary state.

```
event: presence.heartbeat
data: {
  "stone_health": "thriving",
  "service_count": 3,
  "services_healthy": 3,
  "pond_active": true,
  "timestamp": "2026-01-26T14:30:30Z"
}
```

#### Stone Events

```
event: stone.health.changed
data: {
  "old": "thriving",
  "new": "withering",
  "reason": "memory_pressure",
  "timestamp": "2026-01-26T14:31:00Z"
}
```

```
event: stone.load.updated
data: {
  "cpu_percent": 78.5,
  "memory_percent": 82.1,
  "disk_percent": 45.0,
  "timestamp": "2026-01-26T14:31:05Z"
}
```

```
event: stone.tended
data: {
  "by": "garden-rake",
  "from": "192.168.1.50",
  "timestamp": "2026-01-26T14:32:00Z"
}
```

#### Service Events

```
event: service.sprouted
data: {
  "service": "mongodb",
  "offering": "mongodb",
  "timestamp": "2026-01-26T14:33:00Z"
}
```

```
event: service.started
data: {
  "service": "mongodb",
  "timestamp": "2026-01-26T14:33:05Z"
}
```

```
event: service.stopped
data: {
  "service": "mongodb",
  "reason": "user_request",
  "timestamp": "2026-01-26T14:34:00Z"
}
```

```
event: service.uprooted
data: {
  "service": "mongodb",
  "timestamp": "2026-01-26T14:35:00Z"
}
```

```
event: service.health.changed
data: {
  "service": "mongodb",
  "old": "healthy",
  "new": "unhealthy",
  "timestamp": "2026-01-26T14:36:00Z"
}
```

```
event: service.activity
data: {
  "service": "mongodb",
  "type": "request",
  "timestamp": "2026-01-26T14:36:01Z"
}
```

#### Security Events

```
event: pond.joined
data: {
  "pond_name": "home-pond",
  "timestamp": "2026-01-26T14:37:00Z"
}
```

```
event: pond.left
data: {
  "timestamp": "2026-01-26T14:38:00Z"
}
```

#### Background Task Events

```
event: task.started
data: {
  "task": "backup",
  "description": "Nightly backup",
  "timestamp": "2026-01-26T03:00:00Z"
}
```

```
event: task.completed
data: {
  "task": "backup",
  "duration_seconds": 120,
  "timestamp": "2026-01-26T03:02:00Z"
}
```

#### Milestone Events

```
event: stone.milestone
data: {
  "type": "uptime",
  "value": "7_days",
  "timestamp": "2026-01-26T14:40:00Z"
}
```

---

## Adapter Architecture

Adapters are independent services that:
1. Connect to Moss SSE endpoint
2. Maintain internal state model
3. Translate events to their medium
4. Handle reconnection gracefully

```
┌─────────────────────────────────────────────────────────────────┐
│  STONE                                                          │
│                                                                 │
│  ┌─────────────┐                                                │
│  │             │                                                │
│  │    Moss     │─────────────────────────────────────────────── │
│  │   :7185     │   GET /api/v1/presence/stream                  │
│  │             │                                                │
│  └──────┬──────┘                                                │
│         │ SSE                                                   │
│         ├───────────────────┬───────────────────┐               │
│         ▼                   ▼                   ▼               │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐       │
│  │   Firefly   │     │   Cricket   │     │ OLED Adapter│       │
│  │  (LED 5x5)  │     │   (Audio)   │     │  (Display)  │       │
│  └──────┬──────┘     └──────┬──────┘     └──────┬──────┘       │
│         ▼                   ▼                   ▼               │
│      [Matrix]           [Speaker]            [Screen]           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

Each adapter is:
- **Optional**: Moss works without any adapters
- **Independent**: Adapters don't coordinate with each other
- **Autonomous**: All rendering logic is adapter-local
- **Resilient**: Reconnects automatically, handles missed events via snapshot

---

## Lantern Aggregation

For garden-wide dashboards, Lantern aggregates presence streams from all stones.

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  LANTERN (Garden-wide dashboard)                                │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  Presence Aggregator                                     │   │
│  │  - Connects to each stone's /api/v1/presence/stream      │   │
│  │  - Identifies as: ?adapter=lantern&version=0.1.0         │   │
│  │  - Maintains composite garden state                      │   │
│  │  - Re-broadcasts as garden-wide stream                   │   │
│  └─────────────────────────────────────────────────────────┘   │
│         │                                                       │
│         ▼                                                       │
│  GET /api/v1/garden/presence/stream  (aggregated SSE)          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
         │
         │ SSE connections (one per stone)
         ▼
┌─────────┐  ┌─────────┐  ┌─────────┐
│ Stone 1 │  │ Stone 2 │  │ Stone 3 │
│  Moss   │  │  Moss   │  │  Moss   │
└─────────┘  └─────────┘  └─────────┘
```

### Lantern Endpoints

**Aggregated presence stream:**
```
GET /api/v1/garden/presence/stream
Accept: text/event-stream
```

**Garden-wide snapshot:**
```
event: garden.snapshot
data: {
  "stones": [
    {
      "name": "stone-crystal-forest",
      "health": "thriving",
      "services": ["mongodb", "redis"],
      "presence": ["cricket", "firefly"]
    },
    {
      "name": "stone-quiet-stream",
      "health": "thriving",
      "services": ["postgres"],
      "presence": []
    }
  ],
  "timestamp": "2026-01-26T14:30:00Z"
}
```

**Aggregated events include stone origin:**
```
event: stone.health.changed
data: {
  "stone": "stone-crystal-forest",
  "old": "thriving",
  "new": "withering",
  "timestamp": "2026-01-26T14:31:00Z"
}
```

### Why Lantern Aggregation?

| Concern | Solution |
|---------|---------|
| Web dashboard needs all stones | Connect to Lantern, not N stones |
| Cricket mixer mode needs garden-wide state | Subscribe to Lantern |
| Mobile app (future) | Single endpoint |
| Reduce client complexity | Lantern manages N connections |

### Lantern Identifies Itself

Lantern connects to each stone as:
```
GET /api/v1/presence/stream?adapter=lantern&version=0.1.0
```

Stones see Lantern in their `/api/v1/presence/subscribers` response.

### Dashboard Experience

A Lantern dashboard can display attached presence devices per stone:

```
┌─────────────────────────────────────────────┐
│  stone-crystal-forest                       │
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━              │
│  ◉ thriving │ mongodb ● redis ●            │
│                                             │
│  Presence:  🔊 Cricket  💡 Firefly          │
└─────────────────────────────────────────────┘

┌─────────────────────────────────────────────┐
│  stone-quiet-stream                         │
│  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━              │
│  ◉ thriving │ postgres ●                   │
│                                             │
│  Presence:  (none)                          │
└─────────────────────────────────────────────┘
```

When Cricket's ESP8266 loses WiFi, the icon disappears in real-time.

---

## Adapter Implementation Notes

### For Firefly (5×5 LED Matrix)

| Event | Visual Response |
|-------|-----------------|
| `presence.snapshot` | Initialize display state |
| `stone.health.changed` | Transition color |
| `stone.load.updated` | Adjust brightness field |
| `service.started` | Ripple outward |
| `service.stopped` | Fade inward |
| `stone.tended` | Happy wiggle |
| `pond.joined` | Transition to water mode |
| `service.activity` | Sparkle/ripple |

### For Cricket (Audio)

| Event | Audio Response |
|-------|----------------|
| `presence.snapshot` | Initialize soundscape |
| `stone.health.changed` | Shift cricket behavior |
| `stone.load.updated` | Adjust weather layer |
| `service.started` | Wind chime |
| `service.stopped` | Chime fades |
| `stone.tended` | Gentle tone |
| `pond.joined` | Fade in water sounds |
| `stone.milestone` | Celebration sounds |

### For OLED Display

| Event | Display Response |
|-------|------------------|
| `presence.snapshot` | Render stone name, status |
| `stone.health.changed` | Update status icon |
| `stone.tended` | Show "tending" with rake icon |
| `service.started` | Brief notification |
| `stone.load.updated` | Update load bar |

### For Web Dashboard

| Event | UI Response |
|-------|-------------|
| `presence.snapshot` | Initialize components |
| All events | Update reactive state |
| `presence.heartbeat` | Confirm connection |

---

## Cricket Integration

The existing Cricket specification describes an audio companion that creates spatial soundscape from infrastructure state. Cricket fits perfectly into this protocol:

### Cricket-Specific Mappings

From the Cricket spec, these mappings apply:

**Layer 2 - Per-Stone Cricket Voice:**
- `stone.health.changed` → Cricket behavior (thriving=occasional chirp, withering=strained, wilting=distressed)
- `stone.load.updated` → Chirp frequency (more load = more frequent)

**Layer 3 - Weather (Garden-Wide):**
- Aggregate `stone.load.updated` from all stones → Weather layer (breeze, rain, wind)
- Any `stone.health.changed` to `wilting` → Unsettled weather

**Layer 4 - Events:**
- `service.sprouted` → Wind chime
- `service.stopped` → Chime fades
- `task.started` (backup) → Owl hoot
- `stone.tended` → Gentle tone
- `stone.milestone` → Dawn chorus / celebration

**Security Tell - Water Sounds:**
- `pond.joined` → Fade in gentle stream/fountain
- `pond.left` → Fade out water sounds

Cricket's "rise from silence" philosophy means:
- On connect, don't blast full soundscape—fade in over 60 seconds
- On `service.started`, gentle onset, not sudden sound
- On `stone.health.changed`, crossfade over 30+ seconds

---

## Event Completeness

The protocol defines these event types:

| Category | Events |
|----------|--------|
| Lifecycle | `presence.snapshot`, `presence.heartbeat` |
| Stone | `stone.health.changed`, `stone.load.updated`, `stone.tended`, `stone.milestone` |
| Service | `service.sprouted`, `service.started`, `service.stopped`, `service.uprooted`, `service.health.changed`, `service.activity` |
| Security | `pond.joined`, `pond.left` |
| Tasks | `task.started`, `task.completed` |

Future extensions may add:
- `stone.resting` (maintenance mode)
- `garden.observed` (Rake connected)
- `discovery.stone_found`, `discovery.stone_lost` (for Lantern dashboards)

---

## Implementation Considerations

### Moss Changes Required

1. **New endpoint**: `GET /api/v1/presence/stream`
2. **Snapshot generation**: Query current state on connect
3. **Event emission**: Emit presence events from domain event handlers
4. **Heartbeat task**: Background task sends heartbeat every 30s
5. **Subscriber tracking**: Track connected adapters in memory (for observability)
6. **Subscriber endpoint**: `GET /api/v1/presence/subscribers`

### Subscriber Endpoint

```
GET /api/v1/presence/subscribers
```

Returns currently connected presence adapters:

```json
{
  "subscribers": [
    {
      "adapter": "cricket",
      "version": "0.1.0",
      "connected_since": "2026-01-26T14:30:00Z"
    },
    {
      "adapter": "firefly",
      "version": "0.1.0",
      "connected_since": "2026-01-26T14:30:05Z"
    },
    {
      "adapter": null,
      "version": null,
      "connected_since": "2026-01-26T14:31:00Z"
    }
  ],
  "count": 3
}
```

This endpoint:
- Returns **live** connection state (not cached)
- Includes anonymous adapters (adapter=null)
- Updates immediately on connect/disconnect
- Is purely observational (no side effects)

### What Moss Must NOT Do

- Modify behavior based on adapter presence
- Include presentation hints in events
- Require adapter identification
- Ping or health-check adapters
- Persist subscriber history

### Adapter Responsibilities

- Parse SSE events
- Maintain internal state model
- Handle reconnection (snapshot re-syncs state)
- Translate events to medium
- Implement debouncing if needed
- Own all rendering/sound/display logic

---

## Example: OLED Adapter for ESP8266

A minimal adapter for the NodeMCU ESP8266 with 0.96" OLED:

```
┌──────────────────────────────────┐
│  stone-crystal-forest            │
│  ━━━━━━━━━━━━━━━━━━━━━           │
│  ◉ thriving                      │
│  ▓▓▓▓▓▓░░░░ 65% load             │
│  mongodb ● redis ●               │
└──────────────────────────────────┘
```

When `stone.tended` arrives:

```
┌──────────────────────────────────┐
│  stone-crystal-forest            │
│  ━━━━━━━━━━━━━━━━━━━━━           │
│        🌿 tending 🌿              │
│                                  │
│                                  │
└──────────────────────────────────┘
```

The adapter:
1. Connects to `http://<stone-ip>:7185/api/v1/presence/stream`
2. Parses `presence.snapshot` for initial state
3. Updates display on each event
4. Reconnects on disconnect

All display layout, icons, and transitions are adapter decisions.

---

## Example: RGB Matrix Adapter

For the RP2040 5×5 RGB LED Matrix, a simple mapping:

| `stone.health` | Color |
|----------------|-------|
| `thriving` | Green (#14B446) |
| `withering` | Amber (#C87814) |
| `wilting` | Coral (#C83C28) |
| `resting` | Dim ember (#502D0F) |

| Event | Animation |
|-------|-----------|
| `service.activity` | Random LED sparkle 100ms |
| `stone.tended` | Diagonal shimmer 400ms |
| `service.sprouted` | Ripple from center 800ms |

Brightness field (columns = time, brightness = load) is adapter-local interpretation of `stone.load.updated` history.

---

## Relationship to Existing Specs

| Spec | Relationship |
|------|--------------|
| **Firefly** | Firefly is a presence adapter implementing this protocol |
| **Cricket** | Cricket is a presence adapter implementing this protocol |
| **API v1** | Presence endpoint joins existing v1 API surface |
| **Domain Events** | Presence events derive from domain events |

This document formalizes what Firefly and Cricket specs already assume.

---

## Security Considerations

1. **Local-only by default**: Presence endpoint should only accept connections from localhost or LAN
2. **No authentication required**: Presence data is not sensitive (health, load)
3. **No adapter authentication**: Adapters are anonymous consumers
4. **Pond status is visible**: This is intentional—security state should be observable

---

## Success Criteria

The protocol succeeds if:

1. **Firefly works**: LED matrix displays correct health, load, and responds to events
2. **Cricket works**: Audio soundscape reflects garden state
3. **New adapter is trivial**: OLED display adapter can be written in a weekend
4. **Moss is unaware**: Zero code in Moss knows about adapters
5. **Reconnection works**: Adapter can disconnect and reconnect without state drift

---

## Open Questions

1. **Activity granularity**: Should `service.activity` fire per-request? Or aggregated (e.g., "10 requests in last second")?
   - *Proposal*: Per-request, let adapter debounce

2. **Load update frequency**: How often to emit `stone.load.updated`?
   - *Proposal*: Every 5 seconds, or on significant change (>5% delta)

### Resolved Questions

3. **Multi-stone adapters**: Should an adapter (like a web dashboard) connect to multiple stones?
   - *Resolved*: Yes, via Lantern's `GET /api/v1/garden/presence/stream`

4. **Should adapters identify themselves?**
   - *Resolved*: Optional, via query params `?adapter=name&version=x.y.z`

5. **How to detect adapter disconnect?**
   - *Resolved*: SSE connection state = adapter presence. No additional protocol.

---

## Decision

**Accepted.** The Stone Presence Protocol will be implemented as specified.

Next steps:
1. Implement `GET /api/v1/presence/stream` in Moss (with adapter identification)
2. Implement `GET /api/v1/presence/subscribers` in Moss
3. Implement `GET /api/v1/garden/presence/stream` in Lantern
4. Update Firefly spec to reference this protocol
5. Update Cricket spec to reference this protocol
6. Create reference adapter implementations

---

**Document Status:** Proposal → Accepted  
**Authors:** Specialist team consensus  
**Last Updated:** January 2026
