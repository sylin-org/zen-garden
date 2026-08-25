# 16 — The Autonomy Showcase: Pull the Plug

> A reproducible, scripted, honestly-timed demonstration that no management-UI competitor can copy: a
> 3-stone MongoDB replica set on scavenged hardware survives losing a stone — capture to recovery, on
> camera and in CI-style transcript. Phase: Product (capstone). Depends on: 09 (probe), 11 (supervision),
> 12 (snapshots). Strategy opportunity #2 (the orchestration position Nomad vacated; Komodo/Dokploy/
> Uncloud architecturally cannot do this).

## Mission

The assessment's strategy verdict: the two artifacts that convert Zen Garden's position from claim to
fact are (1) the pull-the-plug demo — autonomous replica-set healing with zero human action — and (2) an
honest VRAM-placement comparison against GPUStack. This prompt builds the first as a *reproducible
artifact*, not a one-off: a scripted scenario others can run on their own three old machines, a recorded
transcript with real timings (philosophy: physicality over theater — no edited waits), and the journey
doc that narrates it. The demo doubles as the project's deepest integration test; its script joins probe
as the `showcase` suite.

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| The mongodb orchestrator (6.2k lines) implements check()/reconcile() single-authority choreography: replica-set init, dynamic membership, placement scoring; it delegates deployment to Moss (the correct layering per the assessment) | `ls src/orchestrators/mongodb/src; grep -rn "reconcile" src/orchestrators/mongodb/src --include="*.rs" \| head` |
| Orchestrator deployment is still dev-grade (.bat scripts, manual docker) unless a later session fixed it — the demo needs a reproducible bring-up; shed-register Decide #5 (orchestrators as offerings) may or may not be done | `ls src/orchestrators/mongodb/*.bat 2>/dev/null; grep -rn "mongodb" src/moss/embedded/manifests/sw \| head -3` |
| Topology elections + WOL + chirps work on the real garden (verified live); discovery cascade + tending verified | — |
| Snapshot capture/restore is the consolidated backup surface (prompt 12); the demo's "data survives" beat uses it | `ls src/moss/src/domain/snapshot*` |
| Probe (prompt 09) runs suites against a live garden with `requires:` tags; `requires: multi-stone` exists | `grep -rn "multi-stone" src/probe/src` |
| Companion layer (cricket chirps, firefly LEDs) is the joy differentiator — if hardware is present, the demo script should surface it (a storm warning when the stone dies); optional, not a gate | — |

## Research first (~60 min)

1. Read the mongodb orchestrator's reconcile loop and failure-detection timings — the demo's honest
   numbers come from here; know what to expect before measuring.
2. Read how a client connects through failover today (replica-set connection string vs prompt 14's
   `garden-rake resolve` — if 14 is done, the demo uses `resolve` and the README headline; if not, the
   native mongo RS string).
3. Determine the orchestrator bring-up path: if Decide #5 landed, `garden-rake offer mongodb-orchestrator`;
   else script the container start honestly and FINDINGS.md the gap (this prompt does NOT fix deployment
   — it exposes whether it needs fixing).
4. Inventory the demo substrate available this session: 3 stones (real, or 3 moss instances in
   VMs/containers on distinct IPs — mDNS needs L2; document what setup you used). Real scavenged hardware
   is the *recorded* artifact's requirement; a virtualized rehearsal is this session's requirement.

## Plan gate — OPERATOR decisions

1. **Substrate for the recorded run**: which 3 machines of the operator's garden host the canonical
   recording (needs operator scheduling — the session can fully rehearse virtualized first).
2. **Kill method** for the canonical run: power cord (the headline), `docker kill` (reproducible
   anywhere), or both — recommend script supports `--kill-mode {power|container|reboot}` with power being
   the recorded one (operator pulls; script detects).
3. Whether the write-load generator runs during failover (recommend yes — "zero writes lost after
   recovery / N seconds of unavailability" is the honest, strong claim; measure don't promise).

## Target shape

```
$ ./samples/pull-the-plug/run.sh --stones oak,fern,quiet-pond --kill-mode container
[00:00.0] garden: 3 stones discovered (oak fern quiet-pond)
[00:04.2] offer mongodb → placed: oak(primary) fern quiet-pond     [placement: fitness-scored]
[00:31.8] replica set rs0 healthy (3/3) — writer started (50 docs/s)
[00:45.0] >>> killing oak (primary) <<<
[00:47.1] topology: oak missing (chirp timeout)
[00:53.4] orchestrator reconcile: rs0 degraded 2/3 → election
[00:58.9] new primary: fern — writer reconnected (resolve returned fern)
[01:24.0] orchestrator: membership pruned; rs0 healthy (2/3, degraded-accepted)
[02:10.3] oak returns → rejoins as secondary → rs0 3/3
─────────────────────────────────────────────────
unavailability: 13.9s | writes lost: 0 | human actions: 1 (the kill)
transcript: pull-the-plug-2026-XX-XX.log  (raw, untrimmed)
```

Deliverables: `samples/pull-the-plug/` (run.sh + README narrating each beat + the raw transcript),
probe `showcase` suite (the same scenario, `requires: multi-stone`, container kill-mode), a journey doc
`docs/journeys/NN-the-night-oak-died.md` (the narrative form — match the existing journeys' voice), and
a `docs/notes/` timing record (honest numbers, methodology, what varies by hardware).

## Implementation

1. Script the scenario stepwise against your rehearsal substrate; every timestamp from real events
   (poll/SSE — `garden-rake observe`/the event stream; no sleeps presented as detection).
2. Find and fix nothing structural: where the scenario stalls (e.g. reconcile slower than expected,
   membership not pruning, writer can't reconnect), measure, record in FINDINGS.md, and — only if the fix
   is ≤ ~50 lines and clearly a bug — fix it with a test. Bigger gaps are findings, not scope creep.
3. Wire the probe `showcase` suite (container kill-mode only).
4. Add the data-survives beat if snapshots make it cheap: snapshot before kill → after recovery, verify
   snapshot still restorable to a fresh stone.
5. Write the journey + the sample README from the actual transcript.
6. If companion hardware is reachable: capture firefly amber→red→white across the incident (a photo/note
   in the journey — the joy layer earning its keep).
7. Rehearse until the script runs clean twice consecutively; hand the operator the canonical-run
   checklist for the real-hardware recording.
8. Commits: `feat(samples): pull-the-plug scenario`, `test(probe): showcase suite`,
   `docs(journeys): the night oak died`.

## Definition of done

- [ ] `run.sh` completes the full arc twice consecutively on the rehearsal substrate; both raw transcripts
      attached to the session report.
- [ ] Measured numbers reported with methodology: unavailability window, writes lost, detection latency,
      rejoin time. No number in any doc that wasn't measured.
- [ ] Probe `showcase` suite green (container mode) on the rehearsal substrate.
- [ ] Journey doc + sample README written from real output; zero aspirational phrasing (run the
      DOCUMENTATION.md red-flag check on your own text).
- [ ] FINDINGS.md lists every rough edge the scenario exposed, ranked (this list is the next maturation
      input — the demo is also a diagnostic).
- [ ] Operator checklist for the canonical real-hardware recording delivered.

## Out of scope

The GPUStack comparison benchmark (strategy artifact #2 — write a FINDINGS.md sketch of its method, but
it is its own future prompt). Fixing orchestrator deployment plumbing (expose, don't fix). Editing
recorded timings to look better — the philosophy forbids it and so does this prompt.
