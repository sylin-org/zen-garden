# ADR-0005 — The living will: capture, checkpoints, and replant

**Status:** Accepted · 2026-08-26
**Depends on:** [ADR-0001](ADR-0001-offering-directory.md) (identity plane =
the offering directory), [ADR-0002](ADR-0002-port-allocation-and-residence.md)
(addresses survive rebirth), [ADR-0004](ADR-0004-discovery-envelope-and-uri-grammar.md)
(the room knows which stones hold which banks)
**Closes on witness:** W7 — "the night the drive died, replayed honestly"
**Referenced by:** OFFERINGS.md §7 (storage slice), DEBT entries created herein

## Provenance

Two operator scenarios posed as design targets (2026-08-26), quoted in spirit:

1. **The NAS guarantee.** An adopted NAS serves as the garden's backup sink.
   A seed bank is plugged into *any* stone — "doesn't matter": that moss
   streams updates to the registered NAS. Checkpoints accumulate — say five,
   one per day. Ransomware locks the pendrive; the NAS survives untouched.
2. **The death of stone-river-pebble.** mongodb runs there; its data lives in
   a local mount that also streams elsewhere — another stone for replication,
   or the NAS. Disaster kills the stone. But its **signature** — the local
   offering files defining name, spec, identity — was backed up. It can be
   brought anew, elsewhere, with no or minimal losses.

Plus a binding cost law: *"Backup should be a least-cost operation: just
copy everything raw somewhere first — a backup workspace — then release the
service. Now we can do background work: zip everything, send to storage,
wipe the workspace so space is reclaimed."* And the observation that opened
the grammar: engines like MongoDB cannot be byte-copied safely mid-flight;
manifests must be able to declare both the answer and the process.

## Context

Replication is not backup — the one verdict every external source repeats:
a deletion or ransomware event *replicates beautifully*. Convergent systems
converge onto lies. The garden already splits naturally along the seam:

| Plane | Contents | Size | Cadence | Loss tolerance |
|---|---|---|---|---|
| **Identity** | record.json, plan.json, events.jsonl, configs/ (the directory) | KBs | event-driven (`OfferingChanged`) | ~zero |
| **Data** | volumes/, mounts | GB–TB | scheduled stream → checkpoints | defined by RPO |

The PoC proved both halves' value separately: ADR-0001's wipe recovery
proved the identity plane resurrects offerings; journey 05 ("the night the
drive died") showed physical banking works but costs manual ceremony —
hostname edits, Avahi restarts. Meanwhile ORCH-0041 designed engine-level
consistency hooks and left them orphaned (its own scar: Docker-pause around
a tar window captures torn directories for buffered-write databases —
*"pause freezes mid-flight; it is not crash-consistent"*). The landscape
survey (2026-08-26: Syncthing/Seafile/Nextcloud comparisons, Uncloud's
replicated-volumes roadmap, Perkeep's philosophy) confirms nobody currently
offers identity-coupled backup as a default lifecycle property.

## Decision

### 1. Capture intent lives in the manifest — declared, never inferred

```yaml
managed:
  ...
capture:
  mode: stateless          # nothing to preserve beyond signature+configs
  # -- or --
  mode: lock-and-copy      # consistent under an application lock; raw copy inside the window
  quiesce: { exec: ["mongosh", "--eval", "db.fsyncLock()"],  timeout_s: 30 }
  resume:  { exec: ["mongosh", "--eval", "db.fsyncUnlock()"], timeout_s: 30 }
  max_locked_s: 120        # hard bound; exceed = abort + loud degradation
  # -- or --
  mode: export             # byte-copy wrong even under lock; produce an export instead
  export: { exec: ["pg_dump", "-Fc", "-f", "{workspace}/db.dump"], timeout_s: 900 }
```

Adopted from ORCH-0041's model verbatim where applicable, with its laws:
quiesce failure aborts cleanly; **resume executes finally-style on every
exit path** (a stranded `fsyncLock` outranks every other disaster); hooks
are manifest-sourced only, never API-supplied; validation fails loud at
load. Defaults lean safe: an undeclared `capture` on something with volumes
is treated as untrusted-for-consistency and surfaces honestly, not silently
tarred.

Templates substitute a closed vocabulary at load time — `{fqn}`, `{stem}`,
`{instance}`, `{volume.<name>}` (host paths), `{workspace}`, `{port.<role>}`
— mirroring §5.1's `${input.k}` precedent; unknown variables are load errors.
Hooks run in-container (servers must be live to be told anything); sibling
helper files may ride the manifest stem (`mongodb.capture.sh` beside
`mongodb.offering.yaml`). Secrets enter only via declared inputs — never
baked into stored scripts.

### 2. The two-phase pipeline

Lock time is budgeted by DISK SPEED, never network speed:

| Phase | Domain | Steps |
|---|---|---|
| **A · synchronous, bounded** | local disk | carve `{workspace}` → quiesce → **imprint** (raw copy volumes → workspace) → resume |
| **B · asynchronous, unbounded** | CPU / network / sinks | pack (zstd) → ferry (stream to configured sinks) → commit checkpoint → reclaim workspace |

Phase B may stall forever without touching a database lock. Big-volume
escape hatch: stage directly onto a locally mounted bank (one write instead
of two). Workspaces live at `~/.zen-garden/workspace/{fqn}/{run}/`
(`MOSS_WORKSPACE_DIR` override); caretaking sweeps orphans after N days;
`explain` reports workspace usage and last-checkpoint age (**RPO shown, not
hidden**). Stopped offerings skip quiesce entirely (direct imprint is
consistent by definition); `export` mode requires the live server.

### 3. Checkpoint commits are atomic and dumb-storage-friendly

Everything lands under `checkpoints/run-{ts}.partial/` with an embedded
SHA-256 manifest; one rename makes it `checkpoints/{ts}/`. Rotation keeps N
checkpoints (default five daily). Restore = select → verify checksums →
unpack into fresh volumes → replant. This requires no agent on the sink —
only writable space — which is what makes an SMB-mounted NAS a legitimate
tier-1 sink.

### 4. Sinks and the replication lane

- **Checkpoint sink** (backup semantics): bounded history, never converges,
  survives client compromise by staying out of the converge path. Tier 1 =
  shared storage the operator registers as a sink role on a storage entity
  (roles extend the seed-bank role precedent). Honest limits recorded:
  push-mode over stored credentials means compromise could theoretically
  reach the share; upgrade tiers named now (NAS-side snapshot layers; later
  a minimal pull-mode receiver) so immutability strengthening has a path.
- **Replication lane** (live semantics): cross-stone streaming converges
  toward current state — propagates deletions and corruption with them;
  precisely why checkpoints exist alongside, and why the two are never
  conflated in tooling or UI language.

Together any planted offering defaults into built-in 3-2-1 posture: live
mount + dormant replica + off-stone checkpoints — not because the operator
configured DR, but because planting quietly opted in.

### 5. Roaming stewardship

"Plug the USB anywhere" works through runtime roles ported from PoC law
(STORAGE-0006): first-online-wins Primary; stale-heartbeat detection with
dormant self-promotion; deterministic lower-stone_id yielding; pin = claim-
Primary with last-pin-wins GUIDv7 arbitration; recognition of replicas by
name-as-FQN with reserved-default communal semantics (`bank::default`
replicates broadly, named banks stay private groups). Whichever moss hosts
a bank becomes its steward and streams; stewardship follows the device, and
discovery-triggered announcements make new arrivals known within seconds
(ADR-0004 rich frames + the storage grid row).

### 6. Replant: the rehydration contract wearing boots

Successor flow: fetch signature (offering directory) + latest verified
checkpoint → write the directory → registry loads it → `place()` from the
stored spec → allocations restore per ADR-0002 → same FQN, same connection
strings. The fresh incarnation's audit chain opens with
`Replanted{predecessor_offering_id, final_hash}` — lineage inside the
tamper-evident ledger rather than tribal memory.

### 7. Secrets and scope hygiene

Identity exports redact resolved input values to placeholders with an
encrypted sidecar for the receiving stone; captured data honors the target
sink's confidentiality posture; nothing secret ever rides discovery frames
(ADR-0004's embargo, inherited). **Structural note:** `capture:` rides the
manifest but sits OUTSIDE `WorkloadSpec` and outside `plan_hash` — backup
policy is lifecycle intent, not desired execution; hashing it would flip
plans on policy edits (ORCH-0041's digest gotcha, prevented structurally).

### 8. Storage rides the discovery envelope — a plugged drive is just news

Operator ruling (2026-08-26): for the announcement mechanism, a plugged USB
drive **is** a new offering. Plug a bank: *"hey guys, I got storage X, and
here's some data about it."* Unplug: *"I lost storage X."* Every stone hears,
updates its hot cache. The PoC required a SECOND protocol for this
(STORAGE_BEACON, own type/cache/merge) purely because fat chirps couldn't
afford passengers — ADR-0004's depth tiers delete that subsystem outright.
Binding semantics:

1. **Per-domain revisions.** `bank_rev` rides beside `svc_rev` in every
   anchor frame; bank lifecycle never re-announces services and vice versa.
2. **State bumps revs; measurements piggyback.** Mount / eject / rename /
   visibility-change are lifecycle facts (bump `bank_rev`, emit rich frame).
   Capacity and used-bytes are TELEMETRY — they never trigger frames, they
   ride along as annotations on whatever frame happens anyway. This is the
   anti-spam law; without it a write-heavy volume spams the room.
3. **Liveness is inherited, never timered.** Bank rows live and die with the
   hosting stone's heartbeat in the topology cache — the existing expiry
   sweep dims them wholesale (`seen_at` stamped, resurrection-aware). Clean
   eject announces absence authoritatively; yanked drives resolve through
   expiry. Announce loudly what you know; expire quietly what you can't
   prove.
4. **Hearing-before-meeting.** Bank knowledge learned through middlemen (rich
   replies overheard relays, forwarding queries) lands as TTL'd candidates;
   the holding stone's own live frame promotes them. Ghost-prevention pool,
   third instantiation after offerings and services.
5. **Names follow ADR-0003.** Logical bank identity is an FQN:
   `bank::default` = communal replication group (auto-joins by name);
   private sets use explicit instances. Physical devices carry GUIDv7 ids
   (per-device, path derivation) distinct from the logical set — announce-
   ment frames include BOTH ({fqn, device_id, state, roles[],
   capacity_bytes, used_bytes}), honoring STORAGE-0006's two-name model.
6. **Newcomer bootstrapping unchanged:** the boot rich ask fills the entire
   storage map in one exchange; later Cloud Filter-style placeholder
   consumers feed from these same frames.

## Law encoded

> Replication converges, and therefore faithfully repeats your mistakes;
> checkpoints remember, so you can refuse the mistake. Lock time belongs to
> disk speed, never network speed. A lock is a statement the application
> makes; a freeze is something done to it. And the will is written in two
> planes — kilobytes of identity carried eagerly, terabytes of memory
> carried patiently.

## Alternatives considered

- **Pause-only snapshots (ORCH-0041's status quo)** — rejected: torn
  directories for any buffered-write engine; documented as inadequate by its
  own author ADR.
- **Always-export (never raw copy)** — right answer for databases, wasteful
  mandate for flat files; retained as a MODE under the grammar instead of a
  global rule.
- **Continuous block-level CDP everywhere** — maximal safety, homelab-
  hostile complexity (the Ceph/Gluster lesson); checkpoints+replication hit
  the real threat model at garden scale.
- **In-container cron backups per offering** — hides lifecycle ownership,
  invisible RPO, no workspace discipline, no checkpoint rotation; rejected
  in favor of moss-owned pipeline with surfaced state.
- **Wire-the-user's-tools approach (document Syncthing/restic recipes)** —
  useful companions, unacceptable primary: breaks identity coupling and the
  delight thesis (adoption-by-presence).

## Consequences

### Positive

- Survival story completes the charter arc: services outlive machines
  *mechanically*, demonstrated not asserted — W7 gates it.
- The niche-defining demo exists: kill a stone, watch the garden re-grow it,
  connection string unchanged. Marketing artifact == engineering gate.
- Storage-aware placement arrives pre-seeded (facts census measures disks;
  manifests carry rules; banks carry roles).
- Checkpoint commit protocol runs against any dumb fileshare — zero-agent
  sinks keep homelab friction near zero.

### Negative

- Workspace disks need headroom ≈ largest volumes' bytes at imprint time;
  sweeps and `explain` surface pressure but cannot repeal physics.
- Push-to-SMB carries the stated ransomware asymmetry until upgrade tiers
  land — honest limitation, explicitly deferred, tracked as new debt.
- Export-type engines require images containing their tools (mongodump/
  pg_dump presence becomes a manifest-quality criterion).
- Manifest corpus grows a review duty: every stateful catalog entry must
  eventually declare a capture mode (tracked as debt against RC0).

### Neutral

- Rest ≠ captured, awake ≠ safe: phases handle stopped offerings directly;
  scheduler treats running/stopped merely as differing Phase-A paths.
- PoC's `CeremonyPolicy` file dies forgotten with honor; this ADR replaces
  it wholly (mode names revised toward v1 vocabulary: stateless /
  lock-and-copy / export).

## References

- Mechanism home: `crates/moss/src/offerings/` (new `capture.rs`, pipeline
  task, replant command), glossary vocabulary extensions.
- PoC prior art: ORCH-0039/0040/0041 (snapshot pipeline + quiesce hooks +
  the orphaned ceremony.rs scar), STORAGE-0003 (beacon triggers),
  STORAGE-0006 (roles/pins/arbitration), STORAGE-0008 (garden-tier routing,
  Primary-or-proxy + loop guard), STORAGE-0009 (storage-as-entity, roles),
  STORAGE-0012/0015 (Cloud Filter stack — the visibility dream this ADR's
  sinks eventually feed), journeys/05 ("the night the drive died"),
  caretaking sweeps (orphan hygiene pattern).
- Landscape survey 2026-08-26 (in-session): Syncthing/Resilio/Seafile
  conflict-propagation consensus, Nextcloud maintenance tax, Uncloud
  replicated-volumes roadmap position, Perkeep philosophy, rclone #6051
  closure → stratosync/cascade/LNXDrive emergence.
- Completion gate: **W7** — primary-holding stone killed; dormant promotes;
  bank physically moves between stones between phases; replant restores
  river-pebble'smongodb-class service elsewhere; FQN, allocation policy, and
  burned connection string all held; audit chains narrate the lineage.
