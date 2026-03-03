---
globs: src/moss/src/infra/docker*.rs, src/moss/src/domain/offerings/**/*.rs
alwaysApply: false
---
# Docker & Container Operations

## Container Naming Convention (CRITICAL)
Managed offerings MUST use `zen-offering-{encoded}` container naming.
Use `OfferingFqn::encoded_for_container()` to derive the container-safe suffix.

```
ollama           → zen-offering-ollama
ollama::dev      → zen-offering-ollama--dev
image:nginx:latest → zen-offering-img-nginx-latest
```

## Rules
- ❌ NEVER adopt containers with other names (e.g., `my-mongo`, `user-redis`)
- ❌ NEVER adopt native services as managed containers
- ✅ ALWAYS check for `zen-offering-{name}` before deploying
- ✅ ALWAYS deploy new managed offerings as `zen-offering-{name}`
- ✅ ALWAYS adopt orphaned `zen-offering-*` containers (self-heal)

## Pattern
```rust
// Before deploying managed offering
let container_name = format!("zen-offering-{}", offering);

if state.docker.container_exists(&container_name).await? {
    // Adopt existing container instead of deploying new one
    adopt_offering_container(&state.docker, &state.manifests, offering).await?;
    return Ok(());
}
```

## Reference
Decision: `docs/decisions/OFFER-0002-container-namespace-collision.md`
