//! Single point of ingestion (CODE-RULES R2.9).
//!
//! Owns the UDP socket(s). Every datagram — multicast or unicast — is
//! parsed, deduped, and handed to the dispatcher exactly once. Parse
//! failures are counted and dropped, never propagated as panics.

use crate::config::DiscoveryConfig;
use garden_contract::consts;
use garden_contract::wire::Announcement;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// A datagram that survived parsing and dedup, with provenance.
#[derive(Debug, Clone)]
pub struct Ingested {
    pub announcement: Announcement,
    pub source: SocketAddr,
    pub received_at: chrono::DateTime<chrono::Utc>,
}

/// Ingest-time rejection reasons, counted for posture (B3).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IngestStats {
    pub parsed: u64,
    pub bad_json: u64,
    pub deduped: u64,
}

/// The receive-side dedup window (PoC parity: 5s per `msg_id`).
#[derive(Debug, Default)]
pub struct DedupCache {
    seen: HashMap<Uuid, Instant>,
    ttl: Duration,
}

impl DedupCache {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            seen: HashMap::new(),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// True if this `msg_id` is novel inside the window; records it either
    /// way. Expired entries are evicted lazily — no sweeper task (L18).
    pub fn admit(&mut self, id: Uuid) -> bool {
        let now = Instant::now();
        self.seen.retain(|_, seen| now.duration_since(*seen) < self.ttl);
        self.seen.insert(id, now).is_none()
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

/// The bound listener. `run` drives the receive loop until `token` cancels.
pub struct Ingress {
    socket: Arc<UdpSocket>,
    group: Option<Ipv4Addr>,
    dedup_ttl_secs: u64,
}

impl Ingress {
    /// Bind the discovery socket. Joins the multicast group on every
    /// eligible IPv4 interface when `cfg` speaks multicast; loopback-only
    /// tests pass `group: None` and speak unicast.
    pub async fn bind(cfg: &DiscoveryConfig, group: Option<Ipv4Addr>) -> std::io::Result<Self> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, cfg.port)).await?;
        socket.set_broadcast(true)?;
        let group = match group {
            Some(g) => {
                for ip in eligible_interfaces() {
                    if let IpAddr::V4(v4) = ip {
                        // Join per interface; a refusal on one NIC must not
                        // silence the others (PoC lesson: multi-homed hosts).
                        let _ = socket.join_multicast_v4(&g, v4);
                    }
                }
                socket.set_multicast_loop_v4(true)?;
                Some(g)
            }
            None => None,
        };
        Ok(Self {
            socket: Arc::new(socket),
            group,
            dedup_ttl_secs: cfg.dedup_ttl_secs,
        })
    }

    /// The socket, for announcers that speak through the same port number.
    pub fn socket(&self) -> Arc<UdpSocket> {
        self.socket.clone()
    }

    /// Drive ingestion until cancelled. Emits every accepted datagram to
    /// `dispatch`; the dispatcher's bounded queue applies backpressure.
    pub async fn run(
        &self,
        token: CancellationToken,
        dispatch: mpsc::Sender<Ingested>,
    ) -> IngestStats {
        let mut stats = IngestStats::default();
        let mut dedup = DedupCache::new(self.dedup_ttl_secs);
        let mut buf = vec![0u8; 65_535];

        loop {
            tokio::select! {
                _ = token.cancelled() => return stats,
                recv = self.socket.recv_from(&mut buf) => {
                    let Ok((n, source)) = recv else { continue };
                    let Ok(ann) = serde_json::from_slice::<Announcement>(&buf[..n]) else {
                        stats.bad_json += 1;
                        tracing::debug!(%source, bytes = n, "ingest: undecodable datagram");
                        continue;
                    };
                    // v1 always dedups; the PoC's no-msg_id bypass is a quirk
                    // we do not inherit (contract::wire doc).
                    let novel = match ann.msg_id {
                        Some(id) => dedup.admit(id),
                        None => true,
                    };
                    if !novel {
                        stats.deduped += 1;
                        continue;
                    }
                    stats.parsed += 1;
                    let ingested = Ingested {
                        announcement: ann,
                        source,
                        received_at: chrono::Utc::now(),
                    };
                    if dispatch.send(ingested).await.is_err() {
                        // Dispatcher gone: shutdown is underway.
                        return stats;
                    }
                }
            }
        }
    }
}

/// Eligible IPv4 interfaces for multicast joins: no loopback, no
/// link-local, no known virtual adapters by name (PoC COMM-0003 heuristic,
/// minimal form; the MAC-OUI table was dead code there and is not missed).
fn eligible_interfaces() -> Vec<IpAddr> {
    const VIRTUAL: [&str; 9] = [
        "veth", "virbr", "docker", "br-", "vmnet", "vboxnet", "wsl", "hyper-v", "loopback",
    ];
    let mut out = Vec::new();
    let Ok(ifaces) = if_addrs::get_if_addrs() else {
        return out;
    };
    for iface in ifaces {
        let ip = iface.ip();
        let IpAddr::V4(v4) = ip else { continue };
        if v4.is_loopback() || v4.is_link_local() {
            continue;
        }
        let name = iface.name.to_lowercase();
        if VIRTUAL.iter().any(|v| name.contains(v)) {
            continue;
        }
        out.push(ip);
    }
    out
}

/// Convenience: a `DedupCache` at the fleet default window.
pub fn fleet_dedup() -> DedupCache {
    DedupCache::new(consts::DEDUP_TTL_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_admits_novel_and_rejects_repeat_within_window() {
        let mut cache = DedupCache::new(5);
        let id = Uuid::now_v7();
        assert!(cache.admit(id), "first sight is novel");
        assert!(!cache.admit(id), "repeat inside window is deduped");
        let other = Uuid::now_v7();
        assert!(cache.admit(other));
        assert_eq!(cache.len(), 2);
    }
}
