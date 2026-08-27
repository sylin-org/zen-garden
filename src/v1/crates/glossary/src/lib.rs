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

/// Offering vocabulary — modes and statuses of placed work (OFFERINGS.md §1).
/// Wire strings lowercase, byte-compatible with the PoC.
pub mod offering {
    /// Planted by the garden from a catalog manifest; stone drives lifecycle.
    pub const MANAGED: &str = "managed";
    /// Found already running on the host; the stone watches it.
    pub const ADOPTED: &str = "adopted";
    /// Lives elsewhere entirely; registered here for discovery only.
    pub const BORROWED: &str = "borrowed";

    /// Install in flight.
    pub const INSTALLING: &str = "installing";
    /// Serving.
    pub const RUNNING: &str = "running";
    /// At rest — reconcile will not auto-start it (OFFERINGS.md §3.2).
    pub const STOPPED: &str = "stopped";
    /// Scheduling fence; no runtime action implied.
    pub const CORDONED: &str = "cordoned";
    /// Nourish/upgrade in flight.
    pub const MAINTENANCE: &str = "maintenance";
    /// Reconcile exhausted its patience, or connectivity lost.
    pub const DEGRADED: &str = "degraded";
    /// Not yet known.
    pub const UNKNOWN: &str = "unknown";

    /// Adoption control levels: how much the stone may do to a found thing.
    pub mod control {
        pub const FULL: &str = "full";
        pub const MONITOR: &str = "monitor";
        pub const ANNOUNCE: &str = "announce";
    }
}

/// The naming well: poetical stone names (`stone-{adjective}-{noun}`),
/// transcribed from the PoC's dictionaries.
pub mod fqn;
pub mod naming;

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
