//! Platform abstraction utilities
//!
//! Provides platform-aware path resolution with centralized
//! OS-specific conditionals.

use std::path::PathBuf;

/// Platform paths interface
pub trait PlatformPaths {
    fn data_dir(&self) -> PathBuf;
    fn config_dir(&self) -> PathBuf;
    fn temp_dir(&self) -> PathBuf;
}

/// Windows platform paths
#[cfg(target_os = "windows")]
pub struct WindowsPaths;

#[cfg(target_os = "windows")]
impl PlatformPaths for WindowsPaths {
    fn data_dir(&self) -> PathBuf {
        let programdata = std::env::var("PROGRAMDATA").unwrap_or_else(|_| "C:\\ProgramData".into());
        PathBuf::from(programdata).join("zen-garden")
    }

    fn config_dir(&self) -> PathBuf {
        PathBuf::from(".zen-garden")
    }

    fn temp_dir(&self) -> PathBuf {
        std::env::temp_dir().join("zen-garden")
    }
}

/// Unix platform paths
#[cfg(target_os = "linux")]
pub struct UnixPaths;

#[cfg(target_os = "linux")]
impl PlatformPaths for UnixPaths {
    fn data_dir(&self) -> PathBuf {
        PathBuf::from("/var/lib/zen-garden")
    }

    fn config_dir(&self) -> PathBuf {
        PathBuf::from("/etc/zen-garden")
    }

    fn temp_dir(&self) -> PathBuf {
        PathBuf::from("/tmp/zen-garden")
    }
}

/// Get platform-specific paths implementation
pub fn get_platform_paths() -> Box<dyn PlatformPaths> {
    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsPaths)
    }

    #[cfg(target_os = "linux")]
    {
        Box::new(UnixPaths)
    }
}

/// Convenience function - get data directory for current platform
pub fn data_dir() -> PathBuf {
    PathBuf::from(crate::constants::paths::data_dir())
}

/// Convenience function - get config directory for current platform
pub fn config_dir() -> PathBuf {
    PathBuf::from(crate::constants::paths::config_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_paths() {
        let paths = get_platform_paths();

        // Data dir should contain "zen-garden"
        let data = paths.data_dir();
        assert!(data.to_string_lossy().contains("zen-garden"));

        // Config dir should contain "zen-garden"
        let config = paths.config_dir();
        assert!(config.to_string_lossy().contains("zen-garden"));

        // Temp dir should contain "zen-garden"
        let temp = paths.temp_dir();
        assert!(temp.to_string_lossy().contains("zen-garden"));
    }

    #[test]
    fn test_convenience_functions() {
        let data = data_dir();
        assert!(data.to_string_lossy().contains("zen-garden"));

        let config = config_dir();
        assert!(config.to_string_lossy().contains("zen-garden"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_windows_paths() {
        let paths = WindowsPaths;

        let data = paths.data_dir();
        assert!(data.to_string_lossy().contains("zen-garden"));

        let config = paths.config_dir();
        assert_eq!(config.to_string_lossy(), ".zen-garden");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_unix_paths() {
        let paths = UnixPaths;

        let data = paths.data_dir();
        assert_eq!(data.to_string_lossy(), "/var/lib/zen-garden");

        let config = paths.config_dir();
        assert_eq!(config.to_string_lossy(), "/etc/zen-garden");

        let temp = paths.temp_dir();
        assert_eq!(temp.to_string_lossy(), "/tmp/zen-garden");
    }
}
