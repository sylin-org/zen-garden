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

