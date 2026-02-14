//! Zen Garden Moss - Service orchestration daemon
//!
//! Entry point with CLI dispatch. Install/uninstall run synchronously
//! before the Tokio runtime to prevent accidental daemon startup.
//! All orchestration logic delegated to bootstrap module.

#[cfg(target_os = "windows")]
use garden_moss::ensure_windows_stone_name_config;
use garden_moss::infra::kill_existing_moss_processes_graceful;
#[cfg(target_os = "windows")]
use garden_moss::infra::{
    cleanup_after_service_update, cleanup_updater_process, finalize_service_update,
};
use garden_moss::{init_tracing, run_daemon, Cli, Commands, DaemonConfig};

fn main() -> anyhow::Result<()> {
    let cli = <Cli as clap::Parser>::parse();

    // ── Synchronous subcommands (no Tokio runtime, no daemon) ────────
    // Install and uninstall are pure setup/teardown operations.
    // They must never activate the daemon loop, API server, or service stack.
    if let Some(command) = &cli.command {
        return match command {
            Commands::Install => garden_moss::infra::installer::install(),
            Commands::Uninstall => garden_moss::infra::installer::uninstall(),
            #[cfg(target_os = "windows")]
            Commands::TakeRoot | Commands::InstallService => {
                garden_moss::infra::installer::install()
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
    let (log_tx, _) = tokio::sync::broadcast::channel::<String>(1024);

    // Initialize tracing/logging (returns guard that must be held for process lifetime)
    let _log_guard = init_tracing(&config, log_tx.clone());

    // Handle --force flag: kill existing processes
    if config.force {
        tracing::info!("--force flag set, attempting graceful shutdown of existing moss processes");
        if let Err(e) = kill_existing_moss_processes_graceful().await {
            tracing::warn!(error = ?e, "Failed to shutdown existing processes, continuing anyway");
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // Run daemon (all orchestration in bootstrap::run)
    run_daemon(config, log_tx).await
}
