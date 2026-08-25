# STONE-0001: LineageOS ARM64 phones as full Stones (native musl binary)

**Status**: Accepted
**Date**: 2026-06-07

## Context

Two proposals described running Zen Garden on a smartphone, with conflicting assumptions:

- [stone-phone-repurposing.md](../proposals/stone-phone-repurposing.md) allows a phone to be a
  **full Stone**, but assumes **PostmarketOS** (mainline Linux + systemd) and a
  natively-installed `garden-moss` binary managed by a systemd unit.
- [stone-pebble-android-tier.md](../proposals/stone-pebble-android-tier.md) assumes Android
  **cannot run real Docker** ("Docker ❌ proot only") and relegates phones to a degraded,
  sensor-only **Pebble** tier that polls Lantern and runs no containers.

Both assumptions were overtaken by the device. The Phone-to-Stone effort recompiled the
LineageOS `msm-4.9` kernel with the container primitives (`PID/IPC/USER_NS`, cgroups,
`BRIDGE_NETFILTER`, `OVERLAY_FS`, `VETH`, `NF_NAT`) plus USB-Ethernet drivers, and **native
`dockerd` runs on Android** — no proot, no PostmarketOS, no systemd. A rooted phone on
LineageOS (a maintained, wide-device-support ROM) can therefore be a real, discoverable Stone
running the standard `garden-moss`.

The target device is a Google Pixel 3 XL (SDM845, aarch64 / ARMv8.2-A, 8 cores, ~3.5 GB RAM,
~49 GB free on `/data`, Adreno 630 → CPU-only), LineageOS 22.2, kernel `4.9.337-kintsugi-docker+`,
root via Magisk. Its userland is Android/bionic — **no glibc, no systemd, read-only `/`**.

## Decision

1. **`garden-moss` runs natively on the host as a fully-static `aarch64-unknown-linux-musl`
   binary** — not in a container. Moss is the Stone's host management daemon (the installer
   deploys it as a `/usr/local/bin` binary + service, per
   [NewStone-linux-x64.ps1](../../installer/NewStone-linux-x64.ps1)); it coordinates Docker and
   owns host concerns, so it must run on the host. Android has no glibc, so the host binary is a
   static musl ELF. Built musl-natively in an Alpine arm64 container under QEMU
   ([Dockerfile.linux-arm64-musl](../../Dockerfile.linux-arm64-musl) +
   [compile-linux-arm64-musl.ps1](../../installer/compile-linux-arm64-musl.ps1)). The optional
   `udev` Cargo feature is **off** for this build (no libudev on Android; the storage monitor's
   polling fallback carries on).

2. **`aarch64-unknown-linux-gnu` (glibc) is also a first-class build target** for ARM64 **Linux**
   stones (Raspberry Pi and similar), via [Dockerfile.linux-arm64](../../Dockerfile.linux-arm64) +
   [compile-linux-arm64.ps1](../../installer/compile-linux-arm64.ps1) +
   [build-linux-arm64.ps1](../../installer/build-linux-arm64.ps1), mirroring the x64/x86 pipelines
   (cross-compile, DistConfig packaging). Those stones deploy the binary natively, same as x64.

3. **Docker is brought up on the device with a cgroup v1 view inside a private mount namespace.**
   The kernel is 4.9, which predates `BPF_CGROUP_DEVICE` (4.15+), so runc's cgroup **v2** device
   controller (eBPF) fails. dockerd/containerd/runc (static, runc pinned to 1.1.12) run under
   `unshare -m` where `/sys/fs/cgroup` is a tmpfs with `devices`+`freezer` mounted as fresh v1
   hierarchies and a tmpfs cpuset stand-in. All daemon paths live on `/data` (no `/var`, `/run`,
   `/etc` writable on Android). See [install-dockerd-android.ps1](../../installer/install-dockerd-android.ps1)
   and the porting log below.

4. **First-boot deploy is over ADB; boot persistence is two Magisk services** (no systemd):
   `dockerd.sh` (sets up `/run`+`/var` tmpfs, the cgroup v1 namespace, starts containerd+dockerd)
   and `garden-moss.sh` (waits for `docker info`, starts the native moss binary with `DOCKER_HOST`
   + `ZG_*` paths on `/data`). [install-dockerd-android.ps1](../../installer/install-dockerd-android.ps1)
   then [deploy-android.ps1](../../installer/deploy-android.ps1).

All changes are additive; x64/x86/Windows builds are untouched (the `udev` feature is default-on).

## Rationale

**Native binary, not a container (corrected mid-effort).** An earlier iteration containerized
moss because the static-musl build looked hard (`aws-lc-sys`/BoringSSL via the koi sibling repo;
`libudev`). But moss is the *host* management daemon — it coordinates Docker, manages storage,
hardware, and networking — so running it as a Docker workload is architecturally backwards
(degraded host access, and it is the thing that orchestrates the containers). The musl blockers
turned out tractable: `libudev` is now an optional feature (gated off; the monitor already had a
polling fallback), and `aws-lc-sys` builds fine for musl in an Alpine arm64 builder. The
*offerings* moss orchestrates (MongoDB, etc.) are the containers — not moss.

**Why a cgroup v1 namespace for Docker.** runc on cgroup v2 manages the device controller via
eBPF (`BPF_CGROUP_DEVICE`), which the 4.9 kernel lacks — every `docker run` (even `--privileged`)
fails. cgroup v1's devices controller uses allow/deny files (no eBPF) and works. Android's
`/sys/fs/cgroup` is v2 (used for its own management) and its v1 controllers are kernel-bound with
quirks (cpuset is `noprefix`), so the v1 view is built in a private mount namespace to avoid
disturbing Android. Trade-off: no per-container memory/cpu/blkio limits (offerings self-cap,
e.g. Mongo's `--wiredTigerCacheSizeGB`).

**ADB bootstrap, no systemd.** A fresh phone has no running Moss to accept the HTTP package
deploy, LineageOS ships no `sshd`, and the single USB-C port makes wired Ethernet and PC-`adb`
mutually exclusive. ADB is the bootstrap channel; `su -c` (Magisk) is the privileged path
(`adb root` is unavailable on the production build). Consistent with
[ARCH-0008](ARCH-0008-drop-systemd-sandbox.md), Moss does not need systemd; Magisk `service.d`
scripts are the boot hooks.

**Docker access is portable.** Moss talks to Docker through `bollard`
(`connect_with_socket_defaults()`, honoring `DOCKER_HOST`), not the CLI. The phone's dockerd
socket lives at `/data/docker/docker.sock` (no `/var/run`), and moss is launched with
`DOCKER_HOST=unix:///data/docker/docker.sock` — no code change.

## Consequences

- **No per-container resource limits on the phone** (cgroup v1 with only devices+freezer).
  Acceptable; offerings self-cap. Documented in the porting log.
- **Image pulls need DNS + connectivity.** Android has no `/etc/resolv.conf` and the phone has no
  LAN yet, so dockerd cannot resolve registries; load images from tar (`docker load`) until the
  USB-C Ethernet adapter is attached.
- **LAN discovery requires the wired adapter.** The container kernel skips the WiFi vendor module
  (ABI-broken by `BRIDGE_NETFILTER`), so there is no `wlan0`; multicast discovery needs `eth0`,
  and `adb` then moves to TCP (`adb tcpip 5555`).
- **Hardware profiling is already Android-safe** — Adreno reports CPU-only (no false GPU), the
  architecture is read from the build target, `/sys` reads are failure-tolerant. Discovery socket
  tuning (`SO_REUSEPORT`, multicast interface) is deferred until device evidence shows it is
  needed, not changed speculatively.
- **MongoDB orchestrator** builds for `linux/arm64` via
  [build-orchestrators.ps1](../../installer/build-orchestrators.ps1) `-Platform linux/arm64`
  (buildx + QEMU).
- **The Pebble tier is preserved** for **unrooted / sensor-only** phones (no container kernel,
  no root): those genuinely cannot run Docker and remain a distinct, pull-based node type. A
  rooted phone with a container kernel is a full Stone, not a Pebble.
- The Android porting mismatches and their fixes are tracked in
  [docs/notes/android-stone-porting-log.md](../notes/android-stone-porting-log.md); the two
  proposals point here as the authoritative path.
