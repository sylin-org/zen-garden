---
audience: [operator]
doc_type: guide
status: current
last_verified: 2026-03-25
note: "Windows-only feature. Cloud Filter integration for native Explorer sync."
---

# Cloud Filter (Windows Explorer Integration)

**Access garden storages as native folders in Windows Explorer — files download on demand.**

---

## Overview

Cloud Filter makes managed storages appear as a "Zen Garden" folder in Windows Explorer, similar to OneDrive or iCloud Drive. Each storage shows as a subdirectory. Files are cloud placeholders: they appear in the file listing with their real names and sizes, but content downloads only when you open a file. Saving or pasting files into the folder writes them back to the storage.

---

## Requirements

- **Windows 10 version 1709** (Fall Creators Update) or later, or **Windows 11**
- Moss running on the Windows machine (interactive or as a service)
- At least one managed storage (local or visible from another stone in the garden)

---

## How It Works

Cloud Filter starts automatically when Moss launches on a supported Windows version. No manual configuration is needed.

### Startup sequence

1. Moss checks Cloud Filter API support on the current Windows version.
2. A **sync root** registers under `%USERPROFILE%\Zen Garden\`. This folder appears in Explorer's navigation pane with a "Zen Garden" label.
3. The **provider** connects to the sync root, enabling CfApi callbacks.
4. A **storage watcher** creates placeholder subdirectories for each available storage. Three event sources trigger reconciliation: local volume changes, remote stone storage appearing or departing, and a 60-second heartbeat catch-all.
5. An **ingest watcher** monitors the sync root for user-created files and copies them to the corresponding storage.

### Reading files (hydration)

To open a file from the Zen Garden folder, double-click it in Explorer. Windows sends a hydration request to Moss, which fetches the requested byte range from the storage (local mount or remote stone API). Only the range Windows needs is transferred — large files do not load entirely into memory.

### Writing files (ingest)

To add files, paste, drag, or save them into any storage subdirectory. The ingest watcher detects new files, copies them to the actual storage mount (or proxies to the remote Primary stone), and marks them in-sync. The storage replication system picks up the change independently — no manual step is needed.

Files pasted while a storage is offline are retried automatically when that storage comes back online.

### Other Explorer operations

| Operation | Behavior |
|-----------|----------|
| Rename a file or folder | Propagates to the storage |
| Delete a file or folder | Propagates to the storage |
| Rename a top-level storage folder | Renames the replica set across all local volumes |
| Delete a top-level storage folder | Rejected — use `garden-rake storage release` instead |
| Move between storages | Cross-storage copy + delete (handled transparently) |
| Drag a file in from outside | Ingested into the target storage |
| Drag a file out | Deleted from the storage (best-effort) |
| Dehydrate (free up space) | Approved — reverts file to a placeholder |

---

## Availability Signals

### Toast notifications

Windows toast notifications fire when a storage replica set crosses the available/offline boundary:

- **Connected** — a set gained its first ready member (first appearance).
- **Back online** — a set that was offline has at least one member again.
- **Offline** — a set lost its last ready member; no stone is reachable.

Adding a second replica to an already-available set, or removing a non-last replica, is silent.

### Explorer info bar

When any storage is offline, Explorer displays a blue info bar above the file list:

> "'my-files' is not reachable. Check that the stone hosting it is powered on and connected to your network."

The bar clears automatically when all storages come back online.

### Placeholder overlay icons

| Storage state | Explorer icon |
|---------------|---------------|
| Online (local or remote) | Green checkmark (in-sync) |
| Offline (no stone reachable) | Cloud pending icon |

---

## Limitations

- **Windows only.** macOS and Linux use WebDAV for native file access (see the [Storage Guide](storage.md)).
- **Moss must be running.** Stopping Moss disconnects the provider; files already hydrated remain on disk, but new hydration requests fail until Moss restarts.
- **No offline editing.** Placeholder files require a connection to the hosting stone. Editing a hydrated file while the storage is offline may lose changes.
- **Sync root path.** The sync root is always `%USERPROFILE%\Zen Garden\` and is not configurable.

---

## Troubleshooting

### Sync root not appearing in Explorer

- Verify Moss is running: `garden-rake stone health`.
- Check that Cloud Filter API is supported: Moss logs `Cloud Filter API not supported on this Windows version` if not.
- Confirm the Windows Search service (WSearch) is running — sync root registration requires it.
- Look for `sync root registered` or `sync root already registered` in Moss logs.

### Placeholder errors (0x8007017C)

This error indicates the CfApi mini-filter rejected a placeholder operation. Moss re-registers the sync root on version mismatch, which clears stale state. To force a fresh registration, stop Moss, delete `%USERPROFILE%\Zen Garden\`, and restart Moss.

### Files not syncing back

- The ingest watcher logs activity at `debug` level. Increase log verbosity to see transfer events.
- Verify the target storage has a writable route: check `garden-rake storage status` for the storage state.
- Files pasted while a storage is offline are retried when it comes online.

### Stale placeholders

The 60-second heartbeat reconciler removes stale placeholder directories that no longer correspond to any known storage. If a stale entry persists, restarting Moss triggers a clean reconcile pass.

---

## Design Rationale

> [STORAGE-0012: Cloud Filter Architecture Rebuild](../decisions/STORAGE-0012-cloud-filter-rebuild.md)
