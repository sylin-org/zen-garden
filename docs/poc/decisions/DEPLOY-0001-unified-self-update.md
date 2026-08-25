# DEPLOY-0001 — Unified self-update via one flow + a per-platform supervisor

- **Status:** Accepted (2026-06-07)
- **Deciders:** Leonardo (maintainer), moss agent
- **Grounding:** field survey of comparable daemons (Syncthing, Caddy, MinIO, go-update,
  Tailscale/Vector/Telegraf, Mender/RAUC/balena, Squirrel/Sparkle, s6/runit) + a deep analysis
  of the current zen-garden build/deploy/apply/restart + companion lifecycle. See
  `docs/notes/deploy-0001-implementation-brief.md` for the full findings + step-by-step plan.

## Principles (the rubric, in priority order)
1. **One flow, same shape on every platform — OS-specifics only.** The update *process* does
   not fork per OS; it reads one declarative table (`HostProfile`).
2. **Minimalist over ceremony.** Reuse the native supervisor where one exists; add the smallest
   possible thing only where there's a real gap. No frameworks.
3. **Don't reinvent.** Adopt proven mechanisms (the `self_replace` crate; rename-aside swap;
   Syncthing's exit-code contract; embedded-style mark-good rollback).

## Context
Today the *intake* is already unified (`deploy.ps1 → POST /api/v1/stone/deploy → verify SHA →
stage`), but **apply + restart fork three ways**: Linux uses systemd (`ExecStartPre=garden-moss
pre-start` applies + `Restart=always` respawns); Windows uses an external updater .exe; **Android
has neither** (Magisk `service.d` runs once at boot and never respawns, nothing calls the apply
step, and the apply target is hardcoded to read-only `/usr/local/bin`). The field is unanimous:
the unit being replaced must never be the one that respawns it, and where no OS supervisor exists
you must supply one. **Android's missing supervisor is the whole divergence.**

## Decision
**Agnostic core in moss — identical on every platform:**
`receive → verify SHA → stage (temp, same filesystem) → apply (rename-aside) → exit(code)`,
then on next start a **mark-good** health check that reverts on failure.
- **Apply** writes to `HostProfile.paths.bin_install` / `.companions` (never literal `/usr/local/bin`).
  One shared rename-aside helper (`installer/apply.rs`, promoted from `pre_start.rs::replace_file`)
  for rake/lantern/companions; **`self_replace`** crate for moss's own binary (handles the running
  Unix inode + Windows locked-exe). Keep the `.old` backup for rollback.
- **Exit-code contract** (consts in `garden-common`): `0 EXIT_STOP` (don't respawn) · `10
  EXIT_RESTART_APPLY` (staged upgrade pending — apply then respawn) · `11 EXIT_RESTART` (respawn,
  apply is a no-op) · `1 EXIT_FATAL` (crash — respawn with backoff). `bootstrap/server.rs` picks
  the code at its final `process::exit`. The deploy handler ALWAYS just stages + cancels the token
  (no per-OS apply in-band).
- **Per-platform supervisor — the only OS-specific seam** (declared in `HostProfile.runtime`):

  | OS | Supervisor | Work |
  |----|-----------|------|
  | Linux | systemd (`Restart=always`, `ExecStartPre=garden-moss pre-start`) | keep as-is; later add `StartLimit*` |
  | Windows | SCM (`sc failure restart`) | generalize the finalize to the shared applier |
  | Android | **NEW tiny watchdog** in `service.d`: `while :; do garden-moss pre-start; garden-moss; case $? in 0)break;; 1)backoff;; *)loop;; esac; done` with a pidfile/flock + exponential backoff | the one new part |
- **Mark-good rollback (platform-agnostic):** new binary must serve `:7185` with `catalog_ready`
  within N launches or the supervisor reverts to `.old`. Wire up the currently-dead
  `UpdateTransaction` (or a minimal `installed-version.json` pending/good breadcrumb).
- **Packaging:** the aarch64-musl (Android) binary is packaged through the **existing**
  `New-PlatformPackage` (Platform=`linux`, Arch=`arm64-musl` — `package.json.platform=="linux"` is
  already accepted by the handler on the phone). `deploy.ps1` gets an arm64 arch case. Companions
  ride the package (passenger model); **firefly-only on arm64-musl for v1** (cricket pending the
  ALSA/bionic audit — see `docs/notes/cricket-android-audio-research.md`).

## Decisions taken (defaults, override later if needed)
- Keep `Platform='linux'/arch=arm64-musl` (zero ValidateSet churn). · firefly-only on musl v1. ·
  Exit codes `10/11`; keep systemd `Restart=always` (no unit change v1). · Minimal
  `installed-version.json` breadcrumb for rollback. · Phone's first hop = one-time **adb bootstrap**;
  all subsequent updates via **HTTP**. · Defer switching the handler from system `tar` to the
  hardened extractor (separate hardening).

## Consequences
- **+** One flow everywhere; the OS contributes only a path + a respawner. Fixes a latent Linux
  bug (install path `/usr/local/bin/companions` ≠ scan path `{data}/companions`). Android gains
  crash-recovery for free. Retires the M12/M14-class special-cases into the supervisor concept.
- **−** A hand-rolled Android watchdog is new surface (mitigated: tiny, backoff, flock, mark-good
  rollback, adb escape hatch). `self_replace` is a new dep. Windows updater path gets refactored
  (regression risk — keep SCM crash-net + `.old`).
- **Recovery nets:** phone = adb-over-TCP (`adb connect 192.168.1.120:5555`); wyse = `ssh
  stone@stone-silent-cascade` + systemd; `.old` rollback on both; atomic rename can't half-write.
