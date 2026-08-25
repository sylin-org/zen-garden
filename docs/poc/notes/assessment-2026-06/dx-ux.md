---
audience: [maintainer, contributor]
doc_type: assessment
status: current
date: 2026-06-11
---

> Part of the [June 2026 project assessment](README.md). Journey claims were verified live against the
> built binaries and a real 12-stone garden where noted; counts were adversarially re-verified.

# DX Ergonomics & UX

Zen Garden's developer and user experience is sharply two-tiered: the *interaction design* (discovery
cascade, manifest-driven CLI, error-message craft) is better than most homelab tooling, while the *journey
infrastructure* around it (releases, CI, docs accuracy, build reproducibility) is absent or actively
misleading. Today, nobody can compile the project from a single clean clone, and nobody at all can install
it from the public materials.

## Contributor journey: clone → build → test → understand → PR

Friction points in order of encounter:

| # | Stage | Friction | Severity | Evidence |
|---|-------|----------|----------|----------|
| 1 | Clone | Public repo is ~2 months behind the author's working tree (origin/dev pushed 2026-04-18; local commits through 2026-06-10) | Major | GitHub `pushed_at` vs local log |
| 2 | Clone | No CONTRIBUTING.md; README contains no build instructions at all | Major | repo root; README.md |
| 3 | Build | **`cargo check` fails on a clean clone**: five `koi-*` crates are path dependencies on `../koi` (`Cargo.toml:98-102`). Nuance: all five are published on crates.io and the sibling repo is public, so this is setup friction with a small fix — but it is undocumented outside one phone-stone guide, and it still blocks any CI | **Blocker** | Cargo.toml:98-102; crates.io |
| 4 | Build | The documented verification command fails: `.agentic/CONTEXT.md` says `cargo test --package moss`; the package is `garden-moss` | Minor | src/moss/Cargo.toml |
| 5 | Build | The only real build path is 8,653 lines of Windows PowerShell requiring Windows, admin, Docker Desktop, and a host Rust toolchain; no Linux/macOS dev story | Major | installer/ |
| 6 | Test | The only scripted `cargo test` invocation is hardcoded off — `build.ps1:184` passes `-SkipTests` unconditionally; Linux/ARM compile scripts contain no test run at all | Major | installer/build.ps1:184 |
| 7 | Test | `tests/` is dead since January and lies about itself: compose builds a deleted path (`../src/linux/moss`), pre-rename ports (3001/3004), nonexistent Dockerfile.build; `tests/README.md` claims "Tests run automatically in GitHub Actions" — no workflow has ever existed | Major | tests/docker-compose.test.yml:6; .github/ |
| 8 | Understand | The AI/contributor bootstrap docs misdirect: `.agentic/reference/utilities.md` documents `GardenHttpClient` (zero consumers) and a TUI-primitives path that doesn't exist (actual: `rake/src/ui/rendering.rs`); CONTEXT.md's module map omits 5 of 13 src/ directories and 5 of 7 orchestrators | Major | utilities.md vs tree |
| 9 | Understand | code-standards.md teaches garden-common's "canonical" `Stone`/`Current` (ARCH-0003) as the standard — zero production consumers; moss built its own (which code-standards §5 separately prescribes). The MANDATORY "NO duplicate structs" rule is violated by 21 names duplicated directly between moss and rake | Major | common/src/lib.rs:46; verified struct census |
| 10 | Understand | Repo root doubles as a scratchpad: committed `build-warnings.txt`, `build-output.txt`, `check_doc.rs` (not valid Rust), `test_if_addrs.rs`, three January `test-*.ps1` | Minor | git ls-files |
| 11 | PR | Nothing gates a merge: zero CI ever; lint policy deliberately toothless (`unwrap_used = "allow"` workspace-wide with a never-executed per-crate plan; no `-D warnings`); the 7 orchestrator crates — including the second-largest crate in the repo — inherit no lints and carry blanket `#![allow(dead_code)]` | Major | workspace Cargo.toml lints; orchestrators |
| 12 | PR | No version traceability: builds are `{major}.{minor}.{yyyyMMddHHmm}` with no git SHA or tag | Minor | installer/build.ps1:88-89 |

The paradox worth stating plainly: behind the broken outer loop sits an unusually disciplined inner loop —
2,483 unit tests, zero production unwraps in the heaviest domain files, an in-process axum test harness,
self-skipping Docker/live-garden tests, a cross-language URI conformance corpus, and a grep-enforced
scaffolding-debt tracker. The contributor-experience problem is not test culture; it is that none of it
runs automatically and none of it is reachable from a clean clone.

## User journey: hear about it → install → first service → day-2 ops

| # | Stage | Friction | Severity | Evidence |
|---|-------|----------|----------|----------|
| 1 | Hear about it | The README's headline pitch — `MONGODB_URI=zen-garden:mongodb/mydb` — is not wired: the URI parser's only consumer is its own test corpus; no binary resolves it | Major | README.md:25-31; src/common/tests/uri_corpus.rs:11 |
| 2 | Install | **The Getting Started block is fictional**: `zen-garden/stone:latest` is not a published image and `ANNOUNCE_SERVICE` appears nowhere in the repo outside the README | **Blocker** | README.md:83-92 |
| 3 | Install | **Zero GitHub releases ever**, so installer/install.sh and install.ps1 — which fetch `releases/latest` — fail for every user | **Blocker** | install.sh:42-67 |
| 4 | Install | first-stone.md and troubleshooting.md describe a product that doesn't exist: 15+ verified mismatches — commands never shipped (`discover`, `describe`, `take-away`, `renew-certificate`), wrong pond forms, invalid flags, wrong API paths, wrong GitHub org, nonexistent installer parameters; plus a false "SSH is disabled by default" claim while the preseed installs ssh-server with stone/stone + NOPASSWD sudo | **Blocker** | first-stone.md; debian-preseed.template |
| 5 | Install | Linux/macOS/Pi users have no path at all: USB imaging is Windows-only PowerShell; install.sh rejects non-x86; `platform_id()` returns "unknown" for aarch64 — while the docs recommend Raspberry Pi hardware | **Blocker** (for that audience) | install.sh:17-26; installer/package.rs:28-36 |
| 6 | First service | If a stone is running, the **core loop genuinely delights**: 4-level endpoint cascade (`--at` > `ZG_STONE` > tending cache > mDNS) with automatic stale-cache recovery; a best-in-class no-stones-found error listing 3 causes and 5 concrete fixes; `observe`'s 12-stone table; `pulse`'s adaptive TUI; empty states with next-step hints. Verified live | Strength | rake connection/resolution.rs, resilient.rs |
| 7 | First service | **The help system teaches a grammar the parser rejects**: 25 of the manifest's 130 examples fail against the actual clap parser (23 use removed natural-language keywords — `list at stone-01`, `find mongodb ensure` — and 2 reference flags that no longer exist). Copy the official example, get a parse error — maximally trust-destroying for first-time builders | Major | command_manifest.rs; cli_build.rs |
| 8 | First service | Vocabulary split-brain: `wake` = start container AND Wake-on-LAN; `release` = un-adopt AND unmount; four unrelated `refresh`es; the API layer says "plant" while the CLI says `offer`; "nourishment" simultaneously deprecated (glossary) and current (README, `nourish`); backup commands internally named `Nurturing*` | Major | command_manifest.rs; glossary.md |
| 9 | Day-2 | Exit codes cannot be trusted by scripts: pond subcommand failures print to stderr and **return exit 0** (`pond join` failure looks successful; `pond init` shares the defect); `find` calls `std::process::exit` mid-logic at 5 sites, bypassing cleanup; storage bails correctly — three disciplines in one binary | Major | rake pond.rs, find.rs |
| 10 | Day-2 | Global `-o json` is honored by exactly 3 of 36 commands (find, config, pond); `list -o json` prints a human table (verified live) while rake-automation.md claims it "works with most commands" | Major | verified live; rake-automation.md |
| 11 | Day-2 | Moss-down with explicit `--at` yields a raw anyhow chain down to "os error 10061" — the polished-error standard set by the discovery path is not applied to the equally common direct-connection failure | Minor | rake dispatch.rs, verified live |
| 12 | Day-2 | The "weather" failure vocabulary — the project's most distinctive UX idea, claimed "Implemented" in joy-of-understanding.md — exists nowhere in code (0 occurrences as health vocabulary in src/) | Minor (credibility) | verified grep |

What the good parts prove matters: the discovery cascade, tending cache, `--field` dot-path extraction,
and the manifest-driven help architecture are *exactly* the right shape for first-time builders. The
product's interaction layer was designed by someone who understands the audience; the journey that gets a
user to that layer was never built.

## The first 30 minutes

A realistic first-time builder — Linux laptop, three old ThinkPads, found the repo through a self-hosting
thread. Minute 0–3: the README pitch lands (`zen-garden:mongodb/mydb` is a genuinely compelling
one-liner). Minute 3–5: they run the Getting Started block; `docker pull zen-garden/stone:latest` returns
*repository does not exist*. Minute 5–10: they fall back to `installer/install.sh`; it queries
`releases/latest` against a repository with zero releases and dies. Minute 10–15: first-stone.md tells
them to run `NewStone-linux-x64.ps1` — a Windows-only PowerShell script, with parameters it doesn't
accept; on Linux this path doesn't exist at all. Minute 15–25: determined, they clone and try to build
from source; there are no build instructions in the README, and `cargo check` fails immediately on five
path dependencies into `../koi` — nothing tells them the koi crates are on crates.io or that cloning the
sibling repo fixes it. Minute 25–30: they skim troubleshooting.md and find recovery commands
(`garden-rake take-away`, `place keystone`) the CLI has never shipped. They close the tab. The cruel irony
is that everything *behind* this wall — the discovery cascade, the tending cache, `observe`, the five-fix
error messages — is precisely the experience this person came looking for; they will never see it.

## Quick wins (≤1 day each)

1. **Push the local branch.** The public repo is 2 months stale; this is a `git push`.
2. **Regression-test the help examples**: one unit test feeding every manifest example through
   `build_clap_app().try_get_matches_from()`, then fix the 25 stale examples, the observe footer hint,
   and the rake-automation.md reference.
3. **Delete or rewrite the fictional README Getting Started block** and demote the unwired `zen-garden:`
   URI from headline to roadmap (or wire it — see [strategy.md](strategy.md) opportunity #3).
4. **Pull first-stone.md and troubleshooting.md** (mark superseded or delete) — rewriting is structural,
   but removing actively false front-door docs is an hour.
5. **Delete `tests/` (10 dead files) and the 7 root scratch artifacts.**
6. **Fix the .agentic bootstrap docs**: `cargo test --package garden-moss`, remove GardenHttpClient,
   correct the TUI-primitives path, update the module map (+5 dirs, +5 orchestrators).
7. **Fix pond exit codes**: replace the `Ok(())`-after-`eprintln` sites with `Err`.
8. **Remove the `changeme` default invite passphrase** — require `--passphrase` or generate one.
9. **Delete dead CLI surface**: unrouted Lift/Place/Invite/presence, the `ceremony`/`template`
   "coming soon" stubs, hide `election`, purge stale `cmd::` constants.
10. **Fix stale manifest help text**: pond "implementation pending," tend "90 seconds" (no TTL exists),
    `GARDEN_QUIET` → `ZG_QUIET`.
11. **Finish the nourishment↔update rename** across README, introduction.md, and the glossary; fix the
    Lantern port contradiction.
12. **Delete publish.ps1** (passes parameters deploy.ps1 hasn't accepted since February) or fold it into deploy.ps1.
13. **Convert s3_gateway.rs's 18 production `Response::builder().unwrap()` calls to its own
    `build_response()` helper** (defined in the same file), then flip `unwrap_used` to `warn` for moss's api.
14. **Suppress the debug-build "manifest validated" stdout line when piped** — it pollutes JSON in dev.
15. **Add interim feature flags to garden-common** (`client`, `system`, `transport`, default-off) so
    companions and orchestrator images stop compiling reqwest/sysinfo/netstat2 for a handful of type imports.

## Structural fixes

1. **Resolve the `../koi` path dependency** — switch to the published crates.io versions with a local
   `[patch]` for sibling development. Everything else — CI, contributor builds, reproducibility — is
   blocked behind a workspace that cannot `cargo check` from a single clean clone.
2. **Stand up a tag-driven release + CI pipeline**: check/test/deny on PR, cross-compile artifacts on tag
   (the Docker build images already exist). This single change makes install.sh/install.ps1 functional and
   converts the install story from fiction to fact.
3. **One output/exit contract for all 36 rake commands**: collapse the four formatting systems
   (OutputWriter — zero uses — CliFormatter, Layout, ~1,247 raw `println!`s) into one pipeline; route
   `-o json` through a single envelope at the dispatch layer; ban `process::exit` in command bodies in
   favor of one top-level error→exit-code mapper.
4. **Regenerate user docs from the command manifest.** The manifest is already declarative data;
   first-stone.md, troubleshooting.md, and the help corpus should be generated from (or CI-validated
   against) it, ending this docs-drift class permanently. Fold the vocabulary reconciliation (plant/offer,
   the wake/release/remove/refresh homonyms, store/storage/backup merge) into this pass.
5. **Split garden-common** into contracts vs runtime and move the ~13.6k moss-only lines into moss;
   resolve the duplicate struct names (the 21 moss↔rake wire-contract duplicates first). This is as much a
   DX fix as an architecture fix: today the "shared contracts" crate is the single most misleading thing a
   new contributor reads.
6. **Build the Linux/ARM install path**: extend the (genuinely good) Rust self-installer with aarch64
   `platform_id` support and ship release artifacts for it. The docs recommend Raspberry Pi hardware to an
   audience that currently has zero supported route onto it.
