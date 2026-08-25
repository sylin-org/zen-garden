# Zen Garden Cultivation Specification

**Backup, restore, and garden recovery through seed banks**

**Status:** Proposal  
**Date:** January 2026  
**Authors:** Collaborative design session

---

## Table of Contents

1. [Overview](#overview)
2. [Philosophy](#philosophy)
3. [Core Concepts](#core-concepts)
4. [Identity Model](#identity-model)
5. [Seed Bank Structure](#seed-bank-structure)
6. [Access Modes](#access-modes)
7. [API Specification](#api-specification)
8. [Configuration](#configuration)
9. [Cultivation Flow](#cultivation-flow)
10. [Recovery Flow](#recovery-flow)
11. [Commands](#commands)
12. [Failure Handling](#failure-handling)
13. [Security Considerations](#security-considerations)

---

## Overview

### What is Cultivation?

**Cultivation** is Zen Garden's backup and recovery system. It enables offerings to survive hardware death by storing their data in **seed banks** — portable, garden-scoped storage that any Moss can read from or write to.

When a stone dies, its offerings can be **recovered** on any other stone. The offering's identity persists. Its data is restored. Applications reconnect automatically.

### The Problem

Without cultivation:

```bash
# Stone-01 dies. MongoDB is gone.
# You can re-plant MongoDB on stone-02:
garden-rake offer mongodb --at stone-02

# But it's empty. Your data is lost.
# You needed backups. You needed to configure them.
# You needed to remember where they were.
# You needed to restore manually.
```

With cultivation:

```bash
# Stone-01 dies. MongoDB existed as offering_id abc123.
# Recovery is one command:
garden-rake recover mongo-analytics --at stone-02

# Or recover the entire garden:
garden-rake recover garden --at stone-02

# MongoDB comes back with its data.
# Same offering_id. Same name. Memories intact.
```

### Key Characteristics

1. **Distributed** — Each Moss backs up its own offerings. No central coordinator.
2. **Portable** — Seed banks can move between stones (USB) or be shared (NAS).
3. **Self-describing** — Seed banks contain everything needed for recovery.
4. **Identity-preserving** — Offerings keep their offering_id across death and recovery.
5. **Participatory** — Offerings check for memories at startup and restore themselves.

---

## Philosophy

### Gardens Remember

A garden is more than the stones currently running. It's the **accumulated history** of what has grown there. When a frost kills the plants, the garden doesn't cease to exist — it waits for spring, ready to regrow from seed.

Seed banks hold the garden's memory. Stones may die. Offerings persist.

### Cultivation, Not Backup

We use **cultivation** rather than "backup" deliberately:

| Term | Connotation |
|------|-------------|
| Backup | Defensive, fear-driven, checkbox compliance |
| Cultivation | Nurturing, growth-oriented, tending to health |

You don't "run backups" in Zen Garden. You **cultivate** — nurturing your garden's long-term resilience as a natural part of tending it.

### Wishful Recovery

Recovery is **hopeful, not forced**. When you plant an offering wishfully:

```bash
garden-rake offer mongodb wishfully as mongo-analytics:abc123
```

You're saying: "I hope this offering has memories. If it does, restore them. If not, start fresh."

The offering participates in its own recovery. At startup, it asks: "Do I have memories?" The seed bank answers. Life continues.

### Physical Metaphor

Seed banks can be physical:

- A USB drive you can hold in your hand
- A NAS humming in the corner
- A hard drive in a trusted stone

When disaster strikes, you can point to the seed bank: "The garden's memory is *there*." Physicality provides comfort. You know where your data lives.

---

## Core Concepts

### Seed Bank

A **seed bank** is storage that holds offering backups. It can be:

- A USB drive plugged into a stone
- A network share (NAS) accessible to multiple stones
- A local directory on a stone with extra capacity
- Any storage accessible via the cultivation API

Seed banks are **garden-scoped**. A seed bank knows which garden it belongs to via the `garden_id` in its structure. Multiple gardens can share physical storage by using separate directories.

### Cultivation

**Cultivation** is the process of creating and maintaining backups:

1. Moss snapshots an offering (using offering-specific methods)
2. Moss writes the snapshot to an available seed bank
3. Moss updates the seed bank index
4. Retention policies prune old snapshots

Cultivation happens automatically on a schedule. Each Moss cultivates its own offerings independently.

### Recovery

**Recovery** is the process of restoring offerings from a seed bank:

1. Operator initiates recovery (or automation detects missing offerings)
2. System reads available backups from seed bank
3. Offerings are planted **wishfully** — preserving their offering_id
4. At startup, each offering checks for memories and restores if found
5. Offerings announce themselves; applications reconnect

### Memories

We call backup data **memories**. An offering "has memories" if backups exist in a seed bank. Recovery "restores memories" to an offering.

This language reinforces that offerings have continuity — they're not disposable containers but persistent services with history.

---

## Identity Model

### The Problem with Names Alone

If offerings are identified only by name:

```bash
# Plant MongoDB
garden-rake offer mongodb as mongo-analytics

# Later, rename it
garden-rake call mongo-analytics mongo-legacy

# Where are the backups?
# Under "mongo-analytics"? "mongo-legacy"?
# What if you reuse "mongo-analytics" for a new database?
```

Names change. Identity shouldn't.

### Offering Identity

Every offering has two identifiers:

| Identifier | Purpose | Mutability |
|------------|---------|------------|
| `offering_id` | Permanent identity (UUID) | Immutable |
| `offering_name` | Human-facing name | Mutable via `garden-rake call` |

```yaml
offering_id: a1b2c3d4-5678-90ab-cdef-1234567890ab
offering_name: mongo-analytics
offering_type: mongodb
```

### Lifecycle

```bash
# Plant with default name (type becomes name)
garden-rake offer mongodb
# → offering_id: {new UUID}
# → offering_name: mongodb

# Plant with explicit name
garden-rake offer mongodb as mongo-analytics
# → offering_id: {new UUID}
# → offering_name: mongo-analytics

# Rename
garden-rake call mongo-analytics mongo-legacy
# → offering_id: unchanged
# → offering_name: mongo-legacy

# Plant second MongoDB (distinct identity)
garden-rake offer mongodb as mongo-scratch
# → offering_id: {different UUID}
# → offering_name: mongo-scratch

# Wishful plant (preserve specific identity)
garden-rake offer mongodb wishfully as mongo-analytics:a1b2c3d4
# → offering_id: a1b2c3d4 (preserved)
# → offering_name: mongo-analytics
```

### Stone Identity (Parallel)

Stones follow the same pattern:

| Identifier | Purpose | Mutability |
|------------|---------|------------|
| `stone_id` | Permanent identity (UUID) | Immutable |
| `stone_name` | Human-facing name | Mutable via `garden-rake call` |

### Backup Keying

Backups are keyed by `offering_id`, not name:

```
seed-bank/offerings/a1b2c3d4/
  ├── identity.yaml      # Current name, name history
  ├── latest -> 2026-01-23T03:00:00Z
  └── 2026-01-23T03:00:00Z/
      └── ...
```

When an offering is renamed, its backup history follows automatically.

---

## Seed Bank Structure

### Universal Layout

Every seed bank — USB, NAS, local disk — uses the same structure:

```
{seed-bank-root}/
├── garden.yaml                         # Garden identity
│
├── stones/                             # Stone manifests
│   ├── {stone_id}/
│   │   ├── identity.yaml               # stone_id, name, hardware
│   │   └── moss.toml                   # Last known configuration
│   └── ...
│
├── offerings/                          # Offering backups
│   ├── {offering_id}/
│   │   ├── identity.yaml               # Offering identity + name history
│   │   ├── latest -> {timestamp}/      # Symlink to most recent
│   │   ├── {timestamp}/
│   │   │   ├── manifest.yaml           # Everything to restore
│   │   │   └── data.archive.gz         # Snapshot data
│   │   └── ...
│   └── ...
│
├── index.yaml                          # Quick lookup catalog
│
└── .locks/                             # Coordination
    └── index.lock
```

### garden.yaml

Identifies which garden this seed bank belongs to:

```yaml
# garden.yaml

garden_id: jade-mountain-a1b2c3d4
garden_name: jade-mountain
created: 2026-01-01T00:00:00Z

seed_bank:
  initialized: 2026-01-01T00:00:00Z
  format_version: 1
```

When a Moss mounts storage, it checks `garden.yaml` to confirm it's the right garden's seed bank.

### stones/{stone_id}/identity.yaml

Preserves stone information for disaster recovery:

```yaml
# stones/abc123/identity.yaml

stone_id: abc123-def456-...
current_name: stone-01

names:
  - name: stone-01
    from: 2026-01-01T00:00:00Z
    to: null

hardware:
  architecture: x64
  cpu: Intel Celeron J4105
  memory_mb: 8192
  
last_seen: 2026-01-23T02:00:00Z
offerings:
  - offering_id: a1b2c3d4
    name: mongo-analytics
  - offering_id: e5f6g7h8
    name: cache
```

### offerings/{offering_id}/identity.yaml

Tracks offering identity and name history:

```yaml
# offerings/a1b2c3d4/identity.yaml

offering_id: a1b2c3d4-5678-90ab-cdef-1234567890ab
offering_type: mongodb
current_name: mongo-legacy

names:
  - name: mongodb
    from: 2026-01-01T00:00:00Z
    to: 2026-01-10T00:00:00Z
  - name: mongo-analytics
    from: 2026-01-10T00:00:00Z
    to: 2026-01-20T00:00:00Z
  - name: mongo-legacy
    from: 2026-01-20T00:00:00Z
    to: null                        # Current

provenance:
  - stone_id: abc123
    stone_name: stone-01
    from: 2026-01-01T00:00:00Z
    to: null                        # Current location
```

### offerings/{offering_id}/{timestamp}/manifest.yaml

Everything needed to restore this offering:

```yaml
# offerings/a1b2c3d4/2026-01-23T03:00:00Z/manifest.yaml

offering_id: a1b2c3d4-5678-90ab-cdef-1234567890ab
offering_name: mongo-legacy         # Name at backup time
offering_type: mongodb

source:
  stone_id: abc123
  stone_name: stone-01
  timestamp: 2026-01-23T03:00:00Z

container:
  image: mongo:7
  ports:
    - "27017:27017"
  volumes:
    - name: mongo-data
      path: /data/db
  environment:
    MONGO_INITDB_DATABASE: mydb

snapshot:
  method: mongodump
  files:
    - name: data.archive.gz
      size_bytes: 142000000
      checksum: sha256:abc123...
  
restore:
  command:
    - "mongorestore"
    - "--archive=/backup/data.archive"
    - "--gzip"
    - "--drop"

healthcheck:
  test: ["CMD", "mongosh", "--eval", "db.adminCommand('ping')"]
  timeout_seconds: 30
```

### index.yaml

Quick lookup without traversing directories:

```yaml
# index.yaml

last_updated: 2026-01-23T03:15:00Z
updated_by: 
  stone_id: abc123
  stone_name: stone-01

offerings:
  a1b2c3d4:
    name: mongo-legacy
    type: mongodb
    latest: 2026-01-23T03:00:00Z
    backup_count: 7
    total_size_bytes: 980000000
    
  e5f6g7h8:
    name: cache
    type: redis
    latest: 2026-01-23T03:00:00Z
    backup_count: 7
    total_size_bytes: 84000000

stones:
  abc123:
    name: stone-01
    last_seen: 2026-01-23T03:00:00Z
    offering_count: 2
```

---

## Access Modes

Seed banks can be accessed in three ways. All modes use the same physical structure and API surface.

### Local Mode

A seed bank directly accessible to one stone (USB drive, local disk):

```
┌─────────────┐         ┌─────────────┐
│  stone-01   │         │  stone-02   │
│             │         │             │
│  [USB]──────┼────────>│  (via API)  │
│  /mnt/usb   │         │             │
│             │         │             │
│  Exposes    │         │  Calls      │
│  API        │         │  API        │
└─────────────┘         └─────────────┘
```

The stone with local access:
- Mounts the storage
- Announces `cultivation:seed-bank` capability
- Exposes the cultivation API
- Other stones call the API to read/write

**Configuration:**

```toml
[[cultivation.seed_banks]]
type = "path"
path = "/mnt/usb/zen-garden"
announce = true                     # Expose API for others
```

### Shared Mode

A seed bank accessible to all stones (NAS, network share):

```
┌─────────────┐
│  stone-01   │────┐
└─────────────┘    │
                   │      ┌─────────────────┐
┌─────────────┐    ├─────>│  nas.local      │
│  stone-02   │────┤      │  /volume1/zen-  │
└─────────────┘    │      │  garden         │
                   │      └─────────────────┘
┌─────────────┐    │
│  stone-03   │────┘
└─────────────┘

Each stone mounts directly.
No API intermediary.
```

Each stone mounts the NAS independently and writes directly to the filesystem.

**Configuration:**

```toml
[[cultivation.seed_banks]]
type = "network"
protocol = "nfs"                    # or "smb", "cifs"
host = "nas.local"
share = "/volume1/zen-garden"
mount_point = "/mnt/seed-bank"

# For SMB with credentials
# username = "${NAS_USER}"
# password = "${NAS_PASS}"
```

### Gateway Mode

One stone has access to storage (NAS, cloud, special hardware) and serves as gateway:

```
┌─────────────┐         ┌─────────────┐         ┌─────────────┐
│  stone-01   │         │  stone-02   │         │    NAS      │
│             │         │  (gateway)  │         │             │
│  (via API)──┼────────>│             │────────>│  storage    │
│             │         │  Exposes    │         │             │
│             │         │  API        │         │             │
└─────────────┘         └─────────────┘         └─────────────┘
```

The gateway stone:
- Has exclusive access to storage (credentials, network path, etc.)
- Announces `cultivation:seed-bank` capability
- Exposes the cultivation API
- Other stones call the API

**Configuration on gateway:**

```toml
[[cultivation.seed_banks]]
type = "network"
protocol = "smb"
host = "nas.local"
share = "/volume1/zen-garden"
mount_point = "/mnt/nas"
announce = true                     # Act as gateway
```

**Configuration on other stones:**

```toml
[[cultivation.seed_banks]]
type = "remote"
stone = "stone-02"                  # Gateway stone name
# Discovered via mDNS, uses cultivation API
```

### Portable USB Pattern

USB drives can move between stones. The seed bank follows:

```
Day 1: USB in stone-01
  └── stone-01 announces cultivation:seed-bank
  └── All stones backup via stone-01

Day 2: USB moved to stone-02
  └── stone-01 detects unmount, stops announcing
  └── stone-02 detects mount, starts announcing
  └── All stones backup via stone-02

The seed bank data is unchanged.
Only the access path moved.
```

Moss detects mount/unmount events and adjusts announcements automatically.

---

## API Specification

### Cultivation API

All seed bank access (local, gateway, remote) uses the same API surface.

#### Write Backup

```http
POST /api/v1/cultivation/offerings/{offering_id}/backups

Content-Type: multipart/form-data
  manifest: (manifest.yaml content)
  data: (data.archive.gz binary)

Response 201 Created:
{
  "offering_id": "a1b2c3d4-...",
  "timestamp": "2026-01-23T03:00:00Z",
  "size_bytes": 142000000
}
```

#### List All Offerings

```http
GET /api/v1/cultivation/offerings

Response 200:
{
  "offerings": [
    {
      "offering_id": "a1b2c3d4-...",
      "name": "mongo-legacy",
      "type": "mongodb",
      "latest": "2026-01-23T03:00:00Z",
      "backup_count": 7,
      "total_size_bytes": 980000000
    },
    ...
  ]
}
```

#### List Backups for Offering

```http
GET /api/v1/cultivation/offerings/{offering_id}/backups

Response 200:
{
  "offering_id": "a1b2c3d4-...",
  "name": "mongo-legacy",
  "backups": [
    {
      "timestamp": "2026-01-23T03:00:00Z",
      "size_bytes": 142000000,
      "checksum": "sha256:abc123..."
    },
    {
      "timestamp": "2026-01-22T03:00:00Z",
      "size_bytes": 140500000,
      "checksum": "sha256:def456..."
    }
  ]
}
```

#### Get Latest Backup

```http
GET /api/v1/cultivation/offerings/{offering_id}/backups/latest

Response 200:
Content-Type: multipart/form-data
  manifest: (manifest.yaml content)
  data: (data.archive.gz binary)
```

#### Get Specific Backup

```http
GET /api/v1/cultivation/offerings/{offering_id}/backups/{timestamp}

Response 200:
Content-Type: multipart/form-data
  manifest: (manifest.yaml content)
  data: (data.archive.gz binary)
```

#### Prune Old Backups

```http
DELETE /api/v1/cultivation/offerings/{offering_id}/prune
  ?keep_daily=7
  &keep_weekly=4
  &keep_monthly=6

Response 200:
{
  "offering_id": "a1b2c3d4-...",
  "pruned": 12,
  "remaining": 17,
  "space_freed_bytes": 1200000000
}
```

#### List Stones

```http
GET /api/v1/cultivation/stones

Response 200:
{
  "stones": [
    {
      "stone_id": "abc123-...",
      "name": "stone-01",
      "last_seen": "2026-01-23T03:00:00Z",
      "offering_count": 2
    }
  ]
}
```

#### Get Stone Manifest

```http
GET /api/v1/cultivation/stones/{stone_id}

Response 200:
{
  "stone_id": "abc123-...",
  "identity": { ... },              # identity.yaml content
  "config": { ... }                 # moss.toml content
}
```

#### Update Stone Manifest

```http
PUT /api/v1/cultivation/stones/{stone_id}

Content-Type: application/json
{
  "identity": { ... },
  "config": { ... }
}

Response 200:
{
  "stone_id": "abc123-...",
  "updated": "2026-01-23T03:00:00Z"
}
```

### mDNS Announcement

Stones with seed bank access announce:

```
_zen-garden._tcp.local.

TXT "capability=cultivation:seed-bank"
TXT "seed-bank-name=usb-backup"
TXT "space-available=500GB"
TXT "offering-count=12"
```

---

## Configuration

### Moss Configuration

```toml
# moss.toml

[cultivation]
enabled = true
schedule = "0 3 * * *"              # Cron: daily at 3 AM

# Local staging (always enabled)
[cultivation.local]
path = "/var/zen-garden/snapshots"
retention = { count = 3 }           # Keep 3 local snapshots

# Seed bank targets (in priority order)
[[cultivation.seed_banks]]
type = "network"
protocol = "nfs"
host = "nas.local"
share = "/volume1/zen-garden"
mount_point = "/mnt/seed-bank"
priority = 1

[[cultivation.seed_banks]]
type = "path"
path = "/mnt/usb/zen-garden"
announce = true
priority = 2

[[cultivation.seed_banks]]
type = "remote"
stone = "stone-gateway"
priority = 3

# Retention policy for seed banks
[cultivation.retention]
daily = 7                           # Keep 7 daily backups
weekly = 4                          # Keep 4 weekly backups
monthly = 6                         # Keep 6 monthly backups

# Write strategy when multiple seed banks available
[cultivation.strategy]
write = "first"                     # or "all" for redundancy
read = "first"                      # Try in priority order
```

### Offering-Specific Cultivation

Offerings can customize their snapshot method via `{offering}.migration.yaml`:

```yaml
# mongodb.migration.yaml

version: "1"
strategy: stateful-snapshot

snapshot:
  method: mongodump
  command:
    - "mongodump"
    - "--archive=/backup/snapshot.archive"
    - "--gzip"
  volume: mongo-data
  timeout_seconds: 300

restore:
  command:
    - "mongorestore"
    - "--archive=/backup/snapshot.archive"
    - "--gzip"
    - "--drop"
  post_restore_healthcheck: true
  timeout_seconds: 600
```

If no migration manifest exists, Moss uses volume snapshot (default).

---

## Cultivation Flow

### Scheduled Backup

```
┌─────────────────────────────────────────────────────────────────┐
│                    CULTIVATION FLOW                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   1. Schedule triggers (cron: 0 3 * * *)                        │
│                         │                                       │
│                         ▼                                       │
│   2. For each offering on this stone:                           │
│      ┌──────────────────────────────────────────┐               │
│      │ a. Create snapshot (offering-specific)   │               │
│      │ b. Write to local staging                │               │
│      │ c. Get available seed bank               │               │
│      │ d. Write to seed bank                    │               │
│      │ e. Update index                          │               │
│      │ f. Apply retention policy                │               │
│      └──────────────────────────────────────────┘               │
│                         │                                       │
│                         ▼                                       │
│   3. Update stone manifest in seed bank                         │
│                         │                                       │
│                         ▼                                       │
│   4. Report health status                                       │
│      (last_backup, backup_age, etc.)                            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Backup Code Flow

```rust
async fn cultivate(&self) -> Result<CultivationReport> {
    let mut report = CultivationReport::new();
    
    // Get available seed bank
    let seed_bank = self.get_seed_bank().await?;
    
    // Backup each offering
    for offering in self.registry.offerings() {
        match self.cultivate_offering(&offering, &seed_bank).await {
            Ok(backup) => report.succeeded(offering.id, backup),
            Err(e) => report.failed(offering.id, e),
        }
    }
    
    // Update stone manifest
    self.update_stone_manifest(&seed_bank).await?;
    
    // Apply retention
    self.apply_retention(&seed_bank).await?;
    
    Ok(report)
}

async fn cultivate_offering(
    &self,
    offering: &Offering,
    seed_bank: &SeedBank,
) -> Result<Backup> {
    // 1. Create snapshot
    let snapshot = self.snapshot(offering).await?;
    
    // 2. Write to local staging
    let local_path = self.write_local_staging(&snapshot).await?;
    
    // 3. Write to seed bank
    let backup = seed_bank.write_backup(
        offering.id,
        &snapshot.manifest,
        &snapshot.data,
    ).await?;
    
    Ok(backup)
}
```

### Seed Bank Abstraction

```rust
enum SeedBank {
    Local { path: PathBuf },
    Network { mount: PathBuf },
    Remote { endpoint: Url },
}

impl SeedBank {
    async fn write_backup(
        &self,
        offering_id: &OfferingId,
        manifest: &Manifest,
        data: &[u8],
    ) -> Result<Backup> {
        match self {
            SeedBank::Local { path } |
            SeedBank::Network { mount: path } => {
                // Direct filesystem write
                let backup_path = path
                    .join("offerings")
                    .join(offering_id.to_string())
                    .join(timestamp_now());
                
                fs::create_dir_all(&backup_path).await?;
                fs::write(backup_path.join("manifest.yaml"), manifest).await?;
                fs::write(backup_path.join("data.archive.gz"), data).await?;
                
                // Update latest symlink
                let latest = path
                    .join("offerings")
                    .join(offering_id.to_string())
                    .join("latest");
                symlink(&backup_path, &latest).await?;
                
                Ok(Backup { path: backup_path, ... })
            }
            SeedBank::Remote { endpoint } => {
                // API call to gateway stone
                let response = self.client
                    .post(format!("{}/cultivation/offerings/{}/backups", 
                        endpoint, offering_id))
                    .multipart(form)
                    .send()
                    .await?;
                
                Ok(response.json().await?)
            }
        }
    }
    
    async fn get_latest(
        &self,
        offering_id: &OfferingId,
    ) -> Result<Option<Backup>> {
        // Similar pattern: filesystem or API
        ...
    }
}
```

---

## Recovery Flow

### Full Garden Recovery

```
┌─────────────────────────────────────────────────────────────────┐
│                 GARDEN RECOVERY FLOW                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   1. garden-rake recover garden --from {seed-bank}              │
│                         │                                       │
│                         ▼                                       │
│   2. Read seed bank index                                       │
│      ┌──────────────────────────────────────┐                   │
│      │ offerings:                           │                   │
│      │   abc123: mongo-analytics (mongodb)  │                   │
│      │   def456: cache (redis)              │                   │
│      │   ghi789: postgres-main (postgresql) │                   │
│      └──────────────────────────────────────┘                   │
│                         │                                       │
│                         ▼                                       │
│   3. For each offering:                                         │
│      garden-rake offer {type} wishfully as {name}:{id}          │
│                         │                                       │
│                         ▼                                       │
│   4. Moss plants offering (normal flow)                         │
│                         │                                       │
│                         ▼                                       │
│   5. Before starting: "Do I have memories?"                     │
│      ┌──────────────────────────────────────┐                   │
│      │ Check seed bank for offering_id      │                   │
│      │ Found? → Restore data                │                   │
│      │ Not found? → Start fresh             │                   │
│      └──────────────────────────────────────┘                   │
│                         │                                       │
│                         ▼                                       │
│   6. Start → Healthcheck → Announce                             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Wishful Planting

The `wishfully` modifier preserves offering identity:

```rust
struct PlantRequest {
    offering_type: String,          // "mongodb"
    offering_name: String,          // "mongo-analytics"
    wishful_id: Option<OfferingId>, // Some(abc123) for recovery
}

async fn plant_offering(&self, request: PlantRequest) -> Result<()> {
    // 1. Resolve identity
    let offering_id = request.wishful_id
        .unwrap_or_else(|| OfferingId::new());
    
    // 2. Pull image, create container
    self.pull_image(&request.image).await?;
    let container = self.create_container(&request).await?;
    
    // 3. Check for memories BEFORE starting
    if let Some(seed_bank) = self.get_seed_bank().await.ok() {
        if let Some(backup) = seed_bank.get_latest(&offering_id).await.ok() {
            // Has memories! Restore.
            self.restore_data(&container, &backup).await?;
        }
    }
    
    // 4. Start, healthcheck, announce
    self.start_container(&container).await?;
    self.wait_healthy(&container).await?;
    self.announce(&offering_id, &request.name).await?;
    
    Ok(())
}
```

### Single Offering Recovery

```bash
# By current name
garden-rake recover mongo-analytics

# By offering_id
garden-rake recover --id abc123

# To specific stone
garden-rake recover mongo-analytics --at stone-02

# Fork (new identity, copy data)
garden-rake recover mongo-analytics as mongo-copy
# → New offering_id
# → Data from abc123's backup
```

---

## Commands

### Cultivation Commands

```bash
# Trigger immediate backup (all offerings on this stone)
garden-rake cultivate

# Backup specific offering
garden-rake cultivate mongo-analytics

# Check cultivation status
garden-rake cultivation status

# View backup history
garden-rake cultivation history mongo-analytics

# Prune old backups
garden-rake cultivation prune --keep-daily 7 --keep-weekly 4
```

### Seed Bank Commands

```bash
# Designate storage as seed bank
garden-rake tend /mnt/usb as seed-bank
garden-rake tend nas.local:/volume1/zen-garden as seed-bank

# List known seed banks
garden-rake seed-banks

# Check seed bank health
garden-rake seed-bank status nas-main

# Remove seed bank designation
garden-rake forget seed-bank usb-backup
```

### Recovery Commands

```bash
# Recover full garden
garden-rake recover garden --from nas.local:/volume1/zen-garden

# Recover full garden (auto-discover seed bank)
garden-rake recover garden

# Recover specific offering
garden-rake recover mongo-analytics

# Recover by offering_id
garden-rake recover --id abc123

# Recover to specific stone
garden-rake recover mongo-analytics --at stone-02

# Fork (new identity)
garden-rake recover mongo-analytics as mongo-fork

# Dry run
garden-rake recover garden --dry-run

# Filter by category
garden-rake recover garden --only database,cache

# Exclude offerings
garden-rake recover garden --exclude mongo-scratch
```

### Naming Commands

```bash
# Plant with explicit name
garden-rake offer mongodb as mongo-analytics

# Rename offering
garden-rake call mongo-analytics mongo-legacy

# Rename stone
garden-rake call stone-01 stone-primary

# Wishful plant (preserve identity)
garden-rake offer mongodb wishfully as mongo-analytics:abc123
```

---

## Failure Handling

### Seed Bank Unreachable

```
Backup attempt:
  1. Try primary seed bank (NAS) → Unreachable
  2. Try secondary seed bank (USB) → Available → Use it
  3. No seed banks available?
     → Write to local staging only
     → Mark backup as "staged, not synced"
     → Retry on next cycle
```

Local staging ensures backups aren't lost due to temporary seed bank unavailability.

### Partial Backup Failure

```
Cultivating 5 offerings:
  mongodb      ✓ backed up
  redis        ✓ backed up  
  postgresql   ✗ snapshot failed (disk full)
  minio        ✓ backed up
  grafana      ✓ backed up

Report partial success.
Retry failed offerings next cycle.
```

### Corrupt Backup Detection

Manifests include checksums:

```yaml
snapshot:
  files:
    - name: data.archive.gz
      checksum: sha256:abc123...
```

On restore:
1. Download backup
2. Verify checksum
3. Checksum mismatch? → Try previous backup
4. All backups corrupt? → Alert, start fresh

### Concurrent Write Coordination

Multiple Mosses may write to shared seed bank simultaneously:

1. Each offering writes to its own directory (no conflict)
2. Index updates use file locking:
   ```
   .locks/index.lock
   ```
3. Lock timeout: 30 seconds
4. Lock held too long? → Break lock, warn in logs

---

## Security Considerations

### Seed Bank Access

For shared seed banks (NAS), access control options:

1. **Network isolation** — Seed bank only reachable from garden VLAN
2. **Credentials** — SMB/NFS authentication
3. **Encryption at rest** — NAS-level or filesystem encryption

### Backup Encryption

For sensitive data, Moss can encrypt before writing:

```toml
[cultivation.encryption]
enabled = true
key_source = "keystone"             # Derive from garden identity
```

Flow:
1. Snapshot offering
2. Encrypt with garden key
3. Write encrypted blob to seed bank
4. On restore: decrypt with garden key

Only Mosses in this garden can decrypt.

### Credential Storage

NAS credentials in moss.toml:

```toml
[[cultivation.seed_banks]]
type = "network"
username = "${NAS_USER}"            # Environment variable
password = "${NAS_PASS}"
```

Or via secrets offering (if available):

```toml
[[cultivation.seed_banks]]
type = "network"
credentials = "vault:nas/cultivation"
```

---

## Appendix: Vocabulary Summary

| Term | Meaning |
|------|---------|
| **Cultivation** | The practice of backing up offerings |
| **Seed bank** | Storage that holds offering backups |
| **Memories** | Backup data for an offering |
| **Wishfully** | Planting with hope of restoring memories |
| **Tend** | Designating storage as a seed bank |
| **Recover** | Restoring offerings from seed bank |
| **Offering ID** | Permanent identity (UUID) |
| **Offering name** | Human-facing name (mutable) |

---

## Appendix: Migration from External Backups

If you have existing backups from outside Zen Garden:

```bash
# Import existing MongoDB dump
garden-rake cultivate import \
  --offering-type mongodb \
  --name mongo-analytics \
  --from /path/to/mongodump.archive.gz

# This creates:
# - New offering_id
# - Backup entry in seed bank
# - Ready for wishful planting
```

---

## References

- [Ceremony Specification](garden-distributed-ceremonies.md) — Migration ceremonies use similar snapshot methods
- [Offering Modes](../archive/proposals/offering-modes.md) — How offerings are planted, adopted, borrowed
- [Moss Daemon Lifecycle](../specs/moss-daemon-lifecycle.md) — Moss architecture

---

**Last Updated:** January 2026  
**Status:** Proposal — pending review and implementation
