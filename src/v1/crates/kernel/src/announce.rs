//! The announcer: speaks chirps on change, heartbeats otherwise (L18).
//!
//! Change-driven with a debounce floor, heartbeat as the liveness ceiling —
//! the PoC's `announce_if_changed` machinery, actually implemented this
//! time. The source is a port: the daemon decides what a chirp says.

use garden_contract::chirp::ChirpBody;
use garden_contract::consts;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

/// Where chirp content comes from. The kernel never invents identity.
pub trait ChirpSource: Send + Sync {
    /// The body to speak right now.
    fn body(&self) -> ChirpBody;
    /// Bumps whenever the body's meaning changes (services, health, address).
    fn version(&self) -> tokio::sync::watch::Receiver<u64>;
}

/// Speak `body` to the fleet: multicast to the group, plus unicast copies
/// to any explicit `peers` (tests use loopback).
pub async fn send_chirp(
    socket: &UdpSocket,
    group: std::net::Ipv4Addr,
    port: u16,
    mut body: ChirpBody,
    seq: u64,
) -> std::io::Result<()> {
    body.status = garden_glossary::presence::ONLINE.into();
    body.last_seen = chrono::Utc::now();
    body.seq = Some(seq);
    body.proto = Some(consts::PROTO_V1.into());
    let ann = garden_contract::wire::Announcement::new(
        consts::announcement::STONE_CHIRP,
        serde_json::to_value(&body).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
    );
    let bytes = serde_json::to_vec(&ann)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let group_addr = std::net::SocketAddr::from((group, port));
    socket.send_to(&bytes, group_addr).await?;
    Ok(())
}

/// Speak a goodbye — three quick copies, debounce-free (PoC parity).
pub async fn send_goodbye(
    socket: &UdpSocket,
    group: std::net::Ipv4Addr,
    port: u16,
    body: ChirpBody,
) -> std::io::Result<()> {
    let ann = garden_contract::wire::Announcement::new(
        consts::announcement::STONE_GOODBYE,
        serde_json::to_value(&body).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
    );
    let bytes = serde_json::to_vec(&ann)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let group_addr = std::net::SocketAddr::from((group, port));
    for _ in 0..3 {
        socket.send_to(&bytes, group_addr).await?;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Ok(())
}

/// Drive the announcer until cancelled: heartbeat on the clock, chirp on
/// change (debounced to the heartbeat floor so a flap can't flood).
pub async fn run(
    socket: Arc<UdpSocket>,
    group: std::net::Ipv4Addr,
    port: u16,
    source: Arc<dyn ChirpSource>,
    token: CancellationToken,
) -> u64 {
    let mut seq: u64 = 0;
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(
        source.body().seq.map(|_| consts::HEARTBEAT_SECS).unwrap_or(consts::HEARTBEAT_SECS),
    ));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut version = source.version();
    let debounce = std::time::Duration::from_secs(consts::HEARTBEAT_SECS);
    let mut last_change_chirp = tokio::time::Instant::now() - debounce;

    // Speak immediately on boot: the garden should learn of us at once.
    seq += 1;
    let body = source.body();
    if let Err(e) = send_chirp(&socket, group, port, body, seq).await {
        tracing::warn!(error = %e, "boot chirp failed");
    }

    loop {
        tokio::select! {
            _ = token.cancelled() => return seq,
            _ = version.changed() => {
                // Change-driven chirp, debounced (L18: change is the event).
                let since = tokio::time::Instant::now().duration_since(last_change_chirp);
                if since < debounce {
                    tokio::time::sleep(debounce - since).await;
                }
                last_change_chirp = tokio::time::Instant::now();
                seq += 1;
                let body = source.body();
                if let Err(e) = send_chirp(&socket, group, port, body, seq).await {
                    tracing::warn!(error = %e, "change chirp failed");
                }
            }
            _ = heartbeat.tick() => {
                seq += 1;
                let body = source.body();
                if let Err(e) = send_chirp(&socket, group, port, body, seq).await {
                    tracing::warn!(error = %e, "heartbeat chirp failed");
                }
            }
        }
    }
}
