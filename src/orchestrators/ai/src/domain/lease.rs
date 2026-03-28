//! Lease-on-demand reservation for high-VRAM instances.
//!
//! Prevents small-model traffic from stealing large GPU resources through
//! adaptive-decay leases. When a large model acquires a lease on a high-tier
//! instance, smaller models are routed to lower tiers.
//!
//! Harvested from ollama-orchestrator domain/lease.rs — this module is
//! fully generic (operates on endpoint strings, not offering-specific types).

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Minimum lease duration.
const MIN_LEASE_SECS: u64 = 15;

/// Maximum lease duration.
const MAX_LEASE_SECS: u64 = 300;

/// Growth factor applied to the lease duration on each extension.
const LEASE_GROWTH: f64 = 1.25;

/// A lease on an instance endpoint for a specific model.
#[derive(Debug, Clone)]
struct Lease {
    model: String,
    expires_at: Instant,
    duration: Duration,
}

/// Manages active leases on high-tier instances.
#[derive(Debug)]
pub struct LeaseManager {
    leases: HashMap<String, Lease>, // endpoint → lease
}

impl LeaseManager {
    pub fn new() -> Self {
        Self {
            leases: HashMap::new(),
        }
    }

    /// Try to acquire or extend a lease for `model` on `endpoint`.
    ///
    /// Returns `true` if the lease was acquired/extended, `false` if
    /// the endpoint is leased to a different model.
    pub fn acquire(&mut self, endpoint: &str, model: &str, now: Instant) -> bool {
        if let Some(lease) = self.leases.get_mut(endpoint) {
            if lease.model == model {
                // Same model: extend the lease
                let new_dur = Duration::from_secs_f64(
                    (lease.duration.as_secs_f64() * LEASE_GROWTH).min(MAX_LEASE_SECS as f64),
                );
                lease.duration = new_dur;
                lease.expires_at = now + new_dur;
                return true;
            }
            // Different model: deny if lease is still active
            if lease.expires_at > now {
                return false;
            }
        }

        // No active lease: create one
        let duration = Duration::from_secs(MIN_LEASE_SECS);
        self.leases.insert(
            endpoint.to_string(),
            Lease {
                model: model.to_string(),
                expires_at: now + duration,
                duration,
            },
        );
        true
    }

    /// Release a lease on an endpoint.
    pub fn release(&mut self, endpoint: &str) {
        self.leases.remove(endpoint);
    }

    /// Remove expired leases.
    pub fn reap_expired(&mut self, now: Instant) {
        self.leases.retain(|_, lease| lease.expires_at > now);
    }

    /// Check if an endpoint is leased to a specific model.
    pub fn is_leased_for(&self, endpoint: &str, model: &str, now: Instant) -> bool {
        self.leases
            .get(endpoint)
            .is_some_and(|l| l.model == model && l.expires_at > now)
    }

    /// Check if an endpoint has any active lease.
    pub fn is_leased(&self, endpoint: &str, now: Instant) -> bool {
        self.leases
            .get(endpoint)
            .is_some_and(|l| l.expires_at > now)
    }

    /// Get the model that currently holds a lease on an endpoint.
    pub fn get_lease(&self, endpoint: &str, now: Instant) -> Option<&str> {
        self.leases
            .get(endpoint)
            .filter(|l| l.expires_at > now)
            .map(|l| l.model.as_str())
    }

    /// All active leases (for dashboard display).
    pub fn active_leases(&self, now: Instant) -> Vec<(&str, &str)> {
        self.leases
            .iter()
            .filter(|(_, l)| l.expires_at > now)
            .map(|(ep, l)| (ep.as_str(), l.model.as_str()))
            .collect()
    }
}

impl Default for LeaseManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_and_extend() {
        let mut mgr = LeaseManager::new();
        let now = Instant::now();

        assert!(mgr.acquire("ep1", "llama3:8b", now));
        assert!(mgr.is_leased("ep1", now));
        assert!(mgr.is_leased_for("ep1", "llama3:8b", now));

        // Same model extends
        assert!(mgr.acquire("ep1", "llama3:8b", now));
    }

    #[test]
    fn deny_different_model() {
        let mut mgr = LeaseManager::new();
        let now = Instant::now();

        assert!(mgr.acquire("ep1", "llama3:8b", now));
        assert!(!mgr.acquire("ep1", "qwen:7b", now));
    }

    #[test]
    fn expired_lease_allows_new() {
        let mut mgr = LeaseManager::new();
        let now = Instant::now();

        assert!(mgr.acquire("ep1", "llama3:8b", now));

        let later = now + Duration::from_secs(MAX_LEASE_SECS + 1);
        assert!(mgr.acquire("ep1", "qwen:7b", later));
    }

    #[test]
    fn release() {
        let mut mgr = LeaseManager::new();
        let now = Instant::now();

        mgr.acquire("ep1", "llama3:8b", now);
        mgr.release("ep1");
        assert!(!mgr.is_leased("ep1", now));
    }

    #[test]
    fn reap_expired() {
        let mut mgr = LeaseManager::new();
        let now = Instant::now();

        mgr.acquire("ep1", "llama3:8b", now);
        mgr.acquire("ep2", "qwen:7b", now);

        let later = now + Duration::from_secs(MAX_LEASE_SECS + 1);
        mgr.reap_expired(later);

        assert!(mgr.active_leases(later).is_empty());
    }
}
