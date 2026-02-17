//! HTTP server lifecycle management
//!
//! Handles server binding, graceful shutdown, and error handling.
//! Extracted from main.rs for cleaner separation of concerns.

use crate::infra::CompanionRegistry;
use axum::Router;
use garden_common::console::{
    try_boot_banner, try_shutdown_banner, BootBannerInfo, ConsoleEvent, ConsolePrinter,
    EventCategory, EventStatus, ShutdownBannerInfo,
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
            drain_deadline_secs: 8,
            hard_deadline_secs: 15,
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
                .listen(128)
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
/// 3. Server drains in-flight requests (8s deadline)
/// 4. Goodbye announcement, sd_notify STOPPING, process::exit(0)
///
/// Admin/deploy shutdowns call `shutdown_token.cancel()` directly — same cascade.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    listener: TcpListener,
    app: Router,
    api_endpoint: &str,
    console: Arc<ConsolePrinter>,
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

    // Print boot banner to TTY1 (Linux only)
    try_boot_banner(boot_banner.as_ref());

    // MOSS-0004: Notify systemd that we're ready (Type=notify)
    #[cfg(target_os = "linux")]
    {
        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Ready]);
        tracing::debug!("sd_notify: READY=1");
    }

    // MOSS-0004: Start systemd watchdog ping task (WatchdogSec=60)
    // Ping every 25s — well within the 60s watchdog window.
    #[cfg(target_os = "linux")]
    {
        let watchdog_token = shutdown_token.child_token();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(25));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]);
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
        std::process::exit(1);
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

    // Send goodbye announcement after drain (non-blocking — hard watchdog is ticking)
    if let Some(callback) = shutdown_callback {
        tracing::info!("Sending goodbye announcement before shutdown");
        callback().await;
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

    // MOSS-0004: Notify systemd we're stopping (completes the lifecycle)
    #[cfg(target_os = "linux")]
    {
        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]);
        tracing::debug!("sd_notify: STOPPING=1");
    }

    // Print shutdown banner to TTY1 (Linux only)
    try_shutdown_banner(shutdown_banner.as_ref());

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
    tracing::info!("Forcing process exit to ensure clean termination");
    std::process::exit(0);

    // Unreachable, but satisfies the return type for the compiler
    #[allow(unreachable_code)]
    Ok(())
}
