---
audience: [contributor, maintainer, ai]
doc_type: adr
status: proposed
last_verified: 2026-05-05
canonical: true
---

# ORCH-0039: Seed-Based Offering Replication

**Status**: Proposed
**Date**: 2026-05-05
**Deciders**: leo (architect), Pavilion engineer
**Tags**: offerings, replication, provenance, seeds, pavilion, drag-canvas
**Supersedes (partially)**: [ORCH-0001](ORCH-0001-replant-ceremony.md) — reuses its data plane (harvest, NurturingStore) but replaces its user-facing surface (CLI ceremony) with a noun-based artifact model and a drag-canvas UX

---

## Context

Replant — moving an offering's full state between stones — was specified in ORCH-0001 as a three-phase ceremony (Collect → Transfer → Plant) driven by `garden-rake replant <offering> from <source> to <target>`. The data plane in ORCH-0001 is real: `src/moss/src/infra/harvest.rs` captures volumes + committed images locally, `NurturingStore` persists harvests to seed-banks, `mirror_capabilities` provides the cross-stone HTTP orchestration template, and the orchestrator framework (`src/orchestrators/common/src/cluster/`) already owns replica-set semantics for orchestrated offerings (mongo / postgres / valkey / weaviate / ollama / ai).

What ORCH-0001 didn't anticipate, and what discovery for the M2 Pavilion milestone surfaced:

1. **The user-facing primitive should be a noun, not a verb.** "Replant" is an action; what users actually want to manipulate is the captured state itself — a thing they can name, store, browse, restore, and migrate. ORCH-0001's CLI grammar treats the harvest as a transient side-effect of the replant verb. Pavilion's drag-canvas UX wants it as a persistent artifact.

2. **An offering's provenance is the offering's own concern.** The set of events that produced an instance's current state — backups taken, restores applied, members joining/leaving, and (in M3) every storage touch — should be a sidecar to the offering, not a global garden-level log. Each running instance can compare its own event-id watermark against peers and decide for itself whether it's behind.

3. **The replica-set semantics already exist** in `src/orchestrators/common/src/cluster/`. ORCH-0007 wired this for MongoDB; ORCH-0012 extracted the reusable `ClusterAdapter` trait. There is no need for a separate "join replica set" gesture — planting an offering with the same FQN on a second stone makes it a set member by virtue of the orchestrator's reactive reconcile loop.

4. **`StoneClient` already auto-upgrades transport** between stones (`src/moss/src/infra/stone_client.rs:88-98`) — HTTPS with pond client cert in ponded gardens, plain HTTP in dry gardens. No SSH layer is required for cross-stone replication, despite earlier proposals invoking SSH.

5. **External mount directories declared in offering manifests are user data.** ORCH-0001's harvest captures volumes (Docker bind mounts under control of the offering) but doesn't pack arbitrary host directories the offering manifest declares as external mounts. A backup that silently omits user data is broken.

The Pavilion M2 milestone (PAVILION-0002 §M2) named "Pavilion-internal drag-drop" and "Replant ceremony (drag-initiated)" as separate line items. Both depend on a model where harvests are first-class, addressable, and visible — which ORCH-0001 doesn't deliver.

---

## Decision

We will build offering replication on **three load-bearing concepts**:

1. A **seed** — a persistent, addressable snapshot of an offering instance at a specific moment, carrying the data, the committed image, the external mount payload, and a manifest signed by SHA512 file hashes. Seeds are written to storage banks (or to local disk, on user choice).

2. A **per-offering event log sidecar** — an append-only chain of GUIDV7-tagged events that an offering instance owns. Events record lifecycle moments (member joined, backup taken, restore applied, reconfig) and, in M3, every storage touch. The log is stored alongside the offering's data, replicates with it, and is the canonical source for "is this instance behind?"

3. A **canvas-and-drag user surface** — Pavilion hoists Lantern's 3D topology view as the unified spatial substrate where stones, banks, offerings, and seeds coexist as nodes. Drag pairings carry semantics derived from `(source_kind, source_state, target_kind, target_state)` resolution; modal pickers are the keyboard-equivalent path.

Replication happens in two complementary modes:
- **Snapshot fetch + apply** (M2): a lagging instance pulls a complete snapshot at GUIDV7 X, applies it locally, and resumes from there. Whole-archive transfer.
- **Live event stream + diff sync** (M3): a dormant peer holds a long-lived HTTPS connection to the primary, receives event notifications in real time, and pulls only the bytes its own SHA512-manifest comparison says it lacks.

### Identity

The **OfferingFqn is the load-bearing identifier** ([OFFER-0003](OFFER-0003-offering-fqn.md), [STORAGE-0013](STORAGE-0013-replica-set-identity.md)). All instances sharing one FQN form an emergent replica set under the orchestrator framework. Plant-from-seed defaults to the seed's source FQN; an explicit `as_fqn` override is the "fork" path that derives a new independent instance from existing seeded data.

### Seed metadata schema

A seed carries (minimum):

```text
{
  id:               <uuid>,
  source_fqn:       OfferingFqn,        // the FQN this seed was captured from
  source_stone:     StoneName,          // the stone running that instance at capture time
  source_event_id:  GUIDV7,             // the event log watermark the seed represents
  created_at:       Iso8601,
  manifest_digest:  Sha256,             // hash of the offering's compiled manifest at capture
  image: {
    ref: "zen-harvest/<offering>:<timestamp>",
    transport: "docker_save",            // M2: docker save tarball; later: registry pull
    size_bytes: u64,
    sha512: <hex>
  },
  volumes:          [{ name, container_path, size_bytes, sha512 }],
  external_mounts:  [{ host_path, container_path, size_bytes, sha512 }],
  size_total_bytes: u64
}
```

`manifest_digest` exists because if the offering's compiled manifest has drifted between capture and restore, the user is restoring against a different shape than they captured. Default behaviour: warn-and-proceed; advanced: refuse-without-explicit-acknowledgement. Same posture nourish takes today.

### Event log

Each running offering instance owns an append-only file at `<offering_state_dir>/events.log` recording:

```text
{
  event_id:        GUIDV7,
  prev_event_id:   GUIDV7?,             // chain pointer; first event has none
  fqn:             OfferingFqn,
  at:              Iso8601,
  kind:            "set_initialized" | "member_joined" | "member_left" |
                   "backup_taken" | "restore_applied" | "reconfig" |
                   <M3:> "storage_touch",
  actor:           { stone: StoneName, user: Option<String> },
  details:         <kind-specific JSON>
}
```

Each instance also persists a `last_event_id` watermark per FQN it participates in, sitting beside the tending file in the per-stone state directory.

### Retention

Event log retention is **truncate-since-snapshot, triggered every backup** (whether the backup is local-only or remote-to-bank). Once a snapshot is durably written, the events that produced it are reconstructable from the snapshot, and earlier events are pruned in a background sweep. Bounded growth, no information loss.

### Snapshot frequency

Snapshots are produced **both on user request and periodically in the background**. The periodic schedule is per-offering (defaulting to a configurable interval in the offering's compiled manifest, e.g. 4 hours), with a hard backstop ensuring no offering goes more than 24 hours without one. User-initiated snapshots emit `backup_taken`; periodic ones emit the same event with `details: { trigger: "scheduled" }`.

### Drag pairing resolution

Drag-and-drop semantics are encoded as a resolution table, not a switch statement:

| Source | Target | Action |
|---|---|---|
| Offering | Bank | Capture snapshot → write to bank → emit `backup_taken` |
| Seed | Same FQN on origin stone | Plant from seed → emit `restore_applied` (restore in place) |
| Seed | Empty stone | Plant from seed → orchestrator's reconcile joins it to the set |
| Seed | Stone running same FQN | Plant from seed → emit `restore_applied` (warn user about set-primary overwrite on next reconcile) |
| Seed | Stone running different FQN | Plant from seed → independent instance, no conflict |

Cross-cutting to all pairings: dropping on a stone with the user's explicit `as_fqn` override forks the instance under a new identity.

### Transport

Cross-stone calls go through `StoneClient`, which already chooses HTTPS-with-pond-client-cert when available and falls back to plain HTTP in dry gardens. No SSH layer. Server-authenticated TLS is what's enforceable today (`src/moss/src/bootstrap/tls.rs:101` defers mutual mTLS to "Phase 4"); mutual auth lands when the pond Phase 4 work does.

### M2 vs M3 cut

**M2 ships:**
- Per-offering event log primitive: types, append, read-since, watermark file
- Events for set-level operations only: `set_initialized`, `member_joined`, `member_left`, `backup_taken`, `restore_applied`, `reconfig`
- Snapshot capture (writes to bank-set or local disk per user pick) with full SHA512 manifest
- Snapshot read endpoints (manifest, full archive, single file) — single-file path is forward-compat for M3 diff sync without endpoint churn
- Periodic snapshot scheduler in Moss
- Plant-from-snapshot endpoint, FQN-defaulted, fork via `as_fqn`
- Pavilion canvas + drag substrate + the two pairings (offering→bank backup, seed→stone plant)
- Pavilion `useJobProgress` SSE hook generalising the existing storage-observer parser
- Set-state visualisation on the canvas (membership + primary + connection string from orchestrator `/api/cluster/status`)
- Service-card "Backup…" picker (keyboard path, no drag required)

**M3 defers:**
- Storage-touch events (per-write GUIDV7 events on external mounts and offering data volumes)
- Live event-stream long-lived HTTPS connection from primary to dormant peers
- Diff-based catchup using SHA512 manifest comparison
- Dormant-instance lifecycle (running-but-passive instances tracking primary state)
- Mutual mTLS for cross-stone replication (waits for pond Phase 4)
- Set-event time-travel (pause orchestrator, restore set-wide, force resync)

### Pre-build factorings

Three small refactors land before the new endpoints and absorb duplication that would otherwise grow:

1. **`src/moss/src/infra/communications/cross_stone.rs`** — extract `fetch_from_stone::<T>(endpoint, path)` and `post_to_stone::<Req, Res>(endpoint, path, body)` from `mirror_capabilities` (`offering_capabilities.rs:938`). Pure refactor; mirror_capabilities becomes a 3-line caller.
2. **`src/moss/src/domain/health.rs::verify_service_health(state, offering, timeout)`** — extracted from `phases/water.rs`. The wait-for-health logic is welded to ceremony-state-management today; replant needs it standalone.
3. **`restore_harvest_with_staging()` in `infra/harvest.rs`** — current `restore_harvest()` extracts straight to live volumes with no rollback. The staging variant uses a temp volume + atomic-rename so a failed mid-restore doesn't leave torn state. Replant uses staging; nourish keeps direct.

---

## Rationale

**Why "noun" over "verb".** A persisted, addressable seed is browsable, dragable, and time-stamped. The user's mental model is "I have these saved states; I can place them where I want." A verb-only model (CLI replant) hides every state behind a transient action. The seed-as-noun is also what makes the "drag the seed back to the offering = restore, drag it to a different stone = plant" symmetry work.

**Why per-offering event log, not garden-level.** Offering provenance is the offering's own concern. A garden-level event log is a coordination point with no single owner; a per-offering log replicates with the offering, has clear ownership, and lets each instance autonomously decide whether to sync. This matches how database replication actually works (per-replica-set oplogs, per-cluster WALs) — we're putting zen-garden's concerns at the same scope.

**Why FQN as identity.** The orchestrator framework already filters by FQN (`src/orchestrators/mongodb/src/tasks/discovery.rs:44-46`), and `OfferingFqn` already encodes the instance suffix ([OFFER-0003](OFFER-0003-offering-fqn.md)). Using FQN as the seed-source identity makes "plant on second stone" automatically join the correct set, with no new gesture needed.

**Why truncate-since-snapshot, every-backup-triggered.** Bounded growth, zero data loss, simple invariant. After a snapshot lands, the events behind it are by definition reconstructable; pruning them is correct, not lossy.

**Why both periodic and user-initiated snapshots.** User-initiated covers the "I'm about to do something risky" case. Periodic guarantees a recent point dormant peers can catch up to without replaying weeks of events when the M3 streaming lands. Both write the same `backup_taken` event with different `details.trigger`.

**Why HTTPS-via-StoneClient instead of SSH.** SSH was a red herring. `StoneClient.request()` already auto-selects transport based on pond cert availability. In ponded gardens that's TLS-encrypted with server identity verified against the pond CA; in dry gardens it's plain HTTP on a trusted LAN. The server-auth-only caveat is real but is a known M3-or-later concern (mutual auth via pond Phase 4), not a blocker for M2.

**Why the canvas as primary surface.** Direct manipulation collapses many surfaces (Services, Storage, Pond) into one spatial substrate where the relationships between stones, banks, offerings, and seeds are visible. Modal-driven flows still exist (Service-card "Backup…" picker) for keyboard, accessibility, and CLI-equivalence — they just aren't the primary path.

---

## Consequences

### Positive

- **One primitive, three features.** The same seed catalog endpoints serve replant, user-initiated backup-to-disk, and seed-bank export. No three-times-in-three-shapes implementation.
- **Time-travel restore for free.** Multiple seeds for the same FQN form a chronology — yesterday's `mongodb::prd`, last week's, etc. Browse and plant any historical seed.
- **Forks are a first-class derivation.** Drag a `mongodb::prd` seed onto a stone, override `as_fqn` to `mongodb::staging`, and you have a staging environment seeded from production data — no separate "clone" path.
- **Replica-set semantics inherit existing orchestrator infrastructure.** No new "join set" gesture, endpoint, or coordination state in zen-garden's offering layer. The orchestrator framework already converges.
- **Audit log per offering, free.** Every meaningful state-changing operation produces an event with an actor and a timestamp. Provenance is built into the data plane, not bolted on.
- **Drag-canvas UX directly demonstrable.** Once the substrate ships, future M-stage features (drag stone → pond, drag bank → bank, drag file → bank) are pairing-table additions, not new substrate.

### Negative

- **Bigger than ORCH-0001's CLI replant.** ORCH-0001 was scoped to one CLI command + three phases. This decision spans data layer (event log), HTTP layer (seed catalog + plant endpoints), and a new Pavilion canvas + drag substrate. The M2 cut still bundles 25+ commits.
- **Snapshot-only restore in M2 means whole-archive transfer.** A 5 GB seed transfers 5 GB even if the destination already has 4.9 GB of identical content. M3's diff-sync fixes this, but until then, large offerings on small WANs feel it.
- **Server-authenticated-only TLS in ponded gardens isn't mutual.** A malicious peer on the LAN can request a snapshot stream from the primary, and the primary cannot cryptographically verify which peer is asking. This is a known limit of pond Phase 2; M3 either inherits Phase 4 mutual auth or layers HMAC tokens on top.
- **Manifest-digest drift handling defers a real problem.** "Warn and proceed" lets the user override at their own risk; some classes of drift (a removed volume in the new manifest) will create silent restore failures that surface later as missing data. Long-term, drift-aware restore needs structured per-volume reconciliation.

### Neutral

- **Implies an explicit "external mounts are seedable" stance.** Anything declared in the offering's compiled manifest gets packed. Offering authors who don't want certain mounts in seeds need to declare them differently (or this gets a `seedable: false` annotation in a future manifest schema rev — not in this ADR).
- **Banks become the canonical seed catalog.** "Where's the catalog of all my mongo backups across the garden?" → "in whichever banks they were written to." The bank's existing replication semantics carry seed availability across stones; gardens with no banks fall back to local-disk seeds with no off-stone copy.
- **Pavilion's M2 close gets reshaped.** What was "tray polish + 3D topology + drag-drop + replant" becomes "tray polish + canvas-as-3D-topology + drag substrate + seed system." Same milestone, different decomposition; same scope.

---

## Alternatives Considered

### Alternative 1: Bank-backed event log (rejected)

**Description**: Place the per-FQN event log in a bank rather than as a sidecar to the offering's working directory. Use the bank's existing replication for log durability.

**Pros**: Reuses bank replication; no new persistence layer; logs survive offering deletion.

**Cons**: Externalises offering provenance — the offering loses ownership of its own history. Coupling the log lifecycle to a bank's lifecycle is wrong (you can replant an offering whose bank is currently offline; you can have an offering that never had a bank). Each instance no longer autonomously owns its watermark.

**Rejected because**: An offering's provenance is the offering's concern. Bank-backed storage of *seeds* (the snapshots) is correct; bank-backed storage of *events* makes the bank a coordination point with implicit ownership it shouldn't have.

### Alternative 2: Modal-driven replant per ORCH-0001 (rejected as primary)

**Description**: Ship ORCH-0001's CLI ceremony plus a Pavilion modal driver, no canvas, no drag.

**Pros**: Smallest scope; reuses ceremony framework; CLI parity.

**Cons**: Misses the "killer UX" — direct manipulation. Hides the seed as a transient verb-driven side-effect. Doesn't deliver the M2 spec's drag-drop line item.

**Rejected because**: The user-facing surface defines the data model the system needs to expose. The drag-canvas surface requires seeds-as-nouns; modal-only doesn't. We can't ship modal-only and retrofit the canvas later without reshaping the data layer twice.

The CLI / modal path remains a first-class keyboard-equivalent in this ADR — Service-card "Backup…" picker, `garden-rake plant <offering> from seed <id>` — but is layered over the same noun-based primitives.

### Alternative 3: SSH-tunnelled replication transport (rejected)

**Description**: Build a per-offering or per-stone SSH channel between primary and dormant peers for live event streaming, separate from the existing HTTP API.

**Pros**: SSH is widely understood; stone-stone SSH credentials are already documented (`stone-ssh.md`); SSH gives mutual auth out of the box.

**Cons**: Duplicates transport infrastructure that `StoneClient` already provides. Pond mTLS (today server-auth-only, mutual in Phase 4) is the project's chosen auth substrate; building a parallel SSH channel forks the auth story. Operator-facing SSH credentials shouldn't be runtime credentials — that violates the boundary between admin access and service-to-service authentication.

**Rejected because**: `StoneClient` is the right transport. SSH in the runtime is a layering violation.

### Alternative 4: Ceremony-framework-driven seeds (rejected)

**Description**: Use the koi-common ceremony framework (`CeremonyHost` / `CeremonyRules` — currently pond-only) to drive backup and restore as multi-step server-controlled dialogues.

**Pros**: Consistent with the existing pond ceremonies; reuses the proven step-prompt protocol.

**Cons**: The ceremony framework is shaped for question-answer dialogues (passphrase entry, TOTP confirmation, branching choices). Backup and restore are streaming long-running operations with progress, not branching prompts. Forcing seeds through the ceremony shape would either pollute the framework or produce a degenerate one-step ceremony per operation.

**Rejected because**: Wrong tool. Use the existing job/SSE infrastructure (`/api/v1/stone/presence/stream` + the EventBus) for progress, not the ceremony framework.

---

## References

- [ORCH-0001](ORCH-0001-replant-ceremony.md) — predecessor; this ADR supersedes its user-facing surface and CLI grammar but reuses its data plane (harvest, restore, seed-bank persistence)
- [ORCH-0007](ORCH-0007-managed-logical-sets.md) — MongoDB replica set orchestrator; the reactive `reconcile()` loop is what makes drag-seed-onto-stone produce a set member without explicit join
- [ORCH-0012](ORCH-0012-cluster-adapter-extraction.md) — `ClusterAdapter` trait that all orchestrators implement
- [STORAGE-0006](STORAGE-0006-seed-bank-replication.md) — seed-bank replication, roles, pond encryption; banks are where seeds live
- [STORAGE-0009](STORAGE-0009-managed-storage-and-file-sharing.md) — managed storage including external mount semantics that seeds must capture
- [STORAGE-0013](STORAGE-0013-replica-set-identity.md) — replica-set name as display identity; aligns with FQN-as-identity
- [OFFER-0003](OFFER-0003-offering-fqn.md) — Offering FQN v1, the seed source/target identity
- [OFFER-0006](OFFER-0006-image-direct-and-fqn-v2.md) — FQN v2; seed metadata records the FQN at this revision
- [PAVILION-0002](PAVILION-0002-revised-milestone-shape.md) — M2 milestone; this ADR realises the "Replant ceremony" and "Pavilion-internal drag-drop" line items as one coherent surface
- [SECURITY-0001](SECURITY-0001-pond-tiers.md) / [SECURITY-0004](SECURITY-0004-tier2-deferral.md) — pond auth tiers; the server-auth-only caveat in this ADR cites the Phase 2 / Phase 4 split documented there
- `src/orchestrators/common/src/cluster/` — the existing logical-set abstraction this ADR builds on
- `src/moss/src/infra/harvest.rs` — the existing harvest implementation that becomes the snapshot capture path
- `src/moss/src/infra/nurturing_store.rs` — the existing seed-bank persistence layer
- `src/moss/src/infra/stone_client.rs` — the cross-stone transport with auto-upgrade to HTTPS in ponded gardens
