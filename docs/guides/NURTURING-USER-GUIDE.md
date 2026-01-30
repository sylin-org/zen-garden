# Zen Garden Nurturing User Guide

Reference guide for environment setup, backup policies, and service recovery.

> **Version**: 0.1 (based on codebase as of 2026-01-30)
> **Scope**: Local A/B backups, seed bank replication, and restore operations

---

## Table of Contents

1. [Environment Setup](#1-environment-setup)
2. [Seed Bank Preparation](#2-seed-bank-preparation)
3. [Policy Configuration](#3-policy-configuration)
4. [Manual Backup Operations](#4-manual-backup-operations)
5. [Scheduled Nurturing](#5-scheduled-nurturing)
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
  ├── ceremonies/          # Nourishment journals
  ├── staging/             # Deployment packages
  ├── mounts/              # Seed bank mount points
  └── nurturing/           # A/B slot index
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
6. Manifest written to `.zen-garden/manifest.yaml`

### Seed Bank Manifest Structure

After preparation, the seed bank contains:
```
{mount_path}/.zen-garden/
├── manifest.yaml           # Identity and metadata
├── garden/
│   ├── index.yaml          # Backup index
│   └── offerings/          # Offering snapshots
└── journal/                # Sync history
```

**Manifest fields:**
```yaml
version: 1
id: "01956a3e-7c00-7000-8000-..."    # Immutable GUIDv7
pool_id: "01956a3e-7c00-7000-..."    # Sync group (for multi-bank pools)
name: "portable-backup"
visibility: "open"                    # open or closed
filesystem: "btrfs"                   # btrfs or ext4
created_at: "2026-01-28T10:35:00Z"
created_by:
  stone: "stone-coral-prairie"
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

The nurturing scheduler uses these defaults:
```rust
NurturingWorkflowConfig {
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
curl -X POST http://localhost:7185/api/v1/stone/nurturing/{offering} \
  -H "Content-Type: application/json" \
  -d '{"commit_image": true}'
```

Response includes:
- `slot`: Which slot was used (A or B)
- `harvest_id`: Unique snapshot identifier
- `size_bytes`: Snapshot size

### List Local Snapshots

```bash
curl http://localhost:7185/api/v1/stone/nurturing/{offering}
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
curl -X POST http://localhost:7185/api/v1/stone/nurturing/{offering}/replicate \
  -H "Content-Type: application/json" \
  -d '{"seed_bank": "portable-backup"}'
```

### List Remote Snapshots

```bash
curl http://localhost:7185/api/v1/stone/nurturing/remote/{seed_bank_name}
```

---

## 5. Scheduled Nurturing

### Timer Integration

The nurturing scheduler is triggered by system timers calling HTTP endpoints:

| Endpoint | Purpose |
|----------|---------|
| `POST /api/v1/nurturing/{offering}/trigger` | Trigger single offering |
| `POST /api/v1/nurturing/trigger-all` | Trigger all running offerings |

### Workflow Execution

When triggered, the scheduler executes:

1. **Harvest**: Create local A/B snapshot
2. **Route**: Find available seed banks using routing strategy
3. **Replicate**: Copy to seed bank(s) with failover
4. **Prune**: Remove excess remote snapshots (retention policy)

### Setting Up Timers

**Linux (systemd):**

Create a timer unit `/etc/systemd/system/zen-nurturing-{offering}.timer`:
```ini
[Unit]
Description=Zen Garden Nurturing Timer for {offering}

[Timer]
OnCalendar=daily
RandomizedDelaySec=1800
Persistent=true

[Install]
WantedBy=timers.target
```

Create a service unit `/etc/systemd/system/zen-nurturing-{offering}.service`:
```ini
[Unit]
Description=Zen Garden Nurturing Trigger for {offering}

[Service]
Type=oneshot
ExecStart=/usr/bin/curl -X POST http://localhost:7185/api/v1/nurturing/{offering}/trigger
```

Enable:
```bash
systemctl daemon-reload
systemctl enable --now zen-nurturing-{offering}.timer
```

**Windows (Task Scheduler):**

Create a scheduled task that runs:
```powershell
Invoke-WebRequest -Method POST -Uri "http://localhost:7185/api/v1/nurturing/{offering}/trigger"
```

**Gap:** No CLI command to create/manage timers automatically.

---

## 6. Restore Operations

### Restore from Local A/B Slot

```bash
curl -X POST http://localhost:7185/api/v1/stone/nurturing/{offering}/restore \
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
curl -X POST http://localhost:7185/api/v1/stone/nurturing/{offering}/restore-remote \
  -H "Content-Type: application/json" \
  -d '{"seed_bank": "portable-backup", "harvest_id": null}'
```

If `harvest_id` is omitted, restores from the latest snapshot on that seed bank.

### Disaster Recovery Procedure

**Scenario:** Stone lost, need to recover services to a new stone.

1. **Identify available backups:**
   ```bash
   # Mount seed bank on new stone
   mount /dev/sdb1 /var/lib/zen-garden/mounts/portable-backup

   # List offerings with snapshots
   curl http://localhost:7185/api/v1/stone/nurturing/remote/portable-backup
   ```

2. **Install the offering:**
   ```bash
   garden-rake install {offering}
   ```

3. **Restore from seed bank:**
   ```bash
   curl -X POST http://localhost:7185/api/v1/stone/nurturing/{offering}/restore-remote \
     -d '{"seed_bank": "portable-backup"}'
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
| Backup to cloud (S3) | S3 gateway exists but not integrated with nurturing |

### Recommendations for Future Work

1. **Add Rake restore commands:**
   ```bash
   garden-rake restore {offering} from slot A
   garden-rake restore {offering} from seed-bank portable-backup
   ```

2. **Add timer management to Rake:**
   ```bash
   garden-rake schedule {offering} every 24h
   garden-rake schedule list
   ```

3. **Make retention configurable:**
   ```bash
   garden-rake configure nurturing --retention 10
   ```

4. **Add backup status command:**
   ```bash
   garden-rake status nurturing
   # Shows all offerings, last backup times, replication status
   ```

---

## API Reference Summary

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/stone/nurturing` | List all offerings with slots |
| GET | `/api/v1/stone/nurturing/{offering}` | Get slots for offering |
| POST | `/api/v1/stone/nurturing/{offering}` | Create snapshot |
| POST | `/api/v1/stone/nurturing/{offering}/restore` | Restore from local slot |
| DELETE | `/api/v1/stone/nurturing/{offering}` | Delete all snapshots |
| POST | `/api/v1/stone/nurturing/{offering}/replicate` | Replicate to seed bank |
| GET | `/api/v1/stone/nurturing/remote/{seed_bank}` | List remote snapshots |
| POST | `/api/v1/stone/nurturing/{offering}/restore-remote` | Restore from seed bank |
| POST | `/api/v1/nurturing/{offering}/trigger` | Trigger full workflow |
| POST | `/api/v1/nurturing/trigger-all` | Trigger all offerings |

---

*Document generated from codebase analysis. Features described are based on actual implementation, not design documents.*
