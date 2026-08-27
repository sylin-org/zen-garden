//! The garden's vocabulary, defined once (CODE-RULES R1.1).
//!
//! Every domain noun and verb the rest of the system may speak lives here.
//! Zero dependencies — this crate is the leaf everything shares. If a name
//! is not here, either it is not a domain concept, or it belongs here and
//! the code is wrong.
//!
//! Every garden word carries its gloss: the nearest standard term, and how
//! the garden word differs — the difference is the feature, so the gloss
//! names it. Household surfaces translate through the plain register
//! (R1.1 as amended 2026-08-27), never reusing operator nouns untranslated.

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
    /// At rest — a desired state, not an event: reconcile will not
    /// auto-start it (OFFERINGS.md §3.2). The wire word is the PoC's
    /// byte-compat spelling; rake's verb is `rest` (≈ `stop`, but it
    /// stays stopped across reboots).
    pub const STOPPED: &str = "stopped";
    /// Cordoned — a fence around the bed: nothing new is scheduled in,
    /// nothing running is touched (≈ taint, in the k8s sense, per-offering).
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
/// Glosses give the nearest standard term first, then the divergence:
/// the garden verb earns its keep precisely where it is NOT the standard
/// term (OFFERINGS.md §1 pairs them the same way).
pub mod verbs {
    /// Walk the room: the whole garden as the attached moss sees it.
    pub const OBSERVE: &str = "observe";
    /// Search by name pattern — `grep`, over stones and offerings.
    pub const FIND: &str = "find";
    /// Plant new work from catalog or image — desired state from birth,
    /// not a one-shot `run` (ports, inputs, and the ledger ride along).
    pub const OFFER: &str = "offer";
    /// Take out of service as a *state* (≈ `stop`, but it stays stopped
    /// across reboots; wake reverses it).
    pub const REST: &str = "rest";
    /// Raise a rested offering (≈ `start`, plus identity: resurrects
    /// from the stored spec — same FQN, same connection string).
    pub const WAKE: &str = "wake";
    /// Carry a planted offering to another stone (`migrate`, with the
    /// directory and ledger coming along).
    pub const MOVE: &str = "move";
    /// Tend what is planted — nourish/upgrade, canary rings first (J3)
    /// (`update`, but the garden drives and can revert it).
    pub const NOURISH: &str = "nourish";
    /// Render the placed record by hand: what runs and WHY it decided
    /// so (`describe`, in the k8s sense).
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
