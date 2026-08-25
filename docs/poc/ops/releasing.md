# Releasing Zen Garden

**Purpose:** How a git tag produces installable binaries, with version→commit traceability.
**Audience:** Maintainers

---

Zen Garden ships **compiled binaries** (it is an application, not a crates.io library) and builds koi
from the sibling `../koi` checkout (see [koi-dependency.md](../guides/koi-dependency.md)).

## What exists today

- **Local pipeline** — `installer/build.ps1` produces every package on this machine, resolving the real
  `../koi` on disk. Tests run by default; pass `-SkipTests` only deliberately. This is the working
  release path right now.
- **CI quality gate** — `.github/workflows/ci.yml` builds and tests the workspace (koi checked out as a
  sibling) and `cargo check`s the orchestrators on every push/PR.

## Planned: tag-driven CI release

> **Status: deferred pending a stable koi version surface.** `.github/workflows/release.yml` is not yet
> built. A released binary statically embeds koi, so reproducible releases require koi to guarantee a
> version surface — published semver crates, or stable tags zen can depend on with a range (see
> [koi-dependency.md](../guides/koi-dependency.md)). While koi is pre-1.0 and dogfooded from a local
> `../koi` (a moving target that makes breaking changes), release automation is on hold — cut releases
> with the local pipeline above. The steps below describe the intended shape so the local pipeline and
> the future workflow stay aligned; resume this work once koi offers a version surface.

A release will be cut by tagging:

1. Ensure `dev` is green in CI and that the koi `../koi` resolves to is the koi you intend to ship.
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0` (the version line tracks `version.json`'s
   `major.minor` — currently `0.2`).
3. The `release` workflow (`.github/workflows/release.yml`, tag `v*`) will:
   - check out zen-garden and koi as siblings so `../koi` resolves,
   - cross-compile each **Linux** target via the root `Dockerfile.linux-*` (koi bind-mounted at `/koi`,
     workspace at `/build`) and build **windows-x64** on a native Windows runner,
   - set `GIT_SHA` (zen short SHA) and record the koi SHA, and
   - upload `garden-moss` / `garden-rake` / `garden-lantern` (+ companions) per target as release assets.

## Targets

linux-x64, linux-x86, linux-arm64 (glibc), linux-arm64-musl — via the root `Dockerfile.linux-*`;
windows-x64 — native (`installer/build-windows-x64.ps1`).

## Version / traceability

`garden-rake --version` → `{major}.{minor}.{patch}.{build}+{sha}` (e.g. `0.2.0.202601231053+abc1234`):
`CARGO_PKG_VERSION` (`major.minor.patch`) + `BUILD_NUMBER` + the short git SHA. This is wired today in
moss and rake — `src/build-utils` injects `GIT_SHA` (CI `$GIT_SHA`, else `git rev-parse`, else `unknown`).

Enriched per-package `version.json` carrying `commit` (zen SHA) and `koi_commit` (the koi SHA built
against) is **planned** alongside `release.yml`; today's `version.json` carries `major`/`minor` only.
