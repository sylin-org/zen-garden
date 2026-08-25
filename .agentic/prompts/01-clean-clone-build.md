# 01 — Make a Clean Clone Compile

> A stranger's `git clone && cargo check --workspace` must succeed with no sibling checkouts and no
> instructions beyond the README. Phase: Gate. Depends on: nothing. Blocks: everything (02 CI, all
> contributors, all releases).

## Mission

The root workspace currently cannot compile from a clean single-repo clone because it declares **path
dependencies on a sibling repo** (`../koi`). Convert those to published crates.io dependencies, keep a
frictionless local-development override for the maintainer (who hacks on koi and zen-garden together), and
document the one-command build in the README. When you finish, a fresh clone on a machine that has never
seen koi builds the entire root workspace.

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| Root `Cargo.toml` lines ~98–102 declare five path deps: `koi-embedded`, `koi-certmesh`, `koi-common`, `koi-crypto`, `koi-truststore` → `../koi/crates/...` | `grep -n "koi" Cargo.toml` |
| All five koi crates are **published on crates.io** (sibling repo: github.com/sylin-org/koi) | `cargo search koi-certmesh` (or check https://crates.io/crates/koi-certmesh) |
| Four workspace members inherit them: moss (4 koi crates), rake (2), lantern (1), pavilion (1) | `grep -rn "koi-" src/*/Cargo.toml` |
| Orchestrator crates (`src/orchestrators/*`) are excluded from the workspace and have **zero** koi deps — they already clean-build | `grep -c koi src/orchestrators/*/Cargo.lock` (expect 0s) |
| README.md contains no build instructions at all | `grep -n "cargo" README.md` |

## Research first (~30 min)

1. Read root `Cargo.toml` fully — workspace members, the koi dependency block and its comments.
2. Determine the **published versions** of the five koi crates and whether the local `../koi` checkout has
   diverged from them: compare `../koi/crates/*/Cargo.toml` versions against crates.io. If the local koi
   is AHEAD of crates.io (unpublished changes that zen-garden depends on), this is an OPERATOR item — the
   fix then starts with publishing koi, which only the maintainer can do.
3. Read how Cargo `[patch.crates-io]` interacts with workspaces (it must live in the workspace root
   manifest; patched paths are ignored when absent only if you gate them — they are NOT, so see Target
   shape for the chosen pattern).

## Plan gate

Produce a short plan stating: the version pins you will use, the local-override mechanism you chose, and
the README addition. **OPERATOR**: if local koi has unpublished changes the workspace needs (research
step 2), stop and present the version diff instead of proceeding.

## Target shape

Workspace dependencies become version deps:

```toml
[workspace.dependencies]
koi-embedded   = "0.x"   # pin to the published minor actually used
koi-certmesh   = "0.x"
koi-common     = "0.x"
koi-crypto     = "0.x"
koi-truststore = "0.x"
```

Local development override — **not** committed in the manifest. Use Cargo's config-level patching so the
committed tree is clean-clone-true and the maintainer opts in locally:

```toml
# .cargo/config.toml.example  (committed; the maintainer copies to .cargo/config.toml, which is gitignored)
[patch.crates-io]
koi-embedded   = { path = "../koi/crates/koi-embedded" }
koi-certmesh   = { path = "../koi/crates/koi-certmesh" }
koi-common     = { path = "../koi/crates/koi-common" }
koi-crypto     = { path = "../koi/crates/koi-crypto" }
koi-truststore = { path = "../koi/crates/koi-truststore" }
```

README gains a Build section (place after Getting Started; keep it five lines):

```markdown
## Building from source

    git clone https://github.com/sylin-org/zen-garden && cd zen-garden
    cargo build --workspace            # builds moss, rake, lantern, companions
    cd src/orchestrators/ollama && cargo build   # orchestrators build standalone

Developing against a local koi checkout? Copy `.cargo/config.toml.example` to `.cargo/config.toml`.
```

## Implementation

1. Pin the five version deps in `[workspace.dependencies]`; remove the path entries and the
   "old structure / Phase 6" fossil comments at the top of the members list while you are in the file.
2. Create `.cargo/config.toml.example`; add `.cargo/config.toml` to `.gitignore`.
3. Regenerate `Cargo.lock` (`cargo update -w` is NOT wanted — run `cargo check --workspace` and let only
   the koi entries change; inspect the lock diff to confirm nothing else moved).
4. Add the README Build section.
5. Commit in two: `fix(build): replace koi path deps with published crates` and `docs: add build-from-source section`.

## Definition of done

- [ ] `git stash -u && git clone . /tmp/zg-clean && cd /tmp/zg-clean && cargo check --workspace` exits 0
      on a machine path where `../koi` does not exist (simulate: clone into a directory whose parent has
      no koi). Report the full output tail.
- [ ] `cd src/orchestrators/ollama && cargo check` still exits 0 (unchanged, but prove it).
- [ ] `git grep -n '\.\./koi' -- Cargo.toml src/` returns nothing.
- [ ] With `.cargo/config.toml` copied from the example and a koi checkout present, `cargo check
      --workspace` also exits 0 (maintainer path still works).
- [ ] Lock-file diff touches only koi-related entries.

## Out of scope

Do not upgrade any non-koi dependency. Do not touch orchestrator crates. Do not restructure the workspace
members list beyond deleting the dead comments. Do not publish anything.
