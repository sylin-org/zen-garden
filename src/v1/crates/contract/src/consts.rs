//! Discovery-domain constants — the wire's fixed points (R1.7).
//!
//! Values harvested from the PoC (branch `poc`); changing any of these is a
//! breaking change to R0.5 and must fail fixture tests before it merges.

/// UDP port every garden voice speaks discovery on.
pub const DISCOVERY_PORT: u16 = 7184;
/// IPv4 multicast group chirps are spoken to (octet-form of
/// [`MULTICAST_GROUP_STR`; pinned equal by test).
pub const MULTICAST_GROUP: std::net::Ipv4Addr = std::net::Ipv4Addr::new(239, 255, 42, 99);
/// Isolation group for v1 experiments (DEBT D1: use until interop is proven).
pub const MULTICAST_GROUP_ISOLATED: std::net::Ipv4Addr = std::net::Ipv4Addr::new(239, 255, 42, 199);
/// The production group in dotted form; fixture test pins it to [`MULTICAST_GROUP`].
pub const MULTICAST_GROUP_STR: &str = "239.255.42.99";
/// The isolated group in dotted form; fixture test pins it to [`MULTICAST_GROUP_ISOLATED`].
pub const MULTICAST_GROUP_ISOLATED_STR: &str = "239.255.42.199";
/// Isolation port for v1 experiments.
pub const DISCOVERY_PORT_ISOLATED: u16 = 7284;
/// How long a received `msg_id` is remembered before it may be accepted again.
pub const DEDUP_TTL_SECS: u64 = 5;
/// Heartbeat interval; also the debounce floor for change-driven chirps.
pub const HEARTBEAT_SECS: u64 = 30;
/// Silence longer than this marks a peer offline (two missed heartbeats + grace).
pub const OFFLINE_THRESHOLD_SECS: u64 = 90;

/// Announcement `type` discriminators, byte-exact with the PoC wire
/// (transcribed from `poc/common/src/infra/communications/announcement_types.rs`
/// — lowercase; pinned again by fixture test).
pub mod announcement {
    pub const DISCOVERY_REQUEST: &str = "discovery_request";
    pub const DISCOVERY_RESPONSE: &str = "discovery_response";
    pub const STONE_CHIRP: &str = "stone_chirp";
    pub const STONE_GOODBYE: &str = "stone_goodbye";
    pub const ELECTION_REQUEST: &str = "election_request";
    pub const ELECTION_CANDIDATE: &str = "election_candidate";
    pub const ELECTION_RESULT: &str = "election_result";
    pub const STORAGE_BEACON: &str = "storage_beacon";
    pub const TOOLS_BEACON: &str = "tools_beacon";

    /// All discriminators the v0 wire defines. A type not listed here is a
    /// v1 extension — v0 stones silently ignore it.
    pub const ALL_V0: [&str; 9] = [
        DISCOVERY_REQUEST,
        DISCOVERY_RESPONSE,
        STONE_CHIRP,
        STONE_GOODBYE,
        ELECTION_REQUEST,
        ELECTION_CANDIDATE,
        ELECTION_RESULT,
        STORAGE_BEACON,
        TOOLS_BEACON,
    ];
}

/// v1 chirp body schema marker. Absent on v0 chirps; presence lets v1 peers
/// recognize v1 speakers without guessing from field shapes.
pub const PROTO_V1: &str = "zg/1";
