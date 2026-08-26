//! The Docker adapter (OFFERINGS.md §4, D10): the first real world beneath
//! the seam. Speaks bollard; keeps PoC-compatible `zen-offering-*` naming.
//! Everything Docker-specific in v1 lives in this file.

use super::runtime::{
    DeployOutcome, Runtime, RuntimeError, RunningWorkload, VolumeMount, WorkloadSpec,
};
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
    StopContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::{HostConfig, PortBinding, RestartPolicyNameEnum};
use bollard::Docker;
use futures::StreamExt;
use std::collections::HashMap;

/// Container name prefix — byte-compatible with the PoC fleet so tooling
/// and habits carry over (poc constants/mod.rs:135).
pub const CONTAINER_PREFIX: &str = "zen-offering-";

pub struct DockerRuntime {
    docker: Docker,
}

impl DockerRuntime {
    /// Connect via platform defaults (Windows named pipe / Unix socket /
    /// DOCKER_HOST). Connection failures surface as a named startup step
    /// (L17), not a silent fallback.
    pub fn connect() -> Result<Self, RuntimeError> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
        Ok(Self { docker })
    }

    fn container_name(name: &str) -> String {
        format!("{CONTAINER_PREFIX}{name}")
    }

    async fn pull(&self, image: &str) -> Result<(), RuntimeError> {
        let opts = CreateImageOptions {
            from_image: image.to_string(),
            ..Default::default()
        };
        let mut stream = self.docker.create_image(Some(opts), None, None);
        while let Some(info) = stream.next().await {
            match info {
                Ok(progress) => {
                    if let Some(status) = progress.status {
                        tracing::debug!(image, status, "pull");
                    }
                }
                Err(e) => return Err(RuntimeError::Failed(format!("pull {image}: {e}"))),
            }
        }
        Ok(())
    }

    /// Translate a Docker container state into offering status vocabulary.
    fn map_state(running: Option<bool>, restarting: Option<bool>) -> String {
        match (running, restarting) {
            (Some(true), _) => garden_glossary::offering::RUNNING.into(),
            (_, Some(true)) => garden_glossary::offering::DEGRADED.into(),
            _ => garden_glossary::offering::STOPPED.into(),
        }
    }
}

#[async_trait::async_trait]
impl Runtime for DockerRuntime {
    fn kind(&self) -> &'static str {
        "docker"
    }

    async fn host_ports(&self, name: &str) -> HashMap<String, u16> {
        let mut out = HashMap::new();
        if let Ok(inspect) =
            self.docker.inspect_container(&Self::container_name(name), None).await
            && let Some(ports) = inspect.network_settings.and_then(|n| n.ports)
        {
            for (key, bindings) in ports {
                for b in bindings.into_iter().flatten() {
                    if let Some(host) =
                        b.host_port.as_deref().and_then(|h| h.parse::<u16>().ok())
                    {
                        out.insert(key.clone(), host);
                    }
                }
            }
        }
        out
    }

    async fn deploy(
        &self,
        name: &str,
        spec: &WorkloadSpec,
    ) -> Result<DeployOutcome, RuntimeError> {
        let full = Self::container_name(name);

        // Idempotence: already placed and running → say so.
        if let Some(existing) = self.inspect(name).await {
            if existing.status == garden_glossary::offering::RUNNING {
                return Ok(DeployOutcome::AlreadyRunning);
            }
        } else {
            self.pull(&spec.image).await?;

            let exposed: HashMap<String, HashMap<(), ()>> =
                spec.named_ports.values().map(|p| (format!("{p}/tcp"), HashMap::new())).collect();
            let bindings: HashMap<String, Option<Vec<PortBinding>>> = spec
                .named_ports
                .values()
                .map(|p| {
                    (
                        format!("{p}/tcp"),
                        Some(vec![PortBinding { host_ip: None, host_port: None }]),
                    )
                })
                .collect();

            let mounts = ensure_volume_dirs(spec.volumes.as_slice())?;
            let host_config = HostConfig {
                binds: Some(mounts),
                port_bindings: Some(bindings),
                restart_policy: Some(bollard::models::RestartPolicy {
                    name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
                    maximum_retry_count: None,
                }),
                ..Default::default()
            };

            let config = Config {
                image: Some(spec.image.clone()),
                labels: Some(HashMap::from([("zg.offering".into(), name.into())])),
                exposed_ports: Some(exposed),
                env: Some(spec.env.iter().map(|(k, v)| format!("{k}={v}")).collect()),
                host_config: Some(host_config),
                ..Default::default()
            };

            self.docker
                .create_container(
                    Some(CreateContainerOptions { name: full.clone(), platform: None }),
                    config,
                )
                .await
                .map_err(|e| RuntimeError::Failed(format!("create {full}: {e}")))?;
        }

        self.start(name).await?;
        Ok(DeployOutcome::Created)
    }

    async fn start(&self, name: &str) -> Result<(), RuntimeError> {
        self.docker
            .start_container::<String>(&Self::container_name(name), None)
            .await
            .map_err(|e| RuntimeError::Failed(e.to_string()))
    }

    async fn stop(&self, name: &str) -> Result<(), RuntimeError> {
        self.docker
            .stop_container(&Self::container_name(name), None::<StopContainerOptions>)
            .await
            .map_err(|e| RuntimeError::Failed(e.to_string()))
    }

    async fn remove(&self, name: &str) -> Result<(), RuntimeError> {
        self.docker
            .remove_container(
                &Self::container_name(name),
                Some(RemoveContainerOptions { force: true, v: false, ..Default::default() }),
            )
            .await
            .map_err(|e| RuntimeError::Failed(e.to_string()))
    }

    async fn inspect(&self, name: &str) -> Option<RunningWorkload> {
        let inspect = self.docker.inspect_container(&Self::container_name(name), None).await.ok()?;
        let state = inspect.state.clone().unwrap_or_default();
        let status = Self::map_state(state.running, state.restarting);
        let image =
            inspect.config.as_ref().and_then(|c| c.image.clone()).unwrap_or_default();
        Some(RunningWorkload { name: name.to_string(), image, status, port_map: Default::default() })
    }

    async fn list(&self) -> Vec<RunningWorkload> {
        let opts: ListContainersOptions<String> = ListContainersOptions {
            all: true,
            filters: HashMap::from([("name".into(), vec![CONTAINER_PREFIX.into()])]),
            ..Default::default()
        };
        match self.docker.list_containers(Some(opts)).await {
            Ok(list) => list
                .into_iter()
                .filter_map(|c| {
                    let full_name = c.names.as_ref()?.first()?.to_string();
                    let status = Self::map_state(
                        c.state.as_deref().map(|s| s == "running"),
                        c.status.as_deref().map(|s| s == "restarting"),
                    );
                    Some(RunningWorkload {
                        name: full_name.trim_start_matches('/').trim_start_matches(CONTAINER_PREFIX).to_string(),
                        image: c.image.clone().unwrap_or_default(),
                        status,
                        port_map: Default::default(),
                    })
                })
                .collect(),
            Err(e) => {
                tracing::warn!(error = %e, "docker list failed");
                Vec::new()
            }
        }
    }
}

/// Volume hosts must exist before Docker can bind-mount them.
fn ensure_volume_dirs(volumes: &[VolumeMount]) -> Result<Vec<String>, RuntimeError> {
    let mut binds = Vec::with_capacity(volumes.len());
    for v in volumes {
        std::fs::create_dir_all(&v.host_path)
            .map_err(|e| RuntimeError::Failed(format!("volume {}: {e}", v.host_path)))?;
        binds.push(format!("{}:{}", v.host_path, v.container_path));
    }
    Ok(binds)
}
