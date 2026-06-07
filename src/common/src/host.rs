//! Host profile — the single typed reader of this node's platform configuration.
//!
//! > One thing reads the config; everybody else reads from the thing.
//!
//! Moss assumes a conventional Linux host in dozens of places (paths, the Docker
//! socket, writable `/etc`, a system CA store, a route to a registry, systemd).
//! Each of those is read inconsistently today — compile-time consts, scattered
//! `env::var()`, bare client builders — so a non-standard host (a phone, a Pi, a
//! minimal/air-gapped box) has to be *patched* rather than *configured*.
//!
//! `HostProfile` is the one value object that resolves every such assumption,
//! **once**, at startup. It is immutable for the process lifetime and shared
//! behind `Arc` with no lock (code-standards §5: node self-description). Every
//! consumer — in `garden-common` and in `moss` — reads from [`profile()`]
//! instead of reading the environment itself. Moss additionally exposes it via
//! `FromRef<Moss> for Arc<HostProfile>` for explicit handler dependencies.
//!
//! The struct *is* the namespace (code-standards §1): `profile().paths.config`,
//! `profile().runtime.docker_socket`, `profile().identity.hostname`. State that
//! has more than two valid shapes is an enum, never a bool pair (§8).
//!
//! Env-prefix policy: the documented-primary `ZG_*` prefix is read first; the
//! legacy `GARDEN_*` name is honored as a deprecated fallback with a one-time
//! warning (load runs once, so the warning is naturally one-time per key).

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

// ============================================================================
// HostProfile — the aggregate
// ============================================================================

/// What this host *is* and *permits*. Resolved once from env + platform
/// defaults; immutable for the process lifetime.
#[derive(Clone, Debug)]
pub struct HostProfile {
    /// Resolved platform identity (drives every default below).
    pub platform: Platform,
    pub paths: Paths,
    pub runtime: Runtime,
    pub identity: Identity,
    pub network: Network,
    pub tls: Tls,
}

impl HostProfile {
    /// Resolve the profile from the environment and platform defaults.
    ///
    /// Pure (no I/O beyond reading env + a couple of marker-file `exists()`
    /// probes during platform detection), so it is safe to call early in
    /// bootstrap before any aggregate is built.
    pub fn from_env() -> Self {
        let platform = Platform::detect();
        Self {
            platform,
            paths: Paths::from_env(platform),
            runtime: Runtime::from_env(platform),
            identity: Identity::from_env(platform),
            network: Network::from_env(platform),
            tls: Tls::from_env(platform),
        }
    }
}

// ============================================================================
// Platform
// ============================================================================

/// The class of host Moss is running on. Selects every default in the profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    /// A conventional Linux host: writable `/etc` & `/var`, systemd, system CA store.
    LinuxStandard,
    /// A rooted Android device (LineageOS phone Stone): read-only `/` & `/etc`,
    /// no systemd, bionic libc, only `/data` writable.
    Android,
    /// A minimal/containerized/air-gapped host: single writable tree, no systemd,
    /// no registry route.
    Minimal,
}

impl Platform {
    fn detect() -> Platform {
        if let Some(v) = env_var("ZG_HOST_PROFILE", "GARDEN_HOST_PROFILE") {
            match v.to_ascii_lowercase().as_str() {
                "android" => return Platform::Android,
                "minimal" => return Platform::Minimal,
                "linux" | "linux-standard" | "standard" => return Platform::LinuxStandard,
                other => {
                    tracing::warn!(value = other, "unknown ZG_HOST_PROFILE; falling back to heuristic")
                }
            }
        }
        // Android markers (only meaningful on linux-family targets).
        #[cfg(target_os = "linux")]
        {
            if std::path::Path::new("/system/build.prop").exists()
                || std::path::Path::new("/system/bin/linker64").exists()
            {
                return Platform::Android;
            }
        }
        if std::env::var("ZG_CONTAINER").is_ok() || std::env::var("ZEN_GARDEN_CONTAINER").is_ok() {
            return Platform::Minimal;
        }
        Platform::LinuxStandard
    }
}

// ============================================================================
// paths
// ============================================================================

/// Filesystem locations this host writes to. Defaults follow `platform`; each is
/// individually overridable by env. The single source these resolve from is
/// here — `constants::paths::*` becomes a thin reader of this profile.
#[derive(Clone, Debug)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub temp: PathBuf,
    pub bin_install: PathBuf,
    pub companions: PathBuf,
    pub network_state: PathBuf,
}

impl Paths {
    fn from_env(p: Platform) -> Self {
        let data = env_path("ZG_DATA_DIR", "GARDEN_DATA_DIR").unwrap_or_else(|| {
            PathBuf::from(match p {
                Platform::Android => "/data/zen-garden",
                Platform::LinuxStandard | Platform::Minimal => "/var/lib/zen-garden",
            })
        });
        let config = env_path("ZG_CONFIG_DIR", "GARDEN_CONFIG_DIR").unwrap_or_else(|| match p {
            Platform::Android => PathBuf::from("/data/zen-garden/config"),
            Platform::Minimal => data.join("config"),
            Platform::LinuxStandard => PathBuf::from("/etc/zen-garden"),
        });
        let temp = env_path("ZG_TEMP_DIR", "TMPDIR").unwrap_or_else(|| {
            PathBuf::from(match p {
                Platform::Android => "/data/local/tmp",
                Platform::LinuxStandard | Platform::Minimal => "/tmp",
            })
        });
        let bin_install =
            env_path("ZG_BIN_INSTALL_DIR", "GARDEN_BIN_INSTALL_DIR").unwrap_or_else(|| match p {
                Platform::Android => PathBuf::from("/data/zen-garden/bin"),
                Platform::Minimal => data.join("bin"),
                Platform::LinuxStandard => PathBuf::from("/usr/local/bin"),
            });
        let companions = env_path("ZG_COMPANIONS_DIR", "GARDEN_COMPANIONS_DIR")
            .or_else(|| std::env::var("GARDEN_companions_dir").ok().map(PathBuf::from))
            .unwrap_or_else(|| data.join("companions"));
        let network_state = config.join("network-state.json");
        Self {
            config,
            data,
            temp,
            bin_install,
            companions,
            network_state,
        }
    }
}

// ============================================================================
// runtime — container/Docker execution context
// ============================================================================

#[derive(Clone, Debug)]
pub struct Runtime {
    /// Explicit Docker socket path. `None` → let bollard honor `DOCKER_HOST`
    /// (`connect_with_defaults`).
    pub docker_socket: Option<PathBuf>,
    pub image_pull_policy: ImagePullPolicy,
    pub privilege_escalation: PrivilegeMode,
    pub scheduler: Scheduler,
    pub container: Container,
}

/// What to do before creating a container that already exists locally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImagePullPolicy {
    /// Always pull, fail if unreachable.
    Always,
    /// Pull, but fall back to a locally-present image if the pull fails.
    IfNotPresent,
    /// Never pull; require the image to be present (air-gapped).
    Never,
}

/// How Moss runs privileged host commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivilegeMode {
    /// Prefix with `sudo`.
    Sudo,
    /// Run directly (already root, e.g. rooted Android).
    Direct,
    /// Privileged host ops are unavailable.
    None,
}

/// The host's task scheduler / service supervisor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheduler {
    Systemd,
    Cron,
    None,
}

/// Security posture for offering containers.
#[derive(Clone, Debug)]
pub struct Container {
    pub privilege: ContainerPrivilege,
    pub user: Option<String>,
    pub network_mode: NetworkMode,
    pub bind_address: IpAddr,
    pub restart_policy: RestartPolicy,
}

/// Single posture knob (§8): one enum expands to the right bollard fields, so
/// impossible combinations (e.g. Privileged + cap_drop=ALL) are unrepresentable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerPrivilege {
    /// Use the image's own user/caps (the correct default on a kernel without
    /// `CONFIG_ANDROID_PARANOID_NETWORK`).
    ImageDefault,
    /// Grant ambient `CAP_NET_RAW`/`CAP_NET_ADMIN` + `group_add 3003/3004` so a
    /// non-root (gosu-dropped) process can still open sockets under
    /// paranoid-network. Interim for un-patched Android kernels.
    AmbientNetRaw,
    /// Share the host network namespace.
    HostNetwork,
    /// Fully privileged. Escape hatch only; never a default.
    Privileged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkMode {
    Bridge,
    Host,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestartPolicy {
    No,
    OnFailure,
    Always,
    UnlessStopped,
}

impl Runtime {
    fn from_env(p: Platform) -> Self {
        let docker_socket = env_path("ZG_DOCKER_SOCKET", "GARDEN_DOCKER_SOCKET").or_else(|| {
            match p {
                Platform::Android => Some(PathBuf::from("/data/docker/docker.sock")),
                Platform::LinuxStandard | Platform::Minimal => None,
            }
        });
        let image_pull_policy = match env_lower("ZG_IMAGE_PULL_POLICY", "GARDEN_IMAGE_PULL_POLICY")
            .as_deref()
        {
            Some("always") => ImagePullPolicy::Always,
            Some("ifnotpresent") | Some("if-not-present") | Some("local") => {
                ImagePullPolicy::IfNotPresent
            }
            Some("never") => ImagePullPolicy::Never,
            _ => match p {
                Platform::LinuxStandard => ImagePullPolicy::Always,
                Platform::Android => ImagePullPolicy::IfNotPresent,
                Platform::Minimal => ImagePullPolicy::Never,
            },
        };
        let privilege_escalation =
            match env_lower("ZG_PRIVILEGE_ESCALATION", "GARDEN_PRIVILEGE_ESCALATION").as_deref() {
                Some("sudo") => PrivilegeMode::Sudo,
                Some("direct") => PrivilegeMode::Direct,
                Some("none") => PrivilegeMode::None,
                _ => match p {
                    Platform::LinuxStandard => PrivilegeMode::Sudo,
                    Platform::Android => PrivilegeMode::Direct,
                    Platform::Minimal => PrivilegeMode::None,
                },
                // Safety net: callers that shell out with `sudo` must additionally
                // downgrade to direct execution when `geteuid()==0` at runtime, so a
                // mis-set profile never breaks host ops. That check lives at the
                // consumption site in moss (which links libc), not here.
            };
        let scheduler = match env_lower("ZG_SCHEDULER", "GARDEN_SCHEDULER").as_deref() {
            Some("systemd") => Scheduler::Systemd,
            Some("cron") => Scheduler::Cron,
            Some("none") => Scheduler::None,
            _ => match p {
                Platform::LinuxStandard => Scheduler::Systemd,
                Platform::Android | Platform::Minimal => Scheduler::None,
            },
        };
        Self {
            docker_socket,
            image_pull_policy,
            privilege_escalation,
            scheduler,
            container: Container::from_env(p),
        }
    }
}

impl Container {
    fn from_env(p: Platform) -> Self {
        let privilege = match env_lower("ZG_CONTAINER_PRIVILEGE", "GARDEN_CONTAINER_PRIVILEGE")
            .as_deref()
        {
            Some("imagedefault") | Some("image-default") | Some("default") => {
                ContainerPrivilege::ImageDefault
            }
            Some("ambientnetraw") | Some("ambient-net-raw") | Some("ambient") => {
                ContainerPrivilege::AmbientNetRaw
            }
            Some("hostnetwork") | Some("host-network") | Some("host") => {
                ContainerPrivilege::HostNetwork
            }
            Some("privileged") => ContainerPrivilege::Privileged,
            // Default ImageDefault everywhere: a Stone kernel is expected to be
            // built WITHOUT CONFIG_ANDROID_PARANOID_NETWORK (HOST-0001). Operators
            // on an un-patched Android kernel set ZG_CONTAINER_PRIVILEGE=ambient.
            _ => ContainerPrivilege::ImageDefault,
        };
        let user = env_var("ZG_CONTAINER_USER", "GARDEN_CONTAINER_USER");
        let network_mode = match env_lower("ZG_CONTAINER_NETWORK", "GARDEN_CONTAINER_NETWORK")
            .as_deref()
        {
            Some("host") => NetworkMode::Host,
            Some("bridge") => NetworkMode::Bridge,
            // Android's per-network policy routing leaves the host unable to route to
            // docker0 containers, so bridge + published ports (-p) are unreachable from the
            // LAN even with NAT enabled. Host networking binds each offering's ports
            // directly on the host stack — every present and future port is LAN-reachable
            // with no per-offering config. Conventional hosts keep bridge isolation + -p.
            _ => match p {
                Platform::Android => NetworkMode::Host,
                Platform::LinuxStandard | Platform::Minimal => NetworkMode::Bridge,
            },
        };
        let bind_address = env_var("ZG_CONTAINER_BIND_ADDRESS", "GARDEN_CONTAINER_BIND_ADDRESS")
            .and_then(|s| s.parse().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        let restart_policy = match env_lower("ZG_CONTAINER_RESTART", "GARDEN_CONTAINER_RESTART")
            .as_deref()
        {
            Some("no") | Some("none") => RestartPolicy::No,
            Some("on-failure") | Some("onfailure") => RestartPolicy::OnFailure,
            Some("always") => RestartPolicy::Always,
            Some("unless-stopped") | Some("unlessstopped") => RestartPolicy::UnlessStopped,
            _ => match p {
                // Persistent always-on Stones (incl. an Android phone Stone) keep offerings
                // up across reboots — unless-stopped guarantees dockerd restarts them on
                // every daemon start, not just on a non-clean exit.
                Platform::LinuxStandard | Platform::Android => RestartPolicy::UnlessStopped,
                // Minimal/ephemeral hosts have no supervisor; don't restart-loop a failure.
                Platform::Minimal => RestartPolicy::No,
            },
        };
        Self {
            privilege,
            user,
            network_mode,
            bind_address,
            restart_policy,
        }
    }
}

// ============================================================================
// identity — host file write policy (read-only /etc)
// ============================================================================

#[derive(Clone, Debug)]
pub struct Identity {
    pub hostname: WritePolicy,
    pub hosts_file: WritePolicy,
    pub motd: WritePolicy,
}

/// Whether a host identity file is writable here, and where. `Skip` = log a
/// warning and continue (the central read-only-`/etc` fix). §8 — not a bool pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WritePolicy {
    Write(PathBuf),
    Skip,
}

impl Identity {
    fn from_env(p: Platform) -> Self {
        let writable = matches!(p, Platform::LinuxStandard);
        Self {
            hostname: write_policy("ZG_HOSTNAME_FILE", "GARDEN_HOSTNAME_FILE", "/etc/hostname", writable),
            hosts_file: write_policy("ZG_HOSTS_FILE", "GARDEN_HOSTS_FILE", "/etc/hosts", writable),
            motd: write_policy("ZG_MOTD_FILE", "GARDEN_MOTD_FILE", "/etc/motd", writable),
        }
    }
}

fn write_policy(zg: &str, garden: &str, default_path: &str, writable_by_default: bool) -> WritePolicy {
    match env_var(zg, garden).as_deref() {
        Some("skip") | Some("none") => WritePolicy::Skip,
        Some(path) => WritePolicy::Write(PathBuf::from(path)),
        None => {
            if writable_by_default {
                WritePolicy::Write(PathBuf::from(default_path))
            } else {
                WritePolicy::Skip
            }
        }
    }
}

// ============================================================================
// network
// ============================================================================

#[derive(Clone, Debug)]
pub struct Network {
    /// Explicit primary interface override; `None` → detect at runtime.
    pub interface: Option<String>,
    pub config_method: NetConfigMethod,
    pub dns_provisioning: DnsProvisioning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetConfigMethod {
    /// Detect ifupdown/netplan/NetworkManager at runtime.
    Auto,
    Ifupdown,
    Netplan,
    NetworkManager,
    /// Do not provision host network config (Android/minimal).
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DnsProvisioning {
    SystemdResolved,
    ResolvConf(PathBuf),
    None,
}

impl Network {
    fn from_env(p: Platform) -> Self {
        let interface = env_var("ZG_NETWORK_INTERFACE", "GARDEN_NETWORK_INTERFACE");
        let config_method = match env_lower("ZG_NET_CONFIG_METHOD", "GARDEN_NET_CONFIG_METHOD")
            .as_deref()
        {
            Some("auto") => NetConfigMethod::Auto,
            Some("ifupdown") => NetConfigMethod::Ifupdown,
            Some("netplan") => NetConfigMethod::Netplan,
            Some("networkmanager") | Some("network-manager") => NetConfigMethod::NetworkManager,
            Some("none") => NetConfigMethod::None,
            _ => match p {
                Platform::LinuxStandard => NetConfigMethod::Auto,
                Platform::Android | Platform::Minimal => NetConfigMethod::None,
            },
        };
        let dns_provisioning = match env_var("ZG_DNS_PROVISIONING", "GARDEN_DNS_PROVISIONING")
            .as_deref()
        {
            Some("systemd-resolved") | Some("systemd") => DnsProvisioning::SystemdResolved,
            Some("none") => DnsProvisioning::None,
            Some(path) if path.starts_with('/') => DnsProvisioning::ResolvConf(PathBuf::from(path)),
            _ => match p {
                Platform::LinuxStandard => DnsProvisioning::SystemdResolved,
                Platform::Android | Platform::Minimal => DnsProvisioning::None,
            },
        };
        Self {
            interface,
            config_method,
            dns_provisioning,
        }
    }
}

// ============================================================================
// tls
// ============================================================================

#[derive(Clone, Debug)]
pub struct Tls {
    pub root_source: TlsRootSource,
    pub extra_ca_bundle: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsRootSource {
    /// Bundled webpki roots only (host-trust-store independent).
    Bundled,
    /// The OS trust store.
    System,
    /// Bundled + extra CA bundle / system.
    Merged,
}

impl Tls {
    fn from_env(p: Platform) -> Self {
        let root_source = match env_lower("ZG_TLS_ROOT_SOURCE", "GARDEN_TLS_ROOT_SOURCE").as_deref()
        {
            Some("bundled") => TlsRootSource::Bundled,
            Some("system") => TlsRootSource::System,
            Some("merged") => TlsRootSource::Merged,
            _ => match p {
                // Conventional Linux trusts the OS store (corporate proxies / custom PKI).
                // Defaulting it to bundled-only would silently drop the system CA.
                Platform::LinuxStandard => TlsRootSource::System,
                // Android/bionic has no system CA store — bundled webpki roots.
                Platform::Android => TlsRootSource::Bundled,
                // Minimal/air-gapped: bundled roots (+ optional extra_ca_bundle).
                Platform::Minimal => TlsRootSource::Merged,
            },
        };
        let extra_ca_bundle = env_path("ZG_EXTRA_CA_BUNDLE", "GARDEN_EXTRA_CA_BUNDLE");
        Self {
            root_source,
            extra_ca_bundle,
        }
    }
}

// ============================================================================
// Shared accessor — "everybody reads from the thing"
// ============================================================================

static PROFILE: OnceLock<Arc<HostProfile>> = OnceLock::new();

/// The process-wide host profile. Lazily resolved from the environment on first
/// access if [`init`] was not called first. Cheap to call (an `Arc` clone).
pub fn profile() -> Arc<HostProfile> {
    PROFILE
        .get_or_init(|| Arc::new(HostProfile::from_env()))
        .clone()
}

/// Install an explicitly-built profile (e.g. with TOML overrides) before first
/// access. Returns `false` if the profile was already resolved (the lazy path or
/// a prior `init` won the race) — call this first thing in bootstrap.
pub fn init(p: HostProfile) -> bool {
    PROFILE.set(Arc::new(p)).is_ok()
}

// ============================================================================
// env helpers — ZG_* primary, GARDEN_* deprecated fallback
// ============================================================================

/// Read a host-config env var: `ZG_*` first, `GARDEN_*` as a deprecated fallback
/// (warned once — `from_env` runs a single time). Empty values are treated as unset.
fn env_var(zg_key: &str, garden_key: &str) -> Option<String> {
    if let Ok(v) = std::env::var(zg_key) {
        if !v.is_empty() {
            return Some(v);
        }
    }
    if let Ok(v) = std::env::var(garden_key) {
        if !v.is_empty() {
            tracing::warn!(
                preferred = zg_key,
                deprecated = garden_key,
                "host config read from deprecated env var; prefer the ZG_ prefix"
            );
            return Some(v);
        }
    }
    None
}

fn env_path(zg_key: &str, garden_key: &str) -> Option<PathBuf> {
    env_var(zg_key, garden_key).map(PathBuf::from)
}

fn env_lower(zg_key: &str, garden_key: &str) -> Option<String> {
    env_var(zg_key, garden_key).map(|v| v.to_ascii_lowercase())
}

// ============================================================================
// tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_defaults_land_on_data() {
        let p = HostProfile {
            platform: Platform::Android,
            paths: Paths::from_env(Platform::Android),
            runtime: Runtime::from_env(Platform::Android),
            identity: Identity::from_env(Platform::Android),
            network: Network::from_env(Platform::Android),
            tls: Tls::from_env(Platform::Android),
        };
        assert_eq!(p.paths.config, PathBuf::from("/data/zen-garden/config"));
        assert_eq!(p.paths.data, PathBuf::from("/data/zen-garden"));
        assert_eq!(p.identity.hostname, WritePolicy::Skip);
        assert_eq!(p.runtime.scheduler, Scheduler::None);
        assert_eq!(p.runtime.image_pull_policy, ImagePullPolicy::IfNotPresent);
        assert_eq!(p.network.config_method, NetConfigMethod::None);
    }

    #[test]
    fn linux_standard_preserves_conventional_paths() {
        let paths = Paths::from_env(Platform::LinuxStandard);
        // Only assert when no env override is present (CI hygiene).
        if std::env::var_os("ZG_CONFIG_DIR").is_none()
            && std::env::var_os("GARDEN_CONFIG_DIR").is_none()
        {
            assert_eq!(paths.config, PathBuf::from("/etc/zen-garden"));
        }
        let id = Identity::from_env(Platform::LinuxStandard);
        assert!(matches!(id.motd, WritePolicy::Write(_)));
    }

    #[test]
    fn write_policy_skip_keyword() {
        assert_eq!(
            write_policy("ZG_NONEXISTENT_TEST_KEY", "GARDEN_NONEXISTENT_TEST_KEY", "/etc/x", false),
            WritePolicy::Skip
        );
    }
}
