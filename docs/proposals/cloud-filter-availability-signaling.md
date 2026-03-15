---
audience: [developer, ai]
doc_type: proposal
status: draft
last_verified: 2026-03-13
---

# Cloud Filter — Storage Availability Signaling

**Author**: onose
**Date**: 2026-03-13
**Candidate ADR**: STORAGE-0016
**Evolves**: STORAGE-0012 (Cloud Filter rebuild), STORAGE-0015 (StorageRouter)

---

## Problem Statement

Explorer placeholders for replica set folders are always created with
`mark_in_sync()` regardless of whether any stone hosting that storage is
reachable. A user who powers off the stone holding their storage sees no
visual difference in Explorer — the folder looks healthy until they click
into it and get an error.

Three distinct situations have no visual distinction:

| Situation | What the user experiences today |
|-----------|--------------------------------|
| Storage is on this stone (local disk) | Green ✓ |
| Storage is on another garden stone (LAN) | Green ✓ |
| No stone with this storage is reachable | Green ✓ (then error on access) |

The result is silent failure — users lose trust in the drive letter and
resort to trial-and-error to understand what is actually accessible.

---

## Proposed Solution

### Two overlay states, not three

Use CfApi's built-in IN_SYNC state machine to signal one thing: **can the
user access their files right now?**

| State | CfApi flag | Explorer shows |
|-------|-----------|----------------|
| Online (local or remote) | `mark_in_sync()` | ✓ green checkmark |
| Offline (no stone reachable) | no `mark_in_sync()` | ↑ cloud pending |

Local vs remote-online is not distinguished at the overlay level. The
latency difference is imperceptible on LAN, and adding a third overlay icon
increases cognitive load without providing actionable information. Location
detail lives in secondary surfaces (tooltip, status bar).

### Surface hierarchy

Each surface answers a different question:

| Surface | Question answered | Content |
|---------|------------------|---------|
| Overlay | Can I access this? | Available / Offline |
| Tooltip (hover) | Where is this stored? | "On this device" / "On stone-golden-summit" |
| Explorer info bar | What went wrong? | Actionable offline message via `CfReportSyncStatus` |
| Toast notification | Did something change? | On state *transition* only, not on startup |
| Context menu | What can I do? | "Show in Zen Garden" / "Why is this offline?" |

### Availability definition

A replica set is **online** if at least one of its member stones is
currently present in the garden registry with a recent beacon. It is
**offline** if the registry contains no live entry for any member stone.

Local and remote-online are both online. The distinction is a detail, not
a state.

---

## Alternatives Considered

### Three-state overlay (local / remote-online / offline)

- **Pros**: Maximum information density at a glance.
- **Cons**: OneDrive, Dropbox, and iCloud have trained users on two states
  (synced / not-synced). A third state requires learning. Users will
  misread it. Custom icons require a shell extension DLL (complex, fragile,
  consumes one of Windows' 15 global overlay slots).
- **Why not**: Information that requires learning to interpret fails the UX
  test. Local vs remote belongs in tooltips, not overlays.

### Custom shell extension overlay handler

- **Pros**: Full control over icons; can show any state.
- **Cons**: Requires a separate DLL registered in the Windows shell. The
  15-slot global limit is typically exhausted by Dropbox, Git tools, and
  antivirus. Breaks across Windows feature updates. High maintenance burden.
- **Why not**: CfApi's built-in states are sufficient and zero-maintenance.

### Red ✕ error state for offline via `CfReportSyncStatus`

- **Pros**: Unmistakable — users cannot miss it.
- **Cons**: Red ✕ is alarming. A stone that reboots triggers an error state
  for 30–90 seconds every time. Transient network blips become user-visible
  errors. Emotional cost exceeds informational value for a home NAS context.
- **Why not**: Reserve error states for persistent, user-actionable failures.
  Use pending (↑) for recoverable offline — it reads as "not yet" rather
  than "broken". Promote to error only after a configurable silence period
  (Phase 4 consideration).

### No overlay changes (status quo)

- **Pros**: Zero implementation cost.
- **Why not**: Silent failure erodes trust. Users have no signal that a
  storage is offline until they attempt access.

---

## Implementation Plan

### Phase 1 — IN_SYNC flag reflects reachability (Immediate)

**What**: `reconcile_placeholders` tracks reachability separately from
existence. Placeholders are created without `mark_in_sync()` when offline
and updated to IN_SYNC when they come online (and vice versa).

**Changes**:

- `mod.rs`: Extend `reconcile_placeholders` to receive a reachability set
  alongside the existence set. For each known storage, call
  `update_storage_placeholder_state(name, online)`.
- `placeholders.rs`: Add `update_storage_placeholder_state()` using
  `CfUpdatePlaceholder` (or the `cloud-filter` crate's equivalent) to
  toggle the IN_SYNC flag on an existing placeholder directory.
- `mod.rs`: Derive the reachability set from `registry` — a storage is
  reachable if its hosting stone has a live entry.
- On heartbeat pass: update ALL placeholder states (not just
  added/removed), so a stone that goes offline triggers the icon change
  within 60 s.

**Technical investigation**: Confirm `cloud-filter` crate exposes
`CfUpdatePlaceholder` or an equivalent for toggling IN_SYNC on an existing
directory placeholder. If not, call the Win32 API directly via `windows-rs`.

**Result**: Explorer shows ✓ for online storages and ↑ for offline ones.
State updates within one heartbeat cycle (~60 s) of a stone arriving or
departing.

---

### Phase 2 — Tooltip with stone name and locality

**What**: Populate the placeholder's custom blob with structured metadata
so the Explorer tooltip (and Details pane) shows where the storage lives.

**Changes**:

- `placeholders.rs`: Replace `blob(name.into())` with a structured blob
  encoding `{ stone: String, local: bool }` serialised as compact JSON or
  a fixed binary layout.
- `provider.rs`: In `fetch_placeholders`, read the blob from the sync root
  folder and surface it via `CF_PLACEHOLDER_STANDARD_INFO`.
- Explorer tooltip text (via `CF_PLACEHOLDER_BASIC_INFO::Description`):
  - Online + local: `"On this device"`
  - Online + remote: `"On stone-golden-summit"`
  - Offline: `"stone-golden-summit is not reachable"`

**Note**: The `cloud-filter` crate's current API surface for
`CF_PLACEHOLDER_BASIC_INFO` needs audit before committing to this shape.

---

### Phase 3 — Actionable offline message in Explorer info bar

**What**: When a storage transitions to offline, call
`CfReportSyncStatus` with a non-fatal, user-readable explanation that
appears in Explorer's yellow info bar above the file list.

**Message format**:
> *"stone-golden-summit is not reachable. Check that it's powered on and
> connected to your network."*

**Changes**:

- `mod.rs`: On each reconcile pass, when a storage transitions from online
  → offline, call `CfReportSyncStatus` with `CF_SYNC_STATUS` populated
  with `Description` and `DeviceId`.
- Clear the status when the storage comes back online.

**Constraint**: `CfReportSyncStatus` is per-file or per-directory, not
per-sync-root. Call it on the replica set placeholder directory.

---

### Phase 4 — Toast notifications on state transitions

**What**: Notify the user once when a storage goes offline or comes back,
via Windows Action Center. Do not notify on startup regardless of state.

**Rules**:
- Suppress all notifications for the first 120 s after moss starts.
- Notify on offline transition: *"'storage' is offline — stone-golden-summit
  is not reachable."*
- Notify on online recovery: *"'storage' is back online."*
- No repeat notifications for the same state — fire once per transition.

**Changes**:

- `mod.rs`: Track previous state per storage name in a `HashMap<String, bool>`.
  Fire notification only on flip.
- New helper: `notify_storage_state_change(name, online, stone_name)` using
  `windows-rs` WinRT notification API (`ToastNotificationManager`).
- Startup suppression: compare `Instant::now()` against a `started_at`
  captured when the watcher spawns.

---

## Impact

- **Explorer UX**: Offline storages are visibly distinct from online ones.
  No silent failures.
- **Reconcile loop**: Heartbeat pass grows to include state updates for all
  known placeholders, not just added/removed ones. Overhead is minimal
  (one `CfUpdatePlaceholder` call per placeholder per 60 s).
- **No API changes**: Purely infra-layer change in `cloud_filter/`.
- **Windows only**: CfApi is Windows-specific. No Linux impact.

---

## Open Questions

- Does `cloud-filter` 0.0.6 expose `CfUpdatePlaceholder` for toggling
  IN_SYNC on an existing directory? If not, is a version bump available,
  or do we call Win32 directly?
- What is the right silence window before promoting offline to a red ✕
  error state (Phase 4 extension)? 5 minutes? Configurable?
- Should the context menu "Why is this offline?" entry open the Moss
  web UI, or trigger a `rake diagnose` equivalent?

---

## References

- [STORAGE-0012 — Cloud Filter Architecture Rebuild](../decisions/STORAGE-0012-cloud-filter-rebuild.md)
- [STORAGE-0015 — StorageRouter and Domain Policy Extraction](../decisions/STORAGE-0015-cloud-drive-storage-router.md)
- [Microsoft CfApi — `CfUpdatePlaceholder`](https://learn.microsoft.com/en-us/windows/win32/api/cfapi/nf-cfapi-cfupdateplaceholder)
- [Microsoft CfApi — `CfReportSyncStatus`](https://learn.microsoft.com/en-us/windows/win32/api/cfapi/nf-cfapi-cfreportproviderprogressonexit)
- [`cloud-filter` crate](https://crates.io/crates/cloud-filter)
