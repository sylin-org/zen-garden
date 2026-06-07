---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0041: Snapshot Quiesce/Resume Hooks

**Date**: 2026-05-29
**Status**: Accepted
**Builds on**: ORCH-0039 (seed-based offering replication), ORCH-0040 (snapshot image by reference)

---

## Problem

`capture_into` (`src/moss/src/infra/snapshot.rs:174`) is the single place an
offering's volumes are archived. Its only consistency measure is a Docker
**pause** around the tar window (`pause_container` → `archive_volumes` →
`unpause_container`, lines 278–312). Pause freezes a running process mid-flight;
it is **not crash-consistent** for a database with buffered writes and an open
write-ahead log. A snapshot of a paused-but-not-flushed MongoDB or PostgreSQL can
capture a torn data directory.

ORCH-0039 explicitly deferred the manifest-schema extension point for snapshot
behavior ("...or this gets a `seedable: false` annotation in a future manifest
schema rev — not in this ADR"). The journey docs describe a `ceremony:` block
(quiesce/resume around snapshots) as if it exists — and in fact the **data model
already exists, fully built and orphaned**: `CeremonyPolicy` /
`CeremonyMode { Unsafe, Quiesceable, Stateless }` / `ExecConfig` in
`src/common/src/manifests/ceremony.rs`, exported from `manifests/mod.rs:31`,
unit-tested, carrying the exact `mongosh fsyncLock`/`fsyncUnlock` example in its
doc comment. It is wired into nothing: not `ManagedConfig`, not `CompiledOffering`,
not the snapshot flow, and no manifest declares `ceremony:`.

## Decision

Adopt the existing `ceremony.rs` model (do not define a new struct) and bracket
the snapshot archive window with application-level quiesce/resume hooks.

### Manifest surface

```yaml
# in a stateful offering's manifest
ceremony:
  mode: quiesceable           # unsafe (default) | quiesceable | stateless
  quiesce:
    exec: ["mongosh", "--eval", "db.fsyncLock()"]
    timeout_seconds: 30
  resume:
    exec: ["mongosh", "--eval", "db.fsyncUnlock()"]
    timeout_seconds: 30
  max_quiesce_seconds: 120
```

`CeremonyMode` semantics:

- **`unsafe`** (default) — today's behavior: pause-only, no app hooks. Preserves
  existing snapshots for every offering that declares nothing.
- **`quiesceable`** — run `quiesce.exec` while the container is **running** (a
  DB needs a live server to take a lock), then pause → archive → unpause, then
  run `resume.exec`. `CeremonyPolicy::validate()` already requires both
  `quiesce` and `resume` for this mode.
- **`stateless`** — no pause and no hooks; archive live (only safe when there is
  genuinely no data volume to tear).

### Threading

1. `ceremony: Option<CeremonyPolicy>` on `ManagedConfig` (`offering.rs:96`), the
   `ManifestFile`/`FrontmatterFile` deserialize structs, and `ServiceTemplate`.
2. `ceremony: CeremonyPolicy` (default `Unsafe`) on `CompiledOffering`
   (`catalog/entry.rs`), populated in `compile()` (`catalog/aggregate.rs`, mirroring
   the `coordination: entry.coordination.clone()` line). `validate()` runs at
   compile time so a malformed `quiesceable` policy fails loud at load.
3. In `capture_into`, read `compiled.ceremony` (reuse the existing
   `get_compiled(&fqn_string)` call already made for the manifest digest) and
   branch the consistency window on `mode`.

### Execution sequence (quiesceable)

```
quiesce.exec (container running)  →  pause  →  archive  →  unpause  →  resume.exec
```

Hooks run via the existing `ContainerRuntime::exec_in_container`
(`docker/exec.rs:458`), whose doc already names this use case. A non-zero
**quiesce** exit aborts the capture (the partial dir is reaped by
`capture_snapshot`'s existing failure cleanup). A **resume** failure is
loud-warn-not-fatal and **must run on every exit path** — mirroring the
`if paused { unpause_container }` bracket — because a stuck-quiesced database
(e.g. left in `fsyncLock`) is worse than today's stuck-paused container.
`max_quiesce_seconds` bounds the whole pause+archive window.

## Implementation Requirements

- Reuse `ceremony.rs` verbatim; the only additions are the field on
  `ManagedConfig`/`ServiceTemplate`/`CompiledOffering` and the branch in
  `capture_into`. No new docker primitive — `exec_in_container` exists.
- `resume.exec` runs in a `finally`-style guard so a panic/early-return between
  quiesce and resume cannot strand the lock.
- Exclude `ceremony` from the snapshot `manifest_digest` (or default-skip its
  serialization) so adding the field does not flip the digest of every existing
  snapshot and trip drift detection (`plant.rs:557`).
- Quiesce/resume commands are **manifest-sourced only**, never user-request
  data — they go straight to `docker exec`, so they stay within the
  validate-at-boundary rule by never accepting them from an API caller.
- The periodic scheduler (`snapshot_scheduler.rs`) needs no change (hooks live in
  `capture_into`); a quiesce failure trips the existing per-offering backoff,
  which is acceptable (caps at 24h).
- Tests: `CeremonyMode` deserialize/default, `validate()` rejects quiesceable
  without quiesce/resume (already covered in `ceremony.rs`), and a snapshot-flow
  test that resume runs even when archive fails.

## Consequences

- Stateful offerings get crash-consistent snapshots: flush + lock before the tar,
  unlock after — instead of a bare process freeze.
- The long-orphaned `ceremony.rs` becomes live, and the `ceremony:` block the
  journey docs describe is finally real.
- Offerings that declare nothing keep today's pause-only behavior (`Unsafe`
  default), so the change is backward-compatible.
- The resume-always-runs guarantee is the sharp edge; it is handled with the same
  bracket discipline as the existing unpause cleanup, plus `max_quiesce_seconds`
  as a backstop.
- `stateless` mode lets genuinely volume-less offerings skip the pause that was
  added to fix the "file changed as we read it" tar failure — to be used only
  when there is no data volume.
