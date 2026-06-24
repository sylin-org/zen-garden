//! Loopback-only signing-oracle listener (pond authorization plane).
//!
//! Serves `POST /api/v1/pond/sign` on `127.0.0.1:MOSS_SIGN_LOOPBACK` **only**.
//! Signing is an impersonation oracle — anyone who can reach it can have Moss
//! vouch for this stone's identity — so it is confined to loopback at the OS
//! level (a remote packet is never routed to a `127.0.0.1` socket), independent
//! of any application-layer guard. The route is deliberately absent from the
//! `0.0.0.0` API routers (`router::configure`/`configure_public`).
//!
//! `rake` asks its *local* Moss to sign each request here, then sends the clear
//! request (carrying the envelope) to the target stone.

use crate::Moss;
use axum::{Router, routing::post};
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;

/// Spawn the loopback signing oracle. Bind failure is non-fatal — it disables
/// per-request signing (rake falls back to the existing transport) rather than
/// aborting Moss startup.
pub async fn spawn(state: Moss, shutdown: CancellationToken) {
    let port = garden_common::constants::MOSS_SIGN_LOOPBACK;

    let listener = match bind_loopback(port).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(
                error = %e,
                port,
                "Pond sign oracle bind failed — per-request signing unavailable"
            );
            return;
        }
    };

    let app = Router::new()
        .route("/api/v1/pond/sign", post(crate::api::v1::pond::pond_sign_v1))
        .with_state(state);

    tracing::info!(port, "Pond sign oracle listening on 127.0.0.1 (loopback only)");

    tokio::spawn(async move {
        let server = axum::serve(listener, app)
            .with_graceful_shutdown(async move { shutdown.cancelled().await });
        if let Err(e) = server.await {
            tracing::error!(error = %e, "Pond sign oracle stopped with error");
        }
    });
}

/// Bind a `127.0.0.1` TCP listener with `SO_REUSEADDR` so a fast restart (e.g.
/// self-update) does not fail on a socket lingering in `TIME_WAIT`.
async fn bind_loopback(port: u16) -> anyhow::Result<tokio::net::TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};

    let addr: SocketAddr = ([127, 0, 0, 1], port).into();

    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
        .map_err(|e| anyhow::anyhow!("sign socket create: {e}"))?;
    socket
        .set_reuse_address(true)
        .map_err(|e| anyhow::anyhow!("sign socket SO_REUSEADDR: {e}"))?;
    socket
        .set_nonblocking(true)
        .map_err(|e| anyhow::anyhow!("sign socket non-blocking: {e}"))?;
    socket
        .bind(&addr.into())
        .map_err(|e| anyhow::anyhow!("sign bind 127.0.0.1:{port}: {e}"))?;
    socket
        .listen(garden_common::constants::server::TCP_BACKLOG)
        .map_err(|e| anyhow::anyhow!("sign listen 127.0.0.1:{port}: {e}"))?;

    let std_listener: std::net::TcpListener = socket.into();
    tokio::net::TcpListener::from_std(std_listener)
        .map_err(|e| anyhow::anyhow!("sign tokio convert: {e}"))
}
