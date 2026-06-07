# Phone Stone (LineageOS, ARM64)

A rooted Android phone running **LineageOS** with a **container-enabled kernel** runs as a
full Zen Garden Stone: `garden-moss` orchestrates the phone's native Docker, offers
services (MongoDB, etc.), and joins the garden on `:7185` beside x86 Stones.

This guide covers building, deploying, and iterating on an ARM64 phone Stone from an
x64 development box. For the device-side bring-up (custom kernel, native `dockerd`) and
the day-to-day `adb` workflow, see the Phone-to-Stone project notes.

## How it runs

The phone runs **native Docker** (the kernel ships the container primitives). `garden-moss`
runs **inside a glibc container** (`zen-garden/stone:arm64`) on that Docker — it never
touches Android's bionic libc. The container uses `--network host` (so multicast discovery
binds the phone's interfaces) and mounts the host `docker.sock` (so Moss orchestrates
sibling offering containers).

This is why the build target is the **GNU container image**, not a static musl binary:
the phone's runtime is Docker, and a glibc container sidesteps musl's C-dependency
friction (`aws-lc-rs`/BoringSSL via `koi`, `libudev`) entirely. See
[STONE-0001](../decisions/STONE-0001-lineageos-arm64-full-stone.md).

## Prerequisites

| Requirement | Notes |
|---|---|
| Docker Desktop (x64 host) | Build + cross-compile + QEMU emulation for arm64 images |
| `koi` sibling repo at `../koi` | Path dependency (mounted into the builder) |
| Android platform-tools (`adb`) | Auto-detected by `deploy-android.ps1` |
| Device: rooted LineageOS, container kernel, **native `dockerd` running** | The "Stage 3a" device bring-up; deploy is blocked until `docker run hello-world` works on the phone |

## 1. Build the ARM64 binaries

Cross-compiles `garden-moss` + `garden-rake` for `aarch64-unknown-linux-gnu` in a Docker
builder (mirrors the x64/x86 pipelines). Core tier is all a phone Stone needs.

```powershell
# binaries only -> dist/linux-arm64/
.\installer\compile-linux-arm64.ps1 -Targets garden-moss,garden-rake -Fast

# or binaries + deployment package (tar.gz) -> dist/staging/
.\installer\build-linux-arm64.ps1 -Version 0.1.0 -Tier core
```

## 2. Build the runnable Stone image

Packages the binaries into `zen-garden/stone:arm64` (built on the x64 host via QEMU;
the script registers the binfmt handlers if missing) and saves a tar for transfer.

```powershell
.\installer\build-stone-image-arm64.ps1 -Save   # -> dist/stone-arm64.tar
```

## 3. Deploy to the phone over ADB

First-boot bootstrap (there is no running Moss yet to accept the normal HTTP deploy).
The script pre-flights the device authorization and the on-phone Docker daemon, pushes
the image, loads and runs it, and installs a Magisk boot service.

```powershell
.\installer\deploy-android.ps1            # builds image if needed, then deploys
```

The container is created with `--restart unless-stopped`; the Magisk service at
`/data/adb/service.d/garden-moss.sh` covers the cold-boot race before Docker's restart
policy acts (no systemd on Android).

## 4. Observe and iterate

```powershell
$adb = (Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\Google.PlatformTools_*\platform-tools\adb.exe").FullName
& $adb shell "su -c 'docker ps'"
& $adb shell "su -c 'docker logs --tail 50 moss'"
& $adb shell "su -c 'curl -s http://127.0.0.1:7185/health'"
```

Iterate: edit → `compile-linux-arm64.ps1` → `deploy-android.ps1` → observe → repeat.

## Networking

The phone has a **single USB-C port**: while it carries `adb`, wired Ethernet is not
attached. To put the Stone on the LAN for discovery (`garden-rake discover` from another
box), attach a USB-C Ethernet adapter (`eth0`) — at which point `adb` moves to TCP:

```powershell
& $adb tcpip 5555            # run while still on USB
& $adb connect <phone-ip>:5555
```

WiFi (`wlan0`) is typically unavailable on the container kernel (the WiFi vendor module
is skipped); a wired adapter is the supported LAN path.

## Constraints

- **MongoDB on 4 GB**: cap WiredTiger cache (`--wiredTigerCacheSizeGB 0.5`) so the offering
  fits alongside Moss.
- **CPU-only**: Adreno GPU exposes no CUDA/ROCm/OpenVINO; Moss reports the Stone as
  CPU-only (correct, no false GPU detection).
- **Deploy is gated on on-device Docker** — `deploy-android.ps1` stops with guidance if
  the phone's `dockerd` is not up.
