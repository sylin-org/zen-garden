# The Custom Offering

*The built-in templates don't fit. You write your own.*

---

## The Story

Your friend wrote an application—a home inventory system. It's a Docker image they published: `ghcr.io/yourfriend/home-inventory:1.0.0`. They want to deploy it in their garden.

There's no built-in offering for it. They need to create a custom template.

---

### Starting Simple

They begin by checking what a minimal offering needs:

```bash
garden-rake scaffold offering home-inventory
```

```
Creating offering scaffold: home-inventory

  Created: ~/.zen-garden/offerings/home-inventory/

  Files:
    manifest.yaml     Offering definition
    README.md         Documentation template

  Next steps:
    1. Edit manifest.yaml with your container configuration
    2. Test with: garden-rake grow home-inventory --dry-run
    3. Deploy with: garden-rake grow home-inventory
```

The garden created a skeleton. Time to fill it in.

---

### The Manifest

They open `manifest.yaml`:

```yaml
# ~/.zen-garden/offerings/home-inventory/manifest.yaml

offering:
  name: home-inventory
  version: 1.0.0
  description: "Home inventory tracking system"

container:
  image: ghcr.io/yourfriend/home-inventory:1.0.0
  ports:
    - host: 8080
      container: 8080
      protocol: tcp
  environment:
    - name: DATABASE_PATH
      value: /data/inventory.db
  volumes:
    - name: data
      path: /data

health:
  check:
    type: http
    path: /health
    port: 8080
  interval: 30s
  timeout: 10s
  retries: 3

ceremony:
  mode: quiesceable
  quiesce:
    command: ["curl", "-X", "POST", "localhost:8080/api/quiesce"]
  resume:
    command: ["curl", "-X", "POST", "localhost:8080/api/resume"]

discovery:
  service_type: home-inventory
  announce: true
```

This tells the garden:
- What image to run
- What ports to expose
- What volumes to persist
- How to check health
- How to safely snapshot for updates

---

### Testing the Manifest

Before deploying, they validate:

```bash
garden-rake grow home-inventory --dry-run
```

```
Validating offering: home-inventory

  Manifest: ~/.zen-garden/offerings/home-inventory/manifest.yaml

  Container:
    Image: ghcr.io/yourfriend/home-inventory:1.0.0
    Ports: 8080/tcp
    Volumes: data → /data
    Environment: DATABASE_PATH=/data/inventory.db

  Health Check:
    Type: HTTP GET /health:8080
    Interval: 30s, Timeout: 10s, Retries: 3

  Ceremony:
    Mode: quiesceable
    Quiesce: curl -X POST localhost:8080/api/quiesce
    Resume: curl -X POST localhost:8080/api/resume

  Discovery:
    Service Type: home-inventory
    Will be announced to garden

  Validation:
    ✓ Manifest syntax valid
    ✓ Image reference valid
    ✓ Ports available (8080 not in use)
    ✓ Volume paths valid
    ⚠ Image not cached (will pull on deploy)

No changes made (dry run).
```

Everything looks good. The image isn't cached yet, but that's expected.

---

### Deploying

```bash
garden-rake grow home-inventory
```

```
Growing home-inventory on stone-amber-ridge...

  Pulling image ghcr.io/yourfriend/home-inventory:1.0.0...
    1.0.0: Pulling from yourfriend/home-inventory
    a1b2c3d4: Pull complete
    e5f6g7h8: Pull complete
    Digest: sha256:abc123...
    Status: Downloaded newer image
    ✓ Image pulled (245 MB)

  Creating volumes...
    data → /var/lib/zen-garden/offerings/home-inventory/data
    ✓ Volumes created

  Creating container...
    Name: zen-offering-home-inventory
    ✓ Container created

  Starting container...
    ✓ Container started

  Waiting for health check...
    HTTP GET http://localhost:8080/health
    Attempt 1/3... passed (200 OK, 45ms)
    ✓ Health check passed

  Announcing to garden...
    ✓ Registered as home-inventory on stone-amber-ridge

✓ home-inventory is ready

  Access: http://stone-amber-ridge.local:8080
  Logs: garden-rake logs home-inventory
```

The custom offering is running.

---

### Adding Resources Constraints

After running for a week, they notice it's using more memory than expected. They add resource limits:

```yaml
# manifest.yaml (updated)

container:
  image: ghcr.io/yourfriend/home-inventory:1.0.0
  ports:
    - host: 8080
      container: 8080
  resources:
    memory:
      limit: 512Mi
      request: 256Mi
    cpu:
      limit: "1.0"
      request: "0.25"
```

Apply the change:

```bash
garden-rake refresh home-inventory
```

```
Refreshing home-inventory...

  Changes detected:
    + resources.memory.limit: 512Mi
    + resources.memory.request: 256Mi
    + resources.cpu.limit: 1.0
    + resources.cpu.request: 0.25

  Applying changes...
    Updating container configuration... done
    Restarting container... done
    Verifying health... passed

✓ home-inventory refreshed
```

---

### Adding Dependencies

The inventory system needs a database. Rather than bundle it, they add Redis as a dependency:

```yaml
# manifest.yaml (updated)

offering:
  name: home-inventory
  version: 1.1.0
  description: "Home inventory tracking system"

  dependencies:
    - offering: redis
      required: true
      inject_connection: true

container:
  image: ghcr.io/yourfriend/home-inventory:1.1.0
  ports:
    - host: 8080
      container: 8080
  environment:
    - name: REDIS_URL
      from: dependency.redis.connection_string
```

The garden will:
1. Ensure Redis is running before starting home-inventory
2. Automatically inject Redis's connection string into the environment
3. Restart home-inventory if Redis moves to a different Stone

```bash
garden-rake refresh home-inventory
```

```
Refreshing home-inventory...

  Dependency check:
    redis: ✓ Running on stone-coral-reef (192.168.1.58:6379)

  Environment injection:
    REDIS_URL = redis://192.168.1.58:6379

  Applying changes...
    Updating environment... done
    Restarting container... done
    Verifying health... passed

✓ home-inventory refreshed with Redis dependency
```

---

### Version Pinning and Updates

Months pass. Version 2.0.0 is released. They want controlled updates:

```yaml
# manifest.yaml

container:
  image: ghcr.io/yourfriend/home-inventory
  tag_policy:
    strategy: semver
    constraint: ">=1.0.0 <3.0.0"
    auto_update: minor  # Automatically apply minor updates, prompt for major
```

Now when they run nourishment:

```bash
garden-rake nourish
```

```
📦 Garden-wide Update Status

Summary: 2 available

  stone-amber-ridge
    AVAILABLE:
      • home-inventory 1.1.0 → 2.0.0 (major version - manual approval required)
      • redis 7.2.7 → 7.2.8 (will auto-apply)

  Apply redis update automatically? [Y/n] y

  Nourishing redis...
    [standard ceremony]
  ✓ redis updated

  home-inventory 2.0.0 is a major version. Review changes before updating:
    Changelog: https://github.com/yourfriend/home-inventory/releases/tag/v2.0.0
    Update command: garden-rake nourish home-inventory
```

Minor updates happen automatically. Major versions require explicit approval.

---

### Sharing the Offering

The template works great. They want to share it with others:

```bash
garden-rake publish offering home-inventory
```

```
Publishing home-inventory to Zen Garden Registry...

  Validating manifest... passed
  Packaging offering... done

  Choose visibility:
    [1] Public (anyone can use)
    [2] Private (only you)
    [3] Shared (specific users)

  Selection: 1

  Publishing...
    Uploading manifest... done
    Creating registry entry... done

✓ home-inventory published

  Others can now install with:
    garden-rake install offering home-inventory
```

Now anyone can use their custom offering.

---

## What Just Happened

### The Offering Manifest Structure

A complete offering manifest has these sections:

```yaml
# Full manifest structure

offering:
  name: string              # Unique identifier
  version: string           # SemVer version
  description: string       # Human-readable description
  author: string            # Optional author info
  license: string           # Optional license
  homepage: string          # Optional project URL

  dependencies:             # Other offerings this requires
    - offering: string      # Dependency name
      required: bool        # Must be running?
      inject_connection: bool  # Auto-inject connection info?

container:
  image: string             # Docker image reference
  tag_policy:               # How to handle versions
    strategy: semver|latest|pinned
    constraint: string      # SemVer constraint
    auto_update: none|patch|minor|all

  ports:                    # Port mappings
    - host: int
      container: int
      protocol: tcp|udp

  environment:              # Environment variables
    - name: string
      value: string         # Static value
      from: string          # Or dynamic reference

  volumes:                  # Persistent storage
    - name: string
      path: string          # Mount path in container

  resources:                # Resource constraints
    memory:
      limit: string         # e.g., "512Mi"
      request: string
    cpu:
      limit: string         # e.g., "1.0"
      request: string

health:
  check:
    type: http|tcp|exec
    # For HTTP:
    path: string
    port: int
    # For TCP:
    port: int
    # For exec:
    command: [string]
  interval: duration
  timeout: duration
  retries: int
  start_period: duration    # Grace period after start

ceremony:
  mode: stateless|quiesceable|unsafe
  # For quiesceable:
  quiesce:
    command: [string]       # Run before snapshot
  resume:
    command: [string]       # Run after snapshot
  # For unsafe:
  warning: string           # Shown before updates

discovery:
  service_type: string      # mDNS service type
  announce: bool            # Broadcast to garden?
  metadata:                 # Additional discovery info
    key: value

backup:
  strategy: full|incremental
  schedule: cron            # Optional override
  retention: duration       # How long to keep
  pre_hook: [string]        # Run before backup
  post_hook: [string]       # Run after backup
```

### Ceremony Modes Explained

| Mode | Use Case | Behavior |
|------|----------|----------|
| `stateless` | Web servers, stateless APIs | Snapshot anytime, no coordination needed |
| `quiesceable` | Databases, stateful apps | Pause writes, snapshot, resume |
| `unsafe` | Unknown apps, legacy systems | Stop completely before snapshot |

For `quiesceable` offerings, the quiesce/resume commands let the app prepare for a consistent snapshot:

```
Normal operation
     │
     ▼
[Quiesce command]
     │ Flushes buffers
     │ Pauses writes
     │ Returns success
     ▼
[Snapshot taken]
     │
     ▼
[Resume command]
     │ Enables writes
     │ Returns success
     ▼
Normal operation
```

### Dynamic Environment Variables

The `from:` syntax allows injecting runtime values:

```yaml
environment:
  # Static value
  - name: APP_NAME
    value: "My App"

  # From dependency
  - name: REDIS_URL
    from: dependency.redis.connection_string

  # From stone info
  - name: STONE_NAME
    from: stone.name

  # From offering info
  - name: DATA_DIR
    from: offering.volumes.data.path
```

Available dynamic references:
- `dependency.<name>.connection_string` — Dependency's connection URL
- `dependency.<name>.host` — Dependency's host
- `dependency.<name>.port` — Dependency's port
- `stone.name` — Current Stone's name
- `stone.ip` — Current Stone's IP
- `offering.volumes.<name>.path` — Volume mount path
- `pond.id` — Pond identifier (if in a pond)

### Local vs Registry Offerings

```
Offering Sources:

~/.zen-garden/offerings/        ← Local offerings (highest priority)
├── home-inventory/
│   └── manifest.yaml
└── my-other-app/
    └── manifest.yaml

Zen Garden Registry             ← Community offerings
├── mongodb
├── redis
├── postgres
└── home-inventory (published)

Built-in                        ← Shipped with Zen Garden
├── mongodb
├── redis
├── postgres
└── ...
```

Local offerings override registry versions. This lets you customize built-in offerings or test changes before publishing.

### Scaffolding Templates

The scaffold command offers templates for common patterns:

```bash
# Basic web service
garden-rake scaffold offering myapp --template web

# Database with persistence
garden-rake scaffold offering mydb --template database

# Background worker
garden-rake scaffold offering myworker --template worker

# Blank template
garden-rake scaffold offering custom
```

Each template pre-fills common patterns:
- **web**: HTTP health check, port 8080, stateless ceremony
- **database**: TCP health check, quiesceable ceremony, volume for data
- **worker**: Exec health check, no ports, stateless ceremony

---

## Commands From This Journey

```bash
# Create offering scaffold
garden-rake scaffold offering myapp
garden-rake scaffold offering myapp --template web

# Validate offering manifest
garden-rake grow myapp --dry-run

# Deploy custom offering
garden-rake grow myapp

# Refresh after manifest changes
garden-rake refresh myapp

# Check offering status
garden-rake status myapp

# View offering manifest
garden-rake show offering myapp

# Publish to registry
garden-rake publish offering myapp

# Install from registry
garden-rake install offering some-offering

# List local offerings
garden-rake list offerings --local

# List registry offerings
garden-rake search offerings

# Remove custom offering
garden-rake rest myapp
garden-rake forget offering myapp  # Also delete local files
```

---

*Zen Garden Documentation — Journeys*
