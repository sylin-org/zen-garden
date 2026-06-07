# Android Stone porting log (working journal)

> **Status: living working log — not a guide/spec.** Captures each mismatch between moss's
> Linux-host assumptions and the Android (LineageOS) runtime, with symptom → cause → resolution.
> Formalize confirmed entries into [STONE-0001](../decisions/STONE-0001-lineageos-arm64-full-stone.md),
> the [phone Stone guide](../guides/phone-stone-lineageos.md), and moss code as they settle.

Device: Pixel 3 XL (crosshatch, SDM845, aarch64), LineageOS 22.2, kernel `4.9.337-kintsugi-docker+`,
Magisk root, no systemd, bionic libc, 3.5 GB RAM, ~49 GB free on `/data`.

Legend: ✅ resolved · 🔄 in progress · ⏳ anticipated (not yet hit) · ❓ needs on-device data

---

## Build / toolchain

| # | Area | Symptom / assumption | Cause | Resolution | Status |
|---|------|----------------------|-------|------------|--------|
| B1 | Deployment model | Initial plan ran moss **in a container** | Misread: moss is the host management daemon (NewStone installs it as a native `/usr/local/bin` systemd binary), not a Docker workload | Pivot to native **static-musl** binary on the host; offerings (MongoDB) remain containers moss orchestrates | ✅ |
| B2 | libc | glibc `garden-moss` can't run on Android/bionic | No glibc on the host | Build `aarch64-unknown-linux-musl` (fully static); runs directly on the Android kernel | ✅ |
| B3 | C deps (musl) | `libudev` (udev crate) blocks a static-musl link | libudev is glibc-oriented; no musl static lib | Made `udev` an optional Cargo feature (default-on); musl build uses `--no-default-features`; storage monitor's existing **polling fallback** carries detection (no hot-plug storage on a phone anyway). No third-party shim. | ✅ |
| B4 | C deps (musl) | `aws-lc-sys` (BoringSSL) for musl | rustls default crypto provider; C+asm | Builds natively in an Alpine arm64 container under QEMU with `cmake` + `cc` | 🔄 (build running) |
| B5 | Builder | aarch64-musl cross-toolchain pain | — | Sidestepped: build musl-natively in `rust:alpine` arm64 under QEMU (`Dockerfile.linux-arm64-musl`) | ✅ |

## Runtime (host assumptions — anticipated, validate on-device)

| # | Area | Symptom / assumption | Likely cause | Planned resolution | Status |
|---|------|----------------------|--------------|--------------------|--------|
| R1 | Paths | moss defaults to `/var/lib/zen-garden`, `/etc/zen-garden` | Android has no writable `/var`; `/etc` is read-only (→ `/system/etc`) | Set `ZG_DATA_DIR=/data/zen-garden`, `ZG_CONFIG_DIR=/data/zen-garden/config` in the Magisk launcher | 🔄 |
| R2 | Init system | moss may shell `systemctl` (service install, restart docker, first-boot unit regen) | No systemd on Android | Expect graceful failure / no-op; if moss hard-fails, add Android detection to skip systemd ops | ⏳ |
| R3 | Docker socket | moss → bollard `connect_with_socket_defaults()` | Needs `/var/run/docker.sock` present + dockerd up | dockerd configured with default socket; launcher waits for `docker info` before starting moss | 🔄 |
| R4 | Networking | mDNS/discovery; no `wlan0` | WiFi vendor module skipped in the container kernel (BRIDGE_NETFILTER ABI break) | LAN via USB-C Ethernet (`eth0`); `adb tcpip 5555` after | ⏳ |
| R5 | Discovery sockets | multicast bind/announce on Android | `SO_REUSEPORT` / explicit multicast interface may be needed | Validate on-device first; change only if evidence shows it (no speculative edits) | ❓ |
| R6 | Hostname / MOTD | moss writes `/etc/motd`, sets hostname | `/etc` read-only; Android hostname differs | Expect no-op/failure; assess severity on-device | ⏳ |
| R7 | Tooling | verify scripts use `curl` | `curl` presence on LineageOS unconfirmed | Scripts try `curl` then `wget`; `netstat` (toybox) for listen check | ❓ |
| R8 | Hardware profile | CPU/RAM/disk/GPU detection | `/sys`,`/proc` reads (guarded); Adreno → CPU-only | Recon says already Android-safe; confirm reported specs on-device | ⏳ |

## dockerd (Stage 3a)

| # | Area | Symptom / assumption | Cause | Planned resolution | Status |
|---|------|----------------------|-------|--------------------|--------|
| D1 | cgroups | Docker expects controllers under `/sys/fs/cgroup/<ctrl>` | Android hybrid: cgroup v2 unified (only `pids` delegated) + v1 controllers at `/dev/memcg`,`/dev/cpuctl`,… | dockerd runs via `cgroupfs`; warns "No memory/cpu/cpuset limit support" — containers run unconstrained (acceptable; offerings self-cap, e.g. Mongo WiredTiger) | ✅ |
| D2 | runtime | newer runc/containerd hang on this kernel | openat2/cgroup features | Static bundle auto-detected (Docker 29.5.3); **runc overridden to 1.1.12**; daemon reports `runtime=runc` | ✅ |
| D3 | networking | bridge/iptables NAT | `iptables=false` model; host-networked offerings | `daemon.json`: `iptables=false`, `bridge=none`; offerings use `--network host` (container networking not yet exercised) | 🔄 |
| D4 | persistence | no systemd to start dockerd at boot | Android init | Magisk `service.d/dockerd.sh` boot launcher (reboot not yet tested) | 🔄 |
| D5 | config path | install aborted at `mkdir /etc/docker: Read-only file system` (so dockerd never installed → `exec dockerd: No such file`) | Android `/etc` is read-only (symlink into `/system`); `set -e` then aborted the whole install | `daemon.json` → `/data/docker/daemon.json` (`dockerd --config-file`); dropped the `/etc` mkdir; switched extract to `gzip \| tar` (toybox `tar -z` unreliable) | ✅ |
| D6 | socket | dockerd default socket `/var/run/docker.sock` unusable | Android has no `/var/run` | socket → `unix:///data/docker/docker.sock` (daemon.json `hosts`); moss + docker CLI reach it via `DOCKER_HOST` | ✅ |
| D7 | exec-root / tmp | dockerd exits: `mkdir /var: read-only file system` | exec-root (`/var/run/docker`) + pidfile (`/var/run/docker.pid`) default under `/var`; no `/tmp` either | daemon.json `exec-root=/data/docker/exec`, `pidfile=/data/docker/docker.pid`; `TMPDIR=/data/local/tmp` in the launcher | ✅ |
| D8 | containerd state | managed containerd exits: `mkdir /run: read-only file system` | containerd defaults `root=/var/lib/containerd`, `state=/run/containerd`; dockerd doesn't expose containerd's state path | Run **containerd standalone** with `/data/docker/containerd.toml` (root/state/grpc under `/data`); dockerd started with `--containerd <data sock>` | ✅ |
| D9 | /run, /var | dockerd exits: `mkdir /run/docker/plugins: mkdir /run: read-only file system` (several `/run` paths hardcoded, independent of exec-root) | `/` is read-only ext4; Android has no `/run` or `/var` | Boot service creates the `/run` + `/var` mountpoints via a brief `mount -o remount,rw /` (Magisk; verity off → persists) and backs them with **tmpfs**. Non-invasive (no partition content changed); idempotent. | ✅ |
| D10 | daemon DNS | `docker run hello-world` pull fails: `lookup registry-1.docker.io on [::1]:53: connection refused` | No `/etc/resolv.conf` on Android → dockerd's Go resolver falls back to localhost:53 (nothing there); also no LAN/internet yet | **Open.** Image *pulls* need a resolver + connectivity. Until the USB-Ethernet adapter is attached, load images from tar (`docker load`); then provide a resolv.conf / DNS for pulls | ❓ |

| D11 | cgroup v2 devices | container create fails: `bpf_prog_query(BPF_CGROUP_DEVICE) failed: invalid argument` (also with `--privileged` / `--device-cgroup-rule`) | runc's cgroup **v2** device controller uses eBPF; `BPF_CGROUP_DEVICE` landed in kernel **4.15**, but this kernel is **4.9** | Give docker a **cgroup v1** view inside a private mount ns (`unshare -m`): shadow `/sys/fs/cgroup` with tmpfs, mount `devices`+`freezer` as fresh v1 hierarchies. v1 devices uses allow/deny files (no eBPF). | ✅ |
| D12 | cpuset noprefix | `open /sys/fs/cgroup/cpuset/docker/cpuset.cpus: no such file` then `mkdir .../cpuset/docker/<id>: no such file` | runc requires a cpuset cgroup; Android's cpuset is kernel-bound to a `noprefix` v1 hierarchy (`/dev/cpuset`, files `cpus`/`mems` not `cpuset.cpus`) — can't re-mount, can't read | Unmount Android's `/dev/*` controllers in the ns; provide a **tmpfs stand-in** at `/sys/fs/cgroup/cpuset` with `cpuset.cpus`/`cpuset.mems` files (no enforcement, runc satisfied) | ✅ |
| D13 | mount propagation | namespace `tmpfs`/cgroup mounts **leaked back to host** and **stacked** across runs (3 tmpfs layers on host `/sys/fs/cgroup`); also shadowed the cpuset stand-in → D12 kept failing | toybox `mount` can't set propagation; `mount --make-rprivate /` silently failed (rc=1) so ns mounts stayed `shared` | Use the **magisk busybox** `mount --make-rprivate /` (rc=0) before any ns mount; busybox for all ns mount/umount. Peeled the leaked host layers. | ✅ |

> **2026-06-07 — Docker fully works on the phone.** dockerd 29.5.3 (cgroup **v1** in a private mount ns) + containerd + runc 1.1.12. A container runs end-to-end: `docker run` of an arm64 image prints `CONTAINER_EXEC_OK / arch=aarch64 / Debian 12`. Image *pulls* still need DNS/LAN (D10) — load from tar until the Ethernet adapter is attached. Net cgroup trade-off: no per-container memory/cpu/blkio limits (offerings self-cap).

## moss daemon (observed on device)

Build profile note: rust-musl-cross (`messense/rust-musl-cross:aarch64-musl`, rustc 1.95) cross-compiles
moss in ~6–15 min vs ~146 min under QEMU — use it for iteration.

| # | Area | Symptom | Cause | Resolution | Status |
|---|------|---------|-------|------------|--------|
| M1 | Docker socket | moss: `Socket not found: /var/run/docker.sock` (ignores DOCKER_HOST) | bollard `connect_with_socket_defaults()` hardcodes `/var/run/docker.sock`; does **not** read DOCKER_HOST (the recon was wrong) | Symlink `/var/run/docker.sock → /data/docker/docker.sock` (tmpfs /var/run) in the boot service. No moss change. | ✅ |
| M2 | TLS roots (startup) | **panic** `stone_client.rs:67` "No CA certificates were loaded from the system" → moss exits | reqwest 0.13 + `rustls` feature defaults to `rustls-platform-verifier` (OS trust store); none on Android | moss `http.rs`: `client_builder()` uses `use_preconfigured_tls()` with a rustls config built from bundled `webpki-roots` (+ explicit aws-lc-rs provider). stone_client + shared HTTP/COMPANION clients route through it. | ✅ (in fast-release) |
| M3 | TLS roots (detection) | background task **panic** `detection/pipeline.rs:63` (same CA error); supervisor caught it (non-fatal) | same; garden-common detection clients use default builder | `danger_accept_invalid_certs(true)` on the two detection probe clients (local liveness probing) — no new deps. | ✅ (in fast-release) |
| M4 | env var name | caches/companions wrote to read-only `/var/lib/zen-garden`; disk metric wrong | launcher set `ZG_DATA_DIR`, but `constants/paths.rs` reads **`GARDEN_DATA_DIR`** | Launcher exports `GARDEN_DATA_DIR` / `GARDEN_CONFIG_DIR` / `GARDEN_COMPANIONS_DIR` = `/data/...` (keeps ZG_ aliases). No moss change. | ✅ |
| M5 | hardcoded CONFIG_DIR | catalog/capabilities/task caches still failed read-only after M4 | several sites use the compile-time const `constants::CONFIG_DIR` (= `/etc/zen-garden`), not the env-honoring `config_dir()` | Bind `/etc/zen-garden → /data/zen-garden/config` in the boot service (note: `/etc` is a symlink to `/system/etc`; the bind lands there). Proper fix (code: use `config_dir()`) deferred. | ✅ (bind) / 🔄 (code) |
| M6 | CPU architecture | `cpu.architecture = "Google Inc. MSM sdm845 C1 DVT1.1\0"` (device-tree model + NUL), not `aarch64` → **offering compatibility filters everything → empty catalog** | `system.rs` get_cpu_info **overwrites** the canonical `std::env::consts::ARCH` with `/proc/device-tree/model` on ARM | Keep `architecture` canonical; use device-tree model for the *model name* (NUL-stripped). Fixes ARM/Pi stones too. | ✅ (in fast-release) |
| M7 | embedded manifests | catalog `total_offerings: 0` even with `catalog_ready: true`; `wiredTiger`/`container-registry` absent from binary | **rust-embed reads from the filesystem at runtime in debug builds** (compile-time path, absent on the phone); only embeds in **release** | Build **fast-release** (rust-embed embeds the manifests). | 🔄 (fast-release building) |
| M8 | /etc writes | `Failed to write /etc/hostname` (first-boot), `/etc/motd` | read-only `/etc`; first-boot/MOTD write there | Non-fatal (logged; moss continues). Proper fix (skip on Android / tolerate) deferred. | ⏳ |
| M9 | discovery | `No eligible network interfaces`, `discovery will fail` | phone has only loopback + cellular rmnet (no eth0/wlan0) | Non-fatal (background task, caught). Pending USB-Ethernet adapter. | ⏳ |
| M-pull | offline image pull | plant failed: `Failed to pull image 'mongo:7' … lookup registry-1.docker.io … connection refused` even though the image was pre-loaded | moss always pulled before create; the phone has no DNS/registry route | `pull_image` now falls back to a locally-present image when the pull fails (`exec.rs`) — air-gapped stones load images via `docker load`. | ✅ |
| M10 | offering can't open sockets | planted MongoDB crash-loops: mongod `socket() failed: Permission denied` + `open: Permission denied` → `exitCode:100` | **Android paranoid-network** (`CONFIG_ANDROID_PARANOID_NETWORK`): kernel restricts `AF_INET` socket creation to root / the `inet` group (gid 3003). mongo's entrypoint `gosu`-drops root→`mongodb` (uid 999), losing privilege. Ruled out empirically: SELinux (container domain permissive, **no AVC** for mongod), seccomp (`seccomp=unconfined` no change), volume perms (chown 999 no change), `--group-add 3003` (dropped by gosu). | **Run the offering as root** — `docker run --user 0 --entrypoint mongod mongo:7 …` → mongod `Waiting for connections, port 27017` ✅. moss-integration (run paranoid-net offering containers as root, or build a kernel without paranoid-network) is the open design choice. | 🔄 (proven manually; moss hook TBD) |

> **2026-06-07 — MongoDB orchestrated + proven on the phone.** moss compiles a 51-offering catalog, resolves `mongodb → mongo:7` (`compatibility: pass` after scoping the x86 AVX/SSE rules to x86_64), and creates `zen-offering-mongodb` (mongo:7, correct `/data/db` bind, mongod.conf, healthcheck, restart policy) from the **locally-loaded** image (M-pull). mongod runs healthy when launched as root (M10). End-to-end validated: **phone → dockerd → moss-orchestrated MongoDB**.

> **2026-06-07 — moss runs natively on the phone.** static-musl `garden-moss` binds `0.0.0.0:7185`, `/health` responds, **Docker connected** (`docker: healthy`), hardware profiled (8 cores, SDM845, 3.5 GB, aarch64). Acceptance #1/#2 met. Catalog/MongoDB pending the fast-release rebuild (M6/M7).

## HostProfile centralization (HOST-0001)

The port surfaced ~10 host assumptions read inconsistently across the code. Rather than keep
patching, a single typed `garden_common::host::HostProfile` (loaded once into `host::profile()`)
now owns them; common + moss read from it. Full audit + design: `docs/notes/host-profile-audit.md`.

Migrated (each verified by cross `cargo check` + on-device): `paths.rs` config/data/companions/
network-state + the `CONFIG_DIR`-const callers (**M5 now code-fixed — the `/etc` bind is no longer
required**); `docker/mod.rs` socket via `runtime.docker_socket` / `connect_with_defaults` (honors
`DOCKER_HOST` — supersedes the M1 symlink); `tty.rs` identity hostname/hosts/motd → `WritePolicy`
(**M8 now code-fixed**); bare reqwest clients (registry/s3-proxy/snapshot/capability/lantern) →
`http::client_builder()` (closes the M2/M3 CA-cert gap for *all* clients); `/proc/cpuinfo` read
made non-fatal.

| # | Area | Symptom | Cause | Resolution | Status |
|---|------|---------|-------|------------|--------|
| M11 | first-boot avahi loop | `First boot retry failed: Failed to restart avahi-daemon` every 3s | `restart_avahi` shells `systemctl restart avahi-daemon`; no systemd/avahi on Android → the step errors → the whole first-boot retries forever (exposed once M8 let first-boot progress) | `restart_avahi` gated on `runtime.scheduler == Systemd`; first-boot wraps avahi + mDNS-test as best-effort (Moss runs its own mDNS) | ✅ |
| M12 | exit after first-boot | first-boot completes, then moss **exits** (~2s after READY) and stays down | `start_first_boot_task` calls `std::process::exit(0)` "so systemd restarts us"; Android has no supervisor → stays down (exposed once M8/M11 let first-boot complete) | `finish_first_boot` gates the restart-exit on `scheduler == Systemd`; on non-systemd hosts moss keeps running (new identity applies next boot) | ✅ |

> **2026-06-07 — HostProfile refactor validated on the phone.** Refactored `garden-moss` (fast-release)
> deployed: stays up on `0.0.0.0:7185`, `catalog_ready: true`, `docker: healthy`, caches + config on
> `/data` via the profile (**no `/etc` bind needed**), `.first-run-complete` written (first-boot
> completes), **no read-only errors / panics / retry loops**. Remaining log lines are expected
> no-LAN WARNs (p2p, IP detection, lsblk).

> **2026-06-07 — LIVE on the LAN. The phone is a full Stone in the garden.** With a USB-C
> Ethernet adapter attached (hot-plugged *after* moss started — moss detected the new interface
> on its own via the route-table/p2p path), the phone came up as **`stone-slate-grove` @
> `192.168.1.120`** (first-boot auto-named it), announcing its **real LAN IP** (not 127.0.0.1) +
> adapter MAC, health *thriving*. **Mutual mDNS discovery confirmed:** the phone discovered both
> peers (`stone-soft-shard` .101, `stone-gentle-cliff` .222) and a peer's garden topology lists
> the phone back with correct specs (`aarch64`, 8 cores) and its `mongodb` service. Acceptance
> **#2 (:7185/health), #3 (discovered with correct specs), #5 (garden inspection)** — green, over
> real Ethernet, driven entirely via LAN HTTP (no adb; the hub holds the USB-C port). #4 MongoDB
> is orchestrated + visible as a service (shown `stopped` — the M10 paranoid-network mongod issue,
> resolved by the kernel drop). Notes: the hub is bus-powered (drains the battery — needs a PD
> brick on its input for a sustained Stone); regain adb via USB → `adb tcpip 5555` →
> `adb connect 192.168.1.120:5555`.

> **2026-06-07 — #4 fully closed: MongoDB runs AND is reachable garden-wide.** The
> Phone-to-Stone agent rebuilt the kintsugi kernel with `# CONFIG_ANDROID_PARANOID_NETWORK
> is not set` → `mongod` now runs as the non-root `mongodb` user (sockets work). It bound
> `0.0.0.0:27017` *inside* the container, but the published port was unreachable from the LAN.
>
> **Root cause (M13): bridge + `-p` publishing does not work on this Android host.** Enabling
> `iptables:true` + the default `docker0` brought up the bridge, the `nat DOCKER` chain, and the
> `DNAT` rule correctly (the kernel has `CONFIG_BRIDGE`/`VETH`/`NF_NAT`/`BRIDGE_NETFILTER`, all
> verified) — yet the **host itself could not route to a `docker0` container** (`host → 172.17.0.2`
> times out; docker-proxy can't reach the container either). This is Android's **per-network
> policy routing**: host-originated packets are steered through per-network routing tables that
> have no `docker0` route, so they never reach the bridge. `bridge-nf-call-iptables=0` did not
> help. mongod's bind was never the problem (confirmed `00000000:6989` = `0.0.0.0` listening).
>
> **Resolution: host networking for offerings** (which was `install-dockerd.sh`'s original
> intent — "offerings use `--network host`"; the real bug was moss creating bridge+`-p`
> containers). Baked into `HostProfile`: `Container::from_env` now defaults
> `network_mode = Host` on `Platform::Android` (conventional hosts keep `Bridge` + `-p`). Each
> offering binds its ports **directly on the host stack** → reachable via the default-ACCEPT
> INPUT chain. Fire-and-forget: every present and future offering port lands on the host, no
> per-offering config. `daemon.json` reverted to the minimal `iptables:false`/`bridge:none`.
>
> **Verified (clean restart, no env, code default):** `docker inspect → Net=host`,
> host `LISTEN 0.0.0.0:27017`, and from a separate LAN PC `GET http://192.168.1.120:27017` →
> `HTTP 200` mongod native-port notice. Peer `stone-gentle-cliff`'s garden topology:
> `stone-slate-grove @ 192.168.1.120 health=thriving services=mongodb:running`. The phone is a
> full Stone running a LAN-reachable MongoDB, and it survives restart via the code default.
