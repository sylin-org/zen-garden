use anyhow::{Context, Result};
use bollard::query_parameters::{ListContainersOptions, LogsOptions, PruneImagesOptions};
use futures_util::stream::{Stream, StreamExt};
use garden_common::console::{self, ConsolePrinter};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use super::ContainerRuntime;
use super::naming::zen_offering_container_name;
use super::spec::LogLine;

impl ContainerRuntime {
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
        use garden_common::host::ImagePullPolicy;

        let stall_timeout = garden_common::constants::timeouts::docker_pull_stall_timeout();
        let policy = garden_common::host::profile().runtime.image_pull_policy;

        // Honor the host image-pull policy before touching the registry.
        let present = self.docker.inspect_image(image).await.is_ok();
        match policy {
            ImagePullPolicy::Never => {
                if present {
                    tracing::debug!(image = %image, "image_pull_policy=Never: using local image");
                    return Ok(());
                }
                anyhow::bail!(
                    "image_pull_policy=Never and image '{}' is not present locally",
                    image
                );
            }
            ImagePullPolicy::IfNotPresent if present => {
                tracing::debug!(image = %image, "image_pull_policy=IfNotPresent: image already present, skipping pull");
                return Ok(());
            }
            _ => {} // Always, or IfNotPresent && !present → pull below
        }

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

        // Capture a pull failure rather than bailing immediately, so we can fall back to a
        // locally-present image (offline / air-gapped stones load images via `docker load`).
        let mut pull_error: Option<String> = None;
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
                    pull_error = Some(e.to_string());
                    break;
                }
                Ok(None) => {
                    // Stream finished -- pull complete
                    break;
                }
                Err(_elapsed) => {
                    pull_error = Some(format!(
                        "pull stalled: no progress for {} seconds",
                        stall_timeout.as_secs()
                    ));
                    break;
                }
            }
        }

        if let Some(err) = pull_error {
            // Pull failed (e.g. no registry connectivity). If the image is already present
            // locally — loaded via `docker load` on an offline/air-gapped stone — use it
            // instead of failing the install.
            if matches!(policy, ImagePullPolicy::IfNotPresent)
                && self.docker.inspect_image(image).await.is_ok()
            {
                tracing::warn!(image = %image, error = %err,
                    "Image pull failed; using locally-present image");
                if let Some(console) = console {
                    console.emit(console::ConsoleEvent::new(
                        console::EventCategory::Services,
                        console::EventStatus::PullComplete,
                        format!("{} (local)", image),
                    ));
                }
                return Ok(());
            }
            anyhow::bail!(
                "Failed to pull image '{}': {} (and no local copy is present)",
                image,
                err
            );
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

    /// Read the most recent `lines` of a container's combined logs (no follow).
    ///
    /// Best-effort reader for the post-install healthcheck scan (COMPAT-0003).
    /// Accumulation is byte-capped (code-standard rule 20).
    pub async fn read_recent_logs(&self, name: &str, lines: usize) -> Result<String> {
        let container_name = zen_offering_container_name(name)?;
        let options = LogsOptions {
            follow: false,
            stdout: true,
            stderr: true,
            tail: lines.min(10_000).to_string(),
            ..Default::default()
        };
        let mut stream = self.docker.logs(&container_name, Some(options));
        let mut out = String::new();
        const MAX_BYTES: usize = 256 * 1024;
        while let Some(result) = stream.next().await {
            match result {
                Ok(output) => {
                    out.push_str(&output.to_string());
                    if out.len() >= MAX_BYTES {
                        break;
                    }
                }
                Err(e) => return Err(anyhow::anyhow!("Docker logs error: {}", e)),
            }
        }
        Ok(out)
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

    // ========================================================================
    // Snapshot consistency & disposal (ORCH-0039)
    // ========================================================================

    /// Pause a running container by its full container name.
    ///
    /// Snapshot capture pauses the container around the volume-archive
    /// step so a live process (e.g. a database flushing its data files)
    /// can't mutate bytes mid-`tar`. Without this, `tar` exits non-zero
    /// ("file changed as we read it") and the capture fails — the failure
    /// mode that filled a stone's disk under ORCH-0039. The image commit
    /// already pauses (see [`commit_container`](Self::commit_container));
    /// this extends the same guarantee to the volume archive.
    ///
    /// Takes the raw container name (e.g. `zen-offering-mongodb`), matching
    /// `commit_container`, because the snapshot flow already derived it.
    pub async fn pause_container(&self, container_name: &str) -> Result<()> {
        self.docker
            .pause_container(container_name)
            .await
            .with_context(|| format!("pause container {container_name}"))?;
        tracing::debug!(container = %container_name, "Container paused for snapshot consistency");
        Ok(())
    }

    /// Unpause a previously paused container.
    ///
    /// The counterpart to [`pause_container`](Self::pause_container). The
    /// snapshot flow calls this on every exit path after a successful
    /// pause — including archive failure — so a paused container is never
    /// left stuck.
    pub async fn unpause_container(&self, container_name: &str) -> Result<()> {
        self.docker
            .unpause_container(container_name)
            .await
            .with_context(|| format!("unpause container {container_name}"))?;
        tracing::debug!(container = %container_name, "Container unpaused");
        Ok(())
    }

    /// Remove a Docker image by reference (`repo:tag` or image id).
    ///
    /// Snapshot capture disposes of the transient
    /// `zen-harvest/<offering>:<tag>` image once `docker save` has copied
    /// its bytes into the snapshot tarball: the Docker image is redundant
    /// afterward (plant loads from the tarball; the manifest keeps the ref
    /// only for diagnostics). Skipping this leaks one tagged image per
    /// capture into the Docker image store.
    ///
    /// Idempotent: a 404 (image already absent) is treated as success.
    /// `force` removes the tag even when the underlying layers are shared.
    pub async fn remove_image(&self, image_ref: &str, force: bool) -> Result<()> {
        use bollard::query_parameters::RemoveImageOptionsBuilder;

        let options = RemoveImageOptionsBuilder::default().force(force).build();
        match self.docker.remove_image(image_ref, Some(options), None).await {
            Ok(_) => {
                tracing::debug!(image = %image_ref, "Removed Docker image");
                Ok(())
            }
            Err(bollard::errors::Error::DockerResponseServerError {
                status_code: 404, ..
            }) => Ok(()),
            Err(e) => {
                Err(anyhow::Error::from(e).context(format!("remove image {image_ref}")))
            }
        }
    }

    /// Save a Docker image to a tarball on disk via the Docker
    /// daemon's `image export` endpoint.
    ///
    /// Used by snapshot capture (ORCH-0039) to bundle the offering's
    /// committed image alongside its volumes. The output format is
    /// the standard `docker save` tarball — `docker load -i <path>`
    /// reverses it on the target stone.
    ///
    /// Streams the response chunk-by-chunk so memory stays bounded
    /// for multi-GB images.
    ///
    /// # Arguments
    /// * `image_ref` - Image reference (`repo:tag` or image id)
    /// * `dest` - File path to write the tar to. Parent directory
    ///   must already exist; the file is overwritten if present.
    pub async fn save_image(
        &self,
        image_ref: &str,
        dest: &std::path::Path,
    ) -> Result<u64> {
        use tokio::io::AsyncWriteExt;

        tracing::info!(
            image = %image_ref,
            dest = %dest.display(),
            "Saving Docker image to tarball"
        );

        let mut stream = self.docker.export_image(image_ref);
        let mut file = tokio::fs::File::create(dest)
            .await
            .with_context(|| format!("create image tarball: {}", dest.display()))?;
        let mut bytes_written: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk
                .with_context(|| format!("export_image stream chunk for {image_ref}"))?;
            file.write_all(&chunk)
                .await
                .with_context(|| format!("write image tarball chunk: {}", dest.display()))?;
            bytes_written += chunk.len() as u64;
        }
        file.flush()
            .await
            .with_context(|| format!("flush image tarball: {}", dest.display()))?;

        tracing::info!(
            image = %image_ref,
            dest = %dest.display(),
            bytes = bytes_written,
            "Docker image saved"
        );

        Ok(bytes_written)
    }

    /// Load a Docker image from a tarball produced by
    /// [`save_image`](Self::save_image) (or any compatible
    /// `docker save` output). Streams the file into the daemon
    /// chunk-by-chunk so memory stays bounded for multi-GB
    /// images.
    ///
    /// Used by snapshot plant (ORCH-0039) to materialise the
    /// captured image on the target stone before recreating the
    /// container. Returns the imported image's `repo:tag` if
    /// the import emits one — this matches the value
    /// `save_image` recorded in the snapshot manifest's
    /// `image.ref_string`.
    pub async fn load_image(&self, tar_path: &std::path::Path) -> Result<()> {
        use bollard::body_full;
        use bollard::query_parameters::ImportImageOptionsBuilder;

        tracing::info!(
            tar = %tar_path.display(),
            "Loading Docker image from tarball"
        );

        let bytes = tokio::fs::read(tar_path)
            .await
            .with_context(|| format!("read image tarball: {}", tar_path.display()))?;

        let mut stream = self.docker.import_image(
            ImportImageOptionsBuilder::default().build(),
            body_full(bytes.into()),
            None,
        );

        // Drain the build-info stream — `import_image` returns
        // progress events and a final completion marker. Errors
        // surface as `Err` items in the stream; we forward the
        // first one.
        while let Some(item) = stream.next().await {
            match item {
                Ok(_info) => {} // progress; keep draining
                Err(e) => {
                    return Err(anyhow::Error::from(e)
                        .context(format!("import image from {}", tar_path.display())));
                }
            }
        }

        tracing::info!(tar = %tar_path.display(), "Docker image loaded");
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

    /// List `zen-harvest/*` images present in the Docker store.
    ///
    /// Snapshot capture commits the running container to a transient
    /// `zen-harvest/<encoded_fqn>:<timestamp>` image, `docker save`s it,
    /// then removes it (`remove_image`). A `zen-harvest/*` image that
    /// outlives its capture is therefore a leak — either an aborted
    /// capture that never reached disposal, or a build before the
    /// dispose-on-every-path fix. `prune_dangling_images` cannot reclaim
    /// these because they are *tagged*, not dangling.
    ///
    /// Returns id, tags, and creation time (Unix seconds) per image so the
    /// caller can age-filter: an image created seconds ago may be the
    /// `docker save` source of a capture in flight, and removing it would
    /// abort that capture. Reclaim only images comfortably older than a
    /// capture's duration.
    pub async fn list_harvest_images(&self) -> Result<Vec<super::ImageInfo>> {
        use bollard::query_parameters::ListImagesOptions;

        let images = self
            .docker
            .list_images(None::<ListImagesOptions>)
            .await
            .context("list Docker images")?;

        Ok(images
            .into_iter()
            .filter(|i| {
                i.repo_tags
                    .iter()
                    .any(|t| t.starts_with("zen-harvest/"))
            })
            .map(|i| super::ImageInfo {
                id: i.id,
                tags: i.repo_tags,
                created_unix: i.created,
            })
            .collect())
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
