//! Typed configuration with env twins (CODE-RULES R3.7).
//!
//! Defaults are the v1 topology (contract::consts — block 7284–7299): this
//! generation owns its room. The PoC's garden is a legacy reference, never
//! a default; deliberate overrides (`--discovery-port`, `--mcast-group`,
//! env twins) exist for experiments, not for coexistence.

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

impl Default for DiscoveryConfig {
    /// The v1 topology — this generation's declared room.
    fn default() -> Self {
        Self {
            port: consts::DISCOVERY_PORT_V1,
            group: consts::MULTICAST_GROUP_V1,
            heartbeat_secs: consts::HEARTBEAT_SECS,
            offline_threshold_secs: consts::OFFLINE_THRESHOLD_SECS,
            dedup_ttl_secs: consts::DEDUP_TTL_SECS,
        }
    }
}

impl DiscoveryConfig {
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

/// HTTP surface config. Port 7285 sits inside the declared v1 block
/// (contract::consts registry); it deliberately never shared a number with
/// the PoC's moss API so both generations can run side-by-side untouched.
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
