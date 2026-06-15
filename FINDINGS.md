# FINDINGS

Adjacent problems noticed while working a scoped prompt, recorded but **not fixed**
(out of the working prompt's scope). See `.agentic/prompts/PROGRESS.md` for the
execution ledger.

## 2026-06-13 — clean clone cannot build `garden-lantern` (frontend dist artifact)

**Found while:** prompt 01 (clean-clone-build / "match koi"), validating a fresh
`git clone .` build against published koi 0.3.0.

**Problem.** With koi pinned to `0.3` and bollard aligned to `0.21`, a clean clone
resolves the whole dependency graph and compiles every koi-dependent crate — but
`cargo check --workspace` still fails on `garden-lantern`:

```
error: #[derive(RustEmbed)] folder 'src/lantern/frontend/dist/' does not exist.
error[E0599]: no function or associated item named `get` found for struct `FrontendAssets`
```

`src/lantern/frontend/dist` is a **built frontend artifact** that is gitignored
(`.gitignore:21`) and therefore absent from any fresh clone. The maintainer's
working tree builds because the dir exists locally. This blocker is **independent
of koi** — it would fail a clean clone regardless of the koi dependency change.

**Why not fixed here.** Out of scope for "match koi" (a frontend
packaging/build-ordering concern, not a dependency reference). It is, however,
squarely within prompt 01's stated mission ("a fresh clone … builds the entire
root workspace"), so prompt 01 is **not fully done** until it is addressed.

**Candidate fixes (for a future session / prompt 01 follow-up):**
- Wire the lantern frontend build into the cargo build via a `build.rs` that runs
  the frontend toolchain (or fails with a clear "run `npm run build` first" message).
- Or generate a placeholder `dist/` (e.g. an empty `index.html`) at build time when
  the real one is absent, so `cargo check` succeeds without the frontend toolchain.
- Or have `RustEmbed` point at a path that always exists and load the real assets at
  runtime when present.

Decision on which approach is a maintainer call (affects the contributor build story
and CI in prompt 02).

**RESOLVED 2026-06-15** (commit `35883baa`): took the placeholder approach — `src/lantern/build.rs`
ensures `frontend/dist/` exists and writes a placeholder `index.html` when the real SPA is absent
(a populated `frontend/dist/` is left untouched, so release builds embed the real assets). Verified
with `frontend/dist` moved aside: `cargo check -p garden-lantern` exits 0 via the placeholder; the
real-dist build is a no-op. This was prompt 02's first concrete (koi-independent) prerequisite.

## 2026-06-13 — `koi-embedded` force-feeds `bollard` to every consumer (KOI issue)

**Found while:** prompt 01 ("match koi"), diagnosing why zen-garden's `bollard 0.20`
collided with koi 0.3.0.

**Problem (root cause of the whole "match koi" scramble).** The dependency chain
`koi-embedded → koi-runtime → bollard` is **mandatory** — no cargo features anywhere:

- `koi-embedded/Cargo.toml`: `koi-runtime.workspace = true` (not optional).
- `koi-runtime/Cargo.toml`: `bollard = "0.21"` (not optional); used **only** in
  `koi-runtime/src/docker.rs` (one of several trait backends: Docker/Podman/systemd/…).

Yet `koi-embedded` already treats the runtime adapter as **off by default** at the API
level (`KoiConfig.runtime_enabled = false`, `RuntimeCore` held as `Option`). So a
consumer that only wants discovery/DNS/health/TLS (exactly zen-garden's usage — it
**never** references `RuntimeConfig`/`RuntimeBackendKind`/`koi_runtime`) still:
1. compiles `bollard` + the Docker API stubs it never calls, and
2. is **version-locked to koi's exact bollard** — because `bollard-stubs` pins with `=`
   and two bollard minors cannot coexist, any consumer that *also* uses bollard (moss's
   own Docker layer) must match koi's version. That coupling is what forced zen's
   `0.20 → 0.21` bump.

**Verdict:** this is a **Koi** architecture issue, not a zen-garden one. The fix is to
make the runtime backend opt-in via cargo features (`koi-embedded/runtime`,
`koi-runtime/docker → dep:bollard`). Then zen sets
`koi-embedded = { version = "0.3", default-features = false }` and pulls **zero** bollard
from koi — decoupling moss's bollard entirely.

**Status (updated 2026-06-13).** Koi has **already implemented** this — `koi-embedded`
now has `[features] default = ["docker","keyring","qr"]` with `docker = ["koi-runtime/docker"]`,
`koi-runtime` taken as `default-features = false`, documented in `docs/guides/embedded.md`
and `docs/adr/014-optional-backend-features.md` (koi branch `feat/optional-backend-features`,
commits `a09ae9d` + `10cb5d4`). It is **not yet published** — crates.io koi is still `0.3.0`
(no features; the guide targets `0.4`).

zen-garden's proper fix is therefore **lean consumption**, not the `bollard 0.21` bump:
once koi publishes `0.4`, set `koi-embedded = { version = "0.4", default-features = false,
features = [...] }`, which pulls **no** bollard from koi, and revert moss's bollard bump
(moss owns its own bollard version, decoupled). Feature needs for zen: `docker` **off**
(zen never uses koi's runtime backend); `qr` likely **on** (pavilion requests
`QrFormat::PngBase64` from the moss-hosted ceremony); `keyring` is a security-posture
decision (OS keychain vs. passphrase vault on headless stones — moss uses `koi_crypto::vault`).
The `bollard 0.21` bump now in the working tree is an INTERIM unblock for the currently
published koi `0.3.0` only.

**Decisions (2026-06-13).** Maintainer chose to **wait for koi 0.4 publish** (rather than
ship the interim) and to **drop `keyring`** (passphrase vault backend — better fit for
headless stones). koi 0.4.0 is building in CI as of 2026-06-13. The lean target for zen:
- `koi-embedded = { version = "0.4", default-features = false, features = ["qr"] }`
  (`qr` kept for pavilion's `QrFormat::PngBase64` ceremony; `docker` + `keyring` off)
- other `koi-* = "0.4"`; ensure no graph edge re-arms `keyring`/`docker` (verify with
  `cargo tree -e normal | grep -E 'bollard|keyring|image'` → empty)
- **revert** `src/moss/Cargo.toml` to `bollard = "0.20"` (moss owns its bollard, decoupled)
- verify: `cargo check --workspace`, `cargo test -p garden-moss --lib`, clean-clone build

## 2026-06-14 — lean migration implemented + adversarially verified

The lean config was implemented and is **green against local `../koi` 0.4.0** (path override):
`koi-embedded/koi-certmesh = { "0.4", default-features = false }`,
`koi-crypto = { "0.4", default-features = false, features = ["qr"] }`,
`koi-common/koi-truststore = "0.4"`, and `src/moss/Cargo.toml` `bollard = "0.20"`.
`cargo check --workspace` green; 947 garden-moss `--lib` tests pass. Dep-graph proof
(Windows + Linux targets): bollard's only consumer is garden-moss (0.20.2, single copy);
`keyring`/`secret-service` crates absent; `qrcode`+`image` present (qr kept); `koi-runtime`
present but bollard-free.

### BLOCKER (koi release, high) — koi 0.4 publish is incomplete

`koi-dashboard` (a new crate extracted in koi 0.4, commit `599504b`) is **not published to
crates.io at any version** (`https://index.crates.io/ko/i-/koi-dashboard` → `NoSuchKey`).
`koi-embedded 0.4` has a **non-optional** dependency on it
(`crates/koi-embedded/Cargo.toml:22` `koi-dashboard = { workspace = true, default-features = false }`).
Therefore `koi-embedded 0.4.0` **cannot be published** and **is absent** from crates.io
(latest there is `0.3.0`), while the other 12 koi crates published at `0.4.0`. Consequence:
with the local override removed (clean clone / CI / Docker), zen's `koi-embedded = "0.4"`
**cannot resolve** — failing both garden-moss and garden-lantern. The green build above only
worked via `.cargo/config.local.toml` (lock koi entries are path deps, no checksum).
**zen's migration is NOT registry-shippable / committable until koi publishes `koi-dashboard`
0.4.0 and then `koi-embedded` 0.4.0.** (Owner: koi release pipeline — re-run/repair publish.)

### keyring drop — SAFE for default flows, with a UX caveat (verified)

Two adversarial refuters confirmed dropping `keyring` does **not** break existing stones:
zen's boot/auto-unlock path reads a **plaintext `{koi_data_dir}/auto-unlock-key` file** and
passphrase-decrypts the CA via the slot table (`koi-embedded/src/lib.rs:725-751`,
`koi-certmesh/src/ca.rs:180-218`) — **no keyring, no vault, no `tpm::is_available()`**. The
pond CA is passphrase/slot-table encrypted; keyring only ever stored a redundant machine-bound
copy (warn-and-proceed if absent, `koi-crypto/src/keys.rs:198-215`). Passphrase unlock (the
universal fallback) and JustMe/MyTeam auto-unlock are unaffected.

CORRECTION to the earlier "better fit for headless stones / OS keychain unavailable" rationale
(above): it is **factually wrong**. koi pins `keyring` with `linux-native` → the **keyutils**
kernel backend, which works on a headless systemd daemon (no D-Bus needed), so koi 0.3.0 stones
*did* use the keyring vault backend. The correct rationale for the drop being safe is: (a) the
unlock path never reads the vault, and (b) koi's keyutils backend is `CredentialPersistence::UntilReboot`
(in-memory; silently rotates the master key across reboots), so the machine-bound Argon2id-over-
`/etc/machine-id` fallback is in fact **more stable**, not less.

CAVEAT (degraded feature, per koi ADR-014): with keyring off, **TOTP credential-store unlock
slots** are unavailable — `add_totp_slot` returns `Err` (its fallback also needs the credential
store). This is the documented trade-off and the default JustMe/MyTeam passphrase flow doesn't
use it. **But moss's pond ceremony still hands the user a TOTP QR even when slot creation failed**
(error only `tracing::error!`-logged at `src/moss/src/api/v1/pond.rs:1274-1276`) → the user scans
a QR for a slot that doesn't exist and a later TOTP unlock returns `NoSlotFound`. **Zen-side fix
worth doing:** when keyring/`add_totp_slot` is unavailable, don't offer TOTP (hide it or return a
clear "TOTP unlock unavailable on this build; use passphrase") instead of a dead QR.

### SEPARATE koi 0.4 regression (not keyring) — auto-unlock writer/reader mismatch

koi 0.4's `save_auto_unlock_key` now writes the passphrase to the **vault** and **deletes** the
legacy plaintext `auto-unlock-key` file (`koi-certmesh/src/lib.rs:732-740`), but the boot reader
`init_certmesh_core` still reads the **plaintext file** (`koi-embedded/src/lib.rs:725-727`) and
never calls `try_auto_unlock` (the vault reader; zero callers repo-wide). Net: on JustMe/MyTeam
stones the CA can boot **LOCKED**, needing a manual `garden-rake pond unlock` after reboot.
Present with keyring ON or OFF → **independent of zen's keyring decision; a koi 0.4 bug** (fix:
point koi-embedded's reader at `try_auto_unlock()`/the vault). Owner: koi.

### Doc items (zen, medium/low)

- `docs/CHANGELOG.md`: no entry for the koi 0.4 lean migration / dropped `docker`+`keyring` /
  bollard 0.20 revert / TOTP-slot unavailability. Add one when the migration lands.
- `src/moss/src/infra/secrets.rs:6-8` docstring overclaims "Platform credential store … when
  available" — in the lean build koi never seals to an OS store; master-key/CA protection is
  always machine-bound Argon2id. Update the docstring.

### Corrections to earlier notes in this file

- The `zbus`/`dbus` lock entries are **NOT** keyring orphans and are **not** prunable: they are
  live transitive Tauri deps of **garden-pavilion** (`zbus ← notify-rust ← tauri-plugin-notification`;
  `dbus ← tao ← tauri-runtime-wry`). The genuine keyring residue (`keyring`, `secret-service`) is
  correctly **absent**.
- Orchestrators confirmed **koi-free** (none depend on any `koi-*`); koi entries seen in some
  orchestrator locks are benign `[[patch.unused]]` from the repo-wide local override and clear on
  a clean re-resolve.

## 2026-06-14 — koi data-path single-source-of-truth (architecture decision)

The auto-unlock investigation surfaced a deeper, pre-existing issue: koi's data directory is an
**ambient global** (`koi_common::paths::koi_data_dir()` / `impl Default for CertmeshPaths`) re-resolved
at ~30 operational sites, so a host that configures a custom `data_dir` (moss does:
`.data_dir(data_dir()/koi)`, run.rs:567) gets a **split** — the certmesh core uses the injected dir
while ambient `CertmeshPaths::default()` calls (moss has 6: pond.rs 748/1003/1026/1247, run.rs 634/1661,
pond_lifecycle.rs:166) silently use koi's platform default. The headline koi bug: `init_certmesh_core`
(koi-embedded 708/715, koi binary 896/904/909) computes injected `paths` then drops them via no-paths
`uninitialized()`/`locked()` constructors.

**Decision (maintainer):** Option A — **koi owns a single machine-scoped data root** (DDD value object,
resolved once at each composition root, owned by `CertmeshCore`, injected). Greenfield, no migration.
A precise koi-agent prompt for the SSOT refactor + the zen-side follow-on is in
[`docs/notes/koi-path-ssot-prompt.md`](koi-path-ssot-prompt.md) (mapped by a 4-agent workflow over koi).
zen-side: drop the `.data_dir()` override (koi owns the location) and, after the koi refactor exposes
`core.paths()`, route moss's 6 `default()` sites through the injected core. Not yet implemented —
sequenced after the koi refactor.

**IMPLEMENTED 2026-06-14** (commits `baee86a5`, `327231b7`, `b9fa3417`). The koi SSOT refactor was
verified done (4-agent audit + koi clippy disallowed-methods gate + koi tests green). zen now:
- **consumes koi via PATH DEPS** (local `../koi`, lean features) — a deliberate dogfooding choice
  *until koi stabilizes*, NOT crates. Clean-clone-from-crates is intentionally deferred; the
  switch-back procedure is in [`docs/guides/koi-dependency.md`](../docs/guides/koi-dependency.md).
- **migrated to the refactored koi API**: 13 sites across `pond.rs`/`run.rs`/`pond_lifecycle.rs`/
  `testing.rs`/`security/tests.rs` now use `core.paths()` (not the removed `CertmeshPaths::default()`),
  the `&self` `configure_auto_unlock_for_profile`, `PondCeremonyRules::new(paths)`, and drop the
  removed `delete_auto_unlock_key`. `cargo check --workspace` + 947 moss tests green vs local koi.
- **KEPT `.data_dir(data_dir()/koi)`** rather than strict-Option-A dropping it. Rationale: with the
  SSOT refactor now honoring a custom data_dir consistently, dropping it would relocate every stone's
  pond CA/certs/vault to koi's platform default (`/var/lib/koi`), which is risky cross-platform (esp.
  Android). Keeping it preserves the data location and still satisfies SSOT (zen provides the root once
  at the composition root; koi owns + injects it). **Decision flagged for the maintainer** — say the
  word to switch to strict drop.

**Open koi-side nit (not zen):** koi's `CertmeshCore::destroy()` removes the certmesh dir/certs/audit +
the `koi-certmesh-ca` TPM key but NOT the auto-unlock *vault* entry, and the standalone
`delete_auto_unlock_key` was removed — so a drained pond leaves an inert auto-unlock vault entry
(encrypted passphrase for a destroyed CA; harmless, overwritten on next init). A koi follow-up could
fold that cleanup into `destroy()`.

## 2026-06-15 — clippy backlog (prompt 02 CI gate starts at `-W`)

`cargo clippy --workspace -- -D warnings` is not clean, so the CI gate (`.github/workflows/ci.yml`)
starts at `-W warnings` with a `TODO(quality-gate)` to ratchet to `-D` after cleanup (prompt 03).
`cargo clippy --workspace --all-targets` reports **99 warnings**, dominated by: collapsible-if (22),
needless/manual idioms (~14), useless conversions (7), `undocumented_unsafe_blocks` (7), too-many-args
(5), constant-value assertions (4). It also surfaces **2 deny-by-default *correctness* lints in test
code** — `never_loop` and `approx_constant` in `garden-common` tests — which fail `clippy --all-targets`
(they do NOT affect rustc `check`/`test`, which are green). The gate uses `clippy --workspace` (no
`--all-targets`), so those two don't block it today; they must be fixed before the gate ever moves to
`clippy --all-targets -D`.

## 2026-06-15 — Windows local `cargo test --workspace` blocked by UAC installer-detection (CI unaffected)

The integration test [`src/companion-sdk/tests/coalescing_load_updates.rs`](src/companion-sdk/tests/coalescing_load_updates.rs)
compiles to `coalescing_load_updates-<hash>.exe`. Because the filename contains the substring
**`update`**, Windows' UAC *installer-detection heuristic* tags the unmanifested binary as an installer
and refuses to launch it without elevation, so `cargo test --workspace` aborts with
`os error 740 (ERROR_ELEVATION_REQUIRED)` — the binary "never executed" (it is NOT a test/assertion
failure). Trigger keywords are `install` / `setup` / `update` / `patch`; this is the only target in the
workspace that hits one (verified by name scan of `tests/*.rs` and crate names).

**Impact is local-Windows-only.** The CI gate runs `cargo test --workspace` on `ubuntu-latest`, which
has no installer-detection heuristic — the binary runs normally there. `cargo test --workspace
--no-fail-fast` on this Windows box is logically green: every other test binary passes; the *only*
failure is this one launch-elevation error. `cargo check --workspace --all-targets` is also green
(compilation is unaffected; only execution is blocked).

**Fix options (out of prompt-02 scope — recorded for the maintainer):**
- Rename the test file so the binary name avoids the trigger word — but "load updates" is the genuine
  domain subject (coalescing of load-update events), so a rename would obscure intent.
- Embed an `asInvoker` application manifest in the test binary (e.g. via an `embed-manifest`/`winres`
  build step keyed to `cfg(test)` + `cfg(windows)`) so Windows stops auto-elevating it. This is the
  clean root-cause fix but adds Windows-only test-build machinery.
- Leave as-is: local Windows devs run `cargo test --workspace --exclude garden-companion-sdk` (or test
  that crate from an elevated shell); CI is the authoritative test gate and is unaffected.

## 2026-06-15 — CI/release artifact review (adversarial, 5 lenses → 23 confirmed)

A 30-agent adversarial review of the uncommitted CI/release artifacts (ci.yml, releasing.md,
koi-sibling setup) before commit. Outcomes:

**Fixed in this scope:** releasing.md was rewritten to mark `release.yml` + enriched `version.json`
(`commit`/`koi_commit`) as **planned, not present** (they had been described in present tense; the
release workflow is the deferred koi-dependent follow-up). The documented `--version` format was
corrected from `{major}.{minor}.{build}+{sha}` to `{major}.{minor}.{patch}.{build}+{sha}` (the code
concatenates the full 3-part `CARGO_PKG_VERSION`; the example was already right, the template wasn't).
The orchestrators `rust-cache` step gained `workspaces: …/orchestrators/${{ matrix.crate }}` — without
it rust-cache keys off the root workspace (which *excludes* orchestrators) so every matrix leg
cache-missed.

**Verified false alarm — `mongocrypt-sys`/`libmongocrypt` on ubuntu.** A reviewer claimed the mongodb
orchestrator's `cargo check` would fail on ubuntu without `libmongocrypt`. It will not: mongocrypt-sys
(`~/.cargo/.../mongocrypt-sys-0.1.5+1.15.1/build.rs`) ships a pregenerated `bindings.rs` and only emits
`cargo:rustc-link-lib=dylib=mongocrypt` — a *link* directive. `cargo check --all-targets` emits metadata
and never links, so the directive is inert. No system lib needed for the check gate (a future
`cargo build`/`test` for that leg would need `libmongocrypt-dev`; noted in a ci.yml comment).

**Deferred — `cargo fmt --check` gate (prompt 03).** ci.yml has no fmt gate, and the workspace is not
fmt-clean today: `cargo fmt --check --all` reports diffs in `src/common/src/client/stone_api.rs`
(~lines 392, 1334+). Adding the gate now would turn CI red on a pre-existing condition. Sequenced with
the clippy backlog cleanup (prompt 03): reformat, then add `cargo fmt --check --all` to the workspace
job. Pure formatting, out of prompt-02 scope.

**Deferred (koi-dependent, already planned) — `release.yml` + `version.json` enrichment.** The "blocker"
cluster (release.yml absent) is the deliberate deferral, not an oversight; releasing.md now says so.
The CLI version wiring, all five koi path deps, and the lean koi feature selection were independently
re-confirmed correct by the review (positive findings).

## 2026-06-15 — koi moved under the workspace → migrated zen to koi 0.4.2 (commit `aec0f024`)

Mid-session, a re-verify of `cargo test --workspace` failed to compile garden-moss (22 errors). Root
cause: the local `../koi` checkout (branch `dev`) advanced past the API zen's earlier SSOT migration
targeted. koi's **P08 "certmesh diet"** (`docs/prompts/P08-certmesh-diet.md` in koi) made breaking
changes — `refactor(certmesh)!: flatten trust profiles to two booleans` (`3ed2fec`), FIDO2 removed
(`unlock_with_fido2`/`add_fido2_slot`/`InputType::Fido2`), automatic failover shed (`ca_announcement`
gone), `configure_auto_unlock_for_profile`→`configure_auto_unlock`, `open_enrollment` lost its deadline
arg, `CreateCaRequest`/`CertmeshStatus` reshaped. zen compiled against koi tags **v0.4.0/v0.4.1** (which
predate the diet) but not `dev` (0.4.2, post-diet).

Operator decision: **migrate zen to the current `../koi` (0.4.2)** and keep consuming `../koi` directly
at whatever version is checked out (no pinning) — the dogfooding contract. Migration committed in
`aec0f024` (pond.rs, pond_lifecycle.rs, mdns.rs, run.rs, secrets.rs, pond.html, rake ceremony_render.rs,
Cargo.lock, CHANGELOG). Verified green + rust-reviewer pass.

**Behavior changes worth noting (from koi's feature removals, not zen's choice):**
- FIDO2 pond unlock is gone (passphrase + TOTP remain).
- Pond enrollment no longer auto-expires — `open_enrollment()` is a toggle; `pond invite --ttl` is
  accepted but ignored (warns). Operator must close enrollment explicitly. (No zen close-enrollment
  endpoint exists yet — a gap, out of migration scope.)

**Noted characteristic (rust-reviewer #3, MEDIUM, NOT fixed — faithful to koi's removed code):**
`mdns.rs build_ca_announcement` names the service `koi-ca-{roster_primary.hostname}` without verifying
the roster primary is *this* stone. This mirrors koi's removed `ca_announcement` exactly, and the gate
(`ca_initialized && !ca_locked`) already means this node holds the CA. A stale roster after a manual
promote could mislabel; revisit if promote-then-announce proves wrong in practice (would need the stone
name plumbed into mdns.rs).

**koi-ref implication for CI/release (open):** koi `dev` is a moving target (it oscillated 0.4.2↔0.5.0
this session). ci.yml `KOI_REF=""` tracks koi's default branch ("feed from whatever's available"), so
the gate will go red whenever koi makes a breaking change — that *is* the dogfooding signal, but it means
the PR's CI is only green while koi's default branch carries a zen-compatible certmesh API.
