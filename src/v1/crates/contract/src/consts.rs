//! Discovery-domain constants — the wire's fixed points (R1.7).
//!
//! v1 owns a declared topology of its own (charter amendment 2026-08-25):
//! the PoC proved the mechanisms work; v1 chooses its room deliberately
//! instead of inheriting one. Port management is part of the design: the
//! v1 block is **7284–7299**, assigned below and in the registry comments.
//!
//! Changing an assignment is a contract change: it must land here first,
//! with its fixture pin updated in the same commit.

use std::net::Ipv4Addr;

// ---------------------------------------------------------------------------
// The v1 topology — this generation's home. Every default points here.
// ---------------------------------------------------------------------------

/// UDP port where v1 stones speak discovery (chirps, ask/tell).
pub const DISCOVERY_PORT_V1: u16 = 7284;
/// IPv4 multicast group of the v1 discovery room.
pub const MULTICAST_GROUP_V1: Ipv4Addr = Ipv4Addr::new(239, 255, 42, 199);
/// The v1 group in dotted form; fixture test pins it to [`MULTICAST_GROUP_V1`].
pub const MULTICAST_GROUP_V1_STR: &str = "239.255.42.199";

// Block registry (assigned / reserved):
//   7284/udp      discovery multicast room        (this file)
//   7285/tcp      stone HTTP surface              (kernel::config::HttpConfig)
//   7286..7299    reserved for v1 subsystems — storage proxy, MCP surface,
//                 companions; claim here before first bind anywhere else.

// ---------------------------------------------------------------------------
// The PoC topology — legacy reference only. Never a default again.
// Kept so ops tooling and docs can name the old room precisely.
// ---------------------------------------------------------------------------

/// UDP port the PoC fleet speaks discovery on.
pub const DISCOVERY_PORT_POC: u16 = 7184;
/// IPv4 multicast group of the PoC discovery room.
pub const MULTICAST_GROUP_POC: Ipv4Addr = Ipv4Addr::new(239, 255, 42, 99);
/// The PoC group in dotted form; fixture test pins it to [`MULTICAST_GROUP_POC`].
pub const MULTICAST_GROUP_POC_STR: &str = "239.255.42.99";

// ---------------------------------------------------------------------------
// Protocol timing and vocabulary (shared shape with the PoC wire format).
// ---------------------------------------------------------------------------

/// How long a received `msg_id` is remembered before it may be accepted again.
pub const DEDUP_TTL_SECS: u64 = 5;
/// Heartbeat interval; also the debounce floor for change-driven chirps.
pub const HEARTBEAT_SECS: u64 = 30;
/// Silence longer than this marks a peer offline (two missed heartbeats + grace).
pub const OFFLINE_THRESHOLD_SECS: u64 = 90;

/// How long an unconfirmed candidate survives (ADR-0004 §3): knowledge
/// heard through middlemen — an overheard rich answer about a stone we
/// have never met. Must outlive the room's IGMP convergence breath
/// (L24: one querier cycle, ~60–125s) with margin; dies before a rumor
/// can matter. The stone's own live frame promotes the truth and
/// retires the rumor, whichever comes first.
pub const CANDIDATE_TTL_SECS: u64 = 300;

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

    /// The full-voice announcement (ADR-0004 A2.2): presence plus one or
    /// more inventory domains, spoken on boot and on change. A v1
    /// extension — NOT in `ALL_V0`; foreign-generation stones silently
    /// ignore a kind they never defined.
    pub const STONE_SONG: &str = "stone_song";

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
