//! The probe: a transient ear on the room. Ask who is here, listen out
//! the window, leave.
//!
//! One implementation serves every speaker (R1.2): rake attaches through
//! it, moss checks name collisions through it. Unlike the resident
//! ingress/dispatch machinery, this is visitor machinery — bind, join,
//! one request, collect answers until the deadline.

use garden_contract::consts::announcement;
use garden_contract::discovery::{DiscoveryRequest, DiscoveryResponse};
use garden_contract::wire::Announcement;
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

/// Bind a transient ear on `port`. Same-host stones share discovery ports
/// via SO_REUSEADDR (same mechanics as the resident ingress).
async fn bind_ear(port: u16, group: Option<Ipv4Addr>) -> std::io::Result<UdpSocket> {
    let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
    let sock = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    sock.set_reuse_address(true)?;
    sock.bind(&addr.into())?;
    sock.set_nonblocking(true)?;
    let socket = UdpSocket::from_std(sock.into())?;
    if let Some(g) = group {
        for ip in crate::ingress::eligible_interfaces() {
            if let IpAddr::V4(v4) = ip {
                let _ = socket.join_multicast_v4(g, v4);
            }
        }
        socket.set_multicast_loop_v4(true)?;
    }
    Ok(socket)
}

/// Ask the room who is here and gather answers until `timeout` elapses.
/// Deduped by stone identity; sorted by name. Pass `group: None` to skip
/// multicast joins — loopback tests then speak unicast.
pub async fn ask_the_room(
    port: u16,
    group: Option<Ipv4Addr>,
    timeout: Duration,
    requester: &str,
) -> std::io::Result<Vec<DiscoveryResponse>> {
    ask_with(DiscoveryRequest::for_moss(requester), port, group, timeout).await
}

/// The same walk, rich form (ADR-0004 §1): answers carry the
/// respondents' service inventories, so the caller seeds its cache in
/// one exchange. Costs the room a fat reply per stone — ask rich only
/// when inventory is the point.
pub async fn ask_the_room_rich(
    port: u16,
    group: Option<Ipv4Addr>,
    timeout: Duration,
    requester: &str,
) -> std::io::Result<Vec<DiscoveryResponse>> {
    ask_with(DiscoveryRequest::for_moss_rich(requester), port, group, timeout).await
}

/// One request, spoken to the room (or localhost in tests); every answer
/// until the deadline.
async fn ask_with(
    req: DiscoveryRequest,
    port: u16,
    group: Option<Ipv4Addr>,
    timeout: Duration,
) -> std::io::Result<Vec<DiscoveryResponse>> {
    let socket = bind_ear(port, group).await?;
    let ann = Announcement::new(
        announcement::DISCOVERY_REQUEST,
        serde_json::to_value(&req)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
    );
    let bytes = serde_json::to_vec(&ann)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let target = match group {
        Some(g) => SocketAddr::from((g, port)),
        None => SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
    };
    socket.send_to(&bytes, target).await?;

    // The listen: every answer until the deadline.
    let mut seen: Vec<DiscoveryResponse> = Vec::new();
    let key = |r: &DiscoveryResponse| {
        if r.stone.id.is_empty() {
            r.stone.name.clone()
        } else {
            r.stone.id.clone()
        }
    };
    let mut msg_ids: HashSet<uuid::Uuid> = HashSet::new();
    let deadline = Instant::now() + timeout;
    let mut buf = vec![0u8; 65_535];
    while Instant::now() < deadline {
        let wait = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(wait, socket.recv_from(&mut buf)).await {
            Err(_) => break, // deadline reached
            Ok(Err(_)) => continue,
            Ok(Ok((n, _source))) => {
                let Ok(ann) = serde_json::from_slice::<Announcement>(&buf[..n]) else {
                    continue;
                };
                if ann.kind != announcement::DISCOVERY_RESPONSE {
                    continue;
                }
                if let Some(id) = ann.msg_id && !msg_ids.insert(id) {
                    continue;
                }
                let Ok(resp) = serde_json::from_value::<DiscoveryResponse>(ann.data) else {
                    continue;
                };
                let k = key(&resp);
                if !seen.iter().any(|s| key(s) == k) {
                    seen.push(resp);
                }
            }
        }
    }
    seen.sort_by(|a, b| a.stone.name.cmp(&b.stone.name));
    Ok(seen)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    /// End-to-end over loopback unicast: a stone speaks first, the probe
    /// hears. (Two sockets sharing one port make unicast delivery arbitrary
    /// on Windows — so the responder does not wait for the ask.)
    #[tokio::test]
    async fn hears_a_discovery_response_on_loopback() {
        let ear = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let ear_port = ear.local_addr().unwrap().port();
        drop(ear);

        let responder = tokio::spawn(async move {
            let sock = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
            let resp = DiscoveryResponse {
                stone: garden_contract::chirp::Stone {
                    id: "id-loopback".into(),
                    name: "stone-echo".into(),
                    moss: garden_contract::chirp::Moss { version: "0.1.0".into() },
                    network: garden_contract::chirp::Network {
                        address: garden_contract::chirp::PeerAddress {
                            ip: IpAddr::from(Ipv4Addr::LOCALHOST),
                            port: 7285,
                            tls_port: None,
                        },
                        mac: None,
                    },
                },
                lantern_endpoint: None,
                inventory: Default::default(),
            };
            let ann = Announcement::new(
                announcement::DISCOVERY_RESPONSE,
                serde_json::to_value(&resp).unwrap(),
            );
            let bytes = serde_json::to_vec(&ann).unwrap();
            let dst = SocketAddr::from((Ipv4Addr::LOCALHOST, ear_port));
            // A few copies: first may land before the ear is listening.
            for _ in 0..5 {
                sock.send_to(&bytes, dst).await.unwrap();
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });

        tokio::time::sleep(Duration::from_millis(100)).await;
        let found = ask_the_room(ear_port, None, Duration::from_secs(3), "probe-test")
            .await
            .unwrap();
        responder.await.unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].stone.name, "stone-echo");
        assert_eq!(found[0].stone.id, "id-loopback");
    }

    #[tokio::test]
    async fn quiet_room_yields_nothing() {
        let ear = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = ear.local_addr().unwrap().port();
        drop(ear);
        let found = ask_the_room(port, None, Duration::from_millis(200), "probe-test")
            .await
            .unwrap();
        assert!(found.is_empty());
    }

    /// The rich ask declares its depth on the wire: a listener captures
    /// the request datagram and reads `rich: true` (lean asks read false —
    /// `skip_serializing_if` keeps the field absent).
    #[tokio::test]
    async fn rich_ask_declares_itself_on_the_wire() {
        let capture = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = capture.local_addr().unwrap().port();

        let listener = tokio::spawn(async move {
            let mut buf = vec![0u8; 65_535];
            let (n, _) = tokio::time::timeout(
                Duration::from_secs(3),
                capture.recv_from(&mut buf),
            )
            .await
            .expect("ask must arrive")
            .expect("recv ok");
            let ann: Announcement = serde_json::from_slice(&buf[..n]).unwrap();
            serde_json::from_value::<DiscoveryRequest>(ann.data).unwrap()
        });

        let _ = ask_the_room_rich(port, None, Duration::from_millis(150), "probe-test")
            .await
            .unwrap();

        let req = listener.await.unwrap();
        assert!(req.rich, "the rich variant must speak its depth");
    }

    /// A rich response's inventory survives the probe's parse-and-dedup
    /// path intact (B1: the cache-feeding shape is the wire shape).
    #[tokio::test]
    async fn rich_answers_keep_their_inventory() {
        let ear = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let ear_port = ear.local_addr().unwrap().port();
        drop(ear);

        let responder = tokio::spawn(async move {
            let sock = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
            let mut resp = DiscoveryResponse {
                stone: garden_contract::chirp::Stone {
                    id: "id-rich".into(),
                    name: "stone-rich".into(),
                    moss: garden_contract::chirp::Moss { version: "0.1.0".into() },
                    network: garden_contract::chirp::Network {
                        address: garden_contract::chirp::PeerAddress {
                            ip: IpAddr::from(Ipv4Addr::LOCALHOST),
                            port: 7285,
                            tls_port: None,
                        },
                        mac: None,
                    },
                },
                lantern_endpoint: None,
                inventory: Default::default(),
            };
            resp.inventory = garden_contract::chirp::InventoryMap {
                services: Some(garden_contract::chirp::Inventory {
                    rev: Some(3),
                    total: None,
                    items: vec![garden_contract::chirp::ServiceEntry {
                        offering_id: "oid-1".into(),
                        name: "redis::default".into(),
                        stem: "redis".into(),
                        category: "data".into(),
                        state: garden_contract::chirp::ServiceState {
                            status: "running".into(),
                            role: None,
                        },
                        ports: Default::default(),
                capabilities: Default::default(),
                    }],
                }),
                ..Default::default()
            };
            let ann = Announcement::new(
                announcement::DISCOVERY_RESPONSE,
                serde_json::to_value(&resp).unwrap(),
            );
            let bytes = serde_json::to_vec(&ann).unwrap();
            let dst = SocketAddr::from((Ipv4Addr::LOCALHOST, ear_port));
            for _ in 0..5 {
                sock.send_to(&bytes, dst).await.unwrap();
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });

        tokio::time::sleep(Duration::from_millis(100)).await;
        let found = ask_the_room_rich(ear_port, None, Duration::from_secs(3), "probe-test")
            .await
            .unwrap();
        responder.await.unwrap();

        let inv = found[0]
            .inventory
            .services
            .as_ref()
            .expect("rich answer carries inventory");
        assert_eq!(inv.items[0].name, "redis::default");
        assert_eq!(inv.rev, Some(3));
    }
}
