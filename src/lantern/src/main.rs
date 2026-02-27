//! Lantern — Zen Garden dashboard and service registry daemon
//!
//! Bootstrap only: CLI parsing, logging, state init, spawn tasks, serve HTTP.

use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

use garden_lantern::bootstrap::router;
use garden_lantern::tasks::{activity, aggregation, cleanup, discovery};
use garden_lantern::AppState;

#[derive(Parser)]
#[command(name = "lantern")]
#[command(about = "Zen Garden Lantern - Dashboard & service registry daemon")]
struct Cli {
    /// Lantern identifier
    #[arg(long, env = "LANTERN_NAME")]
    lantern_name: Option<String>,

    /// HTTP server port
    #[arg(long, env = "LANTERN_HTTP_PORT")]
    http_port: Option<u16>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, env = "RUST_LOG")]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = cli.log_level.unwrap_or_else(|| "info".to_string());
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&log_level)),
        )
        .init();

    let lantern_name = cli.lantern_name.unwrap_or_else(|| "lantern-01".to_string());
    let http_port = cli
        .http_port
        .unwrap_or(garden_common::constants::LANTERN_HTTP);

    tracing::info!(
        lantern_name = %lantern_name,
        http_port = http_port,
        "Lantern daemon starting"
    );

    // Initialize Koi embedded (mDNS for Lantern — browse + _http._tcp registration)
    let koi_handle = {
        let koi = koi_embedded::Builder::new()
            .service_mode(koi_embedded::ServiceMode::EmbeddedOnly)
            .mdns(true)
            .dns_enabled(false)
            .health(false)
            .certmesh(false)
            .proxy(false)
            .build()
            .expect("Failed to build Koi embedded for Lantern");

        let handle = koi
            .start()
            .await
            .expect("Failed to start Koi embedded for Lantern");

        tracing::info!("Koi embedded started (mDNS browse + registration)");
        std::sync::Arc::new(handle)
    };

    // Register Lantern dashboard as _http._tcp for mDNS discoverability
    if let Ok(mdns) = koi_handle.mdns() {
        let http_txt = garden_common::mdns::build_http_txt(
            &garden_common::mdns::HttpServiceComponent::Lantern,
            "/",
            env!("CARGO_PKG_VERSION"),
        );

        let (ip, _mac) = garden_common::infra::network::get_local_ip_and_mac();
        let ip_opt = if ip == "127.0.0.1" || ip.is_empty() {
            None
        } else {
            Some(ip.clone())
        };

        match mdns.register(koi_embedded::RegisterPayload {
            name: lantern_name.clone(),
            service_type: garden_common::constants::HTTP_SERVICE_TYPE.to_string(),
            port: http_port,
            ip: ip_opt,
            lease_secs: None,
            txt: http_txt,
        }) {
            Ok(_) => {
                tracing::info!(
                    name = %lantern_name,
                    port = http_port,
                    "Lantern registered as _http._tcp on mDNS"
                );
            }
            Err(e) => {
                tracing::warn!(error = ?e, "Failed to register Lantern _http._tcp mDNS");
            }
        }
    } else {
        tracing::warn!("mDNS not available for Lantern HTTP registration");
    }

    // Initialize application state
    let state = AppState::new(lantern_name, http_port, koi_handle);

    // Spawn background tasks
    let _ttl_handle = cleanup::spawn_ttl_cleanup(&state);
    let _agg_handle = aggregation::spawn_aggregation(&state);
    let _activity_handle = activity::spawn_activity_collector(&state);
    let _discovery_handle = discovery::spawn_discovery(&state);

    // Build and serve HTTP
    let app = router::configure(state);
    let addr: SocketAddr = format!("0.0.0.0:{}", http_port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!(?addr, "Lantern HTTP server ready");

    // On Windows interactive mode, open the dashboard in the default browser
    #[cfg(target_os = "windows")]
    if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        let url = format!("http://localhost:{}", http_port);
        if let Err(e) = open::that(&url) {
            tracing::warn!("Failed to open browser: {}", e);
        } else {
            tracing::info!(url = %url, "Opened dashboard in browser");
        }
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Lantern daemon shutdown complete");
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut sigterm = signal(SignalKind::terminate()).expect("Failed to install SIGTERM");
        let mut sigint = signal(SignalKind::interrupt()).expect("Failed to install SIGINT");

        tokio::select! {
            _ = sigterm.recv() => tracing::info!("SIGTERM received"),
            _ = sigint.recv() => tracing::info!("SIGINT received"),
        }
    }

    #[cfg(windows)]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        tracing::info!("Ctrl+C received");
    }
}
