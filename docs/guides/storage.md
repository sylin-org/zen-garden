---
audience: [operator, visitor]
doc_type: guide
status: current
last_verified: 2026-03-08
canonical: true
note: "Authoritative guide for managed storage: setup, file sharing, replication, and access."
---

# Storage Guide

**Turn any drive into shared, replicated storage for your garden.**

---

## Overview

Zen Garden treats storage as a first-class entity. You can plug in a USB drive, point at a NAS share, or designate a local folder — and zen-garden turns it into managed storage that:

- **Replicates** across stones automatically
- **Serves files** over WebDAV, S3, and the REST API
- **Appears in network browsers** via SMB signpost shares
- **Backs up offerings** when given the seed-bank role
- **Works on every platform** — Linux, macOS, and Windows

This guide walks you through the entire journey, starting with a single USB drive and building up to multi-stone replication with file sharing.

---

## Quick Start

Any directory can become managed storage. Point zen-garden at it and give it a name:

```bash
# 1. Adopt an existing folder (NAS mount, USB drive, local directory)
garden-rake storage adopt /mnt/my-drive --name my-files

# 2. Confirm it's ready
garden-rake storage-status
```

That's it. Zen-garden creates a `.zen-garden/` folder alongside your existing files (never touches them), and the storage is live. Files you place on it replicate to other stones that have a storage with the same name.

> **Have a brand-new blank drive?** Use `garden-rake storage prepare /dev/sdb --name my-files` instead — it formats, partitions, mounts, and adopts in one step. See [Preparing a Blank Device](#preparing-a-blank-device) for details.

---

## Concepts

Before diving into setup, here are the building blocks you'll encounter.

### What is a Storage?

A **storage** is any filesystem that zen-garden manages. It can live on:

| Medium | Example |
|--------|---------|
| USB drive | Portable SSD plugged into a stone |
| NAS share | An NFS or SMB mount from a Synology/TrueNAS |
| Local path | A folder on the stone's internal disk |

Every storage gets a **manifest** — a small JSON file at `.zen-garden/manifest.json` that records its identity. This manifest is the sole source of truth. No filesystem labels, no magic paths — just plug the drive into any stone and moss recognizes it.

### Names and Replicas

Each storage has a **name** (like `family-photos` or `offsite-backup`). The name is a logical identifier shared across replicas. If you prepare two USB drives with the same name, zen-garden treats them as replicas of each other and keeps their contents in sync.

Each physical device also has a unique **ID** (a GUIDv7). Two drives with the same name but different IDs are replicas. Two drives with different names are independent storages.

### Roles

A storage can carry one or more **roles**:

| Role | Purpose |
|------|---------|
| `seed-bank` | Receives offering snapshots. Required for backup to target this storage. |

Roles are composable flags — a storage can be a seed-bank and hold user files at the same time. If you don't assign the seed-bank role, the storage is purely for user files and objects.

### Primary vs. Dormant

When the same storage name exists on multiple stones, zen-garden elects one as **Primary** and the rest become **Dormant**:

| State | Accepts writes? | Accepts reads? | Replicates? |
|-------|----------------|----------------|-------------|
| **Primary** | Yes | Yes | Pushes changes to dormant replicas |
| **Dormant** | No (proxies to Primary) | Yes (local copy) | Pulls changes from Primary |

You can manually claim Primary with the **pin** command — useful when you want a specific stone to own writes.

### Visibility

Each storage has a visibility setting:

| Visibility | Behavior |
|------------|----------|
| `open` | Visible and accessible to all stones in the garden |
| `closed` | Only accessible on the local stone |
| `read-only` | Visible to all, but degraded to read-only |

Default visibility is `open`.

---

## Adopting Storage

The most common way to add storage. Use this for any directory that already exists — a NAS mount, a USB drive with files on it, or a local folder.

Adopt is **non-destructive** — it writes a `.zen-garden/` dotfolder alongside your existing files and never touches your data.

### Step 1: Mount (if needed)

If it's a USB drive, moss may have already mounted it. For a NAS share, mount it first:

```bash
# NFS mount example
sudo mount -t nfs nas.local:/volume1/media /mnt/nas-media

# SMB mount example
sudo mount -t cifs //nas.local/media /mnt/nas-media -o guest
```

For a local directory, nothing to mount — just point at it.

### Step 2: Adopt

```bash
garden-rake storage adopt /mnt/nas-media --name family-media
```

This creates `.zen-garden/manifest.json` on the storage and walks the existing files to build a baseline replication log (the **content catalog**). The catalog tells other replicas what already exists, so they know what to sync.

To adopt as a seed-bank (receives offering backups):

```bash
garden-rake storage adopt /mnt/archive --name archive --roles seed-bank
```

### Step 3: Verify

```bash
garden-rake storage-status
```

You'll see the adopted storage listed with its existing capacity and usage.

### What Adopt Creates

```
/mnt/nas-media/
├── .zen-garden/               ← Created by adopt
│   ├── manifest.json          ← Identity and configuration
│   ├── changelog.jsonl        ← Replication log (seeded with existing files)
│   ├── storage/               ← S3 objects namespace
│   └── memories/              ← Offering snapshots (if seed-bank role)
├── Photos/                    ← Your existing data (untouched)
├── Videos/                    ← Your existing data (untouched)
└── Documents/                 ← Your existing data (untouched)
```

---

## Preparing a Blank Device

Use this when you have a **brand-new, empty drive** and want zen-garden to format, partition, mount, and adopt it in one step. If the drive already has data on it, use [Adopt](#adopting-storage) instead.

### Step 1: Identify the Device

```bash
garden-rake storage devices
```

Sample output:

```
Device          Size      Filesystem    Mounted
/dev/sdb       500 GB    (none)        No
/dev/sdc       1.0 TB    ext4          No
```

Choose the device you want to prepare. Double-check the device name — **prepare formats the entire device**.

### Step 2: Prepare

```bash
# Basic: name it yourself
garden-rake storage prepare /dev/sdb --name family-photos

# Let zen-garden pick a random name
garden-rake storage prepare /dev/sdb --random

# Use btrfs instead of the default ext4
garden-rake storage prepare /dev/sdb --name backups --fs btrfs

# Also give it the seed-bank role (for offering backups)
garden-rake storage prepare /dev/sdb --name backups --roles seed-bank
```

Under the hood, prepare does four things: formats the device, mounts it, creates the `.zen-garden/` structure, and writes the manifest. The result is identical to mounting a blank drive and running `adopt` — prepare just handles the low-level device setup for you.

### Step 3: Verify

```bash
garden-rake storage-status
```

Sample output:

```
Storage                 Capacity     Used      Avail     Use%   Device     Role       Pin
family-photos           500 GB       1.2 GB    498 GB    0%     /dev/sdb   Primary    pinned
─────────────────────────────────────────────────────────────────────────────────────────────
Total                   500 GB       1.2 GB    498 GB
```

---

## Accessing Files

Once a storage exists, you can access its files in several ways. Pick the one that fits your workflow.

### WebDAV (Recommended for Desktop Use)

WebDAV gives you a network folder that feels like a local drive. It works on every platform without installing anything.

**macOS (Finder):**

1. Open Finder
2. Press **Cmd+K** (Connect to Server)
3. Enter: `http://stone-name.local:7185/dav/family-photos/`
4. Click Connect

The storage appears as a network drive in Finder's sidebar.

**Linux (File Manager):**

Most Linux file managers (Nautilus, Dolphin, Thunar) support WebDAV:

1. Open your file manager
2. Click "Connect to Server" or type in the address bar:
   ```
   dav://stone-name.local:7185/dav/family-photos/
   ```
3. The storage opens as a folder

For command-line or fstab mounting:

```bash
# Install davfs2
sudo apt install davfs2

# Mount
sudo mount -t davfs http://stone-name:7185/dav/family-photos/ /mnt/photos

# Or add to /etc/fstab for persistence
# http://stone-name:7185/dav/family-photos/ /mnt/photos davfs rw,user,noauto 0 0
```

**Windows:**

1. Open File Explorer
2. Right-click "This PC" → "Map Network Drive"
3. Enter: `http://stone-name:7185/dav/family-photos/`
4. Check "Reconnect at sign-in" if you want it to persist

Or from a command prompt:

```cmd
net use Z: http://stone-name:7185/dav/family-photos/
```

**What WebDAV supports:**

| Operation | Supported |
|-----------|-----------|
| Browse folders | Yes |
| Read files | Yes |
| Create files and folders | Yes |
| Rename and move | Yes |
| Delete | Yes |
| Copy | Yes |
| Lock (concurrent editing) | Yes |

All WebDAV operations go through zen-garden's replication pipeline — writes are recorded in the changelog and propagated to dormant replicas.

### S3 Gateway (For Applications)

The S3 gateway provides an S3-compatible API. Applications that speak S3 (backup tools, media servers, data pipelines) can use it directly.

**Endpoint:** `http://stone-name:7185/api/v1/storage/s3`

**Bucket mapping:** Each bucket maps to a directory under `.zen-garden/storage/` on the selected storage.

```bash
# List buckets
curl http://stone-name:7185/api/v1/storage/s3

# List objects in a bucket
curl http://stone-name:7185/api/v1/storage/s3/my-bucket

# Upload an object
curl -X PUT \
  -H "Content-Type: image/jpeg" \
  --data-binary @photo.jpg \
  http://stone-name:7185/api/v1/storage/s3/photos/vacation/beach.jpg

# Download an object
curl -o beach.jpg \
  http://stone-name:7185/api/v1/storage/s3/photos/vacation/beach.jpg

# Delete an object
curl -X DELETE \
  http://stone-name:7185/api/v1/storage/s3/photos/vacation/beach.jpg
```

**Targeting a specific storage** (when you have multiple):

```bash
# Via header
curl -H "X-Seed-Bank: family-photos" \
  http://stone-name:7185/api/v1/storage/s3

# Via query parameter
curl "http://stone-name:7185/api/v1/storage/s3?seed-bank=family-photos"
```

If you don't specify a storage, the S3 gateway uses the default `zen-garden` storage.

### REST API (For Automation)

The garden-tier REST API provides file and object access with automatic Primary routing. You talk to any stone, and it routes to the right place.

```bash
# Read a file
curl http://stone-name:7185/api/v1/garden/storage/family-photos/fs/Photos/sunset.jpg

# Write a file
curl -X PUT \
  --data-binary @sunset.jpg \
  http://stone-name:7185/api/v1/garden/storage/family-photos/fs/Photos/sunset.jpg

# Delete a file
curl -X DELETE \
  http://stone-name:7185/api/v1/garden/storage/family-photos/fs/Photos/sunset.jpg

# Check if a file exists (metadata only)
curl -I http://stone-name:7185/api/v1/garden/storage/family-photos/fs/Photos/sunset.jpg

# List root directory
curl http://stone-name:7185/api/v1/garden/storage/family-photos/fs

# List subdirectory
curl "http://stone-name:7185/api/v1/garden/storage/family-photos/fs?path=Photos"

# List subdirectory recursively (3 levels deep)
curl "http://stone-name:7185/api/v1/garden/storage/family-photos/fs?path=Photos&depth=3"

# Full recursive listing from root
curl "http://stone-name:7185/api/v1/garden/storage/family-photos/fs?depth=all"
```

The REST API also handles objects (S3-style) and memories (offering snapshots):

```bash
# Objects (under .zen-garden/storage/)
curl http://stone-name:7185/api/v1/garden/storage/family-photos/objects/bucket/key.dat

# Memories (offering snapshots)
curl http://stone-name:7185/api/v1/garden/storage/backups/memories
curl http://stone-name:7185/api/v1/garden/storage/backups/memories/immich
```

### SMB Signpost (Network Browser Discovery)

When you have storages on a stone, moss generates a lightweight **signpost share** — a read-only Samba share containing `.url` shortcut files. This share appears in your network browser (Windows Explorer's Network view, macOS Finder's Network sidebar, Linux file managers).

The shortcuts point to the WebDAV endpoints for each storage. Clicking a shortcut opens the storage in your browser or file manager.

The signpost is automatic — it refreshes whenever storages are added, removed, or renamed. No configuration needed.

---

## Replication

Replication keeps storage replicas in sync. When you write a file to a Primary storage, it propagates to all Dormant replicas with the same name.

### How It Works

```
  Stone A (Primary)                     Stone B (Dormant)
  ┌─────────────────┐                  ┌─────────────────┐
  │ family-photos    │   changelog      │ family-photos    │
  │   Photos/        │ ──────────────►  │   Photos/        │
  │   Videos/        │   SSE stream     │   Videos/        │
  │   .zen-garden/   │                  │   .zen-garden/   │
  └─────────────────┘                  └─────────────────┘
```

1. A file is written to the Primary (via WebDAV, S3, REST API, or direct filesystem write)
2. The write goes through `ContentStore`, which appends a **changelog entry**
3. The changelog entry is broadcast as a **StorageTick** on the SSE stream
4. Dormant replicas subscribe to the SSE stream and download changes
5. The dormant replica applies the change and advances its **cursor** (a GUIDv7 tracking position)

### Changelog Entries

Every mutation is recorded with:

| Field | Description |
|-------|-------------|
| `c` | Cursor (GUIDv7 — monotonically increasing, sortable) |
| `op` | Operation: `C` (created), `M` (modified), `D` (deleted) |
| `path` | Relative file path |
| `bytes` | File size (0 for deletes) |

You can inspect the changelog for any storage:

```bash
curl http://stone-name:7185/api/v1/stone/storage/banks/family-photos/changes
```

### Setting Up Replication

Replication happens automatically when two or more storages share the same name. Here's a concrete example:

**On Stone A** (this will become Primary):

```bash
garden-rake storage adopt /mnt/ssd --name shared-docs
```

**On Stone B** (this will become Dormant):

```bash
garden-rake storage adopt /mnt/usb --name shared-docs
```

That's it. Both storages have the name `shared-docs`. Moss discovers both via the garden topology and one claims Primary. The other becomes Dormant and starts replicating.

### Controlling Primary Assignment

By default, the first stone to claim a name wins Primary. You can override this with **pinning**:

```bash
# Claim Primary on this stone (persists across reboots)
garden-rake storage pin shared-docs

# Release the claim
garden-rake storage unpin shared-docs
```

When a storage is pinned, its Primary role is sticky — even if the stone reboots, it reclaims Primary when it comes back online.

Pin state is stored on the device itself (`.zen-garden/pin.json`), so if you move the drive to a different stone, the pin moves with it.

### Testing Replication

To verify replication is working:

**1. Write a file on the Primary:**

```bash
# Via WebDAV
echo "Hello from Stone A" > /tmp/test.txt
curl -T /tmp/test.txt http://stone-a:7185/dav/shared-docs/test.txt
```

**2. Check the changelog:**

```bash
curl http://stone-a:7185/api/v1/stone/storage/banks/shared-docs/changes
```

You should see a `C` (create) entry for `test.txt`.

**3. Read from the Dormant replica:**

```bash
curl http://stone-b:7185/api/v1/garden/storage/shared-docs/fs/test.txt
```

If replication is working, you'll get "Hello from Stone A" back.

**4. Monitor the SSE stream (live):**

```bash
curl -N http://stone-a:7185/api/v1/stone/storage/stream
```

Write another file and watch the tick events appear in real time.

---

## Managing Storages

### Viewing Status

The `storage-status` command gives you a dashboard of all storages:

```bash
garden-rake storage-status
```

Sample output:

```
Storage                 Capacity     Used      Avail     Use%   Device     Role       Pin
family-photos           1.0 TB       450 GB    550 GB    45%    /dev/sdb   Primary    pinned
shared-docs             500 GB       12 GB     488 GB    2%     /dev/sdc   Dormant    -
backups (seed-bank)     2.0 TB       800 GB    1.2 TB    40%    /dev/sdd   Primary    -
─────────────────────────────────────────────────────────────────────────────────────────────
Total                   3.5 TB       1.26 TB   2.24 TB
```

### Renaming

```bash
curl -X PATCH \
  -H "Content-Type: application/json" \
  -d '{"name": "vacation-photos"}' \
  http://stone-name:7185/api/v1/stone/storage/banks/family-photos/rename
```

Renaming changes the logical name. If you have replicas, rename them all to keep them in sync.

### Changing Visibility

```bash
# Make a storage private (local-only)
curl -X PATCH \
  -H "Content-Type: application/json" \
  -d '{"visibility": "closed"}' \
  http://stone-name:7185/api/v1/stone/storage/banks/family-photos/visibility

# Make it visible again
curl -X PATCH \
  -H "Content-Type: application/json" \
  -d '{"visibility": "open"}' \
  http://stone-name:7185/api/v1/stone/storage/banks/family-photos/visibility
```

### Adding or Removing Roles

```bash
# Add the seed-bank role
curl -X PATCH \
  -H "Content-Type: application/json" \
  -d '{"roles": ["seed-bank"]}' \
  http://stone-name:7185/api/v1/stone/storage/banks/family-photos/roles

# Remove all roles (pure file storage)
curl -X PATCH \
  -H "Content-Type: application/json" \
  -d '{"roles": []}' \
  http://stone-name:7185/api/v1/stone/storage/banks/family-photos/roles
```

### Unmounting (Release)

Temporarily unmount a storage without removing it:

```bash
garden-rake storage release family-photos
```

The storage disappears from active listings but its manifest remains on the device. Plug it back in (or remount the NAS share) and moss picks it up again.

### Removing

Permanently remove a storage from this stone:

```bash
curl -X DELETE http://stone-name:7185/api/v1/stone/storage/banks/family-photos
```

This removes the manifest and stops managing the device. Your files remain on the device — only the `.zen-garden/` metadata is cleaned up.

---

## Scenarios

### Scenario 1: Personal File Server

You want to share files from a stone to your home network.

```bash
# Adopt a local directory
garden-rake storage adopt /home/stone/shared --name personal

# Access from any device via WebDAV
# macOS: Finder → Cmd+K → http://stone-name.local:7185/dav/personal/
# Linux: nautilus → dav://stone-name.local:7185/dav/personal/
# Windows: Explorer → Map Network Drive → http://stone-name:7185/dav/personal/
```

### Scenario 2: NAS Integration

You have a Synology/TrueNAS with an NFS share and want zen-garden to manage it.

```bash
# Mount the NFS share
sudo mount -t nfs nas.local:/volume1/media /mnt/nas-media

# Add to fstab for persistence
echo "nas.local:/volume1/media /mnt/nas-media nfs defaults 0 0" | sudo tee -a /etc/fstab

# Adopt the mounted share
garden-rake storage adopt /mnt/nas-media --name media-library
```

Now you can access `/mnt/nas-media` via WebDAV from any device, and it replicates to other stones with a storage named `media-library`.

### Scenario 3: Offsite Backup with USB Rotation

Keep one USB at home, take one offsite. Both carry offering backups.

```bash
# Prepare two blank drives with the same name and seed-bank role
# (Use prepare here because the drives are brand new and need formatting)
garden-rake storage prepare /dev/sdb --name offsite --roles seed-bank
garden-rake storage prepare /dev/sdc --name offsite --roles seed-bank

# Configure backup to target this storage
garden-rake backup configure immich --seed-bank offsite
garden-rake backup configure postgres --seed-bank offsite
```

Nurturing writes to whichever `offsite` drive is plugged in. Swap drives weekly — the one at home replicates the latest state, and you take the updated one offsite.

### Scenario 4: Multi-Stone Replication

You have three stones and want a replicated file share across all of them.

```bash
# On each stone, adopt a storage with the same name
# Stone A:
garden-rake storage adopt /mnt/ssd --name team-share

# Stone B:
garden-rake storage adopt /mnt/usb-drive --name team-share

# Stone C:
garden-rake storage adopt /mnt/local-ssd --name team-share
```

All three stones now replicate `team-share`. Pin one as Primary if you want deterministic write routing:

```bash
# On Stone A:
garden-rake storage pin team-share
```

Writes to `team-share` go to Stone A. Reads work from any stone (served from the local copy).

### Scenario 5: Application Storage via S3

An application needs S3-compatible object storage (e.g., a photo gallery, a backup tool).

```bash
# Adopt a directory for the application
garden-rake storage adopt /mnt/app-ssd --name app-data

# Point the application at the S3 gateway
# Endpoint: http://stone-name:7185/api/v1/storage/s3
# Bucket: app-bucket
# Header: X-Seed-Bank: app-data
```

The application uses standard S3 operations (PUT, GET, DELETE) and zen-garden handles replication.

---

## The .zen-garden Directory

Every managed storage has a `.zen-garden/` directory at its root. Here's what lives inside:

```
.zen-garden/
├── manifest.json         # Identity: ID, name, visibility, roles, origin
├── changelog.jsonl       # Append-only replication log (one JSON line per mutation)
├── pin.json              # Present when this replica claims Primary (contains pin_id)
├── last_cursor           # Replication cursor (tracks sync position)
├── last-known-good/      # Resilience snapshot of the manifest
├── memories/             # Offering snapshots (only when seed-bank role is active)
│   ├── immich/
│   │   ├── manifest.json
│   │   └── 2026-03-07T12-00-00Z.tar.gz
│   └── postgres/
│       └── ...
└── storage/              # S3 objects namespace
    └── my-bucket/
        └── my-key.dat
```

**Never modify these files by hand.** Use the CLI or API instead. The exception is the manifest — if you need to fix a broken storage, you can carefully edit `manifest.json` (see Troubleshooting).

A symlink named `Zen Garden` pointing to `.zen-garden/` is created at the storage root for user discoverability. You can toggle this with visibility settings.

### Manifest Schema (Version 4)

```json
{
  "version": 4,
  "id": "019c0789-abcd-7000-8000-123456789abc",
  "name": "family-photos",
  "visibility": "open",
  "origin_stone": "stone-coral-prairie",
  "filesystem": "ext4",
  "created_at": "2026-03-08T10:00:00Z",
  "encrypted": false,
  "pond_fingerprint": null,
  "roles": ["seed-bank"]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `version` | number | Manifest format version (current: 4) |
| `id` | string | Unique device ID (GUIDv7). Never changes, never shared between devices. |
| `name` | string | Logical storage name. Shared across replicas. |
| `visibility` | string | `open`, `closed`, or `read-only` |
| `origin_stone` | string | Hostname of the stone that created this storage |
| `filesystem` | string | `ext4` or `btrfs` |
| `created_at` | string | ISO 8601 timestamp |
| `encrypted` | boolean | Whether content is encrypted |
| `pond_fingerprint` | string? | CA fingerprint (present when encrypted) |
| `roles` | array | Composable roles (e.g., `["seed-bank"]`) |

---

## CLI Reference

| Command | Purpose |
|---------|---------|
| `garden-rake storage devices` | List available (unprepared) devices |
| `garden-rake storage prepare /dev/sdX --name NAME` | Format and claim a blank device |
| `garden-rake storage adopt /path --name NAME` | Adopt existing storage non-destructively |
| `garden-rake storage-status` | Dashboard of all storages with capacity and roles |
| `garden-rake storage list` | List connected storages |
| `garden-rake storage info NAME` | Detailed info about a storage |
| `garden-rake storage release NAME` | Unmount without removing |
| `garden-rake storage pin NAME` | Claim Primary role (persists across reboots) |
| `garden-rake storage unpin NAME` | Release Primary claim |

### Prepare Flags

| Flag | Description | Default |
|------|-------------|---------|
| `--name <name>` | Human-readable storage name | Required (unless `--random`) |
| `--random` | Generate a random name | false |
| `--fs <type>` | Filesystem: `ext4` or `btrfs` | `ext4` |
| `--roles <role,...>` | Composable roles (e.g., `seed-bank`) | none |
| `--encrypted` | Enable content encryption | false |

### Adopt Flags

| Flag | Description | Default |
|------|-------------|---------|
| `--name <name>` | Storage name | Required |
| `--roles <role,...>` | Composable roles | none |
| `--encrypted` | Enable content encryption | false |

---

## System Configuration

### udev Rules (Recommended on Linux)

Desktop environments automount USB drives, which can conflict with moss's manifest-first scanning. Install a udev rule to let moss handle USB storage:

```bash
sudo tee /etc/udev/rules.d/99-zen-garden.rules << 'EOF'
# zen-garden: Suppress udisks2 automounting for USB storage
SUBSYSTEM=="block", ENV{ID_USB_DRIVER}=="usb-storage", ENV{UDISKS_IGNORE}="1"
SUBSYSTEM=="block", ENV{ID_USB_DRIVER}=="uas", ENV{UDISKS_IGNORE}="1"
EOF

sudo udevadm control --reload-rules
sudo udevadm trigger
```

Verify with:

```bash
udevadm info /dev/sdb | grep UDISKS_IGNORE
# Should show: E: UDISKS_IGNORE=1
```

On headless servers without a desktop, these rules are optional but harmless to install.

### Persistent NAS Mounts

For NAS shares that should always be available, add them to `/etc/fstab`:

```bash
# NFS
nas.local:/volume1/media  /mnt/nas-media  nfs  defaults,_netdev  0  0

# SMB/CIFS
//nas.local/media  /mnt/nas-media  cifs  guest,_netdev,iocharset=utf8  0  0
```

Then adopt the mount:

```bash
sudo mount /mnt/nas-media
garden-rake storage adopt /mnt/nas-media --name media
```

### Samba Integration (SMB Signpost)

Moss generates a Samba config fragment at `{data_dir}/signpost/smb.conf.fragment`. To include it in your system Samba configuration:

```bash
# Add to the end of /etc/samba/smb.conf:
include = /var/lib/zen-garden/signpost/smb.conf.fragment
```

Restart Samba:

```bash
sudo systemctl restart smbd
```

The signpost share appears in network browsers as a read-only folder containing `.url` shortcuts to your storages' WebDAV endpoints.

---

## API Reference

### Stone-Tier (Local Administration)

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/stone/storage` | Storage overview |
| GET | `/api/v1/stone/storage/health` | Health status |
| GET | `/api/v1/stone/storage/candidates` | Eligible devices |
| POST | `/api/v1/stone/storage/prepare` | Prepare blank device |
| POST | `/api/v1/stone/storage/adopt` | Adopt existing storage |
| GET | `/api/v1/stone/storage/banks` | List local storages |
| GET | `/api/v1/stone/storage/banks/{name}` | Storage details |
| DELETE | `/api/v1/stone/storage/banks/{name}` | Remove storage |
| POST | `/api/v1/stone/storage/banks/{name}/release` | Unmount |
| POST | `/api/v1/stone/storage/banks/{name}/pin` | Claim Primary |
| POST | `/api/v1/stone/storage/banks/{name}/unpin` | Release Primary |
| PATCH | `/api/v1/stone/storage/banks/{name}/visibility` | Set visibility |
| PATCH | `/api/v1/stone/storage/banks/{name}/rename` | Rename |
| PATCH | `/api/v1/stone/storage/banks/{name}/roles` | Set roles |
| GET | `/api/v1/stone/storage/banks/{name}/changes` | Replication changelog |
| GET | `/api/v1/stone/storage/stream` | SSE replication stream |

### Garden-Tier (Cross-Stone, Name-Based Routing)

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/garden/storage` | All storages across garden |
| GET | `/api/v1/garden/storage/{name}` | Discover replicas |
| GET | `/api/v1/garden/storage/{name}/fs` | Directory listing (`?path=&depth=N`) |
| GET/PUT/DELETE/HEAD | `/api/v1/garden/storage/{name}/fs/*path` | User file operations |
| GET/PUT/DELETE/HEAD | `/api/v1/garden/storage/{name}/objects/*path` | S3 object operations |
| GET | `/api/v1/garden/storage/{name}/memories` | List offerings with snapshots |
| GET | `/api/v1/garden/storage/{name}/memories/{offering}` | List snapshots |
| GET | `/api/v1/garden/storage/{name}/memories/{offering}/{harvest}` | Download snapshot |

### WebDAV

| Path | Purpose |
|------|---------|
| `/dav/{name}/*path` | Full RFC 4918 WebDAV (PROPFIND, GET, PUT, DELETE, MKCOL, COPY, MOVE, LOCK) |

### S3 Gateway

| Method | Endpoint | Purpose |
|--------|----------|---------|
| GET | `/api/v1/storage/s3` | List buckets (XML) |
| GET | `/api/v1/storage/s3/{bucket}` | List objects (XML) |
| PUT | `/api/v1/storage/s3/{bucket}/*key` | Put object |
| GET | `/api/v1/storage/s3/{bucket}/*key` | Get object |
| HEAD | `/api/v1/storage/s3/{bucket}/*key` | Object metadata |
| DELETE | `/api/v1/storage/s3/{bucket}/*key` | Delete object |

**Storage selection:** Use `X-Seed-Bank` header or `?seed-bank=NAME` query parameter.

---

## Troubleshooting

### Storage Not Detected After Plugging In

**Symptom:** USB drive connected but not in `storage-status`

**Check:**

```bash
# Is the device visible to the system?
lsblk

# Check moss logs for scanning activity
sudo journalctl -u garden-moss -n 50 | grep -i storage

# Is there a manifest on the device?
sudo mount /dev/sdb1 /mnt
ls /mnt/.zen-garden/manifest.json
```

**Common causes:**

1. **No manifest** — The device hasn't been prepared or adopted. Run `garden-rake storage prepare` or `garden-rake storage adopt`.
2. **Desktop automounter grabbed it** — Install udev rules (see [System Configuration](#system-configuration)).
3. **Permission denied** — Ensure moss runs with access to block devices.

### Replication Not Working

**Symptom:** Files written on Primary don't appear on Dormant replicas

**Check:**

```bash
# Verify both storages have the same name
garden-rake storage-status  # Run on both stones

# Check the changelog on the Primary
curl http://primary-stone:7185/api/v1/stone/storage/banks/my-storage/changes

# Monitor the SSE stream on the Dormant
curl -N http://dormant-stone:7185/api/v1/stone/storage/stream
```

**Common causes:**

1. **Different names** — Storage names must match exactly for replication.
2. **Visibility is `closed`** — Set to `open` on the Primary.
3. **Network issue** — Stones must be able to reach each other over HTTP (port 7185).
4. **No Primary elected** — Pin one storage to force Primary: `garden-rake storage pin my-storage`.

### Manifest Corrupt or Missing

**Symptom:** `storage info` errors or storage not recognized

**Recovery:**

```bash
# Mount the device manually
sudo mount /dev/sdb1 /mnt

# Check if manifest exists
cat /mnt/.zen-garden/manifest.json

# If corrupt, you can re-adopt (non-destructive)
garden-rake storage adopt /mnt --name recovered-storage
```

Re-adopting writes a fresh manifest and rebuilds the content catalog. Your files are preserved.

### Wrong Storage is Primary

**Symptom:** Writes go to an unexpected stone

**Fix:**

```bash
# On the stone that should be Primary:
garden-rake storage pin my-storage

# On the stone that should not be Primary:
garden-rake storage unpin my-storage
```

Pin is persistent — it survives reboots and even moving the drive to a different stone.

### WebDAV Connection Refused

**Symptom:** Can't connect via WebDAV

**Check:**

```bash
# Is moss running?
systemctl status garden-moss

# Is the port open?
curl http://stone-name:7185/health

# Try the WebDAV endpoint directly
curl -X PROPFIND http://stone-name:7185/dav/my-storage/
```

**Common causes:**

1. **Firewall** — Ensure port 7185 is open.
2. **Storage name typo** — The path must match the storage name exactly: `/dav/exact-name/`.
3. **Moss not running** — Start it: `sudo systemctl start garden-moss`.

---

## Further Reading

- [Backup Guide](./nurturing.md) — Backup configuration and scheduling
- [First Stone](./first-stone.md) — Setting up your first stone
- [Seed Banks Guide](./seed-banks.md) — Legacy seed bank setup (pre-STORAGE-0009)
- [Troubleshooting](./troubleshooting.md) — General troubleshooting
