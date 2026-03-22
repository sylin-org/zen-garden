use anyhow::{Context, Result};
use bollard::container::{InspectContainerOptions, ListContainersOptions, StatsOptions};
use bollard::models::HealthStatusEnum;
use futures_util::stream::TryStreamExt;
use garden_common::constants::OFFERING_CONTAINER_PREFIX;
use garden_common::{ServiceHealthStatus, ServiceStatus};
use std::collections::HashMap;

use super::naming::{decode_zen_offering_container_name, zen_offering_container_name};
use super::spec::ContainerSpec;
use super::Client;

impl Client {
    pub(super) async fn container_exists(&self, name: &str) -> Result<bool> {
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
        if let Some(health) = state.health
            && let Some(status) = health.status {
                return Ok(match status {
                    HealthStatusEnum::HEALTHY => ServiceHealthStatus::Healthy,
                    HealthStatusEnum::UNHEALTHY => ServiceHealthStatus::Degraded,
                    HealthStatusEnum::STARTING => ServiceHealthStatus::Degraded,
                    HealthStatusEnum::NONE | HealthStatusEnum::EMPTY => {
                        ServiceHealthStatus::Healthy
                    }
                });
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

        if let Some(state) = inspect.state
            && let Some(started_at) = state.started_at {
                // Parse ISO 8601 timestamp
                if let Ok(started) = chrono::DateTime::parse_from_rfc3339(&started_at) {
                    let now = chrono::Utc::now();
                    let duration = now.signed_duration_since(started);
                    return Ok(duration.num_seconds().max(0) as u64);
                }
            }

        Ok(0)
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
        let command = config.cmd.as_ref().filter(|c| !c.is_empty()).cloned();

        // Env (filter out auto-injected vars that install_service adds)
        let auto_prefixes = [
            "KOI_ENDPOINT=",
            "GARDEN_STONE_ENDPOINT=",
            "GARDEN_OFFERING_NAME=",
        ];
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
        if let Some(ref host_config) = info.host_config
            && let Some(ref bindings) = host_config.port_bindings {
                for (container_port_key, host_bindings) in bindings {
                    let container_port: u16 = container_port_key
                        .split('/')
                        .next()
                        .and_then(|p| p.parse().ok())
                        .unwrap_or(0);
                    if container_port == 0 {
                        continue;
                    }

                    if let Some(hb_list) = host_bindings {
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
            // Config files can't be introspected from Docker -- this is
            // only used for spec comparison in needs_cycle(), where
            // config files are handled separately (file write + restart).
            config_files: vec![],
        })
    }

    /// Check if the running container matches the desired spec.
    ///
    /// Compares command, environment, and volumes. Returns `true` if the
    /// container needs to be recycled (stop -> remove -> create -> start).
    pub async fn needs_cycle(&self, name: &str, desired: &ContainerSpec) -> Result<bool> {
        let running = self.inspect_container_spec(name).await?;

        // Compare command -- use effective_command() so config file flags
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
        if let Some(ref host_config) = info.host_config
            && let Some(ref bindings) = host_config.port_bindings {
                for (container_port_key, host_bindings) in bindings {
                    let container_port: u16 = container_port_key
                        .split('/')
                        .next()
                        .and_then(|p| p.parse().ok())
                        .unwrap_or(0);
                    if container_port == 0 {
                        continue;
                    }
                    if let Some(hb_list) = host_bindings {
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
        Ok(ports)
    }
}
