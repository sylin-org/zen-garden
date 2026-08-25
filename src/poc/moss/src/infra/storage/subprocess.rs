//! Timed subprocess execution for storage operations
//!
//! Wraps `Command::output()` with a deadline so that a hung mount/blkid/lsblk
//! call cannot block the system forever. On timeout the child process is killed.
//!
//! Two variants:
//! - [`run_timed`] — async (tokio), used by registry.rs
//! - [`run_timed_sync`] — blocking, used by device.rs

use std::process::Output;
use std::time::Duration;
use tracing::warn;

/// Error returned when a subprocess fails or times out
#[derive(Debug)]
pub enum SubprocessError {
    /// The command did not complete within the allowed deadline
    Timeout(Duration),

    /// Failed to spawn or wait on the child process
    Io(std::io::Error),
}

impl std::fmt::Display for SubprocessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout(d) => write!(f, "subprocess timed out after {:?}", d),
            Self::Io(e) => write!(f, "subprocess I/O error: {}", e),
        }
    }
}

impl std::error::Error for SubprocessError {}

impl From<std::io::Error> for SubprocessError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

// ============================================================================
// Async variant (tokio) — used by registry.rs
// ============================================================================

/// Drain a taken child handle into a `Vec<u8>`.
#[cfg(target_os = "linux")]
async fn read_handle<R: tokio::io::AsyncReadExt + Unpin>(handle: Option<R>) -> Vec<u8> {
    match handle {
        Some(mut r) => {
            let mut buf = Vec::new();
            let _ = r.read_to_end(&mut buf).await;
            buf
        }
        None => Vec::new(),
    }
}

/// Run `sudo <args…>` with a deadline.
///
/// On timeout the child is killed (SIGKILL) and [`SubprocessError::Timeout`] is returned.
///
/// # Example
/// ```ignore
/// let output = run_sudo_timed(&["mount", "/dev/sdb1", "/mnt/sb"], Duration::from_secs(30)).await?;
/// ```
#[cfg(target_os = "linux")]
#[expect(dead_code)]
pub async fn run_sudo_timed(args: &[&str], timeout: Duration) -> Result<Output, SubprocessError> {
    use std::process::Stdio;
    use tokio::process::Command;

    let mut child = Command::new("sudo")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(result) => {
            let status = result?;
            let stdout = read_handle(stdout_handle).await;
            let stderr = read_handle(stderr_handle).await;
            Ok(Output {
                status,
                stdout,
                stderr,
            })
        }
        Err(_elapsed) => {
            // Deadline exceeded — kill the child
            let _ = child.kill().await;
            warn!(
                cmd = %format!("sudo {}", args.join(" ")),
                timeout_secs = timeout.as_secs(),
                "Subprocess timed out, child killed"
            );
            Err(SubprocessError::Timeout(timeout))
        }
    }
}

/// Run an arbitrary command with a deadline (async).
#[cfg(target_os = "linux")]
#[expect(dead_code)]
pub async fn run_command_timed(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Output, SubprocessError> {
    use std::process::Stdio;
    use tokio::process::Command;

    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(result) => {
            let status = result?;
            let stdout = read_handle(stdout_handle).await;
            let stderr = read_handle(stderr_handle).await;
            Ok(Output {
                status,
                stdout,
                stderr,
            })
        }
        Err(_elapsed) => {
            let _ = child.kill().await;
            warn!(
                cmd = %format!("{} {}", program, args.join(" ")),
                timeout_secs = timeout.as_secs(),
                "Subprocess timed out, child killed"
            );
            Err(SubprocessError::Timeout(timeout))
        }
    }
}

/// Convenience: run `sudo <args…>` with a deadline, suppressing stdout.
///
/// Returns the `Output` (stdout will be empty, stderr captured).
#[cfg(target_os = "linux")]
pub async fn run_sudo_timed_quiet(
    args: &[&str],
    timeout: Duration,
) -> Result<Output, SubprocessError> {
    use std::process::Stdio;
    use tokio::process::Command;

    let mut child = Command::new("sudo")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    let stderr_handle = child.stderr.take();

    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(result) => {
            let status = result?;
            let stderr = read_handle(stderr_handle).await;
            Ok(Output {
                status,
                stdout: Vec::new(),
                stderr,
            })
        }
        Err(_elapsed) => {
            let _ = child.kill().await;
            warn!(
                cmd = %format!("sudo {}", args.join(" ")),
                timeout_secs = timeout.as_secs(),
                "Subprocess timed out, child killed"
            );
            Err(SubprocessError::Timeout(timeout))
        }
    }
}

// ============================================================================
// Sync variant — used by device.rs
// ============================================================================

/// Run a command synchronously with a deadline.
///
/// Spawns the child, then polls `try_wait()` until it completes or the
/// deadline expires. On timeout the child is killed.
pub fn run_command_timed_sync(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Output, SubprocessError> {
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let deadline = std::time::Instant::now() + timeout;
    let poll_interval = Duration::from_millis(50);

    loop {
        match child.try_wait()? {
            Some(status) => {
                // Child exited — collect output
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut r| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut r, &mut buf).unwrap_or(0);
                        buf
                    })
                    .unwrap_or_default();
                let stderr = child
                    .stderr
                    .take()
                    .map(|mut r| {
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut r, &mut buf).unwrap_or(0);
                        buf
                    })
                    .unwrap_or_default();

                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            None => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait(); // reap zombie
                    warn!(
                        cmd = %format!("{} {}", program, args.join(" ")),
                        timeout_secs = timeout.as_secs(),
                        "Sync subprocess timed out, child killed"
                    );
                    return Err(SubprocessError::Timeout(timeout));
                }
                std::thread::sleep(poll_interval);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subprocess_error_display() {
        let err = SubprocessError::Timeout(Duration::from_secs(30));
        assert!(err.to_string().contains("30"));
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn test_io_error_wrapping() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err = SubprocessError::Io(io_err);
        assert!(err.to_string().contains("not found"));
    }
}
