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
    let socket = bind_ear(port, group).await?;

    // The ask: one request, spoken to the room (or localhost in tests).
    let req = DiscoveryRequest::for_moss(requester);
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
        r.stone_id.clone().unwrap_or_else(|| r.stone_name.clone())
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
    seen.sort_by(|a, b| a.stone_name.cmp(&b.stone_name));
    Ok(seen)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
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

        tokio::time::sleep(Duration::from_millis(100)).await;
        let found = ask_the_room(ear_port, None, Duration::from_secs(3), "probe-test")
            .await
            .unwrap();
        responder.await.unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].stone_name, "stone-echo");
        assert_eq!(found[0].stone_id.as_deref(), Some("id-loopback"));
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
}
