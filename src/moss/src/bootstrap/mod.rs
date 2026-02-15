//! Bootstrap and initialization logic
//!
//! Handles daemon startup sequence:
//! - Configuration loading and merging
//! - Preinstall manifest loading
//! - First boot initialization
//! - Auto-install requested offerings
//! - HTTP router configuration
//! - HTTP server lifecycle
//! - Docker/capabilities initialization
//! - Main daemon orchestration

pub mod config;
pub mod first_boot;
pub mod preinstall;
pub mod router;
pub mod run;
pub mod server;
pub mod startup;
pub mod tls;

#[cfg(target_os = "windows")]
pub use config::ensure_windows_stone_name_config;
pub use config::{init_tracing, DaemonConfig};
pub use first_boot::run_first_boot_initialization;
pub use preinstall::{load_preinstall_manifest, PreInstallManifest};
pub use run::run as run_daemon;
pub use server::{bind as bind_server, run as run_server, ServerConfig};
pub use startup::{connect_docker, init_capabilities, DockerConfig};
