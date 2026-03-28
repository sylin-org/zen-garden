//! Shared application state — thin facade (code standard §14).
//!
//! Re-exports domain contexts and holds the [`OfferingRegistry`]. Each domain
//! owns its state; `AppState` is the composition root.

use crate::catalog::OfferingRegistry;

/// Shared state for the AI orchestrator.
///
/// Passed to tasks and API handlers via `Arc<AppState>`. Domain contexts
/// will be added as modules are generalized from the Ollama orchestrator.
pub struct AppState {
    /// Immutable offering catalog (set at startup).
    pub catalog: OfferingRegistry,

    // Domain contexts — will be populated during Phase 1 generalization:
    // pub instances: Arc<RwLock<HashMap<String, ServiceInstance>>>,
    // pub demand: Arc<RwLock<DemandLedger>>,
    // pub fitness: Arc<RwLock<BenchmarkRun>>,
    // pub metrics: Arc<RwLock<MetricsEngine>>,
    // pub config: Arc<RwLock<RouterConfig>>,
    // pub vram_budgets: Arc<RwLock<Vec<StoneVramBudget>>>,
}
