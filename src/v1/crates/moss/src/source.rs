//! The daemon's voice: what this stone says when it chirps.
//!
//! M0 speaks a static body — identity, address, thriving, no offerings yet.
//! When services exist, a source that bumps its version watch on change
//! replaces this; the announcer already listens for it (L18).

use garden_contract::chirp::{ChirpFrame, Network, PeerAddress, Presence, Reception, Stone};
use garden_kernel::announce::ChirpSource;
use std::net::IpAddr;
use std::sync::Arc;

/// A chirp source whose body does not change. The version watch never fires.
pub struct StaticChirpSource {
    body: ChirpFrame,
    version_tx: tokio::sync::watch::Sender<u64>,
}

impl StaticChirpSource {
    pub fn new(body: ChirpFrame) -> Arc<Self> {
        let (version_tx, _) = tokio::sync::watch::channel(0);
        Arc::new(Self { body, version_tx })
    }
}

impl ChirpSource for StaticChirpSource {
    fn body(&self) -> ChirpFrame {
        self.body.clone()
    }

    fn version(&self) -> tokio::sync::watch::Receiver<u64> {
        self.version_tx.subscribe()
    }
}

/// Best-effort LAN address for the chirp: first eligible non-loopback IPv4,
/// loopback as honest fallback (a lone stone on a laptop still speaks).
pub fn local_lan_ip() -> IpAddr {
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        let lan = ifaces.into_iter().find_map(|iface| match iface.ip() {
            IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_link_local() => Some(IpAddr::V4(v4)),
            _ => None,
        });
        if let Some(ip) = lan {
            return ip;
        }
    }
    IpAddr::from(std::net::Ipv4Addr::LOCALHOST)
}

/// Build the M0 frame: alive, thriving, offering nothing yet. `boot_id`
/// distinguishes this boot's frames from a restart's; the announcer stamps
/// `proto`/`seq` (meta is its section to fill).
pub fn static_body(
    stone_id: String,
    stone_name: String,
    boot_id: String,
    http_port: u16,
    moss_version: String,
) -> ChirpFrame {
    use garden_glossary::{health, presence};
    let now = chrono::Utc::now();
    ChirpFrame {
        stone: Stone {
            id: stone_id,
            name: stone_name,
            moss: garden_contract::chirp::Moss { version: moss_version },
            network: Network {
                address: PeerAddress { ip: local_lan_ip(), port: http_port, tls_port: None },
                mac: None,
            },
        },
        presence: Presence {
            health: health::THRIVING.into(),
            status: presence::ONLINE.into(),
        },
        services: Default::default(),
        meta: garden_contract::chirp::FrameMeta {
            proto: None,
            boot_id: Some(boot_id),
            seq: None,
        },
        received: Reception { discovered_at: now, last_seen: now },
    }
}
