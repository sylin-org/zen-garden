//! Platform-specific infrastructure (ARCH-0002).
//!
//! - `linux` / `windows` — `PlatformRuntime` implementations (console + signals).
//! - `registry` — Windows registry read/write via `winreg` (Windows-only).
//! - `service_env` — Cross-platform read/write of manageable environment
//!   variables for adopted (bare-metal) services.

#[cfg(target_os = "linux")]
pub mod linux;
pub mod windows;

#[cfg(target_os = "windows")]
pub mod registry;

pub mod service_env;

pub use garden_common::PlatformRuntime;

use std::sync::Arc;

/// Create the concrete `PlatformRuntime` for the current platform.
///
/// This is the **single** `#[cfg]` for runtime selection. All code above this
/// point is platform-agnostic.
pub fn create_runtime() -> Arc<dyn PlatformRuntime> {
    #[cfg(target_os = "linux")]
    {
        Arc::new(linux::LinuxRuntime::new())
    }

    #[cfg(target_os = "windows")]
    {
        Arc::new(windows::WindowsRuntime::new())
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Arc::new(windows::WindowsRuntime::new()) // stdout fallback for macOS / other
    }
}
