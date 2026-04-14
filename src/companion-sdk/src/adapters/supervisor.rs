//! Adapter supervisor — manages adapter lifecycle.
//!
//! Book VI Chapter 2 lands a placeholder type so the module compiles;
//! Chapter 3 fleshes out the discovery loop, filter task, spawn/reap
//! logic, and grace window.

use super::factory::AdapterFactory;
use crate::garden::{Garden, Pulse};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Default discovery-tick interval.
pub const DEFAULT_DISCOVERY_INTERVAL: Duration = Duration::from_secs(5);

/// Default grace window before reaping an adapter whose device disappeared.
pub const DEFAULT_GRACE_WINDOW: Duration = Duration::from_secs(2);

/// Supervisor aggregate managing adapter lifecycles.
///
/// **Book VI Chapter 2**: skeleton only — `register` and configuration
/// work; `run` / `spawn` / `reap` / `status` arrive in Chapter 3.
pub struct Adapters {
    factories: RwLock<Vec<Box<dyn AdapterFactory>>>,
    #[allow(dead_code)] // used in Chapter 3
    garden: Arc<Garden>,
    #[allow(dead_code)] // used in Chapter 3
    pulse: Arc<Pulse>,
    #[allow(dead_code)] // used in Chapter 3
    discovery_interval: Duration,
    #[allow(dead_code)] // used in Chapter 3
    grace_window: Duration,
}

impl Adapters {
    /// Construct an empty supervisor bound to a [`Garden`] + [`Pulse`].
    pub fn new(garden: Arc<Garden>, pulse: Arc<Pulse>) -> Self {
        Self {
            factories: RwLock::new(Vec::new()),
            garden,
            pulse,
            discovery_interval: DEFAULT_DISCOVERY_INTERVAL,
            grace_window: DEFAULT_GRACE_WINDOW,
        }
    }

    /// Override the default discovery interval.
    pub fn with_discovery_interval(mut self, d: Duration) -> Self {
        self.discovery_interval = d;
        self
    }

    /// Override the default grace window.
    pub fn with_grace_window(mut self, d: Duration) -> Self {
        self.grace_window = d;
        self
    }

    /// Register a factory. Adapters produced by this factory become
    /// candidates on every subsequent discovery tick.
    pub fn register<F: AdapterFactory>(&self, factory: F) {
        self.factories
            .write()
            .expect("Adapters factories lock poisoned")
            .push(Box::new(factory));
    }

    /// Number of registered factories. For diagnostics / tests.
    pub fn factory_count(&self) -> usize {
        self.factories
            .read()
            .expect("Adapters factories lock poisoned")
            .len()
    }

    /// Kinds of all registered factories.
    pub fn factory_kinds(&self) -> Vec<&'static str> {
        self.factories
            .read()
            .expect("Adapters factories lock poisoned")
            .iter()
            .map(|f| f.kind())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{Adapter, AdapterInfo, AdapterProfile};
    use crate::garden::{Event, PulseConfig};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn fixture() -> Adapters {
        let pulse = Arc::new(Pulse::new(PulseConfig {
            dedup_capacity: 16,
            broadcast_capacity: 64,
        }));
        pulse.register_namespace("core");
        let garden = Garden::new(pulse.clone());
        Adapters::new(garden, pulse)
    }

    struct TestFactory {
        kind: &'static str,
    }
    struct TestAdapter {
        kind: &'static str,
        id: String,
    }

    impl Adapter for TestAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                kind: self.kind,
                id: self.id.clone(),
                device: None,
            }
        }
        fn profile(&self) -> AdapterProfile {
            AdapterProfile::default()
        }
        fn run(
            self: Box<Self>,
            _events: mpsc::Receiver<Event>,
            _garden: Arc<Garden>,
            _pulse: Arc<Pulse>,
            shutdown: CancellationToken,
        ) -> super::super::adapter::BoxFuture<'static, ()> {
            Box::pin(async move {
                shutdown.cancelled().await;
            })
        }
    }

    impl AdapterFactory for TestFactory {
        fn kind(&self) -> &'static str {
            self.kind
        }
        fn discover(&self) -> Vec<Box<dyn Adapter>> {
            vec![Box::new(TestAdapter {
                kind: self.kind,
                id: "only".into(),
            })]
        }
    }

    #[test]
    fn empty_supervisor_has_no_factories() {
        let s = fixture();
        assert_eq!(s.factory_count(), 0);
        assert!(s.factory_kinds().is_empty());
    }

    #[test]
    fn register_increments_factory_count() {
        let s = fixture();
        s.register(TestFactory { kind: "a" });
        s.register(TestFactory { kind: "b" });
        assert_eq!(s.factory_count(), 2);
        assert_eq!(s.factory_kinds(), vec!["a", "b"]);
    }

    #[test]
    fn with_intervals_overrides_defaults() {
        let s = fixture()
            .with_discovery_interval(Duration::from_secs(1))
            .with_grace_window(Duration::from_millis(500));
        assert_eq!(s.discovery_interval, Duration::from_secs(1));
        assert_eq!(s.grace_window, Duration::from_millis(500));
    }
}
