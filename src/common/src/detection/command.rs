//! Command-based service detection
//!
//! Executes shell commands to detect installed services (e.g., "mongod --version").
//! Supports:
//! - Exit code validation
//! - Output pattern matching (regex)
//! - Timeout handling
//! - Windows fallback paths for common programs

use crate::manifests::CommandDetection;
use anyhow::{Context, Result};
use regex::Regex;
use std::process::Command;
use std::time::Duration;

/// Execute a command string via the appropriate shell/interpreter.
///
/// On Windows:
/// - PowerShell commands (`powershell`/`pwsh` prefix): call PowerShell
///   directly with `-NoProfile -NonInteractive -Command` to avoid
///   `cmd /C` banner noise and quote mangling.
/// - Other commands: use `cmd /C` for shell features (pipes, etc.).
///
/// On Unix: uses `sh -c` which handles pipes, quotes, and shell builtins.
///
/// This replaces the previous `split_whitespace()` approach which broke
/// commands with quoted arguments.
#[cfg(windows)]
fn execute_via_shell(command: &str) -> std::io::Result<std::process::Output> {
    let trimmed = command.trim();

    // PowerShell commands: extract the -Command argument and call directly.
    // This avoids cmd /C which adds banner text to stdout and can mangle
    // nested quotes.
    if let Some(ps_cmd) = extract_powershell_command(trimmed) {
        let shell = if trimmed.starts_with("pwsh") {
            "pwsh"
        } else {
            "powershell"
        };
        return Command::new(shell)
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps_cmd])
            .output();
    }

    // Everything else: cmd /C
    Command::new("cmd")
        .args(["/C", trimmed])
        .output()
}

/// Extract the PowerShell -Command argument from a command string like:
///   `powershell -Command "Get-Process | Where-Object { ... }"`
///
/// Returns the command body (without surrounding quotes) if the string
/// starts with powershell/pwsh and contains -Command. Returns None if
/// the string doesn't match this pattern.
#[cfg(windows)]
fn extract_powershell_command(command: &str) -> Option<String> {
    let lower = command.to_lowercase();
    if !lower.starts_with("powershell") && !lower.starts_with("pwsh") {
        return None;
    }

    // Find -Command (case-insensitive) and take everything after it
    let idx = lower.find("-command")?;
    let after = command[idx + "-command".len()..].trim();

    if after.is_empty() {
        return None;
    }

    // Strip surrounding quotes if present
    let body = if (after.starts_with('"') && after.ends_with('"'))
        || (after.starts_with('\'') && after.ends_with('\''))
    {
        &after[1..after.len() - 1]
    } else {
        after
    };

    Some(body.to_string())
}

#[cfg(not(windows))]
fn execute_via_shell(command: &str) -> std::io::Result<std::process::Output> {
    Command::new("sh")
        .args(["-c", command])
        .output()
}

/// Detect service by executing a command
///
/// # Examples
/// ```ignore
/// let config = CommandDetection {
///     command: "mongod --version".into(),
///     expected_pattern: Some("db version".into()),
///     expected_exit_code: None,
/// };
/// let detected = detect_by_command(&config, Duration::from_secs(5)).await?;
/// ```
pub async fn detect_by_command(
    config: &CommandDetection,
    timeout: Duration,
) -> Result<DetectionResult> {
    let command = &config.command;

    tracing::debug!(command = %command, os = std::env::consts::OS, "Executing command detection");

    // Execute command via the system shell so that quoted arguments,
    // pipes, and shell builtins work correctly.
    //
    // Previous approach used split_whitespace() which broke commands
    // containing quoted strings (e.g., PowerShell -Command "...").
    let output = tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking({
            let command = command.to_string();
            move || execute_via_shell(&command)
        }),
    )
    .await
    .context("Command execution timeout")?
    .context("Failed to spawn command task")?;

    // Handle command execution result with detailed logging
    let output = match output {
        Ok(out) => out,
        Err(e) => {
            tracing::debug!(
                command = %command,
                error = %e,
                error_kind = ?e.kind(),
                "Shell execution failed"
            );
            // Shell itself not found is a system-level problem
            return Ok(DetectionResult {
                detected: false,
                version: None,
                details: format!("Shell execution failed: {e}"),
            });
        }
    };

    // Check exit code
    let expected_code = config.expected_exit_code.unwrap_or(0);
    let actual_code = output.status.code().unwrap_or(-1);
    if actual_code != expected_code {
        tracing::debug!(
            command = %command,
            expected = expected_code,
            actual = actual_code,
            "Command exit code mismatch"
        );
        return Ok(DetectionResult {
            detected: false,
            version: None,
            details: format!(
                "Exit code mismatch: expected {}, got {}",
                expected_code, actual_code
            ),
        });
    }

    // Check output pattern if specified
    if let Some(pattern_str) = &config.expected_pattern {
        let pattern = Regex::new(pattern_str).context("Invalid regex pattern")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        if !pattern.is_match(&combined) {
            tracing::debug!(
                command = %command,
                pattern = %pattern_str,
                "Command output pattern mismatch"
            );
            return Ok(DetectionResult {
                detected: false,
                version: None,
                details: format!("Output pattern not found: {}", pattern_str),
            });
        }

        // Try to extract version from output
        let version = extract_version(&combined);

        tracing::info!(
            command = %command,
            version = ?version,
            "Service detected via command"
        );

        return Ok(DetectionResult {
            detected: true,
            version,
            details: format!("Detected via command: {}", command),
        });
    }

    // No pattern check - just exit code was enough
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = extract_version(&stdout);

    tracing::info!(
        command = %command,
        version = ?version,
        "Service detected via command"
    );

    Ok(DetectionResult {
        detected: true,
        version,
        details: format!("Detected via command: {}", command),
    })
}

/// Attempt to extract version from command output
fn extract_version(text: &str) -> Option<String> {
    // Common version patterns
    let patterns = [
        r"version[:\s]+([0-9]+\.[0-9]+(?:\.[0-9]+)?)",
        r"v([0-9]+\.[0-9]+(?:\.[0-9]+)?)",
        r"([0-9]+\.[0-9]+(?:\.[0-9]+)?)",
    ];

    for pattern_str in &patterns {
        if let Ok(re) = Regex::new(pattern_str)
            && let Some(caps) = re.captures(text)
            && let Some(version) = caps.get(1)
        {
            return Some(version.as_str().to_string());
        }
    }

    None
}

/// Detection result
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Whether service was detected
    pub detected: bool,
    /// Extracted version (if available)
    pub version: Option<String>,
    /// Human-readable detection details
    pub details: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_version() {
        assert_eq!(
            extract_version("MongoDB version 7.0.5"),
            Some("7.0.5".into())
        );
        assert_eq!(extract_version("v5.4.2"), Some("5.4.2".into()));
        assert_eq!(extract_version("PostgreSQL 15.3"), Some("15.3".into()));
        assert_eq!(extract_version("no version here"), None);
    }

    #[tokio::test]
    async fn test_detect_by_command_success() {
        // Shell-executed: no need for platform-specific wrapping
        let config = CommandDetection {
            command: "echo test".into(),
            expected_pattern: Some("test".into()),
            expected_exit_code: None,
        };

        let result = detect_by_command(&config, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(result.detected);
    }

    #[tokio::test]
    async fn test_detect_by_command_not_found() {
        let config = CommandDetection {
            command: "nonexistent_command_12345".into(),
            expected_pattern: None,
            expected_exit_code: None,
        };

        // Through shell, unknown command returns non-zero exit code
        let result = detect_by_command(&config, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(!result.detected);
    }

    #[tokio::test]
    async fn test_detect_by_command_pattern_mismatch() {
        let config = CommandDetection {
            command: if cfg!(windows) {
                "cmd /c echo test"
            } else {
                "echo test"
            }
            .into(),
            expected_pattern: Some("nonexistent_pattern".into()),
            expected_exit_code: None,
        };

        let result = detect_by_command(&config, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(!result.detected);
    }

    /// Test Ollama detection - only runs if ollama is installed
    #[tokio::test]
    async fn test_detect_ollama() {
        // Test with the actual pattern from the manifest
        let config = CommandDetection {
            command: "ollama --version".into(),
            expected_pattern: Some(r"version is ([0-9]+\.[0-9]+\.[0-9]+)".into()),
            expected_exit_code: Some(0),
        };

        match detect_by_command(&config, Duration::from_secs(5)).await {
            Ok(result) => {
                println!("Detection result: {:?}", result);
                if result.detected {
                    println!("✓ Ollama detected! Version: {:?}", result.version);
                } else {
                    println!("✗ Ollama NOT detected: {}", result.details);
                }
            }
            Err(e) => {
                println!("Detection error (ollama not installed?): {}", e);
            }
        }
    }
}
