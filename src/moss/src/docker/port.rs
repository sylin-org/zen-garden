use anyhow::{Context, Result};
use garden_common::manifests::get_ports_catalog;
use garden_common::types::{PortConflictHandler, PortRemediation};
use std::collections::HashMap;
use std::net::TcpListener;

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
                port,
                range_start,
                range_end
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

    // No catalog entry -- universal increment-by-one fallback.
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
