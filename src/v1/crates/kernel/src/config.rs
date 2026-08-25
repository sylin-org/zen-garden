//! Typed configuration with env twins (CODE-RULES R3.7).
//!
//! Defaults are the PoC wire values so v1 is fleet-compatible out of the
//! box; `--isolate` (or the env twins) points experiments at the isolated
//! group/port so production gardens stay unbothered (DEBT D1).

use garden_contract::consts;
use std::net::Ipv4Addr;

/// Everything the kernel needs to speak on the wire.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// UDP port for discovery traffic.
    pub port: u16,
    /// Multicast group chirps join and speak to.
    pub group: Ipv4Addr,
    /// Heartbeat period; also the change-debounce floor.
    pub heartbeat_secs: u64,
    /// Silence longer than this marks a peer offline.
    pub offline_threshold_secs: u64,
    /// Received `msg_id` memory window.
    pub dedup_ttl_secs: u64,
}

impl DiscoveryConfig {
    /// Fleet-compatible defaults (PoC wire values).
    pub fn fleet_default() -> Self {
        Self {
            port: consts::DISCOVERY_PORT,
            group: consts::MULTICAST_GROUP,
            heartbeat_secs: consts::HEARTBEAT_SECS,
            offline_threshold_secs: consts::OFFLINE_THRESHOLD_SECS,
            dedup_ttl_secs: consts::DEDUP_TTL_SECS,
        }
    }

    /// Isolated experiment values (DEBT D1) — same protocol, private room.
    pub fn isolated() -> Self {
        Self {
            port: consts::DISCOVERY_PORT_ISOLATED,
            group: consts::MULTICAST_GROUP_ISOLATED,
            ..Self::fleet_default()
        }
    }

    /// Env twins: `GARDEN_V1_DISCOVERY_PORT`, `GARDEN_V1_MCAST_GROUP`.
    /// Environment is for deployment concerns only; absent vars keep defaults.
    pub fn from_env(mut self) -> Self {
        if let Ok(v) = std::env::var("GARDEN_V1_DISCOVERY_PORT")
            && let Ok(p) = v.parse()
        {
            self.port = p;
        }
        if let Ok(v) = std::env::var("GARDEN_V1_MCAST_GROUP")
            && let Ok(g) = v.parse()
        {
            self.group = g;
        }
        self
    }
}

/// HTTP surface config. Default is deliberately NOT the PoC's 7185: a v1
/// proto must never collide with a live PoC moss on the same machine.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub port: u16,
}

impl HttpConfig {
    pub const DEFAULT_PORT: u16 = 7285;

    pub fn from_env() -> Self {
        let port = std::env::var("GARDEN_V1_HTTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(Self::DEFAULT_PORT);
        Self { port }
    }
}
