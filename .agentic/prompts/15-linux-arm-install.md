# 15 — The Linux/ARM Install Path

> A Linux user with an old laptop — or a Pi, or a phone-adjacent ARM board — gets from curl to a
> discoverable stone without touching Windows. Phase: Product. Depends on: 02 (release artifacts).
> Strategy opportunity #1 (the Windows-10 exit wave needs an install path that isn't Windows-authored;
> the docs recommend Pi hardware to an audience with zero supported route onto it).

## Mission

Today every install road runs through the author's Windows machine: USB imaging is Windows-only
PowerShell, `install.sh` rejects non-x86, and the self-installer's platform detection returns "unknown"
for aarch64. Meanwhile the **Rust self-installer inside the moss binary** (BUILD-0003,
`src/moss/src/infra/installer/`, ~3.6k lines — fresh install/update/repair, Docker+avahi provisioning,
systemd registration, health verification) is the project's most product-shaped asset and already does
the hard part. Close the gap: aarch64 support in platform detection and packaging, a hardened
`install.sh` that covers x64 + arm64 (+ arm64-musl for Android-adjacent), and a tested
existing-Linux-box onboarding that the prompt-06 quickstart can point at with a straight face.

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| `installer/install.sh` (108 lines) fetches `releases/latest` and rejects non-x86 (~17-26) | `sed -n '1,70p' installer/install.sh` |
| `installer/package.rs` (or the packaging module) `platform_id()` returns "unknown" for aarch64 (~28-36) | `grep -rn "platform_id\|aarch64" installer/ src/moss/src/infra/installer --include="*.rs"` |
| Cross-compile Dockerfiles for linux-arm64 and linux-arm64-musl exist at repo root and are exercised by the PowerShell pipeline | `ls Dockerfile.linux-arm64*` |
| The self-installer provisions Docker + avahi-daemon + stone user via apt and registers systemd units — Debian-family assumptions | `grep -rn "apt\|systemd" src/moss/src/infra/installer --include="*.rs" \| head -10` |
| Release workflow (prompt 02) publishes per-target binaries; verify arm64 targets are in its matrix — if not, adding them is in-scope here | read `.github/workflows/release.yml` |
| DEPLOY-0001 (unified self-update with rollback) is the update path post-install; the installer only needs to get a stone to "enrolled + self-updating" | `ls docs/decisions/DEPLOY-0001*` |
| An Android/Termux-or-Magisk stone path exists separately (`docs/guides/phone-stone-lineageos.md`) — out of scope here, but arm64-musl artifacts serve it | — |

## Research first (~60 min)

1. Read `installer/install.sh` and the self-installer module fully — the script's only real jobs are:
   detect platform → download the right moss binary → exec `moss install` (the binary does the rest).
   Confirm that's the actual contract; if install.sh duplicates provisioning logic, the fix is to thin it.
2. Read the packaging/platform-id code paths end to end (build-time naming ↔ install-time detection must
   agree on target triple names).
3. Enumerate the OS matrix honestly: Debian 12/13 and Ubuntu LTS (apt assumptions hold), Raspberry Pi OS
   (Debian-derived, arm64), Fedora/Arch (apt assumptions BREAK — decide: explicit unsupported-with-message
   vs package-manager abstraction; recommend the former now, honest error + FINDINGS.md).
4. Check Docker-on-arm64 install path (get.docker.com handles it; verify the self-installer uses it or
   apt's docker.io).

## Plan gate — OPERATOR decisions

1. Distro support statement for v0.x: recommend "Debian-family (Debian 12+, Ubuntu 22.04+, RPi OS) on
   x64/arm64; everything else gets a clear *not yet* with the manual steps documented". Confirm.
2. Whether arm64-musl artifacts ship in the same release lane (recommend yes — phone stones consume them).
3. Pi-specific touches (recommend none beyond arm64 — no GPIO/boot-config magic; a Pi is just an arm64
   Debian box here; FINDINGS.md any Pi-specific friction discovered while testing).

## Target shape

```
# the one-liner the README/first-stone.md publishes (post-release):
curl -fsSL https://github.com/sylin-org/zen-garden/releases/latest/download/install.sh | sh

$ # on an arm64 Pi or an x64 ThinkPad alike:
  detecting platform… linux-arm64
  downloading garden-moss 0.1.0+abc1234… ok (sha256 verified)
  → garden-moss install
    docker: present (27.x) | avahi: installing… ok | stone user: created
    systemd: garden-moss.service enabled + started
  ✓ stone 'gentle-meadow' is alive — from another machine: garden-rake observe
```

install.sh requirements: POSIX sh (not bash-only), curl-or-wget tolerant, sha256 verification of the
downloaded binary against the release's checksums file, idempotent re-run (delegates to the
self-installer's repair mode), readable error when unsupported (distro/arch) with a link to manual steps.

## Implementation

1. Fix `platform_id()`/packaging for aarch64 (+ musl variant naming); align names with the Dockerfile
   targets and the release workflow's asset names.
2. Ensure the release matrix builds and uploads: linux-x64, linux-arm64, linux-arm64-musl (+ checksums
   file). Extend prompt 02's workflow if missing.
3. Rewrite `install.sh` to the target shape (it stays ≤150 lines — the Rust installer owns complexity).
4. Self-installer: verify/extend arm64 paths (avahi/docker provisioning identical; any x86-only
   assumptions — grep for `x86_64` literals).
5. Test matrix, honestly executed and transcribed: (a) x64 Debian VM or container — full path; (b) arm64
   — real Pi/ARM board if available, else qemu/binfmt container for install-logic (note the limitation:
   systemd-in-container caveats; document what was and wasn't verifiable). An Android/musl smoke is a
   bonus, not a gate.
6. Update first-stone.md's path-A section (coordinate with prompt 06's structure) + the distro support
   statement; CHANGELOG entry.
7. Commits: `feat(install): aarch64 platform support`, `ci(release): arm64 + musl artifacts`,
   `feat(install): hardened multi-arch install.sh`, `docs: linux/arm install path`.

## Definition of done

- [ ] `sh install.sh` transcript on x64 Debian-family: curl → verified download → installed → service
      active → `garden-rake observe` from another host sees the stone.
- [ ] arm64 transcript (real hardware or documented-container-best-effort) of at least: platform
      detection, binary selection, self-installer provisioning steps.
- [ ] `install.sh` passes `shellcheck` clean; unsupported-distro path prints the honest message
      (transcript).
- [ ] Release workflow builds all three Linux targets + checksums (local dry-run of the build steps or
      workflow-syntax validation + per-target `cargo build --target` proof).
- [ ] `platform_id()` unit tests cover x86_64/aarch64/musl mappings.
- [ ] Docs updated; no Windows-only step remains in the Linux path.

## Out of scope

NewStone USB imaging changes (Windows-authored; stays path B). Android/Magisk flow (existing guide).
Non-Debian package-manager support (honest error now). macOS. Self-update mechanics (DEPLOY-0001 owns
them — verify they don't break, don't extend them).
