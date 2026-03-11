use anyhow::{Context, Result};
use bollard::container::{
    Config, CreateContainerOptions, InspectContainerOptions, KillContainerOptions,
    ListContainersOptions, LogsOptions, RemoveContainerOptions, RenameContainerOptions,
    RestartContainerOptions, StartContainerOptions, StatsOptions, StopContainerOptions,
};
use bollard::image::{CreateImageOptions, PruneImagesOptions};
use bollard::models::{ContainerCreateResponse, HealthStatusEnum, HostConfig, PortBinding};
use bollard::Docker;
use futures_util::stream::{Stream, StreamExt, TryStreamExt};
use garden_common::console::{self, ConsolePrinter};
use garden_common::constants::{OFFERING_CONTAINER_PREFIX, OFFERING_FQN_CONTAINER_SEPARATOR};
use garden_common::manifests::get_ports_catalog;
use garden_common::offerings::OfferingFqn;
use garden_common::types::{PortConflictHandler, PortRemediation};
use garden_common::{ServiceHealthStatus, ServiceStatus};
use std::collections::HashMap;
use std::net::TcpListener;
use std::pin::Pin;
use std::sync::Arc;

pub fn zen_offering_container_name(offering_name: &str) -> Result<String> {
    let fqn = OfferingFqn::parse(offering_name)
        .map_err(|e| anyhow::anyhow!("Invalid offering name '{}': {}", offering_name, e))?;
    Ok(format!(
        "{}{}",
        OFFERING_CONTAINER_PREFIX,
        fqn.encoded_for_container()
    ))
}

pub fn decode_zen_offering_container_name(container_name: &str) -> Option<String> {
    let trimmed = container_name.trim_start_matches('/');
    let suffix = trimmed.strip_prefix(OFFERING_CONTAINER_PREFIX)?;
    Some(decode_offering_container_suffix(suffix))
}

fn decode_offering_container_suffix(encoded: &str) -> String {
    // Image-direct containers: img-nginx-latest → image:nginx-latest (best-effort)
    if let Some(rest) = encoded.strip_prefix("img-") {
        if let Some((sanitized_ref, instance)) =
            rest.split_once(OFFERING_FQN_CONTAINER_SEPARATOR)
        {
            return format!("image:{}::{}", sanitized_ref, instance);
        }
        return format!("image:{}", rest);
    }

    // Curated containers: mongodb--prod → mongodb::prod
    if let Some((offering, instance)) = encoded.split_once(OFFERING_FQN_CONTAINER_SEPARATOR) {
        format!("{}::{}", offering, instance)
    } else {
        encoded.to_string()
    }
}

// ============================================================================
// Container Networking
// ============================================================================

/// DNS, env, and host configuration injected into every managed container.
struct ContainerNetworking {
    /// DNS servers (bridge gateway for resolved, plus fallback).
    dns: Vec<String>,
    /// Search domains (e.g., `["zengarden"]`).
    dns_search: Vec<String>,
    /// Extra host mappings (e.g., `host.docker.internal:<ip>`).
    extra_hosts: Vec<String>,
    /// Auto-injected environment variables (`KOI_ENDPOINT`, `GARDEN_STONE_ENDPOINT`, etc.).
    env_inject: Vec<String>,
}

// ============================================================================
// Container Specification
// ============================================================================

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

// ============================================================================
// Port Availability and Remediation (Catalog-Driven)
// ============================================================================

/// Check if a TCP port is available for binding
fn is_port_available(port: u16) -> bool {
    TcpListener::bind(("0.0.0.0", port)).is_ok()
}

/// Get the platform-specific conflict handler for a port from the catalog
fn get_conflict_handler(port: u16) -> Option<&'static PortConflictHandler> {
    let catalog = get_ports_catalog()?;
    let port_entry = catalog.ports.get(&port)?;

    #[cfg(target_os = "linux")]
    {
        port_entry.linux.as_ref()
    }

    #[cfg(target_os = "macos")]
    {
        port_entry.macos.as_ref()
    }

    #[cfg(target_os = "windows")]
    {
        port_entry.windows.as_ref()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Get the default (cross-platform) remediation for a port from the catalog
fn get_default_remediation(port: u16) -> Option<&'static PortRemediation> {
    let catalog = get_ports_catalog()?;
    let port_entry = catalog.ports.get(&port)?;
    port_entry.default.as_ref()
}

/// Find the next available port in a given range
fn find_available_port_in_range(start: u16, end: u16) -> Option<u16> {
    (start..=end).find(|&port| is_port_available(port))
}

/// Run a shell command and return success status
async fn run_command(cmd: &str) -> Result<bool> {
    #[cfg(unix)]
    {
        let status = tokio::process::Command::new("sh")
            .args(["-c", cmd])
            .status()
            .await
            .context(format!("Failed to run command: {}", cmd))?;
        Ok(status.success())
    }

    #[cfg(windows)]
    {
        let status = tokio::process::Command::new("cmd")
            .args(["/C", cmd])
            .status()
            .await
            .context(format!("Failed to run command: {}", cmd))?;
        Ok(status.success())
    }
}

/// Execute automatic remediation from catalog
async fn execute_auto_remediation(
    port: u16,
    commands: &[String],
    files: &Option<Vec<garden_common::types::RemediationFile>>,
) -> Result<()> {
    tracing::info!(port = port, "Executing automatic port remediation");

    // Run remediation commands
    for cmd in commands {
        tracing::debug!(command = cmd, "Running remediation command");
        let success = run_command(cmd).await?;
        if !success {
            anyhow::bail!("Remediation command failed: {}", cmd);
        }
    }

    // Create any post-remediation files
    if let Some(files_to_create) = files {
        for file in files_to_create {
            tracing::debug!(path = file.path, "Creating remediation file");

            // Remove symlink if exists (common for /etc/resolv.conf)
            let path = std::path::Path::new(&file.path);
            if path.is_symlink() {
                std::fs::remove_file(path)
                    .context(format!("Failed to remove symlink: {}", file.path))?;
            }

            std::fs::write(path, &file.content)
                .context(format!("Failed to create file: {}", file.path))?;
        }
    }

    // Give the system time to release the port
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Verify port is now available
    if is_port_available(port) {
        tracing::info!(port = port, "Port is now available after remediation");
        Ok(())
    } else {
        anyhow::bail!(
            "Port {} is still in use after remediation. Another service may be using it.",
            port
        );
    }
}

/// Attempt to remediate a port conflict using platform-specific handler
async fn remediate_port_with_handler(port: u16, handler: &PortConflictHandler) -> Result<()> {
    // If there's a detection command, verify the expected culprit is running
    if let Some(detection_cmd) = &handler.detection {
        let culprit_active = run_command(detection_cmd).await.unwrap_or(false);
        if !culprit_active {
            anyhow::bail!(
                "Port {} is in use by a service other than {}. \
                 Check what's using it with: sudo lsof -i :{}",
                port,
                handler.common_culprit,
                port
            );
        }
    }

    // Execute remediation based on type
    match &handler.remediation {
        PortRemediation::Auto { commands, files } => {
            tracing::info!(
                port = port,
                culprit = handler.common_culprit,
                "Auto-remediating port conflict"
            );
            execute_auto_remediation(port, commands, files).await
        }
        PortRemediation::Remap {
            range_start,
            range_end,
        } => {
            // Platform handler specified remap - this is unusual but supported
            anyhow::bail!(
                "Port {} has platform-specific remap rule (range {}-{}), but remap should be handled at resolution level",
                port, range_start, range_end
            );
        }
        PortRemediation::Manual { message } => {
            anyhow::bail!("Port {} conflict: {}", port, message);
        }
        PortRemediation::Fail { message } => {
            anyhow::bail!("Port {} conflict: {}", port, message);
        }
    }
}

/// Resolve a port conflict - either remediate or remap
///
/// Returns the actual host port to use (may be different if remapped).
/// `docker_occupied` maps host ports already claimed by Docker containers
/// (including stopped ones) to avoid silent conflicts on restart.
async fn resolve_port_conflict(
    requested_port: u16,
    docker_occupied: &HashMap<u16, String>,
) -> Result<u16> {
    // First, check for platform-specific handler
    if let Some(handler) = get_conflict_handler(requested_port) {
        // Platform-specific handling (Auto, Manual, Fail)
        remediate_port_with_handler(requested_port, handler).await?;
        return Ok(requested_port);
    }

    // No platform handler - check for default remediation (typically Remap)
    if let Some(default_remediation) = get_default_remediation(requested_port) {
        match default_remediation {
            PortRemediation::Remap {
                range_start,
                range_end,
            } => {
                tracing::info!(
                    port = requested_port,
                    range_start = range_start,
                    range_end = range_end,
                    "Port in use, finding available port in remap range"
                );

                match find_available_port_in_range(*range_start, *range_end) {
                    Some(new_port) => {
                        tracing::info!(
                            original_port = requested_port,
                            remapped_port = new_port,
                            "Port remapped successfully"
                        );
                        return Ok(new_port);
                    }
                    None => {
                        anyhow::bail!(
                            "Port {} is in use and no available port found in remap range {}-{}",
                            requested_port,
                            range_start,
                            range_end
                        );
                    }
                }
            }
            PortRemediation::Auto { commands, files } => {
                tracing::info!(
                    port = requested_port,
                    "Auto-remediating port conflict (default handler)"
                );
                execute_auto_remediation(requested_port, commands, files).await?;
                return Ok(requested_port);
            }
            PortRemediation::Manual { message } => {
                anyhow::bail!("Port {} conflict: {}", requested_port, message);
            }
            PortRemediation::Fail { message } => {
                anyhow::bail!("Port {} conflict: {}", requested_port, message);
            }
        }
    }

    // No catalog entry — universal increment-by-one fallback.
    // Try requested_port+1 through +100, checking both TCP bind and Docker occupancy.
    for candidate in (requested_port + 1)..=(requested_port.saturating_add(100)) {
        if is_port_available(candidate) && !docker_occupied.contains_key(&candidate) {
            tracing::info!(
                original_port = requested_port,
                remapped_port = candidate,
                "Port remapped via universal increment fallback"
            );
            return Ok(candidate);
        }
    }

    anyhow::bail!(
        "Port {} is in use and no available port found in range {}-{}",
        requested_port,
        requested_port + 1,
        requested_port.saturating_add(100)
    );
}

/// Pre-flight check for port availability with automatic remediation/remapping
///
/// Uses the well-known ports catalog to determine how to handle conflicts:
/// - For ports with auto-remediation (e.g., DNS port 53), runs commands to free the port
/// - For ports with remap configuration, finds the next available port in range
/// - For uncatalogued ports, increments by one until a vacant port is found
/// - For manual or fail types, returns an actionable error message
///
/// `docker_occupied` maps host ports claimed by any Docker container (including
/// stopped ones) so we avoid conflicts that TCP bind alone would miss.
///
/// Returns the resolved port mappings - (actual_host_port, container_port).
/// The actual_host_port may differ from the requested port if it was remapped.
pub async fn check_and_remediate_ports(
    ports: &[(u16, u16)],
    docker_occupied: &HashMap<u16, String>,
) -> Result<Vec<(u16, u16)>> {
    let mut resolved_ports = Vec::with_capacity(ports.len());

    for (host_port, container_port) in ports {
        if is_port_available(*host_port) && !docker_occupied.contains_key(host_port) {
            // Port is available (both TCP-bindable and not claimed by a stopped container)
            resolved_ports.push((*host_port, *container_port));
        } else {
            // Port conflict - attempt resolution
            tracing::info!(port = host_port, "Port is in use, attempting resolution");
            let actual_host_port = resolve_port_conflict(*host_port, docker_occupied).await?;
            resolved_ports.push((actual_host_port, *container_port));
        }
    }

    Ok(resolved_ports)
}

pub struct DockerManager {
    docker: Docker,
}

impl DockerManager {
    pub fn new() -> Result<Self> {
        #[cfg(target_os = "windows")]
        let docker = {
            tracing::debug!("Connecting to Docker via Windows named pipe");
            Docker::connect_with_named_pipe_defaults().context(
                "Failed to connect to Docker daemon via named pipe (is Docker Desktop running?)",
            )?
        };

        #[cfg(not(target_os = "windows"))]
        let docker = {
            tracing::debug!("Connecting to Docker via Unix socket");
            Docker::connect_with_socket_defaults()
                .context("Failed to connect to Docker daemon via Unix socket")?
        };

        Ok(Self { docker })
    }

    /// Check if Docker daemon is available and responsive
    pub async fn is_healthy(&self) -> bool {
        self.docker.ping().await.is_ok()
    }

    /// Get Docker version
    pub async fn get_docker_version(&self) -> Result<String> {
        let version = self
            .docker
            .version()
            .await
            .context("Failed to get Docker version")?;

        // Extract version string (e.g., "24.0.7")
        Ok(version.version.unwrap_or_else(|| "unknown".to_string()))
    }

    /// Get the gateway IP of the default Docker bridge network.
    ///
    /// Containers on the bridge network can reach the host via this IP.
    /// Used to configure systemd-resolved bridge listener and container DNS.
    pub async fn bridge_gateway(&self) -> Option<String> {
        let network = self
            .docker
            .inspect_network("bridge", None::<bollard::network::InspectNetworkOptions<String>>)
            .await
            .ok()?;

        network
            .ipam?
            .config?
            .into_iter()
            .find_map(|c| c.gateway)
    }

    /// Build container networking configuration.
    ///
    /// Every managed container gets:
    /// - DNS pointing to systemd-resolved (via Docker bridge gateway)
    /// - Search domain `zengarden` (Koi DNS zone, forwarded by resolved)
    /// - Environment variables for reaching host services (Koi HTTP, Moss API)
    /// - `host.docker.internal` extra host mapping
    async fn container_networking(&self, name: &str) -> ContainerNetworking {
        let host_ip = garden_common::infra::network::get_local_ip();
        let dns_ip = match self.bridge_gateway().await {
            Some(gw) => gw,
            None => {
                tracing::warn!(
                    offering = %name,
                    fallback = %host_ip,
                    "bridge gateway unavailable, using host IP for container DNS"
                );
                host_ip.clone()
            }
        };

        ContainerNetworking {
            dns: vec![dns_ip, "8.8.8.8".to_string()],
            dns_search: vec!["zengarden".to_string()],
            extra_hosts: vec![format!("host.docker.internal:{}", host_ip)],
            env_inject: vec![
                format!(
                    "KOI_ENDPOINT=http://{}:{}",
                    host_ip,
                    garden_common::constants::KOI_HTTP
                ),
                format!(
                    "GARDEN_STONE_ENDPOINT=http://{}:{}",
                    host_ip,
                    garden_common::constants::MOSS_HTTP
                ),
                format!("GARDEN_OFFERING_NAME={}", name),
            ],
        }
    }

    /// Stop a service container
    pub async fn stop_service(
        &self,
        name: &str,
        console: Option<&Arc<ConsolePrinter>>,
    ) -> Result<()> {
        let container_name = zen_offering_container_name(name)?;

        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::Stopping,
                name.to_string(),
            ));
        }
        tracing::info!(service = %name, "Stopping service via Docker API");

        self.docker
            .stop_container(&container_name, None::<StopContainerOptions>)
            .await
            .context("Failed to stop container")?;

        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::Stopped,
                name.to_string(),
            ));
        }
        tracing::info!(service = %name, "Service stopped successfully");
        Ok(())
    }

    /// Start a service container
    pub async fn start_service(
        &self,
        name: &str,
        console: Option<&Arc<ConsolePrinter>>,
    ) -> Result<()> {
        let container_name = zen_offering_container_name(name)?;

        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::Starting,
                name.to_string(),
            ));
        }
        tracing::info!(service = %name, "Starting service via Docker API");

        self.docker
            .start_container(&container_name, None::<StartContainerOptions<String>>)
            .await
            .context("Failed to start container")?;

        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::Running,
                name.to_string(),
            ));
        }
        tracing::info!(service = %name, "Service started successfully");
        Ok(())
    }

    /// Rename a service container (stop → rename → start).
    ///
    /// The container is renamed from `zen-offering-{old_encoded}` to
    /// `zen-offering-{new_encoded}`. Volumes are bound by container ID
    /// so they survive the rename. The container must be stopped first.
    pub async fn rename_service(&self, old_name: &str, new_name: &str) -> Result<()> {
        let old_container = zen_offering_container_name(old_name)?;
        let new_container = zen_offering_container_name(new_name)?;

        tracing::info!(
            old = %old_container,
            new = %new_container,
            "Renaming service container"
        );

        self.docker
            .rename_container(
                &old_container,
                RenameContainerOptions {
                    name: new_container.clone(),
                },
            )
            .await
            .with_context(|| {
                format!("Failed to rename container {old_container} → {new_container}")
            })?;

        Ok(())
    }

    pub async fn install_service(
        &self,
        name: &str,
        spec: &ContainerSpec,
        console: Option<&Arc<ConsolePrinter>>,
    ) -> Result<Vec<(u16, u16)>> {
        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::Requesting,
                format!("{} → {}", name, spec.image),
            ));
        }
        tracing::info!(service = %name, image = %spec.image, "Installing service via Docker API");

        // Prefix container name with "zen-offering-" to identify as Zen Garden offering
        // Note: zen-companion-* prefix is reserved for sidecars/companion containers
        let container_name = zen_offering_container_name(name)?;

        // Check if container already exists
        if self.container_exists(&container_name).await? {
            anyhow::bail!("Container '{}' already exists", container_name);
        }

        // Scan Docker port occupancy (including stopped containers), excluding our own
        let docker_occupied = self.scan_port_occupancy(Some(&container_name)).await?;

        // Pre-flight port availability check with automatic remediation/remapping
        let resolved_ports = check_and_remediate_ports(&spec.ports, &docker_occupied).await?;

        // Log any port remappings
        for ((original, _), (actual, _)) in spec.ports.iter().zip(resolved_ports.iter()) {
            if original != actual {
                tracing::info!(
                    service = %name,
                    original_port = original,
                    actual_port = actual,
                    "Port was remapped due to conflict"
                );
            }
        }

        // Pull image if not present
        self.pull_image(&spec.image, console).await?;

        // Configure port bindings (using resolved ports)
        let mut port_bindings = HashMap::new();
        for (host_port, container_port) in &resolved_ports {
            port_bindings.insert(
                format!("{}/tcp", container_port),
                Some(vec![PortBinding {
                    host_ip: Some("0.0.0.0".to_string()),
                    host_port: Some(host_port.to_string()),
                }]),
            );
        }

        // Configure volumes
        let mut binds = Vec::new();
        for (host_path, container_path) in &spec.volumes {
            binds.push(format!("{}:{}", host_path, container_path));
        }

        // TOPO-0002: Auto-inject shared topology directory mount
        // Cross-cutting infrastructure concern — every managed container gets
        // read-write access to the topology directory for pre-warmed discovery.
        let topo_host = garden_common::constants::paths::topology_dir();
        let topo_container = garden_common::constants::paths::CONTAINER_TOPOLOGY_DIR;
        binds.push(format!("{}:{}", topo_host, topo_container));

        // Config file injection: create empty config files on the host and
        // bind-mount them into the container. This lets config patches write
        // content to these files and restart — no container recreation needed.
        let mut effective_cmd = spec.command.clone();
        for cf in &spec.config_files {
            let host_dir = garden_common::constants::paths::offering_config_dir(name);
            tokio::fs::create_dir_all(&host_dir).await.context(
                format!("Failed to create config dir: {}", host_dir),
            )?;

            let filename = std::path::Path::new(&cf.path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "config".to_string());
            let host_path = format!("{}/{}", host_dir, filename);

            // Create empty config file if it doesn't exist (idempotent)
            if !tokio::fs::try_exists(&host_path).await.unwrap_or(false) {
                let empty_content = cf.format.empty_content();
                tokio::fs::write(&host_path, empty_content).await.context(
                    format!("Failed to write empty config file: {}", host_path),
                )?;
                tracing::info!(
                    service = %name,
                    host_path = %host_path,
                    container_path = %cf.path,
                    "Created empty config file"
                );
            }

            // Bind-mount the config file into the container (read-only)
            binds.push(format!("{}:{}:ro", host_path, cf.path));

            // If the manifest declares a flag, add it to the container command
            // so the software knows to read this config file.
            if let Some(ref flag) = cf.flag {
                let flag_args: Vec<String> = flag.split_whitespace().map(String::from).collect();
                let cmd = effective_cmd.get_or_insert_with(Vec::new);
                cmd.extend(flag_args);
            }
        }

        // Container networking: DNS (via systemd-resolved on bridge gateway),
        // env vars (Koi/Moss endpoints), and host.docker.internal mapping.
        let net = self.container_networking(name).await;
        let mut full_env = spec.environment.clone();
        full_env.extend(net.env_inject);

        let host_config = HostConfig {
            port_bindings: Some(port_bindings),
            binds: Some(binds),
            extra_hosts: Some(net.extra_hosts),
            dns: Some(net.dns),
            dns_search: Some(net.dns_search),
            restart_policy: Some(bollard::models::RestartPolicy {
                name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
                maximum_retry_count: None,
            }),
            ..Default::default()
        };

        let cmd_refs: Option<Vec<&str>> = effective_cmd
            .as_ref()
            .map(|c| c.iter().map(|s| s.as_str()).collect());
        let env_refs: Vec<&str> = full_env.iter().map(|s| s.as_str()).collect();

        let config = Config {
            image: Some(spec.image.as_str()),
            cmd: cmd_refs,
            env: Some(env_refs),
            host_config: Some(host_config),
            ..Default::default()
        };

        // Create container
        let response: ContainerCreateResponse = self
            .docker
            .create_container(
                Some(CreateContainerOptions {
                    name: &container_name,
                    platform: None,
                }),
                config,
            )
            .await
            .context("Failed to create container")?;

        tracing::info!(container_id = %response.id, container_name = %container_name, "Container created");

        // Start container
        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::Creating,
                name.to_string(),
            ));
        }

        self.docker
            .start_container(&container_name, None::<StartContainerOptions<String>>)
            .await
            .context("Failed to start container")?;

        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::Running,
                name.to_string(),
            ));
        }
        tracing::info!(service = %name, container_name = %container_name, "Service started successfully");
        Ok(resolved_ports)
    }

    pub async fn remove_service(
        &self,
        name: &str,
        console: Option<&Arc<ConsolePrinter>>,
    ) -> Result<()> {
        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::Removing,
                name.to_string(),
            ));
        }
        tracing::info!(service = %name, "Removing service via Docker API");

        let container_name = zen_offering_container_name(name)?;

        if !self.container_exists(&container_name).await? {
            anyhow::bail!("Container '{}' does not exist", container_name);
        }

        // Stop container
        self.docker
            .stop_container(&container_name, None::<StopContainerOptions>)
            .await
            .context("Failed to stop container")?;

        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::Stopped,
                name.to_string(),
            ));
        }

        // Remove container
        self.docker
            .remove_container(
                &container_name,
                Some(RemoveContainerOptions {
                    v: true, // Remove associated volumes
                    force: true,
                    link: false,
                }),
            )
            .await
            .context("Failed to remove container")?;

        tracing::info!(service = %name, container_name = %container_name, "Service removed successfully");
        Ok(())
    }

    /// Recreate a container with a new spec, preserving data volumes.
    ///
    /// Purpose-built for config patch cycling: stops the running container,
    /// removes it **without** deleting volumes (`v: false`), then creates and
    /// starts a replacement with the updated spec.
    ///
    /// Unlike `remove_service` (which uses `v: true` for full uninstalls),
    /// this method preserves all Docker volumes so data survives the cycle.
    /// Unlike `install_service`, this skips the image pull (the image is
    /// already present) and port availability pre-flight (ports were just freed
    /// by the stop).
    pub async fn recreate_service(
        &self,
        name: &str,
        spec: &ContainerSpec,
    ) -> Result<Vec<(u16, u16)>> {
        let container_name = zen_offering_container_name(name)?;
        tracing::info!(service = %name, "Recreating container for config convergence");

        // Stop the running container
        self.docker
            .stop_container(&container_name, None::<StopContainerOptions>)
            .await
            .context("Failed to stop container for recreate")?;

        // Remove container WITHOUT deleting volumes (v: false)
        self.docker
            .remove_container(
                &container_name,
                Some(RemoveContainerOptions {
                    v: false, // Preserve volumes — this is a reconfigure, not an uninstall
                    force: false,
                    link: false,
                }),
            )
            .await
            .context("Failed to remove container for recreate")?;

        tracing::info!(service = %name, "Old container removed (volumes preserved)");

        // Scan Docker port occupancy (old container just removed, so its ports are freed)
        let docker_occupied = self.scan_port_occupancy(None).await?;

        // Resolve ports (same logic as install_service)
        let resolved_ports = check_and_remediate_ports(&spec.ports, &docker_occupied).await?;

        // Configure port bindings
        let mut port_bindings = HashMap::new();
        for (host_port, container_port) in &resolved_ports {
            port_bindings.insert(
                format!("{}/tcp", container_port),
                Some(vec![PortBinding {
                    host_ip: Some("0.0.0.0".to_string()),
                    host_port: Some(host_port.to_string()),
                }]),
            );
        }

        // Configure volumes
        let mut binds = Vec::new();
        for (host_path, container_path) in &spec.volumes {
            binds.push(format!("{}:{}", host_path, container_path));
        }

        // TOPO-0002: Auto-inject shared topology directory mount
        let topo_host = garden_common::constants::paths::topology_dir();
        let topo_container = garden_common::constants::paths::CONTAINER_TOPOLOGY_DIR;
        binds.push(format!("{}:{}", topo_host, topo_container));

        // Config file injection (same as install_service)
        let mut effective_cmd = spec.command.clone();
        for cf in &spec.config_files {
            let host_dir = garden_common::constants::paths::offering_config_dir(name);
            tokio::fs::create_dir_all(&host_dir).await.context(
                format!("Failed to create config dir: {}", host_dir),
            )?;

            let filename = std::path::Path::new(&cf.path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "config".to_string());
            let host_path = format!("{}/{}", host_dir, filename);

            if !tokio::fs::try_exists(&host_path).await.unwrap_or(false) {
                let empty_content = cf.format.empty_content();
                tokio::fs::write(&host_path, empty_content).await.context(
                    format!("Failed to write empty config file: {}", host_path),
                )?;
            }

            binds.push(format!("{}:{}:ro", host_path, cf.path));

            if let Some(ref flag) = cf.flag {
                let flag_args: Vec<String> = flag.split_whitespace().map(String::from).collect();
                let cmd = effective_cmd.get_or_insert_with(Vec::new);
                cmd.extend(flag_args);
            }
        }

        // Container networking (same as install_service)
        let net = self.container_networking(name).await;
        let mut full_env = spec.environment.clone();
        full_env.extend(net.env_inject);

        let host_config = HostConfig {
            port_bindings: Some(port_bindings),
            binds: Some(binds),
            extra_hosts: Some(net.extra_hosts),
            dns: Some(net.dns),
            dns_search: Some(net.dns_search),
            restart_policy: Some(bollard::models::RestartPolicy {
                name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
                maximum_retry_count: None,
            }),
            ..Default::default()
        };

        let cmd_refs: Option<Vec<&str>> = effective_cmd
            .as_ref()
            .map(|c| c.iter().map(|s| s.as_str()).collect());
        let env_refs: Vec<&str> = full_env.iter().map(|s| s.as_str()).collect();

        let config = Config {
            image: Some(spec.image.as_str()),
            cmd: cmd_refs,
            env: Some(env_refs),
            host_config: Some(host_config),
            ..Default::default()
        };

        // Create new container
        let response: ContainerCreateResponse = self
            .docker
            .create_container(
                Some(CreateContainerOptions {
                    name: &container_name,
                    platform: None,
                }),
                config,
            )
            .await
            .context("Failed to create container during recreate")?;

        tracing::info!(
            container_id = %response.id,
            container_name = %container_name,
            "New container created"
        );

        // Start the new container
        self.docker
            .start_container(&container_name, None::<StartContainerOptions<String>>)
            .await
            .context("Failed to start container during recreate")?;

        tracing::info!(
            service = %name,
            container_name = %container_name,
            "Container recreated and started successfully"
        );

        Ok(resolved_ports)
    }

    /// Restart a running container via Docker API.
    ///
    /// Much less destructive than recreate — the container keeps its identity,
    /// mounts, and configuration. Only the process inside is restarted.
    /// Used after config file changes (file_restart policy).
    pub async fn restart_service(&self, name: &str) -> Result<()> {
        let container_name = zen_offering_container_name(name)?;
        tracing::info!(service = %name, "Restarting container");
        self.docker
            .restart_container(
                &container_name,
                Some(RestartContainerOptions { t: 10 }),
            )
            .await
            .context("Failed to restart container")?;
        tracing::info!(service = %name, "Container restarted successfully");
        Ok(())
    }

    /// Send a signal to a running container (e.g., SIGHUP for config reload).
    ///
    /// Least destructive option — zero downtime. The process stays running
    /// and re-reads its config file. Used for signal_reload policy.
    pub async fn signal_container(&self, name: &str, signal: &str) -> Result<()> {
        let container_name = zen_offering_container_name(name)?;
        tracing::info!(service = %name, signal = %signal, "Sending signal to container");
        self.docker
            .kill_container(
                &container_name,
                Some(KillContainerOptions {
                    signal: signal.to_string(),
                }),
            )
            .await
            .context("Failed to send signal to container")?;
        tracing::info!(service = %name, signal = %signal, "Signal sent successfully");
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn upgrade_service(
        &self,
        name: &str,
        spec: &ContainerSpec,
        console: Option<&Arc<ConsolePrinter>>,
    ) -> Result<()> {
        let container_name = zen_offering_container_name(name)?;

        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::Upgrading,
                format!("{} → {}", name, spec.image),
            ));
        }
        tracing::info!(service = %name, new_image = %spec.image, "Upgrading service");

        // Pull new image
        self.pull_image(&spec.image, console).await?;

        // Stop and remove old container
        self.remove_service(name, console).await?;

        // Create and start new container
        self.install_service(name, spec, console).await?;

        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::Upgraded,
                name.to_string(),
            ));
        }
        tracing::info!(service = %name, container_name = %container_name, "Service upgraded successfully");
        Ok(())
    }

    async fn container_exists(&self, name: &str) -> Result<bool> {
        let filters = HashMap::from([("name".to_string(), vec![name.to_string()])]);
        let options = ListContainersOptions {
            all: true,
            filters,
            ..Default::default()
        };

        let containers = self
            .docker
            .list_containers(Some(options))
            .await
            .context("Failed to list containers")?;

        Ok(containers.iter().any(|c| {
            c.names
                .as_ref()
                .map(|names| names.iter().any(|n| n.trim_start_matches('/') == name))
                .unwrap_or(false)
        }))
    }

    /// Check if a zen-offering container exists for the given offering name
    pub async fn zen_container_exists(&self, offering: &str) -> Result<bool> {
        let container_name = zen_offering_container_name(offering)?;
        self.container_exists(&container_name).await
    }

    /// Get the Docker image string for a zen-offering container (e.g., "mongo:7")
    pub async fn get_service_image(&self, name: &str) -> Result<String> {
        let container_name = zen_offering_container_name(name)?;
        let inspect = self
            .docker
            .inspect_container(&container_name, None::<InspectContainerOptions>)
            .await
            .context(format!("Failed to inspect container '{}'", container_name))?;

        let config = inspect.config.context("Container has no config")?;
        let image = config.image.unwrap_or_else(|| "<unknown>".to_string());
        Ok(image)
    }

    /// Get the actual running image ID/SHA for a container (not the tag reference)
    /// Returns the full SHA256 like "sha256:abcd1234..." that identifies the actual image
    pub async fn get_service_image_id(&self, name: &str) -> Result<String> {
        let container_name = zen_offering_container_name(name)?;
        let inspect = self
            .docker
            .inspect_container(&container_name, None::<InspectContainerOptions>)
            .await
            .context(format!("Failed to inspect container '{}'", container_name))?;

        // The top-level `image` field contains the actual SHA256 of the running image
        inspect
            .image
            .context(format!("Container '{}' has no image ID", container_name))
    }

    /// Pull a Docker image from registry
    ///
    /// Used during install and nourishment to fetch images.
    /// Pull a Docker image, with stall detection.
    ///
    /// The Docker pull stream can stall indefinitely (network issues, DNS
    /// failure, registry rate-limits). To prevent HTTP handlers from hanging
    /// forever, each stream chunk is guarded by a TTL-with-no-activity
    /// timeout. The timer resets on every progress event from Docker — so
    /// large pulls that keep making progress are fine. The timeout only
    /// fires when Docker goes completely silent.
    pub async fn pull_image(
        &self,
        image: &str,
        console: Option<&Arc<ConsolePrinter>>,
    ) -> Result<()> {
        let stall_timeout = garden_common::constants::timeouts::docker_pull_stall_timeout();

        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::Pulling,
                image.to_string(),
            ));
        }
        tracing::info!(image = %image, "Pulling Docker image");

        let options = CreateImageOptions {
            from_image: image,
            ..Default::default()
        };

        let mut stream = self.docker.create_image(Some(options), None, None);

        loop {
            match tokio::time::timeout(stall_timeout, stream.next()).await {
                Ok(Some(Ok(info))) => {
                    if let Some(status) = info.status {
                        if let Some(console) = console {
                            if let Some(progress) = &info.progress {
                                console.emit(console::ConsoleEvent::new(
                                    console::EventCategory::Services,
                                    console::EventStatus::PullProgress,
                                    format!("{} → {}", image, progress),
                                ));
                            }
                        }
                        tracing::debug!(image = %image, status = %status, "Pull progress");
                    }
                }
                Ok(Some(Err(e))) => {
                    anyhow::bail!("Failed to pull image '{}': {}", image, e);
                }
                Ok(None) => {
                    // Stream finished — pull complete
                    break;
                }
                Err(_elapsed) => {
                    anyhow::bail!(
                        "Image pull stalled for '{}': no progress for {} seconds. \
                         Check network connectivity and Docker Hub access on this stone.",
                        image,
                        stall_timeout.as_secs(),
                    );
                }
            }
        }

        if let Some(console) = console {
            console.emit(console::ConsoleEvent::new(
                console::EventCategory::Services,
                console::EventStatus::PullComplete,
                image.to_string(),
            ));
        }
        tracing::info!(image = %image, "Image pulled successfully");
        Ok(())
    }

    /// Inspect an image's OCI metadata (config, labels, architecture).
    ///
    /// The image must already be pulled locally.
    pub async fn inspect_image_metadata(
        &self,
        image: &str,
    ) -> Result<bollard::models::ImageInspect> {
        self.docker
            .inspect_image(image)
            .await
            .with_context(|| format!("Failed to inspect image '{}'", image))
    }

    /// Get the status of a service by checking its Docker container
    pub async fn get_service_status(&self, name: &str) -> Result<ServiceStatus> {
        let container_name = zen_offering_container_name(name)?;

        let inspect = self
            .docker
            .inspect_container(&container_name, None::<InspectContainerOptions>)
            .await
            .context(format!("Failed to inspect container '{}'", container_name))?;

        let state = inspect.state.context("Container has no state")?;

        let status = if state.running.unwrap_or(false) {
            ServiceStatus::Running
        } else if state.paused.unwrap_or(false) {
            ServiceStatus::Stopped
        } else if state.restarting.unwrap_or(false) {
            ServiceStatus::Degraded
        } else {
            ServiceStatus::Stopped
        };

        Ok(status)
    }

    /// Get the health status of a service by checking its Docker container health
    pub async fn get_service_health(&self, name: &str) -> Result<ServiceHealthStatus> {
        let container_name = zen_offering_container_name(name)?;

        let inspect = self
            .docker
            .inspect_container(&container_name, None::<InspectContainerOptions>)
            .await
            .context(format!("Failed to inspect container '{}'", container_name))?;

        let state = inspect.state.context("Container has no state")?;

        // Check if container is running first
        if !state.running.unwrap_or(false) {
            return Ok(ServiceHealthStatus::Offline);
        }

        // Check Docker health check status if available
        if let Some(health) = state.health {
            if let Some(status) = health.status {
                return Ok(match status {
                    HealthStatusEnum::HEALTHY => ServiceHealthStatus::Healthy,
                    HealthStatusEnum::UNHEALTHY => ServiceHealthStatus::Degraded,
                    HealthStatusEnum::STARTING => ServiceHealthStatus::Degraded,
                    HealthStatusEnum::NONE | HealthStatusEnum::EMPTY => {
                        ServiceHealthStatus::Healthy
                    }
                });
            }
        }

        // If no health check configured, assume healthy if running
        Ok(ServiceHealthStatus::Healthy)
    }

    /// List all zen-offering-prefixed containers (decoded to offering FQNs)
    /// Note: Does not include zen-companion-* sidecars
    pub async fn list_zen_containers(&self) -> Result<Vec<String>> {
        let filters = HashMap::from([(
            "name".to_string(),
            vec![OFFERING_CONTAINER_PREFIX.to_string()],
        )]);
        let options = ListContainersOptions {
            all: true,
            filters,
            ..Default::default()
        };

        let containers = self
            .docker
            .list_containers(Some(options))
            .await
            .context("Failed to list containers")?;

        let names = containers
            .into_iter()
            .filter_map(|c| {
                c.names.and_then(|names| {
                    names.into_iter().find_map(|n| {
                        let trimmed = n.trim_start_matches('/');
                        decode_zen_offering_container_name(trimmed)
                    })
                })
            })
            .collect();

        Ok(names)
    }

    /// List all containers with detailed information (for detection)
    pub async fn list_all_containers(
        &self,
    ) -> Result<Vec<crate::infra::detection::container_inspect::ContainerInfo>> {
        let options = ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        };

        let containers = self
            .docker
            .list_containers(Some(options))
            .await
            .context("Failed to list containers")?;

        let infos = containers
            .into_iter()
            .filter_map(|c| {
                let name = c
                    .names
                    .as_ref()
                    .and_then(|names| names.first())
                    .unwrap_or(&String::new())
                    .to_string();

                let image = c.image.unwrap_or_default();
                let state = c.state.unwrap_or_else(|| "unknown".to_string());

                if !name.is_empty() {
                    Some(crate::infra::detection::container_inspect::ContainerInfo {
                        name,
                        image,
                        state,
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(infos)
    }

    /// Get resource metrics for a specific container
    pub async fn get_container_stats(
        &self,
        name: &str,
    ) -> Result<garden_common::ContainerResources> {
        let container_name = zen_offering_container_name(name)?;

        let stats = self
            .docker
            .stats(
                &container_name,
                Some(StatsOptions {
                    stream: false,
                    one_shot: true,
                }),
            )
            .try_next()
            .await
            .context("Failed to get container stats")?
            .ok_or_else(|| anyhow::anyhow!("No stats available for container"))?;

        // Calculate CPU percentage
        let cpu_delta = stats.cpu_stats.cpu_usage.total_usage as f64
            - stats.precpu_stats.cpu_usage.total_usage as f64;
        let system_delta = stats.cpu_stats.system_cpu_usage.unwrap_or(0) as f64
            - stats.precpu_stats.system_cpu_usage.unwrap_or(0) as f64;
        let cpu_percent = if system_delta > 0.0 && cpu_delta > 0.0 {
            let num_cpus = stats.cpu_stats.online_cpus.unwrap_or(1) as f64;
            (cpu_delta / system_delta) * num_cpus * 100.0
        } else {
            0.0
        };

        // Memory metrics
        let memory_bytes = stats.memory_stats.usage.unwrap_or(0);
        let memory_limit = stats.memory_stats.limit.unwrap_or(0);
        let memory_percent = if memory_limit > 0 {
            (memory_bytes as f64 / memory_limit as f64 * 100.0) as f32
        } else {
            0.0
        };

        // Network I/O
        let (network_rx_bytes, network_tx_bytes) = if let Some(networks) = stats.networks {
            networks.values().fold((0u64, 0u64), |(rx, tx), net| {
                (rx + net.rx_bytes, tx + net.tx_bytes)
            })
        } else {
            (0, 0)
        };

        // Block I/O
        let (block_read_bytes, block_write_bytes) =
            if let Some(io_stats) = stats.blkio_stats.io_service_bytes_recursive {
                io_stats.iter().fold((0u64, 0u64), |(read, write), entry| {
                    match entry.op.as_str() {
                        "read" | "Read" => (read + entry.value, write),
                        "write" | "Write" => (read, write + entry.value),
                        _ => (read, write),
                    }
                })
            } else {
                (0, 0)
            };

        // Container uptime (calculate from started_at timestamp)
        let uptime_seconds = self
            .get_container_uptime(&container_name)
            .await
            .unwrap_or(0);

        Ok(garden_common::ContainerResources {
            cpu_percent: cpu_percent as f32,
            cpu_friendly: format!("{:.2}%", cpu_percent),
            memory_bytes,
            memory_limit_bytes: memory_limit,
            memory_percent,
            memory_friendly: garden_common::format_bytes(memory_bytes),
            memory_limit_friendly: garden_common::format_bytes(memory_limit),
            network_rx_bytes,
            network_tx_bytes,
            network_rx_friendly: garden_common::format_bytes(network_rx_bytes),
            network_tx_friendly: garden_common::format_bytes(network_tx_bytes),
            block_read_bytes,
            block_write_bytes,
            block_read_friendly: garden_common::format_bytes(block_read_bytes),
            block_write_friendly: garden_common::format_bytes(block_write_bytes),
            uptime_seconds,
            uptime_friendly: garden_common::format_uptime(uptime_seconds),
        })
    }

    /// Get service uptime in seconds (public wrapper that applies zen-offering prefix)
    pub async fn get_service_uptime(&self, name: &str) -> Result<u64> {
        let container_name = zen_offering_container_name(name)?;
        self.get_container_uptime(&container_name).await
    }

    /// Get container uptime in seconds
    async fn get_container_uptime(&self, container_name: &str) -> Result<u64> {
        let inspect = self
            .docker
            .inspect_container(container_name, None::<InspectContainerOptions>)
            .await
            .context("Failed to inspect container")?;

        if let Some(state) = inspect.state {
            if let Some(started_at) = state.started_at {
                // Parse ISO 8601 timestamp
                if let Ok(started) = chrono::DateTime::parse_from_rfc3339(&started_at) {
                    let now = chrono::Utc::now();
                    let duration = now.signed_duration_since(started);
                    return Ok(duration.num_seconds().max(0) as u64);
                }
            }
        }

        Ok(0)
    }

    /// Stream logs from a container in real-time (follow mode)
    pub fn get_logs_stream(
        &self,
        name: &str,
        timestamps: bool,
    ) -> Pin<Box<dyn Stream<Item = Result<LogLine>> + Send + 'static>> {
        let name_owned = name.to_string();
        let container_name = match zen_offering_container_name(&name_owned) {
            Ok(value) => value,
            Err(e) => {
                let err_msg = format!("Invalid offering name '{}': {}", name_owned, e);
                return Box::pin(async_stream::stream! {
                    yield Err(anyhow::anyhow!("{}", err_msg));
                });
            }
        };
        let docker = self.docker.clone();

        Box::pin(async_stream::stream! {
            let options = LogsOptions::<String> {
                follow: true,
                stdout: true,
                stderr: true,
                timestamps,
                ..Default::default()
            };

            let mut stream = docker.logs(&container_name, Some(options));

            while let Some(result) = stream.next().await {
                match result {
                    Ok(output) => {
                        let log_line = LogLine {
                            timestamp: if timestamps {
                                Some(chrono::Utc::now().to_rfc3339())
                            } else {
                                None
                            },
                            stream: match output {
                                bollard::container::LogOutput::StdOut { .. } => "stdout".to_string(),
                                bollard::container::LogOutput::StdErr { .. } => "stderr".to_string(),
                                _ => "console".to_string(),
                            },
                            log: output.to_string(),
                        };
                        yield Ok(log_line);
                    }
                    Err(e) => {
                        yield Err(anyhow::anyhow!("Docker logs error: {}", e));
                        break;
                    }
                }
            }
        })
    }

    // ========================================================================
    // Harvest Operations (for ceremony nourishment)
    // ========================================================================

    /// Commit a running container to a new image
    ///
    /// Creates a snapshot of the container's filesystem as a new image.
    /// Used during harvest to preserve container state before nourishment.
    ///
    /// # Arguments
    /// * `container_name` - Full container name (e.g., "zen-offering-mongodb")
    /// * `repo` - Repository name for the new image (e.g., "zen-harvest/mongodb")
    /// * `tag` - Tag for the new image (e.g., "20240115T120000")
    /// * `pause` - Whether to pause the container during commit (recommended for data consistency)
    ///
    /// # Returns
    /// The created image ID
    pub async fn commit_container(
        &self,
        container_name: &str,
        repo: &str,
        tag: &str,
        pause: bool,
    ) -> Result<String> {
        use bollard::container::Config;
        use bollard::image::CommitContainerOptions;

        tracing::info!(
            container = %container_name,
            repo = %repo,
            tag = %tag,
            pause,
            "Committing container to image"
        );

        let options = CommitContainerOptions {
            container: container_name,
            repo,
            tag,
            pause,
            ..Default::default()
        };

        let config = Config::<String>::default();

        let result = self
            .docker
            .commit_container(options, config)
            .await
            .context(format!("Failed to commit container {}", container_name))?;

        let image_id = result.id.unwrap_or_default();
        tracing::info!(
            container = %container_name,
            image_id = %image_id,
            "Container committed successfully"
        );

        Ok(image_id)
    }

    /// Get volume mounts for a container
    ///
    /// Returns a list of (host_path, container_path) tuples for all bind mounts.
    pub async fn get_container_volumes(&self, name: &str) -> Result<Vec<(String, String)>> {
        let container_name = zen_offering_container_name(name)?;
        let info = self
            .docker
            .inspect_container(&container_name, None::<InspectContainerOptions>)
            .await
            .context(format!("Failed to inspect container {}", container_name))?;

        let mounts = info.mounts.unwrap_or_default();

        let volumes: Vec<(String, String)> = mounts
            .iter()
            .filter_map(|m| {
                let source = m.source.as_ref()?;
                let dest = m.destination.as_ref()?;
                Some((source.clone(), dest.clone()))
            })
            .collect();

        tracing::debug!(
            container = %container_name,
            volume_count = volumes.len(),
            "Retrieved container volumes"
        );

        Ok(volumes)
    }

    /// Check if a managed container has the shared topology bind mount (TOPO-0002)
    ///
    /// Returns true if any mount destination matches CONTAINER_TOPOLOGY_DIR.
    /// Used by the health monitor to detect containers created before the
    /// topology mount was auto-injected.
    pub async fn has_topology_mount(&self, name: &str) -> Result<bool> {
        let volumes = self.get_container_volumes(name).await?;
        Ok(volumes
            .iter()
            .any(|(_, dest)| dest == garden_common::constants::paths::CONTAINER_TOPOLOGY_DIR))
    }

    /// Extract a container's runtime config as a `ContainerSpec`.
    ///
    /// Inspects the running container and returns its effective configuration.
    /// Filters out auto-injected env vars and topology mount since those are
    /// re-applied by `install_service()`.
    pub async fn inspect_container_spec(&self, name: &str) -> Result<ContainerSpec> {
        let container_name = zen_offering_container_name(name)?;
        let info = self
            .docker
            .inspect_container(&container_name, None::<InspectContainerOptions>)
            .await
            .context(format!("Failed to inspect container '{}'", container_name))?;

        let config = info.config.as_ref().context("Container has no config")?;

        // Image
        let image = config
            .image
            .clone()
            .unwrap_or_else(|| "<unknown>".to_string());

        // Command
        let command = config
            .cmd
            .as_ref()
            .filter(|c| !c.is_empty())
            .cloned();

        // Env (filter out auto-injected vars that install_service adds)
        let auto_prefixes = ["KOI_ENDPOINT=", "GARDEN_STONE_ENDPOINT=", "GARDEN_OFFERING_NAME="];
        let env: Vec<String> = config
            .env
            .as_ref()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|e| !auto_prefixes.iter().any(|prefix| e.starts_with(prefix)))
            .collect();

        // Ports: parse from host_config.port_bindings
        let mut ports = Vec::new();
        if let Some(ref host_config) = info.host_config {
            if let Some(ref bindings) = host_config.port_bindings {
                for (container_port_key, host_bindings) in bindings {
                    let container_port: u16 = container_port_key
                        .split('/')
                        .next()
                        .and_then(|p| p.parse().ok())
                        .unwrap_or(0);
                    if container_port == 0 {
                        continue;
                    }

                    if let Some(ref hb_list) = host_bindings {
                        for hb in hb_list {
                            let host_port: u16 = hb
                                .host_port
                                .as_deref()
                                .and_then(|p| p.parse().ok())
                                .unwrap_or(0);
                            if host_port > 0 {
                                ports.push((host_port, container_port));
                            }
                        }
                    }
                }
            }
        }

        // Volumes: from mounts, excluding auto-injected mounts:
        // - Topology mount (TOPO-0002)
        // - Config file mounts (config file injection)
        let topo_container_path = garden_common::constants::paths::CONTAINER_TOPOLOGY_DIR;
        let config_dir_prefix = format!("{}/config/", garden_common::constants::paths::data_dir());
        let volumes: Vec<(String, String)> = info
            .mounts
            .unwrap_or_default()
            .iter()
            .filter_map(|m| {
                let source = m.source.as_ref()?;
                let dest = m.destination.as_ref()?;
                if dest == topo_container_path {
                    return None;
                }
                // Filter out config file bind mounts (injected by install_service)
                if source.starts_with(&config_dir_prefix) {
                    return None;
                }
                Some((source.clone(), dest.clone()))
            })
            .collect();

        tracing::debug!(
            container = %container_name,
            image = %image,
            command = ?command,
            ports = ports.len(),
            env_vars = env.len(),
            volumes = volumes.len(),
            "Inspected container spec"
        );

        Ok(ContainerSpec {
            image,
            command,
            ports,
            environment: env,
            volumes,
            // Config files can't be introspected from Docker — this is
            // only used for spec comparison in needs_cycle(), where
            // config files are handled separately (file write + restart).
            config_files: vec![],
        })
    }

    /// Check if the running container matches the desired spec.
    ///
    /// Compares command, environment, and volumes. Returns `true` if the
    /// container needs to be recycled (stop → remove → create → start).
    pub async fn needs_cycle(&self, name: &str, desired: &ContainerSpec) -> Result<bool> {
        let running = self.inspect_container_spec(name).await?;

        // Compare command — use effective_command() so config file flags
        // (injected by install_service) are included in the desired side.
        let desired_cmd = desired.effective_command();
        if running.command != desired_cmd {
            tracing::debug!(
                service = %name,
                running = ?running.command,
                desired = ?desired_cmd,
                "Container command mismatch"
            );
            return Ok(true);
        }

        // Compare environment (sorted, ignoring auto-injected)
        let mut running_env = running.environment.clone();
        let mut desired_env = desired.environment.clone();
        running_env.sort();
        desired_env.sort();
        if running_env != desired_env {
            tracing::debug!(
                service = %name,
                "Container environment mismatch"
            );
            return Ok(true);
        }

        // Compare volumes (sorted by container path)
        let mut running_vols = running.volumes.clone();
        let mut desired_vols = desired.volumes.clone();
        running_vols.sort_by(|a, b| a.1.cmp(&b.1));
        desired_vols.sort_by(|a, b| a.1.cmp(&b.1));
        if running_vols != desired_vols {
            tracing::debug!(
                service = %name,
                "Container volumes mismatch"
            );
            return Ok(true);
        }

        Ok(false)
    }

    /// Ensure a managed container's `/etc/resolv.conf` points at the correct
    /// DNS servers (bridge gateway → systemd-resolved). Patches the file
    /// in-place via `docker exec` — no container restart needed.
    pub async fn reconcile_container_dns(&self, name: &str) -> Result<()> {
        let container_name = zen_offering_container_name(name)?;
        let net = self.container_networking(name).await;

        let nameservers = net.dns.iter()
            .map(|s| format!("nameserver {s}"))
            .collect::<Vec<_>>()
            .join("\\n");
        let search = if net.dns_search.is_empty() {
            String::new()
        } else {
            format!("\\nsearch {}", net.dns_search.join(" "))
        };
        let desired_content = format!("{nameservers}{search}\\n");

        // Read current resolv.conf
        let exec = self.docker.create_exec(
            &container_name,
            bollard::exec::CreateExecOptions {
                cmd: Some(vec!["cat", "/etc/resolv.conf"]),
                attach_stdout: Some(true),
                ..Default::default()
            },
        ).await.context("create exec for resolv.conf read")?;

        let output = self.docker.start_exec(
            &exec.id,
            None::<bollard::exec::StartExecOptions>,
        ).await.context("exec cat /etc/resolv.conf")?;

        let current = match output {
            bollard::exec::StartExecResults::Attached { mut output, .. } => {
                use futures_util::StreamExt;
                let mut buf = String::new();
                while let Some(Ok(chunk)) = output.next().await {
                    buf.push_str(&chunk.to_string());
                }
                buf
            }
            _ => String::new(),
        };

        // Check if first nameserver already matches
        let first_ns = current.lines()
            .find(|l| l.starts_with("nameserver "))
            .and_then(|l| l.strip_prefix("nameserver "))
            .unwrap_or("");
        let desired_primary = net.dns.first().map(|s| s.as_str()).unwrap_or("");

        if first_ns == desired_primary {
            return Ok(());
        }

        tracing::info!(
            service = %name,
            from = %first_ns,
            to = %desired_primary,
            "patching container DNS"
        );

        // Write new resolv.conf via printf
        let exec = self.docker.create_exec(
            &container_name,
            bollard::exec::CreateExecOptions {
                cmd: Some(vec![
                    "sh", "-c",
                    &format!("printf '{desired_content}' > /etc/resolv.conf"),
                ]),
                ..Default::default()
            },
        ).await.context("create exec for resolv.conf write")?;

        self.docker.start_exec(
            &exec.id,
            None::<bollard::exec::StartExecOptions>,
        ).await.context("exec write resolv.conf")?;

        Ok(())
    }

    /// Scan ALL Docker containers (running + stopped) and return a map of
    /// host_port → container_name for every port binding.
    ///
    /// Used during port allocation to avoid conflicts with stopped containers
    /// whose ports are not TCP-bindable but will conflict on restart.
    /// Optionally excludes a specific container (e.g., the one being reinstalled).
    pub async fn scan_port_occupancy(
        &self,
        exclude_container: Option<&str>,
    ) -> Result<HashMap<u16, String>> {
        let options = ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        };

        let containers = self
            .docker
            .list_containers(Some(options))
            .await
            .context("Failed to list containers for port occupancy scan")?;

        let mut occupied = HashMap::new();
        for container in containers {
            let name = container
                .names
                .as_ref()
                .and_then(|names| names.first())
                .map(|n| n.trim_start_matches('/').to_string())
                .unwrap_or_default();

            if name.is_empty() {
                continue;
            }

            // Skip the container we're about to redeploy
            if let Some(exclude) = exclude_container {
                if name == exclude {
                    continue;
                }
            }

            // Extract port bindings from the container summary's ports field
            if let Some(ports) = container.ports {
                for port in ports {
                    if let Some(public_port) = port.public_port {
                        if public_port > 0 {
                            occupied.insert(public_port, name.clone());
                        }
                    }
                }
            }
        }

        Ok(occupied)
    }

    /// Get the actual port bindings from a running container.
    ///
    /// Returns `Vec<(host_port, container_port)>` reflecting what Docker is actually
    /// bound to, which may differ from the manifest if ports were remapped due to conflicts.
    pub async fn get_container_ports(&self, name: &str) -> Result<Vec<(u16, u16)>> {
        let container_name = zen_offering_container_name(name)?;
        let info = self
            .docker
            .inspect_container(&container_name, None::<InspectContainerOptions>)
            .await
            .context(format!("Failed to inspect container '{}'", container_name))?;

        let mut ports = Vec::new();
        if let Some(ref host_config) = info.host_config {
            if let Some(ref bindings) = host_config.port_bindings {
                for (container_port_key, host_bindings) in bindings {
                    let container_port: u16 = container_port_key
                        .split('/')
                        .next()
                        .and_then(|p| p.parse().ok())
                        .unwrap_or(0);
                    if container_port == 0 {
                        continue;
                    }
                    if let Some(ref hb_list) = host_bindings {
                        for hb in hb_list {
                            let host_port: u16 = hb
                                .host_port
                                .as_deref()
                                .and_then(|p| p.parse().ok())
                                .unwrap_or(0);
                            if host_port > 0 {
                                ports.push((host_port, container_port));
                            }
                        }
                    }
                }
            }
        }
        Ok(ports)
    }

    /// Execute a command inside a running container
    ///
    /// Used for quiesce/resume operations during ceremonies.
    pub async fn exec_in_container(
        &self,
        name: &str,
        cmd: &[String],
        timeout_secs: u32,
    ) -> Result<(i64, String)> {
        use bollard::exec::{CreateExecOptions, StartExecResults};

        let container_name = zen_offering_container_name(name)?;

        tracing::debug!(
            container = %container_name,
            cmd = ?cmd,
            "Executing command in container"
        );

        let exec = self
            .docker
            .create_exec(
                &container_name,
                CreateExecOptions {
                    cmd: Some(cmd.to_vec()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .context("Failed to create exec")?;

        let output = match self.docker.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                let deadline = tokio::time::Instant::now()
                    + tokio::time::Duration::from_secs(timeout_secs as u64);

                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => {
                            anyhow::bail!("Exec command timed out after {}s", timeout_secs);
                        }
                        item = output.next() => {
                            match item {
                                Some(Ok(msg)) => result.push_str(&msg.to_string()),
                                Some(Err(e)) => anyhow::bail!("Exec error: {}", e),
                                None => break,
                            }
                        }
                    }
                }
                result
            }
            StartExecResults::Detached => String::new(),
        };

        // Get exit code
        let inspect = self.docker.inspect_exec(&exec.id).await?;
        let exit_code = inspect.exit_code.unwrap_or(-1);

        Ok((exit_code, output))
    }

    /// Prune dangling Docker images
    ///
    /// Returns (count_pruned, bytes_reclaimed).
    pub async fn prune_dangling_images(&self) -> Result<(usize, u64)> {
        let mut filters = HashMap::new();
        filters.insert("dangling", vec!["true"]);

        let options = Some(PruneImagesOptions { filters });
        let response = self
            .docker
            .prune_images(options)
            .await
            .context("Failed to prune dangling Docker images")?;

        let count = response
            .images_deleted
            .as_ref()
            .map(|v| v.len())
            .unwrap_or(0);
        let bytes = response.space_reclaimed.unwrap_or(0) as u64;

        Ok((count, bytes))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LogLine {
    pub timestamp: Option<String>,
    pub stream: String,
    pub log: String,
}
