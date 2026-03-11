# Container Collision Avoidance

**Audience**: Developers, SREs  
**Scenario**: Windows/Linux/macOS machines with both Docker and native services  

---

## The Problem

You have a Windows machine with:
- **Docker Desktop** installed (can run containers)
- **Native Ollama** installed at `E:\AI\Ollama\ollama.exe` (not in container)
- **User's MongoDB** container running as `my-mongo` on port 27017
- **Zen Garden Moss** managing services

**Question**: When you run `garden-rake offer mongodb`, what should happen?

❌ **Bad behavior**:
- Adopt native Ollama as containerized service (wrong mode)
- Adopt `my-mongo` container (user's personal container, wrong config)
- Deploy new container on port 27017 (conflicts with `my-mongo`)

✅ **Correct behavior**:
- Check if `zen-offering-mongodb` container exists
- If yes: Adopt it (self-heal)
- If no: Deploy new container as `zen-offering-mongodb`
- If port collision: Fail with clear error message

---

## The Solution: Container Naming Convention

**Rule**: Managed offerings MUST use the `zen-offering-{name}` container naming convention.

**Instance note**: `{name}` is the offering FQN encoded for containers.  
Example: `ollama::dev` → `zen-offering-ollama--dev`.

### Namespace Boundaries

| Namespace | Ownership | Example | Moss Behavior |
|-----------|-----------|---------|---------------|
| `zen-offering-*` | Moss-managed containers | `zen-offering-mongodb` | Full control (adopt/start/stop) |
| `zen-companion-*` | Moss-managed sidecars | `zen-companion-postgres-repmgr-pgbackrest` | Full control |
| Adopted registry | Native services (no container) | Ollama at `localhost:11434` | Monitor only (or start/stop via commands) |
| External containers | User-owned containers | `my-mongo`, `user-redis` | Ignored (not Moss-managed) |
| Native services | Non-containerized | MongoDB native install | Ignored (unless adopted) |

---

## Deployment Logic

When you run `garden-rake offer mongodb`, Moss follows this flow:

```
1. Check: Does zen-offering-mongodb container exist?
   ├─ YES → Adopt it (add to registry if missing)
   │         │
   │         ├─ Running? → Add to registry, monitor
   │         └─ Stopped? → Start + add to registry
   │
   └─ NO → Deploy new container
             │
             ├─ Create: zen-offering-mongodb
             ├─ Image: mongo:7 (from template)
             ├─ Port: 27017 (or next available)
             ├─ Volumes: mongodb-data
             └─ Registry: Add as managed offering

2. If port collision detected:
   └─ Fail with error:
      "Port 27017 already in use. Check for conflicting services:
       - docker ps -a (look for containers on port 27017)
       - netstat -ano | findstr 27017 (Windows)
       - lsof -i :27017 (Linux/macOS)"
```

---

## Examples

### Example 1: Clean Deploy

**Setup**: No MongoDB containers exist

```powershell
PS> garden-rake offer mongodb
Deploying mongodb to stone-crystal-forest...
✓ Pulling image: mongo:7
✓ Creating container: zen-offering-mongodb
✓ Starting on port 27017
✓ Service healthy: mongodb
```

**Result**:
- Container: `zen-offering-mongodb` (running)
- Registry: mongodb (managed, healthy)
- Port: 27017

---

### Example 2: Self-Heal (Orphan Container)

**Setup**: User manually created `zen-offering-mongodb` but it's not in registry

```powershell
# User ran this manually
PS> docker run -d --name zen-offering-mongodb -p 27017:27017 mongo:7
abc123...

# Now use Moss
PS> garden-rake offer mongodb
Found existing zen-offering-mongodb container
✓ Adopting container into registry
✓ Service healthy: mongodb
```

**Result**:
- Container: `zen-offering-mongodb` (running, no restart)
- Registry: mongodb (managed, healthy)
- Port: 27017

---

### Example 3: Port Collision (User's Container)

**Setup**: User has `my-mongo` on port 27017

```powershell
PS> docker ps
CONTAINER ID   IMAGE     COMMAND                  NAMES
xyz789...      mongo:7   "docker-entrypoint.s…"   my-mongo

PS> garden-rake offer mongodb
Deploying mongodb to stone-crystal-forest...
✓ Pulling image: mongo:7
✗ Failed to start container: port 27017 already in use

Troubleshooting:
  1. Check for conflicting containers:
     docker ps -a | findstr 27017
  2. Stop conflicting service:
     docker stop my-mongo
  3. Or use custom port (future feature):
     garden-rake offer mongodb with port=27018
```

**Result**:
- Container: None (deploy failed)
- Registry: No entry
- Port: 27017 (still used by `my-mongo`)

---

### Example 4: Port Collision (Native Service)

**Setup**: Native MongoDB installed on port 27017

```powershell
PS> netstat -ano | findstr 27017
TCP    0.0.0.0:27017    0.0.0.0:0    LISTENING    4567

PS> garden-rake offer mongodb
Deploying mongodb to stone-crystal-forest...
✓ Pulling image: mongo:7
✗ Failed to start container: port 27017 already in use

Troubleshooting:
  1. A native service is using port 27017
  2. Options:
     a) Stop native MongoDB and use containerized version
     b) Adopt native MongoDB instead:
        garden-rake adopt mongodb at localhost:27017
     c) Use custom port for container (future feature)
```

**Result**:
- Container: None (deploy failed)
- Registry: No entry
- Port: 27017 (still used by native MongoDB)

**Solution**: Adopt the native service instead:

```powershell
PS> garden-rake adopt mongodb at localhost:27017
Detecting native MongoDB...
✓ Found MongoDB 7.0.5 at localhost:27017
✓ Adopted as monitoring-only
✓ Service vitality: THRIVING
```

**Result**:
- Container: None (native service)
- Registry: mongodb (adopted, thriving)
- Port: 27017 (native MongoDB)

---

### Example 5: Multi-Mode Coexistence

**Setup**: Native Ollama (adopted) + MongoDB container (managed)

```powershell
# Step 1: Adopt native Ollama
PS> garden-rake adopt ollama
Detecting native Ollama...
✓ Found Ollama 0.15.0 at localhost:11434
✓ Adopted as monitoring-only
✓ Service vitality: THRIVING

# Step 2: Deploy containerized MongoDB
PS> garden-rake offer mongodb
Deploying mongodb to stone-crystal-forest...
✓ Pulling image: mongo:7
✓ Creating container: zen-offering-mongodb
✓ Starting on port 27017
✓ Service healthy: mongodb

# Observe both
PS> garden-rake observe
STONE: stone-crystal-forest
  Services:
    ollama (adopted, native)
      • Vitality: THRIVING
      • Endpoint: http://localhost:11434
      • Mode: Monitor only
    
    mongodb (managed, container)
      • Status: Healthy
      • Container: zen-offering-mongodb
      • Port: 27017
      • Mode: Full control
```

**Result**:
- Ollama: Native service (adopted, monitored)
- MongoDB: Container `zen-offering-mongodb` (managed, full control)
- **Both coexist peacefully** (different modes, different ports)

---

## When Moss WON'T Adopt Containers

Moss **ignores** these containers:

| Container Name | Why Ignored | What to Do |
|----------------|-------------|------------|
| `my-mongo` | Wrong name (not `zen-offering-*`) | Rename or migrate |
| `user-redis` | Wrong name | Rename or migrate |
| `zen-offering-foobar` | No template (`foobar.yaml` doesn't exist) | Create template or remove container |
| `postgres-repmgr` | Wrong name | Deploy as `zen-offering-postgres-repmgr` |

**Migration path** (future feature):
```powershell
# Migrate external container into Zen Garden namespace
PS> garden-rake claim my-mongo as mongodb
⚠️  This will:
    1. Stop container: my-mongo
    2. Rename to: zen-offering-mongodb
    3. Start container with new name
    4. Add to Moss registry
    
Proceed? [Y/n]: y
✓ Container migrated
✓ Service healthy: mongodb
```

---

## When Moss WILL Adopt Containers

Moss **adopts** containers that:

1. ✅ Match naming convention: `zen-offering-{name}`
2. ✅ Have valid template: `manifests/sw/{category}/{name}.yaml`
3. ✅ Discovered during:
   - Startup (coordinator task)
   - Health monitor (every 30s)
   - Manual reconcile (`garden-rake reconcile`)

**Example**: Orphan container self-heal

```powershell
# User manually starts container
PS> docker run -d --name zen-offering-redis -p 6379:6379 redis:7
abc123...

# Moss discovers it automatically (within 30s)
# Or trigger manual reconcile
PS> garden-rake reconcile
Reconciling garden...
✓ Found orphan: zen-offering-redis
✓ Adopted into registry
✓ Service healthy: redis
```

---

## Port Collision Detection

**Before deploying**, Moss should check for port conflicts:

```rust
// Future implementation
async fn check_port_availability(port: u16) -> Result<bool> {
    // Option 1: Try to bind socket (most reliable)
    match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(_) => Ok(true),  // Port available
        Err(_) => Ok(false), // Port in use
    }
    
    // Option 2: Query Docker for containers using this port
    let containers = docker.list_containers_by_port(port).await?;
    
    // Option 3: Check adopted services registry
    let adopted = state.registry.get_adopted_services_by_port(port).await?;
}
```

**Error message** should be actionable:

```
❌ Failed to deploy mongodb: port 27017 already in use

Possible causes:
  1. Another Zen Garden offering is using this port
  2. A Docker container is using this port (check: docker ps)
  3. A native service is using this port (check: netstat -ano | findstr 27017)

Solutions:
  • If it's a native service: garden-rake adopt mongodb
  • If it's a user container: Stop it or use different port
  • If it's another offering: Choose different port (future feature)
```

---

## Summary

| Scenario | Moss Action | Reason |
|----------|-------------|--------|
| `zen-offering-mongodb` exists | Adopt it | Self-heal (user manually started) |
| No `zen-offering-*` container | Deploy new `zen-offering-mongodb` | Normal managed deploy |
| `my-mongo` exists on port 27017 | Fail with error | Wrong name, port collision |
| Native MongoDB on port 27017 | Fail with error + suggest adopt | Port collision, should use adopted mode |
| `zen-offering-redis` running, not in registry | Adopt during health check | Self-heal (orphan container) |

**Key insight**: The `zen-offering-*` naming convention creates a **clear ownership boundary** between Moss-managed containers and everything else. This prevents accidental adoption while enabling safe self-heal for orphaned Zen Garden containers.

---

## Related Documentation

- [OFFER-0002](../decisions/OFFER-0002-container-namespace-collision.md) - Architecture decision
- [OFFER-0001](../decisions/OFFER-0001-taxonomy.md) - Offering taxonomy (Managed/Adopted/Borrowed)
- [offering-lifecycle.md](offering-lifecycle.md) - How to plant/manage offerings
