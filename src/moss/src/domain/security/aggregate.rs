//! Security aggregate — pond enrollment, ceremony coordination, HTTPS state.
//!
//! Owns all security-related mutable state behind a typed command/query API.
//! External code never reads or writes the raw fields.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{RwLock, broadcast};

use super::event::{SecurityChangeKind, SecurityChanged};
use super::pond_client::PondClient;
use crate::domain::ceremony::CeremonyRegistry;
use crate::domain::metrics::Metrics;

// ============================================================================
// State (private)
// ============================================================================

/// Internal mutable state of the Security aggregate.
struct State {
    /// Enrollment state — true when this stone has valid pond certificates.
    enrolled: bool,
    /// Cornerstone hostname (CA holder), if known.
    cornerstone: Option<String>,
    /// Decorative pond name (e.g. "pond-still-lotus").
    name: Option<String>,
}

impl State {
    fn new() -> Self {
        Self {
            enrolled: false,
            cornerstone: None,
            name: None,
        }
    }
}

// ============================================================================
// Aggregate
// ============================================================================

/// Security aggregate — pond enrollment, ceremony infra, HTTPS lifecycle.
///
/// Generic over the inter-stone HTTP client. Defaults to `StoneClient`
/// (the sole production implementation).
pub struct Security<P: PondClient = crate::infra::stone_client::StoneClient> {
    /// Private mutable state.
    state: RwLock<State>,

    /// Fast-path boolean for hot-loop checks (chirp signing, HTTPS routing).
    /// Kept in sync with `state.enrolled` by every enrollment command.
    active: Arc<AtomicBool>,

    /// HTTPS listener started guard — prevents double-binding :7183.
    https_started: AtomicBool,

    /// Stone-to-stone HTTP client gateway.
    /// Automatically upgrades to HTTPS+mTLS when pond certs are available.
    client: Arc<P>,

    /// Ceremony host — drives pond init/join/unlock ceremonies via koi-common.
    ceremony_host:
        Arc<koi_common::ceremony::CeremonyHost<koi_certmesh::pond_ceremony::PondCeremonyRules>>,

    /// In-memory active ceremony registry.
    ceremony_registry: Arc<CeremonyRegistry>,

    /// Persistent journal for crash recovery.
    ceremony_journal: Arc<dyn super::ceremony_persistence::CeremonyPersistence + Send + Sync>,

    /// Domain event broadcast channel.
    changed: broadcast::Sender<SecurityChanged>,

    /// Metrics injection.
    metrics: Arc<Metrics>,
}

impl<P: PondClient> Security<P> {
    /// Registered domain name for Metrics.
    pub const NAME: &'static str = "security";

    /// Construct a new Security aggregate.
    ///
    /// `active` is the shared `pond_active` arc that is also injected
    /// into the mDNS handle at bootstrap. This is the one exception to
    /// aggregate-owned state — the arc is created before the aggregate
    /// because mDNS needs it first.
    pub async fn new(
        active: Arc<AtomicBool>,
        client: Arc<P>,
        ceremony_host: Arc<
            koi_common::ceremony::CeremonyHost<koi_certmesh::pond_ceremony::PondCeremonyRules>,
        >,
        ceremony_registry: Arc<CeremonyRegistry>,
        ceremony_journal: Arc<dyn super::ceremony_persistence::CeremonyPersistence + Send + Sync>,
        metrics: Arc<Metrics>,
    ) -> Self {
        let (changed, _) = broadcast::channel(64);

        metrics
            .register_domain(Self::NAME, SecurityChangeKind::ALL_NAMES)
            .await;

        Self {
            state: RwLock::new(State::new()),
            active,
            https_started: AtomicBool::new(false),
            client,
            ceremony_host,
            ceremony_registry,
            ceremony_journal,
            changed,
            metrics,
        }
    }

    // ── Queries ──────────────────────────────────────────────────────

    /// Is this stone enrolled in a pond (has valid certs)?
    pub fn enrolled(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// Fast-path alias for `enrolled()` — kept for call-site clarity
    /// where "pond active" is the domain phrase.
    pub fn pond_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// The cornerstone hostname (CA holder), if known.
    pub async fn cornerstone(&self) -> Option<String> {
        self.state.read().await.cornerstone.clone()
    }

    /// The decorative pond name.
    pub async fn pond_name(&self) -> Option<String> {
        self.state.read().await.name.clone()
    }

    /// Is the HTTPS listener started?
    pub fn https_started(&self) -> bool {
        self.https_started.load(Ordering::Relaxed)
    }

    /// Access the inter-stone HTTP client.
    pub fn stone_client(&self) -> &Arc<P> {
        &self.client
    }

    /// Access the ceremony host (koi-common pond ceremony protocol).
    pub fn ceremony_host(
        &self,
    ) -> &Arc<koi_common::ceremony::CeremonyHost<koi_certmesh::pond_ceremony::PondCeremonyRules>>
    {
        &self.ceremony_host
    }

    /// Access the ceremony registry (in-memory active ceremonies).
    pub fn ceremony_registry(&self) -> &Arc<CeremonyRegistry> {
        &self.ceremony_registry
    }

    /// Access the ceremony journal (persistence port).
    pub fn ceremony_journal(
        &self,
    ) -> &Arc<dyn super::ceremony_persistence::CeremonyPersistence + Send + Sync> {
        &self.ceremony_journal
    }

    /// Subscribe to security domain events.
    pub fn changes(&self) -> broadcast::Receiver<SecurityChanged> {
        self.changed.subscribe()
    }

    /// Clone the `pond_active` arc for hot-path injection (e.g. mDNS handle).
    pub fn active_arc(&self) -> Arc<AtomicBool> {
        self.active.clone()
    }

    // ── Commands ─────────────────────────────────────────────────────

    /// Mark this stone as enrolled in a pond.
    ///
    /// Updates enrollment state, emits `SecurityChanged::Enrolled`.
    /// Returns `true` if the enrollment state actually changed.
    pub async fn mark_enrolled(&self, cornerstone: Option<String>) -> bool {
        let changed = {
            let mut s = self.state.write().await;
            let was_enrolled = s.enrolled;
            s.enrolled = true;
            s.cornerstone = cornerstone.clone();
            !was_enrolled
        };
        self.active.store(true, Ordering::Relaxed);

        if changed {
            self.finalize(SecurityChangeKind::Enrolled { cornerstone })
                .await;
        }
        changed
    }

    /// Mark this stone as unenrolled from a pond.
    ///
    /// Clears enrollment state, emits `SecurityChanged::Unenrolled`.
    /// Returns `true` if the enrollment state actually changed.
    pub async fn mark_unenrolled(&self) -> bool {
        let changed = {
            let mut s = self.state.write().await;
            let was_enrolled = s.enrolled;
            s.enrolled = false;
            s.cornerstone = None;
            s.name = None;
            was_enrolled
        };
        self.active.store(false, Ordering::Relaxed);

        if changed {
            self.finalize(SecurityChangeKind::Unenrolled).await;
        }
        changed
    }

    /// Update the `pond_active` flag from an external check (e.g. certmesh status).
    ///
    /// This is a direct-set command — no event is emitted. Use
    /// `mark_enrolled`/`mark_unenrolled` for full enrollment transitions.
    pub fn refresh_active(&self, is_active: bool) {
        self.active.store(is_active, Ordering::Relaxed);
    }

    /// Attempt to mark HTTPS listener as started (CAS).
    ///
    /// Returns `true` if this call actually set the flag (was false, now true).
    /// Returns `false` if HTTPS was already started (idempotent guard).
    pub fn try_set_https_started(&self) -> bool {
        self.https_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Mark HTTPS listener as started (unconditional).
    pub fn set_https_started(&self) {
        self.https_started.store(true, Ordering::Relaxed);
    }

    /// Clear HTTPS listener started flag.
    pub fn clear_https_started(&self) {
        self.https_started.store(false, Ordering::Relaxed);
    }

    /// Set the decorative pond name.
    ///
    /// Emits `SecurityChanged::PondRenamed`.
    pub async fn set_pond_name(&self, name: String) {
        {
            let mut s = self.state.write().await;
            s.name = Some(name.clone());
        }
        self.finalize(SecurityChangeKind::PondRenamed { name })
            .await;
    }

    /// Seed enrollment state at boot (no event emitted).
    pub async fn seed_state(
        &self,
        enrolled: bool,
        cornerstone: Option<String>,
        name: Option<String>,
    ) {
        {
            let mut s = self.state.write().await;
            s.enrolled = enrolled;
            s.cornerstone = cornerstone;
            s.name = name;
        }
        self.active.store(enrolled, Ordering::Relaxed);
    }

    /// Recover incomplete ceremonies from journal (crash recovery).
    ///
    /// Returns count of recovered ceremonies loaded into the registry.
    pub async fn recover_ceremonies(&self) -> anyhow::Result<usize> {
        let incomplete = self.ceremony_journal.load_active().await?;
        let count = incomplete.len();

        for ceremony in incomplete {
            tracing::warn!(
                ceremony_id = %ceremony.id,
                ceremony_type = ceremony.ceremony_type.name(),
                state = ?ceremony.state,
                "Found incomplete ceremony from previous run"
            );
            self.ceremony_registry.insert(ceremony).await;
        }

        if count > 0 {
            tracing::warn!(
                count,
                "Recovered incomplete ceremonies - manual intervention may be required"
            );
        }

        Ok(count)
    }

    // ── Finalize pipeline ────────────────────────────────────────────

    async fn finalize(&self, kind: SecurityChangeKind) {
        let kind_name = kind.name();

        let event = SecurityChanged {
            kind,
            timestamp: chrono::Utc::now(),
        };

        // Emit on the typed broadcast channel
        let _ = self.changed.send(event);

        // Record in Metrics
        self.metrics
            .record_domain_event(Self::NAME, kind_name)
            .await;
    }
}
