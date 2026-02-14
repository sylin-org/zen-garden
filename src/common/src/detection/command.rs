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

/// Windows fallback paths for common programs not in PATH
#[cfg(windows)]
fn get_windows_fallback_paths(program: &str) -> Vec<String> {
    let mut paths = Vec::new();

    // Get LOCALAPPDATA for user-installed programs
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        if program == "ollama" {
            // Standard Ollama install path
            paths.push(format!("{}\\Programs\\Ollama\\ollama.exe", local_app_data));
        }
    }

    // Common Program Files locations
    if let Ok(program_files) = std::env::var("ProgramFiles") {
        if program == "ollama" {
            paths.push(format!("{}\\Ollama\\ollama.exe", program_files));
        }
    }

    paths
}

/// Try to find and execute a command, checking fallback paths on Windows
#[cfg(windows)]
fn try_execute_command(program: &str, args: &[String]) -> std::io::Result<std::process::Output> {
    // First try the command as-is (relies on PATH)
    match Command::new(program).args(args).output() {
        Ok(output) => return Ok(output),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Command not found in PATH, try fallback paths
            tracing::debug!(
                program = %program,
                "Command not found in PATH, trying fallback paths"
            );
        }
        Err(e) => return Err(e),
    }

    // Try fallback paths
    for fallback_path in get_windows_fallback_paths(program) {
        tracing::debug!(fallback_path = %fallback_path, "Trying fallback path");
        match Command::new(&fallback_path).args(args).output() {
            Ok(output) => {
                tracing::info!(
                    fallback_path = %fallback_path,
                    "Found program via fallback path"
                );
                return Ok(output);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                continue; // Try next fallback
            }
            Err(e) => return Err(e),
        }
    }

    // All fallbacks failed, return the original "not found" error
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!(
            "Program '{}' not found in PATH or fallback locations",
            program
        ),
    ))
}

/// Execute command (non-Windows just uses PATH)
#[cfg(not(windows))]
fn try_execute_command(program: &str, args: &[String]) -> std::io::Result<std::process::Output> {
    Command::new(program).args(args).output()
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

    // Parse command into program and args
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        anyhow::bail!("Empty command");
    }

    let program = parts[0];
    let args = &parts[1..];

    // Execute command with timeout
    let output = tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking({
            let program = program.to_string();
            let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            move || try_execute_command(&program, &args)
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
                "Command execution failed"
            );
            if e.kind() == std::io::ErrorKind::NotFound {
                return Ok(DetectionResult {
                    detected: false,
                    version: None,
                    details: format!("Program '{}' not found", program),
                });
            }
            anyhow::bail!("Failed to execute command '{}': {}", command, e);
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
        if let Ok(re) = Regex::new(pattern_str) {
            if let Some(caps) = re.captures(text) {
                if let Some(version) = caps.get(1) {
                    return Some(version.as_str().to_string());
                }
            }
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
        let config = CommandDetection {
            command: if cfg!(windows) {
                "cmd /c echo test"
            } else {
                "echo test"
            }
            .into(),
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
