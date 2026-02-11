//! Process lifecycle management - Moss-specific wrappers
//!
//! This module provides Moss-specific convenience functions that wrap
//! the generic process utilities from `garden_common::infra::process`.

use garden_common::infra::process;

/// Attempt graceful shutdown via HTTP, fallback to force kill
///
/// Moss-specific wrapper that uses the standard Moss HTTP port and binary name.
pub async fn kill_existing_moss_processes_graceful() -> anyhow::Result<()> {
    let shutdown_url = format!("http://127.0.0.1:{}/admin/shutdown", garden_common::constants::MOSS_HTTP);
    process::kill_process_graceful(garden_common::constants::MOSS_BINARY, &shutdown_url).await
}

/// Check if any moss processes are running (excluding current)
///
/// Moss-specific wrapper for process detection.
pub fn check_moss_processes_exist() -> bool {
    process::check_process_exists(garden_common::constants::MOSS_BINARY)
}

/// Force kill all moss processes (excluding current)
///
/// Moss-specific wrapper for force kill.
pub fn kill_existing_moss_processes() -> anyhow::Result<()> {
    process::kill_process(garden_common::constants::MOSS_BINARY)
}
