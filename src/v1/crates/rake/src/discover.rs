//! Rake's ear: ask the room who is here, collect answers for a window.
//!
//! This is the client half of ask/tell (the moss side lives in
//! `garden_kernel::responder`). It is deliberately *transient* — rake is
//! not a garden member, it walks through: bind, join the room's group,
//! one request, listen until the deadline, leave. Envelope and body types
//! come from `garden_contract`; nothing here redefines the wire.

use garden_contract::consts::announcement;
use garden_contract::discovery::{DiscoveryRequest, DiscoveryResponse};
use garden_contract::wire::Announcement;
use serde::Serialize;
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

/// One stone seen in the garden.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Sighting {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stone_id: Option<String>,
    pub stone_name: String,
    /// Address of the stone's HTTP surface.
    pub ip: IpAddr,
    pub http_port: u16,
    pub moss_version: String,
}

/// Insert unless this stone (by id, else by name) is already sighted.
/// Returns false on duplicate. Pure so tests can pin dedup behavior.
fn record(sightings: &mut Vec<Sighting>, s: Sighting) -> bool {
    let key = s.stone_id.clone().unwrap_or_else(|| s.stone_name.clone());
    let dup = sightings.iter().any(|seen| {
        seen.stone_id.clone().unwrap_or_else(|| seen.stone_name.clone()) == key
    });
    if dup { false } else { sightings.push(s); true }
}

/// Bind rake's ear on the room's discovery port. Same-host stones share
/// that port with SO_REUSEADDR (same mechanics as moss ingress).
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
        for ip in garden_kernel::ingress::eligible_interfaces() {
            if let IpAddr::V4(v4) = ip {
                let _ = socket.join_multicast_v4(g, v4);
            }
        }
        socket.set_multicast_loop_v4(true)?;
    }
    Ok(socket)
}

/// Ask the room and gather answers until `timeout` elapses. Pass
/// `group: None` to skip multicast joins (loopback tests speak unicast).
pub async fn ask_the_room(
    port: u16,
    group: Option<Ipv4Addr>,
    timeout: Duration,
) -> std::io::Result<Vec<Sighting>> {
    let socket = bind_ear(port, group).await?;

    // The ask: one request, spoken to the room (or to localhost when no
    // group is given — the loopback-test shape).
    let req = DiscoveryRequest::for_moss("rake");
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

    // The listen: every answer until the deadline, deduped by stone.
    let mut sightings = Vec::new();
    let mut seen_msgs: HashSet<uuid::Uuid> = HashSet::new();
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
                if let Some(id) = ann.msg_id && !seen_msgs.insert(id) {
                    continue;
                }
                let Ok(resp) = serde_json::from_value::<DiscoveryResponse>(ann.data) else {
                    continue;
                };
                record(
                    &mut sightings,
                    Sighting {
                        stone_id: resp.stone_id,
                        stone_name: resp.stone_name,
                        ip: resp.address.ip,
                        http_port: resp.address.port,
                        moss_version: resp.moss_version,
                    },
                );
            }
        }
    }
    sightings.sort_by(|a, b| a.stone_name.cmp(&b.stone_name));
    Ok(sightings)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn sighting(name: &str) -> Sighting {
        Sighting {
            stone_id: Some(format!("id-{name}")),
            stone_name: name.into(),
            ip: IpAddr::from(Ipv4Addr::LOCALHOST),
            http_port: 7285,
            moss_version: "0.1.0".into(),
        }
    }

    #[test]
    fn record_dedups_by_stone_identity() {
        let mut seen = Vec::new();
        assert!(record(&mut seen, sighting("alpha")));
        assert!(!record(&mut seen, sighting("alpha")), "same stone twice");
        assert!(record(&mut seen, sighting("beta")));
        assert_eq!(seen.len(), 2);
    }

    /// End-to-end over loopback unicast: a stone speaks first, rake hears.
    /// (The responder does not wait for the ask: with two sockets sharing
    /// one port, Windows delivers unicast to an arbitrary one — the live
    /// path avoids this by using multicast, where every member hears all.)
    #[tokio::test]
    async fn hears_a_discovery_response_on_loopback() {
        // Rake's ear on a free port.
        let ear = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let ear_port = ear.local_addr().unwrap().port();
        drop(ear);

        // The stone: speaks one response straight at the ear's port.
        let responder = tokio::spawn(async move {
            let sock = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
            let resp = DiscoveryResponse {
                stone_id: Some("id-loopback".into()),
                stone_name: "stone-echo".into(),
                address: garden_contract::chirp::PeerAddress {
                    ip: IpAddr::from(Ipv4Addr::LOCALHOST),
                    port: 7285,
                    tls_port: None,
                },
                moss_version: "0.1.0".into(),
                lantern_endpoint: None,
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

        // Give the responder a beat to bind, then walk the garden.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let found = ask_the_room(ear_port, None, Duration::from_secs(3))
            .await
            .unwrap();
        responder.await.unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].stone_name, "stone-echo");
        assert_eq!(found[0].stone_id.as_deref(), Some("id-loopback"));
        assert_eq!(found[0].http_port, 7285);
    }
}
