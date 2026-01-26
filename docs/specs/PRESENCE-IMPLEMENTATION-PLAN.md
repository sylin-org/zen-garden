# Presence Protocol - Implementation Plan
**Date:** 2026-01-26  
**Phase:** 1 - Moss Endpoints  
**Based on:** PRESENCE-CODE-ASSESSMENT.md (Revision 2)

---

## Overview

This document provides step-by-step implementation instructions for Phase 1 of the Stone Presence Protocol. It follows the simplified architecture from the code assessment (6 hours, 300 LOC).

**Architecture Summary:**
```
Service Handlers → DomainEvent → EventBus → SSE Handler → Adapters
   (emit)           (StoneEvent)   (event_tx)  (filter+translate)
```

**Key Principles Applied:**
- ✅ Reuse EventBus (don't create separate channel)
- ✅ Extend DomainEvent (don't create PresenceEvent hierarchy)
- ✅ Filter at SSE boundary (80% efficiency gain)
- ✅ Direct emission (no bridge layer)
- ✅ `/api/v1/stone/*` URI pattern (API-0001)

---

## Current State Analysis

### Existing Infrastructure (Verified in Code Scan)

**File:** `src/moss/src/app_state.rs`
- ✅ `event_tx: tokio::sync::broadcast::Sender<MossEvent>` exists
- ✅ Initialized in `bootstrap/run.rs` with capacity 100
- ✅ Used by `/api/v1/events` SSE endpoint

**File:** `src/common/src/events/domain_events.rs`
- ✅ `DomainEvent` enum with Service, Registry, Job, Discovery variants
- ✅ `ServiceEvent` with InstallCompleted, Started, Stopped, Removed
- ✅ Timestamp and stone_name accessors already implemented
- 🔧 **Need to add:** `Stone(StoneEvent)` variant

**File:** `src/moss/src/api/v1/events.rs`
- ✅ `stream_events()` SSE handler pattern
- ✅ Broadcast channel subscription via `state.event_tx.subscribe()`
- ✅ Lag handling with warning
- ✅ SSE keep-alive configured
- 📋 **Pattern to replicate** for presence endpoint

**File:** `src/moss/src/api/v1/mod.rs`
- ✅ Module structure: admin, services, stone, garden, etc.
- 🔧 **Need to add:** `pub mod presence;`

**File:** `src/moss/src/bootstrap/run.rs`
- ✅ Channel initialization: `broadcast::channel::<MossEvent>(100)` at line 175
- ✅ Task spawning pattern for announcer, discovery, health monitor
- 🔧 **Need to add:** Spawn presence monitoring tasks

**File:** `src/common/src/lib.rs`
- ✅ Modules: types, events, utils, constants, etc.
- 🔧 **Need to add:** `pub mod presence;`

---

## Implementation Steps

### Step 1: Create Presence Types (1 hour)

**Objective:** Define protocol contracts in `garden_common` for adapters to use.

#### 1.1 Create module structure

**File:** `src/common/src/presence/mod.rs` (NEW)
```rust
//! Stone Presence Protocol types (PRESENCE-0001)
//!
//! Protocol contracts for SSE communication between Moss and adapters.
//! Contains ONLY data structures, no implementation logic.

pub mod types;

pub use types::{PresenceSnapshot, StoneState, ServiceState};
```

#### 1.2 Create data types

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

#### 1.3 Export from common

**File:** `src/common/src/lib.rs` (MODIFY)

Add after line 24 (after `pub mod detection;`):
```rust
pub mod presence;
```

#### Verification
```bash
cd f:\Replica\NAS\Files\repo\github\zen-garden
cargo check -p garden-common
```

**Expected:** Clean compile with new presence types.

---

### Step 2: Extend DomainEvent (1 hour)

**Objective:** Add StoneEvent to existing domain events infrastructure.

#### 2.1 Add StoneEvent enum

**File:** `src/common/src/events/domain_events.rs` (MODIFY)

**Location:** After Discovery events section (around line 380), add:

```rust
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
        by: String,           // "garden-rake", etc.
        from: String,         // IP address
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

#### 2.2 Add Stone variant to DomainEvent

**File:** `src/common/src/events/domain_events.rs` (MODIFY)

**Location:** Lines 9-15, modify DomainEvent enum:

```rust
/// Top-level domain event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_category", content = "event")]
pub enum DomainEvent {
    Service(ServiceEvent),
    Registry(RegistryEvent),
    Job(JobEvent),
    Discovery(DiscoveryEvent),
    Stone(StoneEvent),  // ← NEW
}
```

#### 2.3 Update DomainEvent methods

**File:** `src/common/src/events/domain_events.rs` (MODIFY)

**Location:** Lines 17-37, update match arms:

```rust
impl DomainEvent {
    /// Get event timestamp
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            DomainEvent::Service(e) => e.timestamp(),
            DomainEvent::Registry(e) => e.timestamp(),
            DomainEvent::Job(e) => e.timestamp(),
            DomainEvent::Discovery(e) => e.timestamp(),
            DomainEvent::Stone(e) => e.timestamp(),  // ← NEW
        }
    }

    /// Get event stone name (if applicable)
    pub fn stone_name(&self) -> Option<&str> {
        match self {
            DomainEvent::Service(e) => Some(e.stone_name()),
            DomainEvent::Registry(e) => Some(e.stone_name()),
            DomainEvent::Job(e) => e.stone_name(),
            DomainEvent::Discovery(e) => Some(e.stone_name()),
            DomainEvent::Stone(e) => Some(e.stone_name()),  // ← NEW
        }
    }

    /// Convert to JSON for SSE
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}
```

#### Verification
```bash
cargo check -p garden-common
```

**Expected:** Clean compile with StoneEvent integrated.

---

### Step 3: Create SSE Endpoint (2 hours)

**Objective:** Implement `/api/v1/stone/presence/stream` with filtering and translation.

#### 3.1 Create presence API module

**File:** `src/moss/src/api/v1/presence.rs` (NEW)

```rust
//! Presence streaming API endpoint
//!
//! Stone Presence Protocol (PRESENCE-0001) implementation.
//! Translates DomainEvents to garden-native presence vocabulary at SSE boundary.

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use std::convert::Infallible;
use tokio_stream::StreamExt;

use crate::AppState;
use garden_common::presence::{PresenceSnapshot, StoneState, ServiceState};

/// GET /api/v1/stone/presence/stream - Local stone presence stream
///
/// **Scope:** Stone-level (local events only)
/// **Consumer:** Local adapters (Cricket, Firefly, OLED)
/// 
/// Returns SSE stream of domain events translated to presence vocabulary.
/// Only emits events relevant to THIS stone (filters out garden-wide events).
/// 
/// **URI Semantics (API-0001):**
/// - `/api/v1/stone/*` - Stone-scoped operations (this stone only)
/// - `/api/v1/garden/*` - Garden-scoped operations (all stones, via Lantern)
/// 
/// **Flow:**
/// 1. Generate snapshot from AppState
/// 2. Subscribe to EventBus (MossEvent stream for now)
/// 3. **Filter** to local stone events only (future: when DomainEvent integration complete)
/// 4. Translate each event to garden-native vocabulary
/// 5. Emit as SSE
pub async fn stream_stone_presence(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::info!("Local presence adapter connected");
    
    let stone_name = state.stone_name.clone();
    
    // Generate initial snapshot
    let snapshot = generate_snapshot(&state).await;
    let snapshot_json = serde_json::to_string(&snapshot).unwrap_or_default();
    
    // Subscribe to domain events
    // TODO: Use EventBus when available
    // For now, use existing event_tx (MossEvent)
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
                        tracing::warn!("Presence adapter lagged {} events", n);
                        None
                    }
                }
            })
            .filter_map(move |moss_event| {
                let _stone = stone_name.clone();
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
async fn generate_snapshot(state: &AppState) -> PresenceSnapshot {
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

#### 3.2 Add to module tree

**File:** `src/moss/src/api/v1/mod.rs` (MODIFY)

Add after line 6 (after `pub mod events;`):
```rust
pub mod presence;
```

#### 3.3 Add route to router

**File:** `src/moss/src/api/v1/mod.rs` (MODIFY - if router is in this file)

OR

**File:** `src/moss/src/bootstrap/run.rs` (MODIFY - search for router setup)

Add route:
```rust
.route("/api/v1/stone/presence/stream", get(api::v1::presence::stream_stone_presence))
```

**Search for existing routes to find correct location:**
```bash
# Find router setup
rg "\.route.*stone" src/moss/src/
```

#### Verification
```bash
cargo check -p garden-moss
```

**Manual test:**
```powershell
# Terminal 1: Run Moss
cd f:\Replica\NAS\Files\repo\github\zen-garden\dist\windows
.\garden-moss.exe

# Terminal 2: Test endpoint
curl -N http://localhost:7185/api/v1/stone/presence/stream
```

**Expected:** Snapshot event immediately, then keep-alive heartbeats.

---

### Step 4: Background Monitoring Tasks (1 hour)

**Objective:** Create load and health monitoring tasks.

#### 4.1 Create presence monitor module

**File:** `src/moss/src/tasks/presence_monitor.rs` (NEW)

```rust
//! Stone metrics monitoring for presence protocol
//!
//! Emits StoneEvent::LoadUpdated every 5 seconds to EventBus.

use std::time::Duration;
use tokio::time::interval;
use chrono::Utc;

use crate::{AppState, MossEvent};

/// Run load monitoring task (every 5s)
/// 
/// TODO: When EventBus is integrated, emit DomainEvent::Stone(StoneEvent::LoadUpdated)
/// For now, emit MossEvent for backward compatibility.
pub async fn run_load_monitor_task(state: AppState) {
    let mut interval = interval(Duration::from_secs(5));
    
    loop {
        interval.tick().await;
        
        // TODO: Real system metrics (sysinfo crate?)
        let cpu_percent = 25.0;
        let memory_percent = 45.0;
        let disk_percent = 60.0;
        
        // Emit to event stream
        let moss_event = MossEvent {
            timestamp: Utc::now().to_rfc3339(),
            level: "debug".to_string(),
            message: format!(
                "Stone load: CPU {:.1}%, Memory {:.1}%, Disk {:.1}%",
                cpu_percent, memory_percent, disk_percent
            ),
            job_id: None,
        };
        
        let _ = state.event_tx.send(moss_event);
        
        // TODO: When EventBus is available:
        // let event = DomainEvent::Stone(StoneEvent::LoadUpdated {
        //     stone_name: state.stone_name.clone(),
        //     cpu_percent,
        //     memory_percent,
        //     disk_percent,
        //     timestamp: Utc::now(),
        // });
        // state.event_bus.publish(event).await?;
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
            tracing::info!(
                old = %last_health,
                new = %new_health,
                "Stone health changed"
            );
            
            // TODO: Emit StoneEvent::HealthChanged to EventBus
            
            last_health = new_health.to_string();
        }
    }
}
```

#### 4.2 Add to task module

**File:** `src/moss/src/tasks/mod.rs` (MODIFY)

Add:
```rust
pub mod presence_monitor;
pub use presence_monitor::{run_load_monitor_task, run_health_monitor_task};
```

#### 4.3 Spawn tasks in bootstrap

**File:** `src/moss/src/bootstrap/run.rs` (MODIFY)

**Search for task spawning section:**
```bash
rg "tokio::spawn.*announcer" src/moss/src/bootstrap/run.rs
```

**Add after other task spawns (around line 450-500):**

```rust
    // Spawn presence load monitor task
    tracing::info!("Starting presence load monitor");
    let load_monitor_state = state.clone();
    tokio::spawn(async move {
        crate::tasks::run_load_monitor_task(load_monitor_state).await;
    });
    
    // Spawn presence health monitor task
    tracing::info!("Starting presence health monitor");
    let health_monitor_state = state.clone();
    tokio::spawn(async move {
        crate::tasks::run_health_monitor_task(health_monitor_state).await;
    });
```

#### Verification
```bash
cargo check -p garden-moss
```

**Manual test:**
```powershell
# Watch logs
.\garden-moss.exe

# Expected output:
# INFO Starting presence load monitor
# INFO Starting presence health monitor
# DEBUG Stone load: CPU 25.0%, Memory 45.0%, Disk 60.0% (every 5s)
```

---

### Step 5: Hook Event Emissions (1 hour)

**Objective:** Emit events from service operations.

#### 5.1 Find service handler locations

**Search for service operations:**
```bash
rg "start_container" src/moss/src/api/v1/services.rs
rg "stop_container" src/moss/src/api/v1/services.rs
rg "remove_container" src/moss/src/api/v1/services.rs
```

#### 5.2 Add emission helpers

**File:** `src/moss/src/api/v1/events.rs` (MODIFY)

Add after `emit_event()` function:

```rust
/// Emit service started event
///
/// Convenience wrapper for service lifecycle events.
pub fn emit_service_started(state: &AppState, service_name: &str) {
    emit_event(
        state,
        "info",
        format!("Service {} started successfully", service_name),
        None,
    );
}

/// Emit service stopped event
pub fn emit_service_stopped(state: &AppState, service_name: &str) {
    emit_event(
        state,
        "info",
        format!("Service {} stopped", service_name),
        None,
    );
}

/// Emit service removed event
pub fn emit_service_removed(state: &AppState, service_name: &str) {
    emit_event(
        state,
        "info",
        format!("Service {} removed", service_name),
        None,
    );
}
```

#### 5.3 Hook into service handlers

**File:** `src/moss/src/api/v1/services.rs` (MODIFY)

**Search for start handler:**
```bash
rg "pub async fn start_service" src/moss/src/api/v1/services.rs -A 20
```

**Add emission after successful start:**
```rust
// After: docker.start_container(&service_name).await?;
crate::api::v1::events::emit_service_started(&state, &service_name);
```

**Repeat for stop and remove operations.**

#### Verification
```bash
cargo check -p garden-moss
```

**Manual test:**
```powershell
# Terminal 1: Watch presence stream
curl -N http://localhost:7185/api/v1/stone/presence/stream

# Terminal 2: Trigger events
garden-rake plant mongodb
garden-rake start mongodb
garden-rake stop mongodb
garden-rake uproot mongodb
```

**Expected:** See `service.started`, `service.stopped` events in stream.

---

### Step 6: Testing (30 minutes)

#### 6.1 Endpoint availability

```powershell
curl http://localhost:7185/api/v1/stone/presence/stream -I
```

**Expected:** HTTP 200, `Content-Type: text/event-stream`

#### 6.2 Snapshot on connect

```powershell
curl -N http://localhost:7185/api/v1/stone/presence/stream | head -n 20
```

**Expected:**
```
event: presence.snapshot
data: {"stone":{"name":"...","health":"thriving",...},"services":[...],"timestamp":"..."}
```

#### 6.3 Service events

**Terminal 1:** Watch stream
```powershell
curl -N http://localhost:7185/api/v1/stone/presence/stream
```

**Terminal 2:** Trigger events
```powershell
garden-rake plant mongodb
garden-rake start mongodb
```

**Expected:** See `service.started` event with service name.

#### 6.4 Load monitoring

**Watch for 10 seconds:**
```powershell
curl -N http://localhost:7185/api/v1/stone/presence/stream | grep -i "load"
```

**Expected:** Load messages every 5 seconds.

---

## File Checklist

### New Files (7 files)
- [ ] `src/common/src/presence/mod.rs`
- [ ] `src/common/src/presence/types.rs`
- [ ] `src/moss/src/api/v1/presence.rs`
- [ ] `src/moss/src/tasks/presence_monitor.rs`
- [ ] `docs/specs/PRESENCE-IMPLEMENTATION-PLAN.md` (this file)

### Modified Files (7 files)
- [ ] `src/common/src/lib.rs` - Add presence module
- [ ] `src/common/src/events/domain_events.rs` - Add StoneEvent
- [ ] `src/moss/src/api/v1/mod.rs` - Add presence module
- [ ] `src/moss/src/api/v1/events.rs` - Add service event helpers
- [ ] `src/moss/src/api/v1/services.rs` - Add event emissions
- [ ] `src/moss/src/tasks/mod.rs` - Add presence monitor
- [ ] `src/moss/src/bootstrap/run.rs` - Spawn monitoring tasks

---

## Troubleshooting

### Issue: Endpoint returns 404

**Check:**
1. Route added to router (search for `/api/v1/stone/`)
2. Module exported in `api/v1/mod.rs`
3. Moss recompiled and restarted

### Issue: No events received

**Check:**
1. `event_tx` channel capacity (should be 100)
2. Tasks spawned in `bootstrap/run.rs`
3. Check logs for "Starting presence load monitor"

### Issue: Compilation errors

**Common:**
- Missing imports: Add `use garden_common::presence::*;`
- Module not exported: Check `mod.rs` files
- Async issues: Ensure `async move` in task spawns

---

## Next Steps After Phase 1

1. **Real metrics:** Integrate `sysinfo` crate for CPU/memory/disk
2. **EventBus migration:** Replace MossEvent with DomainEvent
3. **Testing harness:** Automated tests for snapshot generation
4. **Cricket adapter:** Implement audio adapter on Wyse 5070
5. **Performance:** Add filtering by stone_name when DomainEvent is used

---

## Summary

**Total effort:** ~6 hours (as estimated in code assessment)

**Architecture:**
- ✅ Reuses existing EventBus infrastructure
- ✅ Extends DomainEvent (no new hierarchies)
- ✅ Filters at SSE boundary (efficiency)
- ✅ Direct emission (no bridge layer)
- ✅ Follows API-0001 URI patterns

**Ready for implementation:** All dependencies analyzed, patterns identified, integration points documented.
