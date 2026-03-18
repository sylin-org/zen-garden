//! Configuration loading and merging
//!
//! Handles the configuration priority chain:
//! - CLI arguments (highest priority)
//! - Environment variables
//! - Configuration file
//! - Defaults (lowest priority)
//!
//! Extracted from main.rs for cleaner separation of concerns.

use crate::{cli::Cli, infra::MossConfig};
use garden_common::console;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// Merged daemon configuration from all sources
#[derive(Clone)]
pub struct DaemonConfig {
    /// Stone identity name
    pub stone_name: String,
    /// HTTP API port
    pub port: u16,
    /// Log level string
    pub log_level: String,
    /// Console output mode
    pub console_mode: console::ConsoleMode,
    /// Fast sync timeout (optional)
    pub fast_sync_timeout: Option<u64>,
    /// Event deduplication TTL in seconds
    pub event_dedup_ttl_secs: u64,
    /// Original file config (for access to all settings)
    pub file_config: Option<MossConfig>,
    /// Whether --force flag was set
    pub force: bool,
}

impl DaemonConfig {
    /// Load and merge configuration from CLI, environment, and file
    ///
    /// Priority: CLI > Env > Config File > Defaults
    pub async fn from_cli(cli: &Cli) -> anyhow::Result<Self> {
        // Load configuration from file first (lowest priority)
        let file_config = MossConfig::load();

        // Merge log level
        let log_level = cli
            .log_level
            .clone()
            .or_else(|| file_config.as_ref().and_then(|c| c.log_level.clone()))
            .unwrap_or_else(|| "info".to_string());

        // Resolve stone name with complex priority chain
        let stone_name = resolve_stone_name(cli, &file_config).await?;

        // Merge port
        let port = cli
            .port
            .or_else(|| file_config.as_ref().and_then(|c| c.port))
            .unwrap_or(garden_common::constants::MOSS_HTTP);

        // Merge fast sync timeout
        let fast_sync_timeout = cli
            .fast_sync_timeout
            .or_else(|| file_config.as_ref().and_then(|c| c.fast_sync_timeout));

        // Determine console mode
        let console_mode = file_config
            .as_ref()
            .and_then(|c| c.console_mode.as_ref())
            .and_then(|mode_str| mode_str.parse::<console::ConsoleMode>().ok())
            .unwrap_or_else(console::detect_platform_console_mode);

        // Event deduplication TTL
        let event_dedup_ttl_secs = file_config
            .as_ref()
            .map(|c| c.event_dedup_ttl_secs())
            .unwrap_or(10);

        Ok(Self {
            stone_name,
            port,
            log_level,
            console_mode,
            fast_sync_timeout,
            event_dedup_ttl_secs,
            file_config,
            force: cli.force,
        })
    }

    /// Get retry delay for Docker connection
    pub fn docker_retry_delay_secs(&self) -> u64 {
        self.file_config
            .as_ref()
            .map(|c| c.docker_retry_delay_secs())
            .unwrap_or(3)
    }
}

/// Resolve stone name with priority chain
///
/// Priority: explicit CLI flag (--stone-name) > config file > cached name > system hostname > STONE_NAME env > default
///
/// The cached stone name (from data_dir/stone-name) provides reliable persistence
/// on Windows where config file reading may fail. On Linux, hostname is typically
/// set correctly so this is less critical.
async fn resolve_stone_name(cli: &Cli, config: &Option<MossConfig>) -> anyhow::Result<String> {
    use crate::infra::load_cached_stone_name;

    let env_stone_name = std::env::var(garden_common::constants::ENV_STONE_NAME).ok();

    // CLI flag only counts if it wasn't set via env var
    let explicit_cli_stone_name = if cli.stone_name.is_some() && env_stone_name.is_none() {
        cli.stone_name.clone()
    } else {
        None
    };

    // Check cached stone name (reliable on Windows)
    let cached_stone_name = load_cached_stone_name();

    let system_hostname = console::get_hostname().await.ok();

    // Warn if env and hostname mismatch
    if let (Some(env_name), Some(sys_name)) = (&env_stone_name, &system_hostname) {
        if env_name != sys_name {
            tracing::warn!(
                env_stone_name = %env_name,
                system_hostname = %sys_name,
                "STONE_NAME env does not match system hostname; preferring hostname (fix systemd unit to remove Environment=STONE_NAME)"
            );
        }
    }

    let stone_name = explicit_cli_stone_name
        .or_else(|| config.as_ref().and_then(|c| c.stone_name.clone()))
        .or(cached_stone_name)
        .or(system_hostname)
        .or(env_stone_name)
        .unwrap_or_else(|| garden_common::constants::DEFAULT_STONE_NAME.to_string());

    Ok(stone_name)
}

/// Initialize tracing/logging based on configuration
///
/// Composes three tracing layers:
/// 1. **stderr** — for journald on Linux, console on Windows
/// 2. **file** — rotating daily log files in `{data_dir}/logs/`
/// 3. **broadcast** — live log events pushed to a broadcast channel (for SSE streaming)
///
/// Returns a `WorkerGuard` that must be held for the lifetime of the process
/// to ensure the non-blocking file writer flushes on shutdown.
pub fn init_tracing(
    config: &DaemonConfig,
    log: tokio::sync::broadcast::Sender<String>,
) -> tracing_appender::non_blocking::WorkerGuard {
    // Per-layer filtering: stderr respects console mode, file+broadcast always capture info+.
    // This ensures log files and SSE streaming work regardless of console mode (critical on
    // Windows where interactive = Informative, not Verbose).
    let stderr_level = match config.console_mode {
        console::ConsoleMode::Verbose => "info",
        _ => "warn",
    };

    // Helper: build an EnvFilter with mdns_sd suppressed
    // mdns_sd emits spurious ERROR logs about IPv6/TYPE_A/TYPE_AAAA on interfaces that work fine
    let make_filter = |default: &str| -> EnvFilter {
        EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(default))
            .add_directive("mdns_sd=off".parse().unwrap())
    };

    // Layer 1: stderr — console-mode-aware (warn unless verbose)
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .with_filter(make_filter(stderr_level));

    // Layer 2: rotating log file — always info+ (non-blocking)
    let logs_dir = garden_common::constants::paths::logs_dir();
    let _ = std::fs::create_dir_all(&logs_dir); // best-effort
    let file_appender = tracing_appender::rolling::daily(&logs_dir, "garden-moss.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking)
        .with_filter(make_filter("info"));

    // Layer 3: broadcast channel — always info+ (for live SSE streaming)
    let broadcast_layer =
        crate::infra::log_broadcast::LogBroadcastLayer::new(log).with_filter(make_filter("info"));

    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .with(broadcast_layer)
        .init();

    // Legacy structured log (keep for debugging until full migration)
    tracing::info!(
        stone_name = %config.stone_name,
        port = config.port,
        log_level = %config.log_level,
        fast_sync_timeout = ?config.fast_sync_timeout,
        config_loaded = config.file_config.is_some(),
        "Moss daemon starting with merged configuration (priority: CLI > Env > Config > Defaults)"
    );

    guard
}

/// Ensure Windows has a stone_name in config before loading
///
/// Called synchronously in main.rs BEFORE config loading on Windows.
/// This handles the race condition where async first-boot would generate a name
/// too late (after config loading used the default).
///
/// Logic:
/// 1. If stone-name cache exists → use cached name (authoritative)
/// 2. If hardware-id file exists → not first boot → do nothing
/// 3. If config already has stone_name → cache it and return
/// 4. Otherwise → generate name, save to cache AND config
#[cfg(target_os = "windows")]
pub async fn ensure_windows_stone_name_config() {
    use crate::infra::{load_cached_stone_name, save_stone_name_cache};
    use std::path::PathBuf;

    // Check if we have a cached stone name (authoritative source)
    if let Some(cached_name) = load_cached_stone_name() {
        eprintln!("[stone-name] Using cached name: {}", cached_name);
        return;
    }

    // Check if this is first boot (hardware-id file doesn't exist)
    let data_dir = PathBuf::from(garden_common::constants::paths::data_dir());
    let hardware_id_path = data_dir.join("hardware-id");

    if hardware_id_path.exists() {
        // Not first boot but no cached name - check config and cache it
        if let Some(config) = MossConfig::load() {
            if let Some(name) = config.stone_name {
                eprintln!("[stone-name] Caching name from config: {}", name);
                if let Err(e) = save_stone_name_cache(&name).await {
                    eprintln!("[stone-name] Warning: Failed to cache name: {}", e);
                }
                return;
            }
        }
        // No name in config either - this is a problem, but don't generate new name
        eprintln!("[stone-name] Warning: No cached name and no config name found");
        return;
    }

    // Check if config already has a stone_name
    if let Some(config) = MossConfig::load() {
        if let Some(name) = config.stone_name {
            // Config has a name - cache it
            eprintln!("[stone-name] Caching existing config name: {}", name);
            if let Err(e) = save_stone_name_cache(&name).await {
                eprintln!("[stone-name] Warning: Failed to cache name: {}", e);
            }
            return;
        }
    }

    // First boot and no stone_name anywhere - generate one now
    eprintln!("[first-boot] Generating stone name for Windows...");

    // Use a local runtime — called before the daemon runtime is created
    let local_runtime = crate::infra::platform::windows::WindowsRuntime::new();

    let new_name = match console::generate_unique_name_windows(&local_runtime).await {
        Ok(name) => name,
        Err(e) => {
            eprintln!("[first-boot] Failed to generate stone name: {}", e);
            return;
        }
    };

    eprintln!("[first-boot] Generated name: {}", new_name);

    // Save to cache (authoritative source)
    if let Err(e) = save_stone_name_cache(&new_name).await {
        eprintln!("[first-boot] Failed to cache stone name: {}", e);
    } else {
        eprintln!("[first-boot] Cached stone name");
    }

    // Also write to config file (creates if needed)
    if let Err(e) = console::update_moss_config(&local_runtime, &new_name).await {
        eprintln!("[first-boot] Failed to save config: {}", e);
    } else {
        eprintln!("[first-boot] Saved stone name to config");
    }
}
