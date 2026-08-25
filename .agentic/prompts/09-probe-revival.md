# 09 — Probe Revival: One Integration-Test Surface

> garden-probe becomes the project's single, current, runnable integration gate — before the deep
> refactors (11/12/13) start cutting where unit tests can't see. Phase: Verify. Depends on: 02. Blocks:
> 11, 12, 13, 16.

## Mission

The repo has had three generations of integration testing: the `tests/` directory (dead, deleted in
prompt 03), self-skipping in-crate tests, and **garden-probe** — a 9.6k-line dedicated crate that
discovers a live garden over UDP/HTTP and runs categorized suites against it, with plink-SSH "physical
validation". Probe is the right design and the only one that survived — but it has been untouched since
2026-03-22 while moss's API kept moving, so it is rotting toward uselessness. Revive it: make every suite
pass (or honestly skip) against current moss, split its oversized files per the project's own standards,
and wire it to a runnable schedule so it never rots again. The deep refactor prompts that follow assume
"probe green" as their regression baseline for replication, ceremonies, and lifecycle — things unit tests
cannot cover.

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| `src/probe`: 9,650 lines; last substantive commit 2026-03-22 | `git log --oneline -3 -- src/probe` |
| Probe discovers stones via UDP/mDNS + HTTP and runs named suites (read its README/guide: `docs/guides/probe-testing.md`) | `ls src/probe/src` |
| Oversized files violating the project's own 800-line standard: `nurturing.rs` (3,223 lines), `storage.rs` (1,782) | `wc -l src/probe/src/*.rs \| sort -rn \| head` |
| Moss's API surface as of the assessment: 213 unique method-path endpoints; in-crate integration tests cover ~4 files / ~1,100 lines — probe is the only broad-surface gate | — |
| The backup/nurturing API probe exercises is scheduled for consolidation in prompt 12 (three generations → ORCH-0039 snapshots) — your suite names/coverage map must make that cut visible, not block it | read prompt 12 in this directory |
| No CI lane runs probe (it needs a live garden; CI from prompt 02 is unit/check only) | `grep -rn "probe" .github/workflows/ 2>/dev/null` |

## Research first (~60 min)

1. Read `docs/guides/probe-testing.md` and `src/probe/src/main.rs` — suite registry, discovery flow,
   reporting format.
2. Build it: `cargo build -p garden-probe` (workspace member — verify). Fix nothing yet; list compile
   errors if any.
3. Map suite → endpoints: for each suite module, list the moss routes it hits; diff against the current
   router (`grep -n "route(" src/moss/src/bootstrap/router.rs`) to find renamed/removed paths.
4. Determine the dev-garden story available to you: a local moss (`cargo run -p garden-moss` with a dev
   config) is the minimum viable target; multi-stone behavior (election, replication) needs the
   maintainer's garden — those suites get a `requires: multi-stone` tag and skip gracefully.

## Plan gate

Present the suite→endpoint diff (what rotted) and the proposed suite taxonomy before refactoring.
**OPERATOR**: confirm whether a standing dev stone/garden exists for scheduled runs (a Windows Task
Scheduler / cron entry on the dev box pointing probe at the home garden), or whether "runnable manually
with one command, documented" is the bar for now.

## Target shape

One-command developer experience:

```
$ cargo run -p garden-probe -- --suite all --at auto
  garden-probe v0.1.0+abc1234
  discovering… found 3 stones (oak, fern, quiet-pond)
  suite lifecycle      12 passed                          4.2s
  suite storage        8 passed, 2 skipped (no seed bank) 9.8s
  suite pond           skipped (pond inactive)
  suite replication    skipped (requires: multi-stone)
  ─────────────────────────────────────────────
  20 passed, 0 failed, 3 suites skipped  → exit 0
```

Rules the revival enforces: every suite declares `requires:` preconditions and **skips honestly** instead
of failing on a single-stone dev garden; exit code is trustworthy (0 = nothing failed); `--json` emits a
machine-readable report (reuse the shape rake's prompt-07 envelope established if practical). File layout
after the split mirrors moss's domain names:

```
src/probe/src/suites/{lifecycle,offerings,storage,snapshots,pond,topology,companions}.rs   (each <800 lines)
```

## Implementation

1. Make it compile and discover against current moss; fix rotted endpoint paths suite by suite.
2. Split `nurturing.rs` and `storage.rs` along the suites/ layout; rename "nurturing" suite pieces to
   match what prompt 12 will keep (snapshots) vs retire — tag legacy-API tests `requires: legacy-backup`
   so prompt 12 can delete that tag's tests together with the code.
3. Add the `requires:`/skip mechanism + trustworthy exit codes + `--json`.
4. Run the full suite against a local dev moss; iterate until 0 failed (skips allowed and reported).
5. Wire the schedule per OPERATOR (a `scripts/run-probe.ps1` + Task Scheduler XML, or a documented
   one-liner); add a `probe` job to CI marked `if: false # needs live garden` as the visible placeholder
   with a comment pointing at the script — CI stays honest about what it can't do.
6. Update `docs/guides/probe-testing.md` to the revived reality.
7. Commits: `fix(probe): track current moss API`, `refactor(probe): suite layout + skip semantics`,
   `feat(probe): json report + exit contract`, `docs(probe): revival`.

## Definition of done

- [ ] `cargo run -p garden-probe -- --suite all` against a local moss: 0 failed; paste the full transcript.
- [ ] Every suite file <800 lines (`wc -l src/probe/src/suites/*.rs`).
- [ ] Skip semantics demonstrated: pond suite skips cleanly when pond inactive (transcript).
- [ ] `--json | jq .` valid; exit codes verified for pass and induced-fail.
- [ ] Schedule artifact exists per OPERATOR decision; probe-testing.md current.
- [ ] `cargo test --workspace` still green.

## Out of scope

Adding new test coverage for subsystems probe never covered (note gaps in FINDINGS.md — especially
replication and S3 conformance, which prompts 12/13 need; list them as suite stubs with `requires:`).
Touching moss code. Multi-stone suite execution if no garden is reachable.
