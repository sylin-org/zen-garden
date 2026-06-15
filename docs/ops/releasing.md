# Releasing Zen Garden

**Purpose:** How a git tag produces installable binaries, with version→commit traceability.
**Audience:** Maintainers

---

Zen Garden ships **compiled binaries** (it is an application, not a crates.io library) and builds koi
from the sibling `../koi` checkout (see [koi-dependency.md](../guides/koi-dependency.md)).

## Cutting a release

1. Ensure `dev` is green in CI (`.github/workflows/ci.yml`) and that the koi `../koi` resolves to is the
   koi you intend to ship.
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0` (the version line tracks `version.json`'s
   `major.minor` — currently `0.2`).
3. The `release` workflow (`.github/workflows/release.yml`, tag `v*`):
   - checks out zen-garden and koi as siblings so `../koi` resolves,
   - cross-compiles per target via the root `Dockerfile.linux-*` (koi bind-mounted at `/koi`, workspace
     at `/build`) and a native windows runner,
   - sets `GIT_SHA` (zen short SHA) and records the koi SHA, and
   - uploads `garden-moss` / `garden-rake` / `garden-lantern` (+ companions) per target as release assets.

## Targets

linux-x64, linux-x86, linux-arm64 (glibc), linux-arm64-musl, windows-x64.

## Version / traceability

`garden-rake --version` → `{major}.{minor}.{build}+{sha}` (e.g. `0.2.0.202601231053+abc1234`). Each
package's generated `version.json` carries `commit` (zen SHA) and `koi_commit` (the koi SHA built against).

## Local builds

`installer/build.ps1` produces the same packages locally (it resolves the real `../koi` on disk). Tests
run by default; pass `-SkipTests` only deliberately.

> The `release` workflow is the companion to the local pipeline and is added alongside this doc.
