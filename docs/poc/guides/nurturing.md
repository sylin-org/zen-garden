# Zen Garden Backup User Guide

Reference guide for environment setup, backup policies, and service recovery.

> **Version**: 0.2 (updated 2026-03-22 for ARCH-0006 API renames)
> **Scope**: Local A/B backups, seed bank replication, and restore operations

---

## Table of Contents

1. [Environment Setup](#1-environment-setup)
2. [Seed Bank Preparation](#2-seed-bank-preparation)
3. [Policy Configuration](#3-policy-configuration)
4. [Manual Backup Operations](#4-manual-backup-operations)
5. [Scheduled Backups](#5-scheduled-backups)
6. [Restore Operations](#6-restore-operations)
7. [Known Gaps and Limitations](#7-known-gaps-and-limitations)

---

## 1. Environment Setup

### System Requirements

| Platform | Supported | Notes |
|----------|-----------|-------|
| Linux (x86_64) | Primary | Tested on Debian, Ubuntu |
| Windows (x86_64) | Secondary | Requires Docker Desktop |

**Required Software:**
- Docker daemon (must be running and accessible)
- Network access to port 7185 (Moss API)
- UDP port 7184 for stone discovery

### Directory Structure

**Linux:**
```
/etc/zen-garden/           # Configuration
  └── garden-moss.toml     # Main daemon config

/var/lib/zen-garden/       # Data directory
  ├── harvests/            # Backup artifacts
  ├── stored/              # Portable backups
  ├── ceremonies/          # Update journals
  ├── staging/             # Deployment packages
  ├── mounts/              # Seed bank mount points
  └── snapshots/           # A/B slot index
```

**Windows:**
```
.zen-garden/               # Config and data (relative to working directory)
```

### Stone Configuration

Main configuration file: `/etc/zen-garden/garden-moss.toml`

```toml
[stone]
name = "stone-coral-prairie"   # Unique stone identifier
http_port = 7185               # API port
log_level = "info"             # Logging verbosity

[docker]
retry_delay_ms = 1000          # Container operation retry delay

[health]
check_interval_secs = 30       # Health check frequency
```

### Verifying Installation

```bash
# Check stone health
curl http://localhost:7185/api/v1/stone/health

# List discovered stones (from any stone)
curl http://localhost:7185/api/v1/garden

# Check running services
curl http://localhost:7185/api/v1/stone/services
```

---

## 2. Seed Bank Preparation

Seed banks are external storage devices (USB drives, external HDDs) used for off-stone backup replication.

### Prerequisites

- Removable block device (USB drive, external HDD)
- Device should be empty or prepared for reformatting
- Device will be reformatted during preparation (all data erased)

### Prepare a Seed Bank

Using Rake CLI (koan syntax):
```bash
garden-rake prepare seed-bank /dev/sdb named portable-backup
```

Using normative syntax:
```bash
garden-rake prepare seed-bank --device /dev/sdb --name portable-backup
```

**What happens:**
1. Device eligibility verified (removable, writable, not already prepared)
2. GPT partition table created
3. Single partition formatted as btrfs (with zstd compression)
4. Fstab entry added (Linux)
5. Device mounted to `/var/lib/zen-garden/mounts/{name}/`
6. Manifest written to `.zen-garden/manifest.json`

### Seed Bank Manifest Structure

After preparation, the seed bank contains:
```
{mount_path}/.zen-garden/
├── manifest.json           # Identity and metadata
└── garden/
    ├── snapshots/          # Backup snapshots
    │   ├── index.json       # Remote backup index
    │   └── {offering_id}/
    │       ├── offering.json
    │       └── {harvest_id}.tar.gz
    └── storage/            # S3/REST storage root
```

**Manifest fields:**
```json
{
  "version": 2,
  "id": "01956a3e-7c00-7000-8000-000000000001",
  "pool_id": "0195",
  "name": "portable-backup",
  "visibility": "open",
  "filesystem": "btrfs",
  "origin_stone": "stone-coral-prairie",
  "created_at": "2026-01-28T10:35:00Z"
}
```

### Release a Seed Bank

```bash
garden-rake release seed-bank portable-backup
```

This unmounts the device and removes the fstab entry.

### Auto-Mount Behavior

Seed banks with the filesystem label `zen-seed` are automatically mounted when:
- The Moss daemon starts
- The stone scans for available seed banks

Devices are mounted to `/var/lib/zen-garden/mounts/{seed-bank-name}/`.

---

## 3. Policy Configuration

### Retention Policies

**Local A/B Slots:**
- 2 slots per offering (slot A and slot B)
- Automatic rotation: newest snapshot goes to opposite slot
- No configurable retention limit (always 2)

**Remote Snapshots (Seed Banks):**
- Default: 5 snapshots per offering per seed bank
- Enforced during replication (excess pruned automatically)
- Defined in code: `DEFAULT_RETENTION_SLOTS = 5`

### Routing Strategies

When replicating to seed banks, three strategies are available:

| Strategy | Behavior |
|----------|----------|
| `First` | Use first available seed bank (default) |
| `MostCapacity` | Route to seed bank with most free space |
| `All` | Replicate to all available seed banks |

**Current limitation:** Routing strategy is not configurable via CLI or API. It defaults to `First` in the scheduler.

### Workflow Configuration

The backup scheduler uses these defaults:
```rust
SnapshotWorkflowConfig {
    commit_image: true,              // Commit container image during snapshot
    routing_strategy: First,         // Use first available seed bank
    max_replication_attempts: 3,     // Retry failed replications
    continue_on_local_failure: false // Stop workflow if local fails
}
```

**Gap:** These values cannot be modified at runtime.

---

## 4. Manual Backup Operations

### Create a Local Snapshot

```bash
# Via API
curl -X POST http://localhost:7185/api/v1/stone/snapshots/{offering} \
  -H "Content-Type: application/json" \
  -d '{"commit_image": true}'
```

Response includes:
- `slot`: Which slot was used (A or B)
- `harvest_id`: Unique snapshot identifier
- `size_bytes`: Snapshot size

### List Local Snapshots

```bash
curl http://localhost:7185/api/v1/stone/snapshots/{offering}
```

Response shows both slots:
```json
{
  "data": {
    "offering_id": "019c0cc3-...",
    "offering_name": "mongodb",
    "slot_a": {
      "harvest_id": "mongodb-20260130T060021-bae3",
      "created_at": "2026-01-30T06:00:21Z",
      "size_bytes": 52428800
    },
    "slot_b": null
  }
}
```

### Replicate to Seed Bank

```bash
curl -X POST http://localhost:7185/api/v1/stone/snapshots/{offering}/replicate \
  -H "Content-Type: application/json" \
  -d '{"storage": "portable-backup"}'
```

### List Remote Snapshots

```bash
curl http://localhost:7185/api/v1/stone/snapshots/remote/{seed_bank_name}
```

### Hydration Access (Garden Storage Snapshots API)

External orchestrators can read seed bank backups through the **read-only** garden storage
snapshots API. Requests are automatically routed to the stone that hosts the selected seed
bank, and access is **audited** (no auth gating yet).

```bash
# List all remote snapshots on a storage (index)
curl http://localhost:7185/api/v1/garden/storage/{name}/snapshots

# List snapshots for a specific offering
curl http://localhost:7185/api/v1/garden/storage/{name}/snapshots/{offering_id}

# Get hydration metadata (offering.json)
curl http://localhost:7185/api/v1/garden/storage/{name}/snapshots/{offering_id}/manifest

# Download snapshot tarball
curl -o snapshot.tar.gz \
  http://localhost:7185/api/v1/garden/storage/{name}/snapshots/{offering_id}/{harvest_id}
```

---

## 5. Scheduled Backups

### Timer Integration

The backup scheduler is triggered by system timers calling HTTP endpoints:

| Endpoint | Purpose |
|----------|---------|
| `POST /api/v1/snapshots/{offering}/trigger` | Trigger single offering |
| `POST /api/v1/snapshots/trigger-all` | Trigger all running offerings |

### Workflow Execution

When triggered, the scheduler executes:

1. **Harvest**: Create local A/B snapshot
2. **Route**: Find available seed banks using routing strategy
3. **Replicate**: Copy to seed bank(s) with failover
4. **Prune**: Remove excess remote snapshots (retention policy)

### Setting Up Timers

**Linux (systemd):**

Create a timer unit `/etc/systemd/system/zen-backup-{offering}.timer`:
```ini
[Unit]
Description=Zen Garden Backup Timer for {offering}

[Timer]
OnCalendar=daily
RandomizedDelaySec=1800
Persistent=true

[Install]
WantedBy=timers.target
```

Create a service unit `/etc/systemd/system/zen-backup-{offering}.service`:
```ini
[Unit]
Description=Zen Garden Backup Trigger for {offering}

[Service]
Type=oneshot
ExecStart=/usr/bin/curl -X POST http://localhost:7185/api/v1/snapshots/{offering}/trigger
```

Enable:
```bash
systemctl daemon-reload
systemctl enable --now zen-backup-{offering}.timer
```

**Windows (Task Scheduler):**

Create a scheduled task that runs:
```powershell
Invoke-WebRequest -Method POST -Uri "http://localhost:7185/api/v1/snapshots/{offering}/trigger"
```

**Gap:** No CLI command to create/manage timers automatically.

---

## 6. Restore Operations

### Restore from Local A/B Slot

```bash
curl -X POST http://localhost:7185/api/v1/stone/snapshots/{offering}/restore \
  -H "Content-Type: application/json" \
  -d '{"slot": "A"}'
```

If `slot` is omitted, restores from the most recent slot.

**What happens:**
1. Offering container stopped
2. Volumes restored from snapshot archive
3. Container image restored (if committed)
4. Offering container started

### Restore from Seed Bank

```bash
curl -X POST http://localhost:7185/api/v1/stone/snapshots/{offering}/restore-remote \
  -H "Content-Type: application/json" \
  -d '{"storage": "portable-backup", "harvest_id": null}'
```

If `harvest_id` is omitted, restores from the latest snapshot on that seed bank.

### Disaster Recovery Procedure

**Scenario:** Stone lost, need to recover services to a new stone.

1. **Identify available backups:**
   ```bash
   # Mount seed bank on new stone
   mount /dev/sdb1 /var/lib/zen-garden/mounts/portable-backup

   # List offerings with snapshots
   curl http://localhost:7185/api/v1/stone/snapshots/remote/portable-backup
   ```

2. **Install the offering:**
   ```bash
   garden-rake install {offering}
   ```

3. **Restore from seed bank:**
   ```bash
   curl -X POST http://localhost:7185/api/v1/stone/snapshots/{offering}/restore-remote \
     -d '{"storage": "portable-backup"}'
   ```

**Gap:** No CLI command for restore operations. Must use API directly.

---

## 7. Known Gaps and Limitations

### Process Gaps

| Gap | Impact | Workaround |
|-----|--------|------------|
| No Rake CLI for restore | Requires curl/API calls | Use API endpoints directly |
| No timer management CLI | Manual systemd/Task Scheduler setup | Create units/tasks manually |
| Policies not configurable | Cannot adjust retention at runtime | Modify code constants |
| No scheduled retention cleanup | Old local snapshots accumulate | Manual cleanup required |

### UX Gaps

| Gap | Description |
|-----|-------------|
| No backup status dashboard | Cannot view all backups across stones in one place |
| No restore dry-run | Cannot preview what will be restored |
| No backup verification | No integrity check after replication |
| No notifications | No alerts when backups fail or succeed |

### Use Case Gaps

| Use Case | Status |
|----------|--------|
| Cross-stone recovery | Not supported - must restore to originating stone or reinstall |
| Partial restore | Not supported - all-or-nothing per offering |
| Incremental backups | Not supported - full snapshots only |
| Encrypted backups | Planned for future release |
| Backup to cloud (S3) | S3 gateway exists but not integrated with backup |

### Recommendations for Future Work

1. **Add Rake restore commands:**
   ```bash
   garden-rake backup restore {offering} from slot A
   garden-rake backup restore {offering} from seed-bank portable-backup
   ```

2. **Add timer management to Rake:**
   ```bash
   garden-rake backup schedule {offering} every 24h
   garden-rake backup schedule list
   ```

3. **Make retention configurable:**
   ```bash
   garden-rake backup configure --retention 10
   ```

4. **Add backup status command:**
   ```bash
   garden-rake backup status
   # Shows all offerings, last backup times, replication status
   ```

---

## API Reference Summary

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/stone/snapshots` | List all offerings with slots |
| GET | `/api/v1/stone/snapshots/{offering}` | Get slots for offering |
| POST | `/api/v1/stone/snapshots/{offering}` | Create snapshot |
| POST | `/api/v1/stone/snapshots/{offering}/restore` | Restore from local slot |
| DELETE | `/api/v1/stone/snapshots/{offering}` | Delete all snapshots |
| POST | `/api/v1/stone/snapshots/{offering}/replicate` | Replicate to seed bank |
| GET | `/api/v1/stone/snapshots/remote/{seed_bank}` | List remote snapshots |
| POST | `/api/v1/stone/snapshots/{offering}/restore-remote` | Restore from seed bank |
| POST | `/api/v1/snapshots/{offering}/trigger` | Trigger full workflow |
| POST | `/api/v1/snapshots/trigger-all` | Trigger all offerings |

---

*Document generated from codebase analysis. Features described are based on actual implementation, not design documents.*
