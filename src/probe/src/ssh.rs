//! SSH Execution Module - Physical validation via remote command execution
//!
//! Provides SSH command execution capabilities for physical validation tests.
//! Uses `plink` (PuTTY) for Windows-to-Linux SSH execution.
//!
//! # Physical Validation
//!
//! This module enables tests to verify actual filesystem state on stones:
//! - Check if backup files exist after nurturing
//! - Verify Docker volume contents after restore
//! - Inspect seed bank directory structure
//! - Compare file hashes before/after operations
//!
//! # Credentials
//!
//! Default credentials: stone/stone (configurable via environment variables)
//! - `STONE_SSH_USER`: SSH username (default: "stone")
//! - `STONE_SSH_PASSWORD`: SSH password (default: "stone")

use crate::Stone;
use anyhow::{Context, Result};
use std::process::Command;
use std::time::Duration;

/// SSH execution configuration
#[derive(Debug, Clone)]
pub struct SshConfig {
    /// SSH username
    pub user: String,
    /// SSH password
    pub password: String,
    /// Command timeout
    pub timeout: Duration,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            user: std::env::var("STONE_SSH_USER").unwrap_or_else(|_| "stone".to_string()),
            password: std::env::var("STONE_SSH_PASSWORD").unwrap_or_else(|_| "stone".to_string()),
            timeout: Duration::from_secs(30),
        }
    }
}

/// Result of an SSH command execution
#[derive(Debug, Clone)]
pub struct SshResult {
    /// Exit code (0 = success)
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Whether the command succeeded
    pub success: bool,
}

impl SshResult {
    /// Check if output contains a pattern
    pub fn contains(&self, pattern: &str) -> bool {
        self.stdout.contains(pattern) || self.stderr.contains(pattern)
    }

    /// Get trimmed stdout
    pub fn output(&self) -> &str {
        self.stdout.trim()
    }
}

/// SSH executor for running commands on stones
#[derive(Clone)]
pub struct SshExecutor {
    config: SshConfig,
}

impl SshExecutor {
    /// Create a new SSH executor with default config
    pub fn new() -> Self {
        Self {
            config: SshConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: SshConfig) -> Self {
        Self { config }
    }

    /// Extract host from stone endpoint (e.g., "http://192.168.1.100:7185" -> "192.168.1.100")
    fn extract_host(endpoint: &str) -> Result<String> {
        let url = url::Url::parse(endpoint)
            .with_context(|| format!("Invalid endpoint URL: {}", endpoint))?;

        url.host_str()
            .map(|h| h.to_string())
            .ok_or_else(|| anyhow::anyhow!("No host in endpoint: {}", endpoint))
    }

    /// Execute a command on a stone via SSH
    ///
    /// Uses `plink` from PuTTY for SSH execution.
    pub fn exec(&self, stone: &Stone, command: &str) -> Result<SshResult> {
        let host = Self::extract_host(&stone.endpoint)?;
        self.exec_on_host(&host, command)
    }

    /// Execute a command on a specific host
    pub fn exec_on_host(&self, host: &str, command: &str) -> Result<SshResult> {
        let output = Command::new("plink")
            .args([
                "-batch",
                "-ssh",
                &format!("{}@{}", self.config.user, host),
                "-pw",
                &self.config.password,
                command,
            ])
            .output()
            .with_context(|| format!("Failed to execute plink command on {}", host))?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(SshResult {
            exit_code,
            stdout,
            stderr,
            success: output.status.success(),
        })
    }

    /// Check if a file exists on the stone
    pub fn file_exists(&self, stone: &Stone, path: &str) -> Result<bool> {
        let result = self.exec(stone, &format!("test -f {} && echo EXISTS", path))?;
        Ok(result.stdout.contains("EXISTS"))
    }

    /// Check if a directory exists on the stone
    pub fn dir_exists(&self, stone: &Stone, path: &str) -> Result<bool> {
        let result = self.exec(stone, &format!("test -d {} && echo EXISTS", path))?;
        Ok(result.stdout.contains("EXISTS"))
    }

    /// Get file size in bytes
    pub fn file_size(&self, stone: &Stone, path: &str) -> Result<Option<u64>> {
        let result = self.exec(stone, &format!("stat -c %s {} 2>/dev/null", path))?;
        if result.success {
            Ok(result.stdout.trim().parse().ok())
        } else {
            Ok(None)
        }
    }

    /// List files in a directory
    pub fn list_files(&self, stone: &Stone, path: &str) -> Result<Vec<String>> {
        let result = self.exec(stone, &format!("ls -1 {} 2>/dev/null", path))?;
        if result.success {
            Ok(result
                .stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Get MD5 hash of a file
    pub fn file_hash(&self, stone: &Stone, path: &str) -> Result<Option<String>> {
        let result = self.exec(
            stone,
            &format!("md5sum {} 2>/dev/null | cut -d' ' -f1", path),
        )?;
        if result.success && !result.stdout.trim().is_empty() {
            Ok(Some(result.stdout.trim().to_string()))
        } else {
            Ok(None)
        }
    }

    /// Execute a Docker command in a container
    pub fn docker_exec(&self, stone: &Stone, container: &str, command: &str) -> Result<SshResult> {
        self.exec(stone, &format!("docker exec {} {}", container, command))
    }

    /// Check if a Docker volume exists
    pub fn docker_volume_exists(&self, stone: &Stone, volume: &str) -> Result<bool> {
        let result = self.exec(
            stone,
            &format!(
                "docker volume inspect {} >/dev/null 2>&1 && echo EXISTS",
                volume
            ),
        )?;
        Ok(result.stdout.contains("EXISTS"))
    }

    /// List files in a Docker volume (via temporary container)
    pub fn docker_volume_files(&self, stone: &Stone, volume: &str) -> Result<Vec<String>> {
        let result = self.exec(
            stone,
            &format!(
                "docker run --rm -v {}:/vol alpine ls -1 /vol 2>/dev/null",
                volume
            ),
        )?;
        if result.success {
            Ok(result
                .stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Get Docker volume size
    pub fn docker_volume_size(&self, stone: &Stone, volume: &str) -> Result<Option<u64>> {
        let result = self.exec(
            stone,
            &format!(
                "docker run --rm -v {}:/vol alpine du -sb /vol 2>/dev/null | cut -f1",
                volume
            ),
        )?;
        if result.success {
            Ok(result.stdout.trim().parse().ok())
        } else {
            Ok(None)
        }
    }

    /// Check if SSH/plink is available
    pub fn is_available() -> bool {
        Command::new("plink")
            .arg("-V")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Test SSH connectivity to a stone
    pub fn test_connectivity(&self, stone: &Stone) -> Result<bool> {
        let result = self.exec(stone, "echo OK")?;
        Ok(result.success && result.stdout.contains("OK"))
    }
}

impl Default for SshExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Physical validation helper functions
///
/// NOTE: These functions are Linux-only. They use Linux-specific paths
/// and SSH commands that only work on Linux targets. The physical validation
/// tests should check `stone.is_linux()` before calling these functions.
pub mod validation {
    use super::*;
    use garden_common::constants::paths;

    /// Validate that a nurturing snapshot exists on disk (Linux only)
    ///
    /// Checks:
    /// - Nurturing index directory exists
    /// - Nurturing index file exists
    /// - Harvest directory exists (in data_dir/harvests)
    ///
    /// NOTE: Uses Linux-specific paths. Only call this for Linux stones.
    pub fn validate_local_snapshot(
        ssh: &SshExecutor,
        stone: &Stone,
        offering_id: &str,
        _slot: &str,
    ) -> Result<ValidationResult> {
        let mut result = ValidationResult::new("Local Snapshot");

        // Use Linux-specific paths (always /etc/zen-garden and /var/lib/zen-garden)
        // regardless of what platform the probe is compiled on
        let nurturing_index_dir = paths::linux_nurturing_index_dir();
        result.add_check(
            "Nurturing index directory exists",
            ssh.dir_exists(stone, &nurturing_index_dir)?,
        );

        // Check nurturing index file
        let index_path = paths::linux_nurturing_index_path();
        result.add_check(
            "Nurturing index file exists",
            ssh.file_exists(stone, &index_path)?,
        );

        // Check harvest directory (actual backup data)
        let harvest_dir = paths::linux_harvest_dir();
        result.add_check(
            "Harvest directory exists",
            ssh.dir_exists(stone, &harvest_dir)?,
        );

        // List harvests to count them
        if let Ok(harvests) = ssh.list_files(stone, &harvest_dir) {
            result
                .metadata
                .insert("harvest_count".to_string(), harvests.len().to_string());
        }

        // Get index file content to verify offering is tracked
        if let Ok(content) = ssh.exec(stone, &format!("cat {} 2>/dev/null", index_path)) {
            if content.success {
                let has_offering = content.stdout.contains(offering_id);
                result.add_check(
                    &format!(
                        "Offering {} in index",
                        &offering_id[..8.min(offering_id.len())]
                    ),
                    has_offering,
                );
            }
        }

        Ok(result)
    }

    /// Validate that a snapshot was replicated to seed bank
    ///
    /// Checks:
    /// - Seed bank mount point exists
    /// - nurturing directory exists on seed bank
    /// - Offering directory exists
    /// - Harvest tarball exists
    pub fn validate_seed_bank_snapshot(
        ssh: &SshExecutor,
        stone: &Stone,
        seed_bank_mount: &str,
        offering_id: &str,
        harvest_id: &str,
    ) -> Result<ValidationResult> {
        let mut result = ValidationResult::new("Seed Bank Snapshot");

        // Check seed bank mount
        result.add_check("Seed bank mounted", ssh.dir_exists(stone, seed_bank_mount)?);

        // Check nurturing directory on seed bank using path constant
        let nurturing_path = paths::seed_bank_memories_dir(seed_bank_mount);
        result.add_check(
            "Nurturing directory exists",
            ssh.dir_exists(stone, &nurturing_path)?,
        );

        // Check offering directory
        let offering_path = paths::seed_bank_memory_offering_dir(seed_bank_mount, offering_id);
        result.add_check(
            "Offering directory exists",
            ssh.dir_exists(stone, &offering_path)?,
        );

        // Check harvest tarball
        let tarball_path =
            paths::seed_bank_memory_harvest_path(seed_bank_mount, offering_id, harvest_id);
        let tarball_exists = ssh.file_exists(stone, &tarball_path)?;
        result.add_check("Harvest tarball exists", tarball_exists);

        // Get tarball size if it exists
        if tarball_exists {
            if let Ok(Some(size)) = ssh.file_size(stone, &tarball_path) {
                result
                    .metadata
                    .insert("tarball_size_bytes".to_string(), size.to_string());
            }
        }

        // List all harvests for this offering
        if let Ok(files) = ssh.list_files(stone, &offering_path) {
            let harvest_count = files.iter().filter(|f| f.ends_with(".tar.gz")).count();
            result
                .metadata
                .insert("harvest_count".to_string(), harvest_count.to_string());
        }

        Ok(result)
    }

    /// Validate Docker volume contents after restore
    ///
    /// Checks:
    /// - Docker volume exists
    /// - Volume is not empty
    /// - Optional: specific files exist
    pub fn validate_docker_volume(
        ssh: &SshExecutor,
        stone: &Stone,
        volume_name: &str,
        expected_files: Option<&[&str]>,
    ) -> Result<ValidationResult> {
        let mut result = ValidationResult::new("Docker Volume");

        // Check volume exists
        let exists = ssh.docker_volume_exists(stone, volume_name)?;
        result.add_check("Volume exists", exists);

        if !exists {
            return Ok(result);
        }

        // List files in volume
        let files = ssh.docker_volume_files(stone, volume_name)?;
        result.add_check("Volume is not empty", !files.is_empty());
        result
            .metadata
            .insert("file_count".to_string(), files.len().to_string());

        // Check expected files if provided
        if let Some(expected) = expected_files {
            for file in expected {
                result.add_check(
                    &format!("Contains {}", file),
                    files.iter().any(|f| f == *file || f.contains(file)),
                );
            }
        }

        // Get volume size
        if let Ok(Some(size)) = ssh.docker_volume_size(stone, volume_name) {
            result
                .metadata
                .insert("volume_size_bytes".to_string(), size.to_string());
        }

        Ok(result)
    }

    /// Validate retention policy is being enforced
    ///
    /// Counts snapshots on seed bank and verifies <= max_retention
    pub fn validate_retention(
        ssh: &SshExecutor,
        stone: &Stone,
        seed_bank_mount: &str,
        offering_id: &str,
        max_retention: usize,
    ) -> Result<ValidationResult> {
        let mut result = ValidationResult::new("Retention Policy");

        let offering_path = format!("{}/nurturing/{}", seed_bank_mount, offering_id);

        // List all harvests
        let files = ssh.list_files(stone, &offering_path)?;
        let harvest_count = files.iter().filter(|f| f.ends_with(".tar.gz")).count();

        result
            .metadata
            .insert("harvest_count".to_string(), harvest_count.to_string());
        result
            .metadata
            .insert("max_retention".to_string(), max_retention.to_string());

        result.add_check(
            &format!(
                "Retention within limit ({}<={})",
                harvest_count, max_retention
            ),
            harvest_count <= max_retention,
        );

        Ok(result)
    }
}

/// Result of a validation operation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub name: String,
    pub checks: Vec<(String, bool)>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl ValidationResult {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            checks: Vec::new(),
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn add_check(&mut self, description: &str, passed: bool) {
        self.checks.push((description.to_string(), passed));
    }

    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|(_, passed)| *passed)
    }

    pub fn passed_count(&self) -> usize {
        self.checks.iter().filter(|(_, passed)| *passed).count()
    }

    pub fn failed_count(&self) -> usize {
        self.checks.iter().filter(|(_, passed)| !*passed).count()
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "all_passed": self.all_passed(),
            "passed": self.passed_count(),
            "failed": self.failed_count(),
            "checks": self.checks.iter().map(|(desc, passed)| {
                serde_json::json!({
                    "check": desc,
                    "passed": passed
                })
            }).collect::<Vec<_>>(),
            "metadata": self.metadata
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_host() {
        assert_eq!(
            SshExecutor::extract_host("http://192.168.1.100:7185").unwrap(),
            "192.168.1.100"
        );
        assert_eq!(
            SshExecutor::extract_host("http://stone-1.local:7185").unwrap(),
            "stone-1.local"
        );
    }

    #[test]
    fn test_validation_result() {
        let mut result = ValidationResult::new("Test");
        result.add_check("Check 1", true);
        result.add_check("Check 2", false);
        result.add_check("Check 3", true);

        assert!(!result.all_passed());
        assert_eq!(result.passed_count(), 2);
        assert_eq!(result.failed_count(), 1);
    }

    #[test]
    fn test_ssh_config_default() {
        let config = SshConfig::default();
        // Without env vars, defaults to "stone"
        // In tests, we just verify the struct works
        assert_eq!(config.timeout, Duration::from_secs(30));
    }
}
