// Slice 1 of ADR-0002 lands the vocabulary and the two pure decision
// functions; adapters consume them at create moments (slices 3-4).
#![allow(dead_code)]

//! Port address arbitration (ADR-0002) — PURE policy: claims × intents ×
//! observations → decisions. No sockets, no I/O, no time. Adapters call
//! these at create moments; the Converger only ever observes.
//!
//! Vocabulary:
//!   · claim     — a stake in the ledger: one port, one owner, alive while
//!                 the offering exists. Rest counts; "offline" is not free
//!                 (ruling 3 of ADR-0002 provenance).
//!   · intent    — manifest-declared allocation desire for a named role
//!                 (`host_ports:` §5.1); roles absent from it are flexible.
//!   · home      — the allocated address. Identity side of ADR-0002:
//!                 survives rest, rehydration, everything short of uproot.
//!   · residence — where the workload answers NOW. Fact side: chosen at
//!                 create by the adapter via [`residence`], reported
//!                 honestly everywhere facts are read.
//!
//! Determinism contract: role iteration is alphabetical ([BTreeMap]); pool
//! draws ascend from [`Pool::start`]. Same inputs ⇒ same plan ⇒ same hash.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Default service pool (ADR-0002 ruling 1): clear of the reserved
/// 7284–7299 infra block and of typical OS ephemeral ranges. Overridable
/// per stone via `MOSS_SERVICE_PORT_POOL`.
pub const DEFAULT_POOL_START: u16 = 7300;
pub const DEFAULT_POOL_END: u16 = 7449;

/// Contiguous inclusive span of pool ports drawn for flexible allocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pool {
    pub start: u16,
    pub end: u16,
}

impl Default for Pool {
    fn default() -> Self {
        Self {
            start: DEFAULT_POOL_START,
            end: DEFAULT_POOL_END,
        }
    }
}

impl Pool {
    /// `"7300-7449"` inclusive span, or a lone `"9000"` as a single-port pool.
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if let Some((a, b)) = s.split_once('-') {
            let start: u16 = a.trim().parse().map_err(|_| format!("pool start '{a}'"))?;
            let end: u16 = b.trim().parse().map_err(|_| format!("pool end '{b}'"))?;
            if start == 0 || start > end {
                return Err(format!("pool '{s}' is zero or inverted"));
            }
            Ok(Self { start, end })
        } else {
            let port: u16 = s.parse().map_err(|_| format!("pool '{s}' is not N or N-M"))?;
            if port == 0 {
                return Err("pool port 0 is meaningless".into());
            }
            Ok(Self { start: port, end: port })
        }
    }

    /// The env twin `MOSS_SERVICE_PORT_POOL`, else the default pool.
    pub fn from_env() -> Self {
        match std::env::var("MOSS_SERVICE_PORT_POOL") {
            Ok(v) => match Self::parse(&v) {
                Ok(p) => p,
                Err(e) => {
                    // Configuration error on an OPTIONAL knob: warn loudly,
                    // keep the garden addressable (L17 applies to steps, not knobs).
                    tracing::warn!(value = %v, error = %e, "MOSS_SERVICE_PORT_POOL ignored");
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    fn contains(&self, p: u16) -> bool {
        (self.start..=self.end).contains(&p)
    }
}

/// How strongly an offering wants its declared home (ADR-0002 ruling 2:
/// requiredness is declared in the manifest, never inferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Identity-critical — DNS IS :53. Taken at plant by ANYONE (ledger or
    /// socket) ⇒ loud refusal naming the squatter. Never relocates silently.
    Strict,
    /// Preferred home; falls back to a pool draw when unavailable today,
    /// keeps its future-return rights (the claim stays first-come).
    Soft,
    /// Any stable slot; no opinion beyond "mine thereafter".
    Flexible,
}

/// Manifest-declared allocation desire for one named role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intent {
    pub tier: Tier,
    /// Pinned home (`port:`). `None` only for flexible roles synthesized
    /// from bare `ports:` entries.
    pub home: Option<u16>,
}

/// One ledger entry: `owner` permanently holds `port`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub owner: String,
    pub port: u16,
}

impl Claim {
    pub fn new(owner: &str, port: u16) -> Self {
        Self {
            owner: owner.to_string(),
            port,
        }
    }
}

/// Why the allocator refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocError {
    /// A strict role's port is claimed by another garden member — a REAL
    /// dispute between citizens, i.e. a misconfigured pair to surface.
    ClaimConflict { port: u16, holder: String },
    /// Two roles of one offering demand the same port; simultaneous
    /// bindings cannot share a host port.
    DuplicatePin { port: u16 },
    /// Pins plus claims left no pool slot for every flexible role.
    Exhausted { wanted: usize },
    /// The intent set itself was malformed.
    Invalid(String),
}

impl std::fmt::Display for AllocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClaimConflict { port, holder } => {
                write!(f, "host port {port} is held by garden member '{holder}'")
            }
            Self::DuplicatePin { port } => {
                write!(f, "two roles pin the same host port {port}")
            }
            Self::Exhausted { wanted } => write!(
                f,
                "service pool exhausted: no free slot for {wanted} remaining role(s)"
            ),
            Self::Invalid(e) => write!(f, "{e}"),
        }
    }
}

/// Lowest port ≥ `from` not in `blocked`. Pure scanning primitive.
fn first_free(from: u16, to: u16, blocked: &BTreeSet<u16>) -> Option<u16> {
    (from..=to).find(|p| !blocked.contains(p))
}

/// Resolve HOMES for every role against the ledger. Ledger before sockets:
/// claims decide between garden members whether they run, rest, or were
/// adopted yesterday. Reality is someone else's concern here.
///
/// Pass 1 pins strict/soft declarations; pass 2 draws ascending pool slots
/// for what remains. Undeclared roles are implicitly flexible — the caller
/// synthesizes their intents (see compile / offer paths).
pub fn allocate(
    intents: &BTreeMap<String, Intent>,
    claims: &[Claim],
    pool: Pool,
) -> Result<BTreeMap<String, u16>, AllocError> {
    // Malformed up front: one host port cannot serve two roles at once.
    let mut pinned_seen: BTreeSet<u16> = BTreeSet::new();
    for (role, intent) in intents {
        if let Some(home) = intent.home
            && !pinned_seen.insert(home)
        {
            return Err(AllocError::DuplicatePin { port: home });
        }
        if intent.tier == Tier::Strict && intent.home.is_none() {
            return Err(AllocError::Invalid(format!(
                "role '{role}' declared strict without a port"
            )));
        }
    }

    let mut taken: BTreeSet<u16> = claims.iter().map(|c| c.port).collect();
    let mut deferred: Vec<String> = Vec::new();
    let mut homes = BTreeMap::new();

    for (role, intent) in intents {
        match (intent.tier, intent.home) {
            (Tier::Strict, Some(port)) | (Tier::Soft, Some(port))
                if intent.tier == Tier::Strict =>
            {
                if let Some(holder) = claims.iter().find(|c| c.port == port).map(|c| c.owner.clone()) {
                    return Err(AllocError::ClaimConflict { port, holder });
                }
                taken.insert(port);
                homes.insert(role.clone(), port);
            }
            (Tier::Soft, Some(port)) if !taken.contains(&port) => {
                taken.insert(port);
                homes.insert(role.clone(), port);
            }
            // Strict-without-port was validated above; degrade honestly
            // rather than panic if a caller bypasses the pure module's checks.
            (Tier::Strict, None) => {
                return Err(AllocError::Invalid(format!(
                    "role '{role}' declared strict without a port"
                )));
            }
            _ => deferred.push(role.clone()), // soft-fallen-through or flexible
        }
    }

    let wanted = deferred.len();
    for role in deferred {
        let Some(slot) = first_free(pool.start, pool.end, &taken) else {
            return Err(AllocError::Exhausted { wanted });
        };
        taken.insert(slot);
        homes.insert(role, slot);
    }
    Ok(homes)
}

/// Why a create-time residence choice refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResidenceError {
    /// A strict home was occupied at create by an outsider socket — refuse
    /// loudly rather than relocate a pinned identity (ADR-0002 §Decision).
    StrictHomeTaken { port: u16 },
    /// Pins, claims and occupancy together left nowhere to land.
    Exhausted,
}

impl std::fmt::Display for ResidenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StrictHomeTaken { port } => write!(
                f,
                "strict host port {port} is occupied by a foreign process"
            ),
            Self::Exhausted => write!(f, "no residence available in the service pool"),
        }
    }
}

/// Choose where a role BINDS at a create moment (plant / heal / resurrect).
/// `occupied` is socket truth gathered at the edge — any process, ours or
/// theirs. Claims are excluded so a rested neighbour's address is never
/// suggested to a relocated sibling; a squatted HOME is tolerated for
/// soft/flexible tiers (relocation under protest, recorded), while strict
/// refuses outright.
pub fn residence(
    home: u16,
    tier: Tier,
    occupied: &BTreeSet<u16>,
    claims: &[u16],
    pool: Pool,
) -> Result<u16, ResidenceError> {
    if tier == Tier::Strict && occupied.contains(&home) {
        return Err(ResidenceError::StrictHomeTaken { port: home });
    }
    if !occupied.contains(&home) {
        return Ok(home);
    }
    let mut blocked: BTreeSet<u16> = occupied.clone();
    blocked.extend(claims.iter().copied());
    first_free(pool.start, pool.end, &blocked).ok_or(ResidenceError::Exhausted)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const POOL: Pool = Pool {
        start: 7300,
        end: 7449,
    };

    fn flex() -> Intent {
        Intent {
            tier: Tier::Flexible,
            home: None,
        }
    }

    fn soft(p: u16) -> Intent {
        Intent {
            tier: Tier::Soft,
            home: Some(p),
        }
    }

    fn strict(p: u16) -> Intent {
        Intent {
            tier: Tier::Strict,
            home: Some(p),
        }
    }

    #[test]
    fn flexible_roles_draw_ascending_slots_in_role_order() {
        let mut intents = BTreeMap::new();
        intents.insert("web".to_string(), flex());
        intents.insert("api".to_string(), flex());
        let homes = allocate(&intents, &[], POOL).unwrap();
        assert_eq!(homes["api"], 7300, "alphabetical roles, ascending pool");
        assert_eq!(homes["web"], 7301);
    }

    #[test]
    fn ledger_first_a_rested_claim_blocks_flexible_b() {
        // A rests holding 7300 — statuses don't exist down here. B plants.
        let claims = vec![Claim::new("service-a", 7300)];
        let mut intents = BTreeMap::new();
        intents.insert("default".to_string(), flex());
        let homes = allocate(&intents, &claims, POOL).unwrap();
        assert_eq!(homes["default"], 7301, "no socket probe outranks a claim");
    }

    #[test]
    fn soft_pin_is_honored_when_free() {
        let mut intents = BTreeMap::new();
        intents.insert("ui".to_string(), soft(8080));
        let homes = allocate(&intents, &[], POOL).unwrap();
        assert_eq!(homes["ui"], 8080);
    }

    #[test]
    fn soft_pin_degrades_to_pool_when_claimed_and_keeps_future_rights() {
        let claims = vec![Claim::new("spring-app", 8080)];
        let mut intents = BTreeMap::new();
        intents.insert("ui".to_string(), soft(8080));
        let homes = allocate(&intents, &claims, POOL).unwrap();
        assert_eq!(homes["ui"], 7300, "graceful degradation, deterministic");
        // The failed preference is remembered by the manifest itself; when
        // spring-app uproots, this offering's NEXT rebuild may take 8080.
    }

    #[test]
    fn strict_conflict_with_a_garden_member_names_the_holder() {
        let claims = vec![Claim::new("technitium", 53)];
        let mut intents = BTreeMap::new();
        intents.insert("dns".to_string(), strict(53));
        let err = allocate(&intents, &claims, POOL).unwrap_err();
        assert_eq!(
            err,
            AllocError::ClaimConflict {
                port: 53,
                holder: "technitium".into()
            }
        );
    }

    #[test]
    fn strict_unclaimed_ports_outside_the_pool_are_honored() {
        // :53 is below every pool bound by design (DNS identity).
        let mut intents = BTreeMap::new();
        intents.insert("dns".to_string(), strict(53));
        let homes = allocate(&intents, &[], POOL).unwrap();
        assert_eq!(homes["dns"], 53);
    }

    #[test]
    fn duplicate_pins_are_rejected_whatever_the_tiers() {
        let mut intents = BTreeMap::new();
        intents.insert("a".to_string(), soft(8080));
        intents.insert("b".to_string(), flex());
        intents.insert("c".to_string(), soft(8080));
        assert_eq!(
            allocate(&intents, &[], POOL),
            Err(AllocError::DuplicatePin { port: 8080 })
        );
    }

    #[test]
    fn strict_without_port_is_invalid_even_here() {
        let mut intents = BTreeMap::new();
        intents.insert(
            "dns".to_string(),
            Intent {
                tier: Tier::Strict,
                home: None,
            },
        );
        assert!(matches!(allocate(&intents, &[], POOL), Err(AllocError::Invalid(_))));
    }

    #[test]
    fn exhaustion_names_the_shortfall() {
        let tiny = Pool::parse("7500").unwrap();
        let claims = vec![Claim::new("squatter", 7500)];
        let mut intents = BTreeMap::new();
        intents.insert("default".to_string(), flex());
        assert_eq!(
            allocate(&intents, &claims, tiny),
            Err(AllocError::Exhausted { wanted: 1 })
        );
    }

    #[test]
    fn pool_parses_spans_singles_and_rejects_nonsense() {
        assert_eq!(Pool::parse("7300-7449").unwrap(), POOL);
        assert_eq!(
            Pool::parse("9000").unwrap(),
            Pool { start: 9000, end: 9000 }
        );
        assert_eq!(Pool::parse(" 8000 - 8002 ").unwrap().start, 8000);
        assert!(Pool::parse("8100-8000").is_err());
        assert!(Pool::parse("abc").is_err());
        assert!(Pool::parse("0-100").is_err());
    }

    #[test]
    fn residence_returns_home_when_quiet() {
        let out = residence(7300, Tier::Flexible, &BTreeSet::new(), &[], POOL).unwrap();
        assert_eq!(out, 7300);
    }

    #[test]
    fn residence_relocates_past_outsider_without_touching_sibling_claims() {
        let mut occupied = BTreeSet::new();
        occupied.insert(7300); // outsider squatter
        let claims = vec![7310]; // rested sibling's home — untouchable
        let out = residence(7300, Tier::Flexible, &occupied, &claims, POOL).unwrap();
        assert_eq!(out, 7301, "first free slot that isn't occupied or claimed");
    }

    #[test]
    fn strict_residence_refuses_instead_of_relocating_identity() {
        let mut occupied = BTreeSet::new();
        occupied.insert(53);
        assert_eq!(
            residence(53, Tier::Strict, &occupied, &[], POOL),
            Err(ResidenceError::StrictHomeTaken { port: 53 })
        );
    }

    #[test]
    fn soft_residence_relocates_under_protest_but_would_return_home_later() {
        let mut occupied = BTreeSet::new();
        occupied.insert(8080);
        let out = residence(8080, Tier::Soft, &occupied, &[], POOL).unwrap();
        assert_eq!(out, 7300, "temporary neighbor absorbed at the pool edge");
    }

    #[test]
    fn residence_exhaustion_is_explicit() {
        let mut occupied = BTreeSet::new();
        occupied.insert(7300);
        let claims = vec![7301];
        let tiny = Pool::parse("7300-7301").unwrap();
        assert_eq!(
            residence(7300, Tier::Flexible, &occupied, &claims, tiny),
            Err(ResidenceError::Exhausted)
        );
    }
}
