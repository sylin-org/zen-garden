//! The garden's vocabulary, defined once (CODE-RULES R1.1).
//!
//! Every domain noun and verb the rest of the system may speak lives here.
//! Zero dependencies — this crate is the leaf everything shares. If a name
//! is not here, either it is not a domain concept, or it belongs here and
//! the code is wrong.

/// Health vocabulary — a stone's self-assessed vitality.
pub mod health {
    /// Booting; capabilities not yet detected.
    pub const STARTING: &str = "starting";
    /// Running; hardware detection incomplete.
    pub const INITIALIZING: &str = "initializing";
    /// Fully operational (the green on the wall).
    pub const THRIVING: &str = "thriving";
    /// Operational with a degraded component.
    pub const DEGRADED: &str = "degraded";
}

/// Presence vocabulary — a stone's membership state as seen by peers.
pub mod presence {
    /// Actively announcing (seen within the offline threshold).
    pub const ONLINE: &str = "online";
    /// Stopped announcing but remembered for wake-on-LAN.
    pub const OFFLINE: &str = "offline";
}

/// Domain verbs — CLI words, API words, function words. One spelling each.
pub mod verbs {
    pub const OBSERVE: &str = "observe";
    pub const FIND: &str = "find";
    pub const OFFER: &str = "offer";
    pub const REST: &str = "rest";
    pub const WAKE: &str = "wake";
    pub const MOVE: &str = "move";
    pub const NOURISH: &str = "nourish";
    pub const EXPLAIN: &str = "explain";
}

/// Bounded contexts — module and event-domain names.
pub mod domains {
    pub const DISCOVERY: &str = "discovery";
    pub const PRESENCE: &str = "presence";
    pub const HTTP: &str = "http";

    /// API surface categories (L22): every moss route lives in exactly one.
    pub const LOCAL: &str = "local";
    pub const STONE: &str = "stone";
    pub const GARDEN: &str = "garden";
}
