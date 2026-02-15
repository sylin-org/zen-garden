//! TLS server support for Pond HTTPS binding
//!
//! When pond security is active, Moss spawns a second listener on :7183
//! using the stone's certmesh-issued certificate. This module provides
//! the TLS listener wrapper and configuration loading.
//!
//! The HTTPS port serves authenticated routes only. The HTTP port (:7185)
//! remains active as a "lobby" for health checks, pond join, and public status.

use axum::Router;
use garden_common::console::{ConsoleEvent, ConsolePrinter, EventCategory, EventStatus};
use rustls::ServerConfig;
use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// TLS-wrapped TCP listener that implements axum's `Listener` trait.
///
/// Accepts TLS connections using the stone's certmesh-issued certificate.
/// Failed TLS handshakes are logged and retried (the listener never stops).
pub struct TlsListener {
    inner: TcpListener,
    acceptor: TlsAcceptor,
}

impl TlsListener {
    pub fn new(inner: TcpListener, acceptor: TlsAcceptor) -> Self {
        Self { inner, acceptor }
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, addr) = match self.inner.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    tracing::warn!(error = ?e, "TCP accept failed on HTTPS listener");
                    // Brief sleep to avoid busy-loop on persistent errors
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    continue;
                }
            };

            match self.acceptor.accept(stream).await {
                Ok(tls_stream) => return (tls_stream, addr),
                Err(e) => {
                    tracing::debug!(
                        error = ?e,
                        from = %addr,
                        "TLS handshake failed (non-pond client or invalid cert)"
                    );
                    continue;
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

/// Load TLS configuration from certmesh PEM files.
///
/// Reads the stone's certificate chain and private key from the certmesh
/// certificate directory (`~/.koi/certs/<hostname>/`).
///
/// Returns `None` if cert files don't exist (stone not enrolled in pond).
pub fn load_tls_config(cert_path: &Path, key_path: &Path) -> anyhow::Result<ServerConfig> {
    use rustls_pemfile::{certs, private_key};
    use std::io::BufReader;

    let cert_file = std::fs::File::open(cert_path)
        .map_err(|e| anyhow::anyhow!("Failed to open cert file {}: {}", cert_path.display(), e))?;
    let key_file = std::fs::File::open(key_path)
        .map_err(|e| anyhow::anyhow!("Failed to open key file {}: {}", key_path.display(), e))?;

    let cert_chain: Vec<_> = certs(&mut BufReader::new(cert_file))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("Failed to parse certificate PEM: {}", e))?;

    let key = private_key(&mut BufReader::new(key_file))
        .map_err(|e| anyhow::anyhow!("Failed to parse private key PEM: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("No private key found in {}", key_path.display()))?;

    if cert_chain.is_empty() {
        anyhow::bail!("No certificates found in {}", cert_path.display());
    }

    // Ensure a crypto provider is installed (reqwest may have done this already)
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let config = ServerConfig::builder()
        .with_no_client_auth() // Phase 2: server TLS only. mTLS deferred to Phase 4.
        .with_single_cert(cert_chain, key)
        .map_err(|e| anyhow::anyhow!("Failed to build TLS config: {}", e))?;

    Ok(config)
}

/// Attempt to bind and start the HTTPS listener on the pond port.
///
/// Returns the spawned task handle if successful, or None if cert files
/// don't exist (stone not enrolled in pond).
pub async fn try_start_https(
    port: u16,
    stone_name: &str,
    app: Router,
    console: &ConsolePrinter,
    shutdown_notify: Arc<tokio::sync::Notify>,
) -> Option<tokio::task::JoinHandle<()>> {
    // Locate certmesh certificate files
    let certs_dir = std::path::PathBuf::from(garden_common::constants::paths::data_dir())
        .join("koi")
        .join("certs")
        .join(stone_name);
    let cert_path = certs_dir.join("fullchain.pem");
    let key_path = certs_dir.join("key.pem");

    if !cert_path.exists() || !key_path.exists() {
        tracing::debug!(
            cert = %cert_path.display(),
            key = %key_path.display(),
            "No certmesh certs found — HTTPS listener not started"
        );
        return None;
    }

    // Load TLS configuration
    let tls_config = match load_tls_config(&cert_path, &key_path) {
        Ok(config) => config,
        Err(e) => {
            tracing::error!(error = ?e, "Failed to load TLS config — HTTPS listener not started");
            console.emit(ConsoleEvent::new(
                EventCategory::Security,
                EventStatus::Failed,
                format!("TLS config: {}", e),
            ));
            return None;
        }
    };

    let acceptor = TlsAcceptor::from(Arc::new(tls_config));

    // Bind TCP socket on HTTPS port (reuse the same SO_REUSEADDR logic)
    let addr: SocketAddr = format!("0.0.0.0:{}", port).parse().unwrap();
    let tcp_listener = match super::server::bind(port, console).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!(error = ?e, port, "Failed to bind HTTPS port");
            console.emit(ConsoleEvent::new(
                EventCategory::Security,
                EventStatus::Failed,
                format!("HTTPS bind failed on :{}: {}", port, e),
            ));
            return None;
        }
    };

    let tls_listener = TlsListener::new(tcp_listener, acceptor);

    tracing::info!(
        port,
        cert = %cert_path.display(),
        "HTTPS listener starting (pond security)"
    );

    console.emit(ConsoleEvent::new(
        EventCategory::Security,
        EventStatus::Ready,
        format!("HTTPS server → https://{}:{}", addr.ip(), port),
    ));

    // Spawn HTTPS server task alongside HTTP
    let handle = tokio::spawn(async move {
        let server = axum::serve(tls_listener, app).with_graceful_shutdown(async move {
            tokio::select! {
                _ = shutdown_notify.notified() => {
                    tracing::info!("HTTPS server: admin shutdown requested");
                }
                _ = garden_common::infra::platform::shutdown_signal() => {
                    tracing::info!("HTTPS server: OS shutdown signal");
                }
            }
        });

        if let Err(e) = server.await {
            tracing::error!(error = ?e, "HTTPS server error");
        }
    });

    Some(handle)
}
