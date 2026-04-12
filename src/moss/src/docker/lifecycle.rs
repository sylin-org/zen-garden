use anyhow::{Context, Result};
use bollard::models::{ContainerCreateBody, ContainerCreateResponse, HostConfig, PortBinding};
use bollard::query_parameters::{
    CreateContainerOptions, InspectContainerOptions, KillContainerOptions, RemoveContainerOptions,
    RestartContainerOptions, StartContainerOptions, StopContainerOptions,
};
use garden_common::console::{self, ConsolePrinter};
use std::collections::HashMap;
use std::sync::Arc;

use super::ContainerRuntime;
use super::naming::zen_offering_container_name;
use super::port::check_and_remediate_ports;
use super::spec::ContainerSpec;

impl ContainerRuntime {
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
            .start_container(&container_name, None::<StartContainerOptions>)
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

    /// Rename a service container (stop -> rename -> start).
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
                bollard::query_parameters::RenameContainerOptions {
                    name: new_container.clone(),
                },
            )
            .await
            .with_context(|| {
                format!("Failed to rename container {old_container} -> {new_container}")
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
                format!("{} -> {}", name, spec.image),
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

        // Build and start the container
        let (config, _binds_port_bindings) = self
            .build_container_config(name, spec, &resolved_ports)
            .await?;

        // Create container
        let response: ContainerCreateResponse = self
            .docker
            .create_container(
                Some(CreateContainerOptions {
                    name: Some(container_name.clone()),
                    platform: String::new(),
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
            .start_container(&container_name, None::<StartContainerOptions>)
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
                    v: false, // Preserve volumes -- this is a reconfigure, not an uninstall
                    force: false,
                    link: false,
                }),
            )
            .await
            .context("Failed to remove container for recreate")?;

        // Verify the container is fully removed before creating the replacement.
        // Docker's remove API can return before the container is fully cleaned up,
        // which causes "name already in use" errors on the subsequent create.
        self.await_container_removed(&container_name).await?;

        tracing::info!(service = %name, "Old container removed (volumes preserved)");

        // Scan Docker port occupancy (old container just removed, so its ports are freed)
        let docker_occupied = self.scan_port_occupancy(None).await?;

        // Resolve ports (same logic as install_service)
        let resolved_ports = check_and_remediate_ports(&spec.ports, &docker_occupied).await?;

        // Build and start the container
        let (config, _) = self
            .build_container_config(name, spec, &resolved_ports)
            .await?;

        // Create new container
        let response: ContainerCreateResponse = self
            .docker
            .create_container(
                Some(CreateContainerOptions {
                    name: Some(container_name.clone()),
                    platform: String::new(),
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
            .start_container(&container_name, None::<StartContainerOptions>)
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
    /// Much less destructive than recreate -- the container keeps its identity,
    /// mounts, and configuration. Only the process inside is restarted.
    /// Used after config file changes (file_restart policy).
    pub async fn restart_service(&self, name: &str) -> Result<()> {
        let container_name = zen_offering_container_name(name)?;
        tracing::info!(service = %name, "Restarting container");
        self.docker
            .restart_container(
                &container_name,
                Some(RestartContainerOptions {
                    t: Some(10),
                    signal: None,
                }),
            )
            .await
            .context("Failed to restart container")?;
        tracing::info!(service = %name, "Container restarted successfully");
        Ok(())
    }

    /// Send a signal to a running container (e.g., SIGHUP for config reload).
    ///
    /// Least destructive option -- zero downtime. The process stays running
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
                format!("{} -> {}", name, spec.image),
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

    /// Build the Docker container `Config` from a `ContainerSpec` and resolved ports.
    ///
    /// Shared by `install_service` and `recreate_service` to avoid duplicating
    /// the port binding, volume mount, config file injection, and networking logic.
    async fn build_container_config(
        &self,
        name: &str,
        spec: &ContainerSpec,
        resolved_ports: &[(u16, u16)],
    ) -> Result<(ContainerCreateBody, ())> {
        // Configure port bindings (using resolved ports)
        let mut port_bindings = HashMap::new();
        for (host_port, container_port) in resolved_ports {
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
        // Cross-cutting infrastructure concern -- every managed container gets
        // read-write access to the topology directory for pre-warmed discovery.
        let topo_host = garden_common::constants::paths::topology_dir();
        let topo_container = garden_common::constants::paths::CONTAINER_TOPOLOGY_DIR;
        binds.push(format!("{}:{}", topo_host, topo_container));

        // Config file injection: create empty config files on the host and
        // bind-mount them into the container. This lets config patches write
        // content to these files and restart -- no container recreation needed.
        let mut effective_cmd = spec.command.clone();
        for cf in &spec.config_files {
            let host_dir = garden_common::constants::paths::offering_config_dir(name);
            tokio::fs::create_dir_all(&host_dir)
                .await
                .context(format!("Failed to create config dir: {}", host_dir))?;

            let filename = std::path::Path::new(&cf.path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "config".to_string());
            let host_path = format!("{}/{}", host_dir, filename);

            // Create empty config file if it doesn't exist (idempotent)
            if !tokio::fs::try_exists(&host_path).await.unwrap_or(false) {
                let empty_content = cf.format.empty_content();
                tokio::fs::write(&host_path, empty_content)
                    .await
                    .context(format!("Failed to write empty config file: {}", host_path))?;
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

        // GPU device requests from manifest deploy.resources.reservations.devices
        let device_requests = if !spec.device_requests.is_empty() {
            Some(
                spec.device_requests
                    .iter()
                    .map(|dr| bollard::models::DeviceRequest {
                        driver: if dr.driver.is_empty() {
                            None
                        } else {
                            Some(dr.driver.clone())
                        },
                        count: Some(dr.count),
                        capabilities: Some(dr.capabilities.clone()),
                        ..Default::default()
                    })
                    .collect(),
            )
        } else {
            None
        };

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
            device_requests,
            ..Default::default()
        };

        let config = ContainerCreateBody {
            image: Some(spec.image.clone()),
            cmd: effective_cmd,
            env: Some(full_env),
            host_config: Some(host_config),
            ..Default::default()
        };

        Ok((config, ()))
    }

    /// Wait for a container to be fully removed after a `remove_container` call.
    ///
    /// Docker's remove API can return before the container metadata is fully
    /// cleaned up. Re-creating a container with the same name immediately after
    /// removal can hit "name already in use" races. This method polls
    /// `inspect_container` until it returns 404 (not found), with a bounded
    /// retry to avoid infinite loops.
    async fn await_container_removed(&self, container_name: &str) -> Result<()> {
        const MAX_ATTEMPTS: u32 = 10;
        const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

        for attempt in 1..=MAX_ATTEMPTS {
            match self
                .docker
                .inspect_container(container_name, None::<InspectContainerOptions>)
                .await
            {
                // Container still exists — wait and retry
                Ok(_) => {
                    tracing::debug!(
                        container = %container_name,
                        attempt,
                        "Container still present after remove, waiting"
                    );
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
                // 404 Not Found — container is gone (expected path)
                Err(bollard::errors::Error::DockerResponseServerError {
                    status_code: 404, ..
                }) => return Ok(()),
                // Transient error (network, socket) — retry rather than
                // assuming the container is gone (OFFER-0008).
                Err(e) => {
                    tracing::debug!(
                        container = %container_name,
                        attempt,
                        error = %e,
                        "Transient error checking container removal, retrying"
                    );
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
            }
        }

        // If we exhausted retries, warn but don't fail — the create call will
        // produce a clear error if the name is still taken.
        tracing::warn!(
            container = %container_name,
            "Container still visible after {} removal checks ({:.1}s); proceeding anyway",
            MAX_ATTEMPTS,
            MAX_ATTEMPTS as f64 * POLL_INTERVAL.as_secs_f64()
        );
        Ok(())
    }
}
