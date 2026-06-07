/// DNS, env, and host configuration injected into every managed container.
pub(crate) struct ContainerNetworking {
    /// DNS servers (bridge gateway for resolved, plus fallback).
    pub dns: Vec<String>,
    /// DNS search domains. Empty by default; a manifest may set offering-specific
    /// search domains in the future.
    pub dns_search: Vec<String>,
    /// Extra host mappings (e.g., `host.docker.internal:<ip>`).
    pub extra_hosts: Vec<String>,
    /// Auto-injected environment variables (`KOI_ENDPOINT`, `GARDEN_STONE_ENDPOINT`, etc.).
    pub env_inject: Vec<String>,
}

/// Full specification for creating a managed container.
///
/// Replaces positional parameters in `install_service` / `upgrade_service`
/// and serves as the bridge between the config composition engine and Docker.
#[derive(Debug, Clone, Default)]
pub struct ContainerSpec {
    /// Docker image (e.g., "mongo:7").
    pub image: String,
    /// Container command override. `None` = use image default CMD.
    pub command: Option<Vec<String>>,
    /// Port mappings (host_port, container_port).
    pub ports: Vec<(u16, u16)>,
    /// Environment variables in `KEY=VALUE` format.
    pub environment: Vec<String>,
    /// Volume mounts (host_path, container_path).
    pub volumes: Vec<(String, String)>,
    /// Config file mappings from the manifest template.
    /// At install time, empty config files are created on the host and bind-mounted
    /// into the container. Config patches write content to these files and restart.
    pub config_files: Vec<garden_common::manifests::offering::ConfigFileMapping>,
    /// GPU device requests from manifest `deploy.resources.reservations.devices`.
    pub device_requests: Vec<garden_common::manifests::GpuDeviceRequest>,
    /// Memory limit in bytes (OFFER-0009) → bollard `HostConfig.memory`.
    pub memory_bytes: Option<i64>,
    /// CPU limit in nano-CPUs (OFFER-0009) → bollard `HostConfig.nano_cpus`.
    pub nano_cpus: Option<i64>,
    /// Container healthcheck (OFFER-0009) → bollard `ContainerCreateBody.healthcheck`.
    pub healthcheck: Option<garden_common::manifests::offering::ContainerHealthcheck>,
}

impl ContainerSpec {
    /// Return the effective command including config file flags.
    /// This matches what `install_service` writes to the container,
    /// so `needs_cycle` can compare against the running container's Cmd.
    pub fn effective_command(&self) -> Option<Vec<String>> {
        let mut cmd = self.command.clone();
        for cf in &self.config_files {
            if let Some(ref flag) = cf.flag {
                let flag_args: Vec<String> = flag.split_whitespace().map(String::from).collect();
                let c = cmd.get_or_insert_with(Vec::new);
                c.extend(flag_args);
            }
        }
        cmd
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LogLine {
    pub timestamp: Option<String>,
    pub stream: String,
    pub log: String,
}
