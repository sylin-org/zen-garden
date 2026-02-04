//! HTTP server lifecycle management
//!
//! Handles server binding, graceful shutdown, and error handling.
//! Extracted from main.rs for cleaner separation of concerns.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use axum::Router;
use tokio::net::TcpListener;
use garden_common::console::{
    ConsolePrinter, ConsoleEvent, EventCategory, EventStatus,
    BootBannerInfo, ShutdownBannerInfo, try_boot_banner, try_shutdown_banner,
};
use garden_common::infra::platform::shutdown_signal;

/// Server configuration
pub struct ServerConfig {
    pub port: u16,
    pub graceful_shutdown_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: garden_common::ports::MOSS_HTTP,
            graceful_shutdown_timeout_secs: 5,
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
    socket.set_reuse_address(true)
        .map_err(|e| anyhow::anyhow!("Failed to set SO_REUSEADDR: {}", e))?;

    // Set non-blocking before converting to tokio
    socket.set_nonblocking(true)
        .map_err(|e| anyhow::anyhow!("Failed to set non-blocking: {}", e))?;

    // Bind the socket
    match socket.bind(&addr.into()) {
        Ok(()) => {
            // Listen with backlog
            socket.listen(128)
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
                    addr.ip(), addr.port(), e
                )
            };

            console.emit(ConsoleEvent::new(
                EventCategory::System,
                EventStatus::Failed,
                error_msg.clone()
            ));

            anyhow::bail!(error_msg);
        }
    }
}

/// Shutdown callback type for goodbye announcements
pub type ShutdownCallback = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// Run the HTTP server with graceful shutdown support
///
/// This function handles:
/// - Server startup logging
/// - Graceful shutdown on SIGTERM/SIGINT/Ctrl+C
/// - Admin-initiated shutdown via notify channel
/// - In-flight request draining
/// - Goodbye announcement via shutdown_callback (if provided)
pub async fn run(
    listener: TcpListener,
    app: Router,
    api_endpoint: &str,
    console: Arc<ConsolePrinter>,
    shutdown_notify: Arc<tokio::sync::Notify>,
    config: ServerConfig,
    shutdown_callback: Option<ShutdownCallback>,
    boot_banner: Option<BootBannerInfo>,
    shutdown_banner: Option<ShutdownBannerInfo>,
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
        format!("HTTP server → {}", api_endpoint)
    ));

    // Print boot banner to TTY1 (Linux only)
    try_boot_banner(boot_banner.as_ref());

    // Create server with graceful shutdown
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("Shutdown signal received, initiating graceful shutdown");

            // Send goodbye announcement if callback provided
            if let Some(callback) = shutdown_callback {
                tracing::info!("Sending goodbye announcement before shutdown");
                callback().await;
            }
        });

    // Clone console for shutdown events
    let shutdown_console = console.clone();

    // Run server with shutdown coordination
    tokio::select! {
        result = server => {
            if let Err(e) = result {
                tracing::error!(error = ?e, "Server error");
                return Err(e.into());
            }
        }
        _ = shutdown_notify.notified() => {
            tracing::info!("Admin shutdown requested");

            shutdown_console.emit(ConsoleEvent::new(
                EventCategory::System,
                EventStatus::Shutting,
                "Admin requested".to_string()
            ));
        }
    }

    // Allow in-flight requests to complete
    tracing::info!("Waiting up to {}s for in-flight requests to complete", config.graceful_shutdown_timeout_secs);

    console.emit(ConsoleEvent::new(
        EventCategory::System,
        EventStatus::Draining,
        "In-flight requests".to_string()
    ));
    tokio::time::sleep(tokio::time::Duration::from_secs(config.graceful_shutdown_timeout_secs)).await;

    tracing::info!("Moss daemon shutdown complete");

    // Print shutdown banner to TTY1 (Linux only)
    try_shutdown_banner(shutdown_banner.as_ref());

    console.emit(ConsoleEvent::new(
        EventCategory::System,
        EventStatus::Stopped,
        "Shutdown complete".to_string()
    ));

    // Windows-only: Force exit to ensure ports are released
    // On Windows, SSE connections and background tasks can keep the tokio runtime alive
    // even after the server has stopped. This prevents the self-update flow from completing
    // because the temp updater waits for the old process to exit.
    // On Linux, we let the process exit naturally so systemd can properly detect the exit.
    #[cfg(target_os = "windows")]
    {
        tracing::info!("Windows: forcing process exit to release ports");
        std::process::exit(0);
    }

    #[cfg(not(target_os = "windows"))]
    Ok(())
}
