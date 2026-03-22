//! Process lifecycle management utilities
//!
//! Cross-platform utilities for managing daemon processes:
//! - Graceful shutdown via HTTP API with fallback to force kill
//! - Process detection (excluding current process)
//! - Force termination of processes
//!
//! Platform Support:
//! - Windows: Uses `tasklist` and `taskkill`
//! - Linux/macOS: Uses `pgrep` and `kill`
//!
//! # Example
//! ```ignore
//! use garden_common::infra::process::{check_process_exists, kill_process_graceful};
//!
//! if check_process_exists("garden-moss") {
//!     kill_process_graceful("garden-moss", "http://127.0.0.1:7185").await?;
//! }
//! ```

use anyhow::Result;

/// Attempt graceful shutdown via HTTP, fallback to force kill
///
/// First tries to send a shutdown request to the process via HTTP.
/// If that fails or times out after 3 seconds, falls back to force killing.
///
/// # Arguments
/// - `process_name`: Binary name without extension (e.g., "garden-moss")
/// - `shutdown_url`: Full HTTP URL for shutdown endpoint
///
/// # Example
/// ```ignore
/// kill_process_graceful(
///     "garden-moss",
///     "http://127.0.0.1:7185/admin/shutdown"
/// ).await?;
/// ```
pub async fn kill_process_graceful(process_name: &str, shutdown_url: &str) -> Result<()> {
    // Try graceful shutdown via HTTP first
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()?;

    match client.post(shutdown_url).send().await {
        Ok(response) if response.status().is_success() => {
            tracing::info!("Sent graceful shutdown request to {}", process_name);

            // Wait up to 3 seconds for graceful shutdown
            for _ in 0..30 {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                if !check_process_exists(process_name) {
                    tracing::info!("{} shut down gracefully", process_name);
                    return Ok(());
                }
            }

            tracing::warn!("Graceful shutdown timed out after 3s, forcing kill");
        }
        Ok(response) => {
            tracing::warn!(
                status = ?response.status(),
                "Graceful shutdown request returned non-success status"
            );
        }
        Err(e) => {
            tracing::debug!(
                error = ?e,
                "Could not connect to {} for graceful shutdown",
                process_name
            );
        }
    }

    // Graceful shutdown failed or timed out, force kill
    kill_process(process_name)
}

/// Check if any processes with given name are running (excluding current)
///
/// Returns true if at least one process is running besides the current process.
///
/// # Platform Behavior
/// - Windows: Uses `tasklist /FI "IMAGENAME eq <name>.exe"`
/// - Linux/macOS: Uses `pgrep <name>`
///
/// # Example
/// ```ignore
/// if check_process_exists("garden-moss") {
///     println!("Moss is already running");
/// }
/// ```
pub fn check_process_exists(process_name: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let current_pid = std::process::id();
        let exe_name = format!("{}.exe", process_name);

        if let Ok(output) = Command::new("tasklist")
            .args([
                "/FI",
                &format!("IMAGENAME eq {}", exe_name),
                "/FO",
                "CSV",
                "/NH",
            ])
            .output()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(pid_str) = line.split(',').nth(1) {
                    let pid_str = pid_str.trim_matches('"').trim();
                    if let Ok(pid) = pid_str.parse::<u32>()
                        && pid != current_pid
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let current_pid = std::process::id();

        if let Ok(output) = Command::new("pgrep").arg(process_name).output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Ok(pid) = line.trim().parse::<u32>() {
                        if pid != current_pid {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

/// Force kill all processes with given name (excluding current)
///
/// Immediately terminates all matching processes except the current one.
///
/// # Platform Behavior
/// - Windows: Uses `taskkill /F /PID <pid>`
/// - Linux/macOS: Uses `kill -9 <pid>`
///
/// # Example
/// ```ignore
/// kill_process("garden-moss")?;
/// ```
pub fn kill_process(process_name: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let current_pid = std::process::id();
        let exe_name = format!("{}.exe", process_name);

        let output = Command::new("tasklist")
            .args([
                "/FI",
                &format!("IMAGENAME eq {}", exe_name),
                "/FO",
                "CSV",
                "/NH",
            ])
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some(pid_str) = line.split(',').nth(1) {
                    let pid_str = pid_str.trim_matches('"').trim();
                    if let Ok(pid) = pid_str.parse::<u32>()
                        && pid != current_pid
                    {
                        tracing::info!("Killing {} process: PID {}", process_name, pid);
                        let _ = Command::new("taskkill")
                            .args(["/PID", &pid.to_string(), "/F"])
                            .output();
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let current_pid = std::process::id();

        let output = Command::new("pgrep").arg(process_name).output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    if pid != current_pid {
                        tracing::info!("Killing {} process: PID {}", process_name, pid);
                        let _ = Command::new("kill")
                            .args(&["-9", &pid.to_string()])
                            .output();
                    }
                }
            }
        }
    }

    Ok(())
}
