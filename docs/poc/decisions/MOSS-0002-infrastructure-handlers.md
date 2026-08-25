# MOSS-0002: Infrastructure Handlers for Garden-Wide Effects

## Status
Accepted

## Context
Certain offerings have "garden-wide effects" - when deployed on any Stone, they should trigger configuration changes on all Stones in the garden. The primary example is container registries: when a Docker registry is planted on Stone A, all other Stones should configure their local Docker daemon to trust that registry as an insecure source.

This creates several design challenges:
1. **Coupling concern**: Putting behavioral instructions in offering manifests (frontmatter) violates separation of concerns. Manifests should be declarative (what the offering IS), not behavioral (what Moss should DO).
2. **Distributed coordination**: Each Stone must independently react to topology changes - no central coordinator.
3. **Local-only actions**: Each Moss instance should only modify its own local infrastructure (Docker daemon, DNS settings, etc.) - never reach into other Stones.

## Decision
Implement **Infrastructure Handlers** as self-contained modules within Moss that:

1. **Know what they match**: Each handler defines its own matching logic (by offering name, category, tags, or any combination).
2. **Know what to do**: Each handler contains the complete logic for managing local infrastructure when matching offerings appear/disappear.
3. **React to topology changes**: Handlers are triggered after each topology update (chirp received).
4. **Affect only local infrastructure**: Handlers never contact other Stones - they only modify the local system.

### Handler Trait
```rust
#[async_trait]
pub trait InfrastructureHandler: Send + Sync {
    /// Handler identifier for logging/debugging
    fn name(&self) -> &'static str;

    /// Does this offering trigger this handler?
    fn matches(&self, offering: &str, category: &str, tags: &[String]) -> bool;

    /// Sync local infrastructure with current garden state
    /// Called with ALL matching offerings across the garden (not just changes)
    async fn sync(&self, instances: &[OfferingInstance]) -> Result<()>;
}
```

### First Implementation: Docker Registry Handler
- **Matches**: Offerings named "registry", "zot", "harbor", or any offering in category "devops" with tag "container-registry"
- **Action**: Updates `/etc/docker/daemon.json` (Linux) or `%PROGRAMDATA%\docker\config\daemon.json` (Windows) with `insecure-registries` list
- **Restart**: Silently restarts Docker daemon only if the registry list actually changed

### Distributed Pattern
```
Stone A plants registry
    │
    ├─── chirp ───────────────────┬───────────────────────┤
    │                             │                       │
    ▼                             ▼                       ▼
Stone A topology updated    Stone B topology updated    Stone C topology updated
    │                             │                       │
    ▼                             ▼                       ▼
Handler syncs LOCAL         Handler syncs LOCAL         Handler syncs LOCAL
daemon.json                 daemon.json                 daemon.json
```

## Rationale
- **SoC compliance**: Offering manifests remain purely declarative. Behavioral logic lives in Moss domain layer where it belongs.
- **Self-contained handlers**: Each handler is a complete unit - no scattered configuration across manifests and Moss code.
- **Testable**: Handlers can be unit tested independently of topology and Docker.
- **Extensible**: New handlers can be added without modifying manifests (DNS upstream, NTP sources, proxy configuration, etc.).
- **Autonomous Stones**: Aligns with Zen Garden philosophy - Stones are independent, react to garden state, manage themselves.

## Consequences

### Positive
- Clean separation between offering metadata (manifests) and Moss behavior (handlers)
- Each Stone remains autonomous and self-managing
- Easy to add new garden-wide effect types
- Handlers are self-documenting (matching logic + action in one place)

### Negative
- Matching logic is hardcoded in Moss (must update Moss to add new registry types)
- Requires Docker daemon restart for registry changes (brief service interruption)
- Linux root/Windows admin required for daemon.json modification

### Neutral
- Handler registry is initialized at Moss startup (not dynamically loaded)
- Failed handlers don't block topology updates (best-effort pattern)

## Alternatives Considered

### 1. Garden Effects in Frontmatter
```json
{
  "name": "registry",
  "garden_effects": [{ "type": "docker-insecure-registry", "port": 5000 }]
}
```
**Rejected**: Violates SoC - offerings shouldn't know about Moss's Docker management.

### 2. Central Coordinator Stone
One Stone designated to push configuration to all others.
**Rejected**: Violates Zen Garden's distributed, autonomous Stone model. Creates single point of failure.

### 3. No Automation (Documentation Only)
Document manual steps for configuring Docker to trust garden registries.
**Rejected**: Poor user experience, error-prone, doesn't scale.

## Implementation Notes
- Handlers are registered in `InfrastructureHandlerRegistry` during Moss bootstrap
- Hook point: After `upsert_from_chirp()` in coordinator's discovery listener
- Platform-specific daemon.json paths handled in `infra/docker_config.rs`
- Silent restart: Only restart Docker if insecure-registries list actually changed

### Systemd Service Requirements
The Moss systemd service requires specific configuration for infrastructure handlers to work:

1. **ReadWritePaths**: Must include `/etc/docker` because `ProtectSystem=strict` makes the filesystem read-only by default:
   ```
   ReadWritePaths=/etc/zen-garden /var/lib/zen-garden ... /etc/docker
   ```

2. **Docker dependency**: Must use `Wants=docker.service` (not `Requires=docker.service`) to prevent Moss from being stopped when the handler restarts Docker:
   ```
   Wants=docker.service  # Soft dependency - Moss survives Docker restart
   ```
   With `Requires=`, systemd stops Moss when Docker restarts, causing a cascade failure.

## References
- [Docker daemon.json configuration](https://docs.docker.com/engine/reference/commandline/dockerd/)
- Zen Garden philosophy: Autonomous Stones, discovery over configuration
