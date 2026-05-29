---
audience: [contributor, maintainer, ai]
doc_type: adr
status: proposed
last_verified: 2026-05-29
canonical: true
---

# STORAGE-0020: Capacity Governor — Free-Space Admission Control and Pressure-Driven Reclaim

**Status**: Proposed
**Date**: 2026-05-29
**Deciders**: leo (architect)
**Tags**: storage, snapshots, harvest, docker, capacity, governance
**Depends on**: [ORCH-0039](ORCH-0039-seed-based-offering-replication.md), [ORCH-0040](ORCH-0040-snapshot-image-by-reference.md) — builds on their per-offering keep-5 retention; adds the free-space invariant that retention alone cannot enforce.

---

## Context

A stone is an appliance: the disk is ours to use. But "ours to use" is not "ours to fill until the OS and Moss die." Three production incidents proved that no subsystem currently owns the invariant *free space on the root filesystem never reaches zero*.

- **stone-golden-summit** (2026-05-27): `/var/lib/zen-garden/snapshots/mongodb` grew to 379 GB across 382 manifest-less partial captures; `/` hit 95%.
- **stone-silent-cascade** (2026-05-29): `/` hit 100% (zero bytes free) — 85 orphaned `mongodb--legacy` captures, each a 413 MB `image.tar` with no manifest.
- **stone-coral-prairie** (2026-05-29): `/` hit 96% — no orphans at all; 19 *valid* snapshots × 4 offerings (39 GB) because the keep-5 fix was not yet deployed, plus 6.5 GB of leaked `zen-harvest/*` images.

These were three different failure modes (orphan accumulation, count-retention not deployed, image leak) with one shared root: **nothing measures free space, and nothing refuses to write when the disk is nearly full.**

### Why the existing mechanisms did not prevent this

Two subsystems touch disk usage today, and the investigation established that neither is a capacity guardrail:

1. **Snapshot retention** (`infra/snapshot.rs:104`, `domain/snapshot.rs:393`): keep-5, count-based, applied after each successful capture and at startup reconcile. Count is the wrong unit — keep-5 of a 20 GB volume is 100 GB — and it does nothing for orphans (manifest-less dirs are not counted) or for a capture that fails *before* pruning runs.

2. **Caretaking sweep** (`domain/maintenance.rs:73`): a domain-pluggable janitor on a fixed 1-hour interval (`tasks/task_defs/maintenance_sweep.rs:31`). It is the right *shape* — each domain contributes a sweeper, results aggregate into a `SweepRun` — but it is age-triggered, never pressure-triggered, has no concept of free space, and its `sweep_docker` prunes only *dangling* (untagged) images (`docker/exec.rs:523`), so the tagged `zen-harvest/*` leak slipped straight past it.

No code path queries free space before a write. The largest writes — `save_image` → `image.tar` (~880 MB, `docker/exec.rs:310`) and `create_archive` per volume (`common/src/infra/archive.rs:126`) — commit hundreds of MB to GB with no precheck (`infra/snapshot.rs:469`, `:546`).

The measurement primitive already exists and was simply never wired to a policy: `infra::storage::platform::disk_usage(path) -> Option<DiskUsage>` runs `df -B1 --output=used,avail <path>` and returns byte-precise `{ used_bytes, available_bytes }` for the filesystem containing any path — including the root filesystem where snapshots live, which the `Storage` domain does not track as a managed volume.

---

## Decision

Introduce a `Capacity` domain aggregate on `Moss` — a **governor** that owns one invariant: *free space on the filesystem holding `data_dir()` stays above the survival floor.* It owns the **policy** (watermarks, pressure classification, admission decisions) and **orchestrates** reclamation, but it never deletes data itself. Each consumer keeps its own deletion logic behind a `Reclaimable` adapter, so domain knowledge stays in the domain (the policy/mechanism split that keeps this from becoming a god-object).

### 1. Measurement and pressure

The governor polls `disk_usage(data_dir())` (a `df` subprocess, polled at the reclaim cadence, never at 5 s class) and classifies the result into a pressure state machine — an enum, not a set of bool flags, so impossible states are unrepresentable:

```rust
pub enum Pressure {
    Healthy,   // used% < ELEVATED  — no action
    Elevated,  // ELEVATED ..< HIGH — gentle hygiene (reap orphans, remove leaked images)
    High,      // HIGH ..< CRITICAL — reclaim, tighten retention
    Critical,  // >= CRITICAL       — aggressive reclaim + deny admission
}
```

Default watermarks (percent of the snapshots filesystem): `ELEVATED = 75`, `HIGH = 85`, `CRITICAL = 95`, with an absolute admission floor `MIN_FREE = 3 GiB`. Watermarks are constants with `ZG_`-prefixed environment overrides. Pressure transitions publish on a `watch` channel (`on_pressure_changed()`) and emit a `CapacityChanged` broadcast event (`Clone + Serialize`, the wire contract), and a `Critical`/`High` transition raises an `Attention` notification so the condition is never silent — the golden-summit runaway dug its grave invisibly for ten days because nothing surfaced it.

### 2. Admission control (the fail-safe)

Before any large write, the writer asks the governor:

```rust
pub enum Verdict { Allow, Deny { reason: String } }
pub fn reserve(&self, request: ReserveRequest) -> Verdict;
```

`reserve` denies when pressure is `Critical` **or** `available_bytes < MIN_FREE`. The first integration is the single choke point every snapshot capture funnels through — `capture_snapshot` (`infra/snapshot.rs:93`), checked before `capture_into` so a denial aborts cleanly with no partial directory written. A denied capture returns `Err`, which the scheduler's existing `FailureBackoff` already handles, and surfaces as degraded health. This one gate would have prevented all three incidents regardless of any retention bug downstream. Harvest (`infra/harvest.rs:101`) and the streaming multipart assembler (`infra/storage/multipart.rs:171`) are identified as follow-on admission sites; this ADR wires snapshots first because that is where the fires were.

### 3. Reclamation (policy orchestrates, domains execute)

```rust
pub enum ReclaimPriority { Eager, Normal }   // order the governor asks in
pub enum ReclaimLevel    { Routine, Pressure, Critical }  // how hard to reclaim

pub trait Reclaimable: Send + Sync {
    fn name(&self) -> &'static str;
    fn priority(&self) -> ReclaimPriority;
    fn reclaim<'a>(&'a self, level: ReclaimLevel)
        -> Pin<Box<dyn Future<Output = Reclaimed> + Send + 'a>>;
}
```

(The boxed future keeps the trait `dyn`-compatible without an `async-trait` dependency, mirroring the existing `BackgroundTask` trait.) A supervised `CapacityReclaimTask` polls pressure; from `Elevated` upward it asks each registered reclaimer **in priority order**, re-measuring free space after each and stopping once back under the `HIGH` watermark. The level scales aggressiveness — `Routine` at `Elevated`, `Pressure` at `High`, `Critical` at `Critical` — and never reaches "delete everything": each reclaimer enforces its own floor.

Two reclaimers ship with this ADR, as adapters over mechanisms that already exist:

- **`HarvestImageReclaimer`** (`Eager`): enumerates `zen-harvest/*` images via a new `ContainerRuntime::list_images_by_reference` (the runtime is a sealed anti-corruption layer per ARCH-0030, so bollard list access is added there, not leaked out) and removes each tag with the existing `remove_image`. Pure junk — reclaimed first, at every level. Closes the tagged-image leak the dangling-only sweep missed.
- **`SnapshotReclaimer`** (`Normal`): wraps the existing `reconcile_all_snapshots(root, keep)` (`infra/snapshot.rs:612`). `Routine` reaps orphans + keep-5; `Pressure` tightens to keep-3; `Critical` to keep-1 — **never keep-0**. Orphan reaping (manifest-less partials) runs at every level because partials are never valid restore points.

Live offering volumes and the pond keystone are **not** registered as reclaimable — the appliance is ours to fill, but live data and cryptographic identity are sacrosanct.

The Caretaking sweep is left intact for age-based hygiene (stale staging, old binaries, rotated logs); it and the Capacity governor are complementary — one is scheduled hygiene, the other is a pressure-reactive invariant.

---

## Consequences

### Positive

- A backup subsystem can no longer fill the volume it lives on: admission control fails safe independent of any retention bug.
- The three incident failure modes are each closed by a distinct layer (orphan reaping, byte-aware reclaim under pressure, tagged-image reclamation), not a single point.
- Capacity pressure is observable (`CapacityChanged`, notification, recorded reclaim runs) — the silent-runaway class becomes a surfaced, alertable condition.
- Domain knowledge stays in each domain; `Capacity` holds only policy and orchestration, keeping it small and the deletion logic auditable where it belongs.

### Negative / trade-offs

- A new background task and a periodic `df` subprocess (bounded by a 5 s timeout, polled at reclaim cadence) add modest overhead.
- A denied capture is a *deliberate* gap in backup coverage under disk pressure; this is the correct trade (a missed snapshot beats a dead stone) but means "last_backup_at is stale" can now mean "disk was full," which the degraded signal must make explicit.
- `Critical`→keep-1 reclamation removes valid restore points under duress; acceptable as a last resort before zero free space, never as steady state.

### Migration

- Additive: a new `Arc<Capacity>` field on `Moss`, a new supervised task, and one new `ContainerRuntime` method. No wire-format or persisted-schema change.
- Watermarks ship with conservative defaults and `ZG_` overrides; no operator action required.
- Independent of the still-pending fleet rollout of the ORCH-0039/0040 retention fix — the governor is the safety net for stones whether or not that fix is deployed.

---

## Alternatives considered

- **Extend the Caretaking sweep with a free-space trigger instead of a new domain.** Rejected: the sweep is a fixed-interval janitor with no admission-control surface; bolting pressure detection and a pre-write gate onto it would conflate scheduled hygiene with a real-time invariant and leave the largest writes ungated between sweeps.
- **Count-based retention only (deploy keep-5 everywhere and stop).** Rejected: count has no byte ceiling and does nothing for orphans or for failures before pruning — coral-prairie shows keep-5 alone still permits tens of GB.
- **A filesystem quota on `/snapshots`.** Complementary, not a substitute — a quota contains blast radius at the OS level but gives the application no signal to skip a write or reclaim gracefully; worth adding as defense-in-depth, out of scope here.
- **A single centralized janitor that deletes across all subsystems.** Rejected as an anti-pattern: it would need the internals of every domain, violating domain ownership and the one-file-per-concept rule. The `Reclaimable` adapter keeps deletion logic in each domain.

---

## References

- Incident analysis and remediation: `project_snapshot_runaway_incident` (session memory)
- Measurement primitive: `infra::storage::platform::disk_usage` (`src/moss/src/infra/storage/platform.rs:50`)
- Snapshot capture choke point: `capture_snapshot` (`src/moss/src/infra/snapshot.rs:93`)
- Existing retention: `reconcile_all_snapshots` (`src/moss/src/infra/snapshot.rs:612`), `RETENTION_KEEP` (`src/moss/src/infra/snapshot.rs:58`)
- Caretaking sweep (left intact): `src/moss/src/domain/maintenance.rs:73`
- Container runtime seal: [ARCH-0030](ARCH-0030-container-runtime-port.md)
