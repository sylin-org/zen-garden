---
audience: [developer, ai]
doc_type: decision
status: proposed
last_verified: 2026-05-05
---

# STORAGE-0019: Candidate Storage Lifecycle and Foreign-Filesystem Adoption

**Date**: 2026-05-05
**Status**: Proposed
**Depends on**: STORAGE-0014 (Storage Platform Architecture), STORAGE-0017 (Volume State Machine), STORAGE-0018 (Device Health Monitor)
**Evolves**: STORAGE-0010 (Unified `storage add` Command)

## Context

### The Symptom

A user on tty1 of stone-golden-summit ran `garden-rake storage add` and was
shown two errors:

```
Error: No ready volumes found, but detected physical media:
  WDC WDS500G1B0C-00S6U0 (465.8 GB)  needs format
  RTL9210C (0 bytes)  needs partition
```

Both messages were misleading.

The first device was a 500 GB NVMe in a USB enclosure that contained the
user's tax returns on an NTFS volume — not "unformatted", just "has files
Moss won't classify as a clean managed device". Mossʼs eligibility
whitelist (`Empty | Unpartitioned | Unformatted` per
`garden_common::storage::DeviceState`) excluded `HasData`, so the user was
told to wipe a drive that they actually wanted to keep.

The second device was a transient USB-bridge handshake failure, not an
unpartitioned disk. Kernel logs showed:

```
sd 2:0:0:0: [sdc] Read Capacity(10) failed: Result: hostbyte=DID_OK driverbyte=DRIVER_OK
sd 2:0:0:0: [sdc] Sense Key : Illegal Request [current]
sd 2:0:0:0: [sdc] Add. Sense: Invalid command operation code
sd 2:0:0:0: [sdc] 0 512-byte logical blocks: (0 B/0 B)
```

The Realtek RTL9210C bridge returned "Illegal Request" to the SCSI
`READ CAPACITY (10)` command, so Linux honestly reported zero blocks. Three
hours later the same physical enclosure on a different USB port path
enumerated cleanly at 256 GB. The hardware was fine; the bridge firmware
was in a degraded state on one specific port and recovered after replug.
Moss had no way to distinguish "device communication failed" from "this
disk has no partitions" and surfaced the second as the first.

### Problem Class

These two failures share a root cause: **Mossʼs candidate classifier
collapses too many states into too few labels**. Today's taxonomy
(`MediumAction::NeedsPartition | NeedsFormat | Ready | AlreadyManaged | Unreadable`)
cannot distinguish:

| Real state | What Moss says today |
|---|---|
| Drive has user data on a non-Linux filesystem | "needs format" (suggests destruction) |
| USB enclosure with no media inserted | "needs partition" (suggests an action that's impossible on 0 bytes) |
| USB bridge in a transient communication-failure state | "needs partition" (same misleading label) |
| Drive genuinely raw with no partitions | "needs partition" (correct, but indistinguishable from the above) |

The user has no way to tell the cases apart from Moss's output, and no
recourse for the failure modes Moss isn't equipped to detect.

### Relationship to STORAGE-0018

STORAGE-0018 added a device-health probe to the *post-adoption* observe
cycle: managed Volumes get health-checked on every storage tick, with
auto-cleanup for stale removable devices. The signals it reads
(`/sys/block/sdX/device/state`, `ioerr_cnt`) are the same signals a
candidate classifier needs.

This ADR addresses the *pre-adoption* lifecycle phase: the moment between
hotplug detection and the user seeing a candidate in `storage add`. It
introduces a connectivity-recovery stage and a richer state model, then
plugs the resulting candidates into the existing `Volume` flow that
STORAGE-0018 monitors.

### Design Pressure: Moss-as-Appliance

The Zen Garden Moss appliance is intended to run on whatever hardware the
user has — Pentium Silver mini-PCs, off-the-shelf USB enclosures, NVMe
drives bought a year ago and reformatted on whatever computer was handy.
Many of those drives will be NTFS or exFAT because most users are on
Windows. An appliance that refuses to talk to a user's existing drives
isn't an appliance, it's a hobbyist project.

Two pressures, in tension:

- Be honest about what Moss can guarantee on filesystems it doesn't fully
  control (NTFS via ntfs3, exFAT, APFS via apfs-fuse).
- Don't punish the user for owning the wrong filesystem.

The decision below accepts both pressures simultaneously: foreign
filesystems become first-class participants with capability-bound roles,
not refused candidates.

## Decision

The candidate-storage pipeline gains an explicit connectivity-recovery
stage, a five-state classification, and a CLI verb split between
discovery, preserving adoption, and destructive formatting. Foreign
filesystems become first-class with a tier-based capability model. All
user-facing surfaces speak plain language with `<family> (<filesystem>)`
labels.

### 1. Pipeline architecture

A new `Connectivity Health` stage sits between the existing storage
listener and the classifier:

```
┌──────────────────┐    ┌──────────────────┐    ┌───────────────────┐
│  hotplug events  │ ─► │  Connectivity    │ ─► │  classifier +     │
│   (existing      │    │  Health          │    │  reconciler       │
│    listener)     │    │  (new stage)     │    │  (existing)       │
└──────────────────┘    └──────────────────┘    └───────────────────┘
```

The stage transforms an inbound `PhysicalStorageEvent` into a
`PhysicalStorageEvent + ConnectivityStatus`, attempting recovery if the
device is in a degraded state and forwarding the result either way. The
classifier sees more accurate input; downstream consumers see status
metadata they can render.

Recovery escalation, applied only to un-adopted devices:

1. **SCSI rescan** — `echo 1 > /sys/block/sdX/device/rescan`. Re-issues
   `INQUIRY` and `READ CAPACITY`. Cheap, safe, often enough.
2. **USB re-authorization** — `echo 0 > /sys/bus/usb/devices/<port>/authorized; sleep 2; echo 1 > .../authorized`.
   Soft replug; the kernel re-enumerates the device from scratch.
3. **Surface to user** — if both attempts fail, the candidate goes through
   to the classifier marked `Unreachable` with a friendly replug hint.

Boundary conditions:

- **Adopted devices are never auto-recovered.** Re-authorization kills
  inflight I/O. The recovery path runs only for devices not yet in the
  managed `Volumes` map.
- **Identity tracks the medium, not the block name.** USB re-auth may
  return `/dev/sdc` as `/dev/sdd`. The pipeline keys on
  `(usb_port_path, vendor_id, product_id, serial)` so retry budgets and
  state continuity survive renumbering.
- **Per-device retry budget.** One SCSI rescan + one USB re-auth per
  device per minute. Beyond that, the candidate stays `Unreachable`
  until the user intervenes physically.
- **Cancellable.** A physical unplug fires a `Removed` event from the
  listener; any in-flight recovery for that device aborts cleanly via a
  per-device `CancellationToken`.

### 2. Candidate state taxonomy

Five states replace today's two-action menu:

| State | Signals | What Rake offers |
|---|---|---|
| `Adoptable` | Has partitions, has user files | `adopt` (preserves) or `format` (wipes) |
| `Empty` | Filesystem present, zero user files | `adopt` (use as-is) or `format` (reformat) |
| `Raw` | No partition table or no filesystem | `format` (single full-size storage) |
| `NoMedia` | Bridge enumerated, 0 bytes, **no I/O errors** | "Insert a drive into the enclosure" |
| `Unreachable` | Bridge enumerated, 0 bytes, **I/O errors present** | Auto-heal; on failure, friendly replug hint |

`MediumCondition::Unreachable` already exists in the type
([garden_common::storage::MediumCondition](../../src/common/src/storage.rs))
but is not currently emitted by the classifier. The five-state taxonomy
gives every emission path a distinct condition variant; the existing
`Unreadable` variant is renamed to `Unreachable` for vocabulary
consistency (the device is reachable on the bus but unresponsive — not
unreadable per se).

The classifier distinguishes `NoMedia` from `Unreachable` by reading
`/sys/block/sdX/device/ioerr_cnt`. A genuinely empty enclosure does not
accumulate I/O errors; a glitched bridge does.

### 3. CLI verb surface

`add` becomes the discoverer; the destructive and preserving paths get
their own verbs.

| Verb | Action | Destructive? |
|---|---|---|
| `garden-rake storage` / `storage list` | Show currently managed storage | No |
| `garden-rake storage add` | Discovery: scan, classify, render the menu | No |
| `garden-rake storage adopt [set]` | Take existing data as-is. Default joins `storage` set; named arg → `storage::{set}` | No |
| `garden-rake storage format [set]` | Partition + format + add as fresh | YES |
| `garden-rake storage release` | Safely unmount | No |

Two invariants this enforces:

- The destructive verb is *named* `format`. There is no `--format` flag
  hidden behind a generic `add` that might be invoked by accident.
- `adopt` and `format` route through the same `POST /api/v1/stone/storage/add`
  HTTP endpoint that exists today. The CLI vocabulary changes; the wire
  contract does not.

When exactly one candidate matches the verb's state requirement, the
target argument is optional. When several candidates match, Rake
disambiguates by state (`format` only acts on `Raw` or `Empty` drives,
`adopt` only on `Adoptable`).

A read-only preview mount makes inspection ergonomic. Moss mounts
`Adoptable` candidates read-only at a predictable path
(`/mnt/zen-garden/preview/<device-tag>`) and includes the path plus a
sample of top-level entry names in the `storage add` output. The mount is
torn down when the candidate transitions out of `Adoptable` (adoption
succeeds, formatting succeeds, device removed) or after a TTL.

### 4. Foreign-filesystem support

Non-Linux filesystems become first-class via a tier-based capability
model declared at mount time:

```rust
// in garden_common::storage
pub enum FsTier {
    /// ext4 / btrfs / xfs — full Moss semantics.
    Native,

    /// NTFS / exFAT — read-write, with attribute caveats.
    /// Atomic rename, fsync, sparse files all present-but-different.
    /// Replication works; some POSIX-specific attributes flatten on
    /// cross-tier round-trips.
    Foreign,

    /// APFS / HFS+ — read-only on Linux today (apfs-fuse).
    /// Adopt as a library; never as a write target.
    ForeignReadOnly,
}

pub struct FsCapabilities {
    pub tier: FsTier,
    pub case_sensitive: bool,
    pub posix_permissions: bool,
    pub xattrs: bool,
    pub atomic_rename: bool,
    pub sparse_files: bool,
    pub max_filename_bytes: u32,
    pub forbidden_chars: &'static [char],
}
```

Capability-gated behaviors:

| Behavior | Native | Foreign | ForeignReadOnly |
|---|---|---|---|
| Adopt with existing files | ✓ | ✓ | ✓ (read-only mirror) |
| Format as fresh managed storage | ✓ | offered as "convert first" | not supported |
| Live participant in a replica set | ✓ | ✓ | ✗ |
| Take the Primary role | ✓ | ✓ | ✗ |
| Hold encrypted content (pond keystone) | ✓ | accepted, less ideal — NTFS uid/gid simulation cannot match POSIX 0600 file mode enforcement | ✗ |
| Pavilion / Cloud Filter access | ✓ | ✓ | ✓ |

**Tier is observable, not enforced.** The original draft of this ADR
proposed an election tie-breaker that auto-preferred Native over Foreign
for the Primary role. We dropped it: silently demoting a user's working
NTFS drive when they later add a Native peer is paternalistic and the
kind of surprise that erodes trust in an appliance. The user-visible
levers are explicit:

- `garden-rake storage pin <name>` — claim Primary on a specific drive
  when the user wants that drive to lead writes.
- `garden-rake storage migrate <name>` — convert a drive from Foreign
  to Native in place when the user is ready (planned, see Open Questions).
- `garden-rake storage info <name>` — surface the tier and capabilities
  so users who care can read what they're working with.

Foreign drives become Primary the same way Native ones do (whoever was
first, modulo explicit pinning) and stay Primary unless the user
intervenes. The `<family> (<fs>)` labels and `storage info` output make
the tier visible at all times; documentation guides users toward
migrating replication-heavy workloads to Native filesystems when it
matters.

**Dormant ≠ inactive.** Every replica in a set, Primary or Dormant, holds
the same data and stays live-synced via the changelog stream. Primary is
the current write coordinator; Dormant is the current secondary, fully
caught up, ready to take over if asked.

### 5. Plain-language presentation

Every user-facing surface uses `<family> (<filesystem>)` labels:

| Family | Filesystems shown as |
|---|---|
| **Linux** | ext2, ext3, ext4, btrfs, xfs, f2fs, zfs |
| **Windows** | NTFS, FAT16, FAT32, exFAT, ReFS |
| **Mac** | HFS+, APFS |
| **Optical** | ISO9660, UDF |
| **Other** | bare filesystem name in parens, family elided |

`storage list`:

```
$ garden-rake storage list
stone-golden-summit
  ● storage           238 GB  Windows (NTFS)   · primary (sole replica)
  ● storage::archive  916 GB  Linux (btrfs)    · primary

stone-coral-prairie
  ● storage::archive  916 GB  Linux (btrfs)    · dormant   (in sync, ~3 s behind)
```

`storage add` discovery:

```
$ garden-rake storage add
stone-golden-summit · scanning attached storage…

  ▸ Realtek RTL9210C — 256 GB NVMe (USB)
    Has data: Windows (NTFS), 2.4 GB used
    First few entries: photos/, work/, $RECYCLE.BIN/

      garden-rake storage adopt              join the 'storage' set (preserves files)
      garden-rake storage adopt media        join 'storage::media' (preserves files)
      garden-rake storage format             wipe and start fresh

      Inspect first: ls /mnt/zen-garden/preview/rtl9210c-pa1
        (Moss mounts candidates read-only here so you can browse before deciding)

  ▸ Samsung 990 PRO — 1 TB NVMe (NVMe, M.2 slot 0)
    Blank: no partition table.

      garden-rake storage format             partition + format + add as 'storage'
      garden-rake storage format archive     same, into 'storage::archive'

Tip: adopt preserves your files; format wipes the drive.
```

`storage adopt` confirmation — three bullets, no jargon:

```
Adopt 'Realtek RTL9210C' (256 GB · Windows (NTFS)) into the 'storage' set?

  • Your files stay where they are — 2.4 GB cataloged, nothing moved.
  • Read, write, and sharing all work.
  • The garden's other drives stay in sync with this one.

  Continue? [Y/n]
```

`storage format` confirmation — explicit `yes` typed in full because the
action is irreversible:

```
Format 'Samsung 990 PRO' (1 TB) and add as 'storage'?

  • Filesystem: Linux (btrfs)
  • Single partition spanning the whole drive
  • Anything currently on the drive will be erased

  Type 'yes' to continue:
```

Trailing migrate hint after a successful adopt of a Foreign drive — one
line, never repeated, no banner:

```
✓ Adopted into 'storage'.

  Tip: 'garden-rake storage migrate' can move your files onto a Linux
  filesystem on the same drive when you're ready — fully optional, your
  data is fine where it is.
```

The `storage migrate` workflow is deferred to a future ADR (see Open
Questions); the trailing hint is forward-compatible scaffolding.

The `--explain` flag and `garden-rake storage info <name>` provide the
long-form caveats (POSIX permission flattening, atomicity differences,
when migrating to a Native filesystem is worth it) for users and docs
that want them, without cluttering the default flow.

### 6. Notification surface

When the connectivity stage performs a successful recovery, it emits to
three surfaces simultaneously:

**tty1 line** via `wall` to active sessions on the stone:

```
↻ Recovered Realtek RTL9210C on USB port 2-3.4
   Communication hiccup during enumeration · resolved by USB reset · 4.4s
   Drive now visible: 256 GB NVMe · run 'garden-rake storage add' to use it
```

**SSE event** on the unified pulse stream — Pavilion's tray and Rake's
`presence stream` both consume this:

```json
{
  "event": "storage.connectivity.recovered",
  "stone": "stone-golden-summit",
  "timestamp": "2026-05-05T03:51:42Z",
  "device": {
    "vendor": "Realtek", "model": "RTL9210C",
    "size_bytes": 256060514304, "usb_port": "2-3.4"
  },
  "recovery": {
    "attempts": 1,
    "actions": ["scsi_rescan", "usb_reauth"],
    "duration_ms": 4380
  }
}
```

**`storage list` note** — a one-line preface above the candidates
section, surfaced the next time the user runs the command:

```
ⓘ One device required a USB reset to enumerate — see 'garden-rake stone logs' for details.
```

Coalescing rule: if the same device flaps repeatedly (recovers, fails,
recovers within a minute), only the first recovery emits a user-facing
message. Subsequent recoveries are logged at `info` but suppressed from
tty1, the pulse stream, and `storage list`. This keeps tty1 quiet for
genuinely flaky hardware while preserving the audit trail for support.

### 7. Status companion, not status replacement

`ConnectivityStatus` rides alongside the medium event, never replacing
it. Even on success, the status records what happened:

```rust
pub struct ConnectivityStatus {
    pub recoveries_attempted: u32,
    pub recovered_via: Option<RecoveryAction>,
    pub residual_warnings: Vec<ConnectivityWarning>,
}
```

A drive that needed a USB re-auth still arrives at the classifier with
the recovery noted in its status — useful telemetry for "this drive is
flaky, watch it" without blocking adoption. The classifier and Rake can
choose to surface or suppress the status; the data is always there.

## Alternatives Considered

### Ship `usb-storage.quirks=0bda:9210:u` in the appliance image

Forcing all RTL9210/B/C bridges into BOT (Bulk-Only Transport) mode
sidesteps the SCSI command path that misbehaved.

- **Pros**: One-line fix; eliminates the failure mode entirely.
- **Cons**: ~50–60% throughput drop on every working RTL9210 enclosure;
  applies a blanket rule to a vendor/product family without runtime
  evidence; locks the appliance into a workaround that may become stale
  as the upstream kernel improves; signals "Zen Garden expects RTL9210
  bridges to be broken" by default.
- **Why not**: The fix lives at the wrong layer. A bridge handshake
  glitch is a hardware-quirk problem, but the user-visible failure is
  a UX problem (Moss reported the wrong thing). Fixing the application
  layer makes the appliance handle this *and* every similar future
  hardware quirk gracefully without the kernel commitment.

### Refuse to adopt foreign filesystems

Reject NTFS / exFAT / APFS at adoption time and require the user to
reformat to ext4 or btrfs before adding storage.

- **Pros**: Strongest semantic guarantees; predictable replication
  behavior; smaller test matrix.
- **Cons**: Punishes users for owning the hardware they own; converts a
  personal-use appliance into a hobbyist tool; many users would simply
  not adopt their existing drives, leaving the garden empty.
- **Why not**: Moss is meant to run on whatever the user has. Saying
  no to common filesystems contradicts the premise.

### `--format` flag on `add`

Keep the existing single `add` verb and gate destruction behind a flag.

- **Pros**: Smaller CLI surface; backward compatible with STORAGE-0010
  vocabulary.
- **Cons**: Hides destructive intent inside a generic verb; flags get
  forgotten or auto-completed; `--format` next to `--quiet` and other
  ergonomic flags trivializes the wipe.
- **Why not**: Naming the destructive action `format` makes every wipe
  an act of typed consent. The trade-off (one more verb to remember)
  is worth the safety.

### Quiet-success on connectivity recovery

When recovery succeeds, log at `info` but emit nothing to tty1 or the
pulse stream. The user only sees output if recovery fails.

- **Pros**: Less noise; aligns with the "appliance just works" aesthetic.
- **Cons**: When recovery does happen, the user has no signal that Moss
  did anything on their behalf. Trust is built by transparency, and a
  silent self-heal looks identical to "nothing happened".
- **Why not**: User explicitly chose loud-success ("a successful
  recovery is worth a tty1 message"). Coalescing handles the noise
  concern for genuinely flaky hardware.

### Implement `storage migrate` (Foreign → Native conversion) in this round

Ship the workflow that copies data to a peer, wipes the drive, formats
as btrfs, and re-syncs.

- **Pros**: Closes the foreign-filesystem story end-to-end; the trailing
  migrate hint becomes a working command instead of forward-compatible
  scaffolding.
- **Cons**: Adds non-trivial scope (replica safety check, drive locking,
  format orchestration, peer pull) to a milestone that's already
  redesigning detection, classification, and CLI vocabulary.
- **Why not**: The adopt path is what users hit on day one. Migrate is
  a "weeks later, when I've decided to fully commit" workflow that
  benefits from real usage feedback before we shape it. Defer to its
  own ADR.

## Consequences

### Positive

- The user sees what's actually true about their drive. "Has files,
  Windows-formatted" is what NTFS volumes look like, and that's what
  Moss says.
- Self-healing for transient bridge glitches happens silently when
  possible and transparently when it succeeded — neither surprising
  nor invisible.
- Moss accepts whatever filesystem the user has. No one is told their
  hardware doesn't qualify.
- Vocabulary serves casual and technical users in the same column. No
  second pane needed for "but what filesystem really".
- The destructive verb is named `format`. Every wipe is consented to
  by typing the word.
- The wire contract doesn't change. New states ride on existing types,
  the HTTP API is unchanged, Pavilion's Cloud Filter read path works
  against any tier transparently.
- STORAGE-0018's monitoring loop and this ADR's recovery stage share
  diagnostic signals (`ioerr_cnt`, `device/state`). One sysfs reader
  feeds both.

### Negative

- Recovery adds latency to enumeration when it triggers — up to ~10
  seconds per device for a SCSI rescan + USB re-auth + re-discovery
  pass. Mitigated by per-device parallelism (one slow drive doesn't
  block another) and per-device retry budgets (no infinite loops).
- Foreign drives in the Primary role have weaker semantic guarantees
  than Native ones (no POSIX perms, NTFS-specific atomicity edge
  cases). Mitigated by visibility (the tier appears in `storage list`
  and `storage info`) plus explicit user controls (`storage pin`,
  future `storage migrate`). The system never silently demotes a
  Foreign drive when a Native peer arrives.
- The coalescing heuristic can suppress messages a user wanted to see.
  "Recovered, recovered again, recovered again within a minute" emits
  one line on tty1, which may understate a chronic issue. Mitigated by
  always logging recoveries to journald; the support escape hatch is
  `garden-rake stone logs`.
- The CLI now has three verbs (`add`, `adopt`, `format`) where there
  was one. Users have to remember which is which. Mitigated by `add`
  becoming the always-safe discovery entry — it shows the right verb
  to copy-paste for the desired action.

### Neutral

- `MediumInfo` and `StorageInfo` gain new optional fields
  (`fs_capabilities`, `connectivity_status`). Defaults preserve
  backward compatibility on existing manifests; the wire format
  remains a lowercase `filesystem: String` for the canonical token.
- `MediumCondition::Unreadable` is renamed to `Unreachable` to match
  the rest of the vocabulary. Any persisted manifests using the old
  name migrate on first read (serde alias).
- The classifier stops emitting `Ready` for `NoMedia` candidates.
  Tooling that filtered candidates by `NeedsPartition` should switch
  to filtering by the new five-state taxonomy.

## Files Affected

### `garden_common`

| File | Change |
|---|---|
| `src/common/src/storage.rs` | Add `FsTier`, `FsCapabilities`, `ConnectivityStatus`, `ConnectivityWarning`, `RecoveryAction`. Add the five-state `MediumCondition` variants (`Adoptable`, `Empty`, `Raw`, `NoMedia`, `Unreachable`). Add the filesystem-label render function. Round-trip tests. |

### Moss infra

| File | Change |
|---|---|
| `src/moss/src/infra/storage/connectivity/mod.rs` *(new)* | Pipeline entry: `evaluate(event) → (event, ConnectivityStatus)`. |
| `src/moss/src/infra/storage/connectivity/probe.rs` *(new)* | Read `ioerr_cnt`, `iotmo_cnt`, `device/state`, `size`. Decide whether the device is healthy, recoverable, or beyond reach. |
| `src/moss/src/infra/storage/connectivity/recovery.rs` *(new)* | SCSI rescan and USB re-authorization implementations with per-device backoff and cancellation. |
| `src/moss/src/infra/storage/connectivity/outcome.rs` *(new)* | `ConnectivityOutcome` and supporting types. |
| `src/moss/src/infra/storage/monitor/mod.rs` and `monitor/linux.rs` | Route `PhysicalStorageEvent` through the connectivity stage before forwarding. |
| `src/moss/src/infra/storage/platform.rs` | Populate `FsCapabilities` from `blkid` output. Distinguish `NoMedia` from `Unreachable` using `ioerr_cnt`. Mount candidates read-only at `/mnt/zen-garden/preview/<device-tag>`. |
| `src/moss/src/infra/storage/os_platform.rs` | Extend `StoragePlatform` trait if probe primitives are not already exposed via STORAGE-0018. |

### Moss domain

| File | Change |
|---|---|
| `src/moss/src/domain/storage/*` | Classifier emits the five new states. `Volume` carries `FsCapabilities` — observable, not enforced. Election logic is unchanged: whoever was first stays Primary unless the user pins or migrates explicitly. |
| `src/moss/src/domain/storage/health.rs` | Surface `ConnectivityStatus` alongside the existing health view. |

### Notifications

| File | Change |
|---|---|
| `src/moss/src/infra/listeners/*` | `storage.connectivity.recovered` event on the pulse stream. Coalescing window per device. |
| `src/moss/src/infra/console_tty.rs` *(new or extension)* | `wall` integration for tty1 line on recovery. |
| `src/moss/src/api/v1/garden_storage/*` | `storage list` response includes the recent-recovery note when applicable. |

### Rake CLI

| File | Change |
|---|---|
| `src/rake/src/commands/storage/mod.rs` | Verb registration for `adopt` and `format`; `add` becomes the discoverer. |
| `src/rake/src/commands/storage/add.rs` | Render the five-state candidate menu with `<family> (<fs>)` labels and per-state suggested verbs. |
| `src/rake/src/commands/storage/adopt.rs` *(new)* | Three-bullet confirmation; `[Y/n]`; `--explain` flag. |
| `src/rake/src/commands/storage/format.rs` *(new)* | Explicit `yes`-typed confirmation; `--fs` flag for filesystem choice. |
| `src/rake/src/commands/storage/info.rs` *(new)* | Long-form per-storage detail including capability tier, residual warnings, and the explicit `pin` / `migrate` paths. |
| `src/rake/src/commands/storage/list.rs` | Render `<family> (<fs>)` labels, dormant peer status with sync lag, recent-recovery note. |

### Tests

| File | Change |
|---|---|
| `src/common/src/storage.rs` | Round-trip tests for new types. |
| `src/moss/tests/connectivity_recovery.rs` *(new)* | Synthetic sysfs harness; verify recovery escalation, retry budgets, cancellation. |
| `src/moss/tests/foreign_filesystem_classification.rs` *(new)* | Mounted NTFS / exFAT / APFS fixtures (where feasible) → expected `FsCapabilities` and tier assignment. |

### Docs

| File | Change |
|---|---|
| `docs/guides/foreign-filesystems.md` *(new)* | User-facing guide. What "Linux-formatted" / "Windows-formatted" / "Mac-formatted" mean in Moss. The trailing migrate hint and what it does. |
| `docs/specs/discovery.md` or new `docs/specs/storage-candidates.md` | The five-state taxonomy as a spec. |
| `docs/reference/connection-strings.md` | Update if any new env vars surface for connectivity tuning. |

## Implementation Sequence

The work decomposes into seven self-contained units that can be reviewed
and merged independently. Earlier units unblock later ones, but each
ships a coherent slice.

1. **Wire types** — `FsTier`, `FsCapabilities`, `ConnectivityStatus`, the
   five-state `MediumCondition`, the label render function. All in
   `garden_common::storage`. Round-trip tests. No behavior changes.
   **Risk**: low. **Unblocks**: everything else.

2. **`probe.rs`** — sysfs reader that produces `ConnectivityStatus`
   without mutating anything. Pure functions over `/sys` paths;
   testable with synthetic sysfs trees in `tempfile`.
   **Risk**: low. **Hardware required**: no.

3. **`recovery.rs`** — SCSI rescan + USB re-auth implementations.
   Cancellation-aware. Per-device retry budget. Tests against the
   live stone (`stone-golden-summit`'s RTL9210C bridge) confirm the
   real escalation works; CI runs synthetic-only.
   **Risk**: medium (touches /sys writes, requires root).
   **Hardware required**: yes for full coverage; synthetic for CI.

4. **Pipeline wiring** — insert the connectivity stage in
   `infra/storage/monitor.rs`. Forward events with attached status. The
   classifier still emits the old conditions; only the new field is
   present.
   **Risk**: low. **Behavior change**: status is observable but no
   user-facing surface uses it yet.

5. **Classifier rewrite** — emit the five new conditions. Distinguish
   `NoMedia` from `Unreachable` via `ioerr_cnt`. Populate
   `FsCapabilities` from `blkid`. Read-only preview mount.
   **Risk**: medium (changes user-visible classification).
   **Tests**: existing `storage list` integration tests + new fixtures
   for each state.

6. **Rake CLI verbs** — `adopt` and `format` as new commands; `add`
   redesigned as discovery. `<family> (<fs>)` labels everywhere.
   `--explain` flag. `storage info`.
   **Risk**: medium (CLI surface change; existing scripts using
   `storage add /dev/sdX --format` need to migrate to `storage format /dev/sdX`).
   **Mitigation**: keep the old form working for one release cycle
   with a deprecation note.

7. **Notification surface** — pulse SSE event, `wall`-to-tty1 line,
   `storage list` recent-recovery note. Coalescing window.
   **Risk**: low. **Independent of**: everything else; can land last.

The originally-proposed election tie-breaker (Native > Foreign for
Primary preference) was dropped after design review: silent
auto-demotion is paternalistic and the kind of surprise that erodes
trust in an appliance. The user-visible levers (`storage pin`, future
`storage migrate`) plus tier visibility in `storage list` /
`storage info` cover the cases that matter without hidden behavior.

## Open Questions

- **`storage migrate` workflow scope.** The trailing hint after adopt is
  forward-compatible scaffolding. The actual workflow (replica safety
  check → drive lock → format → peer pull → handback) belongs in its
  own ADR. Worth opening a tracker for it now so the trailing hint
  doesn't dangle indefinitely.
- **APFS read-only enablement.** This ADR declares `ForeignReadOnly` but
  the apfs-fuse package isn't in the appliance image today. Whether to
  add it (and accept the FUSE userspace dependency) or defer
  ForeignReadOnly until a kernel APFS driver lands upstream is a
  separate decision.
- **`storage migrate` UX symmetry.** If migrate ends up as `storage migrate`,
  should there be a parallel `storage convert` for "I want to keep this
  drive Foreign but switch which set it belongs to"? Probably yes, but
  out of scope until usage data shows it matters.
- **Encrypted-content-on-Foreign.** The capability table marks this
  "accepted, less ideal" because NTFS uid/gid simulation can't enforce
  POSIX 0600 file mode the way ext4 can. For a personal-use appliance
  where the drive is physically with the user, this is probably fine. If
  Moss ever ships in a multi-tenant context, this row needs to flip to
  "✗".
- **Coalescing window tunability.** One minute is a reasonable default
  for "this is one event the user should see once" vs "this is chronic
  and needs investigation". Whether to expose the window as a config
  knob or hardcode it is a usage-data question.

## References

- [STORAGE-0014](STORAGE-0014-storage-platform-architecture.md) —
  the platform abstraction this ADR extends.
- [STORAGE-0017](STORAGE-0017-volume-state-machine.md) — Volume domain
  object that adopted candidates flow into.
- [STORAGE-0018](STORAGE-0018-device-health-monitor.md) — health probing
  for already-adopted volumes; shares signal sources with this ADR's
  candidate-stage probing.
- [STORAGE-0010](STORAGE-0010-unified-storage-add-command.md) — the CLI
  shape this ADR evolves.
- [STORAGE-0013](STORAGE-0013-replica-set-identity.md) — replica set
  identity model that `adopt [set]` and `format [set]` consume.
- [PAVILION-0001](PAVILION-0001-windows-client-separation.md) — Cloud
  Filter consumer of the storage data plane.
