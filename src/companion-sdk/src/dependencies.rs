//! System dependency management for Companions
//!
//! Provides infrastructure for Companions to declare and auto-install
//! system dependencies at startup.
//!
//! # Example
//!
//! ```ignore
//! use garden_companion_sdk::dependencies::{SystemDependency, ensure_dependencies};
//!
//! // Define what the Companion needs
//! let deps = vec![
//!     SystemDependency::apt_package("alsa-utils", "aplay"),
//! ];
//!
//! // Check and install at startup
//! ensure_dependencies(&deps)?;
//! ```

use anyhow::{Context, Result};
use std::process::Command;

/// A system dependency that can be checked and installed
#[derive(Debug, Clone)]
pub struct SystemDependency {
    /// Human-readable name for logging
    pub name: String,
    /// Command to check if dependency exists (e.g., "which aplay")
    pub check_command: String,
    /// Arguments for check command
    pub check_args: Vec<String>,
    /// Command to install the dependency
    pub install_command: String,
    /// Arguments for install command
    pub install_args: Vec<String>,
}

impl SystemDependency {
    /// Create a dependency from explicit commands
    pub fn new(
        name: impl Into<String>,
        check_command: impl Into<String>,
        check_args: Vec<String>,
        install_command: impl Into<String>,
        install_args: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            check_command: check_command.into(),
            check_args,
            install_command: install_command.into(),
            install_args,
        }
    }

    /// Create a dependency for an apt package
    ///
    /// - `package`: The apt package name (e.g., "alsa-utils")
    /// - `binary`: The binary to check for (e.g., "aplay")
    #[cfg(target_os = "linux")]
    pub fn apt_package(package: impl Into<String>, binary: impl Into<String>) -> Self {
        let package = package.into();
        let binary = binary.into();
        Self {
            name: package.clone(),
            check_command: "which".into(),
            check_args: vec![binary],
            install_command: "sudo".into(),
            install_args: vec!["apt-get".into(), "install".into(), "-y".into(), package],
        }
    }

    /// No-op on non-Linux (apt packages don't apply)
    #[cfg(not(target_os = "linux"))]
    pub fn apt_package(_package: impl Into<String>, _binary: impl Into<String>) -> Self {
        Self {
            name: "n/a".into(),
            check_command: "true".into(), // Always succeeds
            check_args: vec![],
            install_command: "true".into(),
            install_args: vec![],
        }
    }

    /// Check if the dependency is available
    pub fn is_available(&self) -> bool {
        Command::new(&self.check_command)
            .args(&self.check_args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Install the dependency
    pub fn install(&self) -> Result<()> {
        tracing::info!(dependency = %self.name, "Installing system dependency");

        let output = Command::new(&self.install_command)
            .args(&self.install_args)
            .output()
            .with_context(|| format!("Failed to run install command for {}", self.name))?;

        if output.status.success() {
            tracing::info!(dependency = %self.name, "Successfully installed");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to install {}: {}", self.name, stderr.trim())
        }
    }
}

/// Result of dependency check
#[derive(Debug, Clone)]
pub struct DependencyCheckResult {
    /// Dependencies that were already available
    pub already_available: Vec<String>,
    /// Dependencies that were installed
    pub installed: Vec<String>,
    /// Dependencies that failed to install
    pub failed: Vec<(String, String)>,
}

impl DependencyCheckResult {
    /// Returns true if all dependencies are now available
    pub fn all_ok(&self) -> bool {
        self.failed.is_empty()
    }
}

/// Check and install system dependencies
///
/// For each dependency:
/// 1. Check if it's available (runs check_command)
/// 2. If not, attempt to install it (runs install_command)
/// 3. Report results
///
/// # Example
///
/// ```ignore
/// let deps = vec![
///     SystemDependency::apt_package("alsa-utils", "aplay"),
///     SystemDependency::apt_package("alsa-utils", "amixer"),
/// ];
///
/// let result = ensure_dependencies(&deps)?;
/// if !result.all_ok() {
///     tracing::error!("Some dependencies failed to install");
/// }
/// ```
pub fn ensure_dependencies(dependencies: &[SystemDependency]) -> Result<DependencyCheckResult> {
    let mut result = DependencyCheckResult {
        already_available: vec![],
        installed: vec![],
        failed: vec![],
    };

    for dep in dependencies {
        if dep.is_available() {
            tracing::debug!(dependency = %dep.name, "Already available");
            result.already_available.push(dep.name.clone());
        } else {
            tracing::info!(dependency = %dep.name, "Not found, attempting install");
            match dep.install() {
                Ok(()) => {
                    // Verify it's now available
                    if dep.is_available() {
                        result.installed.push(dep.name.clone());
                    } else {
                        result
                            .failed
                            .push((dep.name.clone(), "Installed but still not available".into()));
                    }
                }
                Err(e) => {
                    tracing::error!(
                        dependency = %dep.name,
                        error = %e,
                        "Failed to install"
                    );
                    result.failed.push((dep.name.clone(), e.to_string()));
                }
            }
        }
    }

    // Summary log
    if !result.installed.is_empty() {
        tracing::info!(
            installed = ?result.installed,
            "Installed missing dependencies"
        );
    }
    if !result.failed.is_empty() {
        tracing::warn!(
            failed = ?result.failed,
            "Some dependencies could not be installed"
        );
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn test_which_true_is_available() {
        let dep = SystemDependency::new(
            "test-true",
            "which",
            vec!["true".into()],
            "echo",
            vec!["skip".into()],
        );
        assert!(dep.is_available());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_where_cmd_is_available() {
        // On Windows, use 'where' to find 'cmd.exe'
        let dep = SystemDependency::new(
            "test-cmd",
            "where",
            vec!["cmd".into()],
            "echo",
            vec!["skip".into()],
        );
        assert!(dep.is_available());
    }

    #[test]
    fn test_nonexistent_is_not_available() {
        #[cfg(target_os = "linux")]
        let dep = SystemDependency::new(
            "test-fake",
            "which",
            vec!["this-binary-does-not-exist-12345".into()],
            "echo",
            vec!["skip".into()],
        );
        #[cfg(target_os = "windows")]
        let dep = SystemDependency::new(
            "test-fake",
            "where",
            vec!["this-binary-does-not-exist-12345".into()],
            "echo",
            vec!["skip".into()],
        );
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        let dep = SystemDependency::new("test-fake", "false", vec![], "echo", vec!["skip".into()]);
        assert!(!dep.is_available());
    }
}
