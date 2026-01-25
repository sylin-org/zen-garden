//! UDP announcement type constants
//!
//! Defines standardized announcement types for the p2p transport protocol.
//! All UDP communication MUST use these types via the p2p transport singleton.

/// Discovery request from a stone looking for peers
pub const DISCOVERY_REQUEST: &str = "discovery_request";

/// Discovery response to a request
pub const DISCOVERY_RESPONSE: &str = "discovery_response";

/// Periodic stone chirp with full state (services, capabilities)
pub const STONE_CHIRP: &str = "stone_chirp";

/// Stone going offline announcement (graceful shutdown)
pub const STONE_GOODBYE: &str = "stone_goodbye";

// Moss election protocol (ELECTION-0001)
/// Election request broadcast (start election)
pub const ELECTION_REQUEST: &str = "election_request";

/// Election candidate response (unicast to requester)
pub const ELECTION_CANDIDATE: &str = "election_candidate";

/// Election result announcement (broadcast winner)
pub const ELECTION_RESULT: &str = "election_result";
