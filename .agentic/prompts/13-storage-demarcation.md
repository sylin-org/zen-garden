# 13 — Storage Demarcation: Durability Core, Gateways at the Edge

> The daemon keeps what makes dying hardware trustworthy (banks, replication, snapshots); the NAS surfaces
> (S3, WebDAV, sets, garden-FS) become a feature-gated extension with one blessed access path in core.
> Also: moss's AI-capability surface moves to the orchestrator side, and ollama lands on
> orchestrator-common. Phase: Structure (last). Depends on: 09, 10, 11. Feeds: a defensible default
> attack surface and a daemon whose size matches its mission.

## Mission

The storage plane is ~25k lines — over a fifth of moss — implementing a NAS inside a discovery daemon,
with three overlapping remote-access surfaces, a custom non-SigV4 presign scheme, fake WebDAV locks, and
near-zero end-to-end tests. The assessment's verdict (argued both ways in
`docs/notes/assessment-2026-06/architecture.md` §5 — read it): **keep storage-as-durability in core**
(it is the reliability answer to scavenged hardware and the data-sovereignty differentiator), **extract
storage-as-NAS** behind a cargo feature with its own quality bar. Separately, this prompt completes the
orchestrator-side cleanup the same seam exposes: moss's AI-capability surface (~6.5k lines serving the
ollama orchestrator) moves toward the orchestrator, and ollama finally adopts orchestrator-common instead
of its drifted forks.

This is the largest prompt in the stash. If your session cannot finish it, finish a numbered phase
cleanly and stop — each phase below leaves the tree green.

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| Storage plane ≈25k lines across domain/storage, infra storage modules, api (s3_gateway.rs 2,352; storage.rs 1,993; webdav.rs; garden_storage/) | `wc -l src/moss/src/api/v1/s3_gateway.rs src/moss/src/api/v1/storage.rs` |
| Three remote-access surfaces: per-bank S3 port listeners (23400–23499), legacy S3 path on :7185, WebDAV (`/dav/*` — fake locks: `FakeLs` ~webdav.rs:35) | `grep -n "FakeLs" src/moss/src/api/v1/webdav.rs` |
| Presign is Moss-native HMAC, not SigV4 (`/api/v1/storage/s3/presign`) | `grep -rn "presign" src/moss/src/api --include="*.rs" \| head` |
| The extraction seam EXISTS: the Storage domain has port traits; `domain/storage/mod.rs:132` is one of only 8 hard domain→infra imports | read `src/moss/src/domain/storage/mod.rs` |
| 18 production `Response::builder().unwrap()` in s3_gateway.rs beside its own `build_response()` helper (~280-297) — may be fixed already (dx quick-win); check | `grep -c "unwrap()" src/moss/src/api/v1/s3_gateway.rs` |
| Zero end-to-end tests for replication or S3 conformance (probe gaps noted in prompt 09's FINDINGS) | `grep -rn "s3" src/probe/src/suites 2>/dev/null` |
| Moss's AI-capability surface ≈6.5k lines: offering-capabilities executor + mirroring, placement/fitness/scoring, tool-registry beacons — consumers are the ollama orchestrator's endpoints | `grep -rln "capabilit" src/moss/src/domain --include="*.rs" \| head` |
| ollama orchestrator carries ~1.6k lines of drifted forks of orchestrator-common's discovery/gateway/SSE; orchestrator-common's surviving modules (post-prompt-03) are discovery/gateway/streams | `ls src/orchestrators/common/src` |
| Keep-in-core list (Tier 0 per the assessment): banks, volume roles, Primary/Replica election, changelog replication, snapshots (prompt 12's output), and ONE blessed access path — the garden storage API rake already uses | `grep -rn "garden/storage" src/rake/src --include="*.rs" \| head` |

## Research first (~90 min)

1. Read `docs/notes/assessment-2026-06/architecture.md` §5 (the argument) and the storage domain's port
   traits — the feature gate cuts along ports, not through them.
2. Map which API files belong to which side: durability (banks/volumes/replication/changes/stream +
   garden storage fs/objects read-write that rake uses) vs NAS (s3_gateway, webdav, sets, per-bank port
   listeners, presign).
3. Trace the per-bank S3 port listener lifecycle (who spawns it — should be a supervised conditional task
   post-prompt-11) — gating must cleanly not-spawn it.
4. For the AI surface: list moss endpoints ONLY the ollama orchestrator calls
   (`grep -rn "api/v1" src/orchestrators/ollama/src | grep -o '"/api[^"]*"' | sort -u`), then find their
   moss-side implementation mass.
5. Read ollama's forked discovery/gateway against orchestrator-common's — diff drift before porting.

## Plan gate — OPERATOR decisions

1. **Feature gate name + default**: `storage-gateways` feature on garden-moss; recommend **default ON**
   for now (no behavior change for the maintainer's fleet) with the release builds choosing; flag the
   future default-off flip as a release note. Confirm.
2. **AI-surface destination**: (a) move the capability/fitness/placement logic into the ollama
   orchestrator (daemon shrinks most; orchestrator must learn to query stones directly), or (b) keep a
   minimal capability-reporting core in moss (hardware truth belongs to the stone) and move only
   mirroring/recommendation logic. Recommend (b) — hardware self-description is a stone concern;
   *placement intelligence* is the orchestrator's. Present the split list.
3. WebDAV's fate inside the gated feature: keep as-is behind the gate, or delete now and let demand
   resurrect it (greenfield rules favor delete; Syncthing/OS mounts serve the need). Present both.

## Target shape

```toml
# src/moss/Cargo.toml
[features]
default = ["storage-gateways"]
storage-gateways = []        # S3 per-bank listeners + legacy path, presign, sets, (WebDAV?)
```

```rust
// the route table (prompt 11) grows a feature column — gateway routes vanish at compile time:
#[cfg(feature = "storage-gateways")]
gateway_routes(),   // s3 legacy path, presign, dav, sets
// per-bank S3 listener task: conditional registry entry, also cfg-gated
```

Core keeps: `/api/v1/stone/storage*` (banks/volumes/health/changes/stream), `/api/v1/stone/banks*`,
`/api/v1/garden/storage/{name}/fs*` + `/objects*` (the blessed path rake uses), snapshots (12's surface).
A `cargo build -p garden-moss --no-default-features` daemon serves discovery+offerings+pond+durability —
and is the build the security-sensitive operator chooses.

Orchestrator side after: ollama imports `orchestrator_common::{discovery, gateway, streams}`; its forks
deleted; moss's placement/mirroring code lives beside it (per OPERATOR 2b) or in it (2a).

## Implementation (phased — each leaves green)

**Phase A — gate the gateways.** Introduce the feature; cfg-gate the NAS API files, their route-table
rows, and the per-bank listener task; fix the s3_gateway unwraps if still present (mechanical, its own
helper exists). Verify both builds: default ON byte-for-byte behavior (probe storage suite), and
`--no-default-features` boots + probe core suites pass + gateway routes 404/absent.

**Phase B — AI-surface split.** Execute the OPERATOR-chosen split; move code with its tests; ollama
orchestrator updated to the new call pattern; `cd src/orchestrators/ollama && cargo check && cargo test`.

**Phase C — ollama onto orchestrator-common.** Port discovery/gateway/SSE to the common modules; delete
the ~1.6k fork lines; orchestrator-common gains whatever small deltas the port reveals (one
implementation, finally).

**Phase D — probe coverage for the seam.** Add the S3-conformance smoke (PUT/GET/HEAD/DELETE + one
multipart + one presign round-trip via any S3 CLI against a gated build) and a replication smoke if a
two-bank setup is feasible locally; tag `requires:` honestly.

## Definition of done

- [ ] Both builds green: default and `--no-default-features`; paste `cargo build` + boot logs of each.
- [ ] Gated build: `curl :7185/api/v1/storage/s3` → 404; core storage routes still 200. Probe core
      suites: 0 failed on the gated build.
- [ ] Default build: probe full suite 0 failed; S3 smoke transcript (Phase D) green.
- [ ] `grep -rn "mod discovery" src/orchestrators/ollama/src` → gone (uses orchestrator_common);
      line-delta report for the fork deletion.
- [ ] AI-surface split executed; ollama orchestrator green; moss's domain/ no longer contains the moved
      placement/mirroring mass (name the files in the report).
- [ ] Route-table invariant test green; `cargo test --workspace` green.
- [ ] FINDINGS.md: anything you chose not to gate + why; the WebDAV decision recorded in a short ADR
      (`STORAGE-00xx-gateway-demarcation.md`) including the default-flip plan.

## Out of scope

Rewriting the S3 implementation (SigV4 etc. — FINDINGS.md it). New storage features. Bank/election
internals. The mongodb orchestrator. Changing rake's storage UX beyond what moved endpoints force.
