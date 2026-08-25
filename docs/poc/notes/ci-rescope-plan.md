# Prompt 02 (CI + first release) — re-scoped for local-code (path-dep) koi

> Draft plan, 2026-06-15. Prompt 02 assumed clean-clone-from-crates; zen now consumes koi via path
> deps to a sibling `../koi` (see docs/guides/koi-dependency.md). This re-scopes the CI/release plan
> for that reality. Researched via a 3-agent workflow + local clippy/check runs. Not implemented —
> awaiting the OPERATOR decisions below.

## Ground-truth re-verify (prompt 02 facts — all hold, 3 flags)

All 9 facts in prompt 02 still hold. Drift since 2026-06-11:
- **FLAG A — orchestrator matrix is 7, not 3.** `Cargo.toml` exclude now lists `common, ollama,
  mongodb, postgresql, valkey, weaviate, ai` (Cargo.toml:16-26). The prompt's matrix `{ollama,
  mongodb, common}` must expand to all (ai per prompt 04's keep/leave call).
- **FLAG B — version is 0.2, not 0.1.** `version.json` = `{major:0, minor:2}` (no patch/commit). The
  prompt's `v0.1.0` conflicts → first tag should be **`v0.2.0`**.
- **FLAG C — extra `Dockerfile.moss`** at root (not a cross-compile target; the release matrix must
  not pick it up).

Local verification: `cargo check --workspace --all-targets` → **exit 0** (45s); the gate's rustc step
is green against local koi.

## The re-scope: koi as a sibling everywhere

zen's path dep `koi-* = { path = "../koi/crates/*" }` (Cargo.toml:99-103) resolves `../koi` relative to
the workspace root. So every job/Docker build that compiles moss/rake/lantern/pavilion must obtain koi
as a sibling. (garden-common and ALL orchestrators are koi-free — verified — so orchestrator jobs need
no koi.)

**GitHub Actions trap:** you cannot `actions/checkout` above `$GITHUB_WORKSPACE`. Fix: check out BOTH
repos into subdirs and run cargo from the zen subdir, so `../koi` resolves within the workspace:
```
$GITHUB_WORKSPACE/
  zen-garden/   <- actions/checkout path: zen-garden   (cwd for cargo)
  koi/          <- actions/checkout repository: sylin-org/koi, path: koi  => ../koi ✓
```

**Docker release builds need NO Dockerfile change.** The four `Dockerfile.linux-*` builders use a
**mounted-volume** pattern (no `COPY`): the compile-*.ps1 scripts bind-mount workspace→`/build:ro` and
`../koi`→`/koi:ro` with `-w /build`, so `/build/../koi == /koi` (compile-linux-x64.ps1:296-366). CI
reproduces the same two mounts after the sibling checkout. (Orchestrator images COPY from
context=workspace-root and don't touch koi — unchanged.)

## ⚠️ OPERATOR decision 1 (blocking): which koi ref does CI build against?

koi default branch `main` = `d6bf456`, but the live dogfood branch `feat/p07-one-orchestrator` is **13
commits ahead** and is what local green builds link against. The SSOT path refactor zen now depends on
is NOT on `main` — so `ref: main` in CI would build against pre-refactor koi and **fail to compile**.

- **ci.yml (gate): track the reconciled koi SSOT branch.** Recommend the maintainer merge the dogfood
  branch into a stable koi branch (`main` or `dev`), then CI uses `ref: <that branch>`. Auto-follow is
  correct for a gate whose job is "did koi break us." **You must declare koi's SSOT branch first.**
- **release.yml (tag→binaries): PIN a koi SHA.** A shipped artifact must be reproducible; capture the
  koi SHA at release time, check koi out at it, and record it (see traceability below).

## OPERATOR decision 2: platforms, tag name, branch protection

- **Platforms** (recommend): linux-x64, linux-arm64 (glibc), linux-arm64-musl, windows-x64 — skip
  linux-x86 (frozen). (The pipeline also targets i686; the assessment freezes it.)
- **First tag**: `v0.2.0` (reconcile with version.json 0.2 — NOT the prompt's v0.1.0).
- **Branch protection**: out of scope to change unilaterally (prompt says so).

## ci.yml — the gate (push/PR to dev)

```yaml
name: ci
on: { push: { branches: [dev] }, pull_request: {} }
jobs:
  workspace:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { path: zen-garden }
      - uses: actions/checkout@v4
        with: { repository: sylin-org/koi, ref: <SSOT-branch>, path: koi }  # decision 1
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with: { workspaces: zen-garden }
      - run: cargo check --workspace --all-targets
        working-directory: zen-garden
      - run: cargo clippy --workspace -- -W warnings   # see clippy note — start at -W
        working-directory: zen-garden
      - run: cargo test --workspace
        working-directory: zen-garden
      - uses: EmbarkStudios/cargo-deny-action@v2
        with: { manifest-path: zen-garden/Cargo.toml }
  orchestrators:
    runs-on: ubuntu-latest
    strategy:
      matrix: { crate: [common, ollama, mongodb, postgresql, valkey, weaviate] }  # +ai per prompt 04
    steps:
      - uses: actions/checkout@v4
        with: { path: zen-garden }
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --all-targets
        working-directory: zen-garden/src/orchestrators/${{ matrix.crate }}
```

### clippy note (gate starts at `-W`, not `-D`)

`cargo clippy --workspace -- -D warnings` is **not clean**: 99 warnings (via `--all-targets`),
dominated by collapsible-if (22), needless stuff, 7 `undocumented_unsafe_blocks`, 4 too-many-args.
Plus **2 deny-by-default correctness lints in test code** (`never_loop`, `approx_constant` in
garden-common tests) — these only trip `clippy --all-targets`, not the gate's `clippy --workspace`.
Per prompt 02: start the gate at `-W warnings` with a `# TODO(quality-gate)` and record the count in
FINDINGS.md. (Fixing them is prompt 03 / a quality follow-up. The 2 correctness lints should be fixed
before the gate ever moves to `clippy --all-targets -D`.)

## release.yml — tag-driven (`v*`)

Two job groups, both after the dual sibling checkout:
1. **stone binaries** (matrix: linux-x64, linux-arm64, linux-arm64-musl, windows-x64). Reuse the
   existing pipeline: on ubuntu runners run the linux `compile-*.ps1` via `pwsh` (they already mount
   `../koi`→/koi); on a windows runner run `compile-windows-x64.ps1` natively (resolves `../koi` on
   disk). OR call the Docker builders directly with `-v zen:/build:ro -v koi:/koi:ro -w /build`.
   Upload `garden-moss`/`garden-rake`/`garden-lantern` (+cricket/firefly for `full`) per target as
   release assets. (musl excludes lantern; moss/cricket build `--no-default-features`.)
2. **orchestrator images** — out of scope for the first binary release (prompt: don't publish Docker
   images). Note only.

## Version → SHA traceability (reuse the existing seam, not vergen)

`garden-build-utils::capture_build_number()` (src/build-utils/src/lib.rs:29-33) already injects
`BUILD_NUMBER` via `cargo:rustc-env`; moss (src/moss/src/cli.rs:12,109) and rake
(src/rake/src/cli_build.rs:35-39) build `--version` = `CARGO_PKG_VERSION + "." + BUILD_NUMBER`.

- Add a `GIT_SHA` emission in `capture_build_number()` (CI env `GIT_SHA` → `git rev-parse --short=7`
  fallback → `"unknown"`); append `+{GIT_SHA}` (SemVer build metadata) to the two version `concat!`s.
  Result: `garden-rake --version` → `0.2.0.<buildnum>+abc1234`. Local debug builds fall back to
  `git rev-parse`, satisfying the DoD that even a debug `--version` carries the SHA. (lantern has its
  own inline build.rs copy; can opt in — not in the DoD.)
- In CI set `GIT_SHA=${GITHUB_SHA:0:7}` (stable under shallow/detached checkout).
- `version.json` stays the major/minor SSOT (don't hardcode commit). In `installer/build.ps1` after
  line 89 set `$env:GIT_SHA = git rev-parse --short=7 HEAD`, and emit an ENRICHED `version.json` into
  the package dir with `commit` (zen SHA) + `koi_commit` (the pinned koi SHA) — closing both halves of
  traceability.

## Other implementation items (from prompt 02)

- Delete the hardcoded `-SkipTests` at `installer/build.ps1:184` (make it a `$false`-default param).
- `RELEASING.md` (≤20 lines): tag → workflow → assets; note the koi-ref pinning step.
- Record the clippy warning count in FINDINGS.md (gate started at `-W`).

## Definition of done (adapted)

- [ ] `ci.yml` + `release.yml` exist; YAML lint passes; the gate commands run **green locally** with
      koi as a sibling: `cargo check --workspace --all-targets` ✓ (already verified), `cargo test
      --workspace` green, `cargo clippy --workspace -- -W warnings` (records count), cargo-deny.
- [ ] `garden-rake --version` (debug) contains the short SHA.
- [ ] `installer/build.ps1` no longer hardcodes `-SkipTests`.
- [ ] `RELEASING.md` documents tag → workflow → assets incl. the koi-ref pin.
- [ ] FINDINGS.md records the clippy count.
- [ ] Nothing pushed / no tag — report the exact `git push` / `git tag v0.2.0` commands for the operator.

## Out of scope

Fixing clippy warnings; deleting `tests/` (prompt 03); changing branch protection; publishing
orchestrator Docker images; touching install scripts (prompts 06/15).

## Open items to resolve before implementing

1. **koi SSOT branch** (decision 1) — blocking; CI can't pick a `ref:` until koi's branches are
   reconciled so the refactored koi zen needs is on a stable branch.
2. **Private vs public koi repo** — if private, the koi checkout needs a deploy key / PAT
   (`token:`); if public, `GITHUB_TOKEN` suffices.
3. Platforms + tag name (decision 2).
