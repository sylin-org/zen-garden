---
audience: [operator, visitor]
doc_type: guide
status: current
last_verified: 2026-01-30
canonical: true
note: "Authoritative guide for seed bank setup and management."
---

# Seed Banks Guide

**External storage for nurturing (backups) with replication support.**

---

## Overview

Seed banks are external storage devices (USB drives, portable SSDs) that store backup data from your offerings. zen-garden supports:

- **Multiple named seed banks** - Different purposes (primary, offsite, archive)
- **Replicated seed banks** - Two or more devices forming one logical backup target
- **Manifest-first discovery** - No filesystem labels required, just plug in and go
- **User-controlled targeting** - Choose which seed bank receives backups

---

## Quick Start

### Prepare Your First Seed Bank

```bash
# List available USB devices
garden-rake storage devices

# Prepare a simple seed bank (formats device, creates manifest)
garden-rake storage prepare /dev/sdb --name my-backup

# Verify seed bank detected
garden-rake storage list
```

The seed bank is now ready to receive backups from your offerings.

---

## Concepts

### Manifest-First Discovery

Unlike traditional approaches that rely on filesystem labels, zen-garden uses a **manifest-first** discovery model:

1. **Plug in any USB device** - No special label required
2. **moss scans automatically** - Checks for `.zen-garden/manifest.json`
3. **Mount path derived from manifest** - Supports groups and replicas
4. **Tracked for persistence** - Remounts automatically after disconnection

The manifest file (`.zen-garden/manifest.json`) is the sole source of truth for:
- Seed bank identity (unique ID)
- Name and group membership
- Replica configuration
- Origin stone and creation metadata

### Mount Path Derivation

Where your seed bank mounts depends on its configuration:

| Configuration | Mount Path |
|--------------|------------|
| Simple: `name: "my-backup"` | `/var/lib/zen-garden/mounts/my-backup` |
| Grouped: `group: "offsite"` | `/var/lib/zen-garden/mounts/offsite` |
| Replicated: `group: "primary", replica_id: 1` | `/var/lib/zen-garden/mounts/primary/replica-1` |
| Replicated: `group: "primary", replica_id: 2` | `/var/lib/zen-garden/mounts/primary/replica-2` |

### Replication

For critical data, you can create **replicated seed banks** - multiple physical devices that form one logical backup target:

```
                    ┌─────────────────┐
                    │  Logical Group  │
                    │   "primary"     │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
        ┌─────┴─────┐  ┌─────┴─────┐  ┌─────┴─────┐
        │ Replica 1 │  │ Replica 2 │  │ Replica 3 │
        │  USB-A    │  │  USB-B    │  │  USB-C    │
        └───────────┘  └───────────┘  └───────────┘
```

When nurturing writes to the "primary" group, data goes to all online replicas.

---

## CLI Reference

### List Devices

View all USB storage devices (before preparation):

```bash
garden-rake storage devices

# Output:
# Device          Size      Label         Mounted
# /dev/sdb       500 GB    (none)        No
# /dev/sdc       1.0 TB    BACKUP        No
# /dev/sdd       256 GB    zen-seed      Yes
```

### Prepare Seed Bank

Format and initialize a device as a seed bank:

```bash
# Simple seed bank with custom name
garden-rake storage prepare /dev/sdb --name my-backup

# Simple seed bank with random name
garden-rake storage prepare /dev/sdb --random

# Specify filesystem (default: ext4)
garden-rake storage prepare /dev/sdb --name my-backup --fs btrfs

# Create a grouped seed bank (for logical organization)
garden-rake storage prepare /dev/sdb --name offsite-backup --group offsite

# Create a replicated seed bank (first replica)
garden-rake storage prepare /dev/sdb --group primary --replica 1

# Create second replica of same group
garden-rake storage prepare /dev/sdc --group primary --replica 2
```

**Flags:**

| Flag | Description |
|------|-------------|
| `--name <name>` | Human-readable seed bank name |
| `--random` | Generate a random name |
| `--fs <type>` | Filesystem type (ext4, btrfs, xfs) |
| `--group <name>` | Logical group for organization or replication |
| `--replica <id>` | Replica number within group (1, 2, 3, ...) |

### List Seed Banks

View all connected seed banks:

```bash
garden-rake storage list

# Output:
# Seed Bank        Group      Replica  Device      Mount Path                              Size      Used
# my-backup        -          -        /dev/sdb    /var/lib/zen-garden/mounts/my-backup   500 GB    120 GB
# primary          primary    1        /dev/sdc    /var/lib/zen-garden/mounts/primary/replica-1  1.0 TB    450 GB
# primary          primary    2        /dev/sdd    /var/lib/zen-garden/mounts/primary/replica-2  1.0 TB    448 GB
```

### Seed Bank Info

View detailed information about a seed bank:

```bash
garden-rake storage info my-backup

# Output:
# Name: my-backup
# ID: 01948abc-1234-5678-9abc-def012345678
# Group: (none)
# Replica: (none)
# Device: /dev/sdb
# Mount: /var/lib/zen-garden/mounts/my-backup
# Filesystem: ext4
# Capacity: 500 GB
# Used: 120 GB (24%)
# Origin: stone-coral-prairie
# Created: 2026-01-30T12:00:00Z
```

---

## Setup Scenarios

### Scenario 1: Single Backup Drive

For home use with one backup drive:

```bash
# Prepare the drive
garden-rake storage prepare /dev/sdb --name home-backup

# Configure nurturing to use it
garden-rake nurturing configure immich --seed-bank home-backup
```

### Scenario 2: Rotating Offsite Backups

Keep one drive at home, take one offsite:

```bash
# Prepare two drives in the same group
garden-rake storage prepare /dev/sdb --group offsite --replica 1
garden-rake storage prepare /dev/sdc --group offsite --replica 2

# Configure nurturing with rotation
garden-rake nurturing configure immich --seed-bank offsite --strategy any_replica
```

With `any_replica` strategy, nurturing writes to whichever drive is connected. Swap drives weekly for offsite rotation.

### Scenario 3: Maximum Durability

For critical data, write to multiple drives simultaneously:

```bash
# Prepare three replicas
garden-rake storage prepare /dev/sdb --group critical --replica 1
garden-rake storage prepare /dev/sdc --group critical --replica 2
garden-rake storage prepare /dev/sdd --group critical --replica 3

# Configure nurturing to write to all
garden-rake nurturing configure postgres --seed-bank critical --strategy all_replicas
```

With `all_replicas` strategy, data is written to all connected drives. Even if one fails, you have redundant copies.

### Scenario 4: Purpose-Specific Seed Banks

Different seed banks for different offerings:

```bash
# Prepare purpose-specific drives
garden-rake storage prepare /dev/sdb --name photos-backup
garden-rake storage prepare /dev/sdc --name databases-backup

# Configure each offering
garden-rake nurturing configure immich --seed-bank photos-backup
garden-rake nurturing configure postgres --seed-bank databases-backup
```

---

## System Configuration

### udev Rules (Recommended)

For the best experience, configure udev to prevent desktop automounters from interfering with seed bank discovery.

**Why needed:**
- Desktop environments (GNOME, KDE) automount USB devices
- This can conflict with moss's manifest-first scanning
- udev rules tell udisks2 to ignore USB storage (moss handles it)

**Installation:**

```bash
# Create udev rules file
sudo tee /etc/udev/rules.d/99-zen-garden.rules << 'EOF'
# zen-garden: Suppress udisks2 automounting for USB storage
# moss will scan and manage these devices via manifest-first discovery

# Standard USB mass storage
SUBSYSTEM=="block", ENV{ID_USB_DRIVER}=="usb-storage", ENV{UDISKS_IGNORE}="1"

# USB Attached SCSI (UAS) - faster USB 3.0 protocol
SUBSYSTEM=="block", ENV{ID_USB_DRIVER}=="uas", ENV{UDISKS_IGNORE}="1"
EOF

# Reload udev rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

**Verify rules are active:**

```bash
# Check udev rule loaded
udevadm info /dev/sdb | grep UDISKS_IGNORE

# Should show:
# E: UDISKS_IGNORE=1
```

**Removing rules (if needed):**

```bash
sudo rm /etc/udev/rules.d/99-zen-garden.rules
sudo udevadm control --reload-rules
```

### Headless Servers

On headless servers without a desktop environment, udev rules are typically not needed since there's no automounter. However, installing them provides consistent behavior and future-proofs your setup.

### Systemd Mount Units

For seed banks that should always be mounted (permanent installations), you can create systemd mount units:

```bash
# Create mount unit for a seed bank
sudo tee /etc/systemd/system/var-lib-zen\\x2dgarden-mounts-primary.mount << 'EOF'
[Unit]
Description=Seed Bank: primary
After=local-fs.target

[Mount]
What=/dev/disk/by-uuid/YOUR-UUID-HERE
Where=/var/lib/zen-garden/mounts/primary
Type=ext4
Options=defaults,noatime

[Install]
WantedBy=multi-user.target
EOF

# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable var-lib-zen\\x2dgarden-mounts-primary.mount
sudo systemctl start var-lib-zen\\x2dgarden-mounts-primary.mount
```

---

## Manifest Schema

The manifest file (`.zen-garden/manifest.json`) contains:

```json
{
  "version": 2,
  "id": "01948abc-1234-5678-9abc-def012345678",
  "name": "seed-bank-primary",
  "group": "primary",
  "replica_id": 1,
  "pool_id": null,
  "visibility": "open",
  "origin_stone": "stone-coral-prairie",
  "created_at": "2026-01-30T12:00:00Z",
  "filesystem": "ext4",
  "prepared_by": "moss@stone-coral-prairie",
  "features": {
    "compression": false,
    "encryption": false
  }
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `version` | Yes | Manifest schema version (currently 2) |
| `id` | Yes | Unique device identifier (UUIDv7) |
| `name` | Yes | Human-readable seed bank name |
| `group` | No | Logical group for replicated seed banks |
| `replica_id` | No | Replica number within group (1, 2, ...) |
| `pool_id` | No | Associated pool (for pool-specific backups) |
| `visibility` | Yes | `open` or `closed` |
| `origin_stone` | Yes | Stone that prepared this device |

---

## Write Strategies

When configuring nurturing, you can specify how data is written to replicated seed banks:

| Strategy | Description | Use Case |
|----------|-------------|----------|
| `all_replicas` | Write to all online replicas | Maximum durability, critical data |
| `any_replica` | Write to fastest available | Rotating offsite, single drive at a time |
| `specific` | Write to specific replica_id | Testing, manual control |

```bash
# Configure write strategy
garden-rake nurturing configure immich --seed-bank primary --strategy all_replicas
```

---

## Troubleshooting

### Seed Bank Not Detected

**Symptom:** Device connected but not appearing in `storage list`

**Diagnosis:**
```bash
# Check device is visible to system
lsblk

# Check moss logs for scanning
sudo journalctl -u garden-moss -n 50 | grep -i seed

# Manually trigger scan
garden-rake storage scan
```

**Solutions:**
1. **No manifest:** Device not prepared as seed bank
   - Run `garden-rake storage prepare /dev/sdX --name my-backup`
2. **Filesystem unreadable:** Corrupt or unsupported filesystem
   - Reformat with `--fs ext4`
3. **Permission denied:** moss cannot access device
   - Check moss running as root or has block device access

### Automounter Conflict

**Symptom:** Seed bank mounts at wrong location (e.g., `/media/user/backup`)

**Diagnosis:**
```bash
# Check where device is mounted
mount | grep sdX

# Check if udisks automounted
journalctl -u udisks2 -n 20
```

**Solution:** Install udev rules (see [System Configuration](#system-configuration))

### Manifest Corrupt or Missing

**Symptom:** `storage info` shows error or device not recognized

**Diagnosis:**
```bash
# Mount manually and check manifest
sudo mount /dev/sdb1 /mnt
cat /mnt/.zen-garden/manifest.json
```

**Solution:** Re-prepare the device (WARNING: may lose existing backups):
```bash
# Backup any data first if possible
garden-rake storage prepare /dev/sdb --name my-backup
```

### Replica Out of Sync

**Symptom:** Replicas show different `used_bytes` values

**Diagnosis:**
```bash
# Compare replicas
garden-rake storage list | grep primary
```

**Solutions:**
1. **Normal variance:** Small differences expected (timestamps, metadata)
2. **Significant difference:** One replica may have been offline during writes
   - Run `garden-rake nurturing sync primary` to synchronize

---

## Backwards Compatibility

### Version 1 Manifests

Seed banks created with earlier versions (v1 manifest) continue to work:
- Mount path: `/var/lib/zen-garden/mounts/{name}` (same as before)
- No group/replica support (upgrade by re-preparing)

### `zen-seed` Label

The filesystem label `zen-seed` is no longer required but still works as an optimization hint:
- If present, moss prioritizes scanning that device
- New seed banks don't receive this label by default

---

## API Reference

### Storage Health

```
GET /api/v1/stone/storage/health
```

Response:
```json
{
  "data": {
    "ready": true,
    "bank_count": 1,
    "ready_count": 1,
    "issues": [],
    "banks": [
      {
        "id": "01948abc-1234-5678-9abc-def012345678",
        "name": "primary",
        "device": "/dev/sdb1",
        "mount_path": "/var/lib/zen-garden/mounts/primary",
        "canonical": true,
        "writable": true,
        "ready": true,
        "issues": []
      }
    ]
  }
}
```

### List Seed Banks

```
GET /api/v1/stone/storage/bank
```

Response:
```json
{
  "data": [
    {
      "id": "01948abc-1234-5678-9abc-def012345678",
      "name": "primary",
      "pool_id": "0194",
      "group": "primary",
      "replica_id": 1,
      "device": "/dev/sdb",
      "mount_path": "/var/lib/zen-garden/mounts/primary/replica-1",
      "capacity_bytes": 500000000000,
      "used_bytes": 120000000000,
      "visibility": "open",
      "btrfs": true,
      "origin_stone": "stone-coral-prairie",
      "created_at": "2026-01-30T12:00:00Z",
      "online": true
    }
  ]
}
```

### Prepare Seed Bank

```
POST /api/v1/stone/storage/prepare
Content-Type: application/json

{
  "device": "/dev/sdb",
  "name": "my-backup",
  "filesystem": "ext4",
  "group": "primary",
  "replica_id": 1
}
```

---

## Further Reading

- [STORAGE-0005: Manifest-First Discovery](../decisions/STORAGE-0005-manifest-first-discovery.md) - Architecture decision record
- [Nurturing Guide](./nurturing.md) - Backup configuration and scheduling
- [Hardware Guide](./stone-hardware.md) - Recommended storage devices
