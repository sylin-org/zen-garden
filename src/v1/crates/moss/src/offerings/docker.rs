//! The Docker adapter (OFFERINGS.md §4): the first real world beneath the
//! seam. Speaks bollard; keeps PoC-compatible `zen-offering-*` naming.
//! Everything Docker-specific in v1 lives in this file.

use super::model::WorkloadSpec;
use super::runtime::{Observed, Placement, Runtime, RuntimeError};
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
        let docker =
            Docker::connect_with_local_defaults().map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
        Ok(Self { docker })
    }

    /// Container name for an offering FQN. Moniker-slugged (glossary::fqn):
    /// `memcached` (= memcached::default) keeps its PoC-compatible
    /// `zen-offering-memcached`; foreign instances slug their full FQN
    /// (`mc::prod` → `zen-offering-mc_prod`). Since the grammar bans `:`
    /// inside names outright, slugging is injective — no two offerings
    /// can collide here.
    pub(crate) fn container_name(offering_fqn: &str) -> String {
        format!(
            "{CONTAINER_PREFIX}{}",
            super::directory::slug(&garden_glossary::fqn::moniker(offering_fqn))
        )
    }

    async fn pull(&self, image: &str) -> Result<(), RuntimeError> {
        let opts = CreateImageOptions { from_image: image.to_string(), ..Default::default() };
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

    fn map_state(running: Option<bool>, restarting: Option<bool>) -> String {
        match (running, restarting) {
            (Some(true), _) => garden_glossary::offering::RUNNING.into(),
            (_, Some(true)) => garden_glossary::offering::DEGRADED.into(),
            _ => garden_glossary::offering::STOPPED.into(),
        }
    }

    fn restart_policy(spec: &WorkloadSpec) -> Option<bollard::models::RestartPolicy> {
        let name = match spec.restart.as_str() {
            "always" => RestartPolicyNameEnum::ALWAYS,
            "unless-stopped" => RestartPolicyNameEnum::UNLESS_STOPPED,
            _ => return None,
        };
        Some(bollard::models::RestartPolicy { name: Some(name), maximum_retry_count: None })
    }
}

#[async_trait::async_trait]
impl Runtime for DockerRuntime {
    fn kind(&self) -> &'static str {
        "docker"
    }

    /// Ensure reality matches spec: pull when absent, create when missing,
    /// start when stopped. Port NAMES travel as labels so [`observe`] can
    /// translate Docker's "80/tcp" keys back into PORT-0001 vocabulary.
    async fn place(
        &self,
        name: &str,
        spec: &WorkloadSpec,
    ) -> Result<Placement, RuntimeError> {
        let full = Self::container_name(name);

        let observed = self.observe(name).await;
        if observed.is_none() {
            self.pull(&spec.image).await?;

            let exposed: HashMap<String, HashMap<(), ()>> = spec
                .named_ports
                .iter()
                .map(|(_, &cp)| (format!("{cp}/tcp"), HashMap::new()))
                .collect();
            let bindings: HashMap<String, Option<Vec<PortBinding>>> = spec
                .named_ports
                .iter()
                .map(|(role, &cp)| {
                    // Port ledger as placement constraint: bind the
                    // REMEMBERED host port for this role when one exists
                    // (§6.4); fall back to ephemeral assignment otherwise.
                    let host = spec.allocations.get(role).map(|a| a.home.to_string());
                    if host.is_none() {
                        // Transitional only: pre-migration specs carry no
                        // allocations. Post-migration every role is bound
                        // explicitly (D14's real fix).
                        tracing::warn!(offering = %name, role = %role, "no allocation recorded - dynamic bind");
                    }
                    (
                        format!("{cp}/tcp"),
                        Some(vec![PortBinding { host_ip: None, host_port: host }]),
                    )
                })
                .collect();

            let mut labels: HashMap<String, String> =
                HashMap::from([("zg.offering".into(), name.into())]);
            for (role, &container_port) in &spec.named_ports {
                labels.insert(format!("zg.port.{role}"), container_port.to_string());
            }

            let mut binds = Vec::with_capacity(spec.volumes.len() + spec.configs.len());
            for v in &spec.volumes {
                std::fs::create_dir_all(&v.host_path).map_err(|e| {
                    RuntimeError::Failed(format!("volume {}: {e}", v.host_path))
                })?;
                binds.push(format!("{}:{}", v.host_path, v.container_path));
            }
            // Materialized configs: write content, mount read-only.
            for cfg in &spec.configs {
                if let Some(parent) = std::path::Path::new(&cfg.host_path).parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        RuntimeError::Failed(format!("config dir {}: {e}", parent.display()))
                    })?;
                }
                // If a previous failed run left a directory here (Docker's
                // placeholder habit), clear it so the real file can land.
                if std::path::Path::new(&cfg.host_path).is_dir() {
                    let _ = std::fs::remove_dir_all(&cfg.host_path);
                }
                std::fs::write(&cfg.host_path, &cfg.content).map_err(|e| {
                    RuntimeError::Failed(format!("config {}: {e}", cfg.host_path))
                })?;
                binds.push(format!("{}:{}:ro", cfg.host_path, cfg.container_path));
            }

            let config = Config {
                image: Some(spec.image.clone()),
                labels: Some(labels),
                exposed_ports: Some(exposed),
                env: Some(spec.env.iter().map(|(k, v)| format!("{k}={v}")).collect()),
                host_config: Some(HostConfig {
                    binds: Some(binds),
                    port_bindings: Some(bindings),
                    restart_policy: Self::restart_policy(spec),
                    ..Default::default()
                }),
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

        // Ensure running (create above implies a fresh stopped container).
        if !self.observe(name).await.map(|o| o.running).unwrap_or(false) {
            self.start(name).await?;
        }

        let placed = self.observe(name).await.ok_or_else(|| {
            RuntimeError::Failed(format!("{full} placed but unobservable"))
        })?;
        Ok(Placement { named_host_ports: placed.named_host_ports })
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

    async fn observe(&self, name: &str) -> Option<Observed> {
        let inspect = self.docker.inspect_container(&Self::container_name(name), None).await.ok()?;
        let state = inspect.state.clone().unwrap_or_default();
        let running = state.running.unwrap_or(false);
        let restarting = state.restarting.unwrap_or(false);

        // Named translation via labels: zg.port.<role> = <container_port>.
        let labels = inspect.config.as_ref().and_then(|c| c.labels.clone()).unwrap_or_default();
        let raw = inspect.network_settings.and_then(|n| n.ports).unwrap_or_default();
        let mut named_host_ports = HashMap::new();
        for (key, bindings) in raw {
            let Some(container_port) = key.split('/').next().and_then(|p| p.parse::<u16>().ok())
            else {
                continue;
            };
            let role = labels
                .iter()
                .find(|(k, v)| {
                    k.starts_with("zg.port.")
                        && v.parse::<u16>().map(|v| v == container_port).unwrap_or(false)
                })
                .map(|(k, _)| k.trim_start_matches("zg.port.").to_string());
            for b in bindings.into_iter().flatten() {
                if let Some(host) = b.host_port.as_deref().and_then(|h| h.parse::<u16>().ok()) {
                    let name = role.clone().unwrap_or_else(|| container_port.to_string());
                    named_host_ports.insert(name, host);
                }
            }
        }

        let status = Self::map_state(state.running, Some(restarting));
        let _ = status;
        Some(Observed { running, named_host_ports })
    }

    async fn rehearse_run(
        &self,
        name: &str,
        spec: &super::model::WorkloadSpec,
        volumes_root: &std::path::Path,
        wait_secs: u64,
    ) -> Option<super::runtime::RehearsalFate> {
        use bollard::container::{Config, CreateContainerOptions};
        use bollard::models::HostConfig;
        let full = format!("zen-rehearsal-{}", Self::container_name(name).trim_start_matches(crate::offerings::docker::CONTAINER_PREFIX));

        // No exposed ports, no bindings: a rehearsal never publishes or
        // collides — it boots restored data and speaks to no one.
        let binds: Vec<String> = spec
            .volumes
            .iter()
            .map(|v| {
                let vol_name = std::path::Path::new(&v.host_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| v.host_path.clone());
                format!("{}:{}:rw", volumes_root.join(vol_name).display(), v.container_path)
            })
            .collect();
        let cfg_files: Vec<String> = spec
            .config_files
            .keys()
            .map(|cpath| {
                format!(
                    "{}:{}:ro",
                    volumes_root.join(".config").join(cpath.replace('/', "_")).display(),
                    cpath
                )
            })
            .collect();

        let config = Config {
            image: Some(spec.image.clone()),
            env: Some(spec.env.iter().map(|(k, v)| format!("{k}={v}")).collect()),
            host_config: Some(HostConfig {
                binds: Some([binds, cfg_files].concat()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // A stale rehearsal container from a crashed run never blocks the
        // proof: force-remove before create.
        let _ = self
            .docker
            .remove_container(&full, Some(bollard::container::RemoveContainerOptions { force: true, ..Default::default() }))
            .await;
        self.docker
            .create_container(
                Some(CreateContainerOptions { name: full.clone(), platform: None }),
                config,
            )
            .await
            .map_err(|e| super::runtime::RuntimeError::Failed(format!("create {full}: {e}")))
            .ok()?;
        if let Err(e) = self.docker.start_container::<String>(&full, None).await {
            let _ = self.docker.remove_container(&full, Some(bollard::container::RemoveContainerOptions { force: true, ..Default::default() })).await;
            return Some(super::runtime::RehearsalFate {
                stayed_running: false,
                state: format!("start-failed: {e}"),
                exit_code: None,
                ran_secs: 0,
            });
        }

        // Hold the window, sampling: a container that dies mid-window is
        // red immediately — no point waiting out the clock.
        let started = std::time::Instant::now();
        let mut stayed_running = true;
        let mut state = "running".to_string();
        let mut exit_code = None;
        while std::time::Instant::now().duration_since(started).as_secs() < wait_secs {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if let Ok(inspect) = self.docker.inspect_container(&full, None).await {
                let s = inspect.state.unwrap_or_default();
                if !s.running.unwrap_or(false) {
                    stayed_running = false;
                    state = "exited".into();
                    exit_code = s.exit_code;
                    break;
                }
            }
        }
        let ran_secs = started.elapsed().as_secs();
        let fate = super::runtime::RehearsalFate { stayed_running, state, exit_code, ran_secs };

        // The proof never lingers: the rehearsal container is removed
        // whatever the verdict.
        let _ = self
            .docker
            .remove_container(&full, Some(bollard::container::RemoveContainerOptions { force: true, ..Default::default() }))
            .await;
        Some(fate)
    }

    /// The pull half of nourish (J3): refresh the tag, compare IDs.
    /// A pull of an up-to-date image is cheap (manifest check only).
    async fn refresh_image(
        &self,
        image: &str,
    ) -> Option<Result<super::runtime::ImageRefresh, super::runtime::RuntimeError>> {
        let before = self
            .docker
            .inspect_image(image)
            .await
            .ok()
            .map(|i| i.id.unwrap_or_default());
        let mut pull = self.docker.create_image(
            Some(bollard::image::CreateImageOptions {
                from_image: image.to_string(),
                ..Default::default()
            }),
            None,
            None,
        );
        while let Some(step) = pull.next().await {
            if let Err(e) = step {
                return Some(Err(super::runtime::RuntimeError::Failed(format!(
                    "pull {image}: {e}"
                ))));
            }
        }
        let after = self
            .docker
            .inspect_image(image)
            .await
            .ok()
            .and_then(|i| i.id)
            .unwrap_or_default();
        Some(Ok(super::runtime::ImageRefresh {
            changed: before.as_deref() != Some(after.as_str()) && !after.is_empty(),
            id: after,
        }))
    }

    async fn list(&self) -> Vec<super::runtime::PlacedRef> {
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
                    Some(super::runtime::PlacedRef {
                        // Slugged form of the offering name: instance names
                        // ('mc:prod') read back as 'mc_prod'. No consumer
                        // round-trips this yet; adoption detection will pair
                        // against known registry slugs when it lands.
                        name: full_name
                            .trim_start_matches('/')
                            .trim_start_matches(CONTAINER_PREFIX)
                            .to_string(),
                        status: crate::offerings::model::Status::parse_or_unknown(&status),
                    })
                })
                .collect(),
            Err(e) => {
                tracing::warn!(error = %e, "docker list failed");
                Vec::new()
            }
        }
    }

    /// Docker-logs semantics: history first (bounded by `tail`), then
    /// live follow. stdout/stderr keep their channels; timestamps are
    /// the engine's, not ours.
    fn logs_stream(
        &self,
        name: &str,
        tail: Option<u64>,
        timestamps: bool,
    ) -> Option<super::runtime::LogStream> {
        use bollard::container::LogsOptions;
        use futures::StreamExt as _;
        let container = Self::container_name(name);
        let options = LogsOptions {
            follow: true,
            stdout: true,
            stderr: true,
            timestamps,
            tail: tail.map(|n| n.to_string()).unwrap_or_default(),
            ..Default::default()
        };
        let source = self.docker.logs(&container, Some(options));
        Some(Box::pin(source.map(move |result| match result {
            Ok(output) => {
                let message =
                    output.to_string().trim_end_matches('\n').trim_end_matches('\r').to_string();
                let stream = match output {
                    bollard::container::LogOutput::StdErr { .. } => "stderr",
                    bollard::container::LogOutput::StdOut { .. } => "stdout",
                    _ => "console",
                };
                // Docker's --timestamps prefixes each line with its
                // RFC3339 engine time; split it off so the wire shape
                // stays one field per fact.
                let (timestamp, message) = if timestamps {
                    message
                        .split_once(' ')
                        .map(|(t, rest)| (Some(t.to_string()), rest.to_string()))
                        .unwrap_or((None, message))
                } else {
                    (None, message)
                };
                Ok(super::runtime::LogLine {
                    stream: stream.into(),
                    message,
                    timestamp,
                })
            }
            Err(e) => Err(e.to_string()),
        })))
    }
}

#[async_trait::async_trait]
impl super::capture_run::HookRunner for DockerRuntime {
    async fn exec(
        &self,
        container: &str,
        argv: &[String],
        timeout: std::time::Duration,
    ) -> Result<(), String> {
        use bollard::exec::{CreateExecOptions, StartExecResults};
        let exec = self
            .docker
            .create_exec(
                container,
                CreateExecOptions {
                    cmd: Some(argv.to_vec()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("exec create: {e}"))?;
        let started = self
            .docker
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| format!("exec start: {e}"))?;
        if let StartExecResults::Attached { mut output, .. } = started {
            let drain = async {
                while let Some(_line) = futures::StreamExt::next(&mut output).await {}
            };
            tokio::time::timeout(timeout, drain)
                .await
                .map_err(|_| format!("hook exceeded its {}s budget", timeout.as_secs()))?;
        }
        let inspect = self
            .docker
            .inspect_exec(&exec.id)
            .await
            .map_err(|e| format!("exec inspect: {e}"))?;
        match inspect.exit_code {
            Some(0) | None => Ok(()),
            Some(code) => Err(format!("hook exited {code}")),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// Container naming rides the MONIKER: default instances keep their
    /// PoC-compatible short form, foreign instances slug their full FQN.
    /// With ':' banned in the grammar, this stays collision-free.
    #[test]
    fn container_names_follow_the_moniker_rule() {
        assert_eq!(
            DockerRuntime::container_name("memcached::default"),
            "zen-offering-memcached"
        );
        assert_eq!(
            DockerRuntime::container_name("mc::prod"),
            "zen-offering-mc__prod"
        );
        assert_eq!(
            DockerRuntime::container_name("ollama::adopted"),
            "zen-offering-ollama__adopted"
        );
    }
}

