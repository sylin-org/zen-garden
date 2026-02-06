# OFFER-0003: Offering Fully-Qualified Names (FQN)

**Status**: Accepted  
**Date**: 2026-02-06  
**Deciders**: Engineering  

---

## Context

Zen Garden historically used a single string as both:

- the **offering template type** (e.g., `ollama`)
- the **service instance name** (e.g., `ollama` running on a stone)

This prevents multiple instances of the same offering (e.g., `ollama` and `ollama:dev`) and creates ambiguity in APIs, jobs, and container naming. We also need a stable, portable identity that:

- allows multiple instances on the same or different stones
- keeps compatibility/manifests tied to the offering type
- keeps Rake as a thin client while Moss orchestrates

---

## Decision

We will use **Fully-Qualified Names (FQN)** for service instances:

```
offering[:instance]
```

Rules:

- `offering` = offering template type (manifest key)
- `instance` = optional instance name
- default instance omits `:instance` (canonical `ollama`, not `ollama:ollama`)
- segments are lowercase, `[a-z0-9_-]`, must start with a letter
- `--` is reserved for container encoding

**Registry & API**:

- `Offering.name` → FQN (instance identity)
- `Offering.offering` → template type
- `:name` path parameters accept FQN (URL-encoded)
- Manifest lookup uses **offering type** even when FQN provided

**Container naming**:

- default instance: `zen-offering-{offering}`
- named instance: `zen-offering-{offering}--{instance}`

**Adopted offerings**:

- adopted instances are normalized to `:{adopted}` (e.g., `ollama:adopted`)

---

## Rationale

- **Multi-instance support** without changing the manifest model
- **Clear separation** between template type and instance identity
- **Stable addressing** across APIs, jobs, and events
- **Container safety** (colon not used in container names)

---

## Consequences

### Positive
- Multiple instances per offering are supported (e.g., `ollama`, `ollama:dev`)
- API and CLI target instances unambiguously
- Container namespace remains deterministic and collision-resistant

### Negative
- Path params with `:` must be URL-encoded (`ollama%3Adev`)
- Additional validation rules needed for instance names

### Neutral
- Default instance remains backward compatible (`ollama`)
- Existing container prefix remains `zen-offering-`

---

## Implementation

Key changes:

- New FQN parser/validator in `garden_common::offerings`
- Container name encoding uses `--` for instance separator
- API handlers normalize FQN at ingress
- Rake URL-encodes offering names in path params
- Capabilities mirror endpoint added (Moss orchestrates)

---

## References

- [OFFER-0002](OFFER-0002-container-namespace-collision.md)
- [Offerings Spec](../specs/offerings.md)
- [Offering FQN Spec](../specs/offering-fqn.md)
