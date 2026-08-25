//! Discovery-domain constants — the wire's fixed points (R1.7).
//!
//! Values harvested from the PoC (branch `poc`); changing any of these is a
//! breaking change to R0.5 and must fail fixture tests before it merges.

/// UDP port every garden voice speaks discovery on.
pub const DISCOVERY_PORT: u16 = 7184;
/// IPv4 multicast group chirps are spoken to.
pub const MULTICAST_GROUP: &str = "239.255.42.99";
/// Isolation group for v1 experiments (DEBT D1: use until interop is proven).
pub const MULTICAST_GROUP_ISOLATED: &str = "239.255.42.199";
/// Isolation port for v1 experiments.
pub const DISCOVERY_PORT_ISOLATED: u16 = 7284;
/// How long a received `msg_id` is remembered before it may be accepted again.
pub const DEDUP_TTL_SECS: u64 = 5;
/// Heartbeat interval; also the debounce floor for change-driven chirps.
pub const HEARTBEAT_SECS: u64 = 30;
/// Silence longer than this marks a peer offline (two missed heartbeats + grace).
pub const OFFLINE_THRESHOLD_SECS: u64 = 90;

/// Announcement `type` discriminators, byte-exact with the PoC wire.
pub mod announcement {
    pub const DISCOVERY_REQUEST: &str = "DISCOVERY_REQUEST";
    pub const DISCOVERY_RESPONSE: &str = "DISCOVERY_RESPONSE";
    pub const STONE_CHIRP: &str = "STONE_CHIRP";
    pub const STONE_GOODBYE: &str = "STONE_GOODBYE";
    pub const ELECTION_REQUEST: &str = "ELECTION_REQUEST";
    pub const ELECTION_CANDIDATE: &str = "ELECTION_CANDIDATE";
    pub const ELECTION_RESULT: &str = "ELECTION_RESULT";
    pub const STORAGE_BEACON: &str = "STORAGE_BEACON";
    pub const TOOLS_BEACON: &str = "TOOLS_BEACON";

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
