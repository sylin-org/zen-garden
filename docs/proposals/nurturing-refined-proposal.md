# Zen Garden Nurturing System - Refined Proposal

**Status**: Proposal (Refined from cultivation spec)
**Date**: January 2026
**Based on**: Discussion session refining backup/recovery design

---

## Executive Summary

This proposal refines the original cultivation specification with pragmatic simplifications:

1. **Tiered backup architecture**: A/B rotation on Stones, N-slot retention in seed banks
2. **Policy-driven retention**: Seed bank-level defaults with per-offering overrides
3. **Role-aware backup**: Only primary/standalone offerings sync to seed bank
4. **Visibility model**: Open/shaded for service discoverability
5. **Wishful provisioning**: Declarative "make it available" semantics

---

## Table of Contents

1. [Design Principles](#design-principles)
2. [Tiered Backup Architecture](#tiered-backup-architecture)
3. [Seed Bank Policy](#seed-bank-policy)
4. [Role Detection](#role-detection)
5. [Visibility Model](#visibility-model)
6. [Wishful Semantics](#wishful-semantics)
7. [Manifest Extensions](#manifest-extensions)
8. [Recovery UX](#recovery-ux)
9. [Implementation Phases](#implementation-phases)

---

## Design Principles

### Managed Offerings Only

Only offerings with manifests that declare volume mappings can participate in nurturing. Without knowing where data lives, backup is guesswork.

```yaml
# mongodb.snippet.yaml - volume declared, can be nurtured
volumes:
  - mongo-data:/data/db
```

Offerings without volume declarations: no nurturing capability.

### Local-First, Seed Bank Optional

A garden with no seed bank still works:
- A/B snapshots on local staging
- User can recover from yesterday or day-before
- Seed bank adds offsite insurance, not a requirement

### Simple Rotation Over Complex Retention

The original proposal's retention policies (7 daily + 4 weekly + 6 monthly = 17 slots) add complexity most self-hosters don't need.

Refined approach:
- **Local**: Fixed A/B (2 slots, always)
- **Seed bank**: Configurable slots (default 5)

### Same Name = Tandem

Multiple offerings with the same fully-qualified name are working in tandem:
- Same name, different `offering_id`
- Role (primary/replica) determined by service-specific logic
- Only primary syncs to seed bank; replicas keep local A/B only

---

## Tiered Backup Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         STONE (Local)                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   staging/{offering_id}/                                        │
│   ├── A.archive.gz   ← Today (or most recent)                   │
│   └── B.archive.gz   ← Yesterday (or previous)                  │
│                                                                 │
│   Purpose: Fast local recovery                                  │
│   Rotation: A ↔ B flip daily                                    │
│   Configuration: None (always 2 slots)                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ Upload A to seed bank
                              │ (if primary/standalone)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                         SEED BANK                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   offerings/{offering_id}/                                      │
│   ├── 1.archive.gz   ← Most recent                              │
│   ├── 1.meta.yaml                                               │
│   ├── 2.archive.gz                                              │
│   ├── 2.meta.yaml                                               │
│   ├── ...                                                       │
│   └── N.archive.gz   ← Oldest (N = configured slots)            │
│                                                                 │
│   Purpose: Disaster recovery, offsite backup                    │
│   Rotation: Configurable slots (default 5)                      │
│   Configuration: Policy at seed bank level + overrides          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Why A/B Locally?

- **Predictable**: Always know exactly what you have
- **Space-bounded**: Maximum 2× data size
- **Fast recovery**: No network needed for recent restore
- **Simple**: No configuration, no retention policies

### Why N-Slot in Seed Bank?

- **Deeper history**: 5 days coverage by default
- **Policy-driven**: Different offerings can have different retention
- **Disaster recovery**: Survives Stone death

---

## Seed Bank Policy

### Policy File Location

```
seed-bank/
├── seed-bank.yaml       ← Policy lives here
├── offerings/
│   └── ...
└── index.yaml
```

Policy travels with the seed bank. USB moves, policy moves.

### Policy Structure

```yaml
# seed-bank.yaml
garden_id: jade-mountain-a1b2c3d4
garden_name: jade-mountain

policy:
  default_slots: 5        # Garden-wide default

  # Override by offering type
  offerings:
    mongodb:
      slots: 7            # Databases get more history
    postgresql:
      slots: 7
    redis:
      slots: 3            # Cache, less critical
    grafana:
      slots: 2            # Config-only, minimal history

  # Override by fully-qualified name (highest priority)
  named:
    "mongodb:analytics:prod":
      slots: 10           # Critical production DB
    "mongodb:analytics:dev":
      slots: 2            # Dev environment
```

### Resolution Order

1. **Exact name match** (`mongodb:analytics:prod`) → 10 slots
2. **Offering type match** (`mongodb`) → 7 slots
3. **Default** → 5 slots

### Slot Rotation Logic

```
Given: slots = 5, existing = [1, 2, 3, 4, 5]

On new upload:
  1. Delete slot 5 (oldest)
  2. Shift: 4→5, 3→4, 2→3, 1→2
  3. New archive → slot 1

After: [new, 1, 2, 3, 4]
```

---

## Role Detection

### The Problem

In tandem configurations (replicas), only the primary should sync to seed bank. Otherwise you get duplicate backups.

### Primary Definition

**Primary = the instance that `garden-rake find` returns = the one clients connect to = the one with authoritative data.**

For different services, "primary" means different things:

| Service | Primary Is... | Detection Method |
|---------|---------------|------------------|
| MongoDB replica set | Write primary | `rs.isMaster().ismaster` |
| PostgreSQL streaming | Leader | `pg_is_in_recovery() = false` |
| Redis Sentinel | Master | Sentinel protocol |
| Singleton (Grafana) | The only one | N/A (standalone) |

### Manifest Extension

```yaml
# mongodb.snippet.yaml
healthcheck:
  test: ["CMD", "mongosh", "--eval", "db.adminCommand('ping')"]
  interval: 10s
  timeout: 5s
  retries: 5

role_detection:
  test: ["CMD", "mongosh", "--quiet", "--eval", "rs.isMaster().ismaster"]
  primary_when: "true"
  interval: 30s
```

### Role Values

| Role | Sync to Seed Bank | Local A/B |
|------|-------------------|-----------|
| `primary` | Yes | Yes |
| `replica` | No | Yes |
| `standalone` | Yes | Yes |

If no `role_detection` in manifest → assume `standalone`.

### Event-Driven Updates

Role detection should be as event-driven as possible:
- Service announces role change via SSE event
- Moss updates cached role
- Backup decision uses current role

Periodic polling (interval in manifest) as fallback.

---

## Visibility Model

### Concept

Visibility controls whether a service appears in `find` results for partial matches.

| Visibility | Behavior |
|------------|----------|
| **open** | Appears in find results for partial matches |
| **shaded** | Only findable by exact full name |

### Naming Rationale

"Shaded" fits the garden metaphor — a shaded offering is still there, still thriving, just not in the main path. You must know where to look.

### Find Behavior

```bash
# Garden state:
#   mongodb (open)
#   mongodb:analytics:dev (open)
#   mongodb:analytics:prod (shaded)

$ garden-rake find mongodb
Found:
  [1] mongodb @ stone-01
  [2] mongodb:analytics:dev @ stone-02
# Note: prod not shown (shaded, partial match)

$ garden-rake find mongodb:analytics
Found:
  [1] mongodb:analytics:dev @ stone-02
# Note: prod still not shown (shaded, namespace match isn't exact)

$ garden-rake find mongodb:analytics:prod
Found:
  [1] mongodb:analytics:prod @ stone-03
# Exact match bypasses visibility
```

### CLI Commands

```bash
# Mark as shaded
garden-rake shade mongodb:analytics:prod

# Mark as open
garden-rake unshade mongodb:analytics:prod

# Plant as shaded
garden-rake offer mongodb as analytics:prod shaded
```

### Backup Metadata

Visibility is preserved in backup metadata for recovery:

```yaml
# 1.meta.yaml
offering_type: mongodb
namespace: analytics
instance: prod
visibility: shaded        # Restored as shaded
```

---

## Wishful Semantics

### Core Concept

`wishfully` is declarative service provisioning:

> "I want this service to exist and be available. Make it so."

### Behavior Matrix

| Query | Running | Stopped | Doesn't Exist |
|-------|---------|---------|---------------|
| `find mongodb` | Return | Not found | Not found |
| `find mongodb wishfully` | Return | Wake → Return | Plant → Return |
| `find mongodb:analytics:prod` | Return | **Auto-wake** → Return | Not found |
| `find mongodb:analytics:prod wishfully` | Return | Wake → Return | Plant → Return |

### Auto-Wake on Exact Match

Exact match (full name) automatically wakes stopped offerings:

**Rationale**: If you know the exact full name of a private offering, you clearly intend to use it. Auto-wake is ergonomic.

`wishfully` for exact match specifically means "plant if doesn't exist."

### CLI Examples

```bash
# Standard find - returns running only
$ garden-rake find mongodb
No running mongodb offerings found.

# Wishful find - ensures availability
$ garden-rake find mongodb wishfully
Waking mongodb on stone-01...
mongodb available at stone-01.local:27017

# Exact match - auto-wakes stopped
$ garden-rake find mongodb:analytics:prod
Waking mongodb:analytics:prod on stone-02...
mongodb:analytics:prod available at stone-02.local:27017

# Wishful + doesn't exist - plants it
$ garden-rake find redis wishfully
No redis found. Planting on stone-03...
redis available at stone-03.local:6379

# Wishful with placement hint
$ garden-rake find redis wishfully at stone-02
```

### Programmatic Use

```python
# Standard - fails if not running
conn = zen_garden.connect("mongodb")

# Wishful - ensures availability
conn = zen_garden.connect("mongodb", wishfully=True)
```

---

## Manifest Extensions

### Snapshot Configuration

```yaml
# mongodb.snippet.yaml (extended)
image: mongo:7
container_name: mongodb
ports:
  - [27017, 27017]
volumes:
  - mongo-data:/data/db

healthcheck:
  test: ["CMD", "mongosh", "--eval", "db.adminCommand('ping')"]
  interval: 10s
  timeout: 5s
  retries: 5

# NEW: Role detection for tandem configurations
role_detection:
  test: ["CMD", "mongosh", "--quiet", "--eval", "rs.isMaster().ismaster"]
  primary_when: "true"
  interval: 30s

# NEW: Snapshot configuration for nurturing
snapshot:
  method: exec              # "exec" or "volume"
  command:
    - "mongodump"
    - "--archive=/backup/snapshot.archive"
    - "--gzip"
  quiesce: false            # true = stop container during snapshot
  timeout_seconds: 300

restore:
  command:
    - "mongorestore"
    - "--archive=/backup/snapshot.archive"
    - "--gzip"
    - "--drop"
  timeout_seconds: 600
```

### Snapshot Methods

| Method | Description | Use When |
|--------|-------------|----------|
| `exec` | Run command in container | Service has native dump tool |
| `volume` | Raw volume copy | No native tool, or stateless |

If no `snapshot` section → default to `volume` method.

### Volume-Based Snapshot (Default)

```yaml
# For services without native dump tools
snapshot:
  method: volume
  quiesce: true             # Stop container for consistent copy
```

---

## Recovery UX

### List Available Snapshots

```bash
$ garden-rake nurture mongodb:analytics:prod

LOCAL (stone-01):
  [A] 2026-01-29 03:00 (today)      142 MB
  [B] 2026-01-28 03:00 (yesterday)  140 MB

SEED BANK (nas-main):
  [1] 2026-01-29 03:15              142 MB  ← same as A
  [2] 2026-01-28 03:15              140 MB
  [3] 2026-01-27 03:15              138 MB
  [4] 2026-01-26 03:15              141 MB
  [5] 2026-01-25 03:15              139 MB

Restore from [A/B/1-5/cancel]:
```

### Restore Commands

```bash
# Interactive restore (shows options)
garden-rake nurture mongodb:analytics:prod

# Direct restore from local
garden-rake nurture mongodb:analytics:prod from A

# Direct restore from seed bank
garden-rake nurture mongodb:analytics:prod from 3

# Restore to different Stone
garden-rake nurture mongodb:analytics:prod from 3 at stone-02
```

### Full Garden Recovery

```bash
# List what would be recovered
garden-rake nurture garden --dry-run

# Recover all offerings from seed bank
garden-rake nurture garden

# Recover to specific Stone
garden-rake nurture garden at stone-02
```

---

## Implementation Phases

### Phase 1: Local A/B Nurturing

**Goal**: Basic backup/restore on single Stone

- [ ] Add `snapshot` and `restore` sections to manifest schema
- [ ] Implement volume-based snapshot (default)
- [ ] Implement exec-based snapshot (for offerings with native tools)
- [ ] A/B rotation in local staging
- [ ] `garden-rake nurture <offering>` command
- [ ] Restore from local A or B

**No seed bank, no role detection yet.**

### Phase 2: Seed Bank Integration

**Goal**: Offsite backup with policy-driven retention

- [ ] Seed bank structure (seed-bank.yaml, offerings/, index.yaml)
- [ ] Policy resolution (default → offering type → named)
- [ ] N-slot rotation in seed bank
- [ ] Upload to seed bank after local A/B
- [ ] Metadata per slot (meta.yaml)
- [ ] `garden-rake nurture <offering> from <seed-bank-slot>` command

### Phase 3: Role Detection

**Goal**: Primary/replica awareness for tandem configurations

- [ ] Add `role_detection` to manifest schema
- [ ] Implement role detection (exec command + comparison)
- [ ] Event-driven role updates (SSE)
- [ ] Only primary/standalone syncs to seed bank
- [ ] Replicas keep local A/B only

### Phase 4: Visibility & Wishful

**Goal**: Complete discovery ergonomics

- [ ] `visibility` field in offering state (open/shaded)
- [ ] `garden-rake shade` / `unshade` commands
- [ ] Auto-wake on exact match
- [ ] `wishfully` parameter for find
- [ ] Wishful planting (find → plant if not exists)
- [ ] Visibility preserved in backup metadata

### Phase 5: Scheduled Nurturing

**Goal**: Automatic daily backups

- [ ] Cron-like scheduler in Moss
- [ ] Configurable schedule in moss.toml
- [ ] Seed bank availability detection
- [ ] Retry logic for failed uploads
- [ ] Status reporting (`garden-rake nurture status`)

---

## Configuration Reference

### moss.toml

```toml
[nurturing]
enabled = true
schedule = "0 3 * * *"          # Cron: daily at 3 AM

# Local staging (always A/B)
staging_dir = "/var/zen-garden/staging"

# Seed bank targets (in priority order)
[[nurturing.seed_banks]]
type = "path"
path = "/mnt/usb/zen-garden"

[[nurturing.seed_banks]]
type = "network"
protocol = "nfs"
host = "nas.local"
share = "/volume1/zen-garden"
mount_point = "/mnt/seed-bank"

[[nurturing.seed_banks]]
type = "remote"
stone = "stone-gateway"         # Use another Stone's seed bank via API

# Strategy when multiple seed banks available
[nurturing.strategy]
upload = "first"                # "first" or "all"
```

### seed-bank.yaml

```yaml
garden_id: jade-mountain-a1b2c3d4
garden_name: jade-mountain
format_version: 1
initialized: 2026-01-01T00:00:00Z

policy:
  default_slots: 5

  offerings:
    mongodb: { slots: 7 }
    postgresql: { slots: 7 }
    redis: { slots: 3 }

  named:
    "mongodb:analytics:prod": { slots: 10 }
```

---

## Edge Cases

### Seed Bank Unreachable

```
Daily nurturing:
  1. Snapshot → local A
  2. Rotate: old A → B, new → A
  3. Try upload to seed bank
     ├─ Success → done
     └─ Failed → mark "pending sync", retry next cycle
```

Local A/B always succeeds. Seed bank is best-effort with retry.

### Network Partition (Split Brain)

For now: **last-write-wins** in seed bank.

Future: Lantern-based coordination to prevent split brain.

### Slot Count Policy Change

**Decrease** (5 → 3): Prune slots 4 and 5 on next upload.

**Increase** (3 → 5): New slots fill over time.

---

## Terminology Summary

| Term | Meaning |
|------|---------|
| **Nurturing** | The backup/restore system (not "backup") |
| **Seed bank** | Storage holding offering backups |
| **Staging** | Local directory for A/B snapshots |
| **Shaded** | Private visibility (exact match only) |
| **Wishfully** | Declarative provisioning ("make it available") |
| **Tandem** | Multiple offerings with same name working together |
| **Role** | primary / replica / standalone |

---

## References

- [Original Cultivation Specification](zen-garden-spec-cultivation.md)
- [Unified Garden Resilience Proposal](unified-garden-resilience-proposal.md)
- [Ceremonies Specification](ceremonies.md) — Migration uses similar snapshot methods

---

## Reactive Garden Architecture

### Core Principle

The garden is **reactive and event-driven**. Long-running operations never block — they return job IDs. Apps subscribe to events and respond when capabilities become available.

This enables **progressive enhancement**: apps start with what's available and unlock features as capabilities appear.

### Job-Based Long Operations

Any operation that "takes too long" becomes a job:

```bash
$ garden-rake find mongodb wishfully

Job started: plant-mongodb-20260129-1423
Status: pulling image
Track: garden-rake job plant-mongodb-20260129-1423
```

The caller receives a job ID immediately. They can:
1. Poll the job status
2. Subscribe to events and react when complete

### Unfulfilled Wishes

When a wishful request can't be immediately satisfied, it becomes an **unfulfilled wish**:

```
App: find ollama["nomic-embed-text", "llava"] wishfully
Garden: No compatible ollama found. Wish registered.
        → Wish ID: wish-ollama-abc123
        → Will notify when fulfilled.
```

The wish persists. The garden actively watches for capabilities that match.

**Wishes are standing by default** — they persist until fulfilled or explicitly cancelled. This is simpler than TTL-based expiration.

```bash
# Register a standing wish
find ollama["nomic-embed-text"] wishfully

# Cancel a wish explicitly
garden-rake wish cancel wish-ollama-abc123

# Or: wishes auto-cancel when the app disconnects
```

### Capability Announcements

Services announce not just their existence, but their **sub-capabilities**:

```yaml
Stone announces:
  offering: ollama
  status: running
  capabilities:
    models: ["llama3", "mistral", "nomic-embed-text"]
    vram_mb: 8192
    gpu: "RTX 4060"
```

When capabilities change (model pulled, plugin installed), the service re-announces. The garden matches the new capabilities against pending wishes.

### Sub-Capability Query Syntax

Apps often need specific capabilities within an offering. For example, an app using Ollama might need:
- An embedding model for semantic search
- A vision model for image understanding
- A text model for chat

The sub-capability syntax expresses these requirements:

```bash
# Single capability
find ollama["nomic-embed-text"]

# Multiple required (AND) — comma separated
find ollama["nomic-embed-text", "llava", "mistral"]

# Alternatives (OR) — pipe separated
find ollama["llama3" | "mistral" | "phi3"]

# Mixed: need embedding AND (one of these text models)
find ollama["nomic-embed-text", ("llama3" | "mistral")]
```

**Parsing rules:**

| Syntax | Meaning |
|--------|---------|
| `["a"]` | Has capability "a" |
| `["a", "b"]` | Has "a" AND "b" |
| `["a" \| "b"]` | Has "a" OR "b" |
| `["a", ("b" \| "c")]` | Has "a" AND (has "b" OR "c") |

**Different offerings have different capability types:**

```bash
# Ollama: models
find ollama["nomic-embed-text", "llava"]

# Redis: modules
find redis["redisearch", "redisjson"]

# PostgreSQL: extensions
find postgresql["pgvector", "postgis"]
```

### Manifest Capability Discovery

Offerings declare how to discover their capabilities:

```yaml
# ollama.snippet.yaml
capabilities:
  type: models
  discover:
    command: ["ollama", "list", "--json"]
    parse: ".models[].name"
  refresh_interval: 60s    # Re-check periodically
  refresh_on_event: true   # Re-check when service emits change
```

Moss runs the discovery command and includes results in announcements.

### Reactive Flow Example

```
┌─────────────────────────────────────────────────────────────────┐
│                    REACTIVE DISCOVERY FLOW                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. App starts on fresh garden                                  │
│     └─ Subscribes to garden events                              │
│                                                                 │
│  2. App: find mongodb wishfully                                 │
│     └─ Garden: Job started, planting mongodb...                 │
│     └─ App: continues startup (doesn't block)                   │
│                                                                 │
│  3. App: find ollama["nomic-embed-text", "llava"] wishfully     │
│     └─ Garden: No ollama with those models. Wish registered.    │
│     └─ App: AI features disabled (graceful degradation)         │
│                                                                 │
│  4. Garden event: job.completed (mongodb)                       │
│     └─ App: enables database features                           │
│                                                                 │
│  5. Desktop with GPU comes online                               │
│     └─ Announces: ollama with models: ["llama3"]                │
│     └─ Garden: checks wish — needs nomic-embed-text + llava     │
│     └─ No match yet, wish still pending                         │
│                                                                 │
│  6. User pulls models on desktop                                │
│     └─ Ollama re-announces: ["llama3", "nomic-embed-text",      │
│        "llava"]                                                 │
│     └─ Garden: wish matches! Fulfilled.                         │
│                                                                 │
│  7. Garden event: wish.fulfilled                                │
│     └─ App: enables semantic search + vision features           │
│     └─ UI updates dynamically                                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Event Types

Minimal event set for reactivity:

```yaml
# Job lifecycle
job.completed:
  job_id: plant-mongodb-20260129-1423
  result: success          # or "failed"
  error: null              # error message if failed
  offering_id: abc123
  endpoint: stone-01.local:27017

job.progress:              # Optional, for UI feedback
  job_id: plant-mongodb-20260129-1423
  stage: pulling_image
  percent: 45

# Wish lifecycle
wish.fulfilled:
  wish_id: wish-ollama-abc123
  query: 'ollama["nomic-embed-text", "llava"]'
  offering: ollama
  stone: stone-desktop
  endpoint: stone-desktop.local:11434
  capabilities:
    models: ["llama3", "nomic-embed-text", "llava"]

# Service lifecycle
service.available:
  stone: stone-desktop
  offering: ollama
  endpoint: stone-desktop.local:11434
  capabilities:
    models: ["llama3", "nomic-embed-text"]

service.updated:
  stone: stone-desktop
  offering: ollama
  capabilities:
    models: ["llama3", "nomic-embed-text", "llava"]  # Added llava
  added: { models: ["llava"] }
  removed: {}

service.gone:
  stone: stone-desktop
  offering: ollama
  reason: "stone offline"
```

### Wish Matching Algorithm

When a service announces or updates capabilities:

```
1. Get all pending wishes
2. For each wish:
   a. Parse capability requirements (AND/OR tree)
   b. Evaluate against announced capabilities
   c. If all requirements met → fulfill wish, emit event
   d. If not met → wish remains pending
```

For the wish `ollama["nomic-embed-text", "llava"]`:

```
Announced: models = ["llama3"]
  → nomic-embed-text? No
  → Wish pending

Announced: models = ["llama3", "nomic-embed-text"]
  → nomic-embed-text? Yes
  → llava? No
  → Wish pending

Announced: models = ["llama3", "nomic-embed-text", "llava"]
  → nomic-embed-text? Yes
  → llava? Yes
  → Wish fulfilled!
```

### App Integration Pattern

```python
# Reactive app startup
async def main():
    garden = ZenGarden.connect()

    # Subscribe to events
    garden.on("wish.fulfilled", handle_wish_fulfilled)
    garden.on("service.updated", handle_capability_change)
    garden.on("service.gone", handle_service_lost)

    # Request services (non-blocking)
    db_wish = await garden.find("mongodb", wishfully=True)

    # Need embedding + vision models for full AI features
    ai_wish = await garden.find(
        'ollama["nomic-embed-text", "llava"]',
        wishfully=True
    )

    # Start with what's available now
    if db_wish.fulfilled:
        enable_database(db_wish.endpoint)

    if ai_wish.fulfilled:
        enable_ai_features(ai_wish.endpoint)
    else:
        log.info("AI features pending: waiting for ollama with required models")

    # App continues running, reacts to events
    await run_app()

async def handle_wish_fulfilled(event):
    if "ollama" in event.query:
        enable_ai_features(event.endpoint)
        notify_ui("AI features now available!")

async def handle_service_lost(event):
    if event.offering == "ollama":
        disable_ai_features()
        notify_ui("AI features temporarily unavailable")
```

### Implications for Nurturing

The reactive pattern applies to nurturing too:

```bash
$ garden-rake nurture mongodb:analytics:prod from 3

Job started: restore-mongodb-20260129-1430
Restoring from seed bank slot 3...
Track: garden-rake job restore-mongodb-20260129-1430
```

The restore runs asynchronously. Events announce progress and completion.

### Why This Matters

Without reactive architecture, apps must:
1. Poll for services repeatedly
2. Handle "not found" as errors
3. Implement their own retry logic
4. Miss capabilities that appear after startup

With reactive architecture:
1. Declare what you need once
2. Garden tracks and fulfills
3. Events notify when ready
4. Apps progressively enhance

**The garden becomes infrastructure that responds to intent.**

---

## Implementation Prerequisites

Before implementing the full nurturing and reactive discovery system, these foundational pieces must be in place:

### 1. Offering Identity (`offering_id`)

Every offering instance needs a permanent UUID (`offering_id`) that survives renames and migrations. This is foundational for:
- Backup keying (backups stored by `offering_id`, not name)
- Tandem coordination (same name, different `offering_id`)
- Recovery with preserved identity

**Must be added to all offerings before nurturing implementation.**

### 2. Sub-Capabilities per Offering

Each offering that supports sub-capabilities needs manifest extensions:
- Capability type declaration (models, modules, extensions)
- Discovery command and parsing
- Refresh triggers (interval, event-driven)

**Priority offerings**: Ollama, Redis, PostgreSQL (most common sub-capability use cases).

### 3. Existing Infrastructure to Leverage

**Job mechanism**: Moss already has a job system. Nurturing and wishful operations should use this existing infrastructure rather than creating parallel systems.

**SSE events**: Moss already emits events via SSE. Extend event types for wish fulfillment and capability changes.

**Chirp protocol**: Stone announcements already exist. Extend chirp payload to include capabilities.

---

## Implementation Roadmap

### Phase 1: Foundation
- [ ] Add `offering_id` to all managed offerings
- [ ] Add sub-capability discovery to priority offerings (Ollama, Redis, PostgreSQL)
- [ ] Extend chirp protocol to include capabilities

### Phase 2: Local Nurturing (A/B)
- [ ] Implement A/B snapshot rotation on local staging
- [ ] CLI: `garden-rake nurture <offering>` — list local snapshots
- [ ] CLI: `garden-rake nurture <offering> from A|B` — restore from local
- [ ] Integration with existing job system for async restore

### Phase 3: Seed Bank
- [ ] Seed bank structure and policy file (`seed-bank.yaml`)
- [ ] N-slot rotation with policy resolution
- [ ] CLI: `garden-rake nurture <offering>` — list local + seed bank snapshots
- [ ] CLI: `garden-rake nurture <offering> from <slot>` — restore from seed bank
- [ ] CLI: `garden-rake nurture garden` — list all backed-up offerings (for recovery of dead machines)
- [ ] Centralized seed bank browsing for disaster recovery

### Phase 4: Reactive Discovery
- [ ] Modify `find` to support sub-capability syntax
- [ ] Implement wish registry (standing wishes)
- [ ] Wish matching on capability announcements
- [ ] `wish.fulfilled` event emission
- [ ] Auto-wake on exact match

### Phase 5: Wishful Provisioning
- [ ] `find X wishfully` triggers job if not found
- [ ] Job completion fulfills wish
- [ ] Integration with placement for optimal Stone selection

---

**Last Updated**: January 2026
**Status**: Proposal — pending review and implementation
