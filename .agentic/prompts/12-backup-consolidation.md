# 12 — Backup Consolidation: Three Generations Become One

> One capture engine, one schedule, one HTTP family, one retention policy: ORCH-0039 snapshots absorb
> harvest and nurturing. ~7.4k lines become ~3.5k. Phase: Structure. Depends on: 09 (probe), 11 (the
> scheduler is supervised). Feeds: 16 (the demo restores from snapshots).

## Mission

Moss carries three coexisting backup generations (~7,400 lines): **harvest** (the original capture
engine), **nurturing** (an A/B orchestration + replication layer that *wraps* harvest), and **ORCH-0039
snapshots** (the newest, post-incident-hardened generation with retention and capacity-governor
integration). Three HTTP route families serve them; two scheduling mechanisms drive them; the seam
between them caused the May 2026 disk-fill incident. Consolidate onto snapshots: it absorbs what the
other two uniquely provide (notably nurturing's replication-to-seed-banks and the garden-storage
read-side), then harvest and nurturing are deleted. Greenfield rules apply — no legacy API kept "just in
case"; rake and probe move to the surviving surface in the same session.

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| Harvest: 4 files / 1,412 lines; no routes of its own; no scheduler; capture engine + manifest format | `grep -rln "harvest" src/moss/src --include="*.rs" \| head -20` |
| Nurturing: 7 files / 2,732 lines (core 2,130 + a 602-line garden-storage read API); 14 routes (10 stone-local `/api/v1/stone/snapshots/*` + 4 garden `/api/v1/garden/storage/{name}/snapshots*`); scheduled by EXTERNAL OS timers hitting HTTP trigger endpoints; **directly wraps harvest** (imports HarvestStore/create_harvest) | `grep -rn "HarvestStore\|create_harvest" src/moss/src --include="*.rs" \| grep -i nurtur` |
| ORCH-0039 snapshots: 5 files / 3,242 lines (+85-line capacity-reclaim adapter); 8 handlers across 6 route paths (`/api/v1/stone/offerings/{name}/snapshots*`, `+ /plant`, `/api/v1/stone/banks/{moniker}/seeds`); in-process scheduler (485 ln) — supervised since prompt 11; shares with the others only a garden_common archive helper, one retention constant, and naming conventions | `ls src/moss/src/domain/snapshot* src/moss/src/tasks/*snapshot* 2>/dev/null` |
| The May incident: failed mongodb captures looped without retention and filled a stone's disk; the fix (retention keep-5 + capacity governor + scheduler disposal) landed in the snapshot generation — it is the hardened one | `git log --oneline --grep="snapshot" -15` |
| Rake's `backup` family talks to the nurturing routes; internal names are `Nurturing*` (prompt 07 may have renamed the FAMILY to `storage backup` — check what it calls) | `grep -rn "snapshots" src/rake/src/commands --include="*.rs" \| head` |
| Probe tagged the legacy-API tests `requires: legacy-backup` in prompt 09 — they get deleted with the code; snapshot-surface suites must exist or be written here | `grep -rn "legacy-backup" src/probe/src` |
| Replication of backups to seed banks (nurturing's unique value) and the garden-wide read-side (`/api/v1/garden/storage/{name}/snapshots*`) are FEATURES TO PRESERVE, re-homed onto the snapshot store | read `docs/decisions/ORCH-0039*` + the nurturing replication code before planning |

## Research first (~90 min — this is the deep one)

1. Read ORCH-0039 (the ADR) and the snapshot domain code fully: store layout, manifest format, retention,
   the plant/restore path.
2. Read nurturing end to end: what does A/B give that snapshots lack? What drives replication to seed
   banks? Which parts are orchestration of harvest vs original logic?
3. Read harvest's manifest/format: are existing on-disk harvests forward-readable by the snapshot store?
   **Data on real stones exists** — the migration story is part of the design (a one-time importer, or
   documented manual migration; OPERATOR decides).
4. Map every external consumer of the dying surfaces: rake commands, probe suites, docs
   (`grep -rn "stone/snapshots" src docs --include="*.rs" --include="*.md" | grep -v target`), any
   installer/scheduled-task scripts that hit the HTTP trigger endpoints
   (`grep -rn "snapshots" installer scripts`).
5. Confirm scheduling: with prompt 11 done, the snapshot scheduler is supervised; the OS-timer HTTP
   triggers die with nurturing — make sure no stone-side systemd timer expects them
   (`grep -rn "timer" installer | grep -i snap`).

## Plan gate — OPERATOR decisions

1. **On-disk migration**: importer (snapshots store reads/converts existing harvest+nurturing artifacts
   on first boot), or clean break (existing backups remain readable via a one-off export script, new
   captures start fresh). Recommend the importer only if formats are close; present the format diff.
2. **Route surface**: final snapshot API shape — keep ORCH-0039's 6 paths + absorb the 4 garden read
   paths; the 10 stone-local nurturing routes die. Confirm the deletion list explicitly (it is an API
   break; greenfield, but the operator's own scripts may call them).
3. Replication semantics: nurturing's A/B-to-seed-bank becomes "snapshot store replicates to seed-bank
   banks per the existing bank roles" — confirm the simplification reading is right after research step 2.

## Target shape

After consolidation, one mental model an operator can hold:

```
capture:   snapshot scheduler (supervised, per-offering policy, retention keep-N, capacity-governed)
store:     {data_dir}/snapshots/<offering>/<ts>/ + manifest        (one format)
replicate: snapshot store → seed-bank banks (role-driven, changelog-based like other bank content)
restore:   POST /api/v1/stone/offerings/{name}/snapshots/{id}/plant
read:      GET  /api/v1/stone/offerings/{name}/snapshots ; GET /api/v1/garden/storage/{bank}/snapshots*
rake:      garden-rake storage backup <svc> | restore <svc> --from <snapshot|bank>
```

Module shape: `domain/snapshot/` owns everything; `harvest`/`nurturing` directories cease to exist; the
words "harvest" and "nurturing" survive only in the garden metaphor docs if the maintainer keeps them as
*vocabulary* for this subsystem (FINDINGS.md the naming question — code uses one name).

## Implementation

1. Write/extend the probe snapshot suite FIRST (capture → list → restore round-trip on a throwaway
   offering) — your regression net for the surviving surface. RED on gaps is fine; make it green as you go.
2. Absorb replication: port nurturing's seed-bank replication onto the snapshot store (this is the only
   genuinely new code in the prompt).
3. Absorb the garden read-side: re-home the 4 garden routes onto the snapshot store's data.
4. Migrate rake's backup commands to the snapshot surface (coordinate with prompt 07's family naming).
5. The importer or export script per OPERATOR.
6. Delete nurturing (7 files, 14 routes from the prompt-11 table, its trigger-endpoint scheduling) and
   then harvest (4 files); delete probe's `legacy-backup` suites; sweep docs references into FINDINGS.md
   for prompt 08-style follow-up (or fix inline if ≤10 sites).
7. Run the full probe suite + a manual capture/restore transcript on a local moss with a real offering.
8. Commits: `feat(snapshot): seed-bank replication`, `feat(snapshot): garden read-side`,
   `feat(rake): backup commands on snapshot surface`, `chore(scrub): delete nurturing generation`,
   `chore(scrub): delete harvest generation`.

## Definition of done

- [ ] `grep -rin "nurturing\|HarvestStore" src/moss/src --include="*.rs"` → empty (or only the
      operator-approved vocabulary comments).
- [ ] Route count: the 10 stone-local nurturing routes gone from the table; snapshot + garden-read routes
      present; prompt 11's table test still green.
- [ ] Probe snapshot suite: capture → list → restore round-trip green; paste transcript.
- [ ] Line delta ≈ −3,500 to −4,000 net (report `git diff --shortstat`).
- [ ] Retention + capacity-governor behavior preserved: induce >N captures, observe pruning (transcript).
- [ ] Migration story executed per OPERATOR decision and documented in `docs/guides/storage.md` or the
      snapshot guide.
- [ ] `cargo test --workspace` green.

## Out of scope

Storage gateway extraction (13). Bank/replication internals beyond what snapshot replication needs.
Changing the snapshot format beyond what absorption requires. The companions' backup-related chirps.
