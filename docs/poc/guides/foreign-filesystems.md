---
audience: [operator]
doc_type: guide
status: current
last_verified: 2026-05-05
---

# Foreign Filesystems

How Moss handles drives formatted for Windows or Mac alongside drives
formatted for Linux, and what to expect when you adopt or share them.

---

## The Short Version

Moss accepts your drives as they are. A USB stick formatted for
Windows works as managed storage; an external NVMe formatted on a Mac
shows up as a read-only library. You don't need to reformat anything
to bring a drive into the garden.

When `garden-rake storage list` shows your drives, the format appears
in the third column:

```text
stone-golden-summit
  ● storage           238 GB  Windows (NTFS)   · primary (sole replica)
  ● storage::archive  916 GB  Linux (btrfs)    · primary
  ● storage::shared   500 GB  Windows (exFAT)  · primary
  ● storage::media    2 TB    Mac (APFS)       · primary, read-only
```

A drive's format determines one thing: whether Moss can write to it.
Native (Linux) and Foreign (Windows) drives both adopt, replicate,
and share through Pavilion's Cloud Filter the same way. Mac-formatted
drives adopt as read-only libraries — Linux can read them but not
write to them reliably.

Moss never auto-promotes or auto-demotes a drive based on its format.
Whoever was Primary first stays Primary; the user's `storage pin`
command is the explicit lever for changing that.

---

## Three Tiers, by Capability

Moss groups filesystems into three operational tiers based on what
the underlying Linux driver can guarantee.

### Linux-formatted (Native)

ext2, ext3, ext4, btrfs, xfs, f2fs, zfs.

Full Moss semantics. Atomic rename, fsync, POSIX permissions, extended
attributes, sparse files all work as Moss expects them to. These are
the formats Moss reaches for when it picks a primary write coordinator
in a replica set with mixed formats.

### Windows-formatted (Foreign)

NTFS, exFAT, FAT32, FAT16, ReFS.

Read-write supported through the Linux ntfs3 / exfat kernel drivers.
Replication works in both directions — your files round-trip cleanly
between a Windows-formatted drive on one stone and a Linux-formatted
drive on another. A few things flatten on cross-tier round-trips:

- POSIX permission bits (`0644`, `0700`, etc.) — NTFS doesn't have an
  equivalent, so they default when files cross tiers.
- Linux extended attributes — same story; not represented on NTFS.
- Case sensitivity — NTFS folds case by default, so `Photo.JPG` and
  `photo.jpg` collapse to one file when copied to NTFS.

For a personal-use appliance these caveats are usually invisible.
If they matter for a particular workload, the rule of thumb is to
keep that workload on a Linux-formatted drive.

### Mac-formatted (ForeignReadOnly)

APFS, HFS+.

Linux can read these formats but not write to them reliably. Moss
adopts Mac-formatted drives as read-only libraries: they appear in
`storage list`, the files browse through Pavilion's Cloud Filter, and
they participate in the garden's discovery — but the garden never
writes to them, and they never take the Primary role.

---

## Adopting a Drive That Has Files

Plug the drive in, then on the stone:

```text
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
```

`storage adopt` keeps every file where it is. Moss writes a
`.zen-garden/` folder onto the drive to track replication state; the
rest of the contents are unchanged.

```text
$ garden-rake storage adopt
Adopt 'Realtek RTL9210C' (256 GB · Windows (NTFS)) into the 'storage' set?

  • Your files stay where they are — 2.4 GB cataloged, nothing moved.
  • Read, write, and sharing all work.
  • The garden's other drives stay in sync with this one.

  Continue? [Y/n]
```

After adoption, the drive shows up in `storage list` with its tier
visible in the format column. Pavilion's Explorer view picks it up
automatically; other stones in the garden see it through topology
sync within a few seconds.

`garden-rake storage info <name>` exposes the operational details if
you want to see exactly which capabilities the drive carries:

```text
$ garden-rake storage info storage::media
  Filesystem: NTFS  (Windows-formatted, foreign tier)
  Capabilities:
    case-sensitive:    no
    POSIX permissions: no
    extended attrs:    no
    atomic rename:     yes
    sparse files:      yes
  Replica set: storage::media (1 replica)
  Role: primary
```

---

## Mixed Replica Sets

When a replica set has drives in both tiers, Moss treats them as
equal peers. Whichever drive was Primary stays Primary; the others
sync in real time through the changelog stream regardless of format.

```text
stone-golden-summit
  ● storage::archive  238 GB  Windows (NTFS)   · primary

stone-coral-prairie
  ● storage::archive  916 GB  Linux (btrfs)    · dormant (in sync, ~3 s behind)
```

If you'd rather have the Linux-formatted drive lead writes — for
heavier replication workloads or strict POSIX semantics — pin it:

```text
$ garden-rake storage pin storage::archive --on stone-coral-prairie
```

For a one-time conversion of the Foreign drive itself (move its
files onto a Linux filesystem on the same physical drive), use
`garden-rake storage migrate` (planned, see "Reformatting Later"
below).

---

## Reformatting Later

Adoption is reversible. If you decide later that you'd rather have
the drive on a Linux filesystem (faster replication, slightly stronger
guarantees), the path is:

1. Make sure another stone holds a replica of the same set, so your
   files are safe before you wipe the drive.
2. Run `garden-rake storage migrate <name>` (planned, see open items
   below).
3. Moss will format the drive as btrfs and re-sync from the peer.

The migrate workflow is forward-compatible scaffolding today; until
it ships, the manual path is `storage release` → reformat → `storage
add`. Your files remain on the original drive throughout if you keep
the replica.

---

## Cloud Filter / Pavilion

Pavilion's Cloud Filter view in Windows Explorer works against any
tier transparently. Reading a file on an NTFS-adopted drive is
indistinguishable from reading one on a btrfs-adopted drive — the
StoneApi serves bytes either way, and the local cache is the same.
Writes from Pavilion only land on tiers that accept writes, so a
Mac-formatted (read-only) drive shows files but rejects modifications.

---

## When to Reach for a Native Format

Moss doesn't push you toward Linux-formatted drives — but a few
patterns work better there:

- **Heavy concurrent I/O.** btrfs's COW and ext4's journaling handle
  many writers more gracefully than NTFS via ntfs3.
- **Workloads that depend on POSIX permissions.** NTFS can't enforce
  Unix mode bits; if a service expects `0600` to mean something,
  it'll need a Linux-formatted volume.
- **Replication-heavy roles.** A `seed-bank` that accepts replication
  from many peers is happiest on a Linux-formatted Primary.

The good news: Moss flags these in the operational view before they
become a problem. The Foreign tier is supported, not penalized — you
just see a small tier label that says what trade-off applies.

---

## See Also

- [STORAGE-0019](../decisions/STORAGE-0019-candidate-lifecycle-and-foreign-filesystem-adoption.md) —
  the design decision behind the tier model and the `adopt` / `format`
  verb split.
- [STORAGE-0009](../decisions/STORAGE-0009-managed-storage-and-file-sharing.md) —
  managed-storage architecture this builds on.
- [STORAGE-0013](../decisions/STORAGE-0013-replica-set-identity.md) —
  replica set naming the `adopt [set]` argument follows.
