//! The announcer: sings on change and boot, chirps lean as heartbeats
//! (L18; ADR-0004 A2.2 — boots and changes sing, heartbeats chirp).
//!
//! Change-driven with a debounce floor, heartbeat as the liveness ceiling —
//! the PoC's `announce_if_changed` machinery, actually implemented this
//! time. The source is a port: the daemon decides what a chirp says.

use garden_contract::chirp::ChirpFrame;
use garden_contract::consts;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

/// Where chirp content comes from. The kernel never invents identity.
pub trait ChirpSource: Send + Sync {
    /// The LEAN body to speak right now (heartbeats): anchors plus
    /// rev-only inventory blocks (A2.1) — presence must not amortize
    /// inventory.
    fn body(&self) -> ChirpFrame;
    /// The full-voice inventory blocks (A2.2): (domain, block) pairs in
    /// framer priority order — songs and rich replies speak these.
    /// Empty means this source has no inventory to sing.
    fn song_blocks(&self) -> Vec<(String, serde_json::Value)>;
    /// Bumps whenever the body's meaning changes (services, health, address).
    fn version(&self) -> tokio::sync::watch::Receiver<u64>;
}

/// Speak `body` as a `STONE_CHIRP` to the multicast group. Meta sections
/// are the ANNOUNCER's to stamp: schema marker, liveness, ordering.
pub async fn send_chirp(
    socket: &UdpSocket,
    group: std::net::Ipv4Addr,
    port: u16,
    mut body: ChirpFrame,
    seq: u64,
) -> std::io::Result<()> {
    body.presence.status = garden_glossary::presence::ONLINE.into();
    body.meta.seq = Some(seq);
    body.meta.proto = Some(consts::PROTO_V1.into());
    body.received.last_seen = chrono::Utc::now();
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
    body: ChirpFrame,
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

/// Speak song frames (the framer's output) as `STONE_SONG` to the group.
/// Per-frame meta are the ANNOUNCER's to stamp: schema marker, liveness —
/// seq and part markers belong to the framer and are left alone.
pub async fn send_song(
    socket: &UdpSocket,
    group: std::net::Ipv4Addr,
    port: u16,
    mut frames: Vec<ChirpFrame>,
) -> std::io::Result<()> {
    for frame in &mut frames {
        frame.presence.status = garden_glossary::presence::ONLINE.into();
        frame.meta.proto = Some(consts::PROTO_V1.into());
        frame.received.last_seen = chrono::Utc::now();
        let ann = garden_contract::wire::Announcement::new(
            consts::announcement::STONE_SONG,
            serde_json::to_value(&*frame)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        );
        let bytes = serde_json::to_vec(&ann)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        socket.send_to(&bytes, std::net::SocketAddr::from((group, port))).await?;
    }
    Ok(())
}

/// Speak the full voice: quantize the source's blocks into song frames
/// (A2.3). A source with nothing to sing chirps instead — presence always
/// speaks, silence is not a song.
async fn speak_full(
    socket: &UdpSocket,
    group: std::net::Ipv4Addr,
    port: u16,
    source: &dyn ChirpSource,
    seq: u64,
) {
    let base = source.body();
    let blocks = source.song_blocks();
    if blocks.is_empty() {
        if let Err(e) = send_chirp(socket, group, port, base, seq).await {
            tracing::warn!(error = %e, "voiceless boot chirp failed");
        }
        return;
    }
    let frames = garden_contract::song::frame_song(&base, blocks, seq);
    if let Err(e) = send_song(socket, group, port, frames).await {
        tracing::warn!(error = %e, "song send failed");
    }
}

/// Ask the room who is here (the tell half of boot). The PoC's moss did
/// this at startup so a newcomer converges in one round-trip instead of
/// waiting out a heartbeat. The boot ask is RICH (ADR-0004 §1): the
/// newcomer's opening question is "who are you guys, and what do you
/// have?" — one exchange seeds the whole map.
pub async fn send_discovery_request(
    socket: &UdpSocket,
    group: std::net::Ipv4Addr,
    port: u16,
    requester: &str,
) -> std::io::Result<()> {
    let req = garden_contract::discovery::DiscoveryRequest::for_moss_rich(requester);
    let ann = garden_contract::wire::Announcement::new(
        consts::announcement::DISCOVERY_REQUEST,
        serde_json::to_value(&req).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
    );
    let bytes = serde_json::to_vec(&ann)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    socket.send_to(&bytes, std::net::SocketAddr::from((group, port))).await?;
    Ok(())
}

/// Full-voice re-assertion period, in heartbeats (ADR-0015 law 6).
pub const SONG_EVERY_HEARTBEATS: u64 = 10;

/// Drive the announcer until cancelled: heartbeat on the clock, chirp on
/// change (debounced to the heartbeat floor so a flap can't flood).
pub async fn run(
    socket: Arc<UdpSocket>,
    group: std::net::Ipv4Addr,
    port: u16,
    source: Arc<dyn ChirpSource>,
    requester: String,
    token: CancellationToken,
) -> u64 {
    let mut seq: u64 = 0;
    let mut heartbeat =
        tokio::time::interval(std::time::Duration::from_secs(consts::HEARTBEAT_SECS));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut version = source.version();
    let debounce = std::time::Duration::from_secs(consts::HEARTBEAT_SECS);
    let mut last_change_chirp = tokio::time::Instant::now() - debounce;

    // Speak immediately on boot, in full voice (A2.2: boots sing); the
    // garden should learn of us — and of what we grow — at once.
    seq += 1;
    speak_full(&socket, group, port, source.as_ref(), seq).await;
    // Consume interval's immediate first tick so boot isn't a double-speak.
    heartbeat.tick().await;
    let mut beat: u64 = 0;
    // Then ask who else is here — the room answers in one round-trip.
    if let Err(e) = send_discovery_request(&socket, group, port, &requester).await {
        tracing::warn!(error = %e, "boot discovery request failed");
    }

    loop {
        tokio::select! {
            _ = token.cancelled() => return seq,
            _ = version.changed() => {
                // Change-driven song, debounced (L18: change is the event).
                let since = tokio::time::Instant::now().duration_since(last_change_chirp);
                if since < debounce {
                    tokio::time::sleep(debounce - since).await;
                }
                last_change_chirp = tokio::time::Instant::now();
                seq += 1;
                speak_full(&socket, group, port, source.as_ref(), seq).await;
            }
            _ = heartbeat.tick() => {
                beat += 1;
                seq += 1;
                // Every SONG_EVERY_HEARTBEATS beats the stone sings full
                // voice: gossip must re-assert, because the room converges
                // from loss (ADR-0015 law 6) — a stone whose boot-time
                // answers were lost is re-taught without waiting for a
                // change. The rest stay LEAN (ADR-0004 §1: presence must
                // not amortize inventory).
                if beat % SONG_EVERY_HEARTBEATS == 0 {
                    speak_full(&socket, group, port, source.as_ref(), seq).await;
                } else if let Err(e) = send_chirp(&socket, group, port, source.body(), seq).await {
                    tracing::warn!(error = %e, "heartbeat chirp failed");
                }
            }
        }
    }
}
