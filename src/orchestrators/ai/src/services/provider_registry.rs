//! Process-internal `Arc<dyn Provider>` registry.
//!
//! Under ORCH-0030 R2 the authoritative view of "what providers can
//! serve" lives in
//! [`crate::services::directory_subscriber::CapabilityDirectory`],
//! populated from bus events. What remains is a simple
//! name → `Arc<dyn Provider>` lookup: the dispatcher needs
//! trait-object handles so it can call
//! `provider.onboard(request).await`. That lookup is purely an
//! in-process detail — the handles are constructed at startup and
//! never change — so the registry is a static `HashMap` behind an
//! `RwLock`, not an event-sourced aggregate.
//!
//! # Milestone status (ORCH-0030 M1, additive)
//!
//! This service is added in M1 as pure scaffolding: the registry
//! exists, has tests, and is constructed at startup, but **nothing
//! reads from it yet**. The dispatcher continues to look up
//! providers via the legacy `Directory` aggregate. The switchover
//! happens atomically in M3 (the trait switch milestone), at which
//! point the legacy `Directory` is deleted and the dispatcher
//! consults `ProviderRegistry::get` to obtain the trait object it
//! invokes `onboard` on.
//!
//! # Lifecycle
//!
//! 1. `main.rs` (and the test fixture) constructs an empty
//!    `ProviderRegistry` at startup.
//! 2. After every adapter is constructed, it is registered with the
//!    registry.
//! 3. The registry is frozen after startup — no further mutation.
//! 4. The dispatcher (post-M3) looks up providers by name and calls
//!    `onboard`.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::domain::ids::ProviderName;
use crate::domain::provider::Provider;

/// A name → `Arc<dyn Provider>` lookup.
pub struct ProviderRegistry {
    providers: RwLock<HashMap<ProviderName, Arc<dyn Provider>>>,
}

impl ProviderRegistry {
    /// Construct an empty registry.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            providers: RwLock::new(HashMap::new()),
        })
    }

    /// Register a provider handle. Duplicate names overwrite — the
    /// caller is responsible for uniqueness. In practice this is
    /// only called from startup sequences where every adapter has
    /// a distinct compile-time name.
    pub async fn register(&self, provider: Arc<dyn Provider>) {
        let name = provider.name();
        let mut state = self.providers.write().await;
        state.insert(name, provider);
    }

    /// Look up a provider handle by name. Returns `None` if no
    /// adapter is registered under that name.
    pub async fn get(&self, name: &ProviderName) -> Option<Arc<dyn Provider>> {
        self.providers.read().await.get(name).cloned()
    }

    /// Snapshot every registered provider.
    pub async fn all(&self) -> Vec<Arc<dyn Provider>> {
        self.providers.read().await.values().cloned().collect()
    }

    /// How many providers are currently registered.
    pub async fn len(&self) -> usize {
        self.providers.read().await.len()
    }

    /// Whether the registry holds zero providers.
    pub async fn is_empty(&self) -> bool {
        self.providers.read().await.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::output::Output;
    use crate::domain::provider::{
        ProviderError, ProviderOutcome, ProviderState, ProviderStatePublisher,
    };
    use crate::domain::request::OrchestratorRequest;
    use async_trait::async_trait;
    use tokio::sync::watch;

    /// Minimal stub provider used by the registry's own unit tests.
    /// Implements the legacy `Provider` trait (the M1-era trait
    /// shape; M3 replaces this with the lean trait).
    struct StubProvider {
        name: ProviderName,
        publisher: ProviderStatePublisher,
    }

    impl StubProvider {
        fn new(name: &str) -> Arc<Self> {
            Arc::new(Self {
                name: ProviderName::new(name),
                publisher: ProviderStatePublisher::new(ProviderState::default()),
            })
        }
    }

    #[async_trait]
    impl Provider for StubProvider {
        fn name(&self) -> ProviderName {
            self.name.clone()
        }
        fn state(&self) -> Arc<ProviderState> {
            self.publisher.snapshot()
        }
        fn subscribe(&self) -> watch::Receiver<Arc<ProviderState>> {
            self.publisher.subscribe()
        }
        async fn onboard(
            &self,
            _request: OrchestratorRequest,
        ) -> Result<ProviderOutcome, ProviderError> {
            Ok(ProviderOutcome::Sync(Output::new()))
        }
    }

    #[tokio::test]
    async fn empty_registry_lookups_return_none() {
        let r = ProviderRegistry::new();
        assert!(r.is_empty().await);
        assert_eq!(r.len().await, 0);
        assert!(r.get(&ProviderName::new("nope")).await.is_none());
        assert!(r.all().await.is_empty());
    }

    #[tokio::test]
    async fn register_allows_lookup_by_name() {
        let r = ProviderRegistry::new();
        r.register(StubProvider::new("ollama")).await;
        r.register(StubProvider::new("comfyui")).await;
        assert_eq!(r.len().await, 2);
        assert!(!r.is_empty().await);
        assert!(r.get(&ProviderName::new("ollama")).await.is_some());
        assert!(r.get(&ProviderName::new("comfyui")).await.is_some());
        assert!(r.get(&ProviderName::new("nope")).await.is_none());
    }

    #[tokio::test]
    async fn all_returns_every_registered_provider() {
        let r = ProviderRegistry::new();
        r.register(StubProvider::new("a")).await;
        r.register(StubProvider::new("b")).await;
        r.register(StubProvider::new("c")).await;
        let all = r.all().await;
        assert_eq!(all.len(), 3);
        let mut names: Vec<String> =
            all.iter().map(|p| p.name().as_str().to_string()).collect();
        names.sort();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn duplicate_registration_overwrites() {
        let r = ProviderRegistry::new();
        r.register(StubProvider::new("ollama")).await;
        r.register(StubProvider::new("ollama")).await;
        assert_eq!(r.len().await, 1);
    }
}
