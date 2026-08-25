//! Zen Garden Moss - Service orchestration daemon
//!
//! Entry point with CLI dispatch. Install/uninstall/pre-start run synchronously
//! before the Tokio runtime to prevent accidental daemon startup.
//! All orchestration logic delegated to bootstrap module.

#[cfg(target_os = "windows")]
use garden_moss::ensure_windows_stone_name_config;
use garden_moss::infra::kill_existing_moss_processes_graceful;
#[cfg(target_os = "windows")]
use garden_moss::infra::{
    cleanup_after_service_update, cleanup_updater_process, finalize_service_update,
};
use garden_moss::{Cli, Commands, DaemonConfig, init_tracing, run_daemon};

/// Check if Moss is installed as a system service.
/// Linux: systemd unit file exists. Windows: SCM entry exists.
fn is_installed_as_service() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new("/etc/systemd/system/garden-moss.service").exists()
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("sc")
            .args(["query", "ZenGardenMoss"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

fn main() -> anyhow::Result<()> {
    let cli = <Cli as clap::Parser>::parse();

    // ── Synchronous subcommands (no Tokio runtime, no daemon) ────────
    // Install, uninstall, and pre-start are pure setup/teardown operations.
    // They must never activate the daemon loop, API server, or service stack.
    if let Some(command) = &cli.command {
        return match command {
            Commands::Install { yes, dry_run } => {
                let options = garden_moss::infra::installer::InstallOptions {
                    yes: *yes,
                    dry_run: *dry_run,
                };
                garden_moss::infra::installer::install(&options)
            }
            Commands::Uninstall => garden_moss::infra::installer::uninstall(),
            #[cfg(target_os = "linux")]
            Commands::PreStart { dry_run } => garden_moss::infra::installer::pre_start(*dry_run),
            #[cfg(target_os = "windows")]
            Commands::PreStart { .. } => {
                eprintln!("pre-start is not supported on Windows");
                Ok(())
            }
            #[cfg(target_os = "windows")]
            Commands::TakeRoot | Commands::InstallService => {
                let options = garden_moss::infra::installer::InstallOptions::default();
                garden_moss::infra::installer::install(&options)
            }
        };
    }

    // ── Everything below needs a Tokio runtime ───────────────────────
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async_main(cli))
}

async fn async_main(cli: Cli) -> anyhow::Result<()> {
    // Handle Windows update lifecycle flags (early exit)
    #[cfg(target_os = "windows")]
    if cli.update_finalize {
        return finalize_service_update().await;
    }

    #[cfg(target_os = "windows")]
    if cli.cleanup_old {
        return cleanup_after_service_update().await;
    }

    #[cfg(target_os = "windows")]
    if cli.cleanup_updater {
        // Cleanup temp updater, then continue to run daemon (don't return)
        if let Err(e) = cleanup_updater_process().await {
            eprintln!("Warning: Failed to cleanup updater: {}", e);
        }
    }

    // Windows first-boot: ensure stone_name exists in config BEFORE loading
    // This avoids race condition where async first-boot generates name too late
    #[cfg(target_os = "windows")]
    ensure_windows_stone_name_config().await;

    // Load and merge configuration (CLI > Env > File > Defaults)
    let config = DaemonConfig::from_cli(&cli).await?;

    // Create log broadcast channel (for live SSE streaming via API)
    let (log, _) =
        tokio::sync::broadcast::channel::<String>(garden_common::constants::channels::LOG_STREAM);

    // Initialize tracing/logging (returns guard that must be held for process lifetime)
    let _log_guard = init_tracing(&config, log.clone());

    // Handle --force flag: kill existing processes
    if config.force {
        tracing::info!("--force flag set, attempting graceful shutdown of existing moss processes");
        if let Err(e) = kill_existing_moss_processes_graceful().await {
            tracing::warn!(error = ?e, "Failed to shutdown existing processes, continuing anyway");
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // Ephemeral mode nudge: if not installed as a service, inform the user
    if !is_installed_as_service() {
        tracing::warn!("Moss is running in ephemeral mode (not installed as a service)");
        if cfg!(target_os = "linux") {
            tracing::warn!("To install permanently: sudo garden-moss install");
        } else {
            tracing::warn!("To install permanently: garden-moss install (as Administrator)");
        }
    }

    // Run daemon (all orchestration in bootstrap::run)
    run_daemon(config, log).await
}
