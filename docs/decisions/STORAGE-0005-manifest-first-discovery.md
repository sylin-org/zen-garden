# STORAGE-0005: Manifest-First Seed Bank Discovery

**Status:** Implemented
**Date:** 2026-01-30
**Supersedes:** Partial aspects of STORAGE-0004 (label-based detection)

## Context

The current seed bank detection relies on filesystem labels (`zen-seed`) to identify devices. This approach has limitations:

1. **Single seed bank design**: Label `zen-seed` implies one seed bank per stone
2. **No replication support**: Can't distinguish replicas of the same logical seed bank
3. **Configuration split**: Label identifies device, manifest provides details
4. **Label constraints**: Limited to 16 characters, can't encode rich configuration

Users need:
- Multiple named seed banks (e.g., `primary`, `offsite`, `archive`)
- Replicated seed banks (two devices forming one logical backup target)
- User selection of which seed bank to write to
- Flexible configuration without filesystem-level constraints

## Decision

Adopt a **manifest-first** discovery model where the `.zen-garden/manifest.json` file is the sole source of truth for seed bank identity and configuration.

### Core Principles

1. **Manifest is authoritative** - No filesystem labels required
2. **udev suppresses automounters** - Prevents interference on all USB storage
3. **moss scans all removable devices** - Temp-mount to check for manifest
4. **Mount path derived from manifest** - Supports named groups and replicas

### Discovery Flow

```
For each removable/USB block device:
    │
    ├─► Already mounted at /var/lib/zen-garden/mounts/?
    │       └─► Yes: Check manifest, track for persistence
    │
    └─► Not mounted
            │
            ├─► Temp mount (read-only)
            │
            ├─► Check for .zen-garden/manifest.json
            │       │
            │       ├─► Not found: Unmount, skip (not a seed bank)
            │       │
            │       └─► Found: Read manifest
            │               │
            │               ├─► Derive mount path from manifest
            │               │
            │               ├─► Unmount temp
            │               │
            │               └─► Mount to derived path
            │
            └─► Track for persistence monitoring
```

### Manifest Schema (v2)

```json
{
  "version": 2,
  "id": "01948abc-1234-5678-9abc-def012345678",
  "name": "seed-bank-primary",
  "group": "primary",
  "replica_id": 1,
  "pool_id": null,
  "visibility": "private",
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
| `version` | Yes | Manifest schema version (2 for this design) |
| `id` | Yes | Unique device identifier (GUIDv7) |
| `name` | Yes | Human-readable seed bank name |
| `group` | No | Logical group for replicated seed banks |
| `replica_id` | No | Replica number within group (1, 2, ...) |
| `pool_id` | No | Associated pool (for pool-specific backups) |
| `visibility` | Yes | `private` or `shared` |
| `origin_stone` | Yes | Stone that prepared this device |

### Mount Path Derivation

```rust
fn derive_mount_path(manifest: &SeedBankManifest) -> PathBuf {
    let base = PathBuf::from("/var/lib/zen-garden/mounts");

    match (manifest.group.as_ref(), manifest.replica_id) {
        // Replicated: /mounts/{group}/replica-{id}
        (Some(group), Some(id)) => base.join(group).join(format!("replica-{}", id)),

        // Named group without replica: /mounts/{group}
        (Some(group), None) => base.join(group),

        // Simple: /mounts/{name}
        (None, _) => base.join(&manifest.name),
    }
}
```

**Examples:**

| Manifest | Mount Path |
|----------|------------|
| `name: "seed-bank-zen-garden"` | `/mounts/seed-bank-zen-garden` |
| `group: "primary", replica_id: 1` | `/mounts/primary/replica-1` |
| `group: "primary", replica_id: 2` | `/mounts/primary/replica-2` |
| `group: "offsite"` | `/mounts/offsite` |

### udev Configuration

Suppress udisks2 for all USB storage devices (moss handles them):

```bash
# /etc/udev/rules.d/99-zen-garden.rules

# Suppress udisks2 automounting for USB storage
# zen-garden moss will scan and manage these devices
SUBSYSTEM=="block", ENV{ID_USB_DRIVER}=="usb-storage", ENV{UDISKS_IGNORE}="1"
SUBSYSTEM=="block", ENV{ID_USB_DRIVER}=="uas", ENV{UDISKS_IGNORE}="1"
```

### Backwards Compatibility

- **v1 manifests** (without `group`/`replica_id`): Mount to `/mounts/{name}` as before
- **`zen-seed` label**: Still works as optimization hint, but not required
- **Existing seed banks**: Continue to work unchanged

### API Changes

#### List Seed Banks (Logical View)

```
GET /api/v1/stone/storage/bank
```

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

#### Configure Nurturing Target

```
POST /api/v1/nurturing/configure
{
  "offering_id": "immich",
  "seed_bank": "primary",
  "write_strategy": "all_replicas"
}
```

Write strategies:
- `all_replicas`: Write to all online replicas (default, most durable)
- `any_replica`: Write to fastest available replica
- `specific`: Write to specific `replica_id`

## CLI Changes

### Prepare Command

The `storage prepare` command now supports group and replica flags:

```bash
# Simple seed bank
garden-rake storage prepare /dev/sdb --name my-backup

# Grouped seed bank
garden-rake storage prepare /dev/sdb --group offsite

# Replicated seed bank (first replica)
garden-rake storage prepare /dev/sdb --group primary --replica 1

# Replicated seed bank (second replica)
garden-rake storage prepare /dev/sdc --group primary --replica 2
```

**New Flags:**

| Flag | Type | Description |
|------|------|-------------|
| `--group <name>` | Optional | Logical group name for organization or replication |
| `--replica <id>` | Optional | Replica number within group (1, 2, 3, ...) |

When `--group` is specified without `--replica`, defaults to standalone grouped device.
When `--replica` is specified, `--group` is required.

## File Changes

- `src/common/src/storage.rs` - Added `group`, `replica_id` to `SeedBankManifest`, `SeedBankInfo`, and `PrepareSeedBankRequest`
  - Added `derive_mount_path()`, `logical_name()`, `is_replica()` methods
  - Added `new_replica()` constructor for replicated manifests
  - Bumped `CURRENT_VERSION` to 2
- `src/moss/src/infra/storage/registry.rs` - Manifest-first scanning logic
  - Replaced label-based discovery with manifest-first approach
  - Added `probe_device_for_manifest()` for temp-mount manifest check
  - Added `track_existing_mounts()` for persistence monitoring
- `src/moss/src/infra/storage/device.rs` - Added `list_unmounted_removable_devices()`
  - Added `UnmountedDevice` struct for device info
- `src/moss/src/infra/storage/mod.rs` - Updated exports
- `src/moss/src/api/v1/storage.rs` - Updated `run_prepare_job()` to accept group/replica_id
  - Creates v2 manifests with replication support
  - Derives mount path from manifest configuration
- `src/rake/src/main.rs` - Added `--group` and `--replica` CLI flags to Prepare command
- `src/rake/src/commands/storage.rs` - Updated `PrepareSeedBankCommand` struct and constructor

## Consequences

### Positive

- **Single source of truth**: Manifest defines everything
- **Multiple seed banks**: Different names, pools, purposes
- **Replication**: Multiple devices as one logical target
- **Flexible**: JSON manifest can evolve without filesystem constraints
- **User control**: Choose which seed bank to write to
- **Backwards compatible**: Existing seed banks continue working

### Negative

- **More temp mounts**: Must mount to check for manifest (mitigated by caching)
- **Slightly slower discovery**: Extra I/O for manifest check
- **udev rule recommended**: Best experience requires system configuration

### Neutral

- **Label optional**: `zen-seed` label becomes hint, not requirement
- **Preparation unchanged**: `prepare` command still creates manifest
