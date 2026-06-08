# DEPLOY-0001 implementation brief (resume anchor)

> **Single-prompt resume:** "Continue DEPLOY-0001 from `docs/notes/deploy-0001-implementation-brief.md`."
> ADR (approved): `docs/decisions/DEPLOY-0001-unified-self-update.md`. This brief has everything
> needed to implement + test without re-deriving. Mission: make self-update ONE flow across
> Linux/Windows/Android (OS-specifics only), phone updates via standard HTTP. Then make cricket
> target-agnostic (`docs/notes/cricket-android-audio-research.md`).

## State at handoff (2026-06-07)
- Branch `fix/snapshot-scheduler-disposal`. The earlier phone-Stone + HOST-0001 + capability
  consolidation work is committed (latest: `cc4de0a3`). **No DEPLOY-0001 code written yet** —
  analysis + ADR only. Nothing on any stone changed since.
- The Pixel (`stone-slate-grove`) is a working Stone: native moss on `0.2.0.dev`, mongo healthy,
  `192.168.1.120`, adb-over-TCP persistent (`persist.adb.tcp.port=5555`).

## Access + safety nets
- **Phone:** `adb connect 192.168.1.120:5555` (persistent). Push: `adb -s 192.168.1.120:5555
  push ...`. su shell: `adb -s 192.168.1.120:5555 shell "su -c '...'"`. Binary at `/data/garden-moss`;
  Magisk launcher `/data/adb/service.d/garden-moss.sh`; data on `/data/zen-garden`. ADB is the
  bootstrap + recovery escape hatch.
- **Wyse (`stone-silent-cascade`, x86_64):** `plink -batch -ssh stone@stone-silent-cascade -pw
  stone "..."`; recover via `sudo systemctl restart garden-moss` + `journalctl -u garden-moss`.
- **Discovery:** `deploy.ps1` finds stones via UDP 7184. **Touch ONLY these two** — leave
  `stone-soft-shard` (.101) and `stone-gentle-cliff` (.222) alone. Target by endpoint, not broadcast.
- **Build (arm64-musl):** `messense/rust-musl-cross:aarch64-musl`, caches `zen-cargo-musl-cross` +
  `zen-target-musl-cross`, profile `fast-release`, `--no-default-features` (drops udev) for moss.
  Stage to `dist/linux-arm64-musl/garden-moss`; (cross-build cmd pattern in earlier commits).
- adb path = `C:\Users\onose\AppData\Local\Microsoft\WinGet\Packages\Google.PlatformTools_*\platform-tools\adb.exe`

## Implementation steps (file targets) — minimal, ordered
1. **arm64-musl package via the existing packager.** New `installer/build-android-arm64.ps1`
   (mirror `build-linux-arm64.ps1:111-118`): run `compile-linux-arm64-musl.ps1` then
   `New-PlatformPackage -Platform linux -Architecture arm64-musl -SourceDir dist/linux-arm64-musl`.
   Extend `compile-linux-arm64-musl.ps1:129` with `-Tier full` → also build garden-firefly
   (NOT cricket v1) WITHOUT moss's `--no-default-features`.
2. **Register musl** in `installer/dist.json` (staging + `android-arm64` block, `distDir`
   `../dist/linux-arm64-musl`) and a dispatch branch in `installer/build.ps1`.
3. **Profile-pathed, cross-platform apply.** `src/moss/src/infra/installer/pre_start.rs`: replace
   literal `/usr/local/bin` (`deploy_bin:148`, companions `157-160`) with
   `garden_common::host::profile().paths.bin_install` / `.companions`. Gate ONLY the
   `systemctl daemon-reload`/unit-regen bits behind `scheduler==Systemd` (Android skips). It already
   compiles on Android (target_os=linux); just remove the systemctl assumptions.
4. **Shared applier + self_replace.** New `src/moss/src/infra/installer/apply.rs`: ONE rename-aside
   `replace_file` (promote from `pre_start.rs:317`, delete dup `linux.rs::replace_binary:142`),
   KEEP `.old`. Add `self_replace` to `src/moss/Cargo.toml`; use `self_replace::self_replace()` for
   the `garden-moss` self-swap; shared helper for rake/lantern/companions.
5. **Exit-code contract.** Consts in `garden-common` (`EXIT_STOP=0, EXIT_RESTART_APPLY=10,
   EXIT_RESTART=11, EXIT_FATAL=1`). Thread a shutdown-reason into `bootstrap/server.rs` final
   `process::exit` (currently hardcoded 0 at ~:319). `api/v1/stone.rs deploy_stone_v1`: drop the
   in-band Windows sidecar branch — always stage+cancel, let the supervisor apply.
6. **Android watchdog.** Rewrite `installer/android/garden-moss-service.sh` from one-shot `nohup`
   into a respawn loop: `pre-start` then `garden-moss`, branch on exit code (0=break, 1=expo backoff
   capped ~60s, 10/11=loop+reapply), pidfile/flock single-instance, fork+wait (not pgrep). Install
   moss to a path consistent with `bin_install` (align `deploy-moss-native.sh:16`).
7. **Mark-good rollback.** Applier keeps `.old` (don't delete at `pre_start.rs:333`). Post-restart:
   on healthy `:7185`+`catalog_ready`, delete `.old` (commit); on repeated `EXIT_FATAL`, restore
   `.old`. Wire the dead `src/moss/src/infra/update_transaction.rs` OR a minimal
   `installed-version.json` pending/good breadcrumb.
8. **Crash-loop backoff.** Android watchdog (step 6). Linux `RestartSec=10` already backs off
   (optionally add `StartLimit*` to `installer/templates/garden-moss.service.template`).
9. **deploy.ps1 arch case.** Add arm64-musl/android case to the arch switch (`installer/deploy.ps1:413-419`)
   selecting the arm64-musl package. Handler `stone.rs:480` keeps `platform=="linux"` validation.

## Test sequence (low-risk first)
1. **Wyse first** (systemd + ssh recovery): build linux-x64 pkg from the branch, deploy via HTTP to
   `stone-silent-cascade` only, confirm it applies + restarts + `:7185` healthy.
2. **Phone:** one-time **adb** install of the new moss + the watchdog `service.d` script (bootstrap),
   then **prove HTTP deploy**: POST the arm64-musl package to `192.168.1.120:7185/api/v1/stone/deploy`
   → watchdog applies (`pre-start`) + respawns → `:7185` healthy + mongo intact. This is the
   acceptance: phone updates via HTTP, no adb.
3. Verify mark-good (deploy a good build → `.old` cleaned) and a rollback (deploy a deliberately
   bad/crashing build → reverts to `.old`).

## Key findings to remember (so we don't re-derive)
- Intake is ALREADY unified; only apply+restart forked. ARM64-gnu already packages via the standard
  path; only aarch64-MUSL was the outlier (raw binaries, no package.json, no companions).
- Field precedent: **Syncthing** does exactly stage→atomic-rename(temp same dir, rename live→.old,
  rename new→path, reverse on fail)→exit-with-code + a monitor that respawns; STNORESTART delegates
  to systemd/SCM. Validated our model. Full field survey: it's in this session's transcript +
  summarized in the ADR grounding.
- `self_replace` crate = the cross-platform running-binary swap (Unix ETXTBSY + Windows locked-exe);
  NOT yet a dep. `UpdateTransaction` exists but is DEAD (zero call sites) — reuse for rollback.
- Handler `deploy_stone_v1` shells to system `tar` (`stone.rs:386`), not the hardened
  `package.rs::extract_tar_gz` — latent risk, deferred hardening.
- Companions are passengers (no separate channel); firefly come-up = `companions.rs` scan/registry
  (`companion-ports.json` ledger 7187-7199), respawn after moss restart. `NewFirefly.ps1` is
  unrelated (flashes MCU firmware) — leave it.

## Decisions/defaults: see ADR "Decisions taken". Open questions there are all defaulted for v1.
