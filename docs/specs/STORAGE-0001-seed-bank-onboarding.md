# STORAGE-0001: Seed Bank USB Onboarding

**Status:** Draft  
**Date:** 2026-01-28  
**Author:** Architecture Team

---

## Overview

This specification defines the UX flow and technical implementation for onboarding removable USB storage as a Seed Bank in Zen Garden.

---

## Definitions

### Seed Bank Types

| Type | Naming | Receives Unnamed Requests | Receives Named Requests | Sync Participation |
|------|--------|---------------------------|-------------------------|--------------------|
| **Unnamed** | Default: `seed-bank-zengarden` | ✅ Primary pool | ❌ No name to match | ✅ Syncs with all unnamed |
| **Named + Open** | User-specified | ✅ Fallback if no unnamed | ✅ If name matches | ✅ Syncs with all open |
| **Named + Closed** | User-specified | ❌ Never | ✅ Only if name matches | ✅ Syncs only with same-name closed |

**Naming Rules:**
- Default name (no `as` clause): `seed-bank-zen-garden`
- Random name (`prepare {device} named`): Generates `seed-{adjective}-{noun}` from word lists
- Explicit name (`as backup-vault`): Uses provided name

**Sync Groups & Pool Identity:**
- All **unnamed** seed-banks form a single sync group (garden-wide pool)
- All **named + open** seed-banks sync with the unnamed pool
- **Named + closed** seed-banks with the **same name** sync only among themselves (private replication group)
- Each sync group has a `pool_id` (GUIDv7) assigned when first device joins
- Mismatched `pool_id` on join → SSE event `storage.pool_conflict` for audio/visual alert

### Routing Logic

**For unnamed S3 writes (no `X-Seed-Bank` header):**

1. Local **unnamed** seed-bank present? → Route there
2. No unnamed, but local **named + open**? → Route there (fallback)
3. None local? → Proxy to stone with (unnamed OR named+open)
4. **Named + closed** → Never receives unnamed traffic

**For named S3 writes (`X-Seed-Bank: vault`):**

1. Find seed-bank with name `vault` → Route there (open OR closed)
2. Not found → `404 Not Found`

### Event Pipeline

All outputs fire independently to their respective channels:

```
USB Insert → Moss Event Queue
                 |
         +-------+-------+
         |               |
    TTY1 write      SSE broadcast
    (no-op if       (to all clients)
     no monitor)         |
                   +-----+-----+
                   |           |
                Cricket     Rake/other
                (plays      (displays
                 audio)      notification)
```

- **TTY1**: Fire-and-forget. Writes succeed even if no monitor attached.
- **SSE**: Broadcast to all connected clients. No clients? Event dropped silently.
- **Cricket**: Standard SSE subscriber via `garden-companion-sdk`. No special handler in Moss.

---

## 1. Detection Flow

### 1.1 Device Eligibility

A storage device is **eligible** for seed bank preparation when:

| Criterion | Requirement |
|-----------|-------------|
| Removable | Device is marked as removable by kernel |
| Writable | Mount is read-write or device has no filesystem |
| Empty | Zero visible files (no hidden files except system-created) |
| Not prepared | No `.zen-garden/` directory exists |
| Allowed path | Mounted under `/mnt/*`, `/media/*`, or `/run/media/*` |

### 1.2 Device States

| State | Description | Action |
|-------|-------------|--------|
| `unpartitioned` | Raw device, no partition table | Partition + format |
| `unformatted` | Partition exists, no filesystem | Format only |
| `empty` | Filesystem exists, zero visible files | Create structure |
| `prepared` | Has `.zen-garden/` directory | Already a seed bank |
| `has_data` | Contains visible files | Cannot prepare (user must clean) |

### 1.3 Detection Mechanism

Moss uses the `udev` crate (libudev bindings) for structured event monitoring.
This is preferred over subprocess parsing (`udevadm monitor`) for robustness.

```rust
// Using udev crate for structured events
use udev::{MonitorBuilder, EventType};

async fn monitor_usb_storage() -> Result<()> {
    // Create monitor socket (netlink)
    let socket = MonitorBuilder::new()?
        .match_subsystem("block")?
        .listen()?;
    
    // Wrap in async (udev socket is sync, use tokio::task::spawn_blocking or polling)
    loop {
        let event = poll_udev_event(&socket).await?;
        
        if event.event_type() == EventType::Add && is_usb_storage(&event) {
            let info = analyze_device(&event.devnode())?;
            if info.eligible {
                emit_storage_event(&info).await;
                print_tty_ribbon(&info)?;
            }
        }
    }
}
```

---

## 2. SSE Event

### 2.1 Event Type

Add to `src/common/src/presence/event_types.rs`:

```rust
// Storage events
pub const CATEGORY_STORAGE: &str = "storage";
pub const STORAGE_DETECTED: &str = "storage.detected";
pub const STORAGE_PREPARED: &str = "storage.prepared";
pub const STORAGE_RELEASED: &str = "storage.released";
pub const STORAGE_REMOVED: &str = "storage.removed";
pub const STORAGE_POOL_CONFLICT: &str = "storage.pool_conflict";
pub const STORAGE_READONLY: &str = "storage.readonly_detected";
pub const STORAGE_PREPARE_PROGRESS: &str = "storage.prepare.progress";
```

### 2.2 Event Payload

```json
{
  "type": "storage.detected",
  "timestamp": "2026-01-28T10:30:00Z",
  "data": {
    "device": "/dev/sdb1",
    "mount_path": "/mnt/usb",
    "label": "SANDISK_32GB",
    "capacity_bytes": 32000000000,
    "state": "empty",
    "eligible": true,
    "removable": true
  }
}
```

**Pool Conflict Event:**
```json
{
  "type": "storage.pool_conflict",
  "timestamp": "2026-01-28T10:30:00Z",
  "data": {
    "seed_bank": "backup-vault",
    "this_pool_id": "01956a3e-7c00-7000-8000-000000000001",
    "target_pool_id": "01956a3e-7c00-7000-8000-000000000099",
    "action_required": "merge_or_rename"
  }
}
```

### 2.3 Cricket Integration

Cricket subscribes to storage events via SSE and plays audio feedback using the tune system.

**Event Mappings (in `tunes/zen-tech/tune.yaml`):**

```yaml
# Storage events (seed bank lifecycle)
storage.detected:
  resource: samples/computer-chimes.mp3
  channel: foreground
  debounce_ms: 3000

storage.prepared:
  resource: samples/success-synth.mp3
  channel: foreground
  debounce_ms: 1000

storage.released:
  resource: samples/telephone-dock-beep.mp3
  channel: midground
  debounce_ms: 1000

storage.removed:
  resource: samples/beep-oops.mp3
  channel: midground
  debounce_ms: 1000

storage.pool_conflict:
  resource: samples/alert-short.mp3
  channel: foreground
  debounce_ms: 5000
```

**How It Works:**

Cricket's event handler (`events.rs`) is generic - it looks up the event type in the active tune's event mappings and plays the corresponding audio resource. No special code is needed per event type.

```rust
// In cricket/src/events.rs - Generic handler
async fn on_event(&self, event: SseEvent) {
    // Get mapping from active tune (e.g., "storage.detected" → samples/computer-chimes.mp3)
    let Some(mapping) = self.tune_manager.get_event_mapping(&event.event_type) else {
        return; // No mapping for this event
    };

    // Check debounce, resolve channel, play audio
    // ...
}
```

This design allows tunes to customize which events trigger audio and which samples to use, without code changes.

---

## 3. TTY1 Ribbon

### 3.1 Format (matching existing boot/shutdown ribbons)

```rust
/// Print seed bank detection ribbon to TTY1
pub fn print_storage_detected_ribbon(info: &StorageDetectedInfo) -> Result<()> {
    let divider = "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━";
    
    // USB icon with storage info
    let line1 = format!("    ┌──┐      🌱          Device: {} ({})", 
        info.label, format_bytes(info.capacity_bytes));
    let line2 = "    │▓▓│                  A new seed bank awaits...";
    let line3 = "    └┬─┘";
    let line4 = format!("     │        Prepare:    garden-rake prepare seed-bank");
    
    tty_write("")?;
    tty_write(divider)?;
    tty_write(&line1)?;
    tty_write(line2)?;
    tty_write(line3)?;
    tty_write(&line4)?;
    tty_write(divider)?;
    tty_write("")?;
    
    Ok(())
}
```

### 3.2 Output Example

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    ┌──┐      🌱          Device: SANDISK_32GB (32.00 GB)
    │▓▓│                  A new seed bank awaits...
    └┬─┘
     │        Prepare:    garden-rake prepare seed-bank
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### 3.3 Multiple Devices

When multiple eligible devices are detected:

```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    ┌──┐      🌱          2 devices await preparation
    │▓▓│                  
    └┬─┘                  SANDISK_32GB (32.00 GB) at /mnt/usb
     │                    KINGSTON_64GB (64.00 GB) at /mnt/usb2
     │        
     │        Prepare:    garden-rake prepare seed-bank SANDISK_32GB
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

---

## 4. CLI Commands

### 4.1 Koan Syntax (Human-Friendly)

```bash
# Basic - uses first eligible device, default name (seed-bank-zengarden)
garden-rake prepare seed-bank

# Random generated name (seed-{adjective}-{noun})
garden-rake prepare seed-bank named

# Explicit stone target
garden-rake prepare seed-bank at stone-alpha

# Specific device by label
garden-rake prepare seed-bank SANDISK_32GB

# Specific device by path
garden-rake prepare seed-bank /mnt/usb

# Device + stone
garden-rake prepare seed-bank SANDISK_32GB at stone-alpha

# With custom seed bank name
garden-rake prepare seed-bank SANDISK_32GB as portable-backup
garden-rake prepare seed-bank SANDISK_32GB as portable-backup at stone-alpha

# Random name with device
garden-rake prepare seed-bank SANDISK_32GB named
```

### 4.2 Normative Syntax (Scripting)

```bash
# Full explicit form
garden-rake prepare seed-bank \
    --device SANDISK_32GB \
    --name portable-backup \
    --at stone-alpha

# Path-based
garden-rake prepare seed-bank \
    --device /mnt/usb \
    --at stone-alpha

# JSON output for scripting
garden-rake prepare seed-bank --device SANDISK_32GB --format json
```

### 4.3 Grammar

```
prepare seed-bank [<device>] [named | as <name>] [at <stone>]

<device>  := <label> | <path>
<label>   := [A-Z0-9_-]+           # Device label (e.g., SANDISK_32GB)
<path>    := /mnt/... | /media/... | /run/media/...
<name>    := [a-z0-9-]+            # Seed bank identifier
<stone>   := <stone-name>          # Target stone

# 'named' keyword → generate random seed-{adjective}-{noun}
# 'as <name>' → use explicit name
# neither → use default 'seed-bank-zengarden'
```

### 4.4 Normative Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--device` | `-d` | Device label or mount path |
| `--name` | `-n` | Custom seed bank name |
| `--named` | | Generate random name (`seed-{adjective}-{noun}`) |
| `--visibility` | `-v` | Visibility: `open` (default) or `closed` |
| `--filesystem` | | Filesystem: `btrfs` (default) or `ext4` |
| `--at` | | Target stone (normative form) |
| `--format` | `-f` | Output format: `text`, `json` |
| `--yes` | `-y` | Skip confirmation prompts |

### 4.5 Release Command

Safely unmount seed bank before physical removal:

```bash
# Release specific seed bank
garden-rake release seed-bank portable-backup

# Release all seed banks on current stone
garden-rake release all seed-banks

# Release on specific stone
garden-rake release seed-bank portable-backup at stone-alpha
garden-rake release all seed-banks at stone-alpha
```

**Behavior:**
1. Sync pending writes to disk (`sync`)
2. Unmount filesystem
3. Remove fstab entry (optional, with `--permanent`)
4. Emit `storage.released` SSE event
5. Play Cricket audio cue ("seed-bank-released")

---

## 5. API Endpoints

### 5.1 Storage Namespace

All stone-local storage operations under `/api/v1/stone/storage/`:

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/stone/storage` | List all storage (seed banks + candidates) |
| GET | `/api/v1/stone/storage/candidates` | List eligible devices awaiting preparation |
| POST | `/api/v1/stone/storage/prepare` | Prepare a device as seed bank |
| PATCH | `/api/v1/stone/storage/{name}/visibility` | Change visibility (open/closed) |
| PATCH | `/api/v1/stone/storage/{name}/rename` | Rename seed bank (with pool rules) |
| POST | `/api/v1/stone/storage/{name}/release` | Safely unmount seed bank |
| POST | `/api/v1/stone/storage/release-all` | Safely unmount all seed banks |
| POST | `/api/v1/stone/storage/merge` | Merge two pools (resolve conflict) |
| DELETE | `/api/v1/stone/storage/{name}` | Remove seed bank (doesn't delete data) |

### 5.2 Prepare Endpoint

```http
POST /api/v1/stone/storage/prepare
Content-Type: application/json

{
  "device": "SANDISK_32GB",      // Label or mount path
  "name": "portable-backup",     // Optional, auto-generated if omitted
  "visibility": "open"           // Optional: "open" (default) | "closed"
}
```

**Visibility rules:**
- Unnamed seed-banks: Always `open` (cannot be closed)
- Named seed-banks: Default `open`, can be `closed` for private storage

**Response (accepted - async job):**
```json
{
  "accepted": true,
  "job_id": "01956a3e-7c00-7000-8000-000000000002",
  "message": "Preparing seed bank in background"
}
```

**Job Status Polling:**
```http
GET /api/v1/stone/jobs/{job_id}
```

Format operations run asynchronously. The API returns immediately with a `job_id`. Use the job status endpoint or SSE stream (`storage.prepare.progress`, `storage.prepared`) to track completion.

**Response (success):**
```json
{
  "success": true,
  "seed_bank": {
    "id": "01956a3e-7c00-7000-8000-000000000001",
    "name": "portable-backup",
    "visibility": "open",
    "path": "/mnt/usb",
    "device": "/dev/sdb1",
    "label": "SANDISK_32GB",
    "capacity_bytes": 32000000000,
    "created_at": "2026-01-28T10:35:00Z",
    "protocols": ["s3", "storage"]
  }
}
```

**Response (error - has data):**
```json
{
  "success": false,
  "error": "DEVICE_HAS_DATA",
  "message": "Device contains files and cannot be prepared as a seed bank",
  "device": "SANDISK_32GB",
  "file_count": 47,
  "hint": "Remove all files from the device before preparing"
}
```

**Response (error - needs partition):**
```json
{
  "success": false,
  "error": "DEVICE_UNPARTITIONED",
  "message": "Device has no partition table",
  "device": "/dev/sdb",
  "action_required": "partition_and_format",
  "hint": "Re-submit with partition_and_format: true to create partition table and format"
}
```

### 5.3 Partition and Format

When device needs partitioning:

```http
POST /api/v1/stone/storage/prepare
Content-Type: application/json

{
  "device": "/dev/sdb",
  "name": "portable-backup",
  "partition_and_format": true
}
```

This will:
1. Create GPT partition table on device
2. Create single partition (partition 1) spanning entire device
3. Format partition as **btrfs** with label `zen-seed` (compression: zstd)
4. Add entry to `/etc/fstab` for persistence (UUID-based)
5. Mount at `{data_dir}/mounts/{name}` (e.g., `/var/lib/zen-garden/mounts/portable-backup`)
6. Create seed bank structure (`.zen-garden/` directory)
7. Store device serial number in manifest (if available)

**Filesystem Choice:**
- Default: **btrfs** (snapshots, compression, checksums for backup scenarios)
- Override: `--filesystem ext4` for compatibility edge cases

**Note:** We use GPT + partition 1 (not whole-disk format) for compatibility with USB enclosures and Windows.

### 5.4 List Storage

```http
GET /api/v1/stone/storage
```

**Response:**
```json
{
  "seed_banks": [
    {
      "id": "01956a3e-7c00-7000-8000-000000000001",
      "name": "portable-backup",
      "visibility": "open",
      "path": "/mnt/usb",
      "label": "SANDISK_32GB",
      "capacity_bytes": 32000000000,
      "used_bytes": 1500000000,
      "status": "online",
      "protocols": ["s3", "storage"]
    }
  ],
  "candidates": [
    {
      "device": "/dev/sdc1",
      "mount_path": "/mnt/usb2",
      "label": "KINGSTON_64GB",
      "capacity_bytes": 64000000000,
      "state": "empty",
      "eligible": true
    }
  ]
}
```

### 5.5 Visibility Change

```http
PATCH /api/v1/stone/storage/{name}/visibility
Content-Type: application/json

{
  "visibility": "closed"
}
```

**Response:**
```json
{
  "success": true,
  "name": "portable-backup",
  "visibility": "closed",
  "sync_group": "portable-backup"
}
```

**Behavior:**
- `open` → `closed`: Stops syncing with unnamed pool, starts syncing only with same-name closed banks
- `closed` → `open`: Begins catch-up sync with unnamed pool
- Unnamed seed-banks cannot be changed to `closed` (400 error)

### 5.6 Rename Endpoint

```http
PATCH /api/v1/stone/storage/{name}/rename
Content-Type: application/json

{
  "new_name": "backup-vault"
}
```

**Response (success):**
```json
{
  "success": true,
  "old_name": "seed-bank-zengarden",
  "new_name": "backup-vault",
  "pool_id": "01956a3e-7c00-7000-8000-000000000099"
}
```

**Response (pool conflict):**
```json
{
  "success": false,
  "error": "POOL_CONFLICT",
  "message": "Target pool has different pool_id",
  "this_pool_id": "01956a3e-7c00-7000-8000-000000000001",
  "target_pool_id": "01956a3e-7c00-7000-8000-000000000099",
  "hint": "Use merge: true to combine pools, or choose a different name"
}
```

**Rename Rules:**
1. Device must be **empty** OR joining a pool with **no files** (becomes donor)
2. If target pool exists with different `pool_id`:
   - Emit `storage.pool_conflict` SSE event
   - Return error with both pool IDs
   - User must use explicit merge endpoint (see 5.8)
3. If target pool doesn't exist: new `pool_id` is generated
4. If target pool exists with same `pool_id`: seamless join

### 5.7 Release Endpoint

```http
POST /api/v1/stone/storage/{name}/release
```

**Response:**
```json
{
  "success": true,
  "name": "portable-backup",
  "message": "Seed bank released, safe to remove device"
}
```

**Release All:**
```http
POST /api/v1/stone/storage/release-all
```

**Response:**
```json
{
  "success": true,
  "released": ["portable-backup", "seed-amber-brook"],
  "count": 2
}
```

### 5.8 Merge Endpoint

Explicitly merge two pools with conflicting `pool_id`:

```http
POST /api/v1/stone/storage/merge
Content-Type: application/json

{
  "source": "7c00",           // 4-digit pool_id prefix
  "target": "7c01",           // 4-digit pool_id prefix  
  "policy": "incremental"     // incremental | wipe-target | wipe-source
}
```

**Response:**
```json
{
  "success": true,
  "merged_pool_id": "01956a3e-7c01-7000-8000-000000000099",
  "policy_applied": "incremental",
  "files_merged": 147,
  "conflicts_resolved": 3
}
```

**Merge Policies:**
| Policy | Behavior |
|--------|----------|
| `incremental` (default) | Interleave journals by GUIDv7 timestamp, LWW for conflicts |
| `wipe-target` | Delete target pool content, replace with source |
| `wipe-source` | Delete source pool content, replace with target |

### 5.9 Object Operations

Object CRUD operations on seed bank contents:

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/stone/storage/bank/:id/*path` | Get object (raw bytes) |
| PUT | `/api/v1/stone/storage/bank/:id/*path` | Create/update object |
| DELETE | `/api/v1/stone/storage/bank/:id/*path` | Delete object |
| HEAD | `/api/v1/stone/storage/bank/:id/*path` | Get object metadata |

**Query Parameters:**

| Parameter | Values | Description |
|-----------|--------|-------------|
| `depth` | `1` (default), `2`, `3`, ..., `all`, `-1` | Listing depth for directory paths |

**Examples:**

```http
# List immediate children (default depth=1)
GET /api/v1/stone/storage/bank/backup-vault/apps/myapp/

# List 3 levels deep
GET /api/v1/stone/storage/bank/backup-vault/apps/myapp/?depth=3

# Full recursive listing
GET /api/v1/stone/storage/bank/backup-vault/apps/myapp/?depth=all
GET /api/v1/stone/storage/bank/backup-vault/apps/myapp/?depth=-1  # Unix convention
```

**Depth Behavior:**

| Value | Behavior |
|-------|----------|
| `1` | Immediate children only (default) |
| `2` | Children and grandchildren |
| `N` | N levels of subdirectories |
| `all` or `-1` | Full recursive listing |

**Response (directory listing):**
```json
{
  "path": "apps/myapp/",
  "entries": [
    {"name": "config.json", "type": "file", "size": 1024, "modified": "2026-01-28T10:30:00Z"},
    {"name": "data/", "type": "dir"},
    {"name": "data/users.db", "type": "file", "size": 51200, "modified": "2026-01-28T09:15:00Z"}
  ],
  "truncated": false
}
```

---

## 6. Seed Bank Structure

### 6.1 Directory Layout

Seed banks are mounted under `{data_dir}/mounts/`:
- Linux: `/var/lib/zen-garden/mounts/{name}/`
- Development: `.zen-garden/mounts/{name}/`

```
{data_dir}/mounts/{name}/.zen-garden/
├── manifest.yaml           # Seed bank metadata
├── journal/                # Sync journal (GUIDv7-based)
│   ├── head                # Current journal head pointer
│   └── *.json              # Journal entry batches
├── garden/                 # Cultivation namespace
│   ├── index.yaml         # Backup index
│   ├── offerings/         # Offering backups
│   │   └── {offering_id}/
│   │       └── {timestamp}/
│   │           ├── manifest.yaml
│   │           └── data.archive.gz
│   └── stones/            # Stone identity backups
│       └── {stone_id}/
│           └── identity.yaml
└── apps/                   # S3 namespace (app storage)
    └── {app_name}/
        └── ...
```

### 6.2 Manifest File

```yaml
# /mnt/usb/.zen-garden/manifest.yaml
version: 1
id: 01956a3e-7c00-7000-8000-000000000001  # GUIDv7 (immutable identity)
pool_id: 01956a3e-7c00-7000-8000-000000000099  # Sync group identity
name: portable-backup
visibility: open   # open | closed
filesystem: btrfs  # btrfs | ext4
device_serial: "WD-WMC1T0123456"  # For future cryptographic signing
created_at: 2026-01-28T10:35:00Z
created_by:
  stone: stone-golden-delta
  moss_version: 0.2.202601281035
protocols:
  - s3
  - storage
```

**Identity:**
- `id` is generated once at creation and never changes
- `id` is authoritative for federation sync (not label or name)
- If device is renamed (`name` changes), `id` remains stable
- `pool_id` identifies the sync group; assigned when first device joins a named pool
- `device_serial` stored for future cryptographic pond signatures

---

## 7. Persistence and Re-Discovery

### 7.1 Mount Persistence

Prepared seed banks use label-based auto-mount on scan:

**Filesystem Label (`zen-seed`):**

During preparation, the filesystem is labeled `zen-seed`:
```bash
mkfs.ext4 -L zen-seed /dev/sdb1
# or
mkfs.btrfs -L zen-seed /dev/sdb1
```

**Auto-Mount on Scan:**

`SeedBankRegistry::scan()` automatically mounts unmounted devices with the `zen-seed` label:

```rust
// In registry.rs
pub async fn scan() -> Result<Self> {
    // Auto-mount any unmounted seed banks before scanning
    Self::auto_mount_seed_banks().await?;
    // ... then scan mounts directory for manifests
}

async fn auto_mount_seed_banks() -> Result<()> {
    // 1. Run: lsblk -rno NAME,LABEL,MOUNTPOINT
    // 2. Find devices with label "zen-seed" that have no mountpoint
    // 3. Check device is removable (skip internal drives)
    // 4. Mount to {data_dir}/mounts/{seed-bank-name}
}
```

This approach is preferred over fstab because:
- No stale entries when device is absent
- Works across reboots without configuration
- Handles roaming devices (plugged into different stones)
- Simpler than UUID-based fstab management

**Mount Directory:**
- Linux: `/var/lib/zen-garden/mounts/{name}/`
- Development: `.zen-garden/mounts/{name}/`

### 7.2 Roaming Seed Banks

When a prepared seed bank is plugged into a different stone:
1. Moss detects `.zen-garden/` directory → treats as `prepared` state
2. Reads `manifest.yaml` to get identity (GUIDv7 `id`)
3. Registers locally with original name
4. Begins sync with other seed banks in same sync group

---

## 8. Safety Rails

### 7.1 Path Validation

```rust
const ALLOWED_PREFIXES: &[&str] = &[
    "/mnt/",
    "/media/",
    "/run/media/",
];

fn validate_mount_path(path: &Path) -> Result<(), StorageError> {
    let path_str = path.to_string_lossy();
    
    // Must be under allowed prefix
    if !ALLOWED_PREFIXES.iter().any(|p| path_str.starts_with(p)) {
        return Err(StorageError::PathNotAllowed(path.to_path_buf()));
    }
    
    // Must be a mount point (not subdirectory)
    if !is_mount_point(path)? {
        return Err(StorageError::NotMountPoint(path.to_path_buf()));
    }
    
    Ok(())
}
```

### 7.2 Empty Check

```rust
fn is_device_empty(path: &Path) -> Result<bool, StorageError> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        
        // Ignore system-created hidden files
        if name_str == ".Trash-1000" || 
           name_str == ".Spotlight-V100" ||
           name_str == ".fseventsd" ||
           name_str == "System Volume Information" ||
           name_str == "$RECYCLE.BIN" {
            continue;
        }
        
        // Check for existing seed bank
        if name_str == ".zen-garden" {
            return Err(StorageError::AlreadyPrepared(path.to_path_buf()));
        }
        
        // Any other file = not empty
        return Ok(false);
    }
    Ok(true)
}
```

### 7.3 Removable Check

```rust
fn is_removable(device: &Path) -> Result<bool, StorageError> {
    // Check /sys/block/{device}/removable
    let device_name = device.file_name()
        .ok_or(StorageError::InvalidDevice)?;
    
    let removable_path = Path::new("/sys/block")
        .join(device_name)
        .join("removable");
    
    let content = std::fs::read_to_string(&removable_path)?;
    Ok(content.trim() == "1")
}
```

---

## 9. Implementation Plan

### Phase 1: Core Infrastructure
- [ ] Add storage event types to `event_types.rs`
- [ ] Create `StorageDetectedInfo` and `SeedBankInfo` types in `garden_common`
- [ ] Implement `print_storage_detected_ribbon()` in `tty.rs`

### Phase 2: Moss Detection
- [ ] Create `src/moss/src/infra/storage/` module
- [ ] Implement udev monitoring for USB storage (via `udev` crate)
- [ ] Implement device eligibility checks
- [ ] Check kernel version for btrfs support, fallback to ext4
- [ ] Wire up SSE event emission

### Phase 3: API Endpoints
- [ ] `GET /api/v1/stone/storage`
- [ ] `GET /api/v1/stone/storage/candidates`
- [ ] `POST /api/v1/stone/storage/prepare`
- [ ] `PATCH /api/v1/stone/storage/{name}/visibility`
- [ ] `PATCH /api/v1/stone/storage/{name}/rename`
- [ ] `POST /api/v1/stone/storage/{name}/release`
- [ ] `POST /api/v1/stone/storage/release-all`
- [ ] `DELETE /api/v1/stone/storage/{name}`

### Phase 4: Rake Commands
- [ ] `garden-rake prepare seed-bank` (koan + normative)
- [ ] `garden-rake release seed-bank`
- [ ] Add to command manifest

### Phase 5: Integration
- [ ] Cricket audio feedback for storage events
- [ ] S3 gateway integration with seed banks
- [ ] Journal implementation (see below)
- [ ] Documentation updates

---

## 10. Journal & Sync (Summary)

> **Full specification:** See STORAGE-0002-seed-bank-federation.md

### 10.1 Discovery

Seed banks don't discover each other—the **Moss instances** managing them do via the Storage Beacon protocol (STORAGE-0003).

**Event-Driven Announcements:**

Moss broadcasts a `STORAGE_BEACON` on:
- Seed bank mount (USB insert, boot detection)
- Seed bank unmount (release, removal)
- Visibility change (open ↔ closed)
- When a new stone joins the garden (all storage-having stones beacon)

**Beacon Structure:**
```rust
StorageBeacon {
    stone_id: String,           // Links to TopologyEntry
    stone_name: String,
    endpoint: String,           // HTTP endpoint
    seed_banks: Vec<SeedBankAnnouncement>,
    timestamp: DateTime<Utc>,
}
```

**Cache Design:**
- Separate `StorageCache` references `TopologyCache` by `stone_id`
- All stones lurk-listen and update their cache on beacon receipt
- Cache entries expire when stone goes offline (topology-driven)

**New Stone Flow:**
1. New stone broadcasts `STONE_CHIRP`
2. All stones with seed banks hear the chirp
3. They each broadcast `STORAGE_BEACON`
4. New stone's `StorageCache` is fully populated within seconds

See [STORAGE-0003](../decisions/STORAGE-0003-beacon-protocol.md) for full protocol details.

### 10.2 Journal Format

**Location:** `.zen-garden/journal/`

```
.zen-garden/journal/
├── head                    # Current journal head (GUIDv7)
├── 01956a3e-7c00-7000.json # Journal entries (batched by prefix)
└── ...
```

**Entry:**
```json
{
  "id": "01956a3e-7c00-7000-8000-000000000001",
  "timestamp": "2026-01-28T10:30:00.000Z",
  "origin_stone": "stone-golden-delta",
  "op": "put",
  "path": "apps/myapp/config.json",
  "checksum": "sha256:abc123...",
  "size": 1024,
  "prev": "01956a3e-7bff-7000-8000-000000000000"
}
```

**Operations:** `put`, `delete`, `snapshot`, `merge`

### 10.3 Conflict Resolution

**Same-key concurrent writes:** Last-write-wins (LWW) based on GUIDv7 ordering.
- GUIDv7 encodes timestamp → later wins
- Same timestamp → higher random suffix wins (deterministic, no coordination)

### 10.4 Pool Merge

When pools have conflicting `pool_id`, user must explicitly merge:

```bash
# Show conflict with 4-digit pool prefixes
garden-rake merge seed-bank 7c00 to 7c01

# Merge policies
garden-rake merge seed-bank 7c00 to 7c01 --policy incremental  # default
garden-rake merge seed-bank 7c00 to 7c01 --policy wipe-target  # wipe 7c01
garden-rake merge seed-bank 7c00 to 7c01 --policy wipe-source  # wipe 7c00
```

| Policy | Behavior |
|--------|----------|
| `incremental` (default) | Interleave journals by GUIDv7, LWW for conflicts |
| `wipe-target` | Delete target pool content, copy source |
| `wipe-source` | Delete source pool content, copy target |

### 10.5 Read-Only Seed Banks

Seed banks with write-protect enabled:
- Detected via mount options or write test
- Marked `status: "read-only"` in registry
- **Can serve reads** but cannot update journal
- **If pooled:** Read-only bank is demoted; only R/W banks are active
- SSE event: `storage.readonly_detected`

---

## 11. Future Considerations

### 11.1 Encryption

Seed bank encryption via pool-derived keys (LUKS or similar). Key derivation from pool master secret, stored in stone TPM or user-provided passphrase.

**Status:** Planned for future release.

### 11.2 USB Hub Warnings

Detection and warning for multiple seed banks on same USB hub (power limitations).

**Status:** Low priority, may implement based on user feedback.

---

## References

- [Storage Capability Spec](../proposals/ongoing/zen-garden-storage-capability-spec.md)
- [Service Resolution Spec](../proposals/ongoing/zen-garden-service-resolution-spec.md)
- [PRESENCE-0001](../PRESENCE-0001-COMPLETE.md) - SSE event architecture
- [STORAGE-0002](../decisions/STORAGE-0002-api-structure.md) - API structure decision
- [STORAGE-0003](../decisions/STORAGE-0003-beacon-protocol.md) - Storage beacon protocol
- STORAGE-0002-seed-bank-federation.md - Federation protocol (TBD)

---

**End of Specification**
