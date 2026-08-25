# Offering Fully-Qualified Names (FQN) Specification

**Purpose:** Define how offering instances are named and addressed across the system.
**Audience:** Developers implementing Moss/Rake, operators naming instances.

---

## Overview

Zen Garden separates **offering type** (template) from **instance identity** using a fully-qualified name (FQN).

- **Offering type**: the template/manifest key (e.g., `ollama`)
- **Instance**: optional name to distinguish multiple instances (e.g., `dev`)
- **Source**: optional prefix indicating deployment source (e.g., `image:`)
- **FQN**: `[source:]offering[::instance]`

The FQN is used wherever a specific running instance is referenced (API path params, registry entries, jobs, container names).

---

## Format

```
[source:]offering[::instance]
```

### Curated Offerings

```
ollama               # default instance
ollama::dev          # named instance
postgres::staging    # named instance
mongodb::adopted     # adopted (native) offering
```

### Image-Direct Offerings

```
image:nginx:latest             # default instance, image ref "nginx:latest"
image:nginx:latest::staging    # named instance "staging"
image:ghcr.io/org/app:v2::prod  # registry path with instance
```

### Source Scheme Prefixes

| Prefix | Meaning |
|--------|---------|
| *(none)* | Curated offering from built-in manifest catalog |
| `image:` | Deploy directly from a container image reference |
| `repo:` | Community repository offering (future) |
| `oci:` | OCI artifact reference (future) |

---

## Validation Rules

Each segment (offering and instance) must:

- be lowercase after normalization
- contain only `[a-z0-9_-]`
- start with a letter
- be at most 128 characters
- **not** include the reserved container separator `--`

The `::` separator is used between offering and instance. Only one `::` separator is allowed.

Image refs (after `image:` prefix) follow Docker image reference rules and are not validated against the segment rules above.

---

## Canonicalization

The canonical FQN is:

- trimmed
- lowercased
- uses `::` as the instance separator

Legacy formats are auto-normalized on parse:

```
ollama@adopted   →  ollama::adopted    (V0 @ separator)
ollama:dev       →  ollama::dev        (V1 : separator)
ollama::dev      →  ollama::dev        (V2, canonical)
```

---

## Type: `OfferingFqn`

The FQN is represented as a proper type (`garden_common::offerings::OfferingFqn`) throughout the codebase.

### Fields

| Field | Type | Description |
|-------|------|-------------|
| `source` | `Option<OfferingSource>` | Deployment source (Image, Repo, Oci) |
| `offering` | `String` | Base offering name |
| `instance` | `Option<String>` | Instance name (None = default) |
| `image_ref` | `Option<String>` | Container image reference (image-direct only) |

### Constructors

```rust
OfferingFqn::new("ollama")                              // curated, no instance
OfferingFqn::with_instance("ollama", "dev")              // curated, named instance
OfferingFqn::adopted("ollama")                           // curated, instance = "adopted"
OfferingFqn::image_direct("nginx:latest")                // image-direct, no instance
OfferingFqn::image_direct_with_instance("nginx:latest", "staging")  // image-direct, named
OfferingFqn::parse("ollama::dev")                        // parse any format (handles legacy)
```

### Serialization

Serializes as a plain string in JSON. Deserializes via `parse()`, which auto-normalizes legacy formats. This means persistence load and chirp receive auto-normalize with zero extra code.

```json
"ollama::dev"
"image:nginx:latest::staging"
```

---

## Registry & API Semantics

### Registry

- `Offering.name` stores the **FQN** as `OfferingFqn` (instance identity)
- `Offering.offering` stores the **offering type** as `String` (template key)

### API

Path parameters that refer to a running instance accept FQN:

- `GET /api/v1/stone/offerings/{name}`
- `GET /api/v1/stone/offerings/{name}/capabilities`
- `POST /api/v1/stone/offerings/{name}/capabilities/mirror`

Manifest lookups **always use offering type**, even if FQN is supplied.

**Note:** `::` must be URL-encoded in path segments.

Example:

```
/api/v1/stone/offerings/ollama%3A%3Adev/capabilities
```

---

## Container Encoding

Container names must be safe for Docker and avoid `:`.
FQNs are encoded using `--` between offering and instance, with an `img-` prefix for image-direct.

```
FQN                               Container Name
ollama                          → zen-offering-ollama
ollama::dev                     → zen-offering-ollama--dev
image:nginx:latest              → zen-offering-img-nginx-latest
image:nginx:latest::staging     → zen-offering-img-nginx-latest--staging
```

Use `OfferingFqn::encoded_for_container()` to generate container-safe names.

---

## Adopted Offerings

Adopted (native) offerings use a reserved instance name:

```
ollama::adopted
```

This avoids collisions with managed instances and preserves a consistent identity for detection.
The adoption APIs accept offering type or FQN but normalize to `::adopted`.

Use `OfferingFqn::adopted("ollama")` to construct adopted FQNs.

---

## CLI Usage

Rake accepts FQNs anywhere a service name is expected:

```bash
# Default instance
garden-rake offer ollama

# Named instance
garden-rake offer ollama::dev

# Capabilities on a specific instance
garden-rake capabilities ollama::dev
```

Rake URL-encodes path segments automatically.

---

## Capabilities Mirroring

Capabilities mirroring targets a specific instance (FQN) on both stones:

```bash
garden-rake capabilities ollama mirror from stone-01 to stone-02
garden-rake capabilities ollama::dev mirror from stone-01 to stone-02
```

---

## References

- [OFFER-0003](../decisions/OFFER-0003-offering-fqn.md) (superseded)
- [OFFER-0006](../decisions/OFFER-0006-image-direct-and-fqn-v2.md)
- [Offerings Spec](offerings.md)
