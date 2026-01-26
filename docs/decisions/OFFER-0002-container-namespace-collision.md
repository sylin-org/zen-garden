# OFFER-0002: Container Namespace Collision Avoidance

**Status**: Accepted  
**Date**: 2026-01-25  
**Deciders**: Engineering  

---

## Context

Windows machines (and Linux/macOS) MAY run Docker alongside native services. This creates potential namespace collisions:

**Scenario**: User has MongoDB running as:
- Native service on port 27017 (outside Moss control)
- Docker container `my-mongo` on port 17037 (outside Moss naming convention)
- Docker container `zen-offering-mongodb` (Moss-managed, correct)

**Problem**: When user runs `garden-rake offer mongodb`:
- Should Moss adopt the native MongoDB? ❌ No (wrong deployment mode)
- Should Moss adopt `my-mongo` container? ❌ No (wrong namespace)
- Should Moss deploy new container? ✅ **Yes** (if `zen-offering-mongodb` doesn't exist)

**Risk**: Without namespace enforcement, Moss could:
1. Adopt wrong services (port collision, different config)
2. Treat external containers as Zen Garden offerings
3. Create confusion about which services Moss manages

---

## Decision

**Managed Offerings MUST use the `zen-offering-{name}` container naming convention.**

When deploying managed offerings (containers), Moss will:

1. **Check for existing `zen-offering-{name}` container FIRST**
   - If exists → Adopt it (self-heal: user manually started container)
   - If running but not in registry → Add to registry
   - If stopped → Start and add to registry

2. **Deploy new container ONLY if `zen-offering-{name}` doesn't exist**
   - Use exact name: `zen-offering-mongodb`
   - Ignore containers with different names (even same image/port)
   - Ignore native services (even same port)

3. **NEVER adopt non-Zen containers for managed offerings**
   - `my-mongo` on port 27017 → Ignored (not `zen-offering-*`)
   - Native MongoDB on port 27017 → Ignored (not a container)
   - Only `zen-offering-mongodb` is relevant for `offer mongodb`

4. **For adopted offerings, use separate namespace**
   - Native services: No container, store in adopted registry
   - External containers: Can adopt if explicitly requested (future: `garden-rake claim my-mongo as mongodb`)

---

## Implementation

### Current State (v0.1.0)

**Implemented**:
- ✅ Container naming convention: All managed offerings use `zen-offering-{name}`
- ✅ Discovery: `list_zen_containers()` filters by `zen-offering-` prefix
- ✅ Self-heal: Health monitor adopts orphaned `zen-offering-*` containers (every 30s)
- ✅ Reconciliation: Manual `garden-rake reconcile` adopts all orphaned containers

**TODO** (Future Enhancement):
- ⚠️ **Deploy-time self-heal**: Currently `install_service()` fails if `zen-offering-{name}` exists. Should adopt instead.
- ⚠️ **Port collision detection**: Should check port availability before deploying (return actionable error)

### Container Deployment Flow (install_service_task)

```rust
// src/moss/src/tasks/job_executors.rs
pub async fn install_service_task(state: &AppState, job_id: &str, offering: &str) {
    let container_name = format!("zen-offering-{}", offering);
    
    // Step 1: Check if zen-offering-{name} exists
    match state.docker.get_container_status(&container_name).await {
        Ok(status) if status.exists => {
            // Container exists with correct name - adopt it
            tracing::info!(offering, "Found existing zen-offering container, adopting");
            adopt_existing_container(state, offering, &container_name).await;
            return;
        }
        _ => {
            // Container doesn't exist or Docker error - deploy new
            tracing::info!(offering, "No zen-offering container found, deploying new");
        }
    }
    
    // Step 2: Deploy new container with zen-offering-{name}
    let result = state.docker.create_container(
        &container_name,
        &compiled.image,
        &compiled.ports,
        &compiled.volumes,
        &compiled.environment,
    ).await;
    
    // Step 3: Add to registry
    state.registry.add_service(ServiceInfo {
        name: offering.to_string(),
        container_name: Some(container_name),
        deployment_mode: DeploymentMode::Managed,
        // ...
    }).await;
}
```

### Container Discovery (list_zen_containers)

```rust
// src/moss/src/docker.rs
pub async fn list_zen_containers(&self) -> Result<Vec<String>> {
    // Only list containers with zen-offering- prefix
    let filters = HashMap::from([
        ("name".to_string(), vec!["zen-offering-".to_string()])
    ]);
    
    // Returns: ["mongodb", "postgres", "redis"]
    // (strips zen-offering- prefix)
}
```

### Reconciliation (self-heal)

```rust
// src/moss/src/domain/adoption.rs
pub async fn adopt_all_zen_containers(state: &AppState) -> Result<Vec<String>> {
    // Only adopt containers with zen-offering- prefix
    let existing = state.docker.list_zen_containers().await?;
    
    for offering in existing {
        // Validate against template (must have manifest)
        if let Some(template) = state.manifests.sw.get(&offering) {
            adopt_offering_container(&state.docker, &state.manifests, &offering).await?;
        } else {
            // zen-offering-* container but no template (orphan)
            tracing::warn!(offering, "Found orphaned zen-offering container (no template)");
        }
    }
}
```

---

## Consequences

### Positive

✅ **Clear ownership boundary**: Only `zen-offering-*` containers are Moss-managed  
✅ **No accidental adoption**: Won't claim user's existing containers  
✅ **Self-heal works safely**: Can adopt orphaned `zen-offering-*` containers  
✅ **Port collision detection**: Can detect conflicting services before deploy  
✅ **Multi-mode coexistence**: Managed (containers) + Adopted (native) on same host  

### Negative

⚠️ **Manual migration required**: Can't auto-adopt `my-mongo` → must redeploy as `zen-offering-mongodb`  
⚠️ **Name conflicts**: User can't run both `zen-offering-mongodb` (Moss) and `my-mongo` (personal) on same port  

### Neutral

📌 **Companion containers**: Also use prefix: `zen-companion-{offering}-{sidecar}`  
📌 **Adopted offerings**: Stored in separate registry, no container name  

---

## Examples

### Example 1: Clean Deploy

```bash
# User: garden-rake offer mongodb
# Moss: Checks for zen-offering-mongodb → doesn't exist
# Moss: Deploys new container as zen-offering-mongodb
# Result: Container running, in registry
```

### Example 2: Self-Heal (Orphan Container)

```bash
# User manually runs: docker run -d --name zen-offering-mongodb mongo:7
# User: garden-rake offer mongodb
# Moss: Checks for zen-offering-mongodb → exists
# Moss: Adopts existing container, adds to registry
# Result: Container running, in registry, no downtime
```

### Example 3: Port Collision (Safe Failure)

```bash
# User has native MongoDB on port 27017
# User: garden-rake offer mongodb
# Moss: Checks for zen-offering-mongodb → doesn't exist
# Moss: Attempts to create container on port 27017 → FAILS (port in use)
# Result: Error message, suggests checking for port conflicts
```

### Example 4: External Container (Ignored)

```bash
# User has: docker run -d --name my-mongo -p 27017:27017 mongo:7
# User: garden-rake offer mongodb
# Moss: Checks for zen-offering-mongodb → doesn't exist
# Moss: Ignores my-mongo (wrong name)
# Moss: Attempts to deploy zen-offering-mongodb on port 27017 → FAILS (port in use)
# Result: Error message, user must stop my-mongo or change port
```

### Example 5: Multi-Mode Coexistence

```bash
# User has native Ollama on port 11434 (adopted)
# User: garden-rake offer mongodb
# Moss: Checks for zen-offering-mongodb → doesn't exist
# Moss: Deploys new container as zen-offering-mongodb on port 27017
# Result: Both services coexist:
#   - Ollama (native, adopted, monitored only)
#   - MongoDB (container, managed, full control)
```

---

## Related Decisions

- [OFFER-0001](OFFER-0001-taxonomy.md) - Offering taxonomy (Managed/Adopted/Borrowed)
- [STATE-0001](STATE-0001-stateless-moss.md) - Registry persistence
- [API-0001](API-0001-dual-layer-api.md) - Offerings vs Services API

---

## Notes

**Container name pattern**: `zen-offering-{offering}` (singular, kebab-case)
- ✅ `zen-offering-mongodb`
- ✅ `zen-offering-postgres-repmgr`
- ❌ `zen-offerings-mongodb` (no plural)
- ❌ `garden-offering-mongodb` (wrong prefix)
- ❌ `mongo` (no prefix)

**Companion pattern**: `zen-companion-{offering}-{sidecar}`
- ✅ `zen-companion-postgres-repmgr-pgbackrest`
- ❌ `zen-offering-pgbackrest` (not standalone offering)

**Future consideration**: `garden-rake claim {container} as {offering}` to migrate external containers into Zen Garden namespace (requires stop, rename, restart).
