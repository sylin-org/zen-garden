# Cricket Android Audio Research — Target-Agnostic Audio Backend

> **Status:** research / design note (not an ADR).
> **Goal:** make `garden-cricket` build *and* play sound from a single `aarch64-unknown-linux-musl` static binary that runs on rooted Android (LineageOS), while keeping the **same source** building and playing sound on desktop Linux (and macOS/Windows).
> **Context:** STONE-0001 ships moss + rake as fully-static musl binaries on the phone Stone. Cricket is currently excluded from that build because it links GNU `libasound`.

---

## 1. What cricket does and its current audio stack

### What cricket is

`garden-cricket` is the ambient **audio companion**. It connects to the local Moss over SSE (`SseTransport`) plus a `CommandTransport` HTTP port, subscribes to a fixed set of `core.*` presence events, and on each mapped event plays a short audio sample on one of **4 mixer channels** (foreground / midground / ambient / background). It also handles "hey-tell" commands (`select / volume / list / show / play / stop / on / off / status`) routed via `core.command.invocation` events.

The event→sound mapping lives in tune YAML (`src/cricket/tunes/zen-tech/tune.yaml`). Samples are MP3, embedded in the binary via `rust-embed` (`#[folder = "tunes/"]`), and can be overlaid from the filesystem. Decoding is from in-memory bytes (`Mixer::play_bytes` → `rodio::Decoder` over a `Cursor`).

### The audio stack (the load-bearing part)

| Layer | Crate / API | Portable? |
|-------|-------------|-----------|
| Decode (MP3 → PCM `i16`) | `rodio::Decoder` → **Symphonia** (pure Rust) | ✅ yes — target-agnostic |
| Mix / 4 channels / volume / debounce | `rodio::Sink` + custom logic in `mixer.rs` | mix math is pure Rust; `Sink` is tied to the output |
| **PCM output sink** | `rodio::OutputStream` → `cpal` → `alsa` crate → **libasound (GNU)** | ❌ **NO — this is the only blocker** |

Verified facts (file:line):

- `src/cricket/Cargo.toml:38` — `rodio = "0.18"` is the **only** audio crate. No `cpal` / `alsa-sys` / `kira` / `tinyalsa` direct deps.
- `src/cricket/src/mixer.rs:5` imports `rodio::{OutputStream, OutputStreamHandle, Sink, Source}`.
- `src/cricket/src/mixer.rs:141` — `OutputStream::try_default()?` is the single call that opens the default OS output device (this is the `cpal`/libasound entry point).
- `src/cricket/src/mixer.rs:159,173,186` — the mixer is **already PCM-`i16`-based**: every play path builds `Box<dyn Source<Item = i16> + Send>` and appends to a `Sink`. The sample format the rest of cricket speaks is already `i16`.
- `src/cricket/src/mixer.rs:130-136` — there is already a hand-written `unsafe impl Send/Sync for Mixer` to work around cpal's stream handle not being `Send`. The cpal coupling is *already* a source of friction.
- `src/cricket/src/mixer.rs:11-94` — `ensure_audio_dependencies()` / `init_system_audio()` shell out to `alsa-utils` (`aplay` / `amixer`) and are **already** `#[cfg(target_os = "linux")]`-gated with non-Linux no-op fallbacks. **These are the only existing cfg-gates; the `rodio`/cpal output path itself is compiled unconditionally.**

### The exact libasound dependency chain (verified against `Cargo.lock`)

```
rodio 0.18.1
  └── cpal 0.15.3
        └── alsa 0.9.1            (selected on cfg(target_os = "linux"))
              └── alsa-sys 0.3.1
                    └── build.rs: pkg_config::probe_library("alsa")
                          → cargo:rustc-link-lib=asound   ← HARD dynamic link to libasound.so.2
```

- `cpal/Cargo.toml` gates the `alsa` dep on `cfg(any(target_os = "linux", dragonfly, freebsd, netbsd))`. On `target_os = "android"` it instead uses **oboe + ndk** (AAudio / OpenSL ES via NDK + bionic).
- `alsa-sys-0.3.1/build.rs` emits a **hard dynamic link** (`rustc-link-lib=asound`), *not* `dlopen`. So it needs `libasound-dev` to link and `libasound.so.2` at runtime.

### The abstraction seam that already exists

`mixer.rs` is the **single chokepoint**. Every playback goes through `Mixer::{new, play, play_bytes, play_source, stop, set_master_volume, set_channel_volume}`. The callers (`adapters/audio.rs`, `test_mode.rs`, `manifest.rs`, `main.rs`) only ever touch those methods — never `rodio` directly. Decoding (`rodio::Decoder`, pure Rust) is independent of the output backend. **A backend trait can slot in entirely behind the Mixer's public surface without touching any caller.**

---

## 2. Why it blocks on arm64-musl / Android

The Android binaries are built as `aarch64-unknown-linux-**musl**` — i.e. `target_os = "linux"`, **not** `target_os = "android"`. This single fact is the whole problem:

1. Because `target_os = "linux"`, `cpal` selects the **ALSA backend** (it has no "linux-without-alsa" mode), so the build wants to link GNU `libasound`.
2. The kernel `/dev/snd/pcmC*D*p` PCM ABI **is** present on Android (tinyalsa proves this) — the blocker is purely the **GNU userspace lib**, not the kernel ABI.
3. cpal's Android/Oboe backend is gated on `target_os = "android"`, which would require the **NDK / bionic** toolchain — unreachable from a musl-static build without forking the target triple.

There are **two independent, decisive reasons** libasound cannot go into a musl static binary even if we tried to vendor it:

- **`alsa-sys` hardwires `statik(false)`** in its build.rs — there is no supported static-link path. Attempts hit `__stat_time64` / `__clock_gettime64` and double-libc linker errors (diwic/alsa-sys#10, unresolved upstream).
- **alsa-lib `dlopen()`s its PCM/plugin modules at runtime** (`snd_dlopen`). A **fully static musl binary cannot `dlopen` at all** — musl's `dlopen` in a static binary is a stub returning *"Dynamic loading not supported"*.

So libasound is structurally incompatible with a musl static binary on two levels. This confirms the premise: the kernel ABI is fine; the GNU userspace lib is the wall.

### Is the audio feature-gateable today? Partially, and not cleanly.

- `rodio 0.18` has **no feature flag** to drop `cpal`/ALSA (its `[features]` are decoders + `oboe-shared-stdcxx` only). The cpal output sink is always compiled on Linux. You cannot disable libasound via a rodio Cargo feature alone.
- cricket itself has **no** cfg-gating or feature flags around the *output* path — only the `aplay`/`amixer` helpers are gated. `Mixer` and `OutputStream::try_default()` compile unconditionally.
- `installer/compile-linux-arm64-musl.ps1` builds **only `garden-moss` + `garden-rake`** by default (line 129: `$defaultTargets = @("garden-moss", "garden-rake")`), and applies `--no-default-features` to moss to drop `udev`. Cricket's exclusion from the musl image is achieved simply by **not listing it** — there is no in-repo mechanism that makes cricket compile on musl. (The glibc ARM64 script `compile-linux-arm64.ps1` *does* list cricket, for Raspberry-Pi-class glibc stones where libasound is present.)

---

## 3. Viable target-agnostic options (RANKED)

The system splits cleanly into **shared decode/mix (pure Rust)** and a **target-specific output sink** (the only non-portable part). Every option below is about that sink. Selection should be by **feature flag** (so a musl-on-a-Pi could also opt into the raw sink), with `cfg` fallbacks — *not* `target_os` alone.

### ✅ #1 (RECOMMENDED) — Feature-gated `AudioBackend` trait + pure-Rust raw `/dev/snd` PCM sink (port tinyalsa to Rust)

Define a small trait that consumes interleaved `i16` PCM frames; keep `rodio::Decoder` (Symphonia) for decode and the existing 4-channel mix math as the shared, pure-Rust core. Implementations:

- **desktop** (glibc Linux / macOS / Windows): keep the current `rodio`/cpal sink — **behavior unchanged**.
- **musl/Android**: a small **pure-Rust SNDRV_PCM backend** issuing `SNDRV_PCM_IOCTL_*` ioctls on `/dev/snd/pcmC*D*p` (mirror tinyalsa's `open → hw_params → prepare → writei → xrun-recover` loop using `nix`/`libc` `ioctl`).
- **null/headless**: a no-op sink for CI/tests and when no `/dev/snd` exists.

The SNDRV_PCM ioctl ABI is a **stable kernel UAPI**, identical on desktop Linux and the Android kernel, and reachable from a pure-musl static binary (ioctls are just syscalls — no libc/libasound/NDK/dlopen). tinyalsa is the C reference proving the entire needed surface.

- **Pros:** one codebase, no fork, **truly static** (zero `dlopen`, zero C), the *same* raw sink also works on desktop Linux (genuinely target-agnostic with one code path), keeps in-process 4-channel mixing/looping/volume, drops the cpal `Send/Sync` hack, matches the project's existing `cfg`-gating style. Caller code (`adapters/audio.rs`, `test_mode.rs`, `main.rs`) is untouched.
- **Cons:** you own ~300–500 lines of SNDRV_PCM ioctl code and its xrun/recovery edge cases; must negotiate `hw_params` (rate/format S16/channels) against what the device's PCM accepts; on HAL-routed Android devices you must first program the kernel mixer controls via `/dev/snd/controlC*`.

### #2 — Vendor C `tinyalsa` + thin `tinyalsa-sys` crate, static-link

Add a `tinyalsa-sys` build.rs that compiles tinyalsa's C sources into a static lib (`cc` crate) and `bindgen` the pcm/mixer API; implement the **same `AudioBackend` trait** over it for the musl target.

- **Pros:** reuses battle-tested, Android-proven C (BSD license, compatible); less ioctl code to author; statically linkable, no `dlopen`. Functionally identical runtime result to #1.
- **Cons:** pulls a C build (`cc` + `bindgen`) into the musl cross image; an `unsafe` FFI surface; less "pure Rust" than #1; still needs kernel-mixer setup on HAL devices. **No production-grade pure-Rust tinyalsa exists on crates.io today** — only the unrelated libasound-wrapping `alsa` crate, plus experimental `kernel-asound-sys` / `alsa_ioctl` that give ioctl constants/structs but *not* the PCM state machine. So #1's ioctl loop must be hand-written either way; #2 just borrows the proven C state machine instead.

### #3 — Cargo feature `audio` (default on); musl builds `--no-default-features` → null sink (headless, **silent**)

Wrap the `rodio`/`Mixer` output behind `#[cfg(feature = "audio")]`, provide a silent no-op sink otherwise, and add `garden-cricket` to `compile-linux-arm64-musl.ps1` with the feature off.

- **Pros:** trivial, immediately unblocks the **build**, one codebase, no libasound linkage on the phone, mirrors moss's existing `--no-default-features` pattern.
- **Cons:** phone cricket is **SILENT**. Satisfies "builds + runs" but **not "working audio"**. Best as an **incremental step / validation gate** before #1 lands, *not* the final design.

### ❌ #4 (REJECTED) — AAudio / OpenSL ES via `dlopen` of bionic `libaaudio.so` from the musl binary

Not feasible. (a) musl static binaries cannot `dlopen` (stub returns "Dynamic loading not supported"); even a *dynamic* musl binary can't safely load bionic `.so` (incompatible libc ABI/TLS/relocation in one process). (b) AAudio/OpenSL are NDK APIs meant for bionic-linked apps. The only way to use them is to build against the `aarch64-linux-android` NDK target — i.e. **fork the build/target**, exactly what the goal rules out. **Do not pursue.**

### ❌ #5 (REJECTED) — Swap rodio for kira / tinyaudio / patched cpal

Same platform matrix as cpal: Linux = libasound, Android = AAudio/NDK. Buys nothing for musl; same dead-end. `rodio` is also deeply wired (Sink-per-channel, `Source` trait, `Decoder`), so a swap is a *larger* rewrite of `mixer.rs` than #1's trait extraction. Not recommended.

### Stopgap (orthogonal) — shell out to a static `tinyplay`/`tinymix` on the musl target

Bundle prebuilt aarch64-static `tinyplay`/`tinymix` next to cricket and spawn per sample (parallels the existing `aplay`/`amixer` subprocess pattern). Useful **only** to prove `/dev/snd` works on the actual device before investing in #1; weak as a final design (per-sample process spawn, no in-process mixing/looping/master-volume).

| Option | One codebase? | Static musl? | Working audio on Android? | Effort |
|--------|:---:|:---:|:---:|:---:|
| #1 raw `/dev/snd` ioctl (pure Rust) | ✅ | ✅ | ✅ | High |
| #2 vendor C tinyalsa (FFI) | ✅ | ✅ | ✅ | Med-High |
| #3 feature-gate → null sink | ✅ | ✅ | ❌ silent | Low |
| #4 AAudio via dlopen | ❌ fork | ❌ | n/a | — (rejected) |
| #5 kira/tinyaudio swap | ✅ | ❌ | ❌ | Med (rejected) |

---

## 4. Recommended plan — build + run with working audio on arm64-musl (same codebase as Linux)

Phased: **#3 first to unblock the build immediately**, then **#1 to deliver real audio**, with **#2 as the fallback** if the hand-ported ioctl loop proves too risky. All three converge on the **same `AudioBackend` trait**, so phase 1 is not throwaway work.

### Phase 0 — On-device validation (do this first, regardless of choice)

On the rooted LineageOS Pixel:

1. Confirm `/dev/snd/` lists `pcmC*D*p` and `controlC*` nodes.
2. Push a static aarch64 `tinyplay` + a test WAV; verify it produces sound. Handle SELinux: try `setenforce 0` to isolate MAC issues; check the `audio` gid.
3. If sound only appears after programming kernel mixer controls, the device is **HAL-routed** and #1/#2 must drive `/dev/snd/controlC*` first. If even root + permissive produces nothing, the device is fully HAL-gated and only an NDK/AAudio fork would work (re-evaluate the goal).

This single test determines how much kernel-mixer programming #1/#2 must do.

### Phase 1 — Extract the `AudioBackend` trait + null sink (unblocks the musl build)

**New file** `src/cricket/src/audio/mod.rs` (one file per concept, per code-standards §14):

```rust
/// PCM output sink. Consumes interleaved i16 frames; the mixer owns mixing.
pub trait AudioBackend: Send + Sync {
    fn open(rate: u32, channels: u16) -> anyhow::Result<Self> where Self: Sized;
    fn write_frames(&self, interleaved_i16: &[i16]) -> anyhow::Result<()>;
    fn drain(&self) -> anyhow::Result<()>;
}
```

- `src/cricket/src/audio/rodio_backend.rs` — wraps the current `rodio::OutputStream` + `Sink` path (the desktop default). Move the `unsafe impl Send/Sync` here.
- `src/cricket/src/audio/null_backend.rs` — no-op sink (logs at `debug`). Used for the `--no-default-features` musl build, CI, and `test_mode`.
- Refactor `src/cricket/src/mixer.rs` so `Mixer` holds `Box<dyn AudioBackend>` instead of `OutputStreamHandle` directly. **Public method surface stays identical** — `play`, `play_bytes`, `play_source`, `stop`, `set_master_volume`, `set_channel_volume` — so `adapters/audio.rs`, `test_mode.rs`, `manifest.rs`, `main.rs` are untouched.

**`src/cricket/Cargo.toml`** — make audio a default feature:

```toml
[features]
default = ["audio-rodio"]
audio-rodio = ["dep:rodio"]      # desktop default; pulls cpal/libasound
audio-alsa-raw = []              # phase 2: pure-Rust /dev/snd ioctl sink
# (no feature) → null backend, no libasound linkage

[dependencies]
rodio = { version = "0.18", optional = true }
# Decoder note: rodio's Symphonia decoder is needed even for the raw sink.
# Keep rodio as an optional dep for *decode only* in the raw-sink build,
# OR depend on `symphonia` directly so decode survives without cpal.
```

> **Decision point on decode:** `rodio::Decoder` drags in cpal transitively only via `rodio`'s default sink path? No — `rodio` always compiles cpal on Linux. To decode without libasound, **depend on `symphonia` directly** (`symphonia = { version = "0.5", features = ["mp3"] }`) in the raw-sink build and feed PCM to the trait. Confirm during phase 2 whether `rodio` can be kept decode-only; if not, switch the decode call in `mixer.rs::play_bytes` to a thin `symphonia` helper behind the same `i16`-frame interface.

**`installer/compile-linux-arm64-musl.ps1`** — add cricket to the musl targets with the feature off:

- Line 129: extend `$defaultTargets` to include `garden-cricket` **OR** keep moss+rake default and document that cricket builds with `--no-default-features`.
- In the per-target loop (lines 165-172), the script already special-cases `garden-moss` with `--no-default-features`. Add the same for `garden-cricket`:

```powershell
if ($target -eq "garden-moss" -or $target -eq "garden-cricket") { $cargoArgs += "--no-default-features" }
```

After phase 1: cricket **builds + runs** as a static musl binary on the phone, links no libasound, and is silent. Companion lifecycle (SSE, hey-tell commands, status) all work.

### Phase 2 — Pure-Rust raw `/dev/snd` PCM sink (delivers real audio)

`src/cricket/src/audio/alsa_raw_backend.rs`, gated `#[cfg(feature = "audio-alsa-raw")]`:

- Open `/dev/snd/pcmC{card}D{dev}p` (`O_RDWR`). Discover card/device by scanning `/dev/snd/` (start with `pcmC0D0p`; make configurable via env, e.g. `ZG_CRICKET_PCM`).
- `SNDRV_PCM_IOCTL_HW_PARAMS` — negotiate `SNDRV_PCM_FORMAT_S16_LE`, the mixer's sample rate, channel count, period/buffer sizes (mirror tinyalsa's `pcm_open` defaults).
- `SNDRV_PCM_IOCTL_PREPARE`, then `SNDRV_PCM_IOCTL_WRITEI_FRAMES` in `write_frames`, with xrun recovery on `-EPIPE` via `SNDRV_PCM_IOCTL_PREPARE`.
- Crates: `nix` (`ioctl_*!` macros) or raw `libc::ioctl`. Struct/constant source: hand-declare from `sound/asound.h` UAPI, or pull from experimental `kernel-asound-sys` for the numbers (don't depend on it for the state machine).
- On HAL-routed devices (per phase 0): program required controls via `/dev/snd/controlC{card}` (`SNDRV_CTL_IOCTL_ELEM_WRITE`) before `prepare`.

The `Mixer` already produces interleaved `i16` (it builds `Source<Item = i16>` today), so feeding `write_frames(&[i16])` is a natural fit. Resampling, if the device rejects the source rate, can ride on Symphonia/`rodio`'s existing resampler or a small linear resampler.

Build the phone with `--no-default-features --features audio-alsa-raw`. The same feature also works on desktop Linux (validates the path without a phone). SELinux: ship a small sepolicy allow rule for the binary's domain to access `audio_device`, or document `setenforce 0` for the dev phase.

### Phase 2-alt (fallback within audio) — vendor C tinyalsa (#2)

If the hand-ported ioctl loop proves too fiddly (xrun edge cases, hw_params negotiation), add a `tinyalsa-sys` crate (`cc` compiles vendored tinyalsa C, `bindgen` the API) and implement `AudioBackend` over `pcm_open`/`pcm_writei`. Same runtime result; trades pure-Rust for a proven C state machine. Reachable from musl (tinyalsa uses only kernel ioctls, no GNU userspace).

### Net change surface

- **New:** `src/cricket/src/audio/{mod,rodio_backend,null_backend,alsa_raw_backend}.rs`.
- **Edited:** `src/cricket/src/mixer.rs` (hold `Box<dyn AudioBackend>`; public API unchanged), `src/cricket/Cargo.toml` (features), `installer/compile-linux-arm64-musl.ps1` (cricket target + `--no-default-features`).
- **Untouched:** `adapters/audio.rs`, `adapters/mod.rs`, `test_mode.rs`, `manifest.rs`, `main.rs`, `tunes/`.

---

## 5. Fallback — headless cricket / firefly-only if audio proves infeasible

If phase 0 shows the device is **fully HAL-gated** (no sound from `/dev/snd` even as root + permissive) and an NDK fork is off the table:

1. **Ship the null-sink build (phase 1) as the final phone artifact.** Cricket still runs as a real companion: it connects to Moss, processes presence events and hey-tell commands, reports status — it just emits no sound. This satisfies "builds + runs," keeps cricket present in the fleet, and costs nothing extra (phase 1 is already done).
2. **Lean on firefly for ambient feedback on the phone.** `garden-firefly` is the LED companion; on a phone its analog is screen/notification/vibration feedback. Map the same `core.*` presence events that cricket would have sonified to a visual/haptic channel. (Scope this separately — firefly's own target-agnosticism for Android is not analyzed here.)
3. **Keep audio on glibc stones.** Cricket-with-sound remains the default on Raspberry-Pi-class glibc ARM64 stones (`compile-linux-arm64.ps1` already builds it). The phone simply runs the headless variant. One codebase, two feature configs — no fork.

This fallback is **strictly a superset of doing nothing**: even in the worst case, cricket goes from *excluded* to *present-but-silent* on the phone, and the `AudioBackend` seam means real audio can land later the moment a workable kernel path (or a relaxed single-binary constraint) appears.

---

## Sources

- Repo (verified file:line): `src/cricket/Cargo.toml:38`; `src/cricket/src/mixer.rs:5,11-94,130-136,141,159,173,186`; `installer/compile-linux-arm64-musl.ps1:129,163-172`; `installer/compile-linux-arm64.ps1` (cricket in default glibc targets); `src/cricket/tunes/zen-tech/tune.yaml`; `Cargo.lock` (rodio 0.18.1 → cpal 0.15.3 → alsa 0.9.1 → alsa-sys 0.3.1).
- cpal `0.15.3` Cargo.toml — alsa gated on `cfg(target_os = linux/bsd)`; oboe + ndk on android.
- alsa-sys `0.3.1` build.rs — `pkg_config::probe_library("alsa")` → `link-lib=asound`.
- rodio `0.18.1` Cargo.toml — no feature flag to disable cpal/ALSA.
- diwic/alsa-sys#10 — alsa-sys hardwires `statik(false)`; musl static link fails (`__stat_time64`/`__clock_gettime64`).
- alsa-lib `dlopen`s plugin modules (incompatible with static linking).
- musl: static binaries cannot `dlopen` (stub "Dynamic loading not supported") — openwall musl list, 2012-12-08.
- tinyalsa (github.com/tinyalsa/tinyalsa) — BSD, minimal C over `/dev/snd` kernel ioctls, ships tinyplay/tinymix/tinycap/tinypcminfo.
- termux/termux-packages#821 — direct `/dev/snd` works only with vendor ALSA kernel driver + root; HAL routing is the variable.
- SNDRV_PCM ioctl interface — stable kernel UAPI (`sound/asound.h`).
- No pure-Rust tinyalsa on crates.io; only libasound-wrapping `alsa` crate + experimental `kernel-asound-sys` / `alsa_ioctl` (ioctl bindings only, no PCM state machine).
- Symphonia (github.com/pdeljanov/Symphonia) — pure-Rust MP3 decode → PCM, independent of output backend; rodio's default decoder.
