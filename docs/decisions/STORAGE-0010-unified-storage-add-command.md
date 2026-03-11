---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-03-08
---

# STORAGE-0010: Unified `storage add` Command

**Date**: 2026-03-08
**Status**: Accepted
**Evolves**: STORAGE-0009 (CLI surface, hotplug behavior)

## Context

STORAGE-0009 introduced two CLI commands for bringing storage into the garden:

- **`storage prepare`** — Formats a blank device, mounts it, creates `.zen-garden/`, writes the manifest. Requires an empty device.
- **`storage adopt`** — Creates `.zen-garden/` alongside existing files, catalogs content for replication. Requires a mounted directory with files.

This split created three problems:

1. **Choice paralysis.** New users must decide which command to run before they understand the difference. The guide had to explain both flows, and users who picked the wrong one got an error ("device has existing files" / "path is not a directory").

2. **False dichotomy.** The underlying operation is the same: inspect the target, optionally format it, create `.zen-garden/`, write the manifest, catalog existing content. The only variable is whether formatting happens first. Two commands for one conceptual action.

3. **No interactive path.** Both commands require the user to know their device path, name, and roles upfront. There was no guided flow for first-time users who just plugged in a USB drive.

Separately, the hotplug detection (udev monitor) treated all unmanaged devices identically — the same "A new seed bank awaits" ribbon appeared whether the device was empty, had files, or was already a managed storage. The `HasData` state was filtered out entirely, meaning devices with existing files produced no banner at all.

## Decision

### 1. Unified `storage add` command

`prepare` and `adopt` merge into a single `storage add` command. The command inspects the target and does the right thing:

- **Block device with no filesystem** → formats, mounts, creates `.zen-garden/`
- **Block device with filesystem, no files** → mounts, creates `.zen-garden/`
- **Block device or directory with existing files** → creates `.zen-garden/`, catalogs content
- **Path with existing `.zen-garden/`** → error ("already managed, did you mean `storage-status`?")

The old `prepare` and `adopt` commands become hidden aliases that route to the same endpoint. Existing scripts continue to work.

### 2. Interactive wizard

When invoked with no arguments, `storage add` launches a pond-init-style interactive wizard. The wizard uses the same visual conventions as `ceremony_render.rs`: numbered selections, `✓`/`○`/`·` indicators, `╭─── title ───╮` confirmation box.

#### Step 1 — Device selection

```
  Select a device

  [1] /dev/sdb — 500 GB, no filesystem
      Empty device, will need formatting

  [2] /dev/sdc — 1.0 TB, ext4, 340 GB used
      Contains existing files

  [3] Custom path...
      Enter a mount point or directory manually

  Choose [1-3, Enter=1, esc=cancel]:
```

If `[3]`, prompts for path:

```
  Path: /mnt/nas-media
  ✓ Path
```

#### Step 2 — Name

```
  Storage name (Enter = auto): my-files
  ✓ Storage name
```

Enter with no input generates a random name.

#### Step 3 — Options & Roles

Two labeled groups with continuous numbering. A single shared prompt at the bottom.

```
  Options
  [1]  ✓  Format                      ext4 (no filesystem)

  Roles
  [2]  ○  Seed bank                   Receive offering backups from nurturing.

  Toggle with number, Enter to continue, Esc to cancel:
```

State indicators:

| Indicator | Meaning |
|-----------|---------|
| `✓` | Enabled (toggleable) |
| `○` | Disabled (toggleable) |
| `·` | Locked — cannot be changed (dimmed text, with reason) |

Locked states:

- Device has no filesystem → format locked **on**: `[1]  ✓  Format   ext4 (no filesystem)`
- Device has existing files → format locked **off**: `[1]  ·  Format   device has existing files`
- Directory path (not block device) → format locked **off**: `[1]  ·  Format   path is a directory`

Pressing a locked item's number prints an error: `✗ Cannot toggle: device has existing files.`

New roles slot in as `[3]`, `[4]`, etc. under the Roles header. Each role has a short description visible at a glance.

#### Step 4 — Confirmation

```
  ╭── Confirm storage ──────────────────────╮
  │                                         │
  │  Device:     /dev/sdb                   │
  │  Name:       my-files                   │
  │  Format:     yes (ext4)                 │
  │  Roles:      seed-bank                  │
  │                                         │
  ╰─────────────────────────────────────────╯

  [c] Confirm   [e] Edit   [q] Cancel
```

`[e]` loops back to step 1 with current values pre-filled. `[c]` fires the API call. `[q]` aborts.

### 3. CLI pre-fill and non-interactive mode

CLI flags pre-fill wizard steps and skip them when fully specified. This follows the same pattern as pond init's `--passphrase`/`--profile` flowing into `initial_data`.

```bash
# Full wizard
garden-rake storage add

# Pre-filled name — wizard skips step 2
garden-rake storage add --name photos

# Pre-filled name + role — wizard skips steps 2 and 3
garden-rake storage add --name photos --roles seed-bank

# Device + name + role — only confirmation remains
garden-rake storage add /dev/sdb --name photos --roles seed-bank

# Fully non-interactive (for scripts)
garden-rake storage add /dev/sdb --name photos --roles seed-bank --yes
```

### 4. Zen syntax keywords

The zen parser gains three new keywords for `storage add`:

| Zen keyword | Normative flag | Purpose |
|-------------|---------------|---------|
| `as <name>` | `--name <name>` | Storage name |
| `role <role>` | `--roles <role>` | Role assignment |
| `with` | *(noise)* | Optional readability word, consumed and discarded |

Examples — all equivalent:

```bash
# Zen syntax
garden-rake storage add /dev/sdb as photos with role seed-bank

# Zen without noise word
garden-rake storage add /dev/sdb as photos role seed-bank

# Normative syntax
garden-rake storage add /dev/sdb --name photos --roles seed-bank
```

`as` follows the same pattern as `on <stone>` and `from <url>` — a preposition that extracts the next token. `with` follows the `quietly` pattern — a semantic noise word consumed by the parser with no data payload.

### 5. Unified API endpoint

A single endpoint replaces `prepare` and `adopt`:

```
POST /api/v1/stone/storage/add
Content-Type: application/json

{
  "target": "/dev/sdb",
  "name": "photos",
  "format": true,
  "filesystem": "ext4",
  "roles": ["seed-bank"]
}
```

The server inspects `target` and validates the request:

- If `format: true` and target has existing files → 409 Conflict
- If `format: false` and target has no filesystem → 422 Unprocessable ("device has no filesystem, set format to true")
- If target already has `.zen-garden/` → 409 Conflict ("already managed")

The existing `POST /api/v1/stone/storage/prepare` and `POST /api/v1/stone/storage/adopt` become aliases that route to the same handler. `prepare` implies `format: true`; `adopt` implies `format: false`.

### 6. Context-aware hotplug banners

The udev monitor's hotplug detection differentiates three device states with distinct banners and Firefly events.

#### Managed storage reconnected (state = `Prepared`)

The device has `.zen-garden/manifest.json`. Moss reads the manifest, auto-mounts, registers the storage, and announces to the garden.

```
🌱  ✓       Storage "photos" connected
            Primary, seed-bank, 340 GB used
```

No call to action — the storage is live. Firefly shows "STORAGE CONNECTED" with the storage name.

Event: `StorageConnected { name, roles, used_bytes, capacity_bytes }`

#### Unmanaged device with files (state = `HasData`)

The device has a filesystem and visible files but no `.zen-garden/`. Currently filtered out (no banner). This state is now surfaced.

```
🌱          Storage device connected (500 GB, 340 GB used)
            Contains existing files
            garden-rake storage add /dev/sdb
```

Event: `StorageDetected { device, state: "has_data", capacity_bytes, used_bytes }`

#### Empty or unformatted device (state = `Empty` / `Unformatted` / `Unpartitioned`)

```
🌱          Empty storage connected (500 GB)
            garden-rake storage add /dev/sdb
```

Event: `StorageDetected { device, state: "empty", capacity_bytes }`

#### Monitor code change

The `if info.eligible || is_prepared` gate in `monitor.rs` (line 135) is replaced with a three-way match on `info.state`:

```rust
match info.state {
    DeviceState::Prepared => {
        // Auto-mount, register, announce, connected ribbon
    }
    DeviceState::HasData => {
        // Detected ribbon with "storage add" hint
    }
    DeviceState::Empty | DeviceState::Unformatted | DeviceState::Unpartitioned => {
        // Empty ribbon with "storage add" hint
    }
}
```

`HasData` devices are no longer filtered out. They produce banners and events, guiding the user to `storage add`.

### 7. Backwards compatibility

| Before | After | Breaking? |
|--------|-------|-----------|
| `garden-rake prepare /dev/sdb --name x` | Hidden alias → `storage add /dev/sdb --name x --format` | No |
| `garden-rake storage adopt /path --name x` | Hidden alias → `storage add /path --name x` | No |
| `POST /api/v1/stone/storage/prepare` | Alias → `/api/v1/stone/storage/add` with `format: true` | No |
| `POST /api/v1/stone/storage/adopt` | Alias → `/api/v1/stone/storage/add` with `format: false` | No |
| `DeviceState::HasData` filtered silently | Now produces banner and event | No (additive) |

Old commands and endpoints continue to work unchanged. New users learn `storage add`; existing scripts are unaffected.

## Consequences

### Positive

- **One command to learn.** `storage add` handles blank devices, populated drives, NAS mounts, and local directories. Users don't need to understand the prepare/adopt distinction.
- **Guided onboarding.** The interactive wizard walks first-time users through device selection, naming, and role assignment. No manual page required.
- **Zen syntax reads naturally.** `storage add /dev/sdb as photos with role seed-bank` reads like a sentence, consistent with `offer mongodb on stone-02`.
- **Hotplug closes the loop.** Plugging in a device produces a banner with the exact command to run. Managed devices reconnect silently with a confirmation banner.
- **Scriptable.** `--yes` flag enables fully non-interactive usage for automation.

### Negative

- **Three ribbon functions replace one.** The banner code grows from one function to three, with distinct event types. Minor complexity increase in `tty.rs` and `monitor.rs`.
- **Zen keyword expansion.** `as`, `role`, and `with` are new reserved words in the zen parser. Unlikely to collide with existing usage, but expands the keyword set.

### Neutral

- **Old commands hidden, not removed.** `prepare` and `adopt` continue to work as aliases. No migration required for existing users or documentation that references them.
- **API endpoint aliasing.** Three endpoints (`/add`, `/prepare`, `/adopt`) route to the same handler. Router complexity is trivial.

## Delivery

1. **API unification** — Merge `prepare` and `adopt` handlers into `POST /api/v1/stone/storage/add`. Old endpoints become aliases. Unify `PrepareStorageRequest` and `AdoptStorageRequest` into `AddStorageRequest` in `garden_common::storage`.

2. **Rake command** — Add `storage add` to command manifest. Implement interactive wizard in `commands/storage.rs` using ceremony-style rendering. Wire `as`/`role`/`with` zen keywords in the parser.

3. **Hotplug banners** — Three ribbon functions in `tty.rs`. Three-way match in `monitor.rs`. New `StorageConnected` event type alongside existing `StorageDetected`.

4. **Docs** — Update `docs/guides/storage.md` to lead with `storage add`. Update API reference. Hide `prepare`/`adopt` from primary documentation.

## Related

- [STORAGE-0009](STORAGE-0009-managed-storage-and-file-sharing.md) — Managed storage architecture (evolved by this ADR's CLI and hotplug changes)
- [STORAGE-0005](STORAGE-0005-manifest-first-discovery.md) — Manifest-first discovery (`.zen-garden/manifest.json`)
- [Storage Guide](../guides/storage.md) — User-facing documentation (to be updated)
