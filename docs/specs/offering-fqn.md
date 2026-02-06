# Offering Fully-Qualified Names (FQN) Specification

**Purpose:** Define how offering instances are named and addressed across the system.  
**Audience:** Developers implementing Moss/Rake, operators naming instances.

---

## Overview

Zen Garden separates **offering type** (template) from **instance identity** using a fully-qualified name (FQN).

- **Offering type**: the template/manifest key (e.g., `ollama`)
- **Instance**: optional name to distinguish multiple instances (e.g., `dev`)
- **FQN**: `offering[:instance]`

The FQN is used wherever a specific running instance is referenced (API path params, registry entries, jobs, container names).

---

## Format

```
offering[:instance]
```

Examples:

- `ollama` (default instance)
- `ollama:dev` (named instance)
- `postgres:staging`

---

## Validation Rules

Each segment (offering and instance) must:

- be lowercase after normalization
- contain only `[a-z0-9_-]`
- start with a letter
- be at most 128 characters
- **not** include the reserved container separator `--`

Only one `:` separator is allowed.

---

## Canonicalization

The canonical FQN is:

- trimmed
- lowercased
- uses a single `:` separator

If the instance equals the offering name, it is treated as the **default instance**:

```
ollama:ollama  →  ollama
```

---

## Registry & API Semantics

### Registry

- `Offering.name` stores the **FQN** (instance identity)
- `Offering.offering` stores the **offering type** (template key)

### API

Path parameters that refer to a running instance accept FQN:

- `GET /api/v1/stone/offerings/{name}`
- `GET /api/v1/stone/offerings/{name}/capabilities`
- `POST /api/v1/stone/offerings/{name}/capabilities/mirror`

Manifest lookups **always use offering type**, even if FQN is supplied.

**Note:** `:` must be URL-encoded in path segments.

Example:

```
/api/v1/stone/offerings/ollama%3Adev/capabilities
```

---

## Container Encoding

Container names must be safe for Docker and avoid `:`.  
FQNs are encoded using `--` between offering and instance.

```
FQN           → Container Name
ollama        → zen-offering-ollama
ollama:dev    → zen-offering-ollama--dev
```

---

## Implementation Requirements

- **Ingress normalization**: All API handlers that accept offering names must parse and normalize FQN.
- **Manifest lookup**: Always use `offering` (type) for templates and compatibility.
- **Service identity**: Use FQN in registry, jobs, events, and CLI output.
- **Container names**: Encode FQN using `zen-offering-{offering}--{instance}`.
- **Adoption**: Adopted offerings must use `:{adopted}` instance for stable identity.
- **URL encoding**: CLI and SDKs must URL-encode FQNs in path segments.

---

## Adopted Offerings

Adopted (native) offerings use a reserved instance name:

```
ollama:adopted
```

This avoids collisions with managed instances and preserves a consistent identity for detection.  
The adoption APIs accept offering type or FQN but normalize to `:{adopted}`.

---

## CLI Usage

Rake accepts FQNs anywhere a service name is expected:

```bash
# Default instance
garden-rake offer ollama

# Named instance
garden-rake offer ollama:dev

# Capabilities on a specific instance
garden-rake capabilities ollama:dev
```

Rake URL-encodes path segments automatically.

---

## Capabilities Mirroring

Capabilities mirroring targets a specific instance (FQN) on both stones:

```bash
garden-rake capabilities ollama mirror from stone-01 to stone-02
garden-rake capabilities ollama:dev mirror from stone-01 to stone-02
```

The tended Moss performs orchestration by:

1. Fetching capabilities from source
2. Fetching capabilities from destination
3. Adding missing capabilities to destination

---

## References

- [OFFER-0003](../decisions/OFFER-0003-offering-fqn.md)
- [Offerings Spec](offerings.md)
- [API v1 Spec](api-v1.md)
- [Rake Commands](rake-commands.md)
