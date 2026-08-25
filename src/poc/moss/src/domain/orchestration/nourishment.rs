//! Nourishment orchestration — update job SSE channels.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// SSE job channels for active nourishment (update) jobs.
///
/// Keyed by job ID. Each sender carries line-by-line progress output
/// to connected SSE subscribers.
///
/// Field path: `state.orchestration.nourishment.*`
#[derive(Clone)]
pub struct NourishmentOrchestration {
    pub jobs: Arc<RwLock<HashMap<String, tokio::sync::broadcast::Sender<String>>>>,
}
