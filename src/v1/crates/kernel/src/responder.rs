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
    // Every ask gets our card; a rich ask also gets our inventory
    // (ADR-0004 §1 — the reply is the third depth). The ask's own depth
    // decides: undecodable asks degrade to lean, never crash (R2.5).
    let rich = serde_json::from_value::<garden_contract::discovery::DiscoveryRequest>(
        msg.announcement.data,
    )
    .map(|req| req.rich)
    .unwrap_or(false);
    let body = source.body();
    let services = rich.then(|| body.inventory.services.clone()).flatten();
    let response = DiscoveryResponse {
        stone: body.stone.clone(),
        lantern_endpoint: None,
        services,
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use garden_contract::chirp::{
        ChirpFrame, Inventory, Moss, Network, PeerAddress, Presence, Reception, ServiceEntry,
        ServiceState, Stone,
    };
    use garden_contract::discovery::DiscoveryRequest;
    use std::net::{IpAddr, SocketAddr};
    use std::time::Duration;

    /// A source that speaks a fixed card and a one-offering inventory
    /// (R2.3: every port carries its test double).
    struct FixedSource;

    impl ChirpSource for FixedSource {
        fn body(&self) -> garden_contract::chirp::ChirpFrame {
            let now = chrono::Utc::now();
            ChirpFrame {
                stone: Stone {
                    id: "sid-answer".into(),
                    name: "stone-tells".into(),
                    moss: Moss { version: "1.0.0".into() },
                    network: Network {
                        address: PeerAddress {
                            ip: IpAddr::from(Ipv4Addr::LOCALHOST),
                            port: 7285,
                            tls_port: None,
                        },
                        mac: None,
                    },
                },
                presence: Presence {
                    health: garden_glossary::health::THRIVING.into(),
                    status: garden_glossary::presence::ONLINE.into(),
                },
                inventory: garden_contract::chirp::InventoryMap {
                    services: Some(Inventory {
                        rev: Some(4),
                        total: None,
                        items: vec![ServiceEntry {
                            offering_id: "oid-9".into(),
                            name: "memcached::default".into(),
                            stem: "memcached".into(),
                            category: "data".into(),
                            state: ServiceState { status: "running".into(), role: None },
                            ports: Default::default(),
                        }],
                    }),
                    ..Default::default()
                },
                meta: Default::default(),
                received: Reception { discovered_at: now, last_seen: now },
            }
        }

        fn version(&self) -> tokio::sync::watch::Receiver<u64> {
            tokio::sync::watch::channel(0).1
        }
    }

    /// Drive one ask through ingest→dispatch→responder and return the
    /// response the ear hears. `group` doubles as the unicast loopback
    /// target, so no multicast joins are involved.
    async fn answered_with_raw(data: serde_json::Value) -> DiscoveryResponse {
        let ear = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let ear_port = ear.local_addr().unwrap().port();

        let (dispatcher, handle) = Dispatcher::new(16);
        let token = CancellationToken::new();
        responder_socket_and_claim(
            &dispatcher,
            Ipv4Addr::LOCALHOST,
            ear_port,
            token.clone(),
        )
        .await;
        tokio::spawn(handle.run(token.clone()));

        dispatcher
            .ingest(Ingested {
                announcement: garden_contract::wire::Announcement::new(
                    announcement::DISCOVERY_REQUEST,
                    data,
                ),
                source: SocketAddr::from((Ipv4Addr::LOCALHOST, 55555)),
                received_at: chrono::Utc::now(),
            })
            .await;

        let mut buf = vec![0u8; 65_535];
        let (n, _) = tokio::time::timeout(Duration::from_secs(2), ear.recv_from(&mut buf))
            .await
            .expect("response must arrive")
            .expect("recv ok");
        token.cancel();
        let ann: garden_contract::wire::Announcement =
            serde_json::from_slice(&buf[..n]).unwrap();
        serde_json::from_value(ann.data).unwrap()
    }

    async fn answered_for(req: DiscoveryRequest) -> DiscoveryResponse {
        answered_with_raw(serde_json::to_value(&req).unwrap()).await
    }

    /// Bind a throwaway socket and hand the responder its claim.
    async fn responder_socket_and_claim(
        dispatcher: &Dispatcher,
        group: Ipv4Addr,
        port: u16,
        token: CancellationToken,
    ) {
        let socket = std::sync::Arc::new(
            tokio::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap(),
        );
        claim(dispatcher, socket, group, port, std::sync::Arc::new(FixedSource), token);
    }

    #[tokio::test]
    async fn rich_ask_gets_the_inventory() {
        let resp = answered_for(DiscoveryRequest::for_moss_rich("tester")).await;
        let inv = resp.services.expect("rich ask earns the inventory");
        assert_eq!(inv.rev, Some(4));
        assert_eq!(inv.items[0].name, "memcached::default");
        assert_eq!(resp.stone.name, "stone-tells");
    }

    #[tokio::test]
    async fn lean_ask_gets_the_card_only() {
        let resp = answered_for(DiscoveryRequest::for_moss("tester")).await;
        assert!(resp.services.is_none(), "lean asks must not pay fat replies");
        assert_eq!(resp.stone.id, "sid-answer");
    }

    /// An undecodable ask degrades to a lean answer — the card still
    /// helps the room (R2.5); the malformed ask is the sender's loss.
    #[tokio::test]
    async fn undecodable_ask_gets_the_card() {
        let resp = answered_with_raw(serde_json::json!({"discover": 42})).await;
        assert!(resp.services.is_none(), "no depth can be earned by garbage");
        assert_eq!(resp.stone.name, "stone-tells");
    }
}
