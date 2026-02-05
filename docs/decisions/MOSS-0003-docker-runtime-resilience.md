---
audience: [contributor, maintainer, ai]
doc_type: adr
status: accepted
last_verified: 2026-02-04
canonical: true
---

# MOSS-0003: Docker Runtime Resilience

**Status**: Accepted
**Date**: 2026-02-04
**Deciders**: Architecture Team
**Tags**: [moss, docker, resilience, monitoring, graceful-degradation]

---

## Context

Moss daemon has startup retry logic for Docker connection (30 retries × 2s delay = ~60s tolerance), but **no runtime resilience**. If Docker becomes unavailable after successful startup (e.g., daemon restart, Docker Desktop pause, system resource exhaustion), all Docker-dependent operations fail immediately with no automatic recovery.

### Problem Scenarios

1. **Docker Desktop Pause** (Windows/Mac): User pauses Docker Desktop, API calls fail
2. **Docker Daemon Restart**: `systemctl restart docker` causes temporary unavailability
3. **Resource Exhaustion**: Docker daemon becomes unresponsive under load
4. **Self-Update**: During Moss self-update, Docker may be temporarily unavailable

### Requirements

- Detect Docker disconnection at runtime
- Automatically attempt reconnection
- Allow background tasks to gracefully skip Docker operations when unavailable
- Allow API handlers to return appropriate errors when Docker unavailable
- Emit events for observability (logging, metrics, UI notifications)

---

## Decision

We will implement a **DockerMonitor** background task that mirrors the existing **NetworkMonitor** pattern, providing:

1. **State-aware polling** with configurable intervals
2. **Atomic readiness flag** via `subsystems.docker.ready`
3. **Event broadcasting** via `broadcast::channel`
4. **Graceful degradation** through `require_docker()` helper

### Pattern: Mirror NetworkMonitor

The existing `NetworkMonitor` (introduced for IP change detection and network availability) provides a battle-tested pattern:

```
NetworkMonitor                    DockerMonitor
─────────────────────────────────────────────────────────────
Polls get_local_ip()             Polls docker.is_healthy()
5s interval when disconnected    5s interval when disconnected
30s interval when connected      30s interval when connected
subsystems.network.ready flag    subsystems.docker.ready flag
NetworkEvent enum                DockerEvent enum
```

This consistency reduces cognitive load and ensures predictable behavior across subsystems.

---

## Rationale

### Why Mirror NetworkMonitor?

1. **Proven Pattern**: NetworkMonitor has been in production, handling edge cases like DHCP renewals, cable disconnects, and suspend/resume cycles
2. **Consistent API**: Developers familiar with one monitor understand both
3. **Shared Infrastructure**: Uses same `SubSystems` struct, same `AtomicBool` pattern, same polling approach
4. **Minimal New Code**: ~180 lines total, following existing template

### Why Not Recreate DockerManager?

The bollard Docker client library handles reconnection internally. When the Docker daemon becomes available again, subsequent API calls succeed without needing to recreate the client instance. The monitor only needs to track health state, not manage connection lifecycle.

### Why Atomic Flag?

```rust
// Non-blocking check from any task/handler
if state.subsystems.docker.ready.load(Ordering::Relaxed) {
    // Safe to use Docker
}
```

- **Zero async overhead**: No `.await`, no lock contention
- **Cross-task visibility**: Works from any thread/task
- **Matches network pattern**: `network.ready` uses same approach

### Why 5s/30s Intervals?

| State | Interval | Rationale |
|-------|----------|-----------|
| Disconnected | 5s | Aggressive retry for quick recovery |
| Connected | 30s | Relaxed polling, minimal overhead |

These match NetworkMonitor defaults, providing consistent behavior and avoiding "why are these different?" questions.

---

## Consequences

### Positive

- **Automatic Recovery**: Docker ops resume when daemon becomes available
- **No Crashes**: Background tasks gracefully skip Docker work during outages
- **Clear API Errors**: Handlers return 503 with `DOCKER_UNAVAILABLE` error code
- **Observability**: `DockerEvent::Disconnected` and `DockerEvent::Reconnected` enable logging, metrics, and UI indicators
- **Pattern Consistency**: Developers can apply same mental model as network resilience

### Negative

- **Polling Overhead**: Health check every 30s when connected (minimal: single HTTP HEAD to Docker socket)
- **Delayed Detection**: Up to 30s to detect disconnection when previously connected
- **No Per-Operation Retry**: Individual API calls don't retry; they fail fast and rely on user retry

### Neutral

- **API handlers must opt-in**: Call `require_docker(&state)?` explicitly (not automatic)
- **Background tasks must opt-in**: Check flag before Docker operations
- **Events not persisted**: DockerEvents are in-memory only (no event log)

---

## Implementation

### Files Created

| File | Purpose |
|------|---------|
| `src/moss/src/tasks/docker_monitor.rs` | DockerMonitor task, config, events |

### Files Modified

| File | Changes |
|------|---------|
| `src/moss/src/app_state.rs` | Added `DockerSubSystem` to `SubSystems` |
| `src/moss/src/tasks/mod.rs` | Export `DockerMonitor`, `DockerMonitorConfig`, `DockerEvent` |
| `src/moss/src/lib.rs` | Re-export Docker monitor types |
| `src/moss/src/bootstrap/run.rs` | Start DockerMonitor at Phase 7.5 |
| `src/moss/src/tasks/health_monitor.rs` | Skip container checks when Docker unavailable |
| `src/moss/src/infra/api_helpers.rs` | Added `require_docker()` helper |
| `src/moss/src/infra/mod.rs` | Export `require_docker` |

### Key Code Patterns

#### 1. DockerMonitor Struct (mirrors NetworkMonitor)

```rust
#[derive(Clone)]
pub struct DockerMonitor {
    _docker: Arc<DockerManager>,
    tx: broadcast::Sender<DockerEvent>,
    docker_ready: Arc<AtomicBool>,
}
```

#### 2. State-Aware Polling Loop

```rust
loop {
    let interval = if was_disconnected {
        Duration::from_secs(config.disconnect_retry_secs)  // 5s
    } else {
        Duration::from_secs(config.connected_poll_secs)    // 30s
    };

    tokio::time::sleep(interval).await;

    let is_healthy = docker.is_healthy().await;
    // Update flag and emit event on state change...
}
```

#### 3. SubSystems Integration

```rust
pub struct SubSystems {
    pub network: NetworkSubSystem,
    pub docker: DockerSubSystem,  // NEW
}

pub struct DockerSubSystem {
    pub ready: Arc<AtomicBool>,
}
```

#### 4. API Handler Guard

```rust
pub fn require_docker(state: &AppState) -> Result<(), (StatusCode, Json<ApiErrorResponse>)> {
    if !state.subsystems.docker.ready.load(Ordering::Relaxed) {
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            DOCKER_UNAVAILABLE,
            "Docker daemon is currently unavailable...",
            None,
        ));
    }
    Ok(())
}
```

#### 5. Background Task Guard

```rust
// In health_monitor_task:
if !state.subsystems.docker.ready.load(Ordering::Relaxed) {
    tracing::debug!("Docker unavailable, skipping container checks");
    continue;
}
```

---

## Alternatives Considered

### Alternative 1: Retry on Each Docker Call

- **Description**: Wrap each bollard call with retry logic
- **Pros**: Fine-grained control, per-operation retry
- **Cons**: Code duplication, inconsistent timeout behavior, complex error handling
- **Rejected because**: High maintenance burden, bollard already handles some reconnection

### Alternative 2: Circuit Breaker Pattern

- **Description**: After N failures, stop trying for M seconds
- **Pros**: Prevents thundering herd, reduces load on recovering daemon
- **Cons**: Complex state machine, overkill for single-node scenario
- **Rejected because**: Polling monitor achieves same goal more simply

### Alternative 3: Event-Driven Detection (Docker Events API)

- **Description**: Subscribe to Docker event stream for lifecycle events
- **Pros**: Instant detection, no polling
- **Cons**: Event stream itself can fail, more complex reconnection logic
- **Rejected because**: Polling is simpler and sufficient for 5-30s latency requirements

---

## References

- [NetworkMonitor](../../src/moss/src/tasks/network_monitor.rs) - Pattern template
- [MOSS-0002](MOSS-0002-infrastructure-handlers.md) - Infrastructure handlers (related resilience pattern)
- [Bollard Docker Library](https://github.com/fussybeaver/bollard) - Rust Docker client

---

## Usage Guide

### For API Handlers

Add Docker guard at handler start:

```rust
pub async fn create_service_v1(
    State(state): State<AppState>,
    // ...
) -> Result<Json<Response>, (StatusCode, Json<ApiErrorResponse>)> {
    require_docker(&state)?;  // Returns 503 if Docker unavailable

    // Docker operations safe from here...
}
```

### For Background Tasks

Check flag before Docker operations:

```rust
loop {
    interval.tick().await;

    if !state.subsystems.docker.ready.load(Ordering::Relaxed) {
        tracing::debug!("Docker unavailable, skipping iteration");
        continue;
    }

    // Docker operations...
}
```

### For Event Consumers

Subscribe to Docker events:

```rust
let mut rx = docker_monitor.subscribe();
while let Ok(event) = rx.recv().await {
    match event {
        DockerEvent::Disconnected { reason } => {
            // Log, alert, update UI...
        }
        DockerEvent::Reconnected => {
            // Resume operations, clear alerts...
        }
        DockerEvent::Connected => {
            // Initial connection established
        }
    }
}
```

---

**Last Updated**: February 4, 2026
**Maintained By**: Architecture Team
