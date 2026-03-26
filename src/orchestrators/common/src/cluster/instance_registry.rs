//! Generic instance registry — tracks discovered service instances.
//!
//! Endpoint-keyed map with upsert semantics, health transitions, and
//! removal. Used by the cluster lifecycle to maintain awareness of all
//! instances across stones.

use std::collections::HashMap;
use std::time::Instant;

use super::adapter::{ClusterInstance, InstanceHealth};

/// Endpoint-keyed registry of cluster instances.
///
/// Generic over `I: ClusterInstance` — works with any adapter's instance type.
pub struct InstanceRegistry<I: ClusterInstance> {
    instances: HashMap<String, Entry<I>>,
}

struct Entry<I> {
    instance: I,
    last_seen: Instant,
}

impl<I: ClusterInstance> InstanceRegistry<I> {
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
        }
    }

    /// Insert or update an instance. Returns `true` if this is a new instance.
    pub fn upsert(&mut self, instance: I) -> bool {
        let endpoint = instance.endpoint().to_string();
        let is_new = !self.instances.contains_key(&endpoint);
        self.instances.insert(
            endpoint,
            Entry {
                instance,
                last_seen: Instant::now(),
            },
        );
        is_new
    }

    /// Remove an instance by endpoint. Returns the removed instance if present.
    pub fn remove(&mut self, endpoint: &str) -> Option<I> {
        self.instances.remove(endpoint).map(|e| e.instance)
    }

    /// Get an instance by endpoint.
    pub fn get(&self, endpoint: &str) -> Option<&I> {
        self.instances.get(endpoint).map(|e| &e.instance)
    }

    /// Get a mutable reference to an instance by endpoint.
    pub fn get_mut(&mut self, endpoint: &str) -> Option<&mut I> {
        self.instances.get_mut(endpoint).map(|e| &mut e.instance)
    }

    /// All instances.
    pub fn all(&self) -> impl Iterator<Item = &I> {
        self.instances.values().map(|e| &e.instance)
    }

    /// Number of tracked instances.
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Instances not seen since `cutoff`.
    pub fn stale_since(&self, cutoff: Instant) -> Vec<&I> {
        self.instances
            .values()
            .filter(|e| e.last_seen < cutoff)
            .map(|e| &e.instance)
            .collect()
    }

    /// Instances matching a health status.
    pub fn with_health(&self, health: &InstanceHealth) -> Vec<&I> {
        self.instances
            .values()
            .filter(|e| e.instance.health() == health)
            .map(|e| &e.instance)
            .collect()
    }

    /// All healthy instances.
    pub fn healthy(&self) -> Vec<&I> {
        self.with_health(&InstanceHealth::Healthy)
    }

    /// Mark an instance's last-seen timestamp as now.
    pub fn touch(&mut self, endpoint: &str) {
        if let Some(entry) = self.instances.get_mut(endpoint) {
            entry.last_seen = Instant::now();
        }
    }
}

impl<I: ClusterInstance> Default for InstanceRegistry<I> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestInstance {
        ep: String,
        sid: String,
        sname: String,
        h: InstanceHealth,
    }

    impl ClusterInstance for TestInstance {
        fn endpoint(&self) -> &str { &self.ep }
        fn stone_id(&self) -> &str { &self.sid }
        fn stone_name(&self) -> &str { &self.sname }
        fn health(&self) -> &InstanceHealth { &self.h }
    }

    fn inst(ep: &str) -> TestInstance {
        TestInstance {
            ep: ep.to_string(),
            sid: "id-1".into(),
            sname: "stone-a".into(),
            h: InstanceHealth::Healthy,
        }
    }

    #[test]
    fn upsert_and_get() {
        let mut reg = InstanceRegistry::new();
        assert!(reg.upsert(inst("10.0.0.1:5432")));
        assert!(!reg.upsert(inst("10.0.0.1:5432"))); // update, not new
        assert_eq!(reg.len(), 1);
        assert!(reg.get("10.0.0.1:5432").is_some());
    }

    #[test]
    fn remove() {
        let mut reg = InstanceRegistry::new();
        reg.upsert(inst("10.0.0.1:5432"));
        assert!(reg.remove("10.0.0.1:5432").is_some());
        assert!(reg.is_empty());
    }

    #[test]
    fn healthy_filter() {
        let mut reg = InstanceRegistry::new();
        reg.upsert(inst("10.0.0.1:5432"));
        reg.upsert(TestInstance {
            ep: "10.0.0.2:5432".into(),
            sid: "id-2".into(),
            sname: "stone-b".into(),
            h: InstanceHealth::Down,
        });
        assert_eq!(reg.healthy().len(), 1);
    }
}
