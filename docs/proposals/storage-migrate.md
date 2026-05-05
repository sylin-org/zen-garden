---
audience: [contributor]
doc_type: proposal
status: draft
last_verified: 2026-05-05
---

# Storage Migrate (Foreign → Native, In-Place) Proposal

**Author**: Leo Botinelly
**Date**: 2026-05-05

---

## Problem Statement

[STORAGE-0019](../decisions/STORAGE-0019-candidate-lifecycle-and-foreign-filesystem-adoption.md)
gives users a path to adopt drives in any filesystem Moss recognizes,
including Windows- and Mac-formatted drives. After adoption, the
Foreign tier (NTFS, exFAT) participates fully in replication and the
data plane, with one structural difference: when a replica set has a
mix of Native and Foreign drives, Native drives win the Primary tie-
breaker and Foreign drives stay Dormant.

Some users will later decide to convert a Foreign-tier drive to a
Native one — for stronger fsync semantics, replication priority in
mixed sets, or just consistency across the garden. Today the manual
path is:

1. `storage release` to safely unmount.
2. Reformat the drive externally (e.g. via `mkfs.btrfs`).
3. `storage add` to re-adopt under the new filesystem.

That works but loses the replica-safety automation Moss is otherwise
good at: the user has to remember to verify a peer holds the data
before wiping the drive.

The `garden-rake storage adopt` flow already places a forward-
compatible "Tip: storage migrate ..." line in its success output,
implying a workflow that doesn't yet exist.

## Proposed Solution

`garden-rake storage migrate <name>` — a single command that converts
a Foreign-tier drive to a Native filesystem in place, with replica
safety built in.

### Behavior

```text
$ garden-rake storage migrate storage::archive
Migrate 'storage::archive' from Windows (NTFS) to Linux (btrfs)?

  Replica check:
    ✓ stone-coral-prairie  916 GB  Linux (btrfs)  · primary, in sync
    ✓ stone-emerald-vale   916 GB  Linux (btrfs)  · dormant, in sync

  Plan:
    1. Lock the drive on this stone (no writes during migration).
    2. Format as Linux (btrfs).
    3. Pull a fresh replica from the peer.
    4. Hand back to the replication pipeline.

  Estimated time: ~22 min for 2.4 GB at observed sync rate.
  This drive will be unavailable during the migration. Continue? [y/N]
```

### Required preconditions (verified before any destructive action)

- At least one Native-tier replica of the same set exists on a
  reachable peer, AND that peer's replica is fully in sync (no
  pending changelog entries).
- The drive being migrated is currently Dormant, OR the user
  explicitly accepts a brief Primary handoff to a peer.
- The drive isn't pinned to this stone (or the user explicitly
  confirms unpinning as part of the migrate plan).
- The user typed `yes` in full to the confirmation, given the
  irreversibility.

### Workflow steps

1. **Preflight** — verify the preconditions above, render the plan,
   request confirmation.
2. **Quiesce** — if the drive is Primary, hand off to a peer and wait
   for the handoff to settle.
3. **Lock** — mark the storage entry as "migrating" so the rest of
   Moss doesn't try to write to it.
4. **Format** — run `mkfs.btrfs` (or `mkfs.ext4` if the user
   overrides) on the underlying device.
5. **Re-mount** — under a fresh `.zen-garden/` manifest using the
   same replica set ID and name as before.
6. **Pull** — replicate from the chosen peer; report progress to
   tty1 and the SSE stream.
7. **Re-join** — clear the migrating flag; the storage rejoins the
   replica set as a fully-current Native member.

### Failure modes and rollback

- **Format fails** — drive becomes unrecoverable on this stone. The
  data is still on the peer; user can retry or run `storage add` to
  re-adopt manually.
- **Pull fails or is interrupted** — the partially-replicated drive
  is treated like any partial replica: the changelog stream resumes
  on next opportunity.
- **Network partition during pull** — same as the above; replication
  is resumable.
- **Stone reboots mid-migration** — the "migrating" flag persists in
  the manifest; on next boot Moss checks whether the drive is
  formatted and either resumes the pull or reports the partial state
  for the user to resolve.

## Alternatives Considered

### Manual reformat with documentation

- **Pros**: No new code; users follow `storage release` → external
  reformat → `storage add`.
- **Cons**: Users must manually verify peer-replica safety. One
  mistake (forgetting the peer is still catching up) destroys the
  data.
- **Why not**: The whole point of an appliance is that destructive
  workflows have safety built in. Documenting the pitfall isn't
  enough.

### Format-and-restore from external backup

- **Pros**: Works even with no peer.
- **Cons**: Requires the user to maintain a separate backup; doesn't
  use the garden's existing replication.
- **Why not**: Garden replication IS the safety net. Layering a
  separate backup requirement on top of it defeats the model.

### Refuse migration; require fresh adoption only

- **Pros**: No new code at all.
- **Cons**: Users with an NTFS drive that has years of data they
  don't want to lose end up frozen — they can adopt-as-NTFS but
  never upgrade.
- **Why not**: STORAGE-0019 explicitly accepts Foreign tier as a
  first-class option. Migration is the natural counterpart that lets
  users move along the tier ladder when ready.

## Impact

**New surfaces:**

- `POST /api/v1/stone/storage/banks/{name}/migrate` — initiate
  migration. Body: `{filesystem: "btrfs"|"ext4", peer_stone_id: ...}`.
- `GET /api/v1/stone/storage/banks/{name}/migrate/status` — progress
  (preflight / quiesce / format / pull / re-join, with
  bytes-transferred and ETA).
- `garden-rake storage migrate <name>` — CLI verb.
- A new `MigrationState` enum on `StorageManifest` so the migrating
  state survives reboots.

**Existing surfaces unchanged:**

- Wire format for `MediumCondition`, `FsCapabilities`,
  `ConnectivityStatus` — STORAGE-0019 already lays this groundwork.
- Replica set identity — migration preserves the replica set ID and
  name, so peers don't see a "new" set appear.
- The `adopt` / `format` verbs — those handle the initial-adoption
  workflow; `migrate` is the in-place conversion verb.

**What gets harder:**

- The replica set's data plane needs to gracefully handle a peer that
  briefly disappears (during format) and reappears (during pull).
  The existing eventual-consistency model already supports this; the
  migrate workflow just exercises it intentionally.

## Open Questions

- **Should the user choose btrfs vs ext4?** Default to btrfs for
  COW + checksums; allow `--fs ext4` as an override. Or pick one and
  document the choice.
- **Estimated-time accuracy.** The preflight ETA depends on observed
  sync rate to the chosen peer. Worth a short calibration window
  before showing a number, vs. saying "depends on peer link"?
- **Concurrent migrations.** If a user kicks off two migrations at
  once on the same stone, do they queue or run in parallel? Likely
  queue (stone has finite write bandwidth), but worth deciding.
- **Migrating a Mac-formatted drive.** APFS adoption is read-only,
  so technically there's no "active" replica to preserve. Migration
  becomes "wipe and rebuild from peer" — a degenerate case the
  workflow should handle cleanly.

## References

- [STORAGE-0019](../decisions/STORAGE-0019-candidate-lifecycle-and-foreign-filesystem-adoption.md) —
  the foreign-filesystem adoption work this proposal completes.
- [STORAGE-0006](../decisions/STORAGE-0006-seed-bank-replication.md) —
  changelog-based replication that the pull phase uses.
- [STORAGE-0013](../decisions/STORAGE-0013-replica-set-identity.md) —
  replica set identity model that survives migration.
- [foreign-filesystems guide](../guides/foreign-filesystems.md) — the
  user-facing companion piece referencing this workflow.
