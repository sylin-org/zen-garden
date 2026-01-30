# Presence Protocol - Code Assessment & Implementation Proposal

**Date:** January 26, 2026  
**Phase:** 1 - Moss Endpoints  
**Author:** Code Assessment  
**Revision:** 2 (DDD/SoC/YAGNI/KISS/DRY)

---

## Executive Summary

**Revised architecture based on DDD/SoC/YAGNI/KISS/DRY principles:**

✅ **Reuse EventBus** - Don't create separate broadcast channel (DRY)  
✅ **Extend DomainEvent** - Add `StoneEvent`, reuse existing infrastructure (KISS)  
✅ **Filter at boundary** - SSE endpoint filters to local + relevant events only (Efficiency)  
✅ **SSE endpoint translates** - Companion pattern at boundary (DDD/SoC)  
✅ **No subscriber tracking** - Not needed for protocol (YAGNI)  
✅ **Types in common** - Only reusable protocol contracts (Composability)  

**Key insight:** Presence is just an SSE view over domain events with garden-native translation + smart filtering.

---

## 📊 Comparison: Original vs Revised

| Aspect | Original Proposal ❌ | Revised ✅ | Principle |
|--------|---------------------|-----------|-----------|
| Event broadcast | Separate `presence_tx` | Reuse EventBus/`event_tx` | DRY |
| Event types | New `PresenceEvent` enum | Extend `DomainEvent` | KISS |
| Event filtering | None (Companions filter) | SSE endpoint filters | Efficiency |
| URI semantics | `/api/v1/presence/stream` | `/api/v1/stone/presence/stream` | API-0001 |
| Subscriber tracking | `Arc<RwLock<Vec<...>>>` | None | YAGNI |
| Bridge module | `presence_bridge.rs` | Emit directly | KISS |
| API endpoints | 2 (`/stream`, `/subscribers`) | 1 (`/stone/presence/stream`) | YAGNI |
| Translation layer | Separate module | SSE endpoint boundary | SoC |
| Events/minute to Companion | ~100 (all events) | ~20 (filtered) | Efficiency |
| Lines of code | ~800 | ~300 | KISS |
| Implementation time | 12 hours | 6 hours | Efficiency |

---

## Current Architecture Analysis (Original Assessment)

> **Note:** The analysis below led to the original proposal, which has been revised.
> See "Revised Proposal" section for the simplified approach.

### 1. SSE Infrastructure (Already Exists)

**File:** `src/moss/src/api/v1/events.rs`

```rust
// Current implementation
pub async fn stream_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_tx.subscribe();  // ← Subscribe to broadcast channel
    
    let stream = BroadcastStream::new(rx)
        .filter_map(|result| match result {
            Ok(event) => Some(Ok(event)),
            Err(BroadcastStreamRecvError::Lagged(n)) => {
                tracing::warn!("SSE client lagged {} messages", n);
                None
            }
        })
        .map(|event_result| {
            let event = event_result.unwrap();
            let data = serde_json::to_string(&event).unwrap_or_default();
            Event::default().event("moss-event").data(data)
        })
        .map(Ok);
    
    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

**Pattern:**
1. Handler extracts `AppState`
2. Subscribes to broadcast channel (`state.event_tx.subscribe()`)
3. Wraps in `BroadcastStream` (handles lagging)
4. Maps events to SSE format
5. Returns `Sse` with keep-alive

**This is exactly what we need for `/api/v1/presence/stream`.**

---

### 2. Broadcast Channel (Already Exists)

**File:** `src/moss/src/app_state.rs`

```rust
pub struct AppState {
    // ...existing fields...
    
    /// Event broadcast channel for SSE streaming
    pub event_tx: tokio::sync::broadcast::Sender<MossEvent>,
    
    // ...other fields...
}
```

**File:** `src/moss/src/bootstrap/run.rs` (initialization)

```rust
// Channel created with capacity 100
let (event_tx, _) = tokio::sync::broadcast::channel::<MossEvent>(100);
```

**How it works:**
- `broadcast::channel` creates a multi-producer, multi-consumer channel
- Multiple subscribers can receive the same event
- If a subscriber lags (slow network), lagged messages are dropped with warning
- Sender is stored in `AppState`, subscribers created on-demand via `.subscribe()`

**For presence:**  
We'll add a **second broadcast channel** for presence events:
```rust
pub presence_tx: tokio::sync::broadcast::Sender<PresenceEvent>,
```

Why separate?
- Different event vocabulary (garden-native vs technical)
- Different subscribers (Companions vs debugging tools)
- Independent buffering/backpressure
- Clean separation of concerns

---

### 3. Event Emission (Already Exists)

**File:** `src/moss/src/api/v1/events.rs`

```rust
pub fn emit_event(state: &AppState, level: &str, message: String, job_id: Option<String>) {
    let event = MossEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: level.to_string(),
        message: message.clone(),
        job_id,
    };
    
    // Broadcast to SSE subscribers (ignore if no receivers)
    let _ = state.event_tx.send(event);
    
    // Also log to tracing
    match level {
        "error" => tracing::error!("{}", message),
        "warn" => tracing::warn!("{}", message),
        "debug" => tracing::debug!("{}", message),
        _ => tracing::info!("{}", message),
    }
}
```

**Pattern:** Single function to emit events from anywhere in codebase.

**For presence:**  
We'll create `emit_presence_event()`:
```rust
pub fn emit_presence_event(state: &AppState, event: PresenceEvent) {
    let _ = state.presence_tx.send(event);
}
```

**Called from:**
- Service operations (start, stop, install)
- Load monitor task (every 5s)
- Health computation task
- API handlers (when Rake connects)

---

### 4. Domain Events (Already Exists)

**File:** `src/common/src/events/domain_events.rs`

```rust
pub enum ServiceEvent {
    InstallStarted { ... },
    InstallCompleted { ... },
    Started { ... },
    Stopped { ... },
    Removed { ... },
    HealthChanged { ... },
}
```

**These are the source events for presence.**

**Mapping:**
| Domain Event | Presence Event |
|--------------|----------------|
| `ServiceEvent::InstallCompleted` | `service.sprouted` |
| `ServiceEvent::Started` | `service.started` |
| `ServiceEvent::Stopped` | `service.stopped` |
| `ServiceEvent::Removed` | `service.uprooted` |
| `ServiceEvent::HealthChanged` | `service.health.changed` |

---

### 5. Background Tasks (Already Exists)

**File:** `src/moss/src/tasks/announcer.rs`

```rust
pub async fn run_announcer_task(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    let mut last_hash = None;
    let mut last_announcement = Instant::now();
    
    loop {
        interval.tick().await;
        
        let entry = state.self_entry.read().await.clone();
        
        match crate::announcement::announce_if_changed(
            &entry,
            &mut last_hash,
            &mut last_announcement,
            false,
        ).await {
            Ok(announced) => {
                if announced {
                    tracing::debug!("Topology announcement sent");
                }
            }
            Err(e) => tracing::warn!(error = ?e, "Announcement failed"),
        }
    }
}
```

**Pattern:**
- `tokio::spawn` background task
- `tokio::time::interval` for periodic execution
- Access to `AppState` for reading state
- Error handling with tracing

**For presence:**  
We'll create `run_presence_heartbeat_task()`:
```rust
pub async fn run_presence_heartbeat_task(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    
    loop {
        interval.tick().await;
        
        let heartbeat = generate_heartbeat(&state).await;
        emit_presence_event(&state, PresenceEvent::Heartbeat(heartbeat));
    }
}
```

---

## 🔄 **REVISED PROPOSAL (DDD/SoC/YAGNI/KISS/DRY)**

### Core Principles Applied

1. **Reuse EventBus** - Don't create separate broadcast channel (DRY)
2. **Extend DomainEvent** - Add presence events as new variant (SoC)
3. **SSE endpoint translates** - Companion pattern at boundary (DDD)
4. **Remove subscriber tracking** - Not needed for protocol (YAGNI)
5. **No bridge module** - Emit DomainEvents directly (KISS)
6. **Types in common** - Only protocol contracts (Reusability)

### Architecture (Simplified)

```
Emission Sites → DomainEvent → EventBus → SSE Handler (filter + translate) → Companions
                     ↑                           ↓
              (already exists)        (local + relevant only)
```

**Not:** ~~DomainEvent → PresenceBridge → PresenceEvent → presence_tx → SSE → Companions~~

### Deployment Architecture

**Single Stone (e.g., Wyse 5070):**

```
┌─────────────────────────────────────────────────────────┐
│  Stone: stone-wyse-5070 (192.168.1.100)                │
│                                                         │
│  ┌──────────────────────────────────────────────────┐  │
│  │  Moss Process (garden-moss)                      │  │
│  │  - Port 7185                                      │  │
│  │  - EventBus: All domain events                   │  │
│  │  - SSE Endpoint: /api/v1/presence/stream         │  │
│  │    └─ Filters to local stone events only         │  │
│  └──────────────────────────────────────────────────┘  │
│                         │                               │
│                         │ SSE (localhost:7185)          │
│                         ↓                               │
│  ┌──────────────────────────────────────────────────┐  │
│  │  Cricket Process (garden-cricket)                │  │
│  │  - Connects to http://localhost:7185             │  │
│  │  - Receives filtered events (20/min, not 100)    │  │
│  │  - Translates to audio                           │  │
│  │    └─ Plays through internal speakers            │  │
│  └──────────────────────────────────────────────────┘  │
│                         │                               │
│                         ↓                               │
│                  [Internal Speakers] 🔊                 │
└─────────────────────────────────────────────────────────┘
```

**Key points:**
- ✅ Cricket and Moss on **same hardware** (localhost connection)
- ✅ SSE endpoint filters events **before** sending (efficiency)
- ✅ Cricket only processes events relevant to its stone
- ✅ No network overhead (localhost)
- ✅ Minimal CPU/memory impact on Cricket

**Garden-Wide (Multiple Stones):**

Each stone runs its own Moss + Cricket pair. Cricket on `stone-A` only hears events from `stone-A`, not from `stone-B` or `stone-C`.

```
Stone A              Stone B              Stone C
├─ Moss              ├─ Moss              ├─ Moss
└─ Cricket           └─ Cricket           └─ (no Cricket)
   (hears A)            (hears B)
```

If you want a **garden-wide soundscape** (Cricket hears all stones), that's a different use case:
- Connect Cricket to **Lantern** instead of local Moss
- Lantern aggregates events from all stones
- This is Phase 3+ (out of scope for initial implementation)

---

## API URI Architecture (Semantic Design)

### Existing Pattern (API-0001)

Zen Garden already has clear semantic boundaries in the API:

| Endpoint Pattern | Scope | Handler | Consumer |
|-----------------|-------|---------|----------|
| `/api/v1/stone/*` | Single stone | Moss (local) | Rake, local tools |
| `/api/v1/garden/*` | All stones | Moss (orchestrated) | Rake, Lantern |

**Examples from existing API:**
- `/api/v1/stone/info` - This stone's information
- `/api/v1/stone/nourishment` - This stone's available updates
- `/api/v1/garden/topology` - All stones in garden
- `/api/v1/garden/nourishment` - Garden-wide updates (aggregated by tended stone)

### Presence Endpoints (Following Pattern)

**Phase 1 (Moss):**
```
GET /api/v1/stone/presence/stream
```
- **Scope:** Local stone only
- **Handler:** Moss (filters to local events)
- **Consumer:** Cricket, Firefly, OLED (running on same stone)
- **Events:** service.started (stone-A), load.updated (stone-A)

**Phase 3+ (Lantern):**
```
GET /api/v1/garden/presence/stream
```
- **Scope:** All stones in garden
- **Handler:** Lantern (aggregates from all stones)
- **Consumer:** Garden-wide dashboards, multi-stone Cricket
- **Events:** service.started (stone-A), service.stopped (stone-B), load.updated (stone-C)

### Semantic Benefits

| Benefit | Description |
|---------|-------------|
| **Self-documenting** | URI immediately indicates scope (`/stone/` vs `/garden/`) |
| **Consistent** | Follows existing nourishment, topology patterns |
| **Discoverable** | API consumers know where to look |
| **Scalable** | Natural place for future stone-scoped vs garden-scoped resources |
| **RESTful** | Resource-oriented (presence as a resource, scoped by stone/garden) |

### Consumer Decision Tree

```
Companion deciding which endpoint to use:

Am I running on a Stone?
├─ Yes → Connect to /api/v1/stone/presence/stream (local Moss)
│         Benefit: Low latency, filtered events, no network overhead
│
└─ No → Am I a garden-wide dashboard?
        └─ Yes → Connect to /api/v1/garden/presence/stream (Lantern)
                 Benefit: Single connection for all stones

Examples:
- Cricket on Wyse 5070 → /stone/presence/stream (localhost:7185)
- Firefly on RP2040 → /stone/presence/stream (stone-ip:7185)
- Web dashboard → /garden/presence/stream (lantern:7186)
```

### Future Extension Points

The URI structure naturally accommodates future features:

```
# Stone-scoped
/api/v1/stone/presence/stream          # Events (Phase 1)
/api/v1/stone/presence/snapshot        # Snapshot only (optional)
/api/v1/stone/presence/history?since=  # Historical events (optional)

# Garden-scoped
/api/v1/garden/presence/stream         # Aggregated events (Phase 3)
/api/v1/garden/presence/stones         # Per-stone summary (optional)
```

---

## Proposed Implementation

### Phase 1A: Core Types (Common - Reusable)

**File:** `src/common/src/presence/mod.rs` (NEW)

```rust
//! Presence protocol types
//!
//! Stone Presence Protocol (PRESENCE-0001) - Common types for Companions.
//! 
//! **Design:** Protocol contracts only. Implementation is in Moss.

pub mod types;

pub use types::{PresenceSnapshot, StoneState, ServiceState};
```

**File:** `src/common/src/presence/types.rs` (NEW)

```rust
//! Presence protocol data types (SSE payload contracts)

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Snapshot sent on SSE connect
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresenceSnapshot {
    pub stone: StoneState,
    pub services: Vec<ServiceState>,
    pub timestamp: DateTime<Utc>,
}

/// Stone state summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneState {
    pub name: String,
    pub health: String,      // "thriving", "withering", "wilting"
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub disk_percent: f64,
    pub uptime_seconds: u64,
    pub pond_active: bool,
}

/// Service state summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceState {
    pub name: String,
    pub state: String,       // "running", "stopped", etc.
    pub health: String,      // "healthy", "unhealthy"
}
```

**Rationale:**
- ✅ **Reusable** - Companions (Cricket, Firefly) import these types
- ✅ **Protocol contracts only** - No implementation details
- ✅ **Simple** - Just data structures, no behavior

---

### Phase 1B: Extend DomainEvent (Common - Reusable)

**File:** `src/common/src/events/domain_events.rs` (MODIFY)

Add new event category:

```rust
pub enum DomainEvent {
    Service(ServiceEvent),
    Registry(RegistryEvent),
    Job(JobEvent),
    Discovery(DiscoveryEvent),
    Stone(StoneEvent),  // ← NEW: Stone-level events
}

// ... existing code ...

// ============================================================================
// Stone Events (NEW)
// ============================================================================

/// Stone-level events (health, load, interaction)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum StoneEvent {
    /// Stone health changed
    HealthChanged {
        stone_name: String,
        old_health: String,
        new_health: String,
        reason: Option<String>,
        timestamp: DateTime<Utc>,
    },
    
    /// Stone load/metrics updated
    LoadUpdated {
        stone_name: String,
        cpu_percent: f64,
        memory_percent: f64,
        disk_percent: f64,
        timestamp: DateTime<Utc>,
    },
    
    /// Stone was tended (admin interaction)
    Tended {
        stone_name: String,
        by: String,
        from: String,
        timestamp: DateTime<Utc>,
    },
}

impl StoneEvent {
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            StoneEvent::HealthChanged { timestamp, .. } => *timestamp,
            StoneEvent::LoadUpdated { timestamp, .. } => *timestamp,
            StoneEvent::Tended { timestamp, .. } => *timestamp,
        }
    }
    
    pub fn stone_name(&self) -> &str {
        match self {
            StoneEvent::HealthChanged { stone_name, .. } => stone_name,
            StoneEvent::LoadUpdated { stone_name, .. } => stone_name,
            StoneEvent::Tended { stone_name, .. } => stone_name,
        }
    }
}
```

**Rationale:**
- ✅ **DRY** - Reuses DomainEvent infrastructure
- ✅ **SoC** - Stone events are domain events
- ✅ **Composable** - Other parts of system can subscribe too

---

### Phase 1C: SSE Endpoint (Moss - Companion Pattern)

**File:** `src/moss/src/api/v1/presence.rs` (NEW)

```rust
//! Presence streaming API endpoint
//!
//! Translates DomainEvents to garden-native presence vocabulary.
//! This is the anti-corruption layer between domain and Companions.

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use std::convert::Infallible;
use tokio_stream::StreamExt;

use crate::AppState;
use garden_common::events::{DomainEvent, ServiceEvent, StoneEvent};

/// GET /api/v1/stone/presence/stream - Local stone presence stream
///
/// **Scope:** Stone-level (local events only)
/// **Consumer:** Local Companions (Cricket, Firefly, OLED)
/// 
/// Returns SSE stream of domain events translated to presence vocabulary.
/// Only emits events relevant to THIS stone (filters out garden-wide events).
/// 
/// **URI Semantics:**
/// - `/api/v1/stone/*` - Stone-scoped operations (this stone only)
/// - `/api/v1/garden/*` - Garden-scoped operations (all stones, via Lantern)
/// 
/// Flow:
/// 1. Generate snapshot from AppState
/// 2. Subscribe to EventBus (DomainEvent stream)
/// 3. **Filter** to local stone events only
/// 4. Translate each event to garden-native vocabulary
/// 5. Emit as SSE
pub async fn stream_stone_presence(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::info!("Local presence Companion connected");
    
    let stone_name = state.stone_name.clone();
    
    // Generate initial snapshot
    let snapshot = generate_snapshot(&state).await;
    let snapshot_json = serde_json::to_string(&snapshot).unwrap_or_default();
    
    // Subscribe to domain events
    // TODO: Use EventBus when available
    // For now, use existing event_tx
    let rx = state.event_tx.subscribe();
    
    // Create stream: snapshot first, then filtered + translated events
    let stream = futures_util::stream::once(async move {
        Event::default()
            .event("presence.snapshot")
            .data(snapshot_json)
    })
    .chain(
        tokio_stream::wrappers::BroadcastStream::new(rx)
            .filter_map(|result| async move {
                match result {
                    Ok(event) => Some(event),
                    Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                        tracing::warn!("Presence Companion lagged {} events", n);
                        None
                    }
                }
            })
            .filter_map(move |moss_event| {
                let stone = stone_name.clone();
                async move {
                    // TODO: When EventBus is integrated, filter DomainEvent here:
                    // match &event {
                    //     DomainEvent::Service(e) if e.stone_name() == stone => Some(e),
                    //     DomainEvent::Stone(e) if e.stone_name() == stone => Some(e),
                    //     _ => None, // Discard: Job, Discovery, Registry, other stones
                    // }
                    
                    // For now, translate MossEvent to presence format
                    // This will naturally filter once we switch to DomainEvent
                    translate_to_presence(&moss_event)
                }
            })
    )
    .map(Ok);
    
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Generate presence snapshot from current state
async fn generate_snapshot(state: &AppState) -> garden_common::presence::PresenceSnapshot {
    use garden_common::presence::{PresenceSnapshot, StoneState, ServiceState};
    
    let registry = state.registry.read().await;
    
    // Map services
    let services: Vec<ServiceState> = registry
        .iter()
        .map(|svc| ServiceState {
            name: svc.name.clone(),
            state: svc.status.clone(),
            health: "healthy".to_string(), // TODO: Real health check
        })
        .collect();
    
    // Compute stone state
    let uptime = state.start_time.elapsed().as_secs();
    
    // TODO: Real metrics from system monitor
    let cpu_percent = 25.0;
    let memory_percent = 45.0;
    let disk_percent = 60.0;
    
    let health = compute_health(cpu_percent, memory_percent);
    
    PresenceSnapshot {
        stone: StoneState {
            name: state.stone_name.clone(),
            health,
            cpu_percent,
            memory_percent,
            disk_percent,
            uptime_seconds: uptime,
            pond_active: false, // TODO: Real pond status
        },
        services,
        timestamp: chrono::Utc::now(),
    }
}

/// Compute stone health from metrics
fn compute_health(cpu: f64, memory: f64) -> String {
    if cpu > 95.0 || memory > 95.0 {
        "wilting".to_string()
    } else if cpu > 80.0 || memory > 80.0 {
        "withering".to_string()
    } else {
        "thriving".to_string()
    }
}

/// Translate MossEvent to presence SSE event
/// 
/// This is temporary. When EventBus integration is complete,
/// this will translate DomainEvent instead.
fn translate_to_presence(moss_event: &crate::MossEvent) -> Option<Event> {
    // Parse message for event type
    // This is hacky but temporary until EventBus integration
    
    if moss_event.message.contains("started successfully") {
        let service = extract_service_name(&moss_event.message)?;
        let data = serde_json::json!({
            "service": service,
            "timestamp": moss_event.timestamp,
        });
        Some(Event::default()
            .event("service.started")
            .data(data.to_string()))
    } else if moss_event.message.contains("stopped") {
        let service = extract_service_name(&moss_event.message)?;
        let data = serde_json::json!({
            "service": service,
            "timestamp": moss_event.timestamp,
        });
        Some(Event::default()
            .event("service.stopped")
            .data(data.to_string()))
    } else {
        None // Skip events that don't map to presence
    }
}

/// Extract service name from message (temporary hack)
fn extract_service_name(message: &str) -> Option<String> {
    // TODO: Remove this when DomainEvent integration is complete
    message.split_whitespace().nth(1).map(|s| s.to_string())
}
```

**Rationale:**
- ✅ **KISS** - Single endpoint, no complex infrastructure
- ✅ **SoC** - Translation happens at boundary (Companion pattern)
- ✅ **YAGNI** - No subscriber tracking, no extra endpoints
- ✅ **DRY** - Reuses existing EventBus pattern

---

### Phase 1D: Background Tasks (Moss - Implementation)

**File:** `src/moss/src/tasks/presence_monitor.rs` (NEW)

```rust
//! Stone metrics monitoring for presence protocol

use std::time::Duration;
use tokio::time::interval;
use chrono::Utc;

use crate::AppState;
use garden_common::events::{DomainEvent, StoneEvent};

/// Run load monitoring task (every 5s)
/// 
/// Emits StoneEvent::LoadUpdated to EventBus.
pub async fn run_load_monitor_task(state: AppState) {
    let mut interval = interval(Duration::from_secs(5));
    
    loop {
        interval.tick().await;
        
        // TODO: Real system metrics (sysinfo crate?)
        let cpu_percent = 25.0;
        let memory_percent = 45.0;
        let disk_percent = 60.0;
        
        // Emit domain event (EventBus picks it up)
        let event = DomainEvent::Stone(StoneEvent::LoadUpdated {
            stone_name: state.stone_name.clone(),
            cpu_percent,
            memory_percent,
            disk_percent,
            timestamp: Utc::now(),
        });
        
        // TODO: Publish to EventBus when available
        // For now, construct MossEvent for backward compat
        let moss_event = crate::MossEvent {
            timestamp: Utc::now().to_rfc3339(),
            level: "debug".to_string(),
            message: format!("Load: CPU {:.1}%, Memory {:.1}%", cpu_percent, memory_percent),
            job_id: None,
        };
        let _ = state.event_tx.send(moss_event);
    }
}

/// Run health monitor task (every 30s)
/// 
/// Computes stone health from metrics and emits StoneEvent::HealthChanged.
pub async fn run_health_monitor_task(state: AppState) {
    let mut interval = interval(Duration::from_secs(30));
    let mut last_health = "thriving".to_string();
    
    loop {
        interval.tick().await;
        
        // TODO: Get real metrics
        let cpu = 25.0;
        let memory = 45.0;
        
        let new_health = if cpu > 95.0 || memory > 95.0 {
            "wilting"
        } else if cpu > 80.0 || memory > 80.0 {
            "withering"
        } else {
            "thriving"
        };
        
        if new_health != last_health {
            let event = DomainEvent::Stone(StoneEvent::HealthChanged {
                stone_name: state.stone_name.clone(),
                old_health: last_health.clone(),
                new_health: new_health.to_string(),
                reason: None,
                timestamp: Utc::now(),
            });
            
            // TODO: Publish to EventBus
            tracing::info!(old = %last_health, new = %new_health, "Stone health changed");
            
            last_health = new_health.to_string();
        }
    }
}
```

**Rationale:**
- ✅ **KISS** - Simple monitoring loops
- ✅ **SoC** - Emits domain events, doesn't know about SSE
- ✅ **DRY** - Reuses domain event infrastructure

---

### Phase 1E: Emit from Existing Sites (Moss - Hooks)

**Where to emit domain events:**

1. **Service start** → `src/moss/src/api/v1/services.rs`:
```rust
// After successful start
let event = DomainEvent::Service(ServiceEvent::Started {
    stone_name: state.stone_name.clone(),
    service_name: service_name.clone(),
    timestamp: Utc::now(),
});
// TODO: event_bus.publish(event).await?;
```

2. **Service stop** → Same pattern

3. **Stone tended** → When Rake connects, emit:
```rust
let event = DomainEvent::Stone(StoneEvent::Tended {
    stone_name: state.stone_name.clone(),
    by: "garden-rake".to_string(),
    from: client_ip,
    timestamp: Utc::now(),
});
```

**Rationale:**
- ✅ **KISS** - Direct emission at source
- ✅ **No bridge layer** - Eliminates unnecessary abstraction
- ✅ **DRY** - Single event emission mechanism

---

## Implementation Order (Simplified)

### Step 1: Types in Common (1 hour)
- [ ] Create `src/common/src/presence/types.rs`
- [ ] Define `PresenceSnapshot`, `StoneState`, `ServiceState`
- [ ] Export from `src/common/src/presence/mod.rs`

### Step 2: Extend DomainEvent (1 hour)
- [ ] Add `StoneEvent` enum to `domain_events.rs`
- [ ] Add `Stone(StoneEvent)` variant to `DomainEvent`
- [ ] Update timestamp/stone_name methods

### Step 3: SSE Endpoint (2 hours)
- [ ] Create `src/moss/src/api/v1/presence.rs`
- [ ] Implement `stream_stone_presence()` with snapshot + filtering + translation
- [ ] Add route to router: `/api/v1/stone/presence/stream`
- [ ] Test with `curl -N http://localhost:7185/api/v1/stone/presence/stream`

### Step 4: Background Tasks (1 hour)
- [ ] Create `src/moss/src/tasks/presence_monitor.rs`
- [ ] Implement `run_load_monitor_task()`
- [ ] Implement `run_health_monitor_task()`
- [ ] Spawn tasks in `bootstrap/run.rs`

### Step 5: Hook Emission (1 hour)
- [ ] Add `StoneEvent::Started` emission in service handlers
- [ ] Add `StoneEvent::Stopped` emission
- [ ] Test by planting/uprooting services

**Total: ~6 hours (not 12)**

---

## Key Changes from Original

| Aspect | Original ❌ | Revised ✅ |
|--------|------------|-----------|
| **Broadcast** | Separate `presence_tx` | Reuse `event_tx` / EventBus |
| **Events** | New `PresenceEvent` hierarchy | Extend `DomainEvent` |
| **Subscriber tracking** | `Arc<RwLock<Vec<Subscriber>>>` | None (YAGNI) |
| **Bridge module** | `presence_bridge.rs` with wrappers | Emit directly |
| **Subscribers endpoint** | `GET /subscribers` | Removed (YAGNI) |
| **Translation** | In bridge layer | In SSE endpoint (boundary) |
| **Complexity** | 5 new modules, 12 hours | 3 files modified, 6 hours |

---

## DDD/SoC Alignment

✅ **Domain Layer** - `StoneEvent` is domain knowledge  
✅ **Infrastructure** - SSE endpoint is infrastructure  
✅ **Anti-corruption** - Translation at SSE boundary  
✅ **Ubiquitous Language** - "thriving", "tended" in domain events  

---

## Event Filtering Strategy

### Problem: EventBus Efficiency

**Concern:** EventBus broadcasts ALL domain events:
- Service events from this stone ✓ (relevant)
- Service events from other stones ✗ (not relevant for local Companion)
- Job events ✗ (not presence-relevant)
- Discovery events ✗ (not presence-relevant)
- Registry events ✗ (not presence-relevant)

Companions running on resource-constrained hardware (ESP8266, Wyse 5070) shouldn't waste CPU/memory processing and discarding irrelevant events.

### Solution: Filter at SSE Boundary

**SSE endpoint filters before sending:**

```rust
.filter_map(move |event| {
    let stone = stone_name.clone();
    async move {
        match &event {
            // Only local stone service events
            DomainEvent::Service(e) if e.stone_name() == stone => {
                Some(translate_service_event(e))
            }
            
            // Only local stone events
            DomainEvent::Stone(e) if e.stone_name() == stone => {
                Some(translate_stone_event(e))
            }
            
            // Discard everything else
            _ => None,
        }
    }
})
```

**What Companions receive:**
- ✅ Service events: started, stopped, sprouted, uprooted (local stone only)
- ✅ Stone events: health, load, tended (local stone only)
- ❌ Job events: Not sent (Companion doesn't care about installation progress)
- ❌ Discovery events: Not sent (Companion doesn't care about topology)
- ❌ Registry events: Not sent (Companion doesn't care about Lantern registry)
- ❌ Other stones: Not sent (local Companion only cares about local stone)

### Performance Impact

**Before filtering:**
- EventBus emits ~100 events/minute (garden-wide)
- Companion receives 100 events/minute
- Companion discards ~80 events/minute (80% waste)
- Processing overhead: ~50μs/event × 100 = 5ms/minute

**After filtering:**
- EventBus emits ~100 events/minute (unchanged)
- SSE endpoint filters to ~20 events/minute
- Companion receives 20 events/minute (only relevant ones)
- Processing overhead: ~50μs/event × 20 = 1ms/minute (80% reduction)

**For ESP8266 OLED Companion:**
- 80% fewer events to parse
- 80% less JSON deserialization
- 80% less memory churn
- More headroom for display updates

### Future: Query-Based Filtering (Optional)

**If needed later (YAGNI for now):**

```
GET /api/v1/presence/stream?events=service,stone&stone=local
```

Allows Companions to request specific event types. But default filtering (service + stone, local only) covers 99% of use cases.

---

## Future: EventBus Integration

When EventBus is fully integrated:

**Before (current proposal):**
```rust
state.event_tx.send(moss_event) // ← MossEvent (legacy)
```

**After (EventBus):**
```rust
state.event_bus.publish(domain_event).await? // ← DomainEvent
```

SSE endpoint subscribes to `event_bus.subscribe()` instead of `event_tx.subscribe()`.

**No other changes needed.** The architecture naturally migrates.

---

## Summary of Improvements

| Principle | How Applied |
|-----------|-------------|
| **DRY** | Reuse EventBus, don't duplicate broadcast |
| **YAGNI** | Remove subscriber tracking, remove `/subscribers` endpoint |
| **KISS** | Single SSE endpoint, no bridge layer |
| **SoC** | Domain events in domain, translation at boundary |
| **Reusability** | Types in common, implementation in Moss |

**Effort reduced:** 12 hours → 6 hours  
**Complexity reduced:** 5 new modules → 3 modified files  
**Architecture improved:** Aligned with existing patterns

```rust
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Presence event (garden-native vocabulary)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum PresenceEvent {
    /// Initial snapshot on connect
    Snapshot(SnapshotData),
    
    /// Periodic heartbeat (every 30s)
    Heartbeat(HeartbeatData),
    
    /// Stone health changed
    StoneHealthChanged {
        old: String,
        new: String,
        reason: Option<String>,
        timestamp: DateTime<Utc>,
    },
    
    /// Stone load updated (CPU, memory, disk)
    StoneLoadUpdated {
        cpu_percent: f64,
        memory_percent: f64,
        disk_percent: f64,
        timestamp: DateTime<Utc>,
    },
    
    /// Stone was tended (Rake connected)
    StoneTended {
        by: String,
        from: String,
        timestamp: DateTime<Utc>,
    },
    
    /// Service sprouted (installed)
    ServiceSprouted {
        service: String,
        offering: String,
        timestamp: DateTime<Utc>,
    },
    
    /// Service started
    ServiceStarted {
        service: String,
        timestamp: DateTime<Utc>,
    },
    
    /// Service stopped
    ServiceStopped {
        service: String,
        reason: Option<String>,
        timestamp: DateTime<Utc>,
    },
    
    /// Service uprooted (removed)
    ServiceUprooted {
        service: String,
        timestamp: DateTime<Utc>,
    },
    
    /// Service health changed
    ServiceHealthChanged {
        service: String,
        old: String,
        new: String,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotData {
    pub stone: StoneSnapshot,
    pub services: Vec<ServiceSnapshot>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoneSnapshot {
    pub name: String,
    pub health: String,
    pub load: LoadSnapshot,
    pub uptime_seconds: u64,
    pub pond_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadSnapshot {
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub disk_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSnapshot {
    pub name: String,
    pub state: String,
    pub health: String,
    pub activity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatData {
    pub stone_health: String,
    pub service_count: usize,
    pub services_healthy: usize,
    pub pond_active: bool,
    pub timestamp: DateTime<Utc>,
}
```

**File:** `src/common/src/presence/subscriber.rs` (NEW)

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use super::PresenceEvent;

/// Active presence subscriber (SSE connection)
#[derive(Debug, Clone)]
pub struct Subscriber {
    pub Companion: Option<String>,
    pub version: Option<String>,
    pub connected_since: DateTime<Utc>,
    pub tx: broadcast::Sender<PresenceEvent>,
}

/// Subscriber info for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriberInfo {
    pub Companion: Option<String>,
    pub version: Option<String>,
    pub connected_since: DateTime<Utc>,
}

impl From<&Subscriber> for SubscriberInfo {
    fn from(sub: &Subscriber) -> Self {
        Self {
            Companion: sub.Companion.clone(),
            version: sub.version.clone(),
            connected_since: sub.connected_since,
        }
    }
}
```

**File:** `src/moss/src/app_state.rs` (MODIFY)

Add to `AppState`:
```rust
/// Presence event broadcast channel for Companions
pub presence_tx: tokio::sync::broadcast::Sender<PresenceEvent>,

/// Active presence subscribers (for observability)
pub presence_subscribers: Arc<RwLock<Vec<Subscriber>>>,
```

**File:** `src/moss/src/bootstrap/run.rs` (MODIFY)

Initialize channel:
```rust
let (presence_tx, _) = tokio::sync::broadcast::channel::<PresenceEvent>(100);
let presence_subscribers = Arc::new(RwLock::new(Vec::new()));
```

---

### Phase 1B: SSE Endpoint

**File:** `src/moss/src/api/v1/presence.rs` (NEW)

```rust
//! Presence streaming API endpoints
//!
//! Stone Presence Protocol (PRESENCE-0001) implementation.

use axum::{
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::AppState;
use garden_common::presence::{PresenceEvent, Subscriber, SubscriberInfo};

/// Query params for Companion identification
#[derive(Debug, Deserialize)]
pub struct PresenceQuery {
    pub Companion: Option<String>,
    pub version: Option<String>,
}

/// GET /api/v1/presence/stream - Presence event stream
///
/// Returns SSE stream with:
/// 1. Snapshot on connect (complete current state)
/// 2. Incremental events as they occur
/// 3. Heartbeat every 30 seconds
///
/// Optional query params:
/// - `Companion` - Companion type (e.g., "cricket", "firefly")
/// - `version` - Companion version (e.g., "0.1.0")
pub async fn stream_presence(
    State(state): State<AppState>,
    Query(params): Query<PresenceQuery>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::info!(
        Companion = ?params.Companion,
        version = ?params.version,
        "Presence Companion connected"
    );
    
    // Create subscriber entry
    let subscriber = Subscriber {
        Companion: params.Companion.clone(),
        version: params.version.clone(),
        connected_since: chrono::Utc::now(),
        tx: state.presence_tx.clone(),
    };
    
    // Add to subscriber list
    {
        let mut subscribers = state.presence_subscribers.write().await;
        subscribers.push(subscriber);
    }
    
    // Subscribe to presence events
    let rx = state.presence_tx.subscribe();
    
    // Generate initial snapshot
    let snapshot = generate_snapshot(&state).await;
    
    // Create stream with snapshot first, then events
    let stream = futures_util::stream::once(async move {
        Ok::<PresenceEvent, tokio_stream::wrappers::errors::BroadcastStreamRecvError>(
            PresenceEvent::Snapshot(snapshot)
        )
    })
    .chain(BroadcastStream::new(rx))
    .filter_map(|result| match result {
        Ok(event) => Some(Ok::<PresenceEvent, tokio_stream::wrappers::errors::BroadcastStreamRecvError>(event)),
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            tracing::warn!("Presence Companion lagged {} messages", n);
            None
        }
    })
    .map(|event_result| {
        let event = event_result.unwrap();
        
        // Serialize to JSON
        let data = serde_json::to_string(&event).unwrap_or_default();
        
        // Extract event name for SSE event type
        let event_name = match &event {
            PresenceEvent::Snapshot(_) => "presence.snapshot",
            PresenceEvent::Heartbeat(_) => "presence.heartbeat",
            PresenceEvent::StoneHealthChanged { .. } => "stone.health.changed",
            PresenceEvent::StoneLoadUpdated { .. } => "stone.load.updated",
            PresenceEvent::StoneTended { .. } => "stone.tended",
            PresenceEvent::ServiceSprouted { .. } => "service.sprouted",
            PresenceEvent::ServiceStarted { .. } => "service.started",
            PresenceEvent::ServiceStopped { .. } => "service.stopped",
            PresenceEvent::ServiceUprooted { .. } => "service.uprooted",
            PresenceEvent::ServiceHealthChanged { .. } => "service.health.changed",
        };
        
        Event::default()
            .event(event_name)
            .data(data)
    })
    .map(Ok);
    
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// GET /api/v1/presence/subscribers - List active subscribers
///
/// Returns currently connected presence Companions.
#[derive(Debug, Serialize)]
pub struct SubscribersResponse {
    pub subscribers: Vec<SubscriberInfo>,
    pub count: usize,
}

pub async fn list_subscribers(
    State(state): State<AppState>,
) -> Json<SubscribersResponse> {
    let subscribers = state.presence_subscribers.read().await;
    let infos: Vec<SubscriberInfo> = subscribers.iter().map(|s| s.into()).collect();
    let count = infos.len();
    
    Json(SubscribersResponse {
        subscribers: infos,
        count,
    })
}

/// Generate snapshot from current state
async fn generate_snapshot(state: &AppState) -> garden_common::presence::events::SnapshotData {
    use garden_common::presence::events::*;
    
    let registry = state.registry.read().await;
    let capabilities = state.capabilities.read().await;
    
    // Collect service info
    let services: Vec<ServiceSnapshot> = registry
        .iter()
        .map(|svc| ServiceSnapshot {
            name: svc.name.clone(),
            state: svc.status.clone(),
            health: "healthy".to_string(), // TODO: real health from Docker
            activity: "idle".to_string(), // TODO: real activity
        })
        .collect();
    
    // Compute stone health (simple heuristic for now)
    let cpu_percent = capabilities.as_ref()
        .and_then(|c| c.cpu_count)
        .map(|_| 25.0) // TODO: real CPU monitoring
        .unwrap_or(0.0);
    
    let memory_percent = 45.0; // TODO: real memory monitoring
    let disk_percent = 60.0; // TODO: real disk monitoring
    
    let health = if cpu_percent > 95.0 || memory_percent > 95.0 {
        "wilting"
    } else if cpu_percent > 80.0 || memory_percent > 80.0 {
        "withering"
    } else {
        "thriving"
    };
    
    let uptime = state.start_time.elapsed().as_secs();
    
    SnapshotData {
        stone: StoneSnapshot {
            name: state.stone_name.clone(),
            health: health.to_string(),
            load: LoadSnapshot {
                cpu_percent,
                memory_percent,
                disk_percent,
            },
            uptime_seconds: uptime,
            pond_active: false, // TODO: real pond status
        },
        services,
        timestamp: chrono::Utc::now(),
    }
}
```

**File:** `src/moss/src/api/v1/mod.rs` (MODIFY)

Add presence routes:
```rust
pub mod presence;

// In router setup
.route("/api/v1/presence/stream", get(presence::stream_presence))
.route("/api/v1/presence/subscribers", get(presence::list_subscribers))
```

---

### Phase 1C: Event Bridge

**File:** `src/moss/src/domain/presence_bridge.rs` (NEW)

```rust
//! Bridge between domain events and presence events
//!
//! Subscribes to domain events and translates to garden-native presence vocabulary.

use anyhow::Result;
use garden_common::events::{DomainEvent, ServiceEvent};
use garden_common::presence::PresenceEvent;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::AppState;

/// Start presence bridge task
///
/// Subscribes to domain events and emits corresponding presence events.
pub async fn run_presence_bridge(state: AppState) {
    tracing::info!("Starting presence bridge task");
    
    // TODO: Subscribe to EventBus when available
    // For now, we'll emit from direct call sites
    
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}

/// Emit presence event for service started
pub fn emit_service_started(state: &AppState, service_name: &str) {
    let event = PresenceEvent::ServiceStarted {
        service: service_name.to_string(),
        timestamp: chrono::Utc::now(),
    };
    
    let _ = state.presence_tx.send(event);
}

/// Emit presence event for service stopped
pub fn emit_service_stopped(state: &AppState, service_name: &str, reason: Option<String>) {
    let event = PresenceEvent::ServiceStopped {
        service: service_name.to_string(),
        reason,
        timestamp: chrono::Utc::now(),
    };
    
    let _ = state.presence_tx.send(event);
}

/// Emit presence event for service sprouted (installed)
pub fn emit_service_sprouted(state: &AppState, service_name: &str, offering: &str) {
    let event = PresenceEvent::ServiceSprouted {
        service: service_name.to_string(),
        offering: offering.to_string(),
        timestamp: chrono::Utc::now(),
    };
    
    let _ = state.presence_tx.send(event);
}

/// Emit presence event for service uprooted (removed)
pub fn emit_service_uprooted(state: &AppState, service_name: &str) {
    let event = PresenceEvent::ServiceUprooted {
        service: service_name.to_string(),
        timestamp: chrono::Utc::now(),
    };
    
    let _ = state.presence_tx.send(event);
}

/// Emit presence event for stone load update
pub fn emit_stone_load(state: &AppState, cpu: f64, memory: f64, disk: f64) {
    let event = PresenceEvent::StoneLoadUpdated {
        cpu_percent: cpu,
        memory_percent: memory,
        disk_percent: disk,
        timestamp: chrono::Utc::now(),
    };
    
    let _ = state.presence_tx.send(event);
}
```

---

### Phase 1D: Background Tasks

**File:** `src/moss/src/tasks/presence_monitor.rs` (NEW)

```rust
//! Presence monitoring tasks
//!
//! - Heartbeat: Periodic heartbeat every 30s
//! - Load monitor: Track CPU/memory/disk every 5s
//! - Health monitor: Compute stone health from load

use std::time::Duration;
use tokio::time::interval;

use crate::AppState;
use garden_common::presence::events::HeartbeatData;
use garden_common::presence::PresenceEvent;

/// Run heartbeat task (every 30s)
pub async fn run_heartbeat_task(state: AppState) {
    let mut interval = interval(Duration::from_secs(30));
    
    loop {
        interval.tick().await;
        
        let heartbeat = generate_heartbeat(&state).await;
        let event = PresenceEvent::Heartbeat(heartbeat);
        
        let _ = state.presence_tx.send(event);
    }
}

/// Run load monitoring task (every 5s)
pub async fn run_load_monitor_task(state: AppState) {
    let mut interval = interval(Duration::from_secs(5));
    
    loop {
        interval.tick().await;
        
        // TODO: Real system metrics
        let cpu = 25.0;
        let memory = 45.0;
        let disk = 60.0;
        
        let event = PresenceEvent::StoneLoadUpdated {
            cpu_percent: cpu,
            memory_percent: memory,
            disk_percent: disk,
            timestamp: chrono::Utc::now(),
        };
        
        let _ = state.presence_tx.send(event);
    }
}

/// Generate heartbeat data
async fn generate_heartbeat(state: &AppState) -> HeartbeatData {
    let registry = state.registry.read().await;
    
    let service_count = registry.len();
    let services_healthy = registry.iter().filter(|s| s.status == "running").count();
    
    HeartbeatData {
        stone_health: "thriving".to_string(), // TODO: compute from load
        service_count,
        services_healthy,
        pond_active: false, // TODO: real pond status
        timestamp: chrono::Utc::now(),
    }
}
```

**File:** `src/moss/src/bootstrap/run.rs` (MODIFY)

Spawn tasks:
```rust
// Spawn presence heartbeat task
let presence_heartbeat_state = state.clone();
tokio::spawn(async move {
    crate::tasks::presence_monitor::run_heartbeat_task(presence_heartbeat_state).await;
});

// Spawn load monitor task
let load_monitor_state = state.clone();
tokio::spawn(async move {
    crate::tasks::presence_monitor::run_load_monitor_task(load_monitor_state).await;
});
```

---

### Phase 1E: Hook Emission

**Where to call `emit_presence_event()`:**

1. **Service start** → `src/moss/src/api/v1/services.rs`:
```rust
pub async fn start_service_handler(...) -> ... {
    // ...existing code...
    docker.start_container(&service_name).await?;
    
    // Emit presence event
    crate::domain::presence_bridge::emit_service_started(&state, &service_name);
    
    // ...rest of code...
}
```

2. **Service stop** → Same file:
```rust
crate::domain::presence_bridge::emit_service_stopped(&state, &service_name, None);
```

3. **Service install** → `src/moss/src/tasks/job_executors.rs`:
```rust
// After successful installation
crate::domain::presence_bridge::emit_service_sprouted(&state, &service_name, &offering);
```

4. **Service remove** → `src/moss/src/api/v1/services.rs`:
```rust
crate::domain::presence_bridge::emit_service_uprooted(&state, &service_name);
```

5. **Stone tended** → `src/moss/src/api/v1/stone.rs`:
```rust
// When Rake connects to any stone endpoint
// (Add middleware or emit in handler)
```

---

## Implementation Order

### Step 1: Types & Channel (2 hours)
- [ ] Create `src/common/src/presence/` module
- [ ] Define `PresenceEvent`, `SnapshotData`, etc.
- [ ] Add `presence_tx` to `AppState`
- [ ] Initialize channel in `bootstrap/run.rs`

### Step 2: SSE Endpoint (3 hours)
- [ ] Create `src/moss/src/api/v1/presence.rs`
- [ ] Implement `stream_presence()` handler
- [ ] Implement `list_subscribers()` handler
- [ ] Add routes to router
- [ ] Test with `curl -N http://localhost:7185/api/v1/presence/stream`

### Step 3: Heartbeat Task (1 hour)
- [ ] Create `src/moss/src/tasks/presence_monitor.rs`
- [ ] Implement `run_heartbeat_task()`
- [ ] Spawn task in `bootstrap/run.rs`
- [ ] Verify heartbeat appears in stream every 30s

### Step 4: Event Bridge (4 hours)
- [ ] Create `src/moss/src/domain/presence_bridge.rs`
- [ ] Implement `emit_service_*()` functions
- [ ] Hook into service start/stop/install/remove
- [ ] Test by planting/uprooting services

### Step 5: Load Monitor (2 hours)
- [ ] Implement `run_load_monitor_task()`
- [ ] Add real system metrics (sysinfo crate?)
- [ ] Spawn task
- [ ] Verify load events every 5s

**Total: ~12 hours (1.5 days)**

---

## Key Design Decisions

### Decision 1: Separate Broadcast Channel

**Option A:** Reuse existing `event_tx` (MossEvent)  
**Option B:** Create separate `presence_tx` (PresenceEvent)

**Chosen: B**

**Rationale:**
- Different vocabularies (technical vs garden-native)
- Independent buffering/backpressure
- Cleaner separation of concerns
- Easier to add Companion-specific filtering later

### Decision 2: Hook Placement

**Option A:** Subscribe to EventBus (domain events)  
**Option B:** Direct calls at emission sites

**Chosen: B (for now)**

**Rationale:**
- EventBus not fully integrated yet
- Direct calls are simpler and more explicit
- Can refactor to EventBus later when it's available
- No performance penalty (single broadcast send)

### Decision 3: Subscriber Tracking

**Option A:** Track in middleware (connection open/close)  
**Option B:** Track in handler (manual add/remove)  
**Option C:** Track via broadcast channel introspection

**Chosen: B**

**Rationale:**
- Axum SSE handlers don't have easy access to connection lifecycle
- Manual tracking is explicit and debuggable
- Need to handle cleanup on disconnect (TODO: use Drop trait?)

**TODO:** Implement subscriber cleanup when connection drops.

### Decision 4: Snapshot Generation

**Option A:** Cache snapshot, update on events  
**Option B:** Generate snapshot on-demand from `AppState`

**Chosen: B**

**Rationale:**
- Simpler (no state duplication)
- Always correct (no sync issues)
- Snapshot generation is fast (~1ms)
- Only happens on new connection (infrequent)

---

## Testing Strategy

### Manual Testing (Phase 1)

```powershell
# Terminal 1: Run Moss
cd dist/windows
./garden-moss.exe

# Terminal 2: Watch presence stream
curl -N http://localhost:7185/api/v1/presence/stream

# Terminal 3: Trigger events
garden-rake plant mongodb
garden-rake start mongodb
garden-rake stop mongodb
garden-rake uproot mongodb

# Terminal 4: Check subscribers
curl http://localhost:7185/api/v1/presence/subscribers | jq
```

### Automated Testing (Phase 2)

```rust
#[tokio::test]
async fn test_presence_snapshot() {
    let state = create_test_state();
    let snapshot = generate_snapshot(&state).await;
    
    assert_eq!(snapshot.stone.name, "test-stone");
    assert!(!snapshot.services.is_empty());
}

#[tokio::test]
async fn test_presence_broadcast() {
    let state = create_test_state();
    let mut rx = state.presence_tx.subscribe();
    
    emit_service_started(&state, "test-service");
    
    let event = rx.recv().await.unwrap();
    match event {
        PresenceEvent::ServiceStarted { service, .. } => {
            assert_eq!(service, "test-service");
        }
        _ => panic!("Wrong event type"),
    }
}
```

---

## Performance Considerations

### Broadcast Channel Capacity

Current: `broadcast::channel::<MossEvent>(100)`

**For presence:** Same capacity (100) is sufficient.

**Reasoning:**
- Heartbeat every 30s = 2 events/minute baseline
- Service changes: ~10 events/service (install → start)
- Load updates every 5s = 12 events/minute
- Worst case: ~30 events/minute (0.5 events/second)
- Buffer of 100 = 200 seconds of backlog (Companion has 3+ minutes to recover)

**If Companion lags beyond buffer:**
- Lagged messages are dropped (with warning)
- Next heartbeat/snapshot re-syncs state
- This is **by design** (slow Companions don't block Moss)

### JSON Serialization Overhead

- Per event: ~100 bytes (service event) to ~2KB (snapshot)
- Serialization: ~10μs (service event) to ~500μs (snapshot)
- Frequency: ~30 events/minute = 0.015ms/second overhead
- **Negligible.**

### SSE Keep-Alive

Default Axum keep-alive: 15 seconds  
Protocol specifies: 30 seconds heartbeat

**Both work together:**
- TCP keep-alive prevents connection timeout
- Heartbeat provides application-level liveness + state summary

---

## Open Questions

### Q1: Subscriber Cleanup

**Problem:** When SSE connection drops, subscriber remains in `presence_subscribers` list.

**Options:**
1. Implement custom `Drop` for subscriber handle
2. Periodic cleanup task (scan for dead receivers)
3. Track connection IDs and remove explicitly

**Recommendation:** Option 2 (simple task that prunes every 60s)

### Q2: Load Monitoring

**Problem:** Need real system metrics (CPU, memory, disk).

**Options:**
1. `sysinfo` crate (cross-platform)
2. Platform-specific APIs
3. Read from `/proc` (Linux), WMI (Windows)

**Recommendation:** `sysinfo` crate (already in dependency tree?)

### Q3: Stone Health Computation

**Problem:** What thresholds define "thriving", "withering", "wilting"?

**Proposal:**
- `thriving`: CPU <80%, Memory <80%
- `withering`: CPU 80-95% OR Memory 80-95%
- `wilting`: CPU >95% OR Memory >95%
- `resting`: Explicit maintenance mode flag

### Q4: Pond Status

**Problem:** How to determine if stone is in a pond?

**Answer:** Check for pond configuration file or environment variable.

---

## Next Steps

After Phase 1 completion:

1. **Phase 2:** Rake commands (`presence watch`, `presence test`)
2. **Phase 3:** Cricket Companion on Wyse 5070
3. **Refactor:** Move to EventBus when available
4. **Enhance:** Real metrics, pond status, activity tracking
5. **Cleanup:** Subscriber removal on disconnect

---

## Summary

**We have everything we need.** The presence protocol implementation reuses existing patterns:

✅ **SSE infrastructure**: Copy `/api/v1/events` approach  
✅ **Broadcast channel**: Add `presence_tx` alongside `event_tx`  
✅ **Background tasks**: Copy `announcer` pattern for heartbeat  
✅ **Event emission**: Create `emit_presence_event()` like `emit_event()`  

**Estimated effort:** ~12 hours (1.5 days) for fully working Phase 1.

**No architectural changes needed.** This is a clean addition, not a refactoring.

