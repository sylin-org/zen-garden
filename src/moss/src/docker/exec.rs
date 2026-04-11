use anyhow::{Context, Result};
use bollard::query_parameters::{ListContainersOptions, LogsOptions, PruneImagesOptions};
use futures_util::stream::{Stream, StreamExt};
use garden_common::console::{self, ConsolePrinter};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use super::Client;
use super::naming::zen_offering_container_name;
use super::spec::LogLine;

impl Client {
    /// Pull a Docker image from registry
    ///
    /// Used during install and nourishment to fetch images.
    /// Pull a Docker image, with stall detection.
    ///
    /// The Docker pull stream can stall indefinitely (network issues, DNS
    /// failure, registry rate-limits). To prevent HTTP handlers from hanging
    /// forever, each stream chunk is guarded by a TTL-with-no-activity
    /// timeout. The timer resets on every progress event from Docker -- so
    /// large pulls that keep making progress are fine. The timeout only
    /// fires when Docker goes completely silent.
    pub async fn pull_image(
        &self,
        image: &str,
        console: Option<&Arc<ConsolePrinter>>,
    ) -> Result<()> {
        use bollard::query_parameters::CreateImageOptions;

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
            from_image: Some(image.to_string()),
            ..Default::default()
        };

        let mut stream = self.docker.create_image(Some(options), None, None);

        loop {
            match tokio::time::timeout(stall_timeout, stream.next()).await {
                Ok(Some(Ok(info))) => {
                    if let Some(status) = info.status {
                        if let Some(console) = console
                            && let Some(detail) = &info.progress_detail
                        {
                            let progress = format!("{:?}", detail);
                            console.emit(console::ConsoleEvent::new(
                                console::EventCategory::Services,
                                console::EventStatus::PullProgress,
                                format!("{} -> {}", image, progress),
                            ));
                        }
                        tracing::debug!(image = %image, status = %status, "Pull progress");
                    }
                }
                Ok(Some(Err(e))) => {
                    anyhow::bail!("Failed to pull image '{}': {}", image, e);
                }
                Ok(None) => {
                    // Stream finished -- pull complete
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
            let options = LogsOptions {
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
        use bollard::models::ContainerConfig;
        use bollard::query_parameters::CommitContainerOptions;

        tracing::info!(
            container = %container_name,
            repo = %repo,
            tag = %tag,
            pause,
            "Committing container to image"
        );

        let options = CommitContainerOptions {
            container: Some(container_name.to_string()),
            repo: Some(repo.to_string()),
            tag: Some(tag.to_string()),
            pause,
            ..Default::default()
        };

        let config = ContainerConfig::default();

        let result = self
            .docker
            .commit_container(options, config)
            .await
            .context(format!("Failed to commit container {}", container_name))?;

        let image_id = result.id;
        tracing::info!(
            container = %container_name,
            image_id = %image_id,
            "Container committed successfully"
        );

        Ok(image_id)
    }

    /// Ensure a managed container's `/etc/resolv.conf` points at the correct
    /// DNS servers (bridge gateway -> systemd-resolved). Patches the file
    /// in-place via `docker exec` -- no container restart needed.
    pub async fn reconcile_container_dns(&self, name: &str) -> Result<()> {
        let container_name = zen_offering_container_name(name)?;
        let net = self.container_networking(name).await;

        let nameservers = net
            .dns
            .iter()
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
        let exec = self
            .docker
            .create_exec(
                &container_name,
                bollard::exec::CreateExecOptions {
                    cmd: Some(vec!["cat", "/etc/resolv.conf"]),
                    attach_stdout: Some(true),
                    ..Default::default()
                },
            )
            .await
            .context("create exec for resolv.conf read")?;

        let output = self
            .docker
            .start_exec(&exec.id, None::<bollard::exec::StartExecOptions>)
            .await
            .context("exec cat /etc/resolv.conf")?;

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
        let first_ns = current
            .lines()
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
        let exec = self
            .docker
            .create_exec(
                &container_name,
                bollard::exec::CreateExecOptions {
                    cmd: Some(vec![
                        "sh",
                        "-c",
                        &format!("printf '{desired_content}' > /etc/resolv.conf"),
                    ]),
                    ..Default::default()
                },
            )
            .await
            .context("create exec for resolv.conf write")?;

        self.docker
            .start_exec(&exec.id, None::<bollard::exec::StartExecOptions>)
            .await
            .context("exec write resolv.conf")?;

        Ok(())
    }

    /// Scan ALL Docker containers (running + stopped) and return a map of
    /// host_port -> container_name for every port binding.
    ///
    /// Used during port allocation to avoid conflicts with stopped containers
    /// whose ports are not TCP-bindable but will conflict on restart.
    /// Optionally excludes a specific container (e.g., the one being reinstalled).
    pub async fn scan_port_occupancy(
        &self,
        exclude_container: Option<&str>,
    ) -> Result<HashMap<u16, String>> {
        let options = ListContainersOptions {
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
            if let Some(exclude) = exclude_container
                && name == exclude
            {
                continue;
            }

            // Extract port bindings from the container summary's ports field
            if let Some(ports) = container.ports {
                for port in ports {
                    if let Some(public_port) = port.public_port
                        && public_port > 0
                    {
                        occupied.insert(public_port, name.clone());
                    }
                }
            }
        }

        Ok(occupied)
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
        let mut filters: HashMap<String, Vec<String>> = HashMap::new();
        filters.insert("dangling".to_string(), vec!["true".to_string()]);

        let options = Some(PruneImagesOptions {
            filters: Some(filters),
        });
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
