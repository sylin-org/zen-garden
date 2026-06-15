# 03 — Dead Code Deletion Sweep

> Delete ~12–16k lines of verified-dead code with zero behavior change. Phase: Subtract. Depends on: 02
> (CI as the safety net). Blocks: 04, 10 (every later session reads a smaller, truer tree).

## Mission

The assessment adversarially verified a set of modules, files, and stubs with **zero consumers** (or
self-described dead status). Delete them. This prompt is deliberately mechanical: every item below carries
the verification command that proved it dead; you re-run the command, and if it still returns
zero-consumers, you delete. If ANY command disagrees with the table, skip that item and record it in
FINDINGS.md — do not investigate, do not fix.

The deeper purpose: a contributor or model reading this repo must never again wonder whether
`garden_common::jobs` is the jobs system (it isn't — moss's `domain/jobs` is).

## Ground truth + kill list (verified 2026-06-11)

Re-verify, then delete, in this order. "Consumers" always means: matches outside the module itself,
excluding `target/`.

| # | Item | Lines | Proof of death (re-run before deleting) |
|---|---|---|---|
| 1 | `src/moss/src/infra/cloud_filter/` (6 files) | 2,475 | `grep -rn "cloud_filter" src/moss/src --include="*.rs" \| grep -v "infra/cloud_filter"` → expect ONLY nothing (module not declared in `infra/mod.rs`; unlinked since commit 9fcb49db, PAVILION-0001; replacement lives in `src/pavilion/src/integration/cloud_filter/`) |
| 2 | `src/moss/src/domain/cloud_drive.rs` + its `domain/mod.rs` declaration/re-export | 332 | `grep -rn "DriveAction\|classify_rename\|cloud_drive" src --include="*.rs" \| grep -v "domain/cloud_drive\|infra/cloud_filter\|domain/mod.rs"` → expect empty (only consumer was item 1) |
| 3 | `src/common/src/jobs/` | 1,090 | `grep -rn "garden_common::jobs\|common::jobs" src tools tests --include="*.rs" \| grep -v "src/common/src"` → expect empty. moss's real one: `src/moss/src/domain/jobs/` — DO NOT TOUCH |
| 4 | `src/common/src/events/` | 731 | `grep -rn "garden_common::events" src --include="*.rs" \| grep -v "src/common/src"` → expect empty. Internal consumers (`api_utils/sse.rs`, `jobs/manager.rs`) die with items 3/4 — verify sse.rs's import is from the dying module before removing it, else inline what it needs |
| 5 | `src/common/src/stone.rs` + its `lib.rs` re-exports | 202 | `grep -rn "garden_common::stone\|garden_common::{[^}]*Stone\|Current, Environment\|OsKind" src --include="*.rs" \| grep -v "src/common/src" \| grep -v "tools::Stone\|connection::stone"` → expect empty. CAUTION: `garden_common::tools::Stone` (gateway) and rake's `connection::stone::Stone` are DIFFERENT types — leave them |
| 6 | `src/common/src/client/api.rs` (`GardenHttpClient`, `GardenApiResponse` alias) | 160 | `grep -rn "GardenHttpClient\|GardenApiResponse" src docs .agentic --include="*.rs" \| grep -v "src/common/src/client"` → code hits expect empty; doc hits get fixed in prompt 08 (note them in FINDINGS.md) |
| 7 | `src/common/src/errors.rs` (2-line stub) | 2 | `grep -rn "garden_common::errors" src --include="*.rs"` → expect empty |
| 8 | `src/orchestrators/postgresql`, `valkey`, `weaviate` | 451+448+404 | `grep -n "postgresql\|valkey\|weaviate" installer/build-orchestrators.ps1` — remove those build lanes too. Their `main()`s log a placeholder and exit; confirm by reading each `main.rs` (~18 lines) |
| 9 | `src/orchestrators/common/src/cluster*` primitives (7 files) | 1,359 | `grep -rn "orchestrator_common::cluster\|use.*cluster::" src/orchestrators/{ollama,mongodb} --include="*.rs"` → expect empty (only the three deleted scaffolds consumed them) |
| 10 | `tests/` directory (10 files) | — | Read `tests/docker-compose.test.yml` line ~6: builds `../src/linux/moss`, a path that does not exist; `tests/README.md` claims GitHub Actions CI that never existed |
| 11 | Root scratch: `build-warnings.txt`, `build-output.txt`, `check_doc.rs`, `test_if_addrs.rs`, `test-discovery.ps1`, `test-discovery-direct.ps1`, `test-hw-detection.ps1` | — | `git log --oneline -1 -- <file>` each: all January-era; none referenced by any script (`grep -rn "<filename>" installer scripts` → empty) |
| 12 | Moss stubs: `TimerListener` (617 ln, no callback, registered every boot), the `NoAuth` struct in `infra/auth.rs` (never wired; 4 test-only uses), the `(testing)` election route, `/api/v1/helpers/json-transform`, `domain/service_manager.rs` (13-line tombstone) | ~1,300 | For each: grep the type/path for non-test consumers; for routes, grep rake + docs for callers. NoAuth: if pond middleware references it in a way that compiles, leave it and FINDINGS.md it — prompt 05 owns auth |
| 13 | Rake dead surface: `LiftCommand`, `PlaceCommand`, `InviteCommand`, `presence.rs` (unrouted); `ceremony`/`template` coming-soon manifest entries; stale `cmd::` constants (`TAKE_ROOT`, `MAKE`, `BROWSE`) | ~hundreds | Unrouted = no arm in `route.rs`: `grep -n "Lift\|Place\|Invite\|presence" src/rake/src/route.rs` → expect empty; then delete structs + manifest entries together so the manifest stays the single source of truth |
| 14 | `installer/publish.ps1` | 1 file | Read it: passes parameters `deploy.ps1` does not accept (`grep -n "param" installer/deploy.ps1`) |

## Research first (~30 min)

Skim `.agentic/CONTEXT.md` and `docs/code-standards.md` §14 (file coupling). Read `src/common/src/lib.rs`
fully before touching common — you will edit its re-export list for items 3–7 and must not disturb the
live exports around them.

## Plan gate

No OPERATOR items. Post the kill list with each item's re-verification result (PASS = still dead /
FAIL = found a consumer, skipping) before deleting anything.

## Implementation

- One commit per table row (or per tightly-related pair), message `chore(scrub): delete <item> (dead since <evidence>)`.
- After each commit: `cargo check --workspace` (and `cargo check` in each surviving orchestrator after
  rows 8–9). Run `cargo test --workspace` after rows 2, 7, 9, 13, and at the end.
- Update `Cargo.toml`s when a deletion orphans a dependency (e.g. sqlx in lantern is prompt 04's problem —
  not yours; but if deleting common modules orphans a dep of common, remove it and note the build-time win).
- Do NOT update docs in this session beyond deleting `tests/` (docs corrections belong to prompts 06/08);
  list every doc reference you noticed in FINDINGS.md.

## Definition of done

- [ ] All PASS rows deleted; all FAIL rows reported in FINDINGS.md with the disagreeing grep output.
- [ ] `cargo check --workspace && cargo test --workspace` green; ollama + mongodb + common orchestrators
      `cargo check` green.
- [ ] `git grep -n "cloud_filter" -- src/moss` returns nothing; `git grep -n "garden_common::jobs\|garden_common::events"` returns nothing.
- [ ] Line-delta report: `git diff --shortstat <start>..HEAD` (expect roughly −12,000 to −16,000).
- [ ] FINDINGS.md lists doc references to deleted items (for prompts 06/08) and any skipped rows.

## Out of scope

The ai orchestrator and pavilion (prompt 04 — they are PARKED, not deleted). Lantern's identity (04).
Backup generations (12). uri/ module (14 — it gets wired, not deleted). Any refactoring of live code. Any
doc file except the dead `tests/` directory.
