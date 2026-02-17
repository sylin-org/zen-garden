//! Distributed election service - UDP listener and responder
//!
//! This service implements the election protocol specified in
//! docs/specs/ELECTION-0001-distributed-election.md
//!
//! **REFACTORED (COMM-0001 Phase 4)**: Now uses p2p transport singleton for all UDP operations.
//! Subscribes to UDP events via p2p::subscribe_to_events() instead of binding own socket.
//!
//! **EXTENDED (ORCH-0001 Phase 2)**: Added Fitness scoring mode.
//! - `ScoreMechanism::Blake` (default): BLAKE3 hash delay, first respondent wins.
//! - `ScoreMechanism::Fitness`: Candidates respond immediately with fitness scores.
//!   Requester collects until quiet timeout or hard cap, then picks highest.

use anyhow::Result;
use garden_common::constants::orchestration::{FITNESS_HARD_CAP_MS, FITNESS_QUIET_TIMEOUT_MS};
use garden_common::election::{
    calculate_election_delay, matches_criteria, ElectionCandidate, ElectionRequest, ElectionResult,
    ElectionType, ElectionWinner, ScoreMechanism,
};
use garden_common::infra::communications::announcement_types;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::timeout;

use garden_common::infra::communications::p2p;

/// Maximum pending elections to track
const MAX_PENDING_ELECTIONS: usize = 100;

/// Cleanup interval for expired elections
const CLEANUP_INTERVAL_SECS: u64 = 60;

/// TTL for initiated elections cache (prevent self-response)
const INITIATED_ELECTION_TTL_SECS: u64 = 60;

/// Election service state
///
/// **REFACTORED (COMM-0001 Phase 4)**: Does NOT create/own UDP socket.
/// Subscribes to UDP events from p2p singleton transport.
pub struct ElectionService {
    /// This stone's ID
    stone_id: String,
    /// This stone's name
    stone_name: String,
    /// Pending candidate timers (election_id -> PendingElection)
    pending: Arc<RwLock<HashMap<String, PendingElection>>>,
    /// Initiated elections (self-exclusion) - election_id -> timestamp
    initiated: Arc<RwLock<HashMap<String, Instant>>>,
    /// Current state provider (for criteria evaluation)
    state_provider: Arc<RwLock<Box<dyn StateProvider>>>,
    /// Fitness provider (for OfferingPrimary elections, ORCH-0001)
    fitness_provider: Arc<RwLock<Option<Box<dyn FitnessProvider>>>>,
}

// Make ElectionService clonable by cloning the Arcs
impl Clone for ElectionService {
    fn clone(&self) -> Self {
        Self {
            stone_id: self.stone_id.clone(),
            stone_name: self.stone_name.clone(),
            pending: self.pending.clone(),
            initiated: self.initiated.clone(),
            state_provider: self.state_provider.clone(),
            fitness_provider: self.fitness_provider.clone(),
        }
    }
}

/// Pending election state for candidates
#[allow(dead_code)]
struct PendingElection {
    election_id: String,
    timer_handle: Option<tokio::task::JoinHandle<()>>,
    created_at: Instant,
}

/// Trait for providing stone state for criteria evaluation
pub trait StateProvider: Send + Sync {
    fn get_state(&self) -> HashMap<String, Value>;
}

/// Trait for computing fitness scores (ORCH-0001).
///
/// Implemented by the domain layer, injected into `ElectionService`.
/// The election service never knows _how_ scores are computed — only
/// that it can ask for one given an offering FQN.
pub trait FitnessProvider: Send + Sync {
    /// Compute fitness score for the given offering FQN.
    ///
    /// Returns `Some(score)` if eligible, `None` if ineligible (don't respond).
    /// Score range: `[-1000, 1000]`. `1001` = pinned (always wins).
    /// Also returns `pin_timestamp` if pinned.
    fn compute_fitness(
        &self,
        offering_fqn: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<(i16, Option<String>)>> + Send + '_>>;
}

impl ElectionService {
    /// Create new election service (no socket binding, no async needed)
    pub fn new(
        stone_id: String,
        stone_name: String,
        state_provider: Box<dyn StateProvider>,
    ) -> Self {
        tracing::info!(
            stone_id = %stone_id,
            stone_name = %stone_name,
            "Election service initialized (will subscribe to p2p transport)"
        );

        Self {
            stone_id,
            stone_name,
            pending: Arc::new(RwLock::new(HashMap::new())),
            initiated: Arc::new(RwLock::new(HashMap::new())),
            state_provider: Arc::new(RwLock::new(state_provider)),
            fitness_provider: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the fitness provider (injected after AppState construction).
    ///
    /// Called by bootstrap once the domain layer is ready.
    pub async fn set_fitness_provider(&self, provider: Box<dyn FitnessProvider>) {
        let mut fp = self.fitness_provider.write().await;
        *fp = Some(provider);
        tracing::debug!("Fitness provider set on election service");
    }

    /// Start UDP event listener loop (subscribes to p2p transport)
    /// Call this from bootstrap as background task after p2p initialization
    pub async fn run_listener(self: Arc<Self>) -> Result<()> {
        tracing::info!("Election service listener starting, subscribing to p2p transport");

        // Start cleanup loop
        let cleanup_service = self.clone();
        tokio::spawn(async move {
            cleanup_service.run_cleanup_loop().await;
        });

        // Subscribe to all p2p UDP events (need request, candidate, result)
        let mut udp_rx = p2p::subscribe_to_all().await?;

        loop {
            match udp_rx.recv().await {
                Some((announcement_type, payload, from_addr)) => {
                    if let Err(e) = self
                        .handle_udp_event(announcement_type, payload, from_addr)
                        .await
                    {
                        tracing::debug!(error = ?e, "Failed to handle UDP event");
                    }
                }
                None => {
                    tracing::error!("P2P channel closed");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle incoming UDP event from p2p transport
    async fn handle_udp_event(
        &self,
        announcement_type: String,
        payload: serde_json::Value,
        from_addr: std::net::SocketAddr,
    ) -> Result<()> {
        match announcement_type.as_str() {
            announcement_types::ELECTION_REQUEST => {
                let request: ElectionRequest = serde_json::from_value(payload)?;
                self.handle_election_request(request, from_addr).await?;
            }
            announcement_types::ELECTION_RESULT => {
                let result: ElectionResult = serde_json::from_value(payload)?;
                self.handle_election_result(result).await?;
            }
            // Ignore other event types (handled by coordinator/discovery)
            _ => {}
        }
        Ok(())
    }

    /// Handle ELECTION_REQUEST (as candidate)
    async fn handle_election_request(
        &self,
        req: ElectionRequest,
        requester: std::net::SocketAddr,
    ) -> Result<()> {
        tracing::debug!(
            election_id = %req.election_id,
            election_type = ?req.election_type,
            requester = %requester,
            "Received election request"
        );

        // Self-exclusion: check if this is our own election
        {
            let initiated = self.initiated.read().await;
            if initiated.contains_key(&req.election_id) {
                tracing::debug!(
                    election_id = %req.election_id,
                    "Ignoring own election request (self-exclusion)"
                );
                return Ok(());
            }
        }

        // Evaluate criteria - use sync version for quick rejection
        // (Real implementation would ideally be fully async, but keeping sync for now)
        let state = self.state_provider.read().await.get_state();
        if !matches_criteria(&req.criteria, &state) {
            tracing::debug!(
                election_id = %req.election_id,
                "Not eligible for election (criteria mismatch)"
            );
            return Ok(());
        }

        // Branch on score mechanism
        match req.score_mechanism {
            ScoreMechanism::Fitness => {
                self.handle_fitness_candidacy(&req).await?;
            }
            ScoreMechanism::Blake => {
                self.handle_blake_candidacy(&req).await?;
            }
        }

        Ok(())
    }

    /// Handle Fitness-mode candidacy: compute score immediately, respond without delay.
    async fn handle_fitness_candidacy(&self, req: &ElectionRequest) -> Result<()> {
        // Extract FQN from election type
        let offering_fqn = match &req.election_type {
            ElectionType::OfferingPrimary(fqn) => fqn.clone(),
            _ => {
                tracing::warn!(
                    election_id = %req.election_id,
                    "Fitness mode used with non-OfferingPrimary election type; ignoring"
                );
                return Ok(());
            }
        };

        // Compute fitness via the injected provider
        let fitness_result = {
            let provider_guard = self.fitness_provider.read().await;
            let Some(ref provider) = *provider_guard else {
                tracing::debug!(
                    election_id = %req.election_id,
                    "No fitness provider set — cannot participate in Fitness election"
                );
                return Ok(());
            };
            provider.compute_fitness(&offering_fqn).await
        };

        let Some((score, pin_timestamp)) = fitness_result else {
            tracing::debug!(
                election_id = %req.election_id,
                offering_fqn = %offering_fqn,
                "Ineligible for Fitness election (compute returned None)"
            );
            return Ok(());
        };

        tracing::info!(
            election_id = %req.election_id,
            offering_fqn = %offering_fqn,
            score,
            "Responding to Fitness election immediately"
        );

        // Send candidacy immediately (no delay in Fitness mode)
        let candidate = ElectionCandidate {
            election_id: req.election_id.clone(),
            stone_id: self.stone_id.clone(),
            stone_name: self.stone_name.clone(),
            score: Some(score),
            pin_timestamp,
        };

        p2p::send_announcement(announcement_types::ELECTION_CANDIDATE, &candidate).await?;
        Ok(())
    }

    /// Handle Blake-mode candidacy: BLAKE3 hash delay, first respondent wins.
    async fn handle_blake_candidacy(&self, req: &ElectionRequest) -> Result<()> {
        // Calculate delay
        let delay = calculate_election_delay(&self.stone_id, &req.election_id);
        tracing::info!(
            election_id = %req.election_id,
            delay_ms = delay.as_millis(),
            "Eligible for election, calculated delay"
        );

        // Check pending elections limit
        {
            let pending = self.pending.read().await;
            if pending.len() >= MAX_PENDING_ELECTIONS {
                tracing::warn!(
                    election_id = %req.election_id,
                    "Too many pending elections, rejecting"
                );
                return Ok(());
            }
        }

        // Start timer
        let election_id = req.election_id.clone();
        let stone_id = self.stone_id.clone();
        let stone_name = self.stone_name.clone();
        let pending = self.pending.clone();

        let timer_handle = tokio::spawn(async move {
            tokio::time::sleep(delay).await;

            // Check if still in pending (not cancelled)
            let should_respond = {
                let p = pending.read().await;
                p.contains_key(&election_id)
            };

            if should_respond {
                tracing::info!(
                    election_id = %election_id,
                    "Timer expired, sending candidacy"
                );

                // Send ELECTION_CANDIDATE via p2p transport
                let candidate = ElectionCandidate {
                    election_id: election_id.clone(),
                    stone_id,
                    stone_name,
                    score: None,
                    pin_timestamp: None,
                };

                if let Err(e) =
                    p2p::send_announcement(announcement_types::ELECTION_CANDIDATE, &candidate).await
                {
                    tracing::warn!(error = ?e, "Failed to send candidacy");
                }
            } else {
                tracing::debug!(
                    election_id = %election_id,
                    "Election cancelled before timer expired"
                );
            }
        });

        // Store pending election
        {
            let mut pending = self.pending.write().await;
            pending.insert(
                req.election_id.clone(),
                PendingElection {
                    election_id: req.election_id.clone(),
                    timer_handle: Some(timer_handle),
                    created_at: Instant::now(),
                },
            );
        }

        Ok(())
    }

    /// Handle ELECTION_RESULT (abort our timer if we're a candidate)
    async fn handle_election_result(&self, result: ElectionResult) -> Result<()> {
        tracing::debug!(
            election_id = %result.election_id,
            winner_id = %result.winner_id,
            "Received election result"
        );

        let mut pending = self.pending.write().await;
        if let Some(mut election) = pending.remove(&result.election_id) {
            if let Some(handle) = election.timer_handle.take() {
                handle.abort();
                tracing::info!(
                    election_id = %result.election_id,
                    winner_id = %result.winner_id,
                    "Cancelled our candidacy timer (election won by another)"
                );
            }
        }

        Ok(())
    }

    /// Start an election (as requester)
    ///
    /// **REFACTORED (COMM-0001 Phase 4)**: Uses p2p transport for broadcast and subscribes to events.
    /// **EXTENDED (ORCH-0001 Phase 2)**: Added Fitness collection mode.
    ///
    /// - `ScoreMechanism::Blake`: Takes first respondent (existing behavior).
    /// - `ScoreMechanism::Fitness`: Collects candidates until quiet timeout or hard cap,
    ///   then picks highest score.
    pub async fn start_election(
        &self,
        election_id: String,
        election_type: ElectionType,
        criteria: Value,
        timeout_secs: u64,
        score_mechanism: ScoreMechanism,
    ) -> Result<Option<ElectionWinner>> {
        tracing::info!(
            election_id = %election_id,
            election_type = ?election_type,
            timeout_secs = timeout_secs,
            "Starting election"
        );

        // Cache election_id for self-exclusion
        {
            let mut initiated = self.initiated.write().await;
            initiated.insert(election_id.clone(), Instant::now());
        }

        // Broadcast ELECTION_REQUEST via p2p transport
        let request = ElectionRequest {
            election_id: election_id.clone(),
            election_type,
            criteria,
            score_mechanism: score_mechanism.clone(),
        };

        p2p::send_announcement(announcement_types::ELECTION_REQUEST, &request).await?;

        tracing::debug!(
            election_id = %election_id,
            "Broadcast election request, awaiting candidates"
        );

        // Subscribe to p2p events to receive ELECTION_CANDIDATE responses
        let mut udp_rx =
            p2p::subscribe_to_announcement(announcement_types::ELECTION_CANDIDATE).await?;

        let winner = match score_mechanism {
            ScoreMechanism::Blake => {
                self.collect_blake_winner(&election_id, &mut udp_rx, timeout_secs)
                    .await
            }
            ScoreMechanism::Fitness => {
                self.collect_fitness_winner(&election_id, &mut udp_rx)
                    .await
            }
        };

        // Broadcast ELECTION_RESULT if we have a winner
        if let Some(ref w) = winner {
            let result = ElectionResult {
                election_id: election_id.clone(),
                winner_id: w.stone_id.clone(),
            };

            p2p::send_announcement(announcement_types::ELECTION_RESULT, &result).await?;

            tracing::info!(
                election_id = %election_id,
                winner_id = %w.stone_id,
                winner_name = %w.stone_name,
                "Election completed, winner announced"
            );
        } else {
            tracing::warn!(
                election_id = %election_id,
                timeout_secs = timeout_secs,
                "Election completed with no candidates"
            );
        }

        Ok(winner)
    }

    // ========================================================================
    // Collection strategies
    // ========================================================================

    /// Blake mode: take the first valid respondent.
    async fn collect_blake_winner(
        &self,
        election_id: &str,
        udp_rx: &mut tokio::sync::mpsc::Receiver<(Value, std::net::SocketAddr)>,
        timeout_secs: u64,
    ) -> Option<ElectionWinner> {
        let wait_duration = Duration::from_secs(timeout_secs);
        match timeout(wait_duration, async {
            loop {
                match udp_rx.recv().await {
                    Some((payload, _from_addr)) => {
                        let candidate: ElectionCandidate =
                            match serde_json::from_value(payload) {
                                Ok(c) => c,
                                Err(e) => {
                                    tracing::warn!(error = ?e, "Failed to parse candidate");
                                    continue;
                                }
                            };
                        if candidate.election_id == election_id {
                            return Some(ElectionWinner {
                                stone_id: candidate.stone_id,
                                stone_name: candidate.stone_name,
                            });
                        }
                    }
                    None => {
                        tracing::error!("P2P channel closed");
                        break;
                    }
                }
            }
            None
        })
        .await
        {
            Ok(winner) => winner,
            Err(_) => None,
        }
    }

    /// Fitness mode: collect candidates until quiet timeout (1s) or hard cap (3s),
    /// then pick highest score.
    async fn collect_fitness_winner(
        &self,
        election_id: &str,
        udp_rx: &mut tokio::sync::mpsc::Receiver<(Value, std::net::SocketAddr)>,
    ) -> Option<ElectionWinner> {
        let hard_cap = Duration::from_millis(FITNESS_HARD_CAP_MS);
        let quiet_timeout = Duration::from_millis(FITNESS_QUIET_TIMEOUT_MS);
        let start = Instant::now();
        let mut candidates: Vec<ElectionCandidate> = Vec::new();
        let mut last_received = Instant::now();

        loop {
            let elapsed = start.elapsed();
            if elapsed >= hard_cap {
                tracing::debug!(
                    election_id = %election_id,
                    collected = candidates.len(),
                    "Fitness election: hard cap reached"
                );
                break;
            }

            // Wait for either quiet timeout or remaining hard cap time
            let remaining_hard = hard_cap - elapsed;
            let quiet_remaining = quiet_timeout
                .checked_sub(last_received.elapsed())
                .unwrap_or(Duration::ZERO);

            // If quiet timeout already expired AND we have candidates, decide now
            if quiet_remaining.is_zero() && !candidates.is_empty() {
                tracing::debug!(
                    election_id = %election_id,
                    collected = candidates.len(),
                    "Fitness election: quiet timeout reached"
                );
                break;
            }

            let wait_time = remaining_hard.min(if candidates.is_empty() {
                remaining_hard // No candidates yet, wait up to hard cap
            } else {
                quiet_remaining.max(Duration::from_millis(50)) // At least 50ms poll
            });

            match timeout(wait_time, udp_rx.recv()).await {
                Ok(Some((payload, _from_addr))) => {
                    let candidate: ElectionCandidate = match serde_json::from_value(payload) {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!(error = ?e, "Failed to parse Fitness candidate");
                            continue;
                        }
                    };
                    if candidate.election_id == election_id {
                        tracing::debug!(
                            election_id = %election_id,
                            stone_id = %candidate.stone_id,
                            score = ?candidate.score,
                            "Fitness election: received candidate"
                        );
                        last_received = Instant::now();
                        candidates.push(candidate);
                    }
                }
                Ok(None) => {
                    tracing::error!("P2P channel closed during Fitness election");
                    break;
                }
                Err(_) => {
                    // Timeout — check loop conditions
                    continue;
                }
            }
        }

        resolve_fitness_election(&candidates)
    }

    /// Cleanup expired elections
    async fn run_cleanup_loop(&self) {
        loop {
            tokio::time::sleep(Duration::from_secs(CLEANUP_INTERVAL_SECS)).await;

            let _now = Instant::now();

            // Clean up expired pending elections
            {
                let mut pending = self.pending.write().await;
                pending.retain(|id, election| {
                    if election.created_at.elapsed() > Duration::from_secs(60) {
                        if let Some(handle) = &election.timer_handle {
                            handle.abort();
                        }
                        tracing::debug!(election_id = %id, "Cleaned up expired pending election");
                        false
                    } else {
                        true
                    }
                });
            }

            // Clean up expired initiated elections
            {
                let mut initiated = self.initiated.write().await;
                let ttl = Duration::from_secs(INITIATED_ELECTION_TTL_SECS);
                initiated.retain(|id, timestamp| {
                    if timestamp.elapsed() > ttl {
                        tracing::debug!(election_id = %id, "Cleaned up expired initiated election");
                        false
                    } else {
                        true
                    }
                });
            }
        }
    }
}

// ============================================================================
// Fitness resolution (pure function — easy to test)
// ============================================================================

/// Pick the winning candidate from a set of fitness-scored candidates.
///
/// **Tiebreak rules** (from ORCH-0001 spec):
/// 1. Highest `score` wins.
/// 2. If tied, most-recent `pin_timestamp` wins (pinned stone preference).
/// 3. If still tied, lexicographically higher `stone_id` wins (deterministic).
///
/// Returns `None` if `candidates` is empty.
pub fn resolve_fitness_election(candidates: &[ElectionCandidate]) -> Option<ElectionWinner> {
    if candidates.is_empty() {
        return None;
    }

    let winner = candidates.iter().max_by(|a, b| {
        // 1. Higher score wins
        let score_a = a.score.unwrap_or(-1000);
        let score_b = b.score.unwrap_or(-1000);
        match score_a.cmp(&score_b) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }

        // 2. Most-recent pin_timestamp wins (Some > None, then lexicographic desc)
        match (&a.pin_timestamp, &b.pin_timestamp) {
            (Some(ts_a), Some(ts_b)) => match ts_a.cmp(ts_b) {
                std::cmp::Ordering::Equal => {}
                ord => return ord,
            },
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (None, None) => {}
        }

        // 3. Lexicographically higher stone_id wins
        a.stone_id.cmp(&b.stone_id)
    });

    winner.map(|c| ElectionWinner {
        stone_id: c.stone_id.clone(),
        stone_name: c.stone_name.clone(),
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use garden_common::election::ElectionCandidate;

    fn candidate(id: &str, name: &str, score: i16) -> ElectionCandidate {
        ElectionCandidate {
            election_id: "test-election".to_string(),
            stone_id: id.to_string(),
            stone_name: name.to_string(),
            score: Some(score),
            pin_timestamp: None,
        }
    }

    fn pinned_candidate(id: &str, name: &str, pin_ts: &str) -> ElectionCandidate {
        ElectionCandidate {
            election_id: "test-election".to_string(),
            stone_id: id.to_string(),
            stone_name: name.to_string(),
            score: Some(1001),
            pin_timestamp: Some(pin_ts.to_string()),
        }
    }

    // ====================================================================
    // resolve_fitness_election
    // ====================================================================

    #[test]
    fn test_no_candidates_returns_none() {
        assert!(resolve_fitness_election(&[]).is_none());
    }

    #[test]
    fn test_single_candidate_wins() {
        let candidates = vec![candidate("stone-a", "Alpha", 500)];
        let winner = resolve_fitness_election(&candidates).unwrap();
        assert_eq!(winner.stone_id, "stone-a");
        assert_eq!(winner.stone_name, "Alpha");
    }

    #[test]
    fn test_highest_score_wins() {
        let candidates = vec![
            candidate("stone-a", "Alpha", 300),
            candidate("stone-b", "Bravo", 800),
            candidate("stone-c", "Charlie", 500),
        ];
        let winner = resolve_fitness_election(&candidates).unwrap();
        assert_eq!(winner.stone_id, "stone-b");
    }

    #[test]
    fn test_negative_scores() {
        let candidates = vec![
            candidate("stone-a", "Alpha", -200),
            candidate("stone-b", "Bravo", -50),
        ];
        let winner = resolve_fitness_election(&candidates).unwrap();
        assert_eq!(winner.stone_id, "stone-b");
    }

    #[test]
    fn test_tied_scores_pinned_beats_unpinned() {
        let candidates = vec![
            candidate("stone-a", "Alpha", 500),
            ElectionCandidate {
                election_id: "test-election".to_string(),
                stone_id: "stone-b".to_string(),
                stone_name: "Bravo".to_string(),
                score: Some(500),
                pin_timestamp: Some("2026-02-16T00:00:00Z".to_string()),
            },
        ];
        let winner = resolve_fitness_election(&candidates).unwrap();
        assert_eq!(winner.stone_id, "stone-b");
    }

    #[test]
    fn test_dual_pinned_most_recent_wins() {
        let candidates = vec![
            pinned_candidate("stone-a", "Alpha", "2026-02-14T00:00:00Z"),
            pinned_candidate("stone-b", "Bravo", "2026-02-16T00:00:00Z"),
        ];
        let winner = resolve_fitness_election(&candidates).unwrap();
        // Most-recent pin_timestamp wins (lexicographic comparison)
        assert_eq!(winner.stone_id, "stone-b");
    }

    #[test]
    fn test_dual_pinned_same_timestamp_stone_id_tiebreak() {
        let candidates = vec![
            pinned_candidate("stone-a", "Alpha", "2026-02-16T00:00:00Z"),
            pinned_candidate("stone-z", "Zulu", "2026-02-16T00:00:00Z"),
        ];
        let winner = resolve_fitness_election(&candidates).unwrap();
        // Lexicographically higher stone_id wins
        assert_eq!(winner.stone_id, "stone-z");
    }

    #[test]
    fn test_tied_scores_no_pins_stone_id_tiebreak() {
        let candidates = vec![
            candidate("stone-a", "Alpha", 500),
            candidate("stone-m", "Mike", 500),
        ];
        let winner = resolve_fitness_election(&candidates).unwrap();
        assert_eq!(winner.stone_id, "stone-m");
    }

    #[test]
    fn test_missing_scores_treated_as_minimum() {
        let candidates = vec![
            ElectionCandidate {
                election_id: "test-election".to_string(),
                stone_id: "stone-a".to_string(),
                stone_name: "Alpha".to_string(),
                score: None,
                pin_timestamp: None,
            },
            candidate("stone-b", "Bravo", -999),
        ];
        let winner = resolve_fitness_election(&candidates).unwrap();
        // None → -1000, so -999 beats it
        assert_eq!(winner.stone_id, "stone-b");
    }

    #[test]
    fn test_pinned_1001_beats_max_score() {
        let candidates = vec![
            candidate("stone-a", "Alpha", 1000),
            pinned_candidate("stone-b", "Bravo", "2026-02-16T00:00:00Z"),
        ];
        let winner = resolve_fitness_election(&candidates).unwrap();
        assert_eq!(winner.stone_id, "stone-b");
    }
}
