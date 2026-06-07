---
audience: developer
doc_type: design
status: proposed
---

> Source: multi-agent host-assumption audit (2026-06-07). 75 confirmed assumptions across 7 dimensions.
> Working artifact; promote the HOST-0001 ADR section to docs/decisions/ on acceptance.

# Moss HostProfile / Runtime Configuration

## Summary

Moss was ported to a rooted LineageOS phone (read-only `/` and `/etc`, `/etc` symlinked to `/system/etc`, no systemd, bionic libc, no system CA store, kernel with `CONFIG_ANDROID_PARANOID_NETWORK`, only `/data` writable). Roughly ten distinct host assumptions broke. The root cause is **not a missing facility** — Moss already has env-backed `paths::*` helpers, a host-trust-store-independent `client_builder()`, an `EnvConfig` registry, and a DDD `Moss`/`Current` aggregate wired through `FromRef`. The defect is that these facilities are **read inconsistently**: compile-time consts (`CONFIG_DIR`) and scattered `env::var()` / bare-client constructions bypass them. A handful of genuinely-missing knobs (docker socket, identity-write policy, container security context, DNS provisioning) round out the gap.

The fix is one typed value object, `HostProfile`, loaded **once** at startup, parked on `Current` (it is part of "what this running instance is"), and consumed via `FromRef`. It does not replace the env vars — it becomes the **single typed reader** of them, so the rest of the code reads `state.current.host.runtime.docker_socket` instead of calling `env::var(...)` in a dozen places. The guiding principle: a future non-standard host should be **configured, not patched**.

Two design-phase corrections survived verification and are reflected throughout:
- **TLS is already host-independent.** `http.rs:20-38` already loads bundled webpki roots via `client_builder()`. Remaining work is routing bare `Client::new()` call sites through it and adding a corporate/air-gapped CA source — *not* re-adding bundled roots.
- **Architecture is already canonical.** `resources/system.rs:30` keeps `architecture` from `std::env::consts::ARCH`; the device-tree string only fills `model_name` (`:52`). An arch *override* is a low-priority escape hatch, not a bug fix; it is **cut** from v1.

Three load-bearing corrections from review are folded in as authoritative:
- **Env-prefix reality (`ZG_` vs `GARDEN_`):** the documented-primary `ZG_*` prefix does **not work anywhere** in the relevant readers. `HostProfile::load()` must resolve `ZG_*` first, `GARDEN_*` as deprecated fallback, and name all new knobs `ZG_*`.
- **Bootstrap ordering is a cycle as originally drafted:** the TOML that would carry `[host]` is itself read from the broken `CONFIG_DIR` const. `paths` must resolve env/heuristic-only **before** any TOML read.
- **Several "knobs" are actually bugs:** `DOCKER_HOST` support, fatal `/proc/cpuinfo` read, the interface-candidate heuristic, the `atomic_write` temp-name collision, and the `companions_dir()` fallback are fixes, not configuration surface.

---

## Audit

Confirmed host assumptions, grouped by namespace. Each row: the code site (file:line), severity, and the knob (or, where reclassified, the fix). Severity scale: **Critical** = blocks boot/run on Android; **High** = blocks a major feature; **Medium** = degraded/partial; **Low** = mockability/escape-hatch.

### `paths` (config/data tree placement)

| Site | Severity | Knob / Fix |
|---|---|---|
| `src/moss/src/infra/config.rs:312` (`MossConfig::load`) | **Critical** | `CONFIG_DIR` const → `config_dir()` fn. This governs the file the whole profile reads (see Bootstrap ordering). |
| `src/moss/src/infra/config.rs:384` (`MossConfig::save`) | **Critical** | same const → fn |
| `src/moss/src/infra/persistence.rs:11,22,48,170` | High | `CONFIG_DIR` const → `config_dir()` fn |
| `src/moss/src/infra/hardware.rs:124,148` | High | const → `config_dir()` fn |
| `src/moss/src/infra/task_store.rs:34` | High | const → `config_dir()` fn |
| `src/moss/src/infra/nurturing_store.rs`, `src/moss/src/api/v1/console.rs` | High | const → `config_dir()` fn |
| `src/moss/src/infra/network/state.rs:15` | Medium | const → `{config_dir()}/network-state.json` |
| `src/moss/src/infra/manifests/registry.rs:41` | Medium | const → fn over `data_dir()` |
| `src/moss/src/infra/manifests/hw.rs:19` | Medium | const → fn over `config_dir()` |
| `src/common/src/constants/paths.rs:169-182` (`companions_dir`) | High | fallback `/usr/local/bin/companions` (read-only on Android) → `{data}/companions`. **Plain fix, not a knob.** |
| `src/moss/src/infra/storage/platform.rs:707,850`, `network/linux.rs:109,318` | Medium | new `paths.temp` (`ZG_TEMP_DIR`) |
| `src/moss/src/infra/installer/linux.rs:9` | Medium | new `paths.bin_install` (`ZG_BIN_INSTALL_DIR`) |

### `runtime` (Docker + container security context)

| Site | Severity | Knob / Fix |
|---|---|---|
| `src/moss/src/docker/mod.rs:55` | **Critical** | `connect_with_socket_defaults()` ignores `DOCKER_HOST`. **Fix:** call `connect_with_defaults()` (honors `DOCKER_HOST`); knob = explicit `runtime.docker_socket` path only. Do not reimplement `DOCKER_HOST` parsing. |
| `src/moss/src/docker/lifecycle.rs:540-557` (`build_container_config`) | High | `runtime.container.privilege: ContainerPrivilege` (paranoid-network) |
| `src/moss/src/docker/exec.rs:25,48` | Medium | `runtime.image_pull_policy: ImagePullPolicy` |
| `src/moss/src/infra/docker_config.rs:39,195` | Low | `runtime.docker_config_path`, `runtime.docker_restart_command` |
| `src/moss/src/api/v1/storage.rs:1245+`, `api/v1/admin.rs:283`, `infra/storage/subprocess.rs:72`, `infra/storage/platform.rs:91` | **Critical** | `Command::new("sudo")` → `ENOENT` on Android. `runtime.privilege_escalation: PrivilegeMode` **plus** `geteuid()==0` auto-detect to `Direct`. |
| `src/common/src/infra/timer.rs:331` | High | `runtime.scheduler: Scheduler` (Systemd assumption) |

### `identity` (host file write policy — read-only `/etc`)

| Site | Severity | Knob / Fix |
|---|---|---|
| `src/common/src/console/tty.rs:399` (write `/etc/hostname`) | **Critical** | `identity.hostname: WritePolicy` |
| `src/common/src/console/tty.rs:445` (read `/etc/hostname`, no `hostname`-cmd fallback unlike Windows `:430`) | High | add Linux `hostname`-command read fallback |
| `src/common/src/console/tty.rs:466,511` (read-modify-write `/etc/hosts`) | **Critical** | `identity.hosts_file: WritePolicy` — **separate** from `hostname`; the `:466` read itself fails on Android |
| `src/common/src/console/tty.rs:694` (write `/etc/motd`) | High | `identity.motd: WritePolicy` |
| `src/moss/src/infra/hardware_id.rs:107` | Medium | `identity.machine_id_source: MachineIdSource` |

### `network`

| Site | Severity | Knob / Fix |
|---|---|---|
| `src/moss/src/infra/network/mod.rs:357-387` (hardcoded candidate list + dir-scan, no route lookup) | High | **Fix:** route-table primary detection (`/proc/net/route` / `ip route`). Knob = `network.interface` override only. Do not promote the candidate-list heuristic to config. |
| `src/moss/src/infra/network/linux.rs:31` (ifupdown/netplan apply) | High | `network.config_method: NetConfigMethod` (`None` disables file-based provisioning on Android) |
| `src/moss/src/infra/network/linux.rs:280,454` (DNS write) | Medium | must be **inside** the `config_method` branch, not parallel |
| `src/moss/src/infra/installer/provision.rs:296` (`apt install systemd-resolved`) | **Critical** | `network.dns_provisioning: DnsProvisioning` (no apt/systemd on Android) |
| `src/common/src/infra/communications/p2p.rs:1221` (mcast TTL) | Low | lives in `DiscoveryConfig::from_env()` (`p2p.rs:114`) as `ZG_DISCOVERY_MCAST_TTL` — **not** in `HostProfile` |

### `tls`

| Site | Severity | Knob / Fix |
|---|---|---|
| `src/moss/src/http.rs:20-38` | — | already host-independent (bundled webpki roots). **Do not re-add roots.** |
| `src/moss/src/tasks/discovery.rs:40`, `api/v1/offering_capabilities.rs:841`, `infra/registry_client.rs:106,253` | Medium | route bare `Client::new()`/`Client::builder()` through `http::client_builder()` |
| `src/moss/src/http.rs:36` | Medium | `tls.root_source: TlsRootSource` + `tls.extra_ca_bundle` (corporate/air-gapped CA) |
| `detection/http_probe.rs:33`, `detection/pipeline.rs:65`, `infra/storage/handle.rs:1081` (`danger_accept_invalid_certs`) | — | **intentional** (localhost / pond-CA mTLS, SECURITY-0001) — out of scope, stay as-is |

### `hardware` (CUT from v1)

| Site | Severity | Disposition |
|---|---|---|
| `src/common/src/resources/system.rs:30` (`architecture`) | — | already canonical; **no override knob** |
| `src/common/src/resources/system.rs:52` (`/proc/device-tree/model`) | — | only fills `model_name`; **no path knob** |
| `src/common/src/resources/system.rs:26` (`/proc/cpuinfo` `.context(...)?` fatal) | Medium | **Fix:** make non-fatal — degrade to `model_name="Unknown"`, `features=[]`, arch from `std::env::consts::ARCH`. **Not a path knob.** |

---

## Config model

`HostProfile` lives in a new file `src/moss/src/infra/host_profile.rs` (one type per concept, §14). It follows code-standards §1 (namespaces over prefixes — the struct *is* the namespace, fields are plain names), §3 (no `Context`/`Manager` suffix), §5 (immutable node self-description → `Arc`, no lock, on `Current`), and §8 (state machines as enums, never bool pairs).

```rust
// src/moss/src/infra/host_profile.rs

/// What this host *is* and *permits* — loaded once at startup from env +
/// optional [host] block in garden-moss.toml. Immutable for the process
/// lifetime, shared behind Arc with no lock (code-standards §5).
#[derive(Clone, Debug)]
pub struct HostProfile {
    pub platform: Platform,   // resolved profile identity
    pub paths:    Paths,
    pub runtime:  Runtime,
    pub identity: Identity,
    pub network:  Network,
    pub tls:      Tls,
    // NOTE: no `hardware` namespace in v1 (cut — see Audit)
}

pub enum Platform { LinuxStandard, Android, Minimal }

pub struct Paths {
    pub config:            std::path::PathBuf,
    pub data:              std::path::PathBuf,
    pub temp:              std::path::PathBuf,
    pub bin_install:       std::path::PathBuf,
    pub companions:        std::path::PathBuf,
    pub network_state:     std::path::PathBuf,
    pub runtime_manifests: std::path::PathBuf,
    pub hw_manifests:      std::path::PathBuf,
}

pub struct Runtime {
    pub docker_socket:          Option<std::path::PathBuf>,
    pub docker_config_path:     Option<std::path::PathBuf>,
    pub docker_restart_command: Option<String>,
    pub image_pull_policy:      ImagePullPolicy,
    pub privilege_escalation:   PrivilegeMode,
    pub scheduler:              Scheduler,
    pub container:              Container,
}
pub enum ImagePullPolicy { Always, IfNotPresent, Never }
pub enum PrivilegeMode    { Sudo, Direct, None }
pub enum Scheduler        { Systemd, Cron, None }

pub struct Container {
    pub privilege:    ContainerPrivilege,   // SINGLE posture knob
    pub user:         Option<String>,
    pub network_mode: NetworkMode,
    pub bind_address: std::net::IpAddr,
    pub restart_policy: RestartPolicy,
    pub advanced:     ContainerAdvanced,    // raw caps — escape hatch only
}
/// One enum expands into the bollard fields internally. Avoids representable
/// impossible postures (e.g. Privileged + cap_drop=ALL). §8.
pub enum ContainerPrivilege { ImageDefault, AmbientNetRaw, HostNetwork, Privileged }
pub enum NetworkMode  { Bridge, Host }
pub enum RestartPolicy { No, OnFailure, Always, UnlessStopped }
pub struct ContainerAdvanced { pub cap_add: Vec<String>, pub cap_drop: Vec<String> }

pub struct Identity {
    pub hostname:          WritePolicy,   // /etc/hostname only
    pub hosts_file:        WritePolicy,   // /etc/hosts — SEPARATE
    pub motd:              WritePolicy,   // /etc/motd
    pub machine_id_source: MachineIdSource,
}
/// Skip = log warn, continue (the central read-only-/etc fix). §8 — not a bool pair.
pub enum WritePolicy { Write(std::path::PathBuf), Skip }
pub enum MachineIdSource { MachineId, Dmi, Serial, Override(String) }

pub struct Network {
    pub interface:       Option<String>,      // override; default = route-table detect
    pub config_method:   NetConfigMethod,
    pub dns_provisioning: DnsProvisioning,
}
pub enum NetConfigMethod { Ifupdown, Netplan, NetworkManager, None }
pub enum DnsProvisioning { SystemdResolved, ResolvConf(std::path::PathBuf), None }

pub struct Tls {
    pub root_source:    TlsRootSource,
    pub extra_ca_bundle: Option<std::path::PathBuf>,
}
pub enum TlsRootSource { Bundled, System, Merged }
```

### How and where it loads

`HostProfile::load()` is called **exactly once** in bootstrap, before any aggregate is built, and the result is parked on `Current`:

```rust
// On Current (code-standards §5: node self-description, immutable → Arc, no lock)
pub struct Current {
    pub stone: Arc<Stone>,
    pub host:  Arc<HostProfile>,   // NEW
    // ...existing...
}

// Handlers/infra needing only the profile declare it (code-standards §6):
impl FromRef<Moss> for Arc<HostProfile> {
    fn from_ref(s: &Moss) -> Self { s.current.host.clone() }
}
```

`ContainerRuntime::new()` takes `&HostProfile` so `docker/mod.rs:55` branches on `runtime.docker_socket` (and uses `connect_with_defaults()` to honor `DOCKER_HOST`). Call sites change from `env::var(...)` / `CONFIG_DIR` to field reads such as `state.current.host.runtime.docker_socket`.

**Env-prefix resolution (mandatory).** `HostProfile::load()` is the single place that resolves prefixes: read `ZG_*` first, fall back to `GARDEN_*` with a one-time deprecation warning, name **all new** knobs `ZG_*`. The `env.rs::keys` registry and `paths.rs` (currently 100% `GARDEN_*`) gain `ZG_*`-primary lookups as part of this work — otherwise the "single reader" reads the wrong prefix.

**Bootstrap ordering (resolves the chicken-and-egg).** The `paths` sub-profile is **env/heuristic-only and cannot depend on TOML**. Strict order:

1. Resolve `Platform` (explicit `ZG_HOST_PROFILE` wins; else heuristic).
2. Resolve `paths` from env + platform defaults → compute `config_dir()` (the **function**, never the `CONFIG_DIR` const).
3. Read `garden-moss.toml` from the resolved `paths.config`.
4. Merge the remaining namespaces from env + the TOML `[host]` block (env wins).

`MossConfig::load`/`save` (`config.rs:312,384`) must be migrated to the resolved `paths.config` in the same change, or the profile is read from the path it is trying to fix.

**Single-owner discipline (§14).** `paths.*` is a resolved cache of `garden_common::constants::paths::*`; pick one owner — `HostProfile.paths` becomes the sole source and `paths::*` becomes a thin shim, *or* callers keep calling `paths::*`. Do not maintain two copies. Discovery TTL has exactly one home: `DiscoveryConfig` — it is **not** in `HostProfile`.

---

## Per-platform default table

`Platform` resolves at load: explicit `ZG_HOST_PROFILE` wins; else heuristic (`/system/build.prop` or bionic → `Android`; `ZEN_GARDEN_CONTAINER` → `Minimal`; else `LinuxStandard`).

| Knob | linux-standard | android | minimal |
|---|---|---|---|
| `paths.config` | `/etc/zen-garden` | `/data/zen-garden` | `{data}` (single tree) |
| `paths.data` | `/var/lib/zen-garden` | `/data/zen-garden` | `/var/lib/zen-garden` |
| `paths.temp` | `/tmp` | `/data/local/tmp` *(may be `noexec`)* | `/tmp` |
| `paths.bin_install` | `/usr/local/bin` | `/data/zen-garden/bin` | `{data}/bin` |
| `paths.companions` | `{data}/companions` | `{data}/companions` | `{data}/companions` |
| `runtime.docker_socket` | none → `connect_with_defaults()` | `/data/docker.sock` | `DOCKER_HOST` / none |
| `runtime.image_pull_policy` | `Always` | `IfNotPresent` | `Never` (air-gapped) |
| `runtime.privilege_escalation` | `Sudo` | `Direct` (rooted) | `None` |
| `runtime.scheduler` | `Systemd` | `None` | `None` |
| `runtime.docker_restart_command` | `systemctl restart docker` | none | none |
| `runtime.container.privilege` | `ImageDefault` | `AmbientNetRaw` | `ImageDefault` |
| `runtime.container.network_mode` | `Bridge` | `Host` | `Bridge` |
| `identity.hostname` | `Write(/etc/hostname)` | `Skip` | `Skip` |
| `identity.hosts_file` | `Write(/etc/hosts)` | `Skip` | `Skip` |
| `identity.motd` | `Write(/etc/motd)` | `Skip` | `Skip` |
| `identity.machine_id_source` | `MachineId` | `Serial` / `Override` | `MachineId` |
| `network.interface` | none (route detect) | `wlan0` (override) | none |
| `network.config_method` | auto (ifupdown/netplan) | `None` | `None` |
| `network.dns_provisioning` | `SystemdResolved` | `None` | `None` |
| `tls.root_source` | `Bundled` | `Bundled` | `Merged` (internal CA likely) |

**Override safety:** `runtime.privilege_escalation` is auto-overridden to `Direct` whenever `geteuid()==0`, regardless of profile — a mis-set profile must not silently break every mount. Every `WritePolicy::Skip` and every privilege auto-override logs at WARN so a wrong profile is diagnosable, not silent.

**Caveats (no knob, document only):**
- Timestamps stay UTC (`SystemTime`/`Instant` throughout). Do **not** introduce `chrono::Local` — Android lacks `/etc/localtime` and some libc paths panic.
- `/data/local/tmp` is frequently `noexec` on hardened ROMs. Anything staged-then-executed must use `paths.bin_install`, never `paths.temp`.
- `RLIMIT_NOFILE` is low on bionic (~1024). A future `runtime.nofile_limit` calling `setrlimit` is deferred until the S3 gateway / many-container fd count actually pushes it.

---

## Bugs to fix, not configure

These must be removed from the config surface and filed as straight fixes:

1. **`DOCKER_HOST` support** (`docker/mod.rs:55`). `connect_with_socket_defaults()` ignores `DOCKER_HOST`; `connect_with_defaults()` honors it. Fix = swap the API call; do **not** reimplement `DOCKER_HOST` parsing in `HostProfile`. Knob = explicit socket path only.
2. **`atomic_write` temp-name collision** (`persistence.rs:237`). `path.with_extension("tmp")` mangles meaningful extensions and risks collision between concurrent writers sharing a stem. Fix = `{filename}.{pid}.tmp` or `tempfile::NamedTempFile::new_in(dir)` + `persist`. Out of profile scope.
3. **Fatal `/proc/cpuinfo` read** (`system.rs:26`). `.context(...)?` aborts all capability collection if unreadable. Fix = degrade (`model_name="Unknown"`, `features=[]`, arch from `std::env::consts::ARCH`, which needs no file). A `cpuinfo_path` knob neither fixes this nor is justified — tests use a trait/fixture, not a prod field.
4. **Interface detection** (`network/mod.rs:357-387`). The hardcoded candidate list + dir-scan is a heuristic. Fix = route-table primary detection (`/proc/net/route` / `ip route`) with `network.interface` as override. Do **not** promote `interface_candidates` to config — that is configuring around a bug.
5. **`companions_dir()` fallback** (`paths.rs:169-182`). `/usr/local/bin/companions` is read-only on Android and wrong on every non-installer host. Fix = change the fallback to `{data}/companions`. No config field warranted.

---

## Ranked implementation plan

Tiers are value/effort-ordered; each Tier-1 item is independently shippable.

### Tier 1 — quick fixes, no new struct, unblock Android boot

| # | Change | Severity | Effort |
|---|---|---|---|
| 1 | `CONFIG_DIR` const → `config_dir()` fn at **all** callers — **including `config.rs:312,384`** (the most important two), plus persistence/hardware/task_store/nurturing_store/console | Critical | S |
| 2 | `network/state.rs:15` const → `config_dir()`-derived fn | Medium | S |
| 3 | `manifests/registry.rs:41` + `manifests/hw.rs:19` consts → fns | Medium | S |
| 4 | `companions_dir()` fallback → `{data}/companions` (plain fix) | High | XS |
| 5 | `docker/mod.rs:55` → `connect_with_defaults()` (honors `DOCKER_HOST`) + explicit-socket fallback | Critical | S |
| 6 | `identity` write tolerance: split `hostname`/`hosts_file`/`motd` into `Skip`-capable paths + Linux `hostname`-cmd read fallback (`tty.rs:399,445,466,511,694`) | Critical | M |
| 7 | Route bare clients through `http::client_builder()` (discovery/offering_capabilities/registry_client) | Medium | S |
| 8 | `geteuid()==0` auto-detect → `Direct` privilege escalation (`storage/subprocess.rs:72`, `storage/platform.rs:91`) | Critical | S |
| 9 | Reclassified bug fixes: `atomic_write` temp name; fatal `/proc/cpuinfo` → degrade; route-table interface detection | High | M |

**Tier 1 alone makes Moss boot and run containers on the phone with env vars only** — the "configured, not patched" floor.

### Tier 2 — the typed `HostProfile` + FromRef

| # | Change | Effort |
|---|---|---|
| 10 | Create `infra/host_profile.rs`: namespaced struct + `load()` + `Platform` resolution + **`ZG_*`-then-`GARDEN_*` prefix resolution** | M |
| 11 | Specify and implement bootstrap order (env-only `paths` → `config_dir()` → TOML → merge); migrate `env.rs::keys` + `paths.rs` to `ZG_*`-primary | M |
| 12 | Add `host: Arc<HostProfile>` to `Current`; `FromRef<Moss> for Arc<HostProfile>` | S |
| 13 | Migrate Tier-1 env reads to read from `HostProfile` fields (single reader) | M |
| 14 | `network.config_method` (incl. `None` disabling ifupdown/netplan **and** their DNS sub-steps); `network.dns_provisioning` gating `provision.rs:296` | M |
| 15 | `tls.root_source` + `extra_ca_bundle` in `webpki_tls_config()` | M |

### Tier 3 — container security context + privileged-host ops

| # | Change | Effort |
|---|---|---|
| 16 | `runtime.container.privilege: ContainerPrivilege` as the single posture knob expanding to bollard fields in `build_container_config` (`lifecycle.rs:540-557`); `cap_add`/`cap_drop` only via `ContainerAdvanced` escape hatch | L |
| 17 | `runtime.privilege_escalation` enum at remaining sudo call sites; `runtime.scheduler` at `timer.rs:331` | L |
| 18 | `image_pull_policy`, `docker_config_path`, `docker_restart_command` (air-gapped / non-systemd docker) | M |
| 19 | `paths.temp`, `paths.bin_install`, `identity.machine_id_source` | M |

---

## Paranoid-network run-as-root mechanism

`CONFIG_ANDROID_PARANOID_NETWORK` gates `AF_INET`/`AF_INET6` socket creation on membership in GIDs `AID_INET` (3003) / `AID_NET_RAW` (3004). The naive fix — set container `user=0` (`lifecycle.rs:557`) — is **insufficient and actively wrong** for stateful offerings: mongo, postgres, redis, mysql start as root only to `chown`, then **drop to a service user via `gosu`/`su-exec`/`setpriv`**. After the drop the process is not root and not in `inet`/`net_raw`, so its listen socket fails `EACCES`. `user=0` is silently undone by the image's own entrypoint.

`runtime.container.privilege: ContainerPrivilege` is the single posture knob, applied in `build_container_config`. Variants, ranked:

1. **`AmbientNetRaw`** (default for `Android`). Sets ambient `CAP_NET_RAW`/`CAP_NET_ADMIN` (which survive the `setuid`/`gosu` drop, unlike permitted/effective caps cleared on uid change) plus `group_add: ["3003","3004"]`. Fixes gosu-dropping images without forcing root, preserving the image's own security model. Expands to bollard `HostConfig.cap_add = ["NET_RAW","NET_ADMIN"]` + `group_add` + (where supported) the ambient set.
2. **`HostNetwork`**. Uses the host netns where Moss already has inet access. Simple, but loses port-mapping isolation and collides on ports.
3. **`Privileged`**. Blunt escape hatch; breaks gosu-dropping images and violates least-privilege (security.md). Never a default.
4. **Kernel without `CONFIG_ANDROID_PARANOID_NETWORK`** (or eBPF `bpf-paranoid` shim). Cleanest at host level, but out of Moss's control and not portable to "another phone" — a **deployment recommendation**, not a runtime knob.

Exposing `cap_add`/`cap_drop` as peers of `privilege` would re-introduce representable-impossible postures (§8) — they live only under `ContainerAdvanced` as an explicit override, never as co-equal fields.

---

## ADR draft

```markdown
---
audience: developer
doc_type: decision
status: proposed
---

# HOST-0001: Typed Host/Runtime Profile

**Date**: 2026-06-07
**Status**: Proposed

## Problem

Moss was ported to a rooted LineageOS phone (read-only `/` and `/etc`, `/etc`
symlinked to `/system/etc`, no systemd, bionic libc, no system CA store, kernel
with `CONFIG_ANDROID_PARANOID_NETWORK`, only `/data` writable). Roughly ten host
assumptions broke: `connect_with_socket_defaults()` that ignores `DOCKER_HOST`
(`src/moss/src/docker/mod.rs:55`); `garden_common::constants::CONFIG_DIR` consts
that bypass the env-honoring `paths::config_dir()` — most critically in
`MossConfig::load`/`save` (`src/moss/src/infra/config.rs:312,384`), plus
`persistence.rs:11`, `hardware.rs:124`, `task_store.rs:34`, `network/state.rs:15`,
`manifests/registry.rs:41`, `manifests/hw.rs:19`; writes to read-only
`/etc/hostname`, `/etc/hosts`, `/etc/motd` (`tty.rs:399,466,694`); `sudo` that does
not exist on rooted Android (`storage/subprocess.rs:72`); a forced
`apt install systemd-resolved` (`installer/provision.rs:296`); `eth0`/`wlan0`
interface heuristics (`network/mod.rs:363`); and containers unable to open
`AF_INET` sockets under paranoid-network unless granted ambient net caps.

Two related fixes already landed and must not be redone: `http.rs:20-38` loads
bundled webpki roots via `client_builder()`, and `resources/system.rs:30` keeps
`architecture` canonical from `std::env::consts::ARCH` (device-tree only fills
`model_name`).

The env vars and `paths::*` helpers needed to fix most of this already exist. The
defect is that they are read inconsistently — compile-time consts and scattered
`env::var()` calls bypass them, and the documented-primary `ZG_*` prefix in fact
works nowhere (`env.rs` keys and `paths.rs` are 100% `GARDEN_*`). A future
non-standard host should be configured, not patched.

## Decision

Introduce one typed value object, `HostProfile`
(`src/moss/src/infra/host_profile.rs`), loaded once at startup from env plus an
optional `[host]` block in `garden-moss.toml`, parked on `Current` as
`Arc<HostProfile>` (immutable, no lock — code-standards §5), consumed via
`FromRef<Moss> for Arc<HostProfile>`.

Knobs group into namespaces (§1, §3): `paths`, `runtime`, `identity`, `network`,
`tls`. Multi-value state is an enum, never a bool pair (§8): `WritePolicy`,
`ImagePullPolicy`, `NetConfigMethod`, `DnsProvisioning`, `PrivilegeMode`,
`Scheduler`, `ContainerPrivilege`, `TlsRootSource`. The container security posture
is a single `ContainerPrivilege` enum that expands into bollard fields; raw
caps are an `Advanced` escape hatch, not co-equal fields.

`HostProfile::load()` resolves prefixes `ZG_*`-first / `GARDEN_*`-fallback (warn on
legacy) and names all new knobs `ZG_*`. It resolves a `Platform`
(`LinuxStandard` | `Android` | `Minimal`) — `ZG_HOST_PROFILE` overrides heuristic
detection — selecting per-knob defaults. Bootstrap order is strict and acyclic:
env-only `paths` → `config_dir()` (the function) → read TOML from the resolved dir
→ merge remaining namespaces. `paths` cannot depend on TOML.

Several apparent knobs are reclassified as bugs and fixed directly: `DOCKER_HOST`
support (`connect_with_defaults()`), fatal `/proc/cpuinfo` read (degrade), the
interface-candidate heuristic (route-table detection + override), the `atomic_write`
temp-name collision, and the `companions_dir()` fallback.

The discovery transport keeps its own `DiscoveryConfig::from_env()`
(`p2p.rs:114`); `ZG_DISCOVERY_MCAST_TTL` joins that reader, not `HostProfile`. For
paranoid-network the `Android` default is `ContainerPrivilege::AmbientNetRaw`.

## Consequences

### Positive
- One typed reader of host config; call sites read a field, not `env::var()`.
- New hosts ship a profile (env or `[host]` toml), no source patches.
- Impossible states unrepresentable (enums over bool pairs; single privilege posture).
- Read-only `/etc` becomes a configured `Skip` with a warning, not a boot failure.
- `geteuid()==0` auto-detect prevents a mis-set profile from breaking mounts.
- `tls.root_source = Merged` + `ZG_CA_BUNDLE_PATH` unblocks corporate/air-gapped
  hosts without weakening Android's bundled-roots default.
- `ZG_*` finally works; the legacy `GARDEN_*` prefix is retired on a warn path.

### Negative
- One more aggregate field and a load step in bootstrap.
- Migration touches many call sites (staged across Tiers 1-3).
- A wrong profile can silently degrade (e.g. `Skip` on a host that could write
  `/etc/hostname`) — mitigated by logging every `Skip` and every auto-override.

### Alternatives considered
- **Per-knob `env::var()` at each site** (status quo): cheapest per change, but
  re-creates the inconsistency this ADR removes; no single source of truth.
- **Pass full `Moss` to every consumer**: violates §6 (handlers declare minimal deps).
- **`#[cfg(target_os)]` branches per assumption**: compile-time, cannot configure a
  deployed binary; another phone / Pi / minimal box would need a rebuild.
- **External config crate (config-rs/figment)**: heavier dep; the typed struct +
  existing `EnvConfig`/`paths` cover the need.
- **`user=0` for paranoid-network**: silently undone by gosu-dropping database
  entrypoints; rejected in favor of ambient caps + supplementary GIDs.
```
```

---

Merged document is ready to drop into `docs/` (suggested path `docs/decisions/HOST-0001-host-runtime-profile.md` for the ADR, with the design body as `docs/specs/host-runtime-profile.md` or a single combined file). It integrates the critique's three load-bearing corrections (ZG_/GARDEN_ prefix resolution, acyclic bootstrap ordering with `config.rs:312,384` in Tier-1, and the five bug-reclassifications), cuts the over-engineered `hardware` namespace and the misplaced `discovery.mcast_ttl`, folds raw caps under a single `ContainerPrivilege` enum, and preserves both stale-finding corrections — all verified against code (file:line cited inline).
