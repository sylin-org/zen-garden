//! Distributed election service - UDP listener and responder
//!
//! This service implements the election protocol specified in
//! docs/specs/ELECTION-0001-distributed-election.md

use anyhow::{Context, Result};
use garden_common::election::{
    calculate_election_delay, matches_criteria, ElectionCandidate, ElectionRequest,
    ElectionResult, ElectionType, ElectionWinner,
};
use garden_common::types::{announcement_types, UdpAnnouncement};
use serde_json::Value;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tokio::time::timeout;

/// Maximum pending elections to track
const MAX_PENDING_ELECTIONS: usize = 100;

/// Cleanup interval for expired elections
const CLEANUP_INTERVAL_SECS: u64 = 60;

/// TTL for initiated elections cache (prevent self-response)
const INITIATED_ELECTION_TTL_SECS: u64 = 60;

/// Election service state
///
/// IMPORTANT: Does NOT create its own UDP socket.
/// Subscribes to UDP events from singleton listener in discovery.rs
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

impl ElectionService {
    /// Create new election service (does NOT bind UDP socket)
    pub fn new(
        stone_id: String,
        stone_name: String,
        state_provider: Box<dyn StateProvider>,
    ) -> Self {
        tracing::info!(
            stone_id = %stone_id,
            stone_name = %stone_name,
            "Election service initialized (will subscribe to singleton UDP)"
        );

        Self {
            stone_id,
            stone_name,
            pending: Arc::new(RwLock::new(HashMap::new())),
            initiated: Arc::new(RwLock::new(HashMap::new())),
            state_provider: Arc::new(RwLock::new(state_provider)),
        }
    }

    /// Start UDP listener loop (call this from bootstrap as background task)
    pub async fn run_listener(self: Arc<Self>) -> Result<()> {
        tracing::info!("Election service listener starting");

        let cleanup_service = self.clone();
        tokio::spawn(async move {
            cleanup_service.run_cleanup_loop().await;
        });

        let mut buf = [0u8; 2048];
        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    let data = &buf[..len];
                    if let Err(e) = self.handle_udp_message(data, addr).await {
                        tracing::debug!(error = ?e, "Failed to handle UDP message");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "UDP recv error");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Handle incoming UDP message
    async fn handle_udp_message(&self, data: &[u8], from: SocketAddr) -> Result<()> {
        let msg: UdpAnnouncement = serde_json::from_slice(data)
            .context("Failed to parse UDP announcement")?;

        match msg.announcement_type.as_str() {
            announcement_types::ELECTION_REQUEST => {
                let req: ElectionRequest = serde_json::from_value(msg.data)
                    .context("Failed to parse election request")?;
                self.handle_election_request(req, from).await?;
            }
            announcement_types::ELECTION_RESULT => {
                let result: ElectionResult = serde_json::from_value(msg.data)
                    .context("Failed to parse election result")?;
                self.handle_election_result(result).await?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Handle ELECTION_REQUEST (as candidate)
    async fn handle_election_request(&self, req: ElectionRequest, requester: SocketAddr) -> Result<()> {
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
        let socket = self.socket.clone();
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

                // Send ELECTION_CANDIDATE
                let candidate = ElectionCandidate {
                    election_id: election_id.clone(),
                    stone_id,
                    stone_name,
                };

                let announcement = UdpAnnouncement {
                    announcement_type: announcement_types::ELECTION_CANDIDATE.to_string(),
                    data: serde_json::to_value(&candidate).unwrap(),
                };

                if let Ok(payload) = serde_json::to_vec(&announcement) {
                    if let Err(e) = socket.send_to(&payload, requester).await {
                        tracing::warn!(error = ?e, "Failed to send candidacy");
                    }
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
                    election_id: req.election_id,
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
    pub async fn start_election(
        &self,
        election_id: String,
        election_type: ElectionType,
        criteria: Value,
        timeout_secs: u64,
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

        // Broadcast ELECTION_REQUEST
        let request = ElectionRequest {
            election_id: election_id.clone(),
            election_type,
            criteria,
        };

        let announcement = UdpAnnouncement {
            announcement_type: announcement_types::ELECTION_REQUEST.to_string(),
            data: serde_json::to_value(&request)?,
        };

        let payload = serde_json::to_vec(&announcement)?;
        let broadcast_addr = format!("255.255.255.255:{}", 7184);

        self.socket.send_to(&payload, &broadcast_addr).await?;

        tracing::debug!(
            election_id = %election_id,
            "Broadcast election request, awaiting candidates"
        );

        // Wait for first ELECTION_CANDIDATE response
        let mut buf = [0u8; 2048];
        let wait_duration = Duration::from_secs(timeout_secs);

        let winner = match timeout(wait_duration, async {
            loop {
                match self.socket.recv_from(&mut buf).await {
                    Ok((len, _addr)) => {
                        if let Ok(msg) = serde_json::from_slice::<UdpAnnouncement>(&buf[..len]) {
                            if msg.announcement_type == announcement_types::ELECTION_CANDIDATE {
                                if let Ok(candidate) = serde_json::from_value::<ElectionCandidate>(msg.data) {
                                    if candidate.election_id == election_id {
                                        return Some(ElectionWinner {
                                            stone_id: candidate.stone_id,
                                            stone_name: candidate.stone_name,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!(error = ?e, "Socket recv error while waiting for candidates");
                    }
                }
            }
        }).await {
            Ok(Some(winner)) => Some(winner),
            Ok(None) | Err(_) => None,
        };

        // Broadcast ELECTION_RESULT if we have a winner
        if let Some(ref w) = winner {
            let result = ElectionResult {
                election_id: election_id.clone(),
                winner_id: w.stone_id.clone(),
            };

            let announcement = UdpAnnouncement {
                announcement_type: announcement_types::ELECTION_RESULT.to_string(),
                data: serde_json::to_value(&result)?,
            };

            let payload = serde_json::to_vec(&announcement)?;
            self.socket.send_to(&payload, &broadcast_addr).await?;

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
