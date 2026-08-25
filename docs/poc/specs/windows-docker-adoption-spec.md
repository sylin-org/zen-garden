# Windows Platform: Docker Availability and Offering Modes

**Version**: 1.0  
**Date**: 2026-01-25  
**Status**: Specification  

---

## Overview

Moss on Windows supports **multiple deployment modes** depending on Docker Desktop availability:

| Docker State | Native Adoption | Managed Offerings | Container Adoption |
|--------------|----------------|-------------------|-------------------|
| ✅ **Installed** | ✅ Yes | ✅ Yes | ✅ Yes |
| ❌ **Not Installed** | ✅ Yes | ❌ No | ❌ No |

**Key Principle**: Moss adapts to platform capabilities. If Docker is unavailable, Moss gracefully degrades to **adoption-only mode** while still providing value (monitoring native services).

---

## Capability Matrix

### With Docker Desktop Installed

Moss has **full capabilities**:

```
┌─────────────────────────────────────────────────────┐
│ WINDOWS + DOCKER                                    │
├─────────────────────────────────────────────────────┤
│                                                     │
│  1. Adopt Native Services                          │
│     ✓ Ollama at E:\AI\Ollama\ollama.exe            │
│     ✓ MongoDB native install on port 27017         │
│     ✓ Redis Windows Service                        │
│                                                     │
│  2. Manage Containerized Offerings                 │
│     ✓ Deploy zen-offering-mongodb                  │
│     ✓ Deploy zen-offering-postgres-repmgr          │
│     ✓ Full lifecycle control (start/stop/remove)   │
│                                                     │
│  3. Adopt Zen Garden Containers                    │
│     ✓ Adopt orphaned zen-offering-* containers     │
│     ✓ Self-heal on startup                         │
│     ✓ Reconcile command                            │
│                                                     │
└─────────────────────────────────────────────────────┘
```

### Without Docker Desktop

Moss operates in **adoption-only mode**:

```
┌─────────────────────────────────────────────────────┐
│ WINDOWS (NO DOCKER)                                 │
├─────────────────────────────────────────────────────┤
│                                                     │
│  1. Adopt Native Services                          │
│     ✓ Ollama at E:\AI\Ollama\ollama.exe            │
│     ✓ MongoDB native install on port 27017         │
│     ✓ Redis Windows Service                        │
│                                                     │
│  2. Manage Containerized Offerings                 │
│     ✗ UNAVAILABLE (Docker not installed)           │
│     ✗ garden-rake offer mongodb → Error            │
│                                                     │
│  3. Adopt Zen Garden Containers                    │
│     ✗ UNAVAILABLE (Docker not installed)           │
│                                                     │
└─────────────────────────────────────────────────────┘
```

---

## Docker Detection

### Startup Detection

On Moss startup (`bootstrap/init.rs`):

```rust
pub async fn initialize_docker(state: &AppState) -> DockerAvailability {
    match DockerManager::new().await {
        Ok(docker) => {
            // Docker available - test connectivity
            match docker.ping().await {
                Ok(_) => {
                    tracing::info!("Docker Desktop detected and operational");
                    state.set_docker_available(true).await;
                    state.set_docker_manager(Some(docker)).await;
                    DockerAvailability::Available(docker)
                }
                Err(e) => {
                    tracing::warn!("Docker installed but not running: {}", e);
                    state.set_docker_available(false).await;
                    DockerAvailability::Unavailable("Docker installed but not running".to_string())
                }
            }
        }
        Err(e) => {
            tracing::info!("Docker not available (adoption-only mode): {}", e);
            state.set_docker_available(false).await;
            DockerAvailability::Unavailable("Docker not installed".to_string())
        }
    }
}
```

### Runtime Detection

**Health monitor task** (every 30s) checks Docker availability:

```rust
// src/moss/src/tasks/health_monitor.rs
async fn check_docker_availability(state: &AppState) -> bool {
    match &state.docker {
        Some(docker) => docker.ping().await.is_ok(),
        None => {
            // Try to initialize Docker (user may have installed it)
            match DockerManager::new().await {
                Ok(docker) if docker.ping().await.is_ok() => {
                    tracing::info!("Docker newly detected - switching to full mode");
                    state.set_docker_manager(Some(docker)).await;
                    state.set_docker_available(true).await;
                    
                    // Emit event for user notification
                    state.console.emit(ConsoleEvent::new(
                        EventCategory::System,
                        EventStatus::Changed,
                        "Docker detected - managed offerings now available".to_string()
                    ));
                    true
                }
                _ => false,
            }
        }
    }
}
```

**Detection interval**: 30 seconds (same as health monitor)

---

## State Transitions

### Scenario 1: Docker Installed After Moss Startup

```
Initial State:
  - Moss running in adoption-only mode
  - Native Ollama adopted and monitored
  - Docker not available

User Action:
  1. Downloads Docker Desktop
  2. Installs Docker Desktop
  3. Starts Docker Desktop

Moss Behavior:
  ┌─ Health Monitor Cycle (30s) ─┐
  │                               │
  │  1. Try DockerManager::new()  │
  │  2. docker.ping() → OK        │
  │  3. state.docker = Some()     │
  │  4. Emit console event        │
  │                               │
  └───────────────────────────────┘

Console Output:
  ✓ Docker detected - managed offerings now available

New Capabilities:
  - garden-rake offer mongodb (now works)
  - garden-rake reconcile (now works)
  - Container adoption enabled
```

### Scenario 2: Docker Removed While Moss Running

```
Initial State:
  - Moss running in full mode
  - zen-offering-mongodb container running
  - Native Ollama adopted
  - Docker available

User Action:
  1. Stops Docker Desktop
  2. Uninstalls Docker Desktop

Moss Behavior:
  ┌─ Health Monitor Cycle (30s) ─┐
  │                               │
  │  1. docker.ping() → Error     │
  │  2. state.docker = None       │
  │  3. Emit console event        │
  │  4. Mark containers offline   │
  │                               │
  └───────────────────────────────┘

Console Output:
  ⚠️ Docker unavailable - managed offerings offline

Registry Changes:
  - zen-offering-mongodb: Status → Offline
  - Native Ollama: Unaffected (still monitored)

New Limitations:
  - garden-rake offer redis → Error (Docker unavailable)
  - garden-rake rest mongodb → Error (Container offline)
  - garden-rake adopt mongodb → Still works (native)
```

### Scenario 3: Docker Temporarily Stopped

```
Initial State:
  - Moss running in full mode
  - zen-offering-mongodb container running
  - Docker available

User Action:
  - Stops Docker Desktop (doesn't uninstall)

Moss Behavior:
  ┌─ Health Monitor Cycle (30s) ─┐
  │                               │
  │  1. docker.ping() → Error     │
  │  2. state.docker_available =  │
  │     false (keep manager)      │
  │  3. Emit console event        │
  │                               │
  └───────────────────────────────┘

Console Output:
  ⚠️ Docker Desktop stopped - start Docker to restore services

User Action:
  - Starts Docker Desktop

Moss Behavior:
  ┌─ Health Monitor Cycle (30s) ─┐
  │                               │
  │  1. docker.ping() → OK        │
  │  2. state.docker_available =  │
  │     true                      │
  │  3. Reconcile containers      │
  │  4. Emit console event        │
  │                               │
  └───────────────────────────────┘

Console Output:
  ✓ Docker Desktop restored - reconciling services...
  ✓ zen-offering-mongodb: Running
```

---

## Offering Deployment Precedence

When user runs `garden-rake offer mongodb` on Windows:

### Decision Tree

```
garden-rake offer mongodb
         |
         ├─ [Check Docker Available?]
         │        |
         │        ├─ NO → Error: "Docker required for managed offerings"
         │        │               Suggest: "garden-rake adopt mongodb" (if native exists)
         │        │
         │        └─ YES → Continue
         │                  |
         ├─ [Check zen-offering-mongodb exists?]
         │        |
         │        ├─ YES → Adopt container (self-heal)
         │        │         └─ Add to registry, start if stopped
         │        │
         │        └─ NO → Continue
         │                  |
         ├─ [Check port 27017 available?]
         │        |
         │        ├─ NO → Error: "Port conflict"
         │        │         Check: Docker containers? Native service? User container?
         │        │         Suggest: Stop conflicting service or adopt native
         │        │
         │        └─ YES → Deploy new container
         │                  └─ Create zen-offering-mongodb
```

### Example Flows

#### Flow 1: Native MongoDB exists, user wants managed

```powershell
# User has native MongoDB on port 27017
PS> garden-rake offer mongodb

Deploying mongodb to stone-crystal-forest...
✗ Failed: Port 27017 already in use

Detected: Native MongoDB service
Suggestion: Adopt the native service instead
  garden-rake adopt mongodb

Or stop native service and retry:
  net stop MongoDB
  garden-rake offer mongodb
```

#### Flow 2: Docker container exists (wrong name)

```powershell
# User has: my-mongo container on port 27017
PS> docker ps
CONTAINER ID   IMAGE     NAMES
abc123...      mongo:7   my-mongo

PS> garden-rake offer mongodb

Deploying mongodb to stone-crystal-forest...
✗ Failed: Port 27017 already in use

Detected: Docker container 'my-mongo' (not Zen Garden managed)
Suggestion: Stop container and retry
  docker stop my-mongo
  garden-rake offer mongodb

Or rename container to adopt it (future feature):
  garden-rake claim my-mongo as mongodb
```

#### Flow 3: zen-offering-mongodb exists (correct name)

```powershell
# User manually created zen-offering-mongodb
PS> docker ps
CONTAINER ID   IMAGE     NAMES
xyz789...      mongo:7   zen-offering-mongodb

PS> garden-rake offer mongodb

Deploying mongodb to stone-crystal-forest...
✓ Found existing zen-offering-mongodb container
✓ Adopting into registry (self-heal)
✓ Service healthy: mongodb
```

#### Flow 4: Clean deploy (no conflicts)

```powershell
PS> garden-rake offer mongodb

Deploying mongodb to stone-crystal-forest...
✓ Pulling image: mongo:7
✓ Creating container: zen-offering-mongodb
✓ Starting on port 27017
✓ Service healthy: mongodb
```

---

## Container Adoption Rules

### When to Adopt Containers

Moss will **automatically adopt** containers that:

1. ✅ Match naming convention: `zen-offering-{name}`
2. ✅ Have valid template: `manifests/sw/{category}/{name}.yaml`
3. ✅ Docker is available

**Discovery triggers**:
- Moss startup (coordinator task)
- Health monitor (every 30s)
- Manual reconcile (`garden-rake reconcile`)

### What Moss WON'T Adopt as Containers

Moss **ignores** these containers for managed offerings:

| Container Name | Reason | Alternative |
|----------------|--------|-------------|
| `my-mongo` | Wrong name (not `zen-offering-*`) | Deploy as `zen-offering-mongodb` |
| `user-redis` | Wrong name | Deploy as `zen-offering-redis` |
| `zen-offering-foobar` | No template (invalid offering) | Create template or remove |
| Any container (when Docker unavailable) | Docker not installed | Install Docker Desktop |

---

## Error Messages

### Docker Not Available

```powershell
PS> garden-rake offer mongodb

✗ Error: Docker required for managed offerings

Moss is running in adoption-only mode (Docker not detected).

To deploy managed offerings:
  1. Install Docker Desktop for Windows
     https://docs.docker.com/desktop/install/windows-install/
  2. Start Docker Desktop
  3. Wait 30 seconds (Moss will detect Docker)
  4. Retry: garden-rake offer mongodb

Alternative: Adopt native MongoDB if installed
  garden-rake adopt mongodb
```

### Docker Stopped (Temporarily)

```powershell
PS> garden-rake offer mongodb

✗ Error: Docker Desktop not running

Docker is installed but not running.

To restore managed offerings:
  1. Start Docker Desktop (from Windows Start menu)
  2. Wait for Docker Desktop to be ready
  3. Wait 30 seconds (Moss will detect Docker)
  4. Retry: garden-rake offer mongodb
```

### Port Conflict with Native Service

```powershell
PS> garden-rake offer mongodb

✗ Error: Port 27017 already in use

Detected: Native service on port 27017 (MongoDB?)

Options:
  a) Adopt the native service (monitoring only):
     garden-rake adopt mongodb

  b) Stop native service and use managed container:
     net stop MongoDB
     garden-rake offer mongodb

  c) Use different port (future feature):
     garden-rake offer mongodb with port=27018
```

---

## AppState Structure

```rust
pub struct AppState {
    // Docker manager (None if Docker unavailable)
    pub docker: Arc<RwLock<Option<DockerManager>>>,
    
    // Docker availability flag (for quick checks)
    pub docker_available: Arc<RwLock<bool>>,
    
    // Adopted services registry (always available)
    pub adopted_services: Arc<RwLock<HashMap<String, AdoptedService>>>,
    
    // Managed services registry (requires Docker)
    pub managed_services: Arc<RwLock<HashMap<String, ServiceInfo>>>,
    
    // ... other fields
}

impl AppState {
    pub async fn set_docker_available(&self, available: bool) {
        let mut state = self.docker_available.write().await;
        *state = available;
    }
    
    pub async fn is_docker_available(&self) -> bool {
        *self.docker_available.read().await
    }
    
    pub async fn set_docker_manager(&self, manager: Option<DockerManager>) {
        let mut docker = self.docker.write().await;
        *docker = manager;
    }
}
```

---

## API Behavior

### POST /api/v1/offerings (Plant Offering)

**With Docker**:
```json
POST /api/v1/offerings
{"name": "mongodb"}

→ 202 Accepted
{"job_id": "job_abc123", "status": "installing"}
```

**Without Docker**:
```json
POST /api/v1/offerings
{"name": "mongodb"}

→ 503 Service Unavailable
{
  "error": "DOCKER_UNAVAILABLE",
  "message": "Docker required for managed offerings",
  "suggestion": "Install Docker Desktop or use adoption mode"
}
```

### POST /api/v1/offerings/:name/adopt (Adopt Service)

**Always available** (Docker not required):

```json
POST /api/v1/offerings/mongodb/adopt
{"endpoint": "localhost:27017"}

→ 200 OK
{"name": "mongodb", "mode": "adopted", "vitality": "THRIVING"}
```

### GET /api/v1/stone/capabilities

Returns platform capabilities:

```json
GET /api/v1/stone/capabilities

→ 200 OK
{
  "platform": "Windows",
  "arch": "x86_64",
  "docker_available": true,
  "docker_version": "24.0.7",
  "capabilities": {
    "managed_offerings": true,
    "container_adoption": true,
    "native_adoption": true
  }
}
```

When Docker unavailable:

```json
{
  "platform": "Windows",
  "arch": "x86_64",
  "docker_available": false,
  "docker_version": null,
  "capabilities": {
    "managed_offerings": false,
    "container_adoption": false,
    "native_adoption": true
  }
}
```

---

## Implementation Checklist

### Phase 1: Docker Detection (Current)
- [x] `DockerManager::new()` attempts connection
- [x] Graceful failure if Docker unavailable
- [ ] **TODO**: Store `docker_available` flag in AppState
- [ ] **TODO**: Periodic Docker availability check in health monitor

### Phase 2: State Transitions
- [ ] Detect Docker installed after startup
- [ ] Detect Docker uninstalled/stopped during runtime
- [ ] Emit console events on state changes
- [ ] Update managed services status when Docker unavailable

### Phase 3: API Guards
- [ ] Check `docker_available` in `POST /api/v1/offerings`
- [ ] Return 503 with actionable error if Docker unavailable
- [ ] Add `GET /api/v1/stone/capabilities` endpoint
- [ ] Update OpenAPI spec

### Phase 4: Error Messages
- [ ] Contextual errors for Docker unavailable
- [ ] Suggest Docker installation link
- [ ] Suggest adoption mode for native services
- [ ] Port conflict detection with suggestions

### Phase 5: Documentation
- [x] Windows-specific behavior specification (this doc)
- [ ] Update user guides with Docker requirements
- [ ] Add troubleshooting section for Docker issues
- [ ] CLI help text for Docker-dependent commands

---

## Testing Scenarios

### Test 1: Moss Startup Without Docker

```
Given: Windows machine without Docker Desktop
When: Moss starts
Then:
  - Moss initializes successfully
  - docker_available = false
  - Native adoption works
  - Managed offerings fail with clear error
```

### Test 2: Docker Installed While Moss Running

```
Given: Moss running in adoption-only mode
When: User installs and starts Docker Desktop
Then:
  - Within 30 seconds, Moss detects Docker
  - docker_available = true
  - Console event emitted
  - Managed offerings now work
```

### Test 3: Docker Stopped While Moss Running

```
Given: Moss running with managed offerings
When: User stops Docker Desktop
Then:
  - Within 30 seconds, Moss detects Docker unavailable
  - docker_available = false
  - Managed services marked offline
  - Native adopted services unaffected
```

### Test 4: Offer Command Without Docker

```
Given: Moss in adoption-only mode (no Docker)
When: User runs "garden-rake offer mongodb"
Then:
  - Returns error: "Docker required"
  - Suggests installing Docker
  - Suggests adoption alternative
```

### Test 5: Adopt Command Without Docker

```
Given: Moss in adoption-only mode (no Docker)
When: User runs "garden-rake adopt ollama"
Then:
  - Command succeeds
  - Ollama adopted as native service
  - No Docker required
```

---

## Related Documents

- [OFFER-0001](../decisions/OFFER-0001-taxonomy.md) - Offering taxonomy (Managed/Adopted/Borrowed)
- [OFFER-0002](../decisions/OFFER-0002-container-namespace-collision.md) - Container naming convention
- [Container Collision Avoidance](../guides/container-collision-avoidance.md) - Deployment precedence rules
- [OS-Aware Detection](../guides/os-aware-detection.md) - Platform-specific detection commands

---

## Summary

**Key Principles**:

1. **Graceful Degradation**: Moss works without Docker (adoption-only mode)
2. **Dynamic Adaptation**: Moss detects Docker installation/removal at runtime
3. **Clear Boundaries**: Container adoption only for `zen-offering-*` names
4. **Mode Precedence**: Native service → blocks managed offering on same port
5. **User Guidance**: Actionable errors with clear next steps

**Windows-Specific Behaviors**:

| Feature | Docker Installed | No Docker |
|---------|-----------------|-----------|
| Native adoption | ✅ Yes | ✅ Yes |
| Managed offerings | ✅ Yes | ❌ No (clear error) |
| Container adoption | ✅ Yes | ❌ No |
| Port conflict detection | ✅ Yes | ⚠️ Limited (native only) |
| Self-heal containers | ✅ Yes | ❌ No |
