# 02 — CI and the First Tagged Release

> The 2,400+ existing tests run on every push; a git tag produces installable binaries; versions trace to
> commits. Phase: Gate. Depends on: 01 (clean clone must build). Blocks: every refactoring prompt — none
> of them is safe without this net.

## Mission

No CI workflow has ever existed in this repo, no git tag, no GitHub release — while the test corpus,
`deny.toml`, and a containerized cross-compile pipeline already exist and are simply never invoked
automatically. Stand up (a) a PR/push quality gate, (b) a tag-driven release workflow producing binaries
for the platforms the project already cross-compiles, and (c) version→SHA traceability. You are wiring
paid-for machinery, not inventing process.

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| Zero CI ever: `.github/` contains only `copilot-instructions.md` | `ls .github/; git log --all --oneline -- .github/workflows \| head` (expect empty) |
| Zero tags, zero GitHub releases | `git tag \| wc -l` |
| 2,400+ `#[test]`/`#[tokio::test]` functions exist across the workspace | `grep -rE "#\[(tokio::)?test\]" src --include="*.rs" -l \| grep -v target \| wc -l` |
| The only scripted `cargo test` is bypassed: `installer/build.ps1:184` hardcodes `-SkipTests` | `grep -n "SkipTests" installer/build.ps1` |
| `deny.toml` exists at repo root and is never run automatically | `ls deny.toml` |
| Cross-compile Dockerfiles exist at root: `Dockerfile.linux-x64`, `Dockerfile.linux-arm64`, `Dockerfile.linux-arm64-musl`, `Dockerfile.linux-x86` | `ls Dockerfile.*` |
| Orchestrator crates are OUTSIDE the workspace, hence outside any `--workspace` gate; ai/ollama/mongodb compile standalone | `grep -A8 "^exclude" Cargo.toml` |
| Versioning today is `{major}.{minor}.{yyyyMMddHHmm}` with no SHA (installer/build.ps1 ~88) | `grep -n "yyyyMMdd" installer/build.ps1` |
| `version.json` exists at root | `cat version.json` |

## Research first (~45 min)

1. Read `installer/build.ps1` and one `installer/compile-*.ps1` to understand the existing build inputs
   (features, targets, dist.json) — the release workflow should reuse the Dockerfiles, not the PowerShell.
2. Read `deny.toml` to know which checks are configured (advisories/licenses/bans) so CI failure messages
   make sense.
3. `cargo metadata --no-deps` at root and in `src/orchestrators/ollama` to list buildable members.
4. Check test runtime locally if possible: `cargo test --workspace --no-run` to estimate compile cost;
   structure CI so check+clippy run first and fail fast.

## Plan gate

Present: the workflow file list, the platform matrix for releases, the version scheme, and expected CI
duration. **OPERATOR**: confirm (a) which platforms the first release ships (recommend: linux-x64,
linux-arm64, linux-arm64-musl, windows-x64 — skip linux-x86, the assessment freezes it), (b) the first
tag name (recommend `v0.1.0`), (c) whether the GitHub repo's default branch/protection should change (out
of scope to change it yourself).

## Target shape

`.github/workflows/ci.yml` — the gate (runs on push/PR to dev):

```yaml
name: ci
on: { push: { branches: [dev] }, pull_request: {} }
jobs:
  workspace:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --workspace --all-targets
      - run: cargo clippy --workspace -- -D warnings   # only if currently clean; see step 3
      - run: cargo test --workspace
      - uses: EmbarkStudios/cargo-deny-action@v2
  orchestrators:
    runs-on: ubuntu-latest
    strategy: { matrix: { crate: [ollama, mongodb, common] } }   # ai joins or leaves per prompt 04
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --all-targets
        working-directory: src/orchestrators/${{ matrix.crate }}
```

`.github/workflows/release.yml` — tag-driven (`v*`), builds the platform matrix via the existing
Dockerfiles (or `cross`), uploads `garden-moss`/`garden-rake`/`garden-lantern` per target as release
assets, and embeds traceability:

```
version = tag (e.g. 0.1.0), build metadata = +{short-sha}
→ moss --version prints "garden-moss 0.1.0+abc1234"
```

Wire the SHA via `vergen` or a tiny `build.rs` env (`GIT_SHA` from CI); update `version.json` generation
to include `"commit": "<sha>"`.

## Implementation

1. `ci.yml` first. Run the suite locally before pushing the workflow: `cargo check --workspace
   --all-targets && cargo test --workspace`. If clippy is not clean at `-D warnings`, do NOT fix lints in
   this session — set clippy to `-W warnings` with a `# TODO(quality-gate)` comment and record the count
   in FINDINGS.md.
2. `release.yml` second, reusing root Dockerfiles for linux targets and a windows runner for windows-x64.
3. Version traceability: add the SHA embedding; remove nothing from the PowerShell pipeline (it remains
   the maintainer's local path) but delete the `-SkipTests` hardcode at `installer/build.ps1:184` — CI now
   owns tests, and the local pipeline should not silently skip them either (make it a parameter default
   `$false`).
4. Delete the dead `tests/` directory? NO — that belongs to prompt 03. Leave it.
5. Commits: `ci: add workspace + orchestrator quality gates`, `ci: add tag-driven release workflow`,
   `feat(build): embed git SHA in version output`.

## Definition of done

- [ ] `ci.yml` and `release.yml` exist; `act`-style local validation or YAML lint passes; the ci jobs'
      commands run green **locally** (paste outputs — you cannot see GitHub's runners from here).
- [ ] `garden-rake --version` (debug build) prints a version containing the short SHA.
- [ ] `installer/build.ps1` no longer hardcodes `-SkipTests`.
- [ ] A `RELEASING.md` (or `docs/ops/releasing.md` if docs/ops exists) documents: tag → workflow → assets,
      in ≤20 lines.
- [ ] FINDINGS.md records the clippy warning count if the gate had to start at `-W`.
- [ ] Nothing pushed; no tag created — report the exact commands the operator should run
      (`git push origin dev`, `git tag v0.1.0 && git push origin v0.1.0`).

## Out of scope

Fixing clippy warnings; deleting tests/ (prompt 03); changing branch protection; publishing Docker images
of orchestrators; touching the install scripts (prompt 06/15 handle the consumer side).
