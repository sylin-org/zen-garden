//! Platform-specific infrastructure.
//!
//! - `registry` — Windows registry read/write via `winreg` (Windows-only).
//! - `service_env` — Cross-platform read/write of manageable environment
//!   variables for adopted (bare-metal) services.

#[cfg(target_os = "windows")]
pub mod registry;

pub mod service_env;
