---
audience: developer
doc_type: decision
status: accepted
---

# HOST-0001: Typed Host / Runtime Profile

**Date**: 2026-06-07
**Status**: Accepted

---

## Problem

Porting `garden-moss` to a rooted LineageOS phone (read-only `/` and `/etc`,
`/etc` symlinked into `/system`, no systemd, bionic libc, no system CA store, a
kernel with `CONFIG_ANDROID_PARANOID_NETWORK`, only `/data` writable) surfaced
~10 places where Moss assumes a conventional Linux host. The root cause was not a
missing facility — Moss already had env-backed `constants::paths::*`, a
host-trust-store-independent `http::client_builder()`, and an `EnvConfig` registry
— but these were **read inconsistently**: compile-time consts
(`constants::CONFIG_DIR = /etc/zen-garden`), scattered `env::var()`, and bare
`reqwest::Client::builder()` constructions bypassed them. Each new host had to be
*patched* rather than *configured*.

Representative breakages (full journal: [android-stone-porting-log.md](../notes/android-stone-porting-log.md)):
`bollard::connect_with_socket_defaults()` ignores `DOCKER_HOST`; `CONFIG_DIR`-const
callers wrote to read-only `/etc`; identity files (`/etc/hostname`, `/etc/hosts`,
`/etc/motd`) failed and looped first-boot; `reqwest` defaulted to
`rustls-platform-verifier` and panicked with no CA store; the CPU `architecture`
field was overwritten by `/proc/device-tree/model`; first-boot called
`std::process::exit(0)` "so systemd restarts us" and stayed down where there is no
systemd.

## Decision

Introduce one typed value object, **`garden_common::host::HostProfile`**, that
resolves every host assumption **once** at startup and is read everywhere through a
shared accessor — *"one thing reads the config; everybody reads from the thing."*

- **Home: `garden-common`** (not moss). Several consumers (`constants::paths`,
  `console::tty`, `resources::system`) live in common and cannot depend on moss.
- **Shape: a namespaced value object** (code-standards §1) — `paths`, `runtime`,
  `identity`, `network`, `tls`, plus a resolved `platform`. Multi-state knobs are
  enums, never bool pairs (§8): `WritePolicy`, `ImagePullPolicy`, `Scheduler`,
  `PrivilegeMode`, `ContainerPrivilege`, `NetConfigMethod`, `DnsProvisioning`,
  `TlsRootSource`.
- **One reader: `HostProfile::from_env()`** resolves env (`ZG_*` primary,
  `GARDEN_*` deprecated fallback with a one-time warning) + per-platform defaults
  (`LinuxStandard` / `Android` / `Minimal`, auto-detected), cached in a `OnceLock`
  exposed by `host::profile() -> Arc<HostProfile>`.
- **Windows is unaffected** — the profile is the *unix* single-source; `paths.rs`
  and `network/state.rs` keep their `cfg(windows)` branches.

## Implementation (this change)

Migrated, each verified by cross-`cargo check` and on-device:

| Domain | Sites | Effect |
|---|---|---|
| paths | `config_dir`/`data_dir`/`companions_dir`/network-state + the `CONFIG_DIR`-const callers (persistence, hardware, task_store, nurturing, MossConfig, console) | writes land on `/data` on Android; the runtime `/etc` bind is no longer needed; the `/usr/local/bin` companions fallback bug is fixed |
| runtime | `docker/mod.rs` socket (`runtime.docker_socket` / `connect_with_defaults`, honoring `DOCKER_HOST`); `exec.rs` `image_pull_policy` (Always/IfNotPresent/Never + offline fallback); `lifecycle.rs` container posture (`ContainerPrivilege`, user, network mode, bind address, restart policy — defaults preserve prior behavior) | the `/var/run/docker.sock` symlink workaround is superseded |
| identity | `tty.rs` hostname/hosts/motd → `WritePolicy` (Skip on read-only `/etc`) + Linux `hostname`-command read fallback | first-boot no longer fails on read-only `/etc` |
| tls | bare `reqwest` clients (registry, S3-proxy, snapshot-stream, capability-check, lantern registration) → `http::client_builder()` (bundled webpki roots) | the CA-cert panic is closed for **all** Moss clients |
| network | `detect_primary_interface` → `/proc/net/route` default-route detection + `network.interface` override | a USB Ethernet adapter is found by any name (eth0/usb0/enx…) |

Bugs fixed in passing: fatal `/proc/cpuinfo` read → degrade; `restart_avahi` gated
on `scheduler == Systemd` + first-boot mDNS steps best-effort (no retry loop);
first-boot restart-exit gated on `scheduler` (`finish_first_boot` — keep running
where there is no supervisor); `lsblk` media-scan failure → debug, not a 10-second
`warn` spam; `atomic_write` temp name `{name}.{pid}.tmp` (no same-stem collision).

## Consequences

- A new constrained host is brought up by setting a profile / a few `ZG_*` vars (or
  relying on auto-detected defaults), not by editing call sites.
- Config becomes **read-once / immutable** for the process lifetime (the intended
  centralization). `paths::*` change from per-call env reads to a cached value;
  tests that need to vary env set it before first access.
- The Android paranoid-network container problem is handled at the **kernel** layer
  (build without `CONFIG_ANDROID_PARANOID_NETWORK`); `ContainerPrivilege` defaults
  to `ImageDefault`, with `AmbientNetRaw` available as an env override for
  un-patched kernels.

## Remaining work

A multi-agent implementation review (2026-06-07) confirmed the design is sound and
the migrated sites preserve behavior, and enumerated the host assumptions still
un-migrated. **None break LinuxStandard or the discovery path**; each is an
Android/Minimal failure where `scheduler=None` / `privilege_escalation∈{Direct,None}`.
Apply the gate **at the call site** (validate-at-boundary, §18); the canonical idiom
is `console::restart_avahi` (`tty.rs`).

**P0 — privilege escalation (`sudo` fails on `Direct`/`None`):**
- Storage: `api/v1/storage.rs` `sudo rm`/`mkdir`/`chown`/`mount` (:362,:1245,:1284,:1396);
  `infra/storage/platform.rs` (:709,:713); `infra/storage/adapter.rs:224`;
  `infra/storage/connectivity/recovery.rs:281`. Route through a
  `runtime.privilege_escalation` helper (`Sudo`→prefix when not root, `Direct`→bare, `None`→error).
- Network static-IP: `infra/network/linux.rs` `sudo` (:116,:144,:162,:211,…) — covered
  wholesale by gating the provisioning entry on `network.config_method == None`.

**P1 — scheduler / systemd (`systemctl` + hardcoded `/etc/systemd` on non-systemd):**
- `common/src/infra/timer.rs` `systemctl` (:159,:180,:222,:240) + `/etc/systemd/system`
  writes (:331,:337) — gate on `Scheduler::Systemd`.
- `bootstrap/run.rs` `configure_resolved_for_containers` (:1186–1219, call site :712) —
  gate on `network.dns_provisioning == SystemdResolved`.
- `infra/docker_config.rs:190` `systemctl restart docker`; `infra/installer/linux.rs`
  daemon-reload/enable (:175,:182,:244,:252).

**P2 — lifecycle (graceful-degrade):**
- `api/v1/admin.rs:284,:344` poweroff/reboot; `api/v1/updates.rs:644` reboot;
  `tasks/job_executors.rs:582` hoist the `NetConfigMethod::None` check to the call site.

**Nits / later:** wire `tls.extra_ca_bundle` into `Merged`; route the legacy
`GARDEN_companions_dir` through `env_var()` (M5); decide `host::init()` (wire it in
bootstrap or delete it — the lazy `OnceLock` path is already correct); optionally expose
`Arc<HostProfile>` on `Current` via `FromRef`; consolidate `Environment.os` into the profile.

## Alternatives considered

- **Home in moss on `Current` (the original audit proposal).** Rejected: common
  consumers (`tty.rs`, `paths.rs`, `system.rs`) cannot read it.
- **Per-call env reads everywhere (status quo).** Rejected: that *is* the defect —
  inconsistent readers, no single source, no typed defaults.
- **Patch each host.** Rejected: the explicit non-goal — configure, don't patch.
