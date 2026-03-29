//! Lease-on-demand reservation for high-VRAM instances.
//!
//! When a large model needs a big GPU, the lease system temporarily
//! reserves that instance so small-model traffic doesn't steal it.
//! Leases use adaptive decay: extend on continued use, shrink on idle.

use super::types::Lease;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Minimum lease duration.
const LEASE_MIN: Duration = Duration::from_secs(15);
/// Maximum lease duration (cap for adaptive growth).
#[allow(dead_code)]
const LEASE_MAX: Duration = Duration::from_secs(300);
/// Growth factor per extension.
#[allow(dead_code)]
const LEASE_GROWTH: f64 = 1.25;

/// Manages active leases on high-tier instances.
#[derive(Debug, Default)]
pub struct LeaseManager {
    /// endpoint → active lease
    leases: HashMap<String, Lease>,
}

impl LeaseManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Acquire a lease on an instance for a model.
    ///
    /// If the instance already has a lease for the same model, extend it.
    /// If it has a lease for a different model, check if it's expired.
    pub fn acquire(&mut self, endpoint: &str, model: &str) -> bool {
        if let Some(existing) = self.leases.get_mut(endpoint) {
            if existing.model_name == model {
                // Same model — extend the lease
                existing.extend();
                return true;
            }
            if !existing.is_expired() {
                // Different model, lease still active — deny
                return false;
            }
            // Expired — replace
        }

        self.leases.insert(
            endpoint.to_string(),
            Lease {
                instance_endpoint: endpoint.to_string(),
                model_name: model.to_string(),
                granted_at: Instant::now(),
                duration: LEASE_MIN,
            },
        );
        true
    }

    /// Release a lease on an instance.
    pub fn release(&mut self, endpoint: &str) {
        self.leases.remove(endpoint);
    }

    /// Clean up expired leases. Returns the number of leases released.
    pub fn reap_expired(&mut self) -> usize {
        let before = self.leases.len();
        self.leases.retain(|_, lease| !lease.is_expired());
        before - self.leases.len()
    }

    /// Check if an instance is leased for a specific model.
    pub fn is_leased_for(&self, endpoint: &str, model: &str) -> bool {
        self.leases
            .get(endpoint)
            .map(|l| l.model_name == model && !l.is_expired())
            .unwrap_or(false)
    }

    /// Check if an instance has any active (non-expired) lease.
    pub fn is_leased(&self, endpoint: &str) -> bool {
        self.leases
            .get(endpoint)
            .map(|l| !l.is_expired())
            .unwrap_or(false)
    }

    /// Get active lease info for an instance.
    pub fn get_lease(&self, endpoint: &str) -> Option<&Lease> {
        self.leases.get(endpoint).filter(|l| !l.is_expired())
    }

    /// All active leases (for dashboard display).
    pub fn active_leases(&self) -> Vec<&Lease> {
        self.leases.values().filter(|l| !l.is_expired()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_and_check() {
        let mut mgr = LeaseManager::new();
        assert!(mgr.acquire("ep1", "model-a"));
        assert!(mgr.is_leased("ep1"));
        assert!(mgr.is_leased_for("ep1", "model-a"));
        assert!(!mgr.is_leased_for("ep1", "model-b"));
    }

    #[test]
    fn extend_same_model() {
        let mut mgr = LeaseManager::new();
        mgr.acquire("ep1", "model-a");
        let d1 = mgr.leases["ep1"].duration;
        mgr.acquire("ep1", "model-a"); // extend
        let d2 = mgr.leases["ep1"].duration;
        assert!(d2 > d1);
    }

    #[test]
    fn deny_different_model_active() {
        let mut mgr = LeaseManager::new();
        mgr.acquire("ep1", "model-a");
        assert!(!mgr.acquire("ep1", "model-b")); // denied
    }

    #[test]
    fn release() {
        let mut mgr = LeaseManager::new();
        mgr.acquire("ep1", "model-a");
        mgr.release("ep1");
        assert!(!mgr.is_leased("ep1"));
    }
}
