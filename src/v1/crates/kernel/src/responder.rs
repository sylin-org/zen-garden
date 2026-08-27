//! The tell half of ask/tell: answer `discovery_request` for the room.
//!
//! The PoC's moss answered via the same multicast transport (not unicast),
//! so every stone hears every answer and late joiners benefit too. v1
//! matches. Responses are built from the chirp source — one identity,
//! spoken consistently everywhere.

use crate::announce::ChirpSource;
use crate::dispatch::Dispatcher;
use crate::ingress::Ingested;
use garden_contract::consts::announcement;
use garden_contract::discovery::DiscoveryResponse;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;

/// Claim `discovery_request` and answer until cancelled.
pub fn claim(
    dispatcher: &Dispatcher,
    socket: Arc<UdpSocket>,
    group: Ipv4Addr,
    port: u16,
    source: Arc<dyn ChirpSource>,
    token: CancellationToken,
) {
    let mut requests = dispatcher.claim(announcement::DISCOVERY_REQUEST);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = token.cancelled() => return,
                msg = requests.recv() => match msg {
                    Some(m) => answer(&socket, group, port, source.as_ref(), m).await,
                    None => return,
                },
            }
        }
    });
}

async fn answer(
    socket: &UdpSocket,
    group: Ipv4Addr,
    port: u16,
    source: &dyn ChirpSource,
    msg: Ingested,
) {
    // Any ask gets our card. The rich flag rides the request but the
    // inventory attachment lands in slice 3 — anchors speak for now.
    let body = source.body();
    let response = DiscoveryResponse {
        stone: body.stone.clone(),
        lantern_endpoint: None,
        services: None,
    };
    let ann = garden_contract::wire::Announcement::new(
        announcement::DISCOVERY_RESPONSE,
        match serde_json::to_value(&response) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "response encode failed");
                return;
            }
        },
    );
    let bytes = match serde_json::to_vec(&ann) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "response serialize failed");
            return;
        }
    };
    if let Err(e) = socket.send_to(&bytes, std::net::SocketAddr::from((group, port))).await {
        tracing::warn!(error = %e, "discovery response send failed");
    } else {
        tracing::debug!(to = %msg.source, "answered discovery request");
    }
}
