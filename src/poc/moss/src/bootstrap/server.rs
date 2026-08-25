//! HTTP server lifecycle management
//!
//! Handles server binding, graceful shutdown, and error handling.
//! Extracted from main.rs for cleaner separation of concerns.

use crate::infra::companions::CompanionRegistry;
use axum::Router;
use garden_common::PlatformRuntime;
use garden_common::console::{
    BootBannerInfo, ConsoleEvent, ConsolePrinter, EventCategory, EventStatus, ShutdownBannerInfo,
};
use garden_common::infra::platform::shutdown_signal;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// Server configuration
pub struct ServerConfig {
    pub port: u16,
    /// Maximum time to wait for the server to drain active connections (e.g., SSE streams)
    /// after the graceful shutdown signal fires. If exceeded, the server is dropped and
    /// the process proceeds to exit. Prevents indefinite hangs from long-lived connections.
    pub drain_deadline_secs: u64,
    /// Hard deadline after shutdown signal: if the process hasn't exited by then,
    /// force-exit with process::exit(). This is the last-resort safety net — catches
    /// any combination of stalled drains, blocking tasks, or OS threads.
    pub hard_deadline_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: garden_common::constants::MOSS_HTTP,
            drain_deadline_secs: garden_common::constants::server::DRAIN_DEADLINE_SECS,
            hard_deadline_secs: garden_common::constants::server::HARD_DEADLINE_SECS,
        }
    }
}

/// Bind to the specified address with user-friendly error messages
///
/// Uses SO_REUSEADDR to allow rebinding to a port in TIME_WAIT state.
/// This is critical for Windows self-update where the old process exits
/// but the socket remains in TIME_WAIT for up to 2 minutes.
pub async fn bind(port: u16, console: &ConsolePrinter) -> anyhow::Result<TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};

    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;

    // Create socket with SO_REUSEADDR to allow rebinding during TIME_WAIT
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
        .map_err(|e| anyhow::anyhow!("Failed to create socket: {}", e))?;

    // Set SO_REUSEADDR - critical for Windows self-update
    socket
        .set_reuse_address(true)
        .map_err(|e| anyhow::anyhow!("Failed to set SO_REUSEADDR: {}", e))?;

    // Set non-blocking before converting to tokio
    socket
        .set_nonblocking(true)
        .map_err(|e| anyhow::anyhow!("Failed to set non-blocking: {}", e))?;

    // Bind the socket
    match socket.bind(&addr.into()) {
        Ok(()) => {
            // Listen with backlog
            socket
                .listen(garden_common::constants::server::TCP_BACKLOG)
                .map_err(|e| anyhow::anyhow!("Failed to listen: {}", e))?;

            // Convert to tokio TcpListener
            let std_listener: std::net::TcpListener = socket.into();
            let listener = TcpListener::from_std(std_listener)
                .map_err(|e| anyhow::anyhow!("Failed to convert to tokio listener: {}", e))?;

            tracing::debug!(port = port, "Bound with SO_REUSEADDR");
            Ok(listener)
        }
        Err(e) => {
            let error_msg = if e.kind() == std::io::ErrorKind::AddrInUse {
                format!(
                    "Port {} is already in use. Another garden-moss instance may be running.\n\
                    Try: Stop-Process -Name garden-moss -Force\n\
                    Or use a different port: garden-moss --port <port>",
                    port
                )
            } else {
                format!(
                    "Failed to bind HTTP server to {}:{}: {}\n\
                    Check firewall settings and ensure the port is available.",
                    addr.ip(),
                    addr.port(),
                    e
                )
            };

            console.emit(ConsoleEvent::new(
                EventCategory::System,
                EventStatus::Failed,
                error_msg.clone(),
            ));

            anyhow::bail!(error_msg);
        }
    }
}

/// Shutdown callback type for goodbye announcements
pub type ShutdownCallback = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// Run the HTTP server with graceful shutdown support
///
/// Shutdown flow (MOSS-0004):
/// 1. SIGTERM/SIGINT arrives → signal handler cancels `shutdown_token`
/// 2. Token cancellation cascades to: server (graceful_shutdown), SSE streams,
///    all background tasks, watchdog, drain deadline
/// 3. Goodbye announcement sent (3x UDP for reliability) — peers stop routing
/// 4. Server drains in-flight requests (8s deadline)
/// 5. Topology flush, sd_notify STOPPING, process::exit(0)
///
/// Admin/deploy shutdowns call `shutdown_token.cancel()` directly — same cascade.
#[expect(clippy::too_many_arguments)]
pub async fn run(
    listener: TcpListener,
    app: Router,
    api_endpoint: &str,
    console: Arc<ConsolePrinter>,
    runtime: Arc<dyn PlatformRuntime>,
    shutdown_token: CancellationToken,
    config: ServerConfig,
    shutdown_callback: Option<ShutdownCallback>,
    boot_banner: Option<BootBannerInfo>,
    shutdown_banner: Option<ShutdownBannerInfo>,
    companion_registry: Option<Arc<CompanionRegistry>>,
) -> anyhow::Result<()> {
    let addr = listener.local_addr()?;

    tracing::info!(
        ?addr,
        api_endpoint = %api_endpoint,
        body_limit_mb = 200,
        "Moss HTTP server ready with 200 MB body limit configured"
    );

    // Emit HTTP server ready event
    console.emit(ConsoleEvent::new(
        EventCategory::System,
        EventStatus::Ready,
        format!("HTTP server → {}", api_endpoint),
    ));

    // Print boot banner to physical console (platform-appropriate output)
    if let Some(ref b) = boot_banner {
        runtime.print_boot_banner(b);
    }

    // MOSS-0004: Notify process supervisor that we're ready
    runtime.notify_ready();

    // MOSS-0004: Start watchdog ping task (WatchdogSec=60 on Linux)
    // Ping every 25s — well within the 60s watchdog window.
    {
        let watchdog_runtime = runtime.clone();
        let watchdog_token = shutdown_token.child_token();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
                garden_common::constants::server::WATCHDOG_PING_SECS,
            ));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        watchdog_runtime.notify_watchdog();
                    }
                    _ = watchdog_token.cancelled() => break,
                }
            }
        });
    }

    // ── Shutdown orchestration (MOSS-0004) ────────────────────────────
    // ONE task handles OS signals → cancels the token. Everything cascades.
    // Deploy/admin handlers cancel the same token directly from their API handlers.
    let signal_token = shutdown_token.clone();
    let hard_deadline_secs = config.hard_deadline_secs;
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("OS shutdown signal received, cancelling shutdown token");
        signal_token.cancel();
    });

    // Hard-deadline watchdog: starts counting only after token cancellation.
    // If the process is still alive after hard_deadline_secs, force-exit.
    let watchdog_token = shutdown_token.child_token();
    tokio::spawn(async move {
        watchdog_token.cancelled().await;
        tokio::time::sleep(tokio::time::Duration::from_secs(hard_deadline_secs)).await;
        tracing::error!(
            deadline_secs = hard_deadline_secs,
            "Shutdown deadline exceeded — forcing process exit"
        );
        std::process::exit(garden_common::constants::server::exit::FATAL);
    });

    // DEPLOY-0001 mark-good: once moss survives startup (MARK_GOOD_SECS), commit the upgrade by
    // deleting the rollback snapshot and clearing the crash-loop boot counter. If moss crashes
    // before this fires, the snapshot survives so the supervisor rolls back — the Android watchdog,
    // or on systemd the in-process `crash_loop_guard` (which counts boots-since-upgrade and rolls
    // back after the threshold). Linux/Android only — pre_start (and thus the snapshot) exist only
    // there (the phone runs a linux-musl build, so target_os = "linux" covers it); Windows
    // self-manages via its updater.
    #[cfg(target_os = "linux")]
    tokio::spawn(async {
        tokio::time::sleep(tokio::time::Duration::from_secs(
            garden_common::constants::server::MARK_GOOD_SECS,
        ))
        .await;
        crate::infra::installer::pre_start::commit_upgrade();
        crate::infra::installer::pre_start::reset_boot_attempts();
        tracing::info!("Upgrade marked good — removed rollback snapshot");
    });

    // SIGTERM all companions immediately when shutdown is triggered.
    // Companions are not critical — giving them early notice lets them clean up
    // while the HTTP server is still draining its own connections.
    if let Some(ref registry) = companion_registry {
        let companion_term_registry = Arc::clone(registry);
        let companion_term_token = shutdown_token.child_token();
        tokio::spawn(async move {
            companion_term_token.cancelled().await;
            tracing::info!("Shutdown triggered — sending SIGTERM to all Companions");
            companion_term_registry.sigterm_all().await;
        });
    }

    // Server stops accepting connections when the token is cancelled
    let server_token = shutdown_token.clone();
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move { server_token.cancelled().await });

    // Clone console for shutdown events
    let shutdown_console = console.clone();

    // ── Goodbye + flush: fire BEFORE drain so peers learn early ─────
    // The goodbye tells other stones to stop routing to us, which is
    // exactly what we want while draining in-flight HTTP requests.
    // Topology flush ensures on-disk state is current before the server
    // stops accepting new connections.
    //
    // This runs inside a task gated on token cancellation so it doesn't
    // block the server's normal operation, but fires as soon as shutdown
    // is requested — giving peers maximum notice time.
    let goodbye_token = shutdown_token.child_token();
    let goodbye_callback = shutdown_callback;
    tokio::spawn(async move {
        goodbye_token.cancelled().await;
        if let Some(callback) = goodbye_callback {
            tracing::info!("Shutdown triggered — sending goodbye + flushing topology");
            callback().await;
        }
    });

    // Drain with a deadline. The timer starts only AFTER the token is cancelled.
    // Without this gate, the timer races against the server's entire lifetime
    // and kills the process N seconds after startup.
    let drain_token = shutdown_token.clone();
    let drain_deadline = tokio::time::Duration::from_secs(config.drain_deadline_secs);
    let drained_cleanly = tokio::select! {
        result = server => {
            if let Err(ref e) = result {
                tracing::error!(error = ?e, "Server error");
                return Err(anyhow::anyhow!("Server error: {}", e));
            }
            true
        }
        _ = async {
            drain_token.cancelled().await;
            tokio::time::sleep(drain_deadline).await;
        } => {
            tracing::warn!(
                deadline_secs = config.drain_deadline_secs,
                "Server drain deadline exceeded — dropping server (SSE/long-lived connections will be severed)"
            );
            false
        }
    };

    if drained_cleanly {
        tracing::info!("All connections drained cleanly");
    }

    shutdown_console.emit(ConsoleEvent::new(
        EventCategory::System,
        EventStatus::Shutting,
        "Shutting down".to_string(),
    ));

    // SIGKILL any companion processes that survived the SIGTERM grace period.
    // Without this, orphaned companions keep the systemd CGroup alive and
    // delay the unit transition to `inactive`, blocking restarts/updates.
    if let Some(ref registry) = companion_registry {
        registry.kill_all_survivors().await;
    }

    // Brief pause for any final async cleanup before the runtime drops
    // and aborts all remaining spawned tasks
    tracing::info!("Final cleanup before exit");
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    tracing::info!("Moss daemon shutdown complete");

    // Notify process supervisor we're stopping (completes the lifecycle)
    runtime.notify_stopping();

    // Print shutdown banner to physical console (platform-appropriate output)
    if let Some(ref b) = shutdown_banner {
        runtime.print_shutdown_banner(b);
    }

    console.emit(ConsoleEvent::new(
        EventCategory::System,
        EventStatus::Stopped,
        "Shutdown complete".to_string(),
    ));

    // Force exit on all platforms. Without this, background tasks (41+ tokio::spawn
    // fire-and-forget tasks, OS threads like udev, SSE connections) can keep the
    // process alive indefinitely. The hard-deadline watchdog above is a safety net,
    // but this explicit exit ensures clean termination in the normal path too.
    //
    // On Windows: releases ports so the temp updater can rebind
    // On Linux: ensures systemd sees the exit immediately for Restart=always
    // DEPLOY-0001 exit-code contract: if a validated upgrade is staged, exit RESTART_APPLY so the
    // supervisor runs `garden-moss pre-start` (apply) then respawns. This is read by the hand-rolled
    // Android watchdog; systemd (`Restart=always`) and the Windows SCM respawn on any exit and honor
    // `systemctl stop`, so the code is harmless there. Otherwise it's a clean STOP.
    let staged_pending = std::path::Path::new(&garden_common::constants::paths::staging_dir())
        .join("validated")
        .join("bin")
        .exists();
    let code = if staged_pending {
        garden_common::constants::server::exit::RESTART_APPLY
    } else {
        garden_common::constants::server::exit::STOP
    };
    tracing::info!(
        exit_code = code,
        staged = staged_pending,
        "Forcing process exit to ensure clean termination"
    );
    std::process::exit(code);

    // Unreachable, but satisfies the return type for the compiler
    #[allow(unreachable_code)]
    Ok(())
}
