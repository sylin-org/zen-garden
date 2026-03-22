mod exec;
mod inspect;
mod lifecycle;
mod naming;
mod port;
mod spec;

// Re-export public API (preserves all existing import paths)
pub use naming::{decode_zen_offering_container_name, zen_offering_container_name};
pub use port::check_and_remediate_ports;
pub use spec::{ContainerSpec, LogLine};

use anyhow::{Context, Result};
use bollard::Docker as BollardDocker;

use spec::ContainerNetworking;

pub struct Client {
    docker: BollardDocker,
}

impl Client {
    pub fn new() -> Result<Self> {
        #[cfg(target_os = "windows")]
        let docker = {
            tracing::debug!("Connecting to Docker via Windows named pipe");
            BollardDocker::connect_with_named_pipe_defaults().context(
                "Failed to connect to Docker daemon via named pipe (is Docker Desktop running?)",
            )?
        };

        #[cfg(target_os = "linux")]
        let docker = {
            tracing::debug!("Connecting to Docker via Unix socket");
            BollardDocker::connect_with_socket_defaults()
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
            .inspect_network(
                "bridge",
                None::<bollard::query_parameters::InspectNetworkOptions>,
            )
            .await
            .ok()?;

        network.ipam?.config?.into_iter().find_map(|c| c.gateway)
    }

    /// Build container networking configuration.
    ///
    /// Every managed container gets:
    /// - DNS pointing to systemd-resolved (via Docker bridge gateway)
    /// - Search domain `zengarden` (Koi DNS zone, forwarded by resolved)
    /// - Environment variables for reaching host services (Koi HTTP, Moss API)
    /// - `host.docker.internal` extra host mapping
    pub(super) async fn container_networking(&self, name: &str) -> ContainerNetworking {
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
}
